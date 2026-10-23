# Saldo e extrato

## O saldo

```console
$ inter-pj saldo                        # saldo atual, bloqueios e limite
$ inter-pj saldo --data 2026-08-31      # saldo disponível ao fim do dia
$ inter-pj saldo --json                 # JSON com os nomes de campo da API
```

## O extrato de um período

```console
$ inter-pj extrato                                          # últimos 30 dias, hoje incluído
$ inter-pj extrato --inicio 2026-08-01 --fim 2026-08-31
$ inter-pj extrato --inicio 2026-01-01 --fim 2026-12-31 --dividir-periodo
```

O resultado traz as transações do período e, no fim, as **entradas**, as **saídas** e o
**resultado**. As saídas aparecem com valor negativo.

!!! info "No máximo 90 dias por consulta"
    A API aceita **no máximo 90 dias** por consulta, contando o primeiro e o último dia. A CLI
    confere o período antes de chamar a API. Para períodos maiores, `--dividir-periodo` consulta
    em partes consecutivas.

## O extrato completo

O `extrato completo` traz os detalhes de cada transação: o pagador ou o recebedor de um Pix,
os dados de um boleto, de um pagamento e assim por diante.

```console
$ inter-pj extrato completo --inicio 2026-08-01 --fim 2026-08-31
$ inter-pj extrato completo --tipo-operacao D --tipo-transacao pix --pagina 1 --tamanho-pagina 100
$ inter-pj extrato completo --inicio 2026-08-01 --fim 2026-08-31 --todas-paginas --formato csv > agosto.csv
```

| Opção | O que faz |
| --- | --- |
| `--tipo-operacao C` ou `D` | só as entradas (`C`) ou só as saídas (`D`) |
| `--tipo-transacao TIPO` | só um tipo de transação: `PIX`, `PAGAMENTO`, `TRANSFERENCIA`, `BOLETO_COBRANCA`, `TARIFA`... |
| `--pagina N`, `--tamanho-pagina N` | a paginação manual: a página a partir de 0, com até 10.000 transações |
| `--todas-paginas` | percorre todas as páginas |
| `--dividir-periodo` | divide períodos maiores que 90 dias (requer `--todas-paginas`) |

Com `--todas-paginas`, acima de 10.000 transações a CLI passa para o modo *scroll* da API.
O scroll é um por conta e expira depois de 6 minutos sem uso. Os detalhes de por que a
paginação comum não chega lá estão na [arquitetura](../por-dentro/arquitetura.md).

## O extrato em PDF

```console
$ inter-pj extrato pdf --inicio 2026-08-01 --fim 2026-08-31
$ inter-pj extrato pdf --inicio 2026-08-01 --fim 2026-08-31 --saida - | lpr
```

Sem `--saida`, o arquivo se chama `extrato-<início>-a-<fim>.pdf`, como em
`extrato-2026-08-01-a-2026-08-31.pdf`. Com `--saida -`, o PDF vai para a saída padrão.

O PDF é gravado com permissão `600`, e a CLI **nunca substitui um arquivo existente** sem
`--sobrescrever`.

## O extrato em OFX, para a contabilidade

```console
$ inter-pj extrato --inicio 2026-08-01 --fim 2026-08-31 --formato ofx -o agosto.ofx
$ inter-pj extrato completo --inicio 2026-08-01 --fim 2026-08-31 --todas-paginas --formato ofx -o agosto.ofx
```

O OFX é o formato que os programas de contabilidade, os ERPs e os de finanças pessoais
importam. Ele leva a conta do perfil (`numero_conta` e `digito_conta`), as transações e o
saldo do fim do período, e cada transação tem um identificador que é o mesmo a cada
execução, para o programa não importar duas vezes o que já tem. Os detalhes estão nos
[formatos de saída](formatos-de-saida.md#ofx).

## Para planilhas e programas

O saldo e o extrato têm saída em JSON e em CSV, inclusive no formato do Excel em português,
e o extrato também em OFX: veja os [formatos de saída](formatos-de-saida.md).
