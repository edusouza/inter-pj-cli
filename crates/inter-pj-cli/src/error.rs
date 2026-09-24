//! CLI errors, exit codes and user-facing reporting.

use std::io;

use inter_pj::banking::PeriodoError;
use inter_pj::{ApiError, ApiErrorKind, Error as InterError};

/// Documented exit codes.
pub(crate) mod exit {
    pub(crate) const UNEXPECTED: u8 = 1;
    pub(crate) const USAGE: u8 = 2;
    pub(crate) const CONFIG: u8 = 3;
    pub(crate) const AUTH: u8 = 4;
    pub(crate) const REJECTED: u8 = 5;
    pub(crate) const UNAVAILABLE: u8 = 6;
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum CliError {
    /// Invalid usage detected after argument parsing.
    #[error("{0}")]
    Usage(String),
    /// Invalid statement period.
    #[error("{erro}")]
    Periodo {
        erro: PeriodoError,
        /// What to do instead, shown as a hint.
        dica: Option<&'static str>,
    },
    /// Missing or invalid configuration.
    #[error("{0}")]
    Config(String),
    /// Error returned by the Inter client.
    #[error(transparent)]
    Inter(#[from] InterError),
    /// Local I/O failure.
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: io::Error,
    },
}

impl CliError {
    pub(crate) fn io(context: impl Into<String>, source: io::Error) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }

    pub(crate) fn exit_code(&self) -> u8 {
        match self {
            Self::Usage(_) | Self::Periodo { .. } => exit::USAGE,
            Self::Config(_) => exit::CONFIG,
            Self::Io { .. } => exit::UNEXPECTED,
            Self::Inter(err) => match err {
                InterError::Config(_) | InterError::Identity(_) => exit::CONFIG,
                InterError::Auth(_) => exit::AUTH,
                InterError::Api(api) => match api.kind() {
                    ApiErrorKind::Unauthorized | ApiErrorKind::Forbidden => exit::AUTH,
                    ApiErrorKind::RateLimited
                    | ApiErrorKind::Unavailable
                    | ApiErrorKind::Server => exit::UNAVAILABLE,
                    _ if api.status >= 500 => exit::UNAVAILABLE,
                    _ => exit::REJECTED,
                },
                InterError::Transport(_) => exit::UNAVAILABLE,
                _ => exit::UNEXPECTED,
            },
        }
    }

    /// Suggestions shown after the error message.
    pub(crate) fn hints(&self) -> Vec<&'static str> {
        let err = match self {
            Self::Inter(err) => err,
            Self::Periodo {
                dica: Some(dica), ..
            } => return vec![dica],
            _ => return Vec::new(),
        };
        match err {
            InterError::Api(api) if problem_type(api) == Some("SCROLL_ALREADY_ACTIVE") => vec![
                "já existe uma leitura do extrato em modo scroll para esta conta (outra execução?); ela expira após 6 minutos sem uso",
            ],
            InterError::Api(api) if problem_type(api) == Some("SCROLL_EXPIRED") => vec![
                "a leitura em modo scroll expirou (6 minutos sem requisições); execute o comando novamente",
            ],
            InterError::Auth(api) if api.note.is_none() => vec![
                "confira o client_id, o client_secret e se o certificado/chave são os da mesma integração",
            ],
            InterError::Api(api) => match api.kind() {
                ApiErrorKind::Unauthorized => {
                    vec!["o token foi recusado; tente `inter-pj auth limpar` e repita o comando"]
                }
                ApiErrorKind::Forbidden => vec![
                    "confira se a integração tem os escopos necessários habilitados no Internet Banking PJ",
                    "se a integração tem mais de uma conta, informe --conta-corrente",
                ],
                ApiErrorKind::RateLimited => {
                    vec!["limite de requisições atingido; aguarde um minuto e tente novamente"]
                }
                ApiErrorKind::Unavailable => vec![
                    "o serviço pode estar em manutenção ou fora da janela de funcionamento; tente mais tarde",
                ],
                _ => Vec::new(),
            },
            InterError::Transport(_) => vec![
                "confira a conexão/proxy e se o certificado (.crt) e a chave (.key) da integração estão válidos",
            ],
            _ => Vec::new(),
        }
    }
}

fn problem_type(api: &ApiError) -> Option<&str> {
    api.problem.as_ref()?.type_error.as_deref()
}

/// Prints the error (and hints) to stderr.
pub(crate) fn report(err: &CliError) {
    eprintln!("erro: {err}");
    for hint in err.hints() {
        eprintln!("dica: {hint}");
    }
}
