//! Client behaviour against a mock Inter API: token requests, token reuse and
//! renewal, headers, balance and error mapping.

mod common;

use std::io;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use chrono::NaiveDate;
use common::{
    CLIENT_ID, CLIENT_SECRET, FormEquals, NoHeader, builder, client, mount_token, saldo_body,
    token_body,
};
use inter_pj::{AccessToken, ApiErrorKind, Error, Scope, ScopeSet, TokenStore};
use rust_decimal::Decimal;
use secrecy::ExposeSecret;
use serde_json::json;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn dec(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap()
}

#[tokio::test]
async fn token_request_uses_client_credentials_with_minimum_scope() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth/v2/token"))
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(FormEquals(vec![
            ("client_id", CLIENT_ID),
            ("client_secret", CLIENT_SECRET),
            ("grant_type", "client_credentials"),
            ("scope", "extrato.read"),
        ]))
        .respond_with(ResponseTemplate::new(200).set_body_json(token_body("tok-1", "extrato.read")))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/banking/v2/saldo"))
        .and(header("authorization", "Bearer tok-1"))
        .and(header("accept", "application/json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(saldo_body()))
        .expect(1)
        .mount(&server)
        .await;

    let saldo = client(&server).banking().saldo(None).await.unwrap();

    assert_eq!(saldo.disponivel, Some(dec("2850.55")));
    assert_eq!(saldo.bloqueado_cheque, Some(dec("240.25")));
    assert_eq!(saldo.bloqueado_judicialmente, Some(dec("510.35")));
    assert_eq!(saldo.bloqueado_administrativo, Some(dec("0")));
    assert_eq!(saldo.limite, Some(dec("1000")));
    assert_eq!(saldo.data_referencia, None);
}

#[tokio::test]
async fn saldo_for_a_date_sends_query_parameter() {
    let server = MockServer::start().await;
    mount_token(&server, "tok", "extrato.read", 1).await;
    Mock::given(method("GET"))
        .and(path("/banking/v2/saldo"))
        .and(query_param("dataSaldo", "2026-01-02"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"disponivel": 10.5, "dataReferencia": "02/01/2026"})),
        )
        .expect(1)
        .mount(&server)
        .await;

    let data = NaiveDate::from_ymd_opt(2026, 1, 2).unwrap();
    let saldo = client(&server).banking().saldo(Some(data)).await.unwrap();

    assert_eq!(saldo.disponivel, Some(dec("10.5")));
    assert_eq!(saldo.limite, None);
    assert_eq!(saldo.data_referencia.as_deref(), Some("02/01/2026"));
}

#[tokio::test]
async fn reuses_token_between_calls() {
    let server = MockServer::start().await;
    mount_token(&server, "tok", "extrato.read", 1).await;
    Mock::given(method("GET"))
        .and(path("/banking/v2/saldo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(saldo_body()))
        .expect(3)
        .mount(&server)
        .await;

    let client = client(&server);
    for _ in 0..3 {
        client.banking().saldo(None).await.unwrap();
    }
}

#[tokio::test]
async fn renews_token_once_after_401() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth/v2/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(token_body("velho", "extrato.read")))
        .up_to_n_times(1)
        .expect(1)
        .with_priority(1)
        .mount(&server)
        .await;
    mount_token(&server, "novo", "extrato.read", 1).await;
    Mock::given(method("GET"))
        .and(path("/banking/v2/saldo"))
        .and(header("authorization", "Bearer velho"))
        .respond_with(ResponseTemplate::new(401))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/banking/v2/saldo"))
        .and(header("authorization", "Bearer novo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(saldo_body()))
        .expect(1)
        .mount(&server)
        .await;

    let saldo = client(&server).banking().saldo(None).await.unwrap();
    assert_eq!(saldo.disponivel, Some(dec("2850.55")));
}

#[tokio::test]
async fn gives_up_after_second_401() {
    let server = MockServer::start().await;
    mount_token(&server, "tok", "extrato.read", 2).await;
    Mock::given(method("GET"))
        .and(path("/banking/v2/saldo"))
        .respond_with(ResponseTemplate::new(401))
        .expect(2)
        .mount(&server)
        .await;

    let err = client(&server).banking().saldo(None).await.unwrap_err();
    let Error::Api(api) = err else {
        panic!("esperado Error::Api, obtido {err:?}");
    };
    assert_eq!(api.kind(), ApiErrorKind::Unauthorized);
}

#[tokio::test]
async fn reports_scopes_the_token_did_not_receive() {
    let server = MockServer::start().await;
    mount_token(&server, "tok", "pix.read", 1).await;

    let err = client(&server).banking().saldo(None).await.unwrap_err();
    let Error::Auth(api) = &err else {
        panic!("esperado Error::Auth, obtido {err:?}");
    };
    assert!(
        api.note.as_deref().unwrap().contains("extrato.read"),
        "{err}"
    );
}

#[tokio::test]
async fn token_endpoint_rejection_is_an_auth_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth/v2/token"))
        .respond_with(ResponseTemplate::new(401).set_body_json(
            json!({"error": "invalid_client", "error_description": "Client authentication failed"}),
        ))
        .mount(&server)
        .await;

    let err = client(&server).banking().saldo(None).await.unwrap_err();
    let Error::Auth(api) = &err else {
        panic!("esperado Error::Auth, obtido {err:?}");
    };
    assert_eq!(api.status, 401);
    let message = err.to_string();
    assert!(message.contains("invalid_client"), "{message}");
    assert!(!message.contains(CLIENT_SECRET), "{message}");
}

#[tokio::test]
async fn api_error_keeps_problem_details() {
    let server = MockServer::start().await;
    mount_token(&server, "tok", "extrato.read", 1).await;
    Mock::given(method("GET"))
        .and(path("/banking/v2/saldo"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "title": "Dados inválidos.",
            "detail": "Verifique se os dados informados estão de acordo com a documentação.",
            "violacoes": [{"razao": "Não foi possível converter o valor.", "propriedade": "dataSaldo"}]
        })))
        .mount(&server)
        .await;

    let err = client(&server).banking().saldo(None).await.unwrap_err();
    let Error::Api(api) = &err else {
        panic!("esperado Error::Api, obtido {err:?}");
    };
    assert_eq!(api.kind(), ApiErrorKind::InvalidRequest);
    assert_eq!(api.operation, "GET /banking/v2/saldo");
    let problem = api.problem.as_ref().unwrap();
    assert_eq!(
        problem.violacoes[0].propriedade.as_deref(),
        Some("dataSaldo")
    );
    assert!(
        err.to_string()
            .contains("dataSaldo: Não foi possível converter o valor.")
    );
}

#[tokio::test]
async fn maps_unavailable_and_rate_limited() {
    for (status, kind) in [
        (503, ApiErrorKind::Unavailable),
        (429, ApiErrorKind::RateLimited),
    ] {
        let server = MockServer::start().await;
        mount_token(&server, "tok", "extrato.read", 1).await;
        Mock::given(method("GET"))
            .and(path("/banking/v2/saldo"))
            .respond_with(ResponseTemplate::new(status).set_body_string("indisponível"))
            .mount(&server)
            .await;

        let err = client(&server).banking().saldo(None).await.unwrap_err();
        let Error::Api(api) = &err else {
            panic!("esperado Error::Api, obtido {err:?}");
        };
        assert_eq!(api.kind(), kind);
        assert_eq!(api.body_excerpt.as_deref(), Some("indisponível"));
    }
}

#[tokio::test]
async fn sends_conta_corrente_header_when_configured() {
    let server = MockServer::start().await;
    mount_token(&server, "tok", "extrato.read", 1).await;
    Mock::given(method("GET"))
        .and(path("/banking/v2/saldo"))
        .and(header("x-conta-corrente", "1234567"))
        .respond_with(ResponseTemplate::new(200).set_body_json(saldo_body()))
        .expect(1)
        .mount(&server)
        .await;

    let client = builder(&server).conta_corrente("1234567").build().unwrap();
    client.banking().saldo(None).await.unwrap();
}

#[tokio::test]
async fn omits_conta_corrente_header_by_default() {
    let server = MockServer::start().await;
    mount_token(&server, "tok", "extrato.read", 1).await;
    Mock::given(method("GET"))
        .and(path("/banking/v2/saldo"))
        .and(NoHeader("x-conta-corrente"))
        .respond_with(ResponseTemplate::new(200).set_body_json(saldo_body()))
        .expect(1)
        .mount(&server)
        .await;

    client(&server).banking().saldo(None).await.unwrap();
}

#[tokio::test]
async fn invalid_json_is_a_decode_error() {
    let server = MockServer::start().await;
    mount_token(&server, "tok", "extrato.read", 1).await;
    Mock::given(method("GET"))
        .and(path("/banking/v2/saldo"))
        .respond_with(ResponseTemplate::new(200).set_body_string("isto não é json"))
        .mount(&server)
        .await;

    let err = client(&server).banking().saldo(None).await.unwrap_err();
    assert!(matches!(err, Error::Decode { .. }), "{err:?}");
}

#[tokio::test]
async fn malformed_token_response_does_not_leak_its_content() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth/v2/token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"{"access_token": 987654321, "scope": "extrato.read"}"#),
        )
        .mount(&server)
        .await;

    let err = client(&server).banking().saldo(None).await.unwrap_err();
    assert!(matches!(err, Error::Decode { .. }), "{err:?}");
    assert!(!err.to_string().contains("987654321"), "{err}");
}

#[tokio::test]
async fn access_token_is_cached_and_can_be_renewed() {
    let server = MockServer::start().await;
    mount_token(&server, "tok", "extrato.read pix.read", 2).await;
    let client = client(&server);
    let scopes: ScopeSet = [Scope::ExtratoRead, Scope::PixRead].into_iter().collect();

    let first = client.access_token(&scopes).await.unwrap();
    assert_eq!(first.scopes(), &scopes);
    let cached = client
        .access_token(&ScopeSet::from(Scope::PixRead))
        .await
        .unwrap();
    assert_eq!(
        cached.secret().expose_secret(),
        first.secret().expose_secret()
    );
    client.renew_access_token(&scopes).await.unwrap();
}

#[derive(Debug, Default)]
struct SharedStore(Mutex<Vec<AccessToken>>);

impl TokenStore for SharedStore {
    fn load(&self, _key: &str) -> io::Result<Vec<AccessToken>> {
        Ok(self.0.lock().unwrap().clone())
    }

    fn save(&self, _key: &str, tokens: &[AccessToken]) -> io::Result<()> {
        *self.0.lock().unwrap() = tokens.to_vec();
        Ok(())
    }
}

#[tokio::test]
async fn token_store_is_shared_between_clients() {
    let server = MockServer::start().await;
    mount_token(&server, "persistido", "extrato.read", 1).await;
    Mock::given(method("GET"))
        .and(path("/banking/v2/saldo"))
        .and(header("authorization", "Bearer persistido"))
        .respond_with(ResponseTemplate::new(200).set_body_json(saldo_body()))
        .expect(2)
        .mount(&server)
        .await;

    let store = Arc::new(SharedStore::default());
    for _ in 0..2 {
        let client = builder(&server).token_store(store.clone()).build().unwrap();
        client.banking().saldo(None).await.unwrap();
    }
    assert_eq!(store.0.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn additional_scopes_are_requested_up_front() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth/v2/token"))
        .and(FormEquals(vec![
            ("client_id", CLIENT_ID),
            ("client_secret", CLIENT_SECRET),
            ("grant_type", "client_credentials"),
            ("scope", "extrato.read pix.read"),
        ]))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(token_body("tok", "extrato.read pix.read")),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/banking/v2/saldo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(saldo_body()))
        .mount(&server)
        .await;

    let client = builder(&server)
        .additional_scopes(ScopeSet::from(Scope::PixRead))
        .build()
        .unwrap();
    client.banking().saldo(None).await.unwrap();
    client
        .access_token(&ScopeSet::from(Scope::PixRead))
        .await
        .unwrap();
}

#[test]
fn builder_requires_environment_credentials_and_identity() {
    let err = inter_pj::InterClient::builder().build().unwrap_err();
    assert!(err.to_string().contains("ambiente"), "{err}");

    let err = inter_pj::InterClient::builder()
        .environment(inter_pj::Environment::Sandbox)
        .build()
        .unwrap_err();
    assert!(err.to_string().contains("credenciais"), "{err}");

    let err = inter_pj::InterClient::builder()
        .environment(inter_pj::Environment::Sandbox)
        .credentials(inter_pj::Credentials::new("id", "segredo"))
        .build()
        .unwrap_err();
    assert!(err.to_string().contains("certificado"), "{err}");
}

#[test]
fn environment_selects_official_base_url() {
    let client = inter_pj::InterClient::builder()
        .environment(inter_pj::Environment::Production)
        .credentials(inter_pj::Credentials::new("id", "segredo"))
        .identity(common::identity())
        .build()
        .unwrap();
    assert_eq!(client.base_url(), "https://cdpj.partners.bancointer.com.br");
    assert_eq!(
        client.environment(),
        Some(inter_pj::Environment::Production)
    );
    assert_eq!(
        client.token_cache_key(),
        inter_pj::auth::cache_key("https://cdpj.partners.bancointer.com.br", "id")
    );
}
