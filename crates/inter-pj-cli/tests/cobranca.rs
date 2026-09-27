//! `inter-pj cobranca ...` end to end, against a mock API. The boleto is
//! synthetic (valid check digits) and the Pix is the example of the Banco
//! Central's manual.

mod common;

use std::fs;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use common::{TestEnv, stderr_of, stdout_of};
use serde_json::{Value, json};
use wiremock::matchers::{any, method, path, query_param, query_param_is_missing};
use wiremock::{Mock, ResponseTemplate};

const CODIGO: &str = "0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d";
const LINHA: &str = "07790001161234567800212345678903116050000015000";
const COPIA_E_COLA: &str = "00020126580014br.gov.bcb.pix0136123e4567-e12b-12d1-a456-4266554400005204000053039865802BR5913Fulano de Tal6008BRASILIA62070503***63041D3D";

fn cobranca(pix: bool) -> Value {
    let mut corpo = json!({
        "cobranca": {
            "codigoSolicitacao": CODIGO,
            "seuNumero": "NF-123",
            "dataEmissao": "2026-09-23",
            "dataVencimento": "2026-10-20",
            "valorNominal": 150,
            "tipoCobranca": "SIMPLES",
            "situacao": "A_RECEBER",
            "pagador": {"nome": "Cliente Exemplo Ltda", "cpfCnpj": "12345678000195"}
        },
        "boleto": {"nossoNumero": "12345678", "linhaDigitavel": LINHA}
    });
    if pix {
        corpo["pix"] = json!({"txid": "COBRANCAEXEMPLO00000000001", "pixCopiaECola": COPIA_E_COLA});
    }
    corpo
}

async fn env() -> TestEnv {
    let env = TestEnv::new().await;
    env.write_config("");
    env
}

async fn mount_cobranca(env: &TestEnv, corpo: Value, chamadas: u64) {
    env.mount_token("boleto-cobranca.read", None).await;
    Mock::given(method("GET"))
        .and(path(format!("/cobranca/v3/cobrancas/{CODIGO}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(corpo))
        .expect(chamadas)
        .mount(&env.server)
        .await;
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
async fn consulta_uma_cobranca() {
    let env = env().await;
    mount_cobranca(&env, cobranca(true), 2).await;

    let texto = stdout_of(
        &env.cmd()
            .args(["cobranca", "consultar", CODIGO])
            .assert()
            .success(),
    );
    for linha in [
        "Cobrança NF-123\n  Situação    a receber\n  Valor       R$ 150,00\n  Vencimento  20/10/2026",
        "Pagador     Cliente Exemplo Ltda (12.345.678/0001-95)",
        "Linha digitável  07790.00116 12345.678002 12345.678903 1 16050000015000",
        &format!("Copia e cola  {COPIA_E_COLA}"),
    ] {
        assert!(texto.contains(linha), "{linha}\n{texto}");
    }

    let json: Value = serde_json::from_str(&stdout_of(
        &env.cmd()
            .args(["cobranca", "consultar", CODIGO, "--json"])
            .assert()
            .success(),
    ))
    .unwrap();
    assert_eq!(json, cobranca(true));
}

/// The modules of the QR Code drawn without colors (light modules drawn),
/// read back as a phone would.
fn ler_qr_code(texto: &str) -> String {
    const PX: usize = 4;
    let linhas: Vec<&str> = texto
        .lines()
        .filter(|linha| !linha.is_empty() && linha.chars().all(|c| "█▀▄ ".contains(c)))
        .collect();
    assert!(!linhas.is_empty(), "sem QR Code:\n{texto}");
    let lado = linhas[0].chars().count();
    let mut escuros = vec![vec![false; lado]; linhas.len() * 2];
    for (i, linha) in linhas.iter().enumerate() {
        for (x, c) in linha.chars().enumerate() {
            let (cima, baixo) = match c {
                '█' => (true, true),
                '▀' => (true, false),
                '▄' => (false, true),
                _ => (false, false),
            };
            escuros[2 * i][x] = !cima;
            escuros[2 * i + 1][x] = !baixo;
        }
    }
    let mut imagem = rqrr::PreparedImage::prepare_from_greyscale(lado * PX, lado * PX, |x, y| {
        if escuros[y / PX][x / PX] { 0 } else { 255 }
    });
    let grades = imagem.detect_grids();
    assert_eq!(grades.len(), 1, "QR Code não encontrado:\n{texto}");
    grades[0].decode().unwrap().1
}

#[tokio::test(flavor = "multi_thread")]
async fn desenha_o_qr_code_do_pix() {
    let env = env().await;
    mount_cobranca(&env, cobranca(true), 1).await;
    let texto = stdout_of(
        &env.cmd()
            .args(["cobranca", "consultar", CODIGO, "--qrcode"])
            .assert()
            .success(),
    );
    // Not a terminal, and NO_COLOR: no escape sequences.
    assert!(!texto.contains('\u{1b}'), "{texto}");
    assert!(texto.starts_with("Cobrança NF-123"), "{texto}");
    assert_eq!(ler_qr_code(&texto), COPIA_E_COLA);
}

#[tokio::test(flavor = "multi_thread")]
async fn qrcode_nao_combina_com_json() {
    let env = env().await;
    nothing_is_sent(&env).await;
    for extra in [&["--json"][..], &["--qrcode-png", "-"][..]] {
        env.cmd()
            .args(["cobranca", "consultar", CODIGO, "--qrcode"])
            .args(extra)
            .assert()
            .code(2);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn grava_o_qr_code_em_png() {
    let env = env().await;
    // The second run finds the file before calling the API.
    mount_cobranca(&env, cobranca(true), 1).await;
    let arquivo = env.path("pix.png");
    let assert = env
        .cmd()
        .args(["cobranca", "consultar", CODIGO, "--qrcode-png"])
        .arg(&arquivo)
        .assert()
        .success();
    assert!(
        stderr_of(&assert).contains("QR Code salvo em"),
        "{}",
        stderr_of(&assert)
    );
    let png = fs::read(&arquivo).unwrap();
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));

    env.cmd()
        .args(["cobranca", "consultar", CODIGO, "--qrcode-png"])
        .arg(&arquivo)
        .assert()
        .code(2);
    assert_eq!(fs::read(&arquivo).unwrap(), png);
}

#[tokio::test(flavor = "multi_thread")]
async fn png_pode_ir_para_a_saida_padrao() {
    let env = env().await;
    mount_cobranca(&env, cobranca(true), 1).await;
    let assert = env
        .cmd()
        .args(["cobranca", "consultar", CODIGO, "--qrcode-png", "-"])
        .assert()
        .success();
    // Only the image: the charge is not printed with it.
    assert!(assert.get_output().stdout.starts_with(b"\x89PNG\r\n\x1a\n"));
}

#[tokio::test(flavor = "multi_thread")]
async fn cobranca_sem_pix_nao_tem_qr_code() {
    let env = env().await;
    mount_cobranca(&env, cobranca(false), 2).await;
    let assert = env
        .cmd()
        .args(["cobranca", "consultar", CODIGO, "--qrcode"])
        .assert()
        .success();
    assert!(
        stderr_of(&assert).contains("aviso: a cobrança não tem Pix copia e cola"),
        "{}",
        stderr_of(&assert)
    );
    let arquivo = env.path("pix.png");
    let assert = env
        .cmd()
        .args(["cobranca", "consultar", CODIGO, "--qrcode-png"])
        .arg(&arquivo)
        .assert()
        .code(2);
    assert!(
        stderr_of(&assert).contains("a cobrança não tem Pix copia e cola"),
        "{}",
        stderr_of(&assert)
    );
    assert!(!arquivo.exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn grava_o_pdf_da_cobranca() {
    let env = env().await;
    env.mount_token("boleto-cobranca.read", None).await;
    let documento = b"%PDF-1.4\n% cobranca sintetica\n%%EOF\n";
    Mock::given(method("GET"))
        .and(path(format!("/cobranca/v3/cobrancas/{CODIGO}/pdf")))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"pdf": BASE64.encode(documento)})),
        )
        .expect(2)
        .mount(&env.server)
        .await;

    // Default name, in the current directory.
    let assert = env
        .cmd()
        .current_dir(env.path(""))
        .args(["cobranca", "pdf", CODIGO])
        .assert()
        .success();
    assert!(
        stdout_of(&assert).starts_with(&format!("Cobrança salva em cobranca-{CODIGO}.pdf")),
        "{}",
        stdout_of(&assert)
    );
    let arquivo = env.path(&format!("cobranca-{CODIGO}.pdf"));
    assert_eq!(fs::read(&arquivo).unwrap(), documento);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&arquivo).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
    // Not overwritten, and the API is not called for that.
    env.cmd()
        .current_dir(env.path(""))
        .args(["cobranca", "pdf", CODIGO])
        .assert()
        .code(2);

    let assert = env
        .cmd()
        .args(["cobranca", "pdf", CODIGO, "--saida", "-"])
        .assert()
        .success();
    assert_eq!(assert.get_output().stdout, documento);
}

#[tokio::test(flavor = "multi_thread")]
async fn codigo_invalido_nao_chama_a_api() {
    let env = env().await;
    nothing_is_sent(&env).await;
    let invalido = "../../banking/v2/saldo";
    env.cmd()
        .args(["cobranca", "consultar", invalido])
        .assert()
        .code(2);
    // Nor writes a file, not even with the default name.
    env.cmd()
        .current_dir(env.path(""))
        .args(["cobranca", "pdf", invalido])
        .assert()
        .code(2);
    assert!(!env.path("banking").exists());
}

fn item(codigo: &str, seu_numero: &str, situacao: &str, valor: &str) -> Value {
    json!({
        "cobranca": {
            "codigoSolicitacao": codigo,
            "seuNumero": seu_numero,
            "situacao": situacao,
            "dataVencimento": "2026-09-10",
            "valorNominal": valor,
            "pagador": {"nome": "Cliente Exemplo Ltda", "cpfCnpj": "12345678000195"}
        }
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn lista_as_cobrancas_do_periodo() {
    let env = env().await;
    env.mount_token("boleto-cobranca.read", None).await;
    Mock::given(method("GET"))
        .and(path("/cobranca/v3/cobrancas"))
        .and(query_param("dataInicial", "2026-09-01"))
        .and(query_param("dataFinal", "2026-09-30"))
        .and(query_param("filtrarDataPor", "PAGAMENTO"))
        .and(query_param("situacao", "RECEBIDO"))
        .and(query_param("cpfCnpjPessoaPagadora", "12345678000195"))
        .and(query_param("paginacao.paginaAtual", "0"))
        .and(query_param("paginacao.itensPorPagina", "1000"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ultimaPagina": true,
            "cobrancas": [
                item(CODIGO, "NF-123", "RECEBIDO", "150.00"),
                item("5a6b7c8d-1e2f-4a3b-8c9d-0e1f2a3b4c5d", "NF-124", "RECEBIDO", "300.00")
            ]
        })))
        .expect(3)
        .mount(&env.server)
        .await;
    let args = [
        "cobranca",
        "listar",
        "--inicio",
        "2026-09-01",
        "--fim",
        "2026-09-30",
        "--filtrar-por",
        "pagamento",
        "--situacao",
        "recebida",
        "--documento",
        "12.345.678/0001-95",
    ];

    let texto = stdout_of(&env.cmd().args(args).assert().success());
    assert!(
        texto.starts_with(
            "Cobranças pagas de 01/09/2026 a 30/09/2026 (recebida, CPF/CNPJ 12345678000195)"
        ),
        "{texto}"
    );
    assert!(texto.ends_with("2 cobranças · R$ 450,00\n"), "{texto}");

    let json: Value = serde_json::from_str(&stdout_of(
        &env.cmd().args(args).arg("--json").assert().success(),
    ))
    .unwrap();
    assert_eq!(json["cobrancas"].as_array().unwrap().len(), 2);

    let csv = stdout_of(
        &env.cmd()
            .args(args)
            .args(["--formato", "csv", "--separador", ";"])
            .assert()
            .success(),
    );
    assert!(
        csv.starts_with("\u{feff}codigoSolicitacao;seuNumero;situacao;"),
        "{csv}"
    );
    assert!(csv.contains(";150,00;"), "{csv}");
}

#[tokio::test(flavor = "multi_thread")]
async fn uma_pagina_so() {
    let env = env().await;
    env.mount_token("boleto-cobranca.read", None).await;
    Mock::given(method("GET"))
        .and(path("/cobranca/v3/cobrancas"))
        .and(query_param("paginacao.paginaAtual", "1"))
        .and(query_param("paginacao.itensPorPagina", "50"))
        .and(query_param("ordenarPor", "DATA_VENCIMENTO"))
        .and(query_param("tipoOrdenacao", "DESC"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "totalPaginas": 3,
            "totalElementos": 120,
            "ultimaPagina": false,
            "cobrancas": [item(CODIGO, "NF-123", "A_RECEBER", "150.00")]
        })))
        .expect(2)
        .mount(&env.server)
        .await;
    let args = [
        "cobranca",
        "listar",
        "--pagina",
        "1",
        "--itens-por-pagina",
        "50",
        "--ordenar-por",
        "vencimento",
        "--decrescente",
    ];
    let texto = stdout_of(&env.cmd().args(args).assert().success());
    assert!(
        texto.ends_with(
            "Página 1 de 2 (a primeira é 0); 120 cobranças no período; a próxima é --pagina 2\n"
        ),
        "{texto}"
    );
    // The page as the API returns it.
    let json: Value = serde_json::from_str(&stdout_of(
        &env.cmd().args(args).arg("--json").assert().success(),
    ))
    .unwrap();
    assert_eq!(json["totalElementos"], 120);
}

#[tokio::test(flavor = "multi_thread")]
async fn sumario_por_situacao() {
    let env = env().await;
    env.mount_token("boleto-cobranca.read", None).await;
    Mock::given(method("GET"))
        .and(path("/cobranca/v3/cobrancas/sumario"))
        .and(query_param("dataInicial", "2026-09-01"))
        .and(query_param_is_missing("paginacao.paginaAtual"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"situacao": "A_RECEBER", "valor": 1000, "quantidade": 30},
            {"situacao": "RECEBIDO", "valor": 4000.5, "quantidade": 65},
            {"situacao": "CANCELADO", "valor": 0, "quantidade": 0}
        ])))
        .expect(2)
        .mount(&env.server)
        .await;
    let args = [
        "cobranca",
        "sumario",
        "--inicio",
        "2026-09-01",
        "--fim",
        "2026-09-30",
    ];
    let texto = stdout_of(&env.cmd().args(args).assert().success());
    assert!(
        texto.contains("Total              95  R$ 5.000,50"),
        "{texto}"
    );
    assert!(!texto.contains("cancelada"), "{texto}");
    let csv = stdout_of(
        &env.cmd()
            .args(args)
            .args(["--formato", "csv"])
            .assert()
            .success(),
    );
    assert_eq!(
        csv,
        "situacao,quantidade,valor\r\nA_RECEBER,30,1000\r\nRECEBIDO,65,4000.5\r\nCANCELADO,0,0\r\n"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn filtros_invalidos_nao_chamam_a_api() {
    let env = env().await;
    nothing_is_sent(&env).await;
    for args in [
        &["cobranca", "listar", "--documento", "123"][..],
        &["cobranca", "listar", "--situacao", "paga"][..],
        &["cobranca", "listar", "--itens-por-pagina", "50"][..],
        &[
            "cobranca",
            "listar",
            "--pagina",
            "0",
            "--itens-por-pagina",
            "1001",
        ][..],
        &[
            "cobranca",
            "sumario",
            "--inicio",
            "2026-09-30",
            "--fim",
            "2026-09-01",
        ][..],
    ] {
        env.cmd().args(args).assert().code(2);
    }
}
