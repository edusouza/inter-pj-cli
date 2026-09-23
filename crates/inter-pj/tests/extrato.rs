//! Statements (`/banking/v2/extrato*`) against a mock API. Every name,
//! document and amount here is synthetic.

mod common;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use chrono::NaiveDate;
use common::{client, mount_token};
use inter_pj::Error;
use inter_pj::banking::{
    Detalhe, FiltroExtrato, Periodo, TAMANHO_PAGINA_MAXIMO, TipoOperacao, TipoTransacao,
};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

const EXTRATO: &str = "/banking/v2/extrato";
const COMPLETO: &str = "/banking/v2/extrato/completo";
const EXPORTAR: &str = "/banking/v2/extrato/exportar";

fn agosto() -> Periodo {
    Periodo::new(
        NaiveDate::from_ymd_opt(2026, 8, 1).unwrap(),
        NaiveDate::from_ymd_opt(2026, 8, 31).unwrap(),
    )
    .unwrap()
}

fn dec(s: &str) -> Decimal {
    s.parse().unwrap()
}

/// A synthetic enriched transaction, identified by `id`.
fn completa(id: u32) -> Value {
    json!({
        "idTransacao": id.to_string(),
        "dataTransacao": "2026-08-10",
        "tipoTransacao": "PIX",
        "tipoOperacao": "C",
        "valor": "10.00",
        "titulo": "Pix recebido",
        "descricao": "Cliente de teste",
        "detalhes": {"txId": format!("tx{id}"), "nomePagador": "Cliente de teste"}
    })
}

fn ids(transacoes: &[inter_pj::banking::TransacaoCompleta]) -> Vec<String> {
    transacoes
        .iter()
        .map(|t| t.id_transacao.clone().unwrap_or_default())
        .collect()
}

async fn setup() -> MockServer {
    let server = MockServer::start().await;
    mount_token(&server, "tok", "extrato.read", 1).await;
    server
}

#[tokio::test]
async fn extrato_sends_the_period_and_parses_exact_amounts() {
    let server = setup().await;
    Mock::given(method("GET"))
        .and(path(EXTRATO))
        .and(query_param("dataInicio", "2026-08-01"))
        .and(query_param("dataFim", "2026-08-31"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "transacoes": [
                {"dataEntrada": "2026-08-03", "tipoTransacao": "PIX", "tipoOperacao": "C",
                 "valor": "1500.00", "titulo": "Pix recebido", "descricao": "Cliente", "cpmf": "0.00"},
                {"dataEntrada": "2026-08-05", "tipoTransacao": "PAGAMENTO", "tipoOperacao": "D",
                 "valor": "250.10", "titulo": "Pagamento", "descricao": "Boleto"}
            ]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let transacoes = client(&server).banking().extrato(agosto()).await.unwrap();
    assert_eq!(transacoes.len(), 2);
    assert_eq!(transacoes[0].valor_com_sinal(), Some(dec("1500.00")));
    assert_eq!(transacoes[1].valor_com_sinal(), Some(dec("-250.10")));
    assert_eq!(transacoes[1].tipo_transacao, Some(TipoTransacao::Pagamento));
}

#[tokio::test]
async fn extrato_without_transactions_is_empty() {
    let server = setup().await;
    Mock::given(method("GET"))
        .and(path(EXTRATO))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"transacoes": []})))
        .mount(&server)
        .await;
    assert!(
        client(&server)
            .banking()
            .extrato(agosto())
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn extrato_completo_sends_filters_and_page() {
    let server = setup().await;
    Mock::given(method("GET"))
        .and(path(COMPLETO))
        .and(query_param("dataInicio", "2026-08-01"))
        .and(query_param("dataFim", "2026-08-31"))
        .and(query_param("pagina", "2"))
        .and(query_param("tamanhoPagina", "20"))
        .and(query_param("tipoOperacao", "D"))
        .and(query_param("tipoTransacao", "PAGAMENTO"))
        .and(query_param_is_missing("scrollEnabled"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "totalPaginas": 3, "totalElementos": 41, "ultimaPagina": true,
            "primeiraPagina": false, "tamanhoPagina": 20, "numeroDeElementos": 1,
            "transacoes": [completa(41)]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let filtro = FiltroExtrato::new(agosto())
        .tipo_operacao(TipoOperacao::Debito)
        .tipo_transacao(TipoTransacao::Pagamento);
    let pagina = client(&server)
        .banking()
        .extrato_completo(&filtro, 2, Some(20))
        .await
        .unwrap();
    assert_eq!(pagina.total_elementos, Some(41));
    assert_eq!(pagina.ultima_pagina, Some(true));
    let Some(Detalhe::Pix(pix)) = &pagina.transacoes[0].detalhes else {
        panic!("{:?}", pagina.transacoes[0].detalhes);
    };
    assert_eq!(pix.tx_id.as_deref(), Some("tx41"));
}

#[tokio::test]
async fn all_pages_are_read_in_order() {
    let server = setup().await;
    let maximo = TAMANHO_PAGINA_MAXIMO.to_string();
    for (pagina, ids, ultima) in [(0, [1, 2], false), (1, [3, 4], false), (2, [5, 6], true)] {
        Mock::given(method("GET"))
            .and(path(COMPLETO))
            .and(query_param("pagina", pagina.to_string()))
            .and(query_param("tamanhoPagina", maximo.as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "totalPaginas": 3, "totalElementos": 6, "ultimaPagina": ultima,
                "transacoes": ids.map(completa)
            })))
            .expect(1)
            .mount(&server)
            .await;
    }

    let transacoes = client(&server)
        .banking()
        .extrato_completo_todas(&FiltroExtrato::new(agosto()))
        .await
        .unwrap();
    assert_eq!(ids(&transacoes), ["1", "2", "3", "4", "5", "6"]);
}

#[tokio::test]
async fn large_periods_switch_to_scroll_mode() {
    let server = setup().await;
    // The first page reveals more transactions than pagination can reach.
    Mock::given(method("GET"))
        .and(path(COMPLETO))
        .and(query_param("pagina", "0"))
        .and(query_param_is_missing("scrollEnabled"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "totalPaginas": 3, "totalElementos": 25_000, "ultimaPagina": false,
            "transacoes": [completa(999)]
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(COMPLETO))
        .and(query_param("scrollEnabled", "true"))
        .and(query_param("dataInicio", "2026-08-01"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "totalElementos": 25_000, "numeroDeElementos": 2,
            "scrollId": "550e8400-e29b-41d4-a716-446655440000", "hasMore": true,
            "transacoes": [completa(1), completa(2)]
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(COMPLETO))
        .and(query_param(
            "scrollId",
            "550e8400-e29b-41d4-a716-446655440000",
        ))
        .and(query_param("dataFim", "2026-08-31"))
        .and(query_param_is_missing("scrollEnabled"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "totalElementos": 25_000, "numeroDeElementos": 1, "hasMore": false,
            "transacoes": [completa(3)]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let transacoes = client(&server)
        .banking()
        .extrato_completo_todas(&FiltroExtrato::new(agosto()))
        .await
        .unwrap();
    assert_eq!(ids(&transacoes), ["1", "2", "3"]);
}

#[tokio::test]
async fn active_scroll_error_keeps_its_type() {
    let server = setup().await;
    Mock::given(method("GET"))
        .and(path(COMPLETO))
        .and(query_param("scrollEnabled", "true"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "title": "Já existe um scroll ativo para esta conta corrente.",
            "detail": "Aguarde o scroll atual expirar ou finalize-o antes de iniciar um novo.",
            "typeError": "SCROLL_ALREADY_ACTIVE"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let err = client(&server)
        .banking()
        .iniciar_scroll(&FiltroExtrato::new(agosto()), None)
        .await
        .unwrap_err();
    let Error::Api(api) = &err else {
        panic!("{err:?}");
    };
    let problem = api.problem.as_ref().unwrap();
    assert_eq!(problem.type_error.as_deref(), Some("SCROLL_ALREADY_ACTIVE"));
}

#[tokio::test]
async fn scroll_requests_are_not_repeated_after_server_errors() {
    // The server may have advanced the scroll: repeating could skip a batch.
    let server = setup().await;
    Mock::given(method("GET"))
        .and(path(COMPLETO))
        .and(query_param("scrollId", "abc"))
        .respond_with(ResponseTemplate::new(504))
        .expect(1)
        .mount(&server)
        .await;

    let err = client(&server)
        .banking()
        .continuar_scroll(&FiltroExtrato::new(agosto()), "abc")
        .await
        .unwrap_err();
    assert!(
        matches!(&err, Error::Api(api) if api.status == 504),
        "{err:?}"
    );
}

#[tokio::test]
async fn scroll_requests_are_repeated_when_rate_limited() {
    let server = setup().await;
    Mock::given(method("GET"))
        .and(path(COMPLETO))
        .and(query_param("scrollId", "abc"))
        .respond_with(ResponseTemplate::new(429))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(COMPLETO))
        .and(query_param("scrollId", "abc"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"hasMore": false, "transacoes": [completa(7)]})),
        )
        .expect(1)
        .mount(&server)
        .await;

    let lote = client(&server)
        .banking()
        .continuar_scroll(&FiltroExtrato::new(agosto()), "abc")
        .await
        .unwrap();
    assert_eq!(lote.has_more, Some(false));
}

#[tokio::test]
async fn scroll_without_id_is_a_decode_error() {
    let server = setup().await;
    Mock::given(method("GET"))
        .and(path(COMPLETO))
        .and(query_param("pagina", "0"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"totalElementos": 20_000, "transacoes": []})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(COMPLETO))
        .and(query_param("scrollEnabled", "true"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"hasMore": true, "transacoes": [completa(1)]})),
        )
        .mount(&server)
        .await;

    let err = client(&server)
        .banking()
        .extrato_completo_todas(&FiltroExtrato::new(agosto()))
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Decode { .. }), "{err:?}");
    assert!(err.to_string().contains("scrollId"), "{err}");
}

#[tokio::test]
async fn pdf_is_decoded() {
    let server = setup().await;
    let documento = b"%PDF-1.4\n% documento sintetico de teste\n%%EOF\n".to_vec();
    Mock::given(method("GET"))
        .and(path(EXPORTAR))
        .and(query_param("dataInicio", "2026-08-01"))
        .and(query_param("dataFim", "2026-08-31"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            // Line breaks inside the base64 text are tolerated.
            "pdf": BASE64.encode(&documento).chars().collect::<Vec<_>>()
                .chunks(16).map(|c| c.iter().collect::<String>()).collect::<Vec<_>>().join("\n")
        })))
        .expect(1)
        .mount(&server)
        .await;

    let pdf = client(&server)
        .banking()
        .extrato_pdf(agosto())
        .await
        .unwrap();
    assert_eq!(pdf, documento);
}

#[tokio::test]
async fn content_that_is_not_a_pdf_is_rejected_without_being_quoted() {
    for body in [
        json!({"pdf": BASE64.encode("conteudo-secreto-do-extrato")}),
        json!({"pdf": "!!! não é base64 conteudo-secreto-do-extrato"}),
        json!({}),
    ] {
        let server = setup().await;
        Mock::given(method("GET"))
            .and(path(EXPORTAR))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;

        let err = client(&server)
            .banking()
            .extrato_pdf(agosto())
            .await
            .unwrap_err();
        assert!(matches!(err, Error::Decode { .. }), "{err:?}");
        assert!(!err.to_string().contains("conteudo-secreto"), "{err}");
    }
}
