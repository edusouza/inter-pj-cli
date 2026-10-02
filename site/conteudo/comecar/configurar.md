# Configurar

A configuração é um arquivo TOML com um ou mais **perfis**. Cada perfil aponta para as
credenciais de uma integração. Este guia leva do zero ao primeiro `saldo`; as chaves, as
opções e as variáveis de ambiente estão na [referência de configuração](../referencia/configuracao.md).

## 1. Crie o arquivo

```console
$ inter-pj config init
Arquivo de configuração criado em /home/voce/.config/inter-pj/config.toml
```

O arquivo nasce com um modelo comentado e permissão `600`. Para ver onde ficam o arquivo e o
cache no seu sistema, use `inter-pj config caminho`.

## 2. Edite o perfil

```toml
perfil_padrao = "padrao"

[perfis.padrao]
ambiente = "sandbox"              # ou "producao"
client_id = "<seu client_id>"
certificado = '~/inter/certificado.crt'
chave_privada = '~/inter/chave.key'
# conta_corrente = "<numero>"     # só se a integração tiver mais de uma conta
# escopos = ["extrato.read"]      # opcional: escopos pedidos em todo token
```

!!! warning "Caminhos entre aspas simples"
    Escreva os caminhos entre **aspas simples**, principalmente no Windows
    (`certificado = 'C:\inter\certificado.crt'`). Entre aspas duplas, a barra invertida começa
    um escape do TOML: `"C:\Users\..."` impede a leitura do arquivo, e `"C:\novo\teste.crt"` é
    lido, mas com uma quebra de linha e uma tabulação no lugar de `\n` e `\t`.

Caminhos relativos são relativos ao arquivo de configuração, e `~/` é a sua pasta pessoal.

## 3. Informe o segredo pela variável de ambiente

O `client_secret` **nunca** é aceito como opção de linha de comando, para que não fique no
histórico do shell.

=== "Linux e macOS"

    ```bash
    export INTER_CLIENT_SECRET='<seu client_secret>'
    ```

=== "Windows (PowerShell)"

    ```powershell
    $env:INTER_CLIENT_SECRET = '<seu client_secret>'
    ```

Ele pode, alternativamente, ficar no arquivo de configuração. Nesse caso a CLI avisa se o
arquivo puder ser lido por outros usuários.

## 4. Confira

```console
$ inter-pj config verificar
$ inter-pj config mostrar
$ inter-pj saldo
```

- `config verificar` confere o arquivo, o perfil, o certificado e a chave, e aponta cada
  problema com a linha e a correção.
- `config mostrar` mostra a configuração efetiva, com os segredos ocultos e a origem de cada
  valor (opção, variável de ambiente ou arquivo).

Se o arquivo não for lido, veja [quando algo dá errado](../guias/problemas.md#o-arquivo-de-configuracao-nao-e-lido).

## Próximo passo

[Os primeiros comandos](primeiros-comandos.md).
