//! End-to-end tests of `inter-pj extrato`, `extrato completo` and `extrato
//! pdf`. Every name, document and amount here is synthetic.

mod common;

use std::fs;

use assert_cmd::assert::Assert;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use chrono::NaiveDate;
use common::{TestEnv, stderr_of, stdout_of};
use predicates::prelude::*;
use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param, query_param_is_missing};
use wiremock::{Match, Mock, Request, ResponseTemplate};

const EXTRATO: &str = "/banking/v2/extrato";
const COMPLETO: &str = "/banking/v2/extrato/completo";
const EXPORTAR: &str = "/banking/v2/extrato/exportar";
const AGOSTO: [&str; 4] = ["--inicio", "2026-08-01", "--fim", "2026-08-31"];

async fn env() -> TestEnv {
    let env = TestEnv::new().await;
    env.write_config("");
    env.mount_token("extrato.read", None).await;
    env
}

fn simples() -> Value {
    json!({"transacoes": [
        {"dataEntrada": "2026-08-03", "tipoTransacao": "PIX", "tipoOperacao": "C",
         "valor": "1500.00", "titulo": "Pix recebido", "descricao": "Cliente Exemplo Ltda"},
        {"dataEntrada": "2026-08-05", "tipoTransacao": "PAGAMENTO", "tipoOperacao": "D",
         "valor": "250.10", "titulo": "Pagamento efetuado", "descricao": "Boleto; energia"},
        {"dataEntrada": "2026-08-06", "tipoTransacao": "TARIFA", "tipoOperacao": "D",
         "valor": "2.5", "titulo": "Tarifa", "descricao": "=1+1"}
    ]})
}

async fn mount_extrato(env: &TestEnv, inicio: &str, fim: &str, body: Value, calls: u64) {
    Mock::given(method("GET"))
        .and(path(EXTRATO))
        .and(query_param("dataInicio", inicio))
        .and(query_param("dataFim", fim))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .expect(calls)
        .mount(&env.server)
        .await;
}

fn completa(id: u32, operacao: &str, valor: &str) -> Value {
    json!({
        "idTransacao": id.to_string(),
        "dataInclusao": "2026-08-10 09:00:00",
        "dataTransacao": "2026-08-10",
        "tipoTransacao": "PIX",
        "tipoOperacao": operacao,
        "valor": valor,
        "titulo": if operacao == "C" { "Pix recebido" } else { "Pix enviado" },
        "descricao": format!("Transação {id}"),
        "detalhes": {
            "nomePagador": "Cliente Exemplo", "cpfCnpjPagador": "12345678909",
            "nomeRecebedor": "Fornecedor Exemplo", "cpfCnpjRecebedor": "12345678000195",
            "endToEndId": format!("E0000000020260810{id:04}"), "txId": format!("tx{id}")
        }
    })
}

// --- extrato --------------------------------------------------------------------

/// Descriptions come from whoever sent the Pix: an escape sequence or a line
/// break in them must not reach the terminal, or it could fake a line.
#[tokio::test(flavor = "multi_thread")]
async fn extrato_neutraliza_caracteres_de_controle_de_terceiros() {
    let env = env().await;
    let body = json!({"transacoes": [
        {"dataEntrada": "2026-08-03", "tipoTransacao": "PIX", "tipoOperacao": "C",
         "valor": "1.00", "titulo": "Pix recebido",
         "descricao": "Cliente\u{1b}[2K\n03/08/2026  Pix  Pix recebido  R$ 9.999,00"}
    ]});
    mount_extrato(&env, "2026-08-01", "2026-08-31", body, 1).await;

    let out = stdout_of(&env.cmd().arg("extrato").args(AGOSTO).assert().success());
    assert!(!out.contains('\u{1b}'), "{out}");
    assert!(
        !out.contains("\n03/08/2026  Pix  Pix recebido  R$ 9.999,00"),
        "{out}"
    );
    assert!(
        out.contains("Cliente\u{FFFD}[2K\u{FFFD}03/08/2026"),
        "{out}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn extrato_em_texto_mostra_transacoes_e_totais() {
    let env = env().await;
    mount_extrato(&env, "2026-08-01", "2026-08-31", simples(), 1).await;

    let out = stdout_of(&env.cmd().arg("extrato").args(AGOSTO).assert().success());
    assert_eq!(
        out,
        "\
Extrato de 01/08/2026 a 31/08/2026

Data        Tipo       Descrição                                   Valor
03/08/2026  Pix        Pix recebido · Cliente Exemplo Ltda   R$ 1.500,00
05/08/2026  Pagamento  Pagamento efetuado · Boleto; energia   -R$ 250,10
06/08/2026  Tarifa     Tarifa · =1+1                            -R$ 2,50

Entradas              R$ 1.500,00
Saídas                 -R$ 252,60
Resultado do período  R$ 1.247,40
3 transações
"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn extrato_em_json_preserva_os_campos_da_api() {
    let env = env().await;
    mount_extrato(&env, "2026-08-01", "2026-08-31", simples(), 1).await;

    let out = stdout_of(
        &env.cmd()
            .arg("extrato")
            .args(AGOSTO)
            .arg("--json")
            .assert()
            .success(),
    );
    let json: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(json["transacoes"][1]["valor"], json!(250.1));
    assert_eq!(json["transacoes"][1]["tipoOperacao"], json!("D"));
    assert_eq!(json["transacoes"][0]["dataEntrada"], json!("2026-08-03"));
}

#[tokio::test(flavor = "multi_thread")]
async fn extrato_em_csv_para_scripts_e_para_excel() {
    let env = env().await;
    mount_extrato(&env, "2026-08-01", "2026-08-31", simples(), 2).await;

    let csv = stdout_of(
        &env.cmd()
            .arg("extrato")
            .args(AGOSTO)
            .args(["--formato", "csv"])
            .assert()
            .success(),
    );
    assert_eq!(
        csv,
        "dataEntrada,tipoTransacao,tipoOperacao,titulo,descricao,valor\r\n\
         2026-08-03,PIX,C,Pix recebido,Cliente Exemplo Ltda,1500.00\r\n\
         2026-08-05,PAGAMENTO,D,Pagamento efetuado,Boleto; energia,-250.10\r\n\
         2026-08-06,TARIFA,D,Tarifa,'=1+1,-2.5\r\n"
    );

    let excel = stdout_of(
        &env.cmd()
            .arg("extrato")
            .args(AGOSTO)
            .args(["--formato", "csv", "--separador", ";"])
            .assert()
            .success(),
    );
    assert!(excel.starts_with('\u{feff}'), "BOM para o Excel");
    assert!(
        excel.contains("2026-08-05;PAGAMENTO;D;Pagamento efetuado;\"Boleto; energia\";-250,10\r\n"),
        "{excel}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn extrato_sem_datas_consulta_os_ultimos_30_dias() {
    /// `dataFim` is today and the period has 30 days, today included.
    struct UltimosTrintaDias;
    impl Match for UltimosTrintaDias {
        fn matches(&self, request: &Request) -> bool {
            let data = |name: &str| {
                request
                    .url
                    .query_pairs()
                    .find(|(key, _)| key == name)
                    .and_then(|(_, value)| NaiveDate::parse_from_str(&value, "%Y-%m-%d").ok())
            };
            let (Some(inicio), Some(fim)) = (data("dataInicio"), data("dataFim")) else {
                return false;
            };
            // Tolerates the test running across midnight.
            let hoje = common::hoje();
            (fim - inicio).num_days() == 29 && (hoje - fim).num_days().abs() <= 1
        }
    }
    let env = env().await;
    Mock::given(method("GET"))
        .and(path(EXTRATO))
        .and(UltimosTrintaDias)
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"transacoes": []})))
        .expect(1)
        .mount(&env.server)
        .await;

    env.cmd()
        .arg("extrato")
        .assert()
        .success()
        .stdout(predicate::str::contains("Nenhuma transação no período."));
}

#[tokio::test(flavor = "multi_thread")]
async fn periodo_longo_e_recusado_antes_de_chamar_a_api() {
    let env = TestEnv::new().await;
    env.write_config("");
    env.mount_token("extrato.read", Some(0)).await;

    let assert = env
        .cmd()
        .args(["extrato", "--inicio", "2026-01-01", "--fim", "2026-04-01"])
        .assert()
        .code(2);
    let err = stderr_of(&assert);
    assert!(err.contains("o período tem 91 dias"), "{err}");
    assert!(err.contains("dica: use --dividir-periodo"), "{err}");

    env.cmd()
        .args(["extrato", "--inicio", "2026-02-01", "--fim", "2026-01-31"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("anterior à inicial"));
}

#[tokio::test(flavor = "multi_thread")]
async fn periodo_longo_pode_ser_dividido() {
    let env = env().await;
    let parte = |data: &str, valor: &str| {
        json!({"transacoes": [{"dataEntrada": data, "tipoTransacao": "PIX",
            "tipoOperacao": "C", "valor": valor, "titulo": "Pix recebido"}]})
    };
    mount_extrato(
        &env,
        "2026-01-01",
        "2026-03-31",
        parte("2026-02-10", "100"),
        1,
    )
    .await;
    mount_extrato(
        &env,
        "2026-04-01",
        "2026-04-01",
        parte("2026-04-01", "50"),
        1,
    )
    .await;

    let out = stdout_of(
        &env.cmd()
            .args(["extrato", "--inicio", "2026-01-01", "--fim", "2026-04-01"])
            .arg("--dividir-periodo")
            .assert()
            .success(),
    );
    assert!(
        out.starts_with("Extrato de 01/01/2026 a 01/04/2026"),
        "{out}"
    );
    assert!(
        out.contains("10/02/2026") && out.contains("01/04/2026  Pix"),
        "{out}"
    );
    assert!(out.contains("Resultado do período  R$ 150,00"), "{out}");
}

// --- extrato completo --------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn extrato_completo_mostra_uma_pagina_com_filtros() {
    let env = env().await;
    Mock::given(method("GET"))
        .and(path(COMPLETO))
        .and(query_param("dataInicio", "2026-08-01"))
        .and(query_param("pagina", "1"))
        .and(query_param("tamanhoPagina", "2"))
        .and(query_param("tipoOperacao", "D"))
        .and(query_param("tipoTransacao", "PIX"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "totalPaginas": 3, "totalElementos": 6, "ultimaPagina": false,
            "transacoes": [completa(3, "D", "80.00"), completa(4, "D", "19.90")]
        })))
        .expect(1)
        .mount(&env.server)
        .await;

    let out = stdout_of(
        &env.cmd()
            .args(["extrato", "completo"])
            .args(AGOSTO)
            .args(["--tipo-operacao", "d", "--tipo-transacao", "pix"])
            .args(["--pagina", "1", "--tamanho-pagina", "2"])
            .assert()
            .success(),
    );
    assert!(
        out.contains("Fornecedor Exemplo"),
        "contraparte das saídas: {out}"
    );
    assert!(
        out.contains("-R$ 80,00") && out.contains("-R$ 19,90"),
        "{out}"
    );
    assert!(out.contains("Página 2 de 3 · 2 de 6 transações"), "{out}");
    assert!(
        out.contains("há mais páginas: use --pagina 2 ou --todas-paginas"),
        "{out}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn extrato_completo_le_todas_as_paginas_em_csv() {
    let env = env().await;
    for (pagina, ids, ultima) in [(0, [1, 2], false), (1, [3, 4], false), (2, [5, 6], true)] {
        Mock::given(method("GET"))
            .and(path(COMPLETO))
            .and(query_param("pagina", pagina.to_string()))
            .and(query_param("tamanhoPagina", "10000"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "totalPaginas": 3, "totalElementos": 6, "ultimaPagina": ultima,
                "transacoes": ids.map(|id| completa(id, "C", "10.00"))
            })))
            .expect(1)
            .mount(&env.server)
            .await;
    }

    let csv = stdout_of(
        &env.cmd()
            .args(["extrato", "completo", "--todas-paginas", "--formato", "csv"])
            .args(AGOSTO)
            .assert()
            .success(),
    );
    let linhas: Vec<&str> = csv.split("\r\n").filter(|l| !l.is_empty()).collect();
    assert_eq!(
        linhas[0],
        "idTransacao,dataTransacao,dataInclusao,tipoTransacao,tipoOperacao,titulo,descricao,numeroDocumento,valor,contraparte,documentoContraparte,endToEndId,codigoBarras"
    );
    assert_eq!(linhas.len(), 7, "{csv}");
    assert_eq!(
        linhas[1],
        "1,2026-08-10,2026-08-10,PIX,C,Pix recebido,Transação 1,,10.00,Cliente Exemplo,12345678909,E00000000202608100001,"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn extrato_completo_usa_scroll_acima_de_dez_mil_transacoes() {
    let env = env().await;
    Mock::given(method("GET"))
        .and(path(COMPLETO))
        .and(query_param("pagina", "0"))
        .and(query_param_is_missing("scrollEnabled"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "totalElementos": 12_000, "totalPaginas": 2, "ultimaPagina": false,
            "transacoes": [completa(999, "C", "1")]
        })))
        .expect(1)
        .mount(&env.server)
        .await;
    Mock::given(method("GET"))
        .and(path(COMPLETO))
        .and(query_param("scrollEnabled", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "scrollId": "4a1b2c3d-0000-4000-8000-000000000001", "hasMore": true,
            "transacoes": [completa(1, "C", "1"), completa(2, "D", "2")]
        })))
        .expect(1)
        .mount(&env.server)
        .await;
    Mock::given(method("GET"))
        .and(path(COMPLETO))
        .and(query_param(
            "scrollId",
            "4a1b2c3d-0000-4000-8000-000000000001",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "hasMore": false, "transacoes": [completa(3, "C", "3")]
        })))
        .expect(1)
        .mount(&env.server)
        .await;

    let out = stdout_of(
        &env.cmd()
            .args(["extrato", "completo", "--todas-paginas", "--json"])
            .args(AGOSTO)
            .assert()
            .success(),
    );
    let json: Value = serde_json::from_str(&out).unwrap();
    let ids: Vec<&str> = json["transacoes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["idTransacao"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["1", "2", "3"]);
    assert_eq!(json["transacoes"][0]["detalhes"]["txId"], json!("tx1"));
}

#[tokio::test(flavor = "multi_thread")]
async fn scroll_ja_ativo_tem_dica() {
    let env = env().await;
    Mock::given(method("GET"))
        .and(path(COMPLETO))
        .and(query_param("pagina", "0"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"totalElementos": 50_000, "transacoes": []})),
        )
        .mount(&env.server)
        .await;
    Mock::given(method("GET"))
        .and(path(COMPLETO))
        .and(query_param("scrollEnabled", "true"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "title": "Já existe um scroll ativo para esta conta corrente.",
            "detail": "Aguarde o scroll atual expirar ou finalize-o antes de iniciar um novo.",
            "typeError": "SCROLL_ALREADY_ACTIVE"
        })))
        .expect(1)
        .mount(&env.server)
        .await;

    env.cmd()
        .args(["extrato", "completo", "--todas-paginas"])
        .args(AGOSTO)
        .assert()
        .code(5)
        .stderr(predicate::str::contains("Já existe um scroll ativo"))
        .stderr(predicate::str::contains(
            "dica: já existe uma leitura do extrato",
        ));
}

// --- extrato pdf -------------------------------------------------------------------

fn documento() -> Vec<u8> {
    b"%PDF-1.4\n% extrato sintetico de teste\n%%EOF\n".to_vec()
}

async fn mount_pdf(env: &TestEnv, calls: u64) {
    Mock::given(method("GET"))
        .and(path(EXPORTAR))
        .and(query_param("dataInicio", "2026-08-01"))
        .and(query_param("dataFim", "2026-08-31"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"pdf": BASE64.encode(documento())})),
        )
        .expect(calls)
        .mount(&env.server)
        .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn extrato_pdf_grava_um_arquivo_privado() {
    let env = env().await;
    mount_pdf(&env, 1).await;
    let arquivo = env.path("agosto.pdf");

    env.cmd()
        .args(["extrato", "pdf"])
        .args(AGOSTO)
        .arg("--saida")
        .arg(&arquivo)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Extrato de 01/08/2026 a 31/08/2026 salvo em",
        ));
    assert_eq!(fs::read(&arquivo).unwrap(), documento());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&arquivo).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "o extrato é um dado sensível");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn extrato_pdf_nao_sobrescreve_sem_permissao() {
    let env = env().await;
    // The existing file is detected before calling the API.
    mount_pdf(&env, 1).await;
    let arquivo = env.path("agosto.pdf");
    fs::write(&arquivo, "versão anterior").unwrap();

    env.cmd()
        .args(["extrato", "pdf"])
        .args(AGOSTO)
        .arg("-o")
        .arg(&arquivo)
        .assert()
        .code(2)
        .stderr(predicate::str::contains("já existe; use --sobrescrever"));
    assert_eq!(fs::read_to_string(&arquivo).unwrap(), "versão anterior");

    env.cmd()
        .args(["extrato", "pdf", "--sobrescrever", "--json"])
        .args(AGOSTO)
        .arg("-o")
        .arg(&arquivo)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"bytes\": 44"));
    assert_eq!(fs::read(&arquivo).unwrap(), documento());
}

#[tokio::test(flavor = "multi_thread")]
async fn extrato_pdf_pode_ir_para_a_saida_padrao() {
    let env = env().await;
    mount_pdf(&env, 1).await;

    let assert = env
        .cmd()
        .args(["extrato", "pdf", "--saida", "-"])
        .args(AGOSTO)
        .assert()
        .success();
    assert_eq!(assert.get_output().stdout, documento());
}

#[tokio::test(flavor = "multi_thread")]
async fn extrato_pdf_aceita_no_maximo_90_dias() {
    let env = TestEnv::new().await;
    env.write_config("");
    env.cmd()
        .args([
            "extrato",
            "pdf",
            "--inicio",
            "2026-01-01",
            "--fim",
            "2026-06-30",
        ])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "dica: gere um PDF para cada período",
        ));
}

// --- extrato --formato ofx -----------------------------------------------------------

const SALDO: &str = "/banking/v2/saldo";
const CONTA: &str = "numero_conta = \"7654321\"\ndigito_conta = \"0\"";

async fn env_ofx() -> TestEnv {
    let env = TestEnv::new().await;
    env.write_config(CONTA);
    env.mount_token("extrato.read", None).await;
    env
}

/// The balance at the end of `data`, which goes into the OFX.
async fn mount_saldo_em(env: &TestEnv, data: &str, saldo: Value, calls: u64) {
    Mock::given(method("GET"))
        .and(path(SALDO))
        .and(query_param("dataSaldo", data))
        .respond_with(ResponseTemplate::new(200).set_body_json(saldo))
        .expect(calls)
        .mount(&env.server)
        .await;
}

/// The OFX as text, from Windows-1252 (Latin-1 but for 0x80 to 0x9F, which
/// stay as they are), with LF line breaks and without the moment it was
/// made, which changes on every run.
fn ofx(bytes: &[u8]) -> String {
    let texto: String = bytes.iter().map(|&byte| char::from(byte)).collect();
    assert!(
        !texto.replace("\r\n", "").contains(['\r', '\n']),
        "linhas terminadas em CRLF"
    );
    texto
        .split("\r\n")
        .filter(|linha| !linha.starts_with("<DTSERVER>"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn ofx_da_saida(assert: &Assert) -> String {
    ofx(&assert.get_output().stdout)
}

fn transacoes_ofx(ofx: &str) -> Vec<&str> {
    ofx.split("<STMTTRN>\n")
        .skip(1)
        .map(|transacao| transacao.split("</STMTTRN>").next().unwrap())
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn extrato_em_ofx_para_programas_de_contabilidade() {
    let env = env_ofx().await;
    let body = json!({"transacoes": [
        {"dataEntrada": "2026-08-03", "tipoTransacao": "PIX", "tipoOperacao": "C",
         "valor": "1500.00", "titulo": "Pix recebido", "descricao": "Cliente Exemplo Ltda"},
        {"dataEntrada": "2026-08-05", "tipoTransacao": "PAGAMENTO", "tipoOperacao": "D",
         "valor": "250.1", "titulo": "Pagamento efetuado",
         "descricao": "Energia & Luz <Ltda> – ação\n03/08/2026  R$ 9.999,00"},
        {"dataEntrada": "2026-08-06", "tipoTransacao": "TARIFA", "tipoOperacao": "D",
         "valor": "2.5", "titulo": "Tarifa", "descricao": "Tarifa"},
        {"dataEntrada": "2026-08-06", "tipoTransacao": "TARIFA", "tipoOperacao": "D",
         "valor": "2.5", "titulo": "Tarifa", "descricao": "Tarifa"}
    ]});
    mount_extrato(&env, "2026-08-01", "2026-08-31", body, 2).await;
    mount_saldo_em(&env, "2026-08-31", json!({"disponivel": 3247.3}), 2).await;

    let assert = env
        .cmd()
        .arg("extrato")
        .args(AGOSTO)
        .args(["--formato", "ofx"])
        .assert()
        .success();
    let bytes = assert.get_output().stdout.clone();
    assert_eq!(stderr_of(&assert), "");
    // Windows-1252: "–" is 0x96, "ç" 0xE7, "ã" 0xE3, "·" 0xB7.
    let esperado = b"\x96 a\xe7\xe3o ";
    assert!(
        bytes
            .windows(esperado.len())
            .any(|janela| janela == esperado),
        "Windows-1252"
    );
    let ofx = ofx(&bytes);
    assert!(
        ofx.starts_with(
            "OFXHEADER:100\nDATA:OFXSGML\nVERSION:102\nSECURITY:NONE\nENCODING:USASCII\nCHARSET:1252\nCOMPRESSION:NONE\nOLDFILEUID:NONE\nNEWFILEUID:NONE\n\n<OFX>\n"
        ),
        "{ofx}"
    );
    assert!(
        ofx.contains(
            "<BANKACCTFROM>\n<BANKID>077\n<BRANCHID>0001\n<ACCTID>7654321-0\n<ACCTTYPE>CHECKING\n</BANKACCTFROM>\n<BANKTRANLIST>\n<DTSTART>20260801000000[-3:BRT]\n<DTEND>20260831235959[-3:BRT]\n"
        ),
        "{ofx}"
    );
    assert!(
        ofx.contains("<LEDGERBAL>\n<BALAMT>3247.30\n<DTASOF>20260831235959[-3:BRT]\n</LEDGERBAL>"),
        "o saldo do fim do período: {ofx}"
    );
    assert!(ofx.ends_with("</OFX>\n"), "{ofx}");

    let transacoes = transacoes_ofx(&ofx);
    assert_eq!(transacoes.len(), 4, "{ofx}");
    assert!(
        transacoes[0].starts_with(
            "<TRNTYPE>XFER\n<DTPOSTED>20260803120000[-3:BRT]\n<TRNAMT>1500.00\n<FITID>"
        ),
        "{}",
        transacoes[0]
    );
    assert!(
        transacoes[0].ends_with("<MEMO>Pix recebido \u{b7} Cliente Exemplo Ltda\n"),
        "{}",
        transacoes[0]
    );
    assert!(
        transacoes[1]
            .contains("<TRNTYPE>PAYMENT\n<DTPOSTED>20260805120000[-3:BRT]\n<TRNAMT>-250.10\n"),
        "{}",
        transacoes[1]
    );
    // The special characters are escaped, and a line break in a text of a
    // third party does not make a line of its own.
    assert!(
        transacoes[1].ends_with(
            "<MEMO>Pagamento efetuado \u{b7} Energia &amp; Luz &lt;Ltda&gt; \u{96} a\u{e7}\u{e3}o 03/08/2026 R$ 9.999,00\n"
        ),
        "{}",
        transacoes[1]
    );
    assert!(
        transacoes[2].contains("<TRNTYPE>FEE\n") && transacoes[2].contains("<TRNAMT>-2.50\n"),
        "{}",
        transacoes[2]
    );

    // A FITID for each transaction, even for two equal ones on the same day,
    // and the same in the next run, for the importer not to repeat them.
    let fitid = |transacao: &str| {
        transacao
            .lines()
            .find_map(|linha| linha.strip_prefix("<FITID>"))
            .unwrap()
            .to_owned()
    };
    let fitids: Vec<String> = transacoes.iter().map(|t| fitid(t)).collect();
    let mut unicos = fitids.clone();
    unicos.sort();
    unicos.dedup();
    assert_eq!(unicos.len(), 4, "{fitids:?}");

    let de_novo = env
        .cmd()
        .arg("extrato")
        .args(AGOSTO)
        .args(["--formato", "ofx"])
        .assert()
        .success();
    assert_eq!(ofx_da_saida(&de_novo), ofx);
}

#[tokio::test(flavor = "multi_thread")]
async fn extrato_ofx_vai_para_um_arquivo_privado() {
    let env = env_ofx().await;
    // The existing file is detected before calling the API.
    mount_extrato(&env, "2026-08-01", "2026-08-31", simples(), 2).await;
    mount_saldo_em(&env, "2026-08-31", json!({"disponivel": 0}), 2).await;
    let arquivo = env.path("agosto.ofx");

    let out = stdout_of(
        &env.cmd()
            .arg("extrato")
            .args(AGOSTO)
            .args(["--formato", "ofx", "-o"])
            .arg(&arquivo)
            .assert()
            .success()
            .stderr(predicate::str::contains("aviso: ambiente sandbox")),
    );
    assert!(
        out.starts_with("Extrato de 01/08/2026 a 31/08/2026 salvo em ")
            && out.contains("agosto.ofx (3 transações, "),
        "{out}"
    );
    let gravado = ofx(&fs::read(&arquivo).unwrap());
    assert_eq!(transacoes_ofx(&gravado).len(), 3, "{gravado}");
    assert!(gravado.contains("<BALAMT>0.00\n"), "{gravado}");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&arquivo).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "o extrato é um dado sensível");
    }

    env.cmd()
        .arg("extrato")
        .args(AGOSTO)
        .args(["--formato", "ofx", "--saida"])
        .arg(&arquivo)
        .assert()
        .code(2)
        .stderr(predicate::str::contains("já existe; use --sobrescrever"));

    env.cmd()
        .arg("extrato")
        .args(AGOSTO)
        .args(["--formato", "ofx", "--sobrescrever", "--saida"])
        .arg(&arquivo)
        .assert()
        .success();
}

#[tokio::test(flavor = "multi_thread")]
async fn extrato_ofx_de_um_periodo_dividido() {
    let env = env_ofx().await;
    let parte = |data: &str, valor: &str| {
        json!({"transacoes": [{"dataEntrada": data, "tipoTransacao": "PIX",
            "tipoOperacao": "C", "valor": valor, "titulo": "Pix recebido"}]})
    };
    mount_extrato(
        &env,
        "2026-01-01",
        "2026-03-31",
        parte("2026-02-10", "100"),
        1,
    )
    .await;
    mount_extrato(
        &env,
        "2026-04-01",
        "2026-04-01",
        parte("2026-04-01", "50"),
        1,
    )
    .await;
    mount_saldo_em(&env, "2026-04-01", json!({"disponivel": "150.00"}), 1).await;

    let ofx = ofx_da_saida(
        &env.cmd()
            .args(["extrato", "--inicio", "2026-01-01", "--fim", "2026-04-01"])
            .args(["--dividir-periodo", "--formato", "ofx", "--saida", "-"])
            .assert()
            .success(),
    );
    assert!(
        ofx.contains("<DTSTART>20260101000000[-3:BRT]\n<DTEND>20260401235959[-3:BRT]\n"),
        "{ofx}"
    );
    let transacoes = transacoes_ofx(&ofx);
    assert_eq!(transacoes.len(), 2, "{ofx}");
    assert!(transacoes[0].contains("<DTPOSTED>20260210120000[-3:BRT]\n<TRNAMT>100.00\n"));
    assert!(transacoes[1].contains("<DTPOSTED>20260401120000[-3:BRT]\n<TRNAMT>50.00\n"));
    assert!(ofx.contains("<BALAMT>150.00\n"), "{ofx}");
}

#[tokio::test(flavor = "multi_thread")]
async fn extrato_completo_em_ofx_usa_o_id_e_a_contraparte() {
    let env = env_ofx().await;
    Mock::given(method("GET"))
        .and(path(COMPLETO))
        .and(query_param("pagina", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "totalPaginas": 1, "totalElementos": 2, "ultimaPagina": true,
            "transacoes": [completa(7, "C", "10.00"), completa(8, "D", "3.20")]
        })))
        .expect(1)
        .mount(&env.server)
        .await;
    mount_saldo_em(&env, "2026-08-31", json!({"disponivel": 6.8}), 1).await;

    let ofx = ofx_da_saida(
        &env.cmd()
            .args(["extrato", "completo", "--todas-paginas", "--formato", "ofx"])
            .args(AGOSTO)
            .assert()
            .success(),
    );
    let transacoes = transacoes_ofx(&ofx);
    assert_eq!(
        transacoes,
        [
            "<TRNTYPE>XFER\n<DTPOSTED>20260810120000[-3:BRT]\n<TRNAMT>10.00\n<FITID>7\n<NAME>Cliente Exemplo\n<MEMO>Pix recebido \u{b7} Transação 7\n",
            "<TRNTYPE>XFER\n<DTPOSTED>20260810120000[-3:BRT]\n<TRNAMT>-3.20\n<FITID>8\n<NAME>Fornecedor Exemplo\n<MEMO>Pix enviado \u{b7} Transação 8\n",
        ],
        "{ofx}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn extrato_completo_em_ofx_exige_todas_as_paginas() {
    let env = TestEnv::new().await;
    env.write_config(CONTA);
    env.mount_token("extrato.read", Some(0)).await;

    env.cmd()
        .args(["extrato", "completo", "--formato", "ofx"])
        .args(AGOSTO)
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "o OFX traz o extrato inteiro do período: use --todas-paginas",
        ));
}

#[tokio::test(flavor = "multi_thread")]
async fn ofx_precisa_da_conta_do_perfil() {
    let env = TestEnv::new().await;
    env.write_config("");
    env.mount_token("extrato.read", Some(0)).await;

    env.cmd()
        .arg("extrato")
        .args(AGOSTO)
        .args(["--formato", "ofx"])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("numero_conta e digito_conta"));
}

#[tokio::test(flavor = "multi_thread")]
async fn ofx_sem_o_saldo_do_fim_do_periodo_e_erro() {
    let env = env_ofx().await;
    mount_extrato(&env, "2026-08-01", "2026-08-31", simples(), 1).await;
    mount_saldo_em(&env, "2026-08-31", json!({}), 1).await;

    let assert = env
        .cmd()
        .arg("extrato")
        .args(AGOSTO)
        .args(["--formato", "ofx"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "a API não informou o saldo de 31/08/2026, que o OFX exige",
        ));
    assert_eq!(stdout_of(&assert), "");
}

#[tokio::test(flavor = "multi_thread")]
async fn ofx_vale_apenas_para_o_extrato() {
    let env = TestEnv::new().await;
    env.write_config(CONTA);
    env.mount_token("extrato.read", Some(0)).await;
    for args in [
        &["saldo"][..],
        &["conta"],
        &["extrato", "pdf"],
        &["pix", "recebidos", "listar"],
    ] {
        env.cmd()
            .args(args)
            .args(["--formato", "ofx"])
            .assert()
            .code(2)
            .stderr(predicate::str::contains(
                "o formato ofx vale apenas para o extrato",
            ));
    }
    for args in [
        &["extrato"][..],
        &["extrato", "completo", "--todas-paginas"],
    ] {
        env.cmd()
            .args(args)
            .args(["--saida", "extrato.ofx"])
            .assert()
            .code(2)
            .stderr(predicate::str::contains(
                "--saida vale apenas com --formato ofx",
            ));
    }
    // `--json` wins over `--formato`, as everywhere.
    env.cmd()
        .args(["extrato", "--formato", "ofx", "--json", "-o", "extrato.ofx"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "--saida vale apenas com --formato ofx",
        ));
}

// --- formatos e retentativas ----------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn csv_vale_apenas_para_listagens() {
    let env = TestEnv::new().await;
    env.write_config("");
    for args in [
        &["config", "mostrar"][..],
        &["auth", "token", "--escopo", "extrato.read"],
        &["extrato", "pdf"],
    ] {
        env.cmd()
            .args(args)
            .args(["--formato", "csv"])
            .assert()
            .code(2)
            .stderr(predicate::str::contains(
                "o formato csv vale apenas para listagens",
            ));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn falhas_temporarias_sao_repetidas() {
    let env = env().await;
    Mock::given(method("GET"))
        .and(path(EXTRATO))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .expect(1)
        .mount(&env.server)
        .await;
    mount_extrato(&env, "2026-08-01", "2026-08-31", simples(), 1).await;

    let assert = env
        .cmd()
        .args(["extrato", "-v", "--tentativas", "2"])
        .args(AGOSTO)
        .assert()
        .success();
    let err = stderr_of(&assert);
    assert!(err.contains("resposta 503; tentativa 2 de 2"), "{err}");
}
