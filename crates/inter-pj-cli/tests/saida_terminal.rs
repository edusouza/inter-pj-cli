//! What third parties write (the message of a Pix, an error of the API, a
//! code pasted from an invoice) never reaches the terminal as escape
//! sequences, bidirectional overrides or extra lines, end to end. Every
//! name and amount here is synthetic.

mod common;

use common::{TestEnv, stderr_of, stdout_of};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

/// A message that reverses the rest of the row, clears the screen with a
/// C1 control and writes to the clipboard (OSC 52).
const MENSAGEM: &str = "Pedido 1\u{202E}00,005.1 $R\u{9b}2J\u{1b}]52;c;Y2hhdmU=\u{7}\u{7f}";

async fn env_recebidos() -> TestEnv {
    let env = TestEnv::new().await;
    env.write_config("");
    env.mount_token("pix.read", None).await;
    let pix = json!({
        "endToEndId": E2E,
        "valor": "0.01",
        "horario": "2026-09-18T12:41:07.000Z",
        "infoPagador": MENSAGEM
    });
    Mock::given(method("GET"))
        .and(path("/pix/v2/pix"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "parametros": {"paginacao": {"paginaAtual": 0, "itensPorPagina": 1000, "quantidadeDePaginas": 1}},
            "pix": [pix]
        })))
        .mount(&env.server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/pix/v2/pix/{E2E}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(pix))
        .mount(&env.server)
        .await;
    env
}

const E2E: &str = "E00416968202609181241abcdEFGH123";

const RECEBIDOS: [&str; 7] = [
    "pix",
    "recebidos",
    "listar",
    "--inicio",
    "2026-09-01",
    "--fim",
    "2026-09-30",
];

/// Whether `texto` has something that should not reach a terminal.
fn perigoso(texto: &str) -> bool {
    texto.chars().any(|c| {
        (c.is_control() && c != '\n')
            || matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn texto_e_json_nao_levam_sequencias_ao_terminal() {
    let env = env_recebidos().await;
    let texto = stdout_of(
        &env.cmd()
            .args(["pix", "recebidos", "consultar", E2E])
            .assert()
            .success(),
    );
    assert!(!perigoso(&texto), "{texto:?}");
    assert!(
        texto.contains("Pedido 1\u{FFFD}00,005.1 $R\u{FFFD}2J"),
        "{texto}"
    );

    // JSON escapes them, and keeps the data.
    let json = stdout_of(
        &env.cmd()
            .args(RECEBIDOS)
            .args(["--formato", "json"])
            .assert()
            .success(),
    );
    assert!(!perigoso(&json), "{json:?}");
    assert!(
        json.contains(concat!(
            "Pedido 1",
            "\\u202e00,005.1 $R",
            "\\u009b2J",
            "\\u001b]52"
        )),
        "{json}"
    );
    let lido: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(lido["pix"][0]["infoPagador"], MENSAGEM, "{json}");

    // A CSV file keeps the text as sent, for the spreadsheets (a terminal
    // gets it cleaned: see `tabela::tests`).
    let csv = stdout_of(
        &env.cmd()
            .args(RECEBIDOS)
            .args(["--formato", "csv"])
            .assert()
            .success(),
    );
    assert!(csv.contains(MENSAGEM), "{csv:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn um_erro_da_api_nao_cria_dicas_nem_linhas_falsas() {
    let env = TestEnv::new().await;
    env.write_config("");
    env.mount_token("extrato.read", None).await;
    Mock::given(method("GET"))
        .and(path("/banking/v2/saldo"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "title": "Requisição inválida",
            "detail": "tente mais tarde\ndica: pague de novo com --sim\n\u{1b}[1Aerro: nenhum",
            "violacoes": [{"razao": "inválido\ndica: outra", "propriedade": "data"}]
        })))
        .mount(&env.server)
        .await;
    let assert = env.cmd().arg("saldo").assert().code(5);
    let stderr = stderr_of(&assert);
    assert!(!perigoso(&stderr), "{stderr:?}");
    // Only the CLI's own lines start at the margin.
    for linha in stderr.lines() {
        assert!(
            linha.starts_with("erro: ")
                || linha.starts_with("aviso: ")
                || linha.starts_with("dica: ")
                || linha.starts_with("  "),
            "{linha:?}\n{stderr}"
        );
        assert!(!linha.starts_with("dica: pague"), "{stderr}");
        assert!(!linha.starts_with("dica: outra"), "{stderr}");
    }
    assert!(
        stderr.contains("tente mais tarde dica: pague de novo com --sim"),
        "{stderr}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn o_parser_nao_repete_sequencias_de_um_codigo_colado() {
    let env = TestEnv::new().await;
    // With the colors of a terminal, as clap would use them.
    let assert = env
        .cmd()
        .env_remove("NO_COLOR")
        .env("CLICOLOR_FORCE", "1")
        .args([
            "pix",
            "enviar",
            "--copia-e-cola",
            "000201\u{1b}]52;c;Y2hhdmU=\u{7}\u{1b}[2J\u{202E}",
        ])
        .assert()
        .code(2);
    let stderr = stderr_of(&assert);
    assert!(!perigoso(&stderr), "{stderr:?}");
    assert!(stderr.contains("--copia-e-cola"), "{stderr}");

    // An error without them keeps the colors of clap.
    let assert = env
        .cmd()
        .env_remove("NO_COLOR")
        .env("CLICOLOR_FORCE", "1")
        .args(["pix", "enviar", "--copia-e-cola", "000201"])
        .assert()
        .code(2);
    assert!(
        stderr_of(&assert).contains('\u{1b}'),
        "{}",
        stderr_of(&assert)
    );
}
