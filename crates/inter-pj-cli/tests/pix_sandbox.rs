//! `inter-pj pix cob pagar`, `pix cobv pagar` and `pix sandbox
//! pagar-qrcode` end to end, against a mock API: payments of the sandbox,
//! refused in production before any request. All data is synthetic; the
//! "copia e cola" is the example of the Banco Central's manual.

mod common;

use common::{COPIA_E_COLA, TestEnv, stderr_of, stdout_of};
use serde_json::{Value, json};
use wiremock::matchers::{any, body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

const TXID: &str = "7978c0c97ea847e78e8849634473c1f1";
const E2E: &str = "E00416968202609241310abcdEFGH123";

async fn env() -> TestEnv {
    let env = TestEnv::new().await;
    env.write_config("");
    env
}

/// The API must not be called at all.
async fn nothing_is_sent(env: &TestEnv) {
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&env.server)
        .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn paga_uma_cobranca_imediata_pelo_seu_valor() {
    let env = env().await;
    env.mount_token("cob.read pix.write", None).await;
    Mock::given(method("GET"))
        .and(path(format!("/pix/v2/cob/{TXID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "txid": TXID, "status": "ATIVA", "valor": {"original": "37.00"}
        })))
        .expect(1)
        .mount(&env.server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/pix/v2/cob/pagar/{TXID}")))
        .and(body_json(json!({"valor": 37})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"e2e": E2E})))
        .expect(1)
        .mount(&env.server)
        .await;
    let assert = env
        .cmd()
        .args(["pix", "cob", "pagar", TXID])
        .assert()
        .success();
    assert_eq!(
        stdout_of(&assert),
        format!(
            "Pago no sandbox: R$ 37,00.\nendToEndId  {E2E}\n\nConfira com: inter-pj pix cob consultar {TXID}\n"
        )
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn paga_uma_cobranca_com_vencimento_com_outro_valor() {
    let env = env().await;
    env.mount_token("pix.write", None).await;
    Mock::given(method("POST"))
        .and(path(format!("/pix/v2/cobv/pagar/{TXID}")))
        .and(body_json(json!({"valor": 160.5})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"e2e": E2E})))
        .expect(1)
        .mount(&env.server)
        .await;
    let assert = env
        .cmd()
        .args(["pix", "cobv", "pagar", TXID, "--valor", "160,50", "--json"])
        .assert()
        .success();
    let json: Value = serde_json::from_str(&stdout_of(&assert)).unwrap();
    assert_eq!(json, json!({"endToEndId": E2E, "valor": "160.50"}));
}

#[tokio::test(flavor = "multi_thread")]
async fn paga_um_copia_e_cola() {
    let env = env().await;
    env.mount_token("pix.write", None).await;
    Mock::given(method("POST"))
        .and(path("/pix/v2/sandbox/cob/pagamento"))
        .and(body_json(json!({"qrCode": COPIA_E_COLA, "valor": 10})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"endToEnd": E2E})))
        .expect(1)
        .mount(&env.server)
        .await;
    // The example of the manual has no amount: --valor is required.
    let assert = env
        .cmd()
        .args([
            "pix",
            "sandbox",
            "pagar-qrcode",
            "--copia-e-cola",
            COPIA_E_COLA,
        ])
        .assert()
        .code(2);
    assert!(
        stderr_of(&assert).contains("o código copia e cola não traz o valor: informe --valor"),
        "{}",
        stderr_of(&assert)
    );
    let assert = env
        .cmd()
        .args([
            "pix",
            "sandbox",
            "pagar-qrcode",
            "--copia-e-cola",
            COPIA_E_COLA,
            "--valor",
            "10",
        ])
        .assert()
        .success();
    assert!(
        stdout_of(&assert).starts_with(&format!("Pago no sandbox: R$ 10,00.\nendToEndId  {E2E}\n")),
        "{}",
        stdout_of(&assert)
    );
    // A code with a bad checksum is refused before anything else.
    let estragado = COPIA_E_COLA.replace("1D3D", "0000");
    env.cmd()
        .args([
            "pix",
            "sandbox",
            "pagar-qrcode",
            "--copia-e-cola",
            &estragado,
            "--valor",
            "1",
        ])
        .assert()
        .code(2);
}

/// #39's acceptance criterion: in production, refused with no HTTP call,
/// not even for a token.
#[tokio::test(flavor = "multi_thread")]
async fn em_producao_nada_e_enviado() {
    let env = env().await;
    nothing_is_sent(&env).await;
    for (args, comando) in [
        (
            &["pix", "cob", "pagar", TXID, "--valor", "1"][..],
            "pix cob pagar",
        ),
        (&["pix", "cobv", "pagar", TXID][..], "pix cobv pagar"),
        (
            &[
                "pix",
                "sandbox",
                "pagar-qrcode",
                "--copia-e-cola",
                COPIA_E_COLA,
                "--valor",
                "1",
            ][..],
            "pix sandbox pagar-qrcode",
        ),
    ] {
        let assert = env
            .cmd()
            .args(["--ambiente", "producao"])
            .args(args)
            .assert()
            .code(2);
        assert!(
            stderr_of(&assert).contains(&format!("{comando} existe só no sandbox")),
            "{}",
            stderr_of(&assert)
        );
    }
}
