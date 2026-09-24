//! Colors in the terminal: the tables of what goes to the standard output
//! (listings and lookups) highlight their header, the negative amounts and
//! each status by its tone. Only when the standard output is a terminal
//! that takes colors, without `NO_COLOR` and without `--sem-cor`; a
//! pipe, a file, the summaries and the errors on stderr get plain text.
//!
//! The codes are written only by the CLI, around text already cleaned of
//! control characters: [`crate::output::print`] keeps these codes, and
//! only these.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};

use anstream::{AutoStream, ColorChoice};

/// The tone of a status in the tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tom {
    /// Done as expected: paid, received, approved. Green.
    Positivo,
    /// Still under way: scheduled, being processed, awaiting someone.
    /// Yellow.
    Pendente,
    /// Ended without what was expected: cancelled, rejected, expired, an
    /// error. Red.
    Negativo,
}

impl Tom {
    pub(crate) fn codigo(self) -> &'static str {
        match self {
            Self::Positivo => VERDE,
            Self::Pendente => AMARELO,
            Self::Negativo => VERMELHO,
        }
    }
}

pub(crate) const NEGRITO: &str = "\u{1b}[1m";
pub(crate) const VERMELHO: &str = "\u{1b}[31m";
const VERDE: &str = "\u{1b}[32m";
const AMARELO: &str = "\u{1b}[33m";
pub(crate) const FIM: &str = "\u{1b}[0m";

/// Every escape sequence the CLI writes, the only ones that reach the
/// terminal. None of them moves the cursor, erases or hides text.
pub(crate) const CODIGOS: [&str; 5] = [NEGRITO, VERMELHO, VERDE, AMARELO, FIM];

static ATIVAS: AtomicBool = AtomicBool::new(false);

/// Whether `--sem-cor` is among the arguments, before they are parsed: the
/// help and the errors of the parser follow it too.
pub(crate) fn sem_cor_pedido(argumentos: impl IntoIterator<Item = std::ffi::OsString>) -> bool {
    argumentos
        .into_iter()
        .skip(1)
        .take_while(|argumento| argumento != "--")
        .any(|argumento| argumento == "--sem-cor")
}

/// Decides the colors of the process, once, at the start: off with
/// `sem_cor`, and otherwise by the rules of the terminal (a terminal,
/// `TERM` other than `dumb`; `NO_COLOR` turns them off; `CLICOLOR_FORCE`
/// turns them on, even in a pipe).
pub(crate) fn decidir(sem_cor: bool) {
    if sem_cor {
        ColorChoice::Never.write_global();
    }
    let ativas = AutoStream::choice(&io::stdout()) != ColorChoice::Never;
    ATIVAS.store(ativas, Ordering::Relaxed);
}

/// Whether the standard output gets colors. Off until [`decidir`], as in
/// the unit tests.
pub(crate) fn ativas() -> bool {
    ATIVAS.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;

    fn argumentos(lista: &[&str]) -> Vec<OsString> {
        lista.iter().map(OsString::from).collect()
    }

    #[test]
    fn sem_cor_is_seen_before_the_parser() {
        assert!(sem_cor_pedido(argumentos(&[
            "inter-pj",
            "--sem-cor",
            "saldo"
        ])));
        assert!(sem_cor_pedido(argumentos(&[
            "inter-pj",
            "extrato",
            "--sem-cor"
        ])));
        assert!(!sem_cor_pedido(argumentos(&["inter-pj", "saldo"])));
        // Not the name of the program, nor what comes after `--`.
        assert!(!sem_cor_pedido(argumentos(&["--sem-cor", "saldo"])));
        assert!(!sem_cor_pedido(argumentos(&[
            "inter-pj",
            "--",
            "--sem-cor"
        ])));
    }

    #[test]
    fn no_code_moves_the_cursor_or_hides_text() {
        for codigo in CODIGOS {
            let parametros = codigo
                .strip_prefix("\u{1b}[")
                .and_then(|resto| resto.strip_suffix('m'))
                .unwrap();
            assert!(
                ["0", "1", "31", "32", "33"].contains(&parametros),
                "{codigo:?}"
            );
        }
    }
}
