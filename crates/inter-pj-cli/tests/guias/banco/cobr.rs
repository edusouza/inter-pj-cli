//! The recurring charges of Pix Automático (`/pix/v2/cobr`): one per cycle
//! of an approved recurrence, which the payer's bank debits on the due
//! date. The account already has September's charge of Fulano de Tal's
//! basic plan, which expired after the debit and the 3 new attempts were
//! refused, his account being blocked. A charge created is scheduled by the
//! payer's bank by the time anyone looks at it, for the due date or, when it
//! is not a working day, the next one; a charge not settled yet can be
//! cancelled.

use std::sync::{Arc, Mutex};

use chrono::{Datelike, Days, NaiveDate, Weekday};
use serde_json::{Value, json};
use wiremock::matchers::{path, path_regex};
use wiremock::{MockServer, Request, ResponseTemplate};

use super::automatico::{Automatico, depois};
use super::cobrancas_pix::{no_periodo, pagina, periodo, periodo_invalido};
use super::{parametros, problema, requisicao};

/// The holidays of the payer's city until the end of the year.
const FERIADOS: [&str; 5] = [
    "2026-10-12",
    "2026-11-02",
    "2026-11-15",
    "2026-11-20",
    "2026-12-25",
];

/// The ends of the ids of the payment orders the payer's bank schedules
/// today, in order.
const SUFIXOS_E2E: [&str; 3] = ["Rw5Hn8Tc1Kz", "Gm2Vx7Ls4Qd", "Pb9Jf3Wy6Nc"];

/// The company's account, as the charges show it.
fn recebedor(conta: &Value) -> Value {
    json!({
        "cnpj": "11444777000161",
        "nome": "Empresa Exemplo Ltda",
        "conta": conta["conta"],
        "tipoConta": conta["tipoConta"],
        "agencia": conta["agencia"],
    })
}

/// An attempt to settle a charge, refused because the payer's account was
/// blocked.
fn recusada(liquidacao: &str, tipo: &str, e2e: &str, pedida: &str) -> Value {
    json!({
        "dataLiquidacao": liquidacao,
        "tipo": tipo,
        "endToEndId": e2e,
        "status": "REJEITADA",
        "rejeicao": {"codigo": "AC06", "descricao": "Conta transacional do usuário pagador bloqueada"},
        "atualizacao": [
            {"status": "SOLICITADA", "data": pedida},
            {"status": "REJEITADA", "data": format!("{liquidacao}T09:00:00.000Z")},
        ],
    })
}

/// The charges before the guides.
pub(super) fn iniciais() -> Vec<Value> {
    let conta = json!({"conta": "1234567", "tipoConta": "CORRENTE", "agencia": "0001"});
    vec![json!({
        "idRec": "RR1234567820260901k7Tq2Wm9Zp4",
        "txid": "fulano0042setembro2026planobasico",
        "infoAdicional": "Plano básico de setembro",
        "calendario": {"criacao": "2026-09-03T12:00:00.000Z", "dataDeVencimento": "2026-09-10"},
        "valor": {"original": "89.90"},
        "status": "EXPIRADA",
        "politicaRetentativa": "PERMITE_3R_7D",
        "ajusteDiaUtil": true,
        "recebedor": recebedor(&conta),
        "tentativas": [
            recusada("2026-09-10", "AGND", "E87654321202609100300Ac6Bq2Lx9Mz", "2026-09-03T12:00:05.000Z"),
            recusada("2026-09-11", "NTAG", "E87654321202609110300Hd4Wn7Ts2Ky", "2026-09-10T15:20:41.000Z"),
            recusada("2026-09-14", "NTAG", "E87654321202609140300Mv8Rc3Pz6Lf", "2026-09-11T14:02:13.000Z"),
            recusada("2026-09-16", "NTAG", "E87654321202609160300Qx1Gj5Nb8Wt", "2026-09-14T16:47:55.000Z"),
        ],
        "atualizacao": [
            {"status": "CRIADA", "data": "2026-09-03T12:00:00.000Z"},
            {"status": "ATIVA", "data": "2026-09-03T12:00:05.000Z"},
            {"status": "EXPIRADA", "data": "2026-09-18T03:00:00.000Z"},
        ],
    })]
}

pub(super) async fn montar(servidor: &MockServer, automatico: &Arc<Mutex<Automatico>>) {
    let criacao = Arc::clone(automatico);
    requisicao("PUT", path_regex(r"^/pix/v2/cobr/[A-Za-z0-9]+$"))
        .respond_with(move |request: &Request| criar(&mut criacao.lock().unwrap(), request))
        .mount(servidor)
        .await;
    let consulta = Arc::clone(automatico);
    requisicao("GET", path_regex(r"^/pix/v2/cobr/[A-Za-z0-9]+$"))
        .respond_with(move |request: &Request| {
            let mut automatico = consulta.lock().unwrap();
            agendar(&mut automatico);
            match achar(&mut automatico, txid(request)) {
                Some(cobr) => ResponseTemplate::new(200).set_body_json(&*cobr),
                None => nao_encontrada(),
            }
        })
        .mount(servidor)
        .await;
    let lista = Arc::clone(automatico);
    requisicao("GET", path("/pix/v2/cobr"))
        .respond_with(move |request: &Request| {
            let mut automatico = lista.lock().unwrap();
            agendar(&mut automatico);
            listar(&automatico, request)
        })
        .mount(servidor)
        .await;
    let cancelamento = Arc::clone(automatico);
    requisicao("PATCH", path_regex(r"^/pix/v2/cobr/[A-Za-z0-9]+$"))
        .respond_with(move |request: &Request| cancelar(&mut cancelamento.lock().unwrap(), request))
        .mount(servidor)
        .await;
}

fn txid(request: &Request) -> &str {
    request.url.path().rsplit('/').next().unwrap_or_default()
}

fn achar<'a>(automatico: &'a mut Automatico, txid: &str) -> Option<&'a mut Value> {
    automatico
        .cobrs
        .iter_mut()
        .find(|cobr| cobr["txid"] == txid)
}

fn nao_encontrada() -> ResponseTemplate {
    problema(
        404,
        "Cobrança não encontrada",
        "Não há cobrança recorrente com este txid.",
    )
}

/// The day the debit of `vencimento` is settled: that day, or the next
/// working day when `ajuste` asks for it.
fn liquidacao(vencimento: &str, ajuste: bool) -> String {
    let mut dia = NaiveDate::parse_from_str(vencimento, "%Y-%m-%d").unwrap();
    let util = |dia: NaiveDate| {
        !matches!(dia.weekday(), Weekday::Sat | Weekday::Sun)
            && !FERIADOS.contains(&dia.to_string().as_str())
    };
    while ajuste && !util(dia) {
        dia = dia + Days::new(1);
    }
    dia.to_string()
}

/// A charge of an approved recurrence, created now, waiting for the payer's
/// bank.
fn criar(automatico: &mut Automatico, request: &Request) -> ResponseTemplate {
    let txid = txid(request).to_owned();
    if achar(automatico, &txid).is_some() {
        return problema(
            400,
            "txid já utilizado",
            "Já existe uma cobrança recorrente com este txid.",
        );
    }
    let corpo: Value = serde_json::from_slice(&request.body).unwrap();
    let Some(rec) = automatico
        .recs
        .iter()
        .find(|rec| rec["idRec"] == corpo["idRec"])
        .cloned()
    else {
        return problema(
            404,
            "Recorrência não encontrada",
            "Não há recorrência com este idRec.",
        );
    };
    if rec["status"] != "APROVADA" {
        return problema(
            400,
            "Recorrência não aprovada",
            "Só uma recorrência aprovada pelo pagador aceita cobranças.",
        );
    }
    let agora = automatico.agora();
    let mut cobr = json!({
        "idRec": rec["idRec"],
        "txid": txid,
        "calendario": {"criacao": agora, "dataDeVencimento": corpo["calendario"]["dataDeVencimento"]},
        "valor": corpo["valor"],
        "status": "CRIADA",
        "politicaRetentativa": rec["politicaRetentativa"],
        "ajusteDiaUtil": corpo["ajusteDiaUtil"],
        "recebedor": recebedor(&corpo["recebedor"]),
        "tentativas": [],
        "atualizacao": [{"status": "CRIADA", "data": agora}],
    });
    for campo in ["infoAdicional", "devedor"] {
        if let Some(valor) = corpo.get(campo) {
            cobr[campo] = valor.clone();
        }
    }
    automatico.cobrs.push(cobr.clone());
    ResponseTemplate::new(201).set_body_json(cobr)
}

/// The charges created, scheduled by the payer's bank by the time anyone
/// looks, 5 seconds after they were created.
pub(super) fn agendar(automatico: &mut Automatico) {
    for cobr in &mut automatico.cobrs {
        if cobr["status"] != "CRIADA" {
            continue;
        }
        let criacao = cobr["calendario"]["criacao"].as_str().unwrap().to_owned();
        let agendada = depois(&criacao, 5);
        let dia = liquidacao(
            cobr["calendario"]["dataDeVencimento"].as_str().unwrap(),
            cobr["ajusteDiaUtil"] != false,
        );
        let e2e = format!(
            "E87654321{}0300{}",
            dia.replace('-', ""),
            SUFIXOS_E2E[automatico.agendadas % SUFIXOS_E2E.len()]
        );
        automatico.agendadas += 1;
        cobr["tentativas"] = json!([{
            "dataLiquidacao": dia,
            "tipo": "AGND",
            "endToEndId": e2e,
            "status": "AGENDADA",
            "atualizacao": [
                {"status": "SOLICITADA", "data": criacao},
                {"status": "AGENDADA", "data": agendada},
            ],
        }]);
        cobr["status"] = json!("ATIVA");
        cobr["atualizacao"]
            .as_array_mut()
            .unwrap()
            .push(json!({"status": "ATIVA", "data": agendada}));
    }
}

/// The charges created in the period, with the filters, in pages from 0.
fn listar(automatico: &Automatico, request: &Request) -> ResponseTemplate {
    let parametros = parametros(request);
    let Some(periodo) = periodo(&parametros) else {
        return periodo_invalido();
    };
    let devedor = |cobr: &Value| {
        automatico
            .recs
            .iter()
            .find(|rec| rec["idRec"] == cobr["idRec"])
            .map(|rec| rec["vinculo"]["devedor"].clone())
            .unwrap_or_default()
    };
    let cobrs: Vec<&Value> = automatico
        .cobrs
        .iter()
        .filter(|cobr| {
            no_periodo(&cobr["calendario"]["criacao"], periodo)
                && parametros
                    .get("idRec")
                    .is_none_or(|id| cobr["idRec"] == id.as_str())
                && ["cpf", "cnpj"].iter().all(|campo| {
                    parametros
                        .get(*campo)
                        .is_none_or(|documento| devedor(cobr)[*campo] == documento.as_str())
                })
                && parametros
                    .get("status")
                    .is_none_or(|status| cobr["status"] == status.as_str())
        })
        .collect();
    pagina(&cobrs, &parametros, "cobsr")
}

/// The cancellation of a charge not settled yet, with its scheduled debit.
fn cancelar(automatico: &mut Automatico, request: &Request) -> ResponseTemplate {
    agendar(automatico);
    let agora = automatico.agora();
    let Some(cobr) = achar(automatico, txid(request)) else {
        return nao_encontrada();
    };
    if !["CRIADA", "ATIVA"]
        .iter()
        .any(|status| cobr["status"] == *status)
    {
        return problema(
            400,
            "Cobrança encerrada",
            "Uma cobrança paga, expirada, rejeitada ou cancelada não pode ser cancelada.",
        );
    }
    for tentativa in cobr["tentativas"].as_array_mut().unwrap() {
        if tentativa["status"] == "AGENDADA" || tentativa["status"] == "SOLICITADA" {
            tentativa["status"] = json!("CANCELADA");
            tentativa["atualizacao"]
                .as_array_mut()
                .unwrap()
                .push(json!({"status": "CANCELADA", "data": agora}));
        }
    }
    cobr["status"] = json!("CANCELADA");
    cobr["encerramento"] = json!({"cancelamento": {
        "solicitante": "USUARIO_RECEBEDOR",
        "codigo": "SLCR",
        "descricao": "Cancelamento solicitado pelo usuário recebedor",
    }});
    cobr["atualizacao"]
        .as_array_mut()
        .unwrap()
        .push(json!({"status": "CANCELADA", "data": agora}));
    ResponseTemplate::new(200).set_body_json(&*cobr)
}
