//! `inter-pj pix loc ...` end to end, against a mock API. All data is
//! synthetic.

mod common;

use common::{TestEnv, stderr_of, stdout_of};
use serde_json::{Value, json};
use wiremock::matchers::{any, body_json, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

const TXID: &str = "cobvexemplo0000000000000000001";
const LOCATION: &str = "pix.example.com/qr/v2/cobv/5b7e4c1a5d3f4a2b9c8d7e6f5a4b3c2d";

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

fn loc(id: u64, txid: Option<&str>) -> Value {
    let mut loc = json!({
        "id": id,
        "location": LOCATION,
        "tipoCob": "cobv",
        "criacao": "2026-09-24T13:10:00.000Z"
    });
    if let Some(txid) = txid {
        loc["txid"] = json!(txid);
    }
    loc
}

#[tokio::test(flavor = "multi_thread")]
async fn cria_uma_location_para_usar_com_loc() {
    let env = env().await;
    env.mount_token("payloadlocation.write", None).await;
    Mock::given(method("POST"))
        .and(path("/pix/v2/loc"))
        .and(body_json(json!({"tipoCob": "cobv"})))
        .respond_with(ResponseTemplate::new(201).set_body_json(loc(790, None)))
        .expect(2)
        .mount(&env.server)
        .await;
    let texto = stdout_of(
        &env.cmd()
            .args(["pix", "loc", "criar", "--tipo", "cobv"])
            .assert()
            .success(),
    );
    assert!(
        texto.starts_with(
            "Location criada.\n\nLocation 790\n  Tipo       cobrança com vencimento\n"
        ),
        "{texto}"
    );
    assert!(texto.contains("  Cobrança   nenhuma\n"), "{texto}");
    assert!(
        texto.ends_with("Use com: inter-pj pix cobv criar ... --loc 790\n"),
        "{texto}"
    );
    let json: Value = serde_json::from_str(&stdout_of(
        &env.cmd()
            .args(["pix", "loc", "criar", "--tipo", "COBV", "--json"])
            .assert()
            .success(),
    ))
    .unwrap();
    assert_eq!(json, loc(790, None));
}

#[tokio::test(flavor = "multi_thread")]
async fn lista_as_locations_do_periodo() {
    let env = env().await;
    env.mount_token("payloadlocation.read", None).await;
    Mock::given(method("GET"))
        .and(path("/pix/v2/loc"))
        .and(query_param("inicio", "2026-09-01T00:00:00-03:00"))
        .and(query_param("fim", "2026-09-30T23:59:59-03:00"))
        .and(query_param("txIdPresente", "true"))
        .and(query_param("tipoCob", "cobv"))
        .and(query_param("paginacao.itensPorPagina", "1000"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "parametros": {"paginacao": {"paginaAtual": 0, "itensPorPagina": 1000, "quantidadeDePaginas": 1}},
            "loc": [loc(790, Some(TXID)), loc(791, Some("cobvexemplo0000000000000000002"))]
        })))
        .expect(3)
        .mount(&env.server)
        .await;
    let args = [
        "pix",
        "loc",
        "listar",
        "--inicio",
        "2026-09-01T00:00:00-03:00",
        "--fim",
        "2026-09-30T23:59:59-03:00",
        "--tipo",
        "cobv",
        "--com-cobranca",
    ];
    let texto = stdout_of(&env.cmd().args(args).assert().success());
    assert!(
        texto.starts_with("Locations criadas de 01/09/2026 00:00 a 30/09/2026 23:59 (para cobrança com vencimento, com cobrança)\n\nCriada em"),
        "{texto}"
    );
    assert!(
        texto.contains(&format!("790  cobv  {TXID}  {LOCATION}")),
        "{texto}"
    );
    assert!(texto.ends_with("2 locations · 2 com cobrança\n"), "{texto}");
    let json: Value = serde_json::from_str(&stdout_of(
        &env.cmd().args(args).arg("--json").assert().success(),
    ))
    .unwrap();
    assert_eq!(json["loc"][1]["id"], 791);
    let csv = stdout_of(
        &env.cmd()
            .args(args)
            .args(["--formato", "csv"])
            .assert()
            .success(),
    );
    assert!(
        csv.starts_with(&format!(
            "id,tipoCob,txid,criacao,location\r\n790,cobv,{TXID},2026-09-24T13:10:00.000Z,{LOCATION}\r\n"
        )),
        "{csv}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn consulta_e_desvincula_a_cobranca() {
    let env = env().await;
    env.mount_token("payloadlocation.write payloadlocation.read", None)
        .await;
    Mock::given(method("GET"))
        .and(path("/pix/v2/loc/790"))
        .respond_with(ResponseTemplate::new(200).set_body_json(loc(790, Some(TXID))))
        .expect(2)
        .mount(&env.server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/pix/v2/loc/790/txid"))
        .respond_with(ResponseTemplate::new(200).set_body_json(loc(790, None)))
        .expect(1)
        .mount(&env.server)
        .await;
    let texto = stdout_of(
        &env.cmd()
            .args(["pix", "loc", "consultar", "790"])
            .assert()
            .success(),
    );
    assert!(
        texto.ends_with(&format!("  Cobrança   {TXID}\n")),
        "{texto}"
    );
    let assert = env
        .cmd()
        .args(["pix", "loc", "desvincular", "790", "--sim"])
        .assert()
        .success();
    assert!(
        stdout_of(&assert).starts_with(&format!(
            "Cobrança {TXID} desvinculada: a location está livre.\n\nLocation 790\n"
        )),
        "{}",
        stdout_of(&assert)
    );
    let stderr = stderr_of(&assert);
    assert!(stderr.contains("Location 790 a desvincular"), "{stderr}");
    assert!(
        stderr.contains(&format!(
            "aviso: o QR Code desta location deixa de levar à cobrança {TXID}"
        )),
        "{stderr}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn sem_cobranca_ou_sem_confirmacao_nada_e_desvinculado() {
    let env = env().await;
    env.mount_token("payloadlocation.write payloadlocation.read", None)
        .await;
    Mock::given(method("GET"))
        .and(path("/pix/v2/loc/791"))
        .respond_with(ResponseTemplate::new(200).set_body_json(loc(791, None)))
        .expect(1)
        .mount(&env.server)
        .await;
    Mock::given(method("DELETE"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&env.server)
        .await;
    let assert = env
        .cmd()
        .args(["pix", "loc", "desvincular", "791", "--sim"])
        .assert()
        .code(2);
    assert!(
        stderr_of(&assert).contains("a location 791 não tem cobrança vinculada"),
        "{}",
        stderr_of(&assert)
    );

    let env = self::env().await;
    nothing_is_sent(&env).await;
    // Without a terminal and without --sim, not even the lookup.
    let assert = env
        .cmd()
        .args(["pix", "loc", "desvincular", "790"])
        .write_stdin("s\n")
        .assert()
        .code(2);
    assert!(
        stderr_of(&assert).contains("use --sim"),
        "{}",
        stderr_of(&assert)
    );
    for args in [
        &["pix", "loc", "criar", "--tipo", "boleto"][..],
        &["pix", "loc", "criar"][..],
        &["pix", "loc", "consultar", "abc"][..],
        &["pix", "loc", "consultar", "-1"][..],
        &["pix", "loc", "listar", "--com-cobranca", "--sem-cobranca"][..],
    ] {
        env.cmd().args(args).assert().code(2);
    }
}
