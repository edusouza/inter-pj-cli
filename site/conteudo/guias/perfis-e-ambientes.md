# Perfis e ambientes

## Ambientes

O Inter tem dois ambientes:

- **`sandbox`**: dados fictícios, para testar sem risco;
- **`producao`**: a conta real.

Não há ambiente padrão. O perfil diz qual usar (`ambiente = "sandbox"`), e a CLI não adivinha:
isso evita rodar um comando na conta real achando que era o teste.

## Perfis

Um perfil reúne o que identifica uma integração: o ambiente, o `client_id`, o certificado, a
chave e, se preciso, a conta corrente e os escopos. Vários perfis convivem no mesmo arquivo:

```toml
perfil_padrao = "sandbox"

[perfis.sandbox]
ambiente = "sandbox"
client_id = "<client_id do sandbox>"
certificado = '~/inter/sandbox.crt'
chave_privada = '~/inter/sandbox.key'

[perfis.producao]
ambiente = "producao"
client_id = "<client_id de produção>"
certificado = '~/inter/producao.crt'
chave_privada = '~/inter/producao.key'
```

Sem opção, vale o `perfil_padrao`. Para escolher outro:

```console
$ inter-pj --perfil producao saldo
```

ou, para uma sessão inteira, pela variável `INTER_PERFIL`.

## Quem ganha quando há conflito

Cada valor é resolvido na ordem **opção > variável de ambiente > arquivo**. Assim, o arquivo
guarda o que costuma valer, e uma variável ou uma opção muda um valor só para aquela execução:

```console
$ inter-pj --ambiente producao --perfil padrao saldo
```

Para saber de onde veio cada valor, o `config mostrar` imprime a configuração efetiva, com a
origem de cada um e os segredos ocultos:

```console
$ inter-pj --perfil producao config mostrar
```

A tabela com todas as opções e variáveis está na
[referência de configuração](../referencia/configuracao.md).

## Cada perfil, o seu cache

O cache de tokens é separado por perfil. Para limpar o de um perfil, ou o de todos:

```console
$ inter-pj auth limpar
$ inter-pj auth limpar --todos
```
