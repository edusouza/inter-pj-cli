//! The recurrences of Pix Automático (`/pix/v2/rec`): the payer's
//! authorization of the charges of a contract. The account already has
//! Fulano de Tal's basic plan, approved in September by a confirmation
//! request, Cliente Exemplo Ltda's support contract, which the payer
//! rejected, and Beltrana de Tal's monthly fee, created with the wrong
//! amount and waiting for her approval. A recurrence created is waiting for
//! the payer; the answer to the creation of Cliente Exemplo Ltda's second
//! contract gets lost, though the recurrence is created. A change of the
//! first payment or of the payer's name is made at once, and so is a
//! cancellation.

use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use wiremock::matchers::{path, path_regex};
use wiremock::{MockServer, Request, ResponseTemplate};

use super::automatico::Automatico;
use super::cobrancas_pix::{no_periodo, pagina, periodo, periodo_invalido};
use super::{parametros, problema, requisicao};

/// The contract whose creation is made but whose answer gets lost.
const RESPOSTA_PERDIDA: &str = "suporte-2026-007";

/// The company, as the recurrences show it.
fn recebedor() -> Value {
    json!({"cnpj": "11444777000161", "nome": "Empresa Exemplo Ltda"})
}

/// The recurrences before the guides.
pub(super) fn iniciais() -> Vec<Value> {
    let fulano = json!({
        "idRec": "RR1234567820260901k7Tq2Wm9Zp4",
        "vinculo": {
            "objeto": "Plano básico",
            "devedor": {"cpf": "12345678909", "nome": "Fulano de Tal"},
            "contrato": "plano-basico-0042",
        },
        "calendario": {"dataInicial": "2026-09-10", "periodicidade": "MENSAL"},
        "valor": {"valorRec": "89.90"},
        "recebedor": recebedor(),
        "pagador": {"cpf": "12345678909", "ispbParticipante": "87654321", "codMun": "3106200"},
        "status": "APROVADA",
        "politicaRetentativa": "PERMITE_3R_7D",
        "atualizacao": [
            {"status": "CRIADA", "data": "2026-09-01T13:10:00.000Z"},
            {"status": "APROVADA", "data": "2026-09-02T11:42:17.000Z"},
        ],
        "ativacao": {"tipoJornada": "JORNADA_1"},
        "solicitacao": [{
            "idSolicRec": "SC1234567820260901h3Rw8Kd5Nb2",
            "status": "ACEITA",
            "calendario": {"dataExpiracaoSolicitacao": "2026-09-08T02:59:59.000Z"},
        }],
    });
    let cliente = json!({
        "idRec": "RN1234567820260910m2Hc6Vy8Qd1",
        "vinculo": {
            "objeto": "Suporte técnico",
            "devedor": {"cnpj": "11222333000181", "nome": "Cliente Exemplo Ltda"},
            "contrato": "suporte-2026-003",
        },
        "calendario": {
            "dataInicial": "2026-10-05",
            "dataFinal": "2027-09-05",
            "periodicidade": "MENSAL",
        },
        "valor": {"valorRec": "1200.00"},
        "recebedor": recebedor(),
        "status": "REJEITADA",
        "politicaRetentativa": "NAO_PERMITE",
        "atualizacao": [
            {"status": "CRIADA", "data": "2026-09-10T17:32:45.000Z"},
            {"status": "REJEITADA", "data": "2026-09-12T12:05:09.000Z"},
        ],
        "encerramento": {"rejeicao": {
            "codigo": "AP14",
            "descricao": "Rejeitada pelo usuário pagador, sem interesse no Pix Automático para o recebedor",
        }},
        "solicitacao": [{
            "idSolicRec": "SC1234567820260910r6Gt1Xm4Hs8",
            "status": "REJEITADA",
            "calendario": {"dataExpiracaoSolicitacao": "2026-09-17T02:59:59.000Z"},
        }],
    });
    let beltrana = json!({
        "idRec": "RR1234567820260920p5Jx3Ls7Gv0",
        "vinculo": {
            "objeto": "Mensalidade",
            "devedor": {"cpf": "01234567890", "nome": "Beltrana de Tal"},
            "contrato": "mensalidade-beltrana-2026",
        },
        "calendario": {"dataInicial": "2026-11-10", "periodicidade": "MENSAL"},
        "valor": {"valorRec": "405.00"},
        "recebedor": recebedor(),
        "status": "CRIADA",
        "politicaRetentativa": "PERMITE_3R_7D",
        "atualizacao": [{"status": "CRIADA", "data": "2026-09-20T14:05:33.000Z"}],
    });
    vec![fulano, cliente, beltrana]
}

pub(super) async fn montar(servidor: &MockServer, automatico: &Arc<Mutex<Automatico>>) {
    let criacao = Arc::clone(automatico);
    requisicao("POST", path("/pix/v2/rec"))
        .respond_with(move |request: &Request| criar(&mut criacao.lock().unwrap(), request))
        .mount(servidor)
        .await;
    let lista = Arc::clone(automatico);
    requisicao("GET", path("/pix/v2/rec"))
        .respond_with(move |request: &Request| listar(&lista.lock().unwrap(), request))
        .mount(servidor)
        .await;
    let consulta = Arc::clone(automatico);
    requisicao("GET", path_regex(r"^/pix/v2/rec/[A-Za-z0-9]+$"))
        .respond_with(move |request: &Request| {
            match achar(&mut consulta.lock().unwrap(), id(request)) {
                Some(rec) => ResponseTemplate::new(200).set_body_json(&*rec),
                None => nao_encontrada(),
            }
        })
        .mount(servidor)
        .await;
    let revisao = Arc::clone(automatico);
    requisicao("PATCH", path_regex(r"^/pix/v2/rec/[A-Za-z0-9]+$"))
        .respond_with(move |request: &Request| revisar(&mut revisao.lock().unwrap(), request))
        .mount(servidor)
        .await;
}

fn id(request: &Request) -> &str {
    request.url.path().rsplit('/').next().unwrap_or_default()
}

fn achar<'a>(automatico: &'a mut Automatico, id: &str) -> Option<&'a mut Value> {
    automatico.recs.iter_mut().find(|rec| rec["idRec"] == id)
}

fn nao_encontrada() -> ResponseTemplate {
    problema(
        404,
        "Recorrência não encontrada",
        "Não há recorrência com este idRec.",
    )
}

/// A recurrence created now, waiting for the payer's approval.
fn criar(automatico: &mut Automatico, request: &Request) -> ResponseTemplate {
    let corpo: Value = serde_json::from_slice(&request.body).unwrap();
    let retentativas = corpo["politicaRetentativa"] == "PERMITE_3R_7D";
    let id = automatico.id_rec(retentativas);
    let agora = automatico.agora();
    let mut rec = json!({
        "idRec": id,
        "vinculo": corpo["vinculo"],
        "calendario": corpo["calendario"],
        "recebedor": recebedor(),
        "status": "CRIADA",
        "politicaRetentativa": corpo["politicaRetentativa"],
        "atualizacao": [{"status": "CRIADA", "data": agora}],
    });
    if let Some(valor) = corpo.get("valor") {
        rec["valor"] = valor.clone();
    }
    if let Some(txid) = corpo.pointer("/ativacao/dadosJornada/txid") {
        rec["ativacao"] = json!({"tipoJornada": "JORNADA_3", "dadosJornada": {"txid": txid}});
    }
    automatico.recs.push(rec.clone());
    if corpo["vinculo"]["contrato"] == RESPOSTA_PERDIDA {
        return ResponseTemplate::new(504);
    }
    ResponseTemplate::new(201).set_body_json(rec)
}

/// The recurrences created in the period, with the filters, in pages from
/// 0.
fn listar(automatico: &Automatico, request: &Request) -> ResponseTemplate {
    let parametros = parametros(request);
    let Some(periodo) = periodo(&parametros) else {
        return periodo_invalido();
    };
    let recs: Vec<&Value> = automatico
        .recs
        .iter()
        .filter(|rec| {
            let devedor = &rec["vinculo"]["devedor"];
            no_periodo(&rec["atualizacao"][0]["data"], periodo)
                && ["cpf", "cnpj"].iter().all(|campo| {
                    parametros
                        .get(*campo)
                        .is_none_or(|documento| devedor[*campo] == documento.as_str())
                })
                && parametros
                    .get("status")
                    .is_none_or(|status| rec["status"] == status.as_str())
                && parametros
                    .get("locationPresente")
                    .is_none_or(|presente| (presente == "true") == rec.get("loc").is_some())
        })
        .collect();
    pagina(&recs, &parametros, "recs")
}

/// A change of a recurrence, or its cancellation, made at once.
fn revisar(automatico: &mut Automatico, request: &Request) -> ResponseTemplate {
    let corpo: Value = serde_json::from_slice(&request.body).unwrap();
    let cancelamento = corpo["status"] == "CANCELADA";
    let agora = cancelamento.then(|| automatico.agora());
    let Some(rec) = achar(automatico, id(request)) else {
        return nao_encontrada();
    };
    if ["REJEITADA", "EXPIRADA", "CANCELADA"]
        .iter()
        .any(|encerrada| rec["status"] == *encerrada)
    {
        return problema(
            400,
            "Recorrência encerrada",
            "Uma recorrência rejeitada, expirada ou cancelada não pode ser alterada.",
        );
    }
    if let Some(agora) = agora {
        rec["status"] = json!("CANCELADA");
        rec["encerramento"] = json!({"cancelamento": {
            "solicitante": "USUARIO_RECEBEDOR",
            "codigo": "SLCR",
            "descricao": "Cancelamento solicitado pelo usuário recebedor",
        }});
        rec["atualizacao"]
            .as_array_mut()
            .unwrap()
            .push(json!({"status": "CANCELADA", "data": agora}));
        return ResponseTemplate::new(200).set_body_json(&*rec);
    }
    if let Some(nome) = corpo.pointer("/vinculo/devedor/nome") {
        rec["vinculo"]["devedor"]["nome"] = nome.clone();
    }
    if let Some(inicio) = corpo.pointer("/calendario/dataInicial") {
        rec["calendario"]["dataInicial"] = inicio.clone();
    }
    if let Some(txid) = corpo.pointer("/ativacao/dadosJornada/txid") {
        rec["ativacao"] = json!({"tipoJornada": "JORNADA_3", "dadosJornada": {"txid": txid}});
    }
    ResponseTemplate::new(200).set_body_json(&*rec)
}
