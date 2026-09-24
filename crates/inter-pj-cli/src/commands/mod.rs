//! Command implementations and the context they share.

mod assistente;
mod auth;
mod cobranca;
mod completions;
mod config;
mod extrato;
mod manual;
mod pagamento;
mod pix;
mod pix_automatico;
mod qrcode;
mod saldo;
mod simulacao;
mod webhook;

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use chrono::{DateTime, Days, FixedOffset, NaiveDate, Utc};
use clap::ArgMatches;
use clap::parser::ValueSource;
use inter_pj::{ClientIdentity, Credentials, InterClient};
use secrecy::{ExposeSecret, SecretString};

use crate::cli::{Cli, Command, Formato, GlobalArgs, PeriodoArgs};
use crate::config::{self as settings, Given, Inputs, Settings, Source};
use crate::error::CliError;
use crate::output;
use crate::paths;
use crate::tabela::Separador;
use crate::token_store::FileTokenStore;

/// Request timeout. Longer than the library's default: a statement page can
/// hold 10,000 transactions, and PDFs of long periods are large.
const TIMEOUT: Duration = Duration::from_secs(60);

/// Days in the default period of listings (the last 30 days, today included).
const DIAS_PADRAO: u64 = 30;

/// Reads environment variables that are not bound to flags.
pub(crate) trait Env {
    fn var(&self, name: &str) -> Option<String>;
}

/// The real process environment.
#[derive(Debug)]
pub(crate) struct SystemEnv;

impl Env for SystemEnv {
    fn var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok().filter(|value| !value.is_empty())
    }
}

/// Runs the parsed command.
pub(crate) async fn run(cli: Cli, matches: &ArgMatches) -> Result<(), CliError> {
    if cli.global.formato() == Formato::Csv && !cli.command.aceita_csv() {
        return Err(CliError::Usage(
            "o formato csv vale apenas para listagens (saldo, extrato, pagamentos, cobranças e callbacks); use texto ou json"
                .to_owned(),
        ));
    }
    // Generated from the command definition: no configuration needed.
    let command = match cli.command {
        Command::Completions(args) => return completions::run(&args),
        Command::Manual(args) => return manual::run(&args),
        command => command,
    };
    let context = Context::new(cli.global, matches, &SystemEnv)?;
    match command {
        Command::Saldo(args) => saldo::run(&context, &args).await,
        Command::Extrato(args) => extrato::run(&context, args).await,
        Command::Pix(command) => pix::run(&context, command).await,
        Command::PixAutomatico(command) => pix_automatico::run(&context, command).await,
        Command::Pagamento(command) => pagamento::run(&context, command).await,
        Command::Cobranca(command) => cobranca::run(&context, command).await,
        Command::Webhook(command) => webhook::run(&context, command).await,
        Command::Auth(command) => auth::run(&context, command).await,
        Command::Config(command) => config::run(&context, &command),
        Command::Completions(_) | Command::Manual(_) => unreachable!("handled above"),
    }
}

/// State shared by the commands.
pub(crate) struct Context {
    global: GlobalArgs,
    sources: Sources,
    config_path: PathBuf,
    cache_dir: PathBuf,
    client_secret: Option<SecretString>,
    base_url: Option<String>,
}

/// Origin (flag or environment) of each flag-backed setting.
#[derive(Debug, Default)]
struct Sources {
    perfil: Option<Source>,
    ambiente: Option<Source>,
    client_id: Option<Source>,
    certificado: Option<Source>,
    chave_privada: Option<Source>,
    conta_corrente: Option<Source>,
}

impl Context {
    fn new(global: GlobalArgs, matches: &ArgMatches, env: &dyn Env) -> Result<Self, CliError> {
        let sources = Sources {
            perfil: source_of(matches, "perfil", "INTER_PERFIL"),
            ambiente: source_of(matches, "ambiente", "INTER_AMBIENTE"),
            client_id: source_of(matches, "client_id", "INTER_CLIENT_ID"),
            certificado: source_of(matches, "certificado", "INTER_CERTIFICADO"),
            chave_privada: source_of(matches, "chave_privada", "INTER_CHAVE_PRIVADA"),
            conta_corrente: source_of(matches, "conta_corrente", "INTER_CONTA_CORRENTE"),
        };
        let config_path = paths::config_file(global.config.as_deref())?;
        let cache_dir = paths::cache_dir(env.var(settings::ENV_CACHE_DIR).as_deref())?;
        if let Some(dia) = hoje_fixo(env)? {
            let _ = HOJE_FIXO.set(dia);
        }
        Ok(Self {
            global,
            sources,
            config_path,
            cache_dir,
            client_secret: env.var(settings::ENV_CLIENT_SECRET).map(SecretString::from),
            base_url: env.var(settings::ENV_BASE_URL),
        })
    }

    pub(crate) fn formato(&self) -> Formato {
        self.global.formato()
    }

    pub(crate) fn separador(&self) -> Separador {
        self.global.separador
    }

    pub(crate) fn config_path(&self) -> &PathBuf {
        &self.config_path
    }

    pub(crate) fn cache_dir(&self) -> &PathBuf {
        &self.cache_dir
    }

    /// Loads the configuration file and resolves the selected profile.
    pub(crate) fn settings(&self) -> Result<Settings, CliError> {
        let loaded = settings::load(&self.config_path)?;
        let given = |value: &Option<String>, source: Option<Source>| Given {
            value: value.clone(),
            source,
        };
        let inputs = Inputs {
            perfil: given(&self.global.perfil, self.sources.perfil),
            ambiente: given(&self.global.ambiente, self.sources.ambiente),
            client_id: given(&self.global.client_id, self.sources.client_id),
            certificado: Given {
                value: self.global.certificado.clone(),
                source: self.sources.certificado,
            },
            chave_privada: Given {
                value: self.global.chave_privada.clone(),
                source: self.sources.chave_privada,
            },
            conta_corrente: given(&self.global.conta_corrente, self.sources.conta_corrente),
            client_secret: self.client_secret.clone(),
            base_url: self.base_url.clone(),
        };
        let resolved = Settings::resolve(&loaded, inputs)?;
        for warning in &resolved.warnings {
            output::eprint_linha(&format!("aviso: {warning}"));
        }
        Ok(resolved)
    }

    /// Builds a client for the selected profile.
    pub(crate) fn client(&self, settings: &Settings) -> Result<InterClient, CliError> {
        let required = settings.for_client()?;
        let identity = ClientIdentity::from_pem_files(required.certificado, required.chave_privada)
            .map_err(inter_pj::Error::from)?;
        // A certificate the reader cannot parse is left to the TLS library.
        if let Some(aviso) = identity
            .certificate()
            .ok()
            .and_then(|certificado| auth::aviso_de_validade(&certificado, Utc::now()))
        {
            output::eprint_linha(&format!("aviso: {aviso}"));
        }
        let mut builder = InterClient::builder()
            .environment(required.ambiente)
            .credentials(Credentials::new(
                required.client_id,
                required.client_secret.expose_secret(),
            ))
            .identity(identity)
            .user_agent(concat!("inter-pj-cli/", env!("CARGO_PKG_VERSION")))
            .timeout(TIMEOUT)
            .retry_policy(self.global.retry_policy());
        if let Some(url) = &settings.base_url {
            builder = builder.base_url(url.value.clone());
        }
        if let Some(conta) = &settings.conta_corrente {
            builder = builder.conta_corrente(conta.value.clone());
        }
        if let Some(escopos) = &settings.escopos {
            builder = builder.additional_scopes(escopos.value.clone());
        }
        if !self.global.sem_cache {
            builder = builder.token_store(Arc::new(FileTokenStore::new(&self.cache_dir)));
        }
        Ok(builder.build()?)
    }

    /// Warns on stderr that sandbox data is fictitious (text output only).
    pub(crate) fn warn_if_sandbox(&self, settings: &Settings) {
        if self.formato() == Formato::Texto
            && settings
                .ambiente
                .as_ref()
                .is_some_and(|a| !a.value.is_production())
        {
            eprintln!("aviso: ambiente sandbox — os dados retornados são fictícios");
        }
    }
}

/// Brasília time, the bank's calendar: UTC−3, without daylight saving time
/// since 2019.
const BRASILIA: FixedOffset = match FixedOffset::west_opt(3 * 3600) {
    Some(fuso) => fuso,
    None => panic!("fuso de Brasília"),
};

/// The day `INTER_HOJE` fixes as today, once [`Context::new`] checked it.
static HOJE_FIXO: OnceLock<NaiveDate> = OnceLock::new();

/// Today in the bank's calendar. Scheduling dates, the checks of past
/// dates and the default periods follow it: a machine in UTC, or in
/// another time zone, near midnight would take a scheduling for tomorrow
/// as a payment for today.
fn hoje() -> NaiveDate {
    HOJE_FIXO
        .get()
        .copied()
        .unwrap_or_else(|| hoje_em(Utc::now()))
}

/// Now, in Brasília, on the day of [`hoje`]: the time of the clock, on the
/// day `INTER_HOJE` fixes when there is one. The deadlines that start now
/// (the expiration of a confirmation request, the end of the default
/// period of the Pix listings, the time to cancel a recurring charge) go
/// with the dates of the tests.
fn agora() -> DateTime<FixedOffset> {
    agora_em(Utc::now(), HOJE_FIXO.get().copied())
}

fn agora_em(agora: DateTime<Utc>, hoje_fixo: Option<NaiveDate>) -> DateTime<FixedOffset> {
    let agora = agora.with_timezone(&BRASILIA);
    hoje_fixo
        .and_then(|dia| {
            dia.and_time(agora.time())
                .and_local_timezone(BRASILIA)
                .single()
        })
        .unwrap_or(agora)
}

/// `INTER_HOJE` (AAAA-MM-DD), the day taken as today, so that examples and
/// tests with dates do not go stale. Like `INTER_BASE_URL`, which it needs,
/// it exists for the tests against a local mock of the API: with the bank,
/// a wrong today would schedule payments on the wrong day.
fn hoje_fixo(env: &dyn Env) -> Result<Option<NaiveDate>, CliError> {
    let Some(valor) = env.var(settings::ENV_HOJE) else {
        return Ok(None);
    };
    let Some(url) = env.var(settings::ENV_BASE_URL) else {
        return Err(CliError::Config(format!(
            "{} só vale junto com {}, nos testes contra uma simulação local da API",
            settings::ENV_HOJE,
            settings::ENV_BASE_URL
        )));
    };
    settings::servidor_local(&url)?;
    NaiveDate::parse_from_str(valor.trim(), "%Y-%m-%d")
        .map(Some)
        .map_err(|_| {
            CliError::Config(format!(
                "{}: data inválida \"{valor}\": use o formato AAAA-MM-DD",
                settings::ENV_HOJE
            ))
        })
}

fn hoje_em(agora: DateTime<Utc>) -> NaiveDate {
    agora.with_timezone(&BRASILIA).date_naive()
}

/// Dates of a listing: without `--fim`, today; without `--inicio`, the
/// [`DIAS_PADRAO`] days that end on `--fim`.
fn intervalo(periodo: PeriodoArgs, hoje: NaiveDate) -> (NaiveDate, NaiveDate) {
    let fim = periodo.fim.unwrap_or(hoje);
    let inicio = periodo.inicio.unwrap_or_else(|| {
        fim.checked_sub_days(Days::new(DIAS_PADRAO - 1))
            .unwrap_or(fim)
    });
    (inicio, fim)
}

/// Where the value of a global argument came from, if it was given.
fn source_of(matches: &ArgMatches, id: &str, env: &'static str) -> Option<Source> {
    let mut current = matches;
    let mut found = current.value_source(id);
    while let Some((_, sub)) = current.subcommand() {
        if let Some(source) = sub.value_source(id) {
            found = Some(source);
        }
        current = sub;
    }
    match found? {
        ValueSource::CommandLine => Some(Source::Flag),
        ValueSource::EnvVariable => Some(Source::Env(env)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use clap::{CommandFactory, FromArgMatches};

    use super::*;

    #[test]
    fn today_is_fixed_only_against_a_local_mock() {
        struct Variaveis<'a>(&'a [(&'a str, &'a str)]);
        impl Env for Variaveis<'_> {
            fn var(&self, name: &str) -> Option<String> {
                self.0
                    .iter()
                    .find(|(nome, _)| *nome == name)
                    .map(|(_, valor)| (*valor).to_owned())
            }
        }
        let local = ("INTER_BASE_URL", "http://127.0.0.1:9999");
        assert_eq!(hoje_fixo(&Variaveis(&[local])).unwrap(), None);
        assert_eq!(
            hoje_fixo(&Variaveis(&[local, ("INTER_HOJE", "2026-09-24")])).unwrap(),
            NaiveDate::from_ymd_opt(2026, 9, 24)
        );
        for variaveis in [
            &[("INTER_HOJE", "2026-09-24")][..],
            &[
                ("INTER_BASE_URL", "https://cdpj.partners.bancointer.com.br"),
                ("INTER_HOJE", "2026-09-24"),
            ],
            &[local, ("INTER_HOJE", "24/09/2026")],
        ] {
            let erro = hoje_fixo(&Variaveis(variaveis)).unwrap_err();
            assert!(matches!(erro, CliError::Config(_)), "{variaveis:?}");
        }
    }

    #[test]
    fn today_is_the_bank_calendar_day() {
        let em = |texto: &str| hoje_em(texto.parse().unwrap());
        let dia = |ano, mes, dia| NaiveDate::from_ymd_opt(ano, mes, dia).unwrap();
        // 22:30 in Brasília, already the next day in UTC.
        assert_eq!(em("2026-09-25T01:30:00Z"), dia(2026, 9, 24));
        assert_eq!(em("2026-09-25T02:59:59Z"), dia(2026, 9, 24));
        assert_eq!(em("2026-09-25T03:00:00Z"), dia(2026, 9, 25));
        assert_eq!(em("2026-12-31T23:59:59Z"), dia(2026, 12, 31));
    }

    #[test]
    fn now_is_on_the_fixed_day() {
        let relogio: DateTime<Utc> = "2027-03-05T01:30:15Z".parse().unwrap();
        // The time of the clock in Brasília, 22:30:15 of the day before.
        assert_eq!(
            agora_em(relogio, None).to_rfc3339(),
            "2027-03-04T22:30:15-03:00"
        );
        assert_eq!(
            agora_em(relogio, NaiveDate::from_ymd_opt(2026, 9, 24)).to_rfc3339(),
            "2026-09-24T22:30:15-03:00"
        );
    }

    fn matches(args: &[&str]) -> ArgMatches {
        Cli::command().try_get_matches_from(args).unwrap()
    }

    #[test]
    fn detects_flag_given_after_subcommand() {
        let m = matches(&["inter-pj", "saldo", "--client-id", "x"]);
        assert_eq!(
            source_of(&m, "client_id", "INTER_CLIENT_ID"),
            Some(Source::Flag)
        );
        let m = matches(&["inter-pj", "--client-id", "x", "config", "mostrar"]);
        assert_eq!(
            source_of(&m, "client_id", "INTER_CLIENT_ID"),
            Some(Source::Flag)
        );
    }

    #[test]
    fn default_period_is_the_last_30_days() {
        let data = |mes, dia| NaiveDate::from_ymd_opt(2026, mes, dia).unwrap();
        let periodo = |inicio, fim| PeriodoArgs { inicio, fim };
        let hoje = data(9, 23);
        assert_eq!(intervalo(periodo(None, None), hoje), (data(8, 25), hoje));
        assert_eq!(
            intervalo(periodo(None, Some(data(9, 30))), hoje),
            (data(9, 1), data(9, 30))
        );
        assert_eq!(
            intervalo(periodo(Some(data(9, 1)), None), hoje),
            (data(9, 1), hoje)
        );
    }

    #[test]
    fn absent_flag_has_no_source() {
        let m = matches(&["inter-pj", "saldo"]);
        assert_eq!(source_of(&m, "ambiente", "INTER_AMBIENTE"), None);
        let cli = Cli::from_arg_matches(&m).unwrap();
        assert!(cli.global.ambiente.is_none());
    }
}

/// A command against a mock API, with a sandbox profile and a token with
/// the scopes the test needs.
#[cfg(test)]
pub(crate) mod testes {
    use std::collections::HashMap;
    use std::fs;

    use clap::{CommandFactory, FromArgMatches};
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{Context, Env};
    use crate::cli::{Cli, Command};

    struct EnvFalso(HashMap<&'static str, String>);

    impl Env for EnvFalso {
        fn var(&self, name: &str) -> Option<String> {
            self.0.get(name).cloned()
        }
    }

    pub(crate) struct Cenario {
        pub(crate) server: MockServer,
        pub(crate) context: Context,
        _dir: tempfile::TempDir,
    }

    /// The scenario and the command of `inter-pj <args>`, with a token for
    /// `escopos`.
    pub(crate) async fn cenario(args: &[&str], escopos: &str) -> (Cenario, Command) {
        let server = MockServer::start().await;
        let dir = tempfile::tempdir().unwrap();
        let rcgen::CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed(vec!["cliente.teste".to_owned()]).unwrap();
        let certificado = dir.path().join("certificado.crt");
        let chave = dir.path().join("chave.key");
        fs::write(&certificado, cert.pem()).unwrap();
        fs::write(&chave, signing_key.serialize_pem()).unwrap();
        let config = dir.path().join("config.toml");
        fs::write(
            &config,
            format!(
                "[perfis.padrao]\nambiente = \"sandbox\"\nclient_id = \"id-de-teste\"\ncertificado = '{}'\nchave_privada = '{}'\n",
                certificado.display(),
                chave.display()
            ),
        )
        .unwrap();
        let config = config.display().to_string();
        let mut full = vec!["inter-pj", "--config", &config];
        full.extend_from_slice(args);
        let matches = Cli::command().try_get_matches_from(&full).unwrap();
        let cli = Cli::from_arg_matches(&matches).unwrap();
        let env = EnvFalso(HashMap::from([
            ("INTER_CLIENT_SECRET", "segredo-de-teste".to_owned()),
            ("INTER_BASE_URL", server.uri()),
            (
                "INTER_CACHE_DIR",
                dir.path().join("cache").display().to_string(),
            ),
        ]));
        let context = Context::new(cli.global, &matches, &env).unwrap();
        Mock::given(method("POST"))
            .and(path("/oauth/v2/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "tok",
                "expires_in": 3600,
                "scope": escopos
            })))
            .mount(&server)
            .await;
        (
            Cenario {
                server,
                context,
                _dir: dir,
            },
            cli.command,
        )
    }
}
