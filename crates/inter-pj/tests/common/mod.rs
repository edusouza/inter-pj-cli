//! Helpers shared by the integration tests. All data here is synthetic.

#![allow(dead_code, unreachable_pub)]

use std::time::Duration;

use inter_pj::{ClientIdentity, Credentials, InterClient, InterClientBuilder, RetryPolicy};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Match, Mock, MockServer, Request, ResponseTemplate};

pub const CLIENT_ID: &str = "client-id-de-teste";
pub const CLIENT_SECRET: &str = "client-secret-de-teste";

/// Self-signed certificate and key generated on the fly (nothing is versioned).
pub fn identity() -> ClientIdentity {
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec!["cliente.teste".to_owned()]).unwrap();
    ClientIdentity::from_pem(
        cert.pem().as_bytes(),
        signing_key.serialize_pem().as_bytes(),
    )
    .unwrap()
}

/// Default retry policy with millisecond delays, so retries do not slow tests.
pub fn fast_retries(attempts: u32) -> RetryPolicy {
    RetryPolicy::new(attempts)
        .initial_delay(Duration::from_millis(2))
        .max_delay(Duration::from_millis(10))
}

pub fn builder(server: &MockServer) -> InterClientBuilder {
    InterClient::builder()
        .base_url(server.uri())
        .credentials(Credentials::new(CLIENT_ID, CLIENT_SECRET))
        .identity(identity())
        .retry_policy(fast_retries(RetryPolicy::DEFAULT_ATTEMPTS))
}

pub fn client(server: &MockServer) -> InterClient {
    builder(server).build().unwrap()
}

pub fn token_body(token: &str, scope: &str) -> Value {
    json!({
        "access_token": token,
        "token_type": "Bearer",
        "expires_in": 3600,
        "scope": scope,
    })
}

/// Mounts `POST /oauth/v2/token` answering `token` and expecting `calls` calls.
pub async fn mount_token(server: &MockServer, token: &str, scope: &str, calls: u64) {
    Mock::given(method("POST"))
        .and(path("/oauth/v2/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(token_body(token, scope)))
        .expect(calls)
        .mount(server)
        .await;
}

pub fn saldo_body() -> Value {
    json!({
        "bloqueadoCheque": 240.25,
        "disponivel": 2850.55,
        "bloqueadoJudicialmente": 510.35,
        "bloqueadoAdministrativo": 0,
        "limite": 1000.00,
    })
}

/// Matches `application/x-www-form-urlencoded` bodies with exactly these fields.
pub struct FormEquals(pub Vec<(&'static str, &'static str)>);

impl Match for FormEquals {
    fn matches(&self, request: &Request) -> bool {
        let Ok(mut got) = serde_urlencoded::from_bytes::<Vec<(String, String)>>(&request.body)
        else {
            return false;
        };
        let mut expected: Vec<(String, String)> = self
            .0
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        got.sort();
        expected.sort();
        got == expected
    }
}

/// Matches requests that do **not** carry the header.
pub struct NoHeader(pub &'static str);

impl Match for NoHeader {
    fn matches(&self, request: &Request) -> bool {
        !request.headers.contains_key(self.0)
    }
}
