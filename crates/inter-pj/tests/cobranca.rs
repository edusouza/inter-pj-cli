//! Charges (`/cobranca/v3/cobrancas`) against a mock API. All data is
//! synthetic.

mod common;

use chrono::NaiveDate;
use common::{client, mount_token};
use inter_pj::Error;
use inter_pj::cobranca::{
    Desconto, EmissaoCobranca, EmissaoCobrancaError, Pagador, SituacaoCobranca, Uf,
};
use serde_json::json;
use wiremock::matchers::{any, body_json, header, method, path};
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
