//! The webhooks of the Banking API (`/banking/v2/webhooks`): notifications
//! of the Pix sent and the boletos paid by the account, the history of the
//! callbacks and their retry.

use serde_json::json;

use super::Banking;
use crate::client::ApiRequest;
use crate::endpoint;
use crate::error::{Error, Result};
use crate::pix::is_uuid;
use crate::retry::RetryMode;
use crate::webhook::{
    self, Callback, FiltroCallbacks, ITENS_POR_PAGINA_CALLBACKS_MAXIMO, PaginaCallbacks,
    ReenvioCallbacks, TipoWebhookBanking, Webhook, WebhookUrl,
};

impl Banking<'_> {
    /// Registers the webhook of a kind (`PUT
    /// /banking/v2/webhooks/{tipoWebhook}`, scope `webhook-banking.write`).
    /// Each kind has its own webhook.
    ///
    /// The request is repeated automatically only when it surely was not
    /// processed; after an unknown outcome, look the webhook up with
    /// [`consultar_webhook`](Self::consultar_webhook).
    ///
    /// # Errors
    ///
    /// Failures to obtain a token or to send the request, and the API's
    /// error statuses.
    pub async fn cadastrar_webhook(
        &self,
        tipo: TipoWebhookBanking,
        url: &WebhookUrl,
    ) -> Result<()> {
        let request = ApiRequest::new(endpoint::banking::WEBHOOK_CADASTRAR)
            .path_param("tipoWebhook", tipo.as_str().to_owned())
            .json(json!({ "webhookUrl": url.as_str() }))
            .retry(RetryMode::WhenNotProcessed);
        self.client.execute_empty(request).await
    }

    /// The webhook of a kind, or `None` when there is none (`GET
    /// /banking/v2/webhooks/{tipoWebhook}`, scope `webhook-banking.read`).
    ///
    /// # Errors
    ///
    /// The same as [`cadastrar_webhook`](Self::cadastrar_webhook), and
    /// failures to decode the answer.
    pub async fn consultar_webhook(&self, tipo: TipoWebhookBanking) -> Result<Option<Webhook>> {
        let request = ApiRequest::new(endpoint::banking::WEBHOOK_CONSULTAR)
            .path_param("tipoWebhook", tipo.as_str().to_owned());
        webhook::se_existir(self.client.execute(request).await)
    }

    /// Removes the webhook of a kind (`DELETE
    /// /banking/v2/webhooks/{tipoWebhook}`, scope `webhook-banking.write`).
    ///
    /// # Errors
    ///
    /// The same as [`cadastrar_webhook`](Self::cadastrar_webhook); without
    /// a webhook, status `404`.
    pub async fn excluir_webhook(&self, tipo: TipoWebhookBanking) -> Result<()> {
        let request = ApiRequest::new(endpoint::banking::WEBHOOK_EXCLUIR)
            .path_param("tipoWebhook", tipo.as_str().to_owned())
            .retry(RetryMode::WhenNotProcessed);
        self.client.execute_empty(request).await
    }

    /// One page of the history of the callbacks of a kind, latest first
    /// (`GET /banking/v2/webhooks/{tipoWebhook}/callbacks`, scope
    /// `webhook-banking.read`).
    ///
    /// `pagina` starts at 0. Pages have from 10 to 50 callbacks, 20 without
    /// `itens_por_pagina`. The identifier of the filter is the `endToEnd` of
    /// a Pix sent or the `codigoTransacao` of a boleto paid.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] when the page size or the identifier is
    /// invalid (nothing is sent); otherwise the same as
    /// [`consultar_webhook`](Self::consultar_webhook).
    pub async fn listar_callbacks(
        &self,
        tipo: TipoWebhookBanking,
        filtro: &FiltroCallbacks,
        pagina: u32,
        itens_por_pagina: Option<u32>,
    ) -> Result<PaginaCallbacks> {
        let mut request = ApiRequest::new(endpoint::banking::WEBHOOK_CALLBACKS)
            .path_param("tipoWebhook", tipo.as_str().to_owned())
            .queries(filtro.query())
            .query("pagina", pagina.to_string());
        if let Some(itens) = webhook::itens_por_pagina(itens_por_pagina)? {
            request = request.query("tamanhoPagina", itens.to_string());
        }
        if let Some(id) = filtro.identificador() {
            request = match tipo {
                TipoWebhookBanking::PixPagamento => request.query("endToEnd", end_to_end(id)?),
                TipoWebhookBanking::BoletoPagamento => {
                    request.query("codigoTransacao", uuid(id, "código da transação")?)
                }
            };
        }
        self.client.execute(request).await
    }

    /// Every callback of the history of a kind, reading as many pages of
    /// [`ITENS_POR_PAGINA_CALLBACKS_MAXIMO`] as needed.
    ///
    /// # Errors
    ///
    /// The same as [`listar_callbacks`](Self::listar_callbacks).
    pub async fn listar_todos_callbacks(
        &self,
        tipo: TipoWebhookBanking,
        filtro: &FiltroCallbacks,
    ) -> Result<Vec<Callback>> {
        webhook::todos("callbacks", |pagina| {
            self.listar_callbacks(
                tipo,
                filtro,
                pagina,
                Some(ITENS_POR_PAGINA_CALLBACKS_MAXIMO),
            )
        })
        .await
    }

    /// Asks Inter to send again the callbacks of up to
    /// [`MAX_IDS_REENVIO`](webhook::MAX_IDS_REENVIO) operations of a kind
    /// (`POST /banking/v2/webhooks/{tipoWebhook}/callbacks/retry`, scope
    /// `webhook-banking.write`): the `codigoSolicitacao` of Pix sent, or the
    /// `codigoTransacao` of boletos paid. The answer names those found.
    ///
    /// The request is repeated automatically only when it surely was not
    /// processed.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] when there are no codes, more than
    /// [`MAX_IDS_REENVIO`](webhook::MAX_IDS_REENVIO), or a code that is not a
    /// UUID (nothing is sent); otherwise the same as
    /// [`consultar_webhook`](Self::consultar_webhook).
    pub async fn reenviar_callbacks(
        &self,
        tipo: TipoWebhookBanking,
        codigos: &[String],
    ) -> Result<ReenvioCallbacks> {
        webhook::quantos_reenviar(codigos.len())?;
        let oque = match tipo {
            TipoWebhookBanking::PixPagamento => "código da solicitação do Pix",
            TipoWebhookBanking::BoletoPagamento => "código da transação",
        };
        let codigos = codigos
            .iter()
            .map(|codigo| uuid(codigo, oque))
            .collect::<Result<Vec<_>>>()?;
        let request = ApiRequest::new(endpoint::banking::WEBHOOK_REENVIAR)
            .path_param("tipoWebhook", tipo.as_str().to_owned())
            .json(json!({ "codigoSolicitacao": codigos }))
            .retry(RetryMode::WhenNotProcessed);
        self.client.execute(request).await
    }
}

/// A code in the format of the API, a UUID, in lower case.
fn uuid(codigo: &str, oque: &str) -> Result<String> {
    let codigo = codigo.trim();
    if is_uuid(codigo) {
        Ok(codigo.to_ascii_lowercase())
    } else {
        Err(Error::InvalidInput(
            format!(
                "{oque} inválido: \"{codigo}\"; esperado um UUID (8-4-4-4-12 dígitos hexadecimais)"
            )
            .into(),
        ))
    }
}

/// The `endToEnd` of a Pix: up to 64 letters and digits.
fn end_to_end(id: &str) -> Result<String> {
    if (1..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric()) {
        Ok(id.to_owned())
    } else {
        Err(Error::InvalidInput(
            format!("endToEnd inválido: \"{id}\"; use até 64 letras e dígitos").into(),
        ))
    }
}
