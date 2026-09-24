//! `inter-pj pix` commands of the Pix API (`/pix/v2`): the charges.
//!
//! Doc comments here are `--help` text too (in Portuguese).

use std::path::PathBuf;

use chrono::{DateTime, FixedOffset, NaiveDate};
use clap::{ArgAction, ArgGroup, Args, Subcommand, ValueEnum};
use inter_pj::documento::Documento;
use inter_pj::pix::{ChavePix, InfoAdicional, Txid, TxidError};
use rust_decimal::Decimal;

use super::{parse_chave, parse_data, parse_documento};
use crate::valor::parse_valor;

#[derive(Debug, Subcommand)]
pub(crate) enum PixCobCommand {
    /// Cria uma cobrança imediata, após mostrar um resumo e pedir confirmação
    Criar(Box<PixCobCriarArgs>),
    /// Altera ou remove uma cobrança imediata, após mostrar o antes e o depois
    Revisar(Box<PixCobRevisarArgs>),
    /// Mostra uma cobrança imediata e os Pix que a pagaram (com o QR Code, se pedido)
    Consultar(PixCobConsultarArgs),
    /// Cobranças imediatas criadas em um período (padrão: últimos 30 dias), com filtros
    Listar(PixCobListarArgs),
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Cobrança")]
pub(crate) struct PixCobCriarArgs {
    /// Chave Pix da conta que recebe: e-mail, telefone com +55, CPF/CNPJ ou chave aleatória
    #[arg(long, value_name = "CHAVE", value_parser = parse_chave)]
    pub(crate) chave: ChavePix,

    /// Valor: 149,90, 1.500,00 ou 149.90
    #[arg(long, value_name = "VALOR", value_parser = parse_valor)]
    pub(crate) valor: Decimal,

    /// Permite que o pagador altere o valor
    #[arg(long)]
    pub(crate) valor_alteravel: bool,

    /// Tempo até a cobrança expirar, contado da criação: 3600s, 30m, 2h, 7d [padrão da API: 1 dia]
    #[arg(long, value_name = "DURACAO", value_parser = parse_expiracao)]
    pub(crate) expiracao: Option<u32>,

    /// CPF ou CNPJ de quem paga
    #[arg(long, value_name = "CPF/CNPJ", value_parser = parse_documento, requires = "devedor_nome")]
    pub(crate) devedor_documento: Option<Documento>,

    /// Nome de quem paga, até 200 caracteres
    #[arg(long, value_name = "NOME", requires = "devedor_documento")]
    pub(crate) devedor_nome: Option<String>,

    /// Texto mostrado ao pagador, até 140 caracteres
    #[arg(long, value_name = "TEXTO")]
    pub(crate) solicitacao: Option<String>,

    /// Informação mostrada ao pagador, como NOME=VALOR; repita para até 50
    #[arg(long, value_name = "NOME=VALOR", value_parser = parse_info, action = ArgAction::Append)]
    pub(crate) info: Vec<InfoAdicional>,

    /// Location criada antes, para usar nesta cobrança
    #[arg(long, value_name = "ID")]
    pub(crate) loc: Option<u64>,

    /// txid, de 26 a 35 letras e dígitos [padrão: gerado]; repetir o comando com o mesmo txid não cria outra cobrança
    #[arg(long, value_name = "TXID", value_parser = parse_txid)]
    pub(crate) txid: Option<Txid>,

    #[command(flatten)]
    pub(crate) qr: QrCodeArgs,

    /// Confirma sem perguntar (para scripts)
    #[arg(long, conflicts_with = "simular", help_heading = "Segurança")]
    pub(crate) sim: bool,

    /// Mostra a requisição que seria enviada, sem enviar nada
    #[arg(long, conflicts_with_all = ["qrcode", "qrcode_png"], help_heading = "Segurança")]
    pub(crate) simular: bool,
}

/// `--qrcode` and `--qrcode-png` of the Pix charges.
#[derive(Debug, Args)]
#[command(next_help_heading = "QR Code")]
pub(crate) struct QrCodeArgs {
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
#[command(next_help_heading = "O que muda")]
#[command(group(
    ArgGroup::new("mudanca")
        .required(true)
        .multiple(true)
        .args(["valor", "valor_alteravel", "expiracao", "devedor_documento", "solicitacao", "info", "loc", "chave", "remover"])
))]
pub(crate) struct PixCobRevisarArgs {
    /// txid da cobrança
    #[arg(value_name = "TXID", value_parser = parse_txid)]
    pub(crate) txid: Txid,

    /// Novo valor
    #[arg(long, value_name = "VALOR", value_parser = parse_valor)]
    pub(crate) valor: Option<Decimal>,

    /// Se o pagador pode alterar o valor: sim ou nao
    #[arg(long, value_name = "SIM|NAO", value_enum, ignore_case = true)]
    pub(crate) valor_alteravel: Option<SimNao>,

    /// Novo tempo até expirar, contado da criação: 3600s, 30m, 2h, 7d
    #[arg(long, value_name = "DURACAO", value_parser = parse_expiracao)]
    pub(crate) expiracao: Option<u32>,

    /// CPF ou CNPJ do novo devedor
    #[arg(long, value_name = "CPF/CNPJ", value_parser = parse_documento, requires = "devedor_nome")]
    pub(crate) devedor_documento: Option<Documento>,

    /// Nome do novo devedor
    #[arg(long, value_name = "NOME", requires = "devedor_documento")]
    pub(crate) devedor_nome: Option<String>,

    /// Novo texto mostrado ao pagador
    #[arg(long, value_name = "TEXTO")]
    pub(crate) solicitacao: Option<String>,

    /// Novas informações ao pagador, como NOME=VALOR, no lugar das atuais; repita para até 50
    #[arg(long, value_name = "NOME=VALOR", value_parser = parse_info, action = ArgAction::Append)]
    pub(crate) info: Vec<InfoAdicional>,

    /// Nova location
    #[arg(long, value_name = "ID")]
    pub(crate) loc: Option<u64>,

    /// Nova chave Pix da conta que recebe
    #[arg(long, value_name = "CHAVE", value_parser = parse_chave)]
    pub(crate) chave: Option<ChavePix>,

    /// Remove a cobrança: ela deixa de poder ser paga
    #[arg(long, conflicts_with_all = ["valor", "valor_alteravel", "expiracao", "devedor_documento", "solicitacao", "info", "loc", "chave"])]
    pub(crate) remover: bool,

    /// Confirma sem perguntar (para scripts)
    #[arg(long, help_heading = "Segurança")]
    pub(crate) sim: bool,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct PixCobConsultarArgs {
    /// txid da cobrança, mostrado por `pix cob criar` e `pix cob listar`
    #[arg(value_name = "TXID", value_parser = parse_txid)]
    pub(crate) txid: Txid,

    #[command(flatten)]
    pub(crate) qr: QrCodeArgs,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct PixCobListarArgs {
    #[command(flatten)]
    pub(crate) periodo: PeriodoPixArgs,

    /// Apenas deste devedor (CPF ou CNPJ)
    #[arg(long, value_name = "CPF/CNPJ", value_parser = parse_documento)]
    pub(crate) documento: Option<Documento>,

    /// Apenas neste status: ativa, concluida, removida-pelo-usuario ou removida-pelo-psp
    #[arg(
        long,
        value_enum,
        ignore_case = true,
        value_name = "STATUS",
        hide_possible_values = true
    )]
    pub(crate) status: Option<StatusCobArg>,

    /// Apenas cobranças com location
    #[arg(long, conflicts_with = "sem_location")]
    pub(crate) com_location: bool,

    /// Apenas cobranças sem location
    #[arg(long)]
    pub(crate) sem_location: bool,

    /// Traz só esta página (a primeira é 0), em vez de todas
    #[arg(long, value_name = "N")]
    pub(crate) pagina: Option<u32>,

    /// Itens por página com --pagina, de 1 a 1000 [padrão da API: 100]
    #[arg(long, value_name = "N", requires = "pagina")]
    pub(crate) itens_por_pagina: Option<u32>,
}

/// `--inicio` and `--fim` of the Pix listings.
#[derive(Debug, Clone, Copy, Args)]
pub(crate) struct PeriodoPixArgs {
    /// Início: AAAA-MM-DD (desde o começo do dia, no fuso local) ou data e hora com fuso (2026-09-01T08:00:00-03:00) [padrão: 30 dias atrás]
    #[arg(long, value_name = "DATA", value_parser = parse_momento)]
    pub(crate) inicio: Option<Momento>,

    /// Fim: AAAA-MM-DD (até o fim do dia) ou data e hora com fuso [padrão: agora]
    #[arg(long, value_name = "DATA", value_parser = parse_momento)]
    pub(crate) fim: Option<Momento>,
}

/// A date (the whole day, in the local time zone) or a moment with offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Momento {
    Dia(NaiveDate),
    Instante(DateTime<FixedOffset>),
}

/// `sim` or `nao`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum SimNao {
    Sim,
    #[value(alias = "não")]
    Nao,
}

/// `--status` of the Pix charges, with the API's codes as aliases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum StatusCobArg {
    #[value(alias = "ATIVA")]
    Ativa,
    #[value(alias = "CONCLUIDA")]
    Concluida,
    #[value(
        alias = "removida-pelo-usuario-recebedor",
        alias = "REMOVIDA_PELO_USUARIO_RECEBEDOR"
    )]
    RemovidaPeloUsuario,
    #[value(alias = "REMOVIDA_PELO_PSP")]
    RemovidaPeloPsp,
}

fn parse_txid(value: &str) -> Result<Txid, String> {
    value.parse().map_err(|err: TxidError| err.to_string())
}

/// `NOME=VALOR`.
fn parse_info(value: &str) -> Result<InfoAdicional, String> {
    let (nome, valor) = value
        .split_once('=')
        .ok_or_else(|| "use NOME=VALOR, como Pedido=123".to_owned())?;
    Ok(InfoAdicional::new(nome.trim(), valor.trim()))
}

/// `3600s`, `30m`, `2h` or `7d` (a number alone is seconds).
fn parse_expiracao(value: &str) -> Result<u32, String> {
    let texto = value.trim();
    let (numero, unidade) = texto
        .find(|c: char| !c.is_ascii_digit())
        .map_or((texto, "s"), |i| texto.split_at(i));
    let segundos = match (numero.parse::<u32>(), unidade) {
        (Ok(n), "s") => Some(n),
        (Ok(n), "m") => n.checked_mul(60),
        (Ok(n), "h") => n.checked_mul(3600),
        (Ok(n), "d") => n.checked_mul(86_400),
        _ => None,
    };
    segundos.filter(|s| *s > 0).ok_or_else(|| {
        format!("duração inválida \"{texto}\": use, por exemplo, 3600s, 30m, 2h ou 7d")
    })
}

/// `2026-09-01` or `2026-09-01T08:00:00-03:00`.
fn parse_momento(value: &str) -> Result<Momento, String> {
    let texto = value.trim();
    if let Ok(instante) = DateTime::parse_from_rfc3339(texto) {
        return Ok(Momento::Instante(instante));
    }
    parse_data(texto).map(Momento::Dia).map_err(|_| {
        format!("data inválida \"{texto}\": use AAAA-MM-DD ou data e hora com fuso, como 2026-09-01T08:00:00-03:00")
    })
}
