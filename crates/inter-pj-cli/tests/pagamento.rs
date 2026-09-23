//! `inter-pj pagamento boleto listar|cancelar` end to end, against a mock
//! API. The codes are examples of the API documentation; names, documents
//! and amounts are synthetic.

mod common;

use chrono::{Days, Local};
use common::{TestEnv, stderr_of, stdout_of};
use serde_json::{Value, json};
use wiremock::matchers::{any, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

const PAGAMENTO: &str = "/banking/v2/pagamento";
const TRANSACAO: &str = "3414f226-36fb-4d87-811e-cfd99911d845";
const SETEMBRO: [&str; 4] = ["--inicio", "2026-09-01", "--fim", "2026-09-30"];

fn pagamentos() -> Value {
    json!([
        {
            "codigoTransacao": TRANSACAO,
            "codigoBarra": "07791929500000030107777011678471159007112634",
            "dataInclusao": "2026-09-20 10:00:00",
            "dataPagamento": "2026-10-09",
            "dataVencimentoTitulo": "2026-10-10",
            "valorNominal": 30.1,
            "statusPagamento": "AGENDADO",
            "nomeBeneficiario": "Fornecedor Exemplo",
            "cpfCnpjBeneficiario": "12345678000195"
        },
        {
            "codigoTransacao": "8bbdede4-35db-4ec9-b652-e176841e62c8",
            "dataPagamento": "2026-09-15",
            "dataVencimentoDigitada": "2026-09-15",
            "valorPago": 250.1,
            "statusPagamento": "REALIZADO",
            "nomeBeneficiario": "=Energia Exemplo"
        }
    ])
}

async fn env(scope: &str) -> TestEnv {
    let env = TestEnv::new().await;
    env.write_config("");
    env.mount_token(scope, None).await;
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
async fn lista_pagamentos_em_texto_json_e_csv() {
    let env = env("pagamento-boleto.read").await;
    Mock::given(method("GET"))
        .and(path(PAGAMENTO))
        .and(query_param("dataInicio", "2026-09-01"))
        .and(query_param("dataFim", "2026-09-30"))
        .and(query_param("filtrarDataPor", "PAGAMENTO"))
        .respond_with(ResponseTemplate::new(200).set_body_json(pagamentos()))
        .expect(3)
        .mount(&env.server)
        .await;
    let listar = |formato: &str| {
        let mut cmd = env.cmd();
        cmd.args(["pagamento", "boleto", "listar"])
            .args(SETEMBRO)
            .args(["--filtrar-por", "pagamento", "--formato", formato]);
        cmd
    };

    let texto = stdout_of(&listar("texto").assert().success());
    assert_eq!(
        texto,
        "\
Pagamentos realizados de 01/09/2026 a 30/09/2026

Vencimento  Pagamento   Beneficiário        Status        Valor  Código da transação
10/10/2026  09/10/2026  Fornecedor Exemplo  agendado   R$ 30,10  3414f226-36fb-4d87-811e-cfd99911d845
15/09/2026  15/09/2026  =Energia Exemplo    pago      R$ 250,10  8bbdede4-35db-4ec9-b652-e176841e62c8

2 pagamentos
"
    );

    let json: Value = serde_json::from_str(&stdout_of(&listar("json").assert().success())).unwrap();
    assert_eq!(json["pagamentos"][0]["codigoTransacao"], TRANSACAO);
    assert_eq!(json["pagamentos"][0]["valorNominal"], 30.1);
    assert_eq!(json["pagamentos"][1]["statusPagamento"], "REALIZADO");

    let csv = stdout_of(&listar("csv").assert().success());
    let linhas: Vec<&str> = csv.split("\r\n").collect();
    assert!(
        linhas[0].starts_with("codigoTransacao,codigoBarra,tipo,"),
        "{csv}"
    );
    // Texts that would run as formulas in a spreadsheet are neutralized.
    assert!(linhas[2].contains(",'=Energia Exemplo,"), "{csv}");
}

#[tokio::test(flavor = "multi_thread")]
async fn lista_os_ultimos_30_dias_por_padrao() {
    let env = env("pagamento-boleto.read").await;
    let hoje = Local::now().date_naive();
    let inicio = hoje.checked_sub_days(Days::new(29)).unwrap();
    Mock::given(method("GET"))
        .and(path(PAGAMENTO))
        .and(query_param(
            "dataInicio",
            inicio.format("%Y-%m-%d").to_string(),
        ))
        .and(query_param("dataFim", hoje.format("%Y-%m-%d").to_string()))
        .and(query_param("codigoTransacao", TRANSACAO))
        .respond_with(ResponseTemplate::new(200).set_body_string("null"))
        .expect(1)
        .mount(&env.server)
        .await;

    let assert = env
        .cmd()
        .args(["pagamento", "boleto", "listar", "--codigo-transacao"])
        .arg(TRANSACAO.to_uppercase())
        .assert()
        .success();
    let texto = stdout_of(&assert);
    assert!(texto.ends_with("Nenhum pagamento encontrado.\n"), "{texto}");
    assert!(
        stderr_of(&assert).contains("ambiente sandbox"),
        "{}",
        stderr_of(&assert)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn listagem_invalida_nao_chama_a_api() {
    let env = TestEnv::new().await;
    env.write_config("");
    nothing_is_sent(&env).await;

    let assert = env
        .cmd()
        .args([
            "pagamento",
            "boleto",
            "listar",
            "--inicio",
            "2026-01-01",
            "--fim",
            "2026-06-30",
        ])
        .assert()
        .code(2);
    let stderr = stderr_of(&assert);
    assert!(stderr.contains("no máximo 90 dias"), "{stderr}");
    assert!(
        stderr.contains("dica: consulte um período de até 90 dias"),
        "{stderr}"
    );

    for args in [
        // A typo in the first field fails its check digit.
        &[
            "--codigo",
            "17797777051167847115990071126347192950000003010",
        ][..],
        &["--codigo-transacao", "../../banking/v2/saldo"],
    ] {
        env.cmd()
            .args(["pagamento", "boleto", "listar"])
            .args(args)
            .assert()
            .code(2);
    }
}

async fn mount_busca(env: &TestEnv, body: Value) {
    Mock::given(method("GET"))
        .and(path(PAGAMENTO))
        .and(query_param("codigoTransacao", TRANSACAO))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .expect(1)
        .mount(&env.server)
        .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn cancela_um_agendamento_com_sim() {
    let env = env("pagamento-boleto.read pagamento-boleto.write").await;
    mount_busca(&env, json!([pagamentos()[0].clone()])).await;
    Mock::given(method("DELETE"))
        .and(path(format!("{PAGAMENTO}/{TRANSACAO}")))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&env.server)
        .await;

    let assert = env
        .cmd()
        .args([
            "pagamento",
            "boleto",
            "cancelar",
            TRANSACAO,
            "--sim",
            "--json",
        ])
        .assert()
        .success();
    let json: Value = serde_json::from_str(&stdout_of(&assert)).unwrap();
    assert_eq!(
        json,
        json!({"codigoTransacao": TRANSACAO, "cancelado": true})
    );
    let stderr = stderr_of(&assert);
    for linha in [
        "Agendamento a cancelar",
        "Beneficiário         Fornecedor Exemplo (12.345.678/0001-95)",
        "Valor                R$ 30,10",
        "Pagamento em         09/10/2026",
        "Status               agendado",
    ] {
        assert!(stderr.contains(linha), "{linha}\n{stderr}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn cancelamento_sem_terminal_nao_chama_a_api() {
    let env = TestEnv::new().await;
    env.write_config("");
    nothing_is_sent(&env).await;

    let assert = env
        .cmd()
        .args(["pagamento", "boleto", "cancelar", TRANSACAO])
        .write_stdin("s\n")
        .assert()
        .code(2);
    assert!(
        stderr_of(&assert).contains("--sim"),
        "{}",
        stderr_of(&assert)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn cancelamento_recusado_pela_api() {
    let env = env("pagamento-boleto.read pagamento-boleto.write").await;
    mount_busca(&env, json!([])).await;
    Mock::given(method("DELETE"))
        .and(path(format!("{PAGAMENTO}/{TRANSACAO}")))
        .respond_with(ResponseTemplate::new(422).set_body_json(json!({
            "title": "Não é possível cancelar",
            "detail": "O pagamento já foi realizado."
        })))
        .expect(1)
        .mount(&env.server)
        .await;

    let assert = env
        .cmd()
        .args(["pagamento", "boleto", "cancelar", TRANSACAO, "--sim"])
        .assert()
        .code(5);
    let stderr = stderr_of(&assert);
    assert!(
        stderr.contains("não encontrado entre os incluídos nos últimos 30 dias"),
        "{stderr}"
    );
    assert!(stderr.contains("O pagamento já foi realizado."), "{stderr}");
    assert!(stdout_of(&assert).is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn ajuda_em_portugues() {
    let env = TestEnv::new().await;
    let assert = env
        .cmd()
        .args(["pagamento", "boleto", "listar", "--help"])
        .assert()
        .success();
    let ajuda = stdout_of(&assert);
    assert!(
        ajuda.contains("Uso: inter-pj pagamento boleto listar [OPÇÕES]"),
        "{ajuda}"
    );
    assert!(ajuda.contains("--filtrar-por <DATA>"), "{ajuda}");
    assert!(!ajuda.contains("Usage:"), "{ajuda}");

    let ajuda = stdout_of(&env.cmd().args(["pagamento", "--help"]).assert().success());
    assert!(ajuda.contains("boleto"), "{ajuda}");
}
