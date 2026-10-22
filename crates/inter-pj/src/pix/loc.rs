//! Locations of payloads (`loc`): the address a dynamic QR Code points to,
//! created beforehand and linked to a charge — and the payments of the
//! sandbox.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::cob::ParametrosConsulta;
use super::comum::{LocationPix, PeriodoPix, TipoCob};
use crate::serde_util::{decimal_as_number, lenient};

/// Filters of [`Pix::listar_locs`](super::Pix::listar_locs): the period of
/// creation and, optionally, whether a charge is linked and its kind.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct FiltroLocs {
    /// Period of creation.
    pub periodo: PeriodoPix,
    /// Only locations with (`true`) or without (`false`) a charge.
    pub tx_id_presente: Option<bool>,
    /// Only locations for this kind of charge.
    pub tipo_cob: Option<TipoCob>,
}

impl FiltroLocs {
    /// Every location created in `periodo`.
    pub fn new(periodo: PeriodoPix) -> Self {
        Self {
            periodo,
            tx_id_presente: None,
            tipo_cob: None,
        }
    }

    pub(crate) fn query(&self) -> Vec<(&'static str, String)> {
        let mut query: Vec<(&'static str, String)> = self.periodo.query().into();
        if let Some(presente) = self.tx_id_presente {
            query.push(("txIdPresente", presente.to_string()));
        }
        if let Some(tipo) = &self.tipo_cob {
            query.push(("tipoCob", tipo.as_str().to_owned()));
        }
        query
    }
}

/// A page of [`Pix::listar_locs`](super::Pix::listar_locs)
/// (`PayloadLocationConsultadas`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PaginaLocs {
    /// The filters and the page, as the API understood them.
    #[serde(default)]
    pub parametros: ParametrosConsulta,
    /// The locations of the page.
    #[serde(default, deserialize_with = "lenient::vec")]
    pub loc: Vec<LocationPix>,
}

/// The body of `POST /pix/v2/loc` (`PayloadLocationSolicitada`).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocSolicitada<'a> {
    pub(crate) tipo_cob: &'a str,
}

/// The body of the payment of a charge in the sandbox (`PagarCobrancaPix`).
#[derive(Serialize)]
pub(crate) struct PagamentoCobSandbox {
    #[serde(serialize_with = "decimal_as_number::serialize")]
    pub(crate) valor: Decimal,
}

/// The body of the payment of a "copia e cola" in the sandbox
/// (`MakePaymentCobCobv`).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PagamentoQrCodeSandbox<'a> {
    pub(crate) qr_code: &'a str,
    #[serde(serialize_with = "decimal_as_number::serialize")]
    pub(crate) valor: Decimal,
}

/// A payment made in the sandbox: the end-to-end id of the Pix
/// (`PagarCobrancaPixResponse`, as `e2e`, and `MakePaymentCobCobvResponse`,
/// as `endToEnd`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PagamentoSandbox {
    /// End-to-end id of the Pix, when paid by txid.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub e2e: Option<String>,
    /// End-to-end id of the Pix, when paid by "copia e cola".
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub end_to_end: Option<String>,
}

impl PagamentoSandbox {
    /// The end-to-end id, whichever field brought it.
    pub fn end_to_end_id(&self) -> Option<&str> {
        self.e2e.as_deref().or(self.end_to_end.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use chrono::DateTime;
    use serde_json::json;

    use super::*;

    #[test]
    fn filters_become_the_query() {
        let periodo = PeriodoPix::new(
            DateTime::parse_from_rfc3339("2026-09-01T00:00:00-03:00").unwrap(),
            DateTime::parse_from_rfc3339("2026-09-30T23:59:59-03:00").unwrap(),
        )
        .unwrap();
        let mut filtro = FiltroLocs::new(periodo);
        filtro.tx_id_presente = Some(false);
        filtro.tipo_cob = Some(TipoCob::Cobv);
        assert_eq!(
            filtro.query()[2..],
            [
                ("txIdPresente", "false".to_owned()),
                ("tipoCob", "cobv".to_owned())
            ]
        );
    }

    #[test]
    fn sandbox_payments_send_numbers() {
        let cob = PagamentoCobSandbox {
            valor: "150.00".parse().unwrap(),
        };
        assert_eq!(serde_json::to_value(cob).unwrap(), json!({"valor": 150}));
        let qr = PagamentoQrCodeSandbox {
            qr_code: "000201",
            valor: "100.50".parse().unwrap(),
        };
        assert_eq!(
            serde_json::to_value(qr).unwrap(),
            json!({"qrCode": "000201", "valor": 100.5})
        );
        let pago: PagamentoSandbox =
            serde_json::from_value(json!({"endToEnd": "E00416968202406141552CmNRIqASznP"}))
                .unwrap();
        assert_eq!(
            pago.end_to_end_id(),
            Some("E00416968202406141552CmNRIqASznP")
        );
    }
}
