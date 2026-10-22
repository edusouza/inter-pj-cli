//! The operations of the Pix API (`/pix/v2`).

use std::future::Future;

use serde::Serialize;

use super::cob::{Cob, CobRevisada, CobSolicitada, FiltroCobs, PaginaCobs};
use super::cobv::{Cobv, CobvRevisada, CobvSolicitada, FiltroCobvs, PaginaCobvs};
use super::comum::{ITENS_POR_PAGINA_MAXIMO_PIX, Paginacao};
use super::recebido::{
    Devolucao, DevolucaoSolicitada, FiltroPixRecebidos, IdDevolucao, PaginaPixRecebidos,
    PixRecebido,
};
use super::txid::Txid;
use crate::client::{ApiRequest, InterClient};
use crate::endpoint;
use crate::error::{Error, Result};
use crate::retry::RetryMode;

/// Safety net against an API that never reports the last page.
const MAX_PAGINAS: u32 = 10_000;

/// Operations of the Pix API (`/pix/v2`), the Banco Central's standard for
/// receiving through Pix: charges with a dynamic QR Code, the Pix received
/// and their refunds. Obtained with [`InterClient::pix`].
///
/// Pix *payments* by the account are in the Banking API:
/// [`Banking::enviar_pix`](crate::banking::Banking::enviar_pix).
#[derive(Debug, Clone, Copy)]
pub struct Pix<'a> {
    client: &'a InterClient,
}

impl<'a> Pix<'a> {
    pub(crate) fn new(client: &'a InterClient) -> Self {
        Self { client }
    }

    /// Creates an immediate charge with your `txid` (`PUT
    /// /pix/v2/cob/{txid}`, scope `cob.write`).
    ///
    /// The API refuses a second charge with the same txid, so, after an
    /// unknown outcome (a timeout, `5xx`), the same call can be repeated
    /// without creating two charges; the request itself is repeated
    /// automatically only when it surely was not processed. Use
    /// [`Txid::novo`] for a random txid.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] with a
    /// [`CobrancaPixError`](super::CobrancaPixError) when the charge is
    /// invalid (nothing is sent); otherwise, failures to obtain a token, to
    /// send the request or to decode the answer, and the API's error
    /// statuses.
    pub async fn criar_cob(&self, txid: &Txid, cob: &CobSolicitada) -> Result<Cob> {
        cob.validar()
            .map_err(|err| Error::InvalidInput(Box::new(err)))?;
        let request = ApiRequest::new(endpoint::pix::CRIAR_COB)
            .path_param("txid", txid.as_str().to_owned())
            .json(corpo(cob)?)
            .retry(RetryMode::WhenNotProcessed);
        self.client.execute(request).await
    }

    /// Creates an immediate charge whose txid is chosen by Inter (`POST
    /// /pix/v2/cob`, scope `cob.write`). Prefer
    /// [`criar_cob`](Self::criar_cob): after an unknown outcome, this one
    /// cannot be repeated without the risk of two charges.
    ///
    /// # Errors
    ///
    /// Same as [`criar_cob`](Self::criar_cob).
    pub async fn criar_cob_sem_txid(&self, cob: &CobSolicitada) -> Result<Cob> {
        cob.validar()
            .map_err(|err| Error::InvalidInput(Box::new(err)))?;
        let request = ApiRequest::new(endpoint::pix::CRIAR_COB_SEM_TXID)
            .json(corpo(cob)?)
            .retry(RetryMode::WhenNotProcessed);
        self.client.execute(request).await
    }

    /// Changes an immediate charge, or removes it (`PATCH
    /// /pix/v2/cob/{txid}`, scope `cob.write`). The revision number of the
    /// charge goes up by one.
    ///
    /// # Errors
    ///
    /// Same as [`criar_cob`](Self::criar_cob); also when nothing changes.
    pub async fn revisar_cob(&self, txid: &Txid, revisao: &CobRevisada) -> Result<Cob> {
        revisao
            .validar()
            .map_err(|err| Error::InvalidInput(Box::new(err)))?;
        let request = ApiRequest::new(endpoint::pix::REVISAR_COB)
            .path_param("txid", txid.as_str().to_owned())
            .json(corpo(revisao)?)
            .retry(RetryMode::WhenNotProcessed);
        self.client.execute(request).await
    }

    /// An immediate charge, with the Pix that paid it (`GET
    /// /pix/v2/cob/{txid}`, scope `cob.read`).
    ///
    /// # Errors
    ///
    /// Failures to obtain a token, to send the request or to decode the
    /// answer, and the API's error statuses: `404` for an unknown txid.
    pub async fn consultar_cob(&self, txid: &Txid) -> Result<Cob> {
        let request = ApiRequest::new(endpoint::pix::CONSULTAR_COB)
            .path_param("txid", txid.as_str().to_owned());
        self.client.execute(request).await
    }

    /// One page of the immediate charges created in a period (`GET
    /// /pix/v2/cob`, scope `cob.read`).
    ///
    /// `pagina` starts at 0. Without `itens_por_pagina` the API returns 100
    /// charges per page; it accepts up to [`ITENS_POR_PAGINA_MAXIMO_PIX`].
    /// See [`listar_todas_cobs`](Self::listar_todas_cobs) to read every page.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] when `itens_por_pagina` is out of range;
    /// otherwise the same as [`consultar_cob`](Self::consultar_cob).
    pub async fn listar_cobs(
        &self,
        filtro: &FiltroCobs,
        pagina: u32,
        itens_por_pagina: Option<u32>,
    ) -> Result<PaginaCobs> {
        let request = paginada(
            ApiRequest::new(endpoint::pix::LISTAR_COBS).queries(filtro.query()),
            pagina,
            itens_por_pagina,
        )?;
        self.client.execute(request).await
    }

    /// Every immediate charge of the period, reading as many pages of
    /// [`ITENS_POR_PAGINA_MAXIMO_PIX`] as needed.
    ///
    /// # Errors
    ///
    /// Same as [`listar_cobs`](Self::listar_cobs).
    pub async fn listar_todas_cobs(&self, filtro: &FiltroCobs) -> Result<Vec<Cob>> {
        todas("cobranças imediatas", |numero| async move {
            let pagina = self
                .listar_cobs(filtro, numero, Some(ITENS_POR_PAGINA_MAXIMO_PIX))
                .await?;
            Ok((pagina.cobs, pagina.parametros.paginacao.unwrap_or_default()))
        })
        .await
    }
}

impl Pix<'_> {
    /// Creates a charge with a due date (`PUT /pix/v2/cobv/{txid}`, scope
    /// `cobv.write`). As with [`criar_cob`](Self::criar_cob), the txid makes
    /// a creation with an unknown outcome safe to repeat.
    ///
    /// # Errors
    ///
    /// Same as [`criar_cob`](Self::criar_cob).
    pub async fn criar_cobv(&self, txid: &Txid, cobv: &CobvSolicitada) -> Result<Cobv> {
        cobv.validar()
            .map_err(|err| Error::InvalidInput(Box::new(err)))?;
        let request = ApiRequest::new(endpoint::pix::CRIAR_COBV)
            .path_param("txid", txid.as_str().to_owned())
            .json(corpo(cobv)?)
            .retry(RetryMode::WhenNotProcessed);
        self.client.execute(request).await
    }

    /// Changes a charge with a due date, or removes it (`PATCH
    /// /pix/v2/cobv/{txid}`, scope `cobv.write`).
    ///
    /// # Errors
    ///
    /// Same as [`revisar_cob`](Self::revisar_cob).
    pub async fn revisar_cobv(&self, txid: &Txid, revisao: &CobvRevisada) -> Result<Cobv> {
        revisao
            .validar()
            .map_err(|err| Error::InvalidInput(Box::new(err)))?;
        let request = ApiRequest::new(endpoint::pix::REVISAR_COBV)
            .path_param("txid", txid.as_str().to_owned())
            .json(corpo(revisao)?)
            .retry(RetryMode::WhenNotProcessed);
        self.client.execute(request).await
    }

    /// A charge with a due date, with the Pix that paid it (`GET
    /// /pix/v2/cobv/{txid}`, scope `cobv.read`).
    ///
    /// # Errors
    ///
    /// Same as [`consultar_cob`](Self::consultar_cob).
    pub async fn consultar_cobv(&self, txid: &Txid) -> Result<Cobv> {
        let request = ApiRequest::new(endpoint::pix::CONSULTAR_COBV)
            .path_param("txid", txid.as_str().to_owned());
        self.client.execute(request).await
    }

    /// One page of the charges with a due date created in a period (`GET
    /// /pix/v2/cobv`, scope `cobv.read`).
    ///
    /// # Errors
    ///
    /// Same as [`listar_cobs`](Self::listar_cobs).
    pub async fn listar_cobvs(
        &self,
        filtro: &FiltroCobvs,
        pagina: u32,
        itens_por_pagina: Option<u32>,
    ) -> Result<PaginaCobvs> {
        let request = paginada(
            ApiRequest::new(endpoint::pix::LISTAR_COBVS).queries(filtro.query()),
            pagina,
            itens_por_pagina,
        )?;
        self.client.execute(request).await
    }

    /// Every charge with a due date of the period.
    ///
    /// # Errors
    ///
    /// Same as [`listar_cobs`](Self::listar_cobs).
    pub async fn listar_todas_cobvs(&self, filtro: &FiltroCobvs) -> Result<Vec<Cobv>> {
        todas("cobranças com vencimento", |numero| async move {
            let pagina = self
                .listar_cobvs(filtro, numero, Some(ITENS_POR_PAGINA_MAXIMO_PIX))
                .await?;
            Ok((pagina.cobs, pagina.parametros.paginacao.unwrap_or_default()))
        })
        .await
    }
}

impl Pix<'_> {
    /// One page of the Pix received in a period (`GET /pix/v2/pix`, scope
    /// `pix.read`).
    ///
    /// # Errors
    ///
    /// Same as [`listar_cobs`](Self::listar_cobs).
    pub async fn listar_pix_recebidos(
        &self,
        filtro: &FiltroPixRecebidos,
        pagina: u32,
        itens_por_pagina: Option<u32>,
    ) -> Result<PaginaPixRecebidos> {
        let request = paginada(
            ApiRequest::new(endpoint::pix::LISTAR_RECEBIDOS).queries(filtro.query()),
            pagina,
            itens_por_pagina,
        )?;
        self.client.execute(request).await
    }

    /// Every Pix received in the period.
    ///
    /// # Errors
    ///
    /// Same as [`listar_cobs`](Self::listar_cobs).
    pub async fn listar_todos_pix_recebidos(
        &self,
        filtro: &FiltroPixRecebidos,
    ) -> Result<Vec<PixRecebido>> {
        todas("Pix recebidos", |numero| async move {
            let pagina = self
                .listar_pix_recebidos(filtro, numero, Some(ITENS_POR_PAGINA_MAXIMO_PIX))
                .await?;
            Ok((pagina.pix, pagina.parametros.paginacao.unwrap_or_default()))
        })
        .await
    }

    /// A Pix received, with its refunds (`GET /pix/v2/pix/{e2eId}`, scope
    /// `pix.read`).
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] when `end_to_end_id` has other characters
    /// than letters and digits; otherwise the same as
    /// [`consultar_cob`](Self::consultar_cob).
    pub async fn consultar_pix_recebido(&self, end_to_end_id: &str) -> Result<PixRecebido> {
        let request = ApiRequest::new(endpoint::pix::CONSULTAR_RECEBIDO)
            .path_param("e2eId", e2e_id(end_to_end_id)?);
        self.client.execute(request).await
    }

    /// Refunds all or part of a Pix received (`PUT
    /// /pix/v2/pix/{e2eId}/devolucao/{id}`, scope `pix.write`). **Money
    /// leaves the account.**
    ///
    /// The API does not refund twice with the same `id`, so, after an
    /// unknown outcome, the same call can be repeated safely; the request
    /// itself is repeated automatically only when it surely was not
    /// processed. The refund is processed afterwards: follow it with
    /// [`consultar_devolucao`](Self::consultar_devolucao).
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] when the refund or the end-to-end id is
    /// invalid (nothing is sent); otherwise the same as
    /// [`criar_cob`](Self::criar_cob).
    pub async fn devolver(
        &self,
        end_to_end_id: &str,
        id: &IdDevolucao,
        devolucao: &DevolucaoSolicitada,
    ) -> Result<Devolucao> {
        devolucao
            .validar()
            .map_err(|err| Error::InvalidInput(Box::new(err)))?;
        let request = ApiRequest::new(endpoint::pix::SOLICITAR_DEVOLUCAO)
            .path_param("e2eId", e2e_id(end_to_end_id)?)
            .path_param("id", id.as_str().to_owned())
            .json(corpo(devolucao)?)
            .retry(RetryMode::WhenNotProcessed);
        self.client.execute(request).await
    }

    /// Where a refund stands (`GET /pix/v2/pix/{e2eId}/devolucao/{id}`,
    /// scope `pix.read`).
    ///
    /// # Errors
    ///
    /// Same as [`consultar_pix_recebido`](Self::consultar_pix_recebido).
    pub async fn consultar_devolucao(
        &self,
        end_to_end_id: &str,
        id: &IdDevolucao,
    ) -> Result<Devolucao> {
        let request = ApiRequest::new(endpoint::pix::CONSULTAR_DEVOLUCAO)
            .path_param("e2eId", e2e_id(end_to_end_id)?)
            .path_param("id", id.as_str().to_owned());
        self.client.execute(request).await
    }
}

/// An end-to-end id: letters and digits. The documentation defines 32, but
/// its own examples have more, so only the characters are checked.
fn e2e_id(texto: &str) -> Result<String> {
    let id = texto.trim();
    if (1..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric()) {
        Ok(id.to_owned())
    } else {
        Err(Error::InvalidInput(
            "endToEndId inválido: esperados letras e dígitos (ex.: E12345678202609231200abcdef12345)"
                .into(),
        ))
    }
}

/// A request body, as the API names its fields.
fn corpo(modelo: &impl Serialize) -> Result<serde_json::Value> {
    serde_json::to_value(modelo).map_err(|err| Error::InvalidInput(Box::new(err)))
}

/// `request` for the page `pagina`, with `itens_por_pagina` items.
fn paginada(request: ApiRequest, pagina: u32, itens_por_pagina: Option<u32>) -> Result<ApiRequest> {
    let mut request = request.query("paginacao.paginaAtual", pagina.to_string());
    if let Some(itens) = itens_por_pagina {
        if !(1..=ITENS_POR_PAGINA_MAXIMO_PIX).contains(&itens) {
            return Err(Error::InvalidInput(
                format!("itens por página: de 1 a {ITENS_POR_PAGINA_MAXIMO_PIX}").into(),
            ));
        }
        request = request.query("paginacao.itensPorPagina", itens.to_string());
    }
    Ok(request)
}

/// Every item of a listing, page after page, until the API says there are no
/// more (or sends an empty page).
async fn todas<T, F, Fut>(nome: &str, mut pagina: F) -> Result<Vec<T>>
where
    F: FnMut(u32) -> Fut,
    Fut: Future<Output = Result<(Vec<T>, Paginacao)>>,
{
    let mut itens = Vec::new();
    let mut numero = 0;
    loop {
        let (mut recebidos, paginacao) = pagina(numero).await?;
        let quantos = recebidos.len();
        let tem_mais = paginacao.tem_mais(numero, quantos);
        itens.append(&mut recebidos);
        tracing::info!("{nome}: página {} com {quantos}", numero + 1);
        if quantos == 0 || !tem_mais || numero + 1 >= MAX_PAGINAS {
            return Ok(itens);
        }
        numero += 1;
    }
}
