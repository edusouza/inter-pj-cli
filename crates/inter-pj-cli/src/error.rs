//! CLI errors, exit codes and user-facing reporting.

use std::io;

use inter_pj::banking::PeriodoError;
use inter_pj::{ApiError, ApiErrorKind, Error as InterError};

use crate::chamada::chamada;
use crate::output;

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
    },
    /// A charge whose issue is unknown: it may have been issued. The API
    /// refuses an identical one for 30 minutes, but not after that.
    #[error("{source}")]
    EmissaoIncerta {
        source: InterError,
        /// Command that finds the charge, to run before trying again.
        consulta: String,
    },
    /// A creation without idempotency key (a recurrence, a confirmation
    /// request) whose outcome is unknown: it may have been made, and
    /// repeating it may make another.
    #[error("{source}")]
    CriacaoIncerta {
        source: InterError,
        /// What may have happened: "a recorrência pode ter sido criada".
        situacao: &'static str,
        /// Command that finds it, to run before trying again.
        consulta: String,
    },
    /// A Pix charge whose creation is unknown: it may have been created.
    /// With the same txid, the API does not create a second one.
    #[error("{source}")]
    CobrancaPixIncerta {
        source: InterError,
        /// The command of the charges: `pix cob`, `pix cobv` or
        /// `pix-automatico cobr`.
        comando: &'static str,
        txid: String,
    },
    /// A refund whose outcome is unknown: the money may have left. With the
    /// same id, the API does not refund again.
    #[error("{source}")]
    DevolucaoIncerta {
        source: InterError,
        /// End-to-end id of the Pix refunded.
        e2e: String,
        id: String,
    },
    /// A batch of charges whose request is unknown: it may have been
    /// received. Its charges have txids, so repeating it creates none twice.
    #[error("{source}")]
    LoteCobvIncerto { source: InterError, id: u64 },
    /// A retry of callbacks in blocks that stopped midway: the callbacks of
    /// the first blocks were already asked for.
    #[error("{source}")]
    ReenvioIncompleto {
        source: InterError,
        /// Operations whose callbacks were already asked for.
        pedidos: usize,
        /// Operations in all.
        total: usize,
        /// Command that asks for the rest.
        restantes: String,
    },
    /// A change of a webhook whose outcome is unknown: it may have been made.
    #[error("{source}")]
    WebhookIncerto {
        source: InterError,
        /// What may have happened: "o webhook pode ter sido cadastrado".
        situacao: &'static str,
        /// Command that shows the webhook.
        consulta: String,
    },
    /// `pix devolucao ... --aguardar`: the refund was not made.
    #[error("a devolução não foi feita: {motivo}")]
    DevolucaoNaoRealizada { motivo: String },
    /// `cobranca emitir --aguardar`: the API could not issue the charge.
    #[error("a cobrança não foi emitida: {situacao}")]
    CobrancaNaoEmitida { situacao: String },
    /// `cobranca editar`, or `cobranca edicao --aguardar`: the change failed.
    #[error("a alteração da cobrança não foi feita: {motivo}")]
    EdicaoNaoFeita { motivo: String },
    /// The user did not confirm the operation.
    #[error("operação cancelada: nada foi enviado")]
    Cancelado,
    /// The configuration wizard ended before its last answer.
    #[error("assistente interrompido: nada foi gravado")]
    AssistenteInterrompido,
    /// `pix consultar --aguardar`: the payment ended without being paid.
    #[error("o Pix terminou sem ser pago: {status}")]
    PixNaoPago { status: String },
    /// `pagamento lote consultar --aguardar`: some payments of the batch
    /// were not made.
    #[error("o lote foi processado com erro: {detalhe}")]
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
            Self::Config(_) => exit::CONFIG,
            Self::Io { .. } => exit::UNEXPECTED,
            Self::Cancelado | Self::AssistenteInterrompido => exit::CANCELLED,
            Self::PixNaoPago { .. }
            | Self::LoteComErro { .. }
            | Self::CobrancaNaoEmitida { .. }
            | Self::EdicaoNaoFeita { .. }
            | Self::DevolucaoNaoRealizada { .. } => exit::REJECTED,
            Self::TempoEsgotado { .. } => exit::WAIT_TIMEOUT,
            // Built only when the request may have been processed: a script
            // must not repeat them blindly, unlike a `429` or a refused
            // connection (6).
            Self::ResultadoIncerto { .. }
            | Self::PagamentoIncerto { .. }
            | Self::EmissaoIncerta { .. }
            | Self::CriacaoIncerta { .. }
            | Self::CobrancaPixIncerta { .. }
            | Self::DevolucaoIncerta { .. }
            | Self::LoteCobvIncerto { .. }
            | Self::WebhookIncerto { .. } => exit::UNCERTAIN,
            Self::Inter(err) | Self::ReenvioIncompleto { source: err, .. } => match err {
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
        match self {
            Self::Inter(err) => dicas_da_api(err),
            Self::PagamentosRepetidos { .. } => {
                vec!["se forem mesmo pagamentos distintos, use --permitir-repetidos".to_owned()]
            }
            Self::Periodo {
                dica: Some(dica), ..
            } => vec![(*dica).to_owned()],
            Self::TempoEsgotado { .. } => {
                vec!["consulte de novo mais tarde, ou aumente o --timeout".to_owned()]
            }
            Self::ResultadoIncerto { id_idempotente, .. } => {
                vec![
                    "o pagamento pode ter sido feito: confira o extrato antes de tentar de novo"
                        .to_owned(),
                    format!(
                        "para repetir sem risco de pagar duas vezes, use a mesma chave: --id-idempotente {id_idempotente}"
                    ),
                ]
            }
            Self::PagamentoIncerto {
                situacao, consulta, ..
            } => {
                vec![
                    format!(
                        "{situacao}, e esta API não tem chave de idempotência: repetir o comando pode pagar duas vezes"
                    ),
                    format!("confira antes de tentar de novo: {consulta}"),
                ]
            }
            Self::CriacaoIncerta {
                situacao, consulta, ..
            } => {
                vec![
                    format!(
                        "{situacao}, e esta API não tem chave de idempotência: repetir o comando pode criar outra"
                    ),
                    format!("confira antes de tentar de novo: {consulta}"),
                ]
            }
            Self::CobrancaPixIncerta { comando, txid, .. } => {
                vec![
                    "a cobrança pode ter sido criada; com o mesmo txid, a API não cria outra"
                        .to_owned(),
                    format!("confira com: {} {comando} consultar {txid}", chamada()),
                    format!("ou repita o comando com --txid {txid}"),
                ]
            }
            Self::DevolucaoIncerta { e2e, id, .. } => {
                vec![
                    "a devolução pode ter sido feita; com o mesmo id, a API não devolve de novo"
                        .to_owned(),
                    format!(
                        "confira com: {} pix devolucao consultar {e2e} {id}",
                        chamada()
                    ),
                    format!("ou repita o comando com --id {id}"),
                ]
            }
            Self::LoteCobvIncerto { id, .. } => {
                vec![
                    format!(
                        "o lote pode ter sido recebido: confira com {} pix lote-cobv consultar {id} antes de repetir",
                        chamada()
                    ),
                    "repetir o comando não duplica cobranças: com o mesmo txid, a API não cria outra"
                        .to_owned(),
                ]
            }
            Self::ReenvioIncompleto {
                pedidos,
                total,
                restantes,
                ..
            } => {
                vec![
                    format!("o reenvio de {pedidos} das {total} operações já foi pedido"),
                    format!("para pedir o das outras: {restantes}"),
                ]
            }
            Self::WebhookIncerto {
                situacao, consulta, ..
            } => {
                vec![format!("{situacao}: confira com {consulta}")]
            }
            Self::EmissaoIncerta { consulta, .. } => {
                vec![
                    "a cobrança pode ter sido emitida; por 30 minutos, a API recusa outra com o mesmo seu número, valor, vencimento e pagador".to_owned(),
                    format!("confira antes de tentar de novo: {consulta}"),
                ]
            }
            _ => Vec::new(),
        }
    }
}

/// Suggestions for an error of the Inter client.
fn dicas_da_api(err: &InterError) -> Vec<String> {
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
                return vec![format!(
                    "o token foi recusado; tente `{} auth limpar` e repita o comando",
                    chamada()
                )];
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

/// Whether a payment may have been made despite the error: the request
/// may have reached the API (timeout, dropped connection, `5xx`, an
/// unreadable success). A refused connection never reached it.
pub(crate) fn resultado_incerto(err: &InterError) -> bool {
    match err {
        InterError::Transport(source) => !source.is_connect(),
        InterError::Api(api) => api.status >= 500,
        InterError::Decode { .. } => true,
        _ => false,
    }
}

fn problem_type(api: &ApiError) -> Option<&str> {
    api.problem.as_ref()?.type_error.as_deref()
}

/// Prints the error (and hints) to stderr. Messages of the API reach the
/// terminal without control characters, and the lines after the first are
/// indented: only the CLI's own lines (`erro:`, `dica:`) start at the
/// margin, whatever a text of the API brings.
pub(crate) fn report(err: &CliError) {
    eprintln!(
        "erro: {}",
        continuacao(&output::sem_controle(&err.to_string()))
    );
    for hint in err.hints() {
        eprintln!("dica: {}", output::limpo(&hint));
    }
}

/// `texto` with its lines after the first indented, when they are not.
fn continuacao(texto: &str) -> String {
    let mut linhas = texto.lines();
    let mut saida = linhas.next().unwrap_or_default().to_owned();
    for linha in linhas {
        saida.push('\n');
        if !linha.is_empty() && !linha.starts_with(char::is_whitespace) {
            saida.push_str("  ");
        }
        saida.push_str(linha);
    }
    saida
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_first_line_of_a_message_starts_at_the_margin() {
        assert_eq!(continuacao("uma linha"), "uma linha");
        assert_eq!(
            continuacao("arquivo inválido:\n  - linha 2\ndica: falsa\n\nerro: outra"),
            "arquivo inválido:\n  - linha 2\n  dica: falsa\n\n  erro: outra"
        );
    }
}
