//! The locations of the Pix charges (`/pix/v2/loc`), the addresses of their
//! QR Codes: one created for a charge to come, the listing with the charge
//! each one serves, and the unlinking of a charge, which frees its location
//! for the next.

use std::sync::{Arc, Mutex};

use serde_json::Value;
use wiremock::matchers::{path, path_regex};
use wiremock::{MockServer, Request, ResponseTemplate};

use super::cobrancas_pix::{Cobrancas, no_periodo, pagina, periodo, periodo_invalido};
use super::{parametros, problema, requisicao};

pub(super) async fn montar(servidor: &MockServer, cobrancas: &Arc<Mutex<Cobrancas>>) {
    let criacao = Arc::clone(cobrancas);
    requisicao("POST", path("/pix/v2/loc"))
        .respond_with(move |request: &Request| {
            let corpo: Value = serde_json::from_slice(&request.body).unwrap();
            let tipo = corpo["tipoCob"].as_str().unwrap_or("cob");
            ResponseTemplate::new(201).set_body_json(criacao.lock().unwrap().livre(tipo))
        })
        .mount(servidor)
        .await;
    let lista = Arc::clone(cobrancas);
    requisicao("GET", path("/pix/v2/loc"))
        .respond_with(move |request: &Request| listar(&lista.lock().unwrap(), request))
        .mount(servidor)
        .await;
    let consulta = Arc::clone(cobrancas);
    requisicao("GET", path_regex(r"^/pix/v2/loc/\d+$"))
        .respond_with(move |request: &Request| {
            match achar(&consulta.lock().unwrap(), id(request)) {
                Some(loc) => ResponseTemplate::new(200).set_body_json(loc),
                None => nao_encontrada(),
            }
        })
        .mount(servidor)
        .await;
    let desvinculo = Arc::clone(cobrancas);
    requisicao("DELETE", path_regex(r"^/pix/v2/loc/\d+/txid$"))
        .respond_with(move |request: &Request| {
            let id = id(request);
            let mut cobrancas = desvinculo.lock().unwrap();
            cobrancas.desvincular(id);
            match achar(&cobrancas, id) {
                Some(loc) => ResponseTemplate::new(200).set_body_json(loc),
                None => nao_encontrada(),
            }
        })
        .mount(servidor)
        .await;
}

/// The id of `/pix/v2/loc/{id}` and `/pix/v2/loc/{id}/txid`.
fn id(request: &Request) -> u64 {
    request
        .url
        .path_segments()
        .and_then(|mut partes| partes.nth(3))
        .and_then(|id| id.parse().ok())
        .unwrap_or_default()
}

/// A location, with the txid of its charge.
fn achar(cobrancas: &Cobrancas, id: u64) -> Option<Value> {
    cobrancas.locs().into_iter().find(|loc| loc["id"] == id)
}

fn nao_encontrada() -> ResponseTemplate {
    problema(
        404,
        "Location não encontrada",
        "Não há location com este id.",
    )
}

/// The locations created in the period, with the filters, in pages from 0.
fn listar(cobrancas: &Cobrancas, request: &Request) -> ResponseTemplate {
    let parametros = parametros(request);
    let Some(periodo) = periodo(&parametros) else {
        return periodo_invalido();
    };
    let locs = cobrancas.locs();
    let filtradas: Vec<&Value> = locs
        .iter()
        .filter(|loc| {
            no_periodo(&loc["criacao"], periodo)
                && parametros
                    .get("txIdPresente")
                    .is_none_or(|presente| (presente == "true") == loc.get("txid").is_some())
                && parametros
                    .get("tipoCob")
                    .is_none_or(|tipo| loc["tipoCob"] == tipo.as_str())
        })
        .collect();
    pagina(&filtradas, &parametros, "loc")
}
