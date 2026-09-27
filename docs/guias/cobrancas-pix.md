# Cobranças Pix

A API Pix cria cobranças com QR Code dinâmico, que o cliente paga pelo app de qualquer banco. A cobrança imediata (`pix cob`) é para pagar na hora, até expirar; a [cobrança com vencimento](#cobranças-com-vencimento) (`pix cobv`), até a data de vencimento, com multa, juros e desconto. Criar uma cobrança não tira dinheiro da conta, mas ela vale de verdade: os trilhos são o resumo, a confirmação num terminal (ou `--sim`) e `--simular`, e o limite por operação não se aplica.

Os exemplos são da Empresa Exemplo Ltda, uma empresa fictícia, no perfil de produção, que recebe na chave `pix@empresa.example`. Criar e alterar uma cobrança imediata precisam do escopo `cob.write`, e consultar e listar, do `cob.read`.

- [Criar uma cobrança imediata](#criar-uma-cobrança-imediata)
- [O txid](#o-txid)
- [Alterar e remover](#alterar-e-remover)
- [Uma cobrança paga](#uma-cobrança-paga)
- [Quando o resultado fica incerto](#quando-o-resultado-fica-incerto)
- [As cobranças de um período](#as-cobranças-de-um-período)
- [Cobranças com vencimento](#cobranças-com-vencimento)
  - [Criar uma cobrança com vencimento](#criar-uma-cobrança-com-vencimento)
  - [A validade e os encargos](#a-validade-e-os-encargos)
  - [Por um arquivo](#por-um-arquivo)
  - [Alterar uma cobrança com vencimento](#alterar-uma-cobrança-com-vencimento)
  - [As cobranças com vencimento de um período](#as-cobranças-com-vencimento-de-um-período)

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

## Cobranças com vencimento

A cobrança com vencimento (`pix cobv`) é o boleto do Pix: vale até a data de vencimento, com desconto para quem paga antes e multa e juros para quem paga depois, e tem sempre um devedor, com o CPF ou o CNPJ. O txid, a confirmação, `--simular`, o QR Code, a remoção e o resultado incerto funcionam como na cobrança imediata. Criar e alterar precisam do escopo `cobv.write`, e consultar e listar, do `cobv.read`.

### Criar uma cobrança com vencimento

A nota NF-0931 da Cliente Exemplo Ltda vence no dia 20 de outubro, com R$ 50,00 de desconto para quem pagar até o dia 15, e 2% de multa e 1% de juros ao mês para quem pagar depois:

```console
$ inter-pj pix cobv criar --chave pix@empresa.example --valor 1.850,00 --vencimento 2026-10-20 \
    --devedor-documento 11.222.333/0001-81 --devedor-nome "Cliente Exemplo Ltda" \
    --devedor-endereco "Avenida Brasil, 1200, sala 3" --devedor-cidade "Belo Horizonte" \
    --devedor-uf MG --devedor-cep 30110-000 --devedor-email financeiro@cliente.example \
    --multa 2% --juros 1% --desconto 50,00@2026-10-15 --solicitacao "Referente à NF-0931" \
    --txid nota0931empresaexemplo2026
*** PRODUÇÃO: a cobrança vale de verdade ***
Cobrança Pix com vencimento a criar
  Ambiente     PRODUÇÃO (conta real)
  Valor        R$ 1.850,00 (mil oitocentos e cinquenta reais)
  Vencimento   20/10/2026
  Validade     até 19/11/2026, 30 dias após o vencimento (padrão da API)
  Chave        pix@empresa.example (e-mail)
  Devedor      Cliente Exemplo Ltda (11.222.333/0001-81)
  Endereço     Avenida Brasil, 1200, sala 3 - Belo Horizonte/MG - CEP 30110-000
  E-mail       financeiro@cliente.example
  Multa        2%
  Juros        1% ao mês (dias corridos)
  Desconto     R$ 50,00 até 15/10/2026
  Solicitação  Referente à NF-0931
  txid         nota0931empresaexemplo2026
Criar a cobrança? [s/N] s
Cobrança Pix com vencimento criada.

Cobrança Pix com vencimento nota0931empresaexemplo2026
  Status       ativa
  Valor        R$ 1.850,00
  Vencimento   20/10/2026
  Validade     até 19/11/2026, 30 dias após o vencimento
  Criada em    24/09/2026 10:20:07
  Devedor      Cliente Exemplo Ltda (11.222.333/0001-81)
  Endereço     Avenida Brasil, 1200, sala 3 - Belo Horizonte/MG - CEP 30110-000
  E-mail       financeiro@cliente.example
  Recebedor    Empresa Exemplo Ltda (11.444.777/0001-61)
  Chave        pix@empresa.example
  Multa        2%
  Juros        1% ao mês (dias corridos)
  Desconto     R$ 50,00 até 15/10/2026
  Solicitação  Referente à NF-0931
  Revisão      0
  Location     qrcodepix.inter.example/qr/v2/cobv/nota0931empresaexemplo2026

Copia e cola  00020101021226830014br.gov.bcb.pix2561qrcodepix.inter.example/qr/v2/cobv/nota0931empresaexemplo20265204000053039865802BR5920EMPRESA EXEMPLO LTDA6014BELO HORIZONTE62070503***63046191

Acompanhe com: inter-pj pix cobv consultar nota0931empresaexemplo2026
```

`pix cobv consultar` mostra a cobrança pelo txid, com os Pix que a pagaram, e desenha o QR Code com `--qrcode` ou o grava numa imagem com `--qrcode-png`, enquanto ela está ativa.

### A validade e os encargos

A validade (`--validade-apos-vencimento`, em dias corridos; o padrão da API é 30) é por quanto tempo depois do vencimento a cobrança ainda pode ser paga, com multa e juros. Com 0, ela não aceita pagamento atrasado, e a CLI avisa quando há multa ou juros que assim nunca valeriam. Os encargos aceitam um percentual (`2%`) ou um valor (`4,00`):

- `--multa`: por pagar depois do vencimento;
- `--juros`: um percentual ao mês (ou ao dia ou ao ano, com `--juros-periodo`) ou um valor por dia de atraso;
- `--abatimento`: vale qualquer que seja o dia do pagamento;
- `--desconto`: vale até o vencimento ou até a data depois do `@` (`2%@2026-10-15`), e pode ser repetido para até 3 datas, todas com percentuais ou todas com valores; `--desconto-por-dia` dá um desconto para cada dia pago antes do vencimento;
- `--dias-uteis`: os juros e o desconto por dia contam só os dias úteis.

O vencimento é hoje ou depois, o desconto vale até ele, e os valores fixos de desconto e abatimento são menores que o da cobrança; tudo é conferido antes de qualquer requisição. O devedor pode ter e-mail e endereço (`--devedor-email`, `--devedor-endereco`, `--devedor-cidade`, `--devedor-uf` e `--devedor-cep`).

### Por um arquivo

A cobrança pode vir de um arquivo JSON com os campos da API. `pix cobv modelo` imprime um exemplo com todos os campos, de dados fictícios, vencendo em 30 dias:

```console
$ inter-pj pix cobv modelo > cobv.json
```

Para a mensalidade de outubro do Fulano de Tal, o arquivo `cobv.json` fica assim, com a multa e os juros em valor e um desconto para cada dia pago antes do vencimento:

<!-- guia: arquivo cobv.json -->
```json
{
  "calendario": {"dataDeVencimento": "2026-10-10", "validadeAposVencimento": 10},
  "devedor": {
    "cpf": "123.456.789-09",
    "nome": "Fulano de Tal",
    "logradouro": "Rua da Bahia, 1000",
    "cidade": "Belo Horizonte",
    "uf": "MG",
    "cep": "30160-011"
  },
  "valor": {
    "original": "450,00",
    "multa": {"modalidade": 1, "valorPerc": "9,00"},
    "juros": {"modalidade": 1, "valorPerc": "0,15"},
    "desconto": {"modalidade": 3, "valorPerc": "0,50"}
  },
  "chave": "pix@empresa.example",
  "solicitacaoPagador": "Mensalidade de outubro",
  "infoAdicionais": [{"nome": "Contrato", "valor": "MS-0042"}]
}
```

```console
$ inter-pj pix cobv criar --arquivo cobv.json --txid mensalidade202610fulanodetal --qrcode-png mensalidade.png
*** PRODUÇÃO: a cobrança vale de verdade ***
Cobrança Pix com vencimento a criar
  Ambiente     PRODUÇÃO (conta real)
  Valor        R$ 450,00 (quatrocentos e cinquenta reais)
  Vencimento   10/10/2026
  Validade     até 20/10/2026, 10 dias após o vencimento
  Chave        pix@empresa.example (e-mail)
  Devedor      Fulano de Tal (123.456.789-09)
  Endereço     Rua da Bahia, 1000 - Belo Horizonte/MG - CEP 30160-011
  Multa        R$ 9,00
  Juros        R$ 0,15 por dia (dias corridos)
  Desconto     R$ 0,50 por dia de antecipação (dias corridos)
  Solicitação  Mensalidade de outubro
  Informações  Contrato: MS-0042
  txid         mensalidade202610fulanodetal
Criar a cobrança? [s/N] s
Cobrança Pix com vencimento criada.

Cobrança Pix com vencimento mensalidade202610fulanodetal
  Status       ativa
  Valor        R$ 450,00
  Vencimento   10/10/2026
  Validade     até 20/10/2026, 10 dias após o vencimento
  Criada em    24/09/2026 10:26:07
  Devedor      Fulano de Tal (123.456.789-09)
  Endereço     Rua da Bahia, 1000 - Belo Horizonte/MG - CEP 30160-011
  Recebedor    Empresa Exemplo Ltda (11.444.777/0001-61)
  Chave        pix@empresa.example
  Multa        R$ 9,00
  Juros        R$ 0,15 por dia (dias corridos)
  Desconto     R$ 0,50 por dia de antecipação (dias corridos)
  Solicitação  Mensalidade de outubro
  Revisão      0
  Location     qrcodepix.inter.example/qr/v2/cobv/mensalidade202610fulanodetal

Informações
  Contrato  MS-0042

Copia e cola  00020101021226850014br.gov.bcb.pix2563qrcodepix.inter.example/qr/v2/cobv/mensalidade202610fulanodetal5204000053039865802BR5920EMPRESA EXEMPLO LTDA6014BELO HORIZONTE62070503***6304E49B

Acompanhe com: inter-pj pix cobv consultar mensalidade202610fulanodetal
QR Code salvo em mensalidade.png (25,9 KB)
```

No arquivo, as modalidades dos encargos são números:

| Campo | Modalidades |
| --- | --- |
| `multa` e `abatimento` | 1, um valor fixo; 2, um percentual do valor |
| `juros` | 1, um valor por dia; 2, um percentual ao dia; 3, ao mês; 4, ao ano; de 5 a 8, os mesmos em dias úteis |
| `desconto` | 1, valores fixos até as datas de `descontoDataFixa`; 2, percentuais até as datas; 3, um valor por dia de antecipação; 4, por dia útil; 5, um percentual por dia de antecipação; 6, por dia útil |

Campos desconhecidos são recusados, e as mensagens apontam o campo (`cobv.json, campo "valor.desconto.descontoDataFixa[0].data": ...`); os valores podem ser números (`450.00`) ou textos (`"450,00"`), e o CEP e o CPF ou o CNPJ podem ter pontuação. Com `--arquivo -`, a cobrança vem da entrada padrão, e a confirmação exige `--sim`.

### Alterar uma cobrança com vencimento

`pix cobv revisar` consulta a cobrança, mostra o antes e o depois e muda só o que for informado. O que depende da cobrança atual é conferido depois da consulta, sem nenhuma alteração se falhar. A Cliente Exemplo pediu mais prazo para o desconto, mas o desconto não vale depois do vencimento:

```console
$ inter-pj pix cobv revisar nota0931empresaexemplo2026 --desconto 50,00@2026-10-25 --sim
erro: --desconto: o desconto vale até uma data no vencimento ou antes dele
```

Com o vencimento adiado junto, a alteração passa, e a validade continua de 30 dias, agora depois do novo vencimento:

```console
$ inter-pj pix cobv revisar nota0931empresaexemplo2026 --vencimento 2026-10-30 --desconto 50,00@2026-10-25
Cobrança Pix com vencimento nota0931empresaexemplo2026 a alterar
  Ambiente    PRODUÇÃO (conta real)
  Valor       R$ 1.850,00
  Vencimento  20/10/2026 → 30/10/2026
  Validade    até 19/11/2026, 30 dias após o vencimento → até 29/11/2026, 30 dias após o vencimento
  Devedor     Cliente Exemplo Ltda (11.222.333/0001-81)
  Multa       2%
  Juros       1% ao mês (dias corridos)
  Desconto    R$ 50,00 até 15/10/2026 → R$ 50,00 até 25/10/2026
  Status      ativa
Alterar a cobrança? [s/N] s
Cobrança Pix com vencimento alterada (revisão 1).

Cobrança Pix com vencimento nota0931empresaexemplo2026
  Status       ativa
  Valor        R$ 1.850,00
  Vencimento   30/10/2026
  Validade     até 29/11/2026, 30 dias após o vencimento
  Criada em    24/09/2026 10:20:07
  Devedor      Cliente Exemplo Ltda (11.222.333/0001-81)
  Endereço     Avenida Brasil, 1200, sala 3 - Belo Horizonte/MG - CEP 30110-000
  E-mail       financeiro@cliente.example
  Recebedor    Empresa Exemplo Ltda (11.444.777/0001-61)
  Chave        pix@empresa.example
  Multa        2%
  Juros        1% ao mês (dias corridos)
  Desconto     R$ 50,00 até 25/10/2026
  Solicitação  Referente à NF-0931
  Revisão      1
  Location     qrcodepix.inter.example/qr/v2/cobv/nota0931empresaexemplo2026

Copia e cola  00020101021226830014br.gov.bcb.pix2561qrcodepix.inter.example/qr/v2/cobv/nota0931empresaexemplo20265204000053039865802BR5920EMPRESA EXEMPLO LTDA6014BELO HORIZONTE62070503***63046191
```

Da mesma forma, um vencimento antes do fim do desconto atual pede também o novo `--desconto`. O devedor informado substitui o atual, com o e-mail e o endereço, e as informações adicionais (`--info`) substituem as atuais. Com `--remover`, a cobrança deixa de poder ser paga, e uma cobrança paga ou removida é recusada antes de qualquer alteração.

### As cobranças com vencimento de um período

`pix cobv listar` mostra as cobranças criadas num período, por padrão os últimos 30 dias até agora, com o vencimento de cada uma:

```console
$ inter-pj pix cobv listar --inicio 2026-08-01 --fim 2026-09-30
Cobranças Pix com vencimento criadas de 01/08/2026 00:00 a 30/09/2026 23:59

Vencimento  Status                         Valor  Devedor               txid
10/09/2026  removida pelo recebedor    R$ 450,00  Beltrana de Tal       mensalidade202609beltranadetal
10/10/2026  ativa                      R$ 450,00  Beltrana de Tal       mensalidade202610beltranadetal
30/10/2026  ativa                    R$ 1.850,00  Cliente Exemplo Ltda  nota0931empresaexemplo2026
10/10/2026  ativa                      R$ 450,00  Fulano de Tal         mensalidade202610fulanodetal

4 cobranças · R$ 3.200,00

$ inter-pj pix cobv listar --inicio 2026-08-01 --fim 2026-09-30 --documento 012.345.678-90
Cobranças Pix com vencimento criadas de 01/08/2026 00:00 a 30/09/2026 23:59 (devedor 012.345.678-90)

Vencimento  Status                       Valor  Devedor          txid
10/09/2026  removida pelo recebedor  R$ 450,00  Beltrana de Tal  mensalidade202609beltranadetal
10/10/2026  ativa                    R$ 450,00  Beltrana de Tal  mensalidade202610beltranadetal

2 cobranças · R$ 900,00
```

A listagem tem os filtros de `pix cob listar` e também `--lote ID`, o das cobranças criadas num lote. Em `--formato csv`, os encargos aparecem com a modalidade e o valor (`valor.multa.modalidade`, `valor.multa.valorPerc`), e os descontos por data, só no JSON.
