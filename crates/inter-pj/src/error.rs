use std::error::Error as StdError;
use std::fmt;

use crate::identity::IdentityError;
use crate::problem::Problem;

/// Result type used throughout the crate.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Errors returned by the client.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The client configuration is invalid (environment, base URL, account...).
    #[error("{0}")]
    Config(String),
    /// The mTLS certificate or private key could not be used.
    #[error(transparent)]
    Identity(#[from] IdentityError),
    /// The token endpoint rejected the request, or granted fewer scopes than needed.
    #[error("falha ao obter o token de acesso: {0}")]
    Auth(Box<ApiError>),
    /// An API operation returned an error status.
    #[error(transparent)]
    Api(Box<ApiError>),
    /// The request could not be completed (network, TLS, timeout...).
    #[error("falha de comunicação com o Inter: {}", describe_transport(.0))]
    Transport(#[source] reqwest::Error),
    /// The API answered with a payload that could not be understood.
    #[error("resposta inesperada da API em {operation}: {message}")]
    Decode {
        /// Operation, e.g. `GET /banking/v2/saldo`.
        operation: String,
        /// What went wrong.
        message: String,
    },
}

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Self {
        Self::Transport(err)
    }
}

/// Broad category of an [`ApiError`], derived from its HTTP status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ApiErrorKind {
    /// `400`: invalid request.
    InvalidRequest,
    /// `401`: missing, invalid or expired credentials/token.
    Unauthorized,
    /// `403`: authenticated, but not allowed (e.g. missing scope).
    Forbidden,
    /// `404`: resource not found.
    NotFound,
    /// `409`: conflicting state.
    Conflict,
    /// `422`: request understood but not processable.
    Unprocessable,
    /// `429`: rate limit exceeded.
    RateLimited,
    /// `503`: service unavailable (maintenance or outside operating hours).
    Unavailable,
    /// Other `5xx` statuses.
    Server,
    /// Any other status.
    Other,
}

/// An error status returned by the API.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ApiError {
    /// HTTP status code.
    pub status: u16,
    /// Operation that failed, e.g. `GET /banking/v2/saldo`.
    pub operation: String,
    /// Parsed error payload, when the body had one.
    pub problem: Option<Problem>,
    /// Beginning of the body, when it could not be parsed as a [`Problem`].
    pub body_excerpt: Option<String>,
    /// Extra explanation added by the client (e.g. which scopes were missing).
    pub note: Option<String>,
}

impl ApiError {
    pub(crate) fn new(status: u16, operation: String, body: &[u8]) -> Self {
        let problem = Problem::from_body(body);
        let body_excerpt = if problem.is_none() {
            excerpt(body)
        } else {
            None
        };
        Self {
            status,
            operation,
            problem,
            body_excerpt,
            note: None,
        }
    }

    /// Broad category of the error.
    pub fn kind(&self) -> ApiErrorKind {
        match self.status {
            400 => ApiErrorKind::InvalidRequest,
            401 => ApiErrorKind::Unauthorized,
            403 => ApiErrorKind::Forbidden,
            404 => ApiErrorKind::NotFound,
            409 => ApiErrorKind::Conflict,
            422 => ApiErrorKind::Unprocessable,
            429 => ApiErrorKind::RateLimited,
            503 => ApiErrorKind::Unavailable,
            500..=599 => ApiErrorKind::Server,
            _ => ApiErrorKind::Other,
        }
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} respondeu {} ({})",
            self.operation,
            self.status,
            status_text(self.status)
        )?;
        if let Some(note) = &self.note {
            write!(f, ": {note}")?;
        }
        if let Some(problem) = &self.problem {
            write!(f, ": {problem}")?;
            if let Some(id) = &problem.correlation_id {
                write!(f, "\n  correlationId: {id}")?;
            }
        } else if let Some(excerpt) = &self.body_excerpt {
            write!(f, ": {excerpt}")?;
        }
        Ok(())
    }
}

impl StdError for ApiError {}

fn status_text(status: u16) -> &'static str {
    match status {
        400 => "requisição inválida",
        401 => "não autenticado",
        403 => "acesso negado",
        404 => "não encontrado",
        409 => "conflito",
        422 => "não processável",
        429 => "limite de requisições excedido",
        500 => "erro interno do servidor",
        502 => "gateway inválido",
        503 => "serviço indisponível",
        504 => "tempo esgotado no gateway",
        _ => "erro",
    }
}

fn excerpt(body: &[u8]) -> Option<String> {
    const LIMIT: usize = 200;
    let text = String::from_utf8_lossy(body);
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return None;
    }
    Some(match text.char_indices().nth(LIMIT) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text,
    })
}

fn describe_transport(err: &reqwest::Error) -> String {
    let summary = if err.is_timeout() {
        "tempo de resposta esgotado"
    } else if err.is_connect() {
        "não foi possível estabelecer a conexão (verifique rede, proxy, certificado e chave da integração)"
    } else if err.is_body() || err.is_decode() {
        "falha ao ler a resposta"
    } else {
        "falha ao enviar a requisição"
    };
    let mut details = Vec::new();
    let mut source = err.source();
    while let Some(cause) = source {
        let text = cause.to_string();
        if !details.contains(&text) {
            details.push(text);
        }
        source = cause.source();
    }
    if details.is_empty() {
        summary.to_owned()
    } else {
        format!("{summary} ({})", details.join(": "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_follows_status() {
        let cases = [
            (400, ApiErrorKind::InvalidRequest),
            (401, ApiErrorKind::Unauthorized),
            (403, ApiErrorKind::Forbidden),
            (404, ApiErrorKind::NotFound),
            (409, ApiErrorKind::Conflict),
            (422, ApiErrorKind::Unprocessable),
            (429, ApiErrorKind::RateLimited),
            (500, ApiErrorKind::Server),
            (502, ApiErrorKind::Server),
            (503, ApiErrorKind::Unavailable),
            (418, ApiErrorKind::Other),
        ];
        for (status, kind) in cases {
            assert_eq!(ApiError::new(status, "GET /x".into(), b"").kind(), kind);
        }
    }

    #[test]
    fn display_includes_problem_and_correlation_id() {
        let body = r#"{"title":"Não encontrado","detail":"Entidade não encontrada.","correlationId":"c-1"}"#.as_bytes();
        let err = ApiError::new(404, "GET /pix/v2/cob/abc".into(), body);
        assert_eq!(
            err.to_string(),
            "GET /pix/v2/cob/abc respondeu 404 (não encontrado): Não encontrado — Entidade não encontrada.\n  correlationId: c-1"
        );
    }

    #[test]
    fn display_falls_back_to_body_excerpt() {
        let body = "<html>\n  <body>Bad   Gateway</body>\n</html>".as_bytes();
        let err = ApiError::new(502, "GET /banking/v2/saldo".into(), body);
        assert_eq!(
            err.to_string(),
            "GET /banking/v2/saldo respondeu 502 (gateway inválido): <html> <body>Bad Gateway</body> </html>"
        );
    }

    #[test]
    fn excerpt_is_truncated_on_char_boundary() {
        let body = "ç".repeat(300);
        let excerpt = excerpt(body.as_bytes()).unwrap();
        assert_eq!(excerpt.chars().count(), 201);
        assert!(excerpt.ends_with('…'));
    }
}
