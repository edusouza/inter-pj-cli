# Pagamentos

Boletos, contas de consumo e tributos com código de barras são pagos pelo código. Pagar tira dinheiro da conta, e os trilhos de segurança são os do [Pix](pix.md): o resumo com o valor por extenso, a confirmação num terminal (ou `--sim`), `--simular` e o limite por operação do perfil. Diferente do Pix, esta API não tem chave de idempotência, então um resultado incerto pede uma conferência antes de qualquer nova tentativa.

Os exemplos são da Empresa Exemplo Ltda, uma empresa fictícia, no perfil de produção. Pagar precisa do escopo `pagamento-boleto.write`, e listar, do `pagamento-boleto.read`.

- [Pagar um boleto](#pagar-um-boleto)
- [Contas e tributos](#contas-e-tributos)
- [Vencidos, juros e descontos](#vencidos-juros-e-descontos)
- [Agendar e conferir o beneficiário](#agendar-e-conferir-o-beneficiário)
- [Conferir sem pagar](#conferir-sem-pagar)
- [Quando o resultado fica incerto](#quando-o-resultado-fica-incerto)
- [Os pagamentos feitos](#os-pagamentos-feitos)
- [Cancelar um agendamento](#cancelar-um-agendamento)

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
