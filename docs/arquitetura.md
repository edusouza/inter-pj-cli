# Arquitetura

## Visão geral

```text
┌──────────────────────── crates/inter-pj-cli (binário `inter-pj`) ────────────────────────┐
│ cli.rs        definição dos comandos (clap) e ajuda em português                         │
│ config.rs     arquivo TOML, perfis, precedência flag > env > arquivo, origem dos valores │
│ commands/     saldo, extrato (simples, completo, pdf), pix enviar/consultar, auth, config │
│ token_store   cache de tokens em arquivo (600, gravação atômica)                          │
│ confirmacao   resumo + [s/N] antes de mover dinheiro (só com stdin em terminal)           │
│ valor.rs      valores em reais digitados (150,00 / 1.500,00) e por extenso                │
│ tabela.rs     tabelas em texto alinhado e CSV (RFC 4180, modo Excel pt-BR)                │
│ output.rs     R$ no formato brasileiro, JSON, escrita em stdout                           │
│ files.rs      gravação de arquivos sensíveis com permissão 600                            │
│ error.rs      códigos de saída e dicas                                                    │
└──────────────────────────────────────┬───────────────────────────────────────────────────┘
                                       │ usa
┌──────────────────────── crates/inter-pj (biblioteca `inter_pj`) ─────────────────────────┐
│ client.rs     InterClient: reqwest + rustls, mTLS, bearer, x-conta-corrente, 401 → renova │
│ retry.rs      RetryPolicy: backoff exponencial com jitter, Retry-After, modos de repetição│
│ auth.rs       AccessToken, TokenStore, TokenManager (memória + armazenamento externo)     │
│ endpoint.rs   registro de operações (método, caminho, escopos) — verificado por contrato  │
│ identity.rs   certificado + chave PEM validados                                           │
│ scope.rs      os 36 escopos documentados                                                  │
│ problem.rs    parser tolerante de erros (RFC 7807 e variações)                            │
│ banking/      saldo, extrato (paginação e scroll), PDF, envio e consulta de Pix           │
│ documento.rs  CPF e CNPJ (inclusive o alfanumérico) com dígitos verificadores             │
│ pix/          chave Pix (formatos do DICT) e leitura do copia e cola (BR Code, CRC16)     │
└───────────────────────────────────────────────────────────────────────────────────────────┘
```

A biblioteca não conhece terminal, arquivos de configuração nem diretórios do usuário; a CLI não conhece HTTP. Isso mantém a biblioteca reutilizável (outros programas podem usar o `inter_pj` diretamente) e testável isoladamente.

## Decisões

### TLS com rustls, sem OpenSSL

O Inter exige mTLS em todas as chamadas. `reqwest` com `rustls` elimina a dependência de OpenSSL do sistema, simplifica binários estáticos (musl) e a compilação cruzada. A cadeia do servidor é verificada pelo repositório de certificados do sistema operacional (`rustls-platform-verifier`).

### Escopos mínimos por operação e cache de token

O endpoint de token aceita 5 chamadas por minuto e um token pedido com escopos não habilitados na integração falha inteiro. Por isso cada [`Endpoint`](../crates/inter-pj/src/endpoint.rs) declara os escopos de que precisa e o `TokenManager`:

1. reaproveita um token em memória ou no `TokenStore` que cubra os escopos (superconjunto) e tenha mais de 60 s de validade;
2. senão, pede um novo token com exatamente esses escopos (mais os `escopos` opcionais do perfil);
3. recusa o token se o servidor conceder menos escopos do que o necessário, com mensagem clara;
4. após um `401` com token em cache, invalida-o e repete a requisição uma única vez.

Um mutex serializa as renovações para que chamadas concorrentes não gastem o limite do endpoint de token.

### Registro de endpoints + testes de contrato

Os endpoints ficam em um só lugar (`endpoint.rs`) e são usados tanto para montar as requisições quanto pelos testes de contrato, que conferem método, caminho e escopos com a especificação OpenAPI versionada em `spec/`. Assim, divergências com a documentação oficial quebram o CI em vez de quebrar em produção.

### Retentativas só quando repetir é seguro

Cada requisição tem um modo de repetição. Consultas (`GET`) e o pedido de token são repetidas em `429`, `500`, `502`, `503`, `504`, falhas de conexão e tempo esgotado. As requisições do modo *scroll* do extrato mudam estado no servidor (avançam o cursor): repeti-las depois de um `504` poderia pular um lote inteiro, então elas só são repetidas quando certamente não foram processadas (`429` ou conexão recusada). O envio de Pix também só é repetido nesses dois casos, e sempre com a mesma chave de idempotência; depois de um `5xx` ou de tempo esgotado o resultado é incerto e a decisão fica com quem chamou. Outras operações com efeitos (pagamentos) nunca são repetidas. Falhas de TLS (certificado recusado, CA desconhecida) também não, porque repetir não resolve.

A espera cresce exponencialmente a partir de 1 s, com *jitter* (entre metade e o total do intervalo) e teto de 60 s; um `Retry-After` maior que o teto faz a CLI desistir na hora, com a dica de aguardar.

### Pix: validação local e idempotência

Tudo o que pode ser conferido antes de mover dinheiro é conferido localmente, sem chamar a API:

- chaves Pix são reconhecidas e normalizadas como o DICT as guarda (CPF/CNPJ só com dígitos, e-mail em minúsculas, celular `+55DD9NNNNNNNN`, chave aleatória em minúsculas); um celular digitado sem `+55` gera um erro próprio, em vez de ser lido como CPF inválido;
- CPF e CNPJ têm os dígitos verificadores conferidos, inclusive o CNPJ alfanumérico (letras valem o código ASCII menos 48);
- o Pix copia e cola (BR Code) é decodificado e tem o CRC16 conferido, para mostrar recebedor e valor antes de pagar;
- `PagamentoPix::validar` recusa valores não positivos ou com mais de 2 casas decimais, descrição com mais de 140 caracteres e dados bancários malformados, e `enviar_pix` chama a validação antes de enviar.

Cada envio leva um `x-id-idempotente` (UUID v4 gerado com o gerador aleatório do aws-lc-rs, já presente pelo TLS). Com a mesma chave, a API não paga duas vezes: quando a resposta se perde (tempo esgotado, conexão caída), o mesmo pagamento pode ser reenviado com segurança. O destinatário é um enum marcado por `tipo` (`CHAVE`, `DADOS_BANCARIOS`, `PIX_COPIA_E_COLA`), como o discriminador da especificação; o teste de contrato reproduz, campo a campo, os três exemplos de requisição da documentação.

### Confirmação antes de mover dinheiro

Comandos que movimentam dinheiro validam tudo localmente, mostram um resumo em `stderr` (destino, valor em reais e por extenso, data, ambiente e chave de idempotência) e só enviam depois de um `s` ou `sim`. A resposta só é lida quando o `stdin` é um terminal: `yes | inter-pj pix enviar ...` não paga nada, e scripts precisam dizer `--sim` explicitamente. O limite por operação do perfil vale mesmo com `--sim`.

A pergunta passa pelo trait `Terminal`. Os testes rodam o comando de verdade contra a API simulada com um terminal falso, para provar que uma resposta negativa não faz nenhuma requisição. Os testes E2E do binário cobrem `--simular`, a falta de terminal e o limite.

Quando o envio falha depois de possivelmente ter chegado à API (tempo esgotado, `5xx`, resposta ilegível), o erro vem com a chave de idempotência e a instrução para repetir com `--id-idempotente`, sem risco de pagar duas vezes.

### Extrato completo: paginação e scroll

A paginação tradicional da API alcança apenas as primeiras 10.000 transações de um período. `extrato_completo_todas` pede a primeira página com o tamanho máximo (10.000): se `totalElementos` couber, segue página a página; senão, recomeça no modo *scroll*, lote a lote, até `hasMore = false`. Contadores ausentes ou inconsistentes não levam a laços infinitos (página vazia, total já atingido e um teto de páginas encerram a leitura).

### Detalhes tipados, sem perder dados

`detalhes` não tem discriminador próprio: seu formato depende de `tipoTransacao`. A desserialização lê o tipo e escolhe um dos nove modelos documentados (`DetalhePix`, `DetalhePagamento`...); campos novos ficam em `outros` e tipos sem modelo (ou detalhes fora do formato) ficam em `Detalhe::Outro`, então a saída JSON reproduz tudo o que a API enviou. O teste de contrato deriva o par tipo ↔ modelo dos schemas `Transacao*` da especificação e falha se algum campo documentado não estiver mapeado.

### CSV para planilhas

O CSV segue a RFC 4180 (cabeçalho, CRLF, aspas quando necessário), usa os códigos e nomes de campo da API e valores com sinal (saídas negativas), para somar direto na planilha. Com `--separador ';'` o arquivo sai no padrão do Excel em português (vírgula decimal e BOM UTF-8). Descrições vêm de terceiros (por exemplo, a mensagem de um Pix recebido); textos que começam com `=`, `+`, `-` ou `@` recebem um apóstrofo para não virarem fórmulas (*CSV injection*).

### Valores monetários exatos

Valores usam `rust_decimal::Decimal`. A API envia números JSON (e às vezes strings); a desserialização aceita os dois e converte números pela representação decimal mais curta (`2850.55` continua `2850.55`). Na saída JSON os valores voltam como números, com os mesmos nomes de campo da API.

### Segredos

`client_secret`, tokens e a chave privada ficam em tipos do crate `secrecy` (sem `Debug`/`Display` revelador, memória zerada ao descartar). Logs são filtrados para os crates do projeto, então dependências (HTTP/TLS) não registram cabeçalhos. Testes E2E verificam que segredos nunca aparecem em stdout/stderr, inclusive com `-vv`.

### Configuração explícita

Não há ambiente padrão: sandbox ou produção precisa ser escolhido. A resolução guarda a origem de cada valor (flag, variável, arquivo), exibida por `config mostrar` para facilitar diagnósticos, e lista de uma vez tudo o que falta.

### Erros e códigos de saída

A biblioteca expõe erros tipados (`Error::{Config, Identity, Auth, Api, Transport, Decode}`) com a categoria HTTP (`ApiErrorKind`). A CLI traduz cada categoria para um código de saída estável (ver README), útil em scripts.

### Idioma

Código, comentários e rustdoc em inglês (padrão do ecossistema Rust). Tudo que o usuário lê — ajuda, mensagens, documentação — em português, e os modelos usam os nomes de campo da API (`disponivel`, `bloqueadoCheque`), preservando a linguagem do domínio.
