//! `inter-pj pix-automatico cobr ...` end to end, against a mock API.
//! All data is synthetic.

mod common;

use common::{TestEnv, stderr_of, stdout_of};
use serde_json::{Value, json};
use wiremock::matchers::{any, body_json, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

const ID_REC: &str = "RR1234567820260924abcdefghijk";
const TXID: &str = "7978c0c97ea847e78e8849634473c1f1";
const E2E: &str = "E12345678209910101300abcdef12345";

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
        "cobr",
        "criar",
        "--rec",
        ID_REC,
        "--valor",
        "149,90",
        "--vencimento",
        "2099-10-10",
        "--txid",
        TXID,
        "--conta",
        "1234567",
        "--agencia",
        "0001",
        "--info",
        "Mensalidade de outubro",
        "--devedor-email",
        "cliente@empresa.example",
    ]
}

/// `criar()` with another value for `opcao`.
fn criar_com(opcao: &str, valor: &'static str) -> Vec<&'static str> {
    let mut args = criar();
    let i = args.iter().position(|arg| *arg == opcao).unwrap();
    args[i + 1] = valor;
    args
}

/// `criar()` without `opcao` and its value.
fn criar_sem(opcao: &str) -> Vec<&'static str> {
    let mut args = criar();
    let i = args.iter().position(|arg| *arg == opcao).unwrap();
    args.drain(i..i + 2);
    args
}

fn corpo() -> Value {
    json!({
        "idRec": ID_REC,
        "infoAdicional": "Mensalidade de outubro",
        "calendario": {"dataDeVencimento": "2099-10-10"},
        "valor": {"original": "149.90"},
        "ajusteDiaUtil": true,
        "devedor": {"email": "cliente@empresa.example"},
        "recebedor": {"conta": "1234567", "tipoConta": "CORRENTE", "agencia": "0001"}
    })
}

fn rec(status: &str) -> Value {
    json!({
        "idRec": ID_REC,
        "vinculo": {"objeto": "Mensalidade", "devedor": {"cpf": "12345678909", "nome": "Cliente Exemplo"}, "contrato": "contrato-001"},
        "calendario": {"dataInicial": "2099-10-10", "periodicidade": "MENSAL"},
        "valor": {"valorRec": "149.90"},
        "status": status,
        "politicaRetentativa": "PERMITE_3R_7D"
    })
}

fn cobr(status: &str) -> Value {
    json!({
        "idRec": ID_REC,
        "txid": TXID,
        "infoAdicional": "Mensalidade de outubro",
        "calendario": {"criacao": "2099-09-24T13:00:00.000Z", "dataDeVencimento": "2099-10-10"},
        "valor": {"original": "149.90"},
        "status": status,
        "politicaRetentativa": "PERMITE_3R_7D",
        "ajusteDiaUtil": true,
        "devedor": {"email": "cliente@empresa.example"},
        "recebedor": {"cnpj": "12345678000195", "nome": "Empresa Exemplo Ltda", "conta": "1234567", "tipoConta": "CORRENTE", "agencia": "0001"},
        "tentativas": [{"dataLiquidacao": "2099-10-10", "tipo": "AGND", "status": "AGENDADA", "endToEndId": E2E}],
        "atualizacao": [{"status": "CRIADA", "data": "2099-09-24T13:00:00.000Z"}]
    })
}

/// `cobr("ATIVA")` whose debit on the due date failed.
fn nao_paga() -> Value {
    let mut cobr = cobr("ATIVA");
    cobr["tentativas"][0]["status"] = json!("REJEITADA");
    cobr["tentativas"][0]["rejeicao"] =
        json!({"codigo": "AM04", "descricao": "Saldo insuficiente"});
    cobr
}

#[tokio::test(flavor = "multi_thread")]
async fn cria_a_cobranca_de_uma_recorrencia_aprovada() {
    let env = env().await;
    env.mount_token("rec.read cobr.write", Some(1)).await;
    Mock::given(method("GET"))
        .and(path(format!("/pix/v2/rec/{ID_REC}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(rec("APROVADA")))
        .expect(1)
        .mount(&env.server)
        .await;
    Mock::given(method("PUT"))
        .and(path(format!("/pix/v2/cobr/{TXID}")))
        .and(body_json(corpo()))
        .respond_with(ResponseTemplate::new(201).set_body_json(cobr("CRIADA")))
        .expect(1)
        .mount(&env.server)
        .await;
    let assert = env.cmd().args(criar()).arg("--sim").assert().success();
    let stdout = stdout_of(&assert);
    assert!(
        stdout.starts_with(&format!(
            "Cobrança recorrente criada: o banco do pagador agenda o débito para o vencimento.\n\nCobrança recorrente {TXID}\n  Status"
        )),
        "{stdout}"
    );
    assert!(
        stdout.ends_with(&format!(
            "\n\nAcompanhe com: inter-pj pix-automatico cobr consultar {TXID}\n"
        )),
        "{stdout}"
    );
    let stderr = stderr_of(&assert);
    for linha in [
        "Cobrança recorrente a criar",
        "Cliente Exemplo (123.456.789-09)",
        "contrato-001",
        "R$ 149,90 (cento e quarenta e nove reais e noventa centavos)",
        "10/10/2099, ou o próximo dia útil",
        "conta corrente 1234567, agência 0001",
        "cliente@empresa.example",
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
    assert_eq!(json["metodo"], "PUT");
    assert_eq!(
        json["url"],
        format!("{}/pix/v2/cobr/{TXID}", env.server.uri())
    );
    assert_eq!(json["corpo"], corpo());

    for (args, erro) in [
        (
            criar_com("--vencimento", "2020-01-01"),
            "o vencimento (01/01/2020) já passou",
        ),
        (criar_com("--conta", "1234-5"), "--conta: até 20 dígitos"),
        (criar_com("--agencia", "12345"), "--agencia: até 4 dígitos"),
        (
            criar_com("--devedor-email", "cliente"),
            "--devedor-email: e-mail inválido",
        ),
        (criar_com("--valor", "0"), "o valor deve ser maior que zero"),
        (criar_sem("--conta"), "informe --conta, a conta que recebe"),
    ] {
        let assert = env.cmd().args(&args).arg("--sim").assert().code(2);
        assert!(
            stderr_of(&assert).contains(erro),
            "{erro}\n{}",
            stderr_of(&assert)
        );
    }
    // Without a terminal to confirm, not even the recurrence is looked up.
    env.cmd().args(criar()).assert().code(2);
}

#[tokio::test(flavor = "multi_thread")]
async fn sem_conta_recebe_na_conta_da_configuracao() {
    let env = TestEnv::new().await;
    env.write_config("conta_corrente = \"7654321\"");
    nothing_is_sent(&env).await;
    let assert = env
        .cmd()
        .args(criar_sem("--conta"))
        .args(["--simular", "--json"])
        .assert()
        .success();
    let json: Value = serde_json::from_str(&stdout_of(&assert)).unwrap();
    assert_eq!(
        json["corpo"]["recebedor"],
        json!({"conta": "7654321", "tipoConta": "CORRENTE", "agencia": "0001"})
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn recusa_uma_recorrencia_que_nao_esta_aprovada() {
    let env = env().await;
    env.mount_token("rec.read cobr.write", None).await;
    Mock::given(method("GET"))
        .and(path(format!("/pix/v2/rec/{ID_REC}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(rec("CRIADA")))
        .expect(1)
        .mount(&env.server)
        .await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&env.server)
        .await;
    let assert = env.cmd().args(criar()).arg("--sim").assert().code(2);
    assert!(
        stderr_of(&assert).contains(
            "a recorrência está criada (aguarda a aprovação do pagador): só uma recorrência aprovada pelo pagador aceita cobranças"
        ),
        "{}",
        stderr_of(&assert)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn resultado_incerto_orienta_a_repetir_com_o_mesmo_txid() {
    let env = env().await;
    env.mount_token("rec.read cobr.write", Some(1)).await;
    Mock::given(method("GET"))
        .and(path(format!("/pix/v2/rec/{ID_REC}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(rec("APROVADA")))
        .expect(1)
        .mount(&env.server)
        .await;
    Mock::given(method("PUT"))
        .and(path(format!("/pix/v2/cobr/{TXID}")))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&env.server)
        .await;
    let assert = env.cmd().args(criar()).arg("--sim").assert().code(6);
    let stderr = stderr_of(&assert);
    for dica in [
        "dica: a cobrança pode ter sido criada; com o mesmo txid, a API não cria outra".to_owned(),
        format!("dica: confira com: inter-pj pix-automatico cobr consultar {TXID}"),
        format!("dica: ou repita o comando com --txid {TXID}"),
    ] {
        assert!(stderr.contains(&dica), "{dica}\n{stderr}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn lista_as_cobrancas_de_uma_recorrencia() {
    let env = env().await;
    env.mount_token("cobr.read", Some(1)).await;
    let mut segunda = cobr("ATIVA");
    segunda["txid"] = json!("0a1b2c3d4e5f60718293a4b5c6d7e8f9");
    segunda["calendario"]["dataDeVencimento"] = json!("2099-11-10");
    Mock::given(method("GET"))
        .and(path("/pix/v2/cobr"))
        .and(query_param("inicio", "2099-09-01T00:00:00-03:00"))
        .and(query_param("fim", "2099-09-30T23:59:59-03:00"))
        .and(query_param("idRec", ID_REC))
        .and(query_param("status", "ATIVA"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "parametros": {"paginacao": {"paginaAtual": 0, "itensPorPagina": 1000, "quantidadeDePaginas": 1, "quantidadeTotalDeItens": 2}},
            "cobsr": [cobr("ATIVA"), segunda]
        })))
        .expect(2)
        .mount(&env.server)
        .await;
    let listar = [
        "pix-automatico",
        "cobr",
        "listar",
        "--inicio",
        "2099-09-01T00:00:00-03:00",
        "--fim",
        "2099-09-30T23:59:59-03:00",
        "--rec",
        ID_REC,
        "--status",
        "ativa",
    ];
    let stdout = stdout_of(&env.cmd().args(listar).assert().success());
    assert!(
        stdout.starts_with(&format!(
            "Cobranças recorrentes criadas de 01/09/2099 00:00 a 30/09/2099 23:59 (ativa, recorrência {ID_REC})\n\nVencimento  Status      Valor  Recorrência"
        )),
        "{stdout}"
    );
    for linha in [
        format!("10/10/2099  ativa   R$ 149,90  {ID_REC}  {TXID}"),
        format!("10/11/2099  ativa   R$ 149,90  {ID_REC}  0a1b2c3d4e5f60718293a4b5c6d7e8f9"),
        "2 cobranças · R$ 299,80".to_owned(),
    ] {
        assert!(stdout.contains(&linha), "{linha}\n{stdout}");
    }
    let csv = stdout_of(
        &env.cmd()
            .args(listar)
            .args(["--formato", "csv"])
            .assert()
            .success(),
    );
    assert!(
        csv.starts_with("txid,idRec,status,calendario.criacao,"),
        "{csv}"
    );
    assert!(
        csv.contains(&format!("\r\n{TXID},{ID_REC},ATIVA,")),
        "{csv}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn consulta_e_cancela_uma_cobranca() {
    let env = env().await;
    env.mount_token("cobr.read cobr.write", Some(1)).await;
    Mock::given(method("GET"))
        .and(path(format!("/pix/v2/cobr/{TXID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(cobr("ATIVA")))
        .expect(2)
        .mount(&env.server)
        .await;
    let mut cancelada = cobr("CANCELADA");
    cancelada["tentativas"][0]["status"] = json!("CANCELADA");
    cancelada["encerramento"] = json!({
        "cancelamento": {"solicitante": "USUARIO_RECEBEDOR", "codigo": "SLCR", "descricao": "Cancelada pelo recebedor"}
    });
    Mock::given(method("PATCH"))
        .and(path(format!("/pix/v2/cobr/{TXID}")))
        .and(body_json(json!({"status": "CANCELADA"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(cancelada))
        .expect(1)
        .mount(&env.server)
        .await;

    let stdout = stdout_of(
        &env.cmd()
            .args(["pix-automatico", "cobr", "consultar", TXID])
            .assert()
            .success(),
    );
    assert!(
        stdout.starts_with(&format!(
            "Cobrança recorrente {TXID}\n  Status        ativa (débito agendado)\n"
        )),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!(
            "Tentativas de liquidação\nLiquidação  Tipo         Status    endToEndId                        Motivo\n10/10/2099  agendamento  agendada  {E2E}\n"
        )),
        "{stdout}"
    );
    let assert = env
        .cmd()
        .args(["pix-automatico", "cobr", "cancelar", TXID, "--sim"])
        .assert()
        .success();
    let stdout = stdout_of(&assert);
    assert!(
        stdout.starts_with("Cobrança recorrente cancelada: o débito não será feito.\n\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains(
            "\n  Encerramento  cancelada pelo recebedor: SLCR, Cancelada pelo recebedor\n"
        ),
        "{stdout}"
    );
    let stderr = stderr_of(&assert);
    assert!(
        stderr.contains(&format!("Cobrança recorrente {TXID} a cancelar")),
        "{stderr}"
    );
    // Far from the settlement, no warning about the time limit.
    assert!(!stderr.contains("aviso"), "{stderr}");
}

#[tokio::test(flavor = "multi_thread")]
async fn nao_cancela_uma_cobranca_paga() {
    let env = env().await;
    env.mount_token("cobr.read cobr.write", None).await;
    Mock::given(method("GET"))
        .and(path(format!("/pix/v2/cobr/{TXID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(cobr("CONCLUIDA")))
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
        .args(["pix-automatico", "cobr", "cancelar", TXID, "--sim"])
        .assert()
        .code(2);
    assert!(
        stderr_of(&assert).contains("a cobrança já está concluída (paga): não há o que cancelar"),
        "{}",
        stderr_of(&assert)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn pede_uma_nova_tentativa_dentro_da_politica() {
    let env = env().await;
    env.mount_token("cobr.read cobr.write", Some(1)).await;
    Mock::given(method("GET"))
        .and(path(format!("/pix/v2/cobr/{TXID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(nao_paga()))
        .expect(1)
        .mount(&env.server)
        .await;
    let mut com_tentativa = nao_paga();
    com_tentativa["tentativas"]
        .as_array_mut()
        .unwrap()
        .push(json!({"dataLiquidacao": "2099-10-12", "tipo": "NTAG", "status": "SOLICITADA"}));
    Mock::given(method("POST"))
        .and(path(format!("/pix/v2/cobr/{TXID}/retentativa/2099-10-12")))
        .respond_with(ResponseTemplate::new(200).set_body_json(com_tentativa))
        .expect(1)
        .mount(&env.server)
        .await;
    let assert = env
        .cmd()
        .args([
            "pix-automatico",
            "cobr",
            "retentativa",
            TXID,
            "--data",
            "2099-10-12",
            "--sim",
        ])
        .assert()
        .success();
    let stdout = stdout_of(&assert);
    assert!(
        stdout.starts_with("Nova tentativa pedida para 12/10/2099.\n\n"),
        "{stdout}"
    );
    for linha in [
        "10/10/2099  agendamento     rejeitada   ",
        "AM04, Saldo insuficiente",
        "12/10/2099  nova tentativa  solicitada",
    ] {
        assert!(stdout.contains(linha), "{linha}\n{stdout}");
    }
    let stderr = stderr_of(&assert);
    for linha in [
        format!("Nova tentativa da cobrança recorrente {TXID}"),
        "Liquidação prevista  10/10/2099".to_owned(),
        "Nova tentativa       12/10/2099".to_owned(),
    ] {
        assert!(stderr.contains(&linha), "{linha}\n{stderr}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn recusa_uma_nova_tentativa_fora_da_politica() {
    let env = env().await;
    let retentativa = |data: &'static str| {
        [
            "pix-automatico",
            "cobr",
            "retentativa",
            TXID,
            "--data",
            data,
            "--sim",
        ]
    };
    // A day in the past is refused before anything is looked up.
    let assert = env.cmd().args(retentativa("2020-01-01")).assert().code(2);
    assert!(
        stderr_of(&assert).contains("--data: 01/01/2020 já passou"),
        "{}",
        stderr_of(&assert)
    );

    env.mount_token("cobr.read cobr.write", None).await;
    let mut sem_retentativas = nao_paga();
    sem_retentativas["politicaRetentativa"] = json!("NAO_PERMITE");
    Mock::given(method("GET"))
        .and(path(format!("/pix/v2/cobr/{TXID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(nao_paga()))
        .up_to_n_times(1)
        .expect(1)
        .mount(&env.server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/pix/v2/cobr/{TXID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(sem_retentativas))
        .expect(1)
        .mount(&env.server)
        .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&env.server)
        .await;
    for (data, erro) in [
        (
            "2099-10-20",
            "--data: a nova tentativa é de 11/10/2099 a 17/10/2099, até 7 dias depois da liquidação prevista (10/10/2099)",
        ),
        (
            "2099-10-12",
            "a recorrência desta cobrança não permite novas tentativas",
        ),
    ] {
        let assert = env.cmd().args(retentativa(data)).assert().code(2);
        assert!(
            stderr_of(&assert).contains(erro),
            "{erro}\n{}",
            stderr_of(&assert)
        );
    }
}
