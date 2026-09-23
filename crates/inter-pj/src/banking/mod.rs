//! Banking API (`/banking/v2`): balance and statements; payments and
//! outbound Pix in later versions.

mod detalhe;
mod extrato;
mod periodo;
mod saldo;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use chrono::NaiveDate;
use serde::Deserialize;

pub use detalhe::{
    Detalhe, DetalheBoletoCobranca, DetalheCashback, DetalheCheque, DetalheCompraDebito,
    DetalheDepositoBoleto, DetalhePagamento, DetalhePix, DetalheTarifa, DetalheTransferencia,
};
pub use extrato::{
    FiltroExtrato, LoteScroll, PaginaExtrato, TipoOperacao, TipoTransacao, TransacaoCompleta,
    TransacaoSimples,
};
pub use periodo::{Periodo, PeriodoError};
pub use saldo::Saldo;

use crate::client::{ApiRequest, InterClient};
use crate::endpoint::{self, Endpoint};
use crate::error::{Error, Result};
use crate::retry::RetryMode;

/// Largest page of the enriched statement the API returns.
pub const TAMANHO_PAGINA_MAXIMO: u32 = 10_000;

/// Transactions reachable with traditional pagination; beyond that the
/// enriched statement has to be read in scroll mode.
pub const LIMITE_PAGINACAO: u64 = 10_000;

/// Safety net against an API that never reports the last page.
const MAX_PAGINAS: u32 = 10_000;

/// Operations of the Banking API. Obtained with [`InterClient::banking`].
#[derive(Debug, Clone, Copy)]
pub struct Banking<'a> {
    client: &'a InterClient,
}

impl<'a> Banking<'a> {
    pub(crate) fn new(client: &'a InterClient) -> Self {
        Self { client }
    }

    /// Account balance (`GET /banking/v2/saldo`, scope `extrato.read`).
    ///
    /// Without `data`, returns the current available balance together with
    /// blocked amounts and the credit limit. With `data`, returns only the
    /// available balance at the end of that day.
    ///
    /// # Errors
    ///
    /// Fails when a token cannot be obtained, the request cannot be sent, the
    /// API answers with an error status or the response cannot be decoded.
    pub async fn saldo(&self, data: Option<NaiveDate>) -> Result<Saldo> {
        let mut request = ApiRequest::new(endpoint::banking::SALDO);
        if let Some(data) = data {
            request = request.query("dataSaldo", data.format("%Y-%m-%d").to_string());
        }
        self.client.execute(request).await
    }

    /// Transactions of the period (`GET /banking/v2/extrato`, scope
    /// `extrato.read`), in the order the API returns them.
    ///
    /// # Errors
    ///
    /// Same as [`saldo`](Self::saldo).
    pub async fn extrato(&self, periodo: Periodo) -> Result<Vec<TransacaoSimples>> {
        #[derive(Deserialize)]
        struct Lista {
            #[serde(default)]
            transacoes: Vec<TransacaoSimples>,
        }
        let request = ApiRequest::new(endpoint::banking::EXTRATO).queries(periodo.query());
        let lista: Lista = self.client.execute(request).await?;
        Ok(lista.transacoes)
    }

    /// One page of the enriched statement (`GET /banking/v2/extrato/completo`,
    /// scope `extrato.read`), in traditional pagination mode.
    ///
    /// `pagina` starts at 0. Without `tamanho_pagina` the API returns 50
    /// transactions per page; it accepts up to [`TAMANHO_PAGINA_MAXIMO`].
    /// Only the first [`LIMITE_PAGINACAO`] transactions of a period can be
    /// reached this way: see [`extrato_completo_todas`](Self::extrato_completo_todas).
    ///
    /// # Errors
    ///
    /// Same as [`saldo`](Self::saldo).
    pub async fn extrato_completo(
        &self,
        filtro: &FiltroExtrato,
        pagina: u32,
        tamanho_pagina: Option<u32>,
    ) -> Result<PaginaExtrato> {
        let mut request = ApiRequest::new(endpoint::banking::EXTRATO_COMPLETO)
            .queries(filtro.query())
            .query("pagina", pagina.to_string());
        if let Some(tamanho) = tamanho_pagina {
            request = request.query("tamanhoPagina", tamanho.to_string());
        }
        self.client.execute(request).await
    }

    /// Every transaction of the enriched statement, reading as many pages as
    /// needed: traditional pagination up to [`LIMITE_PAGINACAO`] transactions,
    /// scroll mode beyond that.
    ///
    /// # Errors
    ///
    /// Same as [`saldo`](Self::saldo). In scroll mode, the API also refuses a
    /// new scroll while another one is active for the account (problem type
    /// `SCROLL_ALREADY_ACTIVE`) and expires a scroll after 6 minutes without
    /// requests (`SCROLL_EXPIRED`).
    pub async fn extrato_completo_todas(
        &self,
        filtro: &FiltroExtrato,
    ) -> Result<Vec<TransacaoCompleta>> {
        let tamanho = TAMANHO_PAGINA_MAXIMO;
        let mut pagina = self.extrato_completo(filtro, 0, Some(tamanho)).await?;
        let total = pagina.total_elementos;
        if total.is_some_and(|total| total > LIMITE_PAGINACAO) {
            tracing::info!(
                "{} transações no período: lendo o extrato em modo scroll",
                total.unwrap_or_default()
            );
            return self.extrato_completo_scroll(filtro, tamanho).await;
        }

        let mut transacoes = Vec::new();
        let mut numero = 0;
        loop {
            let recebidas = pagina.transacoes.len();
            let tem_mais = pagina.tem_mais(numero, tamanho);
            transacoes.append(&mut pagina.transacoes);
            tracing::info!(
                "extrato completo: página {} com {recebidas} transações",
                numero + 1
            );
            let completo = total.is_some_and(|total| transacoes.len() as u64 >= total);
            if completo || recebidas == 0 || !tem_mais || numero + 1 >= MAX_PAGINAS {
                return Ok(transacoes);
            }
            numero += 1;
            pagina = self.extrato_completo(filtro, numero, Some(tamanho)).await?;
        }
    }

    async fn extrato_completo_scroll(
        &self,
        filtro: &FiltroExtrato,
        tamanho: u32,
    ) -> Result<Vec<TransacaoCompleta>> {
        let mut lote = self.iniciar_scroll(filtro, Some(tamanho)).await?;
        let mut transacoes = Vec::new();
        loop {
            let recebidas = lote.transacoes.len();
            transacoes.append(&mut lote.transacoes);
            tracing::info!("extrato completo (scroll): lote com {recebidas} transações");
            if lote.has_more != Some(true) {
                return Ok(transacoes);
            }
            let scroll_id = lote.scroll_id.take().filter(|id| !id.trim().is_empty());
            let Some(scroll_id) = scroll_id else {
                return Err(scroll_error(
                    "a API indicou mais transações (hasMore), mas não enviou o scrollId",
                ));
            };
            if recebidas == 0 {
                return Err(scroll_error(
                    "a API indicou mais transações (hasMore), mas enviou um lote vazio",
                ));
            }
            lote = self.continuar_scroll(filtro, &scroll_id).await?;
        }
    }

    /// Starts reading the enriched statement in scroll mode (`scrollEnabled`),
    /// meant for periods with more than [`LIMITE_PAGINACAO`] transactions.
    /// Continue with [`continuar_scroll`](Self::continuar_scroll) while
    /// [`LoteScroll::has_more`] is set.
    ///
    /// Only one scroll can be active per account, and it expires after 6
    /// minutes without requests. These requests change server-side state, so
    /// they are retried only when they surely were not processed.
    ///
    /// # Errors
    ///
    /// Same as [`extrato_completo_todas`](Self::extrato_completo_todas).
    pub async fn iniciar_scroll(
        &self,
        filtro: &FiltroExtrato,
        tamanho_pagina: Option<u32>,
    ) -> Result<LoteScroll> {
        let mut request = scroll_request(filtro).query("scrollEnabled", "true".to_owned());
        if let Some(tamanho) = tamanho_pagina {
            request = request.query("tamanhoPagina", tamanho.to_string());
        }
        self.client.execute(request).await
    }

    /// Fetches the next batch of a scroll started with
    /// [`iniciar_scroll`](Self::iniciar_scroll), with the same filter.
    ///
    /// # Errors
    ///
    /// Same as [`extrato_completo_todas`](Self::extrato_completo_todas).
    pub async fn continuar_scroll(
        &self,
        filtro: &FiltroExtrato,
        scroll_id: &str,
    ) -> Result<LoteScroll> {
        let request = scroll_request(filtro).query("scrollId", scroll_id.to_owned());
        self.client.execute(request).await
    }

    /// Statement of the period as a PDF document (`GET
    /// /banking/v2/extrato/exportar`, scope `extrato.read`).
    ///
    /// # Errors
    ///
    /// Same as [`saldo`](Self::saldo); also fails when the content received
    /// is not a base64-encoded PDF.
    pub async fn extrato_pdf(&self, periodo: Periodo) -> Result<Vec<u8>> {
        #[derive(Deserialize)]
        struct Resposta {
            #[serde(default)]
            pdf: Option<String>,
        }
        const ENDPOINT: Endpoint = endpoint::banking::EXTRATO_EXPORTAR;
        let invalid = |message: &str| Error::Decode {
            operation: ENDPOINT.to_string(),
            message: message.to_owned(),
        };

        let request = ApiRequest::new(ENDPOINT).queries(periodo.query());
        let resposta: Resposta = self.client.execute(request).await?;
        let encoded: String = resposta
            .pdf
            .ok_or_else(|| invalid("a resposta não traz o campo pdf"))?
            .chars()
            .filter(|c| !c.is_ascii_whitespace())
            .collect();
        // Never quote the content in errors: it is the account statement.
        let pdf = BASE64
            .decode(encoded)
            .map_err(|_| invalid("o campo pdf não está em base64"))?;
        if !pdf.starts_with(b"%PDF") {
            return Err(invalid("o conteúdo recebido não é um PDF"));
        }
        Ok(pdf)
    }
}

fn scroll_request(filtro: &FiltroExtrato) -> ApiRequest {
    ApiRequest::new(endpoint::banking::EXTRATO_COMPLETO)
        .queries(filtro.query())
        .retry(RetryMode::WhenNotProcessed)
}

fn scroll_error(message: &str) -> Error {
    Error::Decode {
        operation: endpoint::banking::EXTRATO_COMPLETO.to_string(),
        message: message.to_owned(),
    }
}
