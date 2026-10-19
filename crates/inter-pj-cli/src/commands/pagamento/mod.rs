//! `inter-pj pagamento`

mod boleto;
mod pagar;

use chrono::NaiveDate;
use inter_pj::banking::StatusPagamento;

use super::Context;
use crate::cli::PagamentoCommand;
use crate::confirmacao::Stdio;
use crate::error::CliError;
use crate::tabela::Celula;

pub(super) async fn run(context: &Context, command: PagamentoCommand) -> Result<(), CliError> {
    match command {
        PagamentoCommand::Boleto(command) => boleto::run(context, command, &mut Stdio).await,
    }
}

/// A status in words: `REALIZADO` -> `pago`.
fn descrever_status(status: &StatusPagamento) -> &str {
    match status {
        StatusPagamento::EmProcessamento => "em processamento",
        StatusPagamento::AguardandoAprovacao => "aguardando aprovação",
        StatusPagamento::Aprovado => "aprovado",
        StatusPagamento::Reprovado => "reprovado",
        StatusPagamento::AprovacaoExpirada => "aprovação expirada",
        StatusPagamento::Agendado => "agendado",
        StatusPagamento::AgendadoCancelado => "agendamento cancelado",
        StatusPagamento::Cancelado => "cancelado",
        StatusPagamento::Realizado => "pago",
        StatusPagamento::Erro => "erro",
        StatusPagamento::NaoCompensado => "não compensado",
        other => other.as_str(),
    }
}

/// A date as the API sends it (`2026-10-09`, `2026-10-09 00:00:00` or
/// `09/10/2026`); other formats are kept as text.
fn data(raw: Option<&str>) -> Celula {
    Celula::data(raw.and_then(parse_data), raw)
}

/// [`data`] as text: `09/10/2026`, or the raw text.
fn data_br(raw: &str) -> String {
    parse_data(raw).map_or_else(|| raw.to_owned(), |dia| dia.format("%d/%m/%Y").to_string())
}

fn parse_data(raw: &str) -> Option<NaiveDate> {
    let raw = raw.trim();
    NaiveDate::parse_from_str(raw.get(..10).unwrap_or(raw), "%Y-%m-%d")
        .or_else(|_| NaiveDate::parse_from_str(raw, "%d/%m/%Y"))
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_documented_status_is_described() {
        for status in StatusPagamento::DOCUMENTADOS {
            assert_ne!(descrever_status(status), status.as_str(), "{status}");
        }
        assert_eq!(
            descrever_status(&StatusPagamento::Outro("NOVO".to_owned())),
            "NOVO"
        );
    }

    #[test]
    fn dates_in_the_formats_of_the_api() {
        let dia = Celula::Data(NaiveDate::from_ymd_opt(2026, 10, 9).unwrap());
        for raw in [
            "2026-10-09",
            "2026-10-09 00:00:00",
            "09/10/2026",
            " 2026-10-09T10:00",
        ] {
            assert_eq!(data(Some(raw)), dia, "{raw}");
        }
        assert_eq!(data(Some("amanhã")), Celula::texto(Some("amanhã")));
        assert_eq!(data(None), Celula::Vazia);
        assert_eq!(data_br("2026-10-09 00:00:00"), "09/10/2026");
        assert_eq!(data_br("amanhã"), "amanhã");
    }
}
