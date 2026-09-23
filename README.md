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

$ inter-pj extrato --inicio 2026-08-01 --fim 2026-08-31
Extrato de 01/08/2026 a 31/08/2026

Data        Tipo       Descrição                                   Valor
03/08/2026  Pix        Pix recebido · Cliente Exemplo Ltda   R$ 1.500,00
05/08/2026  Pagamento  Pagamento efetuado · Boleto; energia   -R$ 250,10

Entradas              R$ 1.500,00
Saídas                 -R$ 250,10
Resultado do período  R$ 1.249,90
2 transações
```

## O que já funciona

| Versão | Funcionalidades |
| --- | --- |
| **0.1.0** | Autenticação OAuth2 com mTLS e cache de token · `saldo` · `auth token`/`auth limpar` · `config init`/`caminho`/`mostrar` |
| **0.2.0** | `extrato` · `extrato completo` (detalhes, filtros, todas as páginas e modo scroll) · `extrato pdf` · saída CSV · retentativas automáticas |

O plano completo — Pix, pagamentos, cobranças (boleto com Pix), Pix Cobrança, webhooks e Pix Automático — está em [`docs/roadmap.md`](docs/roadmap.md) e é acompanhado pelas [issues do projeto](https://github.com/edusouza/inter-pj-cli/issues).

## Instalação

**Binários prontos**: baixe o pacote do seu sistema na página de [releases](https://github.com/edusouza/inter-pj-cli/releases) (Linux x86_64 glibc/musl, macOS Apple Silicon/Intel e Windows), confira o `SHA256SUMS` e coloque o `inter-pj` no seu `PATH`.

**Com Cargo** (Rust 1.88 ou superior):

```console
$ cargo install --locked --git https://github.com/edusouza/inter-pj-cli --tag v0.2.0 inter-pj-cli
```

## Configuração

### 1. Crie a integração no Internet Banking PJ

No Internet Banking do Inter Empresas, crie uma integração com os **escopos** que você vai usar (para `saldo` e `extrato`: `extrato.read`) e baixe:

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
certificado = "~/inter/certificado.crt"
chave_privada = "~/inter/chave.key"
# conta_corrente = "<numero>"     # só se a integração tiver mais de uma conta
# escopos = ["extrato.read"]      # opcional: escopos pedidos em todo token
# limite_por_operacao = "1.000,00" # opcional: valor máximo de cada Pix
```

### 3. Informe o segredo pela variável de ambiente

```console
$ export INTER_CLIENT_SECRET='<seu client_secret>'
$ inter-pj config mostrar     # confere a configuração efetiva (segredos ocultos)
$ inter-pj saldo
```

O `client_secret` **nunca** é aceito como flag (evita que fique no histórico do shell). Ele pode, alternativamente, ficar no arquivo de configuração — nesse caso a CLI avisa se o arquivo puder ser lido por outros usuários.

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
| Tentativas por requisição | `--tentativas` | `INTER_TENTATIVAS` |
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

### Extrato

```console
$ inter-pj extrato                                          # últimos 30 dias, hoje incluído
$ inter-pj extrato --inicio 2026-01-01 --fim 2026-12-31 --dividir-periodo

$ inter-pj extrato completo --inicio 2026-08-01 --fim 2026-08-31            # 1ª página, com contraparte
$ inter-pj extrato completo --tipo-operacao D --tipo-transacao pix --pagina 1 --tamanho-pagina 100
$ inter-pj extrato completo --inicio 2026-08-01 --fim 2026-08-31 --todas-paginas --formato csv > agosto.csv

$ inter-pj extrato pdf --inicio 2026-08-01 --fim 2026-08-31                 # extrato-2026-08-01-a-2026-08-31.pdf
$ inter-pj extrato pdf --inicio 2026-08-01 --fim 2026-08-31 --saida - | lpr
```

- A API aceita **no máximo 90 dias por consulta** (contando o primeiro e o último dia). A CLI confere o período antes de chamar a API; `--dividir-periodo` consulta períodos maiores em partes consecutivas.
- `extrato completo` traz os detalhes de cada transação (pagador/recebedor do Pix, dados do boleto, do pagamento...). `--todas-paginas` percorre todas as páginas e, acima de 10.000 transações, passa para o modo *scroll* da API (um por conta, expira após 6 minutos sem uso).
- `extrato pdf` grava o arquivo com permissão `600` e nunca substitui um arquivo existente sem `--sobrescrever`.

### Pix

```console
$ inter-pj pix enviar --chave fornecedor@exemplo.com --valor 150,00 --descricao "NF 123"
Pix a enviar
  Ambiente               sandbox (dados fictícios)
  Chave Pix              fornecedor@exemplo.com (e-mail)
  Valor                  R$ 150,00 (cento e cinquenta reais)
  Quando                 agora
  Descrição              NF 123
  Chave de idempotência  9b2f6c1e-5d0a-4c1b-8f3e-2a7d4e6b8c90
Enviar o Pix? [s/N] s
Pix enviado.
Código da solicitação  c42f0787-02cb-4b31-827e-459ec9d7ece1
Data do pagamento      23/09/2026
Data da operação       23/09/2026
Chave de idempotência  9b2f6c1e-5d0a-4c1b-8f3e-2a7d4e6b8c90

$ inter-pj pix enviar --chave +5511912345678 --valor 1.500,00 --data 2026-10-01   # agendado
$ inter-pj pix enviar --chave 12.345.678/0001-95 --valor 99,90 --simular          # mostra a requisição, não envia
$ inter-pj pix enviar --chave fornecedor@exemplo.com --valor 150 --sim --json     # sem perguntar (scripts)

$ inter-pj pix enviar --copia-e-cola '00020126...6304ABCD'                         # valor e recebedor vêm do código
$ inter-pj pix enviar --valor 250,00 --ispb 00000000 --agencia 0001 --conta 123456-7 \
    --tipo-conta corrente --documento 12.345.678/0001-95 --nome "Fornecedor Exemplo"  # dados bancários
```

O destino é uma chave (`--chave`), um código copia e cola (`--copia-e-cola`) ou dados bancários (`--ispb`, `--agencia`, `--conta`, `--tipo-conta` — `corrente`, `poupanca`, `salario` ou `pagamento` —, `--documento` e `--nome`). O código copia e cola é decodificado localmente, com o CRC conferido: o resumo mostra recebedor, cidade, chave ou URL da cobrança, identificador e mensagem. Se o código fixa o valor, `--valor` é dispensável e um valor diferente é recusado; códigos dinâmicos (cobranças) aceitam outro valor, e o resumo mostra os dois. Textos vindos do código passam por um filtro de caracteres de controle, para que não alterem o que o terminal mostra.

Trilhos de segurança de todo envio:

- **Resumo e confirmação**: antes de enviar, a CLI mostra destino, valor (também por extenso), data e ambiente (produção em destaque) e pergunta `[s/N]`; o padrão é não. `--sim` confirma sem perguntar. Respostas vindas de um *pipe* não valem: sem terminal e sem `--sim`, a CLI recusa (código 2).
- **Validação local**: chave Pix (CPF/CNPJ com dígitos verificadores, e-mail, celular `+55DD9NNNNNNNN`, chave aleatória), valor maior que zero com até 2 casas, descrição de até 140 caracteres e data de agendamento. O valor aceita `150,00`, `1.500,00` e `150.00`; formas ambíguas como `1.500` são recusadas.
- **`--simular`**: valida e mostra a requisição (sem segredos), sem enviar nada.
- **Idempotência**: cada envio leva uma chave (`x-id-idempotente`), mostrada no resumo. Se a resposta se perder (tempo esgotado, erro 5xx), o Pix pode ter sido feito: confira o extrato e, para repetir sem risco de pagar duas vezes, use `--id-idempotente <chave>`.
- **Limite por operação**: com `limite_por_operacao` no perfil, valores acima dele são recusados, mesmo com `--sim`.
- **Aprovação**: conforme a configuração da conta, o Pix aguarda aprovação no Internet Banking (Aprovar > Gestão de Aprovações); a CLI avisa quando for o caso.

A integração precisa do escopo `pagamento-pix.write`.

### Formatos de saída

| Formato | Para quê |
| --- | --- |
| `texto` (padrão) | leitura: tabelas alinhadas, valores em `R$ 1.234,56`, datas `DD/MM/AAAA` |
| `json` (ou `--json`) | automação: os nomes de campo da API e valores numéricos exatos |
| `csv` | planilhas e scripts (`saldo` e `extrato`): RFC 4180, datas `AAAA-MM-DD`, ponto decimal, saídas com valor negativo |

Para o Excel em português, use `--formato csv --separador ';'`: ponto e vírgula, vírgula decimal e UTF-8 com BOM. Textos vindos de terceiros que começam com `=`, `+`, `-` ou `@` (ex.: a mensagem de um Pix) recebem um apóstrofo no CSV, para não serem executados como fórmula pela planilha.

### Retentativas

Consultas que falham por limite de requisições (`429`), instabilidade do servidor (`500`, `502`, `503`, `504`) ou falha de conexão são repetidas automaticamente, com espera crescente (1 s, 2 s, ...) e respeitando o cabeçalho `Retry-After`. O padrão é de 3 tentativas; ajuste com `--tentativas N` (ou `INTER_TENTATIVAS`) ou desative com `--sem-retentativa`. Com `-v`, cada nova tentativa aparece em `stderr`. O envio de Pix só é repetido quando certamente não foi processado (`429` ou conexão recusada), sempre com a mesma chave de idempotência.

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
| 7 | operação cancelada na confirmação (nada foi enviado) |

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
