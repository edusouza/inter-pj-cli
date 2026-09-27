//! `inter-pj cobranca consultar|pdf`

use std::path::PathBuf;

use inter_pj::cobranca::{CobrancaDetalhada, SituacaoCobranca};
use serde_json::json;

use super::render_cobranca;
use crate::cli::{CobrancaConsultarArgs, CobrancaPdfArgs, Formato};
use crate::commands::Context;
use crate::commands::qrcode::OpcoesQr;
use crate::config::Settings;
use crate::error::CliError;
use crate::output;
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

/// The charge, then its QR Code in the terminal and in a PNG, as asked.
pub(super) fn mostrar(
    context: &Context,
    settings: &Settings,
    cobranca: &CobrancaDetalhada,
    opcoes: &OpcoesQr,
) -> Result<(), CliError> {
    opcoes.mostrar(
        context,
        settings,
        &render_cobranca(cobranca),
        cobranca,
        &copia_e_cola(cobranca),
    )
}

/// The "copia e cola" of the charge's Pix, or why there is none.
fn copia_e_cola(cobranca: &CobrancaDetalhada) -> Result<&str, String> {
    let copia_e_cola = cobranca
        .pix
        .as_ref()
        .and_then(|pix| pix.pix_copia_e_cola.as_deref())
        .filter(|texto| !texto.trim().is_empty());
    copia_e_cola.ok_or_else(|| {
        let motivo = if cobranca.cobranca.situacao == Some(SituacaoCobranca::EmProcessamento) {
            "a cobrança ainda está sendo emitida; consulte de novo em instantes"
        } else {
            "a conta não tinha chave Pix quando a cobrança foi emitida, ou ela só aceita boleto"
        };
        format!("a cobrança não tem Pix copia e cola: {motivo}")
    })
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
