# Pix

Enviar um Pix tira dinheiro da conta. A CLI confere antes tudo o que pode, mostra um resumo e só envia depois de uma confirmação; depois, o Pix é acompanhado pelo código da solicitação. Enviar precisa do escopo `pagamento-pix.write`, e consultar, do `pagamento-pix.read`. Os Pix que a conta recebeu, e as devoluções deles, estão no fim do guia.

Os exemplos são da Empresa Exemplo Ltda, uma empresa fictícia, no perfil de produção. Nessa conta, um Pix acima de R$ 10.000,00 espera a aprovação de outra pessoa no Internet Banking, e o perfil tem um limite de R$ 20.000,00 por operação. A [introdução dos guias](README.md) explica como os exemplos são conferidos.

- [Enviar por chave](#enviar-por-chave)
- [O resumo e a confirmação](#o-resumo-e-a-confirmação)
- [Copia e cola](#copia-e-cola)
- [Dados bancários](#dados-bancários)
- [Agendar](#agendar)
- [Conferir sem enviar](#conferir-sem-enviar)
- [Os limites](#os-limites)
- [Quando a resposta se perde](#quando-a-resposta-se-perde)
- [Acompanhar um Pix enviado](#acompanhar-um-pix-enviado)
- [Pix recebidos](#pix-recebidos)
- [Devolver um Pix recebido](#devolver-um-pix-recebido)

## Enviar por chave

```console
$ inter-pj pix enviar --chave financeiro@fornecedor.example --valor 1.200,00 --descricao "NF 2026-0915"
*** PRODUÇÃO: este Pix movimenta dinheiro da conta real ***
Pix a enviar
  Ambiente               PRODUÇÃO (conta real)
  Chave Pix              financeiro@fornecedor.example (e-mail)
  Valor                  R$ 1.200,00 (mil e duzentos reais)
  Quando                 agora
  Descrição              NF 2026-0915
  Chave de idempotência  177f6739-716c-42fe-839a-9d2bbe9f54a6
Enviar o Pix? [s/N] s
Pix enviado.
Código da solicitação  c42f0787-02cb-4b31-827e-459ec9d7ece1
Data do pagamento      24/09/2026
Data da operação       24/09/2026
Chave de idempotência  177f6739-716c-42fe-839a-9d2bbe9f54a6

Acompanhe com: inter-pj pix consultar c42f0787-02cb-4b31-827e-459ec9d7ece1 --aguardar
```

A chave pode ser um e-mail, um CPF ou um CNPJ (com ou sem a pontuação), um celular (`+55`, o DDD e o número) ou uma chave aleatória. A CLI a confere antes de qualquer requisição, com os dígitos verificadores do CPF e do CNPJ. O valor aceita `1.200,00`, `1200,00` e `1200.00`, e uma forma ambígua como `1.200` é recusada. A descrição tem até 140 caracteres.

Uma chave que ninguém cadastrou é recusada pelo banco, e o erro traz a explicação dele:

```console
$ inter-pj pix enviar --chave financeiro@fornecedr.example --valor 100,00 --sim
*** PRODUÇÃO: este Pix movimenta dinheiro da conta real ***
Pix a enviar
  Ambiente               PRODUÇÃO (conta real)
  Chave Pix              financeiro@fornecedr.example (e-mail)
  Valor                  R$ 100,00 (cem reais)
  Quando                 agora
  Chave de idempotência  48ed3019-cfb7-4b05-9da6-3e8f0e2035be
erro: POST /banking/v2/pix respondeu 400 (requisição inválida): Chave Pix não encontrada — A chave informada não está cadastrada no DICT.
```

## O resumo e a confirmação

O resumo mostra o destino, o valor por extenso, quando e o ambiente; em produção, com um aviso em destaque. A pergunta tem o não como padrão: só `s` ou `sim` enviam. E a resposta precisa vir de um terminal, com o resumo à vista. Num script, sem ninguém para lê-lo, a CLI recusa sem enviar nada:

```console
$ inter-pj pix enviar --chave financeiro@fornecedor.example --valor 1.200,00
*** PRODUÇÃO: este Pix movimenta dinheiro da conta real ***
Pix a enviar
  Ambiente               PRODUÇÃO (conta real)
  Chave Pix              financeiro@fornecedor.example (e-mail)
  Valor                  R$ 1.200,00 (mil e duzentos reais)
  Quando                 agora
  Chave de idempotência  d1290880-4fc0-48fa-b976-626a417f724d
erro: confirmação necessária: execute em um terminal, sem redirecionar a entrada nem a saída de erros, para ver o resumo e responder; ou use --sim para confirmar sem perguntar
```

Num script, `--sim` confirma sem perguntar, e `--json` traz o resultado com os nomes de campo da API e a chave de idempotência. O resumo continua na saída de erros, para o log:

```console
$ inter-pj pix enviar --chave financeiro@fornecedor.example --valor 1.200,00 --sim --json
*** PRODUÇÃO: este Pix movimenta dinheiro da conta real ***
Pix a enviar
  Ambiente               PRODUÇÃO (conta real)
  Chave Pix              financeiro@fornecedor.example (e-mail)
  Valor                  R$ 1.200,00 (mil e duzentos reais)
  Quando                 agora
  Chave de idempotência  1bc1dcd9-3027-45c4-9a35-bae3d27f551d
{
  "codigoSolicitacao": "5b9e2d14-7a3c-4f08-9e61-2c8d4a7b3f90",
  "dataOperacao": "2026-09-24",
  "dataPagamento": "2026-09-24",
  "idIdempotente": "1bc1dcd9-3027-45c4-9a35-bae3d27f551d",
  "tipoRetorno": "PROCESSADO"
}
```

## Copia e cola

`--copia-e-cola` paga um código Pix copia e cola, como o de uma fatura. A CLI lê o código e confere o CRC antes de tudo, e o resumo mostra o que ele diz: se é estático ou dinâmico, o recebedor e a cidade, a chave ou o endereço da cobrança, o identificador e a mensagem. O valor vem do código:

```console
$ inter-pj pix enviar --copia-e-cola '00020126510014br.gov.bcb.pix0114123456780001950211Pedido 10515204000053039865406350.005802BR5921Fornecedor Exemplo SA6009SAO PAULO62140510NF2026081563049524'
*** PRODUÇÃO: este Pix movimenta dinheiro da conta real ***
Pix a enviar
  Ambiente               PRODUÇÃO (conta real)
  Copia e cola           estático
  Recebedor              Fornecedor Exemplo SA (SAO PAULO)
  Chave Pix              12.345.678/0001-95 (CNPJ)
  Identificador          NF20260815
  Mensagem do código     Pedido 1051
  Valor                  R$ 350,00 (trezentos e cinquenta reais)
  Quando                 agora
  Chave de idempotência  0b439cb0-d6b0-402b-9648-c2a3e9ab0a97
Enviar o Pix? [s/N] s
Pix enviado.
Código da solicitação  e7a1c3b5-9d2f-4e6a-8b0c-1f3d5a7c9e2b
Data do pagamento      24/09/2026
Data da operação       24/09/2026
Chave de idempotência  0b439cb0-d6b0-402b-9648-c2a3e9ab0a97

Acompanhe com: inter-pj pix consultar e7a1c3b5-9d2f-4e6a-8b0c-1f3d5a7c9e2b --aguardar
```

Use aspas simples: o código tem espaços, e alguns têm `*`, que o shell trocaria por nomes de arquivos. Os textos do código chegam ao resumo sem caracteres de controle, para que um código malicioso não altere o que o terminal mostra. Um código estático com valor fixa o valor:

```console
$ inter-pj pix enviar --copia-e-cola '00020126510014br.gov.bcb.pix0114123456780001950211Pedido 10515204000053039865406350.005802BR5921Fornecedor Exemplo SA6009SAO PAULO62140510NF2026081563049524' --valor 300,00
erro: o código copia e cola fixa o valor em R$ 350,00, e --valor informa R$ 300,00: retire --valor ou use o mesmo valor
```

Um código dinâmico, de uma cobrança mantida pelo recebedor, aceita outro valor, como o de uma cobrança com juros ou desconto; o resumo mostra então os dois, com um aviso para conferir.

## Dados bancários

Sem chave, o Pix vai pelos dados da conta de quem recebe: o ISPB do banco (8 dígitos), a agência, a conta com o dígito, o tipo (`corrente`, `poupanca`, `salario` ou `pagamento`), o CPF ou o CNPJ e o nome do titular:

```console
$ inter-pj pix enviar --valor 3.000,00 --ispb 12345678 --agencia 0001 --conta 7654321 --tipo-conta corrente \
    --documento 123.456.789-09 --nome "Fulano de Tal" --descricao "Pró-labore de setembro"
*** PRODUÇÃO: este Pix movimenta dinheiro da conta real ***
Pix a enviar
  Ambiente               PRODUÇÃO (conta real)
  Titular                Fulano de Tal
  CPF/CNPJ               123.456.789-09
  Instituição            ISPB 12345678
  Agência e conta        0001 / 7654321 (conta corrente)
  Valor                  R$ 3.000,00 (três mil reais)
  Quando                 agora
  Descrição              Pró-labore de setembro
  Chave de idempotência  43d783d3-7961-417c-a90c-3e3859d29d80
Enviar o Pix? [s/N] s
Pix enviado.
Código da solicitação  2d8f4b6a-1c3e-4a5b-9f7d-8e0a2c4b6d13
Data do pagamento      24/09/2026
Data da operação       24/09/2026
Chave de idempotência  43d783d3-7961-417c-a90c-3e3859d29d80

Acompanhe com: inter-pj pix consultar 2d8f4b6a-1c3e-4a5b-9f7d-8e0a2c4b6d13 --aguardar
```

## Agendar

`--data` agenda o Pix para um dia, hoje ou depois. As datas seguem o calendário do banco, o de Brasília, e um agendamento para hoje é um Pix de agora:

```console
$ inter-pj pix enviar --chave financeiro@fornecedor.example --valor 1.200,00 --data 2026-10-01
*** PRODUÇÃO: este Pix movimenta dinheiro da conta real ***
Pix a enviar
  Ambiente               PRODUÇÃO (conta real)
  Chave Pix              financeiro@fornecedor.example (e-mail)
  Valor                  R$ 1.200,00 (mil e duzentos reais)
  Quando                 agendado para 01/10/2026
  Chave de idempotência  19a11e0b-5382-4ce4-a35f-1a9ba7bf44c4
Enviar o Pix? [s/N] s
Pix agendado para 01/10/2026.
Código da solicitação  9a3c5e7b-4d6f-4b8a-8c1e-3f5a7b9d1c24
Data do pagamento      01/10/2026
Data da operação       24/09/2026
Chave de idempotência  19a11e0b-5382-4ce4-a35f-1a9ba7bf44c4

Acompanhe com: inter-pj pix consultar 9a3c5e7b-4d6f-4b8a-8c1e-3f5a7b9d1c24 --aguardar

$ inter-pj pix enviar --chave financeiro@fornecedor.example --valor 1.200,00 --data 2026-09-23
erro: o dia 23/09/2026 já passou: agende para hoje ou depois
```

## Conferir sem enviar

`--simular` confere tudo e mostra a requisição que seria enviada, sem os segredos, sem pedir token e sem enviar nada:

```console
$ inter-pj pix enviar --chave financeiro@fornecedor.example --valor 1.200,00 --simular
*** PRODUÇÃO: este Pix movimenta dinheiro da conta real ***
Pix a enviar
  Ambiente               PRODUÇÃO (conta real)
  Chave Pix              financeiro@fornecedor.example (e-mail)
  Valor                  R$ 1.200,00 (mil e duzentos reais)
  Quando                 agora
  Chave de idempotência  9261b97f-9f8d-48c1-96c3-d2f802635722
Simulação: nada foi enviado.

POST https://cdpj.partners.bancointer.com.br/banking/v2/pix
x-id-idempotente: 9261b97f-9f8d-48c1-96c3-d2f802635722

{
  "destinatario": {
    "chave": "financeiro@fornecedor.example",
    "tipo": "CHAVE"
  },
  "valor": 1200
}
```

## Os limites

Com `limite_por_operacao` no perfil, um valor acima dele é recusado antes de qualquer requisição, mesmo com `--sim`:

```console
$ inter-pj pix enviar --chave financeiro@fornecedor.example --valor 25.000,00 --sim
erro: R$ 25.000,00 passa do limite por operação do perfil "padrao" (R$ 20.000,00); para permitir, ajuste limite_por_operacao em /home/voce/.config/inter-pj/config.toml
```

E, conforme a configuração da conta, um Pix pode esperar a aprovação de outra pessoa no Internet Banking. Nesta conta, os acima de R$ 10.000,00:

```console
$ inter-pj pix enviar --chave financeiro@fornecedor.example --valor 12.000,00 --descricao "Equipamentos"
*** PRODUÇÃO: este Pix movimenta dinheiro da conta real ***
Pix a enviar
  Ambiente               PRODUÇÃO (conta real)
  Chave Pix              financeiro@fornecedor.example (e-mail)
  Valor                  R$ 12.000,00 (doze mil reais)
  Quando                 agora
  Descrição              Equipamentos
  Chave de idempotência  a54655d7-a6f1-4d68-b392-39e385dd7197
Enviar o Pix? [s/N] s
Pix aguardando aprovação no Internet Banking (Aprovar > Gestão de Aprovações): só será enviado depois de aprovado.
Código da solicitação  6f1b3d5c-8e0a-4c2d-9b4f-6a8c0e2d4f35
Data do pagamento      24/09/2026
Data da operação       24/09/2026
Chave de idempotência  a54655d7-a6f1-4d68-b392-39e385dd7197

Acompanhe com: inter-pj pix consultar 6f1b3d5c-8e0a-4c2d-9b4f-6a8c0e2d4f35 --aguardar
```

## Quando a resposta se perde

Cada envio leva uma chave de idempotência, mostrada no resumo: com a mesma chave, o banco não paga duas vezes. Se a resposta se perder depois do envio (um tempo esgotado, um erro 5xx), o Pix pode ter sido feito, e a CLI sai com o código 9 e diz como repetir. Num script, gere a chave antes, com o `uuidgen` por exemplo, e use a mesma em todas as tentativas:

```console
$ inter-pj pix enviar --chave financeiro@fornecedor.example --valor 480,00 --id-idempotente 7c1e4b9a-2f3d-4a8e-9b6c-5d0e1f2a3b4c --sim
*** PRODUÇÃO: este Pix movimenta dinheiro da conta real ***
Pix a enviar
  Ambiente               PRODUÇÃO (conta real)
  Chave Pix              financeiro@fornecedor.example (e-mail)
  Valor                  R$ 480,00 (quatrocentos e oitenta reais)
  Quando                 agora
  Chave de idempotência  7c1e4b9a-2f3d-4a8e-9b6c-5d0e1f2a3b4c
erro: POST /banking/v2/pix respondeu 504 (tempo esgotado no gateway)
dica: o pagamento pode ter sido feito: confira o extrato antes de tentar de novo
dica: para repetir sem risco de pagar duas vezes, use a mesma chave: --id-idempotente 7c1e4b9a-2f3d-4a8e-9b6c-5d0e1f2a3b4c

$ inter-pj pix enviar --chave financeiro@fornecedor.example --valor 480,00 --id-idempotente 7c1e4b9a-2f3d-4a8e-9b6c-5d0e1f2a3b4c --sim
*** PRODUÇÃO: este Pix movimenta dinheiro da conta real ***
Pix a enviar
  Ambiente               PRODUÇÃO (conta real)
  Chave Pix              financeiro@fornecedor.example (e-mail)
  Valor                  R$ 480,00 (quatrocentos e oitenta reais)
  Quando                 agora
  Chave de idempotência  7c1e4b9a-2f3d-4a8e-9b6c-5d0e1f2a3b4c
Pix enviado.
Código da solicitação  b4d6f8a0-2c4e-4d6f-8a0b-2c4e6f8a0b46
Data do pagamento      24/09/2026
Data da operação       24/09/2026
Chave de idempotência  7c1e4b9a-2f3d-4a8e-9b6c-5d0e1f2a3b4c

Acompanhe com: inter-pj pix consultar b4d6f8a0-2c4e-4d6f-8a0b-2c4e6f8a0b46 --aguardar
```

A segunda tentativa recebe o Pix da primeira, que tinha sido feito, sem pagar de novo. Sem a mesma chave, seria outro Pix: nunca repita às cegas um comando que saiu com o código 9.

## Acompanhar um Pix enviado

`pix consultar`, com o código da solicitação, mostra o status, o recebedor, os erros e o histórico de um Pix dos últimos 90 dias:

```console
$ inter-pj pix consultar c42f0787-02cb-4b31-827e-459ec9d7ece1
Pix
  Status                 pago
  Valor                  R$ 1.200,00
  Recebedor              Fornecedor Exemplo SA (12.345.678/0001-95)
  Conta do recebedor     ISPB 12345678, agência 0001, conta 1234567
  Chave Pix              financeiro@fornecedor.example
  End-to-end             E12345678202609241030a7Bc9DeF1gH
  Solicitado em          24/09/2026 10:30:00
  Movimentado em         24/09/2026 10:30:02
  Código da solicitação  c42f0787-02cb-4b31-827e-459ec9d7ece1

Histórico
  24/09/2026 10:30:00  criado
  24/09/2026 10:30:01  enviado ao banco do recebedor
  24/09/2026 10:30:02  pago
```

Um Pix agendado fica `agendado` até o dia, e um que espera aprovação, `aguardando aprovação no Internet Banking`:

```console
$ inter-pj pix consultar 9a3c5e7b-4d6f-4b8a-8c1e-3f5a7b9d1c24
Pix
  Status                 agendado
  Valor                  R$ 1.200,00
  Recebedor              Fornecedor Exemplo SA (12.345.678/0001-95)
  Conta do recebedor     ISPB 12345678, agência 0001, conta 1234567
  Chave Pix              financeiro@fornecedor.example
  Solicitado em          24/09/2026 10:58:00
  Código da solicitação  9a3c5e7b-4d6f-4b8a-8c1e-3f5a7b9d1c24

Histórico
  24/09/2026 10:58:00  criado
  24/09/2026 10:58:01  agendado
```

Com `--aguardar`, a CLI consulta a cada 6 segundos, até um status final, e sai com o código 0 (pago ou agendado), 5 (terminou sem ser pago: reprovado, expirado, cancelado...) ou 8 (o tempo de `--timeout`, 60 segundos por padrão, acabou antes):

```console
$ inter-pj pix consultar c42f0787-02cb-4b31-827e-459ec9d7ece1 --aguardar
Pix
  Status                 pago
  Valor                  R$ 1.200,00
  Recebedor              Fornecedor Exemplo SA (12.345.678/0001-95)
  Conta do recebedor     ISPB 12345678, agência 0001, conta 1234567
  Chave Pix              financeiro@fornecedor.example
  End-to-end             E12345678202609241030a7Bc9DeF1gH
  Solicitado em          24/09/2026 10:30:00
  Movimentado em         24/09/2026 10:30:02
  Código da solicitação  c42f0787-02cb-4b31-827e-459ec9d7ece1

Histórico
  24/09/2026 10:30:00  criado
  24/09/2026 10:30:01  enviado ao banco do recebedor
  24/09/2026 10:30:02  pago
```

## Pix recebidos

Os Pix que a conta recebeu, com ou sem cobrança, ficam na API Pix, com as suas devoluções. Consultá-los precisa do escopo `pix.read`:

```console
$ inter-pj pix recebidos listar --inicio 2026-08-01 --fim 2026-09-30
Pix recebidos de 01/08/2026 00:00 a 30/09/2026 23:59

Horário                    Valor  Devolvido  endToEndId                        txid
03/08/2026 14:05:12  R$ 1.500,00             E12345678202608031705a1B2c3D4e5F
20/08/2026 11:31:26  R$ 2.350,00             E12345678202608201431Zx9cV8bN7mA
28/08/2026 19:02:44    R$ 740,00             E12345678202608282202Lk5jH3gF1dS
02/09/2026 09:15:38  R$ 1.500,00             E12345678202609021215Po0iU9yT8rE  pedido1053empresaexemplo2026

4 Pix · R$ 6.090,00
```

Os filtros são `--txid` (os Pix de uma cobrança), `--com-cobranca` ou `--sem-cobranca`, `--com-devolucao` ou `--sem-devolucao` e `--documento`, o CPF ou o CNPJ de quem pagou:

```console
$ inter-pj pix recebidos listar --inicio 2026-08-01 --fim 2026-09-30 --documento 11.222.333/0001-81
Pix recebidos de 01/08/2026 00:00 a 30/09/2026 23:59 (pagador 11.222.333/0001-81)

Horário                    Valor  Devolvido  endToEndId                        txid
03/08/2026 14:05:12  R$ 1.500,00             E12345678202608031705a1B2c3D4e5F
20/08/2026 11:31:26  R$ 2.350,00             E12345678202608201431Zx9cV8bN7mA
02/09/2026 09:15:38  R$ 1.500,00             E12345678202609021215Po0iU9yT8rE  pedido1053empresaexemplo2026

3 Pix · R$ 5.350,00
```

`--inicio` e `--fim` aceitam uma data, o dia inteiro no fuso local, ou uma data e hora com fuso (`2026-09-01T08:00:00-03:00`). Sem eles, a listagem é dos últimos 30 dias. Todas as páginas são lidas, ou só uma, com `--pagina`. A consulta de um Pix mostra também a mensagem de quem pagou:

```console
$ inter-pj pix recebidos consultar E12345678202608282202Lk5jH3gF1dS
Pix recebido E12345678202608282202Lk5jH3gF1dS
  Valor        R$ 740,00
  Recebido em  28/08/2026 19:02:44
  Chave        pix@empresa.example
  Mensagem     Pedido 1049

Para devolver: inter-pj pix devolucao solicitar E12345678202608282202Lk5jH3gF1dS --valor VALOR (ou --tudo)
```

## Devolver um Pix recebido

Uma devolução tira dinheiro da conta, e os trilhos são os do envio: a CLI consulta o Pix antes, recusa sem enviar nada uma devolução maior que o que resta dele, mostra um resumo e pede a confirmação (ou `--sim`). O limite por operação do perfil vale também aqui, e `--simular` mostra a requisição, sem consultar nem enviar nada. Devolver precisa do escopo `pix.write`:

```console
$ inter-pj pix devolucao solicitar E12345678202608282202Lk5jH3gF1dS --valor 240,00 --descricao "Troca de produto" --id troca0928
*** PRODUÇÃO: esta devolução tira dinheiro da conta real ***
Devolução a solicitar
  Ambiente      PRODUÇÃO (conta real)
  Pix           E12345678202608282202Lk5jH3gF1dS
  Recebido em   28/08/2026 19:02:44
  Valor do Pix  R$ 740,00
  Devolução     R$ 240,00 (duzentos e quarenta reais)
  Descrição     Troca de produto
  id            troca0928
Devolver o Pix? [s/N] s
Devolução solicitada.

Devolução troca0928
  Status         em processamento
  Valor          R$ 240,00
  Pix            E12345678202608282202Lk5jH3gF1dS
  Solicitada em  24/09/2026 14:20:00
  rtrId          D12345678202609241720h4Jk6Lm8NpQ

Acompanhe com: inter-pj pix devolucao consultar E12345678202608282202Lk5jH3gF1dS troca0928 --aguardar
```

Cada devolução tem um id, de 1 a 35 letras e dígitos, gerado pela CLI ou dado com `--id`: com o mesmo id, a API não devolve de novo, e um id seu, como o do pedido, acha a devolução depois. Ela é processada depois do pedido; `pix devolucao consultar` mostra em que pé está e, com `--aguardar` (aceito também por `solicitar`), consulta a cada 6 segundos até o fim, saindo com o código 0 (devolvida), 5 (não realizada, com o motivo) ou 8 (o tempo acabou):

```console
$ inter-pj pix devolucao consultar E12345678202608282202Lk5jH3gF1dS troca0928 --aguardar
Devolução troca0928
  Status         devolvida
  Valor          R$ 240,00
  Pix            E12345678202608282202Lk5jH3gF1dS
  Solicitada em  24/09/2026 14:20:00
  Liquidada em   24/09/2026 14:20:03
  rtrId          D12345678202609241720h4Jk6Lm8NpQ
```

O Pix mostra então o que foi devolvido e o que ainda pode ser:

```console
$ inter-pj pix recebidos consultar E12345678202608282202Lk5jH3gF1dS
Pix recebido E12345678202608282202Lk5jH3gF1dS
  Valor          R$ 740,00
  Recebido em    28/08/2026 19:02:44
  Devolvido      R$ 240,00
  Pode devolver  R$ 500,00
  Chave          pix@empresa.example
  Mensagem       Pedido 1049

Devoluções
id         Status         Valor  Solicitada em
troca0928  devolvida  R$ 240,00  24/09/2026 14:20:00

Para devolver: inter-pj pix devolucao solicitar E12345678202608282202Lk5jH3gF1dS --valor VALOR (ou --tudo)

$ inter-pj pix devolucao solicitar E12345678202608282202Lk5jH3gF1dS --valor 600,00 --sim
erro: a devolução de R$ 600,00 passa do que resta do Pix: R$ 500,00 de R$ 740,00, R$ 240,00 já devolvidos ou em devolução
```

`--tudo` devolve exatamente o que resta:

```console
$ inter-pj pix devolucao solicitar E12345678202608282202Lk5jH3gF1dS --tudo --id troca0928b --sim --aguardar
*** PRODUÇÃO: esta devolução tira dinheiro da conta real ***
Devolução a solicitar
  Ambiente      PRODUÇÃO (conta real)
  Pix           E12345678202608282202Lk5jH3gF1dS
  Recebido em   28/08/2026 19:02:44
  Valor do Pix  R$ 740,00
  Já devolvido  R$ 240,00
  Devolução     R$ 500,00 (quinhentos reais)
  id            troca0928b
Devolução troca0928b
  Status         devolvida
  Valor          R$ 500,00
  Pix            E12345678202608282202Lk5jH3gF1dS
  Solicitada em  24/09/2026 14:25:00
  Liquidada em   24/09/2026 14:25:03
  rtrId          D12345678202609241725r2St4Uv6WxY

$ inter-pj pix recebidos listar --inicio 2026-08-01 --fim 2026-09-30 --com-devolucao
Pix recebidos de 01/08/2026 00:00 a 30/09/2026 23:59 (com devolução)

Horário                  Valor  Devolvido  endToEndId                        txid
28/08/2026 19:02:44  R$ 740,00  R$ 740,00  E12345678202608282202Lk5jH3gF1dS

1 Pix · R$ 740,00 · devolvidos R$ 740,00
```

`--natureza retirada` devolve o dinheiro de um Pix Saque ou o troco de um Pix Troco; o padrão, `original`, é o do Pix comum. A descrição, de até 140 caracteres, vai para quem pagou.
