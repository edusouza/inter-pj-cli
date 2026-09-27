//! The payments the sandbox makes of the Pix charges, as a client would: of
//! an immediate charge or one with a due date by its txid
//! (`/pix/v2/cob/pagar`, `/pix/v2/cobv/pagar`), and of a "copia e cola"
//! (`/pix/v2/sandbox/cob/pagamento`). The charge is paid by a Pix of the
//! amount sent, at the time of the clock of the charges.

use std::sync::{Arc, Mutex};

use chrono::DateTime;
use serde_json::{Value, json};
use wiremock::matchers::{path, path_regex};
use wiremock::{MockServer, Request, ResponseTemplate};

use super::cobrancas_pix::Cobrancas;
use super::{problema, requisicao};

/// Where a charge is: among the immediate ones or those with a due date.
#[derive(Clone, Copy)]
enum Tipo {
    Cob,
    Cobv,
}

pub(super) async fn montar(servidor: &MockServer, cobrancas: &Arc<Mutex<Cobrancas>>) {
    for (tipo, caminho) in [
        (Tipo::Cob, r"^/pix/v2/cob/pagar/[^/]+$"),
        (Tipo::Cobv, r"^/pix/v2/cobv/pagar/[^/]+$"),
    ] {
        let estado = Arc::clone(cobrancas);
        requisicao("POST", path_regex(caminho))
            .respond_with(move |request: &Request| {
                let txid = request.url.path().rsplit('/').next().unwrap_or_default();
                let corpo: Value = serde_json::from_slice(&request.body).unwrap();
                let mut cobrancas = estado.lock().unwrap();
                let posicao = lista(&mut cobrancas, tipo)
                    .iter()
                    .position(|cobranca| cobranca["txid"] == txid);
                match posicao {
                    Some(posicao) => pagar(&mut cobrancas, tipo, posicao, &corpo["valor"]),
                    None => nao_encontrada(),
                }
            })
            .mount(servidor)
            .await;
    }
    let estado = Arc::clone(cobrancas);
    requisicao("POST", path("/pix/v2/sandbox/cob/pagamento"))
        .respond_with(move |request: &Request| {
            let corpo: Value = serde_json::from_slice(&request.body).unwrap();
            let mut cobrancas = estado.lock().unwrap();
            let codigo = &corpo["qrCode"];
            for tipo in [Tipo::Cob, Tipo::Cobv] {
                let posicao = lista(&mut cobrancas, tipo)
                    .iter()
                    .position(|cobranca| cobranca["pixCopiaECola"] == *codigo);
                if let Some(posicao) = posicao {
                    return pagar(&mut cobrancas, tipo, posicao, &corpo["valor"]);
                }
            }
            nao_encontrada()
        })
        .mount(servidor)
        .await;
}

fn lista(cobrancas: &mut Cobrancas, tipo: Tipo) -> &mut Vec<Value> {
    match tipo {
        Tipo::Cob => &mut cobrancas.cobs,
        Tipo::Cobv => &mut cobrancas.cobvs,
    }
}

fn nao_encontrada() -> ResponseTemplate {
    problema(
        404,
        "Cobrança não encontrada",
        "Não há cobrança com este txid ou este código.",
    )
}

/// Pays the charge at `posicao` with a Pix of `valor`, whose endToEndId
/// follows the Pix paid before it.
fn pagar(cobrancas: &mut Cobrancas, tipo: Tipo, posicao: usize, valor: &Value) -> ResponseTemplate {
    if lista(cobrancas, tipo)[posicao]["status"] != "ATIVA" {
        return problema(
            400,
            "Cobrança não pode ser paga",
            "Só uma cobrança ativa pode ser paga.",
        );
    }
    let horario = cobrancas.agora();
    let pagos: usize = cobrancas
        .cobs
        .iter()
        .chain(&cobrancas.cobvs)
        .map(|cobranca| cobranca["pix"].as_array().map_or(0, Vec::len))
        .sum();
    let minuto = DateTime::parse_from_rfc3339(&horario)
        .unwrap()
        .format("%Y%m%d%H%M");
    let e2e = format!("E12345678{minuto}Sbx{:08}", pagos + 1);
    let cobranca = &mut lista(cobrancas, tipo)[posicao];
    cobranca["status"] = json!("CONCLUIDA");
    cobranca["pix"] = json!([{
        "endToEndId": e2e,
        "txid": cobranca["txid"],
        "valor": format!("{:.2}", valor.as_f64().unwrap_or_default()),
        "chave": cobranca["chave"],
        "horario": horario,
        "devolucoes": [],
    }]);
    ResponseTemplate::new(200).set_body_json(json!({"endToEnd": e2e}))
}
