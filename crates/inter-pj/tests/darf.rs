//! DARF payments (`/banking/v2/pagamento/darf`) against a mock API. All data
//! is synthetic.

mod common;

use chrono::NaiveDate;
use common::{client, mount_token};
use inter_pj::Error;
use inter_pj::banking::{
    FiltroDarf, PagamentoDarf, PagamentoDarfError, StatusPagamento, TipoRetornoDarf,
};
use serde_json::json;
use wiremock::matchers::{any, body_json, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const DARF: &str = "/banking/v2/pagamento/darf";
const SOLICITACAO: &str = "8bbdede4-35db-4ec9-b652-e176841e62c8";

fn data(ano: i32, mes: u32, dia: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(ano, mes, dia).unwrap()
}

fn darf() -> PagamentoDarf {
    PagamentoDarf {
        cnpj_cpf: "12.345.678/0001-95".parse().unwrap(),
        codigo_receita: "0220".to_owned(),
        data_vencimento: data(2026, 10, 30),
        descricao: "IRPJ de setembro".to_owned(),
        nome_empresa: "Empresa Exemplo".to_owned(),
        telefone_empresa: Some("+5500000000000".to_owned()),
        periodo_apuracao: data(2026, 9, 30),
        valor_principal: "47.14".parse().unwrap(),
        valor_multa: Some("0".parse().unwrap()),
        valor_juros: Some("10.11".parse().unwrap()),
        referencia: "13609400849201739".to_owned(),
    }
}

fn api_status(err: &Error) -> u16 {
    match err {
        Error::Api(api) => api.status,
        other => panic!("esperado Error::Api, obtido {other:?}"),
    }
}

async fn setup(scope: &str) -> MockServer {
    let server = MockServer::start().await;
    mount_token(&server, "tok", scope, 1).await;
    server
}

#[tokio::test]
async fn pays_a_darf_with_the_documented_body() {
    let server = setup("pagamento-darf.write").await;
    Mock::given(method("POST"))
        .and(path(DARF))
        .and(body_json(json!({
            "cnpjCpf": "12345678000195",
            "codigoReceita": "0220",
            "dataVencimento": "2026-10-30",
            "descricao": "IRPJ de setembro",
            "nomeEmpresa": "Empresa Exemplo",
            "telefoneEmpresa": "+5500000000000",
            "periodoApuracao": "2026-09-30",
            "valorPrincipal": 47.14,
            "valorMulta": 0,
            "valorJuros": 10.11,
            "referencia": "13609400849201739"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "quantidadeAprovadores": 0,
            "autenticacao": "AUTENTICACAO-DE-TESTE",
            "dataPagamento": "01/10/2026",
            "tipoRetorno": "PAGAMENTO",
            "codigoSolicitacao": SOLICITACAO
        })))
        .expect(1)
        .mount(&server)
        .await;

    let resposta = client(&server).banking().pagar_darf(&darf()).await.unwrap();
    assert_eq!(resposta.tipo_retorno, Some(TipoRetornoDarf::Pagamento));
    assert_eq!(resposta.codigo_solicitacao.as_deref(), Some(SOLICITACAO));
    assert_eq!(resposta.data_pagamento.as_deref(), Some("01/10/2026"));
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
    let banking = client.banking();

    let mut invalido = darf();
    invalido.referencia = "12.345".to_owned();
    let err = banking.pagar_darf(&invalido).await.unwrap_err();
    let Error::InvalidInput(source) = &err else {
        panic!("{err:?}");
    };
    assert_eq!(source.downcast_ref(), Some(&PagamentoDarfError::Referencia));

    let invertido = FiltroDarf {
        periodo: Some((data(2026, 9, 30), data(2026, 9, 1))),
        ..FiltroDarf::default()
    };
    assert!(matches!(
        banking.darfs(&invertido).await,
        Err(Error::InvalidInput(_))
    ));
    let codigo_invalido = FiltroDarf {
        codigo_solicitacao: Some("../pix".to_owned()),
        ..FiltroDarf::default()
    };
    assert!(matches!(
        banking.darfs(&codigo_invalido).await,
        Err(Error::InvalidInput(_))
    ));
}

/// Without an idempotency key, only a request that surely was not processed
/// is repeated.
#[tokio::test]
async fn darfs_are_retried_only_when_surely_not_processed() {
    let server = setup("pagamento-darf.write").await;
    Mock::given(method("POST"))
        .and(path(DARF))
        .respond_with(ResponseTemplate::new(429))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(DARF))
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&server)
        .await;

    let err = client(&server)
        .banking()
        .pagar_darf(&darf())
        .await
        .unwrap_err();
    assert_eq!(api_status(&err), 500);
}

#[tokio::test]
async fn lists_darfs_with_the_filters() {
    let server = setup("pagamento-boleto.read").await;
    Mock::given(method("GET"))
        .and(path(DARF))
        .and(query_param("dataInicio", "2026-10-01"))
        .and(query_param("dataFim", "2026-10-31"))
        .and(query_param("codigoReceita", "0220"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "codigoSolicitacao": SOLICITACAO,
                "tipoDarf": "PRETO",
                "valor": 47.14,
                "valorMulta": 0,
                "valorJuros": 10.11,
                "valorTotal": 57.25,
                "tipo": "DARF",
                "periodoApuracao": "2026-09-30 00:00:00",
                "dataPagamento": "2026-10-01 10:00:00",
                "referencia": 13_609_400_849_201_739_u64,
                "dataVencimento": "2026-10-30 00:00:00",
                "codigoReceita": "0220",
                "statusPagamento": "REALIZADO",
                "dataInclusao": "2026-10-01 10:00:00",
                "cnpjCpf": "12345678000195",
                "aprovacoesNecessarias": 0,
                "aprovacoesRealizadas": 0
            }
        ])))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(DARF))
        .and(query_param("codigoSolicitacao", SOLICITACAO))
        .respond_with(ResponseTemplate::new(200).set_body_string("null"))
        .expect(1)
        .mount(&server)
        .await;

    let client = client(&server);
    let filtro = FiltroDarf {
        periodo: Some((data(2026, 10, 1), data(2026, 10, 31))),
        codigo_receita: Some("0220".to_owned()),
        codigo_solicitacao: None,
    };
    let darfs = client.banking().darfs(&filtro).await.unwrap();
    assert_eq!(darfs.len(), 1);
    assert_eq!(darfs[0].status_pagamento, Some(StatusPagamento::Realizado));
    assert_eq!(darfs[0].valor_total, Some("57.25".parse().unwrap()));
    assert_eq!(darfs[0].referencia.as_deref(), Some("13609400849201739"));

    let por_codigo = FiltroDarf {
        codigo_solicitacao: Some(format!(" {} ", SOLICITACAO.to_uppercase())),
        ..FiltroDarf::default()
    };
    assert!(
        client
            .banking()
            .darfs(&por_codigo)
            .await
            .unwrap()
            .is_empty()
    );
}

/// Some banking operations answer errors in a legacy shape (`{"erro": {...}}`),
/// which is read as a [`Problem`](inter_pj::Problem) like the others.
#[tokio::test]
async fn rejections_in_the_legacy_shape_keep_their_message() {
    let server = setup("pagamento-darf.write").await;
    Mock::given(method("POST"))
        .and(path(DARF))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "erro": {
                "mensagem": "Requisição inválida",
                "mensagemDetalhe": "Código de receita não permitido",
                "codigo": 51_002,
                "status": 400
            }
        })))
        .expect(1)
        .mount(&server)
        .await;

    let err = client(&server)
        .banking()
        .pagar_darf(&darf())
        .await
        .unwrap_err();
    let Error::Api(api) = &err else {
        panic!("{err:?}");
    };
    let problem = api.problem.as_ref().unwrap();
    assert_eq!(problem.title.as_deref(), Some("Requisição inválida"));
    assert_eq!(
        problem.detail.as_deref(),
        Some("Código de receita não permitido")
    );
    assert!(
        err.to_string().contains("Código de receita não permitido"),
        "{err}"
    );
}
