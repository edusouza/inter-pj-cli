//! `inter-pj pix lote-cobv ...` end to end, against a mock API. All data is
//! synthetic; due dates are far in the future, so that none has passed.

mod common;

use common::{TestEnv, stderr_of, stdout_of};
use serde_json::{Value, json};
use wiremock::matchers::{any, body_json, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

const CSV: &str = "txid;calendario.dataDeVencimento;devedor.cnpj;devedor.cpf;devedor.nome;valor.original;valor.multa.modalidade;valor.multa.valorPerc;chave\r
mensalidade209910cliente0001;2099-10-20;12.345.678/0001-95;;Cliente Exemplo Ltda;150,00;2;2,00;pix@empresa.example\r
mensalidade209910cliente0002;2099-10-25;;123.456.789-09;Fulano de Tal;89,90;;;pix@empresa.example\r
";

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

fn corpo(descricao: &str) -> Value {
    json!({
        "descricao": descricao,
        "cobsv": [
            {
                "txid": "mensalidade209910cliente0001",
                "calendario": {"dataDeVencimento": "2099-10-20"},
                "devedor": {"cnpj": "12345678000195", "nome": "Cliente Exemplo Ltda"},
                "valor": {"original": "150.00", "multa": {"modalidade": 2, "valorPerc": "2.00"}},
                "chave": "pix@empresa.example"
            },
            {
                "txid": "mensalidade209910cliente0002",
                "calendario": {"dataDeVencimento": "2099-10-25"},
                "devedor": {"cpf": "12345678909", "nome": "Fulano de Tal"},
                "valor": {"original": "89.90"},
                "chave": "pix@empresa.example"
            }
        ]
    })
}

fn negada() -> Value {
    json!({
        "txid": "mensalidade209910cliente0002",
        "status": "NEGADA",
        "problema": {
            "type": "https://pix.bcb.gov.br/api/v2/error/CobVOperacaoInvalida",
            "title": "Cobrança inválida.",
            "status": 400,
            "violacoes": [{"razao": "O campo cobv.devedor.nome não respeita o schema.", "propriedade": "cobv.devedor.nome"}]
        }
    })
}

fn lote(cobsv: &Value) -> Value {
    json!({
        "id": 42,
        "descricao": "Mensalidades de outubro",
        "criacao": "2026-09-24T13:10:00.358Z",
        "cobsv": cobsv
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn cria_um_lote_a_partir_de_uma_planilha() {
    let env = env().await;
    env.mount_token("lotecobv.write", Some(1)).await;
    Mock::given(method("PUT"))
        .and(path("/pix/v2/lotecobv/42"))
        .and(body_json(corpo("Mensalidades de outubro")))
        .respond_with(ResponseTemplate::new(202))
        .expect(1)
        .mount(&env.server)
        .await;
    let caminho = env.path("lote.csv");
    std::fs::write(&caminho, CSV).unwrap();
    let assert = env
        .cmd()
        .args(["pix", "lote-cobv", "criar", "42", "--arquivo"])
        .arg(&caminho)
        .args(["--descricao", "Mensalidades de outubro", "--sim"])
        .assert()
        .success();
    assert_eq!(
        stdout_of(&assert),
        "Lote 42 recebido: as 2 cobranças são criadas em instantes.\n\nAcompanhe com: inter-pj pix lote-cobv consultar 42 --aguardar\n"
    );
    let stderr = stderr_of(&assert);
    for linha in [
        "Lote de cobranças com vencimento a criar",
        "  Cobranças    2\n",
        "  Valor total  R$ 239,90\n",
        "  Vencimentos  de 20/10/2099 a 25/10/2099\n",
        // The table of the charges, within the summary.
        "\n  mensalidade209910cliente0002  25/10/2099   R$ 89,90  Fulano de Tal",
    ] {
        assert!(stderr.contains(linha), "{linha}\n{stderr}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn cria_a_partir_de_um_json_com_a_descricao_do_arquivo() {
    let env = env().await;
    env.mount_token("lotecobv.write", Some(1)).await;
    Mock::given(method("PUT"))
        .and(path("/pix/v2/lotecobv/43"))
        .and(body_json(corpo("Do arquivo")))
        .respond_with(ResponseTemplate::new(202))
        .expect(1)
        .mount(&env.server)
        .await;
    let mut arquivo = corpo("Do arquivo");
    // As people write them: documents with punctuation, amounts as text.
    arquivo["cobsv"][0]["devedor"]["cnpj"] = json!("12.345.678/0001-95");
    arquivo["cobsv"][1]["valor"]["original"] = json!("89,90");
    let caminho = env.path("lote.json");
    std::fs::write(&caminho, arquivo.to_string()).unwrap();
    let assert = env
        .cmd()
        .args([
            "pix",
            "lote-cobv",
            "criar",
            "43",
            "--sim",
            "--json",
            "--arquivo",
        ])
        .arg(&caminho)
        .assert()
        .success();
    let json: Value = serde_json::from_str(&stdout_of(&assert)).unwrap();
    assert_eq!(json, json!({"id": 43}));
}

/// #38's acceptance criterion: a file with an invalid charge is refused
/// whole, with the line and the field of each problem.
#[tokio::test(flavor = "multi_thread")]
async fn arquivo_com_cobranca_invalida_e_recusado_inteiro() {
    let env = env().await;
    nothing_is_sent(&env).await;
    let invalido = format!(
        "{CSV}mensalidade209910cliente0003;2020-01-10;;123.456.789-09;Beltrano;10,00;;;pix@empresa.example\r
mensalidade209910cliente0001;2099-10-20;;123.456.789-09;Repetido;10,00;3;2,00;pix@empresa.example\r
"
    );
    let caminho = env.path("lote.csv");
    std::fs::write(&caminho, invalido).unwrap();
    let assert = env
        .cmd()
        .args([
            "pix",
            "lote-cobv",
            "criar",
            "42",
            "--descricao",
            "x",
            "--sim",
            "--arquivo",
        ])
        .arg(&caminho)
        .assert()
        .code(2);
    let stderr = stderr_of(&assert);
    for linha in [
        "lote.csv: 2 cobranças com problema; nada foi enviado:",
        "  linha 4, campo \"calendario.dataDeVencimento\": o vencimento (10/01/2020) já passou",
        "  linha 5, campo \"valor.multa.modalidade\": 3 não é uma das modalidades: 1, 2",
    ] {
        assert!(stderr.contains(linha), "{linha}\n{stderr}");
    }
    // A CSV has no description: it must come with --descricao.
    std::fs::write(&caminho, CSV).unwrap();
    let assert = env
        .cmd()
        .args(["pix", "lote-cobv", "criar", "42", "--sim", "--arquivo"])
        .arg(&caminho)
        .assert()
        .code(2);
    assert!(
        stderr_of(&assert).contains("informe a descrição do lote com --descricao"),
        "{}",
        stderr_of(&assert)
    );
    // Without a terminal and without --sim, nothing is sent, and the file
    // on the standard input cannot be the answer too.
    for extra in [&["--arquivo"][..], &["--arquivo", "-"][..]] {
        let mut comando = env.cmd();
        comando
            .args(["pix", "lote-cobv", "criar", "42", "--descricao", "x"])
            .args(extra);
        if extra.len() == 1 {
            comando.arg(&caminho).write_stdin("s\n");
        } else {
            comando.write_stdin(CSV);
        }
        let assert = comando.assert().code(2);
        assert!(
            stderr_of(&assert).contains("use --sim"),
            "{}",
            stderr_of(&assert)
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn simulacao_e_modelos() {
    let env = env().await;
    nothing_is_sent(&env).await;
    let assert = env
        .cmd()
        .args([
            "pix",
            "lote-cobv",
            "criar",
            "42",
            "--descricao",
            "Mensalidades de outubro",
            "--arquivo",
            "-",
            "--simular",
            "--json",
        ])
        .write_stdin(CSV)
        .assert()
        .success();
    let json: Value = serde_json::from_str(&stdout_of(&assert)).unwrap();
    assert_eq!(json["metodo"], "PUT");
    assert_eq!(
        json["url"],
        format!("{}/pix/v2/lotecobv/42", env.server.uri())
    );
    assert_eq!(json["corpo"], corpo("Mensalidades de outubro"));
    // Both templates are valid batches.
    for tipo in ["json", "csv"] {
        let modelo = stdout_of(
            &env.cmd()
                .args(["pix", "lote-cobv", "modelo", tipo])
                .assert()
                .success(),
        );
        let assert = env
            .cmd()
            .args([
                "pix",
                "lote-cobv",
                "criar",
                "7",
                "--descricao",
                "Modelo",
                "--arquivo",
                "-",
                "--simular",
                "--json",
            ])
            .write_stdin(modelo)
            .assert()
            .success();
        let json: Value = serde_json::from_str(&stdout_of(&assert)).unwrap();
        assert_eq!(
            json["corpo"]["cobsv"].as_array().unwrap().len(),
            2,
            "{tipo}"
        );
    }
}

/// With an unknown outcome, the error says how to check before repeating.
#[tokio::test(flavor = "multi_thread")]
async fn resultado_incerto_orienta_a_conferir() {
    let env = env().await;
    env.mount_token("lotecobv.write", Some(1)).await;
    Mock::given(method("PUT"))
        .and(path("/pix/v2/lotecobv/42"))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&env.server)
        .await;
    let assert = env
        .cmd()
        .args([
            "pix",
            "lote-cobv",
            "criar",
            "42",
            "--descricao",
            "x",
            "--arquivo",
            "-",
            "--sim",
        ])
        .write_stdin(CSV)
        .assert()
        .code(9);
    let stderr = stderr_of(&assert);
    assert!(
        stderr.contains(
            "dica: o lote pode ter sido recebido: confira com inter-pj pix lote-cobv consultar 42 antes de repetir"
        ),
        "{stderr}"
    );
    assert!(
        stderr.contains("com o mesmo txid, a API não cria outra"),
        "{stderr}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn revisa_cobrancas_do_lote() {
    let env = env().await;
    env.mount_token("lotecobv.write", Some(1)).await;
    Mock::given(method("PATCH"))
        .and(path("/pix/v2/lotecobv/42"))
        .and(body_json(json!({
            "cobsv": [
                {"txid": "mensalidade209910cliente0001", "calendario": {"dataDeVencimento": "2099-10-30"}, "valor": {"original": "160.00"}},
                {"txid": "mensalidade209910cliente0002", "status": "REMOVIDA_PELO_USUARIO_RECEBEDOR"}
            ]
        })))
        .respond_with(ResponseTemplate::new(202))
        .expect(1)
        .mount(&env.server)
        .await;
    let assert = env
        .cmd()
        .args(["pix", "lote-cobv", "revisar", "42", "--arquivo", "-", "--sim"])
        .write_stdin(
            "txid;calendario.dataDeVencimento;valor.original;status\r\nmensalidade209910cliente0001;2099-10-30;160,00;\r\nmensalidade209910cliente0002;;;REMOVIDA_PELO_USUARIO_RECEBEDOR\r\n",
        )
        .assert()
        .success();
    assert!(
        stdout_of(&assert)
            .starts_with("Lote 42 recebido: as alterações de 2 cobranças são feitas em instantes."),
        "{}",
        stdout_of(&assert)
    );
    let stderr = stderr_of(&assert);
    for linha in [
        "Lote de cobranças com vencimento a alterar",
        "  Removidas  1\n",
        "mensalidade209910cliente0001  valor R$ 160,00, vencimento 30/10/2099",
        "mensalidade209910cliente0002  remover (deixa de poder ser paga)",
    ] {
        assert!(stderr.contains(linha), "{linha}\n{stderr}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn consulta_e_aguarda_o_lote() {
    let env = env().await;
    env.mount_token("lotecobv.read", None).await;
    Mock::given(method("GET"))
        .and(path("/pix/v2/lotecobv/42"))
        .respond_with(ResponseTemplate::new(200).set_body_json(lote(&json!([
            {"txid": "mensalidade209910cliente0001", "status": "CRIADA", "criacao": "2026-09-24T13:10:03.000Z"},
            negada()
        ]))))
        .up_to_n_times(2)
        .expect(2)
        .mount(&env.server)
        .await;
    let texto = stdout_of(
        &env.cmd()
            .args(["pix", "lote-cobv", "consultar", "42"])
            .assert()
            .success(),
    );
    assert!(
        texto.starts_with("Lote 42: Mensalidades de outubro\n"),
        "{texto}"
    );
    assert!(
        texto.contains("  Cobranças  2 · 1 criada · 1 negada\n"),
        "{texto}"
    );
    assert!(
        texto.contains("Problemas\n  mensalidade209910cliente0002  Cobrança inválida. cobv.devedor.nome: O campo cobv.devedor.nome não respeita o schema."),
        "{texto}"
    );
    let assert = env
        .cmd()
        .args(["pix", "lote-cobv", "consultar", "42", "--aguardar"])
        .assert()
        .code(5);
    assert!(
        stderr_of(&assert).contains("o lote foi processado com erro: 1 de 2 cobranças negadas"),
        "{}",
        stderr_of(&assert)
    );
    Mock::given(method("GET"))
        .and(path("/pix/v2/lotecobv/42"))
        .respond_with(ResponseTemplate::new(200).set_body_json(lote(&json!([
            {"txid": "mensalidade209910cliente0001", "status": "EM_PROCESSAMENTO"}
        ]))))
        .mount(&env.server)
        .await;
    let assert = env
        .cmd()
        .args([
            "pix",
            "lote-cobv",
            "consultar",
            "42",
            "--aguardar",
            "--timeout",
            "1s",
        ])
        .assert()
        .code(8);
    assert!(
        stderr_of(&assert).contains("o lote ainda está com 1 em processamento"),
        "{}",
        stderr_of(&assert)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn lista_sumario_e_situacao() {
    let env = env().await;
    env.mount_token("lotecobv.read", None).await;
    Mock::given(method("GET"))
        .and(path("/pix/v2/lotecobv"))
        .and(query_param("inicio", "2026-09-01T00:00:00-03:00"))
        .and(query_param("fim", "2026-09-30T23:59:59-03:00"))
        .and(query_param("paginacao.itensPorPagina", "1000"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "parametros": {"paginacao": {"paginaAtual": 0, "itensPorPagina": 1000, "quantidadeDePaginas": 1}},
            "lotes": [lote(&json!([{"txid": "mensalidade209910cliente0001", "status": "CRIADA"}, negada()]))]
        })))
        .expect(2)
        .mount(&env.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/pix/v2/lotecobv/42/sumario"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "dataCriacaoProcessamento": "2026-09-24T13:10:00Z",
            "statusProcessamento": "FINALIZADO",
            "totalCobrancas": 2,
            "totalCobrancasNegadas": 1,
            "totalCobrancasCriadas": 1
        })))
        .expect(1)
        .mount(&env.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/pix/v2/lotecobv/42/situacao/NEGADA"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 42, "status": "NEGADA", "criacao": "2026-09-24T13:10:00.358Z", "cobsv": [negada()]
        })))
        .expect(1)
        .mount(&env.server)
        .await;
    let args = [
        "pix",
        "lote-cobv",
        "listar",
        "--inicio",
        "2026-09-01T00:00:00-03:00",
        "--fim",
        "2026-09-30T23:59:59-03:00",
    ];
    let texto = stdout_of(&env.cmd().args(args).assert().success());
    assert!(
        texto.starts_with("Lotes de cobranças com vencimento criados de 01/09/2026 00:00 a 30/09/2026 23:59\n\nCriado em"),
        "{texto}"
    );
    assert!(
        texto.contains("42  Mensalidades de outubro          2        1        1"),
        "{texto}"
    );
    assert!(texto.ends_with("1 lote · 2 cobranças\n"), "{texto}");
    let csv = stdout_of(
        &env.cmd()
            .args(args)
            .args(["--formato", "csv"])
            .assert()
            .success(),
    );
    assert!(
        csv.ends_with("42,Mensalidades de outubro,2026-09-24T13:10:00.358Z,2,1,1,0\r\n"),
        "{csv}"
    );
    let texto = stdout_of(
        &env.cmd()
            .args(["pix", "lote-cobv", "sumario", "42"])
            .assert()
            .success(),
    );
    assert!(texto.contains("  Negadas        1\n"), "{texto}");
    assert!(
        texto.ends_with("Veja por quê: inter-pj pix lote-cobv situacao 42 negada\n"),
        "{texto}"
    );
    let texto = stdout_of(
        &env.cmd()
            .args(["pix", "lote-cobv", "situacao", "42", "negada"])
            .assert()
            .success(),
    );
    assert!(texto.contains("  Situação   negada\n"), "{texto}");
    assert!(texto.contains("Problemas\n"), "{texto}");
    for args in [
        &["pix", "lote-cobv", "situacao", "42", "paga"][..],
        &["pix", "lote-cobv", "consultar", "abc"][..],
    ] {
        env.cmd().args(args).assert().code(2);
    }
}
