//! `inter-pj cobranca ...` end to end, against a mock API. The boleto is
//! synthetic (valid check digits) and the Pix is the example of the Banco
//! Central's manual.

mod common;

use std::fs;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use chrono::Days;
use common::{CLIENT_SECRET, TOKEN, TestEnv, hoje, stderr_of, stdout_of};
use serde_json::{Value, json};
use wiremock::matchers::{
    any, body_json, body_partial_json, method, path, query_param, query_param_is_missing,
};
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
        "Linha digitável   07790.00116 12345.678002 12345.678903 1 16050000015000",
        "Código de barras  07791160500000150000001112345678001234567890",
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

/// Without a Pix that makes a QR Code, `--qrcode` and `--qrcode-png` fail
/// alike (5), before showing the charge or writing the image: a script
/// must not read the charge and miss the QR Code.
#[tokio::test(flavor = "multi_thread")]
async fn cobranca_sem_pix_nao_tem_qr_code() {
    let mut em_processamento = cobranca(false);
    em_processamento["cobranca"]["situacao"] = json!("EM_PROCESSAMENTO");
    let mut crc_errado = cobranca(true);
    crc_errado["pix"]["pixCopiaECola"] = json!(COPIA_E_COLA.replace("1D3D", "1D3E"));
    for (corpo, motivo) in [
        (
            cobranca(false),
            "a cobrança não tem Pix copia e cola: a conta não tinha chave Pix",
        ),
        (
            em_processamento,
            "a cobrança não tem Pix copia e cola: a cobrança ainda está sendo emitida",
        ),
        (crc_errado, "Pix copia e cola inválido"),
    ] {
        let env = env().await;
        mount_cobranca(&env, corpo, 4).await;
        let arquivo = env.path("pix.png");
        let arquivo = arquivo.to_str().unwrap();
        for extra in [
            &["--qrcode"][..],
            &["--qrcode-png", arquivo],
            &["--qrcode-png", arquivo, "--json"],
            &["--qrcode-png", "-"],
        ] {
            let assert = env
                .cmd()
                .args(["cobranca", "consultar", CODIGO])
                .args(extra)
                .assert()
                .code(5);
            assert_eq!(stdout_of(&assert), "", "{extra:?}");
            let stderr = stderr_of(&assert);
            assert!(
                stderr.contains(&format!("erro: {motivo}")),
                "{extra:?}\n{stderr}"
            );
            assert!(!stderr.contains("aviso"), "{extra:?}\n{stderr}");
        }
        assert!(!env.path("pix.png").exists());
    }
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

// --- emitir -------------------------------------------------------------------------

/// `dias` after today in Brasília, as the CLI counts.
fn daqui_a(dias: u64) -> String {
    hoje()
        .checked_add_days(Days::new(dias))
        .unwrap()
        .format("%Y-%m-%d")
        .to_string()
}

/// A charge given in options, due in 30 days.
fn opcoes(vencimento: &str) -> Vec<String> {
    [
        "cobranca",
        "emitir",
        "--seu-numero",
        "NF-123",
        "--valor",
        "150,00",
        "--vencimento",
        vencimento,
        "--pagador-documento",
        "12.345.678/0001-95",
        "--pagador-nome",
        "Cliente Exemplo Ltda",
        "--pagador-endereco",
        "Avenida Brasil",
        "--pagador-numero",
        "1200",
        "--pagador-cidade",
        "Belo Horizonte",
        "--pagador-uf",
        "MG",
        "--pagador-cep",
        "30110-000",
        "--pagador-email",
        "financeiro@exemplo.com.br",
        "--multa",
        "2%",
        "--juros",
        "1%",
        "--dias-agenda",
        "30",
        "--mensagem",
        "Referente à NF 123",
    ]
    .map(str::to_owned)
    .to_vec()
}

/// The body the options above become, as the API documents it.
fn corpo(vencimento: &str) -> Value {
    json!({
        "seuNumero": "NF-123",
        "valorNominal": 150,
        "dataVencimento": vencimento,
        "numDiasAgenda": 30,
        "pagador": {
            "cpfCnpj": "12345678000195",
            "tipoPessoa": "JURIDICA",
            "nome": "Cliente Exemplo Ltda",
            "endereco": "Avenida Brasil",
            "numero": "1200",
            "cidade": "Belo Horizonte",
            "uf": "MG",
            "cep": "30110000",
            "email": "financeiro@exemplo.com.br"
        },
        "multa": {"codigo": "PERCENTUAL", "taxa": 2},
        "mora": {"codigo": "TAXAMENSAL", "taxa": 1},
        "mensagem": {"linha1": "Referente à NF 123"}
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn emite_pelas_opcoes_e_mostra_como_acompanhar() {
    let env = env().await;
    let vencimento = daqui_a(30);
    env.mount_token("boleto-cobranca.write", Some(1)).await;
    Mock::given(method("POST"))
        .and(path("/cobranca/v3/cobrancas"))
        .and(body_json(corpo(&vencimento)))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"codigoSolicitacao": CODIGO})),
        )
        .expect(1)
        .mount(&env.server)
        .await;

    let assert = env
        .cmd()
        .args(opcoes(&vencimento))
        .arg("--sim")
        .assert()
        .success();
    assert_eq!(
        stdout_of(&assert),
        format!(
            "Cobrança solicitada: a emissão termina em instantes.\nCódigo  {CODIGO}\n\nAcompanhe com: inter-pj cobranca consultar {CODIGO}\n"
        )
    );
    let stderr = stderr_of(&assert);
    for linha in [
        "Cobrança a emitir",
        "sandbox (dados fictícios)",
        "R$ 150,00 (cento e cinquenta reais)",
        "Cliente Exemplo Ltda (12.345.678/0001-95)",
        "Avenida Brasil, 1200 - Belo Horizonte/MG - CEP 30110-000",
        "1% ao mês",
        "30 dias após o vencimento, se não for paga",
        "boleto e Pix (se a conta tiver chave Pix)",
    ] {
        assert!(stderr.contains(linha), "{linha}\n{stderr}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn emite_o_modelo_e_aguarda_o_boleto_e_o_pix() {
    let env = env().await;
    let modelo = stdout_of(&env.cmd().args(["cobranca", "modelo"]).assert().success());
    let json: Value = serde_json::from_str(&modelo).unwrap();
    assert_eq!(json["dataVencimento"], daqui_a(30).as_str());
    let arquivo = env.path("cobranca.json");
    fs::write(&arquivo, &modelo).unwrap();

    // One token for both: the charge is issued, then queried.
    env.mount_token("boleto-cobranca.write boleto-cobranca.read", None)
        .await;
    Mock::given(method("POST"))
        .and(path("/cobranca/v3/cobrancas"))
        .and(body_partial_json(json!({
            "seuNumero": "NF-123",
            "dataVencimento": daqui_a(30),
            "pagador": {"cpfCnpj": "12345678000195", "tipoPessoa": "JURIDICA", "cep": "30110000"},
            "mensagem": {"linha1": "Referente à NF 123", "linha2": "Obrigado pela preferência"},
            "formasRecebimento": ["BOLETO", "PIX"]
        })))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"codigoSolicitacao": CODIGO})),
        )
        .expect(1)
        .mount(&env.server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/cobranca/v3/cobrancas/{CODIGO}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(cobranca(true)))
        .expect(1)
        .mount(&env.server)
        .await;

    let png = env.path("pix.png");
    let assert = env
        .cmd()
        .args(["cobranca", "emitir", "--arquivo"])
        .arg(&arquivo)
        .args(["--sim", "--aguardar", "--qrcode-png"])
        .arg(&png)
        .assert()
        .success();
    let stdout = stdout_of(&assert);
    assert!(stdout.starts_with("Cobrança NF-123"), "{stdout}");
    assert!(
        stdout.contains(&format!("Copia e cola  {COPIA_E_COLA}")),
        "{stdout}"
    );
    assert!(
        stderr_of(&assert).contains("QR Code salvo em"),
        "{}",
        stderr_of(&assert)
    );
    assert!(fs::read(&png).unwrap().starts_with(b"\x89PNG\r\n\x1a\n"));
}

/// A charge issued without a Pix (the account had no Pix key) has no QR
/// Code: after the issue, that is a warning, and not an error a script
/// would read as "not issued" and issue the charge again.
#[tokio::test(flavor = "multi_thread")]
async fn emitida_sem_pix_avisa_e_termina_bem() {
    let env = env().await;
    let modelo = stdout_of(&env.cmd().args(["cobranca", "modelo"]).assert().success());
    let arquivo = env.path("cobranca.json");
    fs::write(&arquivo, &modelo).unwrap();
    env.mount_token("boleto-cobranca.write boleto-cobranca.read", None)
        .await;
    Mock::given(method("POST"))
        .and(path("/cobranca/v3/cobrancas"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"codigoSolicitacao": CODIGO})),
        )
        .expect(2)
        .mount(&env.server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/cobranca/v3/cobrancas/{CODIGO}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(cobranca(false)))
        .expect(2)
        .mount(&env.server)
        .await;

    let png = env.path("pix.png");
    let assert = env
        .cmd()
        .args(["cobranca", "emitir", "--arquivo"])
        .arg(&arquivo)
        .args(["--sim", "--aguardar", "--qrcode-png"])
        .arg(&png)
        .assert()
        .success();
    assert!(stdout_of(&assert).starts_with("Cobrança NF-123"));
    let stderr = stderr_of(&assert);
    assert!(
        stderr.contains(
            "aviso: a cobrança foi emitida, mas sem o QR Code: a cobrança não tem Pix copia e cola"
        ),
        "{stderr}"
    );
    assert!(!png.exists());

    // With the image on the standard output, the charge goes to stderr.
    let assert = env
        .cmd()
        .args(["cobranca", "emitir", "--arquivo"])
        .arg(&arquivo)
        .args(["--sim", "--aguardar", "--qrcode-png", "-"])
        .assert()
        .success();
    assert_eq!(stdout_of(&assert), "");
    assert!(
        stderr_of(&assert).contains("Cobrança NF-123"),
        "{}",
        stderr_of(&assert)
    );
}

/// Once the charge is requested, its code and how to follow it go to
/// stderr: with the image on the standard output, nothing else would say
/// which charge it was when the wait ends badly.
#[tokio::test(flavor = "multi_thread")]
async fn o_codigo_da_cobranca_solicitada_nunca_se_perde() {
    let vencimento = daqui_a(30);
    for (situacao, codigo_de_saida, erro) in [
        (
            "EM_PROCESSAMENTO",
            8,
            "mas o tempo de espera acabou (1 s) com ela ainda em processamento",
        ),
        (
            "FALHA_EMISSAO",
            5,
            "a cobrança não foi emitida: falha na emissão",
        ),
    ] {
        let env = env().await;
        env.mount_token("boleto-cobranca.write boleto-cobranca.read", None)
            .await;
        Mock::given(method("POST"))
            .and(path("/cobranca/v3/cobrancas"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"codigoSolicitacao": CODIGO})),
            )
            .expect(1)
            .mount(&env.server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/cobranca/v3/cobrancas/{CODIGO}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "cobranca": {"codigoSolicitacao": CODIGO, "situacao": situacao}
            })))
            .mount(&env.server)
            .await;
        let assert = env
            .cmd()
            .args(opcoes(&vencimento))
            .args([
                "--sim",
                "--aguardar",
                "--timeout",
                "1s",
                "--qrcode-png",
                "-",
            ])
            .assert()
            .code(codigo_de_saida);
        assert_eq!(stdout_of(&assert), "", "{situacao}");
        let stderr = stderr_of(&assert);
        for linha in [
            format!(
                "Cobrança solicitada: código {CODIGO}. Acompanhe com: inter-pj cobranca consultar {CODIGO}"
            ),
            erro.to_owned(),
        ] {
            assert!(stderr.contains(&linha), "{linha}\n{stderr}");
        }
        assert!(!stderr.contains("--timeout"), "{stderr}");
    }
}

/// The answer of the API in `--json`, with and without `--aguardar`, and
/// the exit codes of the issue in the binary: the secret and the token
/// never show, even with `-vv`.
#[tokio::test(flavor = "multi_thread")]
async fn emissao_em_json_e_codigos_de_saida_sem_vazar_segredo() {
    let vencimento = daqui_a(30);
    let sem_segredo = |assert: &assert_cmd::assert::Assert| {
        let saida = format!("{}{}", stdout_of(assert), stderr_of(assert));
        for segredo in [CLIENT_SECRET, TOKEN] {
            assert!(!saida.contains(segredo), "{segredo:?} vazou:\n{saida}");
        }
    };
    let pedido = |status: u16| {
        Mock::given(method("POST"))
            .and(path("/cobranca/v3/cobrancas"))
            .respond_with(
                ResponseTemplate::new(status).set_body_json(json!({"codigoSolicitacao": CODIGO})),
            )
            .expect(1)
    };

    // The request alone: the answer of the API, as it came.
    let env = self::env().await;
    env.mount_token("boleto-cobranca.write", None).await;
    pedido(200).mount(&env.server).await;
    let assert = env
        .cmd()
        .arg("-vv")
        .args(opcoes(&vencimento))
        .args(["--sim", "--json"])
        .assert()
        .success();
    sem_segredo(&assert);
    let json: Value = serde_json::from_str(&stdout_of(&assert)).unwrap();
    assert_eq!(json, json!({"codigoSolicitacao": CODIGO}));
    assert!(
        stderr_of(&assert).contains("POST /cobranca/v3/cobrancas → 200"),
        "{}",
        stderr_of(&assert)
    );

    // With --aguardar, the charge issued.
    let env = self::env().await;
    env.mount_token("boleto-cobranca.write boleto-cobranca.read", None)
        .await;
    pedido(200).mount(&env.server).await;
    mount_cobranca(&env, cobranca(true), 1).await;
    let assert = env
        .cmd()
        .arg("-vv")
        .args(opcoes(&vencimento))
        .args(["--sim", "--json", "--aguardar"])
        .assert()
        .success();
    sem_segredo(&assert);
    let json: Value = serde_json::from_str(&stdout_of(&assert)).unwrap();
    assert_eq!(json, cobranca(true));

    // Refused for lack of permission (4), and an uncertain result (9).
    for (status, codigo_de_saida) in [(403, 4), (503, 9)] {
        let env = self::env().await;
        env.mount_token("boleto-cobranca.write", None).await;
        pedido(status).mount(&env.server).await;
        let assert = env
            .cmd()
            .arg("-vv")
            .args(opcoes(&vencimento))
            .arg("--sim")
            .assert()
            .code(codigo_de_saida);
        sem_segredo(&assert);
        assert_eq!(stdout_of(&assert), "", "{status}");
    }

    // Not issued (5) and the time running out (8), in text.
    for (situacao, codigo_de_saida) in [("FALHA_EMISSAO", 5), ("EM_PROCESSAMENTO", 8)] {
        let env = self::env().await;
        env.mount_token("boleto-cobranca.write boleto-cobranca.read", None)
            .await;
        pedido(200).mount(&env.server).await;
        Mock::given(method("GET"))
            .and(path(format!("/cobranca/v3/cobrancas/{CODIGO}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "cobranca": {"codigoSolicitacao": CODIGO, "seuNumero": "NF-123", "situacao": situacao}
            })))
            .mount(&env.server)
            .await;
        let assert = env
            .cmd()
            .arg("-vv")
            .args(opcoes(&vencimento))
            .args(["--sim", "--aguardar", "--timeout", "1s"])
            .assert()
            .code(codigo_de_saida);
        sem_segredo(&assert);
        // The charge, as it stands, on stdout.
        assert!(
            stdout_of(&assert).starts_with("Cobrança NF-123"),
            "{situacao}: {}",
            stdout_of(&assert)
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn simulacao_da_emissao_nao_envia_nada() {
    let env = env().await;
    nothing_is_sent(&env).await;
    let vencimento = daqui_a(30);

    let assert = env
        .cmd()
        .args(opcoes(&vencimento))
        .arg("--simular")
        .assert()
        .success();
    let stdout = stdout_of(&assert);
    assert!(
        stdout.starts_with(&format!(
            "Simulação: nada foi enviado.\n\nPOST {}/cobranca/v3/cobrancas\n",
            env.server.uri()
        )),
        "{stdout}"
    );

    let assert = env
        .cmd()
        .args(opcoes(&vencimento))
        .args(["--simular", "--json"])
        .assert()
        .success();
    let json: Value = serde_json::from_str(&stdout_of(&assert)).unwrap();
    assert_eq!(json["corpo"], corpo(&vencimento));

    // The template, from the standard input.
    let modelo = stdout_of(&env.cmd().args(["cobranca", "modelo"]).assert().success());
    let assert = env
        .cmd()
        .args([
            "cobranca",
            "emitir",
            "--arquivo",
            "-",
            "--simular",
            "--json",
        ])
        .write_stdin(modelo)
        .assert()
        .success();
    let json: Value = serde_json::from_str(&stdout_of(&assert)).unwrap();
    assert_eq!(json["corpo"]["valorNominal"], 150);
    assert_eq!(json["corpo"]["desconto"]["quantidadeDias"], 5);
}

#[tokio::test(flavor = "multi_thread")]
async fn emissao_invalida_ou_sem_confirmacao_nao_chama_a_api() {
    let env = env().await;
    nothing_is_sent(&env).await;
    let vencimento = daqui_a(30);

    // No terminal and no --sim: the summary, then the refusal.
    let assert = env
        .cmd()
        .args(opcoes(&vencimento))
        .write_stdin("s\n")
        .assert()
        .code(2);
    let stderr = stderr_of(&assert);
    assert!(stderr.contains("Cobrança a emitir"), "{stderr}");
    assert!(stderr.contains("use --sim"), "{stderr}");

    let mut passado = opcoes("2020-01-10");
    passado.push("--sim".to_owned());
    let assert = env.cmd().args(passado).assert().code(2);
    assert!(
        stderr_of(&assert).contains("o vencimento (10/01/2020) já passou"),
        "{}",
        stderr_of(&assert)
    );

    let mut barata = opcoes(&vencimento);
    let i = barata.iter().position(|opcao| opcao == "--valor").unwrap();
    barata[i + 1] = "2,00".to_owned();
    let assert = env.cmd().args(barata).arg("--sim").assert().code(2);
    assert!(
        stderr_of(&assert).contains("--valor: "),
        "{}",
        stderr_of(&assert)
    );

    let arquivo = env.path("cobranca.json");
    let modelo = stdout_of(&env.cmd().args(["cobranca", "modelo"]).assert().success());
    fs::write(&arquivo, modelo.replace("30110-000", "3011")).unwrap();
    let assert = env
        .cmd()
        .args(["cobranca", "emitir", "--sim", "--arquivo"])
        .arg(&arquivo)
        .assert()
        .code(2);
    assert!(
        stderr_of(&assert).contains("cobranca.json, campo \"pagador.cep\""),
        "{}",
        stderr_of(&assert)
    );

    // clap: what only makes sense after waiting, and waiting in a simulation.
    for extra in [&["--sim", "--qrcode"][..], &["--simular", "--aguardar"][..]] {
        env.cmd()
            .args(opcoes(&vencimento))
            .args(extra)
            .assert()
            .code(2);
    }
}

/// The commands the issue suggests run on the same account: the global
/// options given on the command line go into them, the ones in the
/// environment stay there.
#[tokio::test(flavor = "multi_thread")]
async fn as_dicas_da_emissao_sao_da_mesma_conta() {
    let env = TestEnv::new().await;
    env.write_config(&format!(
        "\n[perfis.filial]\nambiente = \"sandbox\"\nclient_id = \"id-da-filial\"\ncertificado = '{}'\nchave_privada = '{}'",
        env.path("certificado.crt").display(),
        env.path("chave.key").display()
    ));
    env.mount_token("boleto-cobranca.write", None).await;
    Mock::given(method("POST"))
        .and(path("/cobranca/v3/cobrancas"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(4)
        .expect(4)
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
    ];
    let vencimento = daqui_a(30);
    for (globais, no_ambiente, chamada) in &casos {
        let mut cmd = env.cmd();
        if let Some(perfil) = no_ambiente {
            cmd.env("INTER_PERFIL", perfil);
        }
        let assert = cmd
            .args(globais)
            .args(opcoes(&vencimento))
            .arg("--sim")
            .assert()
            .code(9);
        let stderr = stderr_of(&assert);
        assert!(
            stderr.contains(&format!(
                "dica: confira antes de tentar de novo: {chamada} cobranca listar --filtrar-por emissao --seu-numero NF-123"
            )),
            "{globais:?}: {stderr}"
        );
        assert!(
            stderr.contains("(boleto-cobranca.read), e uma lista vazia logo depois do erro não prova que nada foi emitido"),
            "{stderr}"
        );
    }

    // After the 503s, the charge is requested: how to follow it keeps the
    // account too.
    Mock::given(method("POST"))
        .and(path("/cobranca/v3/cobrancas"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"codigoSolicitacao": CODIGO})),
        )
        .expect(1)
        .mount(&env.server)
        .await;
    let assert = env
        .cmd()
        .args(["-p", "filial"])
        .args(opcoes(&vencimento))
        .arg("--sim")
        .assert()
        .success();
    let stdout = stdout_of(&assert);
    assert!(
        stdout.contains(&format!(
            "Acompanhe com: inter-pj -p filial cobranca consultar {CODIGO}"
        )),
        "{stdout}"
    );
}

/// The API refuses a repeated charge for 30 minutes, but a charge that may
/// have been issued is not repeated: the error says how to find it first.
#[tokio::test(flavor = "multi_thread")]
async fn resultado_incerto_orienta_a_conferir_antes_de_emitir_de_novo() {
    let env = env().await;
    env.mount_token("boleto-cobranca.write", Some(1)).await;
    Mock::given(method("POST"))
        .and(path("/cobranca/v3/cobrancas"))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&env.server)
        .await;

    let assert = env
        .cmd()
        .args(opcoes(&daqui_a(30)))
        .arg("--sim")
        .assert()
        .code(9);
    let stderr = stderr_of(&assert);
    assert!(
        stderr.contains("dica: a cobrança pode ter sido emitida"),
        "{stderr}"
    );
    assert!(
        stderr.contains(
            "dica: confira antes de tentar de novo: inter-pj cobranca listar --filtrar-por emissao --seu-numero NF-123"
        ),
        "{stderr}"
    );
}

// --- cancelar, editar, edicao e pagar ----------------------------------------------

const EDICAO: &str = "5a6b7c8d-1e2f-4a3b-8c9d-0e1f2a3b4c5d";

#[tokio::test(flavor = "multi_thread")]
async fn cancela_depois_de_mostrar_a_cobranca() {
    let env = env().await;
    env.mount_token("boleto-cobranca.write boleto-cobranca.read", None)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/cobranca/v3/cobrancas/{CODIGO}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(cobranca(true)))
        .expect(1)
        .mount(&env.server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/cobranca/v3/cobrancas/{CODIGO}/cancelar")))
        .and(body_json(json!({"motivoCancelamento": "Pedido cancelado"})))
        .respond_with(ResponseTemplate::new(202))
        .expect(1)
        .mount(&env.server)
        .await;

    let assert = env
        .cmd()
        .args([
            "cobranca",
            "cancelar",
            CODIGO,
            "--motivo",
            " Pedido cancelado ",
            "--sim",
        ])
        .assert()
        .success();
    assert_eq!(
        stdout_of(&assert),
        format!("Cancelamento solicitado.\n\nConfira com: inter-pj cobranca consultar {CODIGO}\n")
    );
    let stderr = stderr_of(&assert);
    for linha in [
        "Cobrança a cancelar",
        "NF-123",
        "a receber",
        "R$ 150,00",
        "Cliente Exemplo Ltda (12.345.678/0001-95)",
        "Pedido cancelado",
    ] {
        assert!(stderr.contains(linha), "{linha}\n{stderr}");
    }
}

/// What `cancelar`, `editar` and `pagar` suggest runs on the same account:
/// the global options of the command line go into it.
#[tokio::test(flavor = "multi_thread")]
async fn as_sugestoes_das_alteracoes_sao_da_mesma_conta() {
    let env = TestEnv::new().await;
    env.write_config(&format!(
        "\n[perfis.filial]\nambiente = \"sandbox\"\nclient_id = \"id-da-filial\"\ncertificado = '{}'\nchave_privada = '{}'",
        env.path("certificado.crt").display(),
        env.path("chave.key").display()
    ));
    env.mount_token("boleto-cobranca.write boleto-cobranca.read", None)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/cobranca/v3/cobrancas/{CODIGO}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(cobranca(true)))
        .mount(&env.server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/cobranca/v3/cobrancas/{CODIGO}/cancelar")))
        .respond_with(ResponseTemplate::new(202))
        .mount(&env.server)
        .await;
    Mock::given(method("PATCH"))
        .and(path(format!("/cobranca/v3/cobrancas/{CODIGO}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": "PROCESSANDO",
            "codigoEdicao": EDICAO
        })))
        .mount(&env.server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/cobranca/v3/cobrancas/{CODIGO}/pagar")))
        .respond_with(ResponseTemplate::new(204))
        .mount(&env.server)
        .await;
    for (args, sugestao) in [
        (
            vec!["cancelar", CODIGO, "--motivo", "Pedido cancelado", "--sim"],
            format!("Confira com: inter-pj -p filial cobranca consultar {CODIGO}"),
        ),
        (
            vec!["editar", CODIGO, "--valor", "200,00", "--sim"],
            format!(
                "Acompanhe com: inter-pj -p filial cobranca edicao {EDICAO} --aguardar --recem-pedida"
            ),
        ),
        (
            vec!["pagar", CODIGO, "--com", "boleto"],
            format!("Confira com: inter-pj -p filial cobranca consultar {CODIGO}"),
        ),
    ] {
        let assert = env
            .cmd()
            .args(["-p", "filial", "cobranca"])
            .args(&args)
            .assert()
            .success();
        let stdout = stdout_of(&assert);
        assert!(
            stdout.ends_with(&format!("{sugestao}\n")),
            "{args:?}\n{stdout}"
        );
    }
}

/// `cancelar`, `editar` and `pagar` have no idempotency key: an answer lost
/// after the request (here, a 503) may hide an operation done, so the
/// result is uncertain (9), with the command that checks it. A refusal (4xx)
/// stays 5 and the rate limit, which surely did nothing, 6.
#[tokio::test(flavor = "multi_thread")]
async fn alteracao_de_resultado_incerto_sai_com_9() {
    let operacoes: [(&str, &[&str], &str, &str); 3] = [
        (
            "POST",
            &["cancelar", CODIGO, "--motivo", "Pedido cancelado", "--sim"],
            "/cancelar",
            "dica: a cobrança pode ter sido cancelada",
        ),
        (
            "PATCH",
            &["editar", CODIGO, "--valor", "200,00", "--sim"],
            "",
            "dica: a alteração pode ter sido pedida, e repetir o comando pediria outra",
        ),
        (
            "POST",
            &["pagar", CODIGO, "--com", "pix"],
            "/pagar",
            "dica: a cobrança pode ter sido paga",
        ),
    ];
    for (metodo, args, sufixo, situacao) in operacoes {
        for (status, codigo_de_saida) in [(503, 9), (400, 5), (429, 6)] {
            let env = env().await;
            env.mount_token("boleto-cobranca.write boleto-cobranca.read", None)
                .await;
            Mock::given(method("GET"))
                .and(path(format!("/cobranca/v3/cobrancas/{CODIGO}")))
                .respond_with(ResponseTemplate::new(200).set_body_json(cobranca(true)))
                .mount(&env.server)
                .await;
            Mock::given(method(metodo))
                .and(path(format!("/cobranca/v3/cobrancas/{CODIGO}{sufixo}")))
                .respond_with(ResponseTemplate::new(status))
                .expect(1)
                .mount(&env.server)
                .await;
            let assert = env
                .cmd()
                .args(["-vv", "--sem-retentativa", "cobranca"])
                .args(args)
                .assert()
                .code(codigo_de_saida);
            let stderr = stderr_of(&assert);
            for segredo in [CLIENT_SECRET, TOKEN] {
                assert!(!stderr.contains(segredo), "{segredo:?} vazou:\n{stderr}");
            }
            let conferir = format!(
                "dica: confira antes de tentar de novo: inter-pj cobranca consultar {CODIGO}"
            );
            if codigo_de_saida == 9 {
                assert!(stderr.contains(situacao), "{args:?}\n{stderr}");
                assert!(stderr.contains(&conferir), "{args:?}\n{stderr}");
            } else {
                assert!(!stderr.contains(&conferir), "{args:?} {status}\n{stderr}");
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn cancelamento_sem_confirmacao_ou_motivo_nao_chama_a_api() {
    let env = env().await;
    nothing_is_sent(&env).await;
    // No terminal and no --sim: not even the lookup.
    let assert = env
        .cmd()
        .args([
            "cobranca",
            "cancelar",
            CODIGO,
            "--motivo",
            "Pedido cancelado",
        ])
        .write_stdin("s\n")
        .assert()
        .code(2);
    assert!(
        stderr_of(&assert).contains("use --sim"),
        "{}",
        stderr_of(&assert)
    );
    let longo = "x".repeat(51);
    for motivo in ["", "  ", longo.as_str(), "linha\nquebrada"] {
        env.cmd()
            .args(["cobranca", "cancelar", CODIGO, "--motivo", motivo, "--sim"])
            .assert()
            .code(2);
    }
    env.cmd()
        .args(["cobranca", "cancelar", CODIGO, "--sim"])
        .assert()
        .code(2);
}

#[tokio::test(flavor = "multi_thread")]
async fn edita_e_acompanha_a_alteracao() {
    let env = env().await;
    let vencimento = daqui_a(40);
    env.mount_token("boleto-cobranca.write boleto-cobranca.read", None)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/cobranca/v3/cobrancas/{CODIGO}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(cobranca(true)))
        .expect(1)
        .mount(&env.server)
        .await;
    Mock::given(method("PATCH"))
        .and(path(format!("/cobranca/v3/cobrancas/{CODIGO}")))
        .and(body_json(
            json!({"dataVencimento": vencimento, "valorNominal": 200}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": "PROCESSANDO",
            "codigoEdicao": EDICAO
        })))
        .expect(1)
        .mount(&env.server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/cobranca/v3/cobrancas/edicao/{EDICAO}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"status": "SUCESSO"})))
        .expect(2)
        .mount(&env.server)
        .await;

    let assert = env
        .cmd()
        .args([
            "cobranca",
            "editar",
            CODIGO,
            "--valor",
            "200,00",
            "--vencimento",
            &vencimento,
            "--sim",
        ])
        .assert()
        .success();
    let stdout = stdout_of(&assert);
    assert!(
        stdout.starts_with("Alteração em processamento."),
        "{stdout}"
    );
    assert!(
        stdout.ends_with(&format!(
            "Acompanhe com: inter-pj cobranca edicao {EDICAO} --aguardar --recem-pedida\n"
        )),
        "{stdout}"
    );
    let stderr = stderr_of(&assert);
    let novo = chrono::NaiveDate::parse_from_str(&vencimento, "%Y-%m-%d")
        .unwrap()
        .format("%d/%m/%Y")
        .to_string();
    for linha in [
        "Cobrança a alterar".to_owned(),
        "R$ 150,00 → R$ 200,00".to_owned(),
        format!("20/10/2026 → {novo}"),
        "aviso: a consulta pode levar até 30 minutos".to_owned(),
    ] {
        assert!(stderr.contains(&linha), "{linha}\n{stderr}");
    }

    let texto = stdout_of(
        &env.cmd()
            .args(["cobranca", "edicao", EDICAO, "--aguardar"])
            .assert()
            .success(),
    );
    assert!(texto.starts_with("Alteração feita"), "{texto}");
    let json: Value = serde_json::from_str(&stdout_of(
        &env.cmd()
            .args(["cobranca", "edicao", EDICAO, "--json"])
            .assert()
            .success(),
    ))
    .unwrap();
    assert_eq!(json, json!({"codigoEdicao": EDICAO, "status": "SUCESSO"}));
}

#[tokio::test(flavor = "multi_thread")]
async fn alteracao_que_falhou_sai_com_5() {
    let env = env().await;
    env.mount_token("boleto-cobranca.read", None).await;
    Mock::given(method("GET"))
        .and(path(format!("/cobranca/v3/cobrancas/edicao/{EDICAO}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"status": "FALHA"})))
        .expect(2)
        .mount(&env.server)
        .await;
    // Waiting or not, a failure is an error, after showing the change.
    let texto = stdout_of(
        &env.cmd()
            .args(["cobranca", "edicao", EDICAO])
            .assert()
            .code(5),
    );
    assert!(texto.starts_with("A alteração não foi feita."), "{texto}");
    env.cmd()
        .args(["cobranca", "edicao", EDICAO, "--aguardar"])
        .assert()
        .code(5);
}

/// The command `editar` suggests goes on with its wait: a change the API
/// does not show yet is not refused (5) there, and the time running out
/// keeps its code (8). Without `--recem-pedida`, the same 404 is an error at
/// once, as for a code typed wrong.
#[tokio::test(flavor = "multi_thread")]
async fn a_dica_de_editar_espera_a_alteracao_aparecer() {
    let env = env().await;
    env.mount_token("boleto-cobranca.write boleto-cobranca.read", None)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/cobranca/v3/cobrancas/{CODIGO}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(cobranca(true)))
        .expect(1)
        .mount(&env.server)
        .await;
    Mock::given(method("PATCH"))
        .and(path(format!("/cobranca/v3/cobrancas/{CODIGO}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": "PROCESSANDO",
            "codigoEdicao": EDICAO
        })))
        .expect(1)
        .mount(&env.server)
        .await;
    let consulta = format!("/cobranca/v3/cobrancas/edicao/{EDICAO}");
    let nao_encontrada = Mock::given(method("GET"))
        .and(path(consulta.clone()))
        .respond_with(ResponseTemplate::new(404))
        .mount_as_scoped(&env.server)
        .await;
    let sem_desfecho = format!(
        "erro: a alteração foi pedida (código {EDICAO}), mas o tempo de espera acabou (1 s) sem que a consulta a encontrasse"
    );

    let assert = env
        .cmd()
        .args(["cobranca", "editar", CODIGO, "--valor", "200,00", "--sim"])
        .args(["--aguardar", "--timeout", "1s"])
        .assert()
        .code(8);
    let stderr = stderr_of(&assert);
    assert!(stderr.contains(&sem_desfecho), "{stderr}");
    let sugerido: Vec<String> = stderr
        .lines()
        .find_map(|linha| linha.strip_prefix("dica: acompanhe com: inter-pj "))
        .unwrap_or_else(|| panic!("{stderr}"))
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    assert_eq!(
        sugerido,
        ["cobranca", "edicao", EDICAO, "--aguardar", "--recem-pedida"]
    );

    // The suggested command, with the 404 still there.
    let assert = env
        .cmd()
        .args(&sugerido)
        .args(["--timeout", "1s"])
        .assert()
        .code(8);
    let stderr = stderr_of(&assert);
    assert!(stderr.contains(&sem_desfecho), "{stderr}");
    assert!(
        stderr.contains("dica: a alteração já foi pedida: não a peça de novo"),
        "{stderr}"
    );
    env.cmd()
        .args(["cobranca", "edicao", EDICAO, "--aguardar"])
        .assert()
        .code(5);
    // Only a wait goes on.
    env.cmd()
        .args(["cobranca", "edicao", EDICAO, "--recem-pedida"])
        .assert()
        .code(2);

    // Once the API shows the change, the wait goes to its end.
    drop(nao_encontrada);
    Mock::given(method("GET"))
        .and(path(consulta))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"status": "SUCESSO"})))
        .expect(1)
        .mount(&env.server)
        .await;
    let texto = stdout_of(&env.cmd().args(&sugerido).assert().success());
    assert!(texto.starts_with("Alteração feita"), "{texto}");
}

/// A query of a change that failed exits with 5, as `editar` and `edicao
/// --aguardar` do, after showing it; one done or being processed, with 0.
#[tokio::test(flavor = "multi_thread")]
async fn consulta_de_alteracao_que_falhou_sai_com_5() {
    for (status, codigo_de_saida, texto) in [
        ("SUCESSO", 0, "Alteração feita"),
        ("FALHA", 5, "A alteração não foi feita."),
        ("PROCESSANDO", 0, "Alteração em processamento."),
    ] {
        let env = env().await;
        env.mount_token("boleto-cobranca.read", None).await;
        Mock::given(method("GET"))
            .and(path(format!("/cobranca/v3/cobrancas/edicao/{EDICAO}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"status": status})))
            .expect(2)
            .mount(&env.server)
            .await;
        let assert = env
            .cmd()
            .args(["cobranca", "edicao", EDICAO])
            .assert()
            .code(codigo_de_saida);
        assert!(stdout_of(&assert).starts_with(texto), "{status}");
        let assert = env
            .cmd()
            .args(["cobranca", "edicao", EDICAO, "--json"])
            .assert()
            .code(codigo_de_saida);
        let json: Value = serde_json::from_str(&stdout_of(&assert)).unwrap();
        assert_eq!(json, json!({"codigoEdicao": EDICAO, "status": status}));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn edicao_invalida_nao_chama_a_api() {
    let env = env().await;
    nothing_is_sent(&env).await;
    for args in [
        &["cobranca", "editar", CODIGO, "--sim"][..],
        &["cobranca", "editar", CODIGO, "--valor", "2,00", "--sim"][..],
        &[
            "cobranca",
            "editar",
            CODIGO,
            "--vencimento",
            "2020-01-10",
            "--sim",
        ][..],
        &["cobranca", "editar", CODIGO, "--valor", "200"][..],
        &[
            "cobranca",
            "editar",
            "../banking/v2/saldo",
            "--valor",
            "200",
            "--sim",
        ][..],
        &["cobranca", "edicao", "../../banking/v2/saldo"][..],
    ] {
        env.cmd().args(args).assert().code(2);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn paga_no_sandbox() {
    let env = env().await;
    env.mount_token("boleto-cobranca.write", Some(1)).await;
    Mock::given(method("POST"))
        .and(path(format!("/cobranca/v3/cobrancas/{CODIGO}/pagar")))
        .and(body_json(json!({"pagarCom": "PIX"})))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&env.server)
        .await;
    let assert = env
        .cmd()
        .args(["cobranca", "pagar", CODIGO, "--com", "pix"])
        .assert()
        .success();
    assert_eq!(
        stdout_of(&assert),
        format!(
            "Cobrança paga no sandbox, com o Pix.\n\nConfira com: inter-pj cobranca consultar {CODIGO}\n"
        )
    );
}

/// Refused before any request: not even a token is asked for.
#[tokio::test(flavor = "multi_thread")]
async fn pagar_em_producao_nao_chama_a_api() {
    let env = env().await;
    nothing_is_sent(&env).await;
    let assert = env
        .cmd()
        .args([
            "--ambiente",
            "producao",
            "cobranca",
            "pagar",
            CODIGO,
            "--com",
            "boleto",
        ])
        .assert()
        .code(2);
    assert!(
        stderr_of(&assert).contains("cobranca pagar existe só no sandbox"),
        "{}",
        stderr_of(&assert)
    );
}

/// A query that fails during the wait does not lose the charge requested:
/// one the next query may answer (503) only delays it; one that waiting
/// does not fix (403) ends with the code and the command that follows it
/// (9), and the request is never sent again.
#[tokio::test(flavor = "multi_thread")]
async fn uma_consulta_que_falha_na_espera_nao_perde_a_cobranca() {
    let vencimento = daqui_a(30);
    let pedido = || {
        Mock::given(method("POST"))
            .and(path("/cobranca/v3/cobrancas"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"codigoSolicitacao": CODIGO})),
            )
            .expect(1)
    };
    let consulta = format!("/cobranca/v3/cobrancas/{CODIGO}");

    // A 503, then the charge issued: the wait goes on to the next interval.
    let env = env().await;
    env.mount_token("boleto-cobranca.write boleto-cobranca.read", None)
        .await;
    pedido().mount(&env.server).await;
    Mock::given(method("GET"))
        .and(path(consulta.clone()))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .expect(1)
        .mount(&env.server)
        .await;
    mount_cobranca(&env, cobranca(true), 1).await;
    let assert = env
        .cmd()
        .args(opcoes(&vencimento))
        .args([
            "--sim",
            "--aguardar",
            "--sem-retentativa",
            "--timeout",
            "30s",
        ])
        .assert()
        .success();
    assert!(stdout_of(&assert).starts_with("Cobrança NF-123"));
    assert!(
        stderr_of(&assert).contains("aguardando: a consulta falhou"),
        "{}",
        stderr_of(&assert)
    );

    // A 403: no permission to read, which waiting does not fix.
    let env = self::env().await;
    env.mount_token("boleto-cobranca.write boleto-cobranca.read", None)
        .await;
    pedido().mount(&env.server).await;
    Mock::given(method("GET"))
        .and(path(consulta))
        .respond_with(ResponseTemplate::new(403))
        .expect(1)
        .mount(&env.server)
        .await;
    let assert = env
        .cmd()
        .args(opcoes(&vencimento))
        .args(["--sim", "--aguardar"])
        .assert()
        .code(9);
    assert_eq!(stdout_of(&assert), "");
    let stderr = stderr_of(&assert);
    for linha in [
        format!("erro: a cobrança foi solicitada (código {CODIGO}), mas a consulta falhou: "),
        "dica: a cobrança já foi solicitada: não a emita de novo".to_owned(),
        format!("dica: acompanhe com: inter-pj cobranca consultar {CODIGO}"),
    ] {
        assert!(stderr.contains(&linha), "{linha}\n{stderr}");
    }
}

/// Declining the confirmation sends nothing and exits with 7. The question
/// needs a terminal, which `script` (util-linux) gives the command.
#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread")]
async fn confirmacao_negada_sai_com_7() {
    let env = env().await;
    nothing_is_sent(&env).await;
    let linha = std::iter::once(env!("CARGO_BIN_EXE_inter-pj").to_owned())
        .chain(opcoes(&daqui_a(30)))
        .map(|arg| format!("'{}'", arg.replace('\'', r"'\''")))
        .collect::<Vec<_>>()
        .join(" ");
    let assert = env
        .programa("script")
        .args(["--quiet", "--return", "--command", &linha, "/dev/null"])
        .write_stdin("n\n")
        .assert()
        .code(7);
    // The terminal of `script` gets stdout and stderr.
    let saida = stdout_of(&assert);
    for linha in [
        "Emitir a cobrança? [s/N]",
        "erro: operação cancelada: nada foi enviado",
    ] {
        assert!(saida.contains(linha), "{linha}\n{saida}");
    }
}
