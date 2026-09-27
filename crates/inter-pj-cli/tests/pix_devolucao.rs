//! `inter-pj pix devolucao ...` end to end, against a mock API: refunds take
//! money out of the account, so every rail is checked. All data is
//! synthetic.

mod common;

use common::{TestEnv, stderr_of, stdout_of};
use serde_json::{Value, json};
use wiremock::matchers::{any, body_json, method, path, path_regex};
use wiremock::{Mock, ResponseTemplate};

const E2E: &str = "E00416968202609181241abcdEFGH123";
const ID: &str = "D7978c0c97ea847e78e8849634473c1f1";

async fn env(extra: &str) -> TestEnv {
    let env = TestEnv::new().await;
    env.write_config(extra);
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

/// The Pix of 300,00 with `devolucoes`, looked up `vezes` times.
async fn mount_pix(env: &TestEnv, devolucoes: &Value, vezes: u64) {
    Mock::given(method("GET"))
        .and(path(format!("/pix/v2/pix/{E2E}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "endToEndId": E2E,
            "txid": "a1b2c3d4e5f60718293a4b5c6d7e8f90",
            "valor": "300.00",
            "horario": "2026-09-18T12:41:07.000Z",
            "devolucoes": devolucoes
        })))
        .expect(vezes)
        .mount(&env.server)
        .await;
}

/// No refund may be requested.
async fn no_refund(env: &TestEnv) {
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&env.server)
        .await;
}

fn devolucao(status: &str) -> Value {
    json!({
        "id": ID,
        "rtrId": "D00416968202609181300abcdefghijk",
        "valor": "100.00",
        "horario": {"solicitacao": "2026-09-18T15:00:00Z"},
        "status": status
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn devolve_parte_do_pix() {
    let env = env("").await;
    env.mount_token("pix.write pix.read", None).await;
    mount_pix(
        &env,
        &json!([{"id": "D1", "valor": "50.00", "status": "DEVOLVIDO"}]),
        1,
    )
    .await;
    Mock::given(method("PUT"))
        .and(path(format!("/pix/v2/pix/{E2E}/devolucao/{ID}")))
        .and(body_json(json!({
            "valor": "100.00",
            "natureza": "ORIGINAL",
            "descricao": "Pedido cancelado"
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(devolucao("EM_PROCESSAMENTO")))
        .expect(1)
        .mount(&env.server)
        .await;
    let assert = env
        .cmd()
        .args([
            "pix",
            "devolucao",
            "solicitar",
            E2E,
            "--valor",
            "100,00",
            "--natureza",
            "original",
            "--descricao",
            "Pedido cancelado",
            "--id",
            ID,
            "--sim",
        ])
        .assert()
        .success();
    let stdout = stdout_of(&assert);
    assert!(
        stdout.starts_with(&format!(
            "Devolução solicitada.\n\nDevolução {ID}\n  Status         em processamento\n"
        )),
        "{stdout}"
    );
    assert!(
        stdout.ends_with(&format!(
            "Acompanhe com: inter-pj pix devolucao consultar {E2E} {ID} --aguardar\n"
        )),
        "{stdout}"
    );
    let stderr = stderr_of(&assert);
    for linha in [
        "Devolução a solicitar",
        "Valor do Pix  R$ 300,00",
        "Já devolvido  R$ 50,00",
        "Devolução     R$ 100,00 (cem reais)",
        &format!("id            {ID}"),
    ] {
        assert!(stderr.contains(linha), "{linha}\n{stderr}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn devolve_tudo_o_que_resta_com_um_id_gerado() {
    let env = env("").await;
    env.mount_token("pix.write pix.read", None).await;
    mount_pix(
        &env,
        &json!([
            {"id": "D1", "valor": "150.00", "status": "DEVOLVIDO"},
            {"id": "D2", "valor": "50.00", "status": "EM_PROCESSAMENTO"},
            {"id": "D3", "valor": "300.00", "status": "NAO_REALIZADO"}
        ]),
        1,
    )
    .await;
    Mock::given(method("PUT"))
        .and(path_regex(format!(
            r"^/pix/v2/pix/{E2E}/devolucao/[0-9a-f]{{32}}$"
        )))
        .and(body_json(json!({"valor": "100.00"})))
        .respond_with(ResponseTemplate::new(201).set_body_json(devolucao("DEVOLVIDO")))
        .expect(1)
        .mount(&env.server)
        .await;
    let assert = env
        .cmd()
        .args([
            "pix",
            "devolucao",
            "solicitar",
            E2E,
            "--tudo",
            "--sim",
            "--json",
        ])
        .assert()
        .success();
    let json: Value = serde_json::from_str(&stdout_of(&assert)).unwrap();
    assert_eq!(json["status"], "DEVOLVIDO");
    let stderr = stderr_of(&assert);
    assert!(stderr.contains("Em devolução  R$ 50,00"), "{stderr}");
    assert!(stderr.contains("R$ 100,00 (cem reais)"), "{stderr}");
}

/// The acceptance criterion of #36: a refund larger than the Pix is
/// refused locally when the Pix is known.
#[tokio::test(flavor = "multi_thread")]
async fn devolucao_maior_que_o_que_resta_e_recusada() {
    let env = env("").await;
    env.mount_token("pix.write pix.read", None).await;
    mount_pix(
        &env,
        &json!([{"id": "D1", "valor": "250.00", "status": "DEVOLVIDO"}]),
        2,
    )
    .await;
    no_refund(&env).await;
    let assert = env
        .cmd()
        .args([
            "pix",
            "devolucao",
            "solicitar",
            E2E,
            "--valor",
            "50,01",
            "--sim",
        ])
        .assert()
        .code(2);
    assert!(
        stderr_of(&assert).contains(
            "a devolução de R$ 50,01 passa do que resta do Pix: R$ 50,00 de R$ 300,00, R$ 250,00 já devolvidos ou em devolução"
        ),
        "{}",
        stderr_of(&assert)
    );
    let env2 = self::env("").await;
    env2.mount_token("pix.write pix.read", None).await;
    mount_pix(
        &env2,
        &json!([{"id": "D1", "valor": "300.00", "status": "DEVOLVIDO"}]),
        1,
    )
    .await;
    no_refund(&env2).await;
    let assert = env2
        .cmd()
        .args(["pix", "devolucao", "solicitar", E2E, "--tudo", "--sim"])
        .assert()
        .code(2);
    assert!(
        stderr_of(&assert).contains("nada a devolver"),
        "{}",
        stderr_of(&assert)
    );
    // The same refund once more: only the second lookup of the first Pix.
    let assert = env
        .cmd()
        .args([
            "pix",
            "devolucao",
            "solicitar",
            E2E,
            "--valor",
            "300",
            "--sim",
        ])
        .assert()
        .code(2);
    assert!(stderr_of(&assert).contains("passa do que resta"));
}

#[tokio::test(flavor = "multi_thread")]
async fn o_limite_por_operacao_vale_para_devolucoes() {
    let env = env("limite_por_operacao = \"80,00\"").await;
    // A known amount above the limit: refused before any request.
    nothing_is_sent(&env).await;
    let assert = env
        .cmd()
        .args([
            "pix",
            "devolucao",
            "solicitar",
            E2E,
            "--valor",
            "100",
            "--sim",
        ])
        .assert()
        .code(2);
    assert!(
        stderr_of(&assert).contains("R$ 100,00 passa do limite por operação do perfil"),
        "{}",
        stderr_of(&assert)
    );

    // --tudo learns the amount from the Pix: refused after the lookup.
    let env = self::env("limite_por_operacao = \"80,00\"").await;
    env.mount_token("pix.write pix.read", None).await;
    mount_pix(&env, &json!([]), 1).await;
    no_refund(&env).await;
    let assert = env
        .cmd()
        .args(["pix", "devolucao", "solicitar", E2E, "--tudo", "--sim"])
        .assert()
        .code(2);
    assert!(
        stderr_of(&assert).contains("R$ 300,00 passa do limite por operação"),
        "{}",
        stderr_of(&assert)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn sem_confirmacao_ou_com_dados_invalidos_nada_e_enviado() {
    let env = env("").await;
    nothing_is_sent(&env).await;
    // Without a terminal and without --sim, not even the lookup.
    let assert = env
        .cmd()
        .args(["pix", "devolucao", "solicitar", E2E, "--valor", "10"])
        .write_stdin("s\n")
        .assert()
        .code(2);
    assert!(
        stderr_of(&assert).contains("use --sim"),
        "{}",
        stderr_of(&assert)
    );
    let longa = "x".repeat(141);
    for args in [
        &["--valor", "0", "--sim"][..],
        &["--valor", "10", "--descricao", longa.as_str(), "--sim"][..],
        &["--valor", "10", "--id", "D-1", "--sim"][..],
        &["--valor", "10", "--tudo", "--sim"][..],
        &["--sim"][..],
        &["--tudo", "--simular"][..],
        &["--valor", "10", "--simular", "--aguardar"][..],
        &["--valor", "10", "--natureza", "saque", "--sim"][..],
    ] {
        env.cmd()
            .args(["pix", "devolucao", "solicitar", E2E])
            .args(args)
            .assert()
            .code(2);
    }
    env.cmd()
        .args([
            "pix",
            "devolucao",
            "solicitar",
            "../x",
            "--valor",
            "10",
            "--sim",
        ])
        .assert()
        .code(2);
}

#[tokio::test(flavor = "multi_thread")]
async fn simulacao_mostra_a_requisicao() {
    let env = env("").await;
    nothing_is_sent(&env).await;
    let assert = env
        .cmd()
        .args([
            "pix",
            "devolucao",
            "solicitar",
            E2E,
            "--valor",
            "10",
            "--id",
            ID,
            "--simular",
            "--json",
        ])
        .assert()
        .success();
    let json: Value = serde_json::from_str(&stdout_of(&assert)).unwrap();
    assert_eq!(json["metodo"], "PUT");
    assert_eq!(
        json["url"],
        format!("{}/pix/v2/pix/{E2E}/devolucao/{ID}", env.server.uri())
    );
    assert_eq!(json["corpo"], json!({"valor": "10.00"}));
}

/// With an unknown outcome, the error says how to check and how to repeat
/// without refunding twice; the request is not repeated by itself.
#[tokio::test(flavor = "multi_thread")]
async fn resultado_incerto_orienta_a_repetir_com_o_mesmo_id() {
    let env = env("").await;
    env.mount_token("pix.write pix.read", None).await;
    mount_pix(&env, &json!([]), 1).await;
    Mock::given(method("PUT"))
        .and(path(format!("/pix/v2/pix/{E2E}/devolucao/{ID}")))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&env.server)
        .await;
    let assert = env
        .cmd()
        .args([
            "pix",
            "devolucao",
            "solicitar",
            E2E,
            "--valor",
            "10",
            "--id",
            ID,
            "--sim",
        ])
        .assert()
        .code(6);
    let stderr = stderr_of(&assert);
    for dica in [
        "dica: a devolução pode ter sido feita; com o mesmo id, a API não devolve de novo"
            .to_owned(),
        format!("dica: confira com: inter-pj pix devolucao consultar {E2E} {ID}"),
        format!("dica: ou repita o comando com --id {ID}"),
    ] {
        assert!(stderr.contains(&dica), "{dica}\n{stderr}");
    }
}

/// Repeating with the id of a refund already requested sends nothing.
#[tokio::test(flavor = "multi_thread")]
async fn o_mesmo_id_nao_devolve_de_novo() {
    let env = env("").await;
    env.mount_token("pix.write pix.read", None).await;
    mount_pix(
        &env,
        &json!([{"id": ID, "valor": "100.00", "status": "DEVOLVIDO"}]),
        1,
    )
    .await;
    no_refund(&env).await;
    let assert = env
        .cmd()
        .args([
            "pix",
            "devolucao",
            "solicitar",
            E2E,
            "--valor",
            "100",
            "--id",
            ID,
            "--sim",
        ])
        .assert()
        .success();
    assert!(
        stderr_of(&assert).contains(&format!("aviso: a devolução {ID} já tinha sido solicitada")),
        "{}",
        stderr_of(&assert)
    );
    assert!(
        stdout_of(&assert).starts_with(&format!("Devolução {ID}\n  Status  devolvida\n")),
        "{}",
        stdout_of(&assert)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn consulta_e_aguarda_a_devolucao() {
    let env = env("").await;
    env.mount_token("pix.read", None).await;
    let caminho = format!("/pix/v2/pix/{E2E}/devolucao/{ID}");
    Mock::given(method("GET"))
        .and(path(caminho.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(devolucao("DEVOLVIDO")))
        .up_to_n_times(2)
        .expect(2)
        .mount(&env.server)
        .await;
    let texto = stdout_of(
        &env.cmd()
            .args(["pix", "devolucao", "consultar", E2E, ID])
            .assert()
            .success(),
    );
    assert!(
        texto.starts_with(&format!(
            "Devolução {ID}\n  Status         devolvida\n  Valor          R$ 100,00\n"
        )),
        "{texto}"
    );
    env.cmd()
        .args(["pix", "devolucao", "consultar", E2E, ID, "--aguardar"])
        .assert()
        .success();

    let mut recusada = devolucao("NAO_REALIZADO");
    recusada["motivo"] = json!("Conta do pagador encerrada");
    Mock::given(method("GET"))
        .and(path(caminho.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(recusada))
        .up_to_n_times(1)
        .expect(1)
        .mount(&env.server)
        .await;
    let assert = env
        .cmd()
        .args(["pix", "devolucao", "consultar", E2E, ID, "--aguardar"])
        .assert()
        .code(5);
    assert!(
        stderr_of(&assert).contains("a devolução não foi feita: Conta do pagador encerrada"),
        "{}",
        stderr_of(&assert)
    );

    Mock::given(method("GET"))
        .and(path(caminho))
        .respond_with(ResponseTemplate::new(200).set_body_json(devolucao("EM_PROCESSAMENTO")))
        .mount(&env.server)
        .await;
    let assert = env
        .cmd()
        .args([
            "pix",
            "devolucao",
            "consultar",
            E2E,
            ID,
            "--aguardar",
            "--timeout",
            "1s",
        ])
        .assert()
        .code(8);
    assert!(
        stderr_of(&assert)
            .contains("tempo de espera esgotado (1 s): a devolução ainda está em processamento"),
        "{}",
        stderr_of(&assert)
    );
}
