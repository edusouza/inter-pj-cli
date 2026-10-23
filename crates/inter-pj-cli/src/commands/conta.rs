//! `inter-pj conta`

use serde_json::json;

use super::Context;
use crate::cli::Formato;
use crate::config::NumeroConta;
use crate::error::CliError;
use crate::output;

/// Code of Banco Inter in the Brazilian payment system (COMPE).
pub(super) const BANCO: &str = "077";
pub(super) const NOME_BANCO: &str = "Banco Inter S.A.";
/// Banco Inter has a single branch, the same for every account.
pub(super) const AGENCIA: &str = "0001";

/// The bank, the branch and the account of the profile. The API has no
/// operation that returns them: the account comes from the configuration
/// file, and no request is made.
pub(super) fn run(context: &Context) -> Result<(), CliError> {
    let settings = context.settings()?;
    let conta = settings.for_conta()?;
    match context.formato() {
        Formato::Json => output::print_json(&json!({
            "banco": BANCO,
            "nomeBanco": NOME_BANCO,
            "agencia": AGENCIA,
            "numeroConta": conta.numero,
            "digitoConta": conta.digito,
        })),
        // `commands::run` refuses csv for this command.
        Formato::Texto | Formato::Csv => output::print(&render(conta)),
    }
}

fn render(conta: &NumeroConta) -> String {
    output::key_values_left(&[
        ("Banco", format!("{BANCO} - {NOME_BANCO}")),
        ("Agência", AGENCIA.to_owned()),
        ("Conta", conta.to_string()),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_bank_branch_and_account() {
        let conta = NumeroConta {
            numero: "7654321".to_owned(),
            digito: "0".to_owned(),
        };
        assert_eq!(
            render(&conta),
            "Banco    077 - Banco Inter S.A.\nAgência  0001\nConta    7654321-0"
        );
    }
}
