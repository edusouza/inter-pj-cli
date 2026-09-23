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
    pub(crate) const CANCELLED: u8 = 7;
    pub(crate) const WAIT_TIMEOUT: u8 = 8;
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
    /// A payment whose outcome is unknown (timeout, `5xx`): it may have been
    /// made.
    #[error("{source}")]
    ResultadoIncerto {
        source: InterError,
        /// Key that repeats the payment without paying twice.
        id_idempotente: String,
    },
    /// The user did not confirm the operation.
    #[error("operação cancelada: nada foi enviado")]
    Cancelado,
    /// `pix consultar --aguardar`: the payment ended without being paid.
    #[error("o Pix terminou sem ser pago: {status}")]
    PixNaoPago { status: String },
    /// `pix consultar --aguardar`: no final status before the time limit.
    #[error("tempo de espera esgotado ({segundos} s): o Pix ainda está {status}")]
    TempoEsgotado { status: String, segundos: u64 },
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
            Self::Cancelado => exit::CANCELLED,
            Self::PixNaoPago { .. } => exit::REJECTED,
            Self::TempoEsgotado { .. } => exit::WAIT_TIMEOUT,
            Self::Inter(err) | Self::ResultadoIncerto { source: err, .. } => match err {
                InterError::InvalidInput(_) => exit::USAGE,
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
    pub(crate) fn hints(&self) -> Vec<String> {
        let err = match self {
            Self::Inter(err) => err,
            Self::Periodo {
                dica: Some(dica), ..
            } => return vec![(*dica).to_owned()],
            Self::TempoEsgotado { .. } => {
                return vec!["consulte de novo mais tarde, ou aumente o --timeout".to_owned()];
            }
            Self::ResultadoIncerto { id_idempotente, .. } => {
                return vec![
                    "o pagamento pode ter sido feito: confira o extrato antes de tentar de novo"
                        .to_owned(),
                    format!(
                        "para repetir sem risco de pagar duas vezes, use a mesma chave: --id-idempotente {id_idempotente}"
                    ),
                ];
            }
            _ => return Vec::new(),
        };
        let hints: Vec<&str> = match err {
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
        };
        hints.into_iter().map(str::to_owned).collect()
    }
}

fn problem_type(api: &ApiError) -> Option<&str> {
    api.problem.as_ref()?.type_error.as_deref()
}

/// Prints the error (and hints) to stderr. Messages of the API reach the
/// terminal without control characters.
pub(crate) fn report(err: &CliError) {
    eprintln!("erro: {}", crate::output::sem_controle(&err.to_string()));
    for hint in err.hints() {
        eprintln!("dica: {hint}");
    }
}
