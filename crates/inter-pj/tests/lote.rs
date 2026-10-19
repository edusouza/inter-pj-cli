//! Batches of payments (`/banking/v2/pagamento/lote`) against a mock API.
//! The code is an example of the API documentation; everything else is
//! synthetic.

mod common;

use chrono::NaiveDate;
use common::{client, mount_token};
use inter_pj::Error;
use inter_pj::banking::{
    ItemLote, ItemLoteError, LotePagamentos, LotePagamentosError, PagamentoBoleto,
    PagamentoBoletoError, PagamentoDarf, PagamentoDoLote, StatusBoletoDoLote, StatusDarfDoLote,
    StatusLote,
};
use serde_json::json;
use wiremock::matchers::{any, body_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const LOTE: &str = "/banking/v2/pagamento/lote";
const ID_LOTE: &str = "0123456789abcdef01234567";
const LINHA: &str = "07797777051167847115990071126347192950000003010";
const BARRAS: &str = "07791929500000030107777011678471159007112634";

fn data(ano: i32, mes: u32, dia: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(ano, mes, dia).unwrap()
}

fn boleto(valor: &str) -> ItemLote {
    PagamentoBoleto::new(
        LINHA.parse().unwrap(),
        valor.parse().unwrap(),
        data(2026, 10, 10),
    )
    .into()
}

fn darf() -> ItemLote {
    PagamentoDarf {
        cnpj_cpf: "12.345.678/0001-95".parse().unwrap(),
        codigo_receita: "0220".to_owned(),
        data_vencimento: data(2026, 10, 30),
        descricao: "IRPJ de setembro".to_owned(),
        nome_empresa: "Empresa Exemplo".to_owned(),
        telefone_empresa: None,
        periodo_apuracao: data(2026, 9, 30),
        valor_principal: "47.14".parse().unwrap(),
        valor_multa: None,
        valor_juros: None,
        referencia: "13609400849201739".to_owned(),
    }
    .into()
}

fn lote(pagamentos: Vec<ItemLote>) -> LotePagamentos {
    LotePagamentos {
        meu_identificador: Some("Despesas de outubro".to_owned()),
        pagamentos,
    }
}

fn api_status(err: &Error) -> u16 {
    match err {
        Error::Api(api) => api.status,
        other => panic!("esperado Error::Api, obtido {other:?}"),
    }
}

#[tokio::test]
async fn sends_a_batch_and_follows_it() {
    let server = MockServer::start().await;
    mount_token(
        &server,
        "tok",
        "pagamento-lote.write pagamento-lote.read",
        1,
    )
    .await;
    Mock::given(method("POST"))
        .and(path(LOTE))
        .and(body_json(json!({
            "meuIdentificador": "Despesas de outubro",
            "pagamentos": [
                {
                    "tipoPagamento": "BOLETO",
                    "codBarraLinhaDigitavel": BARRAS,
                    "valorPagar": 30.1,
                    "dataVencimento": "2026-10-10"
                },
                {
                    "tipoPagamento": "DARF",
                    "cnpjCpf": "12345678000195",
                    "codigoReceita": "0220",
                    "dataVencimento": "2026-10-30",
                    "descricao": "IRPJ de setembro",
                    "nomeEmpresa": "Empresa Exemplo",
                    "periodoApuracao": "2026-09-30",
                    "valorPrincipal": 47.14,
                    "referencia": "13609400849201739"
                }
            ]
        })))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "idLote": ID_LOTE,
            "status": "EMPROCESSAMENTO",
            "meuIdentificador": "Despesas de outubro",
            "qtdePagamentos": 2
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("{LOTE}/{ID_LOTE}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "idLote": ID_LOTE,
            "status": "PROCESSADOCOMERRO",
            "meuIdentificador": "Despesas de outubro",
            "qtdePagamentos": 2,
            "contaCorrente": "1234567",
            "dataCriacao": "2026-10-01T10:00:00",
            "pagamentos": [
                {
                    "tipoPagamento": "BOLETO",
                    "seqId": 1,
                    "codigoTransacao": "3414f226-36fb-4d87-811e-cfd99911d845",
                    "status": "AGENDADO",
                    "codBarraLinhaDigitavel": BARRAS,
                    "valorPagar": 30.1,
                    "dataPagamento": "2026-10-10",
                    "dataVencimento": "2026-10-10"
                },
                {
                    "tipoPagamento": "DARF",
                    "seqId": 2,
                    "status": "ERRO_PAGAMENTO",
                    "detalhe": "Saldo insuficiente",
                    "valorTotal": 47.14
                }
            ]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = client(&server);
    let enviado = client
        .banking()
        .enviar_lote(&lote(vec![boleto("30.1"), darf()]))
        .await
        .unwrap();
    assert_eq!(enviado.id_lote.as_deref(), Some(ID_LOTE));
    assert_eq!(enviado.status, Some(StatusLote::EmProcessamento));
    assert_eq!(enviado.qtde_pagamentos, Some(2));

    let consultado = client
        .banking()
        .consultar_lote(&format!(" {ID_LOTE} "))
        .await
        .unwrap();
    assert_eq!(consultado.status, Some(StatusLote::ProcessadoComErro));
    let [PagamentoDoLote::Boleto(boleto), PagamentoDoLote::Darf(darf)] = &consultado.pagamentos[..]
    else {
        panic!("{:?}", consultado.pagamentos);
    };
    assert_eq!(boleto.status, Some(StatusBoletoDoLote::Agendado));
    assert_eq!(darf.status, Some(StatusDarfDoLote::ErroPagamento));
    assert_eq!(darf.detalhe.as_deref(), Some("Saldo insuficiente"));
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

    let invalido = |lote: LotePagamentos| async move {
        let err = banking.enviar_lote(&lote).await.unwrap_err();
        let Error::InvalidInput(source) = err else {
            panic!("{err:?}");
        };
        *source.downcast::<LotePagamentosError>().unwrap()
    };
    assert_eq!(
        invalido(lote(vec![darf()])).await,
        LotePagamentosError::Quantidade(1)
    );
    assert_eq!(
        invalido(lote(vec![darf(), boleto("30.1"), boleto("-1")])).await,
        LotePagamentosError::Item {
            indice: 2,
            erro: ItemLoteError::Boleto(PagamentoBoletoError::ValorNaoPositivo),
        }
    );

    for id in [
        "",
        "123",
        "../../banking/v2/saldo",
        "0123456789abcdef0123456/",
    ] {
        assert!(
            matches!(
                banking.consultar_lote(id).await,
                Err(Error::InvalidInput(_))
            ),
            "{id}"
        );
    }
}

/// Without an idempotency key, only a request that surely was not processed
/// is repeated.
#[tokio::test]
async fn batches_are_retried_only_when_surely_not_processed() {
    let server = MockServer::start().await;
    mount_token(&server, "tok", "pagamento-lote.write", 1).await;
    Mock::given(method("POST"))
        .and(path(LOTE))
        .respond_with(ResponseTemplate::new(429))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(LOTE))
        .respond_with(ResponseTemplate::new(502))
        .expect(1)
        .mount(&server)
        .await;

    let err = client(&server)
        .banking()
        .enviar_lote(&lote(vec![boleto("30.1"), darf()]))
        .await
        .unwrap_err();
    assert_eq!(api_status(&err), 502);
}
