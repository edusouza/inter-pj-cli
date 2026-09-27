//! The history of the callbacks of the webhooks and their retry, against a
//! mock API. All data is synthetic.

mod common;

use chrono::DateTime;
use common::{client, mount_token};
use inter_pj::Error;
use inter_pj::pix::{ChavePix, Txid};
use inter_pj::webhook::{FiltroCallbacks, TipoWebhookBanking};
use serde_json::{Value, json};
use wiremock::matchers::{any, body_json, method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

const URL: &str = "https://api.empresa.example/inter/webhook";
const CODIGO: &str = "0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d";
const OUTRO: &str = "1c8f5d2b-6e4a-4b3c-8d9e-8f7a6b5c4d3e";
const TXID: &str = "7978c0c97ea847e78e8849634473c1f1";

async fn setup(scope: &str) -> MockServer {
    let server = MockServer::start().await;
    mount_token(&server, "tok", scope, 1).await;
    server
}

fn filtro() -> FiltroCallbacks {
    FiltroCallbacks::new(
        DateTime::parse_from_rfc3339("2026-09-01T00:00:00-03:00").unwrap(),
        DateTime::parse_from_rfc3339("2026-09-02T00:00:00-03:00").unwrap(),
    )
    .unwrap()
}

fn tentativa(codigo: &str, sucesso: bool) -> Value {
    let mut tentativa = json!({
        "webhookUrl": URL,
        "payload": [{"codigoSolicitacao": codigo, "situacao": "RECEBIDO", "dataHoraSituacao": "2026-09-01T10:00:00-03:00"}],
        "numeroTentativa": 1,
        "dataHoraDisparo": "2026-09-01T10:00:05-03:00",
        "sucesso": sucesso,
        "httpStatus": if sucesso { 200 } else { 503 }
    });
    if !sucesso {
        tentativa["mensagemErro"] = "Service Unavailable".into();
    }
    tentativa
}

#[tokio::test]
async fn the_cobranca_history_reads_every_page_with_its_filters() {
    let server = setup("boleto-cobranca.read").await;
    for (pagina, ultima, codigo) in [("0", false, CODIGO), ("1", true, OUTRO)] {
        Mock::given(method("GET"))
            .and(path("/cobranca/v3/cobrancas/webhook/callbacks"))
            .and(query_param("dataHoraInicio", "2026-09-01T00:00:00-03:00"))
            .and(query_param("dataHoraFim", "2026-09-02T00:00:00-03:00"))
            .and(query_param("pagina", pagina))
            .and(query_param("tamanhoPagina", "50"))
            .and(query_param_is_missing("codigoSolicitacao"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "totalElementos": 3, "totalPaginas": 2,
                "primeiraPagina": pagina == "0", "ultimaPagina": ultima,
                "data": if ultima {
                    json!([tentativa(codigo, true)])
                } else {
                    json!([tentativa(codigo, false), tentativa(codigo, true)])
                }
            })))
            .expect(1)
            .mount(&server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path("/cobranca/v3/cobrancas/webhook/callbacks"))
        .and(query_param("pagina", "0"))
        .and(query_param("tamanhoPagina", "10"))
        .and(query_param("codigoSolicitacao", CODIGO))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "totalElementos": 1, "totalPaginas": 1, "ultimaPagina": true,
            "data": [tentativa(CODIGO, false)]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = client(&server);
    let todos = client
        .cobranca()
        .listar_todos_callbacks(&filtro())
        .await
        .unwrap();
    assert_eq!(todos.len(), 3);
    assert_eq!(todos[0].sucesso, Some(false));
    assert_eq!(todos[0].http_status, Some(503));
    assert_eq!(
        todos[0].mensagem_erro.as_deref(),
        Some("Service Unavailable")
    );
    assert_eq!(todos[2].valores("codigoSolicitacao"), [OUTRO]);

    let mut so_um = filtro();
    so_um.identificador = Some(format!(" {CODIGO} "));
    let pagina = client
        .cobranca()
        .listar_callbacks(&so_um, 0, Some(10))
        .await
        .unwrap();
    assert_eq!(pagina.total_elementos, Some(1));
    assert_eq!(pagina.data[0].disparo(), Some("2026-09-01T10:00:05-03:00"));
}

#[tokio::test]
async fn banking_filters_by_the_identifier_of_each_kind() {
    let server = setup("webhook-banking.read").await;
    Mock::given(method("GET"))
        .and(path("/banking/v2/webhooks/pix-pagamento/callbacks"))
        .and(query_param("endToEnd", "E00416968202609241310abcdEFGH123"))
        .and(query_param_is_missing("codigoTransacao"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "totalElementos": 1, "totalPaginas": 1, "primeiraPagina": true, "ultimaPagina": true,
            "data": [{
                "webhookUrl": URL,
                "payload": {"codigoSolicitacao": CODIGO, "endToEnd": "E00416968202609241310abcdEFGH123"},
                "numeroTentativa": 2,
                "dataEnvio": "2026-09-24T13:10:00Z",
                "sucesso": true,
                "httpStatus": 200
            }]
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/banking/v2/webhooks/boleto-pagamento/callbacks"))
        .and(query_param(
            "codigoTransacao",
            "8bbdede4-35db-4ec9-b652-e176841e62c8",
        ))
        .and(query_param_is_missing("endToEnd"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "totalElementos": 0, "totalPaginas": 0, "ultimaPagina": true, "data": []
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = client(&server);
    let mut filtro = filtro();
    filtro.identificador = Some("E00416968202609241310abcdEFGH123".to_owned());
    let callbacks = client
        .banking()
        .listar_todos_callbacks(TipoWebhookBanking::PixPagamento, &filtro)
        .await
        .unwrap();
    assert_eq!(callbacks[0].disparo(), Some("2026-09-24T13:10:00Z"));
    assert_eq!(callbacks[0].numero_tentativa, Some(2));
    assert_eq!(callbacks[0].valores("codigoSolicitacao"), [CODIGO]);

    // Codes are sent in lower case.
    filtro.identificador = Some("8BBDEDE4-35DB-4EC9-B652-E176841E62C8".to_owned());
    let callbacks = client
        .banking()
        .listar_todos_callbacks(TipoWebhookBanking::BoletoPagamento, &filtro)
        .await
        .unwrap();
    assert!(callbacks.is_empty());
}

#[tokio::test]
async fn the_pix_history_filters_by_txid() {
    let server = setup("webhook.read").await;
    Mock::given(method("GET"))
        .and(path("/pix/v2/webhook/callbacks"))
        .and(query_param("txid", TXID))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "totalElementos": 1, "totalPaginas": 1, "ultimaPagina": true,
            "data": [{
                "webhookUrl": URL,
                "payload": {"pix": [{"endToEndId": "E00416968202609241310abcdEFGH123", "txid": TXID, "valor": "37.00"}]},
                "numeroTentativa": 1,
                "dataHoraDisparo": "2026-09-24T13:10:02Z",
                "sucesso": false,
                "httpStatus": 404
            }]
        })))
        .expect(1)
        .mount(&server)
        .await;
    let client = client(&server);
    let mut filtro = filtro();
    filtro.identificador = Some(TXID.to_owned());
    let callbacks = client.pix().listar_todos_callbacks(&filtro).await.unwrap();
    assert_eq!(callbacks[0].valores("txid"), [TXID]);
    assert_eq!(callbacks[0].http_status, Some(404));
}

#[tokio::test]
async fn retries_send_the_ids_and_name_those_found() {
    let server = setup("boleto-cobranca.write webhook-banking.write webhook.write").await;
    Mock::given(method("POST"))
        .and(path("/cobranca/v3/cobrancas/webhook/callbacks/retry"))
        .and(body_json(json!({"codigoSolicitacao": [CODIGO, OUTRO]})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"foundIds": [CODIGO]})))
        .expect(1)
        .mount(&server)
        .await;
    // Each kind of the Banking API names its codes in a field of its own.
    Mock::given(method("POST"))
        .and(path(
            "/banking/v2/webhooks/boleto-pagamento/callbacks/retry",
        ))
        .and(body_json(
            json!({"codigoTransacao": ["8bbdede4-35db-4ec9-b652-e176841e62c8"]}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"foundIds": []})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/banking/v2/webhooks/pix-pagamento/callbacks/retry"))
        .and(body_json(json!({"codigoSolicitacao": [CODIGO]})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"foundIds": [CODIGO]})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/pix/v2/webhook/callbacks/retry"))
        .and(body_json(
            json!({"txId": [TXID], "chavePix": "+5511912345678"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"foundIds": [TXID]})))
        .expect(1)
        .mount(&server)
        .await;

    let client = client(&server);
    let reenvio = client
        .cobranca()
        .reenviar_callbacks(&[CODIGO.to_owned(), OUTRO.to_owned()])
        .await
        .unwrap();
    assert_eq!(reenvio.found_ids, [CODIGO]);
    let reenvio = client
        .banking()
        .reenviar_callbacks(
            TipoWebhookBanking::BoletoPagamento,
            &["8BBDEDE4-35DB-4EC9-B652-E176841E62C8".to_owned()],
        )
        .await
        .unwrap();
    assert!(reenvio.found_ids.is_empty());
    let reenvio = client
        .banking()
        .reenviar_callbacks(TipoWebhookBanking::PixPagamento, &[CODIGO.to_owned()])
        .await
        .unwrap();
    assert_eq!(reenvio.found_ids, [CODIGO]);
    let chave: ChavePix = "+55 (11) 91234-5678".parse().unwrap();
    let txid: Txid = TXID.parse().unwrap();
    let reenvio = client
        .pix()
        .reenviar_callbacks(&chave, &[txid])
        .await
        .unwrap();
    assert_eq!(reenvio.found_ids, [TXID]);
}

#[tokio::test]
async fn nothing_is_sent_when_the_request_is_invalid() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let client = client(&server);
    let invalido = |resultado: Result<_, Error>| {
        let err = resultado.unwrap_err();
        assert!(matches!(err, Error::InvalidInput(_)), "{err}");
        err.to_string()
    };

    let muitos: Vec<String> = (0..51)
        .map(|n| format!("0b7e4c1a-5d3f-4a2b-9c8d-{n:012}"))
        .collect();
    assert!(
        invalido(
            client
                .cobranca()
                .reenviar_callbacks(&muitos)
                .await
                .map(|_| ())
        )
        .contains("de 1 a 50")
    );
    invalido(client.cobranca().reenviar_callbacks(&[]).await.map(|_| ()));
    assert!(
        invalido(
            client
                .banking()
                .reenviar_callbacks(TipoWebhookBanking::PixPagamento, &["123".to_owned()])
                .await
                .map(|_| ())
        )
        .contains("código da solicitação do Pix inválido")
    );
    assert!(
        invalido(
            client
                .cobranca()
                .listar_callbacks(&filtro(), 0, Some(9))
                .await
                .map(|_| ())
        )
        .contains("de 10 a 50")
    );

    let mut filtro = filtro();
    filtro.identificador = Some("curto".to_owned());
    invalido(
        client
            .pix()
            .listar_callbacks(&filtro, 0, None)
            .await
            .map(|_| ()),
    );
    invalido(
        client
            .banking()
            .listar_callbacks(TipoWebhookBanking::BoletoPagamento, &filtro, 0, None)
            .await
            .map(|_| ()),
    );
    filtro.identificador = Some("E0041/../../saldo".to_owned());
    assert!(
        invalido(
            client
                .banking()
                .listar_callbacks(TipoWebhookBanking::PixPagamento, &filtro, 0, None)
                .await
                .map(|_| ())
        )
        .contains("endToEnd inválido")
    );
}
