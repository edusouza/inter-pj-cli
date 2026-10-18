use std::fmt;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{TimeDelta, Utc};
use reqwest::header::{ACCEPT, HeaderValue};
use secrecy::ExposeSecret;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use url::{Host, Url};

use crate::auth::{self, AccessToken, TokenManager, TokenStore};
use crate::banking::Banking;
use crate::credentials::Credentials;
use crate::endpoint::{self, Endpoint, Method};
use crate::environment::Environment;
use crate::error::{ApiError, Error, Result};
use crate::identity::{ClientIdentity, IdentityError};
use crate::retry::{self, RetryMode, RetryPolicy};
use crate::scope::ScopeSet;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Token lifetime documented by Inter, used when the response omits `expires_in`.
const DEFAULT_TOKEN_LIFETIME_SECS: i64 = 3600;

/// Client for the Inter Empresas APIs.
///
/// Cheap to clone: clones share the connection pool and the token cache.
#[derive(Clone)]
pub struct InterClient {
    inner: Arc<Inner>,
}

struct Inner {
    http: reqwest::Client,
    base_url: Url,
    environment: Option<Environment>,
    credentials: Credentials,
    conta_corrente: Option<HeaderValue>,
    tokens: TokenManager,
    retry: RetryPolicy,
}

impl InterClient {
    /// Starts building a client.
    pub fn builder() -> InterClientBuilder {
        InterClientBuilder::default()
    }

    /// Environment the client was built for, if one was set.
    pub fn environment(&self) -> Option<Environment> {
        self.inner.environment
    }

    /// Base URL requests are sent to.
    pub fn base_url(&self) -> &str {
        self.inner.base_url.as_str().trim_end_matches('/')
    }

    /// Key under which this client's tokens are stored (see [`auth::cache_key`]).
    pub fn token_cache_key(&self) -> &str {
        self.inner.tokens.key()
    }

    /// Operations of the Banking API (balance, statements, payments...).
    pub fn banking(&self) -> Banking<'_> {
        Banking::new(self)
    }

    /// Returns an access token covering `scopes`, reusing a cached one when possible.
    ///
    /// # Errors
    ///
    /// Fails when the token endpoint rejects the credentials, does not grant
    /// every scope in `scopes` or cannot be reached.
    pub async fn access_token(&self, scopes: &ScopeSet) -> Result<AccessToken> {
        self.token(scopes, false).await
    }

    /// Requests a brand new access token for `scopes`, ignoring cached ones.
    ///
    /// # Errors
    ///
    /// Same as [`access_token`](Self::access_token).
    pub async fn renew_access_token(&self, scopes: &ScopeSet) -> Result<AccessToken> {
        self.token(scopes, true).await
    }

    async fn token(&self, scopes: &ScopeSet, force_new: bool) -> Result<AccessToken> {
        self.inner
            .tokens
            .token(scopes, force_new, |requested| {
                self.request_token(requested, scopes)
            })
            .await
    }

    /// Calls `POST /oauth/v2/token` with the client credentials.
    async fn request_token(&self, requested: ScopeSet, required: &ScopeSet) -> Result<AccessToken> {
        let operation = endpoint::TOKEN.to_string();
        let url = self.url_for(&endpoint::TOKEN, &[])?;
        let scope = requested.to_string();
        let form = [
            ("client_id", self.inner.credentials.client_id()),
            (
                "client_secret",
                self.inner.credentials.client_secret().expose_secret(),
            ),
            ("grant_type", "client_credentials"),
            ("scope", scope.as_str()),
        ];

        // Asking for a token has no side effects, so it is always safe to retry.
        let response = self
            .send(&operation, RetryMode::Idempotent, || {
                self.inner
                    .http
                    .post(url.clone())
                    .header(ACCEPT, "application/json")
                    .form(&form)
            })
            .await?;
        let status = response.status();
        let body = response.bytes().await?;

        if !status.is_success() {
            return Err(Error::Auth(Box::new(ApiError::new(
                status.as_u16(),
                operation,
                &body,
            ))));
        }

        // Never include serde's message here: it may quote the token itself.
        let parsed: TokenResponse = serde_json::from_slice(&body).map_err(|_| Error::Decode {
            operation: operation.clone(),
            message: "resposta do endpoint de token em formato inesperado".to_owned(),
        })?;

        let granted = parsed
            .scope
            .as_deref()
            .map_or_else(|| requested.clone(), ScopeSet::parse_lenient);
        if !granted.is_superset(required) {
            let mut error = ApiError::new(status.as_u16(), operation, b"");
            error.note = Some(format!(
                "o token foi emitido sem os escopos {}; habilite-os na integração (Internet Banking PJ)",
                granted.missing_from(required)
            ));
            return Err(Error::Auth(Box::new(error)));
        }

        let lifetime = parsed
            .expires_in
            .as_ref()
            .and_then(seconds)
            .unwrap_or(DEFAULT_TOKEN_LIFETIME_SECS);
        let expires_at = Utc::now() + TimeDelta::seconds(lifetime);
        Ok(AccessToken::new(parsed.access_token, granted, expires_at))
    }

    /// Sends an API request and decodes the JSON response.
    pub(crate) async fn execute<T: DeserializeOwned>(&self, request: ApiRequest) -> Result<T> {
        let operation = request.endpoint.to_string();
        let required = request.endpoint.scope_set();
        let url = self.url_for(&request.endpoint, &request.path_params)?;

        let mut token = self.access_token(&required).await?;
        let mut renewed = false;
        loop {
            let response = self
                .send(&operation, request.retry, || {
                    let mut builder = self
                        .inner
                        .http
                        .request(request.endpoint.method.to_reqwest(), url.clone())
                        .bearer_auth(token.secret().expose_secret())
                        .header(ACCEPT, "application/json");
                    if let Some(conta) = &self.inner.conta_corrente {
                        builder = builder.header("x-conta-corrente", conta.clone());
                    }
                    for (name, value) in &request.headers {
                        builder = builder.header(*name, value.as_str());
                    }
                    if !request.query.is_empty() {
                        builder = builder.query(&request.query);
                    }
                    if let Some(body) = &request.body {
                        builder = builder.json(body);
                    }
                    builder
                })
                .await?;
            let status = response.status();

            if status == reqwest::StatusCode::UNAUTHORIZED && !renewed {
                tracing::debug!(
                    "token rejeitado (401); renovando e repetindo a requisição uma vez"
                );
                self.inner.tokens.invalidate(&token).await;
                token = self.renew_access_token(&required).await?;
                renewed = true;
                continue;
            }

            let body = response.bytes().await?;
            if !status.is_success() {
                return Err(Error::Api(Box::new(ApiError::new(
                    status.as_u16(),
                    operation,
                    &body,
                ))));
            }
            return serde_json::from_slice(&body).map_err(|err| Error::Decode {
                operation,
                message: err.to_string(),
            });
        }
    }

    /// Sends the request built by `build`, retrying transient failures as
    /// `mode` allows and the client's [`RetryPolicy`] says. Returns the last
    /// response, whatever its status.
    async fn send(
        &self,
        operation: &str,
        mode: RetryMode,
        build: impl Fn() -> reqwest::RequestBuilder,
    ) -> Result<reqwest::Response> {
        let policy = &self.inner.retry;
        let mut attempt = 1;
        loop {
            let started = Instant::now();
            let (delay, reason) = match build().send().await {
                Ok(response) => {
                    let status = response.status();
                    log_response(operation, status, started);
                    if !mode.retries_status(status) {
                        return Ok(response);
                    }
                    let asked = retry::retry_after(response.headers(), Utc::now());
                    match policy.delay(attempt, asked, retry::jitter()) {
                        Some(delay) => (delay, format!("resposta {}", status.as_u16())),
                        None => return Ok(response),
                    }
                }
                Err(err) => {
                    if !mode.retries_transport(&err) {
                        return Err(err.into());
                    }
                    match policy.delay(attempt, None, retry::jitter()) {
                        Some(delay) => (
                            delay,
                            if err.is_timeout() {
                                "tempo esgotado".to_owned()
                            } else {
                                "falha de conexão".to_owned()
                            },
                        ),
                        None => return Err(err.into()),
                    }
                }
            };
            attempt += 1;
            tracing::info!(
                "{operation}: {reason}; tentativa {attempt} de {} em {:.1} s",
                policy.max_attempts(),
                delay.as_secs_f64()
            );
            tokio::time::sleep(delay).await;
        }
    }

    fn url_for(&self, endpoint: &Endpoint, params: &[(&'static str, String)]) -> Result<Url> {
        let mut url = self.inner.base_url.clone();
        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|()| Error::Config("URL base inválida".to_owned()))?;
            segments.pop_if_empty();
            for segment in endpoint.path.trim_start_matches('/').split('/') {
                match segment.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
                    Some(name) => {
                        let value = params
                            .iter()
                            .find(|(param, _)| *param == name)
                            .map(|(_, value)| value.as_str())
                            .ok_or_else(|| {
                                Error::Config(format!("parâmetro de caminho ausente: {name}"))
                            })?;
                        segments.push(value);
                    }
                    None => {
                        segments.push(segment);
                    }
                }
            }
        }
        Ok(url)
    }
}

impl fmt::Debug for InterClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InterClient")
            .field("base_url", &self.base_url())
            .field("environment", &self.inner.environment)
            .field("credentials", &self.inner.credentials)
            .field(
                "conta_corrente",
                &self.inner.conta_corrente.as_ref().map(|_| "[REDACTED]"),
            )
            .finish_non_exhaustive()
    }
}

/// A request to one of the [`Endpoint`]s.
#[derive(Debug)]
pub(crate) struct ApiRequest {
    endpoint: Endpoint,
    path_params: Vec<(&'static str, String)>,
    query: Vec<(&'static str, String)>,
    headers: Vec<(&'static str, String)>,
    body: Option<serde_json::Value>,
    retry: RetryMode,
}

impl ApiRequest {
    /// A request without parameters; only `GET` requests are retried.
    pub(crate) fn new(endpoint: Endpoint) -> Self {
        Self {
            endpoint,
            path_params: Vec::new(),
            query: Vec::new(),
            headers: Vec::new(),
            body: None,
            retry: if endpoint.method == Method::Get {
                RetryMode::Idempotent
            } else {
                RetryMode::Never
            },
        }
    }

    /// Value of a `{name}` placeholder of the path, percent-encoded when sent.
    pub(crate) fn path_param(mut self, name: &'static str, value: String) -> Self {
        self.path_params.push((name, value));
        self
    }

    /// Extra header, besides authorization and `x-conta-corrente`.
    pub(crate) fn header(mut self, name: &'static str, value: String) -> Self {
        self.headers.push((name, value));
        self
    }

    /// JSON body.
    pub(crate) fn json(mut self, body: serde_json::Value) -> Self {
        self.body = Some(body);
        self
    }

    pub(crate) fn query(mut self, name: &'static str, value: String) -> Self {
        self.query.push((name, value));
        self
    }

    pub(crate) fn queries(
        mut self,
        pairs: impl IntoIterator<Item = (&'static str, String)>,
    ) -> Self {
        self.query.extend(pairs);
        self
    }

    /// Overrides when the request may be repeated.
    pub(crate) fn retry(mut self, mode: RetryMode) -> Self {
        self.retry = mode;
        self
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    expires_in: Option<serde_json::Value>,
    #[serde(default)]
    scope: Option<String>,
}

#[allow(clippy::cast_possible_truncation)] // bounded before the cast
fn seconds(value: &serde_json::Value) -> Option<i64> {
    let seconds = match value {
        serde_json::Value::Number(n) => n.as_f64()?,
        serde_json::Value::String(s) => s.trim().parse().ok()?,
        _ => return None,
    };
    (seconds.is_finite() && (0.0..=86_400.0 * 365.0).contains(&seconds)).then_some(seconds as i64)
}

fn log_response(operation: &str, status: reqwest::StatusCode, started: Instant) {
    tracing::debug!(
        "{operation} → {} ({} ms)",
        status.as_u16(),
        started.elapsed().as_millis()
    );
}

/// Builder for [`InterClient`].
#[derive(Debug)]
pub struct InterClientBuilder {
    environment: Option<Environment>,
    base_url: Option<String>,
    credentials: Option<Credentials>,
    identity: Option<ClientIdentity>,
    conta_corrente: Option<String>,
    token_store: Option<Arc<dyn TokenStore>>,
    additional_scopes: ScopeSet,
    root_certificates: Option<Vec<u8>>,
    timeout: Duration,
    connect_timeout: Duration,
    user_agent: String,
    retry: RetryPolicy,
}

impl Default for InterClientBuilder {
    fn default() -> Self {
        Self {
            environment: None,
            base_url: None,
            credentials: None,
            identity: None,
            conta_corrente: None,
            token_store: None,
            additional_scopes: ScopeSet::new(),
            root_certificates: None,
            timeout: DEFAULT_TIMEOUT,
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            user_agent: concat!("inter-pj/", env!("CARGO_PKG_VERSION")).to_owned(),
            retry: RetryPolicy::default(),
        }
    }
}

impl InterClientBuilder {
    /// Environment to talk to. Required unless [`base_url`](Self::base_url) is set.
    #[must_use]
    pub fn environment(mut self, environment: Environment) -> Self {
        self.environment = Some(environment);
        self
    }

    /// Overrides the base URL (tests, proxies). It must use `https`, except
    /// for loopback addresses (`localhost`, `127.0.0.1`, `::1`).
    #[must_use]
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    /// OAuth client credentials of the integration. Required.
    #[must_use]
    pub fn credentials(mut self, credentials: Credentials) -> Self {
        self.credentials = Some(credentials);
        self
    }

    /// mTLS certificate and private key of the integration. Required.
    #[must_use]
    pub fn identity(mut self, identity: ClientIdentity) -> Self {
        self.identity = Some(identity);
        self
    }

    /// Checking account used in every request (`x-conta-corrente` header).
    ///
    /// Only needed when the integration is associated with more than one
    /// account. Digits only, including the check digit, without leading zeros.
    #[must_use]
    pub fn conta_corrente(mut self, conta_corrente: impl Into<String>) -> Self {
        self.conta_corrente = Some(conta_corrente.into());
        self
    }

    /// Persistent token cache shared between processes.
    #[must_use]
    pub fn token_store(mut self, store: Arc<dyn TokenStore>) -> Self {
        self.token_store = Some(store);
        self
    }

    /// Scopes requested in every token, besides the ones each operation needs.
    ///
    /// Lets several operations share one token (the token endpoint accepts
    /// only five calls per minute). All of them must be enabled for the
    /// integration, otherwise token requests fail.
    #[must_use]
    pub fn additional_scopes(mut self, scopes: ScopeSet) -> Self {
        self.additional_scopes = scopes;
        self
    }

    /// Trusts **only** the given PEM root certificate(s) to verify the server,
    /// instead of the operating system's trust store. Meant for tests and
    /// TLS-inspecting proxies.
    #[must_use]
    pub fn root_certificates_pem(mut self, pem: impl Into<Vec<u8>>) -> Self {
        self.root_certificates = Some(pem.into());
        self
    }

    /// Total timeout of each request (default: 30 s).
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Overrides the `User-Agent` header.
    #[must_use]
    pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = user_agent.into();
        self
    }

    /// How transient failures of idempotent requests are retried (default:
    /// [`RetryPolicy::default`], three attempts).
    #[must_use]
    pub fn retry_policy(mut self, policy: RetryPolicy) -> Self {
        self.retry = policy;
        self
    }

    /// Validates the configuration and builds the client.
    ///
    /// # Errors
    ///
    /// Fails when a required setting is missing (environment or base URL,
    /// credentials, identity), a setting is invalid (base URL, checking
    /// account, root certificates) or the TLS backend rejects the identity.
    pub fn build(self) -> Result<InterClient> {
        let base_url = match (&self.base_url, self.environment) {
            (Some(url), _) => parse_base_url(url)?,
            (None, Some(environment)) => parse_base_url(environment.base_url())?,
            (None, None) => {
                return Err(Error::Config(
                    "ambiente não definido: informe sandbox ou produção".to_owned(),
                ));
            }
        };
        let credentials = self.credentials.ok_or_else(|| {
            Error::Config("credenciais não informadas (client_id e client_secret)".to_owned())
        })?;
        if credentials.client_id().trim().is_empty() {
            return Err(Error::Config("client_id vazio".to_owned()));
        }
        if credentials
            .client_secret()
            .expose_secret()
            .trim()
            .is_empty()
        {
            return Err(Error::Config("client_secret vazio".to_owned()));
        }
        let identity = self.identity.ok_or_else(|| {
            Error::Config("certificado e chave privada (mTLS) não informados".to_owned())
        })?;
        let conta_corrente = self
            .conta_corrente
            .as_deref()
            .map(parse_conta_corrente)
            .transpose()?;

        let loopback = is_loopback(&base_url);
        let mut http = reqwest::Client::builder()
            .identity(identity.to_reqwest()?)
            .user_agent(self.user_agent)
            .timeout(self.timeout)
            .connect_timeout(self.connect_timeout)
            .redirect(reqwest::redirect::Policy::none());
        if base_url.scheme() == "https" {
            http = http.https_only(true);
        }
        if loopback {
            http = http.no_proxy();
        }
        if let Some(pem) = &self.root_certificates {
            let certificates = reqwest::Certificate::from_pem_bundle(pem)
                .map_err(|err| Error::Config(format!("certificado raiz inválido: {err}")))?;
            http = http.tls_certs_only(certificates);
        }
        let http = http
            .build()
            .map_err(|err| Error::Identity(IdentityError::Tls(error_chain(&err))))?;

        let key = auth::cache_key(base_url.as_str(), credentials.client_id());
        Ok(InterClient {
            inner: Arc::new(Inner {
                http,
                base_url,
                environment: self.environment,
                credentials,
                conta_corrente,
                tokens: TokenManager::new(key, self.token_store, self.additional_scopes),
                retry: self.retry,
            }),
        })
    }
}

fn parse_base_url(raw: &str) -> Result<Url> {
    let url = Url::parse(raw.trim())
        .map_err(|err| Error::Config(format!("URL base inválida ({err})")))?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(Error::Config(
            "URL base não pode conter usuário ou senha".to_owned(),
        ));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(Error::Config(
            "URL base não pode conter query ou fragmento".to_owned(),
        ));
    }
    match url.scheme() {
        "https" => Ok(url),
        "http" if is_loopback(&url) => Ok(url),
        _ => Err(Error::Config(
            "URL base deve usar https (http só é aceito em endereços locais)".to_owned(),
        )),
    }
}

fn is_loopback(url: &Url) -> bool {
    match url.host() {
        Some(Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        Some(Host::Ipv4(ip)) => IpAddr::V4(ip).is_loopback(),
        Some(Host::Ipv6(ip)) => IpAddr::V6(ip).is_loopback(),
        None => false,
    }
}

fn parse_conta_corrente(raw: &str) -> Result<HeaderValue> {
    let conta = raw.trim();
    let valid = !conta.is_empty()
        && conta.len() <= 20
        && conta.bytes().all(|b| b.is_ascii_digit())
        && !conta.starts_with('0');
    if !valid {
        // Never echo the value: it is the user's account number.
        return Err(Error::Config(
            "conta corrente inválida: use apenas dígitos (incluindo o dígito verificador), sem zeros à esquerda"
                .to_owned(),
        ));
    }
    let mut value = HeaderValue::from_str(conta)
        .map_err(|_| Error::Config("conta corrente inválida".to_owned()))?;
    value.set_sensitive(true);
    Ok(value)
}

fn error_chain(err: &dyn std::error::Error) -> String {
    let mut parts = vec![err.to_string()];
    let mut source = err.source();
    while let Some(cause) = source {
        parts.push(cause.to_string());
        source = cause.source();
    }
    parts.join(": ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The client is shared across tasks and errors cross thread boundaries
    /// (Rust API Guidelines C-SEND-SYNC and C-GOOD-ERR).
    #[test]
    fn public_types_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync + 'static>() {}
        assert_send_sync::<InterClient>();
        assert_send_sync::<InterClientBuilder>();
        assert_send_sync::<Error>();
        assert_send_sync::<ApiError>();
        assert_send_sync::<AccessToken>();
        assert_send_sync::<ClientIdentity>();
        assert_send_sync::<Credentials>();
    }

    #[test]
    fn base_url_requires_https_except_loopback() {
        assert!(parse_base_url("https://exemplo.com.br").is_ok());
        assert!(parse_base_url("http://127.0.0.1:8080").is_ok());
        assert!(parse_base_url("http://localhost:1234/prefixo").is_ok());
        assert!(parse_base_url("http://[::1]:1234").is_ok());
        let err = parse_base_url("http://exemplo.com.br").unwrap_err();
        assert!(err.to_string().contains("https"), "{err}");
        assert!(parse_base_url("ftp://127.0.0.1").is_err());
    }

    #[test]
    fn base_url_rejects_credentials_query_and_garbage() {
        assert!(parse_base_url("https://user:pass@exemplo.com.br").is_err());
        assert!(parse_base_url("https://exemplo.com.br/?a=1").is_err());
        assert!(parse_base_url("não é url").is_err());
    }

    #[test]
    fn conta_corrente_validation_never_echoes_value() {
        assert!(parse_conta_corrente("1234567").is_ok());
        assert!(parse_conta_corrente(" 98765 ").is_ok());
        for invalid in ["", "0123", "12a45", "12-3", "123456789012345678901"] {
            let err = parse_conta_corrente(invalid).unwrap_err().to_string();
            assert!(
                !invalid.is_empty() && !err.contains(invalid) || invalid.is_empty(),
                "{err}"
            );
        }
    }

    #[test]
    fn only_get_requests_are_idempotent_by_default() {
        let request = |method| {
            ApiRequest::new(Endpoint {
                method,
                path: "/x",
                scopes: &[],
            })
        };
        assert_eq!(request(Method::Get).retry, RetryMode::Idempotent);
        for method in [Method::Post, Method::Put, Method::Patch, Method::Delete] {
            assert_eq!(request(method).retry, RetryMode::Never, "{method}");
        }
    }

    /// A payment must never be sent twice because of a transient error.
    #[tokio::test]
    async fn requests_with_side_effects_are_never_retried() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        const PAGAR_BOLETO: Endpoint = Endpoint {
            method: Method::Post,
            path: "/banking/v2/pagamento",
            scopes: &[crate::Scope::PagamentoBoletoWrite],
        };
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth/v2/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "tok",
                "expires_in": 3600,
                "scope": "pagamento-boleto.write",
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/banking/v2/pagamento"))
            .respond_with(ResponseTemplate::new(503))
            .expect(1)
            .mount(&server)
            .await;

        let rcgen::CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed(vec!["cliente.teste".to_owned()]).unwrap();
        let identity = ClientIdentity::from_pem(
            cert.pem().as_bytes(),
            signing_key.serialize_pem().as_bytes(),
        )
        .unwrap();
        let client = InterClient::builder()
            .base_url(server.uri())
            .credentials(Credentials::new("id", "segredo"))
            .identity(identity)
            .retry_policy(RetryPolicy::new(5).initial_delay(Duration::from_millis(1)))
            .build()
            .unwrap();

        let err = client
            .execute::<serde_json::Value>(ApiRequest::new(PAGAR_BOLETO))
            .await
            .unwrap_err();
        assert!(
            matches!(&err, Error::Api(api) if api.status == 503),
            "{err:?}"
        );
    }

    #[test]
    fn token_lifetime_is_parsed_leniently() {
        assert_eq!(seconds(&serde_json::json!(3600)), Some(3600));
        assert_eq!(seconds(&serde_json::json!(3599.9)), Some(3599));
        assert_eq!(seconds(&serde_json::json!("1800")), Some(1800));
        assert_eq!(seconds(&serde_json::json!(-1)), None);
        assert_eq!(seconds(&serde_json::json!(null)), None);
    }
}
