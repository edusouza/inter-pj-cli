//! `inter-pj pagamento lote enviar|consultar|modelo` end to end, against a
//! mock API. The codes are examples of the API documentation and of its
//! sandbox; documents, names and amounts are synthetic.

mod common;

use common::{TestEnv, stderr_of, stdout_of};
use serde_json::{Value, json};
use wiremock::matchers::{any, body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

const LOTE: &str = "/banking/v2/pagamento/lote";
const ID_LOTE: &str = "0123456789abcdef01234567";

/// A boleto of R$ 30,10 due on 2026-10-10, the water bill of the sandbox and
/// a DARF, as saved by Excel in Portuguese.
const CSV: &str = "\u{feff}tipoPagamento;codBarraLinhaDigitavel;valorPagar;dataVencimento;cnpjCpf;codigoReceita;periodoApuracao;valorPrincipal;referencia;descricao;nomeEmpresa\r
BOLETO;07797.77705 11678.471159 90071.126347 1 15950000003010;;;;;;;;;\r
BOLETO;82670000000653301602023123106000000002830894;65,33;2026-10-10;;;;;;;\r
DARF;;;2099-10-30;12.345.678/0001-95;0220;2026-09-30;47,14;13609400849201739;IRPJ de setembro;Empresa Exemplo\r
";

fn corpo() -> Value {
    json!({
        "meuIdentificador": "Outubro",
        "pagamentos": [
            {
                "tipoPagamento": "BOLETO",
                "codBarraLinhaDigitavel": "07791159500000030107777011678471159007112634",
                "valorPagar": 30.1,
                "dataVencimento": "2026-10-10"
            },
            {
                "tipoPagamento": "BOLETO",
                "codBarraLinhaDigitavel": "82670000000653301602023123106000000002830894",
                "valorPagar": 65.33,
                "dataVencimento": "2026-10-10"
            },
            {
                "tipoPagamento": "DARF",
                "cnpjCpf": "12345678000195",
                "codigoReceita": "0220",
                "dataVencimento": "2099-10-30",
                "descricao": "IRPJ de setembro",
                "nomeEmpresa": "Empresa Exemplo",
                "periodoApuracao": "2026-09-30",
                "valorPrincipal": 47.14,
                "referencia": "13609400849201739"
            }
        ]
    })
}

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

#[tokio::test(flavor = "multi_thread")]
async fn os_modelos_sao_lotes_validos() {
    let env = env().await;
    nothing_is_sent(&env).await;
    for tipo in ["json", "csv"] {
        let modelo = stdout_of(
            &env.cmd()
                .args(["pagamento", "lote", "modelo", tipo])
                .assert()
                .success(),
        );
        let arquivo = env.path(&format!("lote.{tipo}"));
        std::fs::write(&arquivo, &modelo).unwrap();
        let assert = env
            .cmd()
            .args(["pagamento", "lote", "enviar", "--arquivo"])
            .arg(&arquivo)
            .args(["--simular", "--json"])
            .assert()
            .success();
        let json: Value = serde_json::from_str(&stdout_of(&assert)).unwrap();
        let tipos: Vec<&str> = json["corpo"]["pagamentos"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["tipoPagamento"].as_str().unwrap())
            .collect();
        assert_eq!(tipos, ["BOLETO", "BOLETO", "DARF"], "{tipo}");
        assert!(
            stderr_of(&assert).contains("R$ 779,00 (setecentos e setenta e nove reais)"),
            "{}",
            stderr_of(&assert)
        );
    }
    // The CSV template opens in Excel: `;` and a byte order mark.
    let csv = stdout_of(
        &env.cmd()
            .args(["pagamento", "lote", "modelo", "csv"])
            .assert()
            .success(),
    );
    assert!(
        csv.starts_with("\u{feff}tipoPagamento;codBarraLinhaDigitavel;"),
        "{csv}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn envia_um_lote_de_planilha() {
    let env = env().await;
    env.mount_token("pagamento-lote.write", Some(1)).await;
    Mock::given(method("POST"))
        .and(path(LOTE))
        .and(body_json(corpo()))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "idLote": ID_LOTE,
            "status": "EMPROCESSAMENTO",
            "meuIdentificador": "Outubro",
            "qtdePagamentos": 3
        })))
        .expect(1)
        .mount(&env.server)
        .await;
    let arquivo = env.path("lote.csv");
    std::fs::write(&arquivo, CSV).unwrap();

    let assert = env
        .cmd()
        .args(["pagamento", "lote", "enviar", "--arquivo"])
        .arg(&arquivo)
        .args(["--identificador", "Outubro", "--sim"])
        .assert()
        .success();
    let stdout = stdout_of(&assert);
    assert!(
        stdout.starts_with("Lote recebido: 3 pagamentos, em processamento."),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!(
            "Acompanhe com: inter-pj pagamento lote consultar {ID_LOTE} --aguardar"
        )),
        "{stdout}"
    );
    let stderr = stderr_of(&assert);
    for linha in [
        "Lote a enviar",
        "Pagamentos     2 boletos e contas (R$ 95,43) e 1 DARF (R$ 47,14)",
        "Total          R$ 142,57 (cento e quarenta e dois reais e cinquenta e sete centavos)",
        "linha 3  conta/tributo  82670000000-1 65330160202-1 31231060000-1 00002830894-8",
    ] {
        assert!(stderr.contains(linha), "{linha}\n{stderr}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn lote_com_problemas_nao_chama_a_api() {
    let env = env().await;
    nothing_is_sent(&env).await;
    let arquivo = env.path("lote.csv");
    let com_erros = CSV
        .replace(";65,33;2026-10-10;", ";65,33;;")
        .replace(";0220;", ";220;");
    std::fs::write(&arquivo, com_erros).unwrap();

    let assert = env
        .cmd()
        .args(["pagamento", "lote", "enviar", "--arquivo"])
        .arg(&arquivo)
        .arg("--sim")
        .assert()
        .code(2);
    let stderr = stderr_of(&assert);
    assert!(
        stderr.contains("lote.csv: 2 pagamentos com problema; nada foi enviado:"),
        "{stderr}"
    );
    assert!(
        stderr.contains("  linha 3, campo \"dataVencimento\": obrigatório"),
        "{stderr}"
    );
    assert!(
        stderr.contains(
            "  linha 4, campo \"codigoReceita\": \"220\": o código da receita tem 4 dígitos, e o Excel tira os zeros à esquerda"
        ),
        "{stderr}"
    );

    // One payment is not a batch.
    let um = CSV.lines().take(2).collect::<Vec<_>>().join("\n");
    std::fs::write(&arquivo, um).unwrap();
    let assert = env
        .cmd()
        .args(["pagamento", "lote", "enviar", "--arquivo"])
        .arg(&arquivo)
        .arg("--sim")
        .assert()
        .code(2);
    assert!(
        stderr_of(&assert).contains("o lote deve ter de 2 a 150 pagamentos, e tem 1"),
        "{}",
        stderr_of(&assert)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn le_o_lote_da_entrada_padrao() {
    let env = env().await;
    nothing_is_sent(&env).await;
    // The input is the file, so no one can answer the question.
    let assert = env
        .cmd()
        .args(["pagamento", "lote", "enviar", "--arquivo", "-"])
        .write_stdin(CSV)
        .assert()
        .code(2);
    assert!(
        stderr_of(&assert).contains("--sim"),
        "{}",
        stderr_of(&assert)
    );

    let assert = env
        .cmd()
        .args(["pagamento", "lote", "enviar", "--arquivo", "-", "--simular"])
        .write_stdin(CSV)
        .assert()
        .success();
    assert!(
        stderr_of(&assert).contains("\n  Arquivo     entrada padrão\n"),
        "{}",
        stderr_of(&assert)
    );

    let assert = env
        .cmd()
        .args(["pagamento", "lote", "enviar", "--arquivo", "-", "--sim"])
        .write_stdin("{\"pagamentos\": [")
        .assert()
        .code(2);
    assert!(
        stderr_of(&assert).contains("entrada padrão: JSON inválido na linha 1"),
        "{}",
        stderr_of(&assert)
    );
}

/// The same boleto twice: refused, even with --sim, unless the user says
/// they are distinct payments.
#[tokio::test(flavor = "multi_thread")]
async fn pagamentos_repetidos_sao_recusados() {
    let env = env().await;
    nothing_is_sent(&env).await;
    let repetido = format!("{CSV}BOLETO;07791159500000030107777011678471159007112634;;;;;;;;;\r\n");
    let assert = env
        .cmd()
        .args(["pagamento", "lote", "enviar", "--arquivo", "-", "--sim"])
        .write_stdin(repetido.clone())
        .assert()
        .code(2);
    let stderr = stderr_of(&assert);
    assert!(
        stderr.contains("pagamentos repetidos em entrada padrão:\n  linha 2 e linha 5: o mesmo pagamento aparece mais de uma vez"),
        "{stderr}"
    );
    // A hint of its own, not a line of the list.
    assert!(
        stderr.contains("\ndica: se forem mesmo pagamentos distintos, use --permitir-repetidos"),
        "{stderr}"
    );

    let assert = env
        .cmd()
        .args([
            "pagamento",
            "lote",
            "enviar",
            "--arquivo",
            "-",
            "--simular",
            "--permitir-repetidos",
        ])
        .write_stdin(repetido)
        .assert()
        .success();
    assert!(
        stderr_of(&assert)
            .contains("aviso: linha 2 e linha 5: o mesmo pagamento aparece mais de uma vez"),
        "{}",
        stderr_of(&assert)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn o_limite_vale_para_cada_pagamento() {
    let env = TestEnv::new().await;
    env.write_config("limite_por_operacao = \"50,00\"");
    nothing_is_sent(&env).await;
    let arquivo = env.path("lote.csv");
    std::fs::write(&arquivo, CSV).unwrap();

    let assert = env
        .cmd()
        .args(["pagamento", "lote", "enviar", "--arquivo"])
        .arg(&arquivo)
        .arg("--sim")
        .assert()
        .code(2);
    let stderr = stderr_of(&assert);
    assert!(
        stderr.contains("linha 3: R$ 65,33 passa do limite por operação"),
        "{stderr}"
    );
    assert!(!stderr.contains("Lote a enviar"), "{stderr}");
}

async fn mount_lote(env: &TestEnv, status: &str) {
    Mock::given(method("GET"))
        .and(path(format!("{LOTE}/{ID_LOTE}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "idLote": ID_LOTE,
            "status": status,
            "qtdePagamentos": 2,
            "pagamentos": [
                {"tipoPagamento": "BOLETO", "status": "PAGO", "valorPagar": 30.1},
                {"tipoPagamento": "DARF", "status": "ERRO_PAGAMENTO", "valorTotal": 47.14, "detalhe": "Saldo insuficiente"}
            ]
        })))
        .mount(&env.server)
        .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn consulta_um_lote() {
    let env = env().await;
    env.mount_token("pagamento-lote.read", None).await;
    mount_lote(&env, "PROCESSADOCOMERRO").await;

    let texto = stdout_of(
        &env.cmd()
            .args(["pagamento", "lote", "consultar", ID_LOTE])
            .assert()
            .success(),
    );
    assert!(
        texto.starts_with(&format!(
            "Lote {ID_LOTE}\n  Status      processado com erro"
        )),
        "{texto}"
    );
    assert!(
        texto.contains("DARF    erro no pagamento  R$ 47,14"),
        "{texto}"
    );
    assert!(texto.contains("Saldo insuficiente"), "{texto}");

    let json: Value = serde_json::from_str(&stdout_of(
        &env.cmd()
            .args(["pagamento", "lote", "consultar", ID_LOTE, "--json"])
            .assert()
            .success(),
    ))
    .unwrap();
    assert_eq!(json["pagamentos"][1]["tipoPagamento"], "DARF");
    assert_eq!(json["pagamentos"][1]["detalhe"], "Saldo insuficiente");
}

#[tokio::test(flavor = "multi_thread")]
async fn aguardar_sai_conforme_o_resultado() {
    for (status, codigo) in [
        ("PROCESSADOSEMERRO", 0),
        ("PROCESSADOCOMERRO", 5),
        ("EMPROCESSAMENTO", 8),
    ] {
        let env = env().await;
        env.mount_token("pagamento-lote.read", None).await;
        mount_lote(&env, status).await;
        let assert = env
            .cmd()
            .args([
                "pagamento",
                "lote",
                "consultar",
                ID_LOTE,
                "--aguardar",
                "--timeout",
                "1s",
            ])
            .assert()
            .code(codigo);
        let stderr = stderr_of(&assert);
        match codigo {
            5 => assert!(
                stderr.contains("o lote foi processado com erro: 1 de 2 pagamentos não foi feito"),
                "{stderr}"
            ),
            8 => assert!(
                stderr
                    .contains("tempo de espera esgotado (1 s): o lote ainda está em processamento"),
                "{stderr}"
            ),
            _ => assert!(!stderr.contains("erro:"), "{stderr}"),
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn identificador_invalido_nao_chama_a_api() {
    let env = env().await;
    nothing_is_sent(&env).await;
    env.cmd()
        .args(["pagamento", "lote", "consultar", "../../banking/v2/saldo"])
        .assert()
        .code(2);
}
