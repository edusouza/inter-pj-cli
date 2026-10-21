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
    /// The operation may have been processed (timeout, `5xx`, an
    /// unreadable success): check before any new attempt.
    pub(crate) const UNCERTAIN: u8 = 9;
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum CliError {
    /// Invalid usage detected after argument parsing.
    #[error("{0}")]
    Usage(String),
    /// `pagamento lote enviar`: the same payment more than once in the
    /// file, which `--permitir-repetidos` allows.
    #[error("pagamentos repetidos em {arquivo}:\n  {}", .repetidos.join("\n  "))]
    PagamentosRepetidos {
        arquivo: String,
        /// Where each payment repeats, and why it matters.
        repetidos: Vec<String>,
    },
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
    /// A payment without idempotency key (boleto, DARF, batch) whose outcome
    /// is unknown: it may have been made, and repeating it may pay twice.
    #[error("{source}")]
    PagamentoIncerto {
        source: InterError,
        /// What may have happened: "o pagamento pode ter sido feito".
        situacao: &'static str,
        /// Command that finds the payment, to run before trying again.
        consulta: String,
        /// Scopes that command needs, which the payment did not: the
        /// reading of the payments.
        escopos: &'static str,
    },
    /// The user did not confirm the operation.
    #[error("operação cancelada: nada foi enviado")]
    Cancelado,
    /// `pix consultar --aguardar`: the payment ended without being paid.
    #[error("o Pix terminou sem ser pago: {status}")]
    PixNaoPago { status: String },
    /// `pagamento lote consultar --aguardar`, or `enviar`: some payments of
    /// the batch were not made, or the API says it had errors.
    #[error("o lote foi processado, mas {detalhe}")]
    LoteComErro { detalhe: String },
    /// `--aguardar`: no final status before the time limit.
    #[error("tempo de espera esgotado ({segundos} s): {oque} ainda está {status}")]
    TempoEsgotado {
        /// What was awaited: `o Pix`, `o lote`.
        oque: &'static str,
        status: String,
        segundos: u64,
    },
    /// Local I/O failure.
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: io::Error,
    },
    /// A configuration file that could not be read as TOML, and the command
    /// that fixes it, when `config verificar --corrigir` does.
    #[error("{mensagem}")]
    ConfigFile {
        mensagem: String,
        verificar: Option<String>,
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
            Self::Usage(_) | Self::PagamentosRepetidos { .. } | Self::Periodo { .. } => exit::USAGE,
            Self::Config(_) | Self::ConfigFile { .. } => exit::CONFIG,
            Self::Io { .. } => exit::UNEXPECTED,
            Self::Cancelado => exit::CANCELLED,
            Self::PixNaoPago { .. } | Self::LoteComErro { .. } => exit::REJECTED,
            Self::TempoEsgotado { .. } => exit::WAIT_TIMEOUT,
            // Built only when the request may have been processed: a script
            // must not repeat them blindly, unlike a `429` or a refused
            // connection (6).
            Self::ResultadoIncerto { .. } | Self::PagamentoIncerto { .. } => exit::UNCERTAIN,
            Self::Inter(err) => match err {
                InterError::InvalidInput(_) => exit::USAGE,
                InterError::Config(_) | InterError::Identity(_) => exit::CONFIG,
                InterError::Auth(_) => exit::AUTH,
                InterError::Api(api) => match api.kind() {
                    ApiErrorKind::Unauthorized | ApiErrorKind::Forbidden => exit::AUTH,
                    ApiErrorKind::RateLimited
                    | ApiErrorKind::Unavailable
                    | ApiErrorKind::Server => exit::UNAVAILABLE,
                    _ => exit::REJECTED,
                },
                InterError::Transport(_) | InterError::TokenTransport(_) => exit::UNAVAILABLE,
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
            Self::PagamentosRepetidos { .. } => {
                return vec![
                    "se forem mesmo pagamentos distintos, use --permitir-repetidos".to_owned(),
                ];
            }
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
            Self::PagamentoIncerto {
                situacao,
                consulta,
                escopos,
                ..
            } => {
                return vec![
                    format!(
                        "{situacao}, e esta API não tem chave de idempotência: repetir o comando pode pagar duas vezes"
                    ),
                    format!("confira antes de tentar de novo: {consulta}"),
                    format!(
                        "a consulta precisa da permissão de leitura dos pagamentos ({escopos}), e uma lista vazia logo depois do erro não prova que nada foi pago: espere alguns minutos e confira também o extrato e os agendamentos no Internet Banking"
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
            InterError::Transport(_) | InterError::TokenTransport(_) => vec![
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
    if let CliError::ConfigFile {
        verificar: Some(verificar),
        ..
    } = err
    {
        eprintln!(
            "dica: `{verificar}` mostra a correção de cada linha; com `--corrigir`, ele a aplica e guarda uma cópia do arquivo original"
        );
    }
    for hint in err.hints() {
        eprintln!("dica: {hint}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_uncertain_outcome_exits_with_9_whatever_the_cause() {
        let ilegivel = || InterError::Decode {
            operation: "POST /banking/v2/pagamento".to_owned(),
            message: "formato inesperado".to_owned(),
        };
        let pagamento = CliError::PagamentoIncerto {
            source: ilegivel(),
            situacao: "o pagamento pode ter sido feito",
            consulta: "inter-pj pagamento boleto listar --codigo 0".to_owned(),
            escopos: "pagamento-boleto.read",
        };
        assert_eq!(pagamento.exit_code(), exit::UNCERTAIN);
        let pix = CliError::ResultadoIncerto {
            source: ilegivel(),
            id_idempotente: "9b2f6c1e-5d0a-4c1b-8f3e-2a7d4e6b8c90".to_owned(),
        };
        assert_eq!(pix.exit_code(), exit::UNCERTAIN);
        // Outside an uncertain outcome, the error keeps its category.
        assert_eq!(CliError::Inter(ilegivel()).exit_code(), exit::UNEXPECTED);
    }
}
