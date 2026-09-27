# inter-pj

[![CI](https://github.com/edusouza/inter-pj-cli/actions/workflows/ci.yml/badge.svg)](https://github.com/edusouza/inter-pj-cli/actions/workflows/ci.yml)
[![Licença: MIT OR Apache-2.0](https://img.shields.io/badge/licen%C3%A7a-MIT%20OR%20Apache--2.0-blue)](#licença)

CLI em Rust para acessar a sua **conta PJ do Inter Empresas** pela linha de comando, usando as [APIs oficiais do Inter](https://developers.inter.co/references) (OAuth2 + mTLS).

> **Projeto não oficial.** Não tem vínculo com o Banco Inter. Use por sua conta e risco e comece pelo ambiente **sandbox**.

```console
$ inter-pj saldo
Saldo disponível          R$ 2.850,55
Bloqueado em cheque         R$ 240,25
Bloqueado judicialmente     R$ 510,35
Bloqueado administrativo      R$ 0,00
Limite                    R$ 1.000,00

$ inter-pj saldo --json | jq .disponivel
2850.55
```

## O que já funciona

| Versão | Funcionalidades |
| --- | --- |
| **0.1.0** | Autenticação OAuth2 com mTLS e cache de token · `saldo` · `auth token`/`auth limpar` · `config init`/`caminho`/`mostrar` |
| **0.1.1** | `config verificar [--corrigir]`: confere o arquivo de configuração e o perfil e corrige caminhos do Windows entre aspas duplas |

O plano completo — extrato, Pix, pagamentos, cobranças (boleto com Pix), Pix Cobrança, webhooks e Pix Automático — está em [`docs/roadmap.md`](docs/roadmap.md) e é acompanhado pelas [issues do projeto](https://github.com/edusouza/inter-pj-cli/issues).

## Instalação

**Binários prontos**: baixe o pacote do seu sistema na página de [releases](https://github.com/edusouza/inter-pj-cli/releases) (Linux x86_64 glibc/musl, macOS Apple Silicon/Intel e Windows), confira o `SHA256SUMS` e coloque o `inter-pj` no seu `PATH`.

**Com Cargo** (Rust 1.88 ou superior):

```console
$ cargo install --locked --git https://github.com/edusouza/inter-pj-cli --tag v0.1.1 inter-pj-cli
```

## Configuração

### 1. Crie a integração no Internet Banking PJ

No Internet Banking do Inter Empresas, crie uma integração com os **escopos** que você vai usar (para `saldo`: `extrato.read`) e baixe:

- o **certificado** (`.crt`) e a **chave privada** (`.key`);
- o **client_id** e o **client_secret** (o segredo só é mostrado uma vez).

Guarde esses arquivos fora de qualquer repositório, com permissão restrita (`chmod 600`).

### 2. Crie o arquivo de configuração

```console
$ inter-pj config init
Arquivo de configuração criado em /home/voce/.config/inter-pj/config.toml
```

Edite o perfil criado:

```toml
perfil_padrao = "padrao"

[perfis.padrao]
ambiente = "sandbox"              # ou "producao"
client_id = "<seu client_id>"
certificado = '~/inter/certificado.crt'
chave_privada = '~/inter/chave.key'
# conta_corrente = "<numero>"     # só se a integração tiver mais de uma conta
# escopos = ["extrato.read"]      # opcional: escopos pedidos em todo token
```

Escreva os caminhos entre **aspas simples**, principalmente no Windows (`certificado = 'C:\inter\certificado.crt'`). Entre aspas duplas, a barra invertida começa um escape do TOML. `"C:\Users\..."` impede a leitura do arquivo (`\U` pede 8 dígitos hexadecimais). Já `"C:\novo\teste.crt"` é lido, mas com uma quebra de linha e uma tabulação no lugar de `\n` e `\t`. Caminhos relativos são relativos ao arquivo de configuração, e `~/` é a sua pasta pessoal.

### 3. Informe o segredo pela variável de ambiente

```console
$ export INTER_CLIENT_SECRET='<seu client_secret>'
$ inter-pj config verificar   # confere o arquivo, o perfil, o certificado e a chave
$ inter-pj config mostrar     # mostra a configuração efetiva (segredos ocultos)
$ inter-pj saldo
```

No PowerShell, defina o segredo com `$env:INTER_CLIENT_SECRET = '<seu client_secret>'`.

O `client_secret` **nunca** é aceito como flag (evita que fique no histórico do shell). Ele pode, alternativamente, ficar no arquivo de configuração — nesse caso a CLI avisa se o arquivo puder ser lido por outros usuários.

### Se o arquivo de configuração não for lido

O `config verificar` aponta cada problema com a linha e a correção. Com `--corrigir`, ele troca as aspas duplas dos caminhos do Windows por aspas simples e guarda o original em `config.toml.bak`:

```console
PS> inter-pj config mostrar
erro: arquivo de configuração inválido (C:\Users\voce\AppData\Roaming\inter-pj\config.toml, linha 7): o valor de certificado está entre aspas duplas e tem uma barra invertida, que em TOML começa um escape; nos caminhos do Windows, use aspas simples (certificado = 'C:\pasta\arquivo')
dica: `inter-pj config verificar` mostra a correção de cada linha; com `--corrigir`, ele a aplica e guarda uma cópia do arquivo original

PS> inter-pj config verificar
Arquivo: C:\Users\voce\AppData\Roaming\inter-pj\config.toml

erro      linha 7, certificado: caminho do Windows entre aspas duplas: em TOML, a barra invertida começa um escape (\U pede 8 dígitos hexadecimais), e o arquivo não é lido
          corrija para: certificado = 'C:\Users\voce\inter\certificado.crt'
erro      linha 8, chave_privada: caminho do Windows entre aspas duplas: em TOML, a barra invertida começa um escape (\U pede 8 dígitos hexadecimais), e o arquivo não é lido
          corrija para: chave_privada = 'C:\Users\voce\inter\chave.key'

2 erros e 0 avisos.
Para trocar as aspas: inter-pj config verificar --corrigir
erro: a configuração tem 2 erros

PS> inter-pj config verificar --corrigir
Arquivo: C:\Users\voce\AppData\Roaming\inter-pj\config.toml

corrigido linha 7, certificado: aspas simples no lugar das aspas duplas
corrigido linha 8, chave_privada: aspas simples no lugar das aspas duplas
ok        sintaxe: o arquivo é TOML válido
ok        perfil: padrao (arquivo)
ok        ambiente: sandbox (arquivo)
ok        client_id: *****************4e5f (arquivo)
ok        client_secret: definido, oculto (variável INTER_CLIENT_SECRET)
ok        certificado: C:\Users\voce\inter\certificado.crt (arquivo)
ok        chave_privada: C:\Users\voce\inter\chave.key (arquivo)
ok        certificado e chave: lidos e aceitos pela biblioteca TLS

Cópia do arquivo original: C:\Users\voce\AppData\Roaming\inter-pj\config.toml.bak

Nenhum problema encontrado.
```

O comando sai com código 3 se encontrar algum erro, e `--json` dá o resultado para scripts. O `client_secret` nunca aparece, e a conta corrente sai mascarada, como no `config mostrar`.

### Perfis, variáveis e precedência

Vários perfis (ex.: `sandbox` e `producao`) podem conviver no mesmo arquivo; escolha com `--perfil` ou `INTER_PERFIL`. Cada valor é resolvido na ordem **flag > variável de ambiente > arquivo**:

| Configuração | Flag | Variável de ambiente |
| --- | --- | --- |
| Perfil | `-p`, `--perfil` | `INTER_PERFIL` |
| Arquivo de configuração | `--config` | `INTER_CONFIG` |
| Ambiente (`sandbox`/`producao`) | `--ambiente` | `INTER_AMBIENTE` |
| client_id | `--client-id` | `INTER_CLIENT_ID` |
| client_secret | — | `INTER_CLIENT_SECRET` |
| Certificado (`.crt`) | `--certificado` | `INTER_CERTIFICADO` |
| Chave privada (`.key`) | `--chave-privada` | `INTER_CHAVE_PRIVADA` |
| Conta corrente | `--conta-corrente` | `INTER_CONTA_CORRENTE` |
| Diretório de cache | — | `INTER_CACHE_DIR` |

Locais padrão: configuração em `~/.config/inter-pj/config.toml` (Windows: `%APPDATA%\inter-pj\config.toml`) e cache em `~/.cache/inter-pj` (Windows: `%LOCALAPPDATA%\inter-pj`). Veja com `inter-pj config caminho`.

## Uso

```console
$ inter-pj saldo                        # saldo atual, bloqueios e limite
$ inter-pj saldo --data 2026-08-31      # saldo disponível ao fim do dia
$ inter-pj saldo --json                 # JSON com os nomes de campo da API

$ inter-pj auth token --escopo extrato.read      # valida credenciais e mostra a validade
$ inter-pj auth limpar                           # apaga os tokens em cache do perfil

$ inter-pj --perfil producao saldo
$ inter-pj --help                                # ajuda de todos os comandos
```

### Tokens e rate limit

O endpoint de token do Inter aceita apenas **5 chamadas por minuto** e cada token vale **60 minutos**. A CLI pede tokens só com os escopos que o comando precisa e os reaproveita entre execuções por meio de um cache local (arquivo com permissão `600`). Use `--sem-cache` para desativá-lo.

Para chamar a API diretamente (ex.: com `curl`), `auth token --exibir` imprime apenas o token — trate-o como uma senha:

```console
$ curl --cert certificado.crt --key chave.key \
    -H "Authorization: Bearer $(inter-pj auth token --escopo extrato.read --exibir)" \
    https://cdpj-sandbox.partners.uatinter.co/banking/v2/saldo
```

### Códigos de saída

| Código | Significado |
| --- | --- |
| 0 | sucesso |
| 1 | erro inesperado |
| 2 | uso incorreto (argumentos inválidos) |
| 3 | configuração ausente ou inválida (inclui certificado/chave) |
| 4 | falha de autenticação ou acesso negado (credenciais, escopos, 401/403) |
| 5 | requisição rejeitada pela API (400, 404, 409, 422) |
| 6 | serviço indisponível, limite de requisições (429), erro 5xx ou falha de rede |

Mensagens de erro vão para `stderr`, em português, com a explicação da API e dicas. Com `-v`/`-vv` a CLI mostra detalhes das requisições (método, caminho, status e tempo) — nunca tokens, segredos ou corpos de resposta.

## Segurança e privacidade

- Nenhuma credencial, certificado, token ou número de conta faz parte do repositório; o CI roda o [gitleaks](https://github.com/gitleaks/gitleaks) sobre todo o histórico a cada push.
- Segredos são mantidos em tipos que não aparecem em logs nem em mensagens de erro.
- TLS com [rustls](https://github.com/rustls/rustls) (sem OpenSSL), mTLS obrigatório e somente `https`.

Detalhes e como reportar vulnerabilidades em [`SECURITY.md`](SECURITY.md).

## Desenvolvimento

O projeto é um workspace Cargo:

| Crate | Conteúdo |
| --- | --- |
| [`crates/inter-pj`](crates/inter-pj) | biblioteca: cliente HTTP, OAuth2/mTLS, cache de token, modelos das APIs |
| [`crates/inter-pj-cli`](crates/inter-pj-cli) | binário `inter-pj`: comandos, configuração, formatação |

```console
$ cargo test --workspace          # unitários, integração (mock), mTLS real, contrato e E2E
$ cargo clippy --workspace --all-targets -- -D warnings
$ cargo fmt --all --check
```

Os testes rodam offline, sem credenciais: um servidor mock simula a API, os certificados são gerados a cada execução e os contratos são verificados contra a [especificação OpenAPI](spec/). Arquitetura em [`docs/arquitetura.md`](docs/arquitetura.md); fluxo de contribuição em [`CONTRIBUTING.md`](CONTRIBUTING.md).

## Licença

Distribuído sob os termos de qualquer uma das licenças, à sua escolha:

- Apache License, Version 2.0 ([`LICENSE-APACHE`](LICENSE-APACHE))
- MIT license ([`LICENSE-MIT`](LICENSE-MIT))
