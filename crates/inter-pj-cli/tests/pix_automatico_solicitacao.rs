//! `inter-pj pix-automatico solicitacao ...` end to end, against a mock API.
//! All data is synthetic.

mod common;

use common::{TestEnv, stderr_of, stdout_of};
use serde_json::{Value, json};
use wiremock::matchers::{any, body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

const ID_REC: &str = "RR1234567820260924abcdefghijk";
const ID: &str = "SC1234567820260924abcdefghijk";
const EXPIRA: &str = "2099-10-01T23:59:59-03:00";

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

fn criar() -> Vec<&'static str> {
    vec![
        "pix-automatico",
        "solicitacao",
        "criar",
        "--rec",
        ID_REC,
        "--documento",
        "123.456.789-09",
        "--ispb",
        "12345678",
        "--agencia",
        "0001",
        "--conta",
        "1234567",
        "--expiracao",
        EXPIRA,
    ]
}

/// `criar()` with another value for `opcao`.
fn criar_com(opcao: &str, valor: &'static str) -> Vec<&'static str> {
    let mut args = criar();
    let i = args.iter().position(|arg| *arg == opcao).unwrap();
    args[i + 1] = valor;
    args
}

fn corpo() -> Value {
    json!({
        "idRec": ID_REC,
        "calendario": {"dataExpiracaoSolicitacao": EXPIRA},
        "destinatario": {"cpf": "12345678909", "conta": "1234567", "ispbParticipante": "12345678", "agencia": "0001"}
    })
}

fn rec(status: &str) -> Value {
    json!({
        "idRec": ID_REC,
        "vinculo": {"objeto": "Mensalidade", "devedor": {"cpf": "12345678909", "nome": "Cliente Exemplo"}, "contrato": "contrato-001"},
        "calendario": {"dataInicial": "2099-10-10", "periodicidade": "MENSAL"},
        "valor": {"valorRec": "149.90"},
        "status": status,
        "politicaRetentativa": "NAO_PERMITE"
    })
}

fn solicitacao(status: &str) -> Value {
    json!({
        "idSolicRec": ID,
        "idRec": ID_REC,
        "calendario": {"dataExpiracaoSolicitacao": "2099-10-02T02:59:59.000Z"},
        "status": status,
        "destinatario": {"cpf": "12345678909", "conta": "1234567", "ispbParticipante": "12345678", "agencia": "0001"},
        "atualizacao": [{"status": "CRIADA", "data": "2026-09-24T13:00:00.000Z"}],
        "recPayload": rec("CRIADA")
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn pede_a_aprovacao_depois_de_mostrar_a_recorrencia() {
    let env = env().await;
    env.mount_token("rec.read solicrec.write", Some(1)).await;
    Mock::given(method("GET"))
        .and(path(format!("/pix/v2/rec/{ID_REC}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(rec("CRIADA")))
        .expect(1)
        .mount(&env.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/pix/v2/solicrec"))
        .and(body_json(corpo()))
        .respond_with(ResponseTemplate::new(201).set_body_json(solicitacao("CRIADA")))
        .expect(1)
        .mount(&env.server)
        .await;
    let assert = env.cmd().args(criar()).arg("--sim").assert().success();
    let stdout = stdout_of(&assert);
    assert!(
        stdout.starts_with(&format!(
            "Solicitação criada: o banco do pagador vai pedir que ele aprove a recorrência.\n\nSolicitação de confirmação {ID}\n"
        )),
        "{stdout}"
    );
    for linha in [
        format!("Acompanhe com: inter-pj pix-automatico solicitacao consultar {ID}"),
        format!("ou pela recorrência: inter-pj pix-automatico rec consultar {ID_REC}"),
    ] {
        assert!(stdout.contains(&linha), "{linha}\n{stdout}");
    }
    let stderr = stderr_of(&assert);
    for linha in [
        "Solicitação de confirmação a enviar",
        "Cliente Exemplo (123.456.789-09)",
        "contrato-001",
        "R$ 149,90 em cada pagamento",
        "123.456.789-09, banco com ISPB 12345678, agência 0001, conta 1234567",
        "Expira em",
    ] {
        assert!(stderr.contains(linha), "{linha}\n{stderr}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn simulacao_e_erros_nao_consultam_nem_enviam() {
    let env = env().await;
    nothing_is_sent(&env).await;
    let assert = env
        .cmd()
        .args(criar())
        .args(["--simular", "--json"])
        .assert()
        .success();
    let json: Value = serde_json::from_str(&stdout_of(&assert)).unwrap();
    assert_eq!(json["metodo"], "POST");
    assert_eq!(json["url"], format!("{}/pix/v2/solicrec", env.server.uri()));
    assert_eq!(json["corpo"], corpo());

    for (opcao, valor, erro) in [
        ("--ispb", "416968", "--ispb: o ISPB do banco tem 8 dígitos"),
        ("--conta", "1234-5", "--conta: até 20 dígitos"),
        (
            "--expiracao",
            "2020-01-01",
            "--expiracao: 01/01/2020 23:59:59 já passou",
        ),
        ("--expiracao", "sempre", "duração inválida \"sempre\""),
    ] {
        let assert = env
            .cmd()
            .args(criar_com(opcao, valor))
            .arg("--sim")
            .assert()
            .code(2);
        assert!(
            stderr_of(&assert).contains(erro),
            "{erro}\n{}",
            stderr_of(&assert)
        );
    }
    // Without a terminal to confirm, not even the recurrence is looked up.
    env.cmd().args(criar()).assert().code(2);
}

/// The expiration counts from the bank's now: with today fixed by
/// `INTER_HOJE`, a day after it has not passed, whatever the clock says,
/// and a day before it has.
#[tokio::test(flavor = "multi_thread")]
async fn a_expiracao_conta_do_dia_do_banco() {
    let env = env().await;
    nothing_is_sent(&env).await;
    // The end of the day is the bank's, in Brasília, whatever the time zone
    // of the machine.
    let assert = env
        .cmd()
        .env("INTER_HOJE", "2020-01-10")
        .env("TZ", "UTC")
        .args(criar_com("--expiracao", "2020-01-15"))
        .arg("--simular")
        .assert()
        .success();
    assert!(
        stdout_of(&assert).contains(r#""dataExpiracaoSolicitacao": "2020-01-15T23:59:59-03:00""#),
        "{}",
        stdout_of(&assert)
    );
    let assert = env
        .cmd()
        .env("INTER_HOJE", "2020-01-10")
        .args(criar_com("--expiracao", "2020-01-09"))
        .arg("--sim")
        .assert()
        .code(2);
    assert!(
        stderr_of(&assert).contains("--expiracao: 09/01/2020 23:59:59 já passou"),
        "{}",
        stderr_of(&assert)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn recusa_uma_recorrencia_que_nao_aguarda_aprovacao() {
    let env = env().await;
    env.mount_token("rec.read solicrec.write", None).await;
    Mock::given(method("GET"))
        .and(path(format!("/pix/v2/rec/{ID_REC}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(rec("APROVADA")))
        .expect(1)
        .mount(&env.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/pix/v2/solicrec"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&env.server)
        .await;
    let assert = env.cmd().args(criar()).arg("--sim").assert().code(2);
    assert!(
        stderr_of(&assert).contains("a recorrência já foi aprovada pelo pagador"),
        "{}",
        stderr_of(&assert)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn resultado_incerto_orienta_a_conferir_pela_recorrencia() {
    let env = env().await;
    env.mount_token("rec.read solicrec.write", Some(1)).await;
    Mock::given(method("GET"))
        .and(path(format!("/pix/v2/rec/{ID_REC}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(rec("CRIADA")))
        .expect(1)
        .mount(&env.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/pix/v2/solicrec"))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&env.server)
        .await;
    let assert = env.cmd().args(criar()).arg("--sim").assert().code(9);
    let stderr = stderr_of(&assert);
    for dica in [
        "dica: a solicitação pode ter sido enviada ao pagador, e esta API não tem chave de idempotência: repetir o comando pode criar outra".to_owned(),
        format!("dica: confira antes de tentar de novo: inter-pj pix-automatico rec consultar {ID_REC}"),
    ] {
        assert!(stderr.contains(&dica), "{dica}\n{stderr}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn consulta_e_cancela_uma_solicitacao_sem_resposta() {
    let env = env().await;
    env.mount_token("solicrec.read solicrec.write", Some(1))
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/pix/v2/solicrec/{ID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(solicitacao("RECEBIDA")))
        .expect(2)
        .mount(&env.server)
        .await;
    let mut cancelada = solicitacao("CANCELADA");
    cancelada["atualizacao"]
        .as_array_mut()
        .unwrap()
        .push(json!({"status": "CANCELADA", "data": "2026-09-24T14:00:00.000Z"}));
    Mock::given(method("PATCH"))
        .and(path(format!("/pix/v2/solicrec/{ID}")))
        .and(body_json(json!({"status": "CANCELADA"})))
        .respond_with(ResponseTemplate::new(201).set_body_json(cancelada))
        .expect(1)
        .mount(&env.server)
        .await;

    let stdout = stdout_of(
        &env.cmd()
            .args(["pix-automatico", "solicitacao", "consultar", ID])
            .assert()
            .success(),
    );
    assert!(
        stdout.starts_with(&format!(
            "Solicitação de confirmação {ID}\n  Status            recebida pelo pagador\n"
        )),
        "{stdout}"
    );
    let assert = env
        .cmd()
        .args(["pix-automatico", "solicitacao", "cancelar", ID, "--sim"])
        .assert()
        .success();
    let stdout = stdout_of(&assert);
    assert!(stdout.starts_with("Solicitação cancelada.\n\n"), "{stdout}");
    assert!(
        stdout.contains("  Status            cancelada\n"),
        "{stdout}"
    );
    assert!(
        stderr_of(&assert).contains(&format!("Solicitação {ID} a cancelar")),
        "{}",
        stderr_of(&assert)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn nao_cancela_uma_solicitacao_ja_respondida() {
    let env = env().await;
    env.mount_token("solicrec.read solicrec.write", None).await;
    Mock::given(method("GET"))
        .and(path(format!("/pix/v2/solicrec/{ID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(solicitacao("ACEITA")))
        .expect(1)
        .mount(&env.server)
        .await;
    Mock::given(method("PATCH"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&env.server)
        .await;
    let assert = env
        .cmd()
        .args(["pix-automatico", "solicitacao", "cancelar", ID, "--sim"])
        .assert()
        .code(2);
    assert!(
        stderr_of(&assert).contains("a solicitação está aceita pelo pagador"),
        "{}",
        stderr_of(&assert)
    );
}
