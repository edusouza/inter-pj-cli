//! The bank of the guides: a mock of the Inter API with the account of a
//! fictitious company, Empresa Exemplo Ltda, from June to September 2026
//! (`extrato.json`, as the enriched statement sends it). The balance follows
//! from the statement, so every command tells the same story. Every name,
//! document, key and amount is synthetic.

use std::collections::HashMap;
use std::fmt::Write as _;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use rust_decimal::Decimal;
use serde_json::{Value, json};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

/// The access token the bank gives, and wants in every request.
const TOKEN: &str = "token-dos-guias";

/// The balance at the end of 31/05/2026, before the first transaction.
const SALDO_INICIAL: &str = "9845.37";

/// The page size of the enriched statement when none is asked for.
const TAMANHO_PAGINA: usize = 50;

type Resposta = fn(&Request) -> ResponseTemplate;

pub(crate) struct Banco {
    servidor: MockServer,
}

impl Banco {
    pub(crate) async fn novo() -> Self {
        let servidor = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth/v2/token"))
            .respond_with(token as Resposta)
            .mount(&servidor)
            .await;
        let consultas: [(&str, Resposta); 4] = [
            ("/banking/v2/saldo", saldo),
            ("/banking/v2/extrato", extrato),
            ("/banking/v2/extrato/completo", extrato_completo),
            ("/banking/v2/extrato/exportar", extrato_pdf),
        ];
        for (caminho, resposta) in consultas {
            Mock::given(method("GET"))
                .and(path(caminho))
                .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
                .respond_with(resposta)
                .mount(&servidor)
                .await;
        }
        Self { servidor }
    }

    pub(crate) fn uri(&self) -> String {
        self.servidor.uri()
    }
}

/// A token with the scopes asked for, as the bank gives to an integration
/// that has them all.
fn token(request: &Request) -> ResponseTemplate {
    let formulario: HashMap<String, String> =
        serde_urlencoded::from_bytes(&request.body).unwrap_or_default();
    ResponseTemplate::new(200).set_body_json(json!({
        "access_token": TOKEN,
        "token_type": "Bearer",
        "expires_in": 3600,
        "scope": formulario.get("scope").map_or("", String::as_str),
    }))
}

fn parametros(request: &Request) -> HashMap<String, String> {
    request.url.query_pairs().into_owned().collect()
}

/// Without a date, the balance now, with the blocked amounts and the limit;
/// with `dataSaldo`, only the balance at the end of that day.
fn saldo(request: &Request) -> ResponseTemplate {
    let data = parametros(request).remove("dataSaldo");
    let movimento: Decimal = transacoes()
        .iter()
        .filter(|t| {
            data.as_deref()
                .is_none_or(|data| texto(t, "dataTransacao") <= data)
        })
        .map(|t| {
            let valor: Decimal = texto(t, "valor").parse().unwrap();
            if texto(t, "tipoOperacao") == "D" {
                -valor
            } else {
                valor
            }
        })
        .sum();
    let disponivel = numero(SALDO_INICIAL.parse::<Decimal>().unwrap() + movimento);
    ResponseTemplate::new(200).set_body_json(match data {
        Some(_) => json!({ "disponivel": disponivel }),
        None => json!({
            "disponivel": disponivel,
            "bloqueadoCheque": 0,
            "bloqueadoJudicialmente": 0,
            "bloqueadoAdministrativo": 0,
            "limite": 5000,
        }),
    })
}

fn extrato(request: &Request) -> ResponseTemplate {
    let transacoes: Vec<Value> = do_periodo(&parametros(request))
        .map(|t| {
            json!({
                "dataEntrada": t["dataTransacao"],
                "tipoTransacao": t["tipoTransacao"],
                "tipoOperacao": t["tipoOperacao"],
                "valor": t["valor"],
                "titulo": t["titulo"],
                "descricao": t["descricao"],
            })
        })
        .collect();
    ResponseTemplate::new(200).set_body_json(json!({ "transacoes": transacoes }))
}

/// The enriched statement, in pages from 0 and with the filters.
fn extrato_completo(request: &Request) -> ResponseTemplate {
    let parametros = parametros(request);
    let filtro =
        |campo: &str, t: &Value| parametros.get(campo).is_none_or(|v| texto(t, campo) == v);
    let transacoes: Vec<Value> = do_periodo(&parametros)
        .filter(|t| filtro("tipoOperacao", t) && filtro("tipoTransacao", t))
        .collect();
    let pagina: usize = parametros.get("pagina").map_or(0, |p| p.parse().unwrap());
    let tamanho: usize = parametros
        .get("tamanhoPagina")
        .map_or(TAMANHO_PAGINA, |t| t.parse().unwrap());
    let total_paginas = transacoes.len().div_ceil(tamanho);
    let nesta: Vec<&Value> = transacoes
        .iter()
        .skip(pagina * tamanho)
        .take(tamanho)
        .collect();
    ResponseTemplate::new(200).set_body_json(json!({
        "totalPaginas": total_paginas,
        "totalElementos": transacoes.len(),
        "ultimaPagina": pagina + 1 >= total_paginas,
        "primeiraPagina": pagina == 0,
        "tamanhoPagina": tamanho,
        "numeroDeElementos": nesta.len(),
        "transacoes": nesta,
    }))
}

/// A one-page PDF with the period in its title.
fn extrato_pdf(request: &Request) -> ResponseTemplate {
    let parametros = parametros(request);
    let data = |campo: &str| {
        let data = &parametros[campo];
        format!("{}/{}/{}", &data[8..10], &data[5..7], &data[..4])
    };
    let titulo = format!("Extrato de {} a {}", data("dataInicio"), data("dataFim"));
    ResponseTemplate::new(200).set_body_json(json!({ "pdf": BASE64.encode(pdf(&titulo)) }))
}

fn pdf(titulo: &str) -> Vec<u8> {
    let conteudo = format!("BT /F1 14 Tf 56 780 Td ({titulo}) Tj ET");
    let objetos = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Contents 4 0 R \
         /Resources << /Font << /F1 5 0 R >> >> >>"
            .to_owned(),
        format!(
            "<< /Length {} >>\nstream\n{conteudo}\nendstream",
            conteudo.len()
        ),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
    ];
    let mut pdf = String::from("%PDF-1.4\n");
    let mut posicoes = Vec::new();
    for (numero, objeto) in (1..).zip(&objetos) {
        posicoes.push(pdf.len());
        let _ = write!(pdf, "{numero} 0 obj\n{objeto}\nendobj\n");
    }
    let xref = pdf.len();
    let _ = write!(pdf, "xref\n0 {}\n0000000000 65535 f \n", objetos.len() + 1);
    for posicao in posicoes {
        let _ = writeln!(pdf, "{posicao:010} 00000 n ");
    }
    let _ = write!(
        pdf,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objetos.len() + 1
    );
    pdf.into_bytes()
}

/// The statement of the account, as the enriched statement sends it.
fn transacoes() -> Vec<Value> {
    serde_json::from_str(include_str!("extrato.json")).unwrap()
}

/// The transactions from `dataInicio` to `dataFim`.
fn do_periodo(parametros: &HashMap<String, String>) -> impl Iterator<Item = Value> {
    let inicio = parametros["dataInicio"].clone();
    let fim = parametros["dataFim"].clone();
    transacoes().into_iter().filter(move |t| {
        let data = texto(t, "dataTransacao");
        inicio.as_str() <= data && data <= fim.as_str()
    })
}

fn texto<'a>(transacao: &'a Value, campo: &str) -> &'a str {
    transacao[campo].as_str().unwrap_or_default()
}

/// A JSON number with the digits of `valor`, as the API sends balances.
fn numero(valor: Decimal) -> Value {
    serde_json::from_str(&valor.to_string()).unwrap()
}
