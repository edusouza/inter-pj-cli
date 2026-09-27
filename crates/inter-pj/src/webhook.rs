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
//! and 60). Each API also keeps the history of the attempts
//! ([`Callback`]), latest first, and sends again, on request, the callbacks
//! of up to [`MAX_IDS_REENVIO`] operations at a time.

use std::fmt;
use std::future::Future;
use std::str::FromStr;

use chrono::{DateTime, FixedOffset, SecondsFormat};
use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;

use crate::error::{Error, Result};
use crate::serde_util::lenient;

/// Fewest callbacks per page of a history (`tamanhoPagina`).
pub const ITENS_POR_PAGINA_CALLBACKS_MINIMO: u32 = 10;

/// Most callbacks per page of a history (`tamanhoPagina`).
pub const ITENS_POR_PAGINA_CALLBACKS_MAXIMO: u32 = 50;

/// Most operations whose callbacks one request sends again.
pub const MAX_IDS_REENVIO: usize = 50;

/// Safety net against an API that never reports the last page.
const MAX_PAGINAS: u32 = 10_000;

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

/// Which callbacks of a history to read: the attempts made from `inicio` to
/// `fim`, optionally only those of one operation.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct FiltroCallbacks {
    /// Start of the period, inclusive.
    pub inicio: DateTime<FixedOffset>,
    /// End of the period, inclusive.
    pub fim: DateTime<FixedOffset>,
    /// Only the callbacks of this operation: the `endToEnd` of a Pix sent or
    /// the `codigoTransacao` of a boleto paid (Banking), the
    /// `codigoSolicitacao` of a charge (Cobrança) or the txid of a Pix
    /// charge (Pix). Checked before the request.
    pub identificador: Option<String>,
}

impl FiltroCallbacks {
    /// The attempts made from `inicio` to `fim`.
    ///
    /// # Errors
    ///
    /// When the period ends before it starts.
    pub fn new(
        inicio: DateTime<FixedOffset>,
        fim: DateTime<FixedOffset>,
    ) -> Result<Self, PeriodoCallbacksError> {
        if fim < inicio {
            return Err(PeriodoCallbacksError);
        }
        Ok(Self {
            inicio,
            fim,
            identificador: None,
        })
    }

    pub(crate) fn query(&self) -> [(&'static str, String); 2] {
        [
            ("dataHoraInicio", data_hora(self.inicio)),
            ("dataHoraFim", data_hora(self.fim)),
        ]
    }

    /// The identifier of the filter, trimmed, when there is one.
    pub(crate) fn identificador(&self) -> Option<&str> {
        self.identificador
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
    }
}

/// A period that ends before it starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("o período termina antes de começar")]
pub struct PeriodoCallbacksError;

/// `2026-09-01T00:00:00-03:00`, as the documentation accepts
/// (`yyyy-MM-dd'T'HH:mm[:ss][.SSS]XXX`).
fn data_hora(momento: DateTime<FixedOffset>) -> String {
    momento.to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// A page of a history of callbacks, as received.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PaginaCallbacks {
    /// Callbacks of every page.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::u64"
    )]
    pub total_elementos: Option<u64>,
    /// Number of pages.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::u64"
    )]
    pub total_paginas: Option<u64>,
    /// Whether this is the first page.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::bool"
    )]
    pub primeira_pagina: Option<bool>,
    /// Whether this is the last page.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::bool"
    )]
    pub ultima_pagina: Option<bool>,
    /// The attempts, latest first.
    #[serde(default, deserialize_with = "lenient::vec")]
    pub data: Vec<Callback>,
}

impl PaginaCallbacks {
    /// Whether pages after `pagina` remain. Without the number of pages, a
    /// page of `itens_por_pagina` callbacks means there may be more.
    pub fn tem_mais(&self, pagina: u32, itens_por_pagina: u32) -> bool {
        if let Some(ultima) = self.ultima_pagina {
            return !ultima;
        }
        match self.total_paginas {
            Some(paginas) => u64::from(pagina) + 1 < paginas,
            None => self.data.len() as u64 >= u64::from(itens_por_pagina),
        }
    }
}

/// An attempt to send a callback to a webhook, as received.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Callback {
    /// The address of the webhook when the attempt was made.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub webhook_url: Option<String>,
    /// What was sent, as sent: its format depends on the API and the kind
    /// of webhook. See [`valores`](Self::valores).
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub payload: Value,
    /// Which attempt this was, from 1.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::u64"
    )]
    pub numero_tentativa: Option<u64>,
    /// When the attempt was made (RFC 3339; Cobrança and Pix).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub data_hora_disparo: Option<String>,
    /// When the attempt was made (RFC 3339; Banking).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub data_envio: Option<String>,
    /// Whether the webhook's server accepted it.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::bool"
    )]
    pub sucesso: Option<bool>,
    /// The status the webhook's server answered.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::u64"
    )]
    pub http_status: Option<u64>,
    /// Why the attempt failed.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub mensagem_erro: Option<String>,
}

impl Callback {
    /// When the attempt was made, whatever the API calls it.
    pub fn disparo(&self) -> Option<&str> {
        self.data_hora_disparo
            .as_deref()
            .or(self.data_envio.as_deref())
    }

    /// Every value of the field `nome` in the payload, in order and without
    /// repetitions: in the payload itself, in the items of a list, and in the
    /// lists and objects of those (`{"pix": [{"txid": ...}]}`). Numbers come
    /// as text.
    ///
    /// ```
    /// use inter_pj::webhook::Callback;
    ///
    /// let callback: Callback = serde_json::from_value(serde_json::json!({
    ///     "payload": {"pix": [{"txid": "7978c0c97ea847e78e8849634473c1f1"}]}
    /// }))
    /// .unwrap();
    /// assert_eq!(callback.valores("txid"), ["7978c0c97ea847e78e8849634473c1f1"]);
    /// ```
    pub fn valores(&self, nome: &str) -> Vec<String> {
        let mut valores = Vec::new();
        coletar(&self.payload, nome, 0, &mut valores);
        valores
    }
}

fn coletar(valor: &Value, nome: &str, profundidade: usize, valores: &mut Vec<String>) {
    if profundidade > 4 {
        return;
    }
    match valor {
        Value::Object(objeto) => {
            let encontrado = match objeto.get(nome) {
                Some(Value::String(texto)) if !texto.trim().is_empty() => {
                    Some(texto.trim().to_owned())
                }
                Some(Value::Number(numero)) => Some(numero.to_string()),
                _ => None,
            };
            if let Some(encontrado) = encontrado
                && !valores.contains(&encontrado)
            {
                valores.push(encontrado);
            }
            for (chave, filho) in objeto {
                if chave != nome {
                    coletar(filho, nome, profundidade + 1, valores);
                }
            }
        }
        Value::Array(itens) => {
            for item in itens {
                coletar(item, nome, profundidade + 1, valores);
            }
        }
        _ => {}
    }
}

/// The answer of a retry of callbacks, as received.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ReenvioCallbacks {
    /// The identifiers whose callbacks will be sent again; the others were
    /// not found.
    #[serde(default, deserialize_with = "lenient::vec")]
    pub found_ids: Vec<String>,
}

/// The page size of a history, checked.
pub(crate) fn itens_por_pagina(itens: Option<u32>) -> Result<Option<u32>> {
    match itens {
        Some(itens)
            if !(ITENS_POR_PAGINA_CALLBACKS_MINIMO..=ITENS_POR_PAGINA_CALLBACKS_MAXIMO)
                .contains(&itens) =>
        {
            Err(Error::InvalidInput(
                format!(
                    "callbacks por página: de {ITENS_POR_PAGINA_CALLBACKS_MINIMO} a {ITENS_POR_PAGINA_CALLBACKS_MAXIMO}"
                )
                .into(),
            ))
        }
        itens => Ok(itens),
    }
}

/// How many operations a retry may name: from 1 to [`MAX_IDS_REENVIO`].
pub(crate) fn quantos_reenviar(quantos: usize) -> Result<()> {
    if (1..=MAX_IDS_REENVIO).contains(&quantos) {
        Ok(())
    } else {
        Err(Error::InvalidInput(
            format!("informe de 1 a {MAX_IDS_REENVIO} identificadores por reenvio, não {quantos}")
                .into(),
        ))
    }
}

/// Every callback of a history, reading pages of
/// [`ITENS_POR_PAGINA_CALLBACKS_MAXIMO`] until the last one.
pub(crate) async fn todos<F, Fut>(nome: &str, mut pagina: F) -> Result<Vec<Callback>>
where
    F: FnMut(u32) -> Fut,
    Fut: Future<Output = Result<PaginaCallbacks>>,
{
    let mut callbacks = Vec::new();
    let mut numero = 0;
    loop {
        let mut atual = pagina(numero).await?;
        let recebidos = atual.data.len();
        let tem_mais = atual.tem_mais(numero, ITENS_POR_PAGINA_CALLBACKS_MAXIMO);
        callbacks.append(&mut atual.data);
        tracing::info!("{nome}: página {} com {recebidos}", numero + 1);
        let completo = atual
            .total_elementos
            .is_some_and(|total| callbacks.len() as u64 >= total);
        if completo || recebidos == 0 || !tem_mais || numero + 1 >= MAX_PAGINAS {
            return Ok(callbacks);
        }
        numero += 1;
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

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

    #[test]
    fn periods_are_sent_with_their_offset() {
        let inicio = DateTime::parse_from_rfc3339("2026-09-01T00:00:00-03:00").unwrap();
        let fim = DateTime::parse_from_rfc3339("2026-09-02T00:00:00Z").unwrap();
        let filtro = FiltroCallbacks::new(inicio, fim).unwrap();
        assert_eq!(
            filtro.query(),
            [
                ("dataHoraInicio", "2026-09-01T00:00:00-03:00".to_owned()),
                ("dataHoraFim", "2026-09-02T00:00:00Z".to_owned()),
            ]
        );
        assert_eq!(
            FiltroCallbacks::new(fim, inicio),
            Err(PeriodoCallbacksError)
        );
        let mut filtro = filtro;
        filtro.identificador = Some("  ".to_owned());
        assert_eq!(filtro.identificador(), None);
        filtro.identificador = Some(" abc ".to_owned());
        assert_eq!(filtro.identificador(), Some("abc"));
    }

    #[test]
    fn pages_end_where_the_api_says() {
        let pagina = |ultima: Option<bool>, paginas: Option<u64>, itens: usize| PaginaCallbacks {
            total_paginas: paginas,
            ultima_pagina: ultima,
            data: vec![Callback::default(); itens],
            ..PaginaCallbacks::default()
        };
        assert!(!pagina(Some(true), Some(9), 50).tem_mais(0, 50));
        assert!(pagina(Some(false), None, 1).tem_mais(0, 50));
        assert!(pagina(None, Some(2), 50).tem_mais(0, 50));
        assert!(!pagina(None, Some(2), 50).tem_mais(1, 50));
        assert!(pagina(None, None, 50).tem_mais(0, 50));
        assert!(!pagina(None, None, 49).tem_mais(0, 50));
    }

    #[test]
    fn values_are_found_wherever_the_payload_keeps_them() {
        let callback = |payload: Value| Callback {
            payload,
            ..Callback::default()
        };
        // Cobrança: a list of charges.
        let cobranca = callback(json!([
            {"codigoSolicitacao": "0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d", "situacao": "RECEBIDO"},
            {"codigoSolicitacao": "0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d"},
            {"codigoSolicitacao": "1c8f5d2b-6e4a-4b3c-8d9e-8f7a6b5c4d3e"}
        ]));
        assert_eq!(
            cobranca.valores("codigoSolicitacao"),
            [
                "0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d",
                "1c8f5d2b-6e4a-4b3c-8d9e-8f7a6b5c4d3e"
            ]
        );
        // Pix: the Banco Central's `{"pix": [...]}`.
        let pix = callback(
            json!({"pix": [{"endToEndId": "E00416968202609241310abcdEFGH123", "txid": "7978c0c97ea847e78e8849634473c1f1", "valor": "37.00"}]}),
        );
        assert_eq!(pix.valores("txid"), ["7978c0c97ea847e78e8849634473c1f1"]);
        // Banking: one object; numbers come as text.
        let banking = callback(
            json!({"codigoTransacao": "8bbdede4-35db-4ec9-b652-e176841e62c8", "numero": 42}),
        );
        assert_eq!(banking.valores("numero"), ["42"]);
        assert!(banking.valores("txid").is_empty());
        assert!(callback(Value::Null).valores("txid").is_empty());
        assert!(callback(json!({})).valores("txid").is_empty());
    }

    #[test]
    fn the_time_of_an_attempt_has_two_names() {
        let cobranca: Callback =
            serde_json::from_value(json!({"dataHoraDisparo": "2026-09-24T14:15:22Z"})).unwrap();
        let banking: Callback =
            serde_json::from_value(json!({"dataEnvio": "2026-09-24T14:15:23Z"})).unwrap();
        assert_eq!(cobranca.disparo(), Some("2026-09-24T14:15:22Z"));
        assert_eq!(banking.disparo(), Some("2026-09-24T14:15:23Z"));
    }

    #[test]
    fn limits_are_checked_before_anything_is_sent() {
        assert_eq!(itens_por_pagina(None).unwrap(), None);
        assert_eq!(itens_por_pagina(Some(10)).unwrap(), Some(10));
        assert_eq!(itens_por_pagina(Some(50)).unwrap(), Some(50));
        assert!(itens_por_pagina(Some(9)).is_err());
        assert!(itens_por_pagina(Some(51)).is_err());
        assert!(quantos_reenviar(1).is_ok());
        assert!(quantos_reenviar(50).is_ok());
        assert!(quantos_reenviar(0).is_err());
        assert_eq!(
            quantos_reenviar(51).unwrap_err().to_string(),
            "informe de 1 a 50 identificadores por reenvio, não 51"
        );
    }
}
