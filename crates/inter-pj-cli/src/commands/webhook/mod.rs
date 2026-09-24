//! `inter-pj webhook banking|cobranca|pix|recorrencia|cobranca-recorrente`:
//! the webhooks, the addresses Inter calls when something happens in the
//! account. Registering or
//! removing one shows the current webhook and asks for confirmation: a new
//! address receives the notifications of the account's payments.

mod callbacks;

use std::fmt::Write as _;
use std::net::IpAddr;
use std::time::Duration;

use chrono::{Local, TimeZone};
use inter_pj::pix::ChavePix;
use inter_pj::pix_automatico::TipoWebhookPixAutomatico;
use inter_pj::webhook::{TipoWebhookBanking, Webhook, WebhookUrl};
use inter_pj::{Environment, Error as InterError, InterClient};
use serde_json::{Map, Value, json};

use self::callbacks::Api;
use super::cobranca::argumento;
use crate::cli::{
    Formato, WebhookBankingCommand, WebhookCadastroArgs, WebhookCobrancaCommand, WebhookCommand,
    WebhookExclusaoArgs, WebhookPixAutomaticoCommand, WebhookPixCommand,
};
use crate::commands::Context;
use crate::confirmacao::{Stdio, Terminal, confirmar, descrever_ambiente, pode_confirmar};
use crate::error::{CliError, resultado_incerto};
use crate::output::{self, horario_em, limpo, secao};

/// Between blocks of a retry, when there are more than the API accepts
/// per minute.
const INTERVALO_REENVIO: Duration = Duration::from_secs(12);

pub(super) async fn run(context: &Context, command: WebhookCommand) -> Result<(), CliError> {
    match command {
        WebhookCommand::Banking(command) => banking(context, command).await,
        WebhookCommand::Cobranca(command) => cobranca(context, command).await,
        WebhookCommand::Pix(command) => pix(context, command).await,
        WebhookCommand::Recorrencia(command) => {
            let alvo = Alvo::PixAutomatico(TipoWebhookPixAutomatico::Recorrencia);
            pix_automatico(context, &alvo, command).await
        }
        WebhookCommand::CobrancaRecorrente(command) => {
            let alvo = Alvo::PixAutomatico(TipoWebhookPixAutomatico::CobrancaRecorrente);
            pix_automatico(context, &alvo, command).await
        }
    }
}

async fn banking(context: &Context, command: WebhookBankingCommand) -> Result<(), CliError> {
    match command {
        WebhookBankingCommand::Cadastrar(args) => {
            let alvo = Alvo::Banking(args.tipo.into());
            cadastrar(context, &alvo, &args.cadastro, &mut Stdio).await
        }
        WebhookBankingCommand::Consultar(args) => {
            let alvos: Vec<Alvo> = match args.tipo {
                Some(tipo) => vec![Alvo::Banking(tipo.into())],
                None => TipoWebhookBanking::TODOS.map(Alvo::Banking).to_vec(),
            };
            consultar(context, &alvos).await
        }
        WebhookBankingCommand::Excluir(args) => {
            let alvo = Alvo::Banking(args.tipo.into());
            excluir(context, &alvo, &args.exclusao, &mut Stdio).await
        }
        WebhookBankingCommand::Callbacks(args) => {
            let tipo = TipoWebhookBanking::from(args.tipo);
            let identificador =
                identificador_banking(tipo, args.end_to_end, args.codigo_transacao)?;
            callbacks::listar(context, Api::Banking(tipo), identificador, &args.callbacks).await
        }
        WebhookBankingCommand::Reenviar(args) => {
            let api = Api::Banking(args.tipo.into());
            callbacks::reenviar(context, api, args.codigos, None, INTERVALO_REENVIO).await
        }
    }
}

async fn cobranca(context: &Context, command: WebhookCobrancaCommand) -> Result<(), CliError> {
    match command {
        WebhookCobrancaCommand::Cadastrar(args) => {
            cadastrar(context, &Alvo::Cobranca, &args, &mut Stdio).await
        }
        WebhookCobrancaCommand::Consultar => consultar(context, &[Alvo::Cobranca]).await,
        WebhookCobrancaCommand::Excluir(args) => {
            excluir(context, &Alvo::Cobranca, &args, &mut Stdio).await
        }
        WebhookCobrancaCommand::Callbacks(args) => {
            callbacks::listar(context, Api::Cobranca, args.codigo, &args.callbacks).await
        }
        WebhookCobrancaCommand::Reenviar(args) => {
            callbacks::reenviar(
                context,
                Api::Cobranca,
                args.codigos,
                None,
                INTERVALO_REENVIO,
            )
            .await
        }
    }
}

async fn pix(context: &Context, command: WebhookPixCommand) -> Result<(), CliError> {
    match command {
        WebhookPixCommand::Cadastrar(args) => {
            let alvo = Alvo::Pix(args.chave);
            cadastrar(context, &alvo, &args.cadastro, &mut Stdio).await
        }
        WebhookPixCommand::Consultar(args) => consultar(context, &[Alvo::Pix(args.chave)]).await,
        WebhookPixCommand::Excluir(args) => {
            let alvo = Alvo::Pix(args.chave);
            excluir(context, &alvo, &args.exclusao, &mut Stdio).await
        }
        WebhookPixCommand::Callbacks(args) => {
            let txid = args.txid.map(|txid| txid.as_str().to_owned());
            callbacks::listar(context, Api::Pix, txid, &args.callbacks).await
        }
        WebhookPixCommand::Reenviar(args) => {
            let txids = args
                .txids
                .iter()
                .map(|txid| txid.as_str().to_owned())
                .collect();
            let chave = Some(&args.chave);
            callbacks::reenviar(context, Api::Pix, txids, chave, INTERVALO_REENVIO).await
        }
    }
}

/// The webhook of the recurrences or of the recurring charges.
async fn pix_automatico(
    context: &Context,
    alvo: &Alvo,
    command: WebhookPixAutomaticoCommand,
) -> Result<(), CliError> {
    match command {
        WebhookPixAutomaticoCommand::Cadastrar(args) => {
            cadastrar(context, alvo, &args, &mut Stdio).await
        }
        WebhookPixAutomaticoCommand::Consultar => {
            consultar(context, std::slice::from_ref(alvo)).await
        }
        WebhookPixAutomaticoCommand::Excluir(args) => {
            excluir(context, alvo, &args, &mut Stdio).await
        }
    }
}

/// The filter of a Banking history: the `endToEnd` of a Pix sent or the
/// code of the transaction of a boleto paid, each for its kind.
fn identificador_banking(
    tipo: TipoWebhookBanking,
    end_to_end: Option<String>,
    codigo_transacao: Option<String>,
) -> Result<Option<String>, CliError> {
    match (tipo, end_to_end, codigo_transacao) {
        (_, None, None) => Ok(None),
        (TipoWebhookBanking::PixPagamento, Some(e2e), None) => Ok(Some(e2e)),
        (TipoWebhookBanking::BoletoPagamento, None, Some(codigo)) => Ok(Some(codigo)),
        (TipoWebhookBanking::PixPagamento, _, Some(_)) => Err(CliError::Usage(
            "--codigo-transacao vale para boleto-pagamento; nos Pix enviados, use --end-to-end"
                .to_owned(),
        )),
        (TipoWebhookBanking::BoletoPagamento, Some(_), _) => Err(CliError::Usage(
            "--end-to-end vale para pix-pagamento; nos boletos pagos, use --codigo-transacao"
                .to_owned(),
        )),
    }
}

/// Which webhook: of a kind of the Banking API, of the Cobrança API, of a
/// Pix key or of Pix Automático.
#[derive(Debug, Clone)]
enum Alvo {
    Banking(TipoWebhookBanking),
    Cobranca,
    Pix(ChavePix),
    PixAutomatico(TipoWebhookPixAutomatico),
}

impl Alvo {
    /// `do tipo pix-pagamento`, `de cobranças`, `da chave pix@empresa.example`.
    fn de(&self) -> String {
        match self {
            Self::Banking(tipo) => format!("do tipo {tipo}"),
            Self::Cobranca => "de cobranças".to_owned(),
            Self::Pix(chave) => format!("da chave {chave}"),
            Self::PixAutomatico(TipoWebhookPixAutomatico::Recorrencia) => {
                "de recorrências".to_owned()
            }
            Self::PixAutomatico(_) => "de cobranças recorrentes".to_owned(),
        }
    }

    /// What it notifies.
    fn notifica(&self) -> &'static str {
        match self {
            Self::Banking(tipo) => tipo.descricao(),
            Self::Cobranca => "cobranças recebidas, canceladas e expiradas",
            Self::Pix(_) => "cobranças Pix pagas (imediatas e com vencimento)",
            Self::PixAutomatico(TipoWebhookPixAutomatico::Recorrencia) => {
                "mudanças de status das recorrências do Pix Automático"
            }
            Self::PixAutomatico(_) => {
                "mudanças de status das cobranças recorrentes do Pix Automático"
            }
        }
    }

    /// Where the notifications arrive, when Inter adds a path to the
    /// address: `https://api.empresa.example/inter/rec`.
    fn entrega(&self, url: &str) -> Option<String> {
        match self {
            Self::PixAutomatico(tipo) => Some(format!("{url}{}", tipo.sufixo())),
            _ => None,
        }
    }

    /// `inter-pj webhook ... <acao> ...`, ready to paste in a shell.
    fn comando(&self, acao: &str) -> String {
        match self {
            Self::Banking(tipo) => format!("inter-pj webhook banking {acao} {tipo}"),
            Self::Cobranca => format!("inter-pj webhook cobranca {acao}"),
            Self::Pix(chave) => {
                format!("inter-pj webhook pix {acao} {}", argumento(chave.as_str()))
            }
            Self::PixAutomatico(TipoWebhookPixAutomatico::Recorrencia) => {
                format!("inter-pj webhook recorrencia {acao}")
            }
            Self::PixAutomatico(_) => format!("inter-pj webhook cobranca-recorrente {acao}"),
        }
    }

    async fn consultar(&self, client: &InterClient) -> Result<Option<Webhook>, InterError> {
        match self {
            Self::Banking(tipo) => client.banking().consultar_webhook(*tipo).await,
            Self::Cobranca => client.cobranca().consultar_webhook().await,
            Self::Pix(chave) => client.pix().consultar_webhook(chave).await,
            Self::PixAutomatico(tipo) => client.pix_automatico().consultar_webhook(*tipo).await,
        }
    }

    async fn cadastrar(&self, client: &InterClient, url: &WebhookUrl) -> Result<(), InterError> {
        match self {
            Self::Banking(tipo) => client.banking().cadastrar_webhook(*tipo, url).await,
            Self::Cobranca => client.cobranca().cadastrar_webhook(url).await,
            Self::Pix(chave) => client.pix().cadastrar_webhook(chave, url).await,
            Self::PixAutomatico(tipo) => {
                client.pix_automatico().cadastrar_webhook(*tipo, url).await
            }
        }
    }

    async fn excluir(&self, client: &InterClient) -> Result<(), InterError> {
        match self {
            Self::Banking(tipo) => client.banking().excluir_webhook(*tipo).await,
            Self::Cobranca => client.cobranca().excluir_webhook().await,
            Self::Pix(chave) => client.pix().excluir_webhook(chave).await,
            Self::PixAutomatico(tipo) => client.pix_automatico().excluir_webhook(*tipo).await,
        }
    }
}

async fn consultar(context: &Context, alvos: &[Alvo]) -> Result<(), CliError> {
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let mut webhooks = Vec::with_capacity(alvos.len());
    for alvo in alvos {
        webhooks.push(alvo.consultar(&client).await?);
    }
    match context.formato() {
        Formato::Json => {
            if let [webhook] = webhooks.as_slice() {
                return output::print_json(webhook);
            }
            // Both kinds of the Banking API, by kind.
            let mut todos = Map::new();
            for (alvo, webhook) in alvos.iter().zip(&webhooks) {
                if let Alvo::Banking(tipo) = alvo {
                    todos.insert(tipo.as_str().to_owned(), json!(webhook));
                }
            }
            output::print_json(&Value::Object(todos))
        }
        // `commands::run` refuses csv for this command.
        Formato::Texto | Formato::Csv => {
            context.warn_if_sandbox(&settings);
            let textos: Vec<String> = alvos
                .iter()
                .zip(&webhooks)
                .map(|(alvo, webhook)| render(alvo, webhook.as_ref()))
                .collect();
            output::print(&textos.join("\n\n"))
        }
    }
}

async fn cadastrar(
    context: &Context,
    alvo: &Alvo,
    args: &WebhookCadastroArgs,
    terminal: &mut dyn Terminal,
) -> Result<(), CliError> {
    // Nothing is looked up when no one could confirm.
    pode_confirmar(terminal, args.sim)?;
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let atual = alvo.consultar(&client).await?;
    let url_atual = atual
        .as_ref()
        .and_then(|webhook| webhook.webhook_url.as_deref());
    if url_atual == Some(args.url.as_str()) {
        return match context.formato() {
            Formato::Json => output::print_json(&atual),
            Formato::Texto | Formato::Csv => output::print(&format!(
                "O webhook já usa esta URL: nada a alterar.\n\n{}",
                render(alvo, atual.as_ref())
            )),
        };
    }
    let ambiente = settings.ambiente.as_ref().map(|setting| setting.value);
    output::eprint(&resumo(alvo, url_atual, &args.url, ambiente));
    let pergunta = if url_atual.is_some() {
        "Trocar a URL do webhook?"
    } else {
        "Cadastrar o webhook?"
    };
    confirmar(terminal, args.sim, pergunta)?;
    alvo.cadastrar(&client, &args.url)
        .await
        .map_err(|err| incerto(err, alvo, "o webhook pode ter sido cadastrado"))?;
    match context.formato() {
        Formato::Json => {
            let mut cadastrado = json!({ "webhookUrl": args.url.as_str() });
            if let Alvo::Pix(chave) = alvo {
                cadastrado["chave"] = json!(chave.as_str());
            }
            output::print_json(&cadastrado)
        }
        Formato::Texto | Formato::Csv => output::print(&format!(
            "Webhook cadastrado: o Inter passa a notificar {} em {}.\n\nConfira com: {}",
            alvo.notifica(),
            alvo.entrega(args.url.as_str())
                .unwrap_or_else(|| args.url.to_string()),
            alvo.comando("consultar")
        )),
    }
}

async fn excluir(
    context: &Context,
    alvo: &Alvo,
    args: &WebhookExclusaoArgs,
    terminal: &mut dyn Terminal,
) -> Result<(), CliError> {
    // Nothing is looked up when no one could confirm.
    pode_confirmar(terminal, args.sim)?;
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let Some(atual) = alvo.consultar(&client).await? else {
        return Err(CliError::Usage(format!(
            "nenhum webhook {} cadastrado: não há o que excluir",
            alvo.de()
        )));
    };
    let ambiente = settings.ambiente.as_ref().map(|setting| setting.value);
    output::eprint(&resumo_exclusao(alvo, &atual, ambiente));
    confirmar(terminal, args.sim, "Excluir o webhook?")?;
    alvo.excluir(&client)
        .await
        .map_err(|err| incerto(err, alvo, "o webhook pode ter sido excluído"))?;
    match context.formato() {
        Formato::Json => output::print_json(&atual),
        Formato::Texto | Formato::Csv => output::print(&format!(
            "Webhook excluído: o Inter deixa de notificar {}.",
            alvo.notifica()
        )),
    }
}

/// An unknown outcome comes with the command that shows the webhook.
fn incerto(err: InterError, alvo: &Alvo, situacao: &'static str) -> CliError {
    if resultado_incerto(&err) {
        CliError::WebhookIncerto {
            source: err,
            situacao,
            consulta: alvo.comando("consultar"),
        }
    } else {
        err.into()
    }
}

/// The webhook about to be registered, and the one it replaces.
fn resumo(
    alvo: &Alvo,
    atual: Option<&str>,
    nova: &WebhookUrl,
    ambiente: Option<Environment>,
) -> String {
    let mut linhas = vec![
        ("Ambiente", descrever_ambiente(ambiente)),
        ("Notifica", alvo.notifica().to_owned()),
    ];
    if let Some(atual) = atual {
        linhas.push(("URL atual", limpo(atual).into_owned()));
    }
    linhas.push(("Nova URL", nova.to_string()));
    if let Some(entrega) = alvo.entrega(nova.as_str()) {
        linhas.push(("Entrega em", entrega));
    }
    let acao = if atual.is_some() {
        "a trocar"
    } else {
        "a cadastrar"
    };
    let mut texto = secao(&format!("Webhook {} {acao}", alvo.de()), &linhas);
    let host_atual = atual
        .and_then(|atual| WebhookUrl::parse(atual).ok())
        .map(|url| url.host().to_owned());
    if host_atual
        .as_deref()
        .is_some_and(|host| host != nova.host())
    {
        let _ = write!(
            texto,
            "\naviso: as notificações passam a ir para {}, e não mais para {}",
            nova.host(),
            limpo(host_atual.as_deref().unwrap_or_default())
        );
    }
    if endereco_local(nova.host()) {
        let _ = write!(
            texto,
            "\naviso: {} é um endereço local ou de rede privada; o Inter precisa alcançar a URL pela internet",
            nova.host()
        );
    }
    texto
}

/// The webhook about to be removed.
fn resumo_exclusao(alvo: &Alvo, atual: &Webhook, ambiente: Option<Environment>) -> String {
    resumo_exclusao_em(alvo, atual, ambiente, &Local)
}

fn resumo_exclusao_em<Tz: TimeZone>(
    alvo: &Alvo,
    atual: &Webhook,
    ambiente: Option<Environment>,
    fuso: &Tz,
) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let mut linhas = vec![("Ambiente", descrever_ambiente(ambiente))];
    linhas.extend(detalhes(alvo, atual, fuso));
    let mut texto = secao(&format!("Webhook {} a excluir", alvo.de()), &linhas);
    let _ = write!(
        texto,
        "\naviso: o Inter deixa de notificar {}",
        alvo.notifica()
    );
    texto
}

/// What the API tells of a webhook.
fn detalhes<Tz: TimeZone>(alvo: &Alvo, webhook: &Webhook, fuso: &Tz) -> Vec<(&'static str, String)>
where
    Tz::Offset: std::fmt::Display,
{
    let mut linhas = vec![("Notifica", alvo.notifica().to_owned())];
    if let Some(url) = &webhook.webhook_url {
        linhas.push(("URL", limpo(url).into_owned()));
        if let Some(entrega) = alvo.entrega(url) {
            linhas.push(("Entrega em", limpo(&entrega).into_owned()));
        }
    }
    if let Some(criacao) = &webhook.criacao {
        linhas.push(("Cadastrado em", horario_em(criacao, fuso)));
    }
    if let Some(atualizacao) = &webhook.atualizacao {
        linhas.push(("Alterado em", horario_em(atualizacao, fuso)));
    }
    linhas
}

/// A webhook, with the times in the local time zone.
fn render(alvo: &Alvo, webhook: Option<&Webhook>) -> String {
    render_em(alvo, webhook, &Local)
}

fn render_em<Tz: TimeZone>(alvo: &Alvo, webhook: Option<&Webhook>, fuso: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let titulo = format!("Webhook {}", alvo.de());
    match webhook {
        Some(webhook) => secao(&titulo, &detalhes(alvo, webhook, fuso)),
        None => format!(
            "{titulo}\n  Nenhum webhook cadastrado: o Inter não notifica {}.\n  Para cadastrar: {} --url https://...",
            alvo.notifica(),
            alvo.comando("cadastrar")
        ),
    }
}

/// Whether Inter surely cannot reach `host`: a name or an address of the
/// machine itself or of a private network.
fn endereco_local(host: &str) -> bool {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = host.parse::<IpAddr>() {
        return match ip {
            IpAddr::V4(ip) => {
                ip.is_loopback() || ip.is_private() || ip.is_link_local() || ip.is_unspecified()
            }
            IpAddr::V6(ip) => {
                ip.is_loopback()
                    || ip.is_unspecified()
                    || ip.is_unique_local()
                    || ip.is_unicast_link_local()
            }
        };
    }
    host == "localhost"
        || [".localhost", ".local", ".internal", ".lan", ".home.arpa"]
            .iter()
            .any(|sufixo| host.ends_with(sufixo))
}

#[cfg(test)]
mod tests {
    use chrono::FixedOffset;
    use wiremock::matchers::{any, method, path};
    use wiremock::{Mock, ResponseTemplate};

    use super::*;
    use crate::cli::Command;
    use crate::commands::testes;
    use crate::confirmacao::testes::TerminalFalso;

    const URL: &str = "https://api.empresa.example/inter/webhook";

    fn webhook(json: Value) -> Webhook {
        serde_json::from_value(json).unwrap()
    }

    fn brasilia() -> FixedOffset {
        FixedOffset::west_opt(3 * 3600).unwrap()
    }

    #[test]
    fn a_webhook_in_detail() {
        let cobrancas = webhook(json!({
            "webhookUrl": URL,
            "criacao": "2026-09-01T12:00:00Z",
            "atualizacao": "2026-09-24T13:15:00Z"
        }));
        assert_eq!(
            render_em(&Alvo::Cobranca, Some(&cobrancas), &brasilia()),
            "\
Webhook de cobranças
  Notifica       cobranças recebidas, canceladas e expiradas
  URL            https://api.empresa.example/inter/webhook
  Cadastrado em  01/09/2026 09:00:00
  Alterado em    24/09/2026 10:15:00"
        );
        let chave: ChavePix = "pix@empresa.example".parse().unwrap();
        assert_eq!(
            render_em(&Alvo::Pix(chave), None, &brasilia()),
            "\
Webhook da chave pix@empresa.example
  Nenhum webhook cadastrado: o Inter não notifica cobranças Pix pagas (imediatas e com vencimento).
  Para cadastrar: inter-pj webhook pix cadastrar pix@empresa.example --url https://..."
        );
    }

    #[test]
    fn pix_automatico_webhooks_say_where_the_notifications_arrive() {
        let recorrencias = Alvo::PixAutomatico(TipoWebhookPixAutomatico::Recorrencia);
        let atual = webhook(json!({"webhookUrl": URL, "criacao": "2026-09-01T12:00:00Z"}));
        assert_eq!(
            render_em(&recorrencias, Some(&atual), &brasilia()),
            "\
Webhook de recorrências
  Notifica       mudanças de status das recorrências do Pix Automático
  URL            https://api.empresa.example/inter/webhook
  Entrega em     https://api.empresa.example/inter/webhook/rec
  Cadastrado em  01/09/2026 09:00:00"
        );
        let cobrancas = Alvo::PixAutomatico(TipoWebhookPixAutomatico::CobrancaRecorrente);
        assert_eq!(
            render_em(&cobrancas, None, &brasilia()),
            "\
Webhook de cobranças recorrentes
  Nenhum webhook cadastrado: o Inter não notifica mudanças de status das cobranças recorrentes do Pix Automático.
  Para cadastrar: inter-pj webhook cobranca-recorrente cadastrar --url https://..."
        );
        let nova: WebhookUrl = "https://api.empresa.example/pix-automatico"
            .parse()
            .unwrap();
        assert!(
            resumo(&cobrancas, None, &nova, None)
                .ends_with("\n  Entrega em  https://api.empresa.example/pix-automatico/cobr"),
            "{}",
            resumo(&cobrancas, None, &nova, None)
        );
        assert_eq!(
            recorrencias.comando("excluir"),
            "inter-pj webhook recorrencia excluir"
        );
        // The other webhooks get their notifications at the address itself.
        assert_eq!(Alvo::Cobranca.entrega(URL), None);
    }

    #[test]
    fn hints_quote_what_the_shell_would_read() {
        let chave: ChavePix = "financeiro&cia@empresa.example".parse().unwrap();
        assert_eq!(
            Alvo::Pix(chave).comando("consultar"),
            "inter-pj webhook pix consultar 'financeiro&cia@empresa.example'"
        );
        assert_eq!(
            Alvo::Banking(TipoWebhookBanking::BoletoPagamento).comando("excluir"),
            "inter-pj webhook banking excluir boleto-pagamento"
        );
    }

    #[test]
    fn a_new_address_is_shown_with_the_one_it_replaces() {
        let nova: WebhookUrl = "https://novo.empresa.example/inter".parse().unwrap();
        let texto = resumo(
            &Alvo::Banking(TipoWebhookBanking::PixPagamento),
            Some(URL),
            &nova,
            Some(Environment::Production),
        );
        assert_eq!(
            texto,
            "\
Webhook do tipo pix-pagamento a trocar
  Ambiente   PRODUÇÃO (conta real)
  Notifica   Pix enviados pela conta
  URL atual  https://api.empresa.example/inter/webhook
  Nova URL   https://novo.empresa.example/inter
aviso: as notificações passam a ir para novo.empresa.example, e não mais para api.empresa.example"
        );
        // Another path on the same server needs no warning.
        let mesma: WebhookUrl = "https://api.empresa.example/outro".parse().unwrap();
        assert!(!resumo(&Alvo::Cobranca, Some(URL), &mesma, None).contains("aviso"));

        let local: WebhookUrl = "https://192.168.0.10:8443/inter".parse().unwrap();
        let texto = resumo(&Alvo::Cobranca, None, &local, Some(Environment::Sandbox));
        assert!(
            texto.starts_with("Webhook de cobranças a cadastrar\n"),
            "{texto}"
        );
        assert!(
            texto.ends_with("aviso: 192.168.0.10 é um endereço local ou de rede privada; o Inter precisa alcançar a URL pela internet"),
            "{texto}"
        );
    }

    #[test]
    fn a_removal_says_what_stops() {
        let atual = webhook(json!({"webhookUrl": URL, "criacao": "2026-09-01T12:00:00Z"}));
        assert_eq!(
            resumo_exclusao_em(
                &Alvo::Banking(TipoWebhookBanking::BoletoPagamento),
                &atual,
                Some(Environment::Sandbox),
                &brasilia()
            ),
            "\
Webhook do tipo boleto-pagamento a excluir
  Ambiente       sandbox (dados fictícios)
  Notifica       boletos pagos pela conta
  URL            https://api.empresa.example/inter/webhook
  Cadastrado em  01/09/2026 09:00:00
aviso: o Inter deixa de notificar boletos pagos pela conta"
        );
    }

    #[test]
    fn private_addresses_are_recognized() {
        for local in [
            "localhost",
            "api.localhost",
            "servidor.local",
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.0.1",
            "[::1]",
            "[fd00::1]",
            "[fe80::1]",
        ] {
            assert!(endereco_local(local), "{local}");
        }
        for publico in ["api.empresa.example", "203.0.113.10", "[2001:db8::1]"] {
            assert!(!endereco_local(publico), "{publico}");
        }
    }

    // --- the commands against a mock API ------------------------------------

    async fn cenario(args: &[&str]) -> (testes::Cenario, WebhookCommand) {
        let mut todos = vec!["webhook"];
        todos.extend_from_slice(args);
        match testes::cenario(&todos, "boleto-cobranca.read boleto-cobranca.write").await {
            (cenario, Command::Webhook(comando)) => (cenario, comando),
            (_, outro) => panic!("{outro:?}"),
        }
    }

    /// Only the lookup is sent, answering `atual`.
    async fn apenas_a_consulta(cenario: &testes::Cenario, atual: Option<Value>) {
        let resposta = match atual {
            Some(atual) => ResponseTemplate::new(200).set_body_json(atual),
            None => ResponseTemplate::new(404),
        };
        Mock::given(method("GET"))
            .and(path("/cobranca/v3/cobrancas/webhook"))
            .respond_with(resposta)
            .expect(1)
            .mount(&cenario.server)
            .await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&cenario.server)
            .await;
    }

    fn cadastro(comando: WebhookCommand) -> WebhookCadastroArgs {
        match comando {
            WebhookCommand::Cobranca(WebhookCobrancaCommand::Cadastrar(args)) => args,
            outro => panic!("{outro:?}"),
        }
    }

    fn exclusao(comando: WebhookCommand) -> WebhookExclusaoArgs {
        match comando {
            WebhookCommand::Cobranca(WebhookCobrancaCommand::Excluir(args)) => args,
            outro => panic!("{outro:?}"),
        }
    }

    #[tokio::test]
    async fn a_declined_registration_only_looks_the_webhook_up() {
        let (cenario, comando) = cenario(&[
            "cobranca",
            "cadastrar",
            "--url",
            "https://novo.empresa.example/inter",
        ])
        .await;
        apenas_a_consulta(&cenario, Some(json!({"webhookUrl": URL}))).await;
        let mut terminal = TerminalFalso::respondendo("n\n");
        let err = cadastrar(
            &cenario.context,
            &Alvo::Cobranca,
            &cadastro(comando),
            &mut terminal,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, CliError::Cancelado), "{err}");
        assert_eq!(terminal.perguntas, ["Trocar a URL do webhook? [s/N] "]);
    }

    #[tokio::test]
    async fn the_same_address_changes_nothing() {
        let (cenario, comando) = cenario(&["cobranca", "cadastrar", "--url", URL]).await;
        apenas_a_consulta(&cenario, Some(json!({"webhookUrl": URL}))).await;
        let mut terminal = TerminalFalso::respondendo("");
        cadastrar(
            &cenario.context,
            &Alvo::Cobranca,
            &cadastro(comando),
            &mut terminal,
        )
        .await
        .unwrap();
        assert!(terminal.perguntas.is_empty());
    }

    #[tokio::test]
    async fn a_missing_webhook_has_nothing_to_remove() {
        let (cenario, comando) = cenario(&["cobranca", "excluir", "--sim"]).await;
        apenas_a_consulta(&cenario, None).await;
        let err = excluir(
            &cenario.context,
            &Alvo::Cobranca,
            &exclusao(comando),
            &mut TerminalFalso::respondendo(""),
        )
        .await
        .unwrap_err();
        assert_eq!(
            err.to_string(),
            "nenhum webhook de cobranças cadastrado: não há o que excluir"
        );
    }

    #[tokio::test]
    async fn a_declined_removal_only_looks_the_webhook_up() {
        let (cenario, comando) = cenario(&["cobranca", "excluir"]).await;
        apenas_a_consulta(&cenario, Some(json!({"webhookUrl": URL}))).await;
        let mut terminal = TerminalFalso::respondendo("\n");
        let err = excluir(
            &cenario.context,
            &Alvo::Cobranca,
            &exclusao(comando),
            &mut terminal,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, CliError::Cancelado), "{err}");
        assert_eq!(terminal.perguntas, ["Excluir o webhook? [s/N] "]);
    }
}
