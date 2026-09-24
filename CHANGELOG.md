# Changelog

Todas as mudanças relevantes deste projeto são documentadas aqui.

O formato segue o [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/) e o projeto adota o [Versionamento Semântico](https://semver.org/lang/pt-BR/).

## [Não lançado]

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
