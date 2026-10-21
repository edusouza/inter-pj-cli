# Glossário

**Ambiente**
:   `sandbox` (dados fictícios, para testes) ou `producao` (a conta real). O perfil escolhe
    um; não há padrão.

**Cache de tokens**
:   Arquivo local, com permissão `600`, em que a CLI guarda os tokens para reaproveitá-los entre
    execuções. Existe por causa do limite de 5 chamadas por minuto do endpoint de token.

**`client_id` e `client_secret`**
:   As credenciais OAuth2 de uma integração. O segredo só é mostrado uma vez, na criação, e a CLI
    nunca o aceita como opção de linha de comando.

**Conta corrente**
:   A conta a que um comando se refere. Só precisa ser informada quando a integração tem mais de
    uma; é enviada ao banco no cabeçalho `x-conta-corrente`.

**Escopo**
:   A permissão que um token carrega, como `extrato.read`. Cada comando pede só os escopos de
    que precisa, e todos precisam estar habilitados na integração.

**Integração**
:   O cadastro, feito no Internet Banking PJ, que dá a uma aplicação o acesso às APIs: gera o
    certificado, a chave privada, o `client_id` e o `client_secret`, e define os escopos.

**mTLS**
:   *Mutual TLS*: além de a CLI conferir o certificado do servidor, o servidor confere o
    certificado da CLI. O Inter exige mTLS em todas as chamadas.

**OAuth2 client credentials**
:   O modo em que a aplicação se autentica com o `client_id` e o `client_secret`, sem um usuário
    no meio, e recebe um token com validade de 60 minutos.

**Perfil**
:   Um conjunto de configurações de uma integração no arquivo de configuração, como `sandbox` e
    `producao`. Escolhe-se com `--perfil` ou `INTER_PERFIL`.

**`Retry-After`**
:   O cabeçalho com que o banco diz quanto esperar antes de tentar de novo. A CLI o respeita.

**Scroll**
:   O modo da API do extrato para percorrer mais de 10.000 transações: em vez de páginas, um
    cursor que avança lote a lote. Há um por conta, e ele expira depois de 6 minutos sem uso.

**Token de acesso**
:   A credencial de curta duração que a API aceita nas chamadas. Trate-o como uma senha: só é
    exibido com `auth token --exibir`.
