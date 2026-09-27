# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Unofficial Rust client and CLI for the Banco Inter "Inter Empresas" (PJ business account) APIs: OAuth2 client credentials over mutual TLS. Cargo workspace with two crates:

- `crates/inter-pj`: library `inter_pj` (HTTP client, OAuth2/mTLS, token management, API models).
- `crates/inter-pj-cli`: binary `inter-pj` (clap commands, TOML config/profiles, file token cache, output formatting).

## Commands

These are the same checks CI runs (`.github/workflows/ci.yml`), which fails on any warning:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
cargo deny check                                  # license allowlist, advisories, sources (deny.toml)
gitleaks git . --config .gitleaks.toml --redact   # secret scan over full history
```

Targeted tests:

```sh
cargo test -p inter-pj --lib                          # library unit tests
cargo test -p inter-pj --test client                  # library vs wiremock mock API
cargo test -p inter-pj --test mtls                    # real TLS handshake requiring a client cert
cargo test -p inter-pj --test contract                # registry/models vs OpenAPI spec
cargo test -p inter-pj-cli --test cli <name_filter>   # E2E of the built binary
cargo test -p inter-pj --lib auth::tests::reuses_superset   # single test by path/substring
```

Run the CLI: `cargo run -p inter-pj-cli -- saldo --json`. MSRV is 1.88 (`rust-version` in `Cargo.toml`, checked in CI). Edition 2024.

All tests run offline with no credentials. Certificates are generated at runtime with `rcgen`, and the API is mocked with `wiremock`.

## Architecture

**Layering rule:** the library knows nothing about terminals, config files, or user directories, and the CLI knows nothing about HTTP. Keep it that way. For example, the CLI's file cache (`token_store.rs`, `FileTokenStore`) implements the library's `TokenStore` trait and is injected via `InterClientBuilder::token_store`.

**Request flow (library):** an API group (`client.banking()`, `banking/mod.rs`) builds an `ApiRequest` from an `Endpoint` constant, then calls `InterClient::execute`, which:

1. asks `TokenManager` (`auth.rs`) for a token covering the endpoint's scopes. It reuses one from memory or the `TokenStore` if it is a superset with more than 60 s of validity. Otherwise it requests exactly those scopes plus the profile's optional extra scopes. It rejects the token if the server granted fewer scopes than needed. A mutex serializes renewals because the token endpoint allows only 5 calls/min.
2. sends the request with bearer auth and the optional `x-conta-corrente` header.
3. on `401`, invalidates the cached token and retries **once** with a fresh one.
4. maps errors through `problem.rs`, a tolerant RFC 7807 parser, into `Error::{Config, Identity, Auth, Api, Transport, Decode}` with an `ApiErrorKind`.

**Endpoint registry + contract tests:** `endpoint.rs` is the single source of every operation's method, path template, and required scopes. Each new endpoint must also be added to `endpoint::ALL`. `tests/contract.rs` checks everything in `ALL` against `spec/inter-empresas-openapi.json`, and also checks that the `Scope` enum (`scope.rs`) matches the spec's scopes exactly (excluding the out-of-scope Fórum API).

**CLI flow:** `main.rs` keeps the raw clap `ArgMatches` alongside the parsed `Cli`, because `commands::Context` uses `value_source` to record where each setting came from (flag, env, or file). Precedence is flag > env > file, and `config mostrar` prints each value's origin. `Context::settings()` resolves the profile (`config.rs`), `Context::client()` builds the `InterClient`, and each command in `commands/` renders text via `output.rs` (Brazilian `R$ 1.234,56` formatting) or JSON. There is no default environment: `sandbox` or `producao` must be chosen explicitly.

**Exit codes are a stable, documented contract** (README, `main.rs` doc comment): the `CliError::exit_code` mapping in `crates/inter-pj-cli/src/error.rs` translates library error kinds to codes 1–6. `CliError::hints` adds the Portuguese `dica:` lines.

## Adding an endpoint

This is the recipe in `CONTRIBUTING.md`:

1. Declare it in `endpoint.rs` and add it to `ALL`.
2. Model the response with API field names (`#[serde(rename_all = "camelCase")]`, `#[non_exhaustive]`, optional fields). Money is `rust_decimal::Decimal`. The API sends numbers or strings, and serialization goes back to JSON numbers via `serde_util::decimal_as_number`.
3. Add wiremock tests (success plus relevant errors) in `crates/inter-pj/tests/client.rs`, using helpers in `tests/common/mod.rs`, and a contract test that the model accepts the schema example.
4. Add the CLI command with E2E tests in `crates/inter-pj-cli/tests/cli.rs` covering text, JSON, and exit codes.

## Testing internals

- E2E tests (`tests/cli.rs`, `TestEnv`) run the real binary with every inherited `INTER_*` variable removed. They point it at the mock through `INTER_BASE_URL`, an override not documented for users. They isolate state with `INTER_CONFIG` and `INTER_CACHE_DIR`, which must be an **absolute** path because the CLI rejects relative ones. They also set `HOME`/`USERPROFILE`/`XDG_*`.
- E2E tests assert that the synthetic secret and token never appear in stdout/stderr, even with `-vv`. Preserve this when touching logging or error messages.
- Unix permission checks (mode 600/700 on cache files) are `#[cfg(unix)]`. CI runs tests on Linux, macOS, and Windows.

## Conventions

- **Language split:** code, comments, rustdoc, and commit messages in English. Everything the user reads (CLI help, errors, warnings, README/docs, CHANGELOG) is in Portuguese. Domain names follow the API's Portuguese field names (`saldo`, `disponivel`, `bloqueadoCheque`).
- Lints: `unsafe_code = "forbid"`, `clippy::pedantic` warn, `unreachable_pub` warn (use `pub(crate)` inside the binary), and `missing_docs` in the library.
- **Secrets:** hold `client_secret`, tokens, and keys in `secrecy` types, which have no revealing `Debug`/`Display`. The `client_secret` is never accepted as a CLI flag. Logging (`logging.rs`) only enables the `inter_pj` and `inter_pj_cli` targets, so HTTP/TLS dependencies cannot leak headers. Never log tokens, secrets, or response bodies.
- **Never commit real data:** no credentials, certificates, account numbers, CPF/CNPJ, or real API responses, not even in tests or examples. `.gitignore` blocks `*.crt`, `*.key`, `*.pem`, `config.toml`, and similar files. Use only synthetic values.
- **Dependencies:** the tree is kept deliberately small (`reqwest`/`tokio` with default features off, `idna_adapter` pinned to the ASCII-only back end). Any new crate must pass the `deny.toml` license allowlist.
- **Commits:** Conventional Commits (`feat:`, `fix:`, `test:`, `docs:`, `ci:`, `chore:`, `build:`) with an optional scope such as `feat(cli):`, referencing the issue (`Refs #N`). PRs use `Closes #N`. Work is tracked per version in GitHub issue epics, with the plan in `docs/roadmap.md`. Versions are delivered as stacked PR branches named `claude/vX.Y.Z-N-<topic>`, with the release PR last.

## Spec and release

- `spec/inter-empresas-openapi.json` is about 22k lines. Grep it rather than reading it whole. To update it, replace the file without reformatting, run `python3 spec/sanitizar.py` (which replaces real-looking personal data with synthetic values), then run the contract tests. The test `spec_examples_contain_no_real_looking_personal_data` guards this.
- Release: bump `version` in the root `Cargo.toml` (`[workspace.package]`), add a `## [X.Y.Z] - date` section to `CHANGELOG.md` (Keep a Changelog, in Portuguese), and push tag `vX.Y.Z`. `release.yml` fails if the tag, the Cargo version, and the CHANGELOG section disagree.
