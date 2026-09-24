# Webhooks

Um webhook é um endereço do servidor da empresa que o Inter chama quando algo acontece na conta: um Pix enviado, um boleto pago, uma cobrança recebida. A CLI cadastra, consulta e exclui os webhooks de cada API, mostra o histórico das tentativas de entrega das notificações (os callbacks) e pede o reenvio das que não chegaram. Quem recebe as notificações é o servidor da empresa, não a CLI.

| Comando | O Inter notifica | Escopos |
| --- | --- | --- |
| `webhook banking ... pix-pagamento` | os Pix enviados pela conta | `webhook-banking.read` e `webhook-banking.write` |
| `webhook banking ... boleto-pagamento` | os boletos pagos pela conta | `webhook-banking.read` e `webhook-banking.write` |
| `webhook cobranca ...` | as cobranças recebidas, canceladas e expiradas | `boleto-cobranca.read` e `boleto-cobranca.write` |
| `webhook pix ... CHAVE` | as cobranças Pix pagas, imediatas e com vencimento, com um webhook por chave Pix | `webhook.read` e `webhook.write` |
| `webhook recorrencia ...` | as mudanças de status das recorrências do Pix Automático | `webhookrec.read` e `webhookrec.write` |
| `webhook cobranca-recorrente ...` | as mudanças de status das cobranças recorrentes do Pix Automático | `webhookcobr.read` e `webhookcobr.write` |

Consultar e ver o histórico pedem o escopo de leitura (`.read`) da API; cadastrar, excluir e pedir o reenvio, o de escrita (`.write`).

Os exemplos são da Empresa Exemplo Ltda, uma empresa fictícia, no perfil de produção. Desde agosto, o servidor da empresa, `api.empresa.example`, recebe as notificações dos Pix enviados, das cobranças e da chave Pix `pix@empresa.example`.

- [Os webhooks da conta](#os-webhooks-da-conta)
- [Cadastrar um webhook](#cadastrar-um-webhook)
- [Trocar a URL](#trocar-a-url)
- [Pix Automático](#pix-automático)
- [O histórico dos callbacks](#o-histórico-dos-callbacks)
- [Pedir o reenvio](#pedir-o-reenvio)
- [No Pix e no Banking](#no-pix-e-no-banking)
- [Excluir um webhook](#excluir-um-webhook)

## Os webhooks da conta

`consultar` mostra para onde vão as notificações. No Banking, sem o tipo, mostra os dois, e o que não tem webhook diz como cadastrar:

```console
$ inter-pj webhook banking consultar
Webhook do tipo pix-pagamento
  Notifica       Pix enviados pela conta
  URL            https://api.empresa.example/inter/pix-enviados
  Cadastrado em  01/08/2026 09:00:00

Webhook do tipo boleto-pagamento
  Nenhum webhook cadastrado: o Inter não notifica boletos pagos pela conta.
  Para cadastrar: inter-pj webhook banking cadastrar boleto-pagamento --url https://...
```

```console
$ inter-pj webhook cobranca consultar
Webhook de cobranças
  Notifica       cobranças recebidas, canceladas e expiradas
  URL            https://api.empresa.example/inter/cobrancas
  Cadastrado em  01/08/2026 09:02:10
$ inter-pj webhook pix consultar pix@empresa.example
Webhook da chave pix@empresa.example
  Notifica       cobranças Pix pagas (imediatas e com vencimento)
  URL            https://api.empresa.example/inter/pix-cobrancas
  Cadastrado em  01/08/2026 09:04:45
```

Os horários estão no fuso da máquina, aqui o de Brasília. Para um script, `--json` traz os campos da API; no Banking, sem o tipo, um objeto com os dois, e o que não tem webhook como `null`:

```console
$ inter-pj webhook banking consultar --json
{
  "boleto-pagamento": null,
  "pix-pagamento": {
    "criacao": "2026-08-01T12:00:00.000Z",
    "webhookUrl": "https://api.empresa.example/inter/pix-enviados"
  }
}
```

## Cadastrar um webhook

Os boletos que a empresa paga ainda não são notificados. `cadastrar` mostra o webhook a cadastrar e pede confirmação, porque o endereço novo passa a receber os dados dos pagamentos da conta:

```console
$ inter-pj webhook banking cadastrar boleto-pagamento --url https://api.empresa.example/inter/boletos-pagos
Webhook do tipo boleto-pagamento a cadastrar
  Ambiente  PRODUÇÃO (conta real)
  Notifica  boletos pagos pela conta
  Nova URL  https://api.empresa.example/inter/boletos-pagos
Cadastrar o webhook? [s/N] s
Webhook cadastrado: o Inter passa a notificar boletos pagos pela conta em https://api.empresa.example/inter/boletos-pagos.

Confira com: inter-pj webhook banking consultar boleto-pagamento
```

Num script, sem um terminal para a pergunta, `--sim` confirma. Cadastrar a URL que o webhook já tem não muda nada:

```console
$ inter-pj webhook banking cadastrar boleto-pagamento --url https://api.empresa.example/inter/boletos-pagos --sim
O webhook já usa esta URL: nada a alterar.

Webhook do tipo boleto-pagamento
  Notifica       boletos pagos pela conta
  URL            https://api.empresa.example/inter/boletos-pagos
  Cadastrado em  24/09/2026 11:02:27
```

A URL precisa começar com `https://`, e a CLI a confere antes de qualquer requisição:

```console
$ inter-pj webhook banking cadastrar boleto-pagamento --url http://api.empresa.example/inter/boletos-pagos
erro: valor inválido 'http://api.empresa.example/inter/boletos-pagos' para '--url <URL>': a URL do webhook precisa começar com https://

Para mais informações, use '--help'.
```

O Inter também precisa alcançar a URL pela internet: o resumo avisa quando ela aponta para a própria máquina ou para uma rede privada (`localhost`, `192.168.0.10`, `servidor.local`).

## Trocar a URL

A empresa está levando o sistema de cobranças para um servidor novo. Num webhook que já existe, `cadastrar` troca a URL, e o resumo mostra a atual e avisa que as notificações vão para outro servidor:

```console
$ inter-pj webhook cobranca cadastrar --url https://novo.empresa.example/inter/cobrancas
Webhook de cobranças a trocar
  Ambiente   PRODUÇÃO (conta real)
  Notifica   cobranças recebidas, canceladas e expiradas
  URL atual  https://api.empresa.example/inter/cobrancas
  Nova URL   https://novo.empresa.example/inter/cobrancas
aviso: as notificações passam a ir para novo.empresa.example, e não mais para api.empresa.example
Trocar a URL do webhook? [s/N] s
Webhook cadastrado: o Inter passa a notificar cobranças recebidas, canceladas e expiradas em https://novo.empresa.example/inter/cobrancas.

Confira com: inter-pj webhook cobranca consultar
```

```console
$ inter-pj webhook cobranca consultar
Webhook de cobranças
  Notifica       cobranças recebidas, canceladas e expiradas
  URL            https://novo.empresa.example/inter/cobrancas
  Cadastrado em  01/08/2026 09:02:10
  Alterado em    24/09/2026 11:08:27
```

## Pix Automático

Nos dois webhooks do Pix Automático, o Inter entrega as notificações num caminho que acrescenta à URL cadastrada: `/rec`, nas das recorrências, e `/cobr`, nas das cobranças recorrentes. Com a mesma URL nos dois, o servidor distingue uma notificação da outra pelo caminho, e o resumo e a consulta mostram onde cada uma chega:

```console
$ inter-pj webhook recorrencia cadastrar --url https://api.empresa.example/inter/pix-automatico
Webhook de recorrências a cadastrar
  Ambiente    PRODUÇÃO (conta real)
  Notifica    mudanças de status das recorrências do Pix Automático
  Nova URL    https://api.empresa.example/inter/pix-automatico
  Entrega em  https://api.empresa.example/inter/pix-automatico/rec
Cadastrar o webhook? [s/N] s
Webhook cadastrado: o Inter passa a notificar mudanças de status das recorrências do Pix Automático em https://api.empresa.example/inter/pix-automatico/rec.

Confira com: inter-pj webhook recorrencia consultar
$ inter-pj webhook cobranca-recorrente cadastrar --url https://api.empresa.example/inter/pix-automatico
Webhook de cobranças recorrentes a cadastrar
  Ambiente    PRODUÇÃO (conta real)
  Notifica    mudanças de status das cobranças recorrentes do Pix Automático
  Nova URL    https://api.empresa.example/inter/pix-automatico
  Entrega em  https://api.empresa.example/inter/pix-automatico/cobr
Cadastrar o webhook? [s/N] s
Webhook cadastrado: o Inter passa a notificar mudanças de status das cobranças recorrentes do Pix Automático em https://api.empresa.example/inter/pix-automatico/cobr.

Confira com: inter-pj webhook cobranca-recorrente consultar
```

```console
$ inter-pj webhook recorrencia consultar
Webhook de recorrências
  Notifica       mudanças de status das recorrências do Pix Automático
  URL            https://api.empresa.example/inter/pix-automatico
  Entrega em     https://api.empresa.example/inter/pix-automatico/rec
  Cadastrado em  24/09/2026 11:14:27
```

Quando o servidor não aceita uma notificação do Pix Automático, o Inter tenta de novo até 4 vezes, 20, 30, 60 e 120 minutos depois. Esses dois webhooks não têm histórico de callbacks nem reenvio.

## O histórico dos callbacks

Quando o servidor não aceita uma notificação, porque respondeu com um erro ou não respondeu, o Inter tenta de novo até 4 vezes: 20, 30, 60 e 120 minutos depois (no Banking, 5, 10, 30 e 60). Cada tentativa fica no histórico, com o status HTTP que o servidor respondeu. As de agosto, no webhook das cobranças:

```console
$ inter-pj webhook cobranca callbacks --inicio 2026-08-01 --fim 2026-08-31
Callbacks do webhook de cobranças de 01/08/2026 00:00 a 31/08/2026 23:59

Disparo              Tentativa  Entregue  HTTP  Código da cobrança                    Erro
21/08/2026 06:00:02          5  não        400  5c2e8a41-7d3b-4f6e-9a1c-2b4d6f8e0a13  Bad Request
21/08/2026 04:00:02          4  não        400  5c2e8a41-7d3b-4f6e-9a1c-2b4d6f8e0a13  Bad Request
21/08/2026 03:00:02          3  não        400  5c2e8a41-7d3b-4f6e-9a1c-2b4d6f8e0a13  Bad Request
21/08/2026 02:30:02          2  não        400  5c2e8a41-7d3b-4f6e-9a1c-2b4d6f8e0a13  Bad Request
21/08/2026 02:10:02          1  não        400  5c2e8a41-7d3b-4f6e-9a1c-2b4d6f8e0a13  Bad Request
10/08/2026 16:42:33          2  sim        200  8e1f3a5c-7b9d-4e2f-8a4c-6e8f0a2c4e61
10/08/2026 16:22:33          1  não        503  8e1f3a5c-7b9d-4e2f-8a4c-6e8f0a2c4e61  Service Unavailable

7 tentativas · 1 entregue · 6 falharam

Sem entrega no período: 1 operação. Para pedir o reenvio:
  inter-pj webhook cobranca reenviar 5c2e8a41-7d3b-4f6e-9a1c-2b4d6f8e0a13
```

A cobrança que a Beltrana de Tal pagou em 10/08 chegou na segunda tentativa: na primeira, o servidor estava reiniciando (`503`). A que expirou em 21/08, do Fulano de Tal, não chegou em nenhuma das cinco: o sistema da empresa recusava as cobranças expiradas (`400`). A dica considera todas as tentativas do período, então uma cobrança entregue numa tentativa posterior não entra nela.

O período segue as regras das listagens Pix: datas `AAAA-MM-DD`, do começo do primeiro dia ao fim do último, ou data e hora com fuso, e, por padrão, os últimos 30 dias até agora. `--falhas` mostra só as tentativas que falharam, e `--codigo` (cobranças), `--txid` (Pix), `--end-to-end` e `--codigo-transacao` (Banking), só as de uma operação. A CLI lê todas as páginas do histórico, ou só uma, com `--pagina` (a primeira é 0) e `--itens-por-pagina`, de 10 a 50. A listagem sai também em `--json` e em `--formato csv`, com os campos da API e a URL de cada tentativa.

## Pedir o reenvio

Com o sistema corrigido, a empresa pede ao Inter que envie de novo a notificação da cobrança que expirou, com o comando da dica:

```console
$ inter-pj webhook cobranca reenviar 5c2e8a41-7d3b-4f6e-9a1c-2b4d6f8e0a13
Reenvio pedido para 1 de 1 operação: o Inter vai enviar os callbacks de novo.
```

O reenvio vai para a URL que o webhook tem agora, a do servidor novo, e a nova tentativa entra no histórico:

```console
$ inter-pj webhook cobranca callbacks --codigo 5c2e8a41-7d3b-4f6e-9a1c-2b4d6f8e0a13 --inicio 2026-08-01 --fim 2026-09-24
Callbacks do webhook de cobranças de 01/08/2026 00:00 a 24/09/2026 23:59 (cobrança 5c2e8a41-7d3b-4f6e-9a1c-2b4d6f8e0a13)

Disparo              Tentativa  Entregue  HTTP  Código da cobrança                    Erro
24/09/2026 11:26:27          6  sim        200  5c2e8a41-7d3b-4f6e-9a1c-2b4d6f8e0a13
21/08/2026 06:00:02          5  não        400  5c2e8a41-7d3b-4f6e-9a1c-2b4d6f8e0a13  Bad Request
21/08/2026 04:00:02          4  não        400  5c2e8a41-7d3b-4f6e-9a1c-2b4d6f8e0a13  Bad Request
21/08/2026 03:00:02          3  não        400  5c2e8a41-7d3b-4f6e-9a1c-2b4d6f8e0a13  Bad Request
21/08/2026 02:30:02          2  não        400  5c2e8a41-7d3b-4f6e-9a1c-2b4d6f8e0a13  Bad Request
21/08/2026 02:10:02          1  não        400  5c2e8a41-7d3b-4f6e-9a1c-2b4d6f8e0a13  Bad Request

6 tentativas · 1 entregue · 5 falharam
```

Uma notificação entregue também pode ser pedida de novo, quando o sistema da empresa a perdeu. Uma cobrança sem notificações, como a NF-0910, que ainda não foi paga, não é encontrada, e a CLI a lista:

```console
$ inter-pj webhook cobranca reenviar 8e1f3a5c-7b9d-4e2f-8a4c-6e8f0a2c4e61 9d7b5f3e-1c0a-4e8f-9b7d-5f3e1c0a8e25
Reenvio pedido para 1 de 2 operações: o Inter vai enviar os callbacks de novo.

Não encontradas:
  9d7b5f3e-1c0a-4e8f-9b7d-5f3e1c0a8e25
```

`reenviar` recebe os códigos que o histórico mostra: o das cobranças; no Pix, a chave e os txids das cobranças; e no Banking, o código da solicitação dos Pix enviados (`pix-pagamento`) ou o da transação dos boletos pagos (`boleto-pagamento`). A CLI confere os códigos antes de qualquer requisição, manda uma vez os repetidos e divide mais de 50 em blocos de 50. Como o Inter aceita 5 pedidos de reenvio por minuto, com mais de 5 blocos a CLI espera 12 segundos entre um e outro, e, se um bloco falhar, a dica traz o comando que pede o reenvio dos que faltam.

## No Pix e no Banking

No Pix, o histórico reúne as notificações de todas as chaves, e cada uma traz o txid da cobrança paga. Em setembro, a cobrança do pedido 1053:

```console
$ inter-pj webhook pix callbacks --inicio 2026-09-01 --fim 2026-09-24
Callbacks dos webhooks Pix de 01/09/2026 00:00 a 24/09/2026 23:59

Disparo              Tentativa  Entregue  HTTP  txid
02/09/2026 09:15:40          1  sim        200  pedido1053empresaexemplo2026

1 tentativa · 1 entregue · 0 falharam
```

No Pix, o reenvio pede também a chave das cobranças, a mesma para todos os txids:

```console
$ inter-pj webhook pix reenviar pix@empresa.example pedido1053empresaexemplo2026
Reenvio pedido para 1 de 1 operação: o Inter vai enviar os callbacks de novo.
```

No Banking, cada tipo tem o seu histórico, e o Inter tenta de novo mais cedo. O Pix que a empresa enviou ao Fornecedor Exemplo SA em 08/09 chegou na segunda tentativa, 5 minutos depois da primeira. O filtro é o `endToEnd` do Pix, e a tabela mostra o código da solicitação, que é o que `reenviar` recebe:

```console
$ inter-pj webhook banking callbacks pix-pagamento --end-to-end E12345678202609081344Mn6bV5cX4zQ --inicio 2026-09-01 --fim 2026-09-24
Callbacks do webhook pix-pagamento de 01/09/2026 00:00 a 24/09/2026 23:59 (endToEnd E12345678202609081344Mn6bV5cX4zQ)

Disparo              Tentativa  Entregue  HTTP  Código da solicitação                 Erro
08/09/2026 10:49:04          2  sim        200  0b7e3d9c-2a4f-4c8e-b1d6-7f5a9e3c2b10
08/09/2026 10:44:04          1  não        504  0b7e3d9c-2a4f-4c8e-b1d6-7f5a9e3c2b10  Gateway Timeout

2 tentativas · 1 entregue · 1 falhou
```

## Excluir um webhook

O sistema novo da empresa confere os Pix enviados pelo extrato e não precisa mais das notificações. `excluir` mostra o webhook e pede confirmação:

```console
$ inter-pj webhook banking excluir pix-pagamento
Webhook do tipo pix-pagamento a excluir
  Ambiente       PRODUÇÃO (conta real)
  Notifica       Pix enviados pela conta
  URL            https://api.empresa.example/inter/pix-enviados
  Cadastrado em  01/08/2026 09:00:00
aviso: o Inter deixa de notificar Pix enviados pela conta
Excluir o webhook? [s/N] s
Webhook excluído: o Inter deixa de notificar Pix enviados pela conta.
```

Sem webhook, não há o que excluir:

```console
$ inter-pj webhook banking excluir pix-pagamento --sim
erro: nenhum webhook do tipo pix-pagamento cadastrado: não há o que excluir
```

Se a resposta de um cadastro ou de uma exclusão se perde (tempo esgotado, erro 5xx), a CLI não sabe se a mudança foi feita: sai com o código 9 e mostra o comando que consulta o webhook, para conferir antes de tentar de novo.
