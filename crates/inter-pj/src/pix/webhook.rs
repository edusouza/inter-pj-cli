//! The webhooks of the Pix API (`/pix/v2/webhook/{chave}`): notifications
//! of the Pix charges (`cob`, `cobv`) paid, one webhook per Pix key, the
//! history of the callbacks and their retry.

use serde_json::json;

use super::api::Pix;
use super::chave::ChavePix;
use super::txid::Txid;
use crate::client::ApiRequest;
use crate::endpoint;
use crate::error::{Error, Result};
use crate::retry::RetryMode;
use crate::webhook::{
    self, Callback, FiltroCallbacks, ITENS_POR_PAGINA_CALLBACKS_MAXIMO, PaginaCallbacks,
    ReenvioCallbacks, Webhook, WebhookUrl,
};

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

    /// One page of the history of the callbacks, latest first (`GET
    /// /pix/v2/webhook/callbacks`, scope `webhook.read`).
    ///
    /// `pagina` starts at 0. Pages have from 10 to 50 callbacks, 20 without
    /// `itens_por_pagina`. The identifier of the filter is the txid of a
    /// charge.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] when the page size or the txid is invalid
    /// (nothing is sent); otherwise the same as
    /// [`consultar_webhook`](Self::consultar_webhook).
    pub async fn listar_callbacks(
        &self,
        filtro: &FiltroCallbacks,
        pagina: u32,
        itens_por_pagina: Option<u32>,
    ) -> Result<PaginaCallbacks> {
        let mut request = ApiRequest::new(endpoint::pix::WEBHOOK_CALLBACKS)
            .queries(filtro.query())
            .query("pagina", pagina.to_string());
        if let Some(itens) = webhook::itens_por_pagina(itens_por_pagina)? {
            request = request.query("tamanhoPagina", itens.to_string());
        }
        if let Some(id) = filtro.identificador() {
            let txid: Txid = id
                .parse()
                .map_err(|err| Error::InvalidInput(Box::new(err)))?;
            request = request.query("txid", txid.as_str().to_owned());
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
    /// [`MAX_IDS_REENVIO`](webhook::MAX_IDS_REENVIO) charges of a Pix key, by
    /// their txids (`POST /pix/v2/webhook/callbacks/retry`, scope
    /// `webhook.write`). The answer names those found.
    ///
    /// The key goes in the body as the DICT keeps it (phones with the `+`).
    /// The request is repeated automatically only when it surely was not
    /// processed.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] when there are no txids or more than
    /// [`MAX_IDS_REENVIO`](webhook::MAX_IDS_REENVIO) (nothing is sent);
    /// otherwise the same as [`consultar_webhook`](Self::consultar_webhook).
    pub async fn reenviar_callbacks(
        &self,
        chave: &ChavePix,
        txids: &[Txid],
    ) -> Result<ReenvioCallbacks> {
        webhook::quantos_reenviar(txids.len())?;
        let request = ApiRequest::new(endpoint::pix::WEBHOOK_REENVIAR)
            .json(json!({ "txId": txids, "chavePix": chave.as_str() }))
            .retry(RetryMode::WhenNotProcessed);
        self.client.execute(request).await
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
