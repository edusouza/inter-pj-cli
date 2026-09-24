//! `inter-pj manual <diretório>`: a man page for every command, generated
//! from the command definition, as `--help` shows it.

use std::fs;

use chrono::Local;
use clap_mangen::Man;

use crate::cli::{self, ManualArgs};
use crate::error::CliError;
use crate::output;

pub(super) fn run(args: &ManualArgs) -> Result<(), CliError> {
    let diretorio = &args.diretorio;
    if diretorio.exists() && !diretorio.is_dir() {
        return Err(CliError::Usage(format!(
            "{} não é um diretório",
            diretorio.display()
        )));
    }
    let paginas = paginas()?;
    fs::create_dir_all(diretorio)
        .map_err(|err| CliError::io(format!("falha ao criar {}", diretorio.display()), err))?;
    for pagina in &paginas {
        let caminho = diretorio.join(&pagina.arquivo);
        fs::write(&caminho, &pagina.texto)
            .map_err(|err| CliError::io(format!("falha ao gravar {}", caminho.display()), err))?;
    }
    output::print(&format!(
        "{} páginas de manual gravadas em {}",
        paginas.len(),
        diretorio.display()
    ))
}

/// A man page: its file name (`inter-pj-pix-enviar.1`) and its roff text.
#[derive(Debug)]
pub(super) struct Pagina {
    pub(super) arquivo: String,
    pub(super) texto: String,
}

/// The page of `inter-pj` and of each command below it, in the order of
/// the help.
pub(super) fn paginas() -> Result<Vec<Pagina>, CliError> {
    let mut raiz = cli::command();
    raiz.build();
    let mut paginas = Vec::new();
    acrescentar(&raiz, &mut paginas)?;
    Ok(paginas)
}

fn acrescentar(comando: &clap::Command, paginas: &mut Vec<Pagina>) -> Result<(), CliError> {
    let nome = comando
        .get_display_name()
        .unwrap_or_else(|| comando.get_name());
    // Every field of the title line: an empty one would shift the others.
    let man = Man::new(comando.clone())
        .title(nome.to_uppercase())
        .date(Local::now().format("%Y-%m-%d").to_string())
        .source(concat!("inter-pj ", env!("CARGO_PKG_VERSION")))
        .manual("Manual do inter-pj");
    let mut roff = Vec::new();
    man.render(&mut roff)
        .map_err(|err| CliError::io("falha ao gerar a página de manual", err))?;
    paginas.push(Pagina {
        arquivo: man.get_filename(),
        texto: traduzir(&String::from_utf8_lossy(&roff)),
    });
    for sub in comando.get_subcommands().filter(|sub| !sub.is_hide_set()) {
        acrescentar(sub, paginas)?;
    }
    Ok(())
}

/// `clap_mangen` writes the headings and a few labels in English, as clap
/// does in the help (see `cli::command`). The text after the options (the
/// `after_help`) is laid out in lines and columns, so it goes as is (`.nf`)
/// instead of being refilled; the version always follows it. The coding
/// line tells `man` that the page is in UTF-8.
fn traduzir(roff: &str) -> String {
    const TROCAS: [(&str, &str); 10] = [
        (".SH NAME\n", ".SH NOME\n"),
        (".SH SYNOPSIS\n", ".SH SINOPSE\n"),
        (".SH DESCRIPTION\n", ".SH DESCRIÇÃO\n"),
        (".SH OPTIONS\n", ".SH OPÇÕES\n"),
        (".SH Comandos\n", ".SH COMANDOS\n"),
        (".SH EXTRA\n", ".SH OBSERVAÇÕES\n.nf\n"),
        (".SH VERSION\n", ".fi\n.SH VERSÃO\n"),
        ("Possible values:", "Valores possíveis:"),
        (
            "May also be specified with the ",
            "Também pode vir da variável de ambiente ",
        ),
        (" environment variable. ", ". "),
    ];
    let mut texto = String::from(".\\\" -*- coding: UTF-8 -*-\n");
    texto.push_str(roff);
    for (de, para) in TROCAS {
        texto = texto.replace(de, para);
    }
    texto.replace("[default: ", "[padrão: ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_are_in_portuguese() {
        let paginas = paginas().unwrap();
        assert_eq!(paginas[0].arquivo, "inter-pj.1");
        let raiz = &paginas[0].texto;
        assert!(raiz.starts_with(".\\\" -*- coding: UTF-8 -*-\n"), "{raiz}");
        let titulo = raiz
            .lines()
            .find(|linha| linha.starts_with(".TH "))
            .unwrap();
        assert!(
            titulo.starts_with(".TH INTER-PJ 1 20")
                && titulo.ends_with(concat!(
                    " \"inter-pj ",
                    env!("CARGO_PKG_VERSION"),
                    "\" \"Manual do inter-pj\""
                )),
            "{titulo}"
        );
        for trecho in [
            ".SH NOME\ninter\\-pj \\- CLI não oficial para a conta PJ do Inter Empresas\n",
            ".SH SINOPSE\n",
            ".SH COMANDOS\n",
            ".SH \"OPÇÕES GLOBAIS\"\n",
            "inter\\-pj\\-pix\\-automatico(1)",
        ] {
            assert!(raiz.contains(trecho), "{trecho}\n{raiz}");
        }
        for pagina in &paginas {
            for ingles in [
                ".SH NAME",
                ".SH SYNOPSIS",
                ".SH DESCRIPTION",
                ".SH OPTIONS",
                ".SH SUBCOMMANDS",
                ".SH EXTRA",
                ".SH VERSION",
                ".SH AUTHORS",
                "Possible values",
                "[default:",
                "May also be specified",
                "environment variable",
            ] {
                assert!(
                    !pagina.texto.contains(ingles),
                    "{}: {ingles}\n{}",
                    pagina.arquivo,
                    pagina.texto
                );
            }
        }
    }
}
