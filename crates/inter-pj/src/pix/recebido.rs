//! Pix received by the account and their refunds.

use std::fmt::{self, Write as _};
use std::str::FromStr;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;

use super::cob::ParametrosConsulta;
use super::comum::{CobrancaPixError, PeriodoPix, texto, valor};
use super::txid::Txid;
use crate::documento::Documento;
use crate::serde_util::{api_enum, decimal_texto, lenient, string_serde};

/// Longest [`IdDevolucao`].
pub const ID_DEVOLUCAO_MAXIMO: usize = 35;

/// Longest [`DevolucaoSolicitada::descricao`].
pub const MAX_DESCRICAO_DEVOLUCAO: usize = 140;

/// Identifier of a refund (`id`): 1 to 35 letters and digits, chosen by the
/// receiver. The API does not refund twice with the same id, so a refund
/// whose answer was lost can be repeated with it.
///
/// ```
/// use inter_pj::pix::IdDevolucao;
///
/// let id: IdDevolucao = "D123".parse().unwrap();
/// assert_eq!(id.as_str(), "D123");
/// assert!("D-123".parse::<IdDevolucao>().is_err());
/// assert_eq!(IdDevolucao::novo().as_str().len(), 32);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IdDevolucao(String);

impl IdDevolucao {
    /// A new random id: 32 hexadecimal digits in lower case.
    ///
    /// # Panics
    ///
    /// Never in practice: AWS-LC aborts the process itself when the operating
    /// system cannot provide random bytes.
    pub fn novo() -> Self {
        let mut bytes = [0u8; 16];
        aws_lc_rs::rand::fill(&mut bytes)
            .expect("o sistema não forneceu bytes aleatórios para o id da devolução");
        let mut id = String::with_capacity(32);
        for byte in bytes {
            let _ = write!(id, "{byte:02x}");
        }
        Self(id)
    }

    /// The id as sent.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for IdDevolucao {
    type Err = IdDevolucaoError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let id = raw.trim();
        if (1..=ID_DEVOLUCAO_MAXIMO).contains(&id.len())
            && id.bytes().all(|b| b.is_ascii_alphanumeric())
        {
            Ok(Self(id.to_owned()))
        } else {
            Err(IdDevolucaoError)
        }
    }
}

impl fmt::Display for IdDevolucao {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A text that is not an [`IdDevolucao`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("id da devolução inválido: use de 1 a 35 letras e dígitos, sem espaços nem símbolos")]
pub struct IdDevolucaoError;

/// A refund to request (`DevolucaoSolicitada`), with
/// [`Pix::devolver`](super::Pix::devolver). The refunds of a Pix cannot
/// add up to more than it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct DevolucaoSolicitada {
    /// Amount to refund.
    #[serde(serialize_with = "decimal_texto::serialize")]
    pub valor: Decimal,
    /// Which part of the Pix is refunded; the API's default is the original
    /// amount.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub natureza: Option<NaturezaDevolucao>,
    /// Message to the payer, up to [`MAX_DESCRICAO_DEVOLUCAO`] characters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub descricao: Option<String>,
}

impl DevolucaoSolicitada {
    /// A refund of `valor`.
    pub fn new(valor: Decimal) -> Self {
        Self {
            valor,
            natureza: None,
            descricao: None,
        }
    }

    /// Checks what can be checked before sending.
    ///
    /// # Errors
    ///
    /// The first field the API would refuse.
    pub fn validar(&self) -> Result<(), CobrancaPixError> {
        valor(self.valor, "valor", false)?;
        if let Some(descricao) = &self.descricao {
            texto(descricao, "descricao", MAX_DESCRICAO_DEVOLUCAO)?;
        }
        Ok(())
    }
}

/// Which part of a Pix a refund is about (`DevolucaoSolicitadaNatureza`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum NaturezaDevolucao {
    /// `ORIGINAL`: a common Pix, or the purchase of a Pix Troco.
    Original,
    /// `RETIRADA`: the cash of a Pix Saque, or the change of a Pix Troco.
    Retirada,
}

impl NaturezaDevolucao {
    /// Every nature.
    pub const TODAS: [Self; 2] = [Self::Original, Self::Retirada];

    /// Code used by the API.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Original => "ORIGINAL",
            Self::Retirada => "RETIRADA",
        }
    }
}

impl Serialize for NaturezaDevolucao {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// Filters of [`Pix::listar_pix_recebidos`](super::Pix::listar_pix_recebidos):
/// the period in which the Pix were processed and, optionally, the charge,
/// whether there is one, whether it was refunded and the payer.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct FiltroPixRecebidos {
    /// Period.
    pub periodo: PeriodoPix,
    /// Only the Pix of this charge.
    pub txid: Option<Txid>,
    /// Only Pix of (`true`) or without (`false`) a charge.
    pub tx_id_presente: Option<bool>,
    /// Only Pix with (`true`) or without (`false`) refunds.
    pub devolucao_presente: Option<bool>,
    /// CPF or CNPJ of the payer.
    pub devedor: Option<Documento>,
}

impl FiltroPixRecebidos {
    /// Every Pix received in `periodo`.
    pub fn new(periodo: PeriodoPix) -> Self {
        Self {
            periodo,
            txid: None,
            tx_id_presente: None,
            devolucao_presente: None,
            devedor: None,
        }
    }

    pub(crate) fn query(&self) -> Vec<(&'static str, String)> {
        let mut query: Vec<(&'static str, String)> = self.periodo.query().into();
        if let Some(txid) = &self.txid {
            query.push(("txId", txid.as_str().to_owned()));
        }
        if let Some(presente) = self.tx_id_presente {
            query.push(("txIdPresente", presente.to_string()));
        }
        if let Some(presente) = self.devolucao_presente {
            query.push(("devolucaoPresente", presente.to_string()));
        }
        match &self.devedor {
            Some(Documento::Cpf(cpf)) => query.push(("cpf", cpf.clone())),
            Some(Documento::Cnpj(cnpj)) => query.push(("cnpj", cnpj.clone())),
            None => {}
        }
        query
    }
}

/// A page of [`Pix::listar_pix_recebidos`](super::Pix::listar_pix_recebidos)
/// (`PixConsultados`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PaginaPixRecebidos {
    /// The filters and the page, as the API understood them.
    #[serde(default)]
    pub parametros: ParametrosConsulta,
    /// The Pix of the page.
    #[serde(default, deserialize_with = "lenient::vec")]
    pub pix: Vec<PixRecebido>,
}

/// A Pix received (`Pix`), inside a charge or in the listing of the Pix
/// received.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PixRecebido {
    /// Identifier of the transaction in the Pix system (`endToEndId`).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub end_to_end_id: Option<String>,
    /// txid of the charge paid, if any.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub txid: Option<String>,
    /// Amount received.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::decimal",
        serialize_with = "decimal_texto::serialize_option"
    )]
    pub valor: Option<Decimal>,
    /// How the amount is made up (original, interest, fine, discount,
    /// rebate, withdrawal, change), as received.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub componentes_valor: Option<Value>,
    /// Pix key of the receiver.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub chave: Option<String>,
    /// When the Pix was processed (RFC 3339).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub horario: Option<String>,
    /// Message of the payer.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub info_pagador: Option<String>,
    /// Refunds of this Pix.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "lenient::vec"
    )]
    pub devolucoes: Vec<Devolucao>,
}

/// A refund of a Pix received (`Devolucao`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Devolucao {
    /// Identifier chosen for the refund.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub id: Option<String>,
    /// Identifier of the refund in the Pix system (`rtrId`).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub rtr_id: Option<String>,
    /// Amount refunded.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::decimal",
        serialize_with = "decimal_texto::serialize_option"
    )]
    pub valor: Option<Decimal>,
    /// When the refund was requested and settled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub horario: Option<HorarioDevolucao>,
    /// Where the refund stands.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<StatusDevolucao>,
    /// Why the refund reached its status.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub motivo: Option<String>,
    /// Which part of the Pix or why it was refunded (`ORIGINAL`,
    /// `MED_FRAUDE`...), as the refunds of Pix Automático bring it.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub natureza: Option<String>,
    /// The text of the refund, as the refunds of Pix Automático bring it.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub descricao: Option<String>,
}

/// When a refund was requested and settled (RFC 3339).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct HorarioDevolucao {
    /// Requested.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub solicitacao: Option<String>,
    /// Settled.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub liquidacao: Option<String>,
}

api_enum! {
    /// Where a refund stands.
    pub enum StatusDevolucao {
        /// `EM_PROCESSAMENTO`: being processed.
        EmProcessamento => "EM_PROCESSAMENTO",
        /// `DEVOLVIDO`: refunded.
        Devolvido => "DEVOLVIDO",
        /// `NAO_REALIZADO`: not made.
        NaoRealizado => "NAO_REALIZADO",
    }
}

string_serde!(StatusDevolucao);

impl StatusDevolucao {
    /// Whether the refund ended, made or not.
    pub fn is_final(&self) -> bool {
        matches!(self, Self::Devolvido | Self::NaoRealizado)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn refunds_are_checked_before_sending() {
        let mut devolucao = DevolucaoSolicitada::new("7.89".parse().unwrap());
        devolucao.validar().unwrap();
        assert_eq!(
            serde_json::to_value(&devolucao).unwrap(),
            json!({"valor": "7.89"})
        );
        devolucao.natureza = Some(NaturezaDevolucao::Retirada);
        devolucao.descricao = Some("Troco devolvido".to_owned());
        assert_eq!(
            serde_json::to_value(&devolucao).unwrap(),
            json!({"valor": "7.89", "natureza": "RETIRADA", "descricao": "Troco devolvido"})
        );
        devolucao.descricao = Some("x".repeat(141));
        assert_eq!(devolucao.validar().unwrap_err().campo(), "descricao");
        devolucao.valor = Decimal::ZERO;
        assert_eq!(devolucao.validar().unwrap_err().campo(), "valor");
    }

    #[test]
    fn refund_ids_have_up_to_35_letters_and_digits() {
        assert!("a".parse::<IdDevolucao>().is_ok());
        assert!("A1".repeat(17).parse::<IdDevolucao>().is_ok());
        for invalido in ["", "a".repeat(36).as_str(), "a b", "ação", "../x"] {
            assert_eq!(
                invalido.parse::<IdDevolucao>(),
                Err(IdDevolucaoError),
                "{invalido}"
            );
        }
        let id = IdDevolucao::novo();
        assert_eq!(id.as_str().parse::<IdDevolucao>().unwrap(), id);
    }

    #[test]
    fn filters_become_the_query() {
        use chrono::DateTime;
        let periodo = PeriodoPix::new(
            DateTime::parse_from_rfc3339("2026-09-01T00:00:00-03:00").unwrap(),
            DateTime::parse_from_rfc3339("2026-09-30T23:59:59-03:00").unwrap(),
        )
        .unwrap();
        let mut filtro = FiltroPixRecebidos::new(periodo);
        filtro.txid = Some("7978c0c97ea847e78e8849634473c1f1".parse().unwrap());
        filtro.tx_id_presente = Some(true);
        filtro.devolucao_presente = Some(false);
        filtro.devedor = Some("123.456.789-09".parse().unwrap());
        assert_eq!(
            filtro.query()[2..],
            [
                ("txId", "7978c0c97ea847e78e8849634473c1f1".to_owned()),
                ("txIdPresente", "true".to_owned()),
                ("devolucaoPresente", "false".to_owned()),
                ("cpf", "12345678909".to_owned()),
            ]
        );
    }

    #[test]
    fn a_received_pix_keeps_what_came() {
        let pix: PixRecebido = serde_json::from_value(json!({
            "endToEndId": "E12345678202609231200abcdef12345",
            "valor": "110.00",
            "componentesValor": {"original": {"valor": 100}, "juros": {"valor": "10.00"}},
            "horario": "2026-09-23T12:00:00.000Z",
            "devolucoes": [{"id": "D1", "valor": 7.5, "status": "DEVOLVIDO", "horario": {"liquidacao": "2026-09-23T12:05:00Z"}}]
        }))
        .unwrap();
        assert_eq!(pix.valor, Some("110.00".parse().unwrap()));
        let devolucao = &pix.devolucoes[0];
        assert_eq!(devolucao.status, Some(StatusDevolucao::Devolvido));
        assert!(
            devolucao
                .status
                .as_ref()
                .is_some_and(StatusDevolucao::is_final)
        );
        let de_volta = serde_json::to_value(&pix).unwrap();
        assert_eq!(de_volta["valor"], "110.00");
        assert_eq!(de_volta["devolucoes"][0]["valor"], "7.50");
        assert_eq!(de_volta["componentesValor"]["original"]["valor"], 100);
        assert_eq!(
            StatusDevolucao::from("OUTRO"),
            StatusDevolucao::Outro("OUTRO".to_owned())
        );
    }
}
