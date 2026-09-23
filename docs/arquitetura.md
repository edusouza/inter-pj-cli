# Arquitetura

## Visão geral

```text
┌──────────────────────── crates/inter-pj-cli (binário `inter-pj`) ────────────────────────┐
│ cli.rs        definição dos comandos (clap) e ajuda em português                         │
│ config.rs     arquivo TOML, perfis, precedência flag > env > arquivo, origem dos valores │
│ commands/     saldo, auth, config                                                         │
│ token_store   cache de tokens em arquivo (600, gravação atômica)                          │
│ output.rs     R$ no formato brasileiro, tabelas, JSON                                     │
│ error.rs      códigos de saída e dicas                                                    │
└──────────────────────────────────────┬───────────────────────────────────────────────────┘
                                       │ usa
┌──────────────────────── crates/inter-pj (biblioteca `inter_pj`) ─────────────────────────┐
│ client.rs     InterClient: reqwest + rustls, mTLS, bearer, x-conta-corrente, 401 → renova │
│ auth.rs       AccessToken, TokenStore, TokenManager (memória + armazenamento externo)     │
│ endpoint.rs   registro de operações (método, caminho, escopos) — verificado por contrato  │
│ identity.rs   certificado + chave PEM validados                                           │
│ scope.rs      os 36 escopos documentados                                                  │
│ problem.rs    parser tolerante de erros (RFC 7807 e variações)                            │
│ banking/      modelos e operações da API Banking                                          │
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
