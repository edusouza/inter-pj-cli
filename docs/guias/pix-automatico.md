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
