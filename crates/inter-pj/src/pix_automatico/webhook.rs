//! The webhooks of Pix Automático (`/pix/v2/webhookrec` and
//! `/pix/v2/webhookcobr`): notifications of the changes of the recurrences
//! and of the recurring charges, and the bodies they carry.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{CobR, PixAutomatico, Rec};
use crate::client::ApiRequest;
use crate::endpoint::{self, Endpoint};
use crate::error::{Error, Result};
use crate::retry::RetryMode;
use crate::serde_util::lenient;
use crate::webhook::{self, Webhook, WebhookUrl};

/// The two webhooks of Pix Automático. Inter posts the notifications to the
/// registered address followed by `/rec` or `/cobr`, and tries again up to
/// 4 times (after 20, 30, 60 and 120 minutes) when it fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TipoWebhookPixAutomatico {
    /// `webhookrec`: the recurrences, posted to `{webhookUrl}/rec` as
    /// [`NotificacaoRecs`].
    Recorrencia,
    /// `webhookcobr`: the recurring charges, posted to `{webhookUrl}/cobr`
    /// as [`NotificacaoCobsR`].
    CobrancaRecorrente,
}

impl TipoWebhookPixAutomatico {
    /// Both webhooks.
    pub const TODOS: [Self; 2] = [Self::Recorrencia, Self::CobrancaRecorrente];

    /// The resource as the API names it (`webhookrec`).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Recorrencia => "webhookrec",
            Self::CobrancaRecorrente => "webhookcobr",
        }
    }

    /// What the webhook notifies, in words.
    pub const fn descricao(self) -> &'static str {
        match self {
            Self::Recorrencia => "recorrências do Pix Automático",
            Self::CobrancaRecorrente => "cobranças recorrentes do Pix Automático",
        }
    }

    /// The path Inter adds to the registered address (`/rec`).
    pub const fn sufixo(self) -> &'static str {
        match self {
            Self::Recorrencia => "/rec",
            Self::CobrancaRecorrente => "/cobr",
        }
    }

    const fn endpoints(self) -> [Endpoint; 3] {
        match self {
            Self::Recorrencia => [
                endpoint::pix_automatico::WEBHOOK_REC_CADASTRAR,
                endpoint::pix_automatico::WEBHOOK_REC_CONSULTAR,
                endpoint::pix_automatico::WEBHOOK_REC_EXCLUIR,
            ],
            Self::CobrancaRecorrente => [
                endpoint::pix_automatico::WEBHOOK_COBR_CADASTRAR,
                endpoint::pix_automatico::WEBHOOK_COBR_CONSULTAR,
                endpoint::pix_automatico::WEBHOOK_COBR_EXCLUIR,
            ],
        }
    }
}

impl fmt::Display for TipoWebhookPixAutomatico {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for TipoWebhookPixAutomatico {
    type Err = Error;

    fn from_str(raw: &str) -> Result<Self> {
        Self::TODOS
            .into_iter()
            .find(|tipo| tipo.as_str() == raw.trim())
            .ok_or_else(|| {
                Error::InvalidInput(
                    format!(
                        "webhook do Pix Automático desconhecido: \"{raw}\"; use webhookrec ou webhookcobr"
                    )
                    .into(),
                )
            })
    }
}

/// The body of a notification of recurrences (`POST {webhookUrl}/rec`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct NotificacaoRecs {
    /// The recurrences that changed: their status, history, end and
    /// activation.
    #[serde(default, deserialize_with = "lenient::vec")]
    pub recs: Vec<Rec>,
}

/// The body of a notification of recurring charges (`POST
/// {webhookUrl}/cobr`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct NotificacaoCobsR {
    /// The recurring charges that changed: their status, history, attempts
    /// and the Pix that paid them.
    #[serde(default, deserialize_with = "lenient::vec")]
    pub cobsr: Vec<CobR>,
}

impl PixAutomatico<'_> {
    /// Registers a webhook of Pix Automático (`PUT /pix/v2/webhookrec` or
    /// `/pix/v2/webhookcobr`, scope `webhookrec.write` or
    /// `webhookcobr.write`). The notifications go to the address followed by
    /// [`TipoWebhookPixAutomatico::sufixo`].
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
        tipo: TipoWebhookPixAutomatico,
        url: &WebhookUrl,
    ) -> Result<()> {
        let [cadastrar, _, _] = tipo.endpoints();
        let request = ApiRequest::new(cadastrar)
            .json(json!({ "webhookUrl": url.as_str() }))
            .retry(RetryMode::WhenNotProcessed);
        self.client.execute_empty(request).await
    }

    /// A webhook of Pix Automático, or `None` when there is none (`GET
    /// /pix/v2/webhookrec` or `/pix/v2/webhookcobr`, scope `webhookrec.read`
    /// or `webhookcobr.read`).
    ///
    /// # Errors
    ///
    /// The same as [`cadastrar_webhook`](Self::cadastrar_webhook), and
    /// failures to decode the answer.
    pub async fn consultar_webhook(
        &self,
        tipo: TipoWebhookPixAutomatico,
    ) -> Result<Option<Webhook>> {
        let [_, consultar, _] = tipo.endpoints();
        webhook::se_existir(self.client.execute(ApiRequest::new(consultar)).await)
    }

    /// Removes a webhook of Pix Automático (`DELETE /pix/v2/webhookrec` or
    /// `/pix/v2/webhookcobr`, scope `webhookrec.write` or
    /// `webhookcobr.write`).
    ///
    /// # Errors
    ///
    /// The same as [`cadastrar_webhook`](Self::cadastrar_webhook); without
    /// a webhook, status `404`.
    pub async fn excluir_webhook(&self, tipo: TipoWebhookPixAutomatico) -> Result<()> {
        let [_, _, excluir] = tipo.endpoints();
        let request = ApiRequest::new(excluir).retry(RetryMode::WhenNotProcessed);
        self.client.execute_empty(request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_are_the_resources_of_the_api() {
        for tipo in TipoWebhookPixAutomatico::TODOS {
            assert_eq!(
                tipo.as_str().parse::<TipoWebhookPixAutomatico>().unwrap(),
                tipo
            );
            let [cadastrar, consultar, excluir] = tipo.endpoints();
            let caminho = format!("/pix/v2/{tipo}");
            assert_eq!(cadastrar.path, caminho);
            assert_eq!(consultar.path, caminho);
            assert_eq!(excluir.path, caminho);
        }
        assert!("webhook".parse::<TipoWebhookPixAutomatico>().is_err());
        assert_eq!(
            TipoWebhookPixAutomatico::CobrancaRecorrente.sufixo(),
            "/cobr"
        );
    }
}
