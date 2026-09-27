//! `--qrcode` and `--qrcode-png`: the QR Code of a charge's Pix, in the
//! terminal and in a PNG, next to the charge.

use std::path::PathBuf;

use serde::Serialize;

use super::Context;
use crate::cli::Formato;
use crate::config::Settings;
use crate::error::CliError;
use crate::output;
use crate::qr::{self, QrPix};
use crate::saida::{Saida, tamanho};

/// What to do with the Pix of a charge, besides showing the charge. The
/// default: nothing.
#[derive(Default)]
pub(crate) struct OpcoesQr {
    qrcode: bool,
    png: Option<Saida>,
}

impl OpcoesQr {
    /// Refuses what would mix outputs and checks the image file, before
    /// any request.
    pub(crate) fn new(
        context: &Context,
        qrcode: bool,
        png: Option<PathBuf>,
        sobrescrever: bool,
    ) -> Result<Self, CliError> {
        let png = png.map(|caminho| Saida::new(caminho, sobrescrever));
        if qrcode && (context.formato() == Formato::Json || png.as_ref().is_some_and(Saida::stdout))
        {
            return Err(CliError::Usage(
                "--qrcode desenha no terminal: não combina com --json nem com --qrcode-png -"
                    .to_owned(),
            ));
        }
        if let Some(png) = &png {
            png.conferir()?;
        }
        Ok(Self { qrcode, png })
    }

    /// Whether the standard output is for the image alone.
    pub(crate) fn png_no_stdout(&self) -> bool {
        self.png.as_ref().is_some_and(Saida::stdout)
    }

    /// The charge (`texto`, or `json`), then the QR Code of `copia_e_cola`
    /// in the terminal and in a PNG, as asked. `copia_e_cola` is the
    /// charge's "copia e cola", or why it has none.
    pub(crate) fn mostrar(
        &self,
        context: &Context,
        settings: &Settings,
        texto: &str,
        json: &impl Serialize,
        copia_e_cola: &Result<&str, String>,
    ) -> Result<(), CliError> {
        let qr_code = || {
            copia_e_cola
                .clone()
                .and_then(QrPix::new)
                .map_err(CliError::Usage)
        };
        // The image alone goes to the standard output.
        if let Some(png) = self.png.as_ref().filter(|png| png.stdout()) {
            return png.gravar(&qr_code()?.png());
        }
        match context.formato() {
            Formato::Json => output::print_json(json)?,
            // `commands::run` refuses csv for these commands.
            Formato::Texto | Formato::Csv => {
                context.warn_if_sandbox(settings);
                output::print(texto)?;
            }
        }
        if self.qrcode {
            match qr_code() {
                Ok(qr) => output::print_raw(&format!("\n{}", qr.terminal(qr::cores())))?,
                Err(err) => eprintln!("aviso: {err}"),
            }
        }
        if let Some(png) = &self.png {
            let imagem = qr_code()?.png();
            png.gravar(&imagem)?;
            eprintln!(
                "QR Code salvo em {} ({})",
                png.caminho().display(),
                tamanho(imagem.len())
            );
        }
        Ok(())
    }
}
