//! The webhook of the Cobrança API (`/cobranca/v3/cobrancas/webhook`):
//! notifications of the charges received, cancelled and expired.

use serde_json::json;

use super::Cobranca;
use crate::client::ApiRequest;
use crate::endpoint;
use crate::error::Result;
use crate::retry::RetryMode;
use crate::webhook::{self, Webhook, WebhookUrl};

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
}
