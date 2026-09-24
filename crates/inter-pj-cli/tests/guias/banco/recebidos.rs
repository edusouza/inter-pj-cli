//! The Pix the account received (`/pix/v2/pix`), which are those of the
//! statement, and their refunds. A refund is in processing when asked for,
//! and refunded by the next time someone looks.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeDelta};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use wiremock::matchers::{path, path_regex};
use wiremock::{MockServer, Request, ResponseTemplate};

use super::{conta, parametros, problema, requisicao};
use crate::sessao::HOJE;

/// The key of the account that received them.
const CHAVE: &str = "pix@empresa.example";

/// The txids of the Pix that paid a charge.
const TXIDS: [(&str, &str); 1] = [(
    "E12345678202609021215Po0iU9yT8rE",
    "pedido1053empresaexemplo2026",
)];

/// The rtrIds of the refunds, in order.
const RTR_IDS: [&str; 4] = [
    "D12345678202609241720h4Jk6Lm8NpQ",
    "D12345678202609241725r2St4Uv6WxY",
    "D12345678202609241730z8Ab0Cd2EfG",
    "D12345678202609241735j6Kl8Mn0OpR",
];

/// The refunds asked for, by the endToEndId of the Pix.
#[derive(Default)]
struct Estado {
    devolucoes: HashMap<String, Vec<Value>>,
}

pub(super) async fn montar(servidor: &MockServer) {
    let estado = Arc::new(Mutex::new(Estado::default()));
    let lista = Arc::clone(&estado);
    requisicao("GET", path("/pix/v2/pix"))
        .respond_with(move |request: &Request| listar(&lista.lock().unwrap(), request))
        .mount(servidor)
        .await;
    let consulta = Arc::clone(&estado);
    requisicao("GET", path_regex(r"^/pix/v2/pix/[^/]+$"))
        .respond_with(move |request: &Request| {
            let e2e = request.url.path().rsplit('/').next().unwrap_or_default();
            match recebido(&consulta.lock().unwrap(), e2e) {
                Some(pix) => ResponseTemplate::new(200).set_body_json(pix),
                None => nao_encontrado(),
            }
        })
        .mount(servidor)
        .await;
    let pedidos = Arc::clone(&estado);
    requisicao("PUT", path_regex(r"^/pix/v2/pix/[^/]+/devolucao/[^/]+$"))
        .respond_with(move |request: &Request| devolver(&mut pedidos.lock().unwrap(), request))
        .mount(servidor)
        .await;
    requisicao("GET", path_regex(r"^/pix/v2/pix/[^/]+/devolucao/[^/]+$"))
        .respond_with(move |request: &Request| {
            let (e2e, id) = e2e_e_id(request);
            match devolucao(&estado.lock().unwrap(), &e2e, &id) {
                Some(devolucao) => ResponseTemplate::new(200).set_body_json(devolucao),
                None => nao_encontrado(),
            }
        })
        .mount(servidor)
        .await;
}

fn nao_encontrado() -> ResponseTemplate {
    problema(
        404,
        "Não encontrado",
        "Não há Pix recebido ou devolução com este identificador.",
    )
}

/// The Pix of the statement that came in, as the Pix API shows them, with
/// the payer's document (for the filter) and the refunds.
fn todos(estado: &Estado) -> Vec<(Value, Value)> {
    conta::transacoes()
        .into_iter()
        .filter(|t| t["tipoTransacao"] == "PIX" && t["tipoOperacao"] == "C")
        .map(|t| {
            let detalhes = &t["detalhes"];
            let e2e = detalhes["endToEndId"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            // The statement shows the time in Brasília; the API, in UTC.
            let horario = NaiveDateTime::parse_from_str(
                t["dataInclusao"].as_str().unwrap_or_default(),
                "%Y-%m-%d %H:%M:%S",
            )
            .unwrap()
                + TimeDelta::hours(3);
            let mut pix = json!({
                "endToEndId": e2e,
                "valor": t["valor"],
                "chave": CHAVE,
                "horario": horario.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
                "devolucoes": estado.devolucoes.get(&e2e).cloned().unwrap_or_default(),
            });
            if let Some(mensagem) = detalhes["descricaoPix"].as_str() {
                pix["infoPagador"] = json!(mensagem);
            }
            if let Some((_, txid)) = TXIDS.iter().find(|(de, _)| *de == e2e) {
                pix["txid"] = json!(txid);
            }
            (pix, detalhes["cpfCnpjPagador"].clone())
        })
        .collect()
}

fn recebido(estado: &Estado, e2e: &str) -> Option<Value> {
    todos(estado)
        .into_iter()
        .map(|(pix, _)| pix)
        .find(|pix| pix["endToEndId"] == e2e)
}

/// The Pix of the period, with the filters, in pages from 0.
fn listar(estado: &Estado, request: &Request) -> ResponseTemplate {
    let parametros = parametros(request);
    let momento = |campo: &str| {
        DateTime::<FixedOffset>::parse_from_rfc3339(
            parametros.get(campo).map_or("", String::as_str),
        )
    };
    let (Ok(inicio), Ok(fim)) = (momento("inicio"), momento("fim")) else {
        return problema(
            400,
            "Período inválido",
            "inicio e fim são obrigatórios, em RFC 3339.",
        );
    };
    let sim = |campo: &str| parametros.get(campo).map(|valor| valor == "true");
    let pix: Vec<Value> = todos(estado)
        .into_iter()
        .filter(|(pix, documento)| {
            let horario =
                DateTime::parse_from_rfc3339(pix["horario"].as_str().unwrap_or_default()).unwrap();
            let devolvido = pix["devolucoes"].as_array().is_some_and(|d| !d.is_empty());
            inicio <= horario
                && horario <= fim
                && parametros
                    .get("txid")
                    .is_none_or(|txid| pix["txid"] == txid.as_str())
                && sim("txIdPresente").is_none_or(|presente| presente == pix.get("txid").is_some())
                && sim("devolucaoPresente").is_none_or(|presente| presente == devolvido)
                && ["cpf", "cnpj"]
                    .iter()
                    .filter_map(|campo| parametros.get(*campo))
                    .all(|filtro| documento == filtro.as_str())
        })
        .map(|(pix, _)| pix)
        .collect();
    let pagina: usize = parametros
        .get("paginacao.paginaAtual")
        .map_or(0, |p| p.parse().unwrap());
    let itens: usize = parametros
        .get("paginacao.itensPorPagina")
        .map_or(100, |i| i.parse().unwrap());
    let nesta: Vec<&Value> = pix.iter().skip(pagina * itens).take(itens).collect();
    ResponseTemplate::new(200).set_body_json(json!({
        "parametros": {
            "inicio": parametros["inicio"],
            "fim": parametros["fim"],
            "paginacao": {
                "paginaAtual": pagina,
                "itensPorPagina": itens,
                "quantidadeDePaginas": pix.len().div_ceil(itens),
                "quantidadeTotalDeItens": pix.len(),
            },
        },
        "pix": nesta,
    }))
}

fn e2e_e_id(request: &Request) -> (String, String) {
    let partes: Vec<&str> = request.url.path().split('/').collect();
    // /pix/v2/pix/{e2eId}/devolucao/{id}
    (partes[4].to_owned(), partes[6].to_owned())
}

fn devolucao(estado: &Estado, e2e: &str, id: &str) -> Option<Value> {
    estado
        .devolucoes
        .get(e2e)?
        .iter()
        .find(|devolucao| devolucao["id"] == id)
        .cloned()
}

/// Asks for a refund: in processing now, refunded by the next look. The
/// same id is the same refund.
fn devolver(estado: &mut Estado, request: &Request) -> ResponseTemplate {
    let (e2e, id) = e2e_e_id(request);
    if let Some(existente) = devolucao(estado, &e2e, &id) {
        return ResponseTemplate::new(201).set_body_json(existente);
    }
    let Some(pix) = recebido(estado, &e2e) else {
        return nao_encontrado();
    };
    let corpo: Value = serde_json::from_slice(&request.body).unwrap();
    let decimal = |valor: &Value| -> Decimal {
        valor
            .as_str()
            .map_or_else(|| valor.to_string(), str::to_owned)
            .parse()
            .unwrap()
    };
    let comprometido: Decimal = pix["devolucoes"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|devolucao| decimal(&devolucao["valor"]))
        .sum();
    let valor = decimal(&corpo["valor"]);
    if valor > decimal(&pix["valor"]) - comprometido {
        return problema(
            400,
            "Valor de devolução inválido",
            "O valor da devolução passa do que resta do Pix.",
        );
    }
    let ordem: usize = estado.devolucoes.values().map(Vec::len).sum();
    // The refunds of the guide are asked for 5 minutes apart, from 14:20.
    let minutos = 14 * 60 + 20 + 5 * ordem;
    let hora = |segundos: u32| {
        let (horas, minutos) = (minutos / 60 + 3, minutos % 60);
        format!("{HOJE}T{horas:02}:{minutos:02}:{segundos:02}.000Z")
    };
    let mut pedida = json!({
        "id": id,
        "rtrId": RTR_IDS[ordem],
        "valor": format!("{valor:.2}"),
        "horario": { "solicitacao": hora(0) },
        "status": "EM_PROCESSAMENTO",
    });
    for campo in ["natureza", "descricao"] {
        if let Some(texto) = corpo[campo].as_str() {
            pedida[campo] = json!(texto);
        }
    }
    let mut feita = pedida.clone();
    feita["status"] = json!("DEVOLVIDO");
    feita["horario"]["liquidacao"] = json!(hora(3));
    estado.devolucoes.entry(e2e).or_default().push(feita);
    ResponseTemplate::new(201).set_body_json(pedida)
}
