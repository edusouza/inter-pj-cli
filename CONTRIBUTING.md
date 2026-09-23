# Como contribuir

Obrigado pelo interesse! Este guia descreve como o projeto é desenvolvido.

## Regra número 1: nenhum dado pessoal ou credencial no repositório

Nunca versione — nem como exemplo, nem em testes, nem em issues/PRs:

- `client_id`, `client_secret`, tokens de acesso;
- certificados (`.crt`, `.pem`), chaves privadas (`.key`), `.pfx`/`.p12`;
- número de conta corrente, CPF/CNPJ, nomes ou respostas reais da API.

Testes usam apenas dados sintéticos: certificados gerados em tempo de execução (`rcgen`), credenciais fictícias e valores de exemplo da documentação pública. O `.gitignore` bloqueia os formatos de credencial e o CI roda o `gitleaks` sobre todo o histórico. Se algo vazar, siga o procedimento em [`SECURITY.md`](SECURITY.md) — é tratado como incidente grave e removido também do histórico.

## Fluxo de trabalho

1. **Issue primeiro**: todo trabalho parte de uma issue do [backlog](https://github.com/edusouza/inter-pj-cli/issues). Cada versão tem um épico (`[Épico] vX.Y.Z`) com sub-issues; o plano está em [`docs/roadmap.md`](docs/roadmap.md).
2. **Branch** a partir da `main`.
3. **Commits** no padrão [Conventional Commits](https://www.conventionalcommits.org/pt-br/) (`feat:`, `fix:`, `test:`, `docs:`, `chore:`, `ci:`), em inglês, referenciando a issue (`Refs #6`).
4. **Pull request** com descrição do que muda, como foi testado e `Closes #N` para as issues concluídas. O CI precisa estar verde.
5. **Release**: cada versão do roadmap fecha com a atualização do `CHANGELOG.md`, da versão no `Cargo.toml` e uma tag `vX.Y.Z`, que dispara o workflow de release.

## Verificações locais

Antes de abrir um PR, rode o mesmo que o CI:

```console
$ cargo fmt --all --check
$ cargo clippy --workspace --all-targets --locked -- -D warnings
$ cargo test --workspace --locked
$ RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
$ cargo deny check                                  # https://github.com/EmbarkStudios/cargo-deny
$ gitleaks git . --config .gitleaks.toml --redact   # https://github.com/gitleaks/gitleaks
```

A versão mínima suportada do Rust (MSRV) é a declarada em `rust-version` no `Cargo.toml` (hoje, 1.88).

## Testes

A garantia do código são os testes automatizados. Toda funcionalidade nova precisa de:

| Camada | Onde | O que cobre |
| --- | --- | --- |
| Unitários | `#[cfg(test)]` junto ao código | modelos, validações, formatação, configuração |
| Integração da biblioteca | `crates/inter-pj/tests/client.rs` | requisições e respostas contra um servidor mock (`wiremock`) |
| mTLS real | `crates/inter-pj/tests/mtls.rs` | handshake com servidor TLS local que exige certificado de cliente |
| Contrato | `crates/inter-pj/tests/contract.rs` | endpoints, escopos e modelos contra `spec/inter-empresas-openapi.json` |
| E2E do binário | `crates/inter-pj-cli/tests/cli.rs` | o executável `inter-pj` com ambiente isolado |

Ao implementar um endpoint novo:

1. declare-o em `crates/inter-pj/src/endpoint.rs` (método, caminho e escopos) e adicione-o a `ALL` — o teste de contrato confere com a especificação;
2. modele a resposta seguindo os nomes da API (`#[serde(rename_all = "camelCase")]`), com valores monetários em `rust_decimal::Decimal`;
3. escreva testes com o servidor mock para sucesso e para os erros relevantes;
4. adicione o comando na CLI com testes E2E para texto, JSON e códigos de saída.

### Testar manualmente no sandbox

Com uma integração de **sandbox** sua, configure um perfil local (fora do repositório) e rode a CLI normalmente. Nunca cole saídas reais em issues ou PRs sem antes remover dados identificáveis.

## Estilo

- Código, comentários e rustdoc em inglês; textos para o usuário (mensagens, ajuda, documentação) em português.
- `unsafe` é proibido; `clippy::pedantic` é habilitado para o workspace.
- Erros da biblioteca são tipados (`thiserror`); a CLI mapeia cada categoria para um código de saída documentado.
