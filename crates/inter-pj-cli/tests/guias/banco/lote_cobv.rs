//! The batches of charges with a due date (`/pix/v2/lotecobv`): many charges
//! created or changed at once, processed by the time anyone looks. Each
//! charge is created as `pix cobv criar` creates it, a few seconds after the
//! batch, and the one whose txid is already used is refused, as the API
//! does; a change of the batch changes its charges as `pix cobv revisar`.

use std::sync::{Arc, Mutex};

use chrono::{DateTime, TimeDelta};
use serde_json::{Value, json};
use wiremock::matchers::{path, path_regex};
use wiremock::{MockServer, Request, ResponseTemplate};

use super::cobrancas_pix::{Cobrancas, no_periodo, pagina, periodo, periodo_invalido};
use super::cobv::{alterar, nova};
use super::{parametros, problema, requisicao};

pub(super) async fn montar(servidor: &MockServer, cobrancas: &Arc<Mutex<Cobrancas>>) {
    let criacao = Arc::clone(cobrancas);
    requisicao("PUT", path_regex(r"^/pix/v2/lotecobv/\d+$"))
        .respond_with(move |request: &Request| criar(&mut criacao.lock().unwrap(), request))
        .mount(servidor)
        .await;
    let revisao = Arc::clone(cobrancas);
    requisicao("PATCH", path_regex(r"^/pix/v2/lotecobv/\d+$"))
        .respond_with(move |request: &Request| revisar(&mut revisao.lock().unwrap(), request))
        .mount(servidor)
        .await;
    let consulta = Arc::clone(cobrancas);
    requisicao("GET", path_regex(r"^/pix/v2/lotecobv/\d+$"))
        .respond_with(move |request: &Request| {
            match achar(&consulta.lock().unwrap(), id(request)) {
                Some(lote) => ResponseTemplate::new(200).set_body_json(lote),
                None => nao_encontrado(),
            }
        })
        .mount(servidor)
        .await;
    let resumo = Arc::clone(cobrancas);
    requisicao("GET", path_regex(r"^/pix/v2/lotecobv/\d+/sumario$"))
        .respond_with(
            move |request: &Request| match achar(&resumo.lock().unwrap(), id(request)) {
                Some(lote) => ResponseTemplate::new(200).set_body_json(sumario(&lote)),
                None => nao_encontrado(),
            },
        )
        .mount(servidor)
        .await;
    let situacao = Arc::clone(cobrancas);
    requisicao(
        "GET",
        path_regex(r"^/pix/v2/lotecobv/\d+/situacao/[A-Z_]+$"),
    )
    .respond_with(move |request: &Request| {
        let Some(mut lote) = achar(&situacao.lock().unwrap(), id(request)) else {
            return nao_encontrado();
        };
        let pedida = request.url.path().rsplit('/').next().unwrap_or_default();
        let cobsv = lote["cobsv"].as_array_mut().unwrap();
        cobsv.retain(|cobv| cobv["status"] == pedida);
        ResponseTemplate::new(200).set_body_json(lote)
    })
    .mount(servidor)
    .await;
    let lista = Arc::clone(cobrancas);
    requisicao("GET", path("/pix/v2/lotecobv"))
        .respond_with(move |request: &Request| listar(&lista.lock().unwrap(), request))
        .mount(servidor)
        .await;
}

/// The id of `/pix/v2/lotecobv/{id}` and of what follows it.
fn id(request: &Request) -> u64 {
    request
        .url
        .path_segments()
        .and_then(|mut partes| partes.nth(3))
        .and_then(|id| id.parse().ok())
        .unwrap_or_default()
}

fn achar(cobrancas: &Cobrancas, id: u64) -> Option<Value> {
    cobrancas
        .lotes
        .iter()
        .find(|lote| lote["id"] == id)
        .cloned()
}

fn nao_encontrado() -> ResponseTemplate {
    problema(404, "Lote não encontrado", "Não há lote com este id.")
}

/// A batch received and processed at once: each charge created 3 seconds
/// after the one before, or refused when its txid is already used.
fn criar(cobrancas: &mut Cobrancas, request: &Request) -> ResponseTemplate {
    let id = id(request);
    if achar(cobrancas, id).is_some() {
        return problema(400, "Lote já existe", "Já existe um lote com este id.");
    }
    let corpo: Value = serde_json::from_slice(&request.body).unwrap();
    let criacao = cobrancas.agora();
    let inicio = DateTime::parse_from_rfc3339(&criacao).unwrap();
    let mut situacoes = Vec::new();
    for (i, cobv) in (1..).zip(corpo["cobsv"].as_array().unwrap()) {
        let txid = cobv["txid"].as_str().unwrap();
        let usado = cobrancas.cobvs.iter().any(|outra| outra["txid"] == txid);
        let quando = (inicio + TimeDelta::seconds(3 * i))
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string();
        if usado || nova(cobrancas, txid, &quando, cobv).is_none() {
            situacoes.push(json!({
                "txid": txid,
                "status": "NEGADA",
                "problema": {
                    "title": "Cobrança inválida.",
                    "detail": "A cobrança não respeita as regras da API.",
                    "violacoes": [{
                        "razao": "O txid informado já foi utilizado.",
                        "propriedade": "cobv.txid",
                    }],
                },
            }));
        } else {
            situacoes.push(json!({"txid": txid, "status": "CRIADA", "criacao": quando}));
        }
    }
    cobrancas.lotes.push(json!({
        "id": id,
        "descricao": corpo["descricao"],
        "criacao": criacao,
        "cobsv": situacoes,
    }));
    ResponseTemplate::new(202)
}

/// The changes of the charges of a batch, made at once; a charge that is
/// not of the batch or cannot change is left as it is.
fn revisar(cobrancas: &mut Cobrancas, request: &Request) -> ResponseTemplate {
    let id = id(request);
    let Some(lote) = achar(cobrancas, id) else {
        return nao_encontrado();
    };
    let corpo: Value = serde_json::from_slice(&request.body).unwrap();
    let do_lote = cobrancas.do_lote(id);
    for mudanca in corpo["cobsv"].as_array().into_iter().flatten() {
        let txid = mudanca["txid"].as_str().unwrap();
        if do_lote.iter().any(|criada| criada == txid) {
            let _ = alterar(cobrancas, txid, mudanca);
        }
    }
    if let Some(descricao) = corpo.get("descricao") {
        let posicao = cobrancas
            .lotes
            .iter()
            .position(|cada| cada["id"] == lote["id"]);
        cobrancas.lotes[posicao.unwrap()]["descricao"] = descricao.clone();
    }
    ResponseTemplate::new(202)
}

/// The totals of the processing of a batch.
fn sumario(lote: &Value) -> Value {
    let cobsv = lote["cobsv"].as_array().unwrap();
    let em = |status: &str| cobsv.iter().filter(|cobv| cobv["status"] == status).count();
    json!({
        "dataCriacaoProcessamento": lote["criacao"],
        "statusProcessamento": "FINALIZADO",
        "totalCobrancas": cobsv.len(),
        "totalCobrancasCriadas": em("CRIADA"),
        "totalCobrancasNegadas": em("NEGADA"),
    })
}

/// The batches created in the period, in pages from 0.
fn listar(cobrancas: &Cobrancas, request: &Request) -> ResponseTemplate {
    let parametros = parametros(request);
    let Some(periodo) = periodo(&parametros) else {
        return periodo_invalido();
    };
    let lotes: Vec<&Value> = cobrancas
        .lotes
        .iter()
        .filter(|lote| no_periodo(&lote["criacao"], periodo))
        .collect();
    pagina(&lotes, &parametros, "lotes")
}
