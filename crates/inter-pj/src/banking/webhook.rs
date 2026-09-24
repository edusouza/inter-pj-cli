//! The webhooks of the Banking API (`/banking/v2/webhooks`): notifications
//! of the Pix sent and the boletos paid by the account.

use serde_json::json;

use super::Banking;
use crate::client::ApiRequest;
use crate::endpoint;
use crate::error::Result;
use crate::retry::RetryMode;
use crate::webhook::{self, TipoWebhookBanking, Webhook, WebhookUrl};

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
}
