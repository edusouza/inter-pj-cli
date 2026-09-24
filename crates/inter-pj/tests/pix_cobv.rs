//! Pix charges with a due date (`/pix/v2/cobv`) against a mock API. All data
//! is synthetic.

mod common;

use chrono::{DateTime, NaiveDate};
use common::{client, mount_token};
use inter_pj::Error;
use inter_pj::cobranca::Uf;
use inter_pj::pix::{
    CobvRevisada, CobvSolicitada, DescontoCobv, DescontoData, DevedorCobv, FiltroCobvs, JurosCobv,
    ModalidadeJuros, MultaCobv, PeriodoPix, StatusCob, Txid, ValorCobvRevisada,
};
use serde_json::{Value, json};
use wiremock::matchers::{any, body_json, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TXID: &str = "7978c0c97ea847e78e8849634473c1f1";

fn txid() -> Txid {
    TXID.parse().unwrap()
}

fn dia(ano: i32, mes: u32, dia: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(ano, mes, dia).unwrap()
}

fn cobv() -> CobvSolicitada {
    let mut devedor = DevedorCobv::new("123.456.789-09".parse().unwrap(), "Francisco da Silva");
    devedor.logradouro = Some("Alameda Souza, Numero 80, Bairro Braz".to_owned());
    devedor.cidade = Some("Recife".to_owned());
    devedor.uf = Some(Uf::Pe);
    devedor.cep = Some("70011750".to_owned());
    let mut cobv = CobvSolicitada::new(
        "5f84a4c5-c5cb-4599-9f13-7eb4d419dacc".parse().unwrap(),
        "123.45".parse().unwrap(),
        dia(2026, 12, 31),
        devedor,
    );
    cobv.calendario.validade_apos_vencimento = Some(30);
    cobv.valor.multa = Some(MultaCobv::Percentual("15".parse().unwrap()));
    cobv.valor.juros = Some(JurosCobv::new(
        ModalidadeJuros::PercentualDiaDiasCorridos,
        "2".parse().unwrap(),
    ));
    cobv.valor.desconto = Some(DescontoCobv::ValorFixoAteDatas(vec![DescontoData::new(
        dia(2026, 11, 30),
        "30".parse().unwrap(),
    )]));
    cobv.solicitacao_pagador = Some("Cobrança dos serviços prestados.".to_owned());
    cobv
}

fn gerada() -> Value {
    json!({
        "calendario": {"criacao": "2026-09-23T20:15:00.358Z", "dataDeVencimento": "2026-12-31", "validadeAposVencimento": 30},
        "txid": TXID,
        "revisao": 0,
        "loc": {"id": 789, "location": "pix.example.com/qr/v2/cobv/9d36b84fc70b478fb95c12729b90ca25", "tipoCob": "cobv"},
        "status": "ATIVA",
        "devedor": {"logradouro": "Alameda Souza, Numero 80, Bairro Braz", "cidade": "Recife", "uf": "PE", "cep": "70011750", "cpf": "12345678909", "nome": "Francisco da Silva"},
        "recebedor": {"logradouro": "Rua 15 Numero 1200, Bairro São Luiz", "cidade": "São Paulo", "uf": "SP", "cep": "70800100", "cnpj": "12345678000195", "nome": "Empresa de Logística SA"},
        "valor": {
            "original": "123.45",
            "multa": {"modalidade": 2, "valorPerc": "15.00"},
            "juros": {"modalidade": 2, "valorPerc": "2.00"},
            "desconto": {"modalidade": 1, "descontoDataFixa": [{"data": "2026-11-30", "valorPerc": "30.00"}]}
        },
        "chave": "5f84a4c5-c5cb-4599-9f13-7eb4d419dacc",
        "solicitacaoPagador": "Cobrança dos serviços prestados."
    })
}

async fn setup(scope: &str) -> MockServer {
    let server = MockServer::start().await;
    mount_token(&server, "tok", scope, 1).await;
    server
}

#[tokio::test]
async fn creates_a_charge_with_a_due_date() {
    let server = setup("cobv.write").await;
    Mock::given(method("PUT"))
        .and(path(format!("/pix/v2/cobv/{TXID}")))
        .and(body_json(json!({
            "calendario": {"dataDeVencimento": "2026-12-31", "validadeAposVencimento": 30},
            "devedor": {"logradouro": "Alameda Souza, Numero 80, Bairro Braz", "cidade": "Recife", "uf": "PE", "cep": "70011750", "cpf": "12345678909", "nome": "Francisco da Silva"},
            "valor": {
                "original": "123.45",
                "multa": {"modalidade": 2, "valorPerc": "15.00"},
                "juros": {"modalidade": 2, "valorPerc": "2.00"},
                "desconto": {"modalidade": 1, "descontoDataFixa": [{"data": "2026-11-30", "valorPerc": "30.00"}]}
            },
            "chave": "5f84a4c5-c5cb-4599-9f13-7eb4d419dacc",
            "solicitacaoPagador": "Cobrança dos serviços prestados."
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(gerada()))
        .expect(1)
        .mount(&server)
        .await;

    let criada = client(&server)
        .pix()
        .criar_cobv(&txid(), &cobv())
        .await
        .unwrap();
    assert_eq!(criada.status, Some(StatusCob::Ativa));
    assert_eq!(
        criada.recebedor.and_then(|recebedor| recebedor.nome),
        Some("Empresa de Logística SA".to_owned())
    );
    assert_eq!(
        criada
            .calendario
            .and_then(|calendario| calendario.data_de_vencimento),
        Some("2026-12-31".to_owned())
    );
}

#[tokio::test]
async fn invalid_charges_are_not_sent() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let mut invalida = cobv();
    invalida.valor.desconto = Some(DescontoCobv::ValorFixoAteDatas(vec![DescontoData::new(
        dia(2027, 1, 10),
        "30".parse().unwrap(),
    )]));
    let err = client(&server)
        .pix()
        .criar_cobv(&txid(), &invalida)
        .await
        .unwrap_err();
    assert!(matches!(err, Error::InvalidInput(_)), "{err}");
}

#[tokio::test]
async fn revises_removes_and_looks_up_a_charge() {
    let server = setup("cobv.write cobv.read").await;
    let cobv_path = format!("/pix/v2/cobv/{TXID}");
    Mock::given(method("PATCH"))
        .and(path(cobv_path.clone()))
        .and(body_json(json!({"valor": {"original": "567.89"}, "solicitacaoPagador": "Informar cartão fidelidade"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"txid": TXID, "revisao": 1, "status": "ATIVA"})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path(cobv_path.clone()))
        .and(body_json(
            json!({"status": "REMOVIDA_PELO_USUARIO_RECEBEDOR"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"txid": TXID, "revisao": 2, "status": "REMOVIDA_PELO_USUARIO_RECEBEDOR"}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(cobv_path))
        .respond_with(ResponseTemplate::new(200).set_body_json(gerada()))
        .expect(1)
        .mount(&server)
        .await;

    let pix = client(&server);
    let mut valor = ValorCobvRevisada::default();
    valor.original = Some("567.89".parse().unwrap());
    let mut revisao = CobvRevisada::new();
    revisao.valor = Some(valor);
    revisao.solicitacao_pagador = Some("Informar cartão fidelidade".to_owned());
    assert_eq!(
        pix.pix()
            .revisar_cobv(&txid(), &revisao)
            .await
            .unwrap()
            .revisao,
        Some(1)
    );
    let removida = pix
        .pix()
        .revisar_cobv(&txid(), &CobvRevisada::remocao())
        .await
        .unwrap();
    assert_eq!(
        removida.status,
        Some(StatusCob::RemovidaPeloUsuarioRecebedor)
    );
    let cobv = pix.pix().consultar_cobv(&txid()).await.unwrap();
    // Nothing is lost on the way back.
    assert_eq!(serde_json::to_value(&cobv).unwrap(), gerada());
}

#[tokio::test]
async fn lists_the_charges_of_a_batch() {
    let server = setup("cobv.read").await;
    Mock::given(method("GET"))
        .and(path("/pix/v2/cobv"))
        .and(query_param("inicio", "2026-09-01T00:00:00-03:00"))
        .and(query_param("fim", "2026-09-30T23:59:59-03:00"))
        .and(query_param("loteCobVId", "7"))
        .and(query_param("cpf", "12345678909"))
        .and(query_param("paginacao.paginaAtual", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "parametros": {
                "inicio": "2026-09-01T03:00:00Z",
                "fim": "2026-10-01T02:59:59Z",
                "paginacao": {"paginaAtual": 0, "itensPorPagina": 1000, "quantidadeDePaginas": 1, "quantidadeTotalDeItens": 1}
            },
            "cobs": [gerada()]
        })))
        .expect(1)
        .mount(&server)
        .await;
    let periodo = PeriodoPix::new(
        DateTime::parse_from_rfc3339("2026-09-01T00:00:00-03:00").unwrap(),
        DateTime::parse_from_rfc3339("2026-09-30T23:59:59-03:00").unwrap(),
    )
    .unwrap();
    let mut filtro = FiltroCobvs::new(periodo);
    filtro.lote_cob_v_id = Some(7);
    filtro.devedor = Some("123.456.789-09".parse().unwrap());
    let cobvs = client(&server)
        .pix()
        .listar_todas_cobvs(&filtro)
        .await
        .unwrap();
    assert_eq!(cobvs.len(), 1);
    assert_eq!(cobvs[0].txid.as_deref(), Some(TXID));
}
