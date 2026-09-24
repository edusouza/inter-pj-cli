//! `inter-pj pix`: Pix sent (Banking API) and the charges, Pix received and
//! refunds of the Pix API.

mod cob;
mod cobv;
mod consultar;
mod devolucao;
mod encargos;
mod enviar;
mod loc;
mod lote_cobv;
mod recebidos;
mod sandbox;

use std::fmt::Write as _;

use chrono::{DateTime, Days, FixedOffset, Local, NaiveDate, NaiveTime, TimeZone};
use inter_pj::Error as InterError;
use inter_pj::documento::Documento;
use inter_pj::pix::{
    Paginacao, ParametrosConsulta, PeriodoPix, PessoaPix, PixRecebido, StatusCob, StatusDevolucao,
    Txid,
};
use rust_decimal::Decimal;

pub(super) use self::sandbox::{mostrar as mostrar_pagamento, pagar_qrcode};
use super::Context;
use crate::cli::{Momento, PeriodoPixArgs, PixCobListarArgs, PixCommand, StatusCobArg};
use crate::confirmacao::Stdio;
use crate::cores::Tom;
use crate::error::{CliError, resultado_incerto};
use crate::output::{self, horario_em};
use crate::tabela::{Celula, Coluna, Tabela};

/// Days in the default period of the listings (the last 30, today
/// included).
const DIAS_PADRAO: u64 = 30;

pub(super) async fn run(context: &Context, command: PixCommand) -> Result<(), CliError> {
    match command {
        PixCommand::Enviar(args) => enviar::run(context, &args, &mut Stdio).await,
        PixCommand::Consultar(args) => consultar::run(context, &args).await,
        PixCommand::Cob(command) => cob::run(context, command).await,
        PixCommand::Cobv(command) => cobv::run(context, command).await,
        PixCommand::Recebidos(command) => recebidos::run(context, command).await,
        PixCommand::Devolucao(command) => devolucao::run(context, command).await,
        PixCommand::Loc(command) => loc::run(context, command).await,
        PixCommand::LoteCobv(command) => lote_cobv::run(context, command).await,
        PixCommand::Sandbox(command) => sandbox::run(context, command).await,
    }
}

/// The period of the arguments, in the local time zone: by default, from
/// the start of the day 30 days ago (today included) to now.
pub(crate) fn periodo(args: PeriodoPixArgs) -> Result<PeriodoPix, CliError> {
    periodo_em(args, super::agora(), &Local)
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

/// The status of a charge in a table: waiting for the payment, paid or
/// removed.
fn celula_status(status: Option<&StatusCob>) -> Celula {
    let tom = status.and_then(|status| match status {
        StatusCob::Ativa => Some(Tom::Pendente),
        StatusCob::Concluida => Some(Tom::Positivo),
        StatusCob::RemovidaPeloUsuarioRecebedor | StatusCob::RemovidaPeloPsp => Some(Tom::Negativo),
        _ => None,
    });
    Celula::situacao(status.map(descrever_status), tom)
}

/// A CPF or CNPJ with punctuation, or as received.
pub(super) fn documento(texto: &str) -> String {
    Documento::parse(texto).map_or_else(|_| texto.to_owned(), |doc| doc.formatado())
}

/// `Empresa Exemplo (12.345.678/0001-95)`.
pub(super) fn pessoa(pessoa: &PessoaPix) -> Option<String> {
    let nome = pessoa.nome.as_deref().unwrap_or_default().trim();
    match (nome.is_empty(), pessoa.documento()) {
        (true, None) => None,
        (false, None) => Some(nome.to_owned()),
        (true, Some(doc)) => Some(documento(doc)),
        (false, Some(doc)) => Some(format!("{nome} ({})", documento(doc))),
    }
}

/// `Avenida Brasil, 1200 - Belo Horizonte/MG - CEP 30110-000`.
pub(super) fn endereco(
    logradouro: Option<&str>,
    cidade: Option<&str>,
    uf: Option<&str>,
    cep: Option<&str>,
) -> Option<String> {
    let mut partes: Vec<String> = logradouro.map(str::to_owned).into_iter().collect();
    match (cidade, uf) {
        (Some(cidade), Some(uf)) => partes.push(format!("{cidade}/{uf}")),
        (Some(parte), None) | (None, Some(parte)) => partes.push(parte.to_owned()),
        (None, None) => {}
    }
    if let Some(cep) = cep {
        let cep = match (cep.get(..5), cep.get(5..)) {
            (Some(inicio), Some(fim)) if cep.len() == 8 => format!("{inicio}-{fim}"),
            _ => cep.to_owned(),
        };
        partes.push(format!("CEP {cep}"));
    }
    (!partes.is_empty()).then(|| partes.join(" - "))
}

/// The Pix received by a charge, with the times in `fuso`.
pub(super) fn tabela_pix<Tz: TimeZone>(pix: &[PixRecebido], fuso: &Tz) -> Tabela
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
        let devolvido = devolvido(recebido);
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

/// What was refunded of a Pix: the refunds made.
fn devolvido(pix: &PixRecebido) -> Decimal {
    pix.devolucoes
        .iter()
        .filter(|devolucao| devolucao.status == Some(StatusDevolucao::Devolvido))
        .filter_map(|devolucao| devolucao.valor)
        .sum()
}

/// What is being refunded: the refunds neither made nor refused, those of
/// unknown status included.
fn em_devolucao(pix: &PixRecebido) -> Decimal {
    pix.devolucoes
        .iter()
        .filter(|devolucao| {
            !matches!(
                devolucao.status,
                Some(StatusDevolucao::Devolvido | StatusDevolucao::NaoRealizado)
            )
        })
        .filter_map(|devolucao| devolucao.valor)
        .sum()
}

/// What can still be refunded of a Pix, when its amount is known.
fn disponivel(pix: &PixRecebido) -> Option<Decimal> {
    pix.valor
        .map(|valor| valor - devolvido(pix) - em_devolucao(pix))
}

/// A refund status in words: `NAO_REALIZADO` -> `não realizada`.
fn descrever_status_devolucao(status: &StatusDevolucao) -> &str {
    match status {
        StatusDevolucao::EmProcessamento => "em processamento",
        StatusDevolucao::Devolvido => "devolvida",
        StatusDevolucao::NaoRealizado => "não realizada",
        outro => outro.as_str(),
    }
}

fn celula_status_devolucao(status: Option<&StatusDevolucao>) -> Celula {
    let tom = status.and_then(|status| match status {
        StatusDevolucao::EmProcessamento => Some(Tom::Pendente),
        StatusDevolucao::Devolvido => Some(Tom::Positivo),
        StatusDevolucao::NaoRealizado => Some(Tom::Negativo),
        _ => None,
    });
    Celula::situacao(status.map(descrever_status_devolucao), tom)
}

/// The error of the creation of a charge with a txid: with an unknown
/// outcome, how to check (with `comando`, `pix cob`) and repeat it with the
/// same txid.
pub(super) fn incerta(err: InterError, comando: &'static str, txid: &Txid) -> CliError {
    if resultado_incerto(&err) {
        CliError::CobrancaPixIncerta {
            source: err,
            comando,
            txid: txid.to_string(),
        }
    } else {
        err.into()
    }
}

/// Refuses what can no longer change: a charge paid or removed.
fn alteravel(status: Option<&StatusCob>) -> Result<(), CliError> {
    let motivo = match status {
        Some(StatusCob::Concluida) => "já foi paga",
        Some(StatusCob::RemovidaPeloUsuarioRecebedor | StatusCob::RemovidaPeloPsp) => {
            "já foi removida"
        }
        _ => return Ok(()),
    };
    Err(CliError::Usage(format!(
        "a cobrança {motivo}: não pode ser alterada"
    )))
}

/// The "copia e cola" of a charge that can still be paid, or why there is
/// none.
fn copia_e_cola_ativa<'a>(
    status: Option<&StatusCob>,
    texto: Option<&'a str>,
) -> Result<&'a str, String> {
    match status {
        Some(StatusCob::Ativa) | None => texto
            .filter(|texto| !texto.trim().is_empty())
            .ok_or_else(|| "a API não informou o copia e cola desta cobrança".to_owned()),
        Some(status) => Err(format!(
            "a cobrança está {}: o QR Code não serve mais para pagar",
            descrever_status(status)
        )),
    }
}

/// `R$ 37,00 → R$ 40,00`, or what stays.
pub(super) fn antes_e_depois(antes: Option<String>, depois: Option<String>) -> Option<String> {
    match (antes, depois) {
        (Some(antes), Some(depois)) => Some(format!("{antes} → {depois}")),
        (None, Some(depois)) => Some(format!("→ {depois}")),
        (antes, None) => antes,
    }
}

/// The filters of the listings of charges, from the options.
struct Filtros {
    periodo: PeriodoPix,
    devedor: Option<Documento>,
    status: Option<StatusCob>,
    location_presente: Option<bool>,
}

impl Filtros {
    fn de(args: &PixCobListarArgs) -> Result<Self, CliError> {
        Ok(Self {
            periodo: periodo(args.periodo)?,
            devedor: args.documento.clone(),
            status: args.status.map(|status| match status {
                StatusCobArg::Ativa => StatusCob::Ativa,
                StatusCobArg::Concluida => StatusCob::Concluida,
                StatusCobArg::RemovidaPeloUsuario => StatusCob::RemovidaPeloUsuarioRecebedor,
                StatusCobArg::RemovidaPeloPsp => StatusCob::RemovidaPeloPsp,
            }),
            location_presente: match (args.com_location, args.sem_location) {
                (true, _) => Some(true),
                (_, true) => Some(false),
                _ => None,
            },
        })
    }

    /// `Cobranças Pix imediatas criadas de 25/08/2026 00:00 a 23/09/2026
    /// 23:59`, and the filters, plus `outros`.
    fn titulo(&self, cobrancas: &str, outros: &[String]) -> String {
        let formato = "%d/%m/%Y %H:%M";
        let mut texto = format!(
            "{cobrancas} criadas de {} a {}",
            self.periodo.inicio.format(formato),
            self.periodo.fim.format(formato)
        );
        let mut filtros = Vec::new();
        if let Some(status) = &self.status {
            filtros.push(descrever_status(status).to_owned());
        }
        if let Some(documento) = &self.devedor {
            filtros.push(format!("devedor {}", documento.formatado()));
        }
        match self.location_presente {
            Some(true) => filtros.push("com location".to_owned()),
            Some(false) => filtros.push("sem location".to_owned()),
            None => {}
        }
        filtros.extend_from_slice(outros);
        if !filtros.is_empty() {
            let _ = write!(texto, " ({})", filtros.join(", "));
        }
        texto
    }
}

/// `3 cobranças · R$ 450,00 · pagas R$ 150,00`, from the amount and the
/// status of each charge.
fn totais<'a>(cobs: impl IntoIterator<Item = (Option<Decimal>, Option<&'a StatusCob>)>) -> String {
    let (mut quantas, mut total, mut pagas) = (0_usize, Decimal::ZERO, Decimal::ZERO);
    for (valor, status) in cobs {
        quantas += 1;
        let valor = valor.unwrap_or_default();
        total += valor;
        if status == Some(&StatusCob::Concluida) {
            pagas += valor;
        }
    }
    let quantas = match quantas {
        1 => "1 cobrança".to_owned(),
        n => format!("{n} cobranças"),
    };
    let mut texto = format!("{quantas} · {}", output::brl(total));
    if !pagas.is_zero() {
        let _ = write!(texto, " · pagas {}", output::brl(pagas));
    }
    texto
}

/// Where the page asked for with `--pagina` stands among the others, from
/// the parameters of the page and its number of items, `nome` in the
/// period (`cobranças`).
fn paginacao(numero: u32, parametros: &ParametrosConsulta, itens: usize, nome: &str) -> String {
    pagina(
        numero,
        parametros.paginacao.unwrap_or_default(),
        itens,
        nome,
    )
}

/// Where the page `numero` stands, from its `paginacao` and its number of
/// items, `nome` in the period.
pub(super) fn pagina(numero: u32, paginacao: Paginacao, itens: usize, nome: &str) -> String {
    let mut texto = format!("\n\nPágina {numero}");
    if let Some(total) = paginacao.quantidade_de_paginas {
        let _ = write!(texto, " de {} (a primeira é 0)", total.saturating_sub(1));
    }
    if let Some(total) = paginacao.quantidade_total_de_itens {
        let _ = write!(texto, "; {total} {nome} no período");
    }
    if paginacao.tem_mais(numero, itens) {
        let _ = write!(texto, "; a próxima é --pagina {}", numero + 1);
    }
    texto
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_documented_status_has_a_tone() {
        for status in StatusCob::DOCUMENTADOS {
            assert!(
                matches!(celula_status(Some(status)), Celula::Situacao(..)),
                "{status:?}"
            );
        }
        for status in StatusDevolucao::DOCUMENTADOS {
            assert!(
                matches!(celula_status_devolucao(Some(status)), Celula::Situacao(..)),
                "{status:?}"
            );
        }
        assert_eq!(
            celula_status(Some(&StatusCob::Concluida)),
            Celula::Situacao("concluída (paga)".into(), Tom::Positivo)
        );
        assert_eq!(
            celula_status(Some(&StatusCob::Ativa)),
            Celula::Situacao("ativa".into(), Tom::Pendente)
        );
        assert_eq!(
            celula_status_devolucao(Some(&StatusDevolucao::NaoRealizado)),
            Celula::Situacao("não realizada".into(), Tom::Negativo)
        );
        // What the API does not document goes as received, without a tone.
        assert_eq!(
            celula_status(Some(&StatusCob::Outro("NOVO".into()))),
            Celula::Texto("NOVO".into())
        );
        assert_eq!(celula_status(None), Celula::Vazia);
    }

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
    fn where_a_page_stands() {
        let parametros = |paginacao: serde_json::Value| -> ParametrosConsulta {
            serde_json::from_value(serde_json::json!({ "paginacao": paginacao })).unwrap()
        };
        let completa = parametros(serde_json::json!({
            "paginaAtual": 1, "itensPorPagina": 100, "quantidadeDePaginas": 3, "quantidadeTotalDeItens": 250
        }));
        assert_eq!(
            paginacao(1, &completa, 100, "cobranças"),
            "\n\nPágina 1 de 2 (a primeira é 0); 250 cobranças no período; a próxima é --pagina 2"
        );
        assert_eq!(
            paginacao(2, &completa, 50, "cobranças"),
            "\n\nPágina 2 de 2 (a primeira é 0); 250 cobranças no período"
        );
        // Without the number of pages, a full page may have a next one.
        let sem_total = parametros(serde_json::json!({"paginaAtual": 0, "itensPorPagina": 2}));
        assert_eq!(
            paginacao(0, &sem_total, 2, "cobranças"),
            "\n\nPágina 0; a próxima é --pagina 1"
        );
        assert_eq!(paginacao(0, &sem_total, 1, "Pix"), "\n\nPágina 0");
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
