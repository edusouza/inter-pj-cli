//! Generates the pages of the documentation site that come from the repository, and checks the
//! commands of the hand-written ones. The site itself is `site/mkdocs.yml`, built with Zensical
//! and published to GitHub Pages by `.github/workflows/pages.yml`.
//!
//! From the root of the repository, after `cargo build -p inter-pj-cli`:
//!
//! ```text
//! cargo run -p sitegen
//! ```
//!
//! What it writes into `site/conteudo/` (none of it is committed, see `site/.gitignore`):
//!
//! - `referencia/comandos.md`, from the `--help` of the built `inter-pj`;
//! - a copy of the changelog, the security policy, the contribution guide, the architecture,
//!   the roadmap and the README of the library, with the links rewritten ([`paginas`]).
//!
//! Then it checks that every `inter-pj` command written in the other pages exists
//! ([`verificar`]); if one does not, it fails.

mod ajuda;
mod paginas;
mod verificar;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USO: &str = "\
Gera as páginas derivadas do site de documentação em site/conteudo/ e confere os comandos
citados nas demais.

Uso: cargo run -p sitegen -- [OPÇÕES]

Opções:
      --raiz <PASTA>       Raiz do repositório [padrão: a pasta atual]
      --binario <ARQUIVO>  O inter-pj compilado, de onde vem a referência de comandos
                           [padrão: target/debug/inter-pj]
  -h, --help               Mostra esta ajuda
";

struct Argumentos {
    raiz: PathBuf,
    binario: PathBuf,
}

fn ler_argumentos() -> Result<Option<Argumentos>, String> {
    let mut raiz = PathBuf::from(".");
    let mut binario = None;
    let mut argumentos = std::env::args().skip(1);
    while let Some(argumento) = argumentos.next() {
        match argumento.as_str() {
            "-h" | "--help" => return Ok(None),
            "--raiz" => raiz = argumentos.next().ok_or("falta o valor de --raiz")?.into(),
            "--binario" => {
                binario = Some(
                    argumentos
                        .next()
                        .ok_or("falta o valor de --binario")?
                        .into(),
                );
            }
            outro => return Err(format!("argumento desconhecido: {outro}; veja --help")),
        }
    }
    let binario = binario.unwrap_or_else(|| {
        let nome = if cfg!(windows) {
            "inter-pj.exe"
        } else {
            "inter-pj"
        };
        raiz.join("target").join("debug").join(nome)
    });
    Ok(Some(Argumentos { raiz, binario }))
}

fn escrever(caminho: &Path, texto: &str) -> Result<(), String> {
    if let Some(pasta) = caminho.parent() {
        fs::create_dir_all(pasta)
            .map_err(|erro| format!("não consegui criar {}: {erro}", pasta.display()))?;
    }
    fs::write(caminho, texto)
        .map_err(|erro| format!("não consegui gravar {}: {erro}", caminho.display()))
}

fn executar() -> Result<(), String> {
    let Some(argumentos) = ler_argumentos()? else {
        print!("{USO}");
        return Ok(());
    };
    let raiz = argumentos.raiz;
    if !raiz.join("site/mkdocs.yml").is_file() {
        return Err(format!(
            "{} não parece a raiz do repositório (falta site/mkdocs.yml); rode a partir da raiz ou use --raiz",
            raiz.display()
        ));
    }
    let conteudo = raiz.join("site/conteudo");

    let arvore = ajuda::carregar(&argumentos.binario)?;
    escrever(
        &conteudo.join("referencia/comandos.md"),
        &ajuda::pagina(&arvore),
    )?;
    for derivada in paginas::DERIVADAS {
        escrever(
            &conteudo.join(derivada.destino),
            &paginas::gerar(&raiz, derivada)?,
        )?;
    }
    println!(
        "{} páginas geradas em {} (inter-pj {}).",
        paginas::DERIVADAS.len() + 1,
        conteudo.display(),
        arvore.versao
    );

    let problemas = verificar::paginas(&conteudo, &arvore.raiz)?;
    for problema in &problemas {
        eprintln!(
            "site/conteudo/{}:{}: {}",
            problema.pagina, problema.linha, problema.mensagem
        );
    }
    if problemas.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} comando(s) citado(s) no site que o inter-pj não aceita; corrija a página ou o CLI",
            problemas.len()
        ))
    }
}

fn main() -> ExitCode {
    match executar() {
        Ok(()) => ExitCode::SUCCESS,
        Err(erro) => {
            eprintln!("erro: {erro}");
            ExitCode::FAILURE
        }
    }
}
