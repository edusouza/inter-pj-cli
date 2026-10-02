# Formatos de saída

Os comandos de consulta escrevem, por padrão, um texto para pessoas. Com `--formato`, o
mesmo resultado sai em um formato para programas.

| Formato | Para quê |
| --- | --- |
| `texto` (padrão) | leitura: tabelas alinhadas, valores em `R$ 1.234,56`, datas `DD/MM/AAAA` |
| `json` (ou `--json`) | automação: os nomes de campo da API e valores numéricos exatos |
| `csv` | planilhas e scripts (`saldo` e `extrato`): RFC 4180, datas `AAAA-MM-DD`, ponto decimal, saídas com valor negativo |

```console
$ inter-pj saldo --json
$ inter-pj extrato --formato csv > agosto.csv
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

## Onde a saída vai

O resultado sai em `stdout`. As mensagens de erro, os avisos e os detalhes de `-v` saem em
`stderr`, de modo que `inter-pj saldo --json > saldo.json` grava só o JSON.
