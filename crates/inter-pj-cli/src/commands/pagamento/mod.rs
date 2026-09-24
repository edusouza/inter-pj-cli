//! `inter-pj pagamento`

mod boleto;
mod darf;
mod lote;
mod pagar;

use inter_pj::banking::StatusPagamento;

use super::Context;
use crate::cli::PagamentoCommand;
use crate::confirmacao::Stdio;
use crate::cores::Tom;
use crate::error::CliError;
use crate::output::{data_br, parse_data};
use crate::tabela::Celula;

pub(super) async fn run(context: &Context, command: PagamentoCommand) -> Result<(), CliError> {
    match command {
        PagamentoCommand::Boleto(command) => boleto::run(context, command, &mut Stdio).await,
        PagamentoCommand::Darf(command) => darf::run(context, command, &mut Stdio).await,
        PagamentoCommand::Lote(command) => lote::run(context, command, &mut Stdio).await,
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

/// The status of a payment in a table: under way, paid, or not paid.
fn celula_status(status: Option<&StatusPagamento>) -> Celula {
    let tom = status.and_then(|status| match status {
        StatusPagamento::EmProcessamento
        | StatusPagamento::AguardandoAprovacao
        | StatusPagamento::Aprovado
        | StatusPagamento::Agendado => Some(Tom::Pendente),
        StatusPagamento::Realizado => Some(Tom::Positivo),
        StatusPagamento::Reprovado
        | StatusPagamento::AprovacaoExpirada
        | StatusPagamento::AgendadoCancelado
        | StatusPagamento::Cancelado
        | StatusPagamento::Erro
        | StatusPagamento::NaoCompensado => Some(Tom::Negativo),
        _ => None,
    });
    Celula::situacao(status.map(descrever_status), tom)
}

/// A date as the API sends it (`2026-10-09`, `2026-10-09 00:00:00` or
/// `09/10/2026`); other formats are kept as text.
fn data(raw: Option<&str>) -> Celula {
    Celula::data(raw.and_then(parse_data), raw)
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::*;

    #[test]
    fn every_documented_status_has_a_tone() {
        for status in StatusPagamento::DOCUMENTADOS {
            assert!(
                matches!(celula_status(Some(status)), Celula::Situacao(..)),
                "{status:?}"
            );
        }
        assert_eq!(
            celula_status(Some(&StatusPagamento::Realizado)),
            Celula::Situacao("pago".into(), Tom::Positivo)
        );
        assert_eq!(
            celula_status(Some(&StatusPagamento::Agendado)),
            Celula::Situacao("agendado".into(), Tom::Pendente)
        );
        assert_eq!(
            celula_status(Some(&StatusPagamento::NaoCompensado)),
            Celula::Situacao("não compensado".into(), Tom::Negativo)
        );
    }

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
