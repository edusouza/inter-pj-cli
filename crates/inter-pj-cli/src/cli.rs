//! Command line definition.
//!
//! Doc comments in this module are the user-facing `--help` text (in
//! Portuguese), so rustdoc markdown conventions do not apply.
#![allow(clippy::doc_markdown)]

use std::fmt::Write as _;
use std::path::PathBuf;

use chrono::NaiveDate;
use clap::{ArgAction, Args, Parser, Subcommand, ValueEnum};

const AFTER_HELP: &str = "\
Credenciais:
  O client_secret é lido da variável de ambiente INTER_CLIENT_SECRET ou do arquivo de
  configuração (nunca de uma flag, para não ficar no histórico do shell).

Projeto não oficial, sem vínculo com o Banco Inter.
Documentação: https://github.com/edusouza/inter-pj-cli";

const HELP_TEMPLATE: &str = "\
{about-with-newline}
Uso: {usage}

{all-args}{after-help}";

/// The command definition with Portuguese help on every subcommand.
///
/// clap has no localisation support: headings and usage lines are set here
/// recursively instead.
pub(crate) fn command() -> clap::Command {
    localize(<Cli as clap::CommandFactory>::command(), "inter-pj")
}

fn localize(command: clap::Command, path: &str) -> clap::Command {
    let mut usage = format!("{path} [OPÇÕES]");
    for arg in command.get_positionals() {
        let name = arg
            .get_value_names()
            .and_then(|names| names.first())
            .map_or_else(
                || arg.get_id().to_string().to_uppercase(),
                ToString::to_string,
            );
        let _ = if arg.is_required_set() {
            write!(usage, " <{name}>")
        } else {
            write!(usage, " [{name}]")
        };
    }
    if command.has_subcommands() {
        usage.push_str(" <COMANDO>");
    }
    command
        .help_template(HELP_TEMPLATE)
        .override_usage(usage)
        .subcommand_help_heading("Comandos")
        .subcommand_value_name("COMANDO")
        .mut_subcommands(|sub| {
            let sub_path = format!("{path} {}", sub.get_name());
            localize(sub, &sub_path)
        })
}

/// CLI não oficial para a conta PJ do Inter Empresas.
#[derive(Debug, Parser)]
#[command(
    name = "inter-pj",
    version,
    about = "CLI não oficial para a conta PJ do Inter Empresas",
    after_help = AFTER_HELP,
    help_template = HELP_TEMPLATE,
    subcommand_help_heading = "Comandos",
    subcommand_value_name = "COMANDO",
    disable_help_flag = true,
    disable_version_flag = true,
    disable_help_subcommand = true,
    propagate_version = true
)]
pub(crate) struct Cli {
    #[command(flatten)]
    pub(crate) global: GlobalArgs,

    #[command(subcommand)]
    pub(crate) command: Command,
}

/// Options accepted by every command.
#[derive(Debug, Args)]
#[command(next_help_heading = "Opções globais")]
pub(crate) struct GlobalArgs {
    /// Perfil do arquivo de configuração
    #[arg(
        short,
        long,
        global = true,
        env = "INTER_PERFIL",
        value_name = "NOME",
        hide_env_values = true
    )]
    pub(crate) perfil: Option<String>,

    /// Arquivo de configuração [padrão: ~/.config/inter-pj/config.toml]
    #[arg(
        long,
        global = true,
        env = "INTER_CONFIG",
        value_name = "ARQUIVO",
        hide_env_values = true
    )]
    pub(crate) config: Option<PathBuf>,

    /// Ambiente: sandbox ou producao
    #[arg(
        long,
        global = true,
        env = "INTER_AMBIENTE",
        value_name = "AMBIENTE",
        hide_env_values = true
    )]
    pub(crate) ambiente: Option<String>,

    /// client_id da integração
    #[arg(
        long,
        global = true,
        env = "INTER_CLIENT_ID",
        value_name = "ID",
        hide_env_values = true
    )]
    pub(crate) client_id: Option<String>,

    /// Certificado da integração (.crt)
    #[arg(
        long,
        global = true,
        env = "INTER_CERTIFICADO",
        value_name = "ARQUIVO",
        hide_env_values = true
    )]
    pub(crate) certificado: Option<PathBuf>,

    /// Chave privada da integração (.key)
    #[arg(
        long,
        global = true,
        env = "INTER_CHAVE_PRIVADA",
        value_name = "ARQUIVO",
        hide_env_values = true
    )]
    pub(crate) chave_privada: Option<PathBuf>,

    /// Conta corrente (só quando a integração tem mais de uma conta)
    #[arg(
        long,
        global = true,
        env = "INTER_CONTA_CORRENTE",
        value_name = "NUMERO",
        hide_env_values = true
    )]
    pub(crate) conta_corrente: Option<String>,

    /// Formato da saída: texto (padrão) ou json
    #[arg(
        long,
        global = true,
        value_enum,
        default_value_t = Formato::Texto,
        value_name = "FORMATO",
        hide_possible_values = true,
        hide_default_value = true
    )]
    pub(crate) formato: Formato,

    /// Atalho para --formato json
    #[arg(long, global = true)]
    pub(crate) json: bool,

    /// Não usa nem grava o cache local de tokens
    #[arg(long, global = true)]
    pub(crate) sem_cache: bool,

    /// Mostra detalhes da execução em stderr (-vv para mais detalhes)
    #[arg(short, long, global = true, action = ArgAction::Count)]
    pub(crate) verbose: u8,

    /// Mostra esta ajuda
    #[arg(short, long, global = true, action = ArgAction::Help)]
    help: Option<bool>,

    /// Mostra a versão
    #[arg(short = 'V', long, action = ArgAction::Version)]
    version: Option<bool>,
}

impl GlobalArgs {
    /// Effective output format (`--json` wins over `--formato`).
    pub(crate) fn formato(&self) -> Formato {
        if self.json {
            Formato::Json
        } else {
            self.formato
        }
    }
}

/// Output formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum Formato {
    /// Texto legível, com valores em reais
    Texto,
    /// JSON com os nomes de campo da API
    Json,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Consulta o saldo da conta
    Saldo(SaldoArgs),
    /// Tokens de acesso OAuth
    #[command(
        subcommand,
        subcommand_help_heading = "Comandos",
        subcommand_value_name = "COMANDO"
    )]
    Auth(AuthCommand),
    /// Arquivo de configuração e perfis
    #[command(
        subcommand,
        subcommand_help_heading = "Comandos",
        subcommand_value_name = "COMANDO"
    )]
    Config(ConfigCommand),
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct SaldoArgs {
    /// Data da consulta (AAAA-MM-DD). Sem data: saldo atual, bloqueios e limite
    #[arg(long, value_name = "AAAA-MM-DD", value_parser = parse_data)]
    pub(crate) data: Option<NaiveDate>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum AuthCommand {
    /// Obtém (ou reaproveita do cache) um token e mostra escopos e validade
    Token(TokenArgs),
    /// Remove os tokens em cache
    Limpar(LimparArgs),
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct TokenArgs {
    /// Escopo do token (repita ou separe por vírgulas). Padrão: `escopos` do perfil
    #[arg(long = "escopo", value_name = "ESCOPO", value_delimiter = ',')]
    pub(crate) escopos: Vec<String>,

    /// Ignora o cache e pede um novo token
    #[arg(long)]
    pub(crate) renovar: bool,

    /// Imprime apenas o token em stdout (para uso com curl). Cuidado: é uma credencial
    #[arg(long)]
    pub(crate) exibir: bool,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct LimparArgs {
    /// Remove os tokens de todos os perfis
    #[arg(long)]
    pub(crate) todos: bool,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ConfigCommand {
    /// Cria o arquivo de configuração a partir de um modelo comentado
    Init(InitArgs),
    /// Mostra onde ficam o arquivo de configuração e o cache
    Caminho,
    /// Mostra a configuração efetiva do perfil (segredos ocultos)
    Mostrar,
    /// Confere o arquivo de configuração e o perfil, apontando o que corrigir
    Verificar(VerificarArgs),
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct InitArgs {
    /// Sobrescreve o arquivo se ele já existir
    #[arg(long)]
    pub(crate) forcar: bool,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct VerificarArgs {
    /// Troca as aspas duplas dos caminhos do Windows por aspas simples,
    /// guardando uma cópia do arquivo original (config.toml.bak, sem
    /// sobrescrever uma cópia anterior)
    #[arg(long)]
    pub(crate) corrigir: bool,
}

fn parse_data(value: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| format!("data inválida \"{value}\": use o formato AAAA-MM-DD"))
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn command_definition_is_valid() {
        Cli::command().debug_assert();
        command().debug_assert();
    }

    #[test]
    fn help_is_localized_on_subcommands() {
        let mut root = command();
        let help = root
            .find_subcommand_mut("auth")
            .and_then(|auth| auth.find_subcommand_mut("token"))
            .unwrap()
            .render_help()
            .to_string();
        assert!(help.contains("Uso: inter-pj auth token [OPÇÕES]"), "{help}");
        assert!(!help.contains("Usage:"), "{help}");
        assert!(!help.contains("Possible values"), "{help}");
    }

    #[test]
    fn parses_dates_in_iso_format() {
        assert_eq!(
            parse_data("2026-01-31"),
            Ok(NaiveDate::from_ymd_opt(2026, 1, 31).unwrap())
        );
        assert_eq!(
            parse_data("31/01/2026"),
            Err("data inválida \"31/01/2026\": use o formato AAAA-MM-DD".to_owned())
        );
    }

    #[test]
    fn json_flag_overrides_format() {
        let cli = Cli::try_parse_from(["inter-pj", "saldo", "--json"]).unwrap();
        assert_eq!(cli.global.formato(), Formato::Json);
        let cli = Cli::try_parse_from(["inter-pj", "saldo"]).unwrap();
        assert_eq!(cli.global.formato(), Formato::Texto);
    }

    #[test]
    fn scopes_accept_commas_and_repetition() {
        let cli = Cli::try_parse_from([
            "inter-pj",
            "auth",
            "token",
            "--escopo",
            "extrato.read,pix.read",
            "--escopo",
            "cob.read",
        ])
        .unwrap();
        let Command::Auth(AuthCommand::Token(args)) = cli.command else {
            panic!("comando inesperado");
        };
        assert_eq!(args.escopos, ["extrato.read", "pix.read", "cob.read"]);
    }

    #[test]
    fn client_secret_is_not_accepted_as_flag() {
        let err = Cli::try_parse_from(["inter-pj", "saldo", "--client-secret", "x"]).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::UnknownArgument);
    }
}
