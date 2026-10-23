# Formatos de saída

Os comandos de consulta escrevem, por padrão, um texto para pessoas. Com `--formato`, o
mesmo resultado sai em um formato para programas.

| Formato | Para quê |
| --- | --- |
| `texto` (padrão) | leitura: tabelas alinhadas, valores em `R$ 1.234,56`, datas `DD/MM/AAAA` |
| `json` (ou `--json`) | automação: os nomes de campo da API e valores numéricos exatos |
| `csv` | planilhas e scripts (`saldo` e `extrato`): RFC 4180, datas `AAAA-MM-DD`, ponto decimal, saídas com valor negativo |
| `ofx` | programas de contabilidade, ERPs e de finanças pessoais (só o `extrato`): OFX 1.02, como o dos bancos brasileiros |

```console
$ inter-pj saldo --json
$ inter-pj extrato --formato csv > agosto.csv
$ inter-pj extrato --formato ofx -o agosto.ofx
```

## JSON

O JSON usa os mesmos nomes de campo da API do Inter (`disponivel`, `bloqueadoCheque`...), e os
valores monetários saem como números exatos, sem arredondamento de ponto flutuante: `2850.55`
continua `2850.55`.

```console
$ inter-pj saldo --json | jq .disponivel
2850.55
```

## CSV

O CSV segue a RFC 4180: cabeçalho, quebra de linha CRLF e aspas quando necessárias. Usa os
códigos e os nomes de campo da API e valores **com sinal** (as saídas são negativas), para
somar direto na planilha.

### No Excel em português

O Excel em português espera ponto e vírgula entre as colunas e vírgula decimal. Para esse
formato, use `--separador ';'`:

```console
$ inter-pj extrato --formato csv --separador ';' > agosto.csv
```

O arquivo sai com ponto e vírgula, vírgula decimal e UTF-8 com BOM, que o Excel abre sem
estragar os acentos.

### Fórmulas em textos de terceiros

Algumas descrições vêm de terceiros: a mensagem de um Pix recebido, por exemplo. Um texto que
começa com `=`, `+`, `-` ou `@` poderia ser executado como fórmula pela planilha (*CSV
injection*). Por isso a CLI coloca um apóstrofo na frente desses textos.

## OFX

O OFX é o formato de extrato que os programas de contabilidade, os ERPs e os de finanças
pessoais importam. A CLI escreve o OFX 1.02 (SGML) em Windows-1252, como os bancos
brasileiros, para o `extrato` e o `extrato completo`:

```console
$ inter-pj extrato --inicio 2026-08-01 --fim 2026-08-31 --formato ofx -o agosto.ofx
Extrato de 01/08/2026 a 31/08/2026 salvo em agosto.ofx (42 transações, 9,8 KB)
$ inter-pj extrato --inicio 2026-01-01 --fim 2026-06-30 --dividir-periodo --formato ofx > semestre.ofx
$ inter-pj extrato completo --inicio 2026-08-01 --fim 2026-08-31 --todas-paginas --formato ofx -o agosto.ofx
```

O arquivo traz:

- **a conta**: o banco (`077`), a agência (`0001`) e a conta do perfil, das chaves
  `numero_conta` e `digito_conta` do [arquivo de configuração](../referencia/configuracao.md).
  Sem elas, a CLI para antes de chamar a API;
- **as transações do período**, cada uma com o tipo (`CREDIT` ou `DEBIT`, ou um mais
  específico quando o tipo da transação permite: `XFER` para Pix e transferências, `PAYMENT`
  para pagamentos e impostos, `FEE` para tarifas, `INT` para juros...), a data, o valor com
  sinal (as saídas são negativas) e a descrição; no `extrato completo`, também a contraparte;
- **o saldo ao fim do período**, o mesmo de `inter-pj saldo --data` com o último dia. Num
  período que termina hoje, é o saldo do momento da consulta.

Cada transação tem um identificador, o `FITID`, pelo qual o programa reconhece o que já
importou. No `extrato completo`, é o id da transação no banco. No `extrato`, que não tem id,
é calculado da data, do valor e da descrição, e numerado entre as transações iguais do mesmo
dia (duas tarifas iguais no mesmo dia são duas transações). Ele é o mesmo a cada execução:
importar de novo o mesmo mês, ou um período que se sobrepõe a outro, não repete nada. Os
dois comandos, porém, dão identificadores diferentes à mesma transação: para uma conta, use
sempre o mesmo.

!!! info "Detalhes"
    - O `extrato completo` precisa de `--todas-paginas`: o OFX traz o período inteiro.
    - Com `-o` (ou `--saida`), o arquivo é gravado com permissão `600`, e um arquivo existente
      só é substituído com `--sobrescrever`. Sem `-o`, o OFX vai para a saída padrão; num
      terminal, aparece como texto, para conferir.
    - A data das transações vai ao meio-dia, no fuso de Brasília: o banco informa só o dia, e
      assim um programa em outro fuso não a muda de dia.
    - Os textos ficam numa linha só; caracteres que o Windows-1252 não tem, como os emojis,
      viram `?`.

## Onde a saída vai

O resultado sai em `stdout`. As mensagens de erro, os avisos e os detalhes de `-v` saem em
`stderr`, de modo que `inter-pj saldo --json > saldo.json` grava só o JSON.
