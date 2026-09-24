//! The webhook of the Cobrança API (`/cobranca/v3/cobrancas/webhook`):
//! notifications of the charges received, cancelled and expired, the
//! history of the callbacks and their retry.

use serde_json::json;

use super::{Cobranca, codigo};
use crate::client::ApiRequest;
use crate::endpoint;
use crate::error::Result;
use crate::retry::RetryMode;
use crate::webhook::{
    self, Callback, FiltroCallbacks, ITENS_POR_PAGINA_CALLBACKS_MAXIMO, PaginaCallbacks,
    ReenvioCallbacks, Webhook, WebhookUrl,
};

impl Cobranca<'_> {
    /// Registers the webhook, or changes its address (`PUT
    /// /cobranca/v3/cobrancas/webhook`, scope `boleto-cobranca.write`).
    ///
    /// The request is repeated automatically only when it surely was not
    /// processed; after an unknown outcome, look the webhook up with
    /// [`consultar_webhook`](Self::consultar_webhook).
    ///
    /// # Errors
    ///
    /// Failures to obtain a token or to send the request, and the API's
    /// error statuses.
    pub async fn cadastrar_webhook(&self, url: &WebhookUrl) -> Result<()> {
        let request = ApiRequest::new(endpoint::cobranca::WEBHOOK_CADASTRAR)
            .json(json!({ "webhookUrl": url.as_str() }))
            .retry(RetryMode::WhenNotProcessed);
        self.client.execute_empty(request).await
    }

    /// The webhook, or `None` when there is none (`GET
    /// /cobranca/v3/cobrancas/webhook`, scope `boleto-cobranca.read`).
    ///
    /// # Errors
    ///
    /// The same as [`cadastrar_webhook`](Self::cadastrar_webhook), and
    /// failures to decode the answer.
    pub async fn consultar_webhook(&self) -> Result<Option<Webhook>> {
        let request = ApiRequest::new(endpoint::cobranca::WEBHOOK_CONSULTAR);
        webhook::se_existir(self.client.execute(request).await)
    }

    /// Removes the webhook (`DELETE /cobranca/v3/cobrancas/webhook`, scope
    /// `boleto-cobranca.write`).
    ///
    /// # Errors
    ///
    /// The same as [`cadastrar_webhook`](Self::cadastrar_webhook); without
    /// a webhook, status `404`.
    pub async fn excluir_webhook(&self) -> Result<()> {
        let request =
            ApiRequest::new(endpoint::cobranca::WEBHOOK_EXCLUIR).retry(RetryMode::WhenNotProcessed);
        self.client.execute_empty(request).await
    }

    /// One page of the history of the callbacks, latest first (`GET
    /// /cobranca/v3/cobrancas/webhook/callbacks`, scope
    /// `boleto-cobranca.read`).
    ///
    /// `pagina` starts at 0. Pages have from 10 to 50 callbacks, 20 without
    /// `itens_por_pagina`. The identifier of the filter is the
    /// `codigoSolicitacao` of a charge.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`](crate::Error::InvalidInput) when the page
    /// size or the code is invalid (nothing is sent); otherwise the same as
    /// [`consultar_webhook`](Self::consultar_webhook).
    pub async fn listar_callbacks(
        &self,
        filtro: &FiltroCallbacks,
        pagina: u32,
        itens_por_pagina: Option<u32>,
    ) -> Result<PaginaCallbacks> {
        let mut request = ApiRequest::new(endpoint::cobranca::WEBHOOK_CALLBACKS)
            .queries(filtro.query())
            .query("pagina", pagina.to_string());
        if let Some(itens) = webhook::itens_por_pagina(itens_por_pagina)? {
            request = request.query("tamanhoPagina", itens.to_string());
        }
        if let Some(id) = filtro.identificador() {
            request = request.query("codigoSolicitacao", codigo(id)?);
        }
        self.client.execute(request).await
    }

    /// Every callback of the history, reading as many pages of
    /// [`ITENS_POR_PAGINA_CALLBACKS_MAXIMO`] as needed.
    ///
    /// # Errors
    ///
    /// The same as [`listar_callbacks`](Self::listar_callbacks).
    pub async fn listar_todos_callbacks(&self, filtro: &FiltroCallbacks) -> Result<Vec<Callback>> {
        webhook::todos("callbacks", |pagina| {
            self.listar_callbacks(filtro, pagina, Some(ITENS_POR_PAGINA_CALLBACKS_MAXIMO))
        })
        .await
    }

    /// Asks Inter to send again the callbacks of up to
    /// [`MAX_IDS_REENVIO`](webhook::MAX_IDS_REENVIO) charges, by their
    /// `codigoSolicitacao` (`POST
    /// /cobranca/v3/cobrancas/webhook/callbacks/retry`, scope
    /// `boleto-cobranca.write`). The answer names those found.
    ///
    /// The request is repeated automatically only when it surely was not
    /// processed.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`](crate::Error::InvalidInput) when there are no
    /// codes, more than [`MAX_IDS_REENVIO`](webhook::MAX_IDS_REENVIO), or an
    /// invalid code (nothing is sent); otherwise the same as
    /// [`consultar_webhook`](Self::consultar_webhook).
    pub async fn reenviar_callbacks(&self, codigos: &[String]) -> Result<ReenvioCallbacks> {
        webhook::quantos_reenviar(codigos.len())?;
        let codigos = codigos
            .iter()
            .map(|id| codigo(id))
            .collect::<Result<Vec<_>>>()?;
        let request = ApiRequest::new(endpoint::cobranca::WEBHOOK_REENVIAR)
            .json(json!({ "codigoSolicitacao": codigos }))
            .retry(RetryMode::WhenNotProcessed);
        self.client.execute(request).await
    }
}
