//! `inter-pj pix enviar` end to end, against a mock API. Every key, amount
//! and account here is synthetic.

mod common;

use std::fmt::Write as _;

use common::{CLIENT_SECRET, TOKEN, TestEnv, stderr_of, stdout_of};
use predicates::prelude::*;
use serde_json::{Value, json};
use wiremock::matchers::{any, body_json, header, method, path};
use wiremock::{Mock, ResponseTemplate};

const PIX: &str = "/banking/v2/pix";
const ID: &str = "123e4567-e89b-42d3-a456-426614174000";
const CODIGO: &str = "c42f0787-02cb-4b31-827e-459ec9d7ece1";
const ENVIAR: [&str; 6] = [
    "pix",
    "enviar",
    "--chave",
    "fornecedor@empresa.example",
    "--valor",
    "150,00",
];

fn resposta(tipo: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "tipoRetorno": tipo,
        "codigoSolicitacao": CODIGO,
        "dataPagamento": "2026-10-01",
        "dataOperacao": "2026-09-23"
    }))
}

fn tlv(campos: &[(&str, &str)]) -> String {
    let mut texto = String::new();
    for (id, valor) in campos {
        let _ = write!(texto, "{id}{:02}{valor}", valor.chars().count());
    }
    texto
}

/// A static copia e cola code paying `fornecedor@empresa.example`.
fn copia_e_cola(valor: Option<&str>) -> String {
    let conta = tlv(&[
        ("00", "br.gov.bcb.pix"),
        ("01", "fornecedor@empresa.example"),
    ]);
    let mut campos = vec![
        ("00", "01"),
        ("26", conta.as_str()),
        ("52", "0000"),
        ("53", "986"),
    ];
    if let Some(valor) = valor {
        campos.push(("54", valor));
    }
    campos.extend([
        ("58", "BR"),
        ("59", "Fornecedor Exemplo"),
        ("60", "SAO PAULO"),
        ("62", "0505NF123"),
    ]);
    let mut codigo = tlv(&campos);
    codigo.push_str("6304");
    let crc = inter_pj::pix::crc16(codigo.as_bytes());
    let _ = write!(codigo, "{crc:04X}");
    codigo
}

/// The API must not be called at all.
async fn nothing_is_sent(env: &TestEnv) {
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&env.server)
        .await;
}

async fn mount_pix(env: &TestEnv, body: Value, response: ResponseTemplate) {
    env.mount_token("pagamento-pix.write", Some(1)).await;
    Mock::given(method("POST"))
        .and(path(PIX))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        .and(body_json(body))
        .respond_with(response)
        .expect(1)
        .mount(&env.server)
        .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn simulation_shows_the_request_and_sends_nothing() {
    let env = TestEnv::new().await;
    env.write_config("conta_corrente = \"7654321\"");
    nothing_is_sent(&env).await;

    let assert = env
        .cmd()
        .args(ENVIAR)
        .args(["--descricao", "NF 123", "--simular"])
        .assert()
        .success();
    let stdout = stdout_of(&assert);
    let stderr = stderr_of(&assert);
    assert!(
        stdout.starts_with("Simulação: nada foi enviado."),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("POST {}/banking/v2/pix", env.server.uri())),
        "{stdout}"
    );
    assert!(stdout.contains("x-id-idempotente: "), "{stdout}");
    assert!(stdout.contains("x-conta-corrente: *****21"), "{stdout}");
    assert!(
        stdout.contains("\"chave\": \"fornecedor@empresa.example\""),
        "{stdout}"
    );
    assert!(stdout.contains("\"valor\": 150"), "{stdout}");
    assert!(!stdout.contains("7654321"), "{stdout}");
    assert!(stderr.contains("Pix a enviar"), "{stderr}");
    assert!(
        stderr.contains("R$ 150,00 (cento e cinquenta reais)"),
        "{stderr}"
    );

    // In JSON, with the same idempotency key as shown in the summary.
    let assert = env
        .cmd()
        .args(ENVIAR)
        .args(["--simular", "--json", "--id-idempotente", ID])
        .assert()
        .success();
    let json: Value = serde_json::from_str(&stdout_of(&assert)).unwrap();
    assert_eq!(json["simulacao"], true);
    assert_eq!(json["metodo"], "POST");
    assert_eq!(json["cabecalhos"]["x-id-idempotente"], ID);
    assert_eq!(
        json["corpo"],
        json!({
            "valor": 150,
            "destinatario": {"tipo": "CHAVE", "chave": "fornecedor@empresa.example"}
        })
    );
    assert!(stderr_of(&assert).contains(ID));
}

#[tokio::test(flavor = "multi_thread")]
async fn without_a_terminal_it_asks_for_sim() {
    let env = TestEnv::new().await;
    env.write_config("");
    nothing_is_sent(&env).await;

    env.cmd()
        .args(ENVIAR)
        .write_stdin("s\n")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("Pix a enviar").and(predicate::str::contains("--sim")));
}

#[tokio::test(flavor = "multi_thread")]
async fn sim_sends_once_with_the_idempotency_key() {
    let env = TestEnv::new().await;
    env.write_config("");
    mount_pix(
        &env,
        json!({
            "valor": 150,
            "descricao": "NF 123",
            "destinatario": {"tipo": "CHAVE", "chave": "fornecedor@empresa.example"}
        }),
        resposta("PROCESSADO"),
    )
    .await;
    let assert = env
        .cmd()
        .args(ENVIAR)
        .args([
            "--descricao",
            "NF 123",
            "--sim",
            "--id-idempotente",
            &ID.to_uppercase(),
        ])
        .assert()
        .success();
    let stdout = stdout_of(&assert);
    assert!(stdout.starts_with("Pix enviado.\n"), "{stdout}");
    assert!(stdout.contains(CODIGO), "{stdout}");
    assert!(
        stdout.contains(&format!("Chave de idempotência  {ID}")),
        "{stdout}"
    );
    // The key given is the one sent, in lower case.
    let requests = env.server.received_requests().await.unwrap();
    let pix = requests.iter().find(|r| r.url.path() == PIX).unwrap();
    assert_eq!(pix.headers["x-id-idempotente"], ID);
}

#[tokio::test(flavor = "multi_thread")]
async fn operation_limit_is_enforced_even_with_sim() {
    let env = TestEnv::new().await;
    env.write_config("limite_por_operacao = \"100,00\"");
    nothing_is_sent(&env).await;

    env.cmd()
        .args(["pix", "enviar", "--chave", "fornecedor@empresa.example"])
        .args(["--valor", "100,01", "--sim"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "R$ 100,01 passa do limite por operação do perfil \"padrao\" (R$ 100,00)",
        ));
}

#[tokio::test(flavor = "multi_thread")]
async fn amounts_up_to_the_limit_are_sent() {
    let env = TestEnv::new().await;
    env.write_config("limite_por_operacao = 100");
    mount_pix(
        &env,
        json!({"valor": 100, "destinatario": {"tipo": "CHAVE", "chave": "fornecedor@empresa.example"}}),
        resposta("PROCESSADO"),
    )
    .await;

    env.cmd()
        .args(["pix", "enviar", "--chave", "fornecedor@empresa.example"])
        .args(["--valor", "R$ 100,00", "--sim"])
        .assert()
        .success();
}

#[tokio::test(flavor = "multi_thread")]
async fn scheduled_and_pending_approval_are_explained() {
    let env = TestEnv::new().await;
    env.write_config("");
    env.mount_token("pagamento-pix.write", None).await;
    Mock::given(method("POST"))
        .and(path(PIX))
        .and(body_json(json!({
            "valor": 150,
            "dataPagamento": "2099-10-01",
            "destinatario": {"tipo": "CHAVE", "chave": "fornecedor@empresa.example"}
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "tipoRetorno": "AGENDADO",
            "codigoSolicitacao": CODIGO,
            "dataPagamento": "2099-10-01"
        })))
        .expect(1)
        .mount(&env.server)
        .await;
    env.cmd()
        .args(ENVIAR)
        .args(["--data", "2099-10-01", "--sim"])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("Pix agendado para 01/10/2099."))
        .stderr(predicate::str::contains("agendado para 01/10/2099"));

    Mock::given(method("POST"))
        .and(path(PIX))
        .respond_with(resposta("APROVACAO"))
        .expect(1)
        .mount(&env.server)
        .await;
    env.cmd()
        .args(ENVIAR)
        .arg("--sim")
        .assert()
        .success()
        .stdout(predicate::str::starts_with(
            "Pix aguardando aprovação no Internet Banking",
        ));
}

#[tokio::test(flavor = "multi_thread")]
async fn json_output_includes_the_idempotency_key() {
    let env = TestEnv::new().await;
    env.write_config("");
    mount_pix(
        &env,
        json!({"valor": 150, "destinatario": {"tipo": "CHAVE", "chave": "fornecedor@empresa.example"}}),
        resposta("PROCESSADO"),
    )
    .await;

    let assert = env
        .cmd()
        .args(ENVIAR)
        .args(["--sim", "--json", "--id-idempotente", ID])
        .assert()
        .success();
    let json: Value = serde_json::from_str(&stdout_of(&assert)).unwrap();
    assert_eq!(
        json,
        json!({
            "tipoRetorno": "PROCESSADO",
            "codigoSolicitacao": CODIGO,
            "dataPagamento": "2026-10-01",
            "dataOperacao": "2026-09-23",
            "idIdempotente": ID
        })
    );
}

/// After a `5xx` the payment may have been made: never repeated, and the
/// user learns how to repeat it safely.
#[tokio::test(flavor = "multi_thread")]
async fn uncertain_outcome_shows_how_to_repeat_safely() {
    let env = TestEnv::new().await;
    env.write_config("");
    env.mount_token("pagamento-pix.write", None).await;
    Mock::given(method("POST"))
        .and(path(PIX))
        .respond_with(ResponseTemplate::new(504))
        .expect(1)
        .mount(&env.server)
        .await;

    env.cmd()
        .args(ENVIAR)
        .args(["--sim", "--id-idempotente", ID])
        .assert()
        .code(9)
        .stderr(
            predicate::str::contains("o pagamento pode ter sido feito")
                .and(predicate::str::contains(format!("--id-idempotente {ID}"))),
        );
}

/// A `429` was certainly not processed: exit 6, which a script may repeat,
/// unlike 9.
#[tokio::test(flavor = "multi_thread")]
async fn a_request_not_processed_exits_6_not_9() {
    let env = TestEnv::new().await;
    env.write_config("");
    env.mount_token("pagamento-pix.write", None).await;
    Mock::given(method("POST"))
        .and(path(PIX))
        .respond_with(ResponseTemplate::new(429))
        .expect(1)
        .mount(&env.server)
        .await;

    let assert = env
        .cmd()
        .args(ENVIAR)
        .args(["--sim", "--tentativas", "1"])
        .assert()
        .code(6);
    assert!(
        !stderr_of(&assert).contains("pode ter sido feito"),
        "{}",
        stderr_of(&assert)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn production_is_highlighted_in_the_summary() {
    let env = TestEnv::new().await;
    env.write_config("");
    nothing_is_sent(&env).await;

    env.cmd()
        .args(ENVIAR)
        .args(["--ambiente", "producao", "--simular"])
        .assert()
        .success()
        .stderr(
            predicate::str::starts_with(
                "*** PRODUÇÃO: este Pix movimenta dinheiro da conta real ***",
            )
            .and(predicate::str::contains("PRODUÇÃO (conta real)")),
        );
}

#[tokio::test(flavor = "multi_thread")]
async fn invalid_amounts_and_keys_are_usage_errors() {
    let env = TestEnv::new().await;
    env.write_config("");
    nothing_is_sent(&env).await;

    for (args, mensagem) in [
        (
            ["--chave", "fornecedor@empresa.example", "--valor", "1.500"],
            "ambíguo",
        ),
        (
            ["--chave", "fornecedor@empresa.example", "--valor", "0"],
            "maior que zero",
        ),
        (
            ["--chave", "fornecedor@empresa.example", "--valor", "10,555"],
            "ambíguo",
        ),
        (["--chave", "11912345678", "--valor", "10"], "+55"),
        (["--chave", "fulano@", "--valor", "10"], "e-mail inválido"),
    ] {
        env.cmd()
            .args(["pix", "enviar"])
            .args(args)
            .arg("--sim")
            .assert()
            .code(2)
            .stderr(predicate::str::contains(mensagem));
    }
    env.cmd()
        .args(ENVIAR)
        .args(["--data", "2020-01-01", "--sim"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("já passou"));
    env.cmd()
        .args(ENVIAR)
        .args(["--descricao", &"x".repeat(141), "--sim"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("140"));
    env.cmd()
        .args(ENVIAR)
        .args(["--formato", "csv", "--sim"])
        .assert()
        .code(2);
}

#[tokio::test(flavor = "multi_thread")]
async fn secrets_never_reach_the_output() {
    let env = TestEnv::new().await;
    env.write_config("");
    mount_pix(
        &env,
        json!({"valor": 150, "destinatario": {"tipo": "CHAVE", "chave": "fornecedor@empresa.example"}}),
        resposta("PROCESSADO"),
    )
    .await;

    let assert = env
        .cmd()
        .args(ENVIAR)
        .args(["--sim", "-vv"])
        .assert()
        .success();
    let output = format!("{}{}", stdout_of(&assert), stderr_of(&assert));
    assert!(!output.contains(CLIENT_SECRET), "{output}");
    assert!(!output.contains(TOKEN), "{output}");
}

#[tokio::test(flavor = "multi_thread")]
async fn copia_e_cola_pays_the_amount_of_the_code() {
    let env = TestEnv::new().await;
    env.write_config("");
    let codigo = copia_e_cola(Some("150.00"));
    mount_pix(
        &env,
        json!({"valor": 150, "destinatario": {"tipo": "PIX_COPIA_E_COLA", "pixCopiaECola": codigo}}),
        resposta("PROCESSADO"),
    )
    .await;

    env.cmd()
        .args(["pix", "enviar", "--copia-e-cola", &codigo, "--sim"])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("Pix enviado."))
        .stderr(
            predicate::str::contains("Recebedor              Fornecedor Exemplo (SAO PAULO)")
                .and(predicate::str::contains(
                    "fornecedor@empresa.example (e-mail)",
                ))
                .and(predicate::str::contains("Identificador          NF123"))
                .and(predicate::str::contains(
                    "R$ 150,00 (cento e cinquenta reais)",
                )),
        );
}

#[tokio::test(flavor = "multi_thread")]
async fn copia_e_cola_without_amount_takes_valor() {
    let env = TestEnv::new().await;
    env.write_config("");
    let codigo = copia_e_cola(None);
    mount_pix(
        &env,
        json!({"valor": 9.9, "destinatario": {"tipo": "PIX_COPIA_E_COLA", "pixCopiaECola": codigo}}),
        resposta("PROCESSADO"),
    )
    .await;

    env.cmd()
        .args([
            "pix",
            "enviar",
            "--copia-e-cola",
            &codigo,
            "--valor",
            "9,90",
            "--sim",
        ])
        .assert()
        .success();
}

#[tokio::test(flavor = "multi_thread")]
async fn wrong_amounts_and_corrupted_codes_are_refused() {
    let env = TestEnv::new().await;
    env.write_config("");
    nothing_is_sent(&env).await;
    let com_valor = copia_e_cola(Some("150.00"));
    let sem_valor = copia_e_cola(None);
    let corrompido = com_valor.replacen("150.00", "950.00", 1);

    for (args, mensagem) in [
        (
            vec!["--copia-e-cola", com_valor.as_str(), "--valor", "15"],
            "fixa o valor em R$ 150,00",
        ),
        (
            vec!["--copia-e-cola", sem_valor.as_str()],
            "não traz o valor",
        ),
        (
            vec!["--copia-e-cola", corrompido.as_str()],
            "copie o código novamente",
        ),
        (
            vec![
                "--copia-e-cola",
                com_valor.as_str(),
                "--chave",
                "fornecedor@empresa.example",
            ],
            "--chave",
        ),
    ] {
        env.cmd()
            .args(["pix", "enviar"])
            .args(&args)
            .arg("--sim")
            .assert()
            .code(2)
            .stderr(predicate::str::contains(mensagem));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn bank_details_are_sent_for_receivers_without_a_key() {
    let env = TestEnv::new().await;
    env.write_config("");
    mount_pix(
        &env,
        json!({
            "valor": 10,
            "destinatario": {
                "tipo": "DADOS_BANCARIOS",
                "nome": "Fornecedor Exemplo",
                "cpfCnpj": "12345678000195",
                "instituicaoFinanceira": {"ispb": "00000000"},
                "agencia": "0001",
                "contaCorrente": "12345678",
                "tipoConta": "CONTA_PAGAMENTO"
            }
        }),
        resposta("PROCESSADO"),
    )
    .await;

    env.cmd()
        .args(["pix", "enviar", "--valor", "10", "--sim"])
        .args([
            "--ispb",
            "00000000",
            "--agencia",
            "0001",
            "--conta",
            "1234567-8",
        ])
        .args([
            "--tipo-conta",
            "pagamento",
            "--documento",
            "12.345.678/0001-95",
        ])
        .args(["--nome", "Fornecedor Exemplo"])
        .assert()
        .success()
        .stderr(
            predicate::str::contains("CPF/CNPJ               12.345.678/0001-95").and(
                predicate::str::contains("0001 / 12345678 (conta de pagamento)"),
            ),
        );
}

#[tokio::test(flavor = "multi_thread")]
async fn bank_details_must_be_complete() {
    let env = TestEnv::new().await;
    env.write_config("");
    nothing_is_sent(&env).await;

    env.cmd()
        .args(["pix", "enviar", "--valor", "10", "--sim"])
        .args([
            "--ispb",
            "00000000",
            "--agencia",
            "0001",
            "--conta",
            "1234567",
        ])
        .args([
            "--tipo-conta",
            "corrente",
            "--documento",
            "12.345.678/0001-95",
        ])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("--nome"));
    env.cmd()
        .args(["pix", "enviar", "--valor", "10", "--sim"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("--chave"));
}

// --- pix consultar ------------------------------------------------------------

fn consulta(status: &str) -> Value {
    json!({
        "transacaoPix": {
            "status": status,
            "valor": 150,
            "chave": "fornecedor@empresa.example",
            "codigoSolicitacao": CODIGO,
            "recebedor": {"nome": "Fornecedor Exemplo", "cpfCnpj": "***.456.789-**"}
        },
        "historico": [
            {"status": "CRIADO", "dataHoraEvento": "2026-09-23T12:00:00"},
            {"status": status, "dataHoraEvento": "2026-09-23T12:00:01"}
        ]
    })
}

async fn mount_consulta(env: &TestEnv, status: &str, vezes: Option<u64>) {
    let mock = Mock::given(method("GET"))
        .and(path(format!("{PIX}/{CODIGO}")))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(consulta(status)));
    match vezes {
        Some(vezes) => {
            mock.up_to_n_times(vezes)
                .expect(vezes)
                .mount(&env.server)
                .await;
        }
        None => mock.mount(&env.server).await,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn consultar_shows_status_and_history() {
    let env = TestEnv::new().await;
    env.write_config("");
    env.mount_token("pagamento-pix.read", Some(1)).await;
    mount_consulta(&env, "PAGO", Some(2)).await;

    let assert = env
        .cmd()
        .args(["pix", "consultar", CODIGO])
        .assert()
        .success();
    let stdout = stdout_of(&assert);
    for linha in [
        "Status                 pago",
        "Valor                  R$ 150,00",
        "Recebedor              Fornecedor Exemplo (***.456.789-**)",
        "Histórico\n  23/09/2026 12:00:00  criado\n  23/09/2026 12:00:01  pago",
    ] {
        assert!(stdout.contains(linha), "{linha}\n{stdout}");
    }

    let assert = env
        .cmd()
        .args(["pix", "consultar", CODIGO, "--json"])
        .assert()
        .success();
    let json: Value = serde_json::from_str(&stdout_of(&assert)).unwrap();
    assert_eq!(json, consulta("PAGO"));
}

#[tokio::test(flavor = "multi_thread")]
async fn aguardar_stops_at_a_final_status() {
    let env = TestEnv::new().await;
    env.write_config("");
    env.mount_token("pagamento-pix.read", None).await;
    mount_consulta(&env, "AGUARDANDO_APROVACAO", Some(1)).await;
    mount_consulta(&env, "PAGO", Some(1)).await;

    env.cmd()
        .args(["pix", "consultar", CODIGO, "--aguardar", "--timeout", "3s"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Status                 pago"))
        .stderr(predicate::str::contains(
            "aguardando: aguardando aprovação no Internet Banking",
        ));
}

#[tokio::test(flavor = "multi_thread")]
async fn aguardar_gives_up_after_the_timeout() {
    let env = TestEnv::new().await;
    env.write_config("");
    env.mount_token("pagamento-pix.read", None).await;
    mount_consulta(&env, "AGUARDANDO_APROVACAO", None).await;

    env.cmd()
        .args(["pix", "consultar", CODIGO, "--aguardar", "--timeout", "1s"])
        .assert()
        .code(8)
        .stdout(predicate::str::contains(
            "Status                 aguardando aprovação no Internet Banking",
        ))
        .stderr(
            predicate::str::contains("tempo de espera esgotado (1 s)")
                .and(predicate::str::contains("aumente o --timeout")),
        );
}

#[tokio::test(flavor = "multi_thread")]
async fn aguardar_reports_payments_that_were_not_made() {
    let env = TestEnv::new().await;
    env.write_config("");
    env.mount_token("pagamento-pix.read", None).await;
    mount_consulta(&env, "CANCELADO_SEM_SALDO", None).await;

    env.cmd()
        .args(["pix", "consultar", CODIGO, "--aguardar"])
        .assert()
        .code(5)
        .stderr(predicate::str::contains(
            "o Pix terminou sem ser pago: cancelado por falta de saldo",
        ));

    // Without --aguardar the query itself succeeded.
    env.cmd()
        .args(["pix", "consultar", CODIGO])
        .assert()
        .success();
}

#[tokio::test(flavor = "multi_thread")]
async fn request_codes_are_validated_and_unknown_ones_reported() {
    let env = TestEnv::new().await;
    env.write_config("");
    nothing_is_sent(&env).await;
    env.cmd()
        .args(["pix", "consultar", "../saldo"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("UUID"));

    let env = TestEnv::new().await;
    env.write_config("");
    env.mount_token("pagamento-pix.read", None).await;
    Mock::given(method("GET"))
        .and(path(format!("{PIX}/{CODIGO}")))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({
            "title": "Não Encontrado.",
            "status": "404",
            "detail": "Entidade não encontrada."
        })))
        .expect(1)
        .mount(&env.server)
        .await;
    env.cmd()
        .args(["pix", "consultar", CODIGO])
        .assert()
        .code(5)
        .stderr(predicate::str::contains("Entidade não encontrada"));
}

#[tokio::test(flavor = "multi_thread")]
async fn sending_points_to_the_query() {
    let env = TestEnv::new().await;
    env.write_config("");
    mount_pix(
        &env,
        json!({"valor": 150, "destinatario": {"tipo": "CHAVE", "chave": "fornecedor@empresa.example"}}),
        resposta("APROVACAO"),
    )
    .await;

    env.cmd()
        .args(ENVIAR)
        .arg("--sim")
        .assert()
        .success()
        .stdout(predicate::str::ends_with(format!(
            "Acompanhe com: inter-pj pix consultar {CODIGO} --aguardar\n"
        )));
}
