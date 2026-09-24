//! `inter-pj pix-automatico sandbox
//! status-rec|status-solicitacao|status-cobr|pagar-cobr|pagar-qrcode`: the
//! answers of the payer and of their bank, simulated in the sandbox, to test
//! the whole flow of Pix Automático. In production they are the payer's:
//! refused before any request.

use inter_pj::documento::Documento;
use inter_pj::pix_automatico::{
    IdRec, RazaoCancelamentoCobR, RazaoCancelamentoRec, StatusRec, StatusSolicRec,
};
use serde_json::{Map, Value, json};

use crate::cli::{
    Formato, SandboxPagarCobrArgs, SandboxPixAutomaticoCommand, SandboxStatusCobrArgs,
    SandboxStatusRecArgs, SandboxStatusSolicitacaoArgs, StatusRecSandboxArg,
    StatusSolicitacaoSandboxArg,
};
use crate::commands::Context;
use crate::commands::pix::{mostrar_pagamento, pagar_qrcode};
use crate::config::Settings;
use crate::error::CliError;
use crate::output;

pub(super) async fn run(
    context: &Context,
    command: SandboxPixAutomaticoCommand,
) -> Result<(), CliError> {
    match command {
        SandboxPixAutomaticoCommand::StatusRec(args) => status_rec(context, &args).await,
        SandboxPixAutomaticoCommand::StatusSolicitacao(args) => {
            status_solicitacao(context, &args).await
        }
        SandboxPixAutomaticoCommand::StatusCobr(args) => status_cobr(context, &args).await,
        SandboxPixAutomaticoCommand::PagarCobr(args) => pagar_cobr(context, &args).await,
        SandboxPixAutomaticoCommand::PagarQrcode(args) => {
            pagar_qrcode(context, &args, "pix-automatico sandbox pagar-qrcode").await
        }
    }
}

/// Refuses the simulations outside the sandbox, before anything else.
fn so_no_sandbox(settings: &Settings, comando: &str) -> Result<(), CliError> {
    if settings
        .ambiente
        .as_ref()
        .is_some_and(|ambiente| ambiente.value.is_production())
    {
        return Err(CliError::Usage(format!(
            "pix-automatico sandbox {comando} existe só no sandbox, para testes: em produção, quem responde e paga é o pagador, no banco dele"
        )));
    }
    Ok(())
}

async fn status_rec(context: &Context, args: &SandboxStatusRecArgs) -> Result<(), CliError> {
    let settings = context.settings()?;
    so_no_sandbox(&settings, "status-rec")?;
    if args.razao.is_some() && args.status != StatusRecSandboxArg::Cancelada {
        return Err(CliError::Usage(
            "--razao vale só com --status cancelada".to_owned(),
        ));
    }
    let status = StatusRec::from(args.status);
    let razao = args.razao.map(RazaoCancelamentoRec::from);
    let client = context.client(&settings)?;
    client
        .pix_automatico()
        .alterar_status_rec_no_sandbox(&args.id_rec, &status, razao.as_ref())
        .await?;
    let mut resposta = Map::new();
    resposta.insert("idRec".to_owned(), json!(args.id_rec.as_str()));
    resposta.insert("status".to_owned(), json!(status));
    if let Some(razao) = &razao {
        resposta.insert("razao".to_owned(), json!(razao));
    }
    let feito = match (args.status, &razao) {
        (StatusRecSandboxArg::Aprovada, _) => "aprovada".to_owned(),
        (StatusRecSandboxArg::Cancelada, Some(razao)) => format!("cancelada (motivo {razao})"),
        (StatusRecSandboxArg::Cancelada, None) => "cancelada".to_owned(),
    };
    mostrar(
        context,
        &resposta,
        &format!("Recorrência {} {feito} no sandbox.", args.id_rec),
        &consultar_rec(&args.id_rec),
    )
}

async fn status_solicitacao(
    context: &Context,
    args: &SandboxStatusSolicitacaoArgs,
) -> Result<(), CliError> {
    let settings = context.settings()?;
    so_no_sandbox(&settings, "status-solicitacao")?;
    let status = StatusSolicRec::from(args.status);
    let client = context.client(&settings)?;
    client
        .pix_automatico()
        .alterar_status_solicitacao_no_sandbox(&args.id_rec, &status)
        .await?;
    let feito = match args.status {
        StatusSolicitacaoSandboxArg::Aceita => "aceita pelo pagador",
        StatusSolicitacaoSandboxArg::Rejeitada => "rejeitada pelo pagador",
    };
    let mut resposta = Map::new();
    resposta.insert("idRec".to_owned(), json!(args.id_rec.as_str()));
    resposta.insert("status".to_owned(), json!(status));
    mostrar(
        context,
        &resposta,
        &format!(
            "Solicitação de confirmação da recorrência {} {feito} no sandbox.",
            args.id_rec
        ),
        &consultar_rec(&args.id_rec),
    )
}

async fn status_cobr(context: &Context, args: &SandboxStatusCobrArgs) -> Result<(), CliError> {
    let settings = context.settings()?;
    so_no_sandbox(&settings, "status-cobr")?;
    let razao = RazaoCancelamentoCobR::from(args.razao);
    let client = context.client(&settings)?;
    client
        .pix_automatico()
        .cancelar_cobr_no_sandbox(&args.txid, &razao)
        .await?;
    let mut resposta = Map::new();
    resposta.insert("txid".to_owned(), json!(args.txid.as_str()));
    resposta.insert("status".to_owned(), json!("CANCELADA"));
    resposta.insert("razao".to_owned(), json!(razao));
    mostrar(
        context,
        &resposta,
        &format!(
            "Cobrança recorrente {} cancelada pelo banco do pagador no sandbox (motivo {razao}).",
            args.txid
        ),
        &format!("inter-pj pix-automatico cobr consultar {}", args.txid),
    )
}

/// Pays a recurring charge, by default its amount, by the payer of its
/// recurrence.
async fn pagar_cobr(context: &Context, args: &SandboxPagarCobrArgs) -> Result<(), CliError> {
    let settings = context.settings()?;
    so_no_sandbox(&settings, "pagar-cobr")?;
    let client = context.client(&settings)?;
    let cobr = if args.valor.is_none() || args.documento.is_none() {
        Some(client.pix_automatico().consultar_cobr(&args.txid).await?)
    } else {
        None
    };
    let valor = match (args.valor, &cobr) {
        (Some(valor), _) => valor,
        (None, cobr) => cobr
            .as_ref()
            .and_then(|cobr| cobr.valor.as_ref())
            .and_then(|valor| valor.original)
            .ok_or_else(|| {
                CliError::Usage(
                    "a API não informou o valor da cobrança: informe --valor".to_owned(),
                )
            })?,
    };
    let documento = match (&args.documento, &cobr) {
        (Some(documento), _) => documento.clone(),
        (None, cobr) => {
            let id_rec = cobr
                .as_ref()
                .and_then(|cobr| cobr.id_rec.as_deref())
                .and_then(|id| IdRec::parse(id).ok())
                .ok_or_else(sem_pagador)?;
            let rec = client.pix_automatico().consultar_rec(&id_rec, None).await?;
            rec.vinculo
                .as_ref()
                .and_then(|vinculo| vinculo.devedor.as_ref())
                .and_then(|devedor| devedor.documento())
                .and_then(|documento| Documento::parse(documento).ok())
                .ok_or_else(sem_pagador)?
        }
    };
    let pagamento = client
        .pix_automatico()
        .pagar_cobr_no_sandbox(&args.txid, valor, &documento, &args.chave)
        .await?;
    mostrar_pagamento(
        context,
        &pagamento,
        valor,
        &format!("inter-pj pix-automatico cobr consultar {}", args.txid),
    )
}

fn sem_pagador() -> CliError {
    CliError::Usage(
        "a API não informou o devedor da recorrência: informe --documento, o CPF ou CNPJ de quem paga"
            .to_owned(),
    )
}

/// `inter-pj pix-automatico rec consultar ...`.
fn consultar_rec(id_rec: &IdRec) -> String {
    format!("inter-pj pix-automatico rec consultar {id_rec}")
}

/// What the sandbox did: the fields sent, in JSON, or `texto` and the
/// command that shows the result.
fn mostrar(
    context: &Context,
    resposta: &Map<String, Value>,
    texto: &str,
    confira: &str,
) -> Result<(), CliError> {
    match context.formato() {
        Formato::Json => output::print_json(resposta),
        // `commands::run` refuses csv for these commands.
        Formato::Texto | Formato::Csv => {
            output::print(&format!("{texto}\n\nConfira com: {confira}"))
        }
    }
}
