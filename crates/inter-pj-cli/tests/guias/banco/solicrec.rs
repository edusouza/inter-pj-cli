//! The confirmation requests of Pix Automático (`/pix/v2/solicrec`): a
//! recurrence sent to the payer's bank, which asks them to approve it. The
//! account already has the request Fulano de Tal accepted and the one
//! Cliente Exemplo Ltda rejected. A request created is sent and received by
//! the payer's bank by the time anyone looks at it, and waits for the
//! payer's answer; one created or received can be cancelled.

use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use wiremock::matchers::{path, path_regex};
use wiremock::{MockServer, Request, ResponseTemplate};

use super::automatico::Automatico;
use super::{problema, requisicao};

/// The requests before the guides, with the recurrences as the payers saw
/// them.
pub(super) fn iniciais(recs: &[Value]) -> Vec<Value> {
    let rec = |id: &str| {
        recs.iter()
            .find(|rec| rec["idRec"] == id)
            .map(payload)
            .unwrap()
    };
    vec![
        json!({
            "idSolicRec": "SC1234567820260901h3Rw8Kd5Nb2",
            "idRec": "RR1234567820260901k7Tq2Wm9Zp4",
            "calendario": {"dataExpiracaoSolicitacao": "2026-09-08T02:59:59.000Z"},
            "status": "ACEITA",
            "destinatario": {
                "cpf": "12345678909",
                "conta": "1234567",
                "ispbParticipante": "87654321",
                "agencia": "0001",
            },
            "atualizacao": [
                {"status": "CRIADA", "data": "2026-09-01T13:12:04.000Z"},
                {"status": "ENVIADA", "data": "2026-09-01T13:12:06.000Z"},
                {"status": "RECEBIDA", "data": "2026-09-01T13:12:09.000Z"},
                {"status": "ACEITA", "data": "2026-09-02T11:42:17.000Z"},
            ],
            "recPayload": rec("RR1234567820260901k7Tq2Wm9Zp4"),
        }),
        json!({
            "idSolicRec": "SC1234567820260910r6Gt1Xm4Hs8",
            "idRec": "RN1234567820260910m2Hc6Vy8Qd1",
            "calendario": {"dataExpiracaoSolicitacao": "2026-09-17T02:59:59.000Z"},
            "status": "REJEITADA",
            "destinatario": {
                "cnpj": "11222333000181",
                "conta": "7654321",
                "ispbParticipante": "87654321",
                "agencia": "0001",
            },
            "atualizacao": [
                {"status": "CRIADA", "data": "2026-09-10T17:35:12.000Z"},
                {"status": "ENVIADA", "data": "2026-09-10T17:35:14.000Z"},
                {"status": "RECEBIDA", "data": "2026-09-10T17:35:17.000Z"},
                {"status": "REJEITADA", "data": "2026-09-12T12:05:09.000Z"},
            ],
            "recPayload": rec("RN1234567820260910m2Hc6Vy8Qd1"),
        }),
    ]
}

/// The recurrence as the payer's bank shows it to them.
fn payload(rec: &Value) -> Value {
    json!({
        "idRec": rec["idRec"],
        "vinculo": rec["vinculo"],
        "calendario": rec["calendario"],
        "valor": rec["valor"],
        "recebedor": rec["recebedor"],
        "politicaRetentativa": rec["politicaRetentativa"],
    })
}

pub(super) async fn montar(servidor: &MockServer, automatico: &Arc<Mutex<Automatico>>) {
    let criacao = Arc::clone(automatico);
    requisicao("POST", path("/pix/v2/solicrec"))
        .respond_with(move |request: &Request| criar(&mut criacao.lock().unwrap(), request))
        .mount(servidor)
        .await;
    let consulta = Arc::clone(automatico);
    requisicao("GET", path_regex(r"^/pix/v2/solicrec/[A-Za-z0-9]+$"))
        .respond_with(move |request: &Request| {
            let mut automatico = consulta.lock().unwrap();
            automatico.andamento();
            match achar(&mut automatico, id(request)) {
                Some(solicitacao) => ResponseTemplate::new(200).set_body_json(&*solicitacao),
                None => nao_encontrada(),
            }
        })
        .mount(servidor)
        .await;
    let cancelamento = Arc::clone(automatico);
    requisicao("PATCH", path_regex(r"^/pix/v2/solicrec/[A-Za-z0-9]+$"))
        .respond_with(move |request: &Request| cancelar(&mut cancelamento.lock().unwrap(), request))
        .mount(servidor)
        .await;
}

fn id(request: &Request) -> &str {
    request.url.path().rsplit('/').next().unwrap_or_default()
}

fn achar<'a>(automatico: &'a mut Automatico, id: &str) -> Option<&'a mut Value> {
    automatico
        .solicitacoes
        .iter_mut()
        .find(|solicitacao| solicitacao["idSolicRec"] == id)
}

fn nao_encontrada() -> ResponseTemplate {
    problema(
        404,
        "Solicitação não encontrada",
        "Não há solicitação de confirmação com este idSolicRec.",
    )
}

/// A request of the approval of a recurrence waiting for it, created now.
fn criar(automatico: &mut Automatico, request: &Request) -> ResponseTemplate {
    let corpo: Value = serde_json::from_slice(&request.body).unwrap();
    let id_rec = corpo["idRec"].as_str().unwrap_or_default();
    let Some(rec) = automatico
        .recs
        .iter()
        .find(|rec| rec["idRec"] == id_rec)
        .cloned()
    else {
        return problema(
            404,
            "Recorrência não encontrada",
            "Não há recorrência com este idRec.",
        );
    };
    if rec["status"] != "CRIADA" {
        return problema(
            400,
            "Recorrência não aguarda aprovação",
            "Só uma recorrência criada, que aguarda a aprovação do pagador, recebe uma solicitação.",
        );
    }
    let agora = automatico.agora();
    let solicitacao = json!({
        "idSolicRec": automatico.id_solicitacao(),
        "idRec": id_rec,
        "calendario": corpo["calendario"],
        "status": "CRIADA",
        "destinatario": corpo["destinatario"],
        "atualizacao": [{"status": "CRIADA", "data": agora}],
        "recPayload": payload(&rec),
    });
    automatico.solicitacoes.push(solicitacao.clone());
    ResponseTemplate::new(201).set_body_json(solicitacao)
}

/// The cancellation of a request not answered yet.
fn cancelar(automatico: &mut Automatico, request: &Request) -> ResponseTemplate {
    automatico.andamento();
    let agora = automatico.agora();
    let Some(solicitacao) = achar(automatico, id(request)) else {
        return nao_encontrada();
    };
    if !["CRIADA", "RECEBIDA"]
        .iter()
        .any(|status| solicitacao["status"] == *status)
    {
        return problema(
            400,
            "Solicitação não pode ser cancelada",
            "Só uma solicitação criada ou recebida, sem resposta do pagador, pode ser cancelada.",
        );
    }
    solicitacao["status"] = json!("CANCELADA");
    solicitacao["atualizacao"]
        .as_array_mut()
        .unwrap()
        .push(json!({"status": "CANCELADA", "data": agora}));
    ResponseTemplate::new(200).set_body_json(&*solicitacao)
}
