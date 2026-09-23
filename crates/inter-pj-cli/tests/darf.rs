//! `inter-pj pagamento darf pagar|listar` end to end, against a mock API.
//! Every document, name and amount here is synthetic.

mod common;

use common::{TestEnv, stderr_of, stdout_of};
use serde_json::{Value, json};
use wiremock::matchers::{any, body_json, method, path, query_param, query_param_is_missing};
use wiremock::{Mock, ResponseTemplate};

const DARF: &str = "/banking/v2/pagamento/darf";
const SOLICITACAO: &str = "8bbdede4-35db-4ec9-b652-e176841e62c8";

const OPCOES: [&str; 16] = [
    "--codigo-receita",
    "0220",
    "--contribuinte",
    "12.345.678/0001-95",
    "--nome-empresa",
    "Empresa Exemplo",
    "--periodo-apuracao",
    "2026-09-30",
    "--vencimento",
    "2099-10-30",
    "--referencia",
    "13609400849201739",
    "--descricao",
    "IRPJ de setembro",
    "--valor-principal",
    "47,14",
];

fn corpo() -> Value {
    json!({
        "cnpjCpf": "12345678000195",
        "codigoReceita": "0220",
        "dataVencimento": "2099-10-30",
        "descricao": "IRPJ de setembro",
        "nomeEmpresa": "Empresa Exemplo",
        "periodoApuracao": "2026-09-30",
        "valorPrincipal": 47.14,
        "referencia": "13609400849201739"
    })
}

/// The API must not be called at all.
async fn nothing_is_sent(env: &TestEnv) {
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&env.server)
        .await;
}

async fn mount_pagamento(env: &TestEnv, corpo: Value) {
    env.mount_token("pagamento-darf.write", Some(1)).await;
    Mock::given(method("POST"))
        .and(path(DARF))
        .and(body_json(corpo))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "quantidadeAprovadores": 0,
            "dataPagamento": "01/10/2026",
            "tipoRetorno": "PAGAMENTO",
            "codigoSolicitacao": SOLICITACAO
        })))
        .expect(1)
        .mount(&env.server)
        .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn simulacao_de_darf_pelas_opcoes() {
    let env = TestEnv::new().await;
    env.write_config("");
    nothing_is_sent(&env).await;

    let assert = env
        .cmd()
        .args(["pagamento", "darf", "pagar"])
        .args(OPCOES)
        .args(["--juros", "10,11", "--simular", "--json"])
        .assert()
        .success();
    let json: Value = serde_json::from_str(&stdout_of(&assert)).unwrap();
    let mut esperado = corpo();
    esperado["valorJuros"] = json!(10.11);
    assert_eq!(json["corpo"], esperado);
    assert_eq!(
        json["url"],
        format!("{}/banking/v2/pagamento/darf", env.server.uri())
    );
    let stderr = stderr_of(&assert);
    for linha in [
        "DARF a pagar",
        "Contribuinte         Empresa Exemplo (12.345.678/0001-95)",
        "Juros                R$ 10,11",
        "Total                R$ 57,25 (cinquenta e sete reais e vinte e cinco centavos)",
    ] {
        assert!(stderr.contains(linha), "{linha}\n{stderr}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn paga_um_darf_de_um_arquivo() {
    let env = TestEnv::new().await;
    env.write_config("");
    mount_pagamento(&env, corpo()).await;
    let arquivo = env.path("darf.json");
    std::fs::write(&arquivo, serde_json::to_string_pretty(&corpo()).unwrap()).unwrap();

    let assert = env
        .cmd()
        .args(["pagamento", "darf", "pagar", "--arquivo"])
        .arg(&arquivo)
        .arg("--sim")
        .assert()
        .success();
    assert_eq!(
        stdout_of(&assert),
        format!(
            "\
DARF pago.
Código da solicitação  {SOLICITACAO}
Data do pagamento      01/10/2026

Acompanhe com: inter-pj pagamento darf listar --codigo-solicitacao {SOLICITACAO}
"
        )
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn darf_pela_entrada_padrao_exige_sim() {
    let env = TestEnv::new().await;
    env.write_config("");
    let json = corpo().to_string();

    // The input is the file, so no one can answer the question.
    let sem_sim = TestEnv::new().await;
    sem_sim.write_config("");
    nothing_is_sent(&sem_sim).await;
    let assert = sem_sim
        .cmd()
        .args(["pagamento", "darf", "pagar", "--arquivo", "-"])
        .write_stdin(json.clone())
        .assert()
        .code(2);
    assert!(
        stderr_of(&assert).contains("--sim"),
        "{}",
        stderr_of(&assert)
    );

    mount_pagamento(&env, corpo()).await;
    env.cmd()
        .args(["pagamento", "darf", "pagar", "--arquivo", "-", "--sim"])
        .write_stdin(json)
        .assert()
        .success();
}

#[tokio::test(flavor = "multi_thread")]
async fn arquivo_invalido_nao_chama_a_api() {
    let env = TestEnv::new().await;
    env.write_config("");
    nothing_is_sent(&env).await;
    let arquivo = env.path("darf.json");

    let mut com_erro = corpo();
    com_erro["valorMuta"] = json!(27.48);
    for (conteudo, esperado) in [
        (
            com_erro.to_string(),
            "campo desconhecido \"valorMuta\"".to_owned(),
        ),
        (
            "{\"cnpjCpf\": ".to_owned(),
            "JSON inválido na linha 1".to_owned(),
        ),
        (
            json!({"cnpjCpf": "12345678000195"}).to_string(),
            "campo \"codigoReceita\": obrigatório".to_owned(),
        ),
    ] {
        std::fs::write(&arquivo, conteudo).unwrap();
        let assert = env
            .cmd()
            .args(["pagamento", "darf", "pagar", "--arquivo"])
            .arg(&arquivo)
            .arg("--sim")
            .assert()
            .code(2);
        let stderr = stderr_of(&assert);
        assert!(stderr.contains(&esperado), "{esperado}\n{stderr}");
    }

    // An amount above the limit is refused even with --sim.
    env.write_config("limite_por_operacao = \"40,00\"");
    std::fs::write(&arquivo, corpo().to_string()).unwrap();
    let assert = env
        .cmd()
        .args(["pagamento", "darf", "pagar", "--arquivo"])
        .arg(&arquivo)
        .arg("--sim")
        .assert()
        .code(2);
    assert!(
        stderr_of(&assert).contains("R$ 47,14 passa do limite por operação"),
        "{}",
        stderr_of(&assert)
    );
}

fn darfs() -> Value {
    json!([{
        "codigoSolicitacao": SOLICITACAO,
        "valor": 47.14,
        "valorTotal": 57.25,
        "periodoApuracao": "2026-09-30 00:00:00",
        "dataPagamento": "2026-10-01 10:00:00",
        "dataVencimento": "2026-10-30 00:00:00",
        "codigoReceita": "0220",
        "statusPagamento": "REALIZADO"
    }])
}

#[tokio::test(flavor = "multi_thread")]
async fn lista_darfs_de_um_periodo() {
    let env = TestEnv::new().await;
    env.write_config("");
    env.mount_token("pagamento-boleto.read", None).await;
    Mock::given(method("GET"))
        .and(path(DARF))
        .and(query_param("dataInicio", "2026-10-01"))
        .and(query_param("dataFim", "2026-10-31"))
        .and(query_param("codigoReceita", "0220"))
        .respond_with(ResponseTemplate::new(200).set_body_json(darfs()))
        .expect(2)
        .mount(&env.server)
        .await;
    let listar = |extra: &[&str]| {
        let mut cmd = env.cmd();
        cmd.args(["pagamento", "darf", "listar"])
            .args(["--inicio", "2026-10-01", "--fim", "2026-10-31"])
            .args(["--codigo-receita", "0220"])
            .args(extra);
        cmd
    };

    let texto = stdout_of(&listar(&[]).assert().success());
    assert!(
        texto.starts_with("DARFs pagos de 01/10/2026 a 31/10/2026\nCódigo da receita: 0220\n"),
        "{texto}"
    );
    assert!(
        texto.contains(&format!(
            "01/10/2026  0220     30/09/2026  30/10/2026  pago    R$ 57,25  {SOLICITACAO}"
        )),
        "{texto}"
    );
    let json: Value =
        serde_json::from_str(&stdout_of(&listar(&["--json"]).assert().success())).unwrap();
    assert_eq!(json["darfs"][0]["codigoSolicitacao"], SOLICITACAO);
}

#[tokio::test(flavor = "multi_thread")]
async fn sem_datas_vale_o_padrao_da_api() {
    let env = TestEnv::new().await;
    env.write_config("");
    env.mount_token("pagamento-boleto.read", None).await;
    Mock::given(method("GET"))
        .and(path(DARF))
        .and(query_param_is_missing("dataInicio"))
        .and(query_param_is_missing("dataFim"))
        .and(query_param("codigoSolicitacao", SOLICITACAO))
        .respond_with(ResponseTemplate::new(200).set_body_string("null"))
        .expect(1)
        .mount(&env.server)
        .await;

    let assert = env
        .cmd()
        .args([
            "pagamento",
            "darf",
            "listar",
            "--codigo-solicitacao",
            SOLICITACAO,
        ])
        .args(["--formato", "csv"])
        .assert()
        .success();
    assert_eq!(
        stdout_of(&assert),
        "codigoSolicitacao,tipoDarf,tipo,codigoReceita,periodoApuracao,dataVencimento,dataPagamento,dataInclusao,valor,valorMulta,valorJuros,valorTotal,referencia,statusPagamento,cnpjCpf,aprovacoesNecessarias,aprovacoesRealizadas\r\n"
    );
}
