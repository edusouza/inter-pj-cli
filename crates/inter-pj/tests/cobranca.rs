//! Charges (`/cobranca/v3/cobrancas`) against a mock API. All data is
//! synthetic.

mod common;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use chrono::NaiveDate;
use common::{builder, client, mount_token};
use inter_pj::cobranca::{
    Desconto, EdicaoCobranca, EmissaoCobranca, EmissaoCobrancaError, FiltrarDataPor,
    FiltroCobrancas, OrdenarCobrancasPor, Pagador, PagarCom, SituacaoCobranca, StatusEdicao, Uf,
};
use inter_pj::{Environment, Error};
use serde_json::json;
use wiremock::matchers::{
    any, body_json, header, method, path, query_param, query_param_is_missing,
};
use wiremock::{Mock, MockServer, ResponseTemplate};

const COBRANCAS: &str = "/cobranca/v3/cobrancas";
const CODIGO: &str = "0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d";

fn cobranca() -> EmissaoCobranca {
    let mut pagador = Pagador::new(
        "12.345.678/0001-95".parse().unwrap(),
        "Cliente Exemplo Ltda",
        "Avenida Brasil",
        "Belo Horizonte",
        Uf::Mg,
        "30110000",
    );
    pagador.numero = Some("1200".to_owned());
    let mut cobranca = EmissaoCobranca::new(
        "NF-123",
        "150.00".parse().unwrap(),
        NaiveDate::from_ymd_opt(2026, 10, 20).unwrap(),
        pagador,
    );
    cobranca.desconto = Some(Desconto::Percentual {
        taxa: "2".parse().unwrap(),
        quantidade_dias: 5,
    });
    cobranca
}

async fn setup(scope: &str) -> MockServer {
    let server = MockServer::start().await;
    mount_token(&server, "tok", scope, 1).await;
    server
}

#[tokio::test]
async fn issues_a_charge_with_the_documented_body() {
    let server = setup("boleto-cobranca.write").await;
    Mock::given(method("POST"))
        .and(path(COBRANCAS))
        .and(header("authorization", "Bearer tok"))
        .and(body_json(json!({
            "seuNumero": "NF-123",
            "valorNominal": 150,
            "dataVencimento": "2026-10-20",
            "numDiasAgenda": 0,
            "pagador": {
                "cpfCnpj": "12345678000195",
                "tipoPessoa": "JURIDICA",
                "nome": "Cliente Exemplo Ltda",
                "endereco": "Avenida Brasil",
                "numero": "1200",
                "cidade": "Belo Horizonte",
                "uf": "MG",
                "cep": "30110000"
            },
            "desconto": {"codigo": "PERCENTUALDATAINFORMADA", "quantidadeDias": 5, "taxa": 2}
        })))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"codigoSolicitacao": CODIGO})),
        )
        .expect(1)
        .mount(&server)
        .await;

    let resposta = client(&server)
        .cobranca()
        .emitir(&cobranca())
        .await
        .unwrap();
    assert_eq!(resposta.codigo_solicitacao.as_deref(), Some(CODIGO));
}

#[tokio::test]
async fn invalid_requests_never_reach_the_api() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let client = client(&server);

    let mut invalida = cobranca();
    invalida.pagador.cep = "30110-000".to_owned();
    let err = client.cobranca().emitir(&invalida).await.unwrap_err();
    let Error::InvalidInput(source) = &err else {
        panic!("{err:?}");
    };
    assert_eq!(
        source.downcast_ref(),
        Some(&EmissaoCobrancaError::Cep {
            campo: "pagador.cep"
        })
    );
    assert!(matches!(
        client.cobranca().consultar("../../banking/v2/saldo").await,
        Err(Error::InvalidInput(_))
    ));
}

/// The API refuses duplicates for 30 minutes, but the answer to the first
/// request would be lost: only a request that surely was not processed is
/// repeated.
#[tokio::test]
async fn charges_are_retried_only_when_surely_not_processed() {
    let server = setup("boleto-cobranca.write").await;
    Mock::given(method("POST"))
        .and(path(COBRANCAS))
        .respond_with(ResponseTemplate::new(429))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(COBRANCAS))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&server)
        .await;

    let err = client(&server)
        .cobranca()
        .emitir(&cobranca())
        .await
        .unwrap_err();
    assert!(
        matches!(&err, Error::Api(api) if api.status == 503),
        "{err:?}"
    );
}

#[tokio::test]
async fn looks_up_a_charge() {
    let server = setup("boleto-cobranca.read").await;
    Mock::given(method("GET"))
        .and(path(format!("{COBRANCAS}/{CODIGO}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "cobranca": {
                "codigoSolicitacao": CODIGO,
                "seuNumero": "NF-123",
                "situacao": "A_RECEBER",
                "valorNominal": 150,
                "pagador": {"cpfCnpj": "12345678000195", "nome": "Cliente Exemplo Ltda"}
            },
            "boleto": {
                "nossoNumero": "12345678",
                "codigoBarras": "07791159500000150000001112345678001234567890",
                "linhaDigitavel": "07790001161234567800812345678901115950000015000"
            },
            "pix": {"txid": "COBRANCAEXEMPLO00000000001", "pixCopiaECola": "00020101021226..."}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let cobranca = client(&server)
        .cobranca()
        .consultar(&format!(" {CODIGO} "))
        .await
        .unwrap();
    assert_eq!(cobranca.cobranca.situacao, Some(SituacaoCobranca::AReceber));
    assert_eq!(
        cobranca.boleto.and_then(|b| b.nosso_numero).as_deref(),
        Some("12345678")
    );
    assert_eq!(
        cobranca.pix.and_then(|p| p.pix_copia_e_cola).as_deref(),
        Some("00020101021226...")
    );
}

#[tokio::test]
async fn a_charge_being_issued_has_no_boleto_yet() {
    let server = setup("boleto-cobranca.read").await;
    Mock::given(method("GET"))
        .and(path(format!("{COBRANCAS}/{CODIGO}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "cobranca": {"codigoSolicitacao": CODIGO, "situacao": "EM_PROCESSAMENTO"}
        })))
        .mount(&server)
        .await;

    let cobranca = client(&server).cobranca().consultar(CODIGO).await.unwrap();
    assert_eq!(
        cobranca.cobranca.situacao,
        Some(SituacaoCobranca::EmProcessamento)
    );
    assert!(cobranca.boleto.is_none() && cobranca.pix.is_none());
}

fn setembro() -> FiltroCobrancas {
    FiltroCobrancas::new(
        NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(),
        NaiveDate::from_ymd_opt(2026, 9, 30).unwrap(),
    )
}

fn item(seu_numero: &str) -> serde_json::Value {
    json!({
        "cobranca": {
            "codigoSolicitacao": CODIGO,
            "seuNumero": seu_numero,
            "situacao": "A_RECEBER",
            "valorNominal": "150.00",
            "pagador": {"nome": "Cliente Exemplo Ltda", "cpfCnpj": "12345678000195"}
        }
    })
}

#[tokio::test]
async fn lists_a_page_with_every_filter() {
    let server = setup("boleto-cobranca.read").await;
    Mock::given(method("GET"))
        .and(path(COBRANCAS))
        .and(query_param("dataInicial", "2026-09-01"))
        .and(query_param("dataFinal", "2026-09-30"))
        .and(query_param("filtrarDataPor", "EMISSAO"))
        .and(query_param("situacao", "A_RECEBER"))
        .and(query_param("cpfCnpjPessoaPagadora", "12345678000195"))
        .and(query_param("ordenarPor", "VALOR"))
        .and(query_param("tipoOrdenacao", "DESC"))
        .and(query_param("paginacao.paginaAtual", "2"))
        .and(query_param("paginacao.itensPorPagina", "50"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "totalPaginas": 3,
            "totalElementos": 101,
            "tamanhoPagina": 50,
            "primeiraPagina": false,
            "ultimaPagina": true,
            "numeroDeElementos": 1,
            "cobrancas": [item("NF-123")]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let mut filtro = setembro();
    filtro.filtrar_data_por = Some(FiltrarDataPor::Emissao);
    filtro.situacao = Some(SituacaoCobranca::AReceber);
    filtro.cpf_cnpj_pessoa_pagadora = Some("12345678000195".to_owned());
    filtro.ordenar_por = Some(OrdenarCobrancasPor::Valor);
    filtro.decrescente = true;
    let pagina = client(&server)
        .cobranca()
        .listar(&filtro, 2, Some(50))
        .await
        .unwrap();
    assert_eq!(pagina.total_elementos, Some(101));
    assert_eq!(
        pagina.cobrancas[0].cobranca.valor_nominal,
        Some("150.00".parse().unwrap())
    );
}

#[tokio::test]
async fn reads_every_page() {
    let server = setup("boleto-cobranca.read").await;
    for (pagina, ultima, seu_numero) in [("0", false, "NF-1"), ("1", true, "NF-2")] {
        Mock::given(method("GET"))
            .and(path(COBRANCAS))
            .and(query_param("paginacao.paginaAtual", pagina))
            .and(query_param("paginacao.itensPorPagina", "1000"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ultimaPagina": ultima,
                "cobrancas": [item(seu_numero)]
            })))
            .expect(1)
            .mount(&server)
            .await;
    }

    let todas = client(&server)
        .cobranca()
        .listar_todas(&setembro())
        .await
        .unwrap();
    let numeros: Vec<_> = todas
        .iter()
        .map(|c| c.cobranca.seu_numero.as_deref().unwrap())
        .collect();
    assert_eq!(numeros, ["NF-1", "NF-2"]);
}

#[tokio::test]
async fn summarizes_by_situation_without_paging() {
    let server = setup("boleto-cobranca.read").await;
    Mock::given(method("GET"))
        .and(path(format!("{COBRANCAS}/sumario")))
        .and(query_param("dataInicial", "2026-09-01"))
        .and(query_param("seuNumero", "NF-123"))
        .and(query_param_is_missing("paginacao.paginaAtual"))
        .and(query_param_is_missing("ordenarPor"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"situacao": "A_RECEBER", "valor": 1000, "quantidade": 30},
            {"situacao": "RECEBIDO", "valor": 4000.5, "quantidade": 65}
        ])))
        .expect(1)
        .mount(&server)
        .await;

    let mut filtro = setembro();
    filtro.seu_numero = Some("NF-123".to_owned());
    filtro.ordenar_por = Some(OrdenarCobrancasPor::Valor);
    let sumario = client(&server).cobranca().sumario(&filtro).await.unwrap();
    assert_eq!(sumario.len(), 2);
    assert_eq!(sumario[1].valor, Some("4000.5".parse().unwrap()));
    assert_eq!(sumario[1].quantidade, Some(65));
}

#[tokio::test]
async fn invalid_filters_never_reach_the_api() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let invertido = FiltroCobrancas::new(
        NaiveDate::from_ymd_opt(2026, 9, 30).unwrap(),
        NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(),
    );
    let client = client(&server);
    assert!(matches!(
        client.cobranca().listar(&invertido, 0, None).await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        client.cobranca().sumario(&invertido).await,
        Err(Error::InvalidInput(_))
    ));
}

#[tokio::test]
async fn downloads_the_pdf() {
    let server = setup("boleto-cobranca.read").await;
    let pdf = b"%PDF-1.4 boleto de exemplo";
    Mock::given(method("GET"))
        .and(path(format!("{COBRANCAS}/{CODIGO}/pdf")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"pdf": BASE64.encode(pdf)})))
        .expect(1)
        .mount(&server)
        .await;
    let baixado = client(&server).cobranca().pdf(CODIGO).await.unwrap();
    assert_eq!(baixado, pdf);
}

#[tokio::test]
async fn refuses_content_that_is_not_a_pdf() {
    let server = setup("boleto-cobranca.read").await;
    Mock::given(method("GET"))
        .and(path(format!("{COBRANCAS}/{CODIGO}/pdf")))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"pdf": BASE64.encode("<html>")})),
        )
        .mount(&server)
        .await;
    let err = client(&server).cobranca().pdf(CODIGO).await.unwrap_err();
    assert!(
        matches!(&err, Error::Decode { message, .. } if message.contains("não é um PDF")),
        "{err:?}"
    );
}

#[tokio::test]
async fn cancels_with_a_reason() {
    let server = setup("boleto-cobranca.write").await;
    Mock::given(method("POST"))
        .and(path(format!("{COBRANCAS}/{CODIGO}/cancelar")))
        .and(body_json(json!({"motivoCancelamento": "Pedido cancelado"})))
        .respond_with(ResponseTemplate::new(202))
        .expect(1)
        .mount(&server)
        .await;
    let cliente = client(&server);
    cliente
        .cobranca()
        .cancelar(CODIGO, " Pedido cancelado ")
        .await
        .unwrap();
    // A reason too long is refused before any request.
    assert!(matches!(
        cliente.cobranca().cancelar(CODIGO, &"a".repeat(51)).await,
        Err(Error::InvalidInput(_))
    ));
}

#[tokio::test]
async fn edits_and_follows_the_change() {
    const EDICAO: &str = "5a6b7c8d-1e2f-4a3b-8c9d-0e1f2a3b4c5d";
    // One token with both scopes serves both requests.
    let server = setup("boleto-cobranca.write boleto-cobranca.read").await;
    Mock::given(method("PATCH"))
        .and(path(format!("{COBRANCAS}/{CODIGO}")))
        .and(body_json(
            json!({"dataVencimento": "2026-11-10", "valorNominal": 175.9}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": "PROCESSANDO",
            "mensagem": "Edição em processamento",
            "codigoEdicao": EDICAO
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("{COBRANCAS}/edicao/{EDICAO}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"status": "SUCESSO"})))
        .expect(1)
        .mount(&server)
        .await;

    let cliente = client(&server);
    let edicao = EdicaoCobranca::new(
        NaiveDate::from_ymd_opt(2026, 11, 10),
        Some("175.90".parse().unwrap()),
    );
    let resposta = cliente.cobranca().editar(CODIGO, &edicao).await.unwrap();
    assert_eq!(resposta.status, Some(StatusEdicao::Processando));
    let status = cliente
        .cobranca()
        .consultar_edicao(resposta.codigo_edicao.as_deref().unwrap())
        .await
        .unwrap();
    assert_eq!(status.status, Some(StatusEdicao::Sucesso));
    // Nothing to change is refused before any request.
    assert!(matches!(
        cliente
            .cobranca()
            .editar(CODIGO, &EdicaoCobranca::default())
            .await,
        Err(Error::InvalidInput(_))
    ));
}

#[tokio::test]
async fn pays_only_in_the_sandbox() {
    let server = MockServer::start().await;
    mount_token(&server, "tok", "boleto-cobranca.write", 1).await;
    Mock::given(method("POST"))
        .and(path(format!("{COBRANCAS}/{CODIGO}/pagar")))
        .and(body_json(json!({"pagarCom": "PIX"})))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    // Without the sandbox declared, nothing is sent.
    let sem_ambiente = client(&server);
    assert!(matches!(
        sem_ambiente
            .cobranca()
            .pagar_no_sandbox(CODIGO, PagarCom::Pix)
            .await,
        Err(Error::InvalidInput(_))
    ));
    let producao = builder(&server)
        .environment(Environment::Production)
        .build()
        .unwrap();
    assert!(matches!(
        producao
            .cobranca()
            .pagar_no_sandbox(CODIGO, PagarCom::Pix)
            .await,
        Err(Error::InvalidInput(_))
    ));

    let sandbox = builder(&server)
        .environment(Environment::Sandbox)
        .build()
        .unwrap();
    sandbox
        .cobranca()
        .pagar_no_sandbox(CODIGO, PagarCom::Pix)
        .await
        .unwrap();
}
