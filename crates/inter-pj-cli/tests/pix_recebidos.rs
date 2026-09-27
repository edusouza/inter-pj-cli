//! `inter-pj pix recebidos ...` end to end, against a mock API. All data is
//! synthetic.

mod common;

use common::{TestEnv, stderr_of, stdout_of};
use serde_json::{Value, json};
use wiremock::matchers::{any, method, path, query_param, query_param_is_missing};
use wiremock::{Mock, ResponseTemplate};

const E2E: &str = "E00416968202609181241abcdEFGH123";
const TXID: &str = "a1b2c3d4e5f60718293a4b5c6d7e8f90";

async fn env() -> TestEnv {
    let env = TestEnv::new().await;
    env.write_config("");
    env
}

fn pix(e2e: &str, valor: &str, devolucoes: &Value) -> Value {
    json!({
        "endToEndId": e2e,
        "txid": TXID,
        "valor": valor,
        "chave": "pix@empresa.example",
        "horario": "2026-09-18T12:41:07.000Z",
        "infoPagador": "Pedido 123",
        "devolucoes": devolucoes
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn lista_os_pix_do_periodo() {
    let env = env().await;
    env.mount_token("pix.read", None).await;
    Mock::given(method("GET"))
        .and(path("/pix/v2/pix"))
        .and(query_param("inicio", "2026-09-01T00:00:00-03:00"))
        .and(query_param("fim", "2026-09-30T23:59:59-03:00"))
        .and(query_param("txIdPresente", "true"))
        .and(query_param("devolucaoPresente", "true"))
        .and(query_param("cpf", "12345678909"))
        .and(query_param_is_missing("txId"))
        .and(query_param("paginacao.paginaAtual", "0"))
        .and(query_param("paginacao.itensPorPagina", "1000"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "parametros": {"paginacao": {"paginaAtual": 0, "itensPorPagina": 1000, "quantidadeDePaginas": 1}},
            "pix": [
                pix(E2E, "300.00", &json!([{"id": "D1", "valor": "50.00", "status": "DEVOLVIDO"}])),
                pix("E00416968202609181242abcdEFGH456", "150.00", &json!([{"id": "D2", "valor": "150.00", "status": "NAO_REALIZADO"}]))
            ]
        })))
        .expect(3)
        .mount(&env.server)
        .await;
    let args = [
        "pix",
        "recebidos",
        "listar",
        "--inicio",
        "2026-09-01T00:00:00-03:00",
        "--fim",
        "2026-09-30T23:59:59-03:00",
        "--com-cobranca",
        "--com-devolucao",
        "--documento",
        "123.456.789-09",
    ];
    let texto = stdout_of(&env.cmd().args(args).assert().success());
    assert!(
        texto.starts_with("Pix recebidos de 01/09/2026 00:00 a 30/09/2026 23:59 (de cobranças, com devolução, pagador 123.456.789-09)\n\nHorário"),
        "{texto}"
    );
    assert!(
        texto.contains(&format!("R$ 300,00   R$ 50,00  {E2E}  {TXID}")),
        "{texto}"
    );
    // A refund not made does not count.
    assert!(
        texto.ends_with("2 Pix · R$ 450,00 · devolvidos R$ 50,00\n"),
        "{texto}"
    );
    let json: Value = serde_json::from_str(&stdout_of(
        &env.cmd().args(args).arg("--json").assert().success(),
    ))
    .unwrap();
    assert_eq!(json["pix"].as_array().unwrap().len(), 2);
    let csv = stdout_of(
        &env.cmd()
            .args(args)
            .args(["--formato", "csv", "--separador", ";"])
            .assert()
            .success(),
    );
    assert!(
        csv.starts_with(
            "\u{feff}endToEndId;txid;valor;horario;chave;infoPagador;valorDevolvido\r\n"
        ),
        "{csv}"
    );
    assert!(csv.contains(";300,00;"), "{csv}");
    assert!(csv.contains(";Pedido 123;50,00\r\n"), "{csv}");
}

#[tokio::test(flavor = "multi_thread")]
async fn filtra_pela_cobranca_e_traz_uma_pagina() {
    let env = env().await;
    env.mount_token("pix.read", None).await;
    // --txid and --sem-cobranca contradict each other.
    env.cmd()
        .args([
            "pix",
            "recebidos",
            "listar",
            "--txid",
            TXID,
            "--sem-cobranca",
        ])
        .assert()
        .code(2);
    Mock::given(method("GET"))
        .and(path("/pix/v2/pix"))
        .and(query_param("txId", TXID))
        .and(query_param("paginacao.itensPorPagina", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "parametros": {"paginacao": {"paginaAtual": 0, "itensPorPagina": 1, "quantidadeDePaginas": 2, "quantidadeTotalDeItens": 2}},
            "pix": [pix(E2E, "300.00", &json!([]))]
        })))
        .expect(1)
        .mount(&env.server)
        .await;
    let texto = stdout_of(
        &env.cmd()
            .args([
                "pix",
                "recebidos",
                "listar",
                "--txid",
                TXID,
                "--pagina",
                "0",
                "--itens-por-pagina",
                "1",
            ])
            .assert()
            .success(),
    );
    assert!(
        texto.ends_with(
            "Página 0 de 1 (a primeira é 0); 2 Pix no período; a próxima é --pagina 1\n"
        ),
        "{texto}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn consulta_um_pix_e_suas_devolucoes() {
    let env = env().await;
    env.mount_token("pix.read", None).await;
    Mock::given(method("GET"))
        .and(path(format!("/pix/v2/pix/{E2E}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(pix(
            E2E,
            "300.00",
            &json!([{"id": "D1", "valor": "50.00", "status": "DEVOLVIDO", "horario": {"solicitacao": "2026-09-18T15:00:00Z"}}]),
        )))
        .expect(2)
        .mount(&env.server)
        .await;
    let texto = stdout_of(
        &env.cmd()
            .args(["pix", "recebidos", "consultar", E2E])
            .assert()
            .success(),
    );
    for linha in [
        &format!("Pix recebido {E2E}\n  Valor          R$ 300,00\n"),
        "  Devolvido      R$ 50,00\n",
        "  Pode devolver  R$ 250,00\n",
        "  Mensagem       Pedido 123\n",
        "D1  devolvida  R$ 50,00",
        &format!("Para devolver: inter-pj pix devolucao solicitar {E2E} --valor VALOR (ou --tudo)"),
    ] {
        assert!(texto.contains(linha), "{linha}\n{texto}");
    }
    let json: Value = serde_json::from_str(&stdout_of(
        &env.cmd()
            .args(["pix", "recebidos", "consultar", E2E, "--json"])
            .assert()
            .success(),
    ))
    .unwrap();
    assert_eq!(json["devolucoes"][0]["status"], "DEVOLVIDO");
}

#[tokio::test(flavor = "multi_thread")]
async fn identificadores_invalidos_nao_chamam_a_api() {
    let env = env().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&env.server)
        .await;
    for args in [
        &["pix", "recebidos", "consultar", "../../banking/v2/saldo"][..],
        &["pix", "recebidos", "consultar", "E0041 6968"][..],
        &["pix", "recebidos", "listar", "--txid", "curto"][..],
        &[
            "pix",
            "recebidos",
            "listar",
            "--com-devolucao",
            "--sem-devolucao",
        ][..],
        &[
            "pix",
            "recebidos",
            "listar",
            "--documento",
            "123.456.789-00",
        ][..],
    ] {
        let assert = env.cmd().args(args).assert().code(2);
        assert!(!stderr_of(&assert).is_empty());
    }
}
