//! `inter-pj pix`: Pix sent (Banking API) and the charges, Pix received and
//! refunds of the Pix API.

mod cob;
mod consultar;
mod enviar;

use chrono::{DateTime, Days, FixedOffset, Local, NaiveDate, NaiveTime, TimeZone};
use inter_pj::Error as InterError;
use inter_pj::documento::Documento;
use inter_pj::pix::{PeriodoPix, PessoaPix, PixRecebido, StatusCob, Txid};
use rust_decimal::Decimal;

use super::Context;
use crate::cli::{Momento, PeriodoPixArgs, PixCommand};
use crate::confirmacao::Stdio;
use crate::error::{CliError, resultado_incerto};
use crate::output::horario_em;
use crate::tabela::{Celula, Coluna, Tabela};

/// Days in the default period of the listings (the last 30, today
/// included).
const DIAS_PADRAO: u64 = 30;

pub(super) async fn run(context: &Context, command: PixCommand) -> Result<(), CliError> {
    match command {
        PixCommand::Enviar(args) => enviar::run(context, &args, &mut Stdio).await,
        PixCommand::Consultar(args) => consultar::run(context, &args).await,
        PixCommand::Cob(command) => cob::run(context, command).await,
    }
}

/// The period of the arguments, in the local time zone: by default, from
/// the start of the day 30 days ago (today included) to now.
fn periodo(args: PeriodoPixArgs) -> Result<PeriodoPix, CliError> {
    periodo_em(args, Local::now().fixed_offset(), &Local)
}

fn periodo_em<Tz: TimeZone>(
    args: PeriodoPixArgs,
    agora: DateTime<FixedOffset>,
    fuso: &Tz,
) -> Result<PeriodoPix, CliError> {
    let dia = |dia: NaiveDate, hora: NaiveTime| {
        fuso.from_local_datetime(&dia.and_time(hora))
            .earliest()
            .map(|momento| momento.fixed_offset())
            .ok_or_else(|| CliError::Usage(format!("{dia}: horário inexistente no fuso local")))
    };
    let fim_do_dia = NaiveTime::from_hms_opt(23, 59, 59).unwrap_or(NaiveTime::MIN);
    let fim = match args.fim {
        Some(Momento::Instante(instante)) => instante,
        Some(Momento::Dia(fim)) => dia(fim, fim_do_dia)?,
        None => agora,
    };
    let inicio = match args.inicio {
        Some(Momento::Instante(instante)) => instante,
        Some(Momento::Dia(inicio)) => dia(inicio, NaiveTime::MIN)?,
        None => {
            let hoje = agora.with_timezone(fuso).date_naive();
            let inicio = hoje
                .checked_sub_days(Days::new(DIAS_PADRAO - 1))
                .unwrap_or(hoje);
            dia(inicio, NaiveTime::MIN)?
        }
    };
    PeriodoPix::new(inicio, fim).map_err(|err| CliError::Usage(err.to_string()))
}

/// A status in words: `CONCLUIDA` -> `concluída (paga)`.
fn descrever_status(status: &StatusCob) -> &str {
    match status {
        StatusCob::Ativa => "ativa",
        StatusCob::Concluida => "concluída (paga)",
        StatusCob::RemovidaPeloUsuarioRecebedor => "removida pelo recebedor",
        StatusCob::RemovidaPeloPsp => "removida pelo banco",
        outro => outro.as_str(),
    }
}

/// A CPF or CNPJ with punctuation, or as received.
fn documento(texto: &str) -> String {
    Documento::parse(texto).map_or_else(|_| texto.to_owned(), |doc| doc.formatado())
}

/// `Empresa Exemplo (12.345.678/0001-95)`.
fn pessoa(pessoa: &PessoaPix) -> Option<String> {
    let nome = pessoa.nome.as_deref().unwrap_or_default().trim();
    match (nome.is_empty(), pessoa.documento()) {
        (true, None) => None,
        (false, None) => Some(nome.to_owned()),
        (true, Some(doc)) => Some(documento(doc)),
        (false, Some(doc)) => Some(format!("{nome} ({})", documento(doc))),
    }
}

/// The Pix received by a charge, with the times in `fuso`.
fn tabela_pix<Tz: TimeZone>(pix: &[PixRecebido], fuso: &Tz) -> Tabela
where
    Tz::Offset: std::fmt::Display,
{
    let mut tabela = Tabela::new(vec![
        Coluna::texto("Horário", ""),
        Coluna::valor("Valor", ""),
        Coluna::valor("Devolvido", ""),
        Coluna::texto("endToEndId", ""),
    ]);
    for recebido in pix {
        let devolvido: Decimal = recebido
            .devolucoes
            .iter()
            .filter_map(|devolucao| devolucao.valor)
            .sum();
        tabela.linha(vec![
            Celula::texto(
                recebido
                    .horario
                    .as_deref()
                    .map(|horario| horario_em(horario, fuso))
                    .as_deref(),
            ),
            Celula::dinheiro(recebido.valor),
            Celula::dinheiro((!devolvido.is_zero()).then_some(devolvido)),
            Celula::texto(recebido.end_to_end_id.as_deref()),
        ]);
    }
    tabela
}

/// The error of a creation: with an unknown outcome, how to check and
/// repeat it with the same txid.
fn incerta(err: InterError, tipo: &'static str, txid: &Txid) -> CliError {
    if resultado_incerto(&err) {
        CliError::CobrancaPixIncerta {
            source: err,
            tipo,
            txid: txid.to_string(),
        }
    } else {
        err.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn momento(texto: &str) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(texto).unwrap()
    }

    #[test]
    fn periods_are_whole_days_in_the_local_time_zone() {
        let brasilia = FixedOffset::west_opt(3 * 3600).unwrap();
        let agora = momento("2026-09-23T15:00:00-03:00");
        let padrao = periodo_em(
            PeriodoPixArgs {
                inicio: None,
                fim: None,
            },
            agora,
            &brasilia,
        )
        .unwrap();
        assert_eq!(padrao.inicio, momento("2026-08-25T00:00:00-03:00"));
        assert_eq!(padrao.fim, agora);

        let dia = |ano, mes, dia| {
            Some(Momento::Dia(
                NaiveDate::from_ymd_opt(ano, mes, dia).unwrap(),
            ))
        };
        let setembro = periodo_em(
            PeriodoPixArgs {
                inicio: dia(2026, 9, 1),
                fim: dia(2026, 9, 30),
            },
            agora,
            &brasilia,
        )
        .unwrap();
        assert_eq!(setembro.inicio, momento("2026-09-01T00:00:00-03:00"));
        assert_eq!(setembro.fim, momento("2026-09-30T23:59:59-03:00"));

        let instante = periodo_em(
            PeriodoPixArgs {
                inicio: Some(Momento::Instante(momento("2026-09-23T08:00:00Z"))),
                fim: None,
            },
            agora,
            &brasilia,
        )
        .unwrap();
        assert_eq!(instante.inicio, momento("2026-09-23T08:00:00Z"));

        let invertido = periodo_em(
            PeriodoPixArgs {
                inicio: dia(2026, 9, 30),
                fim: dia(2026, 9, 1),
            },
            agora,
            &brasilia,
        );
        assert!(invertido.is_err());
    }

    #[test]
    fn people_show_name_and_document() {
        let mut devedor = PessoaPix::default();
        assert_eq!(pessoa(&devedor), None);
        devedor.cnpj = Some("12345678000195".to_owned());
        assert_eq!(pessoa(&devedor).as_deref(), Some("12.345.678/0001-95"));
        devedor.nome = Some("Empresa Exemplo".to_owned());
        assert_eq!(
            pessoa(&devedor).as_deref(),
            Some("Empresa Exemplo (12.345.678/0001-95)")
        );
    }
}
