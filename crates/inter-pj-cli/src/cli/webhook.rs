//! Arguments of `inter-pj webhook`: the webhooks of the Banking, Cobrança
//! and Pix APIs, the addresses Inter calls when something happens in the
//! account.

use clap::{Args, Subcommand, ValueEnum};
use inter_pj::pix::ChavePix;
use inter_pj::webhook::{TipoWebhookBanking, WebhookUrl};

use super::parse_chave;

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
}

#[derive(Debug, Subcommand)]
pub(crate) enum WebhookCobrancaCommand {
    /// Cadastra ou troca o webhook, após mostrar o atual e pedir confirmação
    Cadastrar(WebhookCadastroArgs),
    /// Mostra o webhook
    Consultar,
    /// Exclui o webhook, após mostrá-lo e pedir confirmação
    Excluir(WebhookExclusaoArgs),
}

#[derive(Debug, Subcommand)]
pub(crate) enum WebhookPixCommand {
    /// Cadastra ou troca o webhook de uma chave Pix, após mostrar o atual e pedir confirmação
    Cadastrar(WebhookPixCadastrarArgs),
    /// Mostra o webhook de uma chave Pix
    Consultar(WebhookPixChaveArgs),
    /// Exclui o webhook de uma chave Pix, após mostrá-lo e pedir confirmação
    Excluir(WebhookPixExcluirArgs),
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

fn parse_url(value: &str) -> Result<WebhookUrl, String> {
    WebhookUrl::parse(value).map_err(|err| err.to_string())
}
