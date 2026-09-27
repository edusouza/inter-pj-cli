//! Confirmation of operations that move money.

use std::io::{self, BufRead, IsTerminal, Write};

use inter_pj::Environment;
use rust_decimal::Decimal;

use crate::config::Settings;
use crate::error::CliError;
use crate::output;

/// Where confirmations are asked: the process' terminal, or a fake in tests.
pub(crate) trait Terminal {
    /// Whether someone can see the summary and answer: stdin and stderr
    /// are terminals. With stderr in a file, the question would be
    /// answered without being read.
    fn interativo(&self) -> bool;

    /// Shows `pergunta` on stderr and reads one line from stdin; `None` at
    /// the end of the input.
    fn perguntar(&mut self, pergunta: &str) -> io::Result<Option<String>>;
}

/// stdin and stderr of the process.
#[derive(Debug)]
pub(crate) struct Stdio;

impl Terminal for Stdio {
    fn interativo(&self) -> bool {
        io::stdin().is_terminal() && io::stderr().is_terminal()
    }

    fn perguntar(&mut self, pergunta: &str) -> io::Result<Option<String>> {
        let mut stderr = io::stderr().lock();
        write!(stderr, "{pergunta}")?;
        stderr.flush()?;
        let mut linha = String::new();
        let lidos = io::stdin().lock().read_line(&mut linha)?;
        Ok((lidos > 0).then_some(linha))
    }
}

/// Asks `pergunta` and proceeds only on a yes (`s` or `sim`); anything
/// else, including an empty answer, cancels. `sim` (`--sim`) confirms
/// without asking.
///
/// # Errors
///
/// [`CliError::Usage`] when there is no terminal to ask (and no `--sim`);
/// [`CliError::Cancelado`] when the answer is not yes.
pub(crate) fn confirmar(
    terminal: &mut dyn Terminal,
    sim: bool,
    pergunta: &str,
) -> Result<(), CliError> {
    if sim {
        return Ok(());
    }
    pode_confirmar(terminal, sim)?;
    let resposta = terminal
        .perguntar(&format!("{pergunta} [s/N] "))
        .map_err(|err| CliError::io("falha ao ler a confirmação", err))?;
    match resposta.as_deref().map(str::trim) {
        Some(sim) if sim.eq_ignore_ascii_case("s") || sim.eq_ignore_ascii_case("sim") => Ok(()),
        _ => Err(CliError::Cancelado),
    }
}

/// Fails early, before any request, when [`confirmar`] could not ask.
///
/// # Errors
///
/// [`CliError::Usage`] when there is no terminal to ask and no `--sim`.
pub(crate) fn pode_confirmar(terminal: &dyn Terminal, sim: bool) -> Result<(), CliError> {
    if sim || terminal.interativo() {
        Ok(())
    } else {
        Err(CliError::Usage(
            "confirmação necessária: execute em um terminal, sem redirecionar a entrada nem a saída de erros, para ver o resumo e responder; ou use --sim para confirmar sem perguntar"
                .to_owned(),
        ))
    }
}

/// Refuses amounts above the profile's `limite_por_operacao`, even with
/// `--sim`.
///
/// # Errors
///
/// [`CliError::Usage`] naming the limit and where to change it.
pub(crate) fn verificar_limite(valor: Decimal, settings: &Settings) -> Result<(), CliError> {
    match &settings.limite_por_operacao {
        Some(limite) if valor > limite.value => Err(CliError::Usage(format!(
            "{} passa do limite por operação do perfil \"{}\" ({}); para permitir, ajuste limite_por_operacao em {}",
            output::brl(valor),
            settings.perfil.value,
            output::brl(limite.value),
            settings.config_path.display()
        ))),
        _ => Ok(()),
    }
}

/// The environment of a summary, with production made evident.
pub(crate) fn descrever_ambiente(ambiente: Option<Environment>) -> String {
    match ambiente {
        Some(ambiente) if ambiente.is_production() => "PRODUÇÃO (conta real)".to_owned(),
        Some(_) => "sandbox (dados fictícios)".to_owned(),
        None => "não definido".to_owned(),
    }
}

#[cfg(test)]
pub(crate) mod testes {
    use super::*;

    /// Answers with `resposta` and records what was asked.
    #[derive(Debug, Default)]
    pub(crate) struct TerminalFalso {
        pub(crate) interativo: bool,
        pub(crate) resposta: Option<&'static str>,
        pub(crate) perguntas: Vec<String>,
    }

    impl TerminalFalso {
        pub(crate) fn respondendo(resposta: &'static str) -> Self {
            Self {
                interativo: true,
                resposta: Some(resposta),
                perguntas: Vec::new(),
            }
        }
    }

    impl Terminal for TerminalFalso {
        fn interativo(&self) -> bool {
            self.interativo
        }

        fn perguntar(&mut self, pergunta: &str) -> io::Result<Option<String>> {
            self.perguntas.push(pergunta.to_owned());
            Ok(self.resposta.map(str::to_owned))
        }
    }

    #[test]
    fn proceeds_only_on_yes() {
        for resposta in ["s\n", "S", " sim ", "SIM\r\n"] {
            let mut terminal = TerminalFalso::respondendo(resposta);
            assert!(
                confirmar(&mut terminal, false, "Enviar?").is_ok(),
                "{resposta}"
            );
            assert_eq!(terminal.perguntas, ["Enviar? [s/N] "]);
        }
        for resposta in ["", "\n", "n", "não", "nao", "y", "yes", "ss", "sim!"] {
            let mut terminal = TerminalFalso::respondendo(resposta);
            let err = confirmar(&mut terminal, false, "Enviar?").unwrap_err();
            assert!(matches!(err, CliError::Cancelado), "{resposta}: {err}");
        }
        // End of input is a no.
        let mut terminal = TerminalFalso {
            interativo: true,
            ..TerminalFalso::default()
        };
        assert!(matches!(
            confirmar(&mut terminal, false, "Enviar?"),
            Err(CliError::Cancelado)
        ));
    }

    #[test]
    fn needs_a_terminal_or_sim() {
        let mut terminal = TerminalFalso::default();
        let err = confirmar(&mut terminal, false, "Enviar?").unwrap_err();
        assert!(
            matches!(&err, CliError::Usage(message) if message.contains("--sim")),
            "{err}"
        );
        assert!(terminal.perguntas.is_empty());

        let mut terminal = TerminalFalso::default();
        assert!(confirmar(&mut terminal, true, "Enviar?").is_ok());
        assert!(terminal.perguntas.is_empty());

        assert!(pode_confirmar(&TerminalFalso::default(), false).is_err());
        assert!(pode_confirmar(&TerminalFalso::default(), true).is_ok());
        assert!(pode_confirmar(&TerminalFalso::respondendo("n"), false).is_ok());
    }
}
