//! The webhooks of the Pix API (`/pix/v2/webhook/{chave}`): notifications
//! of the Pix charges (`cob`, `cobv`) paid, one webhook per Pix key.

use serde_json::json;

use super::api::Pix;
use super::chave::ChavePix;
use crate::client::ApiRequest;
use crate::endpoint;
use crate::error::Result;
use crate::retry::RetryMode;
use crate::webhook::{self, Webhook, WebhookUrl};

impl Pix<'_> {
    /// Registers the webhook of a Pix key of the account (`PUT
    /// /pix/v2/webhook/{chave}`, scope `webhook.write`).
    ///
    /// The request is repeated automatically only when it surely was not
    /// processed; after an unknown outcome, look the webhook up with
    /// [`consultar_webhook`](Self::consultar_webhook).
    ///
    /// # Errors
    ///
    /// Failures to obtain a token or to send the request, and the API's
    /// error statuses.
    pub async fn cadastrar_webhook(&self, chave: &ChavePix, url: &WebhookUrl) -> Result<()> {
        let request = ApiRequest::new(endpoint::pix::WEBHOOK_CADASTRAR)
            .path_param("chave", no_caminho(chave))
            .json(json!({ "webhookUrl": url.as_str() }))
            .retry(RetryMode::WhenNotProcessed);
        self.client.execute_empty(request).await
    }

    /// The webhook of a Pix key, or `None` when there is none (`GET
    /// /pix/v2/webhook/{chave}`, scope `webhook.read`).
    ///
    /// # Errors
    ///
    /// The same as [`cadastrar_webhook`](Self::cadastrar_webhook), and
    /// failures to decode the answer.
    pub async fn consultar_webhook(&self, chave: &ChavePix) -> Result<Option<Webhook>> {
        let request = ApiRequest::new(endpoint::pix::WEBHOOK_CONSULTAR)
            .path_param("chave", no_caminho(chave));
        webhook::se_existir(self.client.execute(request).await)
    }

    /// Removes the webhook of a Pix key (`DELETE /pix/v2/webhook/{chave}`,
    /// scope `webhook.write`).
    ///
    /// # Errors
    ///
    /// The same as [`cadastrar_webhook`](Self::cadastrar_webhook); without
    /// a webhook, status `404`.
    pub async fn excluir_webhook(&self, chave: &ChavePix) -> Result<()> {
        let request = ApiRequest::new(endpoint::pix::WEBHOOK_EXCLUIR)
            .path_param("chave", no_caminho(chave))
            .retry(RetryMode::WhenNotProcessed);
        self.client.execute_empty(request).await
    }
}

/// The key in the path: phones without the `+`, as the documentation asks
/// (`5511912345678`).
fn no_caminho(chave: &ChavePix) -> String {
    match chave {
        ChavePix::Telefone(telefone) => telefone.trim_start_matches('+').to_owned(),
        outra => outra.as_str().to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phones_go_in_the_path_without_the_plus_sign() {
        let chave: ChavePix = "+55 (11) 91234-5678".parse().unwrap();
        assert_eq!(no_caminho(&chave), "5511912345678");
        for outra in [
            "pix@empresa.example",
            "12345678000195",
            "123e4567-e89b-12d3-a456-426614174000",
        ] {
            assert_eq!(no_caminho(&outra.parse().unwrap()), outra);
        }
    }
}
