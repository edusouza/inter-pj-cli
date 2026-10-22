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
        let png =
            png.map(|caminho| Saida::new(caminho, sobrescrever, "qrcode.png", "--qrcode-png"));
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

    /// Whether a QR Code was asked for.
    fn pedido(&self) -> bool {
        self.qrcode || self.png.is_some()
    }
}

/// The charge, then its QR Code in the terminal and in a PNG, as asked. A
/// QR Code that cannot be made fails the command before anything is shown,
/// whichever option asked for it: a script must not read the charge and
/// miss the image.
pub(super) fn mostrar(
    context: &Context,
    settings: &Settings,
    cobranca: &CobrancaDetalhada,
    opcoes: &OpcoesQr,
) -> Result<(), CliError> {
    let qr = if opcoes.pedido() {
        Some(qr_code(cobranca)?)
    } else {
        None
    };
    exibir(context, settings, cobranca, opcoes, qr.as_ref())
}

/// [`mostrar`] for a charge just issued: a QR Code that cannot be made is
/// a warning, and the charge is shown without it, because an error would
/// read as "not issued" and a script would issue the charge again. With the
/// image on the standard output, the charge goes to `stderr`.
pub(super) fn mostrar_emitida(
    context: &Context,
    settings: &Settings,
    cobranca: &CobrancaDetalhada,
    opcoes: &OpcoesQr,
) -> Result<(), CliError> {
    let qr = if opcoes.pedido() {
        match qr_code(cobranca) {
            Ok(qr) => Some(qr),
            Err(err) => {
                eprintln!("aviso: a cobrança foi emitida, mas sem o QR Code: {err}");
                if opcoes.png_no_stdout() {
                    eprintln!("{}", output::sem_controle(&render_cobranca(cobranca)));
                    return Ok(());
                }
                return exibir(context, settings, cobranca, &OpcoesQr::default(), None);
            }
        }
    } else {
        None
    };
    exibir(context, settings, cobranca, opcoes, qr.as_ref())
}

/// The charge and the QR Code `qr`, made for what `opcoes` asks.
fn exibir(
    context: &Context,
    settings: &Settings,
    cobranca: &CobrancaDetalhada,
    opcoes: &OpcoesQr,
    qr: Option<&QrPix>,
) -> Result<(), CliError> {
    // The image alone goes to the standard output.
    if let (Some(png), Some(qr)) = (opcoes.png.as_ref().filter(|png| png.stdout()), qr) {
        return png.gravar(&qr.png());
    }
    match context.formato() {
        Formato::Json => output::print_json(cobranca)?,
        // `commands::run` refuses csv for these commands.
        Formato::Texto | Formato::Csv => {
            context.warn_if_sandbox(settings);
            output::print(&render_cobranca(cobranca))?;
        }
    }
    if let Some(qr) = qr.filter(|_| opcoes.qrcode) {
        output::print_raw(&format!("\n{}", qr.terminal(qr::cores())))?;
    }
    if let (Some(png), Some(qr)) = (&opcoes.png, qr) {
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
