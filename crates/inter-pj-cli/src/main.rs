//! `inter-pj` — unofficial command line interface for the Inter Empresas (PJ)
//! account: balance and, in later versions, statements, Pix, payments and
//! charges.
//!
//! Exit codes: 0 success, 1 unexpected error, 2 invalid usage, 3 invalid or
//! missing configuration, 4 authentication/authorization failure, 5 request
//! rejected by the API, 6 service unavailable, rate limited or network failure.

mod cli;
mod commands;
mod config;
mod doctor;
mod error;
mod logging;
mod output;
mod paths;
mod token_store;

use std::process::ExitCode;

use clap::FromArgMatches;

fn main() -> ExitCode {
    let matches = cli::command().get_matches();
    let cli = cli::Cli::from_arg_matches(&matches).unwrap_or_else(|err| err.exit());
    logging::init(cli.global.verbose);

    let result = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| error::CliError::io("falha ao iniciar o runtime assíncrono", err))
        .and_then(|runtime| runtime.block_on(commands::run(cli, &matches)));

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            error::report(&err);
            ExitCode::from(err.exit_code())
        }
    }
}
