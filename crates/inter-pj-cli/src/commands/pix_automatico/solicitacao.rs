//! `inter-pj pix-automatico solicitacao criar|consultar|cancelar`: the
//! confirmation requests, which ask the payer's bank to have them approve a
//! recurrence.

use std::fmt::Write as _;

use chrono::{DateTime, FixedOffset, Local, NaiveTime, TimeDelta, TimeZone, Timelike};
use inter_pj::pix_automatico::{
    DestinatarioSolicRec, DestinatarioSolicRecGerado, Rec, SolicRec, SolicRecSolicitada, StatusRec,
    StatusSolicRec,
};
use inter_pj::{Environment, Error as InterError, endpoint};

use super::{descrever_calendario, descrever_status, descrever_valor, encerrada};
use crate::cli::{
    Formato, Momento, Prazo, SolicitacaoCancelarArgs, SolicitacaoCommand, SolicitacaoConsultarArgs,
    SolicitacaoCriarArgs,
};
use crate::commands::pix::{documento, pessoa};
use crate::commands::{Context, simulacao};
use crate::confirmacao::{Stdio, Terminal, confirmar, descrever_ambiente, pode_confirmar};
use crate::error::{CliError, resultado_incerto};
use crate::output::{self, horario_em, secao};

/// The time the payer has to answer, without `--expiracao`: 7 days.
const PRAZO_PADRAO: u32 = 7 * 86_400;

pub(super) async fn run(context: &Context, command: SolicitacaoCommand) -> Result<(), CliError> {
    match command {
        SolicitacaoCommand::Criar(args) => criar(context, &args, &mut Stdio).await,
        SolicitacaoCommand::Consultar(args) => consultar(context, &args).await,
        SolicitacaoCommand::Cancelar(args) => cancelar(context, &args, &mut Stdio).await,
    }
}

async fn criar(
    context: &Context,
    args: &SolicitacaoCriarArgs,
    terminal: &mut dyn Terminal,
) -> Result<(), CliError> {
    let agora = Local::now().fixed_offset();
    let expiracao = expira_em(args.expiracao, agora, &Local)?;
    let mut destinatario =
        DestinatarioSolicRec::new(args.documento.clone(), args.conta.trim(), args.ispb.trim());
    destinatario.agencia = args
        .agencia
        .as_deref()
        .map(|agencia| agencia.trim().to_owned());
    let solicitacao = SolicRecSolicitada::new(args.rec.clone(), expiracao, destinatario);
    solicitacao.validar().map_err(|err| {
        let opcao = match err.campo() {
            "destinatario.conta" => "--conta",
            "destinatario.ispbParticipante" => "--ispb",
            "destinatario.agencia" => "--agencia",
            outro => outro,
        };
        CliError::Usage(format!("{opcao}: {err}"))
    })?;
    let settings = context.settings()?;
    let ambiente = settings.ambiente.as_ref().map(|setting| setting.value);
    if args.simular {
        eprintln!("{}", resumo(&solicitacao, None, ambiente));
        return simulacao::mostrar(
            context,
            &settings,
            endpoint::pix_automatico::CRIAR_SOLICITACAO,
            &[],
            &solicitacao,
        );
    }
    // Nothing is looked up when no one could confirm.
    pode_confirmar(terminal, args.sim)?;
    let client = context.client(&settings)?;
    let rec = client
        .pix_automatico()
        .consultar_rec(&args.rec, None)
        .await?;
    aguarda_aprovacao(&rec)?;
    eprintln!("{}", resumo(&solicitacao, Some(&rec), ambiente));
    confirmar(
        terminal,
        args.sim,
        "Enviar a solicitação ao banco do pagador?",
    )?;

    let criada = client
        .pix_automatico()
        .criar_solicitacao(&solicitacao)
        .await
        .map_err(|err| incerta(err, &solicitacao))?;
    match context.formato() {
        Formato::Json => output::print_json(&criada),
        // `commands::run` refuses csv for this command.
        Formato::Texto | Formato::Csv => {
            let id = criada.id_solic_rec.as_deref().unwrap_or_default();
            output::print(&format!(
                "Solicitação criada: o banco do pagador vai pedir que ele aprove a recorrência.\n\n{}\n\nAcompanhe com: inter-pj pix-automatico solicitacao consultar {id}\nou pela recorrência: inter-pj pix-automatico rec consultar {}",
                render_solicitacao(&criada),
                args.rec
            ))
        }
    }
}

/// When the request expires: `prazo` from `agora`, the end of a day in
/// `fuso`, or a moment; never in the past.
fn expira_em<Tz: TimeZone>(
    prazo: Option<Prazo>,
    agora: DateTime<FixedOffset>,
    fuso: &Tz,
) -> Result<DateTime<FixedOffset>, CliError> {
    let agora = agora.with_nanosecond(0).unwrap_or(agora);
    let expiracao = match prazo.unwrap_or(Prazo::Duracao(PRAZO_PADRAO)) {
        Prazo::Duracao(segundos) => agora
            .checked_add_signed(TimeDelta::seconds(i64::from(segundos)))
            .ok_or_else(|| {
                CliError::Usage(format!("--expiracao: prazo longo demais ({segundos} s)"))
            })?,
        Prazo::Momento(Momento::Instante(instante)) => instante,
        Prazo::Momento(Momento::Dia(dia)) => {
            let fim = NaiveTime::from_hms_opt(23, 59, 59).unwrap_or(NaiveTime::MIN);
            fuso.from_local_datetime(&dia.and_time(fim))
                .latest()
                .map(|momento| momento.fixed_offset())
                .ok_or_else(|| {
                    CliError::Usage(format!(
                        "--expiracao: {dia} 23:59:59 não existe no fuso local"
                    ))
                })?
        }
    };
    if expiracao <= agora {
        return Err(CliError::Usage(format!(
            "--expiracao: {} já passou",
            expiracao.format("%d/%m/%Y %H:%M:%S")
        )));
    }
    Ok(expiracao)
}

/// Refuses a recurrence the payer can no longer be asked about.
fn aguarda_aprovacao(rec: &Rec) -> Result<(), CliError> {
    match &rec.status {
        Some(status) if encerrada(Some(status)) => Err(CliError::Usage(format!(
            "a recorrência está {}: não há o que aprovar",
            descrever_status(status)
        ))),
        Some(StatusRec::Aprovada) => Err(CliError::Usage(
            "a recorrência já foi aprovada pelo pagador".to_owned(),
        )),
        _ => Ok(()),
    }
}

/// The error of a creation: with an unknown outcome, how to check before
/// trying again, as the API has no idempotency key.
fn incerta(err: InterError, solicitacao: &SolicRecSolicitada) -> CliError {
    if resultado_incerto(&err) {
        CliError::CriacaoIncerta {
            source: err,
            situacao: "a solicitação pode ter sido enviada ao pagador",
            consulta: format!(
                "inter-pj pix-automatico rec consultar {}",
                solicitacao.id_rec
            ),
        }
    } else {
        err.into()
    }
}

/// The request about to be sent, with the recurrence the payer will see.
fn resumo(
    solicitacao: &SolicRecSolicitada,
    rec: Option<&Rec>,
    ambiente: Option<Environment>,
) -> String {
    let mut linhas = vec![
        ("Ambiente", descrever_ambiente(ambiente)),
        ("Recorrência", solicitacao.id_rec.to_string()),
    ];
    if let Some(rec) = rec {
        linhas.extend(dados_da_recorrencia(rec));
    }
    let destinatario = &solicitacao.destinatario;
    let mut conta = format!(
        "{}, banco com ISPB {}",
        destinatario.documento.formatado(),
        destinatario.ispb_participante
    );
    if let Some(agencia) = &destinatario.agencia {
        let _ = write!(conta, ", agência {agencia}");
    }
    let _ = write!(conta, ", conta {}", destinatario.conta);
    linhas.push(("Conta do pagador", conta));
    linhas.push((
        "Expira em",
        solicitacao
            .expiracao
            .with_timezone(&Local)
            .format("%d/%m/%Y %H:%M:%S")
            .to_string(),
    ));
    let mut texto = String::new();
    if ambiente.is_some_and(Environment::is_production) {
        texto.push_str("*** PRODUÇÃO: o pedido chega ao pagador de verdade ***\n");
    }
    texto.push_str(&secao("Solicitação de confirmação a enviar", &linhas));
    texto
}

/// What the payer is asked to approve.
fn dados_da_recorrencia(rec: &Rec) -> Vec<(&'static str, String)> {
    let mut linhas = Vec::new();
    if let Some(vinculo) = &rec.vinculo {
        if let Some(devedor) = vinculo.devedor.as_ref().and_then(pessoa) {
            linhas.push(("Devedor", devedor));
        }
        if let Some(contrato) = &vinculo.contrato {
            linhas.push(("Contrato", contrato.clone()));
        }
        if let Some(objeto) = &vinculo.objeto {
            linhas.push(("Objeto", objeto.clone()));
        }
    }
    if let Some(periodo) = rec.calendario.as_ref().and_then(descrever_calendario) {
        linhas.push(("Periodicidade", periodo));
    }
    linhas.push(("Valor", descrever_valor(rec.valor.as_ref())));
    linhas
}

async fn consultar(context: &Context, args: &SolicitacaoConsultarArgs) -> Result<(), CliError> {
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let solicitacao = client
        .pix_automatico()
        .consultar_solicitacao(&args.id)
        .await?;
    match context.formato() {
        Formato::Json => output::print_json(&solicitacao),
        // `commands::run` refuses csv for this command.
        Formato::Texto | Formato::Csv => {
            context.warn_if_sandbox(&settings);
            output::print(&render_solicitacao(&solicitacao))
        }
    }
}

async fn cancelar(
    context: &Context,
    args: &SolicitacaoCancelarArgs,
    terminal: &mut dyn Terminal,
) -> Result<(), CliError> {
    // Nothing is looked up when no one could confirm.
    pode_confirmar(terminal, args.sim)?;
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let atual = client
        .pix_automatico()
        .consultar_solicitacao(&args.id)
        .await?;
    cancelavel(atual.status.as_ref())?;
    let ambiente = settings.ambiente.as_ref().map(|setting| setting.value);
    eprintln!(
        "{}",
        secao(
            &format!("Solicitação {} a cancelar", args.id),
            &cancelamento(&atual, ambiente)
        )
    );
    confirmar(terminal, args.sim, "Cancelar a solicitação?")?;
    let cancelada = client
        .pix_automatico()
        .cancelar_solicitacao(&args.id)
        .await?;
    match context.formato() {
        Formato::Json => output::print_json(&cancelada),
        // `commands::run` refuses csv for this command.
        Formato::Texto | Formato::Csv => output::print(&format!(
            "Solicitação cancelada.\n\n{}",
            render_solicitacao(&cancelada)
        )),
    }
}

/// The API cancels only requests created or received, not yet answered.
fn cancelavel(status: Option<&StatusSolicRec>) -> Result<(), CliError> {
    match status {
        Some(StatusSolicRec::Criada | StatusSolicRec::Recebida | StatusSolicRec::Outro(_))
        | None => Ok(()),
        Some(status) => Err(CliError::Usage(format!(
            "a solicitação está {}: só as criadas ou recebidas, ainda sem resposta, podem ser canceladas",
            descrever_status_solicitacao(status)
        ))),
    }
}

/// The lines of the summary of a cancellation.
fn cancelamento(atual: &SolicRec, ambiente: Option<Environment>) -> Vec<(&'static str, String)> {
    let mut linhas = vec![("Ambiente", descrever_ambiente(ambiente))];
    if let Some(status) = &atual.status {
        linhas.push(("Status", descrever_status_solicitacao(status).to_owned()));
    }
    if let Some(id) = &atual.id_rec {
        linhas.push(("Recorrência", id.clone()));
    }
    if let Some(conta) = atual.destinatario.as_ref().and_then(conta) {
        linhas.push(("Conta do pagador", conta));
    }
    linhas
}

/// A status of a request in words: `RECEBIDA` -> `recebida pelo pagador`.
pub(super) fn descrever_status_solicitacao(status: &StatusSolicRec) -> &str {
    match status {
        StatusSolicRec::Criada => "criada (aguarda o envio)",
        StatusSolicRec::Enviada => "enviada ao pagador",
        StatusSolicRec::Recebida => "recebida pelo pagador",
        StatusSolicRec::Rejeitada => "rejeitada pelo pagador",
        StatusSolicRec::Aceita => "aceita pelo pagador",
        StatusSolicRec::Expirada => "expirada sem resposta",
        StatusSolicRec::Cancelada => "cancelada",
        outro => outro.as_str(),
    }
}

/// `123.456.789-09, banco com ISPB 12345678, agência 0001, conta 1234567`.
fn conta(destinatario: &DestinatarioSolicRecGerado) -> Option<String> {
    let mut partes = Vec::new();
    if let Some(doc) = destinatario.cpf.as_deref().or(destinatario.cnpj.as_deref()) {
        partes.push(documento(doc));
    }
    if let Some(ispb) = &destinatario.ispb_participante {
        partes.push(format!("banco com ISPB {ispb}"));
    }
    if let Some(agencia) = &destinatario.agencia {
        partes.push(format!("agência {agencia}"));
    }
    if let Some(conta) = &destinatario.conta {
        partes.push(format!("conta {conta}"));
    }
    (!partes.is_empty()).then(|| partes.join(", "))
}

/// A request in detail, with the times in the local time zone.
fn render_solicitacao(solicitacao: &SolicRec) -> String {
    render_solicitacao_em(solicitacao, &Local)
}

fn render_solicitacao_em<Tz: TimeZone>(solicitacao: &SolicRec, fuso: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let mut linhas = Vec::new();
    if let Some(status) = &solicitacao.status {
        linhas.push(("Status", descrever_status_solicitacao(status).to_owned()));
    }
    if let Some(id) = &solicitacao.id_rec {
        linhas.push(("Recorrência", id.clone()));
    }
    if let Some(conta) = solicitacao.destinatario.as_ref().and_then(conta) {
        linhas.push(("Conta do pagador", conta));
    }
    if let Some(expira) = solicitacao
        .calendario
        .as_ref()
        .and_then(|calendario| calendario.data_expiracao_solicitacao.as_deref())
    {
        linhas.push(("Expira em", horario_em(expira, fuso)));
    }
    if let Some(rec) = &solicitacao.rec_payload {
        linhas.extend(dados_da_recorrencia(rec));
    }
    let titulo = format!(
        "Solicitação de confirmação {}",
        solicitacao.id_solic_rec.as_deref().unwrap_or_default()
    );
    let mut texto = secao(titulo.trim(), &linhas);
    let historico: Vec<(String, String)> = solicitacao
        .atualizacao
        .iter()
        .map(|atualizacao| {
            (
                atualizacao
                    .data
                    .as_deref()
                    .map(|data| horario_em(data, fuso))
                    .unwrap_or_default(),
                atualizacao
                    .status
                    .as_ref()
                    .map(|status| descrever_status_solicitacao(status).to_owned())
                    .unwrap_or_default(),
            )
        })
        .collect();
    if !historico.is_empty() {
        let linhas: Vec<(&str, String)> = historico
            .iter()
            .map(|(quando, oque)| (quando.as_str(), oque.clone()))
            .collect();
        let _ = write!(texto, "\n\n{}", secao("Histórico", &linhas));
    }
    texto
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use serde_json::json;

    use super::*;

    fn brasilia() -> FixedOffset {
        FixedOffset::west_opt(3 * 3600).unwrap()
    }

    fn agora() -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339("2026-09-24T10:00:00.250-03:00").unwrap()
    }

    #[test]
    fn deadlines_are_counted_from_now_or_end_a_day() {
        let em = |prazo| expira_em(prazo, agora(), &brasilia()).map(|momento| momento.to_rfc3339());
        assert_eq!(em(None).unwrap(), "2026-10-01T10:00:00-03:00");
        assert_eq!(
            em(Some(Prazo::Duracao(7200))).unwrap(),
            "2026-09-24T12:00:00-03:00"
        );
        let dia = NaiveDate::from_ymd_opt(2026, 9, 30).unwrap();
        assert_eq!(
            em(Some(Prazo::Momento(Momento::Dia(dia)))).unwrap(),
            "2026-09-30T23:59:59-03:00"
        );
        let ontem = NaiveDate::from_ymd_opt(2026, 9, 23).unwrap();
        assert!(em(Some(Prazo::Momento(Momento::Dia(ontem)))).is_err());
    }

    fn solicitacao(status: &str) -> SolicRec {
        serde_json::from_value(json!({
            "idSolicRec": "SC1234567820260924abcdefghijk",
            "idRec": "RR1234567820260924abcdefghijk",
            "calendario": {"dataExpiracaoSolicitacao": "2026-10-01T13:00:00.000Z"},
            "status": status,
            "destinatario": {"cpf": "12345678909", "conta": "1234567", "ispbParticipante": "12345678", "agencia": "0001"},
            "atualizacao": [{"status": "CRIADA", "data": "2026-09-24T13:00:00.000Z"}, {"status": "ENVIADA", "data": "2026-09-24T13:00:05.000Z"}],
            "recPayload": {
                "idRec": "RR1234567820260924abcdefghijk",
                "vinculo": {"contrato": "contrato-001", "devedor": {"cpf": "12345678909", "nome": "Cliente Exemplo"}},
                "calendario": {"dataInicial": "2026-10-10", "periodicidade": "MENSAL"},
                "valor": {"valorRec": "149.90"}
            }
        }))
        .unwrap()
    }

    #[test]
    fn a_request_in_detail() {
        assert_eq!(
            render_solicitacao_em(&solicitacao("ENVIADA"), &brasilia()),
            "\
Solicitação de confirmação SC1234567820260924abcdefghijk
  Status            enviada ao pagador
  Recorrência       RR1234567820260924abcdefghijk
  Conta do pagador  123.456.789-09, banco com ISPB 12345678, agência 0001, conta 1234567
  Expira em         01/10/2026 10:00:00
  Devedor           Cliente Exemplo (123.456.789-09)
  Contrato          contrato-001
  Periodicidade     mensal, a partir de 10/10/2026, sem fim
  Valor             R$ 149,90 em cada pagamento

Histórico
  24/09/2026 10:00:00  criada (aguarda o envio)
  24/09/2026 10:00:05  enviada ao pagador"
        );
    }

    #[test]
    fn only_requests_without_an_answer_are_cancelled() {
        assert!(cancelavel(Some(&StatusSolicRec::Criada)).is_ok());
        assert!(cancelavel(Some(&StatusSolicRec::Recebida)).is_ok());
        for status in [
            StatusSolicRec::Enviada,
            StatusSolicRec::Aceita,
            StatusSolicRec::Rejeitada,
            StatusSolicRec::Expirada,
            StatusSolicRec::Cancelada,
        ] {
            assert!(cancelavel(Some(&status)).is_err(), "{status}");
        }
    }
}
