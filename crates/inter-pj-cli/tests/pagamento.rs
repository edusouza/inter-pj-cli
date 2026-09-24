//! `inter-pj pagamento boleto pagar|listar|cancelar` end to end, against a
//! mock API. The codes are examples of the API documentation and of its
//! sandbox (one with a due date moved to the current cycle); names,
//! documents and amounts are synthetic.

mod common;

use chrono::Days;
use common::{TestEnv, stderr_of, stdout_of};
use serde_json::{Value, json};
use wiremock::matchers::{any, body_json, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

const PAGAMENTO: &str = "/banking/v2/pagamento";
const TRANSACAO: &str = "3414f226-36fb-4d87-811e-cfd99911d845";
const SETEMBRO: [&str; 4] = ["--inicio", "2026-09-01", "--fim", "2026-09-30"];
/// R$ 30,10, due on 2026-10-10.
const BOLETO: &str = "07797.77705 11678.471159 90071.126347 1 15950000003010";
const BARRAS: &str = "07791159500000030107777011678471159007112634";
/// Water bill of the sandbox: R$ 65,33, no due date in the code.
const CONTA: &str = "82670000000653301602023123106000000002830894";

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
    let hoje = common::hoje();
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

// --- pagar -------------------------------------------------------------------------

fn daqui_a(dias: u64) -> String {
    common::hoje()
        .checked_add_days(Days::new(dias))
        .unwrap()
        .format("%Y-%m-%d")
        .to_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn simulacao_do_pagamento_mostra_a_requisicao_e_nao_envia_nada() {
    let env = TestEnv::new().await;
    env.write_config("conta_corrente = \"7654321\"");
    nothing_is_sent(&env).await;

    let assert = env
        .cmd()
        .args(["pagamento", "boleto", "pagar", BOLETO, "--simular"])
        .assert()
        .success();
    let stdout = stdout_of(&assert);
    assert!(
        stdout.starts_with("Simulação: nada foi enviado."),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("POST {}/banking/v2/pagamento", env.server.uri())),
        "{stdout}"
    );
    assert!(stdout.contains("x-conta-corrente: *****21"), "{stdout}");
    assert!(!stdout.contains("7654321"), "{stdout}");
    let stderr = stderr_of(&assert);
    for linha in [
        "Pagamento a enviar",
        "Tipo             boleto do banco 077",
        "Linha digitável  07797.77705 11678.471159 90071.126347 1 15950000003010",
        "Valor            R$ 30,10 (trinta reais e dez centavos)",
        "Vencimento       10/10/2026",
    ] {
        assert!(stderr.contains(linha), "{linha}\n{stderr}");
    }

    // Scheduled, in JSON: the body as the API documents it.
    let data = daqui_a(5);
    let assert = env
        .cmd()
        .args([
            "pagamento",
            "boleto",
            "pagar",
            BOLETO,
            "--simular",
            "--json",
        ])
        .args(["--data", &data, "--beneficiario", "12.345.678/0001-95"])
        .assert()
        .success();
    let json: Value = serde_json::from_str(&stdout_of(&assert)).unwrap();
    assert_eq!(json["metodo"], "POST");
    assert_eq!(
        json["corpo"],
        json!({
            "codBarraLinhaDigitavel": BARRAS,
            "valorPagar": "30.10",
            "dataPagamento": data,
            "dataVencimento": "2026-10-10",
            "cpfCnpjBeneficiario": "12345678000195"
        })
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn contas_e_tributos_pedem_o_vencimento() {
    let env = TestEnv::new().await;
    env.write_config("");
    nothing_is_sent(&env).await;

    let assert = env
        .cmd()
        .args(["pagamento", "boleto", "pagar", CONTA, "--simular"])
        .assert()
        .code(2);
    assert!(
        stderr_of(&assert).contains("informe --vencimento"),
        "{}",
        stderr_of(&assert)
    );

    let vencimento = daqui_a(10);
    let assert = env
        .cmd()
        .args(["pagamento", "boleto", "pagar", CONTA, "--simular", "--json"])
        .args(["--vencimento", &vencimento])
        .assert()
        .success();
    let json: Value = serde_json::from_str(&stdout_of(&assert)).unwrap();
    assert_eq!(json["corpo"]["valorPagar"], "65.33");
    assert_eq!(json["corpo"]["dataVencimento"], vencimento.as_str());
    assert!(
        stderr_of(&assert).contains("conta ou tributo: água e esgoto"),
        "{}",
        stderr_of(&assert)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn pagamento_sem_terminal_ou_acima_do_limite_nao_chama_a_api() {
    let env = TestEnv::new().await;
    env.write_config("limite_por_operacao = \"30,00\"");
    nothing_is_sent(&env).await;

    // No terminal and no --sim.
    env.cmd()
        .args(["pagamento", "boleto", "pagar", BOLETO, "--valor", "10"])
        .write_stdin("s\n")
        .assert()
        .code(2);

    // Above the limit, even with --sim.
    let assert = env
        .cmd()
        .args(["pagamento", "boleto", "pagar", BOLETO, "--sim"])
        .assert()
        .code(2);
    let stderr = stderr_of(&assert);
    assert!(
        stderr.contains("R$ 30,10 passa do limite por operação do perfil \"padrao\" (R$ 30,00)"),
        "{stderr}"
    );
    assert!(!stderr.contains("Pagamento a enviar"), "{stderr}");
}

#[tokio::test(flavor = "multi_thread")]
async fn paga_com_sim_e_mostra_como_acompanhar() {
    let env = TestEnv::new().await;
    env.write_config("");
    env.mount_token("pagamento-boleto.write", Some(1)).await;
    Mock::given(method("POST"))
        .and(path(PAGAMENTO))
        .and(body_json(json!({
            "codBarraLinhaDigitavel": BARRAS,
            "valorPagar": "31.00",
            "dataVencimento": "2026-10-10"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "quantidadeAprovadores": 0,
            "statusPagamento": "REALIZADO",
            "codigoTransacao": TRANSACAO
        })))
        .expect(1)
        .mount(&env.server)
        .await;

    let assert = env
        .cmd()
        .args([
            "pagamento",
            "boleto",
            "pagar",
            BOLETO,
            "--valor",
            "31",
            "--sim",
        ])
        .assert()
        .success();
    let stdout = stdout_of(&assert);
    assert!(stdout.starts_with("Pagamento realizado."), "{stdout}");
    assert!(
        stdout.contains(&format!(
            "Acompanhe com: inter-pj pagamento boleto listar --codigo-transacao {TRANSACAO}"
        )),
        "{stdout}"
    );
    let stderr = stderr_of(&assert);
    assert!(stderr.contains("Valor no código  R$ 30,10"), "{stderr}");
    assert!(
        stderr.contains(
            "aviso: o valor a pagar (R$ 31,00) é maior que o do código (R$ 30,10): confira juros e multa"
        ),
        "{stderr}"
    );
}

/// Without an idempotency key, a payment that may have been made is not
/// repeated: the error says how to check it first.
#[tokio::test(flavor = "multi_thread")]
async fn resultado_incerto_orienta_a_conferir_antes_de_repetir() {
    let env = TestEnv::new().await;
    env.write_config("");
    env.mount_token("pagamento-boleto.write", Some(1)).await;
    Mock::given(method("POST"))
        .and(path(PAGAMENTO))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&env.server)
        .await;

    let assert = env
        .cmd()
        .args(["pagamento", "boleto", "pagar", BOLETO, "--sim"])
        .assert()
        .code(9);
    let stderr = stderr_of(&assert);
    assert!(stderr.contains("pagar duas vezes"), "{stderr}");
    assert!(
        stderr.contains(&format!(
            "dica: confira antes de tentar de novo: inter-pj pagamento boleto listar --codigo {BARRAS}"
        )),
        "{stderr}"
    );
}

/// The check runs on the account of the payment: the profile or the file
/// chosen on the command line goes into the command it suggests, or it
/// would look at the default profile, find nothing and invite a second
/// payment. One chosen in the environment stays in the shell by itself.
#[tokio::test(flavor = "multi_thread")]
async fn a_conferencia_sugerida_e_na_mesma_conta() {
    let env = TestEnv::new().await;
    env.write_config(&format!(
        "\n[perfis.filial]\nambiente = \"sandbox\"\nclient_id = \"id-da-filial\"\ncertificado = '{}'\nchave_privada = '{}'",
        env.path("certificado.crt").display(),
        env.path("chave.key").display()
    ));
    env.mount_token("pagamento-boleto.write", None).await;
    Mock::given(method("POST"))
        .and(path(PAGAMENTO))
        .respond_with(ResponseTemplate::new(503))
        .expect(3)
        .mount(&env.server)
        .await;
    let config = env.config_path().display().to_string();
    for (opcoes, no_ambiente, chamada) in [
        (vec!["-p", "filial"], None, "inter-pj -p filial".to_owned()),
        (vec![], Some("filial"), "inter-pj".to_owned()),
        (
            vec!["--config", config.as_str(), "--perfil", "filial"],
            None,
            format!("inter-pj --config {config} -p filial"),
        ),
    ] {
        let mut cmd = env.cmd();
        if let Some(perfil) = no_ambiente {
            cmd.env("INTER_PERFIL", perfil);
        }
        let assert = cmd
            .args(&opcoes)
            .args(["pagamento", "boleto", "pagar", BOLETO, "--sim"])
            .assert()
            .code(9);
        let stderr = stderr_of(&assert);
        assert!(
            stderr.contains(&format!(
                "dica: confira antes de tentar de novo: {chamada} pagamento boleto listar --codigo {BARRAS}"
            )),
            "{opcoes:?}: {stderr}"
        );
    }
}
