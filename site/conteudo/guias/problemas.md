# Quando algo dá errado

A CLI escreve os erros em `stderr`, em português, com a explicação que o banco deu e, quando
há o que sugerir, uma linha que começa com `dica:`. O primeiro passo é ler a mensagem inteira.
O segundo é o [código de saída](../referencia/codigos-de-saida.md), que diz a categoria do
problema.

## Ver o que a CLI está fazendo

Com `-v`, e com `-vv` para mais detalhes, a CLI mostra em `stderr` cada requisição: o método, o
caminho, o status e o tempo. Ela **nunca** mostra tokens, segredos nem o corpo das respostas,
então a saída serve para pedir ajuda.

```console
$ inter-pj -v saldo
```

## O arquivo de configuração não é lido

O erro mais comum, no Windows, é um caminho entre aspas duplas. No TOML, a barra invertida
começa um escape, e `"C:\Users\..."` não é um arquivo válido:

```console
PS> inter-pj config mostrar
erro: arquivo de configuração inválido (C:\Users\voce\AppData\Roaming\inter-pj\config.toml, linha 7): o valor de certificado está entre aspas duplas e tem uma barra invertida, que em TOML começa um escape; nos caminhos do Windows, use aspas simples (certificado = 'C:\pasta\arquivo')
dica: `inter-pj config verificar` mostra a correção de cada linha; com `--corrigir`, ele a aplica e guarda uma cópia do arquivo original
```

O `config verificar` aponta cada problema com a linha e a correção:

```console
PS> inter-pj config verificar
Arquivo: C:\Users\voce\AppData\Roaming\inter-pj\config.toml

erro      linha 7, certificado: caminho do Windows entre aspas duplas: em TOML, a barra invertida começa um escape (\U pede 8 dígitos hexadecimais), e o arquivo não é lido
          corrija para: certificado = 'C:\Users\voce\inter\certificado.crt'
erro      linha 8, chave_privada: caminho do Windows entre aspas duplas: em TOML, a barra invertida começa um escape (\U pede 8 dígitos hexadecimais), e o arquivo não é lido
          corrija para: chave_privada = 'C:\Users\voce\inter\chave.key'

2 erros e 0 avisos.
Para trocar as aspas: inter-pj config verificar --corrigir
erro: a configuração tem 2 erros
```

Com `--corrigir`, ele troca as aspas duplas dos caminhos do Windows por aspas simples e guarda o
original em `config.toml.bak`:

```console
PS> inter-pj config verificar --corrigir
```

O comando sai com o código 3 se encontrar algum erro, e `--json` dá o resultado para scripts. O
`client_secret` nunca aparece, e a conta corrente sai mascarada, como no `config mostrar`.

!!! note "Os exemplos acima são ilustrativos"
    Os caminhos e os números de linha são de um arquivo de exemplo. As mensagens da sua
    máquina terão os seus caminhos.

## Credenciais, escopos e acesso negado (código 4)

- **`client_id` ou `client_secret` errados:** rode `inter-pj auth token --escopo extrato.read`.
  Ele valida as credenciais sem fazer mais nada.
- **Escopo que a integração não tem:** o banco recusa o token inteiro. Habilite o escopo na
  integração, no Internet Banking PJ, ou tire-o da lista `escopos` do perfil.
- **Ambiente errado:** confira com `inter-pj config mostrar` se o `ambiente` do perfil é o da
  integração que você criou.
- **Segredo ausente:** defina `INTER_CLIENT_SECRET`. O `config verificar` mostra se o
  `client_secret` está definido e de onde ele vem, sem revelá-lo.

## O certificado ou a chave

O `config verificar` confere se o certificado e a chave são lidos e aceitos pela biblioteca TLS.
A CLI recusa uma chave privada protegida por senha, com uma mensagem explicando. Se a CLI
avisar que o arquivo de configuração (com o `client_secret`) ou a chave podem ser lidos por
outros usuários, o aviso traz o comando que resolve: `chmod 600` no arquivo.

## O limite de requisições (código 6)

O endpoint de token do Inter aceita **5 chamadas por minuto**. Se um script esgotou o limite,
a CLI espera e tenta de novo, respeitando o `Retry-After`; se o banco pedir uma espera maior
do que a CLI aceita aguardar, ela desiste na hora, com a dica de esperar. Veja
[tokens e o limite de chamadas](automacao.md#tokens-e-o-limite-de-chamadas).

## Uma consulta rejeitada pela API (código 5)

A API recusou os parâmetros (`400`, `404`, `409` ou `422`). A mensagem traz a explicação do
banco.

No extrato, um período de mais de 90 dias a CLI confere **antes** de chamar a API.
`--dividir-periodo` consulta períodos maiores em partes, como mostra o
[guia do extrato](saldo-e-extrato.md#o-extrato-de-um-periodo).

## Pedindo ajuda

Abra uma [issue](https://github.com/edusouza/inter-pj-cli/issues) com a versão
(`inter-pj --version`), o sistema, o comando e a saída com `-v`. **Antes de colar qualquer
coisa, remova os dados identificáveis**: números de conta, CPF e CNPJ, nomes e respostas reais
da API. Nunca cole credenciais.

Uma falha de segurança **não** vai numa issue pública: veja
[segurança e privacidade](../referencia/seguranca.md#reportando-vulnerabilidades).
