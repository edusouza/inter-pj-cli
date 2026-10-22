//! Locations (`/pix/v2/loc`), batches of charges with a due date
//! (`/pix/v2/lotecobv`) and the payments of the sandbox against a mock API.
//! All data is synthetic.

mod common;

use chrono::{DateTime, NaiveDate};
use common::{builder, client, mount_token};
use inter_pj::pix::{
    CobvDoLote, CobvRevisada, CobvRevisadaDoLote, CobvSolicitada, DevedorCobv, FiltroLocs,
    LoteCobvRevisado, LoteCobvSolicitado, PeriodoPix, StatusCobvLote, TipoCob, Txid,
    ValorCobvRevisada,
};
use inter_pj::{Environment, Error};
use serde_json::json;
use wiremock::matchers::{any, body_json, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const UM: &str = "fb2761260e554ad593c7226beb5cb650";
const DOIS: &str = "7978c0c97ea847e78e8849634473c1f1";

async fn setup(scope: &str) -> MockServer {
    let server = MockServer::start().await;
    mount_token(&server, "tok", scope, 1).await;
    server
}

fn periodo() -> PeriodoPix {
    PeriodoPix::new(
        DateTime::parse_from_rfc3339("2026-09-01T00:00:00-03:00").unwrap(),
        DateTime::parse_from_rfc3339("2026-09-30T23:59:59-03:00").unwrap(),
    )
    .unwrap()
}

#[tokio::test]
async fn creates_lists_looks_up_and_unlinks_locations() {
    let server = setup("payloadlocation.write payloadlocation.read").await;
    let criada = json!({"id": 7716, "location": "pix.example.com/qr/v2/2353c790eefb11eaadc10242ac120002", "tipoCob": "cob", "criacao": "2026-09-23T21:19:51.013Z"});
    Mock::given(method("POST"))
        .and(path("/pix/v2/loc"))
        .and(body_json(json!({"tipoCob": "cob"})))
        .respond_with(ResponseTemplate::new(201).set_body_json(criada.clone()))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/pix/v2/loc"))
        .and(query_param("txIdPresente", "false"))
        .and(query_param("tipoCob", "cob"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "parametros": {"inicio": "2026-09-01T03:00:00Z", "fim": "2026-10-01T02:59:59Z", "paginacao": {"paginaAtual": 0, "itensPorPagina": 1000, "quantidadeDePaginas": 1, "quantidadeTotalDeItens": 1}},
            "loc": [criada]
        })))
        .expect(1)
        .mount(&server)
        .await;
    let vinculada = json!({"id": 7716, "txid": DOIS, "location": "pix.example.com/qr/v2/2353c790eefb11eaadc10242ac120002", "tipoCob": "cob", "criacao": "2026-09-23T21:19:51.013Z"});
    Mock::given(method("GET"))
        .and(path("/pix/v2/loc/7716"))
        .respond_with(ResponseTemplate::new(200).set_body_json(vinculada))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/pix/v2/loc/7716/txid"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": 7716, "location": "pix.example.com/qr/v2/2353c790eefb11eaadc10242ac120002", "tipoCob": "cob"})))
        .expect(1)
        .mount(&server)
        .await;

    let pix = client(&server);
    let loc = pix.pix().criar_loc(&TipoCob::Cob).await.unwrap();
    assert_eq!(loc.id, Some(7716));
    let mut filtro = FiltroLocs::new(periodo());
    filtro.tx_id_presente = Some(false);
    filtro.tipo_cob = Some(TipoCob::Cob);
    assert_eq!(pix.pix().listar_todas_locs(&filtro).await.unwrap().len(), 1);
    assert_eq!(
        pix.pix().consultar_loc(7716).await.unwrap().txid.as_deref(),
        Some(DOIS)
    );
    assert_eq!(pix.pix().desvincular_loc(7716).await.unwrap().txid, None);
}

fn cobv(nome: &str) -> CobvSolicitada {
    let mut cobv = CobvSolicitada::new(
        "7c084cd4-54af-4172-a516-a7d1a12b75cc".parse().unwrap(),
        "100.00".parse().unwrap(),
        NaiveDate::from_ymd_opt(2026, 12, 31).unwrap(),
        DevedorCobv::new("123.456.789-09".parse().unwrap(), nome),
    );
    cobv.solicitacao_pagador = Some("Informar matrícula".to_owned());
    cobv
}

#[tokio::test]
async fn creates_revises_and_follows_a_batch() {
    let server = setup("lotecobv.write lotecobv.read").await;
    Mock::given(method("PUT"))
        .and(path("/pix/v2/lotecobv/13"))
        .and(body_json(json!({
            "descricao": "Mensalidades de outubro",
            "cobsv": [
                {"txid": UM, "calendario": {"dataDeVencimento": "2026-12-31"}, "devedor": {"cpf": "12345678909", "nome": "João Souza"}, "valor": {"original": "100.00"}, "chave": "7c084cd4-54af-4172-a516-a7d1a12b75cc", "solicitacaoPagador": "Informar matrícula"},
                {"txid": DOIS, "calendario": {"dataDeVencimento": "2026-12-31"}, "devedor": {"cpf": "12345678909", "nome": "Manoel Silva"}, "valor": {"original": "100.00"}, "chave": "7c084cd4-54af-4172-a516-a7d1a12b75cc", "solicitacaoPagador": "Informar matrícula"}
            ]
        })))
        .respond_with(ResponseTemplate::new(202))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path("/pix/v2/lotecobv/13"))
        .and(body_json(json!({"cobsv": [{"txid": UM, "calendario": {"dataDeVencimento": "2027-01-10"}, "valor": {"original": "110.00"}}]})))
        .respond_with(ResponseTemplate::new(202))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/pix/v2/lotecobv/13"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 13,
            "descricao": "Mensalidades de outubro",
            "criacao": "2026-09-23T20:15:00.358Z",
            "cobsv": [
                {"criacao": "2026-09-23T20:15:00.358Z", "txid": UM, "status": "CRIADA"},
                {"txid": DOIS, "status": "NEGADA", "problema": {"type": "https://pix.bcb.gov.br/api/v2/error/CobVOperacaoInvalida", "title": "Cobrança inválida.", "status": 400}}
            ]
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/pix/v2/lotecobv/13/sumario"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "dataCriacaoProcessamento": "2026-09-23T20:15:00.358Z", "statusProcessamento": "CONCLUIDO",
            "totalCobrancas": 2, "totalCobrancasNegadas": 1, "totalCobrancasCriadas": 1
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/pix/v2/lotecobv/13/situacao/NEGADA"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"id": 13, "status": "NEGADA", "cobsv": [{"txid": DOIS}]})),
        )
        .expect(1)
        .mount(&server)
        .await;

    let pix = client(&server);
    let um: Txid = UM.parse().unwrap();
    let lote = LoteCobvSolicitado::new(
        "Mensalidades de outubro",
        vec![
            CobvDoLote::new(um.clone(), cobv("João Souza")),
            CobvDoLote::new(DOIS.parse().unwrap(), cobv("Manoel Silva")),
        ],
    );
    pix.pix().criar_lote_cobv(13, &lote).await.unwrap();

    let mut revisao = CobvRevisada::new();
    revisao.calendario = Some(inter_pj::pix::CalendarioCobv::new(
        NaiveDate::from_ymd_opt(2027, 1, 10).unwrap(),
    ));
    let mut valor = ValorCobvRevisada::default();
    valor.original = Some("110.00".parse().unwrap());
    revisao.valor = Some(valor);
    let revisao = LoteCobvRevisado::new(vec![CobvRevisadaDoLote::new(um, revisao)]);
    pix.pix().revisar_lote_cobv(13, &revisao).await.unwrap();

    let consultado = pix.pix().consultar_lote_cobv(13).await.unwrap();
    assert_eq!(consultado.cobsv[0].status, Some(StatusCobvLote::Criada));
    assert_eq!(consultado.cobsv[1].status, Some(StatusCobvLote::Negada));
    let sumario = pix.pix().sumario_lote_cobv(13).await.unwrap();
    assert_eq!(sumario.total_cobrancas_negadas, Some(1));
    let negadas = pix
        .pix()
        .consultar_lote_cobv_por_situacao(13, &StatusCobvLote::Negada)
        .await
        .unwrap();
    assert_eq!(negadas.cobsv.len(), 1);
}

#[tokio::test]
async fn lists_the_batches_of_a_period() {
    let server = setup("lotecobv.read").await;
    Mock::given(method("GET"))
        .and(path("/pix/v2/lotecobv"))
        .and(query_param("inicio", "2026-09-01T00:00:00-03:00"))
        .and(query_param("paginacao.paginaAtual", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "parametros": {"inicio": "2026-09-01T03:00:00Z", "fim": "2026-10-01T02:59:59Z", "paginacao": {"paginaAtual": 0, "itensPorPagina": 1000, "quantidadeDePaginas": 1, "quantidadeTotalDeItens": 2}},
            "lotes": [{"id": 13, "descricao": "Mensalidades", "cobsv": []}, {"id": 14, "descricao": "Anuidades", "cobsv": []}]
        })))
        .expect(1)
        .mount(&server)
        .await;
    let lotes = client(&server)
        .pix()
        .listar_todos_lotes_cobv(&periodo())
        .await
        .unwrap();
    assert_eq!(
        lotes.iter().filter_map(|lote| lote.id).collect::<Vec<_>>(),
        [13, 14]
    );
}

#[tokio::test]
async fn invalid_batches_are_not_sent() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let um: Txid = UM.parse().unwrap();
    let repetido = LoteCobvSolicitado::new(
        "Mensalidades",
        vec![
            CobvDoLote::new(um.clone(), cobv("A")),
            CobvDoLote::new(um, cobv("B")),
        ],
    );
    let pix = client(&server);
    let err = pix.pix().criar_lote_cobv(13, &repetido).await.unwrap_err();
    assert!(matches!(err, Error::InvalidInput(_)), "{err}");
    let err = pix
        .pix()
        .consultar_lote_cobv_por_situacao(13, &StatusCobvLote::from("OUTRA"))
        .await
        .unwrap_err();
    assert!(matches!(err, Error::InvalidInput(_)), "{err}");
}

#[tokio::test]
async fn pays_charges_in_the_sandbox() {
    let server = setup("pix.write").await;
    Mock::given(method("POST"))
        .and(path(format!("/pix/v2/cob/pagar/{DOIS}")))
        .and(body_json(json!({"valor": 150})))
        .respond_with(
            ResponseTemplate::new(201)
                .set_body_json(json!({"e2e": "E00416968202609231552CmNRIqASznP"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/pix/v2/cobv/pagar/{UM}")))
        .and(body_json(json!({"valor": 100.5})))
        .respond_with(
            ResponseTemplate::new(201)
                .set_body_json(json!({"e2e": "E00416968202609231553CmNRIqASznQ"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/pix/v2/sandbox/cob/pagamento"))
        .and(body_json(json!({"qrCode": "000201010212", "valor": 37})))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"endToEnd": "E00416968202609231554CmNRIqASznR"})),
        )
        .expect(1)
        .mount(&server)
        .await;

    let pix = builder(&server)
        .environment(Environment::Sandbox)
        .build()
        .unwrap();
    let cob = pix
        .pix()
        .pagar_cob_no_sandbox(&DOIS.parse().unwrap(), "150.00".parse().unwrap())
        .await
        .unwrap();
    assert_eq!(
        cob.end_to_end_id(),
        Some("E00416968202609231552CmNRIqASznP")
    );
    let cobv = pix
        .pix()
        .pagar_cobv_no_sandbox(&UM.parse().unwrap(), "100.50".parse().unwrap())
        .await
        .unwrap();
    assert!(cobv.end_to_end_id().is_some());
    let qr = pix
        .pix()
        .pagar_copia_e_cola_no_sandbox(" 000201010212 ", "37".parse().unwrap())
        .await
        .unwrap();
    assert_eq!(qr.end_to_end_id(), Some("E00416968202609231554CmNRIqASznR"));
}

/// Outside the sandbox, the payments are refused before any request.
#[tokio::test]
async fn sandbox_payments_are_refused_elsewhere() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    for pix in [
        builder(&server).build().unwrap(),
        builder(&server)
            .environment(Environment::Production)
            .build()
            .unwrap(),
    ] {
        let txid: Txid = DOIS.parse().unwrap();
        let err = pix
            .pix()
            .pagar_cob_no_sandbox(&txid, "1".parse().unwrap())
            .await
            .unwrap_err();
        assert!(matches!(err, Error::InvalidInput(_)), "{err}");
        let err = pix
            .pix()
            .pagar_copia_e_cola_no_sandbox("000201", "1".parse().unwrap())
            .await
            .unwrap_err();
        assert!(matches!(err, Error::InvalidInput(_)), "{err}");
    }
    let sandbox = builder(&server)
        .environment(Environment::Sandbox)
        .build()
        .unwrap();
    let err = sandbox
        .pix()
        .pagar_cobv_no_sandbox(&UM.parse().unwrap(), "0".parse().unwrap())
        .await
        .unwrap_err();
    assert!(matches!(err, Error::InvalidInput(_)), "{err}");
}
