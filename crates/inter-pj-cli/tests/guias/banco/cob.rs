//! The immediate charges of the Pix API (`/pix/v2/cob`): QR Codes to pay
//! now, until they expire. The account already has two, from the orders of
//! the statement: the one Cliente Exemplo paid on 02/09 with the Pix of the
//! statement, and one removed. A charge created is active, with a location
//! in `inter.example` and its "copia e cola", a dynamic BR Code with a
//! valid CRC16; the answer to the creation of one gets lost.

use std::sync::{Arc, Mutex};

use chrono::{DateTime, TimeDelta};
use serde_json::{Value, json};
use wiremock::matchers::{path, path_regex};
use wiremock::{MockServer, Request, ResponseTemplate};

use super::{parametros, problema, requisicao};
use crate::sessao::HOJE;

/// The Pix key of the account.
const CHAVE: &str = "pix@empresa.example";

/// The charge whose creation is made but whose answer gets lost.
const RESPOSTA_PERDIDA: &str = "pedido1062empresaexemplo2026";

struct Estado {
    cobs: Vec<Value>,
}

impl Estado {
    /// The charges of the orders before the guides.
    fn novo() -> Self {
        let mut removida = cob(
            "pedido1051empresaexemplo2026",
            7000,
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
            7001,
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
        Self {
            cobs: vec![removida, paga],
        }
    }

    fn achar(&mut self, txid: &str) -> Option<&mut Value> {
        self.cobs.iter_mut().find(|cob| cob["txid"] == txid)
    }
}

/// A charge as the API shows it, active.
fn cob(
    txid: &str,
    loc: u64,
    criacao: &str,
    expiracao: u64,
    devedor: &Value,
    valor: &str,
    solicitacao: &str,
) -> Value {
    let location = format!("qrcodepix.inter.example/qr/v2/cob/{txid}");
    json!({
        "calendario": {"criacao": criacao, "expiracao": expiracao},
        "txid": txid,
        "revisao": 0,
        "loc": {"id": loc, "location": location, "tipoCob": "cob", "criacao": criacao},
        "location": location,
        "status": "ATIVA",
        "devedor": devedor,
        "valor": {"original": valor, "modalidadeAlteracao": 0},
        "chave": CHAVE,
        "solicitacaoPagador": solicitacao,
        "infoAdicionais": [],
        "pixCopiaECola": copia_e_cola(&location),
        "pix": [],
    })
}

/// The dynamic BR Code of a location, with its CRC16.
pub(super) fn copia_e_cola(location: &str) -> String {
    let campo = |id: &str, valor: &str| format!("{id}{:02}{valor}", valor.len());
    let conta = campo("00", "br.gov.bcb.pix") + &campo("25", location);
    let corpo = [
        campo("00", "01"),
        campo("01", "12"),
        campo("26", &conta),
        campo("52", "0000"),
        campo("53", "986"),
        campo("58", "BR"),
        campo("59", "EMPRESA EXEMPLO LTDA"),
        campo("60", "BELO HORIZONTE"),
        campo("62", &campo("05", "***")),
        "6304".to_owned(),
    ]
    .concat();
    let crc = crc16(corpo.as_bytes());
    format!("{corpo}{crc:04X}")
}

/// CRC16-CCITT (polynomial 0x1021, initial value 0xFFFF), as the BR Code.
fn crc16(bytes: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for byte in bytes {
        crc ^= u16::from(*byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 == 0 {
                crc << 1
            } else {
                (crc << 1) ^ 0x1021
            };
        }
    }
    crc
}

pub(super) async fn montar(servidor: &MockServer) {
    let estado = Arc::new(Mutex::new(Estado::novo()));
    let criacao = Arc::clone(&estado);
    requisicao("PUT", path_regex(r"^/pix/v2/cob/[^/]+$"))
        .respond_with(move |request: &Request| criar(&mut criacao.lock().unwrap(), request))
        .mount(servidor)
        .await;
    let revisao = Arc::clone(&estado);
    requisicao("PATCH", path_regex(r"^/pix/v2/cob/[^/]+$"))
        .respond_with(move |request: &Request| revisar(&mut revisao.lock().unwrap(), request))
        .mount(servidor)
        .await;
    let consulta = Arc::clone(&estado);
    requisicao("GET", path_regex(r"^/pix/v2/cob/[^/]+$"))
        .respond_with(move |request: &Request| {
            let txid = request.url.path().rsplit('/').next().unwrap_or_default();
            match consulta.lock().unwrap().achar(txid) {
                Some(cob) => ResponseTemplate::new(200).set_body_json(&*cob),
                None => nao_encontrada(),
            }
        })
        .mount(servidor)
        .await;
    requisicao("GET", path("/pix/v2/cob"))
        .respond_with(move |request: &Request| listar(&estado.lock().unwrap().cobs, request))
        .mount(servidor)
        .await;
}

fn nao_encontrada() -> ResponseTemplate {
    problema(
        404,
        "Cobrança não encontrada",
        "Não há cobrança imediata com este txid.",
    )
}

/// A charge created with its txid, at 10:05:12 of today in Brasília and
/// then 7 minutes apart.
fn criar(estado: &mut Estado, request: &Request) -> ResponseTemplate {
    let txid = request.url.path().rsplit('/').next().unwrap_or_default();
    if estado.achar(txid).is_some() {
        return problema(
            400,
            "txid já utilizado",
            "Já existe uma cobrança com este txid.",
        );
    }
    let corpo: Value = serde_json::from_slice(&request.body).unwrap();
    let novas = estado.cobs.len() - 2;
    let criacao = DateTime::parse_from_rfc3339(&format!("{HOJE}T13:05:12.000Z")).unwrap()
        + TimeDelta::minutes(7 * i64::try_from(novas).unwrap());
    let mut criada = cob(
        txid,
        7002 + u64::try_from(novas).unwrap(),
        &criacao.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
        corpo["calendario"]["expiracao"].as_u64().unwrap_or(86_400),
        &corpo["devedor"],
        corpo["valor"]["original"].as_str().unwrap_or_default(),
        corpo["solicitacaoPagador"].as_str().unwrap_or_default(),
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
    estado.cobs.push(criada.clone());
    if txid == RESPOSTA_PERDIDA {
        return ResponseTemplate::new(504);
    }
    ResponseTemplate::new(201).set_body_json(criada)
}

/// A change of an active charge: what came replaces what it had, and the
/// revision goes up; a paid or removed charge cannot change.
fn revisar(estado: &mut Estado, request: &Request) -> ResponseTemplate {
    let txid = request.url.path().rsplit('/').next().unwrap_or_default();
    let corpo: Value = serde_json::from_slice(&request.body).unwrap();
    let Some(cob) = estado.achar(txid) else {
        return nao_encontrada();
    };
    if cob["status"] != "ATIVA" {
        return problema(
            400,
            "Cobrança não pode ser alterada",
            "Só uma cobrança ativa pode ser alterada.",
        );
    }
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

/// The charges created in the period, with the filters, in pages from 0:
/// the listing of the immediate charges and of those with a due date.
pub(super) fn listar<'a>(
    cobs: impl IntoIterator<Item = &'a Value>,
    request: &Request,
) -> ResponseTemplate {
    let parametros = parametros(request);
    let momento = |campo: &str| {
        DateTime::parse_from_rfc3339(parametros.get(campo).map_or("", String::as_str))
    };
    let (Ok(inicio), Ok(fim)) = (momento("inicio"), momento("fim")) else {
        return problema(
            400,
            "Período inválido",
            "inicio e fim são obrigatórios, em RFC 3339.",
        );
    };
    let cobs: Vec<&Value> = cobs
        .into_iter()
        .filter(|cob| {
            let criacao =
                DateTime::parse_from_rfc3339(cob["calendario"]["criacao"].as_str().unwrap())
                    .unwrap();
            let com_location = cob.get("loc").is_some();
            inicio <= criacao
                && criacao <= fim
                && ["cpf", "cnpj"].iter().all(|campo| {
                    parametros
                        .get(*campo)
                        .is_none_or(|documento| cob["devedor"][*campo] == documento.as_str())
                })
                && parametros
                    .get("locationPresente")
                    .is_none_or(|presente| (presente == "true") == com_location)
                && parametros
                    .get("status")
                    .is_none_or(|status| cob["status"] == status.as_str())
        })
        .collect();
    let pagina: usize = parametros
        .get("paginacao.paginaAtual")
        .map_or(0, |pagina| pagina.parse().unwrap());
    let itens: usize = parametros
        .get("paginacao.itensPorPagina")
        .map_or(100, |itens| itens.parse().unwrap());
    let nesta: Vec<&&Value> = cobs.iter().skip(pagina * itens).take(itens).collect();
    ResponseTemplate::new(200).set_body_json(json!({
        "parametros": {
            "inicio": parametros["inicio"],
            "fim": parametros["fim"],
            "paginacao": {
                "paginaAtual": pagina,
                "itensPorPagina": itens,
                "quantidadeDePaginas": cobs.len().div_ceil(itens),
                "quantidadeTotalDeItens": cobs.len(),
            },
        },
        "cobs": nesta,
    }))
}
