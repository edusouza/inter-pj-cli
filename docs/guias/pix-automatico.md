# Pix Automático

No Pix Automático, o pagador autoriza uma vez, no banco dele, as cobranças de um contrato (uma mensalidade, uma assinatura, um plano), que depois são feitas a cada vencimento sem que ele precise pagar uma a uma. A autorização é a **recorrência**: o devedor e o contrato, a periodicidade, o valor (fixo, um mínimo para o limite que o pagador define, ou o de cada cobrança) e se as cobranças não pagas podem ser tentadas de novo. O pagador aprova a recorrência no banco dele, e cada pagamento é então uma cobrança recorrente, que o banco do pagador debita no vencimento.

A API é só para CNPJs com pelo menos 6 meses de atividade. Os exemplos são da Empresa Exemplo Ltda, uma empresa fictícia, no perfil de produção. Criar, alterar e cancelar uma recorrência precisam do escopo `rec.write`, e consultar e listar, do `rec.read`.

- [Criar uma recorrência](#criar-uma-recorrência)
- [Por um arquivo](#por-um-arquivo)
- [Quando o resultado fica incerto](#quando-o-resultado-fica-incerto)
- [As recorrências de um período](#as-recorrências-de-um-período)
- [Uma recorrência aprovada](#uma-recorrência-aprovada)
- [Alterar uma recorrência](#alterar-uma-recorrência)
- [Cancelar uma recorrência](#cancelar-uma-recorrência)
- [Pedir a aprovação](#pedir-a-aprovação)
- [As cobranças recorrentes](#as-cobranças-recorrentes)
- [O QR Code da recorrência](#o-qr-code-da-recorrência)

## Criar uma recorrência

O Sicrano de Tal contratou o plano mensal da empresa, de R$ 149,90, a partir de outubro. Com `--retentativas`, uma cobrança que ele não pagar no vencimento pode ser tentada de novo até 3 vezes, em dias diferentes, em até 7 dias:

```console
$ inter-pj pix-automatico rec criar --devedor-documento 119.000.000-83 --devedor-nome "Sicrano de Tal" \
    --contrato plano-mensal-0107 --objeto "Plano mensal" --data-inicial 2026-10-10 --periodicidade mensal \
    --valor 149,90 --retentativas
*** PRODUÇÃO: a recorrência vale de verdade ***
Recorrência a criar
  Ambiente       PRODUÇÃO (conta real)
  Devedor        Sicrano de Tal (119.000.000-83)
  Contrato       plano-mensal-0107
  Objeto         Plano mensal
  Periodicidade  mensal, a partir de 10/10/2026, sem fim
  Valor          R$ 149,90 (cento e quarenta e nove reais e noventa centavos) em cada pagamento
  Retentativas   até 3 novas tentativas, em 7 dias
Criar a recorrência? [s/N] s
Recorrência criada: aguarda a aprovação do pagador.

Recorrência RR1234567820260924Qm4Tz8Kd2Wb
  Status         criada (aguarda a aprovação do pagador)
  Devedor        Sicrano de Tal (119.000.000-83)
  Contrato       plano-mensal-0107
  Objeto         Plano mensal
  Periodicidade  mensal, a partir de 10/10/2026, sem fim
  Valor          R$ 149,90 em cada pagamento
  Retentativas   até 3 novas tentativas, em 7 dias
  Recebedor      Empresa Exemplo Ltda (11.444.777/0001-61)

Histórico
  24/09/2026 10:20:00  criada

Acompanhe com: inter-pj pix-automatico rec consultar RR1234567820260924Qm4Tz8Kd2Wb
O pagador aprova no banco dele: peça com inter-pj pix-automatico solicitacao criar --rec RR1234567820260924Qm4Tz8Kd2Wb
```

A recorrência criada aguarda a aprovação do pagador, e a CLI mostra como pedi-la. O resumo e a confirmação vêm antes do envio, e os tamanhos, as datas e os valores são conferidos antes de qualquer requisição: a data do primeiro pagamento não pode ter passado. `--data-final` é a do último pagamento (sem ela, a recorrência não tem fim), e `--periodicidade` é `semanal`, `mensal`, `trimestral`, `semestral` ou `anual`. `--valor` fixa o valor de cada pagamento; `--valor-minimo`, quando o valor muda a cada cobrança, é o menor limite que o pagador pode definir no banco dele; sem nenhum dos dois, vale o valor de cada cobrança. `--simular` mostra a requisição sem enviar nada.

## Por um arquivo

A recorrência também pode vir de um arquivo JSON com os campos da API. `rec modelo` imprime um exemplo, de dados fictícios, que começa em 30 dias e dura um ano:

```console
$ inter-pj pix-automatico rec modelo
{
  "vinculo": {
    "objeto": "Mensalidade do plano",
    "devedor": {"cpf": "123.456.789-09", "nome": "Cliente Exemplo"},
    "contrato": "contrato-001"
  },
  "calendario": {"dataInicial": "2026-10-24", "dataFinal": "2027-09-24", "periodicidade": "MENSAL"},
  "valor": {"valorRec": "149,90"},
  "politicaRetentativa": "PERMITE_3R_7D"
}
```

Em setembro, a Cliente Exemplo Ltda recusou o contrato de suporte de valor fixo. O contrato novo tem o valor de cada mês, que segue as horas de suporte, com um mínimo de R$ 500,00 para o limite que ela define no banco, e está no arquivo `suporte.json`:

<!-- guia: arquivo suporte.json -->
```json
{
  "vinculo": {
    "objeto": "Suporte técnico",
    "devedor": {"cnpj": "11.222.333/0001-81", "nome": "Cliente Exemplo Ltda"},
    "contrato": "suporte-2026-007"
  },
  "calendario": {"dataInicial": "2026-10-05", "dataFinal": "2027-09-05", "periodicidade": "MENSAL"},
  "valor": {"valorMinimoRecebedor": "500,00"},
  "politicaRetentativa": "NAO_PERMITE"
}
```

Os campos desconhecidos são recusados, e as mensagens de erro apontam o campo. `--arquivo -` lê a recorrência da entrada padrão.

## Quando o resultado fica incerto

A criação pelo arquivo não recebe resposta:

```console
$ inter-pj pix-automatico rec criar --arquivo suporte.json
*** PRODUÇÃO: a recorrência vale de verdade ***
Recorrência a criar
  Ambiente       PRODUÇÃO (conta real)
  Devedor        Cliente Exemplo Ltda (11.222.333/0001-81)
  Contrato       suporte-2026-007
  Objeto         Suporte técnico
  Periodicidade  mensal, de 05/10/2026 a 05/09/2027
  Valor          o de cada cobrança; o limite do pagador é de pelo menos R$ 500,00
  Retentativas   não permitidas
Criar a recorrência? [s/N] s
erro: POST /pix/v2/rec respondeu 504 (tempo esgotado no gateway)
dica: a recorrência pode ter sido criada, e esta API não tem chave de idempotência: repetir o comando pode criar outra
dica: confira antes de tentar de novo: inter-pj pix-automatico rec listar --documento 11222333000181
```

Um tempo esgotado ou um erro 5xx depois do envio deixam o resultado incerto: a recorrência pode ter sido criada. Esta API não tem chave de idempotência, e repetir o comando pode criar outra; por isso a CLI sai com o código 9 e mostra como conferir. O comando da dica, aqui com o período deste mês:

```console
$ inter-pj pix-automatico rec listar --documento 11.222.333/0001-81 --inicio 2026-09-01 --fim 2026-09-24
Recorrências criadas de 01/09/2026 00:00 a 24/09/2026 23:59 (devedor 11.222.333/0001-81)

Status     Devedor               Periodicidade  Início            Valor     Mínimo  idRec
rejeitada  Cliente Exemplo Ltda  mensal         05/10/2026  R$ 1.200,00             RN1234567820260910m2Hc6Vy8Qd1
criada     Cliente Exemplo Ltda  mensal         05/10/2026               R$ 500,00  RN1234567820260924Hx7Rn3Vp5Lc

2 recorrências · 1 criada · 1 rejeitada
```

A recorrência nova, com o valor mínimo, foi criada: não há o que repetir.

## As recorrências de um período

`rec listar` mostra as recorrências criadas num período, por padrão os últimos 30 dias até agora:

```console
$ inter-pj pix-automatico rec listar --inicio 2026-09-01 --fim 2026-09-24
Recorrências criadas de 01/09/2026 00:00 a 24/09/2026 23:59

Status     Devedor               Periodicidade  Início            Valor     Mínimo  idRec
aprovada   Fulano de Tal         mensal         10/09/2026     R$ 89,90             RR1234567820260901k7Tq2Wm9Zp4
rejeitada  Cliente Exemplo Ltda  mensal         05/10/2026  R$ 1.200,00             RN1234567820260910m2Hc6Vy8Qd1
criada     Beltrana de Tal       mensal         10/11/2026    R$ 405,00             RR1234567820260920p5Jx3Ls7Gv0
criada     Sicrano de Tal        mensal         10/10/2026    R$ 149,90             RR1234567820260924Qm4Tz8Kd2Wb
criada     Cliente Exemplo Ltda  mensal         05/10/2026               R$ 500,00  RN1234567820260924Hx7Rn3Vp5Lc

5 recorrências · 3 criadas · 1 aprovada · 1 rejeitada
```

Os filtros são o status (`--status criada|aprovada|rejeitada|expirada|cancelada`), o devedor (`--documento`), a location (`--com-location`, `--sem-location`) e o convênio (`--convenio`). A listagem sai também em `--json` e em `--formato csv`, com os campos da API.

## Uma recorrência aprovada

O Fulano de Tal aprovou o plano básico em setembro. `rec consultar` mostra a recorrência, o pagador que a aprovou, como ele aprovou, o histórico e as solicitações de confirmação enviadas ao banco dele:

```console
$ inter-pj pix-automatico rec consultar RR1234567820260901k7Tq2Wm9Zp4
Recorrência RR1234567820260901k7Tq2Wm9Zp4
  Status         aprovada (ativa)
  Devedor        Fulano de Tal (123.456.789-09)
  Contrato       plano-basico-0042
  Objeto         Plano básico
  Periodicidade  mensal, a partir de 10/09/2026, sem fim
  Valor          R$ 89,90 em cada pagamento
  Retentativas   até 3 novas tentativas, em 7 dias
  Recebedor      Empresa Exemplo Ltda (11.444.777/0001-61)
  Pagador        123.456.789-09 (ISPB do banco 87654321)
  Ativação       pedido ao banco do pagador (solicitação de confirmação)

Histórico
  01/09/2026 10:10:00  criada
  02/09/2026 08:42:17  aprovada

Solicitações de confirmação
  SC1234567820260901h3Rw8Kd5Nb2  aceita pelo pagador; expira em 07/09/2026 23:59:59
```

## Alterar uma recorrência

O Sicrano de Tal pediu para pagar no dia 15. Antes da aprovação, `rec revisar` muda a data do primeiro pagamento (`--data-inicial`) e a cobrança de ativação (`--txid-ativacao`); o nome do devedor (`--devedor-nome`) e a location (`--loc`) mudam também depois. O resumo mostra o antes e o depois:

```console
$ inter-pj pix-automatico rec revisar RR1234567820260924Qm4Tz8Kd2Wb --data-inicial 2026-10-15
Recorrência RR1234567820260924Qm4Tz8Kd2Wb a alterar
  Ambiente            PRODUÇÃO (conta real)
  Status              criada (aguarda a aprovação do pagador)
  Devedor             Sicrano de Tal
  Primeiro pagamento  10/10/2026 → 15/10/2026
Alterar a recorrência? [s/N] s
Recorrência alterada.

Recorrência RR1234567820260924Qm4Tz8Kd2Wb
  Status         criada (aguarda a aprovação do pagador)
  Devedor        Sicrano de Tal (119.000.000-83)
  Contrato       plano-mensal-0107
  Objeto         Plano mensal
  Periodicidade  mensal, a partir de 15/10/2026, sem fim
  Valor          R$ 149,90 em cada pagamento
  Retentativas   até 3 novas tentativas, em 7 dias
  Recebedor      Empresa Exemplo Ltda (11.444.777/0001-61)

Histórico
  24/09/2026 10:20:00  criada
```

Depois da aprovação, a data do primeiro pagamento não muda mais, e a CLI recusa a alteração sem enviar nada:

```console
$ inter-pj pix-automatico rec revisar RR1234567820260901k7Tq2Wm9Zp4 --data-inicial 2026-10-10 --sim
erro: a recorrência já foi aprovada: a data do primeiro pagamento e a cobrança de ativação não mudam mais
```

## Cancelar uma recorrência

O valor de uma recorrência não muda depois de criada. A da Beltrana de Tal foi criada com R$ 405,00, e não com os R$ 450,00 da mensalidade dela: a recorrência é cancelada e criada de novo. `rec cancelar` mostra a recorrência e pede confirmação:

```console
$ inter-pj pix-automatico rec cancelar RR1234567820260920p5Jx3Ls7Gv0
Recorrência RR1234567820260920p5Jx3Ls7Gv0 a cancelar
  Ambiente       PRODUÇÃO (conta real)
  Status         criada (aguarda a aprovação do pagador)
  Devedor        Beltrana de Tal (012.345.678-90)
  Contrato       mensalidade-beltrana-2026
  Periodicidade  mensal, a partir de 10/11/2026, sem fim
  Valor          R$ 405,00 em cada pagamento
Cancelar a recorrência? [s/N] s
Recorrência cancelada: ela não aceita mais cobranças.

Recorrência RR1234567820260920p5Jx3Ls7Gv0
  Status         cancelada
  Devedor        Beltrana de Tal (012.345.678-90)
  Contrato       mensalidade-beltrana-2026
  Objeto         Mensalidade
  Periodicidade  mensal, a partir de 10/11/2026, sem fim
  Valor          R$ 405,00 em cada pagamento
  Retentativas   até 3 novas tentativas, em 7 dias
  Recebedor      Empresa Exemplo Ltda (11.444.777/0001-61)
  Encerramento   cancelada pelo recebedor: SLCR, Cancelamento solicitado pelo usuário recebedor

Histórico
  20/09/2026 11:05:33  criada
  24/09/2026 10:30:00  cancelada
```

```console
$ inter-pj pix-automatico rec criar --devedor-documento 012.345.678-90 --devedor-nome "Beltrana de Tal" \
    --contrato mensalidade-beltrana-2026 --objeto Mensalidade --data-inicial 2026-11-10 --periodicidade mensal \
    --valor 450,00 --retentativas
*** PRODUÇÃO: a recorrência vale de verdade ***
Recorrência a criar
  Ambiente       PRODUÇÃO (conta real)
  Devedor        Beltrana de Tal (012.345.678-90)
  Contrato       mensalidade-beltrana-2026
  Objeto         Mensalidade
  Periodicidade  mensal, a partir de 10/11/2026, sem fim
  Valor          R$ 450,00 (quatrocentos e cinquenta reais) em cada pagamento
  Retentativas   até 3 novas tentativas, em 7 dias
Criar a recorrência? [s/N] s
Recorrência criada: aguarda a aprovação do pagador.

Recorrência RR1234567820260924Bw2Jy6Fs9Nt
  Status         criada (aguarda a aprovação do pagador)
  Devedor        Beltrana de Tal (012.345.678-90)
  Contrato       mensalidade-beltrana-2026
  Objeto         Mensalidade
  Periodicidade  mensal, a partir de 10/11/2026, sem fim
  Valor          R$ 450,00 em cada pagamento
  Retentativas   até 3 novas tentativas, em 7 dias
  Recebedor      Empresa Exemplo Ltda (11.444.777/0001-61)

Histórico
  24/09/2026 10:35:00  criada

Acompanhe com: inter-pj pix-automatico rec consultar RR1234567820260924Bw2Jy6Fs9Nt
O pagador aprova no banco dele: peça com inter-pj pix-automatico solicitacao criar --rec RR1234567820260924Bw2Jy6Fs9Nt
```

Uma recorrência rejeitada, expirada ou cancelada não muda mais:

```console
$ inter-pj pix-automatico rec cancelar RN1234567820260910m2Hc6Vy8Qd1 --sim
erro: a recorrência já está rejeitada pelo pagador: não há o que cancelar
```

## Pedir a aprovação

O pagador aprova a recorrência no banco dele. Para que o banco lhe peça isso, a empresa envia uma **solicitação de confirmação**, com a conta do pagador. A CLI consulta a recorrência antes e mostra o que o pagador vai aprovar:

```console
$ inter-pj pix-automatico solicitacao criar --rec RR1234567820260924Qm4Tz8Kd2Wb \
    --documento 119.000.000-83 --ispb 87654321 --agencia 0001 --conta 1234567 --expiracao 2026-10-01
*** PRODUÇÃO: o pedido chega ao pagador de verdade ***
Solicitação de confirmação a enviar
  Ambiente          PRODUÇÃO (conta real)
  Recorrência       RR1234567820260924Qm4Tz8Kd2Wb
  Devedor           Sicrano de Tal (119.000.000-83)
  Contrato          plano-mensal-0107
  Objeto            Plano mensal
  Periodicidade     mensal, a partir de 15/10/2026, sem fim
  Valor             R$ 149,90 em cada pagamento
  Conta do pagador  119.000.000-83, banco com ISPB 87654321, agência 0001, conta 1234567
  Expira em         01/10/2026 23:59:59
Enviar a solicitação ao banco do pagador? [s/N] s
Solicitação criada: o banco do pagador vai pedir que ele aprove a recorrência.

Solicitação de confirmação SC1234567820260924Tq6Wn2Hy8Kd
  Status            criada (aguarda o envio)
  Recorrência       RR1234567820260924Qm4Tz8Kd2Wb
  Conta do pagador  119.000.000-83, banco com ISPB 87654321, agência 0001, conta 1234567
  Expira em         01/10/2026 23:59:59
  Devedor           Sicrano de Tal (119.000.000-83)
  Contrato          plano-mensal-0107
  Objeto            Plano mensal
  Periodicidade     mensal, a partir de 15/10/2026, sem fim
  Valor             R$ 149,90 em cada pagamento

Histórico
  24/09/2026 10:40:00  criada (aguarda o envio)

Acompanhe com: inter-pj pix-automatico solicitacao consultar SC1234567820260924Tq6Wn2Hy8Kd
ou pela recorrência: inter-pj pix-automatico rec consultar RR1234567820260924Qm4Tz8Kd2Wb
```

`--ispb` é o código de 8 dígitos do banco do pagador (o do Inter é 00416968), `--conta` vai com o dígito verificador, e `--expiracao` é o prazo para ele responder: `2h`, `7d`, uma data, até o fim do dia, ou data e hora com fuso; sem ela, 7 dias. Os escopos são `solicrec.write` e `solicrec.read`, além do `rec.read`, com que a CLI consulta a recorrência antes de criar a solicitação. Como na recorrência, não há chave de idempotência: um resultado incerto vem com o comando que confere se a solicitação foi enviada.

O banco do pagador recebe a solicitação, e ela fica à espera da resposta dele:

```console
$ inter-pj pix-automatico solicitacao consultar SC1234567820260924Tq6Wn2Hy8Kd
Solicitação de confirmação SC1234567820260924Tq6Wn2Hy8Kd
  Status            recebida pelo pagador
  Recorrência       RR1234567820260924Qm4Tz8Kd2Wb
  Conta do pagador  119.000.000-83, banco com ISPB 87654321, agência 0001, conta 1234567
  Expira em         01/10/2026 23:59:59
  Devedor           Sicrano de Tal (119.000.000-83)
  Contrato          plano-mensal-0107
  Objeto            Plano mensal
  Periodicidade     mensal, a partir de 15/10/2026, sem fim
  Valor             R$ 149,90 em cada pagamento

Histórico
  24/09/2026 10:40:00  criada (aguarda o envio)
  24/09/2026 10:40:02  enviada ao pagador
  24/09/2026 10:40:05  recebida pelo pagador
```

O Sicrano de Tal avisou que a conta dele é outra. Uma solicitação criada ou recebida, ainda sem resposta, pode ser cancelada:

```console
$ inter-pj pix-automatico solicitacao cancelar SC1234567820260924Tq6Wn2Hy8Kd
Solicitação SC1234567820260924Tq6Wn2Hy8Kd a cancelar
  Ambiente          PRODUÇÃO (conta real)
  Status            recebida pelo pagador
  Recorrência       RR1234567820260924Qm4Tz8Kd2Wb
  Conta do pagador  119.000.000-83, banco com ISPB 87654321, agência 0001, conta 1234567
Cancelar a solicitação? [s/N] s
Solicitação cancelada.

Solicitação de confirmação SC1234567820260924Tq6Wn2Hy8Kd
  Status            cancelada
  Recorrência       RR1234567820260924Qm4Tz8Kd2Wb
  Conta do pagador  119.000.000-83, banco com ISPB 87654321, agência 0001, conta 1234567
  Expira em         01/10/2026 23:59:59
  Devedor           Sicrano de Tal (119.000.000-83)
  Contrato          plano-mensal-0107
  Objeto            Plano mensal
  Periodicidade     mensal, a partir de 15/10/2026, sem fim
  Valor             R$ 149,90 em cada pagamento

Histórico
  24/09/2026 10:40:00  criada (aguarda o envio)
  24/09/2026 10:40:02  enviada ao pagador
  24/09/2026 10:40:05  recebida pelo pagador
  24/09/2026 10:45:00  cancelada
```

A nova, com a conta certa:

```console
$ inter-pj pix-automatico solicitacao criar --rec RR1234567820260924Qm4Tz8Kd2Wb \
    --documento 119.000.000-83 --ispb 87654321 --agencia 0001 --conta 7654321 --expiracao 2026-10-01 --sim
*** PRODUÇÃO: o pedido chega ao pagador de verdade ***
Solicitação de confirmação a enviar
  Ambiente          PRODUÇÃO (conta real)
  Recorrência       RR1234567820260924Qm4Tz8Kd2Wb
  Devedor           Sicrano de Tal (119.000.000-83)
  Contrato          plano-mensal-0107
  Objeto            Plano mensal
  Periodicidade     mensal, a partir de 15/10/2026, sem fim
  Valor             R$ 149,90 em cada pagamento
  Conta do pagador  119.000.000-83, banco com ISPB 87654321, agência 0001, conta 7654321
  Expira em         01/10/2026 23:59:59
Solicitação criada: o banco do pagador vai pedir que ele aprove a recorrência.

Solicitação de confirmação SC1234567820260924Lp3Rv9Jc5Xs
  Status            criada (aguarda o envio)
  Recorrência       RR1234567820260924Qm4Tz8Kd2Wb
  Conta do pagador  119.000.000-83, banco com ISPB 87654321, agência 0001, conta 7654321
  Expira em         01/10/2026 23:59:59
  Devedor           Sicrano de Tal (119.000.000-83)
  Contrato          plano-mensal-0107
  Objeto            Plano mensal
  Periodicidade     mensal, a partir de 15/10/2026, sem fim
  Valor             R$ 149,90 em cada pagamento

Histórico
  24/09/2026 10:50:00  criada (aguarda o envio)

Acompanhe com: inter-pj pix-automatico solicitacao consultar SC1234567820260924Lp3Rv9Jc5Xs
ou pela recorrência: inter-pj pix-automatico rec consultar RR1234567820260924Qm4Tz8Kd2Wb
```

A consulta da recorrência mostra as solicitações dela:

```console
$ inter-pj pix-automatico rec consultar RR1234567820260924Qm4Tz8Kd2Wb
Recorrência RR1234567820260924Qm4Tz8Kd2Wb
  Status         criada (aguarda a aprovação do pagador)
  Devedor        Sicrano de Tal (119.000.000-83)
  Contrato       plano-mensal-0107
  Objeto         Plano mensal
  Periodicidade  mensal, a partir de 15/10/2026, sem fim
  Valor          R$ 149,90 em cada pagamento
  Retentativas   até 3 novas tentativas, em 7 dias
  Recebedor      Empresa Exemplo Ltda (11.444.777/0001-61)

Histórico
  24/09/2026 10:20:00  criada

Solicitações de confirmação
  SC1234567820260924Tq6Wn2Hy8Kd  cancelada; expira em 01/10/2026 23:59:59
  SC1234567820260924Lp3Rv9Jc5Xs  recebida pelo pagador; expira em 01/10/2026 23:59:59
```

A resposta do pagador aparece no status da solicitação e no da recorrência. Aceita a solicitação, a recorrência fica aprovada, como a do Fulano de Tal:

```console
$ inter-pj pix-automatico solicitacao consultar SC1234567820260901h3Rw8Kd5Nb2
Solicitação de confirmação SC1234567820260901h3Rw8Kd5Nb2
  Status            aceita pelo pagador
  Recorrência       RR1234567820260901k7Tq2Wm9Zp4
  Conta do pagador  123.456.789-09, banco com ISPB 87654321, agência 0001, conta 1234567
  Expira em         07/09/2026 23:59:59
  Devedor           Fulano de Tal (123.456.789-09)
  Contrato          plano-basico-0042
  Objeto            Plano básico
  Periodicidade     mensal, a partir de 10/09/2026, sem fim
  Valor             R$ 89,90 em cada pagamento

Histórico
  01/09/2026 10:12:04  criada (aguarda o envio)
  01/09/2026 10:12:06  enviada ao pagador
  01/09/2026 10:12:09  recebida pelo pagador
  02/09/2026 08:42:17  aceita pelo pagador
```

Uma recorrência aprovada ou encerrada não recebe solicitação, e uma solicitação respondida não é cancelada. A CLI recusa as duas sem enviar nada:

```console
$ inter-pj pix-automatico solicitacao criar --rec RR1234567820260901k7Tq2Wm9Zp4 \
    --documento 123.456.789-09 --ispb 87654321 --agencia 0001 --conta 1234567 --expiracao 2026-10-01 --sim
erro: a recorrência já foi aprovada pelo pagador
$ inter-pj pix-automatico solicitacao cancelar SC1234567820260901h3Rw8Kd5Nb2 --sim
erro: a solicitação está aceita pelo pagador: só as criadas ou recebidas, ainda sem resposta, podem ser canceladas
```

## As cobranças recorrentes

Aprovada a recorrência, cada pagamento é uma **cobrança recorrente**, uma por ciclo, que o banco do pagador debita no vencimento. A cobrança de outubro do plano básico do Fulano de Tal, com um txid da empresa:

```console
$ inter-pj pix-automatico cobr criar --rec RR1234567820260901k7Tq2Wm9Zp4 --valor 89,90 \
    --vencimento 2026-10-10 --conta 1234567 --agencia 0001 --info "Plano básico de outubro" \
    --txid fulano0042outubro2026planobasico
*** PRODUÇÃO: o débito na conta do pagador é de verdade ***
Cobrança recorrente a criar
  Ambiente          PRODUÇÃO (conta real)
  Recorrência       RR1234567820260901k7Tq2Wm9Zp4
  Devedor           Fulano de Tal (123.456.789-09)
  Contrato          plano-basico-0042
  Objeto            Plano básico
  Valor             R$ 89,90 (oitenta e nove reais e noventa centavos)
  Vencimento        10/10/2026, ou o próximo dia útil
  Conta que recebe  conta corrente 1234567, agência 0001
  Informação        Plano básico de outubro
  Retentativas      até 3 novas tentativas, em 7 dias
  txid              fulano0042outubro2026planobasico
Criar a cobrança recorrente? [s/N] s
Cobrança recorrente criada: o banco do pagador agenda o débito para o vencimento.

Cobrança recorrente fulano0042outubro2026planobasico
  Status        criada (aguarda o banco do pagador)
  Recorrência   RR1234567820260901k7Tq2Wm9Zp4
  Valor         R$ 89,90
  Vencimento    10/10/2026, ou o próximo dia útil
  Criada em     24/09/2026
  Retentativas  até 3 novas tentativas, em 7 dias
  Recebedor     Empresa Exemplo Ltda (11.444.777/0001-61), conta corrente 1234567, agência 0001
  Informação    Plano básico de outubro

Histórico
  24/09/2026 10:55:00  criada

Acompanhe com: inter-pj pix-automatico cobr consultar fulano0042outubro2026planobasico
```

A CLI consulta a recorrência antes: um valor diferente do fixo dela e um vencimento fora do seu período viram avisos no resumo. `--conta` é a conta que recebe, com o dígito verificador (sem ela, a de `--conta-corrente`, que pode vir da configuração), `--tipo-conta` é `corrente` (o padrão), `poupanca` ou `pagamento`, e `--devedor-email`, `--devedor-endereco`, `--devedor-cidade`, `--devedor-uf` e `--devedor-cep` completam os dados do pagador, que é o da recorrência. Sem `--txid`, a CLI gera um; com o mesmo txid, a API não cria uma segunda cobrança, então um resultado incerto vem com o comando que a confere e com o txid para repetir sem risco. Criar e cancelar precisam do escopo `cobr.write`, e consultar e listar, do `cobr.read`, além do `rec.read` para a consulta da recorrência.

O banco do pagador agenda o débito. Um vencimento em fim de semana ou feriado, pelos feriados da cidade do pagador, vai para o próximo dia útil, a não ser com `--sem-ajuste-dia-util`: o dia 10 de outubro é um sábado, e o dia 12, feriado, então o débito fica para o dia 13:

```console
$ inter-pj pix-automatico cobr consultar fulano0042outubro2026planobasico
Cobrança recorrente fulano0042outubro2026planobasico
  Status        ativa (débito agendado)
  Recorrência   RR1234567820260901k7Tq2Wm9Zp4
  Valor         R$ 89,90
  Vencimento    10/10/2026, ou o próximo dia útil
  Criada em     24/09/2026
  Retentativas  até 3 novas tentativas, em 7 dias
  Recebedor     Empresa Exemplo Ltda (11.444.777/0001-61), conta corrente 1234567, agência 0001
  Informação    Plano básico de outubro

Tentativas de liquidação
Liquidação  Tipo         Status    endToEndId                        Motivo
13/10/2026  agendamento  agendada  E87654321202610130300Rw5Hn8Tc1Kz

Histórico
  24/09/2026 10:55:00  criada
  24/09/2026 10:55:05  ativa
```

Só uma recorrência aprovada pelo pagador aceita cobranças:

```console
$ inter-pj pix-automatico cobr criar --rec RR1234567820260924Qm4Tz8Kd2Wb --valor 149,90 \
    --vencimento 2026-10-15 --conta 1234567 --agencia 0001 --sim
erro: a recorrência está criada (aguarda a aprovação do pagador): só uma recorrência aprovada pelo pagador aceita cobranças
```

`cobr listar` mostra as cobranças criadas num período, por padrão os últimos 30 dias até agora, com os filtros de recorrência (`--rec`), devedor (`--documento`), status (`--status criada|ativa|concluida|expirada|rejeitada|cancelada`) e `--convenio`:

```console
$ inter-pj pix-automatico cobr listar --inicio 2026-09-01 --fim 2026-09-24
Cobranças recorrentes criadas de 01/09/2026 00:00 a 24/09/2026 23:59

Vencimento  Status       Valor  Recorrência                    txid
10/09/2026  expirada  R$ 89,90  RR1234567820260901k7Tq2Wm9Zp4  fulano0042setembro2026planobasico
10/10/2026  ativa     R$ 89,90  RR1234567820260901k7Tq2Wm9Zp4  fulano0042outubro2026planobasico

2 cobranças · R$ 179,80
```

A cobrança de setembro do Fulano de Tal não foi paga. `cobr consultar` mostra as tentativas de liquidação, com o motivo de cada recusa, e o histórico:

```console
$ inter-pj pix-automatico cobr consultar fulano0042setembro2026planobasico
Cobrança recorrente fulano0042setembro2026planobasico
  Status        expirada sem pagamento
  Recorrência   RR1234567820260901k7Tq2Wm9Zp4
  Valor         R$ 89,90
  Vencimento    10/09/2026, ou o próximo dia útil
  Criada em     03/09/2026
  Retentativas  até 3 novas tentativas, em 7 dias
  Recebedor     Empresa Exemplo Ltda (11.444.777/0001-61), conta corrente 1234567, agência 0001
  Informação    Plano básico de setembro

Tentativas de liquidação
Liquidação  Tipo            Status     endToEndId                        Motivo
10/09/2026  agendamento     rejeitada  E87654321202609100300Ac6Bq2Lx9Mz  AC06, Conta transacional do usuário pagador bloqueada
11/09/2026  nova tentativa  rejeitada  E87654321202609110300Hd4Wn7Ts2Ky  AC06, Conta transacional do usuário pagador bloqueada
14/09/2026  nova tentativa  rejeitada  E87654321202609140300Mv8Rc3Pz6Lf  AC06, Conta transacional do usuário pagador bloqueada
16/09/2026  nova tentativa  rejeitada  E87654321202609160300Qx1Gj5Nb8Wt  AC06, Conta transacional do usuário pagador bloqueada

Histórico
  03/09/2026 09:00:00  criada
  03/09/2026 09:00:05  ativa
  18/09/2026 00:00:00  expirada
```

Quando o débito falha e a recorrência permite novas tentativas, `cobr retentativa` pede uma, para outro dia, até 7 dias depois da liquidação prevista, e no máximo 3: a CLI confere a política da recorrência e o prazo antes de enviar, e avisa quando já há uma tentativa naquele dia ou quando as 3 já foram pedidas. Na cobrança de setembro, as 3 foram pedidas, e ela expirou:

```console
$ inter-pj pix-automatico cobr retentativa fulano0042setembro2026planobasico --data 2026-09-25 --sim
erro: a cobrança está expirada sem pagamento: não há o que tentar de novo
```

O Fulano de Tal vai pagar outubro na loja: a cobrança de outubro é cancelada, e a recorrência continua para os meses seguintes. `cobr cancelar` mostra a cobrança e pede confirmação; pelas regras do Banco Central, o cancelamento vale até as 22h do dia anterior à liquidação, e depois disso o resumo avisa que o banco pode recusá-lo:

```console
$ inter-pj pix-automatico cobr cancelar fulano0042outubro2026planobasico
Cobrança recorrente fulano0042outubro2026planobasico a cancelar
  Ambiente     PRODUÇÃO (conta real)
  Status       ativa (débito agendado)
  Recorrência  RR1234567820260901k7Tq2Wm9Zp4
  Valor        R$ 89,90
  Vencimento   10/10/2026, ou o próximo dia útil
Cancelar a cobrança recorrente? [s/N] s
Cobrança recorrente cancelada: o débito não será feito.

Cobrança recorrente fulano0042outubro2026planobasico
  Status        cancelada
  Recorrência   RR1234567820260901k7Tq2Wm9Zp4
  Valor         R$ 89,90
  Vencimento    10/10/2026, ou o próximo dia útil
  Criada em     24/09/2026
  Retentativas  até 3 novas tentativas, em 7 dias
  Recebedor     Empresa Exemplo Ltda (11.444.777/0001-61), conta corrente 1234567, agência 0001
  Informação    Plano básico de outubro
  Encerramento  cancelada pelo recebedor: SLCR, Cancelamento solicitado pelo usuário recebedor

Tentativas de liquidação
Liquidação  Tipo         Status     endToEndId                        Motivo
13/10/2026  agendamento  cancelada  E87654321202610130300Rw5Hn8Tc1Kz

Histórico
  24/09/2026 10:55:00  criada
  24/09/2026 10:55:05  ativa
  24/09/2026 11:00:00  cancelada
```

## O QR Code da recorrência

Além da solicitação de confirmação, o pagador pode aprovar a recorrência pelo QR Code dela, lido no app do banco. O QR Code leva a uma **location de recorrência**, criada antes e ligada à recorrência na criação (`rec criar --loc`) ou depois (`rec revisar --loc`). Criar e desvincular uma location precisam do escopo `payloadlocationrec.write`, e consultar e listar, do `payloadlocationrec.read`.

```console
$ inter-pj pix-automatico locrec criar
Location criada.

Location 8101
  Criada em    24/09/2026 11:05:00
  Location     qrcodepix.inter.example/qr/v2/rec/8101
  Recorrência  nenhuma

Use com: inter-pj pix-automatico rec criar ... --loc 8101
```

A Beltrana de Tal prefere aprovar a mensalidade pelo QR Code. A location vai para a recorrência dela, que aguarda a aprovação:

```console
$ inter-pj pix-automatico rec revisar RR1234567820260924Bw2Jy6Fs9Nt --loc 8101
Recorrência RR1234567820260924Bw2Jy6Fs9Nt a alterar
  Ambiente  PRODUÇÃO (conta real)
  Status    criada (aguarda a aprovação do pagador)
  Devedor   Beltrana de Tal
  Location  → 8101
Alterar a recorrência? [s/N] s
Recorrência alterada.

Recorrência RR1234567820260924Bw2Jy6Fs9Nt
  Status         criada (aguarda a aprovação do pagador)
  Devedor        Beltrana de Tal (012.345.678-90)
  Contrato       mensalidade-beltrana-2026
  Objeto         Mensalidade
  Periodicidade  mensal, a partir de 10/11/2026, sem fim
  Valor          R$ 450,00 em cada pagamento
  Retentativas   até 3 novas tentativas, em 7 dias
  Recebedor      Empresa Exemplo Ltda (11.444.777/0001-61)
  Location       qrcodepix.inter.example/qr/v2/rec/8101

Histórico
  24/09/2026 10:35:00  criada
```

A consulta da recorrência traz então o QR Code, com o código copia e cola dele. `--qrcode` o desenha no terminal, e `--qrcode-png` o grava numa imagem, para enviar à Beltrana:

```console
$ inter-pj pix-automatico rec consultar RR1234567820260924Bw2Jy6Fs9Nt --qrcode-png mensalidade-beltrana.png
Recorrência RR1234567820260924Bw2Jy6Fs9Nt
  Status         criada (aguarda a aprovação do pagador)
  Devedor        Beltrana de Tal (012.345.678-90)
  Contrato       mensalidade-beltrana-2026
  Objeto         Mensalidade
  Periodicidade  mensal, a partir de 10/11/2026, sem fim
  Valor          R$ 450,00 em cada pagamento
  Retentativas   até 3 novas tentativas, em 7 dias
  Recebedor      Empresa Exemplo Ltda (11.444.777/0001-61)
  Location       qrcodepix.inter.example/qr/v2/rec/8101

Histórico
  24/09/2026 10:35:00  criada

Copia e cola  00020126180014br.gov.bcb.pix5204000053039865802BR5920EMPRESA EXEMPLO LTDA6014BELO HORIZONTE62070503***80600014br.gov.bcb.pix2538qrcodepix.inter.example/qr/v2/rec/81016304C2C7
QR Code salvo em mensalidade-beltrana.png (25,9 KB)
```

O QR Code de uma recorrência rejeitada, expirada ou cancelada não serve mais, e a CLI não o desenha.

`locrec listar` mostra as locations criadas num período, com a recorrência de cada uma; os filtros são `--com-recorrencia`, `--sem-recorrencia` e `--convenio`:

```console
$ inter-pj pix-automatico locrec listar --inicio 2026-09-01 --fim 2026-09-24
Locations de recorrências criadas de 01/09/2026 00:00 a 24/09/2026 23:59

Criada em              id  Recorrência                    Location
10/09/2026 14:31:02  8100  RN1234567820260910m2Hc6Vy8Qd1  qrcodepix.inter.example/qr/v2/rec/8100
24/09/2026 11:05:00  8101  RR1234567820260924Bw2Jy6Fs9Nt  qrcodepix.inter.example/qr/v2/rec/8101

2 locations · 2 com recorrência
```

A location 8100 é a do contrato de suporte que a Cliente Exemplo Ltda recusou em setembro. A recorrência rejeitada não muda mais, mas o QR Code ainda leva a ela. `locrec consultar` mostra uma location:

```console
$ inter-pj pix-automatico locrec consultar 8100
Location 8100
  Criada em    10/09/2026 14:31:02
  Location     qrcodepix.inter.example/qr/v2/rec/8100
  Recorrência  RN1234567820260910m2Hc6Vy8Qd1
```

`locrec desvincular` solta a recorrência da location, depois de mostrá-las e pedir confirmação. O QR Code deixa de levar à recorrência, que continua como está, sem a location:

```console
$ inter-pj pix-automatico locrec desvincular 8100
Location 8100 a desvincular
  Ambiente     PRODUÇÃO (conta real)
  Location     qrcodepix.inter.example/qr/v2/rec/8100
  Recorrência  RN1234567820260910m2Hc6Vy8Qd1
aviso: o QR Code desta location deixa de levar à recorrência RN1234567820260910m2Hc6Vy8Qd1, que continua como está
Desvincular a recorrência? [s/N] s
Recorrência RN1234567820260910m2Hc6Vy8Qd1 desvinculada: a location está livre.

Location 8100
  Criada em    10/09/2026 14:31:02
  Location     qrcodepix.inter.example/qr/v2/rec/8100
  Recorrência  nenhuma
```

A location livre serve a outra recorrência, com `--loc`, e o QR Code dela, se já foi impresso ou enviado, passa então a levar à nova. Uma location sem recorrência não tem o que desvincular:

```console
$ inter-pj pix-automatico locrec desvincular 8100 --sim
erro: a location 8100 não tem recorrência vinculada: não há o que desvincular
```
