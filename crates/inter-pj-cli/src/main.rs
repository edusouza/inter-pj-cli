//! `inter-pj` — unofficial command line interface for the Inter Empresas (PJ)
//! account: balance, statements and Pix; payments and charges in later
//! versions.
//!
//! Exit codes: 0 success, 1 unexpected error, 2 invalid usage, 3 invalid or
//! missing configuration, 4 authentication/authorization failure, 5 request
//! rejected by the API, 6 service unavailable, rate limited or network
//! failure, 7 operation cancelled at the confirmation, 8 wait timed out, 9
//! operation that may have been processed (check before repeating it).

mod arquivo;
mod cli;
mod commands;
mod config;
mod confirmacao;
mod cores;
mod error;
mod files;
mod logging;
mod output;
mod paths;
mod qr;
mod saida;
mod tabela;
mod token_store;
mod valor;

use std::process::ExitCode;

use clap::FromArgMatches;

fn main() -> ExitCode {
    let sem_cor = cores::sem_cor_pedido(std::env::args_os());
    // Before the parser, whose help and errors have colors too.
    cores::decidir(sem_cor);
    let matches = cli::command()
        .try_get_matches()
        .unwrap_or_else(|err| sair(&err));
    let cli = cli::Cli::from_arg_matches(&matches).unwrap_or_else(|err| sair(&err));
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

/// Ends on an error of the parser, as clap would. A value it quotes (a
/// pasted copia e cola, a barcode) may carry escape sequences: then the
/// message goes without colors, cleaned like every other.
fn sair(err: &clap::Error) -> ! {
    let texto = err.render().to_string();
    if err.use_stderr() && texto.chars().any(|c| output::perigoso(c) && c != '\n') {
        eprint!("{}", output::sem_controle(&texto));
        std::process::exit(err.exit_code());
    }
    err.exit()
}
