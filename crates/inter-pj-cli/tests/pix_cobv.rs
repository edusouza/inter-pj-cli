//! `inter-pj pix cobv ...` end to end, against a mock API. All data is
//! synthetic; the "copia e cola" is the example of the Banco Central's
//! manual. Due dates are far in the future, so that none has passed.

mod common;

use common::{COPIA_E_COLA, TestEnv, ler_qr_code, stderr_of, stdout_of};
use serde_json::{Value, json};
use wiremock::matchers::{any, body_json, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

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

fn cobv(status: &str) -> Value {
    json!({
        "calendario": {"criacao": "2026-09-23T20:15:00.358Z", "dataDeVencimento": "2099-10-20", "validadeAposVencimento": 30},
        "txid": TXID,
        "revisao": 0,
        "loc": {"id": 790, "location": "pix.example.com/qr/v2/cobv/9d36b84fc70b478fb95c12729b90ca25", "tipoCob": "cobv"},
        "status": status,
        "devedor": {
            "cnpj": "12345678000195", "nome": "Cliente Exemplo Ltda", "email": "financeiro@empresa.example",
            "logradouro": "Avenida Brasil, 1200", "cidade": "Belo Horizonte", "uf": "MG", "cep": "30110000"
        },
        "recebedor": {"cnpj": "11222333000181", "nome": "Empresa Exemplo Ltda", "nomeFantasia": "Empresa Exemplo"},
        "valor": {
            "original": "150.00",
            "multa": {"modalidade": "2", "valorPerc": "2.00"},
            "juros": {"modalidade": "3", "valorPerc": "1.00"},
            "desconto": {"modalidade": "1", "descontoDataFixa": [{"data": "2099-10-15", "valorPerc": "10.00"}]}
        },
        "chave": CHAVE,
        "solicitacaoPagador": "Referente à NF 123",
        "pixCopiaECola": COPIA_E_COLA
    })
}

fn criar() -> Vec<&'static str> {
    vec![
        "pix",
        "cobv",
        "criar",
        "--chave",
        CHAVE,
        "--valor",
        "150,00",
        "--vencimento",
        "2099-10-20",
        "--validade-apos-vencimento",
        "30",
        "--devedor-documento",
        "12.345.678/0001-95",
        "--devedor-nome",
        "Cliente Exemplo Ltda",
        "--devedor-email",
        "financeiro@empresa.example",
        "--devedor-endereco",
        "Avenida Brasil, 1200",
        "--devedor-cidade",
        "Belo Horizonte",
        "--devedor-uf",
        "mg",
        "--devedor-cep",
        "30110-000",
        "--multa",
        "2%",
        "--juros",
        "1%",
        "--desconto",
        "10,00@2099-10-15",
        "--solicitacao",
        "Referente à NF 123",
    ]
}

/// `criar()` with `opcao` set to `valor`, in place of its value there.
fn criar_com<'a>(opcao: &'a str, valor: &'a str) -> Vec<&'a str> {
    let mut args = criar();
    match args.iter().position(|arg| *arg == opcao) {
        Some(i) => args[i + 1] = valor,
        None => args.extend([opcao, valor]),
    }
    args
}

fn corpo() -> Value {
    json!({
        "calendario": {"dataDeVencimento": "2099-10-20", "validadeAposVencimento": 30},
        "devedor": {
            "logradouro": "Avenida Brasil, 1200", "cidade": "Belo Horizonte", "uf": "MG", "cep": "30110000",
            "cnpj": "12345678000195", "nome": "Cliente Exemplo Ltda", "email": "financeiro@empresa.example"
        },
        "valor": {
            "original": "150.00",
            "multa": {"modalidade": 2, "valorPerc": "2.00"},
            "juros": {"modalidade": 3, "valorPerc": "1.00"},
            "desconto": {"modalidade": 1, "descontoDataFixa": [{"data": "2099-10-15", "valorPerc": "10.00"}]}
        },
        "chave": CHAVE,
        "solicitacaoPagador": "Referente à NF 123"
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn cria_pelas_opcoes_com_encargos_e_desenha_o_qr_code() {
    let env = env().await;
    env.mount_token("cobv.write", Some(1)).await;
    Mock::given(method("PUT"))
        .and(path(format!("/pix/v2/cobv/{TXID}")))
        .and(body_json(corpo()))
        .respond_with(ResponseTemplate::new(201).set_body_json(cobv("ATIVA")))
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
        stdout.starts_with(&format!(
            "Cobrança Pix com vencimento criada.\n\nCobrança Pix com vencimento {TXID}\n"
        )),
        "{stdout}"
    );
    for linha in [
        "Vencimento   20/10/2099",
        "Validade     até 19/11/2099, 30 dias após o vencimento",
        "Recebedor    Empresa Exemplo Ltda (11.222.333/0001-81)",
        "Desconto     R$ 10,00 até 15/10/2099",
        &format!("Acompanhe com: inter-pj pix cobv consultar {TXID}"),
    ] {
        assert!(stdout.contains(linha), "{linha}\n{stdout}");
    }
    assert_eq!(ler_qr_code(&stdout), COPIA_E_COLA);
    let stderr = stderr_of(&assert);
    for linha in [
        "Cobrança Pix com vencimento a criar",
        "R$ 150,00 (cento e cinquenta reais)",
        "até 19/11/2099, 30 dias após o vencimento",
        "Cliente Exemplo Ltda (12.345.678/0001-95)",
        "Avenida Brasil, 1200 - Belo Horizonte/MG - CEP 30110-000",
        "Multa        2%",
        "Juros        1% ao mês (dias corridos)",
        "R$ 10,00 até 15/10/2099",
    ] {
        assert!(stderr.contains(linha), "{linha}\n{stderr}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn cria_por_arquivo_ou_pela_entrada_padrao() {
    let env = env().await;
    env.mount_token("cobv.write", Some(1)).await;
    let arquivo = json!({
        "calendario": {"dataDeVencimento": "2099-10-20", "validadeAposVencimento": 30},
        "devedor": {
            "cnpj": "12.345.678/0001-95", "nome": "Cliente Exemplo Ltda", "email": "financeiro@empresa.example",
            "logradouro": "Avenida Brasil, 1200", "cidade": "Belo Horizonte", "uf": "MG", "cep": "30110-000"
        },
        "valor": {
            "original": "150,00",
            "multa": {"modalidade": 2, "valorPerc": 2},
            "juros": {"modalidade": "3", "valorPerc": "1.00"},
            "desconto": {"modalidade": 1, "descontoDataFixa": [{"data": "2099-10-15", "valorPerc": "10,00"}]}
        },
        "chave": CHAVE,
        "solicitacaoPagador": "Referente à NF 123"
    });
    let caminho = env.path("cobv.json");
    std::fs::write(&caminho, arquivo.to_string()).unwrap();
    Mock::given(method("PUT"))
        .and(path(format!("/pix/v2/cobv/{TXID}")))
        .and(body_json(corpo()))
        .respond_with(ResponseTemplate::new(201).set_body_json(cobv("ATIVA")))
        .expect(1)
        .mount(&env.server)
        .await;
    let assert = env
        .cmd()
        .args(["pix", "cobv", "criar", "--arquivo"])
        .arg(&caminho)
        .args(["--txid", TXID, "--sim", "--json"])
        .assert()
        .success();
    let json: Value = serde_json::from_str(&stdout_of(&assert)).unwrap();
    assert_eq!(json["txid"], TXID);
    // Modalities as the schema documents them: numbers.
    assert_eq!(
        json["valor"]["multa"],
        json!({"modalidade": 2, "valorPerc": "2.00"})
    );

    // From the standard input, only with --sim: it cannot be the answer too.
    let assert = env
        .cmd()
        .args(["pix", "cobv", "criar", "--arquivo", "-"])
        .write_stdin(arquivo.to_string())
        .assert()
        .code(2);
    assert!(
        stderr_of(&assert).contains("use --sim"),
        "{}",
        stderr_of(&assert)
    );
    // The template is a valid charge.
    let modelo = stdout_of(&env.cmd().args(["pix", "cobv", "modelo"]).assert().success());
    let assert = env
        .cmd()
        .args([
            "pix",
            "cobv",
            "criar",
            "--arquivo",
            "-",
            "--simular",
            "--json",
        ])
        .write_stdin(modelo)
        .assert()
        .success();
    let simulacao: Value = serde_json::from_str(&stdout_of(&assert)).unwrap();
    assert_eq!(simulacao["corpo"]["devedor"]["cnpj"], "12345678000195");
    assert_eq!(
        simulacao["corpo"]["valor"]["desconto"]["modalidade"],
        json!(1)
    );
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
        format!("{}/pix/v2/cobv/{TXID}", env.server.uri())
    );
    assert_eq!(json["corpo"], corpo());
}

#[tokio::test(flavor = "multi_thread")]
async fn cobranca_invalida_nao_chama_a_api() {
    let env = env().await;
    nothing_is_sent(&env).await;
    let casos = [
        (
            criar_com("--vencimento", "2020-01-10"),
            "o vencimento (10/01/2020) já passou",
        ),
        (
            criar_com("--desconto", "10,00@2099-10-25"),
            "--desconto: o desconto vale até uma data no vencimento ou antes dele",
        ),
        (
            [criar(), vec!["--desconto", "2%"]].concat(),
            "--desconto: use só percentuais (2%) ou só valores (10,00) nas datas",
        ),
        (
            criar_com("--multa", "101%"),
            "--multa: o percentual vai até 100",
        ),
        (
            [criar_com("--juros", "0,50"), vec!["--juros-periodo", "ano"]].concat(),
            "--juros-periodo: juros em valor são por dia",
        ),
        (
            criar_com("--abatimento", "150,00"),
            "--abatimento: o valor tem de ser menor que o valor original",
        ),
        (
            criar_com("--devedor-email", "financeiro"),
            "--devedor-email: e-mail inválido",
        ),
    ];
    for (args, esperado) in casos {
        let assert = env.cmd().args(args).arg("--sim").assert().code(2);
        assert!(
            stderr_of(&assert).contains(esperado),
            "{esperado}\n{}",
            stderr_of(&assert)
        );
    }
    env.cmd()
        .args(criar_com("--devedor-cep", "3011"))
        .arg("--sim")
        .assert()
        .code(2);
    let caminho = env.path("cobv.json");
    std::fs::write(
        &caminho,
        r#"{"calendario": {"dataDeVencimento": "2099-10-20"}, "valorMuta": 1}"#,
    )
    .unwrap();
    let assert = env
        .cmd()
        .args(["pix", "cobv", "criar", "--sim", "--arquivo"])
        .arg(&caminho)
        .assert()
        .code(2);
    assert!(
        stderr_of(&assert).contains("campo desconhecido \"valorMuta\""),
        "{}",
        stderr_of(&assert)
    );
    for args in [
        // --arquivo replaces the options.
        &[
            "pix",
            "cobv",
            "criar",
            "--arquivo",
            "cobv.json",
            "--valor",
            "1",
        ][..],
        // The payer is required with the options.
        &[
            "pix",
            "cobv",
            "criar",
            "--chave",
            CHAVE,
            "--valor",
            "1",
            "--vencimento",
            "2099-10-20",
        ][..],
        &["pix", "cobv", "criar", "--sim"][..],
    ] {
        env.cmd().args(args).assert().code(2);
    }
    // Without a terminal and without --sim, nothing is sent.
    env.cmd().args(criar()).write_stdin("s\n").assert().code(2);
}

/// With an unknown outcome, the error says how to check and how to repeat
/// without creating two charges.
#[tokio::test(flavor = "multi_thread")]
async fn resultado_incerto_orienta_a_repetir_com_o_mesmo_txid() {
    let env = env().await;
    env.mount_token("cobv.write", Some(1)).await;
    Mock::given(method("PUT"))
        .and(path(format!("/pix/v2/cobv/{TXID}")))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&env.server)
        .await;
    let assert = env
        .cmd()
        .args(criar())
        .args(["--txid", TXID, "--sim"])
        .assert()
        .code(6);
    let stderr = stderr_of(&assert);
    for dica in [
        format!("dica: confira com: inter-pj pix cobv consultar {TXID}"),
        format!("dica: ou repita o comando com --txid {TXID}"),
    ] {
        assert!(stderr.contains(&dica), "{dica}\n{stderr}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn consulta_a_cobranca() {
    let env = env().await;
    env.mount_token("cobv.read", None).await;
    Mock::given(method("GET"))
        .and(path(format!("/pix/v2/cobv/{TXID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(cobv("ATIVA")))
        .expect(1)
        .mount(&env.server)
        .await;
    let texto = stdout_of(
        &env.cmd()
            .args(["pix", "cobv", "consultar", TXID])
            .assert()
            .success(),
    );
    assert!(
        texto.starts_with(&format!(
            "Cobrança Pix com vencimento {TXID}\n  Status       ativa\n  Valor        R$ 150,00\n"
        )),
        "{texto}"
    );
    for linha in [
        "Endereço     Avenida Brasil, 1200 - Belo Horizonte/MG - CEP 30110-000",
        "Multa        2%",
        "Juros        1% ao mês (dias corridos)",
        &format!("Copia e cola  {COPIA_E_COLA}"),
    ] {
        assert!(texto.contains(linha), "{linha}\n{texto}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn revisa_e_remove_a_cobranca() {
    let env = env().await;
    env.mount_token("cobv.write cobv.read", None).await;
    let cobv_path = format!("/pix/v2/cobv/{TXID}");
    Mock::given(method("GET"))
        .and(path(cobv_path.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(cobv("ATIVA")))
        .expect(3)
        .mount(&env.server)
        .await;
    let mut revisada = cobv("ATIVA");
    revisada["revisao"] = json!(1);
    // The due date and the validity go together: the one not given stays.
    Mock::given(method("PATCH"))
        .and(path(cobv_path.clone()))
        .and(body_json(json!({
            "calendario": {"dataDeVencimento": "2099-10-30", "validadeAposVencimento": 30},
            "valor": {"original": "160.00"}
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(revisada.clone()))
        .expect(1)
        .mount(&env.server)
        .await;
    // A discount without a date lasts until the current due date.
    Mock::given(method("PATCH"))
        .and(path(cobv_path.clone()))
        .and(body_json(json!({
            "valor": {"desconto": {"modalidade": 2, "descontoDataFixa": [{"data": "2099-10-20", "valorPerc": "5.00"}]}}
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(revisada))
        .expect(1)
        .mount(&env.server)
        .await;
    let mut removida = cobv("REMOVIDA_PELO_USUARIO_RECEBEDOR");
    removida["revisao"] = json!(2);
    Mock::given(method("PATCH"))
        .and(path(cobv_path))
        .and(body_json(
            json!({"status": "REMOVIDA_PELO_USUARIO_RECEBEDOR"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(removida))
        .expect(1)
        .mount(&env.server)
        .await;

    let assert = env
        .cmd()
        .args([
            "pix",
            "cobv",
            "revisar",
            TXID,
            "--valor",
            "160",
            "--vencimento",
            "2099-10-30",
            "--sim",
        ])
        .assert()
        .success();
    assert!(
        stdout_of(&assert).starts_with("Cobrança Pix com vencimento alterada (revisão 1)."),
        "{}",
        stdout_of(&assert)
    );
    let stderr = stderr_of(&assert);
    for linha in [
        "R$ 150,00 → R$ 160,00",
        "20/10/2099 → 30/10/2099",
        "até 19/11/2099, 30 dias após o vencimento → até 29/11/2099, 30 dias após o vencimento",
    ] {
        assert!(stderr.contains(linha), "{linha}\n{stderr}");
    }

    let assert = env
        .cmd()
        .args(["pix", "cobv", "revisar", TXID, "--desconto", "5%", "--sim"])
        .assert()
        .success();
    assert!(
        stderr_of(&assert).contains("R$ 10,00 até 15/10/2099 → 5% até 20/10/2099"),
        "{}",
        stderr_of(&assert)
    );

    let assert = env
        .cmd()
        .args(["pix", "cobv", "revisar", TXID, "--remover", "--sim"])
        .assert()
        .success();
    assert!(
        stdout_of(&assert)
            .starts_with("Cobrança Pix com vencimento removida: ela não pode mais ser paga."),
        "{}",
        stdout_of(&assert)
    );
}

/// What depends on the charge is checked after the lookup, and nothing is
/// changed when it fails.
#[tokio::test(flavor = "multi_thread")]
async fn revisao_confere_a_cobranca_atual() {
    let env = env().await;
    env.mount_token("cobv.write cobv.read", None).await;
    Mock::given(method("GET"))
        .and(path(format!("/pix/v2/cobv/{TXID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(cobv("ATIVA")))
        .expect(2)
        .mount(&env.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/pix/v2/cobv/a1b2c3d4e5f60718293a4b5c6d7e8f90"))
        .respond_with(ResponseTemplate::new(200).set_body_json(cobv("CONCLUIDA")))
        .expect(1)
        .mount(&env.server)
        .await;
    Mock::given(method("PATCH"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&env.server)
        .await;
    let casos = [
        (
            &["--vencimento", "2099-10-10"][..],
            "--vencimento: o desconto atual vale até 15/10/2099, depois do novo vencimento",
        ),
        (
            &["--abatimento", "200,00"][..],
            "--abatimento: o valor tem de ser menor que o valor original",
        ),
    ];
    for (extra, esperado) in casos {
        let assert = env
            .cmd()
            .args(["pix", "cobv", "revisar", TXID])
            .args(extra)
            .arg("--sim")
            .assert()
            .code(2);
        assert!(
            stderr_of(&assert).contains(esperado),
            "{esperado}\n{}",
            stderr_of(&assert)
        );
    }
    let assert = env
        .cmd()
        .args([
            "pix",
            "cobv",
            "revisar",
            "a1b2c3d4e5f60718293a4b5c6d7e8f90",
            "--remover",
            "--sim",
        ])
        .assert()
        .code(2);
    assert!(
        stderr_of(&assert).contains("a cobrança já foi paga: não pode ser alterada"),
        "{}",
        stderr_of(&assert)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn lista_as_cobrancas_do_periodo() {
    let env = env().await;
    env.mount_token("cobv.read", None).await;
    let pagina = |paginacao: Value| {
        json!({
            "parametros": {"inicio": "2026-09-01T03:00:00Z", "fim": "2026-10-01T02:59:59Z", "paginacao": paginacao},
            "cobs": [cobv("CONCLUIDA"), cobv("ATIVA")]
        })
    };
    Mock::given(method("GET"))
        .and(path("/pix/v2/cobv"))
        .and(query_param("inicio", "2026-09-01T00:00:00-03:00"))
        .and(query_param("fim", "2026-09-30T23:59:59-03:00"))
        .and(query_param("cnpj", "12345678000195"))
        .and(query_param("loteCobVId", "42"))
        .and(query_param("paginacao.itensPorPagina", "1000"))
        .respond_with(ResponseTemplate::new(200).set_body_json(pagina(json!(
            {"paginaAtual": 0, "itensPorPagina": 1000, "quantidadeDePaginas": 1, "quantidadeTotalDeItens": 2}
        ))))
        .expect(3)
        .mount(&env.server)
        .await;
    // Without the number of pages, a full page means there may be more.
    Mock::given(method("GET"))
        .and(path("/pix/v2/cobv"))
        .and(query_param("paginacao.itensPorPagina", "2"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(pagina(json!({"paginaAtual": 0, "itensPorPagina": 2}))),
        )
        .expect(1)
        .mount(&env.server)
        .await;
    let args = [
        "pix",
        "cobv",
        "listar",
        "--inicio",
        "2026-09-01T00:00:00-03:00",
        "--fim",
        "2026-09-30T23:59:59-03:00",
        "--documento",
        "12.345.678/0001-95",
        "--lote",
        "42",
    ];
    let texto = stdout_of(&env.cmd().args(args).assert().success());
    assert!(
        texto.starts_with("Cobranças Pix com vencimento criadas de 01/09/2026 00:00 a 30/09/2026 23:59 (devedor 12.345.678/0001-95, lote 42)\n\nVencimento  Status"),
        "{texto}"
    );
    assert!(texto.contains("20/10/2099  concluída (paga)"), "{texto}");
    assert!(
        texto.ends_with("2 cobranças · R$ 300,00 · pagas R$ 150,00\n"),
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
            .args(["--formato", "csv"])
            .assert()
            .success(),
    );
    assert!(
        csv.starts_with("txid,status,revisao,calendario.criacao,calendario.dataDeVencimento,calendario.validadeAposVencimento,valor.original,valor.multa.modalidade,valor.multa.valorPerc,"),
        "{csv}"
    );
    assert!(
        csv.contains(",CONCLUIDA,0,2026-09-23T20:15:00.358Z,2099-10-20,30,150.00,2,2.00,3,1.00,,,1,,,12345678000195,"),
        "{csv}"
    );
    let texto = stdout_of(
        &env.cmd()
            .args([
                "pix",
                "cobv",
                "listar",
                "--pagina",
                "0",
                "--itens-por-pagina",
                "2",
            ])
            .assert()
            .success(),
    );
    assert!(
        texto.ends_with("Página 0; a próxima é --pagina 1\n"),
        "{texto}"
    );
}
