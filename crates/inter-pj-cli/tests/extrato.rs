//! End-to-end tests of `inter-pj extrato`, `extrato completo` and `extrato
//! pdf`. Every name, document and amount here is synthetic.

mod common;

use std::fs;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use chrono::{Local, NaiveDate};
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
            let hoje = Local::now().date_naive();
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
