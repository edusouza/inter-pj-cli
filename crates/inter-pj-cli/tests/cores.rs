//! The colors of the tables, end to end. The standard output of the tests
//! is a pipe, where there are never colors; `CLICOLOR_FORCE` stands in for
//! a terminal. Every name and amount here is synthetic.

mod common;

use assert_cmd::Command;
use common::{TestEnv, stderr_of, stdout_of};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

const ESC: char = '\u{1b}';

/// The codes of the CLI, to compare a colored output with a plain one.
const CODIGOS: [&str; 5] = [
    "\u{1b}[1m",
    "\u{1b}[31m",
    "\u{1b}[32m",
    "\u{1b}[33m",
    "\u{1b}[0m",
];

fn sem_codigos(texto: &str) -> String {
    CODIGOS
        .iter()
        .fold(texto.to_owned(), |texto, codigo| texto.replace(codigo, ""))
}

/// A statement with a credit whose description brings escape sequences,
/// and a debit, shown as negative.
async fn env_extrato() -> TestEnv {
    let env = TestEnv::new().await;
    env.write_config("");
    env.mount_token("extrato.read", None).await;
    Mock::given(method("GET"))
        .and(path("/banking/v2/extrato"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"transacoes": [
                {"dataEntrada": "2026-08-03", "tipoTransacao": "PIX", "tipoOperacao": "C",
                 "valor": "1500.00", "titulo": "Pix recebido",
                 "descricao": "Cliente\u{1b}[2K\u{1b}[32m"},
                {"dataEntrada": "2026-08-05", "tipoTransacao": "PAGAMENTO", "tipoOperacao": "D",
                 "valor": "250.10", "titulo": "Pagamento efetuado", "descricao": "Boleto"}
            ]})),
        )
        .mount(&env.server)
        .await;
    env
}

/// `inter-pj` with the variables of the colors as given, not as the tests
/// set them.
fn cmd(env: &TestEnv, variaveis: &[(&str, &str)]) -> Command {
    let mut cmd = env.cmd();
    cmd.env_remove("NO_COLOR")
        .env_remove("CLICOLOR_FORCE")
        .env_remove("CLICOLOR")
        .env("TERM", "xterm-256color");
    for (nome, valor) in variaveis {
        cmd.env(nome, valor);
    }
    cmd
}

const AGOSTO: [&str; 5] = ["extrato", "--inicio", "2026-08-01", "--fim", "2026-08-31"];

#[tokio::test(flavor = "multi_thread")]
async fn fora_de_um_terminal_nao_ha_codigos_ansi() {
    let env = env_extrato().await;
    // Not even when the terminal says it takes colors.
    for variaveis in [&[][..], &[("CLICOLOR", "1")]] {
        let assert = cmd(&env, variaveis).args(AGOSTO).assert().success();
        assert!(!stdout_of(&assert).contains(ESC), "{}", stdout_of(&assert));
        assert!(!stderr_of(&assert).contains(ESC), "{}", stderr_of(&assert));
    }
    for ajuda in [&["--help"][..], &["extrato", "--help"]] {
        let assert = cmd(&env, &[]).args(ajuda).assert().success();
        assert!(!stdout_of(&assert).contains(ESC), "{}", stdout_of(&assert));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn num_terminal_a_tabela_destaca_o_cabecalho_e_os_valores_negativos() {
    let env = env_extrato().await;
    let simples = stdout_of(&cmd(&env, &[]).args(AGOSTO).assert().success());
    let colorido = stdout_of(
        &cmd(&env, &[("CLICOLOR_FORCE", "1")])
            .args(AGOSTO)
            .assert()
            .success(),
    );
    for trecho in [
        "\u{1b}[1mData\u{1b}[0m  ",
        "\u{1b}[1mValor\u{1b}[0m\n",
        "  \u{1b}[31m-R$ 250,10\u{1b}[0m\n",
        // A credit has no color.
        "  R$ 1.500,00\n",
    ] {
        assert!(colorido.contains(trecho), "{trecho:?}\n{colorido}");
    }
    // The codes of the data are replaced like any control character, even
    // one that is also a code of the CLI.
    assert!(
        colorido.contains("Cliente\u{FFFD}[2K\u{FFFD}[32m"),
        "{colorido}"
    );
    // Without the codes, the same text, aligned the same way.
    assert_eq!(sem_codigos(&colorido), simples);
}

#[tokio::test(flavor = "multi_thread")]
async fn no_color_e_sem_cor_vencem_o_terminal() {
    let env = env_extrato().await;
    for (variaveis, argumentos) in [
        (
            &[("CLICOLOR_FORCE", "1"), ("NO_COLOR", "1")][..],
            &AGOSTO[..],
        ),
        (
            &[("CLICOLOR_FORCE", "1")],
            &[
                "--sem-cor",
                "extrato",
                "--inicio",
                "2026-08-01",
                "--fim",
                "2026-08-31",
            ],
        ),
        (
            &[("CLICOLOR_FORCE", "1")],
            &[
                "extrato",
                "--inicio",
                "2026-08-01",
                "--fim",
                "2026-08-31",
                "--sem-cor",
            ],
        ),
    ] {
        let assert = cmd(&env, variaveis).args(argumentos).assert().success();
        assert!(
            !stdout_of(&assert).contains(ESC),
            "{variaveis:?} {argumentos:?}\n{}",
            stdout_of(&assert)
        );
    }

    // The help too, which clap colors in a terminal.
    let colorida = cmd(&env, &[("CLICOLOR_FORCE", "1")])
        .arg("--help")
        .assert()
        .success();
    assert!(stdout_of(&colorida).contains(ESC));
    let sem_cor = cmd(&env, &[("CLICOLOR_FORCE", "1")])
        .args(["--help", "--sem-cor"])
        .assert()
        .success();
    assert!(
        !stdout_of(&sem_cor).contains(ESC),
        "{}",
        stdout_of(&sem_cor)
    );
    assert!(
        stdout_of(&sem_cor).contains("--sem-cor"),
        "{}",
        stdout_of(&sem_cor)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn cada_status_tem_o_seu_tom() {
    let env = TestEnv::new().await;
    env.write_config("");
    env.mount_token("pagamento-boleto.read", None).await;
    let pagamento = |status: &str, codigo: &str| {
        json!({
            "codigoTransacao": codigo,
            "dataVencimentoTitulo": "2099-10-10", "dataPagamento": "2099-10-09",
            "nomeBeneficiario": "Fornecedor Exemplo", "statusPagamento": status,
            "valorNominal": 30.1
        })
    };
    Mock::given(method("GET"))
        .and(path("/banking/v2/pagamento"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            pagamento("AGENDADO", "codigo-1"),
            pagamento("REALIZADO", "codigo-2"),
            pagamento("CANCELADO", "codigo-3"),
            pagamento("UM_STATUS_NOVO", "codigo-4"),
        ])))
        .mount(&env.server)
        .await;
    let colorido = stdout_of(
        &cmd(&env, &[("CLICOLOR_FORCE", "1")])
            .args([
                "pagamento",
                "boleto",
                "listar",
                "--inicio",
                "2099-10-01",
                "--fim",
                "2099-10-31",
            ])
            .assert()
            .success(),
    );
    for trecho in [
        "\u{1b}[33magendado\u{1b}[0m",
        "\u{1b}[32mpago\u{1b}[0m",
        "\u{1b}[31mcancelado\u{1b}[0m",
        // What the CLI does not know has no tone.
        "  UM_STATUS_NOVO  ",
    ] {
        assert!(colorido.contains(trecho), "{trecho:?}\n{colorido}");
    }
}

/// The summaries go to stderr, for someone to read before confirming: in a
/// terminal or not, they are plain. The CSV is the same with or without a
/// terminal (it keeps the text as sent, for the spreadsheets).
#[tokio::test(flavor = "multi_thread")]
async fn resumos_e_csv_nunca_tem_cores() {
    let env = TestEnv::new().await;
    env.write_config("");
    let lote = "\u{feff}tipoPagamento;codBarraLinhaDigitavel;valorPagar;dataVencimento;cnpjCpf;codigoReceita;periodoApuracao;valorPrincipal;referencia;descricao;nomeEmpresa\r
DARF;;;2099-10-30;12.345.678/0001-95;0220;2099-09-30;47,14;13609400849201739;IRPJ de setembro;Empresa Exemplo\r
DARF;;;2099-11-30;12.345.678/0001-95;0220;2099-10-31;52,60;13609400849201739;IRPJ de outubro;Empresa Exemplo\r
";
    let assert = cmd(&env, &[("CLICOLOR_FORCE", "1")])
        .args(["pagamento", "lote", "enviar", "--arquivo", "-", "--simular"])
        .write_stdin(lote)
        .assert()
        .success();
    let resumo = stderr_of(&assert);
    assert!(resumo.contains("DARF"), "{resumo}");
    assert!(!resumo.contains(ESC), "{resumo}");

    let env = env_extrato().await;
    let csv = |variaveis: &[(&str, &str)]| {
        stdout_of(
            &cmd(&env, variaveis)
                .args(AGOSTO)
                .args(["--formato", "csv"])
                .assert()
                .success(),
        )
    };
    assert_eq!(csv(&[("CLICOLOR_FORCE", "1")]), csv(&[]));
}
