//! Locations of recurrences (`/pix/v2/locrec`): the addresses of the QR
//! Codes of recurrences, created beforehand and linked when a recurrence is
//! created ([`RecSolicitada::loc`](super::RecSolicitada::loc)).

use serde::{Deserialize, Serialize};

use super::{LocationRec, PixAutomatico, RecebedorRec, convenio};
use crate::client::ApiRequest;
use crate::endpoint;
use crate::error::Result;
use crate::pix::{ITENS_POR_PAGINA_MAXIMO_PIX, Paginacao, PeriodoPix, paginada, todas};
use crate::retry::RetryMode;
use crate::serde_util::lenient;

/// Which locations of recurrences to list: those created in a period, with
/// optional filters.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct FiltroLocsRec {
    /// When they were created.
    pub periodo: PeriodoPix,
    /// Only those linked (or not) to a recurrence.
    pub id_rec_presente: Option<bool>,
    /// Only those of this agreement, up to
    /// [`MAX_CONVENIO`](super::MAX_CONVENIO) characters.
    pub convenio: Option<String>,
}

impl FiltroLocsRec {
    /// Every location created in `periodo`.
    pub fn new(periodo: PeriodoPix) -> Self {
        Self {
            periodo,
            id_rec_presente: None,
            convenio: None,
        }
    }

    fn query(&self) -> Result<Vec<(&'static str, String)>> {
        let mut query = Vec::from(self.periodo.query());
        if let Some(presente) = self.id_rec_presente {
            query.push(("idRecPresente", presente.to_string()));
        }
        if let Some(filtro) = &self.convenio {
            query.push(("convenio", convenio(filtro)?));
        }
        Ok(query)
    }
}

/// A page of locations of recurrences, as received.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct PaginaLocsRec {
    /// The filters and the page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parametros: Option<ParametrosConsultaLocRec>,
    /// The locations.
    #[serde(default, deserialize_with = "lenient::vec")]
    pub loc: Vec<LocationRec>,
}

/// The filters and the page of a listing of locations of recurrences, as
/// received.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ParametrosConsultaLocRec {
    /// Start of the period (RFC 3339).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub inicio: Option<String>,
    /// End of the period (RFC 3339).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub fim: Option<String>,
    /// The filter of the link to a recurrence.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::bool"
    )]
    pub id_rec_presente: Option<bool>,
    /// The agreement of the filter (`recebedor.convenio`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recebedor: Option<RecebedorRec>,
    /// The page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paginacao: Option<Paginacao>,
}

impl PixAutomatico<'_> {
    /// Creates a location for the QR Code of a recurrence (`POST
    /// /pix/v2/locrec`, scope `payloadlocationrec.write`), to be linked
    /// when the recurrence is created. Repeated automatically only when it
    /// surely was not processed.
    ///
    /// # Errors
    ///
    /// Failures to obtain a token, to send the request or to decode the
    /// answer, and the API's error statuses.
    pub async fn criar_locrec(&self) -> Result<LocationRec> {
        let request = ApiRequest::new(endpoint::pix_automatico::CRIAR_LOCREC)
            .retry(RetryMode::WhenNotProcessed);
        self.client.execute(request).await
    }

    /// One page of the locations of recurrences created in a period (`GET
    /// /pix/v2/locrec`, scope `payloadlocationrec.read`).
    ///
    /// `pagina` starts at 0; without `itens_por_pagina`, the API returns 100
    /// per page, and it accepts up to [`ITENS_POR_PAGINA_MAXIMO_PIX`].
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`](crate::Error::InvalidInput) when
    /// `itens_por_pagina` is out of range or the agreement of the filter is
    /// too long (nothing is sent); otherwise the same as
    /// [`criar_locrec`](Self::criar_locrec).
    pub async fn listar_locrecs(
        &self,
        filtro: &FiltroLocsRec,
        pagina: u32,
        itens_por_pagina: Option<u32>,
    ) -> Result<PaginaLocsRec> {
        let request = paginada(
            ApiRequest::new(endpoint::pix_automatico::LISTAR_LOCRECS).queries(filtro.query()?),
            pagina,
            itens_por_pagina,
        )?;
        self.client.execute(request).await
    }

    /// Every location of recurrences created in a period, reading as many
    /// pages of [`ITENS_POR_PAGINA_MAXIMO_PIX`] as needed.
    ///
    /// # Errors
    ///
    /// The same as [`listar_locrecs`](Self::listar_locrecs).
    pub async fn listar_todas_locrecs(&self, filtro: &FiltroLocsRec) -> Result<Vec<LocationRec>> {
        todas("locations de recorrências", |pagina| async move {
            let pagina = self
                .listar_locrecs(filtro, pagina, Some(ITENS_POR_PAGINA_MAXIMO_PIX))
                .await?;
            let paginacao = pagina
                .parametros
                .and_then(|parametros| parametros.paginacao)
                .unwrap_or_default();
            Ok((pagina.loc, paginacao))
        })
        .await
    }

    /// A location of recurrences (`GET /pix/v2/locrec/{id}`, scope
    /// `payloadlocationrec.read`), with the recurrence linked to it.
    ///
    /// # Errors
    ///
    /// The same as [`criar_locrec`](Self::criar_locrec); unknown locations
    /// fail with status `404`.
    pub async fn consultar_locrec(&self, id: u64) -> Result<LocationRec> {
        let request = ApiRequest::new(endpoint::pix_automatico::CONSULTAR_LOCREC)
            .path_param("id", id.to_string());
        self.client.execute(request).await
    }

    /// Unlinks the recurrence from a location (`DELETE
    /// /pix/v2/locrec/{id}/idRec`, scope `payloadlocationrec.write`): the
    /// QR Code of the location stops leading to that recurrence, which
    /// keeps its status. Repeated automatically only when it surely was
    /// not processed.
    ///
    /// # Errors
    ///
    /// The same as [`consultar_locrec`](Self::consultar_locrec).
    pub async fn desvincular_locrec(&self, id: u64) -> Result<LocationRec> {
        let request = ApiRequest::new(endpoint::pix_automatico::DESVINCULAR_LOCREC)
            .path_param("id", id.to_string())
            .retry(RetryMode::WhenNotProcessed);
        self.client.execute(request).await
    }
}

#[cfg(test)]
mod tests {
    use chrono::DateTime;

    use super::*;

    #[test]
    fn filters_go_in_the_query() {
        let mut filtro = FiltroLocsRec::new(
            PeriodoPix::new(
                DateTime::parse_from_rfc3339("2026-09-01T00:00:00-03:00").unwrap(),
                DateTime::parse_from_rfc3339("2026-09-30T23:59:59-03:00").unwrap(),
            )
            .unwrap(),
        );
        filtro.id_rec_presente = Some(true);
        filtro.convenio = Some("convenio-01".to_owned());
        assert_eq!(
            filtro.query().unwrap(),
            [
                ("inicio", "2026-09-01T00:00:00-03:00".to_owned()),
                ("fim", "2026-09-30T23:59:59-03:00".to_owned()),
                ("idRecPresente", "true".to_owned()),
                ("convenio", "convenio-01".to_owned()),
            ]
        );
        filtro.convenio = Some("x".repeat(61));
        assert!(filtro.query().is_err());
    }
}
