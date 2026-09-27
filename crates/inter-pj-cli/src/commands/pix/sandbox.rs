//! `inter-pj pix cob pagar`, `pix cobv pagar` and `pix sandbox
//! pagar-qrcode`: payments of Pix charges in the sandbox, to test the whole
//! flow. In production, the client pays: they are refused before any
//! request.

use std::fmt::Write as _;

use inter_pj::pix::PagamentoSandbox;
use rust_decimal::Decimal;
use serde_json::json;

use crate::cli::{Formato, PixPagarQrcodeArgs, PixSandboxCommand, PixSandboxPagarArgs};
use crate::commands::Context;
use crate::config::Settings;
use crate::error::CliError;
use crate::output;

pub(super) async fn run(context: &Context, command: PixSandboxCommand) -> Result<(), CliError> {
    match command {
        PixSandboxCommand::PagarQrcode(args) => {
            pagar_qrcode(context, &args, "pix sandbox pagar-qrcode").await
        }
    }
}

/// Refuses the payments outside the sandbox, before anything else.
fn so_no_sandbox(settings: &Settings, comando: &str) -> Result<(), CliError> {
    if settings
        .ambiente
        .as_ref()
        .is_some_and(|ambiente| ambiente.value.is_production())
    {
        return Err(CliError::Usage(format!(
            "{comando} existe só no sandbox, para testes: em produção, quem paga é o cliente, com o QR Code ou o copia e cola"
        )));
    }
    Ok(())
}

/// `pix cob pagar`: pays an immediate charge, by default its amount.
pub(super) async fn pagar_cob(
    context: &Context,
    args: &PixSandboxPagarArgs,
) -> Result<(), CliError> {
    let settings = context.settings()?;
    so_no_sandbox(&settings, "pix cob pagar")?;
    let client = context.client(&settings)?;
    let valor = if let Some(valor) = args.valor {
        valor
    } else {
        let cob = client.pix().consultar_cob(&args.txid).await?;
        valor_da_cobranca(cob.valor.and_then(|valor| valor.original))?
    };
    let pagamento = client.pix().pagar_cob_no_sandbox(&args.txid, valor).await?;
    mostrar(
        context,
        &pagamento,
        valor,
        &format!("inter-pj pix cob consultar {}", args.txid),
    )
}

/// `pix cobv pagar`: pays a charge with a due date, by default its original
/// amount.
pub(super) async fn pagar_cobv(
    context: &Context,
    args: &PixSandboxPagarArgs,
) -> Result<(), CliError> {
    let settings = context.settings()?;
    so_no_sandbox(&settings, "pix cobv pagar")?;
    let client = context.client(&settings)?;
    let valor = if let Some(valor) = args.valor {
        valor
    } else {
        let cobv = client.pix().consultar_cobv(&args.txid).await?;
        valor_da_cobranca(cobv.valor.and_then(|valor| valor.original))?
    };
    let pagamento = client
        .pix()
        .pagar_cobv_no_sandbox(&args.txid, valor)
        .await?;
    mostrar(
        context,
        &pagamento,
        valor,
        &format!("inter-pj pix cobv consultar {}", args.txid),
    )
}

fn valor_da_cobranca(valor: Option<Decimal>) -> Result<Decimal, CliError> {
    valor.ok_or_else(|| {
        CliError::Usage("a API não informou o valor da cobrança: informe --valor".to_owned())
    })
}

/// `pix sandbox pagar-qrcode` (and `comando`, the same for Pix Automático):
/// pays a "copia e cola", by default the amount in it.
pub(crate) async fn pagar_qrcode(
    context: &Context,
    args: &PixPagarQrcodeArgs,
    comando: &str,
) -> Result<(), CliError> {
    let settings = context.settings()?;
    so_no_sandbox(&settings, comando)?;
    let valor = args
        .valor
        .or(args.copia_e_cola.brcode.valor)
        .ok_or_else(|| {
            CliError::Usage("o código copia e cola não traz o valor: informe --valor".to_owned())
        })?;
    let client = context.client(&settings)?;
    let pagamento = client
        .pix()
        .pagar_copia_e_cola_no_sandbox(&args.copia_e_cola.codigo, valor)
        .await?;
    mostrar(context, &pagamento, valor, "inter-pj pix recebidos listar")
}

/// A payment of the sandbox, and the command that shows what it paid.
pub(crate) fn mostrar(
    context: &Context,
    pagamento: &PagamentoSandbox,
    valor: Decimal,
    confira: &str,
) -> Result<(), CliError> {
    let e2e = pagamento.end_to_end_id();
    match context.formato() {
        Formato::Json => output::print_json(&json!({
            "endToEndId": e2e,
            "valor": format!("{valor:.2}"),
        })),
        // `commands::run` refuses csv for these commands.
        Formato::Texto | Formato::Csv => {
            let mut texto = format!("Pago no sandbox: {}.", output::brl(valor));
            if let Some(e2e) = e2e {
                let _ = write!(texto, "\nendToEndId  {e2e}");
            }
            output::print(&format!("{texto}\n\nConfira com: {confira}"))
        }
    }
}
