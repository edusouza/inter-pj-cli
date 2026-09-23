//! Command implementations and the context they share.

mod auth;
mod cobranca;
mod config;
mod extrato;
mod pagamento;
mod pix;
mod saldo;
mod simulacao;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::{Days, Local, NaiveDate};
use clap::ArgMatches;
use clap::parser::ValueSource;
use inter_pj::{ClientIdentity, Credentials, InterClient};
use secrecy::ExposeSecret;

use crate::cli::{Cli, Command, Formato, GlobalArgs, PeriodoArgs};
use crate::config::{self as settings, Given, Inputs, Settings, Source};
use crate::error::CliError;
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
            "o formato csv vale apenas para listagens (saldo, extrato e pagamentos); use texto ou json"
                .to_owned(),
        ));
    }
    let context = Context::new(cli.global, matches, &SystemEnv)?;
    match cli.command {
        Command::Saldo(args) => saldo::run(&context, &args).await,
        Command::Extrato(args) => extrato::run(&context, args).await,
        Command::Pix(command) => pix::run(&context, command).await,
        Command::Pagamento(command) => pagamento::run(&context, command).await,
        Command::Cobranca(command) => cobranca::run(&context, command).await,
        Command::Auth(command) => auth::run(&context, command).await,
        Command::Config(command) => config::run(&context, &command),
    }
}

/// State shared by the commands.
pub(crate) struct Context {
    global: GlobalArgs,
    sources: Sources,
    config_path: PathBuf,
    cache_dir: PathBuf,
    client_secret: Option<String>,
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
        Ok(Self {
            global,
            sources,
            config_path,
            cache_dir,
            client_secret: env.var(settings::ENV_CLIENT_SECRET),
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
            eprintln!("aviso: {warning}");
        }
        Ok(resolved)
    }

    /// Builds a client for the selected profile.
    pub(crate) fn client(&self, settings: &Settings) -> Result<InterClient, CliError> {
        let required = settings.for_client()?;
        let identity = ClientIdentity::from_pem_files(required.certificado, required.chave_privada)
            .map_err(inter_pj::Error::from)?;
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

fn hoje() -> NaiveDate {
    Local::now().date_naive()
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
