# Pagamentos

Boletos, contas de consumo e tributos com código de barras são pagos pelo código, e os DARFs sem código de barras, pelos seus campos; uns e outros podem ir juntos num lote, a partir de uma planilha. Pagar tira dinheiro da conta, e os trilhos de segurança são os do [Pix](pix.md): o resumo com o valor por extenso, a confirmação num terminal (ou `--sim`), `--simular` e o limite por operação do perfil. Diferente do Pix, estas APIs não têm chave de idempotência, então um resultado incerto pede uma conferência antes de qualquer nova tentativa.

Os exemplos são da Empresa Exemplo Ltda, uma empresa fictícia, no perfil de produção. Cada comando precisa de um escopo da integração:

| Comando | Escopo |
| --- | --- |
| `pagamento boleto pagar` e `cancelar` | `pagamento-boleto.write` |
| `pagamento boleto listar` e `pagamento darf listar` | `pagamento-boleto.read` |
| `pagamento darf pagar` | `pagamento-darf.write` |
| `pagamento lote enviar` | `pagamento-lote.write` |
| `pagamento lote consultar` | `pagamento-lote.read` |

- [Pagar um boleto](#pagar-um-boleto)
- [Contas e tributos](#contas-e-tributos)
- [Vencidos, juros e descontos](#vencidos-juros-e-descontos)
- [Agendar e conferir o beneficiário](#agendar-e-conferir-o-beneficiário)
- [Conferir sem pagar](#conferir-sem-pagar)
- [Quando o resultado fica incerto](#quando-o-resultado-fica-incerto)
- [Os pagamentos feitos](#os-pagamentos-feitos)
- [Cancelar um agendamento](#cancelar-um-agendamento)
- [DARF](#darf)
- [DARF vencido](#darf-vencido)
- [Os DARFs pagos](#os-darfs-pagos)
- [Lotes](#lotes)
- [Um lote com problemas](#um-lote-com-problemas)

## Pagar um boleto

```console
$ inter-pj pagamento boleto pagar '07790.00017 23456.700006 00000.015818 1 15850000189000'
*** PRODUÇÃO: este pagamento movimenta dinheiro da conta real ***
Pagamento a enviar
  Ambiente         PRODUÇÃO (conta real)
  Tipo             boleto do banco 077
  Linha digitável  07790.00017 23456.700006 00000.015818 1 15850000189000
  Valor            R$ 1.890,00 (mil oitocentos e noventa reais)
  Vencimento       30/09/2026
  Quando           agora
Confirmar o pagamento? [s/N] s
Pagamento realizado.
Código da transação  3414f226-36fb-4d87-811e-cfd99911d845

Acompanhe com: inter-pj pagamento boleto listar --codigo-transacao 3414f226-36fb-4d87-811e-cfd99911d845
```

O código pode ser a linha digitável, com ou sem a pontuação (47 dígitos nos boletos, 48 nas contas e nos tributos), ou o código de barras (44). A CLI confere todos os dígitos verificadores antes de qualquer requisição, e o valor e o vencimento vêm do próprio código. Um dígito trocado é recusado:

```console
$ inter-pj pagamento boleto pagar '07790.00017 23456.700006 00000.015818 1 15850000189001'
erro: valor inválido '07790.00017 23456.700006 00000.015818 1 15850000189001' para '<CODIGO>': dígito verificador geral não confere: confira a digitação do código

Para mais informações, use '--help'.
```

Os dígitos verificadores não pegam todo erro de digitação: no boleto, um dígito errado no valor ou no vencimento pode passar por eles. Por isso o resumo mostra o valor e o vencimento que o código traz, para você conferir com o documento.

## Contas e tributos

As contas de consumo e os tributos com código de barras, que começam com 8, não trazem o vencimento no código. Informe a data impressa no documento com `--vencimento`:

```console
$ inter-pj pagamento boleto pagar 836400000029483701012021609010000006000012345674
erro: contas e tributos não trazem o vencimento no código: informe --vencimento com a data impressa no documento (AAAA-MM-DD)

$ inter-pj pagamento boleto pagar 836400000029483701012021609010000006000012345674 --vencimento 2026-09-28
*** PRODUÇÃO: este pagamento movimenta dinheiro da conta real ***
Pagamento a enviar
  Ambiente         PRODUÇÃO (conta real)
  Tipo             conta ou tributo: energia elétrica e gás
  Linha digitável  83640000002-9 48370101202-1 60901000000-6 00001234567-4
  Valor            R$ 248,37 (duzentos e quarenta e oito reais e trinta e sete centavos)
  Vencimento       28/09/2026
  Quando           agora
Confirmar o pagamento? [s/N] s
Pagamento realizado.
Código da transação  8c1d2e3f-4a5b-4c6d-8e7f-9a0b1c2d3e4f

Acompanhe com: inter-pj pagamento boleto listar --codigo-transacao 8c1d2e3f-4a5b-4c6d-8e7f-9a0b1c2d3e4f
```

## Vencidos, juros e descontos

Para pagar outro valor, como o de um boleto vencido, com juros e multa, ou o de um com desconto, use `--valor`. O resumo mostra os dois valores, com um aviso, e avisa também quando o pagamento fica para depois do vencimento:

```console
$ inter-pj pagamento boleto pagar '07790.00074 65432.100009 00000.009134 2 15750000031240' --valor 318,65
*** PRODUÇÃO: este pagamento movimenta dinheiro da conta real ***
Pagamento a enviar
  Ambiente         PRODUÇÃO (conta real)
  Tipo             boleto do banco 077
  Linha digitável  07790.00074 65432.100009 00000.009134 2 15750000031240
  Valor            R$ 318,65 (trezentos e dezoito reais e sessenta e cinco centavos)
  Valor no código  R$ 312,40
  Vencimento       20/09/2026
  Quando           agora
aviso: o valor a pagar (R$ 318,65) é maior que o do código (R$ 312,40): confira juros e multa
aviso: o pagamento fica para depois do vencimento (20/09/2026): pode haver juros e multa, ou recusa
Confirmar o pagamento? [s/N] s
Pagamento realizado.
Código da transação  a7b9c1d3-e5f7-4a9b-8c1d-3e5f7a9b1c2d

Acompanhe com: inter-pj pagamento boleto listar --codigo-transacao a7b9c1d3-e5f7-4a9b-8c1d-3e5f7a9b1c2d
```

## Agendar e conferir o beneficiário

`--data` agenda o pagamento para um dia, hoje ou depois, e `--beneficiario` pede ao banco que confira o CPF ou o CNPJ de quem recebe. Um documento que não confere é recusado, sem pagar nada:

```console
$ inter-pj pagamento boleto pagar '07790.00017 23456.700006 00000.016022 7 16000000189000' --data 2026-10-14 --beneficiario 11.222.333/0001-81 --sim
*** PRODUÇÃO: este pagamento movimenta dinheiro da conta real ***
Pagamento a enviar
  Ambiente         PRODUÇÃO (conta real)
  Tipo             boleto do banco 077
  Linha digitável  07790.00017 23456.700006 00000.016022 7 16000000189000
  Valor            R$ 1.890,00 (mil oitocentos e noventa reais)
  Vencimento       15/10/2026
  Quando           agendado para 14/10/2026
  Beneficiário     11.222.333/0001-81 (a API confere)
erro: POST /banking/v2/pagamento respondeu 400 (requisição inválida): Beneficiário não confere — O CPF/CNPJ informado não é o do beneficiário do título.

$ inter-pj pagamento boleto pagar '07790.00017 23456.700006 00000.016022 7 16000000189000' --data 2026-10-14 --beneficiario 12.345.678/0001-95
*** PRODUÇÃO: este pagamento movimenta dinheiro da conta real ***
Pagamento a enviar
  Ambiente         PRODUÇÃO (conta real)
  Tipo             boleto do banco 077
  Linha digitável  07790.00017 23456.700006 00000.016022 7 16000000189000
  Valor            R$ 1.890,00 (mil oitocentos e noventa reais)
  Vencimento       15/10/2026
  Quando           agendado para 14/10/2026
  Beneficiário     12.345.678/0001-95 (a API confere)
Confirmar o pagamento? [s/N] s
Pagamento agendado para 14/10/2026.
Código da transação  4e6a8c0b-2d4f-4b6a-9c8e-0b2d4f6a8c1e
Data do agendamento  14/10/2026

Acompanhe com: inter-pj pagamento boleto listar --codigo-transacao 4e6a8c0b-2d4f-4b6a-9c8e-0b2d4f6a8c1e
Para cancelar: inter-pj pagamento boleto cancelar 4e6a8c0b-2d4f-4b6a-9c8e-0b2d4f6a8c1e
```

## Conferir sem pagar

`--simular` confere o código e mostra a requisição que seria enviada, sem pedir token e sem pagar nada:

```console
$ inter-pj pagamento boleto pagar '07790.00017 23456.700006 00000.016022 7 16000000189000' --simular
*** PRODUÇÃO: este pagamento movimenta dinheiro da conta real ***
Pagamento a enviar
  Ambiente         PRODUÇÃO (conta real)
  Tipo             boleto do banco 077
  Linha digitável  07790.00017 23456.700006 00000.016022 7 16000000189000
  Valor            R$ 1.890,00 (mil oitocentos e noventa reais)
  Vencimento       15/10/2026
  Quando           agora
Simulação: nada foi enviado.

POST https://cdpj.partners.bancointer.com.br/banking/v2/pagamento

{
  "codBarraLinhaDigitavel": "07797160000001890000000123456700000000001602",
  "dataVencimento": "2026-10-15",
  "valorPagar": "1890.00"
}
```

## Quando o resultado fica incerto

Se a resposta se perder depois do envio (um tempo esgotado, um erro 5xx), o pagamento pode ter sido feito, e repetir o comando pode pagar duas vezes. A CLI sai com o código 9 e diz como conferir:

```console
$ inter-pj pagamento boleto pagar 846200000012899002022023609100000007000098765431 --vencimento 2026-09-30 --sim
*** PRODUÇÃO: este pagamento movimenta dinheiro da conta real ***
Pagamento a enviar
  Ambiente         PRODUÇÃO (conta real)
  Tipo             conta ou tributo: telecomunicações
  Linha digitável  84620000001-2 89900202202-3 60910000000-7 00009876543-1
  Valor            R$ 189,90 (cento e oitenta e nove reais e noventa centavos)
  Vencimento       30/09/2026
  Quando           agora
erro: POST /banking/v2/pagamento respondeu 504 (tempo esgotado no gateway)
dica: o pagamento pode ter sido feito, e esta API não tem chave de idempotência: repetir o comando pode pagar duas vezes
dica: confira antes de tentar de novo: inter-pj pagamento boleto listar --codigo 84620000001899002022026091000000000009876543

$ inter-pj pagamento boleto listar --codigo 84620000001899002022026091000000000009876543
Pagamentos incluídos de 26/08/2026 a 24/09/2026
Código: 84620000001-2 89900202202-3 60910000000-7 00009876543-1

Vencimento  Pagamento   Beneficiário          Status      Valor  Código da transação
30/09/2026  24/09/2026  Telefonia Exemplo SA  pago    R$ 189,90  d2f4a6c8-e0b2-4d4f-8a6c-8e0b2d4f6a8b

1 pagamento
```

O pagamento foi feito: não pague de novo. Conforme a configuração da conta, um pagamento pode também esperar a aprovação de outra pessoa no Internet Banking; a CLI avisa quando for o caso.

## Os pagamentos feitos

`pagamento boleto listar` mostra os pagamentos de um período de até 90 dias, pelo dia em que foram incluídos; sem datas, os dos últimos 30 dias. `--filtrar-por` escolhe a data a que o período se refere (`inclusao`, `pagamento` ou `vencimento`), e `--codigo` e `--codigo-transacao` acham um pagamento:

```console
$ inter-pj pagamento boleto listar --inicio 2026-07-01 --fim 2026-09-24
Pagamentos incluídos de 01/07/2026 a 24/09/2026

Vencimento  Pagamento   Beneficiário           Status          Valor  Código da transação
15/07/2026  15/07/2026  Fornecedor Exemplo SA  pago        R$ 480,00  0f1e2d3c-4b5a-4968-8776-655443322110
05/08/2026  05/08/2026  Energia Exemplo SA     pago        R$ 250,10  9e8d7c6b-5a49-4382-9716-05f4e3d2c1b0
30/09/2026  24/09/2026  Fornecedor Exemplo SA  pago      R$ 1.890,00  3414f226-36fb-4d87-811e-cfd99911d845
28/09/2026  24/09/2026  Energia Exemplo SA     pago        R$ 248,37  8c1d2e3f-4a5b-4c6d-8e7f-9a0b1c2d3e4f
20/09/2026  24/09/2026  Papelaria Exemplo      pago        R$ 318,65  a7b9c1d3-e5f7-4a9b-8c1d-3e5f7a9b1c2d
15/10/2026  14/10/2026  Fornecedor Exemplo SA  agendado  R$ 1.890,00  4e6a8c0b-2d4f-4b6a-9c8e-0b2d4f6a8c1e
30/09/2026  24/09/2026  Telefonia Exemplo SA   pago        R$ 189,90  d2f4a6c8-e0b2-4d4f-8a6c-8e0b2d4f6a8b

7 pagamentos

$ inter-pj pagamento boleto listar --filtrar-por vencimento --inicio 2026-10-01 --fim 2026-10-31
Pagamentos com vencimento de 01/10/2026 a 31/10/2026

Vencimento  Pagamento   Beneficiário           Status          Valor  Código da transação
15/10/2026  14/10/2026  Fornecedor Exemplo SA  agendado  R$ 1.890,00  4e6a8c0b-2d4f-4b6a-9c8e-0b2d4f6a8c1e

1 pagamento
```

A listagem sai também em `--json` e em `--formato csv`, com os campos da API.

## Cancelar um agendamento

Um pagamento agendado pode ser cancelado: a CLI mostra o agendamento e pede a confirmação.

```console
$ inter-pj pagamento boleto cancelar 4e6a8c0b-2d4f-4b6a-9c8e-0b2d4f6a8c1e
Agendamento a cancelar
  Ambiente             PRODUÇÃO (conta real)
  Código da transação  4e6a8c0b-2d4f-4b6a-9c8e-0b2d4f6a8c1e
  Beneficiário         Fornecedor Exemplo SA (12.345.678/0001-95)
  Valor                R$ 1.890,00
  Pagamento em         14/10/2026
  Status               agendado
Cancelar o agendamento? [s/N] s
Agendamento cancelado: 4e6a8c0b-2d4f-4b6a-9c8e-0b2d4f6a8c1e

$ inter-pj pagamento boleto listar --codigo-transacao 4e6a8c0b-2d4f-4b6a-9c8e-0b2d4f6a8c1e
Pagamentos incluídos de 26/08/2026 a 24/09/2026

Vencimento  Pagamento   Beneficiário           Status                       Valor  Código da transação
15/10/2026  14/10/2026  Fornecedor Exemplo SA  agendamento cancelado  R$ 1.890,00  4e6a8c0b-2d4f-4b6a-9c8e-0b2d4f6a8c1e

1 pagamento
```

Um pagamento já feito não pode ser cancelado, e a CLI avisa antes de pedir:

```console
$ inter-pj pagamento boleto cancelar 3414f226-36fb-4d87-811e-cfd99911d845 --sim
Agendamento a cancelar
  Ambiente             PRODUÇÃO (conta real)
  Código da transação  3414f226-36fb-4d87-811e-cfd99911d845
  Beneficiário         Fornecedor Exemplo SA (12.345.678/0001-95)
  Valor                R$ 1.890,00
  Pagamento em         24/09/2026
  Status               pago
aviso: o pagamento está pago: a API deve recusar o cancelamento
erro: DELETE /banking/v2/pagamento/{codigoTransacao} respondeu 422 (não processável): Pagamento não pode ser cancelado — Só um pagamento agendado pode ser cancelado.
```

## DARF

O DARF sem código de barras, de tributos federais como o PIS e a COFINS, é pago pelos campos do documento. Cada campo tem uma opção, e o seu nome na API é o do arquivo JSON que `--arquivo` lê:

| Campo do DARF | Opção | Campo da API |
| --- | --- | --- |
| 01 Nome e telefone | `--nome-empresa` e, se quiser, `--telefone` | `nomeEmpresa` e `telefoneEmpresa` |
| 02 Período de apuração | `--periodo-apuracao` | `periodoApuracao` |
| 03 CPF ou CNPJ | `--contribuinte` | `cnpjCpf` |
| 04 Código da receita | `--codigo-receita` | `codigoReceita` |
| 05 Número de referência | `--referencia` | `referencia` |
| 06 Data de vencimento | `--vencimento` | `dataVencimento` |
| 07 Valor do principal | `--valor-principal` | `valorPrincipal` |
| 08 Valor da multa | `--multa` | `valorMulta` |
| 09 Valor dos juros | `--juros` | `valorJuros` |

`--descricao`, que não está no DARF, descreve o pagamento. A API exige todos os campos, menos o telefone, a multa e os juros. A COFINS de agosto da Empresa Exemplo vence amanhã:

```console
$ inter-pj pagamento darf pagar --codigo-receita 2172 --contribuinte 11.444.777/0001-61 \
    --nome-empresa "Empresa Exemplo Ltda" --periodo-apuracao 2026-08-31 --vencimento 2026-09-25 \
    --referencia 13609400849201739 --descricao "COFINS de agosto" --valor-principal 1.234,56
*** PRODUÇÃO: este pagamento movimenta dinheiro da conta real ***
DARF a pagar
  Ambiente             PRODUÇÃO (conta real)
  Contribuinte         Empresa Exemplo Ltda (11.444.777/0001-61)
  Código da receita    2172
  Período de apuração  31/08/2026
  Vencimento           25/09/2026
  Referência           13609400849201739
  Descrição            COFINS de agosto
  Valor principal      R$ 1.234,56
  Total                R$ 1.234,56 (mil duzentos e trinta e quatro reais e cinquenta e seis centavos)
  Quando               agora
Confirmar o pagamento do DARF? [s/N] s
DARF pago.
Código da solicitação  b1c2d3e4-f5a6-4b7c-8d9e-0f1a2b3c4d5e
Data do pagamento      24/09/2026
Autenticação           202609240001

Acompanhe com: inter-pj pagamento darf listar --codigo-solicitacao b1c2d3e4-f5a6-4b7c-8d9e-0f1a2b3c4d5e
```

Antes de enviar, a CLI confere o CPF ou o CNPJ (os dígitos verificadores), o código da receita (4 dígitos), a referência (só dígitos, até 30), os textos e os valores. Como nos boletos, conforme a configuração da conta o pagamento pode esperar a aprovação de outra pessoa no Internet Banking, e, se a resposta se perder, a CLI sai com o código 9 e mostra como conferir antes de qualquer nova tentativa (veja [Quando o resultado fica incerto](#quando-o-resultado-fica-incerto)).

## DARF vencido

Um DARF pago depois do vencimento precisa da multa e dos juros, que a API não calcula. Sem eles, o resumo avisa. Com `--simular`, nada é enviado, e a CLI mostra a requisição:

```console
$ inter-pj pagamento darf pagar --codigo-receita 8109 --contribuinte 11.444.777/0001-61 \
    --nome-empresa "Empresa Exemplo Ltda" --periodo-apuracao 2026-07-31 --vencimento 2026-08-25 \
    --referencia 13609400849201747 --descricao "PIS de julho" --valor-principal 267,49 --simular
*** PRODUÇÃO: este pagamento movimenta dinheiro da conta real ***
DARF a pagar
  Ambiente             PRODUÇÃO (conta real)
  Contribuinte         Empresa Exemplo Ltda (11.444.777/0001-61)
  Código da receita    8109
  Período de apuração  31/07/2026
  Vencimento           25/08/2026
  Referência           13609400849201747
  Descrição            PIS de julho
  Valor principal      R$ 267,49
  Total                R$ 267,49 (duzentos e sessenta e sete reais e quarenta e nove centavos)
  Quando               agora
aviso: o DARF venceu em 25/08/2026 e não tem multa nem juros: pago depois do vencimento, ele precisa dos acréscimos calculados
Simulação: nada foi enviado.

POST https://cdpj.partners.bancointer.com.br/banking/v2/pagamento/darf

{
  "cnpjCpf": "11444777000161",
  "codigoReceita": "8109",
  "dataVencimento": "2026-08-25",
  "descricao": "PIS de julho",
  "nomeEmpresa": "Empresa Exemplo Ltda",
  "periodoApuracao": "2026-07-31",
  "referencia": "13609400849201747",
  "valorPrincipal": 267.49
}
```

O arquivo que `--arquivo` lê é um objeto JSON com os mesmos campos. Com a multa e os juros, o PIS de julho fica assim, em `darf.json`:

<!-- guia: arquivo darf.json -->
```json
{
  "cnpjCpf": "11.444.777/0001-61",
  "codigoReceita": "8109",
  "nomeEmpresa": "Empresa Exemplo Ltda",
  "periodoApuracao": "2026-07-31",
  "dataVencimento": "2026-08-25",
  "referencia": "13609400849201747",
  "descricao": "PIS de julho",
  "valorPrincipal": "267,49",
  "valorMulta": "26,48",
  "valorJuros": "2,67"
}
```

```console
$ inter-pj pagamento darf pagar --arquivo darf.json
*** PRODUÇÃO: este pagamento movimenta dinheiro da conta real ***
DARF a pagar
  Ambiente             PRODUÇÃO (conta real)
  Contribuinte         Empresa Exemplo Ltda (11.444.777/0001-61)
  Código da receita    8109
  Período de apuração  31/07/2026
  Vencimento           25/08/2026
  Referência           13609400849201747
  Descrição            PIS de julho
  Valor principal      R$ 267,49
  Multa                R$ 26,48
  Juros                R$ 2,67
  Total                R$ 296,64 (duzentos e noventa e seis reais e sessenta e quatro centavos)
  Quando               agora
Confirmar o pagamento do DARF? [s/N] s
DARF pago.
Código da solicitação  c2d3e4f5-a6b7-4c8d-9e0f-1a2b3c4d5e6f
Data do pagamento      24/09/2026
Autenticação           202609240002

Acompanhe com: inter-pj pagamento darf listar --codigo-solicitacao c2d3e4f5-a6b7-4c8d-9e0f-1a2b3c4d5e6f
```

Os valores podem ser números (`267.49`) ou textos (`"267,49"`). Um campo desconhecido é recusado, para que um erro de digitação (`valorMuta`) não apague a multa, e as mensagens apontam o campo. Com `--arquivo -`, o DARF vem da entrada padrão, e a confirmação exige `--sim`.

## Os DARFs pagos

`pagamento darf listar` mostra os DARFs pagos num período, pelo dia do pagamento; sem datas, os incluídos nos últimos 30 dias, como a API faz. `--codigo-receita` e `--codigo-solicitacao` filtram:

```console
$ inter-pj pagamento darf listar
DARFs incluídos nos últimos 30 dias

Pagamento   Receita  Apuração    Vencimento  Status        Total  Código da solicitação
24/09/2026  2172     31/08/2026  25/09/2026  pago    R$ 1.234,56  b1c2d3e4-f5a6-4b7c-8d9e-0f1a2b3c4d5e
24/09/2026  8109     31/07/2026  25/08/2026  pago      R$ 296,64  c2d3e4f5-a6b7-4c8d-9e0f-1a2b3c4d5e6f

2 DARFs

$ inter-pj pagamento darf listar --inicio 2026-09-01 --fim 2026-09-30 --codigo-receita 8109
DARFs pagos de 01/09/2026 a 30/09/2026
Código da receita: 8109

Pagamento   Receita  Apuração    Vencimento  Status      Total  Código da solicitação
24/09/2026  8109     31/07/2026  25/08/2026  pago    R$ 296,64  c2d3e4f5-a6b7-4c8d-9e0f-1a2b3c4d5e6f

1 DARF
```

A listagem sai também em `--json` e em `--formato csv`, com os campos da API.

## Lotes

De 2 a 150 boletos, contas, tributos e DARFs podem ir juntos num lote, a partir de uma planilha CSV ou de um arquivo JSON. `pagamento lote modelo` imprime um exemplo, com um boleto, uma conta e um DARF de dados fictícios: em JSON ou, com `csv`, numa planilha separada por `;`, como o Excel em português salva.

```console
$ inter-pj pagamento lote modelo csv > lote.csv
```

Na planilha, cada linha é um pagamento, e cada coluna, um campo da API; as colunas que um pagamento não usa ficam vazias, e as que nenhum usa podem sair. Cada pagamento tem o `tipoPagamento` e os campos do seu tipo:

- `BOLETO`, para boletos, contas e tributos com código de barras: `codBarraLinhaDigitavel` e, quando o código não os traz ou para pagar outro valor, `dataVencimento` e `valorPagar`; se quiser, `dataPagamento`, para agendar, e `cpfCnpjBeneficiario`, como em `pagamento boleto pagar`;
- `DARF`: os campos da API do [DARF](#darf).

A planilha dos pagamentos do dia, `lote.csv`, tem o boleto de outubro do fornecedor, cujo agendamento foi cancelado, o PIS de agosto e, por engano, o boleto de setembro, que já foi pago:

<!-- guia: arquivo lote.csv -->
```csv
tipoPagamento;codBarraLinhaDigitavel;dataVencimento;cnpjCpf;nomeEmpresa;codigoReceita;periodoApuracao;referencia;descricao;valorPrincipal
BOLETO;07790.00017 23456.700006 00000.016022 7 16000000189000;;;;;;;;
BOLETO;07790.00017 23456.700006 00000.015818 1 15850000189000;;;;;;;;
DARF;;2026-09-25;11.444.777/0001-61;Empresa Exemplo Ltda;8109;2026-08-31;13609400849201755;PIS de agosto;267,49
```

```console
$ inter-pj pagamento lote enviar --arquivo lote.csv --identificador "Pagamentos de 24/09"
*** PRODUÇÃO: este lote movimenta dinheiro da conta real ***
Lote a enviar
  Ambiente       PRODUÇÃO (conta real)
  Arquivo        lote.csv
  Identificador  Pagamentos de 24/09
  Pagamentos     2 boletos e contas (R$ 3.780,00) e 1 DARF (R$ 267,49)
  Total          R$ 4.047,49 (quatro mil e quarenta e sete reais e quarenta e nove centavos)

  Onde     Tipo    Documento                                                 Vencimento  Quando        Valor
  linha 2  boleto  07790.00017 23456.700006 00000.016022 7 16000000189000    15/10/2026  agora   R$ 1.890,00
  linha 3  boleto  07790.00017 23456.700006 00000.015818 1 15850000189000    30/09/2026  agora   R$ 1.890,00
  linha 4  DARF    receita 8109 · Empresa Exemplo Ltda (11.444.777/0001-61)  25/09/2026  agora     R$ 267,49
Enviar o lote de 3 pagamentos (R$ 4.047,49)? [s/N] s
Lote recebido: 3 pagamentos, em processamento.
Identificador do lote  5f1b2c3d4e5f60718293a4b5
Meu identificador      Pagamentos de 24/09

Acompanhe com: inter-pj pagamento lote consultar 5f1b2c3d4e5f60718293a4b5 --aguardar
```

O resumo mostra o total de cada tipo e o do lote, por extenso, e cada pagamento, e avisa, como nos pagamentos avulsos, sobre os vencimentos que já passaram e os valores diferentes dos do código.

O banco processa o lote depois de recebê-lo. `pagamento lote consultar` mostra o status do lote e de cada pagamento, e com `--aguardar` consulta de novo a cada 6 segundos, até o fim do processamento:

```console
$ inter-pj pagamento lote consultar 5f1b2c3d4e5f60718293a4b5 --aguardar
Lote 5f1b2c3d4e5f60718293a4b5
  Status             processado com erro
  Meu identificador  Pagamentos de 24/09
  Criado em          24/09/2026 10:15:00
  Pagamentos         3

Tipo    Documento                                                 Status        Valor  Código                                Detalhe
boleto  07790.00017 23456.700006 00000.016022 7 16000000189000    pago    R$ 1.890,00  6b8d0f2a-4c6e-4a8b-9d0f-2a4c6e8b0d3f
boleto  07790.00017 23456.700006 00000.015818 1 15850000189000    erro    R$ 1.890,00                                        Pagamento já realizado para este código de barras.
DARF    receita 8109 · Empresa Exemplo Ltda (11.444.777/0001-61)  pago      R$ 267,49  d3e4f5a6-b7c8-4d9e-8f1a-2b3c4d5e6f7a
erro: o lote foi processado com erro: 1 de 3 pagamentos não foi feito
```

O banco recusou o boleto de setembro, que já tinha sido pago em [Pagar um boleto](#pagar-um-boleto). Não conte com isso: a CLI recusa um pagamento repetido dentro do arquivo, mas não confere os que já foram feitos, e nem todo pagamento em dobro é recusado pelo banco. Antes de enviar um lote, confira os pagamentos feitos com `pagamento boleto listar`.

Com `--aguardar`, a CLI sai com o código 0 quando o lote é processado sem erro, 5 quando algum pagamento não foi feito e 8 quando o tempo acaba (`--timeout`, de 5 minutos por padrão).

Os trilhos de segurança são os dos outros pagamentos: a confirmação ou `--sim`, `--simular` e o limite por operação do perfil, que vale para cada pagamento do lote, e não para o total. Também não há chave de idempotência: se o resultado do envio ficar incerto, confira `pagamento boleto listar` e `pagamento darf listar` antes de enviar de novo.

## Um lote com problemas

Antes de enviar, a CLI confere o lote inteiro, com as regras de cada tipo, e recusa campos desconhecidos ou de outro tipo. Havendo problemas, aponta cada pagamento com problema, pela linha (na planilha) ou pela posição (no JSON), e o campo, e não envia nada.

O Excel, ao abrir um CSV, converte o que parece número ou data: números longos viram notação científica e perdem dígitos, códigos perdem os zeros à esquerda (`0220` vira `220`) e datas mudam de formato. Esta planilha, `excel.csv`, foi aberta e salva no Excel:

<!-- guia: arquivo excel.csv -->
```csv
tipoPagamento;codBarraLinhaDigitavel;dataVencimento;cnpjCpf;nomeEmpresa;codigoReceita;periodoApuracao;referencia;descricao;valorPrincipal
BOLETO;7,79716E+42;;;;;;;;
BOLETO;83640000002-9 48370101202-1 60901000000-6 00001234567-4;28/09/2026;;;;;;;
DARF;;25/09/2026;11.444.777/0001-61;Empresa Exemplo Ltda;8109;31/08/2026;1,36094E+16;PIS de agosto;267,49
```

```console
$ inter-pj pagamento lote enviar --arquivo excel.csv
erro: excel.csv: 3 pagamentos com problema; nada foi enviado:
  linha 2, campo "codBarraLinhaDigitavel": "7,79716E+42" está em notação científica, como o Excel mostra números longos, e os dígitos se perderam; formate a coluna como texto e digite de novo
  linha 3, campo "dataVencimento": data inválida "28/09/2026": use AAAA-MM-DD
  linha 4, campo "referencia": "1,36094E+16" está em notação científica, como o Excel mostra números longos, e os dígitos se perderam; formate a coluna como texto e digite de novo
```

Para editar a planilha no Excel, importe o arquivo (Dados > De Texto/CSV) sem detectar os tipos de dados, ou formate as colunas como texto antes de digitar; linhas digitáveis e CPF ou CNPJ com pontuação, como no modelo, já ficam como texto. A planilha pode ser separada por `,` ou `;`, em UTF-8, com ou sem BOM, e os valores podem ter vírgula (`267,49`).

Em JSON, o arquivo é um objeto com os `pagamentos` e, se quiser, o `meuIdentificador`, que `--identificador` substitui; ou só a lista dos pagamentos, como em `outubro.json`. Nele, o boleto do fornecedor aparece duas vezes:

<!-- guia: arquivo outubro.json -->
```json
[
  {
    "tipoPagamento": "BOLETO",
    "codBarraLinhaDigitavel": "07790.00017 23456.700006 00000.016022 7 16000000189000"
  },
  {
    "tipoPagamento": "BOLETO",
    "codBarraLinhaDigitavel": "07797160000001890000000123456700000000001602"
  }
]
```

```console
$ inter-pj pagamento lote enviar --arquivo outubro.json
erro: pagamentos repetidos em outubro.json:
  pagamento 1 e pagamento 2: o mesmo pagamento aparece mais de uma vez; confira se não é um pagamento em dobro
dica: se forem mesmo pagamentos distintos, use --permitir-repetidos
```

Um pagamento repetido no arquivo é recusado: o mesmo código, na linha digitável ou no código de barras, ou um DARF com o mesmo contribuinte, receita, período, referência e total. Com `--sim`, um aviso não impediria o pagamento em dobro. Se forem mesmo pagamentos distintos, use `--permitir-repetidos`.
