# Especificação OpenAPI

`inter-empresas-openapi.json` é a especificação **unificada** (OpenAPI 3.0.3) das APIs do Inter Empresas, gerada a partir das páginas de referência do [Portal do Desenvolvedor Inter Empresas](https://developers.inter.co/references):

| Produto | Prefixo | Operações |
| --- | --- | --- |
| Autenticação OAuth | `/oauth/v2` | 1 |
| Cobrança (Boleto com Pix) | `/cobranca/v3` | 14 |
| Banking | `/banking/v2` | 18 |
| Pix | `/pix/v2` | 32 |
| Pix Automático | `/pix/v2` | 27 |
| Fórum (fora do escopo, ver #60) | `/forum/v1` | 6 |

Cada operação declara em `security` os escopos OAuth exigidos.

## Para que serve aqui

A especificação é a **fonte de verdade dos testes de contrato** (`crates/inter-pj/tests/contract.rs`):

- todo endpoint implementado (`inter_pj::endpoint::ALL`) precisa existir na especificação com o mesmo método, caminho e escopos;
- e toda operação da especificação, exceto as do Fórum, precisa de um endpoint no registro: o teste lista as que faltarem. A especificação repete uma operação (o pagamento de QR Code no sandbox, também entre as cobranças com vencimento, com um espaço no caminho), declarada em `tests/spec/mod.rs`;
- o enum `Scope` precisa conter exatamente os escopos declarados (exceto os do Fórum);
- os modelos Rust precisam aceitar os exemplos derivados dos schemas.

## Dados pessoais nos exemplos

Alguns exemplos do portal traziam CPFs com dígitos verificadores válidos, telefone e números de conta com aparência real (por exemplo, no detalhe de transferências e nos pagadores de cobranças). Para não manter possíveis dados de terceiros no repositório, eles foram substituídos por valores sintéticos (`123.456.789-09`, `+5500000000000`, contas `1234…`) pelo script [`sanitizar.py`](sanitizar.py), que preserva o restante do arquivo byte a byte.

O teste `spec_examples_contain_no_real_looking_personal_data` falha se um dado desse tipo voltar à especificação.

## Atualização

1. Substitua o arquivo pela versão nova, sem reformatar.
2. Rode `python3 spec/sanitizar.py`.
3. Rode `cargo test -p inter-pj --test contract` e revise o `git diff` da especificação.
4. Registre no `CHANGELOG.md` qualquer mudança de contrato relevante.

Nunca adicione aqui respostas reais da sua conta.
