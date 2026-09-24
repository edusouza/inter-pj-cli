//! Webhooks of the Banking, Cobrança and Pix APIs against a mock API. All
//! data is synthetic.

mod common;

use common::{client, mount_token};
use inter_pj::pix::ChavePix;
use inter_pj::webhook::{TipoWebhookBanking, WebhookUrl};
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const URL: &str = "https://api.empresa.example/inter/webhook";

async fn setup(scope: &str) -> MockServer {
    let server = MockServer::start().await;
    mount_token(&server, "tok", scope, 1).await;
    server
}

fn url() -> WebhookUrl {
    URL.parse().unwrap()
}

#[tokio::test]
async fn banking_has_one_webhook_per_kind() {
    let server = setup("webhook-banking.write webhook-banking.read").await;
    for tipo in ["pix-pagamento", "boleto-pagamento"] {
        Mock::given(method("PUT"))
            .and(path(format!("/banking/v2/webhooks/{tipo}")))
            .and(body_json(json!({"webhookUrl": URL})))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("DELETE"))
            .and(path(format!("/banking/v2/webhooks/{tipo}")))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path("/banking/v2/webhooks/pix-pagamento"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "webhookUrl": URL, "criacao": "2026-09-24T10:15:00.123Z"
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/banking/v2/webhooks/boleto-pagamento"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({
            "title": "Webhook não encontrado", "detail": "Não há webhook cadastrado."
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = client(&server);
    let banking = client.banking();
    for tipo in TipoWebhookBanking::TODOS {
        banking.cadastrar_webhook(tipo, &url()).await.unwrap();
    }
    let webhook = banking
        .consultar_webhook(TipoWebhookBanking::PixPagamento)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(webhook.webhook_url.as_deref(), Some(URL));
    assert_eq!(webhook.criacao.as_deref(), Some("2026-09-24T10:15:00.123Z"));
    assert_eq!(
        banking
            .consultar_webhook(TipoWebhookBanking::BoletoPagamento)
            .await
            .unwrap(),
        None
    );
    for tipo in TipoWebhookBanking::TODOS {
        banking.excluir_webhook(tipo).await.unwrap();
    }
}

#[tokio::test]
async fn cobranca_has_one_webhook() {
    let server = setup("boleto-cobranca.write boleto-cobranca.read").await;
    Mock::given(method("PUT"))
        .and(path("/cobranca/v3/cobrancas/webhook"))
        .and(body_json(json!({"webhookUrl": URL})))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/cobranca/v3/cobrancas/webhook"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "webhookUrl": URL,
            "criacao": "2026-09-01T09:00:00-03:00",
            "atualizacao": "2026-09-24T10:15:00-03:00"
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/cobranca/v3/cobrancas/webhook"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    let client = client(&server);
    let cobranca = client.cobranca();
    cobranca.cadastrar_webhook(&url()).await.unwrap();
    let webhook = cobranca.consultar_webhook().await.unwrap().unwrap();
    assert_eq!(
        webhook.atualizacao.as_deref(),
        Some("2026-09-24T10:15:00-03:00")
    );
    cobranca.excluir_webhook().await.unwrap();
}

#[tokio::test]
async fn a_cobranca_without_webhook_is_none_but_other_errors_are_errors() {
    let server = setup("boleto-cobranca.read").await;
    Mock::given(method("GET"))
        .and(path("/cobranca/v3/cobrancas/webhook"))
        .respond_with(ResponseTemplate::new(404))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/cobranca/v3/cobrancas/webhook"))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({
            "title": "Acesso negado", "detail": "Escopo não habilitado."
        })))
        .mount(&server)
        .await;
    let client = client(&server);
    assert_eq!(client.cobranca().consultar_webhook().await.unwrap(), None);
    let err = client.cobranca().consultar_webhook().await.unwrap_err();
    assert!(err.to_string().contains("403"), "{err}");
}

#[tokio::test]
async fn pix_has_one_webhook_per_key_and_phones_go_without_the_plus_sign() {
    let server = setup("webhook.write webhook.read").await;
    for chave in ["pix@empresa.example", "5511912345678"] {
        Mock::given(method("PUT"))
            .and(path(format!("/pix/v2/webhook/{chave}")))
            .and(body_json(json!({"webhookUrl": URL})))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/pix/v2/webhook/{chave}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "webhookUrl": URL, "chave": chave, "criacao": "2026-09-24T13:15:00.358Z"
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("DELETE"))
            .and(path(format!("/pix/v2/webhook/{chave}")))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
    }

    let client = client(&server);
    let pix = client.pix();
    for chave in ["pix@empresa.example", "+55 (11) 91234-5678"] {
        let chave: ChavePix = chave.parse().unwrap();
        pix.cadastrar_webhook(&chave, &url()).await.unwrap();
        let webhook = pix.consultar_webhook(&chave).await.unwrap().unwrap();
        assert_eq!(webhook.webhook_url.as_deref(), Some(URL));
        pix.excluir_webhook(&chave).await.unwrap();
    }
}

#[tokio::test]
async fn an_uncertain_registration_is_not_repeated() {
    let server = setup("webhook.write").await;
    Mock::given(method("PUT"))
        .and(path("/pix/v2/webhook/pix@empresa.example"))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&server)
        .await;
    let client = common::builder(&server)
        .retry_policy(common::fast_retries(3))
        .build()
        .unwrap();
    let chave: ChavePix = "pix@empresa.example".parse().unwrap();
    assert!(
        client
            .pix()
            .cadastrar_webhook(&chave, &url())
            .await
            .is_err()
    );
}
