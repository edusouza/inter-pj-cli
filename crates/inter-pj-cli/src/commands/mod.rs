//! Command implementations and the context they share.

mod auth;
mod config;
mod saldo;

use std::path::PathBuf;
use std::sync::Arc;

use clap::ArgMatches;
use clap::parser::ValueSource;
use inter_pj::{ClientIdentity, Credentials, InterClient};
use secrecy::ExposeSecret;

use crate::cli::{Cli, Command, Formato, GlobalArgs};
use crate::config::{self as settings, Given, Inputs, Settings, Source};
use crate::doctor;
use crate::error::CliError;
use crate::paths;
use crate::token_store::FileTokenStore;

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
    let context = Context::new(cli.global, matches, &SystemEnv)?;
    match cli.command {
        Command::Saldo(args) => saldo::run(&context, &args).await,
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
    config: Option<Source>,
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
            config: source_of(matches, "config", "INTER_CONFIG"),
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

    pub(crate) fn config_path(&self) -> &PathBuf {
        &self.config_path
    }

    pub(crate) fn cache_dir(&self) -> &PathBuf {
        &self.cache_dir
    }

    /// A command for the user to run next, with the configuration file
    /// when it was given by `--config`.
    pub(crate) fn sugestao(&self, comando: &str) -> String {
        match self.sources.config {
            Some(Source::Flag) => format!(
                "inter-pj --config \"{}\" {comando}",
                self.config_path.display()
            ),
            _ => format!("inter-pj {comando}"),
        }
    }

    /// Loads the configuration file and resolves the selected profile.
    pub(crate) fn settings(&self) -> Result<Settings, CliError> {
        let loaded = settings::load(&self.config_path).map_err(|err| match err {
            CliError::ConfigFile {
                mensagem,
                verificar,
            } => CliError::ConfigFile {
                mensagem,
                verificar: verificar.map(|_| self.sugestao("config verificar")),
            },
            err => err,
        })?;
        let resolved = Settings::resolve(&loaded, self.inputs())?;
        for warning in &resolved.warnings {
            eprintln!("aviso: {warning}");
        }
        for (chave, caminho) in resolved.paths_with_control_chars() {
            eprintln!(
                "aviso: o caminho de {chave} tem um caractere de controle (\"{}\"): entre aspas duplas, a barra invertida começa um escape; use aspas simples ou execute `{}`",
                doctor::escapar_controles(&caminho.display().to_string()),
                self.sugestao("config verificar --corrigir")
            );
        }
        Ok(resolved)
    }

    /// The settings given by flags and environment variables.
    pub(crate) fn inputs(&self) -> Inputs {
        let given = |value: &Option<String>, source: Option<Source>| Given {
            value: value.clone(),
            source,
        };
        Inputs {
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
        }
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
            .user_agent(concat!("inter-pj-cli/", env!("CARGO_PKG_VERSION")));
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
    fn absent_flag_has_no_source() {
        let m = matches(&["inter-pj", "saldo"]);
        assert_eq!(source_of(&m, "ambiente", "INTER_AMBIENTE"), None);
        let cli = Cli::from_arg_matches(&m).unwrap();
        assert!(cli.global.ambiente.is_none());
    }
}
