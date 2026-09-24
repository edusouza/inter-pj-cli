//! `inter-pj webhook banking|cobranca|pix callbacks|reenviar` end to end,
//! against a mock API. All data is synthetic.

mod common;

use common::{TestEnv, stderr_of, stdout_of};
use serde_json::{Value, json};
use wiremock::matchers::{any, body_json, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

const URL: &str = "https://api.empresa.example/inter/webhook";
const UM: &str = "0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d";
const DOIS: &str = "1c8f5d2b-6e4a-4b3c-8d9e-8f7a6b5c4d3e";
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

fn tentativa(codigo: &str, numero: u64, sucesso: bool) -> Value {
    let mut tentativa = json!({
        "webhookUrl": URL,
        "payload": [{"codigoSolicitacao": codigo, "situacao": "RECEBIDO"}],
        "numeroTentativa": numero,
        "dataHoraDisparo": "2026-09-24T13:45:00Z",
        "sucesso": sucesso,
        "httpStatus": if sucesso { 200 } else { 503 }
    });
    if !sucesso {
        tentativa["mensagemErro"] = "Service Unavailable".into();
    }
    tentativa
}

/// 51 distinct codes of charges.
fn codigos() -> Vec<String> {
    (0..51)
        .map(|n| format!("0b7e4c1a-5d3f-4a2b-9c8d-{n:012x}"))
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn lista_os_callbacks_de_cobrancas_e_diz_o_que_reenviar() {
    let env = env().await;
    env.mount_token("boleto-cobranca.read", None).await;
    for (pagina, ultima, dados) in [
        (
            "0",
            false,
            json!([tentativa(UM, 2, true), tentativa(DOIS, 1, false)]),
        ),
        ("1", true, json!([tentativa(UM, 1, false)])),
    ] {
        Mock::given(method("GET"))
            .and(path("/cobranca/v3/cobrancas/webhook/callbacks"))
            .and(query_param("pagina", pagina))
            .and(query_param("tamanhoPagina", "50"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "totalElementos": 3, "totalPaginas": 2, "ultimaPagina": ultima, "data": dados
            })))
            .mount(&env.server)
            .await;
    }
    let periodo = ["--inicio", "2026-09-01", "--fim", "2026-09-30"];
    let texto = stdout_of(
        &env.cmd()
            .args(["webhook", "cobranca", "callbacks"])
            .args(periodo)
            .assert()
            .success(),
    );
    assert!(
        texto.starts_with(
            "Callbacks do webhook de cobranças de 01/09/2026 00:00 a 30/09/2026 23:59\n\nDisparo"
        ),
        "{texto}"
    );
    let cabecalho = texto.lines().nth(2).unwrap();
    for coluna in [
        "Tentativa",
        "Entregue",
        "HTTP",
        "Código da cobrança",
        "Erro",
    ] {
        assert!(cabecalho.contains(coluna), "{cabecalho}");
    }
    assert!(
        texto.contains("3 tentativas · 1 entregue · 2 falharam\n"),
        "{texto}"
    );
    // The charge delivered on the second attempt is not listed again.
    assert!(
        texto.ends_with(&format!(
            "Sem entrega no período: 1 operação. Para pedir o reenvio:\n  inter-pj webhook cobranca reenviar {DOIS}\n"
        )),
        "{texto}"
    );

    let json: Value = serde_json::from_str(&stdout_of(
        &env.cmd()
            .args(["webhook", "cobranca", "callbacks", "--falhas", "--json"])
            .args(periodo)
            .assert()
            .success(),
    ))
    .unwrap();
    assert_eq!(
        json,
        json!({"callbacks": [tentativa(DOIS, 1, false), tentativa(UM, 1, false)]})
    );

    let csv = stdout_of(
        &env.cmd()
            .args(["webhook", "cobranca", "callbacks", "--formato", "csv"])
            .args(periodo)
            .assert()
            .success(),
    );
    assert!(
        csv.starts_with(&format!(
            "dataHoraDisparo,numeroTentativa,sucesso,httpStatus,codigoSolicitacao,mensagemErro,webhookUrl\r\n2026-09-24T13:45:00Z,2,true,200,{UM},,{URL}\r\n"
        )),
        "{csv}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn uma_pagina_so_diz_onde_esta() {
    let env = env().await;
    env.mount_token("webhook.read", None).await;
    Mock::given(method("GET"))
        .and(path("/pix/v2/webhook/callbacks"))
        .and(query_param("pagina", "0"))
        .and(query_param("tamanhoPagina", "10"))
        .and(query_param("txid", TXID))
        .and(query_param("dataHoraInicio", "2026-09-24T00:00:00-03:00"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "totalElementos": 12, "totalPaginas": 2, "primeiraPagina": true, "ultimaPagina": false,
            "data": [{
                "webhookUrl": URL,
                "payload": {"pix": [{"endToEndId": E2E, "txid": TXID, "chave": "pix@empresa.example", "valor": "37.00"}]},
                "numeroTentativa": 1, "dataHoraDisparo": "2026-09-24T13:10:02Z", "sucesso": true, "httpStatus": 200
            }]
        })))
        .expect(1)
        .mount(&env.server)
        .await;
    let texto = stdout_of(
        &env.cmd()
            .args([
                "webhook",
                "pix",
                "callbacks",
                "--txid",
                TXID,
                "--inicio",
                "2026-09-24T00:00:00-03:00",
                "--fim",
                "2026-09-24T23:59:59-03:00",
                "--pagina",
                "0",
                "--itens-por-pagina",
                "10",
            ])
            .assert()
            .success(),
    );
    assert!(texto.contains(&format!("(txid {TXID})\n\n")), "{texto}");
    assert!(texto.contains(TXID), "{texto}");
    assert!(
        texto.ends_with(
            "1 tentativa · 1 entregue · 0 falharam\n\nPágina 0 de 1 (a primeira é 0); 12 callbacks no período; a próxima é --pagina 1\n"
        ),
        "{texto}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn o_banking_filtra_pelo_identificador_de_cada_tipo() {
    let env = env().await;
    env.mount_token("webhook-banking.read", None).await;
    Mock::given(method("GET"))
        .and(path("/banking/v2/webhooks/pix-pagamento/callbacks"))
        .and(query_param("endToEnd", E2E))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "totalElementos": 1, "totalPaginas": 1, "ultimaPagina": true,
            "data": [{
                "webhookUrl": URL, "payload": {"codigoSolicitacao": UM, "endToEnd": E2E},
                "numeroTentativa": 1, "dataEnvio": "2026-09-24T13:10:00Z", "sucesso": true, "httpStatus": 200
            }]
        })))
        .expect(1)
        .mount(&env.server)
        .await;
    let texto = stdout_of(
        &env.cmd()
            .args([
                "webhook",
                "banking",
                "callbacks",
                "pix-pagamento",
                "--end-to-end",
                E2E,
                "--inicio",
                "2026-09-01",
                "--fim",
                "2026-09-30",
            ])
            .assert()
            .success(),
    );
    assert!(
        texto.contains("Callbacks do webhook pix-pagamento de "),
        "{texto}"
    );
    // The code `reenviar` takes, not the endToEnd of the filter.
    assert!(texto.contains("Código da solicitação"), "{texto}");
    assert!(texto.contains(UM), "{texto}");

    let assert = env
        .cmd()
        .args([
            "webhook",
            "banking",
            "callbacks",
            "pix-pagamento",
            "--codigo-transacao",
            UM,
        ])
        .assert()
        .code(2);
    assert!(
        stderr_of(&assert).contains("--codigo-transacao vale para boleto-pagamento"),
        "{}",
        stderr_of(&assert)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn reenvia_mais_de_50_em_blocos_de_50() {
    let env = env().await;
    env.mount_token("boleto-cobranca.write", None).await;
    let codigos = codigos();
    let (primeiro, segundo) = codigos.split_at(50);
    Mock::given(method("POST"))
        .and(path("/cobranca/v3/cobrancas/webhook/callbacks/retry"))
        .and(body_json(json!({"codigoSolicitacao": primeiro})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"foundIds": primeiro})))
        .expect(1)
        .mount(&env.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/cobranca/v3/cobrancas/webhook/callbacks/retry"))
        .and(body_json(json!({"codigoSolicitacao": segundo})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"foundIds": []})))
        .expect(1)
        .mount(&env.server)
        .await;
    // A repeated code goes once.
    let mut args: Vec<&str> = vec!["webhook", "cobranca", "reenviar"];
    args.extend(codigos.iter().map(String::as_str));
    args.push(&codigos[0]);
    let texto = stdout_of(&env.cmd().args(&args).assert().success());
    assert_eq!(
        texto,
        format!(
            "Reenvio pedido para 50 de 51 operações: o Inter vai enviar os callbacks de novo.\n\nNão encontradas:\n  {}\n",
            codigos[50]
        )
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn um_bloco_que_falha_diz_o_que_falta() {
    let env = env().await;
    env.mount_token("boleto-cobranca.write", None).await;
    let codigos = codigos();
    let (primeiro, segundo) = codigos.split_at(50);
    Mock::given(method("POST"))
        .and(path("/cobranca/v3/cobrancas/webhook/callbacks/retry"))
        .and(body_json(json!({"codigoSolicitacao": primeiro})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"foundIds": primeiro})))
        .expect(1)
        .mount(&env.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/cobranca/v3/cobrancas/webhook/callbacks/retry"))
        .and(body_json(json!({"codigoSolicitacao": segundo})))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "title": "Requisição inválida", "detail": "Não foi possível reenviar."
        })))
        .expect(1)
        .mount(&env.server)
        .await;
    let mut args: Vec<&str> = vec!["webhook", "cobranca", "reenviar"];
    args.extend(codigos.iter().map(String::as_str));
    let assert = env.cmd().args(&args).assert().failure();
    let erro = stderr_of(&assert);
    assert!(
        erro.contains("dica: o reenvio de 50 das 51 operações já foi pedido\n"),
        "{erro}"
    );
    assert!(
        erro.contains(&format!(
            "dica: para pedir o das outras: inter-pj webhook cobranca reenviar {}\n",
            codigos[50]
        )),
        "{erro}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn reenvia_callbacks_pix_de_uma_chave() {
    let env = env().await;
    env.mount_token("webhook.write", None).await;
    Mock::given(method("POST"))
        .and(path("/pix/v2/webhook/callbacks/retry"))
        .and(body_json(
            json!({"txId": [TXID], "chavePix": "pix@empresa.example"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"foundIds": [TXID]})))
        .expect(2)
        .mount(&env.server)
        .await;
    let texto = stdout_of(
        &env.cmd()
            .args(["webhook", "pix", "reenviar", "pix@empresa.example", TXID])
            .assert()
            .success(),
    );
    assert_eq!(
        texto,
        "Reenvio pedido para 1 de 1 operação: o Inter vai enviar os callbacks de novo.\n"
    );
    let json: Value = serde_json::from_str(&stdout_of(
        &env.cmd()
            .args([
                "webhook",
                "pix",
                "reenviar",
                "pix@empresa.example",
                TXID,
                "--json",
            ])
            .assert()
            .success(),
    ))
    .unwrap();
    assert_eq!(json, json!({"foundIds": [TXID]}));
}

#[tokio::test(flavor = "multi_thread")]
async fn codigos_invalidos_sao_recusados_antes_do_envio() {
    let env = env().await;
    nothing_is_sent(&env).await;
    for (args, mensagem) in [
        (
            &["webhook", "banking", "reenviar", "pix-pagamento", UM, "123"][..],
            "esperado um UUID",
        ),
        (
            &["webhook", "cobranca", "reenviar", "xyz"][..],
            "código da cobrança inválido",
        ),
        (
            &["webhook", "pix", "reenviar", "pix@empresa.example", "curto"][..],
            "txid",
        ),
        (&["webhook", "cobranca", "reenviar"][..], "CODIGO"),
        (
            &[
                "webhook",
                "cobranca",
                "callbacks",
                "--pagina",
                "0",
                "--itens-por-pagina",
                "5",
            ][..],
            "de 10 a 50",
        ),
        (
            &["webhook", "cobranca", "reenviar", UM, "--formato", "csv"][..],
            "o formato csv vale apenas para listagens",
        ),
    ] {
        let assert = env.cmd().args(args).assert().code(2);
        assert!(
            stderr_of(&assert).contains(mensagem),
            "{args:?}: {}",
            stderr_of(&assert)
        );
    }
}
