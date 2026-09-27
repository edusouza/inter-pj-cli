# Changelog

Todas as mudanças relevantes deste projeto são documentadas aqui.

O formato segue o [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/) e o projeto adota o [Versionamento Semântico](https://semver.org/lang/pt-BR/).

## [Não lançado]

## [0.1.1] - 2026-09-25

Diagnóstico do arquivo de configuração.

### Adicionado

- `inter-pj config verificar`: confere o arquivo de configuração e o perfil. Mostra cada problema com a linha e a correção: caminhos do Windows entre aspas duplas, sintaxe do TOML, o que falta no perfil, arquivos que não existem, certificado e chave aceitos pela biblioteca TLS, conta corrente e permissões. Sai com código 3 quando encontra um erro, e `--json` dá o resultado para scripts (#164).
- `inter-pj config verificar --corrigir`: troca as aspas duplas dos caminhos do Windows por aspas simples, mantendo recuo, comentários e fim de linha, e guarda o original em `config.toml.bak` (permissão 600, sem sobrescrever uma cópia anterior) (#164).

### Corrigido

- Um caminho do Windows entre aspas duplas (`certificado = "C:\Users\..."`) deixava de ser um erro em inglês sobre dígitos unicode. Agora a mensagem, em português, explica o escape do TOML, indica as aspas simples e sugere o `config verificar`. O conteúdo da linha continua sem aparecer, porque pode ter o `client_secret` (#164).
- Os comandos avisam quando um caminho lido do arquivo tem um caractere de controle. Isso acontece quando `\n` ou `\t` entre aspas duplas viram uma quebra de linha ou uma tabulação (#164).
- A linha informada num erro do arquivo de configuração saía com um a menos quando o erro apontava o início de uma linha, como numa chave desconhecida (#164).
- O modelo do `config init` e o README usam aspas simples nos caminhos e explicam o motivo (#164).

## [0.1.0] - 2026-09-23

Primeira versão: autenticação e saldo.

### Adicionado

- `inter-pj saldo [--data AAAA-MM-DD]`: saldo disponível, bloqueios e limite, em texto (R$ no formato brasileiro) ou JSON com valores exatos (#8).
- `inter-pj auth token` (escopos, `--renovar`, `--exibir`) e `inter-pj auth limpar [--todos]` (#6, #7).
- `inter-pj config init|caminho|mostrar`: modelo de configuração com permissão 600, locais padrão e configuração efetiva com a origem de cada valor e segredos ocultos (#5).
- Perfis em arquivo TOML com precedência flag > variável de ambiente > arquivo; `client_secret` apenas por `INTER_CLIENT_SECRET` ou arquivo (#5, #4).
- Autenticação OAuth2 *client credentials* com mTLS via rustls, escopos mínimos por comando, renovação automática após `401` e verificação dos escopos concedidos (#6).
- Cache local de tokens por integração (arquivo 600, gravação atômica), respeitando o limite de 5 chamadas/min do endpoint de token (#7).
- Mensagens de erro em português a partir do formato *problem details* da API, com dicas, e códigos de saída documentados (#9).
- Biblioteca `inter-pj` reutilizável, com registro de endpoints verificado contra a especificação OpenAPI (#2, #10).
- CI com formatação, clippy, testes em Linux/macOS/Windows, MSRV 1.88, documentação, cargo-deny e gitleaks; workflow de release com binários e checksums (#3, #11).
