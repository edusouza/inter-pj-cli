//! Fine, interest, rebate and discount of the charges with a due date: from
//! the options, and in words.

use chrono::NaiveDate;
use inter_pj::pix::{
    AbatimentoCobv, DescontoCobv, DescontoData, EncargoCobv, JurosCobv, ModalidadeJuros, MultaCobv,
    ValorCobvGerado,
};
use rust_decimal::Decimal;

use crate::cli::{DescontoAte, EncargosCobvArgs, PeriodoJuros, TaxaOuValor};
use crate::error::CliError;
use crate::output::{self, data_br, percentual};

/// The charges the options ask for.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Encargos {
    pub(super) multa: Option<MultaCobv>,
    pub(super) juros: Option<JurosCobv>,
    pub(super) abatimento: Option<AbatimentoCobv>,
    pub(super) desconto: Option<DescontoCobv>,
}

impl Encargos {
    /// The charges of the options; a discount without a date lasts until
    /// `vencimento`.
    pub(super) fn de(args: &EncargosCobvArgs, vencimento: NaiveDate) -> Result<Self, CliError> {
        Ok(Self {
            multa: args.multa.map(|multa| match multa {
                TaxaOuValor::Taxa(taxa) => MultaCobv::Percentual(taxa),
                TaxaOuValor::Valor(valor) => MultaCobv::ValorFixo(valor),
            }),
            juros: args
                .juros
                .map(|juros| juros_de(juros, args.juros_periodo, args.dias_uteis))
                .transpose()?,
            abatimento: args.abatimento.map(|abatimento| match abatimento {
                TaxaOuValor::Taxa(taxa) => AbatimentoCobv::Percentual(taxa),
                TaxaOuValor::Valor(valor) => AbatimentoCobv::ValorFixo(valor),
            }),
            desconto: desconto_de(
                &args.desconto,
                args.desconto_por_dia,
                args.dias_uteis,
                vencimento,
            )?,
        })
    }

    pub(super) fn vazio(&self) -> bool {
        *self == Self::default()
    }
}

/// The fine, interest, rebate and discount given, in words.
pub(super) fn linhas(
    multa: Option<&MultaCobv>,
    juros: Option<&JurosCobv>,
    abatimento: Option<&AbatimentoCobv>,
    desconto: Option<&DescontoCobv>,
) -> Vec<(&'static str, String)> {
    let mut linhas = Vec::new();
    if let Some(multa) = multa {
        linhas.push(("Multa", descrever_multa(multa)));
    }
    if let Some(juros) = juros {
        linhas.push(("Juros", descrever_juros(juros)));
    }
    if let Some(abatimento) = abatimento {
        linhas.push(("Abatimento", descrever_abatimento(abatimento)));
    }
    if let Some(desconto) = desconto {
        linhas.push(("Desconto", descrever_desconto(desconto)));
    }
    linhas
}

/// `--juros`, `--juros-periodo` and `--dias-uteis` as a modality.
fn juros_de(
    juros: TaxaOuValor,
    periodo: Option<PeriodoJuros>,
    dias_uteis: bool,
) -> Result<JurosCobv, CliError> {
    use ModalidadeJuros as M;
    Ok(match juros {
        TaxaOuValor::Valor(valor) => {
            if matches!(periodo, Some(PeriodoJuros::Mes | PeriodoJuros::Ano)) {
                return Err(CliError::Usage(
                    "--juros-periodo: juros em valor são por dia; ao mês ou ao ano, use um percentual (1%)"
                        .to_owned(),
                ));
            }
            let modalidade = if dias_uteis {
                M::ValorDiasUteis
            } else {
                M::ValorDiasCorridos
            };
            JurosCobv::new(modalidade, valor)
        }
        TaxaOuValor::Taxa(taxa) => {
            let modalidade = match (periodo.unwrap_or(PeriodoJuros::Mes), dias_uteis) {
                (PeriodoJuros::Dia, false) => M::PercentualDiaDiasCorridos,
                (PeriodoJuros::Mes, false) => M::PercentualMesDiasCorridos,
                (PeriodoJuros::Ano, false) => M::PercentualAnoDiasCorridos,
                (PeriodoJuros::Dia, true) => M::PercentualDiaDiasUteis,
                (PeriodoJuros::Mes, true) => M::PercentualMesDiasUteis,
                (PeriodoJuros::Ano, true) => M::PercentualAnoDiasUteis,
            };
            JurosCobv::new(modalidade, taxa)
        }
    })
}

/// `--desconto` (until dates) or `--desconto-por-dia` as a modality.
fn desconto_de(
    ate_datas: &[DescontoAte],
    por_dia: Option<TaxaOuValor>,
    dias_uteis: bool,
    vencimento: NaiveDate,
) -> Result<Option<DescontoCobv>, CliError> {
    if let Some(por_dia) = por_dia {
        return Ok(Some(match (por_dia, dias_uteis) {
            (TaxaOuValor::Valor(valor), false) => DescontoCobv::ValorPorDiaCorrido(valor),
            (TaxaOuValor::Valor(valor), true) => DescontoCobv::ValorPorDiaUtil(valor),
            (TaxaOuValor::Taxa(taxa), false) => DescontoCobv::PercentualPorDiaCorrido(taxa),
            (TaxaOuValor::Taxa(taxa), true) => DescontoCobv::PercentualPorDiaUtil(taxa),
        }));
    }
    let Some(primeiro) = ate_datas.first() else {
        return Ok(None);
    };
    let percentual = matches!(primeiro.valor, TaxaOuValor::Taxa(_));
    let mut datas = Vec::with_capacity(ate_datas.len());
    for desconto in ate_datas {
        let ((TaxaOuValor::Taxa(valor), true) | (TaxaOuValor::Valor(valor), false)) =
            (desconto.valor, percentual)
        else {
            return Err(CliError::Usage(
                "--desconto: use só percentuais (2%) ou só valores (10,00) nas datas".to_owned(),
            ));
        };
        datas.push(DescontoData::new(desconto.ate.unwrap_or(vencimento), valor));
    }
    Ok(Some(if percentual {
        DescontoCobv::PercentualAteDatas(datas)
    } else {
        DescontoCobv::ValorFixoAteDatas(datas)
    }))
}

/// `2%` or `R$ 4,00`.
fn quanto(e_percentual: bool, valor: Decimal) -> String {
    if e_percentual {
        percentual(valor)
    } else {
        output::brl(valor)
    }
}

fn descrever_multa(multa: &MultaCobv) -> String {
    match multa {
        MultaCobv::Percentual(taxa) => percentual(*taxa),
        MultaCobv::ValorFixo(valor) => output::brl(*valor),
        outra => format!("{outra:?}"),
    }
}

/// `1% ao mês (dias corridos)`, `R$ 0,50 por dia (dias úteis)`.
fn descrever_juros(juros: &JurosCobv) -> String {
    use ModalidadeJuros as M;
    let (periodo, dias) = match juros.modalidade {
        M::ValorDiasCorridos => ("por dia", "dias corridos"),
        M::PercentualDiaDiasCorridos => ("ao dia", "dias corridos"),
        M::PercentualMesDiasCorridos => ("ao mês", "dias corridos"),
        M::PercentualAnoDiasCorridos => ("ao ano", "dias corridos"),
        M::ValorDiasUteis => ("por dia", "dias úteis"),
        M::PercentualDiaDiasUteis => ("ao dia", "dias úteis"),
        M::PercentualMesDiasUteis => ("ao mês", "dias úteis"),
        M::PercentualAnoDiasUteis => ("ao ano", "dias úteis"),
        outra => return format!("{} (modalidade {})", juros.valor_perc, outra.codigo()),
    };
    format!(
        "{} {periodo} ({dias})",
        quanto(juros.modalidade.percentual(), juros.valor_perc)
    )
}

fn descrever_abatimento(abatimento: &AbatimentoCobv) -> String {
    match abatimento {
        AbatimentoCobv::Percentual(taxa) => percentual(*taxa),
        AbatimentoCobv::ValorFixo(valor) => output::brl(*valor),
        outro => format!("{outro:?}"),
    }
}

/// `R$ 10,00 até 15/10/2026; R$ 5,00 até 18/10/2026`, `0,5% por dia de
/// antecipação (dias corridos)`.
fn descrever_desconto(desconto: &DescontoCobv) -> String {
    let ate = |datas: &[DescontoData], e_percentual: bool| {
        datas
            .iter()
            .map(|data| {
                format!(
                    "{} até {}",
                    quanto(e_percentual, data.valor_perc),
                    data.data.format("%d/%m/%Y")
                )
            })
            .collect::<Vec<_>>()
            .join("; ")
    };
    let por_dia = |valor: Decimal, e_percentual: bool, dias: &str| {
        format!(
            "{} por dia de antecipação ({dias})",
            quanto(e_percentual, valor)
        )
    };
    match desconto {
        DescontoCobv::ValorFixoAteDatas(datas) => ate(datas, false),
        DescontoCobv::PercentualAteDatas(datas) => ate(datas, true),
        DescontoCobv::ValorPorDiaCorrido(valor) => por_dia(*valor, false, "dias corridos"),
        DescontoCobv::ValorPorDiaUtil(valor) => por_dia(*valor, false, "dias úteis"),
        DescontoCobv::PercentualPorDiaCorrido(taxa) => por_dia(*taxa, true, "dias corridos"),
        DescontoCobv::PercentualPorDiaUtil(taxa) => por_dia(*taxa, true, "dias úteis"),
        outro => format!("{outro:?}"),
    }
}

/// The charges of an answer, in words; a modality the CLI does not know
/// shows its code.
pub(super) fn linhas_geradas(valor: &ValorCobvGerado) -> Vec<(&'static str, String)> {
    let mut linhas = Vec::new();
    let valor_fixo_ou_percentual = |encargo: &EncargoCobv| {
        let valor = encargo.valor_perc?;
        Some(match encargo.modalidade {
            Some(1) => output::brl(valor),
            Some(2) => percentual(valor),
            outra => desconhecida(outra, valor),
        })
    };
    if let Some(multa) = valor.multa.as_ref().and_then(valor_fixo_ou_percentual) {
        linhas.push(("Multa", multa));
    }
    if let Some(juros) = &valor.juros
        && let Some(taxa) = juros.valor_perc
    {
        let texto = match juros.modalidade.and_then(ModalidadeJuros::de_codigo) {
            Some(modalidade) => descrever_juros(&JurosCobv::new(modalidade, taxa)),
            None => desconhecida(juros.modalidade, taxa),
        };
        linhas.push(("Juros", texto));
    }
    if let Some(abatimento) = valor.abatimento.as_ref().and_then(valor_fixo_ou_percentual) {
        linhas.push(("Abatimento", abatimento));
    }
    if let Some(desconto) = valor.desconto.as_ref().and_then(desconto_gerado) {
        linhas.push(("Desconto", desconto));
    }
    linhas
}

fn desconto_gerado(desconto: &EncargoCobv) -> Option<String> {
    let dias = |modalidade| match modalidade {
        3 | 5 => "dias corridos",
        _ => "dias úteis",
    };
    match desconto.modalidade {
        Some(modalidade @ (1 | 2)) => {
            let datas: Vec<String> = desconto
                .desconto_data_fixa
                .iter()
                .filter_map(|data| {
                    let valor = quanto(modalidade == 2, data.valor_perc?);
                    let ate = data.data.as_deref().map(data_br).unwrap_or_default();
                    Some(format!("{valor} até {ate}"))
                })
                .collect();
            (!datas.is_empty()).then(|| datas.join("; "))
        }
        Some(modalidade @ 3..=6) => Some(format!(
            "{} por dia de antecipação ({})",
            quanto(modalidade >= 5, desconto.valor_perc?),
            dias(modalidade)
        )),
        outra => Some(desconhecida(outra, desconto.valor_perc?)),
    }
}

/// `1.50 (modalidade 9)`.
fn desconhecida(modalidade: Option<u64>, valor: Decimal) -> String {
    match modalidade {
        Some(modalidade) => format!("{valor} (modalidade {modalidade})"),
        None => valor.to_string(),
    }
}

/// The last day of the fixed-date discounts, if any.
pub(super) fn datas_do_desconto(desconto: Option<&DescontoCobv>) -> Vec<NaiveDate> {
    match desconto {
        Some(DescontoCobv::ValorFixoAteDatas(datas) | DescontoCobv::PercentualAteDatas(datas)) => {
            datas.iter().map(|data| data.data).collect()
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;
    use serde_json::json;

    use super::*;
    use crate::cli::{Cli, Command, PixCobvCommand, PixCommand};

    fn dia(texto: &str) -> NaiveDate {
        texto.parse().unwrap()
    }

    fn encargos(opcoes: &[&str]) -> Result<Encargos, CliError> {
        let mut todos = vec![
            "inter-pj",
            "pix",
            "cobv",
            "revisar",
            "7978c0c97ea847e78e8849634473c1f1",
        ];
        todos.extend_from_slice(opcoes);
        let args = match Cli::try_parse_from(todos).unwrap().command {
            Command::Pix(PixCommand::Cobv(PixCobvCommand::Revisar(args))) => args,
            outro => panic!("{outro:?}"),
        };
        Encargos::de(&args.encargos, dia("2026-10-20"))
    }

    fn em_palavras(opcoes: &[&str]) -> Vec<(&'static str, String)> {
        let encargos = encargos(opcoes).unwrap();
        linhas(
            encargos.multa.as_ref(),
            encargos.juros.as_ref(),
            encargos.abatimento.as_ref(),
            encargos.desconto.as_ref(),
        )
    }

    #[test]
    fn options_become_the_modalities() {
        let encargos = encargos(&[
            "--multa",
            "2%",
            "--juros",
            "0,033%",
            "--juros-periodo",
            "dia",
            "--dias-uteis",
            "--abatimento",
            "5,00",
            "--desconto",
            "10,00@2026-10-15",
            "--desconto",
            "5,00",
        ])
        .unwrap();
        assert_eq!(
            serde_json::to_value(encargos.multa).unwrap(),
            json!({"modalidade": 2, "valorPerc": "2.00"})
        );
        assert_eq!(
            encargos.juros,
            Some(JurosCobv::new(
                ModalidadeJuros::PercentualDiaDiasUteis,
                "0.033".parse().unwrap()
            ))
        );
        assert_eq!(
            serde_json::to_value(&encargos.desconto).unwrap(),
            json!({"modalidade": 1, "descontoDataFixa": [
                {"data": "2026-10-15", "valorPerc": "10.00"},
                {"data": "2026-10-20", "valorPerc": "5.00"}
            ]})
        );
        assert_eq!(
            encargos.abatimento,
            Some(AbatimentoCobv::ValorFixo(Decimal::new(5, 0)))
        );
    }

    #[test]
    fn every_modality_in_words() {
        assert_eq!(
            em_palavras(&[
                "--multa",
                "4,00",
                "--juros",
                "1%",
                "--desconto",
                "2%@2026-10-10"
            ]),
            [
                ("Multa", "R$ 4,00".to_owned()),
                ("Juros", "1% ao mês (dias corridos)".to_owned()),
                ("Desconto", "2% até 10/10/2026".to_owned()),
            ]
        );
        assert_eq!(
            em_palavras(&["--juros", "0,50", "--dias-uteis", "--abatimento", "5%"]),
            [
                ("Juros", "R$ 0,50 por dia (dias úteis)".to_owned()),
                ("Abatimento", "5%".to_owned()),
            ]
        );
        assert_eq!(
            em_palavras(&[
                "--juros",
                "12%",
                "--juros-periodo",
                "ano",
                "--desconto-por-dia",
                "0,10"
            ]),
            [
                ("Juros", "12% ao ano (dias corridos)".to_owned()),
                (
                    "Desconto",
                    "R$ 0,10 por dia de antecipação (dias corridos)".to_owned()
                ),
            ]
        );
        assert_eq!(
            em_palavras(&["--desconto-por-dia", "0,5%", "--dias-uteis"]),
            [(
                "Desconto",
                "0,5% por dia de antecipação (dias úteis)".to_owned()
            )]
        );
    }

    #[test]
    fn mixed_or_impossible_options_are_refused() {
        let erro = encargos(&["--desconto", "2%", "--desconto", "10,00@2026-10-15"]).unwrap_err();
        assert!(erro.to_string().contains("só percentuais"), "{erro}");
        let erro = encargos(&["--juros", "0,50", "--juros-periodo", "mes"]).unwrap_err();
        assert!(erro.to_string().starts_with("--juros-periodo: "), "{erro}");
        let revisar = |opcoes: &[&str]| {
            let mut todos = vec![
                "inter-pj",
                "pix",
                "cobv",
                "revisar",
                "7978c0c97ea847e78e8849634473c1f1",
            ];
            todos.extend_from_slice(opcoes);
            Cli::try_parse_from(todos).map(|_| ())
        };
        // --dias-uteis only counts days of interest or of a per-day discount.
        assert!(revisar(&["--multa", "2%", "--dias-uteis"]).is_err());
        assert!(revisar(&["--desconto", "2%", "--desconto-por-dia", "1%"]).is_err());
        assert!(revisar(&["--desconto", "2%@20/10/2026"]).is_err());
        assert!(revisar(&["--juros-periodo", "dia"]).is_err());
        assert!(revisar(&["--juros", "1%", "--dias-uteis"]).is_ok());
    }

    #[test]
    fn answers_in_words() {
        let valor: ValorCobvGerado = serde_json::from_value(json!({
            "original": "150.00",
            "multa": {"modalidade": "2", "valorPerc": "15.00"},
            "juros": {"modalidade": "6", "valorPerc": "0.03"},
            "abatimento": {"modalidade": 1, "valorPerc": "5.00"},
            "desconto": {"modalidade": "1", "descontoDataFixa": [
                {"data": "2026-10-15", "valorPerc": "30.00"},
                {"data": "2026-10-18", "valorPerc": "15.00"}
            ]}
        }))
        .unwrap();
        assert_eq!(
            linhas_geradas(&valor),
            [
                ("Multa", "15%".to_owned()),
                ("Juros", "0,03% ao dia (dias úteis)".to_owned()),
                ("Abatimento", "R$ 5,00".to_owned()),
                (
                    "Desconto",
                    "R$ 30,00 até 15/10/2026; R$ 15,00 até 18/10/2026".to_owned()
                ),
            ]
        );
        let valor: ValorCobvGerado = serde_json::from_value(json!({
            "multa": {"modalidade": 7, "valorPerc": "1.00"},
            "juros": {"modalidade": 9, "valorPerc": "1.00"},
            "desconto": {"modalidade": 4, "valorPerc": "0.20"}
        }))
        .unwrap();
        assert_eq!(
            linhas_geradas(&valor),
            [
                ("Multa", "1.00 (modalidade 7)".to_owned()),
                ("Juros", "1.00 (modalidade 9)".to_owned()),
                (
                    "Desconto",
                    "R$ 0,20 por dia de antecipação (dias úteis)".to_owned()
                ),
            ]
        );
    }
}
