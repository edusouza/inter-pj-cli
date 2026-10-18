//! Outbound Pix (`/banking/v2/pix`) against a mock API. Every key, name,
//! document and amount here is synthetic.

mod common;

use std::time::Duration;

use common::{builder, client, mount_token};
use inter_pj::Error;
use inter_pj::banking::{
    DadosBancarios, Destinatario, IdIdempotente, InstituicaoFinanceira, PagamentoPix,
    PagamentoPixError, StatusPix, TipoConta, TipoRetornoPix,
};
use rust_decimal::Decimal;
use serde_json::json;
use wiremock::matchers::{any, body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const PIX: &str = "/banking/v2/pix";
const CODIGO: &str = "c42f0787-02cb-4b31-827e-459ec9d7ece1";

fn dec(s: &str) -> Decimal {
    s.parse().unwrap()
}

fn por_chave(valor: &str) -> PagamentoPix {
    PagamentoPix::new(
        dec(valor),
        Destinatario::Chave {
            chave: "+55 (11) 91234-5678".parse().unwrap(),
        },
    )
}

fn processado() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "tipoRetorno": "PROCESSADO",
        "codigoSolicitacao": CODIGO,
        "dataPagamento": "2026-09-23",
        "dataOperacao": "2026-09-23"
    }))
}

async fn setup(scope: &str) -> MockServer {
    let server = MockServer::start().await;
    mount_token(&server, "tok", scope, 1).await;
    server
}

fn api_status(err: &Error) -> u16 {
    match err {
        Error::Api(api) => api.status,
        other => panic!("esperado Error::Api, obtido {other:?}"),
    }
}

#[tokio::test]
async fn sends_the_payment_with_the_idempotency_key() {
    let server = setup("pagamento-pix.write").await;
    let id = IdIdempotente::novo();
    let mut pagamento = por_chave("150.10");
    pagamento.descricao = Some("NF 123".to_owned());
    Mock::given(method("POST"))
        .and(path(PIX))
        .and(header("authorization", "Bearer tok"))
        .and(header("x-id-idempotente", id.as_str()))
        .and(body_json(json!({
            "valor": 150.1,
            "descricao": "NF 123",
            "destinatario": {"tipo": "CHAVE", "chave": "+5511912345678"}
        })))
        .respond_with(processado())
        .expect(1)
        .mount(&server)
        .await;

    let solicitacao = client(&server)
        .banking()
        .enviar_pix(&pagamento, &id)
        .await
        .unwrap();
    assert_eq!(solicitacao.tipo_retorno, Some(TipoRetornoPix::Processado));
    assert_eq!(solicitacao.codigo_solicitacao.as_deref(), Some(CODIGO));
}

#[tokio::test]
async fn sends_bank_details_and_copia_e_cola_codes() {
    let server = setup("pagamento-pix.write").await;
    let dados = PagamentoPix::new(
        dec("10"),
        Destinatario::DadosBancarios(DadosBancarios {
            nome: "Fornecedor Exemplo".to_owned(),
            cpf_cnpj: "12.345.678/0001-95".parse().unwrap(),
            instituicao_financeira: InstituicaoFinanceira {
                ispb: "00000000".to_owned(),
            },
            agencia: "0001".to_owned(),
            conta_corrente: "1234567".to_owned(),
            tipo_conta: TipoConta::ContaPagamento,
        }),
    );
    let mut agendado = PagamentoPix::new(
        dec("99.99"),
        Destinatario::PixCopiaECola {
            pix_copia_e_cola: "00020126...6304ABCD".to_owned(),
        },
    );
    agendado.data_pagamento = chrono::NaiveDate::from_ymd_opt(2026, 10, 1);
    let client = client(&server);
    for (pagamento, body) in [
        (
            &dados,
            json!({
                "valor": 10,
                "destinatario": {
                    "tipo": "DADOS_BANCARIOS",
                    "nome": "Fornecedor Exemplo",
                    "cpfCnpj": "12345678000195",
                    "instituicaoFinanceira": {"ispb": "00000000"},
                    "agencia": "0001",
                    "contaCorrente": "1234567",
                    "tipoConta": "CONTA_PAGAMENTO"
                }
            }),
        ),
        (
            &agendado,
            json!({
                "valor": 99.99,
                "dataPagamento": "2026-10-01",
                "destinatario": {"tipo": "PIX_COPIA_E_COLA", "pixCopiaECola": "00020126...6304ABCD"}
            }),
        ),
    ] {
        Mock::given(method("POST"))
            .and(path(PIX))
            .and(body_json(body))
            .respond_with(processado())
            .expect(1)
            .mount(&server)
            .await;
        client
            .banking()
            .enviar_pix(pagamento, &IdIdempotente::novo())
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn invalid_payments_never_reach_the_api() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let client = client(&server);

    for (pagamento, esperado) in [
        (por_chave("0"), PagamentoPixError::ValorNaoPositivo),
        (por_chave("-10"), PagamentoPixError::ValorNaoPositivo),
        (por_chave("1.001"), PagamentoPixError::CasasDecimais),
    ] {
        let err = client
            .banking()
            .enviar_pix(&pagamento, &IdIdempotente::novo())
            .await
            .unwrap_err();
        let Error::InvalidInput(source) = &err else {
            panic!("esperado InvalidInput, obtido {err:?}");
        };
        assert_eq!(source.downcast_ref(), Some(&esperado));
    }
}

/// A `429` surely was not processed: repeating it, with the same key, is safe.
#[tokio::test]
async fn rate_limited_payments_are_retried_with_the_same_key() {
    let server = setup("pagamento-pix.write").await;
    let id = IdIdempotente::novo();
    Mock::given(method("POST"))
        .and(path(PIX))
        .and(header("x-id-idempotente", id.as_str()))
        .respond_with(ResponseTemplate::new(429))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(PIX))
        .and(header("x-id-idempotente", id.as_str()))
        .respond_with(processado())
        .expect(1)
        .mount(&server)
        .await;

    let solicitacao = client(&server)
        .banking()
        .enviar_pix(&por_chave("1"), &id)
        .await
        .unwrap();
    assert_eq!(solicitacao.codigo_solicitacao.as_deref(), Some(CODIGO));
}

/// After a `5xx` the payment may have been made: never repeat it automatically.
#[tokio::test]
async fn payments_are_not_retried_after_server_errors() {
    let server = setup("pagamento-pix.write").await;
    Mock::given(method("POST"))
        .and(path(PIX))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&server)
        .await;

    let err = client(&server)
        .banking()
        .enviar_pix(&por_chave("1"), &IdIdempotente::novo())
        .await
        .unwrap_err();
    assert_eq!(api_status(&err), 503);
}

/// A timeout leaves the outcome unknown: never repeat it automatically.
#[tokio::test]
async fn payments_are_not_retried_after_a_timeout() {
    let server = setup("pagamento-pix.write").await;
    Mock::given(method("POST"))
        .and(path(PIX))
        .respond_with(processado().set_delay(Duration::from_secs(2)))
        .expect(1)
        .mount(&server)
        .await;

    let client = builder(&server)
        .timeout(Duration::from_millis(300))
        .build()
        .unwrap();
    let err = client
        .banking()
        .enviar_pix(&por_chave("1"), &IdIdempotente::novo())
        .await
        .unwrap_err();
    assert!(
        matches!(&err, Error::Transport(source) if source.is_timeout()),
        "{err:?}"
    );
}

#[tokio::test]
async fn queries_a_payment_with_its_history() {
    let server = setup("pagamento-pix.read").await;
    Mock::given(method("GET"))
        .and(path(format!("{PIX}/{CODIGO}")))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("{PIX}/{CODIGO}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "transacaoPix": {
                "status": "AGUARDANDO_APROVACAO",
                "valor": 150.1,
                "chave": "+5511912345678",
                "codigoSolicitacao": CODIGO,
                "recebedor": {"nome": "Fornecedor Exemplo"}
            },
            "historico": [
                {"status": "CRIADO", "dataHoraEvento": "2026-09-23T12:00:00"},
                {"status": "AGUARDANDO_APROVACAO", "dataHoraEvento": "2026-09-23T12:00:01"}
            ]
        })))
        .expect(1)
        .mount(&server)
        .await;

    // Queries are retried like any other GET; codes are normalized.
    let consulta = client(&server)
        .banking()
        .consultar_pix(&format!(" {} ", CODIGO.to_uppercase()))
        .await
        .unwrap();
    let transacao = consulta.transacao_pix.unwrap();
    assert_eq!(transacao.status, Some(StatusPix::AguardandoAprovacao));
    assert!(!StatusPix::AguardandoAprovacao.is_final());
    assert_eq!(transacao.valor, Some(dec("150.1")));
    assert_eq!(consulta.historico.len(), 2);
}

#[tokio::test]
async fn request_codes_must_be_uuids() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let client = client(&server);
    for codigo in [
        "",
        "  ",
        "..",
        "../saldo",
        "c42f0787-02cb-4b31-827e-459ec9d7ece",
    ] {
        let err = client.banking().consultar_pix(codigo).await.unwrap_err();
        assert!(matches!(err, Error::InvalidInput(_)), "{codigo}: {err:?}");
    }
}
