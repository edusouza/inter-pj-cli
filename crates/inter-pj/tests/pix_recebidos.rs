//! Pix received (`/pix/v2/pix`) and their refunds against a mock API. All
//! data is synthetic.

mod common;

use chrono::DateTime;
use common::{client, mount_token};
use inter_pj::Error;
use inter_pj::pix::{
    DevolucaoSolicitada, FiltroPixRecebidos, IdDevolucao, NaturezaDevolucao, PeriodoPix,
    StatusDevolucao,
};
use serde_json::{Value, json};
use wiremock::matchers::{any, body_json, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const E2E: &str = "E12345678202609231200abcdef12345";
const TXID: &str = "7978c0c97ea847e78e8849634473c1f1";

fn recebido() -> Value {
    json!({
        "endToEndId": E2E,
        "txid": TXID,
        "valor": "100.00",
        "chave": "pix@empresa.example",
        "componentesValor": {"original": {"valor": "100.00"}},
        "horario": "2026-09-23T12:00:00.000Z",
        "infoPagador": "Pedido 123",
        "devolucoes": [{"id": "D1", "rtrId": "D12345678202609231205abcde123456", "valor": "10.00", "horario": {"solicitacao": "2026-09-23T12:05:00.000Z"}, "status": "EM_PROCESSAMENTO"}]
    })
}

async fn setup(scope: &str) -> MockServer {
    let server = MockServer::start().await;
    mount_token(&server, "tok", scope, 1).await;
    server
}

#[tokio::test]
async fn lists_the_pix_received_in_a_period() {
    let server = setup("pix.read").await;
    Mock::given(method("GET"))
        .and(path("/pix/v2/pix"))
        .and(query_param("inicio", "2026-09-01T00:00:00-03:00"))
        .and(query_param("fim", "2026-09-30T23:59:59-03:00"))
        .and(query_param("txIdPresente", "true"))
        .and(query_param("devolucaoPresente", "true"))
        .and(query_param("cnpj", "12345678000195"))
        .and(query_param("paginacao.paginaAtual", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "parametros": {
                "inicio": "2026-09-01T03:00:00Z",
                "fim": "2026-10-01T02:59:59Z",
                "paginacao": {"paginaAtual": 0, "itensPorPagina": 1000, "quantidadeDePaginas": 1, "quantidadeTotalDeItens": 1}
            },
            "pix": [recebido()]
        })))
        .expect(1)
        .mount(&server)
        .await;
    let periodo = PeriodoPix::new(
        DateTime::parse_from_rfc3339("2026-09-01T00:00:00-03:00").unwrap(),
        DateTime::parse_from_rfc3339("2026-09-30T23:59:59-03:00").unwrap(),
    )
    .unwrap();
    let mut filtro = FiltroPixRecebidos::new(periodo);
    filtro.tx_id_presente = Some(true);
    filtro.devolucao_presente = Some(true);
    filtro.devedor = Some("12.345.678/0001-95".parse().unwrap());
    let pix = client(&server)
        .pix()
        .listar_todos_pix_recebidos(&filtro)
        .await
        .unwrap();
    assert_eq!(pix.len(), 1);
    assert_eq!(pix[0].end_to_end_id.as_deref(), Some(E2E));
    assert_eq!(
        pix[0].devolucoes[0].status,
        Some(StatusDevolucao::EmProcessamento)
    );
}

#[tokio::test]
async fn looks_a_pix_up_by_its_end_to_end_id() {
    let server = setup("pix.read").await;
    Mock::given(method("GET"))
        .and(path(format!("/pix/v2/pix/{E2E}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(recebido()))
        .expect(1)
        .mount(&server)
        .await;
    let pix = client(&server)
        .pix()
        .consultar_pix_recebido(&format!(" {E2E} "))
        .await
        .unwrap();
    // Nothing is lost on the way back.
    assert_eq!(serde_json::to_value(&pix).unwrap(), recebido());
}

#[tokio::test]
async fn refunds_with_your_id_and_follows_the_refund() {
    let server = setup("pix.write pix.read").await;
    let caminho = format!("/pix/v2/pix/{E2E}/devolucao/D2");
    Mock::given(method("PUT"))
        .and(path(caminho.clone()))
        .and(body_json(
            json!({"valor": "7.89", "natureza": "ORIGINAL", "descricao": "Produto devolvido"}),
        ))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": "D2",
            "rtrId": "D12345678202609231210abcde123456",
            "valor": "7.89",
            "horario": {"solicitacao": "2026-09-23T12:10:00.000Z"},
            "status": "EM_PROCESSAMENTO"
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(caminho))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "D2",
            "valor": "7.89",
            "horario": {"solicitacao": "2026-09-23T12:10:00.000Z", "liquidacao": "2026-09-23T12:10:05.000Z"},
            "status": "DEVOLVIDO"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let pix = client(&server);
    let id: IdDevolucao = "D2".parse().unwrap();
    let mut devolucao = DevolucaoSolicitada::new("7.89".parse().unwrap());
    devolucao.natureza = Some(NaturezaDevolucao::Original);
    devolucao.descricao = Some("Produto devolvido".to_owned());
    let solicitada = pix.pix().devolver(E2E, &id, &devolucao).await.unwrap();
    assert_eq!(solicitada.status, Some(StatusDevolucao::EmProcessamento));
    let consultada = pix.pix().consultar_devolucao(E2E, &id).await.unwrap();
    assert!(
        consultada
            .status
            .as_ref()
            .is_some_and(StatusDevolucao::is_final)
    );
    assert!(
        consultada
            .horario
            .and_then(|horario| horario.liquidacao)
            .is_some()
    );
}

/// Money leaves the account: after a `503`, the refund is not repeated
/// automatically (with the same id, repeating it by hand is safe).
#[tokio::test]
async fn an_uncertain_refund_is_not_repeated() {
    let server = setup("pix.write").await;
    Mock::given(method("PUT"))
        .and(path(format!("/pix/v2/pix/{E2E}/devolucao/D3")))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&server)
        .await;
    let err = client(&server)
        .pix()
        .devolver(
            E2E,
            &"D3".parse().unwrap(),
            &DevolucaoSolicitada::new("1.00".parse().unwrap()),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(err, Error::Api(ref api) if api.status == 503),
        "{err}"
    );
}

#[tokio::test]
async fn invalid_refunds_are_not_sent() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let pix = client(&server);
    let id: IdDevolucao = "D4".parse().unwrap();
    let zero = DevolucaoSolicitada::new("0".parse().unwrap());
    let err = pix.pix().devolver(E2E, &id, &zero).await.unwrap_err();
    assert!(matches!(err, Error::InvalidInput(_)), "{err}");
    let um_real = DevolucaoSolicitada::new("1.00".parse().unwrap());
    for invalido in ["../../banking/v2/saldo", "", "E123 456"] {
        let err = pix
            .pix()
            .devolver(invalido, &id, &um_real)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::InvalidInput(_)), "{invalido}: {err}");
        let err = pix
            .pix()
            .consultar_pix_recebido(invalido)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::InvalidInput(_)), "{invalido}: {err}");
    }
}
