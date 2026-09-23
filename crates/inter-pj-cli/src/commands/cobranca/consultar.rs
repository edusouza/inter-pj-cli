//! `inter-pj cobranca consultar|pdf`

use std::path::PathBuf;

use inter_pj::cobranca::{CobrancaDetalhada, SituacaoCobranca};
use serde_json::json;

use super::render_cobranca;
use crate::cli::{CobrancaConsultarArgs, CobrancaPdfArgs, Formato};
use crate::commands::Context;
use crate::config::Settings;
use crate::error::CliError;
use crate::output;
use crate::qr::{self, QrPix};
use crate::saida::{Saida, tamanho};

pub(super) async fn consultar(
    context: &Context,
    args: &CobrancaConsultarArgs,
) -> Result<(), CliError> {
    let opcoes = OpcoesQr::new(
        context,
        args.qrcode,
        args.qrcode_png.clone(),
        args.sobrescrever,
    )?;
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let cobranca = client.cobranca().consultar(&args.codigo).await?;
    mostrar(context, &settings, &cobranca, &opcoes)
}

/// What to do with the Pix of a charge, besides showing the charge. The
/// default: nothing.
#[derive(Default)]
pub(super) struct OpcoesQr {
    qrcode: bool,
    png: Option<Saida>,
}

impl OpcoesQr {
    /// Refuses what would mix outputs and checks the image file, before
    /// any request.
    pub(super) fn new(
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
    pub(super) fn png_no_stdout(&self) -> bool {
        self.png.as_ref().is_some_and(Saida::stdout)
    }
}

/// The charge, then its QR Code in the terminal and in a PNG, as asked.
pub(super) fn mostrar(
    context: &Context,
    settings: &Settings,
    cobranca: &CobrancaDetalhada,
    opcoes: &OpcoesQr,
) -> Result<(), CliError> {
    // The image alone goes to the standard output.
    if let Some(png) = opcoes.png.as_ref().filter(|png| png.stdout()) {
        return png.gravar(&qr_code(cobranca)?.png());
    }
    match context.formato() {
        Formato::Json => output::print_json(cobranca)?,
        // `commands::run` refuses csv for these commands.
        Formato::Texto | Formato::Csv => {
            context.warn_if_sandbox(settings);
            output::print(&render_cobranca(cobranca))?;
        }
    }
    if opcoes.qrcode {
        match qr_code(cobranca) {
            Ok(qr) => output::print_raw(&format!("\n{}", qr.terminal(qr::cores())))?,
            Err(err) => eprintln!("aviso: {err}"),
        }
    }
    if let Some(png) = &opcoes.png {
        let imagem = qr_code(cobranca)?.png();
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
        return Err(CliError::Usage(format!(
            "a cobrança não tem Pix copia e cola: {motivo}"
        )));
    };
    QrPix::new(copia_e_cola).map_err(CliError::Usage)
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
    let saida = Saida::new(caminho, args.sobrescrever);
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
