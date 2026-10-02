# Instalação

O `inter-pj` é um binário único. Há duas formas de obtê-lo: baixar o pacote pronto da sua
plataforma ou compilar com o Cargo.

## Binários prontos

Baixe o pacote do seu sistema na página de
[releases](https://github.com/edusouza/inter-pj-cli/releases):

| Sistema | Pacote |
| --- | --- |
| Linux x86_64 | `x86_64-unknown-linux-gnu` (glibc) ou `x86_64-unknown-linux-musl` (estático) |
| macOS, Apple Silicon | `aarch64-apple-darwin` |
| macOS, Intel | `x86_64-apple-darwin` |
| Windows | `x86_64-pc-windows-msvc` |

O nome do pacote é `inter-pj-<versão>-<sistema>`, com extensão `.tar.gz` (Linux e macOS) ou
`.zip` (Windows). Dentro dele estão o executável `inter-pj`, o `README.md`, o `CHANGELOG.md` e
as licenças.

**Confira o `SHA256SUMS`** da release antes de usar um binário que lida com credenciais
bancárias:

=== "Linux"

    ```bash
    sha256sum --check --ignore-missing SHA256SUMS
    ```

=== "macOS"

    ```bash
    shasum -a 256 --check --ignore-missing SHA256SUMS
    ```

=== "Windows (PowerShell)"

    ```powershell
    Get-FileHash .\inter-pj-<versão>-x86_64-pc-windows-msvc.zip -Algorithm SHA256
    # compare o valor com a linha do pacote em SHA256SUMS
    ```

Depois, extraia o pacote e coloque o `inter-pj` numa pasta do seu `PATH`.

## Com o Cargo

Requer Rust 1.88 ou superior. O projeto não é publicado no crates.io, então a instalação é
feita a partir do repositório, numa versão fixada por tag:

```console
$ cargo install --locked --git https://github.com/edusouza/inter-pj-cli \
    --tag v0.2.0 inter-pj-cli
```

Para outra versão, troque a tag pela desejada; as versões estão no [changelog](../changelog.md).

Ou compilando a partir do código:

```console
$ git clone https://github.com/edusouza/inter-pj-cli.git
$ cd inter-pj-cli
$ cargo build --release --locked -p inter-pj-cli
```

O executável fica em `target/release/inter-pj`.

## Conferir a instalação

```console
$ inter-pj --version
inter-pj 0.2.0
```

Todos os comandos aceitam `--help`, e a lista completa está na
[referência de comandos](../referencia/comandos.md).

## Próximo passo

Antes de configurar, é preciso ter as credenciais:
[crie a integração no Internet Banking PJ](integracao.md).
