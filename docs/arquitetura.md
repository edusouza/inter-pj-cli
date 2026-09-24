# Arquitetura

## Visão geral

```text
┌──────────────────────── crates/inter-pj-cli (binário `inter-pj`) ─────────────────────────┐
│ cli.rs, cli/  definição dos comandos (clap) e ajuda em português                          │
│ config.rs     arquivo TOML, perfis, precedência flag > env > arquivo, origem dos valores  │
│ commands/     saldo, extrato, pix, pix_automatico, pagamento, cobranca, webhook,          │
│               auth, config                                                                │
│ token_store   cache de tokens em arquivo (600, gravação atômica)                          │
│ confirmacao   resumo + [s/N] antes de mover dinheiro (só com stdin e stderr em terminal)  │
│ arquivo/      pagamentos, cobranças e recorrências em arquivo: JSON da API e CSV          │
│ valor.rs      valores em reais digitados (150,00 / 1.500,00) e por extenso                │
│ tabela.rs     tabelas em texto alinhado e CSV (RFC 4180, modo Excel pt-BR)                │
│ output.rs     R$ no formato brasileiro, JSON, escrita em stdout                           │
│ qr.rs         QR Code do Pix no terminal e em PNG (gravador próprio)                      │
│ saida.rs      PDF e PNG gravados com permissão 600, sem sobrescrever, ou em stdout        │
│ files.rs      arquivos 600, criados com O_EXCL ou trocados por rename (sem seguir links)  │
│ error.rs      códigos de saída e dicas                                                    │
└──────────────────────────────────────┬────────────────────────────────────────────────────┘
                                       │ usa
┌──────────────────────── crates/inter-pj (biblioteca `inter_pj`) ──────────────────────────┐
│ client.rs     InterClient: reqwest + rustls, mTLS, bearer, x-conta-corrente, 401 → renova │
│ retry.rs      RetryPolicy: backoff exponencial com jitter, Retry-After, modos de repetição│
│ auth.rs       AccessToken, TokenStore, TokenManager (memória + armazenamento externo)     │
│ endpoint.rs   registro de operações (método, caminho, escopos) — verificado por contrato  │
│ identity.rs   certificado + chave PEM validados                                           │
│ scope.rs      os 36 escopos documentados                                                  │
│ problem.rs    parser tolerante de erros (RFC 7807 e variações)                            │
│ banking/      saldo, extrato (scroll), PDF, Pix, pagamentos (boleto, DARF, lote)          │
│ cobranca/     emissão, consulta, listagem, sumário, PDF, cancelamento e edição            │
│ boleto.rs     linha digitável e código de barras (FEBRABAN): DVs, valor, vencimento       │
│ documento.rs  CPF e CNPJ (inclusive o alfanumérico) com dígitos verificadores             │
│ pix/          API Pix: cobranças, recebidos, devoluções, locations, lotes e sandbox       │
│               chave Pix (formatos do DICT) e leitura do copia e cola (BR Code, CRC16)     │
│ pix_automatico/                                                                           │
│               Pix Automático: recorrências, solicitações de confirmação, cobranças        │
│               recorrentes, locations, webhooks e sandbox                                  │
│ webhook.rs    webhooks das três APIs: URL conferida, callbacks e reenvio                  │
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

Cada requisição tem um modo de repetição. Consultas (`GET`) e o pedido de token são repetidas em `429`, `500`, `502`, `503`, `504`, falhas de conexão e tempo esgotado. As requisições do modo *scroll* do extrato mudam estado no servidor (avançam o cursor): repeti-las depois de um `504` poderia pular um lote inteiro, então elas só são repetidas quando certamente não foram processadas (`429` ou conexão recusada). O envio de Pix também só é repetido nesses dois casos, e sempre com a mesma chave de idempotência; depois de um `5xx` ou de tempo esgotado o resultado é incerto e a decisão fica com quem chamou. As cobranças Pix e as devoluções seguem a mesma regra e, como levam o txid ou o id da devolução no caminho, quem chamou pode repeti-las com segurança. Pagamentos por código de barras, DARFs, lotes, cancelamentos e as operações da API de Cobrança seguem a mesma regra, mas não têm chave de idempotência: depois de um resultado incerto, o pagamento ou a cobrança deve ser consultado antes de uma nova tentativa. Os cadastros, as exclusões e os reenvios de webhooks também só são repetidos nesses dois casos, e uma alteração incerta vem com o comando que mostra o webhook. No Pix Automático, a criação de recorrências e de solicitações segue a regra dos pagamentos, sem chave de idempotência, e a das cobranças recorrentes, a das cobranças Pix, com o txid no caminho. Nenhuma outra operação com efeitos é repetida. Falhas de TLS (certificado recusado, CA desconhecida) também não, porque repetir não resolve.

A espera cresce exponencialmente a partir de 1 s, com *jitter* (entre metade e o total do intervalo) e teto de 60 s; um `Retry-After` maior que o teto faz a CLI desistir na hora, com a dica de aguardar.

### Pix: validação local e idempotência

Tudo o que pode ser conferido antes de mover dinheiro é conferido localmente, sem chamar a API:

- chaves Pix são reconhecidas e normalizadas como o DICT as guarda (CPF/CNPJ só com dígitos, e-mail em minúsculas, celular `+55DD9NNNNNNNN`, chave aleatória em minúsculas); um celular digitado sem `+55` gera um erro próprio, em vez de ser lido como CPF inválido;
- CPF e CNPJ têm os dígitos verificadores conferidos, inclusive o CNPJ alfanumérico (letras valem o código ASCII menos 48);
- o Pix copia e cola (BR Code) é decodificado e tem o CRC16 conferido, para mostrar recebedor e valor antes de pagar;
- `PagamentoPix::validar` recusa valores não positivos ou com mais de 2 casas decimais, descrição com mais de 140 caracteres e dados bancários malformados, e `enviar_pix` chama a validação antes de enviar.

Cada envio leva um `x-id-idempotente` (UUID v4 gerado com o gerador aleatório do aws-lc-rs, já presente pelo TLS). Com a mesma chave, a API não paga duas vezes: quando a resposta se perde (tempo esgotado, conexão caída), o mesmo pagamento pode ser reenviado com segurança. O destinatário é um enum marcado por `tipo` (`CHAVE`, `DADOS_BANCARIOS`, `PIX_COPIA_E_COLA`), como o discriminador da especificação; o teste de contrato reproduz, campo a campo, os três exemplos de requisição da documentação.

### Boletos e contas conferidos localmente

`boleto::CodigoBarras` aceita a linha digitável ou o código de barras e confere todos os dígitos verificadores antes de qualquer pagamento. São três DVs por campo (módulo 10) e o DV geral (módulo 11) nos boletos, e um DV por bloco nas contas e tributos (módulo 10 ou 11, conforme o terceiro dígito). O valor e o vencimento são decodificados para o resumo.

O fator de vencimento chegou a 9999 em 21/02/2025 e recomeçou em 1000 no dia seguinte, então um mesmo fator corresponde a duas datas, com cerca de 24,6 anos entre elas. Escolhemos a mais próxima da data atual.

O padrão tem um ponto cego conhecido: no boleto, os restos 0, 1 e 10 do módulo 11 viram todos o DV 1, então um dígito errado no valor ou no vencimento pode passar despercebido. Por isso a CLI mostra os dois, decodificados, antes da confirmação. Um teste documenta esse limite.

As implementações foram conferidas com uma implementação de referência independente, usando os exemplos da documentação oficial como vetores de teste.

### DARF e lotes

`PagamentoDarf::validar` confere o que a documentação define (código da receita de 4 dígitos, referência só com dígitos, tamanhos dos textos, valores com até 2 casas) e o CPF/CNPJ do contribuinte já chega validado como `Documento`.

Um lote reúne de 2 a 150 pagamentos (`ItemLote::Boleto` ou `ItemLote::Darf`), serializados com o discriminador `tipoPagamento` da especificação. Os itens são os mesmos modelos dos pagamentos avulsos, com uma diferença documentada: no lote, `valorPagar` do boleto é número, e não texto. `LotePagamentos::validar` aponta o primeiro item inválido; `ItemLote::validar` permite relatar todos. A API aceita o lote (`202`) e o processa depois: `consultar_lote` traz o status do lote e de cada pagamento, lidos pelo `tipoPagamento`; itens de tipos desconhecidos ou fora do formato ficam em `PagamentoDoLote::Outro`, como os detalhes do extrato.

### Confirmação antes de mover dinheiro

Comandos que movimentam dinheiro validam tudo localmente, mostram um resumo em `stderr` (destino, valor em reais e por extenso, data, ambiente e chave de idempotência) e só enviam depois de um `s` ou `sim`. A resposta só é lida quando o `stdin` e o `stderr` são terminais: `yes | inter-pj pix enviar ...` não paga nada, uma pergunta gravada num arquivo (`2> log`) não é respondida às cegas, e scripts precisam dizer `--sim` explicitamente. O limite por operação do perfil vale mesmo com `--sim`.

A pergunta passa pelo trait `Terminal`. Os testes rodam o comando de verdade contra a API simulada com um terminal falso, para provar que uma resposta negativa não faz nenhuma requisição. Os testes E2E do binário cobrem `--simular`, a falta de terminal e o limite.

Quando o envio falha depois de possivelmente ter chegado à API (tempo esgotado, `5xx`, resposta ilegível), o erro vem com a chave de idempotência e a instrução para repetir com `--id-idempotente`, sem risco de pagar duas vezes.

Os pagamentos (boleto, DARF e lote) não têm chave de idempotência. Nesse mesmo caso, o erro vem com o comando que confere se o pagamento foi feito (`pagamento boleto listar --codigo ...`, `pagamento darf listar --codigo-receita ...`), para usar antes de uma nova tentativa; num lote, cada pagamento aparece na listagem do seu tipo.

### Arquivos de pagamento

DARFs (`pagamento darf pagar --arquivo`) e lotes (`pagamento lote enviar --arquivo`) vêm de arquivos com os nomes de campo da API, para que a documentação do Inter valha também para eles. Campos desconhecidos são recusados, porque um erro de digitação (`valorMuta`) não pode apagar a multa em silêncio, e cada erro aponta o arquivo, a linha ou posição e o campo. Valores aceitam número JSON (`47.14`) ou texto como as pessoas digitam (`"47,14"`, com as mesmas regras de `valor.rs`); datas, só `AAAA-MM-DD`, porque `05/10` é ambíguo entre planilhas brasileiras e americanas.

O CSV do lote segue a RFC 4180 (aspas, quebras de linha dentro de aspas, CRLF). O separador, `,` ou `;`, é detectado pelo cabeçalho, e o BOM é ignorado, como o Excel em português grava. Cada linha vira o mesmo objeto JSON do outro formato, então as duas entradas passam pela mesma validação. O lote inteiro é conferido antes do envio, e todos os problemas são relatados de uma vez. O Excel, ao abrir um CSV, troca números longos por notação científica (perdendo dígitos) e tira zeros à esquerda: esses casos são reconhecidos e explicados. Os modelos (`pagamento lote modelo`) usam a linha digitável e o CNPJ formatados, que o Excel mantém como texto.

### Cobranças: emissão assíncrona e QR Code

A emissão é assíncrona: a API responde com o código da solicitação, e a cobrança fica `EM_PROCESSAMENTO` até o boleto e o Pix serem gerados; logo depois do pedido, a consulta pode ainda nem encontrá-la. `cobranca emitir --aguardar` trata os dois casos como emissão em andamento e consulta a cada 6 segundos, o limite do sandbox (10 por minuto). A edição segue o mesmo padrão, acompanhada pelo `codigoEdicao`.

Não há chave de idempotência, mas a API recusa, por 30 minutos, uma cobrança com o mesmo seu número, valor, vencimento e pagador. Um resultado incerto vem com o comando que procura a cobrança pelo seu número, e as dicas que trazem comandos põem entre aspas o que o shell interpretaria.

`EmissaoCobranca::validar` confere o que a documentação define: tamanhos, valor de R$ 2,50 a R$ 99.999.999,99, `numDiasAgenda` até 60, CPF/CNPJ do pagador e do beneficiário final, UF, CEP e a chave de acesso da nota fiscal (dígito verificador, número e série). `tipoPessoa` vem do documento, para que os dois nunca discordem. Os erros apontam o campo da API, que a CLI traduz para a opção (`--pagador-email`) ou para o caminho no arquivo (`pagador.cep`).

O QR Code do Pix é gerado localmente a partir do copia e cola, depois de conferido o CRC16, com o crate `qrcode` sem recursos opcionais (nenhuma dependência de imagem). No terminal, cada caractere desenha dois módulos (`▀`, `▄`, `█`): com a saída em um terminal, em preto no branco por cores ANSI, qualquer que seja o tema; sem cores, os módulos claros é que são desenhados, como no `qrencode -t UTF8`, o que funciona em fundos escuros. O PNG sai de um gravador próprio: 1 bit por pixel, blocos *deflate* sem compressão, CRC-32 e Adler-32. Os testes leem de volta, com um decodificador independente (`rqrr`, só nos testes), o QR Code desenhado nos dois modos e o PNG gravado.

### Pix Cobrança: txid, valores e devoluções

A API Pix segue o padrão do Banco Central: uma cobrança é identificada pelo seu txid (26 a 35 letras e dígitos) e criada com `PUT /cob/{txid}` ou `PUT /cobv/{txid}`. A CLI gera o txid antes do envio (32 dígitos hexadecimais, do gerador aleatório do aws-lc-rs) e o mostra no resumo. Como a API não cria duas cobranças com o mesmo txid, um resultado incerto vem com o comando que consulta a cobrança e o que repete a criação com `--txid`, sem risco de duplicá-la. A devolução funciona da mesma forma, com o seu id no caminho (`PUT /pix/{e2eId}/devolucao/{id}`), e repetir um id que o Pix já tem mostra a devolução existente e o seu desfecho, inclusive no código de saída. Os lotes também levam o seu id no caminho, e as suas cobranças, os seus txids, que a API não repete; ainda assim, um resultado incerto vem com o comando que consulta o lote antes de uma nova tentativa.

Os valores vão como texto com 2 casas (`"37.00"`, até 10 dígitos antes da vírgula), como a especificação define, e as modalidades de multa, juros, abatimento e desconto, como números. Os horários chegam em RFC 3339 e são mostrados no fuso local; os períodos das listagens aceitam datas, lidas como dias inteiros no fuso local, ou data e hora com fuso. As funções que desenham recebem o fuso, para que os testes não dependam da máquina.

Uma devolução é um envio de dinheiro e tem os trilhos do Pix: consulta do Pix antes, resumo, confirmação só de um terminal, `limite_por_operacao` e `--simular`. O que ainda pode ser devolvido é calculado de forma conservadora: do valor do Pix saem as devoluções realizadas e as em andamento, inclusive as de status desconhecido; só as não realizadas não contam. Uma devolução maior que o restante é recusada antes do envio.

A revisão de uma cobrança com vencimento muda só o que foi informado, então ela é conferida depois da consulta, com a cobrança como ficará: um novo vencimento anterior à data de um desconto atual, por exemplo, é recusado antes do envio. Cobranças pagas ou removidas são recusadas sem nenhuma alteração.

O CSV dos lotes de cobranças tem os caminhos dos campos da API como colunas (`valor.multa.valorPerc`, `valor.desconto.descontoDataFixa[0].data`), e cada linha vira o mesmo objeto do JSON, conferido pela mesma validação da criação avulsa. As posições das listas ficam como nas colunas, para que as mensagens apontem a coluna certa, e colunas desconhecidas são recusadas. Como no lote de pagamentos, os estragos do Excel (txid, CPF, CNPJ e location em notação científica; CPF, CNPJ e CEP sem os zeros à esquerda) são reconhecidos e explicados, e txids repetidos são apontados antes do envio.

### Webhooks e callbacks

Cada API tem os seus webhooks: um por tipo no Banking (`pix-pagamento`, `boleto-pagamento`), um na API de Cobrança e um por chave na API Pix. `WebhookUrl` confere o que a documentação define (começa com `https://`, sem espaços) e que a URL tem um servidor, e a CLI avisa quando ele é local ou de rede privada, que o Inter não alcança. Como a nova URL passa a receber as notificações dos pagamentos da conta, cadastrar e excluir seguem os trilhos das operações sensíveis: consulta do webhook atual, o antes e o depois, e confirmação só de um terminal, ou `--sim`; cadastrar a mesma URL não muda nada. A consulta de uma conta sem webhook (`404`) devolve `None`. Chaves Pix de telefone vão no caminho sem o `+`, como a documentação pede, e no corpo do reenvio como o DICT as guarda.

A especificação se contradiz no reenvio de callbacks de cobranças: a chave da operação é `/cobranca/v3/webhook/callbacks/retry`, mas a sua descrição mostra `/cobranca/v3/cobrancas/webhook/callbacks/retry`, o padrão dos reenvios das outras APIs (o caminho do histórico mais `/retry`). A biblioteca usa o segundo. Os testes de contrato o associam à chave da especificação (`spec::DIVERGENCIAS`), conferem escopos e modelos e falham se a especificação passar a ter o caminho, para que a exceção não dure mais que o erro.

O histórico dos callbacks guarda o conteúdo enviado como veio, porque os schemas não o descrevem bem: o do Pix repete o da Cobrança, e o exemplo documentado traz `{}` onde o schema diz uma lista. `Callback::valores` procura um campo onde quer que ele esteja, para mostrar o código da operação que o reenvio recebe. No Banking, esse código não é o do filtro: o histórico de `pix-pagamento` filtra pelo `endToEnd`, e o reenvio pede o `codigoSolicitacao`. O comando sugerido para reenviar considera todas as tentativas do período, então uma operação entregue numa tentativa automática posterior fica de fora.

O reenvio aceita até 50 operações por pedido e 5 pedidos por minuto, em produção. A CLI confere todos os códigos antes do primeiro pedido, para que um código inválido no segundo bloco não deixe o primeiro enviado, tira as repetições e divide o resto em blocos de 50; com mais de 5 blocos, espera 12 segundos entre eles, porque uma retentativa depois de um `429` não cobre o minuto inteiro. Se um bloco falha depois de outros, a dica diz quantos já foram pedidos e traz o comando para os restantes.

### Pix Automático: recorrências, solicitações e cobranças recorrentes

O Pix Automático encadeia três objetos. A recorrência (`rec`) é a autorização do pagador: o devedor e o contrato, o período, a periodicidade, o valor (fixo, um mínimo para o limite que o pagador define, ou o de cada cobrança) e a política de retentativas. O pagador a aprova no banco dele, pelo QR Code de uma location (`locrec`), pelo QR Code composto com uma cobrança imediata ou com vencimento, ou por uma solicitação de confirmação (`solicrec`) que o recebedor envia ao banco do pagador. Aprovada a recorrência, cada ciclo tem uma cobrança recorrente (`cobr`), que o banco do pagador agenda e debita no vencimento.

`IdRec` e `IdSolicRec` são conferidos antes de qualquer requisição (29 letras e dígitos), e as regras que a documentação define (tamanhos, datas, valores, a conta e o ISPB do pagador, a conta que recebe) são conferidas com o campo como a API o nomeia, que a CLI traduz para a opção. A criação de recorrências e de solicitações não tem chave de idempotência: a biblioteca só a repete quando certamente não foi processada, e um resultado incerto vira `CriacaoIncerta`, com o comando que confere o que aconteceu. A cobrança recorrente é criada com o txid no caminho (`PUT /cobr/{txid}`), que a CLI gera quando não é informado; como a API não cria duas com o mesmo txid, repeti-la é seguro, como nas cobranças Pix.

Antes de enviar, a CLI consulta aquilo de que a operação depende. A solicitação e a cobrança consultam a recorrência: só uma aprovada aceita cobranças, e uma já aprovada ou encerrada não aceita solicitação. O cancelamento de uma solicitação vale enquanto ela não tem resposta, e o pedido de nova tentativa confere a política da recorrência e a janela de 7 dias depois da liquidação prevista, que a especificação define. O que só a API decide vira aviso no resumo, e não recusa: um valor diferente do fixo, um vencimento fora do período da recorrência, o limite de 3 tentativas, dois pedidos no mesmo dia e o horário limite do cancelamento (22h do dia anterior à liquidação).

A especificação tem particularidades que a biblioteca e os testes de contrato levam em conta. O caminho do sandbox de cobranças usa `{txId}`, enquanto o parâmetro documentado é `txid`; o sandbox da solicitação é endereçado pela recorrência, e o da recorrência pede o escopo `pix.write`. Os webhooks entregam as notificações no endereço cadastrado seguido de `/rec` e `/cobr`, com os corpos `{recs: [...]}` e `{cobsr: [...]}`, e a CLI mostra esse endereço. Os exemplos trazem o CEP com hífen, a conta como número, o tipo de conta `POUPANÇA` com cedilha e o status das atualizações em `nome`, que os modelos leem como vêm; os testes de contrato leem os exemplos da especificação em tempo de execução, com CPF e CNPJ sintéticos no lugar dos dela.

### Extrato completo: paginação e scroll

A paginação tradicional da API alcança apenas as primeiras 10.000 transações de um período. `extrato_completo_todas` pede a primeira página com o tamanho máximo (10.000): se `totalElementos` couber, segue página a página; senão, recomeça no modo *scroll*, lote a lote, até `hasMore = false`. Contadores ausentes ou inconsistentes não levam a laços infinitos (página vazia, total já atingido e um teto de páginas encerram a leitura).

### Detalhes tipados, sem perder dados

`detalhes` não tem discriminador próprio: seu formato depende de `tipoTransacao`. A desserialização lê o tipo e escolhe um dos nove modelos documentados (`DetalhePix`, `DetalhePagamento`...); campos novos ficam em `outros` e tipos sem modelo (ou detalhes fora do formato) ficam em `Detalhe::Outro`, então a saída JSON reproduz tudo o que a API enviou. O teste de contrato deriva o par tipo ↔ modelo dos schemas `Transacao*` da especificação e falha se algum campo documentado não estiver mapeado.

### CSV para planilhas

O CSV segue a RFC 4180 (cabeçalho, CRLF, aspas quando necessário), usa os códigos e nomes de campo da API e valores com sinal (saídas negativas), para somar direto na planilha. Com `--separador ';'` o arquivo sai no padrão do Excel em português (vírgula decimal e BOM UTF-8). Descrições vêm de terceiros (por exemplo, a mensagem de um Pix recebido); textos que começam com `=`, `+`, `-` ou `@` recebem um apóstrofo para não virarem fórmulas (*CSV injection*), e também esses caracteres depois de `,`, `;`, tabulação ou quebra de linha dentro do texto, porque o Excel em português separa um `.csv` por `;` e começaria uma célula ali. Num arquivo, o CSV traz os textos como vieram; no terminal (`output::print_csv`), eles passam pelo filtro da saída em texto, sem o BOM.

### O que chega ao terminal

`output::perigoso` define o que um texto de terceiros nunca leva ao terminal: os caracteres de controle e os de formatação que invertem ou escondem texto (marcas, *embeddings*, *overrides* e *isolates* bidirecionais, espaços de largura zero, o BOM, os separadores de linha); os *joiners* das sequências de emoji ficam. `limpo` troca todos eles por `�` numa linha; `sem_controle` guarda as quebras de linha de um texto feito de linhas. Tudo o que sai em stdout passa por `print` (filtro sobre o texto todo), `print_json` (os mesmos caracteres escapados como `\u`, sem mudar os dados) ou `print_csv`; tudo o que sai em stderr, por `eprint`, `eprint_linha` (uma linha só, para avisos e progresso) ou `error::report`, que recua as linhas seguintes de um erro para que só as linhas da CLI comecem na margem. A biblioteca põe numa linha os textos de um erro da API (`Problem`), e um erro do parser que cite um valor com esses caracteres (um código colado) sai sem cores, filtrado.

### Valores monetários exatos

Valores usam `rust_decimal::Decimal`. A API envia números JSON (e às vezes strings); a desserialização aceita os dois e converte números pela representação decimal mais curta (`2850.55` continua `2850.55`). Na saída JSON os valores voltam como números, com os mesmos nomes de campo da API.

### Segredos

`client_secret`, tokens e a chave privada ficam em tipos do crate `secrecy` (sem `Debug`/`Display` revelador, memória zerada ao descartar). Logs são filtrados para os crates do projeto, então dependências (HTTP/TLS) não registram cabeçalhos. Testes E2E verificam que segredos nunca aparecem em stdout/stderr, inclusive com `-vv`.

### Configuração explícita

Não há ambiente padrão: sandbox ou produção precisa ser escolhido. A resolução guarda a origem de cada valor (flag, variável, arquivo), exibida por `config mostrar` para facilitar diagnósticos, e lista de uma vez tudo o que falta.

### Erros e códigos de saída

A biblioteca expõe erros tipados (`Error::{Config, Identity, Auth, Api, Transport, Decode}`) com a categoria HTTP (`ApiErrorKind`). A CLI traduz cada categoria para um código de saída estável (ver README), útil em scripts. Um envio cujo resultado ficou incerto (`error::resultado_incerto`: tempo esgotado, `5xx`, resposta ilegível) tem o seu próprio código, 9, e não o da categoria: um script precisa distinguir "pode ter sido feito" de "não foi feito" (um `429` ou uma conexão recusada, 6), que pode repetir.

### Cores só na saída padrão, decididas uma vez

As tabelas são texto simples por padrão (`Tabela::texto`), o que vale para os resumos e os erros, em stderr, e para os testes; as listagens e consultas pedem as cores (`Tabela::texto_colorido`), que só existem se a saída padrão for um terminal que as aceita. A decisão é tomada no início, com as regras do `anstream` (já presente pelo clap): `NO_COLOR` desliga, `CLICOLOR_FORCE` liga, `TERM=dumb` e um pipe desligam; `--sem-cor` é visto antes do parser, para valer também na ajuda. O filtro de caracteres de controle da saída deixa passar só os códigos de cor da CLI (`cores::CODIGOS`), escritos em volta de textos já filtrados; no Windows, eles passam pelo modo ANSI do console (ou pela API dele, num console antigo). O tom de cada status é decidido junto da sua descrição, em cada comando, e um status desconhecido fica sem cor.

### Idioma

Código, comentários e rustdoc em inglês (padrão do ecossistema Rust). Tudo que o usuário lê — ajuda, mensagens, documentação — em português, e os modelos usam os nomes de campo da API (`disponivel`, `bloqueadoCheque`), preservando a linguagem do domínio.

O clap não tem tradução: os títulos e as linhas de uso da ajuda são definidos em `cli::command`, para todos os comandos, e os erros do parser passam por um formatador próprio (`cli::Portugues`), que os escreve a partir do contexto que o clap dá (o argumento, o valor, as sugestões, a linha de uso), sem cores, como os outros erros. Os poucos textos que o clap e a biblioteca padrão escrevem por extenso, como as dicas e as faixas dos números, são reconhecidos e traduzidos, e um teste falha se sobrar inglês. As páginas de manual geradas pelo `clap_mangen` a partir da mesma definição têm os títulos e rótulos em inglês trocados (`commands/manual.rs`), com um teste que falha se algum sobrar depois de uma atualização da dependência.

### Guias conferidos pelos testes

Um exemplo de documentação que ninguém executa fica velho sem aviso. Os blocos `console` dos guias (`docs/guias`) são executados pelo teste `tests/guias` como sessões de terminal: cada guia tem uma pasta pessoal própria, mostrada como `/home/voce`, com a configuração de uma empresa fictícia no perfil de produção, o dia de hoje fixo (`INTER_HOJE`, que a CLI só aceita junto com um `INTER_BASE_URL` local, para que um agendamento real nunca siga um dia falso) e um banco simulado (`tests/guias/banco`), um módulo por API, cujos dados contam uma história só: o saldo de um dia é o saldo inicial mais as transações do extrato até ele (`banco/extrato.json`), então o saldo, o extrato e o extrato completo concordam entre si; os Pix recebidos são os do extrato, e um Pix enviado ou uma devolução podem ser consultados, com a mesma resposta para a mesma chave de idempotência ou o mesmo id; os pagamentos, avulsos ou num lote, entram nas listagens, e num lote um boleto já pago é recusado; a cobrança recebida é a do extrato, uma cobrança emitida fica pronta, com o boleto e o Pix, na requisição seguinte, e a mesma cobrança não é emitida duas vezes; a cobrança Pix paga é a do Pix do extrato com o seu txid, e as locations das cobranças Pix são numeradas na ordem em que foram criadas, uma desvinculada livre para a próxima cobrança, e um lote cria as suas cobranças como `pix cobv criar`, negando a de um txid que já existe; as notificações dos webhooks são as das operações do extrato, e um reenvio aparece no histórico como uma tentativa nova, na URL que o webhook tem então; e uma recorrência do Pix Automático é criada aguardando o pagador, com o id que o banco dá, e alterada ou cancelada na hora, e a solicitação da sua aprovação chega ao banco do pagador na consulta seguinte. A saída de cada comando, stdout e stderr na ordem em que o terminal as mostra, precisa ser a do guia, e o comando precisa falhar quando o guia mostra um `erro:`.

A linha de comando é lida como um shell a leria (aspas, escapes, variáveis na frente, `>` e `<`), mas só o `inter-pj` e o `cat` são executados; um exemplo com pipe ou variáveis do shell é marcado para não rodar. Uma confirmação respondida no guia (`[s/N] s`) vira `--sim`, e um arquivo que o guia mostra, como uma planilha de pagamentos, é gravado na pasta pessoal antes dos comandos que o leem. As chaves e os txids que a CLI gera valem por qualquer outro do mesmo formato, desde que se repitam onde o guia os repete, e a saída que depende do relógio, como os dias que faltam para o certificado vencer, é marcada como ilustrativa: o comando roda, mas só o resultado é conferido. `ATUALIZAR_GUIAS=1` grava no guia o que os comandos imprimem, para quem escreve um exemplo novo.
