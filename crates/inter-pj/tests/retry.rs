//! Retries of idempotent requests (issue #17), against a mock API.

mod common;

use std::time::{Duration, Instant};

use common::{builder, client, fast_retries, mount_token, saldo_body, token_body};
use inter_pj::{ApiErrorKind, Error, InterClient, RetryPolicy};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn mount_saldo(server: &MockServer, response: ResponseTemplate, times: u64) {
    Mock::given(method("GET"))
        .and(path("/banking/v2/saldo"))
        .respond_with(response)
        .up_to_n_times(times)
        .expect(times)
        .mount(server)
        .await;
}

fn api_status(err: &Error) -> u16 {
    match err {
        Error::Api(api) => api.status,
        other => panic!("esperado Error::Api, obtido {other:?}"),
    }
}

#[tokio::test]
async fn rate_limited_get_is_retried_until_it_succeeds() {
    let server = MockServer::start().await;
    mount_token(&server, "tok", "extrato.read", 1).await;
    mount_saldo(&server, ResponseTemplate::new(429), 2).await;
    mount_saldo(&server, ResponseTemplate::new(200).set_body_json(saldo_body()), 1).await;

    let saldo = client(&server).banking().saldo(None).await.unwrap();
    assert!(saldo.disponivel.is_some());
}

#[tokio::test]
async fn transient_server_errors_are_retried() {
    let server = MockServer::start().await;
    mount_token(&server, "tok", "extrato.read", 1).await;
    mount_saldo(&server, ResponseTemplate::new(503), 1).await;
    mount_saldo(&server, ResponseTemplate::new(502), 1).await;
    mount_saldo(&server, ResponseTemplate::new(200).set_body_json(saldo_body()), 1).await;

    client(&server).banking().saldo(None).await.unwrap();
}

#[tokio::test]
async fn gives_up_after_the_last_attempt() {
    let server = MockServer::start().await;
    mount_token(&server, "tok", "extrato.read", 1).await;
    mount_saldo(&server, ResponseTemplate::new(503), 4).await;

    let client = builder(&server).retry_policy(fast_retries(4)).build().unwrap();
    let err = client.banking().saldo(None).await.unwrap_err();
    assert_eq!(api_status(&err), 503);
}

#[tokio::test]
async fn disabled_policy_makes_a_single_attempt() {
    let server = MockServer::start().await;
    mount_token(&server, "tok", "extrato.read", 1).await;
    mount_saldo(&server, ResponseTemplate::new(429), 1).await;

    let client = builder(&server)
        .retry_policy(RetryPolicy::disabled())
        .build()
        .unwrap();
    let err = client.banking().saldo(None).await.unwrap_err();
    let Error::Api(api) = &err else {
        panic!("{err:?}");
    };
    assert_eq!(api.kind(), ApiErrorKind::RateLimited);
}

#[tokio::test]
async fn client_errors_are_not_retried() {
    for status in [400, 403, 404] {
        let server = MockServer::start().await;
        mount_token(&server, "tok", "extrato.read", 1).await;
        mount_saldo(&server, ResponseTemplate::new(status), 1).await;

        let err = client(&server).banking().saldo(None).await.unwrap_err();
        assert_eq!(api_status(&err), status);
    }
}

#[tokio::test]
async fn retry_after_is_honoured() {
    let server = MockServer::start().await;
    mount_token(&server, "tok", "extrato.read", 1).await;
    mount_saldo(
        &server,
        ResponseTemplate::new(429).insert_header("retry-after", "1"),
        1,
    )
    .await;
    mount_saldo(&server, ResponseTemplate::new(200).set_body_json(saldo_body()), 1).await;

    let client = builder(&server)
        .retry_policy(
            RetryPolicy::new(2)
                .initial_delay(Duration::from_millis(1))
                .max_delay(Duration::from_secs(5)),
        )
        .build()
        .unwrap();
    let started = Instant::now();
    client.banking().saldo(None).await.unwrap();
    assert!(started.elapsed() >= Duration::from_secs(1), "{:?}", started.elapsed());
}

#[tokio::test]
async fn retry_after_longer_than_max_delay_fails_at_once() {
    let server = MockServer::start().await;
    mount_token(&server, "tok", "extrato.read", 1).await;
    mount_saldo(
        &server,
        ResponseTemplate::new(429).insert_header("retry-after", "120"),
        1,
    )
    .await;

    let client = builder(&server)
        .retry_policy(RetryPolicy::new(3).max_delay(Duration::from_secs(5)))
        .build()
        .unwrap();
    let started = Instant::now();
    let err = client.banking().saldo(None).await.unwrap_err();
    assert_eq!(api_status(&err), 429);
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[tokio::test]
async fn token_request_is_retried() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth/v2/token"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    mount_token(&server, "tok", "extrato.read", 1).await;
    mount_saldo(&server, ResponseTemplate::new(200).set_body_json(saldo_body()), 1).await;

    client(&server).banking().saldo(None).await.unwrap();
}

#[tokio::test]
async fn rejected_credentials_are_not_retried() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth/v2/token"))
        .respond_with(ResponseTemplate::new(401).set_body_json(token_body("x", "y")))
        .expect(1)
        .mount(&server)
        .await;

    let err = client(&server).banking().saldo(None).await.unwrap_err();
    assert!(matches!(err, Error::Auth(_)), "{err:?}");
}

#[tokio::test]
async fn connection_failures_are_retried() {
    // A port nobody listens on: every attempt fails to connect.
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let client = InterClient::builder()
        .base_url(format!("http://127.0.0.1:{port}"))
        .credentials(inter_pj::Credentials::new("id", "segredo"))
        .identity(common::identity())
        .retry_policy(
            RetryPolicy::new(3)
                .initial_delay(Duration::from_millis(40))
                .max_delay(Duration::from_millis(40)),
        )
        .build()
        .unwrap();

    let started = Instant::now();
    let err = client.banking().saldo(None).await.unwrap_err();
    assert!(matches!(err, Error::Transport(_)), "{err:?}");
    // Two retries, each waiting at least half of the 40 ms delay.
    assert!(started.elapsed() >= Duration::from_millis(40), "{:?}", started.elapsed());
}
