# Os primeiros comandos

Com o [perfil configurado](configurar.md) e o `INTER_CLIENT_SECRET` definido, rode, no sandbox:

```console
$ inter-pj auth token --escopo extrato.read
```

Esse comando só valida as credenciais: pede (ou reaproveita do cache) um token e mostra os
escopos e a validade. Se ele funciona, o certificado, a chave, o `client_id` e o
`client_secret` estão certos.

## O saldo

```console
$ inter-pj saldo
Saldo disponível          R$ 2.850,55
Bloqueado em cheque         R$ 240,25
Bloqueado judicialmente     R$ 510,35
Bloqueado administrativo      R$ 0,00
Limite                    R$ 1.000,00
```

O saldo disponível ao fim de um dia:

```console
$ inter-pj saldo --data 2026-08-31
```

## O extrato

Sem opções, o extrato traz os últimos 30 dias, hoje incluído:

```console
$ inter-pj extrato
$ inter-pj extrato --inicio 2026-08-01 --fim 2026-08-31
```

Mais sobre o extrato, com os detalhes de cada transação e o PDF, no
[guia de saldo e extrato](../guias/saldo-e-extrato.md).

## Para programas

Todo comando de consulta tem uma saída para máquinas:

```console
$ inter-pj saldo --json
$ inter-pj extrato --formato csv
```

Veja os [formatos de saída](../guias/formatos-de-saida.md).

## Outro perfil, outro ambiente

```console
$ inter-pj --perfil producao saldo
```

Não há ambiente padrão: o perfil diz se é `sandbox` ou `producao`. Veja
[perfis e ambientes](../guias/perfis-e-ambientes.md).

## A ajuda

```console
$ inter-pj --help
$ inter-pj extrato completo --help
```

A [referência de comandos](../referencia/comandos.md) reúne a ajuda de todos eles.
