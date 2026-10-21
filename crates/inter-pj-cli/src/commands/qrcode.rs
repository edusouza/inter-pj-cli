//! `--qrcode` and `--qrcode-png`: the QR Code of a charge's Pix, in the
//! terminal and in a PNG, next to the charge.

use std::path::PathBuf;

use serde::Serialize;

use super::Context;
use crate::chamada::argumento;
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
        self.mostrar_com(context, settings, texto, json, copia_e_cola, None)
    }

    /// [`mostrar`](Self::mostrar) for a charge the command has just
    /// created: a PNG that cannot be made or saved is a warning with the
    /// command that saves it later, `consulta` and the image option, and
    /// not an error, which a script would read as "nothing was created"
    /// and repeat. With the image on the standard output, the charge goes
    /// to `stderr` when the image fails.
    pub(crate) fn mostrar_criada(
        &self,
        context: &Context,
        settings: &Settings,
        texto: &str,
        json: &impl Serialize,
        copia_e_cola: &Result<&str, String>,
        consulta: &str,
    ) -> Result<(), CliError> {
        self.mostrar_com(context, settings, texto, json, copia_e_cola, Some(consulta))
    }

    fn mostrar_com(
        &self,
        context: &Context,
        settings: &Settings,
        texto: &str,
        json: &impl Serialize,
        copia_e_cola: &Result<&str, String>,
        criada: Option<&str>,
    ) -> Result<(), CliError> {
        let qr_code = || {
            copia_e_cola
                .clone()
                .and_then(QrPix::new)
                .map_err(CliError::Usage)
        };
        // After a creation, the failure of the image is only a warning.
        let falhou = |png: &Saida, err: CliError| match criada {
            None => Err(err),
            Some(consulta) => {
                eprintln!(
                    "aviso: a cobrança foi criada, mas o QR Code não foi gravado: {}",
                    output::limpo(&err.to_string())
                );
                eprintln!(
                    "aviso: para gravá-lo, use: {consulta} --qrcode-png {}",
                    argumento(&png.caminho().display().to_string())
                );
                Ok(())
            }
        };
        // The image alone goes to the standard output.
        if let Some(png) = self.png.as_ref().filter(|png| png.stdout()) {
            return match qr_code().and_then(|qr| png.gravar(&qr.png())) {
                Ok(()) => Ok(()),
                Err(err) => {
                    if criada.is_some() {
                        eprintln!("{}", output::sem_controle(texto));
                    }
                    falhou(png, err)
                }
            };
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
            let gravada = qr_code().and_then(|qr| {
                let imagem = qr.png();
                png.gravar(&imagem).map(|()| imagem.len())
            });
            match gravada {
                Ok(bytes) => eprintln!(
                    "QR Code salvo em {} ({})",
                    png.caminho().display(),
                    tamanho(bytes)
                ),
                Err(err) => falhou(png, err)?,
            }
        }
        Ok(())
    }
}
