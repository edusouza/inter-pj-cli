# Cobranças Pix

A API Pix cria cobranças com QR Code dinâmico, que o cliente paga pelo app de qualquer banco. A cobrança imediata (`pix cob`) é para pagar na hora, até expirar. Criar uma cobrança não tira dinheiro da conta, mas ela vale de verdade: os trilhos são o resumo, a confirmação num terminal (ou `--sim`) e `--simular`, e o limite por operação não se aplica.

Os exemplos são da Empresa Exemplo Ltda, uma empresa fictícia, no perfil de produção, que recebe na chave `pix@empresa.example`. Criar e alterar precisam do escopo `cob.write`, e consultar e listar, do `cob.read`.

- [Criar uma cobrança imediata](#criar-uma-cobrança-imediata)
- [O txid](#o-txid)
- [Alterar e remover](#alterar-e-remover)
- [Uma cobrança paga](#uma-cobrança-paga)
- [Quando o resultado fica incerto](#quando-o-resultado-fica-incerto)
- [As cobranças de um período](#as-cobranças-de-um-período)

## Criar uma cobrança imediata

O pedido 1061 da Cliente Exemplo Ltda, para pagar em até 2 horas:

```console
$ inter-pj pix cob criar --chave pix@empresa.example --valor 149,90 --expiracao 2h \
    --devedor-documento 11.222.333/0001-81 --devedor-nome "Cliente Exemplo Ltda" \
    --solicitacao "Pedido 1061" --txid pedido1061empresaexemplo2026
*** PRODUÇÃO: a cobrança vale de verdade ***
Cobrança Pix a criar
  Ambiente     PRODUÇÃO (conta real)
  Valor        R$ 149,90 (cento e quarenta e nove reais e noventa centavos)
  Chave        pix@empresa.example (e-mail)
  Expira       2 horas após a criação
  Devedor      Cliente Exemplo Ltda (11.222.333/0001-81)
  Solicitação  Pedido 1061
  txid         pedido1061empresaexemplo2026
Criar a cobrança? [s/N] s
Cobrança Pix criada.

Cobrança Pix pedido1061empresaexemplo2026
  Status       ativa
  Valor        R$ 149,90
  Criada em    24/09/2026 10:05:12
  Expira em    24/09/2026 12:05:12
  Devedor      Cliente Exemplo Ltda (11.222.333/0001-81)
  Chave        pix@empresa.example
  Solicitação  Pedido 1061
  Revisão      0
  Location     qrcodepix.inter.example/qr/v2/cob/pedido1061empresaexemplo2026

Copia e cola  00020101021226840014br.gov.bcb.pix2562qrcodepix.inter.example/qr/v2/cob/pedido1061empresaexemplo20265204000053039865802BR5920EMPRESA EXEMPLO LTDA6014BELO HORIZONTE62070503***630446EF

Acompanhe com: inter-pj pix cob consultar pedido1061empresaexemplo2026
```

A chave (`--chave`) é uma chave Pix da conta que recebe, e o valor, maior que zero, com até 2 casas; `--valor-alteravel` deixa o pagador mudar o valor. A cobrança expira no tempo de `--expiracao`, contado da criação (`3600s`, `30m`, `2h`, `7d`; o padrão da API é 1 dia). Opcionais: o devedor (`--devedor-documento`, com o CPF ou o CNPJ conferido, e `--devedor-nome`), o texto mostrado ao pagador (`--solicitacao`, até 140 caracteres) e informações adicionais (`--info NOME=VALOR`, que pode ser repetida até 50 vezes). Tudo é conferido antes de qualquer requisição.

`--qrcode` desenha o QR Code no terminal, para o cliente ler com o celular, e `--qrcode-png` o grava numa imagem, como nas [cobranças com boleto](cobrancas.md#a-cobrança-emitida); `--simular` mostra a requisição, sem criar nada.

## O txid

Cada cobrança tem um txid, de 26 a 35 letras e dígitos, e com o mesmo txid a API não cria outra cobrança. Sem `--txid`, a CLI gera um e o mostra no resumo; um txid seu, feito do número do pedido, como `pedido1061empresaexemplo2026`, deixa achar a cobrança e repetir a criação sem risco de duplicá-la.

`pix cob consultar` mostra a cobrança pelo txid:

```console
$ inter-pj pix cob consultar pedido1061empresaexemplo2026
Cobrança Pix pedido1061empresaexemplo2026
  Status       ativa
  Valor        R$ 149,90
  Criada em    24/09/2026 10:05:12
  Expira em    24/09/2026 12:05:12
  Devedor      Cliente Exemplo Ltda (11.222.333/0001-81)
  Chave        pix@empresa.example
  Solicitação  Pedido 1061
  Revisão      0
  Location     qrcodepix.inter.example/qr/v2/cob/pedido1061empresaexemplo2026

Copia e cola  00020101021226840014br.gov.bcb.pix2562qrcodepix.inter.example/qr/v2/cob/pedido1061empresaexemplo20265204000053039865802BR5920EMPRESA EXEMPLO LTDA6014BELO HORIZONTE62070503***630446EF
```

## Alterar e remover

Enquanto não é paga, a cobrança pode ser alterada ou removida. A CLI a consulta antes e mostra o antes e o depois. O pedido 1061 ganhou o frete:

```console
$ inter-pj pix cob revisar pedido1061empresaexemplo2026 --valor 159,90 --solicitacao "Pedido 1061, com frete"
Cobrança Pix pedido1061empresaexemplo2026 a alterar
  Ambiente     PRODUÇÃO (conta real)
  Valor        R$ 149,90 → R$ 159,90
  Expira       2 horas após a criação
  Devedor      Cliente Exemplo Ltda (11.222.333/0001-81)
  Solicitação  → Pedido 1061, com frete
  Status       ativa
Alterar a cobrança? [s/N] s
Cobrança Pix alterada (revisão 1).

Cobrança Pix pedido1061empresaexemplo2026
  Status       ativa
  Valor        R$ 159,90
  Criada em    24/09/2026 10:05:12
  Expira em    24/09/2026 12:05:12
  Devedor      Cliente Exemplo Ltda (11.222.333/0001-81)
  Chave        pix@empresa.example
  Solicitação  Pedido 1061, com frete
  Revisão      1
  Location     qrcodepix.inter.example/qr/v2/cob/pedido1061empresaexemplo2026

Copia e cola  00020101021226840014br.gov.bcb.pix2562qrcodepix.inter.example/qr/v2/cob/pedido1061empresaexemplo20265204000053039865802BR5920EMPRESA EXEMPLO LTDA6014BELO HORIZONTE62070503***630446EF
```

`revisar` altera o valor, `--valor-alteravel sim` ou `nao`, a expiração, o devedor, a chave, a solicitação e as informações adicionais, que substituem as atuais. Como na criação, pede a confirmação; sem um terminal nem `--sim`, nem a consulta é feita. Revisar precisa também do escopo `cob.read`, para a consulta.

Depois, o cliente desistiu do pedido. `--remover` faz a cobrança deixar de poder ser paga, e o QR Code deixa de ser gerado: `--qrcode` mostra um aviso no lugar dele, e `--qrcode-png` termina com erro, sem gravar a imagem.

```console
$ inter-pj pix cob revisar pedido1061empresaexemplo2026 --remover
Cobrança Pix pedido1061empresaexemplo2026 a remover
  Ambiente  PRODUÇÃO (conta real)
  Valor     R$ 159,90
  Expira    2 horas após a criação
  Devedor   Cliente Exemplo Ltda (11.222.333/0001-81)
  Status    ativa
Remover a cobrança? [s/N] s
Cobrança Pix removida: ela não pode mais ser paga.

Cobrança Pix pedido1061empresaexemplo2026
  Status       removida pelo recebedor
  Valor        R$ 159,90
  Criada em    24/09/2026 10:05:12
  Expira em    24/09/2026 12:05:12
  Devedor      Cliente Exemplo Ltda (11.222.333/0001-81)
  Chave        pix@empresa.example
  Solicitação  Pedido 1061, com frete
  Revisão      2
  Location     qrcodepix.inter.example/qr/v2/cob/pedido1061empresaexemplo2026

Copia e cola  00020101021226840014br.gov.bcb.pix2562qrcodepix.inter.example/qr/v2/cob/pedido1061empresaexemplo20265204000053039865802BR5920EMPRESA EXEMPLO LTDA6014BELO HORIZONTE62070503***630446EF

$ inter-pj pix cob consultar pedido1061empresaexemplo2026 --qrcode
Cobrança Pix pedido1061empresaexemplo2026
  Status       removida pelo recebedor
  Valor        R$ 159,90
  Criada em    24/09/2026 10:05:12
  Expira em    24/09/2026 12:05:12
  Devedor      Cliente Exemplo Ltda (11.222.333/0001-81)
  Chave        pix@empresa.example
  Solicitação  Pedido 1061, com frete
  Revisão      2
  Location     qrcodepix.inter.example/qr/v2/cob/pedido1061empresaexemplo2026

Copia e cola  00020101021226840014br.gov.bcb.pix2562qrcodepix.inter.example/qr/v2/cob/pedido1061empresaexemplo20265204000053039865802BR5920EMPRESA EXEMPLO LTDA6014BELO HORIZONTE62070503***630446EF
aviso: a cobrança está removida pelo recebedor: o QR Code não serve mais para pagar
```

Uma cobrança paga ou removida é recusada antes de qualquer alteração:

```console
$ inter-pj pix cob revisar pedido1061empresaexemplo2026 --valor 149,90 --sim
erro: a cobrança já foi removida: não pode ser alterada
```

## Uma cobrança paga

A consulta mostra também os Pix que pagaram a cobrança, com os horários no fuso local. A do pedido 1053 foi paga no dia 2, pelo Pix que está no [extrato](saldo-e-extrato.md):

```console
$ inter-pj pix cob consultar pedido1053empresaexemplo2026
Cobrança Pix pedido1053empresaexemplo2026
  Status       concluída (paga)
  Valor        R$ 1.500,00
  Criada em    02/09/2026 09:10:00
  Expira em    02/09/2026 10:10:00
  Devedor      Cliente Exemplo Ltda (11.222.333/0001-81)
  Chave        pix@empresa.example
  Solicitação  Pedido 1053
  Revisão      0
  Location     qrcodepix.inter.example/qr/v2/cob/pedido1053empresaexemplo2026

Pix recebidos
Horário                    Valor  Devolvido  endToEndId
02/09/2026 09:15:38  R$ 1.500,00             E12345678202609021215Po0iU9yT8rE

Copia e cola  00020101021226840014br.gov.bcb.pix2562qrcodepix.inter.example/qr/v2/cob/pedido1053empresaexemplo20265204000053039865802BR5920EMPRESA EXEMPLO LTDA6014BELO HORIZONTE62070503***63048223
```

Um Pix recebido pode ser devolvido, como no guia do [Pix](pix.md#devolver-um-pix-recebido).

## Quando o resultado fica incerto

Se a resposta da criação se perder (um tempo esgotado, um erro 5xx), a cobrança pode ter sido criada. A CLI sai com o código 9 e mostra como consultá-la e como repetir a criação com o mesmo txid:

```console
$ inter-pj pix cob criar --chave pix@empresa.example --valor 89,90 --expiracao 1d \
    --solicitacao "Pedido 1062" --txid pedido1062empresaexemplo2026 --sim
*** PRODUÇÃO: a cobrança vale de verdade ***
Cobrança Pix a criar
  Ambiente     PRODUÇÃO (conta real)
  Valor        R$ 89,90 (oitenta e nove reais e noventa centavos)
  Chave        pix@empresa.example (e-mail)
  Expira       1 dia após a criação
  Solicitação  Pedido 1062
  txid         pedido1062empresaexemplo2026
erro: PUT /pix/v2/cob/{txid} respondeu 504 (tempo esgotado no gateway)
dica: a cobrança pode ter sido criada; com o mesmo txid, a API não cria outra
dica: confira com: inter-pj pix cob consultar pedido1062empresaexemplo2026
dica: ou repita o comando com --txid pedido1062empresaexemplo2026

$ inter-pj pix cob consultar pedido1062empresaexemplo2026
Cobrança Pix pedido1062empresaexemplo2026
  Status       ativa
  Valor        R$ 89,90
  Criada em    24/09/2026 10:12:12
  Expira em    25/09/2026 10:12:12
  Chave        pix@empresa.example
  Solicitação  Pedido 1062
  Revisão      0
  Location     qrcodepix.inter.example/qr/v2/cob/pedido1062empresaexemplo2026

Copia e cola  00020101021226840014br.gov.bcb.pix2562qrcodepix.inter.example/qr/v2/cob/pedido1062empresaexemplo20265204000053039865802BR5920EMPRESA EXEMPLO LTDA6014BELO HORIZONTE62070503***6304F480
```

A cobrança foi criada: o cliente já pode pagá-la. Se a consulta não a achasse, repetir o comando com o mesmo `--txid` seria seguro, porque a API não cria uma segunda cobrança com ele.

## As cobranças de um período

`pix cob listar` mostra as cobranças criadas num período, por padrão os últimos 30 dias até agora:

```console
$ inter-pj pix cob listar --inicio 2026-08-01 --fim 2026-09-30
Cobranças Pix imediatas criadas de 01/08/2026 00:00 a 30/09/2026 23:59

Criada em            Status                         Valor  Devedor               txid
31/08/2026 15:30:00  removida pelo recebedor    R$ 320,00  Fulano de Tal         pedido1051empresaexemplo2026
02/09/2026 09:10:00  concluída (paga)         R$ 1.500,00  Cliente Exemplo Ltda  pedido1053empresaexemplo2026
24/09/2026 10:05:12  removida pelo recebedor    R$ 159,90  Cliente Exemplo Ltda  pedido1061empresaexemplo2026
24/09/2026 10:12:12  ativa                       R$ 89,90                        pedido1062empresaexemplo2026

4 cobranças · R$ 2.069,80 · pagas R$ 1.500,00

$ inter-pj pix cob listar --inicio 2026-09-01 --fim 2026-09-30 --status ativa
Cobranças Pix imediatas criadas de 01/09/2026 00:00 a 30/09/2026 23:59 (ativa)

Criada em            Status     Valor  Devedor  txid
24/09/2026 10:12:12  ativa   R$ 89,90           pedido1062empresaexemplo2026

1 cobrança · R$ 89,90
```

`--inicio` e `--fim` aceitam uma data (o dia inteiro, no fuso local) ou a data e a hora com o fuso (`2026-09-24T08:00:00-03:00`). Os filtros são `--status` (`ativa`, `concluida`, `removida-pelo-usuario` ou `removida-pelo-psp`), `--documento` (o CPF ou o CNPJ do devedor) e `--com-location` ou `--sem-location`. A listagem lê todas as páginas, de 1.000 cobranças cada; `--pagina N` (a primeira é 0), com `--itens-por-pagina`, traz uma só. Em `--formato csv`, as colunas têm os nomes da API (`valor.original`, `devedor.nome`, `pixCopiaECola`), com os códigos e os horários como a API os envia.
