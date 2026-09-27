//! The webhooks of Pix Automático (`/pix/v2/webhookrec`,
//! `/pix/v2/webhookcobr`) and its sandbox (`/pix/v2/sandbox`) against a mock
//! API. All data is synthetic.

mod common;

use common::{builder, client, mount_token};
use inter_pj::pix::Txid;
use inter_pj::pix_automatico::{
    IdRec, RazaoCancelamentoCobR, RazaoCancelamentoRec, StatusRec, StatusSolicRec,
    TipoWebhookPixAutomatico,
};
use inter_pj::webhook::WebhookUrl;
use inter_pj::{Environment, Error, InterClient};
use serde_json::json;
use wiremock::matchers::{any, body_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ID_REC: &str = "RR1234567820260924abcdefghijk";
const TXID: &str = "7978c0c97ea847e78e8849634473c1f1";

fn id_rec() -> IdRec {
    ID_REC.parse().unwrap()
}

fn txid() -> Txid {
    TXID.parse().unwrap()
}

#[tokio::test]
async fn registers_looks_up_and_removes_both_webhooks() {
    let server = MockServer::start().await;
    mount_token(
        &server,
        "tok",
        "webhookrec.read webhookrec.write webhookcobr.read webhookcobr.write",
        1,
    )
    .await;
    for recurso in ["webhookrec", "webhookcobr"] {
        let caminho = format!("/pix/v2/{recurso}");
        let url = format!("https://api.empresa.example/{recurso}");
        Mock::given(method("PUT"))
            .and(path(caminho.as_str()))
            .and(body_json(json!({ "webhookUrl": url })))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(caminho.as_str()))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(
                    json!({"webhookUrl": url, "criacao": "2026-09-24T12:00:00.000Z"}),
                ),
            )
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("DELETE"))
            .and(path(caminho.as_str()))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
    }

    let pix = client(&server);
    let pix = pix.pix_automatico();
    for tipo in TipoWebhookPixAutomatico::TODOS {
        let url = WebhookUrl::parse(&format!("https://api.empresa.example/{tipo}")).unwrap();
        pix.cadastrar_webhook(tipo, &url).await.unwrap();
        let webhook = pix.consultar_webhook(tipo).await.unwrap().unwrap();
        assert_eq!(webhook.webhook_url.as_deref(), Some(url.as_str()));
        pix.excluir_webhook(tipo).await.unwrap();
    }
}

#[tokio::test]
async fn a_missing_webhook_is_none() {
    let server = MockServer::start().await;
    mount_token(&server, "tok", "webhookcobr.read", 1).await;
    Mock::given(method("GET"))
        .and(path("/pix/v2/webhookcobr"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({
            "type": "https://pix.bcb.gov.br/api/v2/error/NaoEncontrado",
            "title": "Não Encontrado",
            "status": 404,
            "detail": "Entidade não encontrada."
        })))
        .expect(1)
        .mount(&server)
        .await;
    let webhook = client(&server)
        .pix_automatico()
        .consultar_webhook(TipoWebhookPixAutomatico::CobrancaRecorrente)
        .await
        .unwrap();
    assert_eq!(webhook, None);
}

fn sandbox(server: &MockServer) -> InterClient {
    builder(server)
        .environment(Environment::Sandbox)
        .build()
        .unwrap()
}

#[tokio::test]
async fn simulates_the_payer_in_the_sandbox() {
    let server = MockServer::start().await;
    mount_token(&server, "tok", "pix.write solicrec.write cobr.write", 1).await;
    Mock::given(method("PATCH"))
        .and(path(format!("/pix/v2/sandbox/solicrec/{ID_REC}/status")))
        .and(body_json(json!({"status": "ACEITA"})))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path(format!("/pix/v2/sandbox/rec/{ID_REC}/status")))
        .and(body_json(json!({"status": "APROVADA"})))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path(format!("/pix/v2/sandbox/rec/{ID_REC}/status")))
        .and(body_json(json!({"status": "CANCELADA", "razao": "SLCR"})))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/pix/v2/sandbox/cobr/pagamento"))
        .and(body_json(json!({
            "valor": 149.9,
            "cpfCnpj": "12345678909",
            "txId": TXID,
            "chave": "pix@empresa.example"
        })))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"endToEnd": "E00416968202609241552abcdef12345"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path(format!("/pix/v2/sandbox/cobr/{TXID}/status")))
        .and(body_json(
            json!({"status": "CANCELADA", "razao": "REQUESTED_BY_PAYER"}),
        ))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    let pix = sandbox(&server);
    let pix = pix.pix_automatico();
    pix.alterar_status_solicitacao_no_sandbox(&id_rec(), &StatusSolicRec::Aceita)
        .await
        .unwrap();
    pix.alterar_status_rec_no_sandbox(&id_rec(), &StatusRec::Aprovada, None)
        .await
        .unwrap();
    let pago = pix
        .pagar_cobr_no_sandbox(
            &txid(),
            "149.90".parse().unwrap(),
            &"123.456.789-09".parse().unwrap(),
            &"pix@empresa.example".parse().unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        pago.end_to_end_id(),
        Some("E00416968202609241552abcdef12345")
    );
    pix.cancelar_cobr_no_sandbox(&txid(), &RazaoCancelamentoCobR::RequestedByPayer)
        .await
        .unwrap();
    pix.alterar_status_rec_no_sandbox(
        &id_rec(),
        &StatusRec::Cancelada,
        Some(&RazaoCancelamentoRec::Slcr),
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn nothing_is_simulated_outside_the_sandbox_nor_out_of_the_rules() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let invalido = |err: Error| assert!(matches!(err, Error::InvalidInput(_)), "{err}");

    // Outside the sandbox.
    let producao = builder(&server)
        .environment(Environment::Production)
        .build()
        .unwrap();
    let pix = producao.pix_automatico();
    invalido(
        pix.alterar_status_rec_no_sandbox(&id_rec(), &StatusRec::Aprovada, None)
            .await
            .unwrap_err(),
    );
    invalido(
        pix.cancelar_cobr_no_sandbox(&txid(), &RazaoCancelamentoCobR::Other)
            .await
            .unwrap_err(),
    );

    // Statuses the sandbox does not offer, a reason without a cancellation
    // and an amount that is not positive.
    let sandbox = sandbox(&server);
    let pix = sandbox.pix_automatico();
    invalido(
        pix.alterar_status_rec_no_sandbox(&id_rec(), &StatusRec::Expirada, None)
            .await
            .unwrap_err(),
    );
    invalido(
        pix.alterar_status_rec_no_sandbox(
            &id_rec(),
            &StatusRec::Aprovada,
            Some(&RazaoCancelamentoRec::Slcr),
        )
        .await
        .unwrap_err(),
    );
    invalido(
        pix.alterar_status_solicitacao_no_sandbox(&id_rec(), &StatusSolicRec::Enviada)
            .await
            .unwrap_err(),
    );
    invalido(
        pix.pagar_cobr_no_sandbox(
            &txid(),
            "0".parse().unwrap(),
            &"123.456.789-09".parse().unwrap(),
            &"pix@empresa.example".parse().unwrap(),
        )
        .await
        .unwrap_err(),
    );
}
