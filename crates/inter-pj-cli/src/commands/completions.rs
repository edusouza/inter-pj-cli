//! `inter-pj completions <shell>`: the completion script of the shell,
//! generated from the command definition.

use clap_complete::Shell;

use crate::cli::{self, CompletionsArgs};
use crate::error::CliError;
use crate::output;

pub(super) fn run(args: &CompletionsArgs) -> Result<(), CliError> {
    output::print_raw(&script(args.shell))
}

/// The script, generated in memory: `clap_complete` panics when a write
/// fails, and a closed pipe (`| head`) is not an error here.
pub(super) fn script(shell: Shell) -> String {
    let mut script = Vec::new();
    clap_complete::generate(shell, &mut cli::command(), "inter-pj", &mut script);
    let script = String::from_utf8_lossy(&script).into_owned();
    match shell {
        // clap_complete 4.6 names the commands `inter__pj__subcmd__pix`
        // while completing, but labels their cases with the dash of the
        // binary as a separator too (`inter__subcmd__pj__subcmd__pix`): bash
        // would complete only the first word. Without the bug, nothing to
        // replace.
        Shell::Bash => script.replace("inter__subcmd__pj", "inter__pj"),
        _ => script,
    }
}

#[cfg(test)]
mod tests {
    use clap::ValueEnum;

    use super::*;

    #[test]
    fn every_shell_gets_a_script_with_the_commands() {
        for shell in Shell::value_variants() {
            let script = script(*shell);
            for comando in ["pix-automatico", "completions", "manual"] {
                assert!(script.contains(comando), "{shell}: {comando}");
            }
        }
    }
}
