//! The locations of the recurrences (`/pix/v2/locrec`): the addresses of
//! the QR Codes with which the payer approves a recurrence. The account
//! already has the location of the support contract that Cliente Exemplo
//! Ltda rejected in September. A location created serves the recurrence
//! created or changed with it, whose lookup then brings its QR Code; the
//! unlinking frees the location, and the recurrence stays as it is,
//! without it.

use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use wiremock::matchers::{path, path_regex};
use wiremock::{MockServer, Request, ResponseTemplate};

use super::automatico::Automatico;
use super::cobrancas_pix::{crc16, no_periodo, pagina, periodo, periodo_invalido};
use super::{parametros, problema, requisicao};

pub(super) async fn montar(servidor: &MockServer, automatico: &Arc<Mutex<Automatico>>) {
    let criacao = Arc::clone(automatico);
    requisicao("POST", path("/pix/v2/locrec"))
        .respond_with(move |_: &Request| {
            ResponseTemplate::new(201).set_body_json(criacao.lock().unwrap().nova_locrec())
        })
        .mount(servidor)
        .await;
    let lista = Arc::clone(automatico);
    requisicao("GET", path("/pix/v2/locrec"))
        .respond_with(move |request: &Request| listar(&lista.lock().unwrap(), request))
        .mount(servidor)
        .await;
    let consulta = Arc::clone(automatico);
    requisicao("GET", path_regex(r"^/pix/v2/locrec/\d+$"))
        .respond_with(
            move |request: &Request| match consulta.lock().unwrap().locrec(id(request)) {
                Some(loc) => ResponseTemplate::new(200).set_body_json(loc),
                None => nao_encontrada(),
            },
        )
        .mount(servidor)
        .await;
    let desvinculo = Arc::clone(automatico);
    requisicao("DELETE", path_regex(r"^/pix/v2/locrec/\d+/idRec$"))
        .respond_with(move |request: &Request| {
            let id = id(request);
            let mut automatico = desvinculo.lock().unwrap();
            automatico.desvincular_locrec(id);
            match automatico.locrec(id) {
                Some(loc) => ResponseTemplate::new(200).set_body_json(loc),
                None => nao_encontrada(),
            }
        })
        .mount(servidor)
        .await;
}

/// The address of the location `id`, without the scheme.
pub(super) fn location(id: u64) -> String {
    format!("qrcodepix.inter.example/qr/v2/rec/{id}")
}

/// The QR Code of a recurrence with the location `loc` (`dadosQR`): the
/// Pix template has only the GUI, and the location is in field 80.
pub(super) fn dados_qr(loc: &Value) -> Value {
    let campo = |id: &str, valor: &str| format!("{id}{:02}{valor}", valor.len());
    let recorrencia =
        campo("00", "br.gov.bcb.pix") + &campo("25", loc["location"].as_str().unwrap());
    let corpo = [
        campo("00", "01"),
        campo("26", &campo("00", "br.gov.bcb.pix")),
        campo("52", "0000"),
        campo("53", "986"),
        campo("58", "BR"),
        campo("59", "EMPRESA EXEMPLO LTDA"),
        campo("60", "BELO HORIZONTE"),
        campo("62", &campo("05", "***")),
        campo("80", &recorrencia),
        "6304".to_owned(),
    ]
    .concat();
    let crc = crc16(corpo.as_bytes());
    json!({"jornada": "JORNADA_2", "pixCopiaECola": format!("{corpo}{crc:04X}")})
}

/// The location a recurrence asked for that is not free.
pub(super) fn location_invalida() -> ResponseTemplate {
    problema(
        400,
        "Location inválida",
        "A location informada não existe ou já tem uma recorrência.",
    )
}

/// The id of `/pix/v2/locrec/{id}` and `/pix/v2/locrec/{id}/idRec`.
fn id(request: &Request) -> u64 {
    request
        .url
        .path_segments()
        .and_then(|mut partes| partes.nth(3))
        .and_then(|id| id.parse().ok())
        .unwrap_or_default()
}

fn nao_encontrada() -> ResponseTemplate {
    problema(
        404,
        "Location não encontrada",
        "Não há location de recorrência com este id.",
    )
}

/// The locations created in the period, with the filters, in pages from 0.
fn listar(automatico: &Automatico, request: &Request) -> ResponseTemplate {
    let parametros = parametros(request);
    let Some(periodo) = periodo(&parametros) else {
        return periodo_invalido();
    };
    let locs = automatico.locrecs();
    let filtradas: Vec<&Value> = locs
        .iter()
        .filter(|loc| {
            no_periodo(&loc["criacao"], periodo)
                && parametros
                    .get("idRecPresente")
                    .is_none_or(|presente| (presente == "true") == loc.get("idRec").is_some())
        })
        .collect();
    pagina(&filtradas, &parametros, "loc")
}
