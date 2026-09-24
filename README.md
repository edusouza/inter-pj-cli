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
| **0.3.0** | `pix enviar` (chave, copia e cola ou dados bancários) com resumo, confirmação, simulação, idempotência e limite por operação · `pix consultar` (com `--aguardar`) |
| **0.4.0** | `pagamento boleto pagar` (boletos, contas e tributos pelo código, conferido localmente) · `boleto listar`/`cancelar` · `pagamento darf pagar`/`listar` · `pagamento lote enviar` (JSON ou planilha CSV), `consultar` e `modelo` · os trilhos de segurança do Pix |
| **0.5.0** | `cobranca emitir` (pelas opções ou por arquivo JSON, com `modelo`) com resumo, confirmação e `--aguardar` · `cobranca consultar` com o QR Code do Pix no terminal ou em PNG · `cobranca pdf` · `cobranca listar`/`sumario` · `cobranca cancelar`/`editar`/`edicao` · `cobranca pagar` no sandbox |
| **0.6.0** | `pix cob` e `pix cobv` (cobranças Pix imediatas e com vencimento, com multa, juros e descontos): criar, com um txid que torna segura a repetição, revisar, consultar com o QR Code e listar; `pix cobv` também por arquivo, com `modelo` · `pix recebidos` · `pix devolucao` com os trilhos de segurança do Pix · `pix loc` · `pix lote-cobv` (JSON ou planilha CSV) · pagamentos no sandbox |
| **0.7.0** | `webhook banking`, `webhook cobranca` e `webhook pix` (Pix enviados e boletos pagos pela conta, cobranças, cobranças Pix por chave): `cadastrar`, com o antes e o depois e confirmação, `consultar` e `excluir` · `callbacks`, o histórico das tentativas de envio com o status HTTP · `reenviar`, em blocos de 50 |
| **0.8.0** | Pix Automático: `pix-automatico rec` (criar, pelas opções ou por arquivo, com `modelo`; listar; consultar com o QR Code; revisar e cancelar) · `solicitacao` (o pedido de aprovação ao banco do pagador) · `cobr` (a cobrança de cada ciclo: criar, com um txid que torna segura a repetição, listar, consultar, cancelar e pedir nova tentativa) · `locrec` · `webhook recorrencia` e `webhook cobranca-recorrente` · `sandbox`, que simula o pagador e o banco dele |

O plano completo — o caminho até a 1.0 — está em [`docs/roadmap.md`](docs/roadmap.md) e é acompanhado pelas [issues do projeto](https://github.com/edusouza/inter-pj-cli/issues).

## Instalação

**Binários prontos**: baixe o pacote do seu sistema na página de [releases](https://github.com/edusouza/inter-pj-cli/releases) (Linux x86_64 glibc/musl, macOS Apple Silicon/Intel e Windows), confira o `SHA256SUMS` e coloque o `inter-pj` no seu `PATH`.

**Com Cargo** (Rust 1.88 ou superior):

```console
$ cargo install --locked --git https://github.com/edusouza/inter-pj-cli --tag v0.8.0 inter-pj-cli
```

### Completions do shell e páginas de manual

Os pacotes das releases trazem os scripts de completion em `completions/` e, fora do Windows, as páginas de manual em `man/man1`. O próprio `inter-pj` também os gera, o que serve a quem instalou com Cargo:

```console
$ inter-pj completions bash > ~/.local/share/bash-completion/completions/inter-pj
$ inter-pj completions zsh > ~/.zfunc/_inter-pj    # com fpath=(~/.zfunc $fpath) antes do compinit
$ inter-pj completions fish > ~/.config/fish/completions/inter-pj.fish
$ inter-pj completions powershell >> $PROFILE
$ inter-pj completions elvish >> ~/.config/elvish/rc.elv
$ inter-pj manual ~/.local/share/man/man1           # uma página por comando
$ man inter-pj-pix-enviar
```

As completions sugerem os comandos e as opções em todos os níveis e, no bash, no zsh e no fish, os valores das opções (`--formato`, `--tipo-conta`...). Abra um novo shell depois de instalá-las.

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
# limite_por_operacao = "1.000,00" # opcional: valor máximo de cada Pix ou pagamento
```

Ou deixe o assistente perguntar e conferir tudo na hora:

```console
$ inter-pj config init --interativo
Assistente de configuração do inter-pj: /home/voce/.config/inter-pj/config.toml. O client_secret não é perguntado; ele fica fora do arquivo.
Nome do perfil [padrao]:
Ambiente (sandbox, com dados fictícios, ou producao) [sandbox]:
client_id da integração: <seu client_id>
Certificado (.crt): ~/inter/certificado.crt
  Integração Exemplo, válido até 05/12/2026
Chave privada (.key): ~/inter/chave.key
  certificado e chave aceitos
Conta corrente, só se a integração tiver mais de uma conta (Enter para pular):
Perfil "padrao" gravado em /home/voce/.config/inter-pj/config.toml.
```

O certificado e a chave são conferidos ao serem informados (um arquivo trocado, uma chave com senha ou um certificado vencido aparecem na hora, e a resposta é pedida de novo), e caminhos relativos são gravados como absolutos. Num arquivo que já existe, o assistente acrescenta um perfil novo, sem mexer nos outros nem nos comentários; `--forcar` recria o arquivo. As respostas também podem vir de um pipe, para automatizar a configuração.

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
$ inter-pj auth certificado                      # titular, emissor e validade do certificado

$ inter-pj --perfil producao saldo
$ inter-pj --help                                # ajuda de todos os comandos
```

### Certificado

Os certificados de produção valem um ano. `auth certificado` lê o do perfil (ou outro, com `--arquivo`, como um renovado antes de trocá-lo na configuração) e mostra o titular, o emissor, a validade e quantos dias faltam:

```console
$ inter-pj auth certificado
Certificado da integração
  Arquivo          /home/usuario/.config/inter-pj/certificado.crt
  Titular          CN=Integração Exemplo, O=Empresa Exemplo Ltda
  Emissor          CN=AC Exemplo, O=Banco Exemplo
  Número de série  1A2B3C4D
  Válido desde     05/12/2025 10:00:00
  Válido até       05/12/2026 10:00:00
  Situação         válido; faltam 72 dias, e a renovação já está aberta no Internet Banking PJ
```

Nos últimos 30 dias da validade, e depois dela, todo comando que acessa a API avisa em stderr. A renovação, no Internet Banking PJ, abre 90 dias antes do fim e mantém o `client_id` e o `client_secret`; `--json` traz a situação (`valido`, `renovavel`, `vencendo`, `vencido` ou `ainda-nao-vale`) e os dias restantes, para um monitoramento.

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

Acompanhe com: inter-pj pix consultar c42f0787-02cb-4b31-827e-459ec9d7ece1 --aguardar

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

Para acompanhar um Pix enviado (últimos 90 dias), use o código da solicitação:

```console
$ inter-pj pix consultar c42f0787-02cb-4b31-827e-459ec9d7ece1                        # status, recebedor, erros e histórico
$ inter-pj pix consultar c42f0787-02cb-4b31-827e-459ec9d7ece1 --aguardar --timeout 5m # até um status final
```

Com `--aguardar`, a CLI consulta a cada 6 segundos (dentro do limite de requisições da API) até o Pix ser pago, agendado ou terminar sem pagamento, e sai com o código 0 (pago ou agendado), 5 (terminou sem ser pago: reprovado, expirado, cancelado, falha...) ou 8 (o tempo acabou antes de um status final; padrão: 60 s). A consulta precisa do escopo `pagamento-pix.read`.

### Pagamentos

Boletos, contas de consumo e tributos com código de barras:

```console
$ inter-pj pagamento boleto pagar '07797.77705 11678.471159 90071.126347 1 15950000003010'
Pagamento a enviar
  Ambiente         sandbox (dados fictícios)
  Tipo             boleto do banco 077
  Linha digitável  07797.77705 11678.471159 90071.126347 1 15950000003010
  Valor            R$ 30,10 (trinta reais e dez centavos)
  Vencimento       10/10/2026
  Quando           agora
Confirmar o pagamento? [s/N] s
Pagamento realizado.
Código da transação  3414f226-36fb-4d87-811e-cfd99911d845

Acompanhe com: inter-pj pagamento boleto listar --codigo-transacao 3414f226-36fb-4d87-811e-cfd99911d845

$ inter-pj pagamento boleto pagar 82670000000653301602023123106000000002830894 --vencimento 2026-10-10   # conta de água
$ inter-pj pagamento boleto pagar '<linha digitável>' --valor 31,20 --data 2026-10-09 --beneficiario 12.345.678/0001-95
```

O código — linha digitável (47 dígitos nos boletos, 48 nas contas e tributos) ou código de barras (44) — tem todos os dígitos verificadores conferidos localmente. Valor e vencimento vêm do próprio código quando ele os traz; contas de consumo e tributos não trazem o vencimento, então precisam de `--vencimento` (a data impressa no documento). O resumo mostra o valor por extenso e, quando `--valor` ou `--vencimento` diferem do código, os dois lados com um aviso (juros, multa ou desconto); também avisa quando o pagamento fica para depois do vencimento. `--data` agenda o pagamento, e `--beneficiario` pede à API que confira o CPF/CNPJ de quem recebe. Os trilhos de segurança são os do Pix: confirmação `[s/N]` ou `--sim`, `--simular` e o limite por operação do perfil.

Esta API não tem chave de idempotência. Se o resultado ficar incerto (tempo esgotado, erro 5xx), o pagamento pode ter sido feito e repetir o comando pode pagar duas vezes: a CLI mostra o `pagamento boleto listar --codigo ...` que confere isso antes de uma nova tentativa. Conforme a configuração da conta, o pagamento aguarda aprovação no Internet Banking. O pagamento precisa do escopo `pagamento-boleto.write`; no sandbox, a documentação oferece os códigos `03395988500000666539201493990000372830030102` (boleto vencido) e `82670000000653301602023123106000000002830894` (conta de água).

```console
$ inter-pj pagamento boleto listar --inicio 2026-09-01 --fim 2026-09-30
Pagamentos incluídos de 01/09/2026 a 30/09/2026

Vencimento  Pagamento   Beneficiário        Status        Valor  Código da transação
10/10/2026  09/10/2026  Fornecedor Exemplo  agendado   R$ 30,10  3414f226-36fb-4d87-811e-cfd99911d845

1 pagamento

$ inter-pj pagamento boleto listar --filtrar-por vencimento --inicio 2026-12-01 --fim 2026-12-31
$ inter-pj pagamento boleto listar --codigo '07797.77705 11678.471159 90071.126347 1 92950000003010'
$ inter-pj pagamento boleto cancelar 3414f226-36fb-4d87-811e-cfd99911d845   # mostra o agendamento e pede confirmação
```

A listagem cobre até 90 dias por consulta; sem datas, mostra os pagamentos incluídos nos últimos 30 dias. `--filtrar-por` escolhe a data a que o período se refere (`inclusao`, `pagamento` ou `vencimento`), e o código (linha digitável ou código de barras) tem os dígitos verificadores conferidos antes da consulta. O cancelamento vale para agendamentos: a CLI mostra o pagamento (beneficiário, valor, data e status) e pede confirmação `[s/N]`, ou `--sim` em scripts. A listagem precisa do escopo `pagamento-boleto.read`; o cancelamento, também de `pagamento-boleto.write`.

### DARF

DARFs sem código de barras (tributos federais) são pagos pelas opções ou por um arquivo JSON com os campos da API:

```console
$ inter-pj pagamento darf pagar --codigo-receita 0220 --contribuinte 12.345.678/0001-95 \
    --nome-empresa "Empresa Exemplo" --periodo-apuracao 2026-09-30 --vencimento 2026-10-30 \
    --referencia 13609400849201739 --descricao "IRPJ de setembro" --valor-principal 47,14
$ inter-pj pagamento darf pagar --arquivo darf.json
$ inter-pj pagamento darf listar --inicio 2026-10-01 --fim 2026-10-31 --codigo-receita 0220
```

```json
{
  "cnpjCpf": "12.345.678/0001-95",
  "codigoReceita": "0220",
  "nomeEmpresa": "Empresa Exemplo",
  "periodoApuracao": "2026-09-30",
  "dataVencimento": "2026-10-30",
  "referencia": "13609400849201739",
  "descricao": "IRPJ de setembro",
  "valorPrincipal": 47.14,
  "valorMulta": 0,
  "valorJuros": "10,11"
}
```

Antes de enviar, a CLI confere o CPF/CNPJ (dígitos verificadores), o código da receita (4 dígitos), a referência (só dígitos, até 30), os textos e os valores; no arquivo, campos desconhecidos são recusados, para que um erro de digitação (`valorMuta`) não apague a multa, e as mensagens apontam o campo. Valores aceitam número (`47.14`) ou texto (`"47,14"`). O resumo mostra principal, multa, juros e o total por extenso, e avisa quando um DARF vencido não tem multa nem juros: esses acréscimos não são calculados pela API. Os trilhos são os mesmos dos outros pagamentos (confirmação, `--sim`, `--simular`, limite por operação e, sem chave de idempotência, o comando para conferir um resultado incerto). Com `--arquivo -`, o DARF vem da entrada padrão e a confirmação exige `--sim`.

A listagem filtra pela data de pagamento; sem datas, mostra os DARFs incluídos nos últimos 30 dias (o padrão da API). O pagamento precisa do escopo `pagamento-darf.write`, e a listagem, de `pagamento-boleto.read`.

### Lotes

Boletos, contas, tributos e DARFs podem ir juntos, de 2 a 150 por lote, a partir de um arquivo JSON ou de uma planilha CSV:

```console
$ inter-pj pagamento lote modelo csv > lote.csv   # planilha de exemplo (ou: modelo json > lote.json)
$ inter-pj pagamento lote enviar --arquivo lote.csv --identificador "Pagamentos de outubro"
Lote a enviar
  Ambiente       sandbox (dados fictícios)
  Arquivo        lote.csv
  Identificador  Pagamentos de outubro
  Pagamentos     2 boletos e contas (R$ 731,86) e 1 DARF (R$ 47,14)
  Total          R$ 779,00 (setecentos e setenta e nove reais)

  Onde     Tipo           Documento                                                Vencimento  Quando      Valor
  linha 2  boleto         03399.20142 93990.000379 28300.301026 5 98850000066653   30/10/2024  agora   R$ 666,53
  linha 3  conta/tributo  82670000000-1 65330160202-1 31231060000-1 00002830894-8  10/10/2026  agora    R$ 65,33
  linha 4  DARF           receita 0220 · Empresa Exemplo (12.345.678/0001-95)      30/10/2026  agora    R$ 47,14
aviso: linha 2: o pagamento fica para depois do vencimento (30/10/2024): pode haver juros e multa, ou recusa
Enviar o lote de 3 pagamentos (R$ 779,00)? [s/N] s
Lote recebido: 3 pagamentos, em processamento.
Identificador do lote  0123456789abcdef01234567
Meu identificador      Pagamentos de outubro

Acompanhe com: inter-pj pagamento lote consultar 0123456789abcdef01234567 --aguardar

$ inter-pj pagamento lote consultar 0123456789abcdef01234567 --aguardar
Lote 0123456789abcdef01234567
  Status             processado com erro
  Meu identificador  Pagamentos de outubro
  Criado em          01/10/2026 09:15:00
  Pagamentos         3

Tipo    Status                 Valor  Código                                Detalhe
boleto  pago               R$ 666,53  3414f226-36fb-4d87-811e-cfd99911d845
boleto  pago                R$ 65,33  8c1d2e3f-4a5b-4c6d-8e7f-9a0b1c2d3e4f
DARF    erro no pagamento   R$ 47,14                                        Saldo insuficiente
erro: o lote foi processado com erro: 1 de 3 pagamentos não foi feito
```

O arquivo usa os nomes de campo da API. Em JSON, é um objeto com `pagamentos` e, opcionalmente, `meuIdentificador` (`--identificador` o substitui), ou só a lista de pagamentos; em CSV, uma coluna por campo, com cabeçalho, e células vazias para os campos ausentes. Cada pagamento tem `tipoPagamento` e os campos do seu tipo:

- `BOLETO` (boletos, contas e tributos com código de barras): `codBarraLinhaDigitavel` e, quando o código não os traz ou para pagar outro valor, `valorPagar` e `dataVencimento`; opcionalmente `dataPagamento` (agendamento) e `cpfCnpjBeneficiario`, como em `pagamento boleto pagar`;
- `DARF`: os campos do arquivo de `pagamento darf pagar`.

Antes de enviar, a CLI confere o lote inteiro com as validações de cada tipo (dígitos verificadores, datas, valores, CPF/CNPJ) e recusa campos desconhecidos ou de outro tipo; havendo problemas, lista todos, com a linha (CSV) ou a posição (JSON) e o campo, e não envia nada. O resumo mostra o total por tipo e por extenso, cada pagamento, e avisa sobre vencimentos passados, valores diferentes dos do código e pagamentos repetidos no arquivo. Os trilhos de segurança são os dos outros pagamentos: confirmação `[s/N]` ou `--sim`, `--simular` e o limite por operação do perfil, que vale para cada pagamento do lote (um lote não é recusado pelo total, e sim pelo pagamento que passa do limite).

O CSV aceita `,` ou `;` como separador (detectado pelo cabeçalho), UTF-8 com ou sem BOM e valores como `65,33`, como o Excel em português salva. Ao abrir um CSV, porém, o Excel converte o que parece número ou data: números longos viram notação científica e perdem dígitos (`1,36094E+16`), códigos perdem os zeros à esquerda (`0220` vira `220`) e datas mudam de formato (`30/10/2026`). Para editar no Excel, importe o arquivo (Dados > De Texto/CSV) sem detectar os tipos de dados, ou formate as colunas como texto antes de digitar; linhas digitáveis e CPF/CNPJ com pontuação, como no modelo, já ficam como texto. Se algo chegar estragado, a conferência recusa o arquivo e diz o que aconteceu, sem enviar nada.

O lote é processado depois do envio: `pagamento lote consultar` mostra o status do lote e de cada pagamento, e com `--aguardar` consulta a cada 6 segundos até o fim do processamento, saindo com o código 0 (processado sem erro), 5 (algum pagamento não foi feito) ou 8 (o tempo acabou; padrão: 5 min). Como nos outros pagamentos, não há chave de idempotência: se o resultado do envio ficar incerto, confira `pagamento boleto listar` e `pagamento darf listar` antes de enviar de novo. O envio precisa do escopo `pagamento-lote.write`, e a consulta, de `pagamento-lote.read`.

### Cobranças

Cobranças são boletos com Pix para os clientes da empresa. A emissão parte das opções, nos casos simples, ou de um arquivo JSON com os campos da API; antes de enviar, a CLI confere tudo e mostra o resumo:

```console
$ inter-pj cobranca emitir --seu-numero NF-123 --valor 150,00 --vencimento 2026-10-20 \
    --pagador-documento 12.345.678/0001-95 --pagador-nome "Cliente Exemplo Ltda" \
    --pagador-endereco "Avenida Brasil" --pagador-numero 1200 --pagador-cidade "Belo Horizonte" \
    --pagador-uf MG --pagador-cep 30110-000 --pagador-email financeiro@exemplo.com.br \
    --desconto 2% --desconto-dias 5 --multa 2% --juros 1% --dias-agenda 30
Cobrança a emitir
  Ambiente      sandbox (dados fictícios)
  Seu número    NF-123
  Valor         R$ 150,00 (cento e cinquenta reais)
  Vencimento    20/10/2026
  Pagador       Cliente Exemplo Ltda (12.345.678/0001-95)
  Endereço      Avenida Brasil, 1200 - Belo Horizonte/MG - CEP 30110-000
  Contato       financeiro@exemplo.com.br
  Desconto      2% para pagamentos até 15/10/2026
  Multa         2%
  Juros         1% ao mês
  Cancelamento  19/11/2026, 30 dias após o vencimento, se não for paga
  Recebimento   boleto e Pix (se a conta tiver chave Pix)
Emitir a cobrança? [s/N] s
Cobrança solicitada: a emissão termina em instantes.
Código  0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d

Acompanhe com: inter-pj cobranca consultar 0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d

$ inter-pj cobranca modelo > cobranca.json    # exemplo com todos os campos, vencendo em 30 dias
$ inter-pj cobranca emitir --arquivo cobranca.json --aguardar --qrcode
```

São obrigatórios o seu número (até 15 caracteres, ex.: o número da nota), o valor (de R$ 2,50 a R$ 99.999.999,99), o vencimento (hoje ou depois) e, do pagador, CPF ou CNPJ, nome, endereço, cidade, UF e CEP; número, complemento, bairro, e-mail e telefone são opcionais. `--desconto`, `--multa` e `--juros` aceitam um percentual (`2%`) ou um valor (`4,00`): o desconto vale para pagamentos até `--desconto-dias` antes do vencimento (padrão: até o vencimento), e os juros são ao mês, em percentual, ou por dia, em valor. `--mensagem` pode ser repetida, até 5 linhas de 78 caracteres, e `--receber-com boleto` ou `pix` restringe as formas de pagamento (padrão: boleto e, se a conta tiver chave, Pix).

Atenção a `--dias-agenda` (`numDiasAgenda`): é por quantos dias depois do vencimento a cobrança não paga continua valendo. O padrão da API, 0, cancela a cobrança no vencimento: pagamentos atrasados não são aceitos, e multa e juros nunca chegam a valer. O resumo mostra a data do cancelamento e avisa quando há multa ou juros sem prazo para valerem, quando o prazo do desconto já passou e quando a cobrança vence hoje, o que só é aceito até as 19h59 (horário de Brasília).

O arquivo (`--arquivo`, ou `-` para a entrada padrão) tem os nomes de campo da API e aceita também o beneficiário final e a nota fiscal, cuja chave de acesso é conferida (dígito verificador, número e série). Campos desconhecidos são recusados e as mensagens apontam o campo (`cobranca.json, campo "pagador.cep": ...`); valores aceitam número (`150.00`) ou texto (`"150,00"`), CEP e CPF/CNPJ aceitam pontuação, e `tipoPessoa` pode ficar de fora, pois vem do documento.

Os trilhos são os dos pagamentos: confirmação `[s/N]` (sem terminal, ou com `--arquivo -`, exige `--sim`) e `--simular`, que mostra a requisição sem enviar nada; o limite por operação não se aplica, porque uma cobrança não tira dinheiro da conta. A emissão é assíncrona: a API responde com o código da solicitação, e a cobrança fica em processamento até o boleto e o Pix serem gerados. Com `--aguardar`, a CLI consulta a cada 6 segundos (até `--timeout`; padrão: 60s) e mostra a cobrança emitida, com o QR Code se `--qrcode` ou `--qrcode-png` forem pedidos, e sai com o código 5 se a emissão falhou ou 8 se o tempo acabou.

Não há chave de idempotência, mas, por 30 minutos, a API recusa outra cobrança com o mesmo seu número, valor, vencimento e pagador. Se o resultado da emissão ficar incerto, o erro traz o comando que procura a cobrança (`cobranca listar --filtrar-por emissao --seu-numero NF-123`) para conferir antes de tentar de novo. A emissão precisa do escopo `boleto-cobranca.write` e, com `--aguardar`, também de `boleto-cobranca.read`.

A consulta mostra a situação, os valores e os encargos, o boleto e o Pix:

```console
$ inter-pj cobranca consultar 0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d
Cobrança NF-123
  Situação    a receber
  Valor       R$ 150,00
  Vencimento  20/10/2026
  Pagador     Cliente Exemplo Ltda (12.345.678/0001-95)
  Emitida em  23/09/2026
  Tipo        simples
  Desconto    2% até 5 dias antes do vencimento
  Multa       2%
  Juros       1% ao mês
  Código      0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d

Boleto
  Nosso número      12345678
  Linha digitável   07790.00116 12345.678002 12345.678903 1 16050000015000
  Código de barras  07791160500000150000001112345678001234567890

Pix
  Copia e cola  00020126580014br.gov.bcb.pix0136123e4567-e12b-...63041D3D
  txid          COBRANCAEXEMPLO00000000001

$ inter-pj cobranca consultar 0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d --qrcode            # desenha o QR Code do Pix
$ inter-pj cobranca consultar 0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d --qrcode-png pix.png # grava o QR Code em PNG
$ inter-pj cobranca pdf 0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d                           # cobranca-<codigo>.pdf
```

As listagens mostram as cobranças de um período, por vencimento (o padrão), emissão ou pagamento; sem datas, as com vencimento nos últimos 30 dias:

```console
$ inter-pj cobranca listar --inicio 2026-09-01 --fim 2026-10-31
Cobranças com vencimento de 01/09/2026 a 31/10/2026

Vencimento  Seu número  Pagador               Situação       Valor  Código
20/10/2026  NF-123      Cliente Exemplo Ltda  a receber  R$ 150,00  0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d
10/09/2026  NF-124      Outro Cliente         recebida   R$ 300,00  5a6b7c8d-1e2f-4a3b-8c9d-0e1f2a3b4c5d
05/09/2026  NF-125      Mercado Exemplo       atrasada    R$ 89,90  9c8d7e6f-5a4b-4c3d-8e2f-1a0b9c8d7e6f

3 cobranças · R$ 539,90 · recebido R$ 300,00

$ inter-pj cobranca sumario --inicio 2026-09-01 --fim 2026-10-31
Cobranças com vencimento de 01/09/2026 a 31/10/2026

Situação   Quantidade        Valor
a receber          11  R$ 1.650,00
atrasada            2    R$ 300,00
recebida           27  R$ 4.200,50
Total              40  R$ 6.150,50

$ inter-pj cobranca listar --situacao atrasada --documento 12.345.678/0001-95
$ inter-pj cobranca listar --filtrar-por pagamento --formato csv --separador ';' > recebidas.csv
```

Os filtros são `--situacao` (`a-receber`, `recebida`, `atrasada`, `cancelada`, `expirada`, `marcada-recebida`, `em-processamento`, `falha-emissao` ou `protesto`), `--pagador` (nome), `--documento` (CPF/CNPJ, conferido), `--seu-numero` e `--tipo` (`simples`, `parcelada` ou `recorrente`). A listagem lê todas as páginas, de 1.000 cobranças cada; `--pagina N` (a primeira é 0) com `--itens-por-pagina` traz uma só, e `--ordenar-por` com `--decrescente` escolhe a ordem. Em CSV, as colunas têm os nomes da API (os aninhados com ponto: `pagador.nome`, `boleto.linhaDigitavel`, `pix.pixCopiaECola`).

`--qrcode` desenha o QR Code do Pix no terminal, para o cliente ler com o celular. Em um terminal, ele sai preto no branco, qualquer que seja o tema. Com `NO_COLOR` ou com a saída redirecionada, sem cores, os módulos claros é que são desenhados, como no `qrencode -t UTF8`, e o código fica certo em terminais de fundo escuro; para imprimir ou enviar, prefira `--qrcode-png`. Antes de desenhar, a CLI confere o copia e cola (CRC16). O PNG e o PDF são gravados com permissão `600` e não sobrescrevem um arquivo existente sem `--sobrescrever`; `-` os envia para a saída padrão. As consultas precisam do escopo `boleto-cobranca.read`.

Uma cobrança ainda não paga pode ser cancelada ou ter o valor e o vencimento alterados. Nos dois casos, a CLI primeiro consulta a cobrança e mostra o que vai mudar, e cobranças pagas, canceladas ou expiradas são recusadas sem nenhuma alteração:

```console
$ inter-pj cobranca editar 0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d --valor 200,00 --vencimento 2026-11-10
Cobrança a alterar
  Ambiente    sandbox (dados fictícios)
  Seu número  NF-123
  Situação    a receber
  Valor       R$ 150,00 → R$ 200,00
  Vencimento  20/10/2026 → 10/11/2026
  Pagador     Cliente Exemplo Ltda (12.345.678/0001-95)
  Código      0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d
aviso: a consulta pode levar até 30 minutos para mostrar o novo valor ou vencimento
Alterar a cobrança? [s/N] s
Alteração em processamento.
Código da alteração  5a6b7c8d-1e2f-4a3b-8c9d-0e1f2a3b4c5d

Acompanhe com: inter-pj cobranca edicao 5a6b7c8d-1e2f-4a3b-8c9d-0e1f2a3b4c5d --aguardar

$ inter-pj cobranca cancelar 0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d --motivo "Pedido cancelado"
```

A API altera só o valor (de R$ 2,50 a R$ 99.999.999,99) e o vencimento (hoje ou depois). A alteração é processada depois do pedido: `cobranca edicao` mostra em que pé ela está e, com `--aguardar` (aceito também por `editar`), consulta a cada 6 segundos até o fim, saindo com o código 0 (feita), 5 (não foi feita) ou 8 (o tempo acabou; padrão: 60s). Mesmo feita, a alteração pode levar até 30 minutos para aparecer em `cobranca consultar`. O motivo do cancelamento tem até 50 caracteres. Os dois comandos pedem confirmação (sem terminal, exigem `--sim`, e nesse caso nem a consulta é feita) e precisam do escopo `boleto-cobranca.write`, além de `boleto-cobranca.read` para a consulta; a API aceita até 10 alterações por minuto.

No sandbox, `cobranca pagar <codigo> --com boleto` (ou `pix`) paga uma cobrança, para testar o fluxo inteiro: emitir, pagar, consultar e, com um webhook cadastrado, receber a notificação. Em produção, quem paga é o cliente, e o comando é recusado antes de qualquer requisição. O pagamento precisa do escopo `boleto-cobranca.write`.

### Cobranças Pix

A API Pix cria cobranças com QR Code dinâmico, que o cliente paga pelo app de qualquer banco. A cobrança imediata (`pix cob`) é para pagar na hora, até expirar:

```console
$ inter-pj pix cob criar --chave pix@empresa.example --valor 149,90 --expiracao 2h \
    --devedor-documento 12.345.678/0001-95 --devedor-nome "Cliente Exemplo Ltda" --solicitacao "Pedido 123"
Cobrança Pix a criar
  Ambiente     sandbox (dados fictícios)
  Valor        R$ 149,90 (cento e quarenta e nove reais e noventa centavos)
  Chave        pix@empresa.example (e-mail)
  Expira       2 horas após a criação
  Devedor      Cliente Exemplo Ltda (12.345.678/0001-95)
  Solicitação  Pedido 123
  txid         7978c0c97ea847e78e8849634473c1f1
Criar a cobrança? [s/N] s
aviso: ambiente sandbox — os dados retornados são fictícios
Cobrança Pix criada.

Cobrança Pix 7978c0c97ea847e78e8849634473c1f1
  Status       ativa
  Valor        R$ 149,90
  Criada em    23/09/2026 10:05:12
  Expira em    23/09/2026 12:05:12
  Devedor      Cliente Exemplo Ltda (12.345.678/0001-95)
  Chave        pix@empresa.example
  Solicitação  Pedido 123
  Revisão      0
  Location     pix.example.com/qr/v2/9d36b84fc70b478fb95c12729b90ca25

Copia e cola  00020101021226760014br.gov.bcb.pix2554pix.example.com/qr/v2/9d36b84fc70b...63040398

Acompanhe com: inter-pj pix cob consultar 7978c0c97ea847e78e8849634473c1f1

$ inter-pj pix cob criar --chave pix@empresa.example --valor 50 --qrcode               # desenha o QR Code para o cliente
$ inter-pj pix cob criar --chave pix@empresa.example --valor 50 --qrcode-png pix.png   # grava o QR Code em PNG
$ inter-pj pix cob criar --chave pix@empresa.example --valor 50 --simular              # mostra a requisição, não cria
```

A chave (`--chave`) é uma chave Pix da conta que recebe, e o valor, maior que zero, com até 2 casas; `--valor-alteravel` deixa o pagador mudar o valor. A cobrança expira no tempo de `--expiracao`, contado da criação (`3600s`, `30m`, `2h`, `7d`; o padrão da API é 1 dia). Opcionais: o devedor (`--devedor-documento`, com CPF ou CNPJ conferido, e `--devedor-nome`), o texto mostrado ao pagador (`--solicitacao`, até 140 caracteres) e informações adicionais (`--info NOME=VALOR`, que pode ser repetida até 50 vezes). Tudo é conferido antes de qualquer requisição.

Cada cobrança tem um txid, de 26 a 35 letras e dígitos. A CLI gera um e o mostra no resumo, ou usa o de `--txid`; com o mesmo txid, a API não cria outra cobrança. Por isso, se o resultado ficar incerto (tempo esgotado, erro 5xx), o erro traz o comando que consulta a cobrança e o que repete a criação com o mesmo txid, sem risco de duplicá-la. Os trilhos são os das cobranças: resumo, confirmação `[s/N]` (sem terminal, exige `--sim`), `--simular` e o destaque de produção; o limite por operação não se aplica, porque a cobrança não tira dinheiro da conta. Criar e alterar precisam do escopo `cob.write`; consultar e listar, de `cob.read`.

Enquanto não é paga, a cobrança pode ser alterada ou removida. A CLI a consulta antes e mostra o antes e o depois; cobranças pagas ou removidas são recusadas sem nenhuma alteração:

```console
$ inter-pj pix cob revisar 7978c0c97ea847e78e8849634473c1f1 --valor 159,90 --solicitacao "Pedido 123, com frete"
Cobrança Pix 7978c0c97ea847e78e8849634473c1f1 a alterar
  Ambiente     sandbox (dados fictícios)
  Valor        R$ 149,90 → R$ 159,90
  Expira       2 horas após a criação
  Devedor      Cliente Exemplo Ltda (12.345.678/0001-95)
  Solicitação  → Pedido 123, com frete
  Status       ativa
Alterar a cobrança? [s/N] s
Cobrança Pix alterada (revisão 1).
...

$ inter-pj pix cob revisar 7978c0c97ea847e78e8849634473c1f1 --remover     # deixa de poder ser paga
```

`revisar` altera o valor, `--valor-alteravel sim` ou `nao`, a expiração, o devedor, a chave, a solicitação e as informações adicionais, que substituem as atuais. Como na criação, pede confirmação; sem terminal nem `--sim`, nem a consulta é feita. Revisar precisa também do escopo `cob.read`, para a consulta.

A consulta mostra a cobrança e os Pix que a pagaram, com os horários no fuso local:

```console
$ inter-pj pix cob consultar a1b2c3d4e5f60718293a4b5c6d7e8f90
Cobrança Pix a1b2c3d4e5f60718293a4b5c6d7e8f90
  Status       concluída (paga)
  Valor        R$ 300,00
  Criada em    18/09/2026 09:40:00
  Expira em    18/09/2026 11:40:00
  Devedor      Outro Cliente (12.345.678/0001-95)
  Chave        pix@empresa.example
  Solicitação  Pedido 123
  Revisão      0
  Location     pix.example.com/qr/v2/9d36b84fc70b478fb95c12729b90ca25

Pix recebidos
Horário                  Valor  Devolvido  endToEndId
18/09/2026 09:41:07  R$ 300,00             E00416968202609181241abcdEFGH123

Copia e cola  00020101021226760014br.gov.bcb.pix2554pix.example.com/qr/v2/9d36b84fc70b...63040136

$ inter-pj pix cob consultar 7978c0c97ea847e78e8849634473c1f1 --qrcode
```

`--qrcode` e `--qrcode-png` funcionam como nas cobranças com boleto, mas o QR Code só é gerado enquanto a cobrança está ativa: para uma cobrança paga ou removida, `--qrcode` mostra um aviso no lugar dele, e `--qrcode-png` termina com erro, sem gravar a imagem.

A listagem mostra as cobranças criadas em um período, por padrão os últimos 30 dias até agora:

```console
$ inter-pj pix cob listar --inicio 2026-09-01 --fim 2026-09-30
Cobranças Pix imediatas criadas de 01/09/2026 00:00 a 30/09/2026 23:59

Criada em            Status                       Valor  Devedor               txid
23/09/2026 10:05:12  ativa                    R$ 149,90  Cliente Exemplo Ltda  7978c0c97ea847e78e8849634473c1f1
18/09/2026 09:40:00  concluída (paga)         R$ 300,00  Outro Cliente         a1b2c3d4e5f60718293a4b5c6d7e8f90
02/09/2026 14:20:00  removida pelo recebedor   R$ 89,90  Mercado Exemplo       0f1e2d3c4b5a69788796a5b4c3d2e1f0

3 cobranças · R$ 539,80 · pagas R$ 300,00

$ inter-pj pix cob listar --status ativa --documento 12.345.678/0001-95
$ inter-pj pix cob listar --inicio 2026-09-23T08:00:00-03:00 --fim 2026-09-23T12:00:00-03:00 --formato csv > manha.csv
```

`--inicio` e `--fim` aceitam uma data (o dia inteiro, no fuso local) ou data e hora com fuso. Os filtros são `--status` (`ativa`, `concluida`, `removida-pelo-usuario` ou `removida-pelo-psp`), `--documento` (CPF/CNPJ do devedor) e `--com-location` ou `--sem-location`. A listagem lê todas as páginas, de 1.000 cobranças cada; `--pagina N` (a primeira é 0) com `--itens-por-pagina` traz uma só. Em CSV, as colunas têm os nomes da API (`valor.original`, `devedor.nome`, `pixCopiaECola`), com os códigos e os horários como a API os envia.

A cobrança com vencimento (`pix cobv`) é o boleto do Pix: vale até a data de vencimento, com desconto por pagar antes e multa e juros por pagar depois, e o devedor é obrigatório:

```console
$ inter-pj pix cobv criar --chave pix@empresa.example --valor 150,00 --vencimento 2026-10-20 \
    --devedor-documento 12.345.678/0001-95 --devedor-nome "Cliente Exemplo Ltda" \
    --devedor-endereco "Avenida Brasil, 1200" --devedor-cidade "Belo Horizonte" --devedor-uf MG \
    --devedor-cep 30110-000 --multa 2% --juros 1% --desconto 10,00@2026-10-15 --solicitacao "Referente à NF 123"
Cobrança Pix com vencimento a criar
  Ambiente     sandbox (dados fictícios)
  Valor        R$ 150,00 (cento e cinquenta reais)
  Vencimento   20/10/2026
  Validade     até 19/11/2026, 30 dias após o vencimento (padrão da API)
  Chave        pix@empresa.example (e-mail)
  Devedor      Cliente Exemplo Ltda (12.345.678/0001-95)
  Endereço     Avenida Brasil, 1200 - Belo Horizonte/MG - CEP 30110-000
  Multa        2%
  Juros        1% ao mês (dias corridos)
  Desconto     R$ 10,00 até 15/10/2026
  Solicitação  Referente à NF 123
  txid         cobvexemplo0000000000000000001
Criar a cobrança? [s/N] s
aviso: ambiente sandbox — os dados retornados são fictícios
Cobrança Pix com vencimento criada.
...

$ inter-pj pix cobv modelo > cobv.json          # exemplo com todos os campos, vencendo em 30 dias
$ inter-pj pix cobv criar --arquivo cobv.json --qrcode-png pix.png
```

A validade (`--validade-apos-vencimento`, em dias corridos; o padrão da API é 30) é por quanto tempo depois do vencimento a cobrança ainda pode ser paga, com multa e juros; com 0, ela não aceita pagamento atrasado, e a CLI avisa quando há multa ou juros que assim nunca valeriam. Os encargos aceitam um percentual (`2%`) ou um valor (`4,00`):

- `--multa`: por pagar depois do vencimento;
- `--juros`: um percentual ao mês (ou ao dia ou ao ano, com `--juros-periodo`) ou um valor por dia de atraso;
- `--abatimento`: vale qualquer que seja o dia do pagamento;
- `--desconto`: vale até o vencimento ou até a data depois do `@` (`2%@2026-10-15`), e pode ser repetido para até 3 datas, todas com percentuais ou todas com valores; `--desconto-por-dia` dá um desconto para cada dia pago antes do vencimento;
- `--dias-uteis`: os juros e o desconto por dia contam só os dias úteis.

O vencimento é hoje ou depois, o desconto vale até ele e os valores fixos de desconto e abatimento são menores que o da cobrança; tudo é conferido antes de qualquer requisição. O devedor pode ter e-mail e endereço (`--devedor-email`, `--devedor-endereco`, `--devedor-cidade`, `--devedor-uf` e `--devedor-cep`). O arquivo (`--arquivo`, ou `-` para a entrada padrão) tem os nomes de campo da API (`calendario.dataDeVencimento`, `valor.multa.modalidade`...), com as modalidades como números; campos desconhecidos são recusados, e as mensagens apontam o campo (`cobv.json, campo "valor.desconto.descontoDataFixa[0].data": ...`). O txid, a confirmação, `--simular`, o QR Code e o resultado incerto funcionam como em `pix cob`; os escopos são `cobv.write` e `cobv.read`.

`revisar` mostra o antes e o depois e muda só o que for informado: um novo vencimento mantém a validade atual, e um desconto sem data vale até o vencimento. O que depende da cobrança atual é conferido depois da consulta, sem nenhuma alteração se falhar: um novo vencimento antes do fim de um desconto atual, por exemplo, pede também o novo `--desconto`. O devedor informado substitui o atual, com o e-mail e o endereço.

```console
$ inter-pj pix cobv revisar cobvexemplo0000000000000000001 --vencimento 2026-10-30 --multa 4,00
Cobrança Pix com vencimento cobvexemplo0000000000000000001 a alterar
  Ambiente    sandbox (dados fictícios)
  Valor       R$ 150,00
  Vencimento  20/10/2026 → 30/10/2026
  Validade    até 19/11/2026, 30 dias após o vencimento → até 29/11/2026, 30 dias após o vencimento
  Devedor     Cliente Exemplo Ltda (12.345.678/0001-95)
  Multa       2% → R$ 4,00
  Juros       1% ao mês (dias corridos)
  Desconto    R$ 10,00 até 15/10/2026
  Status      ativa
Alterar a cobrança? [s/N] s
Cobrança Pix com vencimento alterada (revisão 1).
...

$ inter-pj pix cobv consultar cobvexemplo0000000000000000001 --qrcode
$ inter-pj pix cobv listar --inicio 2026-09-01 --fim 2026-09-30
Cobranças Pix com vencimento criadas de 01/09/2026 00:00 a 30/09/2026 23:59

Vencimento  Status                Valor  Devedor               txid
20/10/2026  ativa             R$ 150,00  Cliente Exemplo Ltda  cobvexemplo0000000000000000001
05/10/2026  concluída (paga)  R$ 300,00  Outro Cliente         cobvexemplo0000000000000000002

2 cobranças · R$ 450,00 · pagas R$ 300,00
```

A listagem tem os filtros de `pix cob listar` e também `--lote ID`; em CSV, os encargos aparecem com a modalidade e o valor (`valor.multa.modalidade`, `valor.multa.valorPerc`), e os descontos por data, só no JSON.

Muitas cobranças com vencimento podem ser criadas ou alteradas de uma vez, em um lote, a partir de um arquivo JSON (nos campos da API) ou de uma planilha CSV (uma cobrança por linha; as colunas têm os caminhos dos campos da API, como `valor.multa.valorPerc`). Antes de enviar, a CLI confere todas as cobranças e, se alguma tiver problema, recusa o arquivo inteiro, apontando a linha e o campo de cada uma:

```console
$ inter-pj pix lote-cobv modelo csv > lote.csv     # ou "modelo" para JSON; dados fictícios, vencendo em 30 dias
$ inter-pj pix lote-cobv criar 42 --arquivo lote.csv --descricao "Mensalidades de outubro"
Lote de cobranças com vencimento a criar
  Ambiente     sandbox (dados fictícios)
  Lote         42
  Descrição    Mensalidades de outubro
  Cobranças    2
  Valor total  R$ 239,90
  Vencimentos  23/10/2026

txid                          Vencimento      Valor  Devedor
mensalidade202610cliente0001  23/10/2026  R$ 150,00  Cliente Exemplo Ltda
mensalidade202610cliente0002  23/10/2026   R$ 89,90  Fulano de Tal
Criar o lote de 2 cobranças? [s/N] s
Lote 42 recebido: as 2 cobranças são criadas em instantes.

Acompanhe com: inter-pj pix lote-cobv consultar 42 --aguardar

$ inter-pj pix lote-cobv criar 43 --arquivo ruim.csv --descricao "Teste" --sim
erro: ruim.csv: 3 cobranças com problema; nada foi enviado:
  linha 2, campo "calendario.dataDeVencimento": obrigatório
  linha 3, campo "calendario.dataDeVencimento": obrigatório
  linha 4, campo "txid": txid inválido: use de 26 a 35 letras e dígitos, sem acentos, espaços, hífens ou símbolos
```

O id do lote é um número escolhido por você, e cada cobrança tem o seu txid, que não pode se repetir no arquivo; com o mesmo txid, a API não cria outra cobrança, e por isso repetir um lote de resultado incerto não duplica nada. Cada cobrança é conferida como em `pix cobv criar`, e vencimentos que já passaram são recusados. A descrição vem do arquivo JSON ou de `--descricao` (obrigatória com CSV). No CSV, o Excel costuma estragar números longos (notação científica) e tirar zeros à esquerda de CPF, CNPJ e CEP: a mensagem diz quando isso aconteceu. As informações adicionais só existem no JSON.

`revisar` envia as mudanças de cobranças do lote, no mesmo formato: só muda o que estiver no arquivo (uma célula vazia não muda nada), e `status` com `REMOVIDA_PELO_USUARIO_RECEBEDOR` remove a cobrança. Um calendário novo precisa do vencimento, porque a CLI não consulta cada cobrança. Os dois pedem confirmação (sem terminal, ou com `--arquivo -`, exigem `--sim`) e aceitam `--simular`.

O lote é processado depois do pedido, e cada cobrança é criada ou negada:

```console
$ inter-pj pix lote-cobv consultar 42
Lote 42: Mensalidades de outubro
  Criado em  24/09/2026 10:10:00
  Cobranças  2 · 1 criada · 1 negada

txid                          Situação  Criada em
mensalidade202610cliente0001  criada    24/09/2026 10:10:03
mensalidade202610cliente0002  negada

Problemas
  mensalidade202610cliente0002  Cobrança inválida. cobv.devedor.nome: O campo cobv.devedor.nome não respeita o schema.

$ inter-pj pix lote-cobv consultar 42 --aguardar      # até nenhuma estar em processamento
$ inter-pj pix lote-cobv sumario 42                   # totais do processamento
$ inter-pj pix lote-cobv situacao 42 negada           # em-processamento, criada ou negada
$ inter-pj pix lote-cobv listar --inicio 2026-09-01   # lotes do período, em texto, JSON ou CSV
```

Com `--aguardar`, a consulta sai com o código 0 se todas as cobranças foram criadas, 5 se alguma foi negada e 8 se o tempo acabou (padrão: 60s). Os escopos são `lotecobv.write` e `lotecobv.read`.

O QR Code de uma cobrança leva a uma location, o endereço onde o banco do pagador busca os dados dela. A API cria uma para cada cobrança, mas uma location pode ser criada antes (para imprimir o QR Code, por exemplo) e usada depois com `--loc` em `pix cob criar`, `pix cobv criar` ou `revisar`:

```console
$ inter-pj pix loc criar --tipo cobv
Location criada.

Location 790
  Tipo       cobrança com vencimento
  Criada em  24/09/2026 10:10:00
  Location   pix.example.com/qr/v2/cobv/5b7e4c1a5d3f4a2b9c8d7e6f5a4b3c2d
  Cobrança   nenhuma

Use com: inter-pj pix cobv criar ... --loc 790

$ inter-pj pix loc listar --inicio 2026-09-01 --fim 2026-09-30
Locations criadas de 01/09/2026 00:00 a 30/09/2026 23:59

Criada em             id  Tipo  txid                              Location
24/09/2026 10:10:00  789  cob   7978c0c97ea847e78e8849634473c1f1  pix.example.com/qr/v2/9d36b84fc70b478fb95c12729b90ca25
24/09/2026 10:10:00  790  cobv  cobvexemplo0000000000000000001    pix.example.com/qr/v2/cobv/5b7e4c1a5d3f4a2b9c8d7e6f5a4b3c2d
24/09/2026 10:10:00  791  cobv                                    pix.example.com/qr/v2/cobv/5b7e4c1a5d3f4a2b9c8d7e6f5a4b3c2d

3 locations · 2 com cobrança

$ inter-pj pix loc consultar 790
$ inter-pj pix loc desvincular 790
Location 790 a desvincular
  Ambiente  sandbox (dados fictícios)
  Location  pix.example.com/qr/v2/cobv/5b7e4c1a5d3f4a2b9c8d7e6f5a4b3c2d
  Cobrança  cobvexemplo0000000000000000001
aviso: o QR Code desta location deixa de levar à cobrança cobvexemplo0000000000000000001
Desvincular a cobrança? [s/N] s
Cobrança cobvexemplo0000000000000000001 desvinculada: a location está livre.
...
```

A listagem filtra por `--tipo cob` ou `cobv` e por `--com-cobranca` ou `--sem-cobranca`. `desvincular` consulta a location antes, recusa sem nenhuma alteração uma location sem cobrança e pede confirmação (sem terminal, `--sim`), porque o QR Code impresso deixa de levar à cobrança. Os escopos são `payloadlocation.write` e `payloadlocation.read`.

No sandbox, as cobranças podem ser pagas pela CLI, para testar o fluxo inteiro (criar, pagar, consultar e, com um webhook cadastrado, receber a notificação):

```console
$ inter-pj pix cob pagar 7978c0c97ea847e78e8849634473c1f1                 # pelo valor da cobrança
Pago no sandbox: R$ 149,90.
endToEndId  E00416968202609241310abcdEFGH123

Confira com: inter-pj pix cob consultar 7978c0c97ea847e78e8849634473c1f1

$ inter-pj pix cobv pagar cobvexemplo0000000000000000001 --valor 153,00     # outro valor
$ inter-pj pix sandbox pagar-qrcode --copia-e-cola '00020101021226...6304ABCD'  # como um cliente pagaria o QR Code
```

Sem `--valor`, `pagar` usa o valor da cobrança, e `pagar-qrcode`, o do código, conferido (CRC16) antes do envio. Em produção, quem paga é o cliente: os três comandos são recusados antes de qualquer requisição. Pagar precisa do escopo `pix.write` (e de `cob.read` ou `cobv.read` para buscar o valor), e a API aceita até 10 pagamentos por minuto.

### Pix recebidos e devoluções

Os Pix que a conta recebeu, com ou sem cobrança, ficam na API Pix, com as suas devoluções:

```console
$ inter-pj pix recebidos listar --inicio 2026-09-01 --fim 2026-09-30
Pix recebidos de 01/09/2026 00:00 a 30/09/2026 23:59

Horário                    Valor  Devolvido  endToEndId                        txid
18/09/2026 09:41:07    R$ 300,00   R$ 50,00  E00416968202609181241abcdEFGH123  a1b2c3d4e5f60718293a4b5c6d7e8f90
22/09/2026 15:33:10     R$ 89,90             E18236120202609221533s0a1b2c3d4e
23/09/2026 10:02:44  R$ 1.200,00             E60701190202609231002x9y8z7w6v5u  cobvexemplo0000000000000000002

3 Pix · R$ 1.589,90 · devolvidos R$ 50,00

$ inter-pj pix recebidos consultar E00416968202609181241abcdEFGH123
Pix recebido E00416968202609181241abcdEFGH123
  Valor          R$ 300,00
  Recebido em    18/09/2026 09:41:07
  Devolvido      R$ 50,00
  Pode devolver  R$ 250,00
  txid           a1b2c3d4e5f60718293a4b5c6d7e8f90
  Chave          pix@empresa.example
  Mensagem       Pedido 123

Devoluções
id  Status        Valor  Solicitada em
D1  devolvida  R$ 50,00  18/09/2026 12:00:00

Para devolver: inter-pj pix devolucao solicitar E00416968202609181241abcdEFGH123 --valor VALOR (ou --tudo)
```

Os filtros da listagem são `--txid` (os Pix de uma cobrança), `--com-cobranca` ou `--sem-cobranca`, `--com-devolucao` ou `--sem-devolucao` e `--documento` (CPF/CNPJ do pagador), além do período e das páginas, como nas cobranças. Em CSV, as colunas têm os nomes da API, mais `valorDevolvido`. As consultas precisam do escopo `pix.read`.

Uma devolução **tira dinheiro da conta**, e os trilhos são os do `pix enviar`:

```console
$ inter-pj pix devolucao solicitar E00416968202609181241abcdEFGH123 --valor 100,00 --descricao "Pedido cancelado"
Devolução a solicitar
  Ambiente      sandbox (dados fictícios)
  Pix           E00416968202609181241abcdEFGH123
  Recebido em   18/09/2026 09:41:07
  Valor do Pix  R$ 300,00
  Já devolvido  R$ 50,00
  Devolução     R$ 100,00 (cem reais)
  Descrição     Pedido cancelado
  id            D7978c0c97ea847e78e8849634473c1f1
Devolver o Pix? [s/N] s
Devolução solicitada.

Devolução D7978c0c97ea847e78e8849634473c1f1
  Status         em processamento
  Valor          R$ 100,00
  Pix            E00416968202609181241abcdEFGH123
  Solicitada em  24/09/2026 10:10:00
  rtrId          D00416968202609241310xyzabcdefgh

Acompanhe com: inter-pj pix devolucao consultar E00416968202609181241abcdEFGH123 D7978c0c97ea847e78e8849634473c1f1 --aguardar

$ inter-pj pix devolucao solicitar E00416968202609181241abcdEFGH123 --tudo --aguardar   # o que resta, até o fim
$ inter-pj pix devolucao solicitar E00416968202609181241abcdEFGH123 --valor 260 --sim
erro: a devolução de R$ 260,00 passa do que resta do Pix: R$ 250,00 de R$ 300,00, R$ 50,00 já devolvidos ou em devolução
```

- **Consulta antes**: a CLI consulta o Pix e recusa, sem enviar nada, uma devolução maior que o que resta dele (o valor menos as devoluções feitas ou em processamento). `--tudo` devolve exatamente esse resto.
- **Resumo e confirmação**: o Pix, o que já foi devolvido e a devolução, com o valor por extenso e a produção em destaque; `[s/N]` só de um terminal, ou `--sim`. Sem terminal nem `--sim`, nem a consulta é feita.
- **Limite por operação**: `limite_por_operacao` vale também para as devoluções, mesmo com `--sim`.
- **`--simular`**: mostra a requisição, sem consultar nem enviar nada.
- **Idempotência**: cada devolução tem um id (1 a 35 letras e dígitos), gerado ou dado com `--id` e mostrado no resumo; com o mesmo id, a API não devolve de novo. Se o resultado ficar incerto, o erro traz o comando que consulta a devolução e o que a repete com o mesmo id; repetir um id que o Pix já tem apenas mostra a devolução existente.

`--natureza retirada` devolve o dinheiro de um Pix Saque ou o troco de um Pix Troco (o padrão, `original`, é o do Pix comum), e `--descricao` (até 140 caracteres) vai para o pagador. A devolução é processada depois do pedido: `pix devolucao consultar` mostra em que pé ela está e, com `--aguardar` (aceito também por `solicitar`), consulta a cada 6 segundos até o fim, saindo com o código 0 (devolvida), 5 (não realizada, com o motivo) ou 8 (o tempo acabou; padrão: 60s). Devolver precisa do escopo `pix.write`, além de `pix.read` para a consulta.

### Pix Automático

No Pix Automático, o pagador autoriza uma vez, no banco dele, as cobranças de um contrato (uma mensalidade, uma assinatura), que depois são feitas a cada vencimento sem que ele precise pagá-las. A autorização é a **recorrência**: o devedor e o contrato, a periodicidade, o valor (fixo, um mínimo para o limite que o pagador define, ou o de cada cobrança) e se as cobranças não pagas podem ser tentadas de novo. A API é só para CNPJs com pelo menos 6 meses de atividade.

```console
$ inter-pj pix-automatico rec criar --devedor-documento 123.456.789-09 --devedor-nome "Cliente Exemplo" \
    --contrato contrato-001 --objeto Mensalidade --data-inicial 2026-10-10 --periodicidade mensal \
    --valor 149,90 --retentativas
Recorrência a criar
  Ambiente       sandbox (dados fictícios)
  Devedor        Cliente Exemplo (123.456.789-09)
  Contrato       contrato-001
  Objeto         Mensalidade
  Periodicidade  mensal, a partir de 10/10/2026, sem fim
  Valor          R$ 149,90 (cento e quarenta e nove reais e noventa centavos) em cada pagamento
  Retentativas   até 3 novas tentativas, em 7 dias
Criar a recorrência? [s/N] s
Recorrência criada: aguarda a aprovação do pagador.

Recorrência RR1234567820260924abcdefghijk
  Status         criada (aguarda a aprovação do pagador)
  Devedor        Cliente Exemplo (123.456.789-09)
  Contrato       contrato-001
  Objeto         Mensalidade
  Periodicidade  mensal, a partir de 10/10/2026, sem fim
  Valor          R$ 149,90 em cada pagamento
  Retentativas   até 3 novas tentativas, em 7 dias
  Recebedor      Empresa Exemplo Ltda (12.345.678/0001-95)

Histórico
  24/09/2026 10:00:00  criada

Acompanhe com: inter-pj pix-automatico rec consultar RR1234567820260924abcdefghijk
O pagador aprova no banco dele: peça com inter-pj pix-automatico solicitacao criar --rec RR1234567820260924abcdefghijk
```

- **Opções ou arquivo**: `--arquivo rec.json` lê a recorrência nos campos da API (`-` para a entrada padrão; `pix-automatico rec modelo` imprime um exemplo com dados fictícios), com os campos desconhecidos recusados e as mensagens apontando o campo.
- **Valor**: `--valor` fixa o valor de cada pagamento; `--valor-minimo`, quando o valor muda a cada cobrança, é o menor limite que o pagador pode definir; sem nenhum dos dois, vale o de cada cobrança.
- **Período**: `--data-inicial` é a data do primeiro pagamento (não pode ter passado) e `--data-final`, a do último; sem ela, a recorrência não tem fim. `--periodicidade` é `semanal`, `mensal`, `trimestral`, `semestral` ou `anual`.
- **Aprovação**: `--loc` usa uma location criada antes, para o QR Code da recorrência, e `--txid-ativacao`, uma cobrança imediata cujo QR Code composto paga a cobrança e aprova a recorrência ao mesmo tempo.
- **Conferência, resumo e confirmação**: tamanhos, datas e valores são conferidos antes de qualquer requisição; o resumo destaca a produção, e `[s/N]` vem só de um terminal (ou `--sim`). `--simular` mostra a requisição sem enviar nada.
- **Resultado incerto**: esta API não tem chave de idempotência. Se o resultado ficar incerto (tempo esgotado, erro 5xx), a recorrência pode ter sido criada, e o erro traz o `rec listar --documento` que confere isso antes de uma nova tentativa.

`pix-automatico rec listar` mostra as recorrências criadas em um período (padrão: últimos 30 dias), com filtros de status (`--status criada|aprovada|rejeitada|expirada|cancelada`), devedor (`--documento`), location (`--com-location`, `--sem-location`) e `--convenio`, em texto, JSON ou CSV com os nomes da API. `rec consultar <idRec>` mostra a recorrência, o pagador que a aprovou, o histórico e, se houver, o QR Code (`--qrcode`, `--qrcode-png`); com `--txid` de uma cobrança imediata ou com vencimento, traz o QR Code composto, que paga a cobrança e aprova a recorrência. `rec revisar <idRec>` muda o nome do devedor e a location e, antes da aprovação, a data do primeiro pagamento (`--data-inicial`) e a cobrança de ativação (`--txid-ativacao`), mostrando o antes e o depois; `rec cancelar <idRec>` cancela a recorrência depois de mostrá-la e pedir confirmação. Recorrências rejeitadas, expiradas ou canceladas são recusadas sem nenhuma alteração. Os escopos são `rec.write` e `rec.read`.

O pagador aprova a recorrência no banco dele. Para que o banco lhe peça isso, envie uma **solicitação de confirmação** com a conta do pagador:

```console
$ inter-pj pix-automatico solicitacao criar --rec RR1234567820260924abcdefghijk \
    --documento 123.456.789-09 --ispb 12345678 --agencia 0001 --conta 1234567 --expiracao 3d
Solicitação de confirmação a enviar
  Ambiente          sandbox (dados fictícios)
  Recorrência       RR1234567820260924abcdefghijk
  Devedor           Cliente Exemplo (123.456.789-09)
  Contrato          contrato-001
  Objeto            Mensalidade
  Periodicidade     mensal, a partir de 10/10/2026, sem fim
  Valor             R$ 149,90 em cada pagamento
  Conta do pagador  123.456.789-09, banco com ISPB 12345678, agência 0001, conta 1234567
  Expira em         27/09/2026 10:00:00
Enviar a solicitação ao banco do pagador? [s/N] s
Solicitação criada: o banco do pagador vai pedir que ele aprove a recorrência.
```

A CLI consulta a recorrência antes e mostra o que o pagador vai aprovar; recorrências já aprovadas ou encerradas são recusadas sem enviar nada. `--ispb` é o código de 8 dígitos do banco do pagador, `--conta` vai com o dígito verificador e `--expiracao` é o prazo para ele responder (`2h`, `7d`, uma data, até o fim do dia, ou data e hora com fuso; padrão: 7 dias). Como na recorrência, não há chave de idempotência: um resultado incerto vem com o comando que confere se a solicitação foi enviada. `solicitacao consultar <idSolicRec>` mostra em que pé ela está (enviada, recebida, aceita, rejeitada, expirada), e `solicitacao cancelar <idSolicRec>` a cancela enquanto não tiver resposta. A resposta do pagador aparece também em `rec consultar`, no status da recorrência e na lista das suas solicitações. Os escopos são `solicrec.write` e `solicrec.read`, além de `rec.read` para a consulta da recorrência.

Aprovada a recorrência, cada pagamento é uma **cobrança recorrente**, uma por ciclo, que o banco do pagador debita no vencimento:

```console
$ inter-pj pix-automatico cobr criar --rec RR1234567820260924abcdefghijk --valor 149,90 \
    --vencimento 2026-10-10 --conta 1234567 --agencia 0001 --info "Mensalidade de outubro"
Cobrança recorrente a criar
  Ambiente          sandbox (dados fictícios)
  Recorrência       RR1234567820260924abcdefghijk
  Devedor           Cliente Exemplo (123.456.789-09)
  Contrato          contrato-001
  Objeto            Mensalidade
  Valor             R$ 149,90 (cento e quarenta e nove reais e noventa centavos)
  Vencimento        10/10/2026, ou o próximo dia útil
  Conta que recebe  conta corrente 1234567, agência 0001
  Informação        Mensalidade de outubro
  Retentativas      até 3 novas tentativas, em 7 dias
  txid              7978c0c97ea847e78e8849634473c1f1
Criar a cobrança recorrente? [s/N] s
Cobrança recorrente criada: o banco do pagador agenda o débito para o vencimento.
```

- **A recorrência antes**: a CLI a consulta e recusa, sem enviar nada, uma que o pagador não aprovou (ou que foi encerrada); um valor diferente do fixo da recorrência e um vencimento fora do seu período viram avisos no resumo.
- **Conta que recebe**: `--conta`, com o dígito verificador (padrão: a de `--conta-corrente`, que pode vir da configuração), `--tipo-conta` (`corrente`, o padrão, `poupanca` ou `pagamento`) e `--agencia`.
- **Vencimento**: não pode ter passado; em fim de semana ou feriado, vai para o próximo dia útil, pelos feriados da cidade do pagador, a não ser com `--sem-ajuste-dia-util`. `--devedor-email`, `--devedor-endereco`, `--devedor-cidade`, `--devedor-uf` e `--devedor-cep` completam os dados do pagador, que é o da recorrência.
- **txid**: sem `--txid`, a CLI gera um. Com o mesmo txid, a API não cria uma segunda cobrança; por isso, um resultado incerto vem com o comando que a confere e com o `--txid` para repetir sem risco.

`pix-automatico cobr listar` mostra as cobranças recorrentes criadas em um período (padrão: últimos 30 dias), com filtros de recorrência (`--rec`), devedor (`--documento`), status (`--status criada|ativa|concluida|expirada|rejeitada|cancelada`) e `--convenio`, em texto, JSON ou CSV com os nomes da API. `cobr consultar <txid>` mostra a cobrança com as tentativas de liquidação (a data, o tipo, o status e o motivo de uma rejeição), o histórico e o Pix que a pagou. `cobr cancelar <txid>` a cancela depois de mostrá-la e pedir confirmação; pelas regras do Banco Central, isso vale até as 22h do dia anterior à liquidação, e depois disso o resumo avisa que o banco pode recusar. Quando o débito falha e a recorrência permite novas tentativas, `cobr retentativa <txid> --data AAAA-MM-DD` pede uma: a CLI confere a política e o prazo (até 7 dias depois da liquidação prevista) antes de enviar, e avisa quando já há uma tentativa naquele dia ou quando as 3 permitidas já foram pedidas. Cobranças pagas, expiradas, rejeitadas ou canceladas são recusadas sem nenhuma alteração. Os escopos são `cobr.write` e `cobr.read`, além de `rec.read` para a consulta da recorrência.

As **locations de recorrências** são os endereços dos QR Codes com que o pagador aprova uma recorrência. `pix-automatico locrec criar` cria uma, para usar com `rec criar --loc`; `locrec listar` mostra as de um período (padrão: últimos 30 dias), com os filtros `--com-recorrencia`, `--sem-recorrencia` e `--convenio`, em texto, JSON ou CSV; `locrec consultar <id>` mostra uma e a recorrência vinculada; e `locrec desvincular <id>`, depois de mostrá-la e pedir confirmação, a solta da recorrência: o QR Code deixa de levar a ela, que continua como está. Os escopos são `payloadlocationrec.write` e `payloadlocationrec.read`. As mudanças das recorrências e das cobranças recorrentes chegam pelos webhooks do Pix Automático (veja [Webhooks](#webhooks)).

No **sandbox**, `pix-automatico sandbox` faz o papel do pagador e do banco dele, para testar o fluxo inteiro:

```console
$ inter-pj pix-automatico sandbox status-rec RR1234567820260924abcdefghijk --status aprovada
Recorrência RR1234567820260924abcdefghijk aprovada no sandbox.

Confira com: inter-pj pix-automatico rec consultar RR1234567820260924abcdefghijk

$ inter-pj pix-automatico cobr criar --rec RR1234567820260924abcdefghijk --valor 149,90 \
    --vencimento 2026-10-10 --conta 1234567 --txid 7978c0c97ea847e78e8849634473c1f1 --sim
$ inter-pj pix-automatico sandbox pagar-cobr 7978c0c97ea847e78e8849634473c1f1 --chave pix@empresa.example
Pago no sandbox: R$ 149,90.
endToEndId  E12345678202610101300abcdef12345

Confira com: inter-pj pix-automatico cobr consultar 7978c0c97ea847e78e8849634473c1f1
```

| Comando | O que simula |
| --- | --- |
| `sandbox status-rec <idRec> --status aprovada\|cancelada [--razao ...]` | o pagador aprova ou cancela a recorrência (o motivo vai só com o cancelamento) |
| `sandbox status-solicitacao <idRec> --status aceita\|rejeitada` | o pagador responde à solicitação de confirmação, que o sandbox identifica pela recorrência |
| `sandbox status-cobr <txid> [--razao ...]` | o banco do pagador cancela a cobrança recorrente (padrão: sem motivo específico) |
| `sandbox pagar-cobr <txid> --chave ...` | o débito da cobrança recorrente, por padrão com o valor dela e pelo devedor da recorrência (`--valor`, `--documento`) |
| `sandbox pagar-qrcode --copia-e-cola ...` | o pagamento de um QR Code, também o composto de uma cobrança imediata com uma recorrência |

Em produção, todos são recusados antes de qualquer requisição.

### Webhooks

Webhooks são os endereços que o Inter chama quando algo acontece na conta. Cada API tem os seus:

| Comando | O Inter notifica | Escopos |
| --- | --- | --- |
| `webhook banking ... pix-pagamento` | os Pix enviados pela conta | `webhook-banking.write` e `webhook-banking.read` |
| `webhook banking ... boleto-pagamento` | os boletos pagos pela conta | `webhook-banking.write` e `webhook-banking.read` |
| `webhook cobranca ...` | as cobranças recebidas, canceladas e expiradas | `boleto-cobranca.write` e `boleto-cobranca.read` |
| `webhook pix ... CHAVE` | as cobranças Pix pagas, com um webhook por chave Pix | `webhook.write` e `webhook.read` |
| `webhook recorrencia ...` | as mudanças de status das recorrências do Pix Automático, em `URL/rec` | `webhookrec.write` e `webhookrec.read` |
| `webhook cobranca-recorrente ...` | as mudanças de status das cobranças recorrentes do Pix Automático, em `URL/cobr` | `webhookcobr.write` e `webhookcobr.read` |

```console
$ inter-pj webhook cobranca cadastrar --url https://novo.empresa.example/inter/cobrancas
Webhook de cobranças a trocar
  Ambiente   sandbox (dados fictícios)
  Notifica   cobranças recebidas, canceladas e expiradas
  URL atual  https://api.empresa.example/inter/cobrancas
  Nova URL   https://novo.empresa.example/inter/cobrancas
aviso: as notificações passam a ir para novo.empresa.example, e não mais para api.empresa.example
Trocar a URL do webhook? [s/N] s
Webhook cadastrado: o Inter passa a notificar cobranças recebidas, canceladas e expiradas em https://novo.empresa.example/inter/cobrancas.

Confira com: inter-pj webhook cobranca consultar

$ inter-pj webhook pix cadastrar pix@empresa.example --url https://api.empresa.example/inter/pix-cobrancas
$ inter-pj webhook pix consultar pix@empresa.example
Webhook da chave pix@empresa.example
  Notifica       cobranças Pix pagas (imediatas e com vencimento)
  URL            https://api.empresa.example/inter/pix-cobrancas
  Cadastrado em  24/09/2026 10:15:00

$ inter-pj webhook banking consultar                  # os dois tipos
$ inter-pj webhook banking excluir pix-pagamento
Webhook do tipo pix-pagamento a excluir
  Ambiente       sandbox (dados fictícios)
  Notifica       Pix enviados pela conta
  URL            https://api.empresa.example/inter/pix
  Cadastrado em  01/09/2026 09:00:00
aviso: o Inter deixa de notificar Pix enviados pela conta
Excluir o webhook? [s/N] s
Webhook excluído: o Inter deixa de notificar Pix enviados pela conta.
```

A URL precisa começar com `https://`, e o Inter precisa alcançá-la pela internet: a CLI a confere antes de qualquer requisição e avisa quando ela aponta para um endereço local ou de rede privada. `cadastrar` consulta o webhook atual e mostra o antes e o depois, porque a nova URL passa a receber as notificações dos pagamentos da conta; cadastrar a mesma URL não muda nada. `cadastrar` e `excluir` pedem confirmação (sem terminal, `--sim`). Quando o servidor do webhook não aceita uma notificação, o Inter tenta de novo até 4 vezes: 20, 30, 60 e 120 minutos depois (no Banking, 5, 10, 30 e 60). Nos dois webhooks do Pix Automático, o Inter entrega as notificações no endereço cadastrado seguido de `/rec` ou `/cobr`, e a CLI mostra esse endereço de entrega no resumo e na consulta; eles não têm histórico de callbacks nem reenvio.

Cada tentativa fica no histórico dos callbacks, com o status HTTP que o servidor respondeu, e as que falharam podem ser pedidas de novo:

```console
$ inter-pj webhook cobranca callbacks --inicio 2026-09-24 --fim 2026-09-24
Callbacks do webhook de cobranças de 24/09/2026 00:00 a 24/09/2026 23:59

Disparo              Tentativa  Entregue  HTTP  Código da cobrança                    Erro
24/09/2026 11:05:00          2  sim        200  0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d
24/09/2026 11:05:01          2  não        503  1c8f5d2b-6e4a-4b3c-8d9e-8f7a6b5c4d3e  Service Unavailable
24/09/2026 10:45:00          1  não        503  0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d  Service Unavailable
24/09/2026 10:45:01          1  não        503  1c8f5d2b-6e4a-4b3c-8d9e-8f7a6b5c4d3e  Service Unavailable

4 tentativas · 1 entregue · 3 falharam

Sem entrega no período: 1 operação. Para pedir o reenvio:
  inter-pj webhook cobranca reenviar 1c8f5d2b-6e4a-4b3c-8d9e-8f7a6b5c4d3e

$ inter-pj webhook cobranca reenviar 1c8f5d2b-6e4a-4b3c-8d9e-8f7a6b5c4d3e
Reenvio pedido para 1 de 1 operação: o Inter vai enviar os callbacks de novo.

$ inter-pj webhook banking callbacks pix-pagamento --end-to-end E00416968202609241310abcdEFGH123
$ inter-pj webhook pix callbacks --txid 7978c0c97ea847e78e8849634473c1f1 --falhas
$ inter-pj webhook pix reenviar pix@empresa.example 7978c0c97ea847e78e8849634473c1f1
```

O período segue as regras das listagens Pix (padrão: últimos 30 dias), e `--falhas` mostra só as tentativas que falharam; todas as páginas são lidas, ou só uma com `--pagina`. O comando sugerido considera todas as tentativas do período, então uma operação entregue numa tentativa posterior não entra nele. `reenviar` recebe os mesmos códigos que o histórico mostra: o código da solicitação dos Pix enviados (`pix-pagamento`) ou o da transação dos boletos pagos (`boleto-pagamento`), o código das cobranças e, no Pix, a chave e os txids das cobranças. Todos são conferidos antes de qualquer requisição, os repetidos vão uma vez e mais de 50 vão em blocos de 50; como o Inter aceita 5 pedidos de reenvio por minuto, com mais de 5 blocos a CLI espera 12 segundos entre eles. As operações não encontradas são listadas, e, se um bloco falhar, a dica traz o comando que pede o reenvio das que faltam.

### Formatos de saída

| Formato | Para quê |
| --- | --- |
| `texto` (padrão) | leitura: tabelas alinhadas, valores em `R$ 1.234,56`, datas `DD/MM/AAAA` |
| `json` (ou `--json`) | automação: os nomes de campo da API e valores numéricos exatos |
| `csv` | planilhas e scripts (`saldo`, `extrato` e as listagens de pagamentos, de cobranças, de cobranças Pix e seus lotes, de Pix recebidos e de locations): RFC 4180, datas `AAAA-MM-DD`, ponto decimal, saídas do extrato com valor negativo |

Para o Excel em português, use `--formato csv --separador ';'`: ponto e vírgula, vírgula decimal e UTF-8 com BOM. Textos vindos de terceiros que começam com `=`, `+`, `-` ou `@` (ex.: a mensagem de um Pix) recebem um apóstrofo no CSV, para não serem executados como fórmula pela planilha.

### Retentativas

Consultas que falham por limite de requisições (`429`), instabilidade do servidor (`500`, `502`, `503`, `504`) ou falha de conexão são repetidas automaticamente, com espera crescente (1 s, 2 s, ...) e respeitando o cabeçalho `Retry-After`. O padrão é de 3 tentativas; ajuste com `--tentativas N` (ou `INTER_TENTATIVAS`) ou desative com `--sem-retentativa`. Com `-v`, cada nova tentativa aparece em `stderr`. O envio de Pix, os pagamentos, a emissão de cobranças, a criação e a alteração de cobranças Pix e as devoluções só são repetidos quando certamente não foram processados (`429` ou conexão recusada); o Pix, sempre com a mesma chave de idempotência, e a devolução, com o mesmo id.

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
| 5 | requisição rejeitada pela API (400, 404, 409, 422); com `--aguardar`, Pix que terminou sem ser pago, lote processado com erro, cobrança que não foi emitida, alteração que não foi feita ou devolução não realizada |
| 6 | serviço indisponível, limite de requisições (429), erro 5xx ou falha de rede |
| 7 | operação cancelada na confirmação (nada foi enviado) |
| 8 | `pix consultar`, `pagamento lote consultar`, `cobranca emitir`, `cobranca editar`, `cobranca edicao`, `pix devolucao` ou `pix lote-cobv consultar` com `--aguardar`: tempo esgotado antes de um status final |

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
