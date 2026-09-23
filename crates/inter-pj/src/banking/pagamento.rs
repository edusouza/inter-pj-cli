use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize, Serializer};

use crate::boleto::CodigoBarras;
use crate::documento::Documento;
use crate::serde_util::{api_enum, decimal_as_number, lenient, string_serde};

/// A boleto, utility bill or tax with a barcode, to pay with
/// [`Banking::pagar_boleto`](super::Banking::pagar_boleto)
/// (`EfetuarPagamento`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PagamentoBoleto {
    /// What to pay; sent as the 44-digit barcode.
    #[serde(rename = "codBarraLinhaDigitavel", serialize_with = "barras")]
    pub codigo: CodigoBarras,
    /// Amount to pay: positive, at most two decimal places. It may differ
    /// from the amount in the code (interest, fines, discounts).
    #[serde(serialize_with = "valor_texto")]
    pub valor_pagar: Decimal,
    /// Day to pay on; today when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_pagamento: Option<NaiveDate>,
    /// Due date of the document.
    pub data_vencimento: NaiveDate,
    /// CPF or CNPJ of the beneficiary; when given, the API checks it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpf_cnpj_beneficiario: Option<Documento>,
}

impl PagamentoBoleto {
    /// A payment of `valor_pagar`, made today, without checking the
    /// beneficiary.
    pub fn new(codigo: CodigoBarras, valor_pagar: Decimal, data_vencimento: NaiveDate) -> Self {
        Self {
            codigo,
            valor_pagar,
            data_pagamento: None,
            data_vencimento,
            cpf_cnpj_beneficiario: None,
        }
    }

    /// Checks the amount, as [`Banking::pagar_boleto`](super::Banking::pagar_boleto)
    /// does before sending (the code was checked when parsed).
    ///
    /// # Errors
    ///
    /// Fails for amounts not positive or with fractions of a cent.
    pub fn validar(&self) -> Result<(), PagamentoBoletoError> {
        if self.valor_pagar <= Decimal::ZERO {
            return Err(PagamentoBoletoError::ValorNaoPositivo);
        }
        if self.valor_pagar.normalize().scale() > 2 {
            return Err(PagamentoBoletoError::CasasDecimais);
        }
        Ok(())
    }
}

fn barras<S: Serializer>(codigo: &CodigoBarras, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(codigo.codigo_barras())
}

/// The API documents `valorPagar` as text: `"26.80"`.
fn valor_texto<S: Serializer>(valor: &Decimal, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&format!("{:.2}", valor.round_dp(2)))
}

/// Why a [`PagamentoBoleto`] cannot be sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum PagamentoBoletoError {
    /// The amount is zero or negative.
    #[error("o valor a pagar deve ser maior que zero")]
    ValorNaoPositivo,
    /// The amount has fractions of a cent.
    #[error("o valor a pagar deve ter no máximo 2 casas decimais")]
    CasasDecimais,
}

/// Answer to [`Banking::pagar_boleto`](super::Banking::pagar_boleto)
/// (`EfetuarPagamentoResponse`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct SolicitacaoPagamento {
    /// Approvals the payment needs in the Internet Banking.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::u64"
    )]
    pub quantidade_aprovadores: Option<u64>,
    /// When the payment is scheduled for, as sent by the API.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub data_agendamento: Option<String>,
    /// Status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_pagamento: Option<StatusPagamento>,
    /// Identifier of the payment, to find or cancel it later.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub codigo_transacao: Option<String>,
}

api_enum! {
    /// Status of a payment by barcode (`SituacaoPagamento`).
    pub enum StatusPagamento {
        /// `EMPROCESSAMENTO`: being processed.
        EmProcessamento => "EMPROCESSAMENTO",
        /// `AGUARDANDO_APROVACAO`: waiting for approval in the Internet Banking.
        AguardandoAprovacao => "AGUARDANDO_APROVACAO",
        /// `APROVADO`: approved.
        Aprovado => "APROVADO",
        /// `REPROVADO`: approval denied.
        Reprovado => "REPROVADO",
        /// `APROVACAO_EXPIRADA`: not approved in time.
        AprovacaoExpirada => "APROVACAO_EXPIRADA",
        /// `AGENDADO`: scheduled.
        Agendado => "AGENDADO",
        /// `AGENDADO_CANCELADO`: scheduled, then cancelled.
        AgendadoCancelado => "AGENDADO_CANCELADO",
        /// `CANCELADO`: cancelled.
        Cancelado => "CANCELADO",
        /// `REALIZADO`: paid.
        Realizado => "REALIZADO",
        /// `ERRO`: failed.
        Erro => "ERRO",
        /// `NAO_COMPENSADO`: not cleared.
        NaoCompensado => "NAO_COMPENSADO",
    }
}

string_serde!(StatusPagamento);

/// A payment by barcode, as listed by
/// [`Banking::pagamentos`](super::Banking::pagamentos)
/// (`InformacoesPagamento`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Pagamento {
    /// Identifier of the payment.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub codigo_transacao: Option<String>,
    /// Barcode paid.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub codigo_barra: Option<String>,
    /// Kind of payment, in the API's words.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub tipo: Option<String>,
    /// Due date informed when paying.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub data_vencimento_digitada: Option<String>,
    /// Due date of the document.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub data_vencimento_titulo: Option<String>,
    /// When the payment was requested.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub data_inclusao: Option<String>,
    /// When it was (or will be) paid.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub data_pagamento: Option<String>,
    /// Amount paid.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::decimal",
        serialize_with = "decimal_as_number::serialize_option"
    )]
    pub valor_pago: Option<Decimal>,
    /// Face value of the document.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::decimal",
        serialize_with = "decimal_as_number::serialize_option"
    )]
    pub valor_nominal: Option<Decimal>,
    /// Status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_pagamento: Option<StatusPagamento>,
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
    /// CPF or CNPJ of the beneficiary.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub cpf_cnpj_beneficiario: Option<String>,
    /// Name of the beneficiary.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub nome_beneficiario: Option<String>,
    /// Bank authentication of the payment.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub autenticacao: Option<String>,
    /// Unique sequential number, internal to Inter.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub nsu: Option<String>,
}

/// Which date the period of [`FiltroPagamentos`] refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DataDoPagamento {
    /// `INCLUSAO`: when the payment was requested (the API's default).
    Inclusao,
    /// `PAGAMENTO`: when it was paid.
    Pagamento,
    /// `VENCIMENTO`: the due date.
    Vencimento,
}

impl DataDoPagamento {
    /// Every value the API accepts.
    pub const TODAS: [Self; 3] = [Self::Inclusao, Self::Pagamento, Self::Vencimento];

    /// Code used by the API.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Inclusao => "INCLUSAO",
            Self::Pagamento => "PAGAMENTO",
            Self::Vencimento => "VENCIMENTO",
        }
    }
}

/// Filters of [`Banking::pagamentos`](super::Banking::pagamentos). Without a
/// period, the API returns the last 30 days.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FiltroPagamentos {
    /// First and last day, both included.
    pub periodo: Option<(NaiveDate, NaiveDate)>,
    /// Which date the period refers to (the API's default: inclusion).
    pub filtrar_por: Option<DataDoPagamento>,
    /// Only payments of this code.
    pub codigo: Option<CodigoBarras>,
    /// Only this payment.
    pub codigo_transacao: Option<String>,
}

impl FiltroPagamentos {
    pub(crate) fn query(&self) -> Vec<(&'static str, String)> {
        let mut query = Vec::new();
        if let Some((inicio, fim)) = self.periodo {
            query.push(("dataInicio", inicio.format("%Y-%m-%d").to_string()));
            query.push(("dataFim", fim.format("%Y-%m-%d").to_string()));
        }
        if let Some(filtrar_por) = self.filtrar_por {
            query.push(("filtrarDataPor", filtrar_por.as_str().to_owned()));
        }
        if let Some(codigo) = &self.codigo {
            query.push(("codBarraLinhaDigitavel", codigo.codigo_barras().to_owned()));
        }
        if let Some(codigo) = &self.codigo_transacao {
            query.push(("codigoTransacao", codigo.trim().to_ascii_lowercase()));
        }
        query
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const LINHA: &str = "07797777051167847115990071126347192950000003010";

    fn data(ano: i32, mes: u32, dia: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(ano, mes, dia).unwrap()
    }

    #[test]
    fn serializes_the_documented_body() {
        let mut pagamento = PagamentoBoleto::new(
            LINHA.parse().unwrap(),
            "30.1".parse().unwrap(),
            data(2026, 10, 10),
        );
        assert_eq!(
            serde_json::to_value(&pagamento).unwrap(),
            json!({
                "codBarraLinhaDigitavel": "07791929500000030107777011678471159007112634",
                "valorPagar": "30.10",
                "dataVencimento": "2026-10-10"
            })
        );
        pagamento.data_pagamento = Some(data(2026, 10, 9));
        pagamento.cpf_cnpj_beneficiario = Some("12.345.678/0001-95".parse().unwrap());
        let body = serde_json::to_value(&pagamento).unwrap();
        assert_eq!(body["dataPagamento"], "2026-10-09");
        assert_eq!(body["cpfCnpjBeneficiario"], "12345678000195");
    }

    #[test]
    fn validates_the_amount() {
        let pagamento = |valor: &str| {
            PagamentoBoleto::new(
                LINHA.parse().unwrap(),
                valor.parse().unwrap(),
                data(2026, 1, 1),
            )
        };
        assert!(pagamento("0.01").validar().is_ok());
        assert!(pagamento("10.500").validar().is_ok());
        assert_eq!(
            pagamento("0").validar(),
            Err(PagamentoBoletoError::ValorNaoPositivo)
        );
        assert_eq!(
            pagamento("1.001").validar(),
            Err(PagamentoBoletoError::CasasDecimais)
        );
    }

    #[test]
    fn builds_the_query_of_the_filters() {
        assert!(FiltroPagamentos::default().query().is_empty());
        let filtro = FiltroPagamentos {
            periodo: Some((data(2026, 9, 1), data(2026, 9, 30))),
            filtrar_por: Some(DataDoPagamento::Vencimento),
            codigo: Some(LINHA.parse().unwrap()),
            codigo_transacao: Some(" 3414F226-36FB-4D87-811E-CFD99911D845 ".to_owned()),
        };
        assert_eq!(
            filtro.query(),
            [
                ("dataInicio", "2026-09-01".to_owned()),
                ("dataFim", "2026-09-30".to_owned()),
                ("filtrarDataPor", "VENCIMENTO".to_owned()),
                (
                    "codBarraLinhaDigitavel",
                    "07791929500000030107777011678471159007112634".to_owned()
                ),
                (
                    "codigoTransacao",
                    "3414f226-36fb-4d87-811e-cfd99911d845".to_owned()
                ),
            ]
        );
    }

    #[test]
    fn parses_payments_leniently() {
        let pagamento: Pagamento = serde_json::from_value(json!({
            "codigoTransacao": "3414f226-36fb-4d87-811e-cfd99911d845",
            "codigoBarra": "07791929500000030107777011678471159007112634",
            "valorPago": 30.1,
            "valorNominal": "30.10",
            "statusPagamento": "AGENDADO",
            "aprovacoesNecessarias": 2,
            "autenticacao": 123_456,
            "novoCampo": true
        }))
        .unwrap();
        assert_eq!(pagamento.status_pagamento, Some(StatusPagamento::Agendado));
        assert_eq!(pagamento.valor_nominal, Some("30.10".parse().unwrap()));
        assert_eq!(pagamento.aprovacoes_necessarias, Some(2));
        assert_eq!(pagamento.autenticacao.as_deref(), Some("123456"));

        let resposta: SolicitacaoPagamento = serde_json::from_value(json!({
            "quantidadeAprovadores": 1,
            "statusPagamento": "AGUARDANDO_APROVACAO",
            "codigoTransacao": "8bbdede4-35db-4ec9-b652-e176841e62c8"
        }))
        .unwrap();
        assert_eq!(
            resposta.status_pagamento,
            Some(StatusPagamento::AguardandoAprovacao)
        );
    }
}
