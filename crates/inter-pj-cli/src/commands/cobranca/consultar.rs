//! `inter-pj cobranca consultar|pdf`

use std::path::PathBuf;

use inter_pj::cobranca::{CobrancaDetalhada, SituacaoCobranca};
use serde_json::json;

use super::render_cobranca;
use crate::cli::{CobrancaConsultarArgs, CobrancaPdfArgs, Formato};
use crate::commands::Context;
use crate::error::CliError;
use crate::output;
use crate::qr::{self, QrPix};
use crate::saida::{Saida, tamanho};

pub(super) async fn consultar(
    context: &Context,
    args: &CobrancaConsultarArgs,
) -> Result<(), CliError> {
    let png = args
        .qrcode_png
        .clone()
        .map(|caminho| Saida::new(caminho, args.sobrescrever, "qrcode.png", "--qrcode-png"));
    let png_no_stdout = png.as_ref().is_some_and(Saida::stdout);
    if args.qrcode && (context.formato() == Formato::Json || png_no_stdout) {
        return Err(CliError::Usage(
            "--qrcode desenha no terminal: não combina com --json nem com --qrcode-png -"
                .to_owned(),
        ));
    }
    if let Some(png) = &png {
        png.conferir()?;
    }
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let cobranca = client.cobranca().consultar(&args.codigo).await?;
    // A QR Code that cannot be made fails the command before anything is
    // shown, whichever option asked for it: a script must not read the
    // charge and miss the image.
    let qr = if args.qrcode || png.is_some() {
        Some(qr_code(&cobranca)?)
    } else {
        None
    };

    // The image alone goes to the standard output.
    if let (Some(png), Some(qr)) = (png.as_ref().filter(|png| png.stdout()), &qr) {
        return png.gravar(&qr.png());
    }
    match context.formato() {
        Formato::Json => output::print_json(&cobranca)?,
        // `commands::run` refuses csv for this command.
        Formato::Texto | Formato::Csv => {
            context.warn_if_sandbox(&settings);
            output::print(&render_cobranca(&cobranca))?;
        }
    }
    if let Some(qr) = qr.as_ref().filter(|_| args.qrcode) {
        output::print_raw(&format!("\n{}", qr.terminal(qr::cores())))?;
    }
    if let (Some(png), Some(qr)) = (&png, &qr) {
        let imagem = qr.png();
        png.gravar(&imagem)?;
        eprintln!(
            "QR Code salvo em {} ({})",
            png.caminho().display(),
            tamanho(imagem.len())
        );
    }
    Ok(())
}

/// The QR Code of the charge's Pix, or why there is none.
fn qr_code(cobranca: &CobrancaDetalhada) -> Result<QrPix, CliError> {
    let copia_e_cola = cobranca
        .pix
        .as_ref()
        .and_then(|pix| pix.pix_copia_e_cola.as_deref())
        .filter(|texto| !texto.trim().is_empty());
    let Some(copia_e_cola) = copia_e_cola else {
        let motivo = if cobranca.cobranca.situacao == Some(SituacaoCobranca::EmProcessamento) {
            "a cobrança ainda está sendo emitida; consulte de novo em instantes"
        } else {
            "a conta não tinha chave Pix quando a cobrança foi emitida, ou ela só aceita boleto"
        };
        return Err(CliError::SemQrCode(format!(
            "a cobrança não tem Pix copia e cola: {motivo}"
        )));
    };
    QrPix::new(copia_e_cola).map_err(CliError::SemQrCode)
}

pub(super) async fn pdf(context: &Context, args: &CobrancaPdfArgs) -> Result<(), CliError> {
    let codigo = args.codigo.trim();
    // Only letters, digits and hyphens of the code go into the default name.
    let nome: String = codigo
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    let caminho = args
        .saida
        .clone()
        .unwrap_or_else(|| PathBuf::from(format!("cobranca-{nome}.pdf")));
    let saida = Saida::new(caminho, args.sobrescrever, "cobranca.pdf", "--saida");
    saida.conferir()?;

    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let documento = client.cobranca().pdf(codigo).await?;
    saida.gravar(&documento)?;
    if saida.stdout() {
        return Ok(());
    }
    match context.formato() {
        Formato::Json => output::print_json(&json!({
            "arquivo": saida.caminho().display().to_string(),
            "bytes": documento.len(),
            "codigoSolicitacao": codigo,
        })),
        // `commands::run` refuses csv for this command.
        Formato::Texto | Formato::Csv => {
            context.warn_if_sandbox(&settings);
            output::print(&format!(
                "Cobrança salva em {} ({})",
                saida.caminho().display(),
                tamanho(documento.len())
            ))
        }
    }
}
