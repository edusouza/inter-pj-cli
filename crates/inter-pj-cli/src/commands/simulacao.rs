//! `--simular`: the request a command would send, without sending it.

use std::fmt::Write as _;

use inter_pj::endpoint::Endpoint;
use serde::Serialize;
use serde_json::{Map, json};

use super::Context;
use crate::cli::Formato;
use crate::config::Settings;
use crate::error::CliError;
use crate::output;

/// Prints the method, URL, headers (the account masked) and body of the
/// request to `endpoint`. Nothing is sent and no token is requested.
pub(super) fn mostrar(
    context: &Context,
    settings: &Settings,
    endpoint: Endpoint,
    cabecalhos: &[(&str, String)],
    corpo: &impl Serialize,
) -> Result<(), CliError> {
    mostrar_em(context, settings, endpoint, &[], cabecalhos, corpo)
}

/// [`mostrar`] for an endpoint whose path has `{placeholders}`, filled with
/// `parametros`.
pub(super) fn mostrar_em(
    context: &Context,
    settings: &Settings,
    endpoint: Endpoint,
    parametros: &[(&str, &str)],
    cabecalhos: &[(&str, String)],
    corpo: &impl Serialize,
) -> Result<(), CliError> {
    let base = settings
        .effective_base_url()
        .unwrap_or_else(|| "<URL do ambiente>".to_owned());
    let mut caminho = endpoint.path.to_owned();
    for (nome, valor) in parametros {
        caminho = caminho.replace(&format!("{{{nome}}}"), valor);
    }
    let url = format!("{}{caminho}", base.trim_end_matches('/'));
    let corpo = serde_json::to_value(corpo)
        .map_err(|err| CliError::io("falha ao gerar JSON", std::io::Error::other(err)))?;
    let mut todos = Map::new();
    for (nome, valor) in cabecalhos {
        todos.insert((*nome).to_owned(), json!(valor));
    }
    if let Some(conta) = &settings.conta_corrente {
        todos.insert(
            "x-conta-corrente".to_owned(),
            json!(output::mask(&conta.value, 2)),
        );
    }
    let metodo = endpoint.method.as_str();
    match context.formato() {
        Formato::Json => output::print_json(&json!({
            "simulacao": true,
            "metodo": metodo,
            "url": url,
            "cabecalhos": todos,
            "corpo": corpo,
        })),
        Formato::Texto | Formato::Csv => {
            let mut texto = format!("Simulação: nada foi enviado.\n\n{metodo} {url}");
            for (nome, valor) in &todos {
                let _ = write!(texto, "\n{nome}: {}", valor.as_str().unwrap_or_default());
            }
            let corpo = serde_json::to_string_pretty(&corpo).unwrap_or_default();
            let _ = write!(texto, "\n\n{corpo}");
            output::print(&texto)
        }
    }
}
