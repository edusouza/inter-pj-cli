//! `inter-pj pix-automatico` commands (`/pix/v2/rec`...): the recurring
//! charges the payer authorizes once.
//!
//! Doc comments here are `--help` text too (in Portuguese).

use std::path::PathBuf;

use chrono::NaiveDate;
use clap::{ArgGroup, Args, Subcommand, ValueEnum};
use inter_pj::cobranca::Uf;
use inter_pj::documento::Documento;
use inter_pj::pix::Txid;
use inter_pj::pix_automatico::{
    IdRec, IdSolicRec, Periodicidade, StatusCobR, StatusRec, TipoContaRecebedor,
};
use rust_decimal::Decimal;

use super::pix::{Momento, PeriodoPixArgs, QrCodeArgs, parse_expiracao, parse_momento, parse_txid};
use super::{parse_cep, parse_data, parse_documento, parse_uf};
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
    /// Solicitações de confirmação: o pedido ao banco do pagador para que ele aprove uma recorrência
    #[command(
        subcommand,
        subcommand_help_heading = "Comandos",
        subcommand_value_name = "COMANDO"
    )]
    Solicitacao(SolicitacaoCommand),
    /// Cobranças recorrentes: cada pagamento de uma recorrência aprovada, que o banco do pagador debita no vencimento
    #[command(
        subcommand,
        subcommand_help_heading = "Comandos",
        subcommand_value_name = "COMANDO"
    )]
    Cobr(CobrCommand),
    /// Locations de recorrências: os endereços dos QR Codes com que o pagador aprova uma recorrência
    #[command(
        subcommand,
        subcommand_help_heading = "Comandos",
        subcommand_value_name = "COMANDO"
    )]
    Locrec(LocrecCommand),
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

#[derive(Debug, Subcommand)]
pub(crate) enum SolicitacaoCommand {
    /// Pede ao banco do pagador que ele aprove uma recorrência, após mostrá-la e pedir confirmação
    Criar(SolicitacaoCriarArgs),
    /// Mostra uma solicitação de confirmação, em que pé ela está e a recorrência
    Consultar(SolicitacaoConsultarArgs),
    /// Cancela uma solicitação ainda não respondida, após mostrá-la e pedir confirmação
    Cancelar(SolicitacaoCancelarArgs),
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Solicitação")]
pub(crate) struct SolicitacaoCriarArgs {
    /// idRec da recorrência que o pagador vai aprovar
    #[arg(long, value_name = "ID_REC", value_parser = parse_id_rec)]
    pub(crate) rec: IdRec,

    /// CPF ou CNPJ do titular da conta do pagador
    #[arg(long, value_name = "CPF/CNPJ", value_parser = parse_documento, help_heading = "Conta do pagador")]
    pub(crate) documento: Documento,

    /// ISPB do banco do pagador: 8 dígitos (o do Inter é 00416968)
    #[arg(long, value_name = "ISPB", help_heading = "Conta do pagador")]
    pub(crate) ispb: String,

    /// Agência, sem o dígito verificador
    #[arg(long, value_name = "AGENCIA", help_heading = "Conta do pagador")]
    pub(crate) agencia: Option<String>,

    /// Conta, com o dígito verificador (que pode ser X), sem pontos nem traços
    #[arg(long, value_name = "CONTA", help_heading = "Conta do pagador")]
    pub(crate) conta: String,

    /// Prazo para o pagador responder: 2h, 7d, uma data AAAA-MM-DD (até o fim do dia) ou data e hora com fuso [padrão: 7d]
    #[arg(long, value_name = "PRAZO", value_parser = parse_prazo)]
    pub(crate) expiracao: Option<Prazo>,

    /// Confirma sem perguntar (para scripts)
    #[arg(long, conflicts_with = "simular", help_heading = "Segurança")]
    pub(crate) sim: bool,

    /// Mostra a requisição que seria enviada, sem consultar nem enviar nada
    #[arg(long, help_heading = "Segurança")]
    pub(crate) simular: bool,
}

/// Until when the payer may answer: a time from now, or a moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Prazo {
    /// Seconds from now.
    Duracao(u32),
    /// A day (until its end, in the local time zone) or a moment.
    Momento(Momento),
}

/// `7d` or `2026-10-01` (or a moment with offset).
fn parse_prazo(value: &str) -> Result<Prazo, String> {
    if value.contains('-') {
        parse_momento(value).map(Prazo::Momento)
    } else {
        parse_expiracao(value).map(Prazo::Duracao)
    }
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct SolicitacaoConsultarArgs {
    /// idSolicRec da solicitação, mostrado por `pix-automatico solicitacao criar`
    #[arg(value_name = "ID_SOLIC_REC", value_parser = parse_id_solic_rec)]
    pub(crate) id: IdSolicRec,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct SolicitacaoCancelarArgs {
    /// idSolicRec da solicitação
    #[arg(value_name = "ID_SOLIC_REC", value_parser = parse_id_solic_rec)]
    pub(crate) id: IdSolicRec,

    /// Confirma sem perguntar (para scripts)
    #[arg(long, help_heading = "Segurança")]
    pub(crate) sim: bool,
}

fn parse_id_solic_rec(value: &str) -> Result<IdSolicRec, String> {
    IdSolicRec::parse(value).map_err(|err| err.to_string())
}

#[derive(Debug, Subcommand)]
pub(crate) enum CobrCommand {
    /// Cria a cobrança de um ciclo de uma recorrência aprovada, após mostrar um resumo e pedir confirmação
    Criar(Box<CobrCriarArgs>),
    /// Cobranças recorrentes criadas em um período (padrão: últimos 30 dias), com filtros
    Listar(CobrListarArgs),
    /// Mostra uma cobrança recorrente, suas tentativas de liquidação e o Pix que a pagou
    Consultar(CobrConsultarArgs),
    /// Cancela uma cobrança recorrente, após mostrá-la e pedir confirmação
    Cancelar(CobrCancelarArgs),
    /// Pede uma nova tentativa de liquidação de uma cobrança não paga, após mostrá-la e pedir confirmação
    Retentativa(CobrRetentativaArgs),
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Cobrança")]
pub(crate) struct CobrCriarArgs {
    /// idRec da recorrência, que o pagador precisa ter aprovado
    #[arg(long, value_name = "ID_REC", value_parser = parse_id_rec)]
    pub(crate) rec: IdRec,

    /// Valor: 149,90, 1.500,00 ou 149.90
    #[arg(long, value_name = "VALOR", value_parser = parse_valor)]
    pub(crate) valor: Decimal,

    /// Data de vencimento (AAAA-MM-DD)
    #[arg(long, value_name = "AAAA-MM-DD", value_parser = parse_data)]
    pub(crate) vencimento: NaiveDate,

    /// Mantém um vencimento em dia não útil [padrão: passa para o próximo dia útil, pelos feriados da cidade do pagador]
    #[arg(long)]
    pub(crate) sem_ajuste_dia_util: bool,

    /// Informação sobre a fatura, até 140 caracteres
    #[arg(long, value_name = "TEXTO")]
    pub(crate) info: Option<String>,

    /// txid da cobrança: 26 a 35 letras e dígitos [padrão: um novo, aleatório]
    #[arg(long, value_name = "TXID", value_parser = parse_txid)]
    pub(crate) txid: Option<Txid>,

    /// Conta que recebe, com o dígito verificador (que pode ser X), sem pontos nem traços [padrão: a de --conta-corrente, que pode vir da configuração]
    #[arg(long, value_name = "CONTA", help_heading = "Conta que recebe")]
    pub(crate) conta: Option<String>,

    /// Tipo da conta: corrente, poupanca ou pagamento [padrão: corrente]
    #[arg(
        long,
        value_enum,
        ignore_case = true,
        value_name = "TIPO",
        hide_possible_values = true,
        default_value = "corrente",
        hide_default_value = true,
        help_heading = "Conta que recebe"
    )]
    pub(crate) tipo_conta: TipoContaArg,

    /// Agência, sem o dígito verificador
    #[arg(long, value_name = "AGENCIA", help_heading = "Conta que recebe")]
    pub(crate) agencia: Option<String>,

    #[command(flatten)]
    pub(crate) devedor: ContatoDevedorArgs,

    /// Confirma sem perguntar (para scripts)
    #[arg(long, conflicts_with = "simular", help_heading = "Segurança")]
    pub(crate) sim: bool,

    /// Mostra a requisição que seria enviada, sem consultar nem enviar nada
    #[arg(long, help_heading = "Segurança")]
    pub(crate) simular: bool,
}

/// The payer's e-mail and address in a recurring charge; the payer
/// themselves is the one of the recurrence.
#[derive(Debug, Args)]
#[command(next_help_heading = "Devedor (opcional)")]
#[allow(clippy::struct_field_names)] // the fields are the options, --devedor-*
pub(crate) struct ContatoDevedorArgs {
    /// E-mail de quem paga
    #[arg(long, value_name = "EMAIL")]
    pub(crate) devedor_email: Option<String>,

    /// Endereço de quem paga: rua, número e complemento, até 200 caracteres
    #[arg(long, value_name = "ENDERECO")]
    pub(crate) devedor_endereco: Option<String>,

    /// Cidade
    #[arg(long, value_name = "CIDADE")]
    pub(crate) devedor_cidade: Option<String>,

    /// UF (sigla do estado)
    #[arg(long, value_name = "UF", value_parser = parse_uf)]
    pub(crate) devedor_uf: Option<Uf>,

    /// CEP: 30110-000 ou 30110000
    #[arg(long, value_name = "CEP", value_parser = parse_cep)]
    pub(crate) devedor_cep: Option<String>,
}

/// The kind of the receiver's account, as an option.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum TipoContaArg {
    Corrente,
    Poupanca,
    Pagamento,
}

impl From<TipoContaArg> for TipoContaRecebedor {
    fn from(arg: TipoContaArg) -> Self {
        match arg {
            TipoContaArg::Corrente => Self::Corrente,
            TipoContaArg::Poupanca => Self::Poupanca,
            TipoContaArg::Pagamento => Self::Pagamento,
        }
    }
}

/// The status of a recurring charge, as an option.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum StatusCobrArg {
    Criada,
    Ativa,
    Concluida,
    Expirada,
    Rejeitada,
    Cancelada,
}

impl From<StatusCobrArg> for StatusCobR {
    fn from(arg: StatusCobrArg) -> Self {
        match arg {
            StatusCobrArg::Criada => Self::Criada,
            StatusCobrArg::Ativa => Self::Ativa,
            StatusCobrArg::Concluida => Self::Concluida,
            StatusCobrArg::Expirada => Self::Expirada,
            StatusCobrArg::Rejeitada => Self::Rejeitada,
            StatusCobrArg::Cancelada => Self::Cancelada,
        }
    }
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct CobrListarArgs {
    #[command(flatten)]
    pub(crate) periodo: PeriodoPixArgs,

    /// Apenas as desta recorrência
    #[arg(long, value_name = "ID_REC", value_parser = parse_id_rec)]
    pub(crate) rec: Option<IdRec>,

    /// Apenas as deste devedor (CPF ou CNPJ)
    #[arg(long, value_name = "CPF/CNPJ", value_parser = parse_documento)]
    pub(crate) documento: Option<Documento>,

    /// Apenas neste status: criada, ativa, concluida, expirada, rejeitada ou cancelada
    #[arg(
        long,
        value_enum,
        ignore_case = true,
        value_name = "STATUS",
        hide_possible_values = true
    )]
    pub(crate) status: Option<StatusCobrArg>,

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
pub(crate) struct CobrConsultarArgs {
    /// txid da cobrança recorrente, mostrado por `pix-automatico cobr criar` e `listar`
    #[arg(value_name = "TXID", value_parser = parse_txid)]
    pub(crate) txid: Txid,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct CobrCancelarArgs {
    /// txid da cobrança recorrente
    #[arg(value_name = "TXID", value_parser = parse_txid)]
    pub(crate) txid: Txid,

    /// Confirma sem perguntar (para scripts)
    #[arg(long, help_heading = "Segurança")]
    pub(crate) sim: bool,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct CobrRetentativaArgs {
    /// txid da cobrança recorrente não paga
    #[arg(value_name = "TXID", value_parser = parse_txid)]
    pub(crate) txid: Txid,

    /// Dia da nova tentativa (AAAA-MM-DD): até 7 dias depois da liquidação prevista, em um dia sem outra tentativa
    #[arg(long, value_name = "AAAA-MM-DD", value_parser = parse_data)]
    pub(crate) data: NaiveDate,

    /// Confirma sem perguntar (para scripts)
    #[arg(long, help_heading = "Segurança")]
    pub(crate) sim: bool,
}

#[derive(Debug, Subcommand)]
pub(crate) enum LocrecCommand {
    /// Cria uma location, o endereço do QR Code de uma recorrência, para usar depois com `rec criar --loc`
    Criar(LocrecCriarArgs),
    /// Locations de recorrências criadas em um período (padrão: últimos 30 dias), com filtros
    Listar(LocrecListarArgs),
    /// Mostra uma location e a recorrência vinculada a ela
    Consultar(LocrecConsultarArgs),
    /// Desvincula a recorrência de uma location, após mostrá-la e pedir confirmação
    Desvincular(LocrecDesvincularArgs),
}

#[derive(Debug, Args)]
pub(crate) struct LocrecCriarArgs {}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct LocrecListarArgs {
    #[command(flatten)]
    pub(crate) periodo: PeriodoPixArgs,

    /// Apenas as locations com uma recorrência vinculada
    #[arg(long, conflicts_with = "sem_recorrencia")]
    pub(crate) com_recorrencia: bool,

    /// Apenas as locations livres
    #[arg(long)]
    pub(crate) sem_recorrencia: bool,

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
pub(crate) struct LocrecConsultarArgs {
    /// id da location, mostrado por `pix-automatico locrec criar` e `listar`
    #[arg(value_name = "ID")]
    pub(crate) id: u64,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct LocrecDesvincularArgs {
    /// id da location
    #[arg(value_name = "ID")]
    pub(crate) id: u64,

    /// Confirma sem perguntar (para scripts)
    #[arg(long, help_heading = "Segurança")]
    pub(crate) sim: bool,
}
