//! The immediate charges of the Pix API (`/pix/v2/cob`): QR Codes to pay
//! now, until they expire. The account already has two, from the orders of
//! the statement: the one Cliente Exemplo paid on 02/09 with the Pix of the
//! statement, and one removed. A charge created is active, with the
//! location it asks for or one of its own, in `inter.example`, and its
//! "copia e cola", a dynamic BR Code with a valid CRC16; the answer to the
//! creation of one gets lost.

use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use wiremock::matchers::{path, path_regex};
use wiremock::{MockServer, Request, ResponseTemplate};

use super::cobrancas_pix::{Cobrancas, copia_e_cola, listar, location_invalida};
use super::{problema, requisicao};

/// The Pix key of the account.
const CHAVE: &str = "pix@empresa.example";

/// The charge whose creation is made but whose answer gets lost.
const RESPOSTA_PERDIDA: &str = "pedido1062empresaexemplo2026";

/// The charges of the orders before the guides.
pub(super) fn iniciais() -> Vec<Value> {
    let mut removida = cob(
        "pedido1051empresaexemplo2026",
        "2026-08-31T18:30:00.000Z",
        86_400,
        &json!({"cpf": "12345678909", "nome": "Fulano de Tal"}),
        "320.00",
        "Pedido 1051",
    );
    removida["status"] = json!("REMOVIDA_PELO_USUARIO_RECEBEDOR");
    removida["revisao"] = json!(1);
    let mut paga = cob(
        "pedido1053empresaexemplo2026",
        "2026-09-02T12:10:00.000Z",
        3600,
        &json!({"cnpj": "11222333000181", "nome": "Cliente Exemplo Ltda"}),
        "1500.00",
        "Pedido 1053",
    );
    paga["status"] = json!("CONCLUIDA");
    paga["pix"] = json!([{
        "endToEndId": "E12345678202609021215Po0iU9yT8rE",
        "txid": "pedido1053empresaexemplo2026",
        "valor": "1500.00",
        "chave": CHAVE,
        "horario": "2026-09-02T12:15:38.000Z",
        "infoPagador": "Pedido 1053",
        "devolucoes": [],
    }]);
    vec![removida, paga]
}

/// A charge as the API shows it, active, with a location of its own, which
/// [`Cobrancas`] numbers.
fn cob(
    txid: &str,
    criacao: &str,
    expiracao: u64,
    devedor: &Value,
    valor: &str,
    solicitacao: &str,
) -> Value {
    let location = format!("qrcodepix.inter.example/qr/v2/cob/{txid}");
    let loc = json!({"id": 0, "location": location, "tipoCob": "cob", "criacao": criacao});
    com_location(
        json!({
            "calendario": {"criacao": criacao, "expiracao": expiracao},
            "txid": txid,
            "revisao": 0,
            "status": "ATIVA",
            "devedor": devedor,
            "valor": {"original": valor, "modalidadeAlteracao": 0},
            "chave": CHAVE,
            "solicitacaoPagador": solicitacao,
            "infoAdicionais": [],
            "pix": [],
        }),
        loc,
    )
}

/// `cob` at the location `loc`, with its QR Code.
fn com_location(mut cob: Value, loc: Value) -> Value {
    let location = loc["location"].as_str().unwrap().to_owned();
    cob["pixCopiaECola"] = json!(copia_e_cola(&location));
    cob["location"] = json!(location);
    cob["loc"] = loc;
    cob
}

pub(super) async fn montar(servidor: &MockServer, cobrancas: &Arc<Mutex<Cobrancas>>) {
    let criacao = Arc::clone(cobrancas);
    requisicao("PUT", path_regex(r"^/pix/v2/cob/[^/]+$"))
        .respond_with(move |request: &Request| criar(&mut criacao.lock().unwrap(), request))
        .mount(servidor)
        .await;
    let revisao = Arc::clone(cobrancas);
    requisicao("PATCH", path_regex(r"^/pix/v2/cob/[^/]+$"))
        .respond_with(move |request: &Request| revisar(&mut revisao.lock().unwrap(), request))
        .mount(servidor)
        .await;
    let consulta = Arc::clone(cobrancas);
    requisicao("GET", path_regex(r"^/pix/v2/cob/[^/]+$"))
        .respond_with(move |request: &Request| {
            match achar(&mut consulta.lock().unwrap(), txid(request)) {
                Some(cob) => ResponseTemplate::new(200).set_body_json(&*cob),
                None => nao_encontrada(),
            }
        })
        .mount(servidor)
        .await;
    let lista = Arc::clone(cobrancas);
    requisicao("GET", path("/pix/v2/cob"))
        .respond_with(move |request: &Request| listar(&lista.lock().unwrap().cobs, request))
        .mount(servidor)
        .await;
}

fn txid(request: &Request) -> &str {
    request.url.path().rsplit('/').next().unwrap_or_default()
}

fn achar<'a>(cobrancas: &'a mut Cobrancas, txid: &str) -> Option<&'a mut Value> {
    cobrancas.cobs.iter_mut().find(|cob| cob["txid"] == txid)
}

fn nao_encontrada() -> ResponseTemplate {
    problema(
        404,
        "Cobrança não encontrada",
        "Não há cobrança imediata com este txid.",
    )
}

/// A charge created with its txid, at the time of the clock of
/// [`Cobrancas`].
fn criar(cobrancas: &mut Cobrancas, request: &Request) -> ResponseTemplate {
    let txid = txid(request);
    if achar(cobrancas, txid).is_some() {
        return problema(
            400,
            "txid já utilizado",
            "Já existe uma cobrança com este txid.",
        );
    }
    let corpo: Value = serde_json::from_slice(&request.body).unwrap();
    let criacao = cobrancas.agora();
    let Some(loc) = cobrancas.location("cob", txid, &criacao, corpo.get("loc")) else {
        return location_invalida();
    };
    let mut criada = com_location(
        cob(
            txid,
            &criacao,
            corpo["calendario"]["expiracao"].as_u64().unwrap_or(86_400),
            &corpo["devedor"],
            corpo["valor"]["original"].as_str().unwrap_or_default(),
            corpo["solicitacaoPagador"].as_str().unwrap_or_default(),
        ),
        loc,
    );
    if corpo.get("devedor").is_none() {
        criada.as_object_mut().unwrap().remove("devedor");
    }
    if corpo.get("solicitacaoPagador").is_none() {
        criada.as_object_mut().unwrap().remove("solicitacaoPagador");
    }
    criada["valor"]["modalidadeAlteracao"] = corpo["valor"]["modalidadeAlteracao"].clone();
    criada["infoAdicionais"] = corpo
        .get("infoAdicionais")
        .cloned()
        .unwrap_or_else(|| json!([]));
    cobrancas.cobs.push(criada.clone());
    if txid == RESPOSTA_PERDIDA {
        return ResponseTemplate::new(504);
    }
    ResponseTemplate::new(201).set_body_json(criada)
}

/// A change of an active charge: what came replaces what it had, a new
/// location frees the one it had, and the revision goes up; a paid or
/// removed charge cannot change.
fn revisar(cobrancas: &mut Cobrancas, request: &Request) -> ResponseTemplate {
    let txid = txid(request).to_owned();
    let corpo: Value = serde_json::from_slice(&request.body).unwrap();
    let Some(cob) = achar(cobrancas, &txid) else {
        return nao_encontrada();
    };
    if cob["status"] != "ATIVA" {
        return problema(
            400,
            "Cobrança não pode ser alterada",
            "Só uma cobrança ativa pode ser alterada.",
        );
    }
    if corpo.get("loc").is_some() {
        let criacao = cob["calendario"]["criacao"].as_str().unwrap().to_owned();
        let Some(loc) = cobrancas.location("cob", &txid, &criacao, corpo.get("loc")) else {
            return location_invalida();
        };
        let cob = achar(cobrancas, &txid).unwrap();
        let antiga = cob.as_object_mut().unwrap().remove("loc");
        *cob = com_location(cob.take(), loc);
        if let Some(antiga) = antiga {
            cobrancas.liberar(antiga);
        }
    }
    let cob = achar(cobrancas, &txid).unwrap();
    for campo in ["devedor", "chave", "solicitacaoPagador", "infoAdicionais"] {
        if let Some(valor) = corpo.get(campo) {
            cob[campo] = valor.clone();
        }
    }
    if let Some(expiracao) = corpo["calendario"].get("expiracao") {
        cob["calendario"]["expiracao"] = expiracao.clone();
    }
    for campo in ["original", "modalidadeAlteracao"] {
        if let Some(valor) = corpo["valor"].get(campo) {
            cob["valor"][campo] = valor.clone();
        }
    }
    if let Some(status) = corpo.get("status") {
        cob["status"] = status.clone();
    }
    cob["revisao"] = json!(cob["revisao"].as_u64().unwrap_or_default() + 1);
    ResponseTemplate::new(200).set_body_json(&*cob)
}
