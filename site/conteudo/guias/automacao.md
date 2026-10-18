# Automação e scripts

A CLI foi feita para rodar também sem ninguém olhando: num script, num cron, num CI. Três
coisas a tornam previsível: a saída em JSON ou CSV, os [códigos de saída](../referencia/codigos-de-saida.md)
estáveis e a separação entre `stdout` e `stderr`.

## Saída para programas

```console
$ inter-pj saldo --json | jq .disponivel
2850.55
```

Só o resultado vai para `stdout`. As mensagens de erro, os avisos e o `-v` vão para `stderr`,
então redirecionar a saída para um arquivo não mistura uma coisa com a outra. Os formatos
estão em [formatos de saída](formatos-de-saida.md).

Um exemplo: o extrato completo do mês, em CSV, para o sistema contábil.

```bash
#!/usr/bin/env bash
set -euo pipefail

inter-pj extrato completo --inicio 2026-08-01 --fim 2026-08-31 --todas-paginas --formato csv > agosto.csv
```

## Reagir ao código de saída

Cada categoria de erro tem o seu código, e o script pode decidir pelo número:

```bash
codigo=0
inter-pj saldo --json > saldo.json || codigo=$?

case "$codigo" in
  0) ;;
  3|4) echo "credenciais ou configuração: precisa de uma pessoa" >&2; exit "$codigo" ;;
  6) echo "o banco está indisponível: tente de novo mais tarde" >&2; exit "$codigo" ;;
  *) echo "falhou com o código $codigo" >&2; exit "$codigo" ;;
esac
```

Os códigos estão na [tabela de códigos de saída](../referencia/codigos-de-saida.md).

## Num cron ou num CI

- Informe o segredo por **variável de ambiente**, vinda do cofre de segredos do seu CI:
  `INTER_CLIENT_SECRET`. A CLI nunca o aceita como opção.
- Aponte para o arquivo de configuração com `INTER_CONFIG` (ou `--config`) e escolha o perfil
  com `INTER_PERFIL`.
- O cache de tokens fica em `~/.cache/inter-pj` (no Windows, `%LOCALAPPDATA%\inter-pj`). Para
  mudar a pasta, use `INTER_CACHE_DIR` com um **caminho absoluto**; a CLI recusa um relativo.
  Num CI sem estado entre execuções, o cache simplesmente começa vazio.
- O certificado e a chave precisam existir como arquivos na máquina que roda o comando. Grave-os
  a partir do cofre de segredos, com permissão `600`, e apague-os no fim.

## Tokens e o limite de chamadas

O endpoint de token do Inter aceita apenas **5 chamadas por minuto**, e cada token vale
**60 minutos**. Para não gastar esse limite, a CLI:

- pede tokens só com os **escopos de que o comando precisa**;
- reaproveita o token entre execuções, num cache local (um arquivo com permissão `600`).

Por isso rodar o mesmo comando em sequência, ou vários comandos que pedem os mesmos escopos,
usa um token só. Com `--sem-cache`, a CLI não lê nem grava o cache, e cada execução pede um
token novo, o que num laço rápido esgota o limite.

```console
$ inter-pj auth token --escopo extrato.read
$ inter-pj auth token --escopo extrato.read --renovar
$ inter-pj auth limpar
```

O primeiro mostra os escopos e a validade do token (e o reaproveita do cache, se puder); o
segundo ignora o cache e pede um novo; o terceiro apaga os tokens em cache do perfil.

### Chamar a API diretamente

Para chamar a API com outra ferramenta, como o `curl`, o `auth token --exibir` imprime apenas
o token. **Trate-o como uma senha.**

```console
$ curl --cert certificado.crt --key chave.key \
    -H "Authorization: Bearer $(inter-pj auth token --escopo extrato.read --exibir)" \
    https://cdpj-sandbox.partners.uatinter.co/banking/v2/saldo
```

## Retentativas

As consultas que falham por um problema passageiro são repetidas sozinhas: o limite de
requisições (`429`), a instabilidade do servidor (`500`, `502`, `503`, `504`) e a falha de
conexão. A espera cresce (1 s, 2 s, ...) e respeita o cabeçalho `Retry-After` do banco.

```console
$ inter-pj --tentativas 5 saldo
$ inter-pj --sem-retentativa saldo
```

O padrão é de **3 tentativas**. `--tentativas N` (ou `INTER_TENTATIVAS`) muda o número, e
`--sem-retentativa` desliga. Com `-v`, cada nova tentativa aparece em `stderr`.

!!! note "Só quando repetir é seguro"
    Uma requisição só é repetida quando repetir não pode causar um efeito duplicado. As
    operações que movimentam dinheiro, nas próximas versões, **nunca** serão repetidas
    sozinhas. A [arquitetura](../por-dentro/arquitetura.md) explica as regras.
