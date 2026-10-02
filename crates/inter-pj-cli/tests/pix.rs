//! `inter-pj pix enviar` end to end, against a mock API. Every key, amount
//! and account here is synthetic.

mod common;

use common::{CLIENT_SECRET, TOKEN, TestEnv, stderr_of, stdout_of};
use predicates::prelude::*;
use serde_json::{Value, json};
use wiremock::matchers::{any, body_json, header, method, path};
use wiremock::{Mock, ResponseTemplate};

const PIX: &str = "/banking/v2/pix";
const ID: &str = "123e4567-e89b-42d3-a456-426614174000";
const CODIGO: &str = "c42f0787-02cb-4b31-827e-459ec9d7ece1";
const ENVIAR: [&str; 6] = [
    "pix",
    "enviar",
    "--chave",
    "fornecedor@exemplo.com",
    "--valor",
    "150,00",
];

fn resposta(tipo: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "tipoRetorno": tipo,
        "codigoSolicitacao": CODIGO,
        "dataPagamento": "2026-10-01",
        "dataOperacao": "2026-09-23"
    }))
}

/// The API must not be called at all.
async fn nothing_is_sent(env: &TestEnv) {
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&env.server)
        .await;
}

async fn mount_pix(env: &TestEnv, body: Value, response: ResponseTemplate) {
    env.mount_token("pagamento-pix.write", Some(1)).await;
    Mock::given(method("POST"))
        .and(path(PIX))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        .and(body_json(body))
        .respond_with(response)
        .expect(1)
        .mount(&env.server)
        .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn simulation_shows_the_request_and_sends_nothing() {
    let env = TestEnv::new().await;
    env.write_config("conta_corrente = \"7654321\"");
    nothing_is_sent(&env).await;

    let assert = env
        .cmd()
        .args(ENVIAR)
        .args(["--descricao", "NF 123", "--simular"])
        .assert()
        .success();
    let stdout = stdout_of(&assert);
    let stderr = stderr_of(&assert);
    assert!(
        stdout.starts_with("Simulação: nada foi enviado."),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("POST {}/banking/v2/pix", env.server.uri())),
        "{stdout}"
    );
    assert!(stdout.contains("x-id-idempotente: "), "{stdout}");
    assert!(stdout.contains("x-conta-corrente: *****21"), "{stdout}");
    assert!(
        stdout.contains("\"chave\": \"fornecedor@exemplo.com\""),
        "{stdout}"
    );
    assert!(stdout.contains("\"valor\": 150"), "{stdout}");
    assert!(!stdout.contains("7654321"), "{stdout}");
    assert!(stderr.contains("Pix a enviar"), "{stderr}");
    assert!(
        stderr.contains("R$ 150,00 (cento e cinquenta reais)"),
        "{stderr}"
    );

    // In JSON, with the same idempotency key as shown in the summary.
    let assert = env
        .cmd()
        .args(ENVIAR)
        .args(["--simular", "--json", "--id-idempotente", ID])
        .assert()
        .success();
    let json: Value = serde_json::from_str(&stdout_of(&assert)).unwrap();
    assert_eq!(json["simulacao"], true);
    assert_eq!(json["metodo"], "POST");
    assert_eq!(json["cabecalhos"]["x-id-idempotente"], ID);
    assert_eq!(
        json["corpo"],
        json!({
            "valor": 150,
            "destinatario": {"tipo": "CHAVE", "chave": "fornecedor@exemplo.com"}
        })
    );
    assert!(stderr_of(&assert).contains(ID));
}

#[tokio::test(flavor = "multi_thread")]
async fn without_a_terminal_it_asks_for_sim() {
    let env = TestEnv::new().await;
    env.write_config("");
    nothing_is_sent(&env).await;

    env.cmd()
        .args(ENVIAR)
        .write_stdin("s\n")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("Pix a enviar").and(predicate::str::contains("--sim")));
}

#[tokio::test(flavor = "multi_thread")]
async fn sim_sends_once_with_the_idempotency_key() {
    let env = TestEnv::new().await;
    env.write_config("");
    mount_pix(
        &env,
        json!({
            "valor": 150,
            "descricao": "NF 123",
            "destinatario": {"tipo": "CHAVE", "chave": "fornecedor@exemplo.com"}
        }),
        resposta("PROCESSADO"),
    )
    .await;
    let assert = env
        .cmd()
        .args(ENVIAR)
        .args([
            "--descricao",
            "NF 123",
            "--sim",
            "--id-idempotente",
            &ID.to_uppercase(),
        ])
        .assert()
        .success();
    let stdout = stdout_of(&assert);
    assert!(stdout.starts_with("Pix enviado.\n"), "{stdout}");
    assert!(stdout.contains(CODIGO), "{stdout}");
    assert!(
        stdout.contains(&format!("Chave de idempotência  {ID}")),
        "{stdout}"
    );
    // The key given is the one sent, in lower case.
    let requests = env.server.received_requests().await.unwrap();
    let pix = requests.iter().find(|r| r.url.path() == PIX).unwrap();
    assert_eq!(pix.headers["x-id-idempotente"], ID);
}

#[tokio::test(flavor = "multi_thread")]
async fn operation_limit_is_enforced_even_with_sim() {
    let env = TestEnv::new().await;
    env.write_config("limite_por_operacao = \"100,00\"");
    nothing_is_sent(&env).await;

    env.cmd()
        .args(["pix", "enviar", "--chave", "fornecedor@exemplo.com"])
        .args(["--valor", "100,01", "--sim"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "R$ 100,01 passa do limite por operação do perfil \"padrao\" (R$ 100,00)",
        ));
}

#[tokio::test(flavor = "multi_thread")]
async fn amounts_up_to_the_limit_are_sent() {
    let env = TestEnv::new().await;
    env.write_config("limite_por_operacao = 100");
    mount_pix(
        &env,
        json!({"valor": 100, "destinatario": {"tipo": "CHAVE", "chave": "fornecedor@exemplo.com"}}),
        resposta("PROCESSADO"),
    )
    .await;

    env.cmd()
        .args(["pix", "enviar", "--chave", "fornecedor@exemplo.com"])
        .args(["--valor", "R$ 100,00", "--sim"])
        .assert()
        .success();
}

#[tokio::test(flavor = "multi_thread")]
async fn scheduled_and_pending_approval_are_explained() {
    let env = TestEnv::new().await;
    env.write_config("");
    env.mount_token("pagamento-pix.write", None).await;
    Mock::given(method("POST"))
        .and(path(PIX))
        .and(body_json(json!({
            "valor": 150,
            "dataPagamento": "2099-10-01",
            "destinatario": {"tipo": "CHAVE", "chave": "fornecedor@exemplo.com"}
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "tipoRetorno": "AGENDADO",
            "codigoSolicitacao": CODIGO,
            "dataPagamento": "2099-10-01"
        })))
        .expect(1)
        .mount(&env.server)
        .await;
    env.cmd()
        .args(ENVIAR)
        .args(["--data", "2099-10-01", "--sim"])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("Pix agendado para 01/10/2099."))
        .stderr(predicate::str::contains("agendado para 01/10/2099"));

    Mock::given(method("POST"))
        .and(path(PIX))
        .respond_with(resposta("APROVACAO"))
        .expect(1)
        .mount(&env.server)
        .await;
    env.cmd()
        .args(ENVIAR)
        .arg("--sim")
        .assert()
        .success()
        .stdout(predicate::str::starts_with(
            "Pix aguardando aprovação no Internet Banking",
        ));
}

#[tokio::test(flavor = "multi_thread")]
async fn json_output_includes_the_idempotency_key() {
    let env = TestEnv::new().await;
    env.write_config("");
    mount_pix(
        &env,
        json!({"valor": 150, "destinatario": {"tipo": "CHAVE", "chave": "fornecedor@exemplo.com"}}),
        resposta("PROCESSADO"),
    )
    .await;

    let assert = env
        .cmd()
        .args(ENVIAR)
        .args(["--sim", "--json", "--id-idempotente", ID])
        .assert()
        .success();
    let json: Value = serde_json::from_str(&stdout_of(&assert)).unwrap();
    assert_eq!(
        json,
        json!({
            "tipoRetorno": "PROCESSADO",
            "codigoSolicitacao": CODIGO,
            "dataPagamento": "2026-10-01",
            "dataOperacao": "2026-09-23",
            "idIdempotente": ID
        })
    );
}

/// After a `5xx` the payment may have been made: never repeated, and the
/// user learns how to repeat it safely.
#[tokio::test(flavor = "multi_thread")]
async fn uncertain_outcome_shows_how_to_repeat_safely() {
    let env = TestEnv::new().await;
    env.write_config("");
    env.mount_token("pagamento-pix.write", None).await;
    Mock::given(method("POST"))
        .and(path(PIX))
        .respond_with(ResponseTemplate::new(504))
        .expect(1)
        .mount(&env.server)
        .await;

    env.cmd()
        .args(ENVIAR)
        .args(["--sim", "--id-idempotente", ID])
        .assert()
        .code(6)
        .stderr(
            predicate::str::contains("o pagamento pode ter sido feito")
                .and(predicate::str::contains(format!("--id-idempotente {ID}"))),
        );
}

#[tokio::test(flavor = "multi_thread")]
async fn production_is_highlighted_in_the_summary() {
    let env = TestEnv::new().await;
    env.write_config("");
    nothing_is_sent(&env).await;

    env.cmd()
        .args(ENVIAR)
        .args(["--ambiente", "producao", "--simular"])
        .assert()
        .success()
        .stderr(
            predicate::str::starts_with(
                "*** PRODUÇÃO: este Pix movimenta dinheiro da conta real ***",
            )
            .and(predicate::str::contains("PRODUÇÃO (conta real)")),
        );
}

#[tokio::test(flavor = "multi_thread")]
async fn invalid_amounts_and_keys_are_usage_errors() {
    let env = TestEnv::new().await;
    env.write_config("");
    nothing_is_sent(&env).await;

    for (args, mensagem) in [
        (
            ["--chave", "fornecedor@exemplo.com", "--valor", "1.500"],
            "ambíguo",
        ),
        (
            ["--chave", "fornecedor@exemplo.com", "--valor", "0"],
            "maior que zero",
        ),
        (
            ["--chave", "fornecedor@exemplo.com", "--valor", "10,555"],
            "ambíguo",
        ),
        (["--chave", "11912345678", "--valor", "10"], "+55"),
        (["--chave", "fulano@", "--valor", "10"], "e-mail inválido"),
    ] {
        env.cmd()
            .args(["pix", "enviar"])
            .args(args)
            .arg("--sim")
            .assert()
            .code(2)
            .stderr(predicate::str::contains(mensagem));
    }
    env.cmd()
        .args(ENVIAR)
        .args(["--data", "2020-01-01", "--sim"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("já passou"));
    env.cmd()
        .args(ENVIAR)
        .args(["--descricao", &"x".repeat(141), "--sim"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("140"));
    env.cmd()
        .args(ENVIAR)
        .args(["--formato", "csv", "--sim"])
        .assert()
        .code(2);
}

#[tokio::test(flavor = "multi_thread")]
async fn secrets_never_reach_the_output() {
    let env = TestEnv::new().await;
    env.write_config("");
    mount_pix(
        &env,
        json!({"valor": 150, "destinatario": {"tipo": "CHAVE", "chave": "fornecedor@exemplo.com"}}),
        resposta("PROCESSADO"),
    )
    .await;

    let assert = env
        .cmd()
        .args(ENVIAR)
        .args(["--sim", "-vv"])
        .assert()
        .success();
    let output = format!("{}{}", stdout_of(&assert), stderr_of(&assert));
    assert!(!output.contains(CLIENT_SECRET), "{output}");
    assert!(!output.contains(TOKEN), "{output}");
}
