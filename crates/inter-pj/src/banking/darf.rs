use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::documento::Documento;
use crate::serde_util::{api_enum, decimal_as_number, lenient, string_serde};

/// A DARF without a barcode (federal taxes), to pay with
/// [`Banking::pagar_darf`](super::Banking::pagar_darf) (`DarfRequest`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PagamentoDarf {
    /// CPF or CNPJ of the taxpayer.
    pub cnpj_cpf: Documento,
    /// Revenue code, 4 digits (e.g. `0220`).
    pub codigo_receita: String,
    /// Due date.
    pub data_vencimento: NaiveDate,
    /// Description, up to 1000 characters.
    pub descricao: String,
    /// Name of the taxpayer, up to 100 characters.
    pub nome_empresa: String,
    /// Phone of the taxpayer, up to 50 characters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub telefone_empresa: Option<String>,
    /// Tax period (*período de apuração*).
    pub periodo_apuracao: NaiveDate,
    /// Principal amount.
    #[serde(serialize_with = "decimal_as_number::serialize")]
    pub valor_principal: Decimal,
    /// Fine, if any.
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "decimal_as_number::serialize_option"
    )]
    pub valor_multa: Option<Decimal>,
    /// Interest, if any.
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "decimal_as_number::serialize_option"
    )]
    pub valor_juros: Option<Decimal>,
    /// Reference number, digits only, up to 30.
    pub referencia: String,
}

impl PagamentoDarf {
    /// Principal plus fine plus interest.
    pub fn valor_total(&self) -> Decimal {
        self.valor_principal
            + self.valor_multa.unwrap_or_default()
            + self.valor_juros.unwrap_or_default()
    }

    /// Checks what can be checked before sending, as
    /// [`Banking::pagar_darf`](super::Banking::pagar_darf) does.
    ///
    /// # Errors
    ///
    /// Returns the first problem found.
    pub fn validar(&self) -> Result<(), PagamentoDarfError> {
        use PagamentoDarfError as E;
        let digitos = |texto: &str| !texto.is_empty() && texto.bytes().all(|b| b.is_ascii_digit());
        let centavos = |valor: Decimal| valor.normalize().scale() <= 2;
        if self.codigo_receita.len() != 4 || !digitos(&self.codigo_receita) {
            return Err(E::CodigoReceita);
        }
        if self.referencia.len() > 30 || !digitos(&self.referencia) {
            return Err(E::Referencia);
        }
        for (texto, maximo, campo) in [
            (Some(&self.descricao), 1000, "a descrição"),
            (Some(&self.nome_empresa), 100, "o nome da empresa"),
            (self.telefone_empresa.as_ref(), 50, "o telefone"),
        ] {
            if let Some(texto) = texto
                && (texto.trim().is_empty() || texto.chars().count() > maximo)
            {
                return Err(E::Texto { campo, maximo });
            }
        }
        if self.valor_principal <= Decimal::ZERO || !centavos(self.valor_principal) {
            return Err(E::ValorPrincipal);
        }
        for valor in [self.valor_multa, self.valor_juros].into_iter().flatten() {
            if valor.is_sign_negative() || !centavos(valor) {
                return Err(E::MultaOuJuros);
            }
        }
        Ok(())
    }
}

/// Why a [`PagamentoDarf`] cannot be sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum PagamentoDarfError {
    /// The revenue code is not 4 digits.
    #[error("o código da receita tem 4 dígitos (ex.: 0220)")]
    CodigoReceita,
    /// The reference is not digits only, up to 30.
    #[error("a referência deve ter só dígitos, até 30")]
    Referencia,
    /// A text is empty or too long.
    #[error("{campo} deve ter de 1 a {maximo} caracteres")]
    Texto {
        /// Which text.
        campo: &'static str,
        /// Longest accepted.
        maximo: usize,
    },
    /// The principal amount is not positive or has fractions of a cent.
    #[error("o valor principal deve ser maior que zero, com até 2 casas decimais")]
    ValorPrincipal,
    /// The fine or the interest is negative or has fractions of a cent.
    #[error("multa e juros não podem ser negativos, com até 2 casas decimais")]
    MultaOuJuros,
}

/// Answer to [`Banking::pagar_darf`](super::Banking::pagar_darf) (`DarfResponse`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct SolicitacaoDarf {
    /// Approvals the payment needs in the Internet Banking.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::u64"
    )]
    pub quantidade_aprovadores: Option<u64>,
    /// Bank authentication of the payment.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub autenticacao: Option<String>,
    /// Day of the payment, as sent by the API (`DD/MM/AAAA`).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub data_pagamento: Option<String>,
    /// Outcome: paid, scheduled, or waiting for approval.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tipo_retorno: Option<TipoRetornoDarf>,
    /// Identifier of the request.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub codigo_solicitacao: Option<String>,
}

api_enum! {
    /// Outcome of a DARF payment request (`TipoRetornoEnum`).
    pub enum TipoRetornoDarf {
        /// `PAGAMENTO`: paid.
        Pagamento => "PAGAMENTO",
        /// `AGENDAMENTO`: scheduled.
        Agendamento => "AGENDAMENTO",
        /// `APROVACAO_PAGAMENTO`: payment waiting for approval.
        AprovacaoPagamento => "APROVACAO_PAGAMENTO",
        /// `APROVACAO_AGENDAMENTO`: scheduling waiting for approval.
        AprovacaoAgendamento => "APROVACAO_AGENDAMENTO",
    }
}

string_serde!(TipoRetornoDarf);

/// A DARF payment, as listed by [`Banking::darfs`](super::Banking::darfs)
/// (`InformacoesPagamentoDarf`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Darf {
    /// Identifier of the request.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub codigo_solicitacao: Option<String>,
    /// Kind of DARF (e.g. `PRETO`).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub tipo_darf: Option<String>,
    /// Principal amount.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::decimal",
        serialize_with = "decimal_as_number::serialize_option"
    )]
    pub valor: Option<Decimal>,
    /// Fine.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::decimal",
        serialize_with = "decimal_as_number::serialize_option"
    )]
    pub valor_multa: Option<Decimal>,
    /// Interest.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::decimal",
        serialize_with = "decimal_as_number::serialize_option"
    )]
    pub valor_juros: Option<Decimal>,
    /// Total paid.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::decimal",
        serialize_with = "decimal_as_number::serialize_option"
    )]
    pub valor_total: Option<Decimal>,
    /// Kind of payment, in the API's words.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub tipo: Option<String>,
    /// Tax period.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub periodo_apuracao: Option<String>,
    /// When it was (or will be) paid.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub data_pagamento: Option<String>,
    /// Reference number.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub referencia: Option<String>,
    /// Due date.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub data_vencimento: Option<String>,
    /// Revenue code.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub codigo_receita: Option<String>,
    /// Status (same values as payments by barcode).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_pagamento: Option<super::StatusPagamento>,
    /// When the payment was requested.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub data_inclusao: Option<String>,
    /// CPF or CNPJ of the taxpayer.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub cnpj_cpf: Option<String>,
    /// Approvals the payment needs.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::u64"
    )]
    pub aprovacoes_necessarias: Option<u64>,
    /// Approvals already given.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::u64"
    )]
    pub aprovacoes_realizadas: Option<u64>,
}

/// Filters of [`Banking::darfs`](super::Banking::darfs).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FiltroDarf {
    /// First and last day, both included.
    pub periodo: Option<(NaiveDate, NaiveDate)>,
    /// Only this revenue code.
    pub codigo_receita: Option<String>,
    /// Only this request.
    pub codigo_solicitacao: Option<String>,
}

impl FiltroDarf {
    pub(crate) fn query(&self) -> Vec<(&'static str, String)> {
        let mut query = Vec::new();
        if let Some((inicio, fim)) = self.periodo {
            query.push(("dataInicio", inicio.format("%Y-%m-%d").to_string()));
            query.push(("dataFim", fim.format("%Y-%m-%d").to_string()));
        }
        if let Some(codigo) = &self.codigo_receita {
            query.push(("codigoReceita", codigo.trim().to_owned()));
        }
        if let Some(codigo) = &self.codigo_solicitacao {
            query.push(("codigoSolicitacao", codigo.trim().to_ascii_lowercase()));
        }
        query
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn data(ano: i32, mes: u32, dia: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(ano, mes, dia).unwrap()
    }

    fn darf() -> PagamentoDarf {
        PagamentoDarf {
            cnpj_cpf: "12.345.678/0001-95".parse().unwrap(),
            codigo_receita: "0220".to_owned(),
            data_vencimento: data(2026, 10, 30),
            descricao: "IRPJ de setembro".to_owned(),
            nome_empresa: "Empresa Exemplo".to_owned(),
            telefone_empresa: None,
            periodo_apuracao: data(2026, 9, 30),
            valor_principal: "47.14".parse().unwrap(),
            valor_multa: Some("27.48".parse().unwrap()),
            valor_juros: Some("10.11".parse().unwrap()),
            referencia: "13609400849201739".to_owned(),
        }
    }

    #[test]
    fn serializes_the_documented_body() {
        assert_eq!(
            serde_json::to_value(darf()).unwrap(),
            json!({
                "cnpjCpf": "12345678000195",
                "codigoReceita": "0220",
                "dataVencimento": "2026-10-30",
                "descricao": "IRPJ de setembro",
                "nomeEmpresa": "Empresa Exemplo",
                "periodoApuracao": "2026-09-30",
                "valorPrincipal": 47.14,
                "valorMulta": 27.48,
                "valorJuros": 10.11,
                "referencia": "13609400849201739"
            })
        );
        assert_eq!(darf().valor_total(), "84.73".parse::<Decimal>().unwrap());
    }

    #[test]
    fn validates_codes_texts_and_amounts() {
        type Alteracao = fn(&mut PagamentoDarf);
        assert!(darf().validar().is_ok());
        let casos: [(Alteracao, PagamentoDarfError); 10] = [
            (
                |d| d.codigo_receita = "220".to_owned(),
                PagamentoDarfError::CodigoReceita,
            ),
            (
                |d| d.codigo_receita = "02A0".to_owned(),
                PagamentoDarfError::CodigoReceita,
            ),
            (
                |d| d.referencia = "12-3".to_owned(),
                PagamentoDarfError::Referencia,
            ),
            (
                |d| d.referencia = "1".repeat(31),
                PagamentoDarfError::Referencia,
            ),
            (
                |d| d.descricao = " ".to_owned(),
                PagamentoDarfError::Texto {
                    campo: "a descrição",
                    maximo: 1000,
                },
            ),
            (
                |d| d.nome_empresa = "x".repeat(101),
                PagamentoDarfError::Texto {
                    campo: "o nome da empresa",
                    maximo: 100,
                },
            ),
            (
                |d| d.telefone_empresa = Some("9".repeat(51)),
                PagamentoDarfError::Texto {
                    campo: "o telefone",
                    maximo: 50,
                },
            ),
            (
                |d| d.valor_principal = Decimal::ZERO,
                PagamentoDarfError::ValorPrincipal,
            ),
            (
                |d| d.valor_principal = "1.005".parse().unwrap(),
                PagamentoDarfError::ValorPrincipal,
            ),
            (
                |d| d.valor_juros = Some("-1".parse().unwrap()),
                PagamentoDarfError::MultaOuJuros,
            ),
        ];
        for (i, (alterar, esperado)) in casos.into_iter().enumerate() {
            let mut pagamento = darf();
            alterar(&mut pagamento);
            assert_eq!(pagamento.validar(), Err(esperado), "caso {i}");
        }
    }

    #[test]
    fn builds_the_query_of_the_filters() {
        let filtro = FiltroDarf {
            periodo: Some((data(2026, 9, 1), data(2026, 9, 30))),
            codigo_receita: Some(" 0220 ".to_owned()),
            codigo_solicitacao: Some("3414F226-36FB-4D87-811E-CFD99911D845".to_owned()),
        };
        assert_eq!(
            filtro.query(),
            [
                ("dataInicio", "2026-09-01".to_owned()),
                ("dataFim", "2026-09-30".to_owned()),
                ("codigoReceita", "0220".to_owned()),
                (
                    "codigoSolicitacao",
                    "3414f226-36fb-4d87-811e-cfd99911d845".to_owned()
                ),
            ]
        );
    }
}
