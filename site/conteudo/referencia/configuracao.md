# Configuração

Como o `inter-pj` descobre o que usar: o arquivo, as opções de linha de comando e as variáveis
de ambiente. Para o passo a passo, veja [configurar](../comecar/configurar.md).

## Onde ficam os arquivos

| O quê | Linux e macOS | Windows |
| --- | --- | --- |
| Arquivo de configuração | `~/.config/inter-pj/config.toml` | `%APPDATA%\inter-pj\config.toml` |
| Cache de tokens | `~/.cache/inter-pj` | `%LOCALAPPDATA%\inter-pj` |

`inter-pj config caminho` mostra os caminhos em uso. Para outro arquivo, use `--config` ou
`INTER_CONFIG`; para outra pasta de cache, `INTER_CACHE_DIR`, que precisa ser um **caminho
absoluto**.

## O arquivo

```toml
perfil_padrao = "padrao"

[perfis.padrao]
ambiente = "sandbox"
client_id = "<seu client_id>"
certificado = '~/inter/certificado.crt'
chave_privada = '~/inter/chave.key'
```

| Chave | Onde | O que é |
| --- | --- | --- |
| `perfil_padrao` | raiz | o perfil usado quando nenhum é escolhido |
| `ambiente` | perfil | `sandbox` (dados fictícios) ou `producao` (a conta real) |
| `client_id` | perfil | o `client_id` da integração |
| `client_secret` | perfil | o segredo da integração; prefira a variável `INTER_CLIENT_SECRET` |
| `certificado` | perfil | o certificado (`.crt`) baixado com a integração |
| `chave_privada` | perfil | a chave privada (`.key`); não pode ter senha |
| `conta_corrente` | perfil | só dígitos; só é preciso se a integração tiver mais de uma conta |
| `escopos` | perfil | escopos pedidos em todo token, além dos de cada comando; todos precisam estar habilitados na integração |

!!! warning "Uma chave desconhecida invalida o arquivo"
    O arquivo é lido de forma estrita: uma chave com o nome errado, na raiz ou num perfil, é
    recusada em vez de ignorada. O erro indica a linha, sem reproduzir o conteúdo, que pode
    ter o segredo.

Os caminhos podem ser absolutos, relativos ao arquivo de configuração ou começar por `~/`.
Escreva-os entre **aspas simples**: no Windows, a barra invertida entre aspas duplas começa um
escape do TOML.

## Opções e variáveis de ambiente

Cada valor é resolvido na ordem **opção de linha de comando > variável de ambiente > arquivo**.
As opções valem para todos os comandos e podem vir antes ou depois deles.

| Configuração | Opção | Variável de ambiente |
| --- | --- | --- |
| Perfil | `-p`, `--perfil` | `INTER_PERFIL` |
| Arquivo de configuração | `--config` | `INTER_CONFIG` |
| Ambiente (`sandbox` ou `producao`) | `--ambiente` | `INTER_AMBIENTE` |
| `client_id` | `--client-id` | `INTER_CLIENT_ID` |
| `client_secret` | — | `INTER_CLIENT_SECRET` |
| Certificado (`.crt`) | `--certificado` | `INTER_CERTIFICADO` |
| Chave privada (`.key`) | `--chave-privada` | `INTER_CHAVE_PRIVADA` |
| Conta corrente | `--conta-corrente` | `INTER_CONTA_CORRENTE` |
| Tentativas por requisição | `--tentativas` | `INTER_TENTATIVAS` |
| Diretório de cache | — | `INTER_CACHE_DIR` |

O `client_secret` **não tem opção de linha de comando**, de propósito: ela ficaria no
histórico do shell e na lista de processos.

Outras opções que não têm variável: `--formato`, `--json` e `--separador` (veja os
[formatos de saída](../guias/formatos-de-saida.md)), `--sem-retentativa`, `--sem-cache` e
`-v`/`-vv`. A lista completa, com as descrições, está na
[referência de comandos](comandos.md#inter-pj).

## Não há ambiente padrão

Se o ambiente não vier da opção, da variável nem do perfil, o comando falha e lista tudo o que
falta, em vez de escolher um. Para ver o que está valendo, e de onde veio cada valor:

```console
$ inter-pj config mostrar
```
