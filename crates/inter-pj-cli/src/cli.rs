//! Command line definition.
//!
//! Doc comments in this module are the user-facing `--help` text (in
//! Portuguese), so rustdoc markdown conventions do not apply.
#![allow(clippy::doc_markdown)]

use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::Duration;

use chrono::NaiveDate;
use clap::{ArgAction, ArgGroup, Args, Parser, Subcommand, ValueEnum};
use inter_pj::RetryPolicy;
use inter_pj::banking::{
    DataDoPagamento, IdIdempotente, TAMANHO_PAGINA_MAXIMO, TipoConta, TipoOperacao, TipoTransacao,
};
use inter_pj::boleto::CodigoBarras;
use inter_pj::cobranca::Uf;
use inter_pj::documento::Documento;
use inter_pj::pix::{BrCode, ChavePix};
use rust_decimal::Decimal;

use crate::tabela::Separador;
use crate::valor::{parse_valor, parse_valor_ou_zero};

mod pix;

pub(crate) use pix::{
    DescontoAte, DevedorCobvArgs, EncargosCobvArgs, Momento, PeriodoJuros, PeriodoPixArgs,
    PixCobCommand, PixCobConsultarArgs, PixCobCriarArgs, PixCobListarArgs, PixCobRevisarArgs,
    PixCobvCommand, PixCobvConsultarArgs, PixCobvCriarArgs, PixCobvListarArgs, PixCobvRevisarArgs,
    PixDevolucaoCommand, PixDevolucaoConsultarArgs, PixDevolucaoSolicitarArgs, PixLocCommand,
    PixLocConsultarArgs, PixLocCriarArgs, PixLocDesvincularArgs, PixLocListarArgs,
    PixLoteCobvArquivoArgs, PixLoteCobvCommand, PixLoteCobvConsultarArgs, PixLoteCobvIdArgs,
    PixLoteCobvListarArgs, PixLoteCobvSituacaoArgs, PixPagarQrcodeArgs, PixRecebidoConsultarArgs,
    PixRecebidosCommand, PixRecebidosListarArgs, PixSandboxCommand, PixSandboxPagarArgs, SimNao,
    StatusCobArg,
};

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
    /// Pix: envio e consulta de pagamentos
    #[command(
        subcommand,
        subcommand_help_heading = "Comandos",
        subcommand_value_name = "COMANDO"
    )]
    Pix(PixCommand),
    /// Pagamentos: boletos, contas de consumo e tributos com código de barras
    #[command(
        subcommand,
        subcommand_help_heading = "Comandos",
        subcommand_value_name = "COMANDO"
    )]
    Pagamento(PagamentoCommand),
    /// Cobranças: boletos com Pix para os clientes da empresa
    #[command(
        subcommand,
        subcommand_help_heading = "Comandos",
        subcommand_value_name = "COMANDO"
    )]
    Cobranca(CobrancaCommand),
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
            Self::Saldo(_)
            | Self::Pagamento(
                PagamentoCommand::Boleto(BoletoCommand::Listar(_))
                | PagamentoCommand::Darf(DarfCommand::Listar(_)),
            )
            | Self::Cobranca(CobrancaCommand::Listar(_) | CobrancaCommand::Sumario(_))
            | Self::Pix(
                PixCommand::Cob(PixCobCommand::Listar(_))
                | PixCommand::Cobv(PixCobvCommand::Listar(_))
                | PixCommand::Recebidos(PixRecebidosCommand::Listar(_))
                | PixCommand::Loc(PixLocCommand::Listar(_))
                | PixCommand::LoteCobv(PixLoteCobvCommand::Listar(_)),
            ) => true,
            Self::Extrato(args) => !matches!(args.comando, Some(ExtratoCommand::Pdf(_))),
            Self::Pix(_)
            | Self::Pagamento(_)
            | Self::Cobranca(_)
            | Self::Auth(_)
            | Self::Config(_) => false,
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

#[derive(Debug, Subcommand)]
pub(crate) enum PixCommand {
    /// Envia um Pix (chave, copia e cola ou dados bancários), após mostrar um resumo e pedir confirmação
    Enviar(Box<PixEnviarArgs>),
    /// Consulta o status e o histórico de um Pix enviado (últimos 90 dias)
    Consultar(PixConsultarArgs),
    /// Cobranças imediatas: QR Code dinâmico, para pagar na hora
    #[command(
        subcommand,
        subcommand_help_heading = "Comandos",
        subcommand_value_name = "COMANDO"
    )]
    Cob(PixCobCommand),
    /// Cobranças com vencimento: QR Code para pagar até uma data, com multa, juros, abatimento e desconto
    #[command(
        subcommand,
        subcommand_help_heading = "Comandos",
        subcommand_value_name = "COMANDO"
    )]
    Cobv(PixCobvCommand),
    /// Pix recebidos pela conta, com as suas devoluções
    #[command(
        subcommand,
        subcommand_help_heading = "Comandos",
        subcommand_value_name = "COMANDO"
    )]
    Recebidos(PixRecebidosCommand),
    /// Devoluções de Pix recebidos: tiram dinheiro da conta
    #[command(
        subcommand,
        subcommand_help_heading = "Comandos",
        subcommand_value_name = "COMANDO"
    )]
    Devolucao(PixDevolucaoCommand),
    /// Locations: os endereços dos QR Codes das cobranças
    #[command(
        subcommand,
        subcommand_help_heading = "Comandos",
        subcommand_value_name = "COMANDO"
    )]
    Loc(PixLocCommand),
    /// Lotes de cobranças com vencimento, criados ou alterados a partir de um arquivo
    #[command(
        name = "lote-cobv",
        subcommand,
        subcommand_help_heading = "Comandos",
        subcommand_value_name = "COMANDO"
    )]
    LoteCobv(PixLoteCobvCommand),
    /// Sandbox: pagamentos de cobranças Pix, para testar o fluxo completo (recusados em produção)
    #[command(
        subcommand,
        subcommand_help_heading = "Comandos",
        subcommand_value_name = "COMANDO"
    )]
    Sandbox(PixSandboxCommand),
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct PixConsultarArgs {
    /// Código da solicitação, mostrado pelo `pix enviar`
    #[arg(value_name = "CODIGO")]
    pub(crate) codigo: String,

    /// Consulta de novo, a cada 6 segundos, até o Pix chegar a um status final
    #[arg(long)]
    pub(crate) aguardar: bool,

    /// Tempo máximo de espera com --aguardar: 60s, 5m, 1h [padrão: 60s]
    #[arg(
        long,
        value_name = "DURACAO",
        value_parser = parse_duracao,
        default_value = "60s",
        hide_default_value = true,
        requires = "aguardar"
    )]
    pub(crate) timeout: Duration,
}

#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("destino")
        .required(true)
        .args(["chave", "copia_e_cola", "ispb"])
))]
pub(crate) struct PixEnviarArgs {
    /// Chave Pix: CPF, CNPJ, e-mail, celular (+55DD9NNNNNNNN) ou chave aleatória
    #[arg(
        long,
        value_name = "CHAVE",
        value_parser = parse_chave,
        help_heading = "Destino (escolha um)"
    )]
    pub(crate) chave: Option<ChavePix>,

    /// Código Pix copia e cola, entre aspas
    #[arg(
        long,
        value_name = "CODIGO",
        value_parser = parse_copia_e_cola,
        help_heading = "Destino (escolha um)"
    )]
    pub(crate) copia_e_cola: Option<CopiaECola>,

    /// ISPB da instituição, 8 dígitos (dados bancários, para quem não tem chave)
    #[arg(
        long,
        value_name = "ISPB",
        value_parser = parse_ispb,
        requires_all = ["agencia", "conta", "tipo_conta", "documento", "nome"],
        help_heading = "Destino (escolha um)"
    )]
    pub(crate) ispb: Option<String>,

    /// Agência, sem o dígito verificador
    #[arg(
        long,
        value_name = "AGENCIA",
        value_parser = parse_agencia,
        requires = "ispb",
        help_heading = "Dados bancários"
    )]
    pub(crate) agencia: Option<String>,

    /// Conta com o dígito verificador (1234567, 123456-7 ou 123456-X)
    #[arg(
        long,
        value_name = "CONTA",
        value_parser = parse_conta,
        requires = "ispb",
        help_heading = "Dados bancários"
    )]
    pub(crate) conta: Option<String>,

    /// Tipo da conta: corrente, poupanca, salario ou pagamento
    #[arg(
        long,
        value_enum,
        ignore_case = true,
        value_name = "TIPO",
        hide_possible_values = true,
        requires = "ispb",
        help_heading = "Dados bancários"
    )]
    pub(crate) tipo_conta: Option<TipoContaArg>,

    /// CPF ou CNPJ do titular da conta
    #[arg(
        long,
        value_name = "CPF/CNPJ",
        value_parser = parse_documento,
        requires = "ispb",
        help_heading = "Dados bancários"
    )]
    pub(crate) documento: Option<Documento>,

    /// Nome do titular da conta
    #[arg(
        long,
        value_name = "NOME",
        requires = "ispb",
        help_heading = "Dados bancários"
    )]
    pub(crate) nome: Option<String>,

    /// Valor em reais: 150,00, 1.500,00 ou 150.00 (dispensável se o copia e cola já traz o valor)
    #[arg(
        long,
        value_name = "VALOR",
        value_parser = parse_valor,
        help_heading = "Pagamento"
    )]
    pub(crate) valor: Option<Decimal>,

    /// Mensagem ao recebedor (até 140 caracteres)
    #[arg(long, value_name = "TEXTO", help_heading = "Pagamento")]
    pub(crate) descricao: Option<String>,

    /// Agenda o Pix para o dia (AAAA-MM-DD). Padrão: agora
    #[arg(
        long,
        value_name = "AAAA-MM-DD",
        value_parser = parse_data,
        help_heading = "Pagamento"
    )]
    pub(crate) data: Option<NaiveDate>,

    /// Confirma sem perguntar (para scripts)
    #[arg(long, conflicts_with = "simular", help_heading = "Segurança")]
    pub(crate) sim: bool,

    /// Mostra a requisição que seria enviada, sem enviar nada
    #[arg(long, help_heading = "Segurança")]
    pub(crate) simular: bool,

    /// Chave de idempotência (UUID) de um envio anterior: repete-o sem pagar duas vezes
    #[arg(
        long,
        value_name = "UUID",
        value_parser = parse_id_idempotente,
        help_heading = "Segurança"
    )]
    pub(crate) id_idempotente: Option<IdIdempotente>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum CobrancaCommand {
    /// Emite uma cobrança (pelas opções ou por --arquivo), após mostrar um resumo e pedir confirmação
    Emitir(Box<CobrancaEmitirArgs>),
    /// Imprime um arquivo JSON de exemplo para --arquivo, com dados fictícios
    Modelo(CobrancaModeloArgs),
    /// Cobranças de um período (padrão: com vencimento nos últimos 30 dias), com filtros
    Listar(CobrancaListarArgs),
    /// Quantidade e valor das cobranças de um período, por situação
    Sumario(CobrancaSumarioArgs),
    /// Mostra uma cobrança: situação, valores, boleto e Pix (com o QR Code, se pedido)
    Consultar(CobrancaConsultarArgs),
    /// Grava o PDF de uma cobrança, com o boleto e o QR Code do Pix
    Pdf(CobrancaPdfArgs),
    /// Cancela uma cobrança, após mostrá-la e pedir confirmação
    Cancelar(CobrancaCancelarArgs),
    /// Altera o valor ou o vencimento de uma cobrança, após mostrar o antes e o depois
    Editar(CobrancaEditarArgs),
    /// Mostra em que pé está uma alteração feita com `cobranca editar`
    Edicao(CobrancaEdicaoArgs),
    /// Sandbox: paga uma cobrança, para testar o fluxo completo (recusado em produção)
    Pagar(CobrancaPagarArgs),
}

/// Where the charge comes from: a file, or the options.
#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("origem")
        .required(true)
        .args(["arquivo", "seu_numero"])
))]
pub(crate) struct CobrancaEmitirArgs {
    /// Arquivo JSON com a cobrança, nos campos da API ("-" para a entrada padrão); veja `cobranca modelo`
    #[arg(long, value_name = "ARQUIVO", help_heading = "Origem (escolha uma)")]
    pub(crate) arquivo: Option<PathBuf>,

    /// Seu número da cobrança, até 15 caracteres (ex.: o número da nota)
    #[arg(
        long,
        value_name = "TEXTO",
        requires_all = ["valor", "vencimento", "pagador_documento", "pagador_nome", "pagador_endereco", "pagador_cidade", "pagador_uf", "pagador_cep"],
        help_heading = "Origem (escolha uma)"
    )]
    pub(crate) seu_numero: Option<String>,

    /// Valor: 150,00, 1.500,00 ou 150.00 (de R$ 2,50 a R$ 99.999.999,99)
    #[arg(long, value_name = "VALOR", value_parser = parse_valor, requires = "seu_numero", help_heading = "Cobrança")]
    pub(crate) valor: Option<Decimal>,

    /// Vencimento (AAAA-MM-DD), hoje ou depois
    #[arg(long, value_name = "AAAA-MM-DD", value_parser = parse_data, requires = "seu_numero", help_heading = "Cobrança")]
    pub(crate) vencimento: Option<NaiveDate>,

    /// Dias após o vencimento até a cobrança não paga ser cancelada, de 0 a 60 [padrão: 0]
    #[arg(
        long,
        value_name = "DIAS",
        requires = "seu_numero",
        help_heading = "Cobrança"
    )]
    pub(crate) dias_agenda: Option<u32>,

    /// Desconto por pagar antes: percentual (2%) ou valor (10,00)
    #[arg(long, value_name = "TAXA|VALOR", value_parser = parse_taxa_ou_valor, requires = "seu_numero", help_heading = "Cobrança")]
    pub(crate) desconto: Option<TaxaOuValor>,

    /// Dias antes do vencimento até quando o desconto vale [padrão: 0, até o vencimento]
    #[arg(
        long,
        value_name = "DIAS",
        requires = "desconto",
        help_heading = "Cobrança"
    )]
    pub(crate) desconto_dias: Option<u32>,

    /// Multa por atraso: percentual (2%) ou valor (4,00)
    #[arg(long, value_name = "TAXA|VALOR", value_parser = parse_taxa_ou_valor, requires = "seu_numero", help_heading = "Cobrança")]
    pub(crate) multa: Option<TaxaOuValor>,

    /// Juros por atraso: percentual ao mês (1%) ou valor por dia (0,33)
    #[arg(long, value_name = "TAXA|VALOR", value_parser = parse_taxa_ou_valor, requires = "seu_numero", help_heading = "Cobrança")]
    pub(crate) juros: Option<TaxaOuValor>,

    /// Linha impressa no boleto, até 78 caracteres; repita para até 5 linhas
    #[arg(long, value_name = "TEXTO", action = ArgAction::Append, requires = "seu_numero", help_heading = "Cobrança")]
    pub(crate) mensagem: Vec<String>,

    /// Formas de recebimento: boleto, pix ou boleto,pix [padrão: boleto e, se a conta tiver chave, Pix]
    #[arg(
        long,
        value_name = "FORMAS",
        value_enum,
        value_delimiter = ',',
        ignore_case = true,
        requires = "seu_numero",
        help_heading = "Cobrança"
    )]
    pub(crate) receber_com: Vec<FormaArg>,

    /// CPF ou CNPJ do pagador
    #[arg(long, value_name = "CPF/CNPJ", value_parser = parse_documento, requires = "seu_numero", help_heading = "Pagador")]
    pub(crate) pagador_documento: Option<Documento>,

    /// Nome do pagador, até 100 caracteres
    #[arg(
        long,
        value_name = "NOME",
        requires = "seu_numero",
        help_heading = "Pagador"
    )]
    pub(crate) pagador_nome: Option<String>,

    /// Rua do pagador, até 100 caracteres
    #[arg(
        long,
        value_name = "RUA",
        requires = "seu_numero",
        help_heading = "Pagador"
    )]
    pub(crate) pagador_endereco: Option<String>,

    /// Número no endereço
    #[arg(
        long,
        value_name = "NUMERO",
        requires = "seu_numero",
        help_heading = "Pagador"
    )]
    pub(crate) pagador_numero: Option<String>,

    /// Complemento do endereço
    #[arg(
        long,
        value_name = "TEXTO",
        requires = "seu_numero",
        help_heading = "Pagador"
    )]
    pub(crate) pagador_complemento: Option<String>,

    /// Bairro
    #[arg(
        long,
        value_name = "BAIRRO",
        requires = "seu_numero",
        help_heading = "Pagador"
    )]
    pub(crate) pagador_bairro: Option<String>,

    /// Cidade
    #[arg(
        long,
        value_name = "CIDADE",
        requires = "seu_numero",
        help_heading = "Pagador"
    )]
    pub(crate) pagador_cidade: Option<String>,

    /// UF (sigla do estado)
    #[arg(long, value_name = "UF", value_parser = parse_uf, requires = "seu_numero", help_heading = "Pagador")]
    pub(crate) pagador_uf: Option<Uf>,

    /// CEP: 30110-000 ou 30110000
    #[arg(long, value_name = "CEP", value_parser = parse_cep, requires = "seu_numero", help_heading = "Pagador")]
    pub(crate) pagador_cep: Option<String>,

    /// E-mail do pagador
    #[arg(
        long,
        value_name = "EMAIL",
        requires = "seu_numero",
        help_heading = "Pagador"
    )]
    pub(crate) pagador_email: Option<String>,

    /// Telefone com DDD: (31) 99999-9999
    #[arg(long, value_name = "TELEFONE", value_parser = parse_telefone, requires = "seu_numero", help_heading = "Pagador")]
    pub(crate) pagador_telefone: Option<Telefone>,

    #[command(flatten)]
    pub(crate) depois: DepoisDaEmissaoArgs,

    /// Confirma sem perguntar (para scripts)
    #[arg(long, conflicts_with = "simular", help_heading = "Segurança")]
    pub(crate) sim: bool,

    /// Mostra a requisição que seria enviada, sem enviar nada
    #[arg(long, conflicts_with = "aguardar", help_heading = "Segurança")]
    pub(crate) simular: bool,
}

/// `cobranca emitir`: what to do once the charge is requested.
#[derive(Debug, Args)]
#[command(next_help_heading = "Depois da emissão")]
pub(crate) struct DepoisDaEmissaoArgs {
    /// Espera a emissão (consulta a cada 6 segundos) e mostra o boleto e o Pix
    #[arg(long)]
    pub(crate) aguardar: bool,

    /// Tempo máximo de espera com --aguardar: 60s, 5m [padrão: 60s]
    #[arg(
        long,
        value_name = "DURACAO",
        value_parser = parse_duracao,
        default_value = "60s",
        hide_default_value = true,
        requires = "aguardar"
    )]
    pub(crate) timeout: Duration,

    /// Com --aguardar, desenha o QR Code do Pix no terminal
    #[arg(long, requires = "aguardar")]
    pub(crate) qrcode: bool,

    /// Com --aguardar, grava o QR Code do Pix em PNG ("-" para a saída padrão)
    #[arg(long, value_name = "ARQUIVO", requires = "aguardar")]
    pub(crate) qrcode_png: Option<PathBuf>,

    /// Sobrescreve a imagem se ela já existir
    #[arg(long, requires = "qrcode_png")]
    pub(crate) sobrescrever: bool,
}

#[derive(Debug, Args)]
pub(crate) struct CobrancaModeloArgs {}

/// `--desconto`, `--multa` and `--juros`: a percentage (`2%`) or an amount (`4,00`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TaxaOuValor {
    Taxa(Decimal),
    Valor(Decimal),
}

/// A phone split into DDD and number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Telefone {
    pub(crate) ddd: String,
    pub(crate) numero: String,
}

/// `--receber-com`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum FormaArg {
    Boleto,
    Pix,
}

/// Filters shared by `cobranca listar` and `cobranca sumario`.
#[derive(Debug, Clone, Args)]
pub(crate) struct FiltroCobrancaArgs {
    #[command(flatten)]
    pub(crate) periodo: PeriodoArgs,

    /// Data a que o período se refere: vencimento (padrão), emissao ou pagamento
    #[arg(
        long,
        value_enum,
        ignore_case = true,
        value_name = "DATA",
        hide_possible_values = true
    )]
    pub(crate) filtrar_por: Option<FiltrarDataPorArg>,

    /// Apenas nesta situação: a-receber, recebida, atrasada, cancelada, expirada, marcada-recebida, em-processamento, falha-emissao ou protesto
    #[arg(
        long,
        value_enum,
        ignore_case = true,
        value_name = "SITUACAO",
        hide_possible_values = true
    )]
    pub(crate) situacao: Option<SituacaoArg>,

    /// Apenas deste pagador (nome)
    #[arg(long, value_name = "NOME")]
    pub(crate) pagador: Option<String>,

    /// Apenas deste pagador (CPF ou CNPJ)
    #[arg(long, value_name = "CPF/CNPJ", value_parser = parse_documento)]
    pub(crate) documento: Option<Documento>,

    /// Apenas com este seu número
    #[arg(long, value_name = "TEXTO")]
    pub(crate) seu_numero: Option<String>,

    /// Apenas deste tipo: simples, parcelada ou recorrente
    #[arg(
        long,
        value_enum,
        ignore_case = true,
        value_name = "TIPO",
        hide_possible_values = true
    )]
    pub(crate) tipo: Option<TipoCobrancaArg>,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct CobrancaListarArgs {
    #[command(flatten)]
    pub(crate) filtro: FiltroCobrancaArgs,

    /// Ordem: pagador (padrão), vencimento, emissao, valor, situacao, seu-numero, tipo ou codigo
    #[arg(
        long,
        value_enum,
        ignore_case = true,
        value_name = "CAMPO",
        hide_possible_values = true
    )]
    pub(crate) ordenar_por: Option<OrdenarPorArg>,

    /// Ordem decrescente
    #[arg(long)]
    pub(crate) decrescente: bool,

    /// Só esta página (começa em 0), em vez de todas
    #[arg(long, value_name = "N")]
    pub(crate) pagina: Option<u32>,

    /// Cobranças por página com --pagina, até 1000 [padrão: 100]
    #[arg(
        long,
        value_name = "N",
        requires = "pagina",
        value_parser = clap::value_parser!(u32).range(1..=1000)
    )]
    pub(crate) itens_por_pagina: Option<u32>,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct CobrancaSumarioArgs {
    #[command(flatten)]
    pub(crate) filtro: FiltroCobrancaArgs,
}

/// `--filtrar-por` of the charges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum FiltrarDataPorArg {
    /// Data de vencimento (padrão)
    Vencimento,
    /// Data de emissão
    #[value(name = "emissao", alias = "emissão")]
    Emissao,
    /// Data do pagamento
    Pagamento,
}

/// `--situacao`: the words of the text output, or the API's codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum SituacaoArg {
    #[value(name = "a-receber", alias = "a_receber")]
    AReceber,
    #[value(name = "recebida", alias = "recebido")]
    Recebida,
    #[value(name = "atrasada", alias = "atrasado")]
    Atrasada,
    #[value(name = "cancelada", alias = "cancelado")]
    Cancelada,
    #[value(name = "expirada", alias = "expirado")]
    Expirada,
    #[value(name = "marcada-recebida", alias = "marcado_recebido")]
    MarcadaRecebida,
    #[value(name = "em-processamento", alias = "em_processamento")]
    EmProcessamento,
    #[value(name = "falha-emissao", alias = "falha_emissao")]
    FalhaEmissao,
    Protesto,
}

/// `--tipo` of the charges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum TipoCobrancaArg {
    #[value(name = "simples")]
    Simples,
    #[value(name = "parcelada", alias = "parcelado")]
    Parcelada,
    #[value(name = "recorrente")]
    Recorrente,
}

/// `--ordenar-por` of the charges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum OrdenarPorArg {
    Pagador,
    Vencimento,
    #[value(name = "emissao", alias = "emissão")]
    Emissao,
    Valor,
    #[value(name = "situacao", alias = "situação")]
    Situacao,
    #[value(name = "seu-numero", alias = "seu-número")]
    SeuNumero,
    Tipo,
    #[value(name = "codigo", alias = "código")]
    Codigo,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct CobrancaConsultarArgs {
    /// Código da cobrança (codigoSolicitacao), mostrado por `cobranca emitir` e `cobranca listar`
    #[arg(value_name = "CODIGO")]
    pub(crate) codigo: String,

    /// Desenha no terminal o QR Code do Pix, para ler com o celular
    #[arg(long)]
    pub(crate) qrcode: bool,

    /// Grava o QR Code do Pix em uma imagem PNG ("-" para a saída padrão)
    #[arg(long, value_name = "ARQUIVO")]
    pub(crate) qrcode_png: Option<PathBuf>,

    /// Sobrescreve a imagem se ela já existir
    #[arg(long, requires = "qrcode_png")]
    pub(crate) sobrescrever: bool,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct CobrancaPdfArgs {
    /// Código da cobrança (codigoSolicitacao)
    #[arg(value_name = "CODIGO")]
    pub(crate) codigo: String,

    /// Arquivo a gravar ("-" para a saída padrão). Padrão: cobranca-<codigo>.pdf
    #[arg(short = 'o', long, value_name = "ARQUIVO")]
    pub(crate) saida: Option<PathBuf>,

    /// Sobrescreve o arquivo se ele já existir
    #[arg(long)]
    pub(crate) sobrescrever: bool,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct CobrancaCancelarArgs {
    /// Código da cobrança (codigoSolicitacao)
    #[arg(value_name = "CODIGO")]
    pub(crate) codigo: String,

    /// Motivo do cancelamento, até 50 caracteres
    #[arg(long, value_name = "TEXTO", value_parser = parse_motivo)]
    pub(crate) motivo: String,

    /// Confirma sem perguntar (para scripts)
    #[arg(long, help_heading = "Segurança")]
    pub(crate) sim: bool,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
#[command(group(
    ArgGroup::new("alteracao")
        .required(true)
        .multiple(true)
        .args(["valor", "vencimento"])
))]
pub(crate) struct CobrancaEditarArgs {
    /// Código da cobrança (codigoSolicitacao)
    #[arg(value_name = "CODIGO")]
    pub(crate) codigo: String,

    /// Novo valor: 150,00, 1.500,00 ou 150.00 (de R$ 2,50 a R$ 99.999.999,99)
    #[arg(long, value_name = "VALOR", value_parser = parse_valor)]
    pub(crate) valor: Option<Decimal>,

    /// Novo vencimento (AAAA-MM-DD), hoje ou depois
    #[arg(long, value_name = "AAAA-MM-DD", value_parser = parse_data)]
    pub(crate) vencimento: Option<NaiveDate>,

    #[command(flatten)]
    pub(crate) espera: EsperaEdicaoArgs,

    /// Confirma sem perguntar (para scripts)
    #[arg(long, help_heading = "Segurança")]
    pub(crate) sim: bool,
}

/// `--aguardar` of `cobranca editar` and `cobranca edicao`.
#[derive(Debug, Clone, Copy, Args)]
pub(crate) struct EsperaEdicaoArgs {
    /// Espera o fim da alteração (consulta a cada 6 segundos)
    #[arg(long)]
    pub(crate) aguardar: bool,

    /// Tempo máximo de espera com --aguardar: 60s, 5m [padrão: 60s]
    #[arg(
        long,
        value_name = "DURACAO",
        value_parser = parse_duracao,
        default_value = "60s",
        hide_default_value = true,
        requires = "aguardar"
    )]
    pub(crate) timeout: Duration,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct CobrancaEdicaoArgs {
    /// Código da alteração (codigoEdicao), mostrado por `cobranca editar`
    #[arg(value_name = "CODIGO_EDICAO")]
    pub(crate) codigo_edicao: String,

    #[command(flatten)]
    pub(crate) espera: EsperaEdicaoArgs,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct CobrancaPagarArgs {
    /// Código da cobrança (codigoSolicitacao)
    #[arg(value_name = "CODIGO")]
    pub(crate) codigo: String,

    /// Como pagar: boleto (código de barras) ou pix (QR Code)
    #[arg(
        long,
        value_enum,
        ignore_case = true,
        value_name = "FORMA",
        hide_possible_values = true
    )]
    pub(crate) com: FormaArg,
}

#[derive(Debug, Subcommand)]
pub(crate) enum PagamentoCommand {
    /// Boletos, contas de consumo e tributos com código de barras
    #[command(
        subcommand,
        subcommand_help_heading = "Comandos",
        subcommand_value_name = "COMANDO"
    )]
    Boleto(BoletoCommand),
    /// DARF sem código de barras (tributos federais)
    #[command(
        subcommand,
        subcommand_help_heading = "Comandos",
        subcommand_value_name = "COMANDO"
    )]
    Darf(DarfCommand),
    /// Lotes de 2 a 150 boletos e DARFs, a partir de um arquivo JSON ou CSV
    #[command(
        subcommand,
        subcommand_help_heading = "Comandos",
        subcommand_value_name = "COMANDO"
    )]
    Lote(LoteCommand),
}

#[derive(Debug, Subcommand)]
pub(crate) enum LoteCommand {
    /// Envia os pagamentos de um arquivo, após conferir todos, mostrar um resumo e pedir confirmação
    Enviar(LoteEnviarArgs),
    /// Mostra um lote e o status de cada pagamento
    Consultar(LoteConsultarArgs),
    /// Imprime um arquivo de exemplo, com dados fictícios: json (padrão) ou csv
    Modelo(LoteModeloArgs),
}

#[derive(Debug, Args)]
pub(crate) struct LoteEnviarArgs {
    /// Arquivo JSON ou CSV com os pagamentos ("-" para a entrada padrão); veja `pagamento lote modelo`
    #[arg(long, value_name = "ARQUIVO", help_heading = "Lote")]
    pub(crate) arquivo: PathBuf,

    /// Seu identificador do lote (até 30 caracteres); substitui o do arquivo
    #[arg(long, value_name = "TEXTO", help_heading = "Lote")]
    pub(crate) identificador: Option<String>,

    /// Confirma sem perguntar (para scripts)
    #[arg(long, conflicts_with = "simular", help_heading = "Segurança")]
    pub(crate) sim: bool,

    /// Mostra a requisição que seria enviada, sem enviar nada
    #[arg(long, help_heading = "Segurança")]
    pub(crate) simular: bool,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct LoteConsultarArgs {
    /// Identificador do lote, mostrado por `pagamento lote enviar`
    #[arg(value_name = "ID_LOTE")]
    pub(crate) id_lote: String,

    /// Consulta de novo, a cada 6 segundos, até o lote ser processado
    #[arg(long)]
    pub(crate) aguardar: bool,

    /// Tempo máximo de espera com --aguardar: 60s, 5m, 1h [padrão: 5m]
    #[arg(
        long,
        value_name = "DURACAO",
        value_parser = parse_duracao,
        default_value = "5m",
        hide_default_value = true,
        requires = "aguardar"
    )]
    pub(crate) timeout: Duration,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct LoteModeloArgs {
    /// Formato do arquivo: json ou csv (separado por ";", para o Excel em português)
    #[arg(
        value_name = "FORMATO",
        value_enum,
        ignore_case = true,
        default_value_t = TipoArquivo::Json,
        hide_possible_values = true,
        hide_default_value = true
    )]
    pub(crate) tipo: TipoArquivo,
}

/// Format of a batch file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum TipoArquivo {
    /// JSON, nos campos da API
    Json,
    /// CSV, um pagamento por linha
    Csv,
}

#[derive(Debug, Subcommand)]
pub(crate) enum DarfCommand {
    /// Paga um DARF (pelas opções ou por --arquivo), após mostrar um resumo e pedir confirmação
    Pagar(Box<DarfPagarArgs>),
    /// Lista os pagamentos de DARF (padrão: incluídos nos últimos 30 dias)
    Listar(DarfListarArgs),
}

#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("origem")
        .required(true)
        .args(["arquivo", "codigo_receita"])
))]
pub(crate) struct DarfPagarArgs {
    /// Arquivo JSON com o DARF, nos campos da API ("-" para a entrada padrão)
    #[arg(long, value_name = "ARQUIVO", help_heading = "Origem (escolha uma)")]
    pub(crate) arquivo: Option<PathBuf>,

    /// Código da receita, 4 dígitos (ex.: 0220)
    #[arg(
        long,
        value_name = "CODIGO",
        requires_all = ["contribuinte", "nome_empresa", "periodo_apuracao", "vencimento", "valor_principal", "referencia", "descricao"],
        help_heading = "Origem (escolha uma)"
    )]
    pub(crate) codigo_receita: Option<String>,

    /// CPF ou CNPJ do contribuinte
    #[arg(
        long,
        value_name = "CPF/CNPJ",
        value_parser = parse_documento,
        requires = "codigo_receita",
        help_heading = "DARF"
    )]
    pub(crate) contribuinte: Option<Documento>,

    /// Nome do contribuinte (até 100 caracteres)
    #[arg(
        long,
        value_name = "NOME",
        requires = "codigo_receita",
        help_heading = "DARF"
    )]
    pub(crate) nome_empresa: Option<String>,

    /// Telefone do contribuinte (opcional, até 50 caracteres)
    #[arg(
        long,
        value_name = "TELEFONE",
        requires = "codigo_receita",
        help_heading = "DARF"
    )]
    pub(crate) telefone: Option<String>,

    /// Período de apuração (AAAA-MM-DD)
    #[arg(
        long,
        value_name = "AAAA-MM-DD",
        value_parser = parse_data,
        requires = "codigo_receita",
        help_heading = "DARF"
    )]
    pub(crate) periodo_apuracao: Option<NaiveDate>,

    /// Vencimento (AAAA-MM-DD)
    #[arg(
        long,
        value_name = "AAAA-MM-DD",
        value_parser = parse_data,
        requires = "codigo_receita",
        help_heading = "DARF"
    )]
    pub(crate) vencimento: Option<NaiveDate>,

    /// Número de referência (só dígitos, até 30)
    #[arg(
        long,
        value_name = "NUMERO",
        requires = "codigo_receita",
        help_heading = "DARF"
    )]
    pub(crate) referencia: Option<String>,

    /// Descrição do pagamento (até 1000 caracteres)
    #[arg(
        long,
        value_name = "TEXTO",
        requires = "codigo_receita",
        help_heading = "DARF"
    )]
    pub(crate) descricao: Option<String>,

    /// Valor principal: 150,00, 1.500,00 ou 150.00
    #[arg(
        long,
        value_name = "VALOR",
        value_parser = parse_valor,
        requires = "codigo_receita",
        help_heading = "Valores"
    )]
    pub(crate) valor_principal: Option<Decimal>,

    /// Multa, se houver
    #[arg(
        long,
        value_name = "VALOR",
        value_parser = parse_valor_ou_zero,
        requires = "codigo_receita",
        help_heading = "Valores"
    )]
    pub(crate) multa: Option<Decimal>,

    /// Juros, se houver
    #[arg(
        long,
        value_name = "VALOR",
        value_parser = parse_valor_ou_zero,
        requires = "codigo_receita",
        help_heading = "Valores"
    )]
    pub(crate) juros: Option<Decimal>,

    /// Confirma sem perguntar (para scripts)
    #[arg(long, conflicts_with = "simular", help_heading = "Segurança")]
    pub(crate) sim: bool,

    /// Mostra a requisição que seria enviada, sem enviar nada
    #[arg(long, help_heading = "Segurança")]
    pub(crate) simular: bool,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct DarfListarArgs {
    /// Primeiro dia de pagamento (AAAA-MM-DD). Só com --fim: 29 dias antes dele
    #[arg(long, value_name = "AAAA-MM-DD", value_parser = parse_data)]
    pub(crate) inicio: Option<NaiveDate>,

    /// Último dia de pagamento (AAAA-MM-DD). Só com --inicio: hoje
    #[arg(long, value_name = "AAAA-MM-DD", value_parser = parse_data)]
    pub(crate) fim: Option<NaiveDate>,

    /// Apenas os DARFs deste código da receita
    #[arg(long, value_name = "CODIGO")]
    pub(crate) codigo_receita: Option<String>,

    /// Apenas o pagamento com este código de solicitação
    #[arg(long, value_name = "UUID", value_parser = parse_uuid)]
    pub(crate) codigo_solicitacao: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum BoletoCommand {
    /// Paga ou agenda um boleto, conta ou tributo, após mostrar um resumo e pedir confirmação
    Pagar(BoletoPagarArgs),
    /// Lista os pagamentos por código de barras de um período (até 90 dias; padrão: incluídos nos últimos 30 dias)
    Listar(BoletoListarArgs),
    /// Cancela um pagamento agendado, após mostrá-lo e pedir confirmação
    Cancelar(BoletoCancelarArgs),
}

#[derive(Debug, Args)]
pub(crate) struct BoletoPagarArgs {
    /// Linha digitável ou código de barras, entre aspas se tiver espaços
    #[arg(value_name = "CODIGO", value_parser = parse_codigo_barras)]
    pub(crate) codigo: CodigoBarras,

    /// Valor a pagar: 150,00, 1.500,00 ou 150.00. Padrão: o valor do código
    #[arg(
        long,
        value_name = "VALOR",
        value_parser = parse_valor,
        help_heading = "Pagamento"
    )]
    pub(crate) valor: Option<Decimal>,

    /// Vencimento (AAAA-MM-DD). Padrão: o do boleto; contas e tributos precisam informar
    #[arg(
        long,
        value_name = "AAAA-MM-DD",
        value_parser = parse_data,
        help_heading = "Pagamento"
    )]
    pub(crate) vencimento: Option<NaiveDate>,

    /// Agenda o pagamento para o dia (AAAA-MM-DD). Padrão: agora
    #[arg(
        long,
        value_name = "AAAA-MM-DD",
        value_parser = parse_data,
        help_heading = "Pagamento"
    )]
    pub(crate) data: Option<NaiveDate>,

    /// CPF ou CNPJ do beneficiário, para a API conferir antes de pagar
    #[arg(
        long,
        value_name = "CPF/CNPJ",
        value_parser = parse_documento,
        help_heading = "Pagamento"
    )]
    pub(crate) beneficiario: Option<Documento>,

    /// Confirma sem perguntar (para scripts)
    #[arg(long, conflicts_with = "simular", help_heading = "Segurança")]
    pub(crate) sim: bool,

    /// Mostra a requisição que seria enviada, sem enviar nada
    #[arg(long, help_heading = "Segurança")]
    pub(crate) simular: bool,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct BoletoListarArgs {
    #[command(flatten)]
    pub(crate) periodo: PeriodoArgs,

    /// Data a que o período se refere: inclusao (padrão), pagamento ou vencimento
    #[arg(
        long,
        value_enum,
        ignore_case = true,
        value_name = "DATA",
        hide_possible_values = true
    )]
    pub(crate) filtrar_por: Option<DataDoPagamentoArg>,

    /// Apenas os pagamentos deste boleto ou conta (linha digitável ou código de barras)
    #[arg(long, value_name = "CODIGO", value_parser = parse_codigo_barras)]
    pub(crate) codigo: Option<CodigoBarras>,

    /// Apenas o pagamento com este código de transação
    #[arg(long, value_name = "UUID", value_parser = parse_uuid)]
    pub(crate) codigo_transacao: Option<String>,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct BoletoCancelarArgs {
    /// Código da transação do agendamento, mostrado por `pagamento boleto listar`
    #[arg(value_name = "CODIGO_TRANSACAO", value_parser = parse_uuid)]
    pub(crate) codigo_transacao: String,

    /// Confirma sem perguntar (para scripts)
    #[arg(long, help_heading = "Segurança")]
    pub(crate) sim: bool,
}

/// `--filtrar-por`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum DataDoPagamentoArg {
    /// Data em que o pagamento foi incluído (padrão)
    #[value(name = "inclusao", alias = "inclusão")]
    Inclusao,
    /// Data em que foi pago
    Pagamento,
    /// Data de vencimento
    Vencimento,
}

impl From<DataDoPagamentoArg> for DataDoPagamento {
    fn from(arg: DataDoPagamentoArg) -> Self {
        match arg {
            DataDoPagamentoArg::Inclusao => Self::Inclusao,
            DataDoPagamentoArg::Pagamento => Self::Pagamento,
            DataDoPagamentoArg::Vencimento => Self::Vencimento,
        }
    }
}

/// A copia e cola code, as given and decoded.
#[derive(Debug, Clone)]
pub(crate) struct CopiaECola {
    pub(crate) codigo: String,
    pub(crate) brcode: BrCode,
}

/// `--tipo-conta`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum TipoContaArg {
    /// Conta corrente
    #[value(name = "corrente", alias = "cc")]
    Corrente,
    /// Conta poupança
    #[value(name = "poupanca", alias = "poupança")]
    Poupanca,
    /// Conta salário
    #[value(name = "salario", alias = "salário")]
    Salario,
    /// Conta de pagamento (bancos digitais e carteiras)
    Pagamento,
}

impl From<TipoContaArg> for TipoConta {
    fn from(arg: TipoContaArg) -> Self {
        match arg {
            TipoContaArg::Corrente => Self::ContaCorrente,
            TipoContaArg::Poupanca => Self::ContaPoupanca,
            TipoContaArg::Salario => Self::ContaSalario,
            TipoContaArg::Pagamento => Self::ContaPagamento,
        }
    }
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

fn parse_chave(value: &str) -> Result<ChavePix, String> {
    ChavePix::parse(value).map_err(|err| err.to_string())
}

fn parse_copia_e_cola(value: &str) -> Result<CopiaECola, String> {
    let codigo = value.trim();
    let brcode = BrCode::parse(codigo).map_err(|err| err.to_string())?;
    Ok(CopiaECola {
        codigo: codigo.to_owned(),
        brcode,
    })
}

fn parse_ispb(value: &str) -> Result<String, String> {
    let ispb = value.trim();
    if ispb.len() == 8 && ispb.bytes().all(|b| b.is_ascii_digit()) {
        Ok(ispb.to_owned())
    } else {
        Err("o ISPB tem 8 dígitos (ex.: 00416968, do Inter)".to_owned())
    }
}

/// Without the usual punctuation (`123.456-7`).
fn sem_pontuacao(value: &str) -> String {
    value
        .chars()
        .filter(|c| !matches!(c, '.' | '-' | ' '))
        .collect()
}

fn parse_agencia(value: &str) -> Result<String, String> {
    let agencia = sem_pontuacao(value);
    if (1..=4).contains(&agencia.len()) && agencia.bytes().all(|b| b.is_ascii_digit()) {
        Ok(agencia)
    } else {
        Err("a agência tem até 4 dígitos, sem o dígito verificador".to_owned())
    }
}

fn parse_conta(value: &str) -> Result<String, String> {
    let conta = sem_pontuacao(value).to_uppercase();
    let numero = conta.strip_suffix('X').unwrap_or(&conta);
    if !numero.is_empty() && conta.len() <= 20 && numero.bytes().all(|b| b.is_ascii_digit()) {
        Ok(conta)
    } else {
        Err("use os dígitos da conta com o dígito verificador (ex.: 123456-7)".to_owned())
    }
}

fn parse_documento(value: &str) -> Result<Documento, String> {
    Documento::parse(value).map_err(|err| err.to_string())
}

fn parse_uf(value: &str) -> Result<Uf, String> {
    value
        .parse()
        .map_err(|err: inter_pj::cobranca::UfError| err.to_string())
}

fn parse_motivo(value: &str) -> Result<String, String> {
    inter_pj::cobranca::motivo_cancelamento(value).map_err(str::to_owned)
}

/// 8 digits, with or without punctuation.
fn parse_cep(value: &str) -> Result<String, String> {
    let cep: String = value
        .chars()
        .filter(|c| !matches!(c, '-' | '.' | ' '))
        .collect();
    if cep.len() == 8 && cep.bytes().all(|b| b.is_ascii_digit()) {
        Ok(cep)
    } else {
        Err("o CEP tem 8 dígitos (ex.: 30110-000)".to_owned())
    }
}

/// A Brazilian phone with DDD, with or without `+55` and punctuation.
fn parse_telefone(value: &str) -> Result<Telefone, String> {
    let mut digitos: String = value.chars().filter(char::is_ascii_digit).collect();
    if matches!(digitos.len(), 12 | 13) && digitos.starts_with("55") {
        digitos.drain(..2);
    }
    if !matches!(digitos.len(), 10 | 11) || value.chars().any(|c| c.is_ascii_alphabetic()) {
        return Err("telefone com DDD, como (31) 99999-9999".to_owned());
    }
    let numero = digitos.split_off(2);
    Ok(Telefone {
        ddd: digitos,
        numero,
    })
}

/// `2%` (or `2,5%`) is a percentage; anything else an amount (`4,00`).
fn parse_taxa_ou_valor(value: &str) -> Result<TaxaOuValor, String> {
    let value = value.trim();
    match value.strip_suffix('%') {
        Some(taxa) => taxa
            .trim()
            .replace(',', ".")
            .parse::<Decimal>()
            .ok()
            .filter(|taxa| !taxa.is_sign_negative())
            .map(TaxaOuValor::Taxa)
            .ok_or_else(|| format!("percentual inválido: {value}")),
        None => parse_valor(value).map(TaxaOuValor::Valor),
    }
}

fn parse_codigo_barras(value: &str) -> Result<CodigoBarras, String> {
    CodigoBarras::parse(value).map_err(|err| err.to_string())
}

/// 8-4-4-4-12 hexadecimal digits, as the API's request and transaction
/// codes; in lower case.
fn parse_uuid(value: &str) -> Result<String, String> {
    let codigo = value.trim();
    let valido = codigo.len() == 36
        && codigo.char_indices().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        });
    if valido {
        Ok(codigo.to_ascii_lowercase())
    } else {
        Err(
            "esperado um UUID, com 8-4-4-4-12 dígitos hexadecimais (ex.: 3414f226-36fb-4d87-811e-cfd99911d845)"
                .to_owned(),
        )
    }
}

fn parse_id_idempotente(value: &str) -> Result<IdIdempotente, String> {
    value
        .parse()
        .map_err(|err: inter_pj::banking::IdIdempotenteError| err.to_string())
}

/// `90`, `90s`, `5m` or `1h`, from 1 second to 1 hour.
fn parse_duracao(value: &str) -> Result<Duration, String> {
    let texto = value.trim();
    let (numero, unidade) = texto
        .find(|c: char| !c.is_ascii_digit())
        .map_or((texto, "s"), |i| texto.split_at(i));
    let segundos = match (numero.parse::<u64>(), unidade) {
        (Ok(n), "s") => Some(n),
        (Ok(n), "m") => n.checked_mul(60),
        (Ok(n), "h") => n.checked_mul(3600),
        _ => None,
    };
    segundos
        .filter(|s| (1..=3600).contains(s))
        .map(Duration::from_secs)
        .ok_or_else(|| {
            format!("duração inválida \"{texto}\": use, por exemplo, 90s, 5m ou 1h (até 1h)")
        })
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
    fn pix_arguments_are_parsed_and_validated() {
        let parse = |args: &[&str]| {
            let mut full = vec!["inter-pj", "pix", "enviar"];
            full.extend_from_slice(args);
            Cli::try_parse_from(full)
        };
        let cli = parse(&[
            "--chave",
            "Fornecedor@Exemplo.com",
            "--valor",
            "1.500,00",
            "--data",
            "2026-10-01",
            "--id-idempotente",
            "123E4567-E89B-42D3-A456-426614174000",
            "--sim",
        ])
        .unwrap();
        let Command::Pix(PixCommand::Enviar(args)) = cli.command else {
            panic!("comando inesperado");
        };
        assert_eq!(args.chave.unwrap().as_str(), "fornecedor@exemplo.com");
        assert_eq!(args.valor, Some("1500.00".parse::<Decimal>().unwrap()));
        assert_eq!(args.data, NaiveDate::from_ymd_opt(2026, 10, 1));
        assert_eq!(
            args.id_idempotente.unwrap().as_str(),
            "123e4567-e89b-42d3-a456-426614174000"
        );
        assert!(args.sim && !args.simular);

        for args in [
            &["--valor", "10"][..],
            &["--chave", "11912345678", "--valor", "10"],
            &["--chave", "fornecedor@exemplo.com", "--valor", "1.500"],
            &[
                "--chave",
                "fornecedor@exemplo.com",
                "--valor",
                "10",
                "--sim",
                "--simular",
            ],
            &[
                "--chave",
                "fornecedor@exemplo.com",
                "--valor",
                "10",
                "--id-idempotente",
                "x",
            ],
        ] {
            assert!(parse(args).is_err(), "{args:?}");
        }
        let err = parse(&["--chave", "11912345678", "--valor", "10"]).unwrap_err();
        assert!(err.to_string().contains("+55"), "{err}");
    }

    #[test]
    fn pix_destinations_are_exclusive_and_bank_details_complete() {
        let parse = |args: &[&str]| {
            let mut full = vec!["inter-pj", "pix", "enviar", "--valor", "10"];
            full.extend_from_slice(args);
            Cli::try_parse_from(full)
        };
        let dados = [
            "--ispb",
            "00000000",
            "--agencia",
            "0001",
            "--conta",
            "123.456-x",
            "--tipo-conta",
            "Poupança",
            "--documento",
            "12.345.678/0001-95",
            "--nome",
            "Fornecedor Exemplo",
        ];
        let Command::Pix(PixCommand::Enviar(args)) = parse(&dados).unwrap().command else {
            panic!("comando inesperado");
        };
        assert_eq!(args.conta.as_deref(), Some("123456X"));
        assert_eq!(args.tipo_conta, Some(TipoContaArg::Poupanca));
        assert_eq!(
            args.documento.map(|d| d.as_str().to_owned()).as_deref(),
            Some("12345678000195")
        );

        // Exactly one destination.
        assert!(parse(&["--chave", "fornecedor@exemplo.com", "--ispb", "00000000"]).is_err());
        // Bank details go together.
        assert!(parse(&dados[..10]).is_err());
        assert!(parse(&["--chave", "fornecedor@exemplo.com", "--agencia", "0001"]).is_err());
        for (flag, invalido) in [
            ("--ispb", "0041696"),
            ("--agencia", "12345"),
            ("--conta", "12a45"),
            ("--tipo-conta", "investimento"),
            ("--documento", "123.456.789-00"),
        ] {
            let mut args = dados.to_vec();
            let i = args.iter().position(|a| *a == flag).unwrap();
            args[i + 1] = invalido;
            assert!(parse(&args).is_err(), "{flag} {invalido}");
        }
        let err = parse(&["--copia-e-cola", "000201"]).unwrap_err();
        assert!(err.to_string().contains("copia e cola"), "{err}");
    }

    #[test]
    fn pix_query_arguments() {
        let cli = Cli::try_parse_from([
            "inter-pj",
            "pix",
            "consultar",
            "c42f0787-02cb-4b31-827e-459ec9d7ece1",
            "--aguardar",
            "--timeout",
            "2m",
        ])
        .unwrap();
        let Command::Pix(PixCommand::Consultar(args)) = cli.command else {
            panic!("comando inesperado");
        };
        assert!(args.aguardar);
        assert_eq!(args.timeout, Duration::from_secs(120));
        // --timeout only makes sense while waiting.
        assert!(
            Cli::try_parse_from(["inter-pj", "pix", "consultar", "x", "--timeout", "5s"]).is_err()
        );
        assert!(Cli::try_parse_from(["inter-pj", "pix", "consultar"]).is_err());
    }

    #[test]
    fn payment_listing_and_cancel_arguments() {
        let parse = |args: &[&str]| {
            let mut full = vec!["inter-pj", "pagamento", "boleto"];
            full.extend_from_slice(args);
            Cli::try_parse_from(full)
        };
        let cli = parse(&[
            "listar",
            "--inicio",
            "2026-09-01",
            "--filtrar-por",
            "Vencimento",
            "--codigo",
            "07797.77705 11678.471159 90071.126347 1 92950000003010",
            "--codigo-transacao",
            " 3414F226-36FB-4D87-811E-CFD99911D845 ",
        ])
        .unwrap();
        assert!(cli.command.aceita_csv());
        let Command::Pagamento(PagamentoCommand::Boleto(BoletoCommand::Listar(args))) = cli.command
        else {
            panic!("comando inesperado");
        };
        assert_eq!(args.periodo.inicio, NaiveDate::from_ymd_opt(2026, 9, 1));
        assert_eq!(args.filtrar_por, Some(DataDoPagamentoArg::Vencimento));
        assert_eq!(
            args.codigo.unwrap().codigo_barras(),
            "07791929500000030107777011678471159007112634"
        );
        assert_eq!(
            args.codigo_transacao.as_deref(),
            Some("3414f226-36fb-4d87-811e-cfd99911d845")
        );

        let cli = parse(&["cancelar", "3414f226-36fb-4d87-811e-cfd99911d845", "--sim"]).unwrap();
        assert!(!cli.command.aceita_csv());
        let Command::Pagamento(PagamentoCommand::Boleto(BoletoCommand::Cancelar(args))) =
            cli.command
        else {
            panic!("comando inesperado");
        };
        assert!(args.sim);

        let cli = parse(&[
            "pagar",
            "82670000000653301602023123106000000002830894",
            "--valor",
            "65,33",
            "--vencimento",
            "2026-10-10",
            "--data",
            "2026-10-09",
            "--beneficiario",
            "12.345.678/0001-95",
            "--sim",
        ])
        .unwrap();
        assert!(!cli.command.aceita_csv());
        let Command::Pagamento(PagamentoCommand::Boleto(BoletoCommand::Pagar(args))) = cli.command
        else {
            panic!("comando inesperado");
        };
        assert_eq!(args.valor, Some("65.33".parse::<Decimal>().unwrap()));
        assert_eq!(args.vencimento, NaiveDate::from_ymd_opt(2026, 10, 10));
        assert_eq!(args.data, NaiveDate::from_ymd_opt(2026, 10, 9));
        assert_eq!(
            args.beneficiario.map(|d| d.as_str().to_owned()).as_deref(),
            Some("12345678000195")
        );
        assert!(args.sim && !args.simular);

        for args in [
            &["pagar"][..],
            &["pagar", "03395988500000666539201493990000372830030103"],
            &[
                "pagar",
                "03395988500000666539201493990000372830030102",
                "--valor",
                "1.500",
            ],
            &[
                "pagar",
                "03395988500000666539201493990000372830030102",
                "--sim",
                "--simular",
            ],
            &[
                "pagar",
                "03395988500000666539201493990000372830030102",
                "--beneficiario",
                "123",
            ],
            &["listar", "--codigo", "123"],
            &["listar", "--codigo-transacao", "../pix"],
            &["listar", "--filtrar-por", "hoje"],
            &["cancelar"],
            &["cancelar", "3414f226-36fb-4d87-811e-cfd99911d84"],
            &["cancelar", "3414f226_36fb-4d87-811e-cfd99911d845"],
        ] {
            assert!(parse(args).is_err(), "{args:?}");
        }
    }

    #[test]
    fn darf_arguments_come_from_options_or_a_file() {
        let parse = |args: &[&str]| {
            let mut full = vec!["inter-pj", "pagamento", "darf"];
            full.extend_from_slice(args);
            Cli::try_parse_from(full)
        };
        let opcoes = [
            "pagar",
            "--codigo-receita",
            "0220",
            "--contribuinte",
            "12.345.678/0001-95",
            "--nome-empresa",
            "Empresa Exemplo",
            "--periodo-apuracao",
            "2026-09-30",
            "--vencimento",
            "2026-10-30",
            "--referencia",
            "13609400849201739",
            "--descricao",
            "IRPJ de setembro",
            "--valor-principal",
            "47,14",
            "--multa",
            "0",
            "--juros",
            "10,11",
        ];
        let Command::Pagamento(PagamentoCommand::Darf(DarfCommand::Pagar(args))) =
            parse(&opcoes).unwrap().command
        else {
            panic!("comando inesperado");
        };
        assert_eq!(
            args.valor_principal,
            Some("47.14".parse::<Decimal>().unwrap())
        );
        assert_eq!(args.multa, Some(Decimal::ZERO));
        assert!(args.arquivo.is_none());

        let cli = parse(&["pagar", "--arquivo", "darf.json", "--sim"]).unwrap();
        assert!(!cli.command.aceita_csv());
        let Command::Pagamento(PagamentoCommand::Darf(DarfCommand::Pagar(args))) = cli.command
        else {
            panic!("comando inesperado");
        };
        assert_eq!(args.arquivo, Some(PathBuf::from("darf.json")));

        // No origin, both origins, options without the revenue code, missing options.
        for args in [
            &["pagar"][..],
            &[
                "pagar",
                "--arquivo",
                "darf.json",
                "--codigo-receita",
                "0220",
            ],
            &["pagar", "--arquivo", "darf.json", "--multa", "1"],
            &opcoes[..opcoes.len() - 6],
            &["pagar", "--arquivo", "darf.json", "--sim", "--simular"],
        ] {
            assert!(parse(args).is_err(), "{args:?}");
        }

        let cli = parse(&[
            "listar",
            "--inicio",
            "2026-09-01",
            "--codigo-solicitacao",
            "3414F226-36FB-4D87-811E-CFD99911D845",
        ])
        .unwrap();
        assert!(cli.command.aceita_csv());
        let Command::Pagamento(PagamentoCommand::Darf(DarfCommand::Listar(args))) = cli.command
        else {
            panic!("comando inesperado");
        };
        assert_eq!(args.inicio, NaiveDate::from_ymd_opt(2026, 9, 1));
        assert_eq!(
            args.codigo_solicitacao.as_deref(),
            Some("3414f226-36fb-4d87-811e-cfd99911d845")
        );
    }

    #[test]
    fn batch_arguments() {
        let parse = |args: &[&str]| {
            let mut full = vec!["inter-pj", "pagamento", "lote"];
            full.extend_from_slice(args);
            Cli::try_parse_from(full)
        };
        let cli = parse(&[
            "enviar",
            "--arquivo",
            "lote.csv",
            "--identificador",
            "Outubro",
            "--sim",
        ])
        .unwrap();
        assert!(!cli.command.aceita_csv());
        let Command::Pagamento(PagamentoCommand::Lote(LoteCommand::Enviar(args))) = cli.command
        else {
            panic!("comando inesperado");
        };
        assert_eq!(args.arquivo, PathBuf::from("lote.csv"));
        assert_eq!(args.identificador.as_deref(), Some("Outubro"));

        let Command::Pagamento(PagamentoCommand::Lote(LoteCommand::Consultar(args))) =
            parse(&["consultar", "0123456789abcdef01234567", "--aguardar"])
                .unwrap()
                .command
        else {
            panic!("comando inesperado");
        };
        assert_eq!(args.timeout, Duration::from_secs(300));

        for (args, esperado) in [
            (&["modelo"][..], TipoArquivo::Json),
            (&["modelo", "CSV"], TipoArquivo::Csv),
        ] {
            let Command::Pagamento(PagamentoCommand::Lote(LoteCommand::Modelo(modelo))) =
                parse(args).unwrap().command
            else {
                panic!("comando inesperado");
            };
            assert_eq!(modelo.tipo, esperado);
        }
        for args in [
            &["enviar"][..],
            &["enviar", "--arquivo", "lote.csv", "--sim", "--simular"],
            &["consultar", "x", "--timeout", "5m"],
            &["modelo", "xml"],
        ] {
            assert!(parse(args).is_err(), "{args:?}");
        }
    }

    #[test]
    fn durations() {
        for (texto, segundos) in [
            ("90", 90),
            ("90s", 90),
            ("5m", 300),
            ("1h", 3600),
            ("1s", 1),
        ] {
            assert_eq!(
                parse_duracao(texto),
                Ok(Duration::from_secs(segundos)),
                "{texto}"
            );
        }
        for texto in [
            "",
            "0",
            "0s",
            "2h",
            "3601",
            "1d",
            "m",
            "1.5m",
            "-1s",
            "99999999999999999999",
        ] {
            assert!(parse_duracao(texto).is_err(), "{texto}");
        }
    }

    #[test]
    fn client_secret_is_not_accepted_as_flag() {
        let err = Cli::try_parse_from(["inter-pj", "saldo", "--client-secret", "x"]).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::UnknownArgument);
    }
}
