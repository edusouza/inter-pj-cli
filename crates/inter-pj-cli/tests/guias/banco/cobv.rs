//! The Pix charges with a due date (`/pix/v2/cobv`), the boleto of the Pix.
//! The account already has the monthly fees of Beltrana de Tal: September's,
//! removed, and October's, active. A charge created is active, with the
//! amount and the fine, interest, rebate and discount it was sent with, the
//! location it asks for or one of its own, in `inter.example`, and its
//! "copia e cola", which a change keeps.

use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use wiremock::matchers::{path, path_regex};
use wiremock::{MockServer, Request, ResponseTemplate};

use super::cobrancas_pix::{Cobrancas, copia_e_cola, listar, location_invalida};
use super::{parametros, problema, requisicao};

/// The Pix key of the account.
const CHAVE: &str = "pix@empresa.example";

/// Days after the due date in which a charge can be paid, when it does not
/// say.
const VALIDADE_PADRAO: u64 = 30;

/// The charges of the monthly fees before the guides.
pub(super) fn iniciais() -> Vec<Value> {
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
        "2026-08-25T12:00:00.000Z",
        "2026-09-10",
        &beltrana,
        "Mensalidade de setembro",
    );
    setembro["status"] = json!("REMOVIDA_PELO_USUARIO_RECEBEDOR");
    setembro["revisao"] = json!(1);
    let outubro = cobv(
        "mensalidade202610beltranadetal",
        "2026-09-15T12:00:00.000Z",
        "2026-10-10",
        &beltrana,
        "Mensalidade de outubro",
    );
    vec![setembro, outubro]
}

/// A monthly fee of R$ 450,00 as the API shows it, active, with a fine of
/// R$ 9,00, interest of R$ 0,15 a day and a location of its own, which
/// [`Cobrancas`] numbers.
fn cobv(txid: &str, criacao: &str, vencimento: &str, devedor: &Value, solicitacao: &str) -> Value {
    let location = format!("qrcodepix.inter.example/qr/v2/cobv/{txid}");
    let loc = json!({"id": 0, "location": location, "tipoCob": "cobv", "criacao": criacao});
    com_location(
        json!({
            "calendario": {
                "criacao": criacao,
                "dataDeVencimento": vencimento,
                "validadeAposVencimento": VALIDADE_PADRAO,
            },
            "txid": txid,
            "revisao": 0,
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
            "pix": [],
        }),
        loc,
    )
}

/// `cobv` at the location `loc`, with its QR Code.
fn com_location(mut cobv: Value, loc: Value) -> Value {
    let location = loc["location"].as_str().unwrap().to_owned();
    cobv["pixCopiaECola"] = json!(copia_e_cola(&location));
    cobv["loc"] = loc;
    cobv
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

pub(super) async fn montar(servidor: &MockServer, cobrancas: &Arc<Mutex<Cobrancas>>) {
    let criacao = Arc::clone(cobrancas);
    requisicao("PUT", path_regex(r"^/pix/v2/cobv/[^/]+$"))
        .respond_with(move |request: &Request| criar(&mut criacao.lock().unwrap(), request))
        .mount(servidor)
        .await;
    let revisao = Arc::clone(cobrancas);
    requisicao("PATCH", path_regex(r"^/pix/v2/cobv/[^/]+$"))
        .respond_with(move |request: &Request| revisar(&mut revisao.lock().unwrap(), request))
        .mount(servidor)
        .await;
    let consulta = Arc::clone(cobrancas);
    requisicao("GET", path_regex(r"^/pix/v2/cobv/[^/]+$"))
        .respond_with(move |request: &Request| {
            match achar(&mut consulta.lock().unwrap(), txid(request)) {
                Some(cobv) => ResponseTemplate::new(200).set_body_json(&*cobv),
                None => nao_encontrada(),
            }
        })
        .mount(servidor)
        .await;
    let lista = Arc::clone(cobrancas);
    requisicao("GET", path("/pix/v2/cobv"))
        .respond_with(move |request: &Request| {
            let cobrancas = lista.lock().unwrap();
            // Those of a batch, when it asks for them.
            let lote = parametros(request)
                .get("loteCobVId")
                .map(|lote| cobrancas.do_lote(lote.parse().unwrap_or_default()));
            let cobvs: Vec<Value> = cobrancas
                .cobvs
                .iter()
                .filter(|cobv| {
                    lote.as_ref()
                        .is_none_or(|txids| txids.iter().any(|txid| cobv["txid"] == *txid))
                })
                .cloned()
                .collect();
            listar(&cobvs, request)
        })
        .mount(servidor)
        .await;
}

fn txid(request: &Request) -> &str {
    request.url.path().rsplit('/').next().unwrap_or_default()
}

fn achar<'a>(cobrancas: &'a mut Cobrancas, txid: &str) -> Option<&'a mut Value> {
    cobrancas.cobvs.iter_mut().find(|cobv| cobv["txid"] == txid)
}

fn nao_encontrada() -> ResponseTemplate {
    problema(
        404,
        "Cobrança não encontrada",
        "Não há cobrança com vencimento com este txid.",
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
    match nova(cobrancas, txid, &criacao, &corpo) {
        Some(criada) => ResponseTemplate::new(201).set_body_json(criada),
        None => location_invalida(),
    }
}

/// The charge `corpo` created at `criacao`, alone or in a batch; `None`
/// when the location it asks for is not free.
pub(super) fn nova(
    cobrancas: &mut Cobrancas,
    txid: &str,
    criacao: &str,
    corpo: &Value,
) -> Option<Value> {
    let loc = cobrancas.location("cobv", txid, criacao, corpo.get("loc"))?;
    let mut criada = com_location(
        cobv(
            txid,
            criacao,
            corpo["calendario"]["dataDeVencimento"].as_str().unwrap(),
            &corpo["devedor"],
            "",
        ),
        loc,
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
    cobrancas.cobvs.push(criada.clone());
    Some(criada)
}

/// Why a charge was not changed.
pub(super) enum Recusa {
    NaoEncontrada,
    Encerrada,
    LocationInvalida,
}

fn revisar(cobrancas: &mut Cobrancas, request: &Request) -> ResponseTemplate {
    let corpo: Value = serde_json::from_slice(&request.body).unwrap();
    match alterar(cobrancas, txid(request), &corpo) {
        Ok(cobv) => ResponseTemplate::new(200).set_body_json(cobv),
        Err(Recusa::NaoEncontrada) => nao_encontrada(),
        Err(Recusa::Encerrada) => problema(
            400,
            "Cobrança não pode ser alterada",
            "Só uma cobrança ativa pode ser alterada.",
        ),
        Err(Recusa::LocationInvalida) => location_invalida(),
    }
}

/// A change of an active charge, alone or in a batch: what came replaces
/// what it had, each charge of the amount whole, a new location frees the
/// one it had, and the revision goes up; a paid or removed charge cannot
/// change.
pub(super) fn alterar(
    cobrancas: &mut Cobrancas,
    txid: &str,
    corpo: &Value,
) -> Result<Value, Recusa> {
    let cobv = achar(cobrancas, txid).ok_or(Recusa::NaoEncontrada)?;
    if cobv["status"] != "ATIVA" {
        return Err(Recusa::Encerrada);
    }
    if corpo.get("loc").is_some() {
        let criacao = cobv["calendario"]["criacao"].as_str().unwrap().to_owned();
        let loc = cobrancas
            .location("cobv", txid, &criacao, corpo.get("loc"))
            .ok_or(Recusa::LocationInvalida)?;
        let cobv = achar(cobrancas, txid).unwrap();
        let antiga = cobv.as_object_mut().unwrap().remove("loc");
        *cobv = com_location(cobv.take(), loc);
        if let Some(antiga) = antiga {
            cobrancas.liberar(antiga);
        }
    }
    let cobv = achar(cobrancas, txid).unwrap();
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
    Ok(cobv.clone())
}
