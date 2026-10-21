//! `inter-pj pix cob ...` end to end, against a mock API. All data is
//! synthetic; the "copia e cola" is the example of the Banco Central's
//! manual.

mod common;

use std::fs;
use std::path::PathBuf;

use common::{COPIA_E_COLA, TestEnv, ler_qr_code, stderr_of, stdout_of};
use serde_json::{Value, json};
use wiremock::matchers::{any, body_json, method, path, path_regex, query_param};
use wiremock::{Mock, Request, Respond, ResponseTemplate};

const TXID: &str = "7978c0c97ea847e78e8849634473c1f1";
const CHAVE: &str = "pix@empresa.example";

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

fn cob(status: &str) -> Value {
    json!({
        "calendario": {"criacao": "2026-09-23T20:15:00.358Z", "expiracao": 3600},
        "txid": TXID,
        "revisao": 0,
        "loc": {"id": 789, "location": "pix.example.com/qr/v2/9d36b84fc70b478fb95c12729b90ca25", "tipoCob": "cob"},
        "location": "pix.example.com/qr/v2/9d36b84fc70b478fb95c12729b90ca25",
        "status": status,
        "devedor": {"cpf": "12345678909", "nome": "Fulano de Tal"},
        "valor": {"original": "37.00"},
        "chave": CHAVE,
        "solicitacaoPagador": "Pedido 123",
        "pixCopiaECola": COPIA_E_COLA
    })
}

fn criar() -> Vec<&'static str> {
    vec![
        "pix",
        "cob",
        "criar",
        "--chave",
        CHAVE,
        "--valor",
        "37,00",
        "--expiracao",
        "1h",
        "--devedor-documento",
        "123.456.789-09",
        "--devedor-nome",
        "Fulano de Tal",
        "--solicitacao",
        "Pedido 123",
    ]
}

fn corpo() -> Value {
    json!({
        "calendario": {"expiracao": 3600},
        "devedor": {"cpf": "12345678909", "nome": "Fulano de Tal"},
        "valor": {"original": "37.00"},
        "chave": CHAVE,
        "solicitacaoPagador": "Pedido 123"
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn cria_com_o_txid_e_desenha_o_qr_code() {
    let env = env().await;
    env.mount_token("cob.write", Some(1)).await;
    Mock::given(method("PUT"))
        .and(path(format!("/pix/v2/cob/{TXID}")))
        .and(body_json(corpo()))
        .respond_with(ResponseTemplate::new(201).set_body_json(cob("ATIVA")))
        .expect(1)
        .mount(&env.server)
        .await;

    let assert = env
        .cmd()
        .args(criar())
        .args(["--txid", TXID, "--sim", "--qrcode"])
        .assert()
        .success();
    let stdout = stdout_of(&assert);
    assert!(
        stdout.starts_with(&format!("Cobrança Pix criada.\n\nCobrança Pix {TXID}\n")),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("Acompanhe com: inter-pj pix cob consultar {TXID}")),
        "{stdout}"
    );
    assert_eq!(ler_qr_code(&stdout), COPIA_E_COLA);
    let stderr = stderr_of(&assert);
    for linha in [
        "Cobrança Pix a criar",
        "R$ 37,00 (trinta e sete reais)",
        "pix@empresa.example (e-mail)",
        "1 hora após a criação",
        "Fulano de Tal (123.456.789-09)",
    ] {
        assert!(stderr.contains(linha), "{linha}\n{stderr}");
    }
}

/// Creates the charge and, before answering, a file where the PNG would go:
/// the write of the image fails after the creation.
struct CriaEOcupaOArquivo(PathBuf);

impl Respond for CriaEOcupaOArquivo {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        fs::write(&self.0, "outro").unwrap();
        ResponseTemplate::new(201).set_body_json(cob("ATIVA"))
    }
}

/// Once the charge exists, a QR Code that cannot be made or saved does not
/// turn the creation into an error: a script would read exit 2 as "nothing
/// was created" and create another charge. The charge is shown, and a
/// warning says how to save the image later.
#[tokio::test(flavor = "multi_thread")]
async fn falha_do_qr_code_depois_da_criacao_e_so_um_aviso() {
    let env = env().await;
    env.mount_token("cob.write", None).await;
    let cob_path = format!("/pix/v2/cob/{TXID}");
    let mut sem_copia_e_cola = cob("ATIVA");
    sem_copia_e_cola["pixCopiaECola"] = Value::Null;
    Mock::given(method("PUT"))
        .and(path(cob_path.clone()))
        .respond_with(ResponseTemplate::new(201).set_body_json(sem_copia_e_cola))
        .up_to_n_times(1)
        .expect(1)
        .mount(&env.server)
        .await;
    let png = env.path("qr.png");
    let assert = env
        .cmd()
        .args(criar())
        .args(["--txid", TXID, "--sim", "--qrcode-png"])
        .arg(&png)
        .assert()
        .success();
    let stdout = stdout_of(&assert);
    assert!(
        stdout.starts_with(&format!("Cobrança Pix criada.\n\nCobrança Pix {TXID}\n")),
        "{stdout}"
    );
    let stderr = stderr_of(&assert);
    for aviso in [
        "aviso: a cobrança foi criada, mas o QR Code não foi gravado: a API não informou o copia e cola desta cobrança".to_owned(),
        format!(
            "aviso: para gravá-lo, use: inter-pj pix cob consultar {TXID} --qrcode-png {}",
            png.display()
        ),
    ] {
        assert!(stderr.contains(&aviso), "{aviso}\n{stderr}");
    }
    assert!(!png.exists());

    // An invalid "copia e cola" with the image on the standard output: the
    // charge, with its txid, goes to stderr.
    let mut invalida = cob("ATIVA");
    invalida["pixCopiaECola"] = json!(COPIA_E_COLA.replace("63041D3D", "63040000"));
    Mock::given(method("PUT"))
        .and(path(cob_path.clone()))
        .respond_with(ResponseTemplate::new(201).set_body_json(invalida))
        .up_to_n_times(1)
        .expect(1)
        .mount(&env.server)
        .await;
    let assert = env
        .cmd()
        .args(criar())
        .args(["--txid", TXID, "--sim", "--qrcode-png", "-"])
        .assert()
        .success();
    assert_eq!(stdout_of(&assert), "");
    let stderr = stderr_of(&assert);
    for linha in [
        format!("Cobrança Pix criada.\n\nCobrança Pix {TXID}\n"),
        "aviso: a cobrança foi criada, mas o QR Code não foi gravado: Pix copia e cola inválido"
            .to_owned(),
        format!("aviso: para gravá-lo, use: inter-pj pix cob consultar {TXID} --qrcode-png -"),
    ] {
        assert!(stderr.contains(&linha), "{linha}\n{stderr}");
    }

    // The file appears between the check and the write.
    Mock::given(method("PUT"))
        .and(path(cob_path))
        .respond_with(CriaEOcupaOArquivo(png.clone()))
        .expect(1)
        .mount(&env.server)
        .await;
    let assert = env
        .cmd()
        .args(criar())
        .args(["--txid", TXID, "--sim", "--qrcode-png"])
        .arg(&png)
        .assert()
        .success();
    assert!(
        stdout_of(&assert).starts_with("Cobrança Pix criada."),
        "{}",
        stdout_of(&assert)
    );
    let stderr = stderr_of(&assert);
    assert!(
        stderr.contains(&format!(
            "aviso: a cobrança foi criada, mas o QR Code não foi gravado: o arquivo {} já existe",
            png.display()
        )),
        "{stderr}"
    );
    assert_eq!(fs::read_to_string(&png).unwrap(), "outro");
}

#[tokio::test(flavor = "multi_thread")]
async fn sem_txid_gera_um_e_o_mostra() {
    let env = env().await;
    env.mount_token("cob.write", Some(1)).await;
    Mock::given(method("PUT"))
        .and(path_regex(r"^/pix/v2/cob/[0-9a-f]{32}$"))
        .and(body_json(corpo()))
        .respond_with(ResponseTemplate::new(201).set_body_json(cob("ATIVA")))
        .expect(1)
        .mount(&env.server)
        .await;
    let assert = env
        .cmd()
        .args(criar())
        .args(["--sim", "--json"])
        .assert()
        .success();
    let json: Value = serde_json::from_str(&stdout_of(&assert)).unwrap();
    assert_eq!(json, cob("ATIVA"));
    let stderr = stderr_of(&assert);
    let linha = stderr
        .lines()
        .find(|linha| linha.trim_start().starts_with("txid"))
        .unwrap();
    let txid = linha.split_whitespace().last().unwrap();
    assert_eq!(txid.len(), 32, "{stderr}");
}

#[tokio::test(flavor = "multi_thread")]
async fn simulacao_mostra_o_txid_na_url() {
    let env = env().await;
    nothing_is_sent(&env).await;
    let assert = env
        .cmd()
        .args(criar())
        .args(["--txid", TXID, "--simular", "--json"])
        .assert()
        .success();
    let json: Value = serde_json::from_str(&stdout_of(&assert)).unwrap();
    assert_eq!(json["metodo"], "PUT");
    assert_eq!(
        json["url"],
        format!("{}/pix/v2/cob/{TXID}", env.server.uri())
    );
    assert_eq!(json["corpo"], corpo());
}

#[tokio::test(flavor = "multi_thread")]
async fn criacao_invalida_ou_sem_confirmacao_nao_chama_a_api() {
    let env = env().await;
    nothing_is_sent(&env).await;
    let assert = env.cmd().args(criar()).write_stdin("s\n").assert().code(2);
    assert!(
        stderr_of(&assert).contains("use --sim"),
        "{}",
        stderr_of(&assert)
    );
    for extra in [
        &["--valor", "0", "--sim"][..],
        &["--solicitacao", "", "--sim"][..],
        &["--txid", "curto", "--sim"][..],
        &["--chave", "pix", "--sim"][..],
        &["--simular", "--qrcode"][..],
    ] {
        env.cmd().args(criar()).args(extra).assert().code(2);
    }
}

/// With an unknown outcome, the error says how to check and how to repeat
/// without creating two charges, and exits with its own code: a script that
/// repeats on 6 would create another charge with another txid.
#[tokio::test(flavor = "multi_thread")]
async fn resultado_incerto_orienta_a_repetir_com_o_mesmo_txid() {
    let env = env().await;
    env.mount_token("cob.write", Some(1)).await;
    Mock::given(method("PUT"))
        .and(path(format!("/pix/v2/cob/{TXID}")))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&env.server)
        .await;
    let assert = env
        .cmd()
        .args(criar())
        .args(["--txid", TXID, "--sim"])
        .assert()
        .code(9);
    let stderr = stderr_of(&assert);
    for dica in [
        "dica: a cobrança pode ter sido criada; com o mesmo txid, a API não cria outra".to_owned(),
        format!("dica: confira com: inter-pj pix cob consultar {TXID}"),
        format!("dica: ou repita o comando com --txid {TXID}"),
    ] {
        assert!(stderr.contains(&dica), "{dica}\n{stderr}");
    }

    // A `429` was not processed: it keeps the code of "try later".
    let limite = self::env().await;
    limite.mount_token("cob.write", Some(1)).await;
    Mock::given(method("PUT"))
        .and(path(format!("/pix/v2/cob/{TXID}")))
        .respond_with(ResponseTemplate::new(429))
        .expect(1)
        .mount(&limite.server)
        .await;
    let assert = limite
        .cmd()
        .args(criar())
        .args(["--txid", TXID, "--sim", "--sem-retentativa"])
        .assert()
        .code(6);
    let stderr = stderr_of(&assert);
    assert!(!stderr.contains("pode ter sido criada"), "{stderr}");
}

/// The commands suggested after a creation run on its account: with the
/// profile, the file, the environment or the account of the command line,
/// a check on the default profile would find nothing and invite a second
/// charge. One chosen in the environment stays in the shell by itself.
#[tokio::test(flavor = "multi_thread")]
async fn os_comandos_sugeridos_sao_da_mesma_conta() {
    let env = TestEnv::new().await;
    env.write_config(&format!(
        "\n[perfis.filial]\nambiente = \"sandbox\"\nclient_id = \"id-da-filial\"\ncertificado = '{}'\nchave_privada = '{}'",
        env.path("certificado.crt").display(),
        env.path("chave.key").display()
    ));
    env.mount_token("cob.write", None).await;
    Mock::given(method("PUT"))
        .and(path(format!("/pix/v2/cob/{TXID}")))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(5)
        .expect(5)
        .mount(&env.server)
        .await;
    let config = env.config_path().display().to_string();
    let casos = [
        (vec!["-p", "filial"], None, "inter-pj -p filial".to_owned()),
        (vec![], Some("filial"), "inter-pj".to_owned()),
        (
            vec!["--config", config.as_str(), "--perfil", "filial"],
            None,
            format!("inter-pj --config {config} -p filial"),
        ),
        (
            vec!["--ambiente", "sandbox"],
            None,
            "inter-pj --ambiente sandbox".to_owned(),
        ),
        (
            vec!["--conta-corrente", "12345678"],
            None,
            "inter-pj --conta-corrente 12345678".to_owned(),
        ),
    ];
    for (opcoes, no_ambiente, chamada) in &casos {
        let mut cmd = env.cmd();
        if let Some(perfil) = no_ambiente {
            cmd.env("INTER_PERFIL", perfil);
        }
        let assert = cmd
            .args(opcoes)
            .args(criar())
            .args(["--txid", TXID, "--sim", "--sem-retentativa"])
            .assert()
            .code(9);
        let stderr = stderr_of(&assert);
        assert!(
            stderr.contains(&format!(
                "dica: confira com: {chamada} pix cob consultar {TXID}"
            )),
            "{opcoes:?}: {stderr}"
        );
    }

    // After the 503s, the charge is created: how to follow it keeps the
    // account too.
    Mock::given(method("PUT"))
        .and(path(format!("/pix/v2/cob/{TXID}")))
        .respond_with(ResponseTemplate::new(201).set_body_json(cob("ATIVA")))
        .expect(1)
        .mount(&env.server)
        .await;
    let assert = env
        .cmd()
        .args(["-p", "filial"])
        .args(criar())
        .args(["--txid", TXID, "--sim"])
        .assert()
        .success();
    let stdout = stdout_of(&assert);
    assert!(
        stdout.contains(&format!(
            "Acompanhe com: inter-pj -p filial pix cob consultar {TXID}"
        )),
        "{stdout}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn consulta_e_revisa_a_cobranca() {
    let env = env().await;
    env.mount_token("cob.write cob.read", None).await;
    let cob_path = format!("/pix/v2/cob/{TXID}");
    Mock::given(method("GET"))
        .and(path(cob_path.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(cob("ATIVA")))
        .expect(3)
        .mount(&env.server)
        .await;
    let mut revisada = cob("ATIVA");
    revisada["revisao"] = json!(1);
    revisada["valor"] = json!({"original": "40.00"});
    Mock::given(method("PATCH"))
        .and(path(cob_path.clone()))
        .and(body_json(
            json!({"valor": {"original": "40.00", "modalidadeAlteracao": 1}}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(revisada))
        .expect(1)
        .mount(&env.server)
        .await;
    let mut removida = cob("REMOVIDA_PELO_USUARIO_RECEBEDOR");
    removida["revisao"] = json!(2);
    Mock::given(method("PATCH"))
        .and(path(cob_path))
        .and(body_json(
            json!({"status": "REMOVIDA_PELO_USUARIO_RECEBEDOR"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(removida))
        .expect(1)
        .mount(&env.server)
        .await;

    let texto = stdout_of(
        &env.cmd()
            .args(["pix", "cob", "consultar", TXID])
            .assert()
            .success(),
    );
    assert!(
        texto.starts_with(&format!("Cobrança Pix {TXID}\n  Status       ativa\n")),
        "{texto}"
    );
    assert!(
        texto.contains(&format!("Copia e cola  {COPIA_E_COLA}")),
        "{texto}"
    );

    let assert = env
        .cmd()
        .args([
            "pix",
            "cob",
            "revisar",
            TXID,
            "--valor",
            "40",
            "--valor-alteravel",
            "sim",
            "--sim",
        ])
        .assert()
        .success();
    assert!(
        stdout_of(&assert).starts_with("Cobrança Pix alterada (revisão 1)."),
        "{}",
        stdout_of(&assert)
    );
    let stderr = stderr_of(&assert);
    assert!(stderr.contains("R$ 37,00 → R$ 40,00"), "{stderr}");
    assert!(stderr.contains("→ sim"), "{stderr}");

    let assert = env
        .cmd()
        .args(["pix", "cob", "revisar", TXID, "--remover", "--sim"])
        .assert()
        .success();
    assert!(
        stdout_of(&assert).starts_with("Cobrança Pix removida: ela não pode mais ser paga."),
        "{}",
        stdout_of(&assert)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn cobranca_paga_nao_e_revisada_nem_tem_qr_code() {
    let env = env().await;
    env.mount_token("cob.write cob.read", None).await;
    Mock::given(method("GET"))
        .and(path(format!("/pix/v2/cob/{TXID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(cob("CONCLUIDA")))
        .expect(2)
        .mount(&env.server)
        .await;
    Mock::given(method("PATCH"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&env.server)
        .await;
    let assert = env
        .cmd()
        .args(["pix", "cob", "revisar", TXID, "--remover", "--sim"])
        .assert()
        .code(2);
    assert!(
        stderr_of(&assert).contains("a cobrança já foi paga: não pode ser alterada"),
        "{}",
        stderr_of(&assert)
    );
    let assert = env
        .cmd()
        .args(["pix", "cob", "consultar", TXID, "--qrcode"])
        .assert()
        .success();
    assert!(
        stderr_of(&assert).contains("o QR Code não serve mais para pagar"),
        "{}",
        stderr_of(&assert)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn lista_as_cobrancas_do_periodo() {
    let env = env().await;
    env.mount_token("cob.read", None).await;
    Mock::given(method("GET"))
        .and(path("/pix/v2/cob"))
        .and(query_param("inicio", "2026-09-01T00:00:00-03:00"))
        .and(query_param("fim", "2026-09-30T23:59:59-03:00"))
        .and(query_param("status", "CONCLUIDA"))
        .and(query_param("cpf", "12345678909"))
        .and(query_param("locationPresente", "true"))
        .and(query_param("paginacao.paginaAtual", "0"))
        .and(query_param("paginacao.itensPorPagina", "1000"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "parametros": {
                "inicio": "2026-09-01T03:00:00Z",
                "fim": "2026-10-01T02:59:59Z",
                "paginacao": {"paginaAtual": 0, "itensPorPagina": 1000, "quantidadeDePaginas": 1, "quantidadeTotalDeItens": 2}
            },
            "cobs": [cob("CONCLUIDA"), cob("CONCLUIDA")]
        })))
        .expect(3)
        .mount(&env.server)
        .await;
    let args = [
        "pix",
        "cob",
        "listar",
        "--inicio",
        "2026-09-01T00:00:00-03:00",
        "--fim",
        "2026-09-30T23:59:59-03:00",
        "--status",
        "concluida",
        "--documento",
        "123.456.789-09",
        "--com-location",
    ];
    let texto = stdout_of(&env.cmd().args(args).assert().success());
    assert!(
        texto.starts_with("Cobranças Pix imediatas criadas de 01/09/2026 00:00 a 30/09/2026 23:59 (concluída (paga), devedor 123.456.789-09, com location)"),
        "{texto}"
    );
    assert!(
        texto.ends_with("2 cobranças · R$ 74,00 · pagas R$ 74,00\n"),
        "{texto}"
    );
    let json: Value = serde_json::from_str(&stdout_of(
        &env.cmd().args(args).arg("--json").assert().success(),
    ))
    .unwrap();
    assert_eq!(json["cobs"].as_array().unwrap().len(), 2);
    let csv = stdout_of(
        &env.cmd()
            .args(args)
            .args(["--formato", "csv", "--separador", ";"])
            .assert()
            .success(),
    );
    assert!(csv.starts_with("\u{feff}txid;status;revisao;"), "{csv}");
    assert!(csv.contains(";37,00;"), "{csv}");
}

#[tokio::test(flavor = "multi_thread")]
async fn filtros_invalidos_nao_chamam_a_api() {
    let env = env().await;
    nothing_is_sent(&env).await;
    for args in [
        &[
            "pix",
            "cob",
            "listar",
            "--inicio",
            "2026-09-30",
            "--fim",
            "2026-09-01",
        ][..],
        &["pix", "cob", "listar", "--inicio", "01/09/2026"][..],
        &["pix", "cob", "listar", "--status", "paga"][..],
        &["pix", "cob", "listar", "--com-location", "--sem-location"][..],
        &["pix", "cob", "listar", "--itens-por-pagina", "10"][..],
        &[
            "pix",
            "cob",
            "listar",
            "--pagina",
            "0",
            "--itens-por-pagina",
            "1001",
        ][..],
        &["pix", "cob", "consultar", "../../banking/v2/saldo"][..],
        &["pix", "cob", "revisar", TXID, "--sim"][..],
    ] {
        env.cmd().args(args).assert().code(2);
    }
}
