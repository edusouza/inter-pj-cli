//! What only the sandbox of Pix Automático does (`/pix/v2/sandbox`): the
//! answers of the payer and of their bank, simulated on the state of Pix
//! Automático ([`automatico`](super::automatico)). The payer approves or
//! cancels a recurrence and accepts or rejects its confirmation request;
//! their bank cancels a recurring charge or pays its scheduled debit. Each
//! answer takes the next time of the clock.

use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use wiremock::matchers::{path, path_regex};
use wiremock::{MockServer, Request, ResponseTemplate};

use super::automatico::Automatico;
use super::{cobr, problema, requisicao};

pub(super) async fn montar(servidor: &MockServer, automatico: &Arc<Mutex<Automatico>>) {
    let recs = Arc::clone(automatico);
    requisicao(
        "PATCH",
        path_regex(r"^/pix/v2/sandbox/rec/[A-Za-z0-9]+/status$"),
    )
    .respond_with(move |request: &Request| status_rec(&mut recs.lock().unwrap(), request))
    .mount(servidor)
    .await;
    let solicitacoes = Arc::clone(automatico);
    requisicao(
        "PATCH",
        path_regex(r"^/pix/v2/sandbox/solicrec/[A-Za-z0-9]+/status$"),
    )
    .respond_with(move |request: &Request| {
        status_solicitacao(&mut solicitacoes.lock().unwrap(), request)
    })
    .mount(servidor)
    .await;
    let cobrs = Arc::clone(automatico);
    requisicao(
        "PATCH",
        path_regex(r"^/pix/v2/sandbox/cobr/[A-Za-z0-9]+/status$"),
    )
    .respond_with(move |request: &Request| status_cobr(&mut cobrs.lock().unwrap(), request))
    .mount(servidor)
    .await;
    let pagamentos = Arc::clone(automatico);
    requisicao("POST", path("/pix/v2/sandbox/cobr/pagamento"))
        .respond_with(move |request: &Request| pagar_cobr(&mut pagamentos.lock().unwrap(), request))
        .mount(servidor)
        .await;
}

/// The id of `/pix/v2/sandbox/{what}/{id}/status`.
fn id(request: &Request) -> String {
    request
        .url
        .path_segments()
        .and_then(|mut partes| partes.nth(4))
        .unwrap_or_default()
        .to_owned()
}

fn rec_nao_encontrada() -> ResponseTemplate {
    problema(
        404,
        "Recorrência não encontrada",
        "Não há recorrência com este idRec.",
    )
}

fn cobr_nao_encontrada() -> ResponseTemplate {
    problema(
        404,
        "Cobrança não encontrada",
        "Não há cobrança recorrente com este txid.",
    )
}

/// `status` and its time, at the end of `atualizacao` of `valor`.
fn registrar(valor: &mut Value, status: &str, agora: &str) {
    valor["status"] = json!(status);
    valor["atualizacao"]
        .as_array_mut()
        .unwrap()
        .push(json!({"status": status, "data": agora}));
}

/// The payer approves a recurrence waiting for them, or cancels one that
/// has not ended, with the reason of the cancellation.
fn status_rec(automatico: &mut Automatico, request: &Request) -> ResponseTemplate {
    let corpo: Value = serde_json::from_slice(&request.body).unwrap();
    let id = id(request);
    let Some(indice) = automatico.recs.iter().position(|rec| rec["idRec"] == id) else {
        return rec_nao_encontrada();
    };
    let aprovacao = corpo["status"] == "APROVADA";
    let status = &automatico.recs[indice]["status"];
    let aceita = if aprovacao {
        *status == "CRIADA"
    } else {
        *status == "CRIADA" || *status == "APROVADA"
    };
    if !aceita {
        return problema(
            400,
            "Status inválido",
            "Só uma recorrência criada é aprovada, e só uma criada ou aprovada é cancelada.",
        );
    }
    let agora = automatico.agora();
    let rec = &mut automatico.recs[indice];
    if aprovacao {
        registrar(rec, "APROVADA", &agora);
        rec["pagador"] = pagador(&rec["vinculo"]["devedor"], "87654321");
    } else {
        registrar(rec, "CANCELADA", &agora);
        let mut cancelamento = json!({"solicitante": "USUARIO_PAGADOR"});
        if let Some(razao) = corpo.get("razao") {
            cancelamento["codigo"] = razao.clone();
        }
        rec["encerramento"] = json!({"cancelamento": cancelamento});
    }
    ResponseTemplate::new(200)
}

/// The payer of an approved recurrence: the document of `pessoa`, in the
/// bank `ispb`.
fn pagador(pessoa: &Value, ispb: &str) -> Value {
    let mut pagador = json!({"ispbParticipante": ispb, "codMun": "3106200"});
    for campo in ["cpf", "cnpj"] {
        if let Some(documento) = pessoa.get(campo) {
            pagador[campo] = documento.clone();
        }
    }
    pagador
}

/// The payer answers the confirmation request of a recurrence, still
/// waiting for them: accepted, the recurrence is approved; rejected, the
/// recurrence is too.
fn status_solicitacao(automatico: &mut Automatico, request: &Request) -> ResponseTemplate {
    automatico.andamento();
    let corpo: Value = serde_json::from_slice(&request.body).unwrap();
    let id_rec = id(request);
    let Some(indice) = automatico
        .recs
        .iter()
        .position(|rec| rec["idRec"] == id_rec)
    else {
        return rec_nao_encontrada();
    };
    let Some(pendente) = automatico.solicitacoes.iter().position(|solicitacao| {
        solicitacao["idRec"] == id_rec && solicitacao["status"] == "RECEBIDA"
    }) else {
        return problema(
            400,
            "Solicitação não encontrada",
            "Não há solicitação de confirmação desta recorrência à espera do pagador.",
        );
    };
    let aceita = corpo["status"] == "ACEITA";
    let agora = automatico.agora();
    let solicitacao = &mut automatico.solicitacoes[pendente];
    registrar(
        solicitacao,
        if aceita { "ACEITA" } else { "REJEITADA" },
        &agora,
    );
    let destinatario = solicitacao["destinatario"].clone();
    let rec = &mut automatico.recs[indice];
    if aceita {
        registrar(rec, "APROVADA", &agora);
        rec["pagador"] = pagador(
            &destinatario,
            destinatario["ispbParticipante"].as_str().unwrap(),
        );
        rec["ativacao"] = json!({"tipoJornada": "JORNADA_1"});
    } else {
        registrar(rec, "REJEITADA", &agora);
        rec["encerramento"] = json!({"rejeicao": {
            "codigo": "AP13",
            "descricao": "Rejeitada pelo usuário pagador",
        }});
    }
    ResponseTemplate::new(200)
}

/// The payer's bank cancels a charge not settled yet, with the reason, and
/// its scheduled debit.
fn status_cobr(automatico: &mut Automatico, request: &Request) -> ResponseTemplate {
    cobr::agendar(automatico);
    let corpo: Value = serde_json::from_slice(&request.body).unwrap();
    let txid = id(request);
    let Some(indice) = automatico
        .cobrs
        .iter()
        .position(|cobr| cobr["txid"] == txid)
    else {
        return cobr_nao_encontrada();
    };
    if automatico.cobrs[indice]["status"] != "ATIVA" {
        return problema(
            400,
            "Cobrança encerrada",
            "Só uma cobrança com o débito agendado pode ser cancelada.",
        );
    }
    let agora = automatico.agora();
    let cobr = &mut automatico.cobrs[indice];
    for tentativa in cobr["tentativas"].as_array_mut().unwrap() {
        if tentativa["status"] == "AGENDADA" {
            registrar(tentativa, "CANCELADA", &agora);
        }
    }
    registrar(cobr, "CANCELADA", &agora);
    cobr["encerramento"] = json!({"cancelamento": {
        "solicitante": "PSP_PAGADOR",
        "codigo": corpo["razao"],
    }});
    ResponseTemplate::new(200)
}

/// The payer's bank pays the scheduled debit of a charge, now.
fn pagar_cobr(automatico: &mut Automatico, request: &Request) -> ResponseTemplate {
    cobr::agendar(automatico);
    let corpo: Value = serde_json::from_slice(&request.body).unwrap();
    let Some(indice) = automatico
        .cobrs
        .iter()
        .position(|cobr| cobr["txid"] == corpo["txId"])
    else {
        return cobr_nao_encontrada();
    };
    if automatico.cobrs[indice]["status"] != "ATIVA" {
        return problema(
            400,
            "Cobrança não pode ser paga",
            "Só uma cobrança com o débito agendado pode ser paga.",
        );
    }
    let agora = automatico.agora();
    let cobr = &mut automatico.cobrs[indice];
    let tentativa = cobr["tentativas"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|tentativa| tentativa["status"] == "AGENDADA")
        .unwrap();
    registrar(tentativa, "PAGA", &agora);
    let e2e = tentativa["endToEndId"].clone();
    registrar(cobr, "CONCLUIDA", &agora);
    ResponseTemplate::new(200).set_body_json(json!({"endToEnd": e2e}))
}
