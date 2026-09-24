//! `inter-pj webhook banking|cobranca|pix callbacks|reenviar`: the history
//! of the attempts to send callbacks to the webhooks, and their retry.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::time::Duration;

use inter_pj::pix::{ChavePix, Txid};
use inter_pj::webhook::{
    Callback, FiltroCallbacks, MAX_IDS_REENVIO, PaginaCallbacks, ReenvioCallbacks,
    TipoWebhookBanking,
};
use inter_pj::{Error as InterError, InterClient};
use serde_json::json;

use super::super::cobranca::argumento;
use crate::cli::{CallbacksArgs, Formato};
use crate::commands::Context;
use crate::commands::pix::periodo;
use crate::cores::Tom;
use crate::error::CliError;
use crate::output::{self, horario_local};
use crate::tabela::{Celula, Coluna, Tabela};

/// Callbacks per page of the API when `--itens-por-pagina` is not given.
const ITENS_POR_PAGINA_PADRAO: u32 = 20;

/// Retries the API accepts per minute (production).
const REENVIOS_POR_MINUTO: usize = 5;

/// Whose callbacks: of a kind of the Banking API, of the Cobrança API or of
/// the Pix API (every key).
#[derive(Debug, Clone, Copy)]
pub(super) enum Api {
    Banking(TipoWebhookBanking),
    Cobranca,
    Pix,
}

impl Api {
    /// The field of the payload that names an operation as `reenviar` takes
    /// it, and its heading.
    fn identificador(self) -> (&'static str, &'static str) {
        match self {
            Self::Banking(TipoWebhookBanking::PixPagamento) => {
                ("codigoSolicitacao", "Código da solicitação")
            }
            Self::Banking(TipoWebhookBanking::BoletoPagamento) => {
                ("codigoTransacao", "Código da transação")
            }
            Self::Cobranca => ("codigoSolicitacao", "Código da cobrança"),
            Self::Pix => ("txid", "txid"),
        }
    }

    /// `do webhook pix-pagamento`, `do webhook de cobranças`, `dos webhooks Pix`.
    fn de(self) -> String {
        match self {
            Self::Banking(tipo) => format!("do webhook {tipo}"),
            Self::Cobranca => "do webhook de cobranças".to_owned(),
            Self::Pix => "dos webhooks Pix".to_owned(),
        }
    }

    /// What the filter of a listing names: `endToEnd`, `cobrança`, `txid`.
    fn filtro(self) -> &'static str {
        match self {
            Self::Banking(TipoWebhookBanking::PixPagamento) => "endToEnd",
            Self::Banking(TipoWebhookBanking::BoletoPagamento) => "transação",
            Self::Cobranca => "cobrança",
            Self::Pix => "txid",
        }
    }

    /// The retry command, up to the identifiers; `chave` for Pix.
    fn reenviar(self, chave: Option<&str>) -> String {
        match self {
            Self::Banking(tipo) => format!("inter-pj webhook banking reenviar {tipo}"),
            Self::Cobranca => "inter-pj webhook cobranca reenviar".to_owned(),
            Self::Pix => format!(
                "inter-pj webhook pix reenviar {}",
                chave.map_or_else(|| "CHAVE".to_owned(), argumento)
            ),
        }
    }

    /// The field of the time of an attempt in the CSV, as the API names it.
    fn campo_do_disparo(self) -> &'static str {
        match self {
            Self::Banking(_) => "dataEnvio",
            Self::Cobranca | Self::Pix => "dataHoraDisparo",
        }
    }

    async fn pagina(
        self,
        client: &InterClient,
        filtro: &FiltroCallbacks,
        pagina: u32,
        itens: Option<u32>,
    ) -> Result<PaginaCallbacks, InterError> {
        match self {
            Self::Banking(tipo) => {
                client
                    .banking()
                    .listar_callbacks(tipo, filtro, pagina, itens)
                    .await
            }
            Self::Cobranca => {
                client
                    .cobranca()
                    .listar_callbacks(filtro, pagina, itens)
                    .await
            }
            Self::Pix => client.pix().listar_callbacks(filtro, pagina, itens).await,
        }
    }

    async fn todos(
        self,
        client: &InterClient,
        filtro: &FiltroCallbacks,
    ) -> Result<Vec<Callback>, InterError> {
        match self {
            Self::Banking(tipo) => client.banking().listar_todos_callbacks(tipo, filtro).await,
            Self::Cobranca => client.cobranca().listar_todos_callbacks(filtro).await,
            Self::Pix => client.pix().listar_todos_callbacks(filtro).await,
        }
    }

    async fn reenviar_bloco(
        self,
        client: &InterClient,
        ids: &[String],
        chave: Option<&ChavePix>,
    ) -> Result<ReenvioCallbacks, InterError> {
        match (self, chave) {
            (Self::Banking(tipo), _) => client.banking().reenviar_callbacks(tipo, ids).await,
            (Self::Cobranca, _) => client.cobranca().reenviar_callbacks(ids).await,
            (Self::Pix, Some(chave)) => {
                let txids = ids
                    .iter()
                    .map(|id| id.parse::<Txid>())
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|err| InterError::InvalidInput(Box::new(err)))?;
                client.pix().reenviar_callbacks(chave, &txids).await
            }
            (Self::Pix, None) => Err(InterError::InvalidInput(
                "informe a chave Pix das cobranças".into(),
            )),
        }
    }
}

/// `webhook ... callbacks`: the attempts of a period, latest first.
pub(super) async fn listar(
    context: &Context,
    api: Api,
    identificador: Option<String>,
    args: &CallbacksArgs,
) -> Result<(), CliError> {
    let periodo = periodo(args.periodo)?;
    let mut filtro = FiltroCallbacks::new(periodo.inicio, periodo.fim)
        .map_err(|err| CliError::Usage(err.to_string()))?;
    filtro.identificador = identificador;
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let (todos, pagina) = match args.pagina {
        Some(numero) => {
            let mut pagina = api
                .pagina(&client, &filtro, numero, args.itens_por_pagina)
                .await?;
            let todos = std::mem::take(&mut pagina.data);
            (todos, Some((numero, pagina)))
        }
        None => (api.todos(&client, &filtro).await?, None),
    };
    let mostrados: Vec<Callback> = todos
        .iter()
        .filter(|callback| !args.falhas || callback.sucesso != Some(true))
        .cloned()
        .collect();
    match context.formato() {
        Formato::Json => match pagina {
            Some((_, mut pagina)) => {
                pagina.data = mostrados;
                output::print_json(&pagina)
            }
            None => output::print_json(&json!({ "callbacks": mostrados })),
        },
        Formato::Csv => output::print_raw(&csv(api, &mostrados).csv(context.separador())),
        Formato::Texto => {
            context.warn_if_sandbox(&settings);
            let mut texto = format!("{}\n\n", titulo(api, &filtro, args.falhas));
            if mostrados.is_empty() {
                texto.push_str("Nenhum callback encontrado.");
            } else {
                texto.push_str(&tabela(api, &mostrados).texto_colorido());
                let _ = write!(texto, "\n\n{}", totais(&mostrados));
            }
            match pagina {
                Some((numero, pagina)) => {
                    let itens = args.itens_por_pagina.unwrap_or(ITENS_POR_PAGINA_PADRAO);
                    texto.push_str(&paginacao(numero, &pagina, itens));
                }
                // Across every page, so a later success is not missed.
                None => {
                    if let Some(dica) = sem_entrega(api, &todos) {
                        let _ = write!(texto, "\n\n{dica}");
                    }
                }
            }
            output::print(&texto)
        }
    }
}

/// `Callbacks do webhook de cobranças de 01/09/2026 00:00 a 30/09/2026
/// 23:59`, and the filters.
fn titulo(api: Api, filtro: &FiltroCallbacks, falhas: bool) -> String {
    let formato = "%d/%m/%Y %H:%M";
    let mut texto = format!(
        "Callbacks {} de {} a {}",
        api.de(),
        filtro.inicio.format(formato),
        filtro.fim.format(formato)
    );
    let mut filtros = Vec::new();
    if let Some(id) = &filtro.identificador {
        filtros.push(format!("{} {id}", api.filtro()));
    }
    if falhas {
        filtros.push("só as falhas".to_owned());
    }
    if !filtros.is_empty() {
        let _ = write!(texto, " ({})", filtros.join(", "));
    }
    texto
}

fn tabela(api: Api, callbacks: &[Callback]) -> Tabela {
    let (campo, cabecalho) = api.identificador();
    let com_erro = callbacks
        .iter()
        .any(|callback| callback.mensagem_erro.is_some());
    let mut colunas = vec![
        Coluna::texto("Disparo", ""),
        Coluna::valor("Tentativa", ""),
        Coluna::texto("Entregue", ""),
        Coluna::valor("HTTP", ""),
        Coluna::texto(cabecalho, ""),
    ];
    if com_erro {
        colunas.push(Coluna::texto("Erro", ""));
    }
    let mut tabela = Tabela::new(colunas);
    for callback in callbacks {
        let ids = callback.valores(campo).join(", ");
        let mut linha = vec![
            Celula::texto(callback.disparo().map(horario_local).as_deref()),
            Celula::texto(callback.numero_tentativa.map(|n| n.to_string()).as_deref()),
            match callback.sucesso {
                Some(true) => Celula::situacao(Some("sim"), Some(Tom::Positivo)),
                Some(false) => Celula::situacao(Some("não"), Some(Tom::Negativo)),
                None => Celula::Vazia,
            },
            Celula::texto(
                callback
                    .http_status
                    .map(|status| status.to_string())
                    .as_deref(),
            ),
            Celula::texto(Some(ids.as_str()).filter(|ids| !ids.is_empty())),
        ];
        if com_erro {
            linha.push(Celula::texto(callback.mensagem_erro.as_deref()));
        }
        tabela.linha(linha);
    }
    tabela
}

/// `3 tentativas · 1 entregue · 2 falharam`.
fn totais(callbacks: &[Callback]) -> String {
    let entregues = callbacks
        .iter()
        .filter(|callback| callback.sucesso == Some(true))
        .count();
    let falhas = callbacks
        .iter()
        .filter(|callback| callback.sucesso == Some(false))
        .count();
    let tentativas = match callbacks.len() {
        1 => "1 tentativa".to_owned(),
        n => format!("{n} tentativas"),
    };
    let falharam = match falhas {
        1 => "1 falhou".to_owned(),
        n => format!("{n} falharam"),
    };
    let entregue = match entregues {
        1 => "1 entregue".to_owned(),
        n => format!("{n} entregues"),
    };
    format!("{tentativas} · {entregue} · {falharam}")
}

/// The operations whose callbacks failed with no delivery in the period,
/// and the command that asks for them again.
fn sem_entrega(api: Api, callbacks: &[Callback]) -> Option<String> {
    let (campo, _) = api.identificador();
    let entregues: HashSet<String> = callbacks
        .iter()
        .filter(|callback| callback.sucesso == Some(true))
        .flat_map(|callback| callback.valores(campo))
        .collect();
    let mut pendentes: Vec<String> = Vec::new();
    let mut chaves: Vec<String> = Vec::new();
    for callback in callbacks
        .iter()
        .filter(|callback| callback.sucesso != Some(true))
    {
        for id in callback.valores(campo) {
            if !entregues.contains(&id) && !pendentes.contains(&id) {
                pendentes.push(id);
            }
        }
        for chave in callback.valores("chave") {
            if !chaves.contains(&chave) {
                chaves.push(chave);
            }
        }
    }
    if pendentes.is_empty() {
        return None;
    }
    let quantas = match pendentes.len() {
        1 => "1 operação".to_owned(),
        n => format!("{n} operações"),
    };
    let chave = match chaves.as_slice() {
        [chave] => Some(chave.as_str()),
        _ => None,
    };
    let ids: Vec<String> = pendentes.iter().map(|id| argumento(id)).collect();
    Some(format!(
        "Sem entrega no período: {quantas}. Para pedir o reenvio:\n  {} {}",
        api.reenviar(chave),
        ids.join(" ")
    ))
}

/// Where the page asked for with `--pagina` stands among the others.
fn paginacao(numero: u32, pagina: &PaginaCallbacks, itens: u32) -> String {
    let mut texto = format!("\n\nPágina {numero}");
    if let Some(total) = pagina.total_paginas {
        let _ = write!(texto, " de {} (a primeira é 0)", total.saturating_sub(1));
    }
    if let Some(total) = pagina.total_elementos {
        let _ = write!(texto, "; {total} callbacks no período");
    }
    if pagina.tem_mais(numero, itens) {
        let _ = write!(texto, "; a próxima é --pagina {}", numero + 1);
    }
    texto
}

/// The fields of the API, with the identifier of the operation.
fn csv(api: Api, callbacks: &[Callback]) -> Tabela {
    let (campo, _) = api.identificador();
    let texto = |nome: &'static str| Coluna::texto(nome, nome);
    let mut tabela = Tabela::new(vec![
        texto(api.campo_do_disparo()),
        texto("numeroTentativa"),
        texto("sucesso"),
        texto("httpStatus"),
        texto(campo),
        texto("mensagemErro"),
        texto("webhookUrl"),
    ]);
    for callback in callbacks {
        let ids = callback.valores(campo).join(" ");
        tabela.linha(vec![
            Celula::texto(callback.disparo()),
            Celula::texto(callback.numero_tentativa.map(|n| n.to_string()).as_deref()),
            Celula::texto(
                callback
                    .sucesso
                    .map(|sucesso| if sucesso { "true" } else { "false" }),
            ),
            Celula::texto(
                callback
                    .http_status
                    .map(|status| status.to_string())
                    .as_deref(),
            ),
            Celula::texto(Some(ids.as_str()).filter(|ids| !ids.is_empty())),
            Celula::texto(callback.mensagem_erro.as_deref()),
            Celula::texto(callback.webhook_url.as_deref()),
        ]);
    }
    tabela
}

/// `webhook ... reenviar`: asks for the callbacks of the operations again,
/// in blocks of [`MAX_IDS_REENVIO`]. With more blocks than the API accepts
/// per minute, waits `intervalo` between them.
pub(super) async fn reenviar(
    context: &Context,
    api: Api,
    ids: Vec<String>,
    chave: Option<&ChavePix>,
    intervalo: Duration,
) -> Result<(), CliError> {
    let mut unicos: Vec<String> = Vec::with_capacity(ids.len());
    for id in ids {
        if !unicos.iter().any(|unico| unico.eq_ignore_ascii_case(&id)) {
            unicos.push(id);
        }
    }
    let blocos: Vec<&[String]> = unicos.chunks(MAX_IDS_REENVIO).collect();
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let espera = blocos.len() > REENVIOS_POR_MINUTO;
    if espera {
        eprintln!(
            "aviso: {} operações vão em {} blocos de até {MAX_IDS_REENVIO}; o Inter aceita {REENVIOS_POR_MINUTO} pedidos de reenvio por minuto, então a CLI espera {} s entre eles",
            unicos.len(),
            blocos.len(),
            intervalo.as_secs()
        );
    }
    let mut encontrados: Vec<String> = Vec::new();
    for (numero, bloco) in blocos.iter().enumerate() {
        if numero > 0 && espera {
            tokio::time::sleep(intervalo).await;
        }
        match api.reenviar_bloco(&client, bloco, chave).await {
            Ok(resposta) => encontrados.extend(resposta.found_ids),
            Err(err) if numero == 0 => return Err(err.into()),
            Err(err) => {
                let pedidos = numero * MAX_IDS_REENVIO;
                let restantes: Vec<String> =
                    unicos[pedidos..].iter().map(|id| argumento(id)).collect();
                return Err(CliError::ReenvioIncompleto {
                    source: err,
                    pedidos,
                    total: unicos.len(),
                    restantes: format!(
                        "{} {}",
                        api.reenviar(chave.map(ChavePix::as_str)),
                        restantes.join(" ")
                    ),
                });
            }
        }
    }
    let nao_encontrados: Vec<&String> = unicos
        .iter()
        .filter(|id| {
            !encontrados
                .iter()
                .any(|encontrado| encontrado.eq_ignore_ascii_case(id))
        })
        .collect();
    match context.formato() {
        // The answers of the blocks, together.
        Formato::Json => output::print_json(&json!({ "foundIds": encontrados })),
        // `commands::run` refuses csv for this command.
        Formato::Texto | Formato::Csv => output::print(&resultado(
            unicos.len(),
            encontrados.len(),
            &nao_encontrados,
        )),
    }
}

/// `Reenvio pedido para 2 de 3 operações.`, and those not found.
fn resultado(total: usize, encontrados: usize, nao_encontrados: &[&String]) -> String {
    let operacoes = if total == 1 {
        "operação"
    } else {
        "operações"
    };
    let mut texto = if encontrados == 0 && total == 1 {
        "A operação não foi encontrada: nenhum callback será reenviado.".to_owned()
    } else if encontrados == 0 {
        format!("Nenhuma das {total} operações foi encontrada: nenhum callback será reenviado.")
    } else {
        format!(
            "Reenvio pedido para {encontrados} de {total} {operacoes}: o Inter vai enviar os callbacks de novo."
        )
    };
    if !nao_encontrados.is_empty() && encontrados > 0 {
        texto.push_str("\n\nNão encontradas:");
        for id in nao_encontrados {
            let _ = write!(texto, "\n  {}", output::limpo(id));
        }
    }
    texto
}

#[cfg(test)]
mod tests {
    use chrono::DateTime;
    use serde_json::Value;

    use super::*;

    fn callback(json: Value) -> Callback {
        serde_json::from_value(json).unwrap()
    }

    const UM: &str = "0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d";
    const DOIS: &str = "1c8f5d2b-6e4a-4b3c-8d9e-8f7a6b5c4d3e";

    fn tentativas() -> Vec<Callback> {
        vec![
            callback(json!({
                "payload": [{"codigoSolicitacao": UM}], "numeroTentativa": 2,
                "dataHoraDisparo": "2026-09-24T14:05:00Z", "sucesso": true, "httpStatus": 200
            })),
            callback(json!({
                "payload": [{"codigoSolicitacao": DOIS}], "numeroTentativa": 1,
                "dataHoraDisparo": "2026-09-24T13:45:00Z", "sucesso": false, "httpStatus": 503,
                "mensagemErro": "Service Unavailable"
            })),
            callback(json!({
                "payload": [{"codigoSolicitacao": UM}], "numeroTentativa": 1,
                "dataHoraDisparo": "2026-09-24T13:45:00Z", "sucesso": false, "httpStatus": 503,
                "mensagemErro": "Service Unavailable"
            })),
        ]
    }

    #[test]
    fn a_later_success_is_a_delivery() {
        let dica = sem_entrega(Api::Cobranca, &tentativas()).unwrap();
        assert_eq!(
            dica,
            format!(
                "Sem entrega no período: 1 operação. Para pedir o reenvio:\n  inter-pj webhook cobranca reenviar {DOIS}"
            )
        );
        assert_eq!(sem_entrega(Api::Cobranca, &tentativas()[..1]), None);
        assert_eq!(
            totais(&tentativas()),
            "3 tentativas · 1 entregue · 2 falharam"
        );
    }

    #[test]
    fn pix_retries_name_the_key_of_the_payload() {
        let falha = callback(json!({
            "payload": {"pix": [{"txid": "7978c0c97ea847e78e8849634473c1f1", "chave": "pix@empresa.example"}]},
            "sucesso": false
        }));
        assert!(sem_entrega(Api::Pix, &[falha]).unwrap().ends_with(
            "inter-pj webhook pix reenviar pix@empresa.example 7978c0c97ea847e78e8849634473c1f1"
        ));
        let sem_chave = callback(json!({
            "payload": {"pix": [{"txid": "7978c0c97ea847e78e8849634473c1f1"}]},
            "sucesso": false
        }));
        assert!(
            sem_entrega(Api::Pix, &[sem_chave])
                .unwrap()
                .ends_with("inter-pj webhook pix reenviar CHAVE 7978c0c97ea847e78e8849634473c1f1")
        );
    }

    #[test]
    fn titles_name_the_period_and_the_filters() {
        let mut filtro = FiltroCallbacks::new(
            DateTime::parse_from_rfc3339("2026-09-01T00:00:00-03:00").unwrap(),
            DateTime::parse_from_rfc3339("2026-09-30T23:59:59-03:00").unwrap(),
        )
        .unwrap();
        assert_eq!(
            titulo(
                Api::Banking(TipoWebhookBanking::PixPagamento),
                &filtro,
                false
            ),
            "Callbacks do webhook pix-pagamento de 01/09/2026 00:00 a 30/09/2026 23:59"
        );
        filtro.identificador = Some(UM.to_owned());
        assert_eq!(
            titulo(Api::Cobranca, &filtro, true),
            format!(
                "Callbacks do webhook de cobranças de 01/09/2026 00:00 a 30/09/2026 23:59 (cobrança {UM}, só as falhas)"
            )
        );
    }

    #[test]
    fn listings_use_the_api_names_in_csv() {
        let csv = csv(Api::Cobranca, &tentativas()).csv(crate::tabela::Separador::Virgula);
        assert!(
            csv.starts_with(&format!(
                "dataHoraDisparo,numeroTentativa,sucesso,httpStatus,codigoSolicitacao,mensagemErro,webhookUrl\r\n2026-09-24T14:05:00Z,2,true,200,{UM},,\r\n"
            )),
            "{csv}"
        );
        let banking = csv_de_banking();
        assert!(banking.starts_with("dataEnvio,"), "{banking}");
    }

    fn csv_de_banking() -> String {
        csv(Api::Banking(TipoWebhookBanking::BoletoPagamento), &[])
            .csv(crate::tabela::Separador::Virgula)
    }

    #[test]
    fn the_result_names_those_not_found() {
        let dois = DOIS.to_owned();
        assert_eq!(
            resultado(2, 1, &[&dois]),
            format!(
                "Reenvio pedido para 1 de 2 operações: o Inter vai enviar os callbacks de novo.\n\nNão encontradas:\n  {DOIS}"
            )
        );
        assert_eq!(
            resultado(1, 0, &[&dois]),
            "A operação não foi encontrada: nenhum callback será reenviado."
        );
        assert_eq!(
            resultado(3, 0, &[&dois]),
            "Nenhuma das 3 operações foi encontrada: nenhum callback será reenviado."
        );
    }

    #[tokio::test]
    async fn more_blocks_than_a_minute_allows_are_spaced() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, ResponseTemplate};

        use crate::cli::{Command, WebhookCobrancaCommand, WebhookCommand};
        use crate::commands::testes;

        let codigos: Vec<String> = (0..251)
            .map(|n| format!("0b7e4c1a-5d3f-4a2b-9c8d-{n:012x}"))
            .collect();
        let mut args = vec!["webhook", "cobranca", "reenviar"];
        args.extend(codigos.iter().map(String::as_str));
        let (cenario, comando) = testes::cenario(&args, "boleto-cobranca.write").await;
        let Command::Webhook(WebhookCommand::Cobranca(WebhookCobrancaCommand::Reenviar(args))) =
            comando
        else {
            panic!("{comando:?}");
        };
        Mock::given(method("POST"))
            .and(path("/cobranca/v3/cobrancas/webhook/callbacks/retry"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"foundIds": []})))
            .expect(6)
            .mount(&cenario.server)
            .await;
        let inicio = std::time::Instant::now();
        reenviar(
            &cenario.context,
            Api::Cobranca,
            args.codigos,
            None,
            Duration::from_millis(20),
        )
        .await
        .unwrap();
        // Five waits between the six blocks.
        assert!(inicio.elapsed() >= Duration::from_millis(100));
    }
}
