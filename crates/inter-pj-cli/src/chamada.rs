//! The command lines the output suggests (`Acompanhe com: inter-pj ...`,
//! `confira antes de tentar de novo: inter-pj ...`): they run on the same
//! account as the command that printed them.
//!
//! The global options that choose the account (the profile, the file, the
//! environment, the credentials), when given on the command line, go into
//! every suggestion; given in the environment, they stay in the shell by
//! themselves. Without them, checking an uncertain payment made with
//! `--perfil outra` would look at the default profile, find nothing, and
//! invite a second payment.

use std::fmt::Write as _;
use std::path::Path;
use std::sync::OnceLock;

use clap::ArgMatches;
use clap::parser::ValueSource;

use crate::cli::GlobalArgs;

/// `inter-pj` and the options of this run that chose the account.
static CHAMADA: OnceLock<String> = OnceLock::new();

/// How to call `inter-pj` again on this account: `inter-pj`, or with the
/// options given on the command line (`inter-pj -p sandbox`).
pub(crate) fn chamada() -> &'static str {
    CHAMADA.get().map_or("inter-pj", String::as_str)
}

/// Fixes [`chamada`] for this run, from the parsed command line.
pub(crate) fn definir(global: &GlobalArgs, matches: &ArgMatches) {
    let _ = CHAMADA.set(montar(global, matches));
}

/// `inter-pj` and each option that chooses the account, when it came from
/// the command line, in a form ready to paste in a shell.
fn montar(global: &GlobalArgs, matches: &ArgMatches) -> String {
    let caminho = |caminho: &Option<std::path::PathBuf>| {
        caminho
            .as_deref()
            .map(Path::display)
            .map(|caminho| caminho.to_string())
    };
    let opcoes = [
        ("config", "--config", caminho(&global.config)),
        ("perfil", "-p", global.perfil.clone()),
        ("ambiente", "--ambiente", global.ambiente.clone()),
        ("client_id", "--client-id", global.client_id.clone()),
        ("certificado", "--certificado", caminho(&global.certificado)),
        (
            "chave_privada",
            "--chave-privada",
            caminho(&global.chave_privada),
        ),
        (
            "conta_corrente",
            "--conta-corrente",
            global.conta_corrente.clone(),
        ),
    ];
    let mut chamada = String::from("inter-pj");
    for (id, opcao, valor) in opcoes {
        if let Some(valor) = valor.filter(|_| da_linha_de_comando(matches, id)) {
            let _ = write!(chamada, " {opcao} {}", argumento(&valor));
        }
    }
    chamada
}

/// Whether the global option `id` came from the command line, before the
/// subcommand or after it.
fn da_linha_de_comando(matches: &ArgMatches, id: &str) -> bool {
    let mut atual = Some(matches);
    while let Some(matches) = atual {
        if matches.value_source(id) == Some(ValueSource::CommandLine) {
            return true;
        }
        atual = matches.subcommand().map(|(_, sub)| sub);
    }
    false
}

/// `texto` as one argument of a shell command: quoted when it has to be.
pub(crate) fn argumento(texto: &str) -> String {
    let simples = |c: char| c.is_ascii_alphanumeric() || "-_./:,+=@%".contains(c);
    if !texto.is_empty() && texto.chars().all(simples) {
        texto.to_owned()
    } else {
        format!("'{}'", texto.replace('\'', r"'\''"))
    }
}

#[cfg(test)]
mod tests {
    use clap::FromArgMatches;

    use super::*;
    use crate::cli::{self, Cli};

    fn chamada_de(args: &[&str]) -> String {
        let matches = cli::command().try_get_matches_from(args).unwrap();
        let cli = Cli::from_arg_matches(&matches).unwrap();
        montar(&cli.global, &matches)
    }

    #[test]
    fn the_options_that_choose_the_account_stay() {
        assert_eq!(chamada_de(&["inter-pj", "saldo"]), "inter-pj");
        assert_eq!(
            chamada_de(&["inter-pj", "-p", "sandbox", "saldo"]),
            "inter-pj -p sandbox"
        );
        // After the subcommand, as clap accepts the global options.
        assert_eq!(
            chamada_de(&["inter-pj", "pix", "consultar", "abc", "--perfil", "filial"]),
            "inter-pj -p filial"
        );
        assert_eq!(
            chamada_de(&[
                "inter-pj",
                "--config",
                "/tmp/minha config.toml",
                "--ambiente",
                "sandbox",
                "--conta-corrente",
                "7654321",
                "saldo",
            ]),
            "inter-pj --config '/tmp/minha config.toml' --ambiente sandbox --conta-corrente 7654321"
        );
        // How the output looks is not about the account.
        assert_eq!(
            chamada_de(&["inter-pj", "--json", "--sem-cache", "saldo"]),
            "inter-pj"
        );
    }

    #[test]
    fn arguments_are_quoted_for_a_shell() {
        assert_eq!(argumento("NF-123"), "NF-123");
        assert_eq!(argumento("pix@empresa.example"), "pix@empresa.example");
        assert_eq!(argumento("Pedido 1"), "'Pedido 1'");
        assert_eq!(argumento("d'água"), r"'d'\''água'");
        assert_eq!(argumento(""), "''");
    }
}
