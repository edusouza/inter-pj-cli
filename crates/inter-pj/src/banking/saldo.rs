use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::serde_util::decimal_as_number;

/// Account balance, as returned by `GET /banking/v2/saldo`.
///
/// Every field is optional: when the balance is queried for a specific date,
/// only [`disponivel`](Self::disponivel) is returned.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Saldo {
    /// Available (net) balance.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "decimal_as_number::serialize_option"
    )]
    pub disponivel: Option<Decimal>,
    /// Amount blocked by deposited cheques.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "decimal_as_number::serialize_option"
    )]
    pub bloqueado_cheque: Option<Decimal>,
    /// Amount blocked by court order.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "decimal_as_number::serialize_option"
    )]
    pub bloqueado_judicialmente: Option<Decimal>,
    /// Amount blocked administratively.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "decimal_as_number::serialize_option"
    )]
    pub bloqueado_administrativo: Option<Decimal>,
    /// Credit limit of the account.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "decimal_as_number::serialize_option"
    )]
    pub limite: Option<Decimal>,
    /// Reference date (`DD/MM/AAAA`), returned when the query falls on a
    /// non-business day: the balance is then the one of the last business day.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_referencia: Option<String>,
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    fn dec(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    #[test]
    fn parses_full_response_exactly() {
        let json = r#"{"bloqueadoCheque":240.25,"disponivel":2850.55,"bloqueadoJudicialmente":510.35,"bloqueadoAdministrativo":0.1,"limite":1000,"dataReferencia":"01/01/2024"}"#;
        let saldo: Saldo = serde_json::from_str(json).unwrap();
        assert_eq!(saldo.disponivel, Some(dec("2850.55")));
        assert_eq!(saldo.bloqueado_cheque, Some(dec("240.25")));
        assert_eq!(saldo.bloqueado_judicialmente, Some(dec("510.35")));
        assert_eq!(saldo.bloqueado_administrativo, Some(dec("0.1")));
        assert_eq!(saldo.limite, Some(dec("1000")));
        assert_eq!(saldo.data_referencia.as_deref(), Some("01/01/2024"));
    }

    #[test]
    fn accepts_values_as_strings_and_missing_fields() {
        let saldo: Saldo = serde_json::from_str(r#"{"disponivel":"-12.30","extra":true}"#).unwrap();
        assert_eq!(saldo.disponivel, Some(dec("-12.30")));
        assert_eq!(saldo.limite, None);
    }

    #[test]
    fn serializes_numbers_with_api_field_names() {
        let saldo: Saldo = serde_json::from_str(r#"{"disponivel":2850.55,"limite":0}"#).unwrap();
        assert_eq!(
            serde_json::to_string(&saldo).unwrap(),
            r#"{"disponivel":2850.55,"limite":0}"#
        );
    }
}
