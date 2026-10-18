//! Payments by barcode (`/banking/v2/pagamento`) against a mock API. The
//! codes are examples of the API documentation; everything else is synthetic.

mod common;

use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{NaiveDate, TimeDelta, Utc};
use common::{client, mount_token};
use inter_pj::banking::{
    DataDoPagamento, FiltroPagamentos, PagamentoBoleto, PagamentoBoletoError, StatusPagamento,
};
use inter_pj::{
    AccessToken, Credentials, Error, InterClient, RetryPolicy, Scope, ScopeSet, TokenStore,
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

/// Payments have no idempotency key: after an error, what matters is
/// whether the request may have reached the API.
#[tokio::test]
async fn tells_whether_a_failed_payment_may_have_been_made() {
    let ilegivel = || ResponseTemplate::new(200).set_body_string("<html>proxy</html>");
    let atrasada = || ResponseTemplate::new(200).set_delay(Duration::from_secs(2));
    for (nome, resposta, processado) in [
        ("500", ResponseTemplate::new(500), true),
        ("502", ResponseTemplate::new(502), true),
        ("503", ResponseTemplate::new(503), true),
        ("504", ResponseTemplate::new(504), true),
        ("200 ilegível", ilegivel(), true),
        ("tempo esgotado", atrasada(), true),
        ("400", ResponseTemplate::new(400), false),
        ("403", ResponseTemplate::new(403), false),
        ("422", ResponseTemplate::new(422), false),
        ("429", ResponseTemplate::new(429), false),
    ] {
        let server = setup("pagamento-boleto.write").await;
        Mock::given(method("POST"))
            .and(path(PAGAMENTO))
            .respond_with(resposta)
            .expect(1)
            .mount(&server)
            .await;
        let err = sem_retentativa(&server)
            .banking()
            .pagar_boleto(&boleto("30.10"))
            .await
            .unwrap_err();
        assert_eq!(err.may_have_been_processed(), processado, "{nome}: {err:?}");
    }
}

/// The token is asked for before the payment: when that fails, nothing was
/// sent, even if the failure looks like one of the payment's (a timeout, an
/// unreadable answer).
#[tokio::test]
async fn a_failed_token_request_sent_no_payment() {
    let ilegivel = || ResponseTemplate::new(200).set_body_string("<html>proxy</html>");
    let atrasada = || ResponseTemplate::new(200).set_delay(Duration::from_secs(2));
    for (nome, resposta) in [("200 ilegível", ilegivel()), ("tempo esgotado", atrasada())] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth/v2/token"))
            .respond_with(resposta)
            .mount(&server)
            .await;
        Mock::given(any())
            .and(path(PAGAMENTO))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&server)
            .await;
        let err = sem_retentativa(&server)
            .banking()
            .pagar_boleto(&boleto("30.10"))
            .await
            .unwrap_err();
        assert!(!err.may_have_been_processed(), "{nome}: {err:?}");
    }

    // Nobody listens: the token request fails to connect.
    let err = InterClient::builder()
        .base_url(format!("http://127.0.0.1:{}", porta_fechada()))
        .credentials(Credentials::new("id", "segredo"))
        .identity(common::identity())
        .retry_policy(RetryPolicy::disabled())
        .build()
        .unwrap()
        .banking()
        .pagar_boleto(&boleto("30.10"))
        .await
        .unwrap_err();
    assert!(matches!(err, Error::TokenTransport(_)), "{err:?}");
    assert!(!err.may_have_been_processed());
}

/// A `401` to the payment renews the token; when the renewal fails, the
/// payment was refused and not sent again.
#[tokio::test]
async fn a_failed_renewal_after_a_401_sent_no_payment() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth/v2/token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(common::token_body("tok", "pagamento-boleto.write")),
        )
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/oauth/v2/token"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>proxy</html>"))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(PAGAMENTO))
        .respond_with(ResponseTemplate::new(401))
        .expect(1)
        .mount(&server)
        .await;
    let err = sem_retentativa(&server)
        .banking()
        .pagar_boleto(&boleto("30.10"))
        .await
        .unwrap_err();
    assert!(!err.may_have_been_processed(), "{err:?}");
}

/// With a token at hand, a refused connection is the payment's own, and
/// it never reached the API.
#[tokio::test]
async fn a_refused_connection_sent_no_payment() {
    let base_url = format!("http://127.0.0.1:{}", porta_fechada());
    let store = Arc::new(Tokens::default());
    let client = InterClient::builder()
        .base_url(base_url)
        .credentials(Credentials::new("id", "segredo"))
        .identity(common::identity())
        .token_store(store.clone())
        .retry_policy(RetryPolicy::disabled())
        .build()
        .unwrap();
    *store.0.lock().unwrap() = vec![AccessToken::new(
        "tok",
        ScopeSet::from(Scope::PagamentoBoletoWrite),
        Utc::now() + TimeDelta::hours(1),
    )];
    let err = client
        .banking()
        .pagar_boleto(&boleto("30.10"))
        .await
        .unwrap_err();
    assert!(
        matches!(&err, Error::Transport(source) if source.is_connect()),
        "{err:?}"
    );
    assert!(!err.may_have_been_processed());
}

fn sem_retentativa(server: &MockServer) -> InterClient {
    common::builder(server)
        .retry_policy(RetryPolicy::disabled())
        .timeout(Duration::from_millis(300))
        .build()
        .unwrap()
}

/// A port nobody listens on.
fn porta_fechada() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

#[derive(Debug, Default)]
struct Tokens(Mutex<Vec<AccessToken>>);

impl TokenStore for Tokens {
    fn load(&self, _key: &str) -> io::Result<Vec<AccessToken>> {
        Ok(self.0.lock().unwrap().clone())
    }

    fn save(&self, _key: &str, tokens: &[AccessToken]) -> io::Result<()> {
        *self.0.lock().unwrap() = tokens.to_vec();
        Ok(())
    }
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
    assert_eq!(
        client.banking().pagamentos(&por_codigo).await.unwrap(),
        Vec::new()
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
