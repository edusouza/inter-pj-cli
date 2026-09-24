//! Arguments of `inter-pj webhook`: the webhooks of the Banking, Cobrança
//! and Pix APIs, the addresses Inter calls when something happens in the
//! account.

use clap::{Args, Subcommand, ValueEnum};
use inter_pj::pix::{ChavePix, Txid};
use inter_pj::webhook::{TipoWebhookBanking, WebhookUrl};

use super::pix::{parse_e2e, parse_txid};
use super::{PeriodoPixArgs, parse_chave, parse_uuid};

#[derive(Debug, Subcommand)]
pub(crate) enum WebhookCommand {
    /// Webhooks da API Banking: Pix enviados e boletos pagos pela conta
    #[command(
        subcommand,
        subcommand_help_heading = "Comandos",
        subcommand_value_name = "COMANDO"
    )]
    Banking(WebhookBankingCommand),
    /// Webhook da API de Cobrança: cobranças recebidas, canceladas e expiradas
    #[command(
        subcommand,
        subcommand_help_heading = "Comandos",
        subcommand_value_name = "COMANDO"
    )]
    Cobranca(WebhookCobrancaCommand),
    /// Webhooks da API Pix, um por chave: cobranças Pix pagas
    #[command(
        subcommand,
        subcommand_help_heading = "Comandos",
        subcommand_value_name = "COMANDO"
    )]
    Pix(WebhookPixCommand),
}

#[derive(Debug, Subcommand)]
pub(crate) enum WebhookBankingCommand {
    /// Cadastra ou troca o webhook de um tipo, após mostrar o atual e pedir confirmação
    Cadastrar(WebhookBankingCadastrarArgs),
    /// Mostra o webhook de um tipo, ou os dos dois tipos
    Consultar(WebhookBankingConsultarArgs),
    /// Exclui o webhook de um tipo, após mostrá-lo e pedir confirmação
    Excluir(WebhookBankingExcluirArgs),
    /// Tentativas de envio ao webhook de um tipo, da mais recente à mais antiga (padrão: últimos 30 dias)
    Callbacks(WebhookBankingCallbacksArgs),
    /// Pede ao Inter que envie de novo os callbacks de Pix enviados ou boletos pagos, pelos seus códigos
    Reenviar(WebhookBankingReenviarArgs),
}

#[derive(Debug, Subcommand)]
pub(crate) enum WebhookCobrancaCommand {
    /// Cadastra ou troca o webhook, após mostrar o atual e pedir confirmação
    Cadastrar(WebhookCadastroArgs),
    /// Mostra o webhook
    Consultar,
    /// Exclui o webhook, após mostrá-lo e pedir confirmação
    Excluir(WebhookExclusaoArgs),
    /// Tentativas de envio ao webhook, da mais recente à mais antiga (padrão: últimos 30 dias)
    Callbacks(WebhookCobrancaCallbacksArgs),
    /// Pede ao Inter que envie de novo os callbacks de cobranças, pelos seus códigos
    Reenviar(WebhookCobrancaReenviarArgs),
}

#[derive(Debug, Subcommand)]
pub(crate) enum WebhookPixCommand {
    /// Cadastra ou troca o webhook de uma chave Pix, após mostrar o atual e pedir confirmação
    Cadastrar(WebhookPixCadastrarArgs),
    /// Mostra o webhook de uma chave Pix
    Consultar(WebhookPixChaveArgs),
    /// Exclui o webhook de uma chave Pix, após mostrá-lo e pedir confirmação
    Excluir(WebhookPixExcluirArgs),
    /// Tentativas de envio aos webhooks Pix, da mais recente à mais antiga (padrão: últimos 30 dias)
    Callbacks(WebhookPixCallbacksArgs),
    /// Pede ao Inter que envie de novo os callbacks de cobranças Pix de uma chave, pelos seus txids
    Reenviar(WebhookPixReenviarArgs),
}

/// The kind of a webhook of the Banking API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum TipoWebhookArg {
    PixPagamento,
    BoletoPagamento,
}

impl From<TipoWebhookArg> for TipoWebhookBanking {
    fn from(tipo: TipoWebhookArg) -> Self {
        match tipo {
            TipoWebhookArg::PixPagamento => Self::PixPagamento,
            TipoWebhookArg::BoletoPagamento => Self::BoletoPagamento,
        }
    }
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct WebhookCadastroArgs {
    /// Endereço que o Inter vai chamar: https://, acessível pela internet
    #[arg(long, value_name = "URL", value_parser = parse_url)]
    pub(crate) url: WebhookUrl,

    /// Confirma sem perguntar (para scripts)
    #[arg(long, help_heading = "Segurança")]
    pub(crate) sim: bool,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct WebhookExclusaoArgs {
    /// Confirma sem perguntar (para scripts)
    #[arg(long, help_heading = "Segurança")]
    pub(crate) sim: bool,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct WebhookBankingCadastrarArgs {
    /// O que notificar: pix-pagamento (Pix enviados) ou boleto-pagamento (boletos pagos)
    #[arg(
        value_name = "TIPO",
        value_enum,
        ignore_case = true,
        hide_possible_values = true
    )]
    pub(crate) tipo: TipoWebhookArg,

    #[command(flatten)]
    pub(crate) cadastro: WebhookCadastroArgs,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct WebhookBankingConsultarArgs {
    /// pix-pagamento ou boleto-pagamento (padrão: os dois)
    #[arg(
        value_name = "TIPO",
        value_enum,
        ignore_case = true,
        hide_possible_values = true
    )]
    pub(crate) tipo: Option<TipoWebhookArg>,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct WebhookBankingExcluirArgs {
    /// pix-pagamento ou boleto-pagamento
    #[arg(
        value_name = "TIPO",
        value_enum,
        ignore_case = true,
        hide_possible_values = true
    )]
    pub(crate) tipo: TipoWebhookArg,

    #[command(flatten)]
    pub(crate) exclusao: WebhookExclusaoArgs,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct WebhookPixCadastrarArgs {
    /// Chave Pix da conta: CPF/CNPJ, e-mail, celular com +55 ou chave aleatória
    #[arg(value_name = "CHAVE", value_parser = parse_chave)]
    pub(crate) chave: ChavePix,

    #[command(flatten)]
    pub(crate) cadastro: WebhookCadastroArgs,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct WebhookPixChaveArgs {
    /// Chave Pix da conta: CPF/CNPJ, e-mail, celular com +55 ou chave aleatória
    #[arg(value_name = "CHAVE", value_parser = parse_chave)]
    pub(crate) chave: ChavePix,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct WebhookPixExcluirArgs {
    /// Chave Pix da conta: CPF/CNPJ, e-mail, celular com +55 ou chave aleatória
    #[arg(value_name = "CHAVE", value_parser = parse_chave)]
    pub(crate) chave: ChavePix,

    #[command(flatten)]
    pub(crate) exclusao: WebhookExclusaoArgs,
}

/// What the histories of callbacks have in common.
#[derive(Debug, Clone, Copy, Args)]
pub(crate) struct CallbacksArgs {
    #[command(flatten)]
    pub(crate) periodo: PeriodoPixArgs,

    /// Só as tentativas que falharam
    #[arg(long)]
    pub(crate) falhas: bool,

    /// Traz só esta página (a primeira é 0), em vez de todas
    #[arg(long, value_name = "N")]
    pub(crate) pagina: Option<u32>,

    /// Callbacks por página com --pagina, de 10 a 50 [padrão da API: 20]
    #[arg(long, value_name = "N", requires = "pagina")]
    pub(crate) itens_por_pagina: Option<u32>,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct WebhookBankingCallbacksArgs {
    /// pix-pagamento ou boleto-pagamento
    #[arg(
        value_name = "TIPO",
        value_enum,
        ignore_case = true,
        hide_possible_values = true
    )]
    pub(crate) tipo: TipoWebhookArg,

    /// Só os callbacks de um Pix enviado, pelo seu endToEnd (pix-pagamento)
    #[arg(
        long,
        value_name = "E2EID",
        conflicts_with = "codigo_transacao",
        value_parser = parse_e2e
    )]
    pub(crate) end_to_end: Option<String>,

    /// Só os callbacks de um boleto pago, pelo código da transação (boleto-pagamento)
    #[arg(long, value_name = "CODIGO", value_parser = parse_uuid)]
    pub(crate) codigo_transacao: Option<String>,

    #[command(flatten)]
    pub(crate) callbacks: CallbacksArgs,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct WebhookCobrancaCallbacksArgs {
    /// Só os callbacks de uma cobrança, pelo seu código
    #[arg(long, value_name = "CODIGO", value_parser = parse_codigo_cobranca)]
    pub(crate) codigo: Option<String>,

    #[command(flatten)]
    pub(crate) callbacks: CallbacksArgs,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct WebhookPixCallbacksArgs {
    /// Só os callbacks de uma cobrança Pix, pelo seu txid
    #[arg(long, value_name = "TXID", value_parser = parse_txid)]
    pub(crate) txid: Option<Txid>,

    #[command(flatten)]
    pub(crate) callbacks: CallbacksArgs,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct WebhookBankingReenviarArgs {
    /// pix-pagamento ou boleto-pagamento
    #[arg(
        value_name = "TIPO",
        value_enum,
        ignore_case = true,
        hide_possible_values = true
    )]
    pub(crate) tipo: TipoWebhookArg,

    /// Códigos das solicitações dos Pix (pix-pagamento) ou das transações dos boletos (boleto-pagamento); mais de 50 vão em blocos de 50
    #[arg(value_name = "CODIGO", required = true, num_args = 1.., value_parser = parse_uuid)]
    pub(crate) codigos: Vec<String>,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct WebhookCobrancaReenviarArgs {
    /// Códigos das cobranças; mais de 50 vão em blocos de 50
    #[arg(
        value_name = "CODIGO",
        required = true,
        num_args = 1..,
        value_parser = parse_codigo_cobranca
    )]
    pub(crate) codigos: Vec<String>,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct WebhookPixReenviarArgs {
    /// Chave Pix das cobranças, a mesma para todas
    #[arg(value_name = "CHAVE", value_parser = parse_chave)]
    pub(crate) chave: ChavePix,

    /// txids das cobranças; mais de 50 vão em blocos de 50
    #[arg(value_name = "TXID", required = true, num_args = 1.., value_parser = parse_txid)]
    pub(crate) txids: Vec<Txid>,
}

/// The code of a charge, as the other charge operations accept it: groups
/// of hexadecimal digits separated by hyphens (a UUID).
fn parse_codigo_cobranca(value: &str) -> Result<String, String> {
    let codigo = value.trim();
    let valido = (8..=64).contains(&codigo.len())
        && codigo.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
        && !codigo.starts_with('-')
        && !codigo.ends_with('-');
    if valido {
        Ok(codigo.to_owned())
    } else {
        Err(
            "código da cobrança inválido: esperado um UUID (dígitos hexadecimais e hífens)"
                .to_owned(),
        )
    }
}

fn parse_url(value: &str) -> Result<WebhookUrl, String> {
    WebhookUrl::parse(value).map_err(|err| err.to_string())
}
