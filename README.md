# inter-pj

[![CI](https://github.com/edusouza/inter-pj-cli/actions/workflows/ci.yml/badge.svg)](https://github.com/edusouza/inter-pj-cli/actions/workflows/ci.yml)
[![Licença: MIT OR Apache-2.0](https://img.shields.io/badge/licen%C3%A7a-MIT%20OR%20Apache--2.0-blue)](#licença)

CLI em Rust para acessar a sua **conta PJ do Inter Empresas** pela linha de comando, usando as [APIs oficiais do Inter](https://developers.inter.co/references) (OAuth2 + mTLS).

> **Projeto não oficial.** Não tem vínculo com o Banco Inter. Use por sua conta e risco e comece pelo ambiente **sandbox**.

```console
$ inter-pj saldo
Saldo disponível          R$ 16.579,17
Bloqueado em cheque            R$ 0,00
Bloqueado judicialmente        R$ 0,00
Bloqueado administrativo       R$ 0,00
Limite                     R$ 5.000,00

$ inter-pj saldo --json | jq .disponivel
16579.17

$ inter-pj extrato --inicio 2026-08-01 --fim 2026-08-10
Extrato de 01/08/2026 a 10/08/2026

Data        Tipo               Descrição                                      Valor
03/08/2026  Pix                Pix recebido · Cliente Exemplo Ltda      R$ 1.500,00
05/08/2026  Pagamento          Pagamento efetuado · Energia Exemplo SA   -R$ 250,10
10/08/2026  Cobrança (boleto)  Boleto recebido · Beltrana de Tal          R$ 890,00

Entradas              R$ 2.390,00
Saídas                 -R$ 250,10
Resultado do período  R$ 2.139,90
3 transações
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

**Binários prontos**: baixe o pacote do seu sistema na página de [releases](https://github.com/edusouza/inter-pj-cli/releases) (Linux x86_64 e ARM64, glibc ou musl; macOS Apple Silicon ou Intel; e Windows), confira o `SHA256SUMS` e coloque o `inter-pj` no seu `PATH`. Desde a 1.0.0, cada pacote tem também uma atestação de origem, que o liga ao workflow, ao commit e à tag que o produziram. Com a [CLI do GitHub](https://cli.github.com/), confira-a assim (o nome é o do pacote baixado):

```console
$ gh attestation verify inter-pj-1.0.0-x86_64-unknown-linux-musl.tar.gz --repo edusouza/inter-pj-cli
```

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
| Diretório de cache (caminho absoluto) | — | `INTER_CACHE_DIR` |

Locais padrão: configuração em `~/.config/inter-pj/config.toml` (Windows: `%APPDATA%\inter-pj\config.toml`) e cache em `~/.cache/inter-pj` (Windows: `%LOCALAPPDATA%\inter-pj`). Veja com `inter-pj config caminho`.

Os comandos que a saída sugere, como `Acompanhe com: inter-pj -p sandbox pix consultar ...`, levam as flags que escolheram a conta (o perfil, o arquivo, o ambiente, as credenciais e a conta corrente), para rodar na mesma conta; as variáveis de ambiente continuam valendo no mesmo shell.

## Uso

Os [guias](docs/guias/README.md) mostram cada assunto com exemplos de terminal, que os testes executam contra uma simulação da API e conferem a cada mudança. Um resumo:

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

O extrato de um período, o extrato completo, com os detalhes de cada transação, os filtros e as páginas, as planilhas e o PDF estão no guia [Saldo e extrato](docs/guias/saldo-e-extrato.md). A API aceita no máximo 90 dias por consulta, e `--dividir-periodo` consulta um período maior em partes.

### Pix

Enviar um Pix por chave, código copia e cola ou dados bancários, agendar e acompanhar um Pix enviado estão no guia [Pix](docs/guias/pix.md), com os trilhos de segurança de todo envio: o resumo com o valor por extenso e a confirmação num terminal (ou `--sim`), a validação local, `--simular`, o limite por operação do perfil, a aprovação no Internet Banking e a chave de idempotência, com o código de saída 9 quando o resultado fica incerto. Enviar precisa do escopo `pagamento-pix.write`, e consultar, do `pagamento-pix.read`.

### Pagamentos

Boletos, contas de consumo e tributos com código de barras estão no guia [Pagamentos](docs/guias/pagamentos.md): o código conferido pelos dígitos verificadores, o valor e o vencimento que ele traz, `--vencimento` das contas, `--valor` de um boleto vencido ou com desconto, o agendamento, a conferência do beneficiário, a listagem e o cancelamento de um agendamento. Os trilhos são os do Pix, mas esta API não tem chave de idempotência: quando o resultado fica incerto, a CLI mostra como conferir antes de pagar de novo. Pagar precisa do escopo `pagamento-boleto.write`, e listar, do `pagamento-boleto.read`.

### DARF

Os DARFs sem código de barras, pagos pelas opções ou por um arquivo JSON com os campos da API, estão no guia [Pagamentos](docs/guias/pagamentos.md#darf): o documento, o código da receita e a referência conferidos antes do envio, o aviso de um DARF vencido sem multa nem juros, que a API não calcula, e a listagem dos DARFs pagos. Pagar precisa do escopo `pagamento-darf.write`, e listar, do `pagamento-boleto.read`.

### Lotes

Os lotes de 2 a 150 boletos, contas, tributos e DARFs, a partir de uma planilha CSV ou de um arquivo JSON, estão no guia [Pagamentos](docs/guias/pagamentos.md#lotes): o modelo, a conferência do lote inteiro antes do envio, com cada problema apontado pela linha e pelo campo (inclusive o que o Excel estraga numa planilha), a recusa dos pagamentos repetidos no arquivo e a consulta com `--aguardar`, que sai com o código 0, 5 ou 8 conforme o resultado. Enviar precisa do escopo `pagamento-lote.write`, e consultar, do `pagamento-lote.read`.

### Cobranças

Emitir cobranças, que são boletos com Pix para os clientes da empresa, está no guia [Cobranças](docs/guias/cobrancas.md): pelas opções ou por um arquivo JSON com os campos da API (`cobranca modelo`), com o resumo e a confirmação, a espera pela emissão, o boleto e o Pix da cobrança emitida, o QR Code no terminal ou numa imagem, o PDF, o prazo depois do vencimento (`--dias-agenda`), a listagem de um período com os seus filtros, o resumo por situação, a alteração do valor ou do vencimento, o cancelamento, a conferência de uma emissão de resultado incerto e o pagamento no sandbox. Emitir, alterar e cancelar precisam do escopo `boleto-cobranca.write`, e consultar e listar, do `boleto-cobranca.read`.

### Cobranças Pix

A API Pix cria cobranças com QR Code dinâmico, que o cliente paga pelo app de qualquer banco. A cobrança imediata (`pix cob`), para pagar na hora, até expirar, está no guia [Cobranças Pix](docs/guias/cobrancas-pix.md): a criação, com o resumo e a confirmação, o txid que torna segura a repetição, a alteração e a remoção, uma cobrança paga com os seus Pix, a conferência de uma criação de resultado incerto e a listagem de um período com os seus filtros. Criar e alterar precisam do escopo `cob.write`; consultar e listar, do `cob.read`.

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

Os Pix que a conta recebeu, com os filtros e as devoluções de cada um, e as devoluções, com os trilhos de segurança do envio (a consulta antes, o resumo, a confirmação, o limite e o id que torna segura a repetição), estão no guia [Pix](docs/guias/pix.md#pix-recebidos). Consultar precisa do escopo `pix.read`, e devolver, do `pix.write`.

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

Para o Excel em português, use `--formato csv --separador ';'`: ponto e vírgula, vírgula decimal e UTF-8 com BOM. Textos vindos de terceiros que começam com `=`, `+`, `-` ou `@` (ex.: a mensagem de um Pix) recebem um apóstrofo no CSV, para não serem executados como fórmula pela planilha; o mesmo vale depois de uma vírgula, de um ponto e vírgula, de uma tabulação ou de uma quebra de linha dentro do texto, para o caso de a planilha separar as colunas pelo outro separador. Num arquivo, o CSV traz os textos como vieram; mostrado no terminal, sem os caracteres de controle, como a saída em texto. O JSON escapa esses caracteres (`\u001b`), com os mesmos dados.

**Cores**: num terminal, as tabelas das listagens e consultas destacam o cabeçalho, os valores negativos (em vermelho) e o status de cada linha pelo seu tom: verde para pago, recebido ou aprovado; amarelo para agendado, em processamento ou aguardando alguém; vermelho para cancelado, rejeitado, expirado ou com erro. Fora de um terminal (num arquivo ou em outro programa) não há cores; no terminal, desative-as com `--sem-cor` ou com a variável `NO_COLOR` (com qualquer valor não vazio), que valem também para a ajuda. `CLICOLOR_FORCE=1` força as cores mesmo fora de um terminal. Resumos, confirmações e erros, em `stderr`, não têm cores, e JSON e CSV nunca.

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
| 6 | serviço indisponível, limite de requisições (429), erro 5xx ou falha de rede, numa operação que certamente não foi feita (uma consulta, ou um envio que a API não recebeu) |
| 7 | operação cancelada na confirmação (nada foi enviado) |
| 8 | `pix consultar`, `pagamento lote consultar`, `cobranca emitir`, `cobranca editar`, `cobranca edicao`, `pix devolucao` ou `pix lote-cobv consultar` com `--aguardar`: tempo esgotado antes de um status final |
| 9 | resultado incerto: um envio pode ter sido feito (tempo esgotado, erro 5xx ou resposta ilegível depois do envio). Confira com o comando da dica antes de tentar de novo; um script não deve repetir o comando às cegas |

Mensagens de erro vão para `stderr`, em português, com a explicação da API e dicas. Com `-v`/`-vv` a CLI mostra detalhes das requisições (método, caminho, status e tempo) — nunca tokens, segredos ou corpos de resposta.

## Segurança e privacidade

- Nenhuma credencial, certificado, token ou número de conta faz parte do repositório; o CI roda o [gitleaks](https://github.com/gitleaks/gitleaks) sobre todo o histórico a cada push.
- Segredos são mantidos em tipos que não aparecem em logs nem em mensagens de erro.
- TLS com [rustls](https://github.com/rustls/rustls) (sem OpenSSL), mTLS obrigatório e somente `https`.

Detalhes e como reportar vulnerabilidades em [`SECURITY.md`](SECURITY.md); o modelo de ameaças e os achados da revisão de segurança da 1.0.0, com as correções, em [`docs/seguranca.md`](docs/seguranca.md).

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
