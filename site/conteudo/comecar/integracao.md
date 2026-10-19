# Criar a integração no Inter

O `inter-pj` fala com as APIs do Inter Empresas do mesmo modo que qualquer integração: com um
certificado de cliente (mTLS) e as credenciais OAuth2 de uma **integração** que você cria no
Internet Banking PJ. Este passo é feito no site do banco, não no `inter-pj`.

!!! tip "Comece pelo sandbox"
    O ambiente **sandbox** tem dados fictícios e serve para conhecer a CLI sem risco. Crie e
    configure primeiro uma integração de sandbox, e só depois a de produção.

## O que criar

No Internet Banking do Inter Empresas, crie uma integração com os **escopos** que você vai
usar. Para `saldo` e `extrato`, o escopo é `extrato.read`.

Ao criar a integração, baixe e guarde:

| O que | Para quê |
| --- | --- |
| **Certificado** (`.crt`) | a identidade TLS do seu lado da conexão |
| **Chave privada** (`.key`) | acompanha o certificado; a CLI recusa chaves protegidas por senha |
| **client_id** | identifica a integração |
| **client_secret** | a senha da integração; **só é mostrado uma vez** |

## Escopos

Cada comando pede ao banco um token só com os escopos de que precisa. Se a integração não
tiver um deles habilitado, o banco recusa o token inteiro e o comando falha com a explicação
do erro. Por isso vale habilitar, na integração, tudo o que você pretende usar. As versões seguintes
do projeto passam a exigir outros escopos, como os de Pix e de cobrança.

## Onde guardar

Guarde o certificado, a chave e o `client_secret` **fora de qualquer repositório**, com
permissão restrita (`chmod 600` no Linux e no macOS). O que a CLI faz com cada um desses
dados está em [segurança e privacidade](../referencia/seguranca.md).

## Próximo passo

Com os arquivos em mãos, [configure o `inter-pj`](configurar.md).
