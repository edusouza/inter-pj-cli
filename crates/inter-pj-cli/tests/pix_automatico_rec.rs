//! `inter-pj pix-automatico rec ...` end to end, against a mock API. All data
//! is synthetic; the "copia e cola" is the example of the Banco Central's
//! manual.

mod common;

use common::{COPIA_E_COLA, TestEnv, ler_qr_code, stderr_of, stdout_of};
use serde_json::{Value, json};
use wiremock::matchers::{any, body_json, method, path, query_param, query_param_is_missing};
use wiremock::{Mock, ResponseTemplate};

const ID: &str = "RR1234567820260924abcdefghijk";
/// The QR Code of a recurrence alone (`JORNADA_2`), from the examples of
/// the API: the location of the recurrence is in field 80.
const QR_DA_RECORRENCIA: &str = "00020126180014br.gov.bcb.pix5204000053039865802BR5913Fulano de Tal6008BRASILIA62070503***80800014br.gov.bcb.pix2558pix.example.com/qr/v2/rec/2353c790eefb11eaadc10242ac120002630462C9";
const TXID: &str = "7978c0c97ea847e78e8849634473c1f1";

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
        "rec",
        "criar",
        "--devedor-documento",
        "123.456.789-09",
        "--devedor-nome",
        "Cliente Exemplo",
        "--contrato",
        "contrato-001",
        "--objeto",
        "Mensalidade",
        "--data-inicial",
        "2099-10-10",
        "--data-final",
        "2100-09-10",
        "--periodicidade",
        "mensal",
        "--valor",
        "149,90",
        "--retentativas",
        "--loc",
        "108",
    ]
}

/// `criar()` with another value for `opcao`.
fn criar_com(opcao: &str, valor: &str) -> Vec<String> {
    let mut args: Vec<String> = criar().into_iter().map(str::to_owned).collect();
    let i = args.iter().position(|arg| arg == opcao).unwrap();
    valor.clone_into(&mut args[i + 1]);
    args
}

fn corpo() -> Value {
    json!({
        "vinculo": {"objeto": "Mensalidade", "devedor": {"cpf": "12345678909", "nome": "Cliente Exemplo"}, "contrato": "contrato-001"},
        "calendario": {"dataInicial": "2099-10-10", "dataFinal": "2100-09-10", "periodicidade": "MENSAL"},
        "valor": {"valorRec": "149.90"},
        "politicaRetentativa": "PERMITE_3R_7D",
        "loc": 108
    })
}

fn rec(status: &str) -> Value {
    json!({
        "idRec": ID,
        "vinculo": {"objeto": "Mensalidade", "devedor": {"cpf": "12345678909", "nome": "Cliente Exemplo"}, "contrato": "contrato-001"},
        "calendario": {"dataInicial": "2099-10-10", "dataFinal": "2100-09-10", "periodicidade": "MENSAL"},
        "valor": {"valorRec": "149.90"},
        "recebedor": {"cnpj": "12345678000195", "nome": "Empresa Exemplo Ltda"},
        "status": status,
        "politicaRetentativa": "PERMITE_3R_7D",
        "loc": {"id": 108, "location": "pix.example.com/qr/v2/rec/2353c790eefb11eaadc10242ac120002", "criacao": "2026-09-24T13:00:00.000Z"},
        "atualizacao": [{"status": "CRIADA", "data": "2026-09-24T13:00:00.000Z"}]
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn cria_pelas_opcoes_e_diz_como_o_pagador_aprova() {
    let env = env().await;
    env.mount_token("rec.write", Some(1)).await;
    Mock::given(method("POST"))
        .and(path("/pix/v2/rec"))
        .and(body_json(corpo()))
        .respond_with(ResponseTemplate::new(201).set_body_json(rec("CRIADA")))
        .expect(1)
        .mount(&env.server)
        .await;

    let assert = env.cmd().args(criar()).arg("--sim").assert().success();
    let stdout = stdout_of(&assert);
    assert!(
        stdout.starts_with(&format!(
            "Recorrência criada: aguarda a aprovação do pagador.\n\nRecorrência {ID}\n"
        )),
        "{stdout}"
    );
    for linha in [
        format!("Acompanhe com: inter-pj pix-automatico rec consultar {ID}"),
        format!("peça com inter-pj pix-automatico solicitacao criar --rec {ID}"),
        format!("ou mostre o QR Code de inter-pj pix-automatico rec consultar {ID} --qrcode"),
    ] {
        assert!(stdout.contains(&linha), "{linha}\n{stdout}");
    }
    let stderr = stderr_of(&assert);
    for linha in [
        "Recorrência a criar",
        "Cliente Exemplo (123.456.789-09)",
        "mensal, de 10/10/2099 a 10/09/2100",
        "R$ 149,90 (cento e quarenta e nove reais e noventa centavos) em cada pagamento",
        "até 3 novas tentativas, em 7 dias",
    ] {
        assert!(stderr.contains(linha), "{linha}\n{stderr}");
    }
}

/// The dates of Pix Automático follow the bank's calendar, like the others:
/// with today fixed by `INTER_HOJE`, the first payment of yesterday has
/// passed and the model starts in 30 days.
#[tokio::test(flavor = "multi_thread")]
async fn datas_seguem_o_dia_do_banco() {
    let env = env().await;
    nothing_is_sent(&env).await;
    let assert = env
        .cmd()
        .env("INTER_HOJE", "2026-09-24")
        .args(criar_com("--data-inicial", "2026-09-23"))
        .arg("--sim")
        .assert()
        .code(2);
    assert!(
        stderr_of(&assert).contains("a data do primeiro pagamento (23/09/2026) já passou"),
        "{}",
        stderr_of(&assert)
    );
    let modelo = stdout_of(
        &env.cmd()
            .env("INTER_HOJE", "2026-09-24")
            .args(["pix-automatico", "rec", "modelo"])
            .assert()
            .success(),
    );
    assert!(
        modelo.contains("\"dataInicial\": \"2026-10-24\""),
        "{modelo}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn cria_pelo_arquivo_do_modelo() {
    let env = env().await;
    let modelo = env
        .cmd()
        .args(["pix-automatico", "rec", "modelo"])
        .assert()
        .success();
    let arquivo = env.path("rec.json");
    std::fs::write(&arquivo, stdout_of(&modelo)).unwrap();

    env.mount_token("rec.write", Some(1)).await;
    Mock::given(method("POST"))
        .and(path("/pix/v2/rec"))
        .respond_with(ResponseTemplate::new(201).set_body_json(rec("CRIADA")))
        .expect(1)
        .mount(&env.server)
        .await;
    env.cmd()
        .args(["pix-automatico", "rec", "criar", "--arquivo"])
        .arg(&arquivo)
        .args(["--sim", "--json"])
        .assert()
        .success();
    let enviado: Value = env.server.received_requests().await.unwrap()[1]
        .body_json()
        .unwrap();
    assert_eq!(enviado["vinculo"]["contrato"], "contrato-001");
    assert_eq!(enviado["valor"], json!({"valorRec": "149.90"}));
    assert_eq!(enviado["politicaRetentativa"], "PERMITE_3R_7D");
}

#[tokio::test(flavor = "multi_thread")]
async fn simulacao_e_erros_nao_chamam_a_api() {
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
    assert_eq!(json["url"], format!("{}/pix/v2/rec", env.server.uri()));
    assert_eq!(json["corpo"], corpo());

    // Without a terminal to confirm, nothing is sent either.
    let assert = env.cmd().args(criar()).write_stdin("s\n").assert().code(2);
    assert!(
        stderr_of(&assert).contains("use --sim"),
        "{}",
        stderr_of(&assert)
    );
    for (opcao, valor, erro) in [
        (
            "--data-inicial",
            "2020-01-01",
            "a data do primeiro pagamento (01/01/2020) já passou",
        ),
        ("--contrato", " ", "--contrato: "),
        ("--objeto", &"x".repeat(36), "--objeto: "),
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
    // A fixed amount and a minimum do not go together.
    let assert = env
        .cmd()
        .args(criar())
        .args(["--valor-minimo", "10", "--sim"])
        .assert()
        .code(2);
    assert!(
        stderr_of(&assert).contains("não é possível usar"),
        "{}",
        stderr_of(&assert)
    );
}

/// Without an idempotency key, the error says how to check before trying
/// again.
#[tokio::test(flavor = "multi_thread")]
async fn resultado_incerto_orienta_a_conferir_antes_de_repetir() {
    let env = env().await;
    env.mount_token("rec.write", Some(1)).await;
    Mock::given(method("POST"))
        .and(path("/pix/v2/rec"))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&env.server)
        .await;
    let assert = env.cmd().args(criar()).arg("--sim").assert().code(9);
    let stderr = stderr_of(&assert);
    for dica in [
        "dica: a recorrência pode ter sido criada, e esta API não tem chave de idempotência: repetir o comando pode criar outra",
        "dica: confira antes de tentar de novo: inter-pj pix-automatico rec listar --documento 12345678909",
    ] {
        assert!(stderr.contains(dica), "{dica}\n{stderr}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn lista_em_texto_json_e_csv() {
    let env = env().await;
    env.mount_token("rec.read", Some(1)).await;
    let mut aprovada = rec("APROVADA");
    aprovada["valor"] = json!({"valorMinimoRecebedor": "50.00"});
    Mock::given(method("GET"))
        .and(path("/pix/v2/rec"))
        .and(query_param("inicio", "2026-09-01T00:00:00-03:00"))
        .and(query_param("fim", "2026-09-30T23:59:59-03:00"))
        .and(query_param("status", "APROVADA"))
        .and(query_param("cpf", "12345678909"))
        .and(query_param_is_missing("cnpj"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "parametros": {"inicio": "2026-09-01T03:00:00Z", "fim": "2026-10-01T02:59:59Z", "paginacao": {"paginaAtual": 0, "itensPorPagina": 1000, "quantidadeDePaginas": 1, "quantidadeTotalDeItens": 2}},
            "recs": [rec("APROVADA"), aprovada]
        })))
        .expect(3)
        .mount(&env.server)
        .await;
    let listar = [
        "pix-automatico",
        "rec",
        "listar",
        "--inicio",
        "2026-09-01T00:00:00-03:00",
        "--fim",
        "2026-09-30T23:59:59-03:00",
        "--status",
        "aprovada",
        "--documento",
        "123.456.789-09",
    ];
    let texto = stdout_of(&env.cmd().args(listar).assert().success());
    assert!(
        texto.starts_with("Recorrências criadas de 01/09/2026 00:00 a 30/09/2026 23:59 (aprovada, devedor 123.456.789-09)\n\nStatus"),
        "{texto}"
    );
    assert!(texto.contains("R$ 149,90"), "{texto}");
    assert!(texto.contains("R$ 50,00"), "{texto}");
    assert!(
        texto.trim_end().ends_with("2 recorrências · 2 aprovadas"),
        "{texto}"
    );

    let json: Value = serde_json::from_str(&stdout_of(
        &env.cmd().args(listar).arg("--json").assert().success(),
    ))
    .unwrap();
    assert_eq!(json["recs"].as_array().unwrap().len(), 2);
    let csv = stdout_of(
        &env.cmd()
            .args(listar)
            .args(["--formato", "csv"])
            .assert()
            .success(),
    );
    assert!(csv.starts_with("idRec,status,vinculo.contrato,"), "{csv}");
    assert_eq!(csv.lines().count(), 3, "{csv}");
}

#[tokio::test(flavor = "multi_thread")]
async fn consulta_com_o_qr_code_composto() {
    let env = env().await;
    env.mount_token("rec.read", Some(1)).await;
    let mut composta = rec("CRIADA");
    composta["dadosQR"] = json!({"jornada": "JORNADA_3", "pixCopiaECola": COPIA_E_COLA});
    Mock::given(method("GET"))
        .and(path(format!("/pix/v2/rec/{ID}")))
        .and(query_param("txid", TXID))
        .respond_with(ResponseTemplate::new(200).set_body_json(composta))
        .expect(1)
        .mount(&env.server)
        .await;
    let assert = env
        .cmd()
        .args([
            "pix-automatico",
            "rec",
            "consultar",
            ID,
            "--txid",
            TXID,
            "--qrcode",
        ])
        .assert()
        .success();
    let stdout = stdout_of(&assert);
    assert!(
        stdout.starts_with(&format!("Recorrência {ID}\n")),
        "{stdout}"
    );
    assert!(
        stdout.contains("Status         criada (aguarda a aprovação do pagador)"),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("Copia e cola  {COPIA_E_COLA}")),
        "{stdout}"
    );
    assert_eq!(ler_qr_code(&stdout), COPIA_E_COLA);
}

#[tokio::test(flavor = "multi_thread")]
async fn consulta_com_o_qr_code_da_recorrencia() {
    let env = env().await;
    env.mount_token("rec.read", Some(1)).await;
    let mut com_location = rec("CRIADA");
    com_location["dadosQR"] = json!({"jornada": "JORNADA_2", "pixCopiaECola": QR_DA_RECORRENCIA});
    Mock::given(method("GET"))
        .and(path(format!("/pix/v2/rec/{ID}")))
        .and(query_param_is_missing("txid"))
        .respond_with(ResponseTemplate::new(200).set_body_json(com_location))
        .expect(1)
        .mount(&env.server)
        .await;
    let assert = env
        .cmd()
        .args(["pix-automatico", "rec", "consultar", ID, "--qrcode"])
        .assert()
        .success();
    let stdout = stdout_of(&assert);
    assert!(
        stdout.contains(&format!("Copia e cola  {QR_DA_RECORRENCIA}")),
        "{stdout}"
    );
    assert_eq!(ler_qr_code(&stdout), QR_DA_RECORRENCIA);
    let stderr = stderr_of(&assert);
    assert!(!stderr.contains("inválido"), "{stderr}");
}

#[tokio::test(flavor = "multi_thread")]
async fn revisa_mostrando_o_antes_e_o_depois() {
    let env = env().await;
    env.mount_token("rec.read rec.write", Some(1)).await;
    Mock::given(method("GET"))
        .and(path(format!("/pix/v2/rec/{ID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(rec("CRIADA")))
        .expect(1)
        .mount(&env.server)
        .await;
    let mut revisada = rec("CRIADA");
    revisada["vinculo"]["devedor"]["nome"] = json!("Cliente Exemplo Ltda");
    Mock::given(method("PATCH"))
        .and(path(format!("/pix/v2/rec/{ID}")))
        .and(body_json(json!({
            "vinculo": {"devedor": {"nome": "Cliente Exemplo Ltda"}},
            "calendario": {"dataInicial": "2099-11-10"}
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(revisada))
        .expect(1)
        .mount(&env.server)
        .await;
    let assert = env
        .cmd()
        .args([
            "pix-automatico",
            "rec",
            "revisar",
            ID,
            "--devedor-nome",
            "Cliente Exemplo Ltda",
            "--data-inicial",
            "2099-11-10",
            "--sim",
        ])
        .assert()
        .success();
    let stdout = stdout_of(&assert);
    assert!(stdout.starts_with("Recorrência alterada.\n\n"), "{stdout}");
    let stderr = stderr_of(&assert);
    for linha in [
        "Cliente Exemplo → Cliente Exemplo Ltda",
        "10/10/2099 → 10/11/2099",
    ] {
        assert!(stderr.contains(linha), "{linha}\n{stderr}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn o_que_nao_muda_mais_e_recusado_depois_da_consulta() {
    let env = env().await;
    env.mount_token("rec.read rec.write", None).await;
    Mock::given(method("GET"))
        .and(path(format!("/pix/v2/rec/{ID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(rec("APROVADA")))
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
        .args([
            "pix-automatico",
            "rec",
            "revisar",
            ID,
            "--data-inicial",
            "2099-11-10",
            "--sim",
        ])
        .assert()
        .code(2);
    assert!(
        stderr_of(&assert).contains("a recorrência já foi aprovada"),
        "{}",
        stderr_of(&assert)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn cancela_depois_de_mostrar_e_nao_cancela_o_que_ja_acabou() {
    let env = env().await;
    env.mount_token("rec.read rec.write", None).await;
    Mock::given(method("GET"))
        .and(path(format!("/pix/v2/rec/{ID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(rec("APROVADA")))
        .up_to_n_times(1)
        .expect(1)
        .mount(&env.server)
        .await;
    let mut cancelada = rec("CANCELADA");
    cancelada["encerramento"] = json!({"cancelamento": {"solicitante": "USUARIO_RECEBEDOR", "codigo": "SLCR", "descricao": "Cancelamento solicitado pelo usuário recebedor"}});
    Mock::given(method("PATCH"))
        .and(path(format!("/pix/v2/rec/{ID}")))
        .and(body_json(json!({"status": "CANCELADA"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(cancelada.clone()))
        .expect(1)
        .mount(&env.server)
        .await;
    let assert = env
        .cmd()
        .args(["pix-automatico", "rec", "cancelar", ID, "--sim"])
        .assert()
        .success();
    let stdout = stdout_of(&assert);
    assert!(
        stdout.starts_with("Recorrência cancelada: ela não aceita mais cobranças."),
        "{stdout}"
    );
    assert!(
        stdout.contains(
            "cancelada pelo recebedor: SLCR, Cancelamento solicitado pelo usuário recebedor"
        ),
        "{stdout}"
    );
    assert!(
        stderr_of(&assert).contains(&format!("Recorrência {ID} a cancelar")),
        "{}",
        stderr_of(&assert)
    );

    // Now cancelled: nothing more to cancel.
    Mock::given(method("GET"))
        .and(path(format!("/pix/v2/rec/{ID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(cancelada))
        .expect(1)
        .mount(&env.server)
        .await;
    let assert = env
        .cmd()
        .args(["pix-automatico", "rec", "cancelar", ID, "--sim"])
        .assert()
        .code(2);
    assert!(
        stderr_of(&assert).contains("a recorrência já está cancelada: não há o que cancelar"),
        "{}",
        stderr_of(&assert)
    );
}
