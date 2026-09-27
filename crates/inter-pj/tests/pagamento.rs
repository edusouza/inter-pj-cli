//! Payments by barcode (`/banking/v2/pagamento`) against a mock API. The
//! codes are examples of the API documentation; everything else is synthetic.

mod common;

use chrono::NaiveDate;
use common::{client, mount_token};
use inter_pj::Error;
use inter_pj::banking::{
    DataDoPagamento, FiltroPagamentos, PagamentoBoleto, PagamentoBoletoError, StatusPagamento,
};
use serde_json::json;
use wiremock::matchers::{any, body_json, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const PAGAMENTO: &str = "/banking/v2/pagamento";
const LINHA: &str = "07797777051167847115990071126347192950000003010";
const BARRAS: &str = "07791929500000030107777011678471159007112634";
const TRANSACAO: &str = "8bbdede4-35db-4ec9-b652-e176841e62c8";

fn data(ano: i32, mes: u32, dia: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(ano, mes, dia).unwrap()
}

fn boleto(valor: &str) -> PagamentoBoleto {
    PagamentoBoleto::new(
        LINHA.parse().unwrap(),
        valor.parse().unwrap(),
        data(2026, 10, 10),
    )
}

fn api_status(err: &Error) -> u16 {
    match err {
        Error::Api(api) => api.status,
        other => panic!("esperado Error::Api, obtido {other:?}"),
    }
}

async fn setup(scope: &str) -> MockServer {
    let server = MockServer::start().await;
    mount_token(&server, "tok", scope, 1).await;
    server
}

#[tokio::test]
async fn pays_a_boleto_with_the_documented_body() {
    let server = setup("pagamento-boleto.write").await;
    let mut pagamento = boleto("30.1");
    pagamento.data_pagamento = Some(data(2026, 10, 9));
    pagamento.cpf_cnpj_beneficiario = Some("12.345.678/0001-95".parse().unwrap());
    Mock::given(method("POST"))
        .and(path(PAGAMENTO))
        .and(body_json(json!({
            "codBarraLinhaDigitavel": BARRAS,
            "valorPagar": "30.10",
            "dataPagamento": "2026-10-09",
            "dataVencimento": "2026-10-10",
            "cpfCnpjBeneficiario": "12345678000195"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "quantidadeAprovadores": 0,
            "dataAgendamento": "2026-10-09 00:00:00",
            "statusPagamento": "AGENDADO",
            "codigoTransacao": TRANSACAO
        })))
        .expect(1)
        .mount(&server)
        .await;

    let resposta = client(&server)
        .banking()
        .pagar_boleto(&pagamento)
        .await
        .unwrap();
    assert_eq!(resposta.status_pagamento, Some(StatusPagamento::Agendado));
    assert_eq!(resposta.codigo_transacao.as_deref(), Some(TRANSACAO));
}

#[tokio::test]
async fn invalid_requests_never_reach_the_api() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let client = client(&server);
    let banking = client.banking();

    let err = banking.pagar_boleto(&boleto("0")).await.unwrap_err();
    let Error::InvalidInput(source) = &err else {
        panic!("{err:?}");
    };
    assert_eq!(
        source.downcast_ref(),
        Some(&PagamentoBoletoError::ValorNaoPositivo)
    );

    let invertido = FiltroPagamentos {
        periodo: Some((data(2026, 9, 30), data(2026, 9, 1))),
        ..FiltroPagamentos::default()
    };
    assert!(matches!(
        banking.pagamentos(&invertido).await,
        Err(Error::InvalidInput(_))
    ));
    let codigo_invalido = FiltroPagamentos {
        codigo_transacao: Some("123".to_owned()),
        ..FiltroPagamentos::default()
    };
    assert!(matches!(
        banking.pagamentos(&codigo_invalido).await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        banking.cancelar_pagamento("../pix").await,
        Err(Error::InvalidInput(_))
    ));
}

/// Without an idempotency key, only a request that surely was not processed
/// is repeated.
#[tokio::test]
async fn payments_are_retried_only_when_surely_not_processed() {
    let server = setup("pagamento-boleto.write").await;
    Mock::given(method("POST"))
        .and(path(PAGAMENTO))
        .respond_with(ResponseTemplate::new(429))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(PAGAMENTO))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&server)
        .await;

    let err = client(&server)
        .banking()
        .pagar_boleto(&boleto("30.10"))
        .await
        .unwrap_err();
    assert_eq!(api_status(&err), 503);
}

#[tokio::test]
async fn lists_payments_with_the_filters() {
    let server = setup("pagamento-boleto.read").await;
    Mock::given(method("GET"))
        .and(path(PAGAMENTO))
        .and(query_param("dataInicio", "2026-09-01"))
        .and(query_param("dataFim", "2026-09-30"))
        .and(query_param("filtrarDataPor", "PAGAMENTO"))
        .and(query_param("codBarraLinhaDigitavel", BARRAS))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "codigoTransacao": TRANSACAO,
                "codigoBarra": BARRAS,
                "valorPago": 30.1,
                "statusPagamento": "REALIZADO",
                "nomeBeneficiario": "Fornecedor Exemplo"
            }
        ])))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(PAGAMENTO))
        .and(query_param("codigoTransacao", TRANSACAO))
        .respond_with(ResponseTemplate::new(200).set_body_string("null"))
        .expect(1)
        .mount(&server)
        .await;

    let client = client(&server);
    let filtro = FiltroPagamentos {
        periodo: Some((data(2026, 9, 1), data(2026, 9, 30))),
        filtrar_por: Some(DataDoPagamento::Pagamento),
        codigo: Some(LINHA.parse().unwrap()),
        codigo_transacao: None,
    };
    let pagamentos = client.banking().pagamentos(&filtro).await.unwrap();
    assert_eq!(pagamentos.len(), 1);
    assert_eq!(
        pagamentos[0].status_pagamento,
        Some(StatusPagamento::Realizado)
    );
    assert_eq!(pagamentos[0].valor_pago, Some("30.1".parse().unwrap()));

    let por_codigo = FiltroPagamentos {
        codigo_transacao: Some(TRANSACAO.to_uppercase()),
        ..FiltroPagamentos::default()
    };
    assert!(
        client
            .banking()
            .pagamentos(&por_codigo)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn cancels_a_scheduled_payment() {
    let server = setup("pagamento-boleto.write").await;
    Mock::given(method("DELETE"))
        .and(path(format!("{PAGAMENTO}/{TRANSACAO}")))
        .respond_with(ResponseTemplate::new(204))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path(format!("{PAGAMENTO}/{TRANSACAO}")))
        .respond_with(ResponseTemplate::new(422).set_body_json(json!({
            "title": "Não é possível cancelar",
            "detail": "O pagamento já foi realizado."
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = client(&server);
    client
        .banking()
        .cancelar_pagamento(&format!(" {} ", TRANSACAO.to_uppercase()))
        .await
        .unwrap();
    let err = client
        .banking()
        .cancelar_pagamento(TRANSACAO)
        .await
        .unwrap_err();
    assert_eq!(api_status(&err), 422);
}
