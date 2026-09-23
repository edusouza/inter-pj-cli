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

O plano completo — cobranças (boleto com Pix), Pix Cobrança, webhooks e Pix Automático — está em [`docs/roadmap.md`](docs/roadmap.md) e é acompanhado pelas [issues do projeto](https://github.com/edusouza/inter-pj-cli/issues).

## Instalação

**Binários prontos**: baixe o pacote do seu sistema na página de [releases](https://github.com/edusouza/inter-pj-cli/releases) (Linux x86_64 glibc/musl, macOS Apple Silicon/Intel e Windows), confira o `SHA256SUMS` e coloque o `inter-pj` no seu `PATH`.

**Com Cargo** (Rust 1.88 ou superior):

```console
$ cargo install --locked --git https://github.com/edusouza/inter-pj-cli --tag v0.4.0 inter-pj-cli
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
# limite_por_operacao = "1.000,00" # opcional: valor máximo de cada Pix ou pagamento
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

Cobranças são boletos com Pix para os clientes da empresa. A consulta mostra a situação, os valores e os encargos, o boleto e o Pix:

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
  Nosso número     12345678
  Linha digitável  07790.00116 12345.678002 12345.678903 1 16050000015000

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

### Formatos de saída

| Formato | Para quê |
| --- | --- |
| `texto` (padrão) | leitura: tabelas alinhadas, valores em `R$ 1.234,56`, datas `DD/MM/AAAA` |
| `json` (ou `--json`) | automação: os nomes de campo da API e valores numéricos exatos |
| `csv` | planilhas e scripts (`saldo`, `extrato` e as listagens de pagamentos e de cobranças): RFC 4180, datas `AAAA-MM-DD`, ponto decimal, saídas do extrato com valor negativo |

Para o Excel em português, use `--formato csv --separador ';'`: ponto e vírgula, vírgula decimal e UTF-8 com BOM. Textos vindos de terceiros que começam com `=`, `+`, `-` ou `@` (ex.: a mensagem de um Pix) recebem um apóstrofo no CSV, para não serem executados como fórmula pela planilha.

### Retentativas

Consultas que falham por limite de requisições (`429`), instabilidade do servidor (`500`, `502`, `503`, `504`) ou falha de conexão são repetidas automaticamente, com espera crescente (1 s, 2 s, ...) e respeitando o cabeçalho `Retry-After`. O padrão é de 3 tentativas; ajuste com `--tentativas N` (ou `INTER_TENTATIVAS`) ou desative com `--sem-retentativa`. Com `-v`, cada nova tentativa aparece em `stderr`. O envio de Pix e os pagamentos só são repetidos quando certamente não foram processados (`429` ou conexão recusada); o Pix, sempre com a mesma chave de idempotência.

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
| 5 | requisição rejeitada pela API (400, 404, 409, 422); com `--aguardar`, Pix que terminou sem ser pago ou lote processado com erro |
| 6 | serviço indisponível, limite de requisições (429), erro 5xx ou falha de rede |
| 7 | operação cancelada na confirmação (nada foi enviado) |
| 8 | `pix consultar` ou `pagamento lote consultar` com `--aguardar`: tempo esgotado antes de um status final |

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
