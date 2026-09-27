//! Pix received by the account and their refunds, as returned by the Pix API.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::serde_util::{api_enum, decimal_texto, lenient, string_serde};

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
