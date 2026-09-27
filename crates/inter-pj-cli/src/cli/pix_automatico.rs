//! `inter-pj pix-automatico` commands (`/pix/v2/rec`...): the recurring
//! charges the payer authorizes once.
//!
//! Doc comments here are `--help` text too (in Portuguese).

use std::path::PathBuf;

use chrono::NaiveDate;
use clap::{ArgGroup, Args, Subcommand, ValueEnum};
use inter_pj::documento::Documento;
use inter_pj::pix::Txid;
use inter_pj::pix_automatico::{IdRec, Periodicidade, StatusRec};
use rust_decimal::Decimal;

use super::pix::{PeriodoPixArgs, QrCodeArgs, parse_txid};
use super::{parse_data, parse_documento};
use crate::valor::parse_valor;

#[derive(Debug, Subcommand)]
pub(crate) enum PixAutomaticoCommand {
    /// Recorrências: a autorização do pagador, com o contrato, a periodicidade e o valor
    #[command(
        subcommand,
        subcommand_help_heading = "Comandos",
        subcommand_value_name = "COMANDO"
    )]
    Rec(RecCommand),
}

#[derive(Debug, Subcommand)]
pub(crate) enum RecCommand {
    /// Cria uma recorrência (pelas opções ou por --arquivo), após mostrar um resumo e pedir confirmação
    Criar(Box<RecCriarArgs>),
    /// Imprime um arquivo JSON de exemplo para `pix-automatico rec criar --arquivo`, com dados fictícios
    Modelo(RecModeloArgs),
    /// Recorrências criadas em um período (padrão: últimos 30 dias), com filtros
    Listar(RecListarArgs),
    /// Mostra uma recorrência, seu histórico e o QR Code (com --txid, o composto com uma cobrança)
    Consultar(RecConsultarArgs),
    /// Altera uma recorrência, após mostrar o antes e o depois
    Revisar(RecRevisarArgs),
    /// Cancela uma recorrência, após mostrá-la e pedir confirmação
    Cancelar(RecCancelarArgs),
}

/// The options of a recurrence, which `--arquivo` replaces.
const OPCOES_REC: [&str; 12] = [
    "devedor_documento",
    "devedor_nome",
    "contrato",
    "objeto",
    "data_inicial",
    "data_final",
    "periodicidade",
    "valor",
    "valor_minimo",
    "retentativas",
    "loc",
    "txid_ativacao",
];

/// Where the recurrence comes from: a file, or the options.
#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("origem")
        .required(true)
        .args(["arquivo", "devedor_documento"])
))]
pub(crate) struct RecCriarArgs {
    /// Arquivo JSON com a recorrência, nos campos da API ("-" para a entrada padrão); veja `pix-automatico rec modelo`
    #[arg(
        long,
        value_name = "ARQUIVO",
        conflicts_with_all = OPCOES_REC,
        help_heading = "Origem (escolha uma)"
    )]
    pub(crate) arquivo: Option<PathBuf>,

    /// CPF ou CNPJ de quem paga
    #[arg(
        long,
        value_name = "CPF/CNPJ",
        value_parser = parse_documento,
        requires_all = ["devedor_nome", "contrato", "data_inicial", "periodicidade"],
        help_heading = "Origem (escolha uma)"
    )]
    pub(crate) devedor_documento: Option<Documento>,

    /// Nome de quem paga, até 140 caracteres
    #[arg(long, value_name = "NOME", help_heading = "Recorrência")]
    pub(crate) devedor_nome: Option<String>,

    /// Número ou código do contrato (ou do pedido) que a recorrência paga, até 35 caracteres
    #[arg(long, value_name = "CONTRATO", help_heading = "Recorrência")]
    pub(crate) contrato: Option<String>,

    /// O que se paga, como o pagador vê no banco, até 35 caracteres
    #[arg(long, value_name = "TEXTO", help_heading = "Recorrência")]
    pub(crate) objeto: Option<String>,

    /// Data do primeiro pagamento (AAAA-MM-DD)
    #[arg(long, value_name = "AAAA-MM-DD", value_parser = parse_data, help_heading = "Recorrência")]
    pub(crate) data_inicial: Option<NaiveDate>,

    /// Data do último pagamento (AAAA-MM-DD) [padrão: sem fim]
    #[arg(long, value_name = "AAAA-MM-DD", value_parser = parse_data, help_heading = "Recorrência")]
    pub(crate) data_final: Option<NaiveDate>,

    /// Periodicidade: semanal, mensal, trimestral, semestral ou anual
    #[arg(
        long,
        value_enum,
        ignore_case = true,
        value_name = "PERIODICIDADE",
        hide_possible_values = true,
        help_heading = "Recorrência"
    )]
    pub(crate) periodicidade: Option<PeriodicidadeArg>,

    /// Permite até 3 novas tentativas de uma cobrança não paga, em dias diferentes, em até 7 dias
    #[arg(long, help_heading = "Recorrência")]
    pub(crate) retentativas: bool,

    /// Valor fixo de cada pagamento: 149,90, 1.500,00 ou 149.90 [padrão: o de cada cobrança]
    #[arg(
        long,
        value_name = "VALOR",
        value_parser = parse_valor,
        conflicts_with = "valor_minimo",
        help_heading = "Valor (escolha um, ou nenhum)"
    )]
    pub(crate) valor: Option<Decimal>,

    /// Menor limite que o pagador pode definir, quando o valor muda a cada cobrança
    #[arg(
        long,
        value_name = "VALOR",
        value_parser = parse_valor,
        help_heading = "Valor (escolha um, ou nenhum)"
    )]
    pub(crate) valor_minimo: Option<Decimal>,

    /// Location criada antes, para o QR Code da recorrência
    #[arg(long, value_name = "ID", help_heading = "Aprovação")]
    pub(crate) loc: Option<u64>,

    /// txid de uma cobrança imediata cujo QR Code composto também aprova a recorrência
    #[arg(long, value_name = "TXID", value_parser = parse_txid, help_heading = "Aprovação")]
    pub(crate) txid_ativacao: Option<Txid>,

    /// Confirma sem perguntar (para scripts)
    #[arg(long, conflicts_with = "simular", help_heading = "Segurança")]
    pub(crate) sim: bool,

    /// Mostra a requisição que seria enviada, sem enviar nada
    #[arg(long, help_heading = "Segurança")]
    pub(crate) simular: bool,
}

#[derive(Debug, Args)]
pub(crate) struct RecModeloArgs {}

/// How often a recurrence is paid, as an option.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum PeriodicidadeArg {
    Semanal,
    Mensal,
    Trimestral,
    Semestral,
    Anual,
}

impl From<PeriodicidadeArg> for Periodicidade {
    fn from(arg: PeriodicidadeArg) -> Self {
        match arg {
            PeriodicidadeArg::Semanal => Self::Semanal,
            PeriodicidadeArg::Mensal => Self::Mensal,
            PeriodicidadeArg::Trimestral => Self::Trimestral,
            PeriodicidadeArg::Semestral => Self::Semestral,
            PeriodicidadeArg::Anual => Self::Anual,
        }
    }
}

/// The status of a recurrence, as an option.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum StatusRecArg {
    Criada,
    Aprovada,
    Rejeitada,
    Expirada,
    Cancelada,
}

impl From<StatusRecArg> for StatusRec {
    fn from(arg: StatusRecArg) -> Self {
        match arg {
            StatusRecArg::Criada => Self::Criada,
            StatusRecArg::Aprovada => Self::Aprovada,
            StatusRecArg::Rejeitada => Self::Rejeitada,
            StatusRecArg::Expirada => Self::Expirada,
            StatusRecArg::Cancelada => Self::Cancelada,
        }
    }
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct RecListarArgs {
    #[command(flatten)]
    pub(crate) periodo: PeriodoPixArgs,

    /// Apenas deste devedor (CPF ou CNPJ)
    #[arg(long, value_name = "CPF/CNPJ", value_parser = parse_documento)]
    pub(crate) documento: Option<Documento>,

    /// Apenas neste status: criada, aprovada, rejeitada, expirada ou cancelada
    #[arg(
        long,
        value_enum,
        ignore_case = true,
        value_name = "STATUS",
        hide_possible_values = true
    )]
    pub(crate) status: Option<StatusRecArg>,

    /// Apenas recorrências com location
    #[arg(long, conflicts_with = "sem_location")]
    pub(crate) com_location: bool,

    /// Apenas recorrências sem location
    #[arg(long)]
    pub(crate) sem_location: bool,

    /// Apenas deste convênio, até 60 caracteres
    #[arg(long, value_name = "CONVENIO")]
    pub(crate) convenio: Option<String>,

    /// Traz só esta página (a primeira é 0), em vez de todas
    #[arg(long, value_name = "N")]
    pub(crate) pagina: Option<u32>,

    /// Itens por página com --pagina, de 1 a 1000 [padrão da API: 100]
    #[arg(long, value_name = "N", requires = "pagina")]
    pub(crate) itens_por_pagina: Option<u32>,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct RecConsultarArgs {
    /// idRec da recorrência, mostrado por `pix-automatico rec criar` e `listar`
    #[arg(value_name = "ID_REC", value_parser = parse_id_rec)]
    pub(crate) id_rec: IdRec,

    /// txid de uma cobrança imediata ou com vencimento: traz o QR Code composto, que paga a cobrança e aprova a recorrência
    #[arg(long, value_name = "TXID", value_parser = parse_txid)]
    pub(crate) txid: Option<Txid>,

    #[command(flatten)]
    pub(crate) qr: QrCodeArgs,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "O que muda")]
#[command(group(
    ArgGroup::new("mudanca")
        .required(true)
        .multiple(true)
        .args(["devedor_nome", "loc", "data_inicial", "txid_ativacao"])
))]
pub(crate) struct RecRevisarArgs {
    /// idRec da recorrência
    #[arg(value_name = "ID_REC", value_parser = parse_id_rec)]
    pub(crate) id_rec: IdRec,

    /// Novo nome do devedor, até 140 caracteres
    #[arg(long, value_name = "NOME")]
    pub(crate) devedor_nome: Option<String>,

    /// Nova location, para o QR Code da recorrência
    #[arg(long, value_name = "ID")]
    pub(crate) loc: Option<u64>,

    /// Nova data do primeiro pagamento (AAAA-MM-DD); só antes da aprovação
    #[arg(long, value_name = "AAAA-MM-DD", value_parser = parse_data)]
    pub(crate) data_inicial: Option<NaiveDate>,

    /// txid de uma nova cobrança de ativação; só antes da aprovação
    #[arg(long, value_name = "TXID", value_parser = parse_txid)]
    pub(crate) txid_ativacao: Option<Txid>,

    /// Confirma sem perguntar (para scripts)
    #[arg(long, help_heading = "Segurança")]
    pub(crate) sim: bool,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct RecCancelarArgs {
    /// idRec da recorrência
    #[arg(value_name = "ID_REC", value_parser = parse_id_rec)]
    pub(crate) id_rec: IdRec,

    /// Confirma sem perguntar (para scripts)
    #[arg(long, help_heading = "Segurança")]
    pub(crate) sim: bool,
}

fn parse_id_rec(value: &str) -> Result<IdRec, String> {
    IdRec::parse(value).map_err(|err| err.to_string())
}
