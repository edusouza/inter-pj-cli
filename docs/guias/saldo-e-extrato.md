# Saldo e extrato

As consultas da conta: o saldo, o extrato de um período, o extrato completo, com os detalhes de cada transação, e o extrato em PDF. Nenhum destes comandos movimenta dinheiro. Todos pedem ao Inter um token com o escopo `extrato.read`, que precisa estar marcado na integração.

Os exemplos são da conta da Empresa Exemplo Ltda, uma empresa fictícia, no perfil de produção. A [introdução dos guias](README.md) explica como eles são conferidos.

- [Saldo](#saldo)
- [Extrato de um período](#extrato-de-um-período)
- [Extrato completo](#extrato-completo)
- [Planilhas](#planilhas)
- [PDF](#pdf)

## Saldo

```console
$ inter-pj saldo
Saldo disponível          R$ 16.579,17
Bloqueado em cheque            R$ 0,00
Bloqueado judicialmente        R$ 0,00
Bloqueado administrativo       R$ 0,00
Limite                     R$ 5.000,00
```

Sem data, o saldo de agora: o disponível, os valores bloqueados e o limite da conta. Com `--data`, o saldo disponível ao fim daquele dia, para conferir o fechamento de um mês:

```console
$ inter-pj saldo --data 2026-08-31
Data da consulta    31/08/2026
Saldo disponível  R$ 16.279,17
```

Para um script, `--json` traz os nomes de campo da API e os valores com ponto decimal, e `--formato csv` uma linha com os mesmos campos (e a data consultada, quando há uma):

```console
$ inter-pj saldo --json
{
  "disponivel": 16579.17,
  "bloqueadoCheque": 0,
  "bloqueadoJudicialmente": 0,
  "bloqueadoAdministrativo": 0,
  "limite": 5000
}

$ inter-pj saldo --formato csv
dataSaldo,disponivel,bloqueadoCheque,bloqueadoJudicialmente,bloqueadoAdministrativo,limite,dataReferencia
,16579.17,0,0,0,5000,
```

Com o [`jq`](https://jqlang.org/), só o disponível:

<!-- guia: não executar -->
```console
$ inter-pj saldo --json | jq .disponivel
16579.17
```

## Extrato de um período

```console
$ inter-pj extrato --inicio 2026-08-01 --fim 2026-08-31
Extrato de 01/08/2026 a 31/08/2026

Data        Tipo               Descrição                                       Valor
03/08/2026  Pix                Pix recebido · Cliente Exemplo Ltda       R$ 1.500,00
05/08/2026  Pagamento          Pagamento efetuado · Energia Exemplo SA    -R$ 250,10
10/08/2026  Cobrança (boleto)  Boleto recebido · Beltrana de Tal           R$ 890,00
12/08/2026  Pix                Pix enviado · Fornecedor Exemplo SA      -R$ 1.200,00
14/08/2026  Compra no débito   Compra no débito · Papelaria Exemplo        -R$ 86,40
17/08/2026  Transferência      Transferência enviada · Fulano de Tal    -R$ 3.000,00
20/08/2026  Pix                Pix recebido · Cliente Exemplo Ltda       R$ 2.350,00
25/08/2026  Tarifa             Tarifa · Pacote de serviços                  -R$ 9,90
28/08/2026  Pix                Pix recebido · Sicrano de Tal               R$ 740,00

Entradas               R$ 5.480,00
Saídas                -R$ 4.546,40
Resultado do período     R$ 933,60
9 transações
```

Uma linha por transação, na ordem em que o banco as envia, com as saídas em valor negativo. No fim, o total das entradas, o das saídas e o resultado do período. As datas seguem o calendário do banco, o de Brasília.

Sem `--inicio` e `--fim`, o extrato é o dos últimos 30 dias, hoje incluído:

```console
$ inter-pj extrato
Extrato de 26/08/2026 a 24/09/2026

Data        Tipo  Descrição                                   Valor
28/08/2026  Pix   Pix recebido · Sicrano de Tal           R$ 740,00
02/09/2026  Pix   Pix recebido · Cliente Exemplo Ltda   R$ 1.500,00
08/09/2026  Pix   Pix enviado · Fornecedor Exemplo SA  -R$ 1.200,00

Entradas               R$ 2.240,00
Saídas                -R$ 1.200,00
Resultado do período   R$ 1.040,00
3 transações
```

Só com `--fim`, são os 30 dias até essa data; só com `--inicio`, de lá até hoje.

### Mais de 90 dias

A API aceita no máximo 90 dias por consulta, contando o primeiro e o último. A CLI confere o período antes de chamar a API. Com `--dividir-periodo`, ela consulta um período maior em partes consecutivas e junta tudo num extrato só:

```console
$ inter-pj extrato --inicio 2026-06-01 --fim 2026-08-31
erro: o período tem 92 dias; a API aceita no máximo 90 dias por consulta
dica: use --dividir-periodo para consultar em partes de até 90 dias

$ inter-pj extrato --inicio 2026-06-01 --fim 2026-08-31 --dividir-periodo
Extrato de 01/06/2026 a 31/08/2026

Data        Tipo               Descrição                                          Valor
10/06/2026  Pix                Pix recebido · Cliente Exemplo Ltda          R$ 2.800,00
25/06/2026  Tarifa             Tarifa · Pacote de serviços                     -R$ 9,90
06/07/2026  Pix                Pix recebido · Cliente Exemplo Ltda          R$ 3.200,00
15/07/2026  Pagamento          Pagamento efetuado · Fornecedor Exemplo SA    -R$ 480,00
27/07/2026  Tarifa             Tarifa · Pacote de serviços                     -R$ 9,90
03/08/2026  Pix                Pix recebido · Cliente Exemplo Ltda          R$ 1.500,00
05/08/2026  Pagamento          Pagamento efetuado · Energia Exemplo SA       -R$ 250,10
10/08/2026  Cobrança (boleto)  Boleto recebido · Beltrana de Tal              R$ 890,00
12/08/2026  Pix                Pix enviado · Fornecedor Exemplo SA         -R$ 1.200,00
14/08/2026  Compra no débito   Compra no débito · Papelaria Exemplo           -R$ 86,40
17/08/2026  Transferência      Transferência enviada · Fulano de Tal       -R$ 3.000,00
20/08/2026  Pix                Pix recebido · Cliente Exemplo Ltda          R$ 2.350,00
25/08/2026  Tarifa             Tarifa · Pacote de serviços                     -R$ 9,90
28/08/2026  Pix                Pix recebido · Sicrano de Tal                  R$ 740,00

Entradas              R$ 11.480,00
Saídas                -R$ 5.046,20
Resultado do período   R$ 6.433,80
14 transações
```

## Extrato completo

`extrato completo` traz os detalhes de cada transação: quem pagou ou recebeu um Pix, os dados de um boleto, de um pagamento, de uma transferência. A tabela mostra a contraparte: quem pagou, nas entradas, e quem recebeu, nas saídas.

```console
$ inter-pj extrato completo --inicio 2026-08-01 --fim 2026-08-31
Extrato completo de 01/08/2026 a 31/08/2026

Data        Tipo               Descrição                                Contraparte                   Valor
03/08/2026  Pix                Pix recebido · Cliente Exemplo Ltda      Cliente Exemplo Ltda    R$ 1.500,00
05/08/2026  Pagamento          Pagamento efetuado · Energia Exemplo SA  Energia Exemplo SA       -R$ 250,10
10/08/2026  Cobrança (boleto)  Boleto recebido · Beltrana de Tal        Beltrana de Tal           R$ 890,00
12/08/2026  Pix                Pix enviado · Fornecedor Exemplo SA      Fornecedor Exemplo SA  -R$ 1.200,00
14/08/2026  Compra no débito   Compra no débito · Papelaria Exemplo     Papelaria Exemplo         -R$ 86,40
17/08/2026  Transferência      Transferência enviada · Fulano de Tal    Fulano de Tal          -R$ 3.000,00
20/08/2026  Pix                Pix recebido · Cliente Exemplo Ltda      Cliente Exemplo Ltda    R$ 2.350,00
25/08/2026  Tarifa             Tarifa · Pacote de serviços                                         -R$ 9,90
28/08/2026  Pix                Pix recebido · Sicrano de Tal            Sicrano de Tal            R$ 740,00

Página 1 de 1 · 9 de 9 transações
```

### Páginas

A API entrega o extrato completo em páginas de 50 transações, numeradas a partir de 0; `--tamanho-pagina` muda o tamanho, até 10.000. A CLI mostra uma página por vez e avisa quando há outras:

```console
$ inter-pj extrato completo --inicio 2026-08-01 --fim 2026-08-31 --tamanho-pagina 5
Extrato completo de 01/08/2026 a 31/08/2026

Data        Tipo               Descrição                                Contraparte                   Valor
03/08/2026  Pix                Pix recebido · Cliente Exemplo Ltda      Cliente Exemplo Ltda    R$ 1.500,00
05/08/2026  Pagamento          Pagamento efetuado · Energia Exemplo SA  Energia Exemplo SA       -R$ 250,10
10/08/2026  Cobrança (boleto)  Boleto recebido · Beltrana de Tal        Beltrana de Tal           R$ 890,00
12/08/2026  Pix                Pix enviado · Fornecedor Exemplo SA      Fornecedor Exemplo SA  -R$ 1.200,00
14/08/2026  Compra no débito   Compra no débito · Papelaria Exemplo     Papelaria Exemplo         -R$ 86,40

Página 1 de 2 · 5 de 9 transações
há mais páginas: use --pagina 1 ou --todas-paginas

$ inter-pj extrato completo --inicio 2026-08-01 --fim 2026-08-31 --tamanho-pagina 5 --pagina 1
Extrato completo de 01/08/2026 a 31/08/2026

Data        Tipo           Descrição                              Contraparte                  Valor
17/08/2026  Transferência  Transferência enviada · Fulano de Tal  Fulano de Tal         -R$ 3.000,00
20/08/2026  Pix            Pix recebido · Cliente Exemplo Ltda    Cliente Exemplo Ltda   R$ 2.350,00
25/08/2026  Tarifa         Tarifa · Pacote de serviços                                      -R$ 9,90
28/08/2026  Pix            Pix recebido · Sicrano de Tal          Sicrano de Tal           R$ 740,00

Página 2 de 2 · 4 de 9 transações
```

`--todas-paginas` lê todas e mostra um extrato só, com os totais, como o `extrato`; com ela, vale também `--dividir-periodo`. Acima de 10.000 transações no período, a CLI passa para o modo *scroll* da API, que permite uma leitura por vez em cada conta e expira depois de 6 minutos sem uso.

### Filtros

`--tipo-operacao` separa as entradas (`C`) das saídas (`D`), e `--tipo-transacao` escolhe um tipo de transação. Os Pix enviados de agosto a meados de setembro:

```console
$ inter-pj extrato completo --inicio 2026-08-01 --fim 2026-09-15 --tipo-operacao D --tipo-transacao pix
Extrato completo de 01/08/2026 a 15/09/2026

Data        Tipo  Descrição                            Contraparte                   Valor
12/08/2026  Pix   Pix enviado · Fornecedor Exemplo SA  Fornecedor Exemplo SA  -R$ 1.200,00
08/09/2026  Pix   Pix enviado · Fornecedor Exemplo SA  Fornecedor Exemplo SA  -R$ 1.200,00

Página 1 de 1 · 2 de 2 transações
```

Os tipos valem em maiúsculas ou minúsculas, e um tipo desconhecido mostra a lista:

```console
$ inter-pj extrato completo --inicio 2026-08-01 --fim 2026-08-31 --tipo-transacao boleto
erro: valor inválido 'boleto' para '--tipo-transacao <TIPO>': tipo de transação desconhecido "boleto"; use um de: ANTECIPACAO_RECEBIVEIS, ANTECIPACAO_RECEBIVEIS_CARTAO, BOLETO_COBRANCA, CAMBIO, CASHBACK, CHEQUE, COMPRA_DEBITO, DEBITO_AUTOMATICO, DEBITO_EM_CONTA, DEPOSITO_BOLETO, DOMICILIO_CARTAO, ESTORNO, FINANCIAMENTO, IMPOSTO, INTERPAG, INVESTIMENTO, JUROS, MAQUININHA_GRANITO, MULTA, OUTROS, PAGAMENTO, PIX, PROVENTOS, SAQUE, TARIFA, TRANSFERENCIA

Para mais informações, use '--help'.
```

Todas as entradas de junho a meados de setembro, lidas em duas partes:

```console
$ inter-pj extrato completo --inicio 2026-06-01 --fim 2026-09-15 --todas-paginas --dividir-periodo --tipo-operacao C
Extrato completo de 01/06/2026 a 15/09/2026

Data        Tipo               Descrição                            Contraparte                 Valor
10/06/2026  Pix                Pix recebido · Cliente Exemplo Ltda  Cliente Exemplo Ltda  R$ 2.800,00
06/07/2026  Pix                Pix recebido · Cliente Exemplo Ltda  Cliente Exemplo Ltda  R$ 3.200,00
03/08/2026  Pix                Pix recebido · Cliente Exemplo Ltda  Cliente Exemplo Ltda  R$ 1.500,00
10/08/2026  Cobrança (boleto)  Boleto recebido · Beltrana de Tal    Beltrana de Tal         R$ 890,00
20/08/2026  Pix                Pix recebido · Cliente Exemplo Ltda  Cliente Exemplo Ltda  R$ 2.350,00
28/08/2026  Pix                Pix recebido · Sicrano de Tal        Sicrano de Tal          R$ 740,00
02/09/2026  Pix                Pix recebido · Cliente Exemplo Ltda  Cliente Exemplo Ltda  R$ 1.500,00

Entradas              R$ 12.980,00
Saídas                     R$ 0,00
Resultado do período  R$ 12.980,00
7 transações
```

### Os detalhes

`--json` traz a página com os nomes de campo da API. Os campos de `detalhes` dependem do tipo da transação: os de um Pix não são os de uma transferência.

```console
$ inter-pj extrato completo --inicio 2026-08-17 --fim 2026-08-17 --json
{
  "totalPaginas": 1,
  "totalElementos": 1,
  "ultimaPagina": true,
  "primeiraPagina": true,
  "tamanhoPagina": 50,
  "numeroDeElementos": 1,
  "transacoes": [
    {
      "idTransacao": "310000111",
      "dataInclusao": "2026-08-17 15:10:00",
      "dataTransacao": "2026-08-17",
      "tipoTransacao": "TRANSFERENCIA",
      "tipoOperacao": "D",
      "valor": 3000,
      "titulo": "Transferência enviada",
      "descricao": "Fulano de Tal",
      "detalhes": {
        "descricaoTransferencia": "Pró-labore de agosto",
        "bancoRecebedor": "Banco Exemplo",
        "contaBancariaRecebedor": "7654321",
        "cpfCnpjRecebedor": "12345678909",
        "nomeRecebedor": "Fulano de Tal",
        "tipoDetalhe": "TRANSFERENCIA",
        "agenciaRecebedor": "0001"
      }
    }
  ]
}
```

## Planilhas

`--formato csv` segue a RFC 4180, com as datas em `AAAA-MM-DD`, ponto decimal e as saídas em valor negativo. As colunas são os campos da API:

```console
$ inter-pj extrato --inicio 2026-08-01 --fim 2026-08-31 --formato csv
dataEntrada,tipoTransacao,tipoOperacao,titulo,descricao,valor
2026-08-03,PIX,C,Pix recebido,Cliente Exemplo Ltda,1500.00
2026-08-05,PAGAMENTO,D,Pagamento efetuado,Energia Exemplo SA,-250.10
2026-08-10,BOLETO_COBRANCA,C,Boleto recebido,Beltrana de Tal,890.00
2026-08-12,PIX,D,Pix enviado,Fornecedor Exemplo SA,-1200.00
2026-08-14,COMPRA_DEBITO,D,Compra no débito,Papelaria Exemplo,-86.40
2026-08-17,TRANSFERENCIA,D,Transferência enviada,Fulano de Tal,-3000.00
2026-08-20,PIX,C,Pix recebido,Cliente Exemplo Ltda,2350.00
2026-08-25,TARIFA,D,Tarifa,Pacote de serviços,-9.90
2026-08-28,PIX,C,Pix recebido,Sicrano de Tal,740.00
```

O CSV do extrato completo traz também o identificador de cada transação e, dos detalhes, a contraparte, o CPF ou CNPJ dela, o `endToEndId` de um Pix e o código de barras de um pagamento:

```console
$ inter-pj extrato completo --inicio 2026-08-01 --fim 2026-08-31 --todas-paginas --formato csv
idTransacao,dataTransacao,dataInclusao,tipoTransacao,tipoOperacao,titulo,descricao,numeroDocumento,valor,contraparte,documentoContraparte,endToEndId,codigoBarras
310000106,2026-08-03,2026-08-03,PIX,C,Pix recebido,Cliente Exemplo Ltda,,1500.00,Cliente Exemplo Ltda,11222333000181,E12345678202608031705a1B2c3D4e5F,
310000107,2026-08-05,2026-08-05,PAGAMENTO,D,Pagamento efetuado,Energia Exemplo SA,,-250.10,Energia Exemplo SA,,,
310000108,2026-08-10,2026-08-10,BOLETO_COBRANCA,C,Boleto recebido,Beltrana de Tal,,890.00,Beltrana de Tal,01234567890,,
310000109,2026-08-12,2026-08-12,PIX,D,Pix enviado,Fornecedor Exemplo SA,,-1200.00,Fornecedor Exemplo SA,,E12345678202608121303Qw8eR4tY6uI,
310000110,2026-08-14,2026-08-14,COMPRA_DEBITO,D,Compra no débito,Papelaria Exemplo,,-86.40,Papelaria Exemplo,,,
310000111,2026-08-17,2026-08-17,TRANSFERENCIA,D,Transferência enviada,Fulano de Tal,,-3000.00,Fulano de Tal,12345678909,,
310000112,2026-08-20,2026-08-20,PIX,C,Pix recebido,Cliente Exemplo Ltda,,2350.00,Cliente Exemplo Ltda,11222333000181,E12345678202608201431Zx9cV8bN7mA,
310000113,2026-08-25,2026-08-25,TARIFA,D,Tarifa,Pacote de serviços,,-9.90,,,,
310000114,2026-08-28,2026-08-28,PIX,C,Pix recebido,Sicrano de Tal,,740.00,Sicrano de Tal,11900000083,E12345678202608282202Lk5jH3gF1dS,
```

Para o Excel em português, use `--separador ';'`: ponto e vírgula entre as colunas, vírgula decimal e UTF-8 com BOM, para os acentos. Grave num arquivo e abra-o no Excel:

```console
$ inter-pj extrato completo --inicio 2026-08-01 --fim 2026-08-31 --todas-paginas --formato csv --separador ';' > agosto.csv
```

Textos de terceiros que começam com `=`, `+`, `-` ou `@`, como a mensagem de um Pix, recebem um apóstrofo no CSV, para que a planilha não os execute como fórmula.

## PDF

`extrato pdf` grava o extrato do período no PDF do Inter:

```console
$ inter-pj extrato pdf --inicio 2026-08-01 --fim 2026-08-31
Extrato de 01/08/2026 a 31/08/2026 salvo em extrato-2026-08-01-a-2026-08-31.pdf (0,6 KB)
```

No Linux e no macOS, o arquivo tem permissão `600`: só o seu usuário o lê. Um arquivo que já existe só é substituído com `--sobrescrever`:

```console
$ inter-pj extrato pdf --inicio 2026-08-01 --fim 2026-08-31
erro: o arquivo extrato-2026-08-01-a-2026-08-31.pdf já existe; use --sobrescrever para substituí-lo

$ inter-pj extrato pdf --inicio 2026-08-01 --fim 2026-08-31 --sobrescrever
Extrato de 01/08/2026 a 31/08/2026 salvo em extrato-2026-08-01-a-2026-08-31.pdf (0,6 KB)
```

`--saida` escolhe outro nome:

```console
$ inter-pj extrato pdf --inicio 2026-08-01 --fim 2026-08-31 --saida agosto.pdf
Extrato de 01/08/2026 a 31/08/2026 salvo em agosto.pdf (0,6 KB)
```

`--saida -` manda o PDF para a saída padrão, para outro programa:

<!-- guia: não executar -->
```console
$ inter-pj extrato pdf --inicio 2026-08-01 --fim 2026-08-31 --saida - | lpr
```

Como no extrato, o período de um PDF vai até 90 dias; para um ano, gere um PDF por trimestre.
