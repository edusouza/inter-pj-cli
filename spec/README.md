# Especificação OpenAPI

`inter-empresas-openapi.json` é a especificação **unificada** (OpenAPI 3.0.3) das APIs do Inter Empresas, gerada a partir das páginas de referência do [Portal do Desenvolvedor Inter Empresas](https://developers.inter.co/references):

| Produto | Prefixo | Operações |
| --- | --- | --- |
| Autenticação OAuth | `/oauth/v2` | 1 |
| Cobrança (Boleto com Pix) | `/cobranca/v3` | 14 |
| Banking | `/banking/v2` | 18 |
| Pix | `/pix/v2` | 33 |
| Pix Automático | `/pix/v2` | 28 |
| Fórum (fora do escopo, ver #60) | `/forum/v1` | 6 |

Cada operação declara em `security` os escopos OAuth exigidos.

## Para que serve aqui

A especificação é a **fonte de verdade dos testes de contrato** (`crates/inter-pj/tests/contract.rs`):

- todo endpoint implementado (`inter_pj::endpoint::ALL`) precisa existir na especificação com o mesmo método, caminho e escopos;
- o enum `Scope` precisa conter exatamente os escopos declarados (exceto os do Fórum);
- os modelos Rust precisam aceitar os exemplos derivados dos schemas.

## Atualização

1. Substitua o arquivo pela versão nova, sem reformatar.
2. Rode `cargo test -p inter-pj --test contract` e revise o `git diff` da especificação.
3. Registre no `CHANGELOG.md` qualquer mudança de contrato relevante.

O arquivo contém apenas a documentação pública do portal, com exemplos fictícios. Nunca adicione aqui respostas reais da sua conta.
