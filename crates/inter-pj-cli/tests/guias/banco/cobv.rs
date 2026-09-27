//! The Pix charges with a due date (`/pix/v2/cobv`), the boleto of the Pix.
//! The account already has the monthly fees of Beltrana de Tal: September's,
//! removed, and October's, active. A charge created is active, with the
//! amount and the fine, interest, rebate and discount it was sent with, a
//! location in `inter.example` and its "copia e cola", which a change keeps.

use std::sync::{Arc, Mutex};

use chrono::{DateTime, TimeDelta};
use serde_json::{Value, json};
use wiremock::matchers::{path, path_regex};
use wiremock::{MockServer, Request, ResponseTemplate};

use super::cob::{copia_e_cola, listar};
use super::{problema, requisicao};
use crate::sessao::HOJE;

/// The Pix key of the account.
const CHAVE: &str = "pix@empresa.example";

/// Days after the due date in which a charge can be paid, when it does not
/// say.
const VALIDADE_PADRAO: u64 = 30;

struct Estado {
    cobvs: Vec<Value>,
}

impl Estado {
    /// The charges of the monthly fees before the guides.
    fn novo() -> Self {
        let beltrana = json!({
            "logradouro": "Rua dos Timbiras, 45",
            "cidade": "Belo Horizonte",
            "uf": "MG",
            "cep": "30140060",
            "cpf": "01234567890",
            "nome": "Beltrana de Tal",
            "email": "beltrana@cliente.example",
        });
        let mut setembro = cobv(
            "mensalidade202609beltranadetal",
            7100,
            "2026-08-25T12:00:00.000Z",
            "2026-09-10",
            &beltrana,
            "Mensalidade de setembro",
        );
        setembro["status"] = json!("REMOVIDA_PELO_USUARIO_RECEBEDOR");
        setembro["revisao"] = json!(1);
        let outubro = cobv(
            "mensalidade202610beltranadetal",
            7101,
            "2026-09-15T12:00:00.000Z",
            "2026-10-10",
            &beltrana,
            "Mensalidade de outubro",
        );
        Self {
            cobvs: vec![setembro, outubro],
        }
    }

    fn achar(&mut self, txid: &str) -> Option<&mut Value> {
        self.cobvs.iter_mut().find(|cobv| cobv["txid"] == txid)
    }
}

/// A monthly fee of R$ 450,00 as the API shows it, active, with a fine of
/// R$ 9,00 and interest of R$ 0,15 a day.
fn cobv(
    txid: &str,
    loc: u64,
    criacao: &str,
    vencimento: &str,
    devedor: &Value,
    solicitacao: &str,
) -> Value {
    let location = format!("qrcodepix.inter.example/qr/v2/cobv/{txid}");
    json!({
        "calendario": {
            "criacao": criacao,
            "dataDeVencimento": vencimento,
            "validadeAposVencimento": VALIDADE_PADRAO,
        },
        "txid": txid,
        "revisao": 0,
        "loc": {"id": loc, "location": location, "tipoCob": "cobv", "criacao": criacao},
        "status": "ATIVA",
        "devedor": devedor,
        "recebedor": recebedor(),
        "valor": {
            "original": "450.00",
            "multa": {"modalidade": 1, "valorPerc": "9.00"},
            "juros": {"modalidade": 1, "valorPerc": "0.15"},
        },
        "chave": CHAVE,
        "solicitacaoPagador": solicitacao,
        "infoAdicionais": [],
        "pixCopiaECola": copia_e_cola(&location),
        "pix": [],
    })
}

/// The account that receives, as the API shows it in each charge.
fn recebedor() -> Value {
    json!({
        "cnpj": "11444777000161",
        "nome": "Empresa Exemplo Ltda",
        "nomeFantasia": "Empresa Exemplo",
        "cidade": "Belo Horizonte",
        "uf": "MG",
    })
}

pub(super) async fn montar(servidor: &MockServer) {
    let estado = Arc::new(Mutex::new(Estado::novo()));
    let criacao = Arc::clone(&estado);
    requisicao("PUT", path_regex(r"^/pix/v2/cobv/[^/]+$"))
        .respond_with(move |request: &Request| criar(&mut criacao.lock().unwrap(), request))
        .mount(servidor)
        .await;
    let revisao = Arc::clone(&estado);
    requisicao("PATCH", path_regex(r"^/pix/v2/cobv/[^/]+$"))
        .respond_with(move |request: &Request| revisar(&mut revisao.lock().unwrap(), request))
        .mount(servidor)
        .await;
    let consulta = Arc::clone(&estado);
    requisicao("GET", path_regex(r"^/pix/v2/cobv/[^/]+$"))
        .respond_with(move |request: &Request| {
            match consulta.lock().unwrap().achar(txid(request)) {
                Some(cobv) => ResponseTemplate::new(200).set_body_json(&*cobv),
                None => nao_encontrada(),
            }
        })
        .mount(servidor)
        .await;
    requisicao("GET", path("/pix/v2/cobv"))
        .respond_with(move |request: &Request| listar(&estado.lock().unwrap().cobvs, request))
        .mount(servidor)
        .await;
}

fn txid(request: &Request) -> &str {
    request.url.path().rsplit('/').next().unwrap_or_default()
}

fn nao_encontrada() -> ResponseTemplate {
    problema(
        404,
        "Cobrança não encontrada",
        "Não há cobrança com vencimento com este txid.",
    )
}

/// A charge created with its txid, at 10:20:07 of today in Brasília and
/// then 6 minutes apart.
fn criar(estado: &mut Estado, request: &Request) -> ResponseTemplate {
    let txid = txid(request);
    if estado.achar(txid).is_some() {
        return problema(
            400,
            "txid já utilizado",
            "Já existe uma cobrança com este txid.",
        );
    }
    let corpo: Value = serde_json::from_slice(&request.body).unwrap();
    let novas = estado.cobvs.len() - 2;
    let criacao = DateTime::parse_from_rfc3339(&format!("{HOJE}T13:20:07.000Z")).unwrap()
        + TimeDelta::minutes(6 * i64::try_from(novas).unwrap());
    let mut criada = cobv(
        txid,
        7102 + u64::try_from(novas).unwrap(),
        &criacao.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
        corpo["calendario"]["dataDeVencimento"].as_str().unwrap(),
        &corpo["devedor"],
        "",
    );
    if let Some(validade) = corpo["calendario"].get("validadeAposVencimento") {
        criada["calendario"]["validadeAposVencimento"] = validade.clone();
    }
    criada["valor"] = corpo["valor"].clone();
    match corpo.get("solicitacaoPagador") {
        Some(solicitacao) => criada["solicitacaoPagador"] = solicitacao.clone(),
        None => {
            criada.as_object_mut().unwrap().remove("solicitacaoPagador");
        }
    }
    if let Some(infos) = corpo.get("infoAdicionais") {
        criada["infoAdicionais"] = infos.clone();
    }
    estado.cobvs.push(criada.clone());
    ResponseTemplate::new(201).set_body_json(criada)
}

/// A change of an active charge: what came replaces what it had, each
/// charge of the amount whole, and the revision goes up; a paid or removed
/// charge cannot change.
fn revisar(estado: &mut Estado, request: &Request) -> ResponseTemplate {
    let corpo: Value = serde_json::from_slice(&request.body).unwrap();
    let Some(cobv) = estado.achar(txid(request)) else {
        return nao_encontrada();
    };
    if cobv["status"] != "ATIVA" {
        return problema(
            400,
            "Cobrança não pode ser alterada",
            "Só uma cobrança ativa pode ser alterada.",
        );
    }
    for campo in [
        "devedor",
        "chave",
        "solicitacaoPagador",
        "infoAdicionais",
        "status",
    ] {
        if let Some(valor) = corpo.get(campo) {
            cobv[campo] = valor.clone();
        }
    }
    for campo in ["dataDeVencimento", "validadeAposVencimento"] {
        if let Some(valor) = corpo["calendario"].get(campo) {
            cobv["calendario"][campo] = valor.clone();
        }
    }
    for campo in ["original", "multa", "juros", "abatimento", "desconto"] {
        if let Some(valor) = corpo["valor"].get(campo) {
            cobv["valor"][campo] = valor.clone();
        }
    }
    cobv["revisao"] = json!(cobv["revisao"].as_u64().unwrap_or_default() + 1);
    ResponseTemplate::new(200).set_body_json(&*cobv)
}
