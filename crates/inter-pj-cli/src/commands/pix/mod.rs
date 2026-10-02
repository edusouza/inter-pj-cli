//! `inter-pj pix`

mod consultar;
mod enviar;

use super::Context;
use crate::cli::PixCommand;
use crate::confirmacao::Stdio;
use crate::error::CliError;

pub(super) async fn run(context: &Context, command: PixCommand) -> Result<(), CliError> {
    match command {
        PixCommand::Enviar(args) => enviar::run(context, &args, &mut Stdio).await,
        PixCommand::Consultar(args) => consultar::run(context, &args).await,
    }
}
