//! Webhooks: the addresses Inter calls when something happens in the
//! account. Each API has its own:
//!
//! | API | Notifies | Operations |
//! | --- | --- | --- |
//! | Banking | Pix sent and boletos paid by the account, one webhook per [`TipoWebhookBanking`] | [`Banking::cadastrar_webhook`](crate::banking::Banking::cadastrar_webhook) |
//! | Cobrança | charges received, cancelled and expired | [`Cobranca::cadastrar_webhook`](crate::cobranca::Cobranca::cadastrar_webhook) |
//! | Pix | Pix charges (`cob`, `cobv`) paid, one webhook per Pix key | [`Pix::cadastrar_webhook`](crate::pix::Pix::cadastrar_webhook) |
//!
//! When the webhook's server does not accept a notification, Inter tries
//! again up to 4 times: 20, 30, 60 and 120 minutes later (Banking: 5, 10, 30
//! and 60).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize, Serializer};

use crate::error::{Error, Result};
use crate::serde_util::lenient;

/// The address of a webhook, checked as the documentation requires: it
/// starts with `https://`, has no spaces and names a server. The scheme is
/// sent in lower case; the rest, as typed.
///
/// ```
/// use inter_pj::webhook::WebhookUrl;
///
/// let url: WebhookUrl = "https://api.empresa.example/inter/pix".parse().unwrap();
/// assert_eq!(url.host(), "api.empresa.example");
/// assert!("http://api.empresa.example/inter/pix".parse::<WebhookUrl>().is_err());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WebhookUrl {
    texto: String,
    host: String,
}

impl WebhookUrl {
    /// Checks the address of a webhook.
    ///
    /// # Errors
    ///
    /// When the text is empty, has spaces, does not start with `https://` or
    /// is not a URL with a server.
    pub fn parse(raw: &str) -> Result<Self, WebhookUrlError> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err(WebhookUrlError::Vazia);
        }
        if raw.chars().any(char::is_whitespace) {
            return Err(WebhookUrlError::Espacos);
        }
        let texto = match raw.get(..8) {
            Some(esquema) if esquema.eq_ignore_ascii_case("https://") => {
                format!("https://{}", &raw[8..])
            }
            _ => return Err(WebhookUrlError::SemHttps),
        };
        let url = reqwest::Url::parse(&texto)
            .map_err(|err| WebhookUrlError::Invalida(motivo(&err.to_string())))?;
        let host = url
            .host_str()
            .filter(|host| !host.is_empty())
            .ok_or(WebhookUrlError::Invalida("falta o servidor"))?
            .to_owned();
        Ok(Self { texto, host })
    }

    /// The address as sent.
    pub fn as_str(&self) -> &str {
        &self.texto
    }

    /// The server (`api.empresa.example`), in lower case.
    pub fn host(&self) -> &str {
        &self.host
    }
}

/// The reasons of the URL parser, in Portuguese.
fn motivo(erro: &str) -> &'static str {
    match erro {
        "empty host" => "falta o servidor",
        "invalid port number" => "porta inválida",
        "invalid international domain name" | "invalid domain character" => {
            "nome do servidor inválido"
        }
        "invalid IPv4 address" | "invalid IPv6 address" => "endereço IP inválido",
        _ => "endereço malformado",
    }
}

impl FromStr for WebhookUrl {
    type Err = WebhookUrlError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        Self::parse(raw)
    }
}

impl fmt::Display for WebhookUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.texto)
    }
}

/// Serialized as sent (see [`as_str`](WebhookUrl::as_str)).
impl Serialize for WebhookUrl {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.texto)
    }
}

/// Why a text is not a valid webhook address.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum WebhookUrlError {
    /// Nothing was typed.
    #[error("informe a URL do webhook")]
    Vazia,
    /// `http://` or no scheme at all.
    #[error("a URL do webhook precisa começar com https://")]
    SemHttps,
    /// Spaces, tabs or line breaks.
    #[error("a URL do webhook não pode ter espaços")]
    Espacos,
    /// Not a URL with a server.
    #[error("URL do webhook inválida: {0}")]
    Invalida(&'static str),
}

/// What a webhook of the Banking API notifies (`tipoWebhook`). Each kind has
/// its own webhook.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TipoWebhookBanking {
    /// `pix-pagamento`: the Pix sent by the account.
    PixPagamento,
    /// `boleto-pagamento`: the boletos paid by the account.
    BoletoPagamento,
}

impl TipoWebhookBanking {
    /// Every kind.
    pub const TODOS: [Self; 2] = [Self::PixPagamento, Self::BoletoPagamento];

    /// The kind as the API names it (`pix-pagamento`).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PixPagamento => "pix-pagamento",
            Self::BoletoPagamento => "boleto-pagamento",
        }
    }

    /// What it notifies, in Portuguese.
    pub const fn descricao(self) -> &'static str {
        match self {
            Self::PixPagamento => "Pix enviados pela conta",
            Self::BoletoPagamento => "boletos pagos pela conta",
        }
    }
}

impl FromStr for TipoWebhookBanking {
    type Err = Error;

    fn from_str(raw: &str) -> Result<Self> {
        Self::TODOS
            .into_iter()
            .find(|tipo| tipo.as_str() == raw.trim())
            .ok_or_else(|| {
                Error::InvalidInput(
                    format!(
                        "tipo de webhook desconhecido: \"{raw}\"; use pix-pagamento ou boleto-pagamento"
                    )
                    .into(),
                )
            })
    }
}

impl fmt::Display for TipoWebhookBanking {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A registered webhook, as received.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Webhook {
    /// The address Inter calls.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub webhook_url: Option<String>,
    /// The Pix key the webhook is for (Pix API).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub chave: Option<String>,
    /// When it was registered (RFC 3339).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub criacao: Option<String>,
    /// When it was last changed (RFC 3339; Cobrança API).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub atualizacao: Option<String>,
}

/// The webhook of a lookup, or `None` for the `404` of an account without
/// one.
pub(crate) fn se_existir(resultado: Result<Webhook>) -> Result<Option<Webhook>> {
    match resultado {
        Ok(webhook) => Ok(Some(webhook)),
        Err(Error::Api(err)) if err.status == 404 => Ok(None),
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_need_https_a_server_and_no_spaces() {
        let url =
            WebhookUrl::parse(" https://api.empresa.example:8443/inter/pix?origem=inter ").unwrap();
        assert_eq!(
            url.as_str(),
            "https://api.empresa.example:8443/inter/pix?origem=inter"
        );
        assert_eq!(url.host(), "api.empresa.example");
        // The scheme is sent as the API's pattern asks; the rest is kept.
        let url = WebhookUrl::parse("HTTPS://Api.Empresa.Example/Inter").unwrap();
        assert_eq!(url.as_str(), "https://Api.Empresa.Example/Inter");
        assert_eq!(url.host(), "api.empresa.example");

        for (raw, erro) in [
            ("", WebhookUrlError::Vazia),
            ("   ", WebhookUrlError::Vazia),
            ("http://api.empresa.example", WebhookUrlError::SemHttps),
            ("api.empresa.example/inter", WebhookUrlError::SemHttps),
            ("ftp://api.empresa.example", WebhookUrlError::SemHttps),
            (
                "https://api.empresa.example/inter pix",
                WebhookUrlError::Espacos,
            ),
            ("https://", WebhookUrlError::Invalida("falta o servidor")),
            (
                "https://api.empresa.example:99999/",
                WebhookUrlError::Invalida("porta inválida"),
            ),
            (
                "https://api.empre%sa.example/",
                WebhookUrlError::Invalida("nome do servidor inválido"),
            ),
        ] {
            assert_eq!(WebhookUrl::parse(raw), Err(erro), "{raw:?}");
        }
    }

    #[test]
    fn messages_say_what_to_fix() {
        assert_eq!(
            WebhookUrl::parse("http://api.empresa.example")
                .unwrap_err()
                .to_string(),
            "a URL do webhook precisa começar com https://"
        );
        assert_eq!(
            WebhookUrl::parse("https://").unwrap_err().to_string(),
            "URL do webhook inválida: falta o servidor"
        );
    }

    #[test]
    fn banking_kinds_are_named_as_in_the_path() {
        for tipo in TipoWebhookBanking::TODOS {
            assert_eq!(tipo.as_str().parse::<TipoWebhookBanking>().unwrap(), tipo);
        }
        assert_eq!(
            "boleto-pagamento".parse::<TipoWebhookBanking>().unwrap(),
            TipoWebhookBanking::BoletoPagamento
        );
        assert!("pix".parse::<TipoWebhookBanking>().is_err());
    }

    #[test]
    fn a_missing_webhook_is_none() {
        use crate::error::ApiError;
        let nao_encontrado = Error::Api(Box::new(ApiError::new(
            404,
            "GET /cobranca/v3/cobrancas/webhook".to_owned(),
            b"",
        )));
        assert_eq!(se_existir(Err(nao_encontrado)).unwrap(), None);
        let proibido = Error::Api(Box::new(ApiError::new(
            403,
            "GET /cobranca/v3/cobrancas/webhook".to_owned(),
            b"",
        )));
        assert!(se_existir(Err(proibido)).is_err());
        assert_eq!(
            se_existir(Ok(Webhook::default())).unwrap(),
            Some(Webhook::default())
        );
    }
}
