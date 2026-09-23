//! Command line definition.
//!
//! Doc comments in this module are the user-facing `--help` text (in
//! Portuguese), so rustdoc markdown conventions do not apply.
#![allow(clippy::doc_markdown)]

use std::fmt::Write as _;
use std::path::PathBuf;

use chrono::NaiveDate;
use clap::{ArgAction, Args, Parser, Subcommand, ValueEnum};
use inter_pj::RetryPolicy;
use inter_pj::banking::{TAMANHO_PAGINA_MAXIMO, TipoOperacao, TipoTransacao};

use crate::tabela::Separador;

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
        usage.push_str(if command.is_subcommand_required_set() {
            " <COMANDO>"
        } else {
            " [COMANDO]"
        });
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

    /// Formato da saída: texto (padrão), json ou csv
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

    /// Separador do CSV: "," (padrão) ou ";" (Excel em português: vírgula decimal e UTF-8 com BOM)
    #[arg(
        long,
        global = true,
        value_name = "SEPARADOR",
        value_parser = parse_separador,
        default_value = ",",
        hide_default_value = true
    )]
    pub(crate) separador: Separador,

    /// Tentativas por requisição em falhas temporárias (429, 5xx, rede) [padrão: 3]
    #[arg(
        long,
        global = true,
        env = "INTER_TENTATIVAS",
        value_name = "N",
        value_parser = clap::value_parser!(u32).range(1..=10),
        default_value_t = RetryPolicy::DEFAULT_ATTEMPTS,
        hide_default_value = true,
        hide_env_values = true
    )]
    pub(crate) tentativas: u32,

    /// Não repete requisições que falharam (o mesmo que --tentativas 1)
    #[arg(long, global = true)]
    pub(crate) sem_retentativa: bool,

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

    /// Retry policy chosen with `--tentativas` / `--sem-retentativa`.
    pub(crate) fn retry_policy(&self) -> RetryPolicy {
        if self.sem_retentativa {
            RetryPolicy::disabled()
        } else {
            RetryPolicy::new(self.tentativas)
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
    /// CSV (RFC 4180) para planilhas
    Csv,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Consulta o saldo da conta
    Saldo(SaldoArgs),
    /// Extrato da conta: movimentações de um período (até 90 dias por consulta)
    #[command(
        args_conflicts_with_subcommands = true,
        subcommand_help_heading = "Comandos",
        subcommand_value_name = "COMANDO"
    )]
    Extrato(ExtratoArgs),
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

impl Command {
    /// Whether the command lists data that makes sense as CSV.
    pub(crate) fn aceita_csv(&self) -> bool {
        match self {
            Self::Saldo(_) => true,
            Self::Extrato(args) => !matches!(args.comando, Some(ExtratoCommand::Pdf(_))),
            Self::Auth(_) | Self::Config(_) => false,
        }
    }
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
pub(crate) struct ExtratoArgs {
    #[command(subcommand)]
    pub(crate) comando: Option<ExtratoCommand>,

    #[command(flatten)]
    pub(crate) periodo: PeriodoArgs,

    /// Divide períodos maiores que 90 dias em consultas consecutivas
    #[arg(long)]
    pub(crate) dividir_periodo: bool,
}

/// Period of a statement. Without dates: the last 30 days, today included.
#[derive(Debug, Clone, Copy, Args)]
pub(crate) struct PeriodoArgs {
    /// Primeiro dia (AAAA-MM-DD). Padrão: 29 dias antes do fim
    #[arg(long, value_name = "AAAA-MM-DD", value_parser = parse_data)]
    pub(crate) inicio: Option<NaiveDate>,

    /// Último dia (AAAA-MM-DD). Padrão: hoje
    #[arg(long, value_name = "AAAA-MM-DD", value_parser = parse_data)]
    pub(crate) fim: Option<NaiveDate>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ExtratoCommand {
    /// Extrato enriquecido: detalhes de cada transação, filtros e paginação
    Completo(ExtratoCompletoArgs),
    /// Salva o extrato do período em PDF
    Pdf(ExtratoPdfArgs),
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct ExtratoCompletoArgs {
    #[command(flatten)]
    pub(crate) periodo: PeriodoArgs,

    /// Apenas entradas (C) ou apenas saídas (D)
    #[arg(
        long,
        value_enum,
        ignore_case = true,
        value_name = "C|D",
        hide_possible_values = true
    )]
    pub(crate) tipo_operacao: Option<OperacaoArg>,

    /// Apenas um tipo de transação (PIX, PAGAMENTO, TRANSFERENCIA, BOLETO_COBRANCA, TARIFA...)
    #[arg(long, value_name = "TIPO", value_parser = parse_tipo_transacao)]
    pub(crate) tipo_transacao: Option<TipoTransacao>,

    /// Página a consultar, a partir de 0
    #[arg(long, value_name = "N", conflicts_with = "todas_paginas")]
    pub(crate) pagina: Option<u32>,

    /// Transações por página (até 10000; padrão da API: 50)
    #[arg(
        long,
        value_name = "N",
        value_parser = clap::value_parser!(u32).range(1..=i64::from(TAMANHO_PAGINA_MAXIMO))
    )]
    pub(crate) tamanho_pagina: Option<u32>,

    /// Busca todas as páginas (acima de 10.000 transações, usa o modo scroll)
    #[arg(long)]
    pub(crate) todas_paginas: bool,

    /// Divide períodos maiores que 90 dias em consultas consecutivas (requer --todas-paginas)
    #[arg(long, requires = "todas_paginas")]
    pub(crate) dividir_periodo: bool,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct ExtratoPdfArgs {
    #[command(flatten)]
    pub(crate) periodo: PeriodoArgs,

    /// Arquivo a gravar ("-" para a saída padrão). Padrão: extrato-<inicio>-a-<fim>.pdf
    #[arg(short = 'o', long, value_name = "ARQUIVO")]
    pub(crate) saida: Option<PathBuf>,

    /// Sobrescreve o arquivo se ele já existir
    #[arg(long)]
    pub(crate) sobrescrever: bool,
}

/// `--tipo-operacao`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum OperacaoArg {
    /// Entradas (crédito)
    #[value(name = "C", alias = "credito", alias = "entrada")]
    Credito,
    /// Saídas (débito)
    #[value(name = "D", alias = "debito", alias = "saida")]
    Debito,
}

impl From<OperacaoArg> for TipoOperacao {
    fn from(arg: OperacaoArg) -> Self {
        match arg {
            OperacaoArg::Credito => Self::Credito,
            OperacaoArg::Debito => Self::Debito,
        }
    }
}

fn parse_data(value: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| format!("data inválida \"{value}\": use o formato AAAA-MM-DD"))
}

fn parse_separador(value: &str) -> Result<Separador, String> {
    match value {
        "," => Ok(Separador::Virgula),
        ";" => Ok(Separador::PontoEVirgula),
        _ => Err(format!(
            "separador inválido \"{value}\": use \",\" ou \";\""
        )),
    }
}

/// Accepts the API names in any case, with `-` or `_` (`boleto-cobranca`).
fn parse_tipo_transacao(value: &str) -> Result<TipoTransacao, String> {
    let normalized = value.trim().to_uppercase().replace('-', "_");
    TipoTransacao::DOCUMENTADOS
        .iter()
        .find(|tipo| tipo.as_str() == normalized)
        .cloned()
        .ok_or_else(|| {
            let validos: Vec<&str> = TipoTransacao::DOCUMENTADOS
                .iter()
                .map(TipoTransacao::as_str)
                .collect();
            format!(
                "tipo de transação desconhecido \"{value}\"; use um de: {}",
                validos.join(", ")
            )
        })
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
    fn help_of_statements_is_localized() {
        let mut root = command();
        let extrato = root.find_subcommand_mut("extrato").unwrap();
        let help = extrato.render_help().to_string();
        assert!(
            help.contains("Uso: inter-pj extrato [OPÇÕES] [COMANDO]"),
            "{help}"
        );
        let completo = extrato
            .find_subcommand_mut("completo")
            .unwrap()
            .render_long_help()
            .to_string();
        for english in ["Possible values", "[default", "Usage:"] {
            assert!(!help.contains(english), "{help}");
            assert!(!completo.contains(english), "{completo}");
        }
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
    fn csv_separator_and_retries_are_global_options() {
        let cli = Cli::try_parse_from([
            "inter-pj",
            "extrato",
            "--formato",
            "csv",
            "--separador",
            ";",
            "--tentativas",
            "5",
        ])
        .unwrap();
        assert_eq!(cli.global.formato(), Formato::Csv);
        assert_eq!(cli.global.separador, Separador::PontoEVirgula);
        assert_eq!(cli.global.retry_policy().max_attempts(), 5);

        let cli = Cli::try_parse_from(["inter-pj", "saldo", "--sem-retentativa"]).unwrap();
        assert_eq!(cli.global.separador, Separador::Virgula);
        assert_eq!(cli.global.retry_policy(), RetryPolicy::disabled());
        assert!(Cli::try_parse_from(["inter-pj", "saldo", "--separador", "|"]).is_err());
        assert!(Cli::try_parse_from(["inter-pj", "saldo", "--tentativas", "0"]).is_err());
    }

    #[test]
    fn statement_filters_are_parsed() {
        let cli = Cli::try_parse_from([
            "inter-pj",
            "extrato",
            "completo",
            "--inicio",
            "2026-08-01",
            "--tipo-operacao",
            "d",
            "--tipo-transacao",
            "boleto-cobranca",
            "--todas-paginas",
            "--dividir-periodo",
        ])
        .unwrap();
        let Command::Extrato(ExtratoArgs {
            comando: Some(ExtratoCommand::Completo(args)),
            ..
        }) = cli.command
        else {
            panic!("comando inesperado");
        };
        assert_eq!(args.tipo_operacao, Some(OperacaoArg::Debito));
        assert_eq!(args.tipo_transacao, Some(TipoTransacao::BoletoCobranca));
        assert_eq!(args.periodo.inicio, NaiveDate::from_ymd_opt(2026, 8, 1));
        assert!(args.todas_paginas && args.dividir_periodo);
    }

    #[test]
    fn statement_options_are_validated() {
        let parse = |args: &[&str]| {
            let mut full = vec!["inter-pj", "extrato"];
            full.extend_from_slice(args);
            Cli::try_parse_from(full)
        };
        let err = parse(&["completo", "--tipo-transacao", "cripto"]).unwrap_err();
        assert!(err.to_string().contains("PIX"), "{err}");
        // Traversing every page and picking one page are exclusive.
        assert!(parse(&["completo", "--pagina", "2", "--todas-paginas"]).is_err());
        // Splitting the period only makes sense when reading every page.
        assert!(parse(&["completo", "--dividir-periodo"]).is_err());
        assert!(parse(&["completo", "--tamanho-pagina", "10001"]).is_err());
        // Statement options belong to the chosen command.
        assert!(parse(&["--dividir-periodo", "pdf"]).is_err());
        assert!(parse(&["--inicio", "2026-01-01", "--dividir-periodo"]).is_ok());
    }

    #[test]
    fn csv_is_only_offered_for_listings() {
        let command = |args: &[&str]| Cli::try_parse_from(args).unwrap().command;
        assert!(command(&["inter-pj", "saldo"]).aceita_csv());
        assert!(command(&["inter-pj", "extrato"]).aceita_csv());
        assert!(command(&["inter-pj", "extrato", "completo"]).aceita_csv());
        assert!(!command(&["inter-pj", "extrato", "pdf"]).aceita_csv());
        assert!(!command(&["inter-pj", "config", "mostrar"]).aceita_csv());
    }

    #[test]
    fn client_secret_is_not_accepted_as_flag() {
        let err = Cli::try_parse_from(["inter-pj", "saldo", "--client-secret", "x"]).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::UnknownArgument);
    }
}
