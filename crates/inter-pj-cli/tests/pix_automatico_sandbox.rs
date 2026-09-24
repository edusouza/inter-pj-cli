//! `inter-pj pix-automatico sandbox ...` end to end, against a mock API: the
//! answers of the payer and of their bank, simulated, refused in production
//! before any request. All data is synthetic; the "copia e cola" is the
//! example of the Banco Central's manual.

mod common;

use common::{COPIA_E_COLA, TestEnv, stderr_of, stdout_of};
use serde_json::{Value, json};
use wiremock::matchers::{any, body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

const ID_REC: &str = "RR1234567820260924abcdefghijk";
const TXID: &str = "7978c0c97ea847e78e8849634473c1f1";
const E2E: &str = "E12345678209910101300abcdef12345";

async fn env() -> TestEnv {
    let env = TestEnv::new().await;
    env.write_config("");
    env
}

fn sandbox(args: &[&'static str]) -> Vec<&'static str> {
    let mut todos = vec!["pix-automatico", "sandbox"];
    todos.extend_from_slice(args);
    todos
}

#[tokio::test(flavor = "multi_thread")]
async fn aprova_e_cancela_uma_recorrencia() {
    let env = env().await;
    env.mount_token("pix.write", Some(1)).await;
    let status = format!("/pix/v2/sandbox/rec/{ID_REC}/status");
    Mock::given(method("PATCH"))
        .and(path(&status))
        .and(body_json(json!({"status": "APROVADA"})))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&env.server)
        .await;
    Mock::given(method("PATCH"))
        .and(path(&status))
        .and(body_json(json!({"status": "CANCELADA", "razao": "NRES"})))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&env.server)
        .await;
    let stdout = stdout_of(
        &env.cmd()
            .args(sandbox(&["status-rec", ID_REC, "--status", "aprovada"]))
            .assert()
            .success(),
    );
    assert_eq!(
        stdout,
        format!(
            "Recorrência {ID_REC} aprovada no sandbox.\n\nConfira com: inter-pj pix-automatico rec consultar {ID_REC}\n"
        )
    );
    let json: Value = serde_json::from_str(&stdout_of(
        &env.cmd()
            .args(sandbox(&[
                "status-rec",
                ID_REC,
                "--status",
                "cancelada",
                "--razao",
                "nres",
            ]))
            .arg("--json")
            .assert()
            .success(),
    ))
    .unwrap();
    assert_eq!(
        json,
        json!({"idRec": ID_REC, "status": "CANCELADA", "razao": "NRES"})
    );
    // A reason goes only with a cancellation.
    let assert = env
        .cmd()
        .args(sandbox(&[
            "status-rec",
            ID_REC,
            "--status",
            "aprovada",
            "--razao",
            "sldb",
        ]))
        .assert()
        .code(2);
    assert!(
        stderr_of(&assert).contains("--razao vale só com --status cancelada"),
        "{}",
        stderr_of(&assert)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn responde_a_solicitacao_pela_recorrencia() {
    let env = env().await;
    env.mount_token("solicrec.write", Some(1)).await;
    Mock::given(method("PATCH"))
        .and(path(format!("/pix/v2/sandbox/solicrec/{ID_REC}/status")))
        .and(body_json(json!({"status": "REJEITADA"})))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&env.server)
        .await;
    let stdout = stdout_of(
        &env.cmd()
            .args(sandbox(&[
                "status-solicitacao",
                ID_REC,
                "--status",
                "rejeitada",
            ]))
            .assert()
            .success(),
    );
    assert!(
        stdout.starts_with(&format!(
            "Solicitação de confirmação da recorrência {ID_REC} rejeitada pelo pagador no sandbox.\n"
        )),
        "{stdout}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn cancela_uma_cobranca_como_o_banco_do_pagador() {
    let env = env().await;
    env.mount_token("cobr.write", Some(1)).await;
    Mock::given(method("PATCH"))
        .and(path(format!("/pix/v2/sandbox/cobr/{TXID}/status")))
        .and(body_json(
            json!({"status": "CANCELADA", "razao": "SETTLEMENT_FAILED"}),
        ))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&env.server)
        .await;
    Mock::given(method("PATCH"))
        .and(path(format!("/pix/v2/sandbox/cobr/{TXID}/status")))
        .and(body_json(
            json!({"status": "CANCELADA", "razao": "UNSPECIFIED"}),
        ))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&env.server)
        .await;
    let stdout = stdout_of(
        &env.cmd()
            .args(sandbox(&[
                "status-cobr",
                TXID,
                "--razao",
                "settlement-failed",
            ]))
            .assert()
            .success(),
    );
    assert_eq!(
        stdout,
        format!(
            "Cobrança recorrente {TXID} cancelada pelo banco do pagador no sandbox (motivo SETTLEMENT_FAILED).\n\nConfira com: inter-pj pix-automatico cobr consultar {TXID}\n"
        )
    );
    // Without --razao, no reason in particular.
    env.cmd()
        .args(sandbox(&["status-cobr", TXID]))
        .assert()
        .success();
}

#[tokio::test(flavor = "multi_thread")]
async fn paga_uma_cobranca_pelo_valor_e_pelo_devedor_da_recorrencia() {
    let env = env().await;
    env.mount_token("cobr.read rec.read cobr.write", Some(1))
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/pix/v2/cobr/{TXID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "idRec": ID_REC, "txid": TXID, "status": "ATIVA", "valor": {"original": "149.90"}
        })))
        .expect(1)
        .mount(&env.server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/pix/v2/rec/{ID_REC}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "idRec": ID_REC, "status": "APROVADA",
            "vinculo": {"devedor": {"cnpj": "11222333000181", "nome": "Cliente Exemplo Ltda"}, "contrato": "contrato-001"}
        })))
        .expect(1)
        .mount(&env.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/pix/v2/sandbox/cobr/pagamento"))
        .and(body_json(json!({
            "valor": 149.9, "cpfCnpj": "11222333000181", "txId": TXID, "chave": "pix@empresa.example"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"e2e": E2E})))
        .expect(1)
        .mount(&env.server)
        .await;
    let assert = env
        .cmd()
        .args(sandbox(&[
            "pagar-cobr",
            TXID,
            "--chave",
            "pix@empresa.example",
        ]))
        .assert()
        .success();
    assert_eq!(
        stdout_of(&assert),
        format!(
            "Pago no sandbox: R$ 149,90.\nendToEndId  {E2E}\n\nConfira com: inter-pj pix-automatico cobr consultar {TXID}\n"
        )
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn com_valor_e_documento_paga_sem_consultar() {
    let env = env().await;
    env.mount_token("cobr.write", Some(1)).await;
    Mock::given(method("POST"))
        .and(path("/pix/v2/sandbox/cobr/pagamento"))
        .and(body_json(json!({
            "valor": 100, "cpfCnpj": "12345678909", "txId": TXID, "chave": "pix@empresa.example"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"e2e": E2E})))
        .expect(1)
        .mount(&env.server)
        .await;
    let json: Value = serde_json::from_str(&stdout_of(
        &env.cmd()
            .args(sandbox(&[
                "pagar-cobr",
                TXID,
                "--chave",
                "pix@empresa.example",
                "--valor",
                "100",
                "--documento",
                "123.456.789-09",
            ]))
            .arg("--json")
            .assert()
            .success(),
    ))
    .unwrap();
    assert_eq!(json, json!({"endToEndId": E2E, "valor": "100.00"}));
}

#[tokio::test(flavor = "multi_thread")]
async fn paga_um_copia_e_cola() {
    let env = env().await;
    env.mount_token("pix.write", Some(1)).await;
    Mock::given(method("POST"))
        .and(path("/pix/v2/sandbox/cob/pagamento"))
        .and(body_json(json!({"qrCode": COPIA_E_COLA, "valor": 10})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"endToEnd": E2E})))
        .expect(1)
        .mount(&env.server)
        .await;
    let assert = env
        .cmd()
        .args(sandbox(&[
            "pagar-qrcode",
            "--copia-e-cola",
            COPIA_E_COLA,
            "--valor",
            "10",
        ]))
        .assert()
        .success();
    assert!(
        stdout_of(&assert).starts_with(&format!("Pago no sandbox: R$ 10,00.\nendToEndId  {E2E}\n")),
        "{}",
        stdout_of(&assert)
    );
}

/// #50's acceptance criterion: in production, refused with no HTTP call,
/// not even for a token.
#[tokio::test(flavor = "multi_thread")]
async fn em_producao_nada_e_enviado() {
    let env = env().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&env.server)
        .await;
    for (args, comando) in [
        (
            sandbox(&["status-rec", ID_REC, "--status", "aprovada"]),
            "status-rec",
        ),
        (
            sandbox(&["status-solicitacao", ID_REC, "--status", "aceita"]),
            "status-solicitacao",
        ),
        (sandbox(&["status-cobr", TXID]), "status-cobr"),
        (
            sandbox(&[
                "pagar-cobr",
                TXID,
                "--chave",
                "pix@empresa.example",
                "--valor",
                "1",
                "--documento",
                "123.456.789-09",
            ]),
            "pagar-cobr",
        ),
        (
            sandbox(&[
                "pagar-qrcode",
                "--copia-e-cola",
                COPIA_E_COLA,
                "--valor",
                "1",
            ]),
            "pagar-qrcode",
        ),
    ] {
        let assert = env
            .cmd()
            .args(["--ambiente", "producao"])
            .args(&args)
            .assert()
            .code(2);
        assert!(
            stderr_of(&assert).contains(&format!(
                "pix-automatico sandbox {comando} existe só no sandbox"
            )),
            "{}",
            stderr_of(&assert)
        );
    }
}
