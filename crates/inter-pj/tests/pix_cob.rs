//! Immediate Pix charges (`/pix/v2/cob`) against a mock API. All data is
//! synthetic.

mod common;

use chrono::DateTime;
use common::{client, mount_token};
use inter_pj::Error;
use inter_pj::pix::{
    CobRevisada, CobSolicitada, CobrancaPixError, Devedor, FiltroCobs, InfoAdicional, PeriodoPix,
    StatusCob, Txid, ValorCobRevisada,
};
use serde_json::{Value, json};
use wiremock::matchers::{
    any, body_json, header, method, path, query_param, query_param_is_missing,
};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TXID: &str = "7978c0c97ea847e78e8849634473c1f1";
const CHAVE: &str = "7d9f0335-8dcc-4054-9bf9-0dbd61d36906";

fn txid() -> Txid {
    TXID.parse().unwrap()
}

fn cob() -> CobSolicitada {
    let mut cob = CobSolicitada::new(CHAVE.parse().unwrap(), "37.00".parse().unwrap());
    cob.calendario.expiracao = Some(3600);
    cob.devedor = Some(Devedor::new(
        "12.345.678/0001-95".parse().unwrap(),
        "Empresa de Serviços SA",
    ));
    cob.valor.modalidade_alteracao = true;
    cob.solicitacao_pagador = Some("Serviço realizado.".to_owned());
    cob.info_adicionais = vec![
        InfoAdicional::new("Campo 1", "Informação Adicional1 do PSP-Recebedor"),
        InfoAdicional::new("Campo 2", "Informação Adicional2 do PSP-Recebedor"),
    ];
    cob
}

/// The charge as the documentation's example (`cobBody2`) sends it.
fn corpo() -> Value {
    json!({
        "calendario": {"expiracao": 3600},
        "devedor": {"cnpj": "12345678000195", "nome": "Empresa de Serviços SA"},
        "valor": {"original": "37.00", "modalidadeAlteracao": 1},
        "chave": CHAVE,
        "solicitacaoPagador": "Serviço realizado.",
        "infoAdicionais": [
            {"nome": "Campo 1", "valor": "Informação Adicional1 do PSP-Recebedor"},
            {"nome": "Campo 2", "valor": "Informação Adicional2 do PSP-Recebedor"}
        ]
    })
}

fn gerada(status: &str) -> Value {
    json!({
        "calendario": {"criacao": "2026-09-23T20:15:00.358Z", "expiracao": 3600},
        "txid": TXID,
        "revisao": 0,
        "loc": {"id": 789, "location": "pix.example.com/qr/9d36b84fc70b478fb95c12729b90ca25", "tipoCob": "cob"},
        "location": "pix.example.com/qr/9d36b84fc70b478fb95c12729b90ca25",
        "status": status,
        "devedor": {"cnpj": "12345678000195", "nome": "Empresa de Serviços SA"},
        "valor": {"original": "37.00", "modalidadeAlteracao": 1},
        "chave": CHAVE,
        "solicitacaoPagador": "Serviço realizado.",
        "pixCopiaECola": "00020101021226830014BR.GOV.BCB.PIX2561pix.example.com/qr/9d36b84fc70b478fb95c12729b90ca255204000053039865802BR5913EMPRESA EXEMPLO6009SAO PAULO62070503***6304ABCD"
    })
}

async fn setup(scope: &str) -> MockServer {
    let server = MockServer::start().await;
    mount_token(&server, "tok", scope, 1).await;
    server
}

#[tokio::test]
async fn creates_a_charge_with_your_txid() {
    let server = setup("cob.write").await;
    Mock::given(method("PUT"))
        .and(path(format!("/pix/v2/cob/{TXID}")))
        .and(header("authorization", "Bearer tok"))
        .and(body_json(corpo()))
        .respond_with(ResponseTemplate::new(201).set_body_json(gerada("ATIVA")))
        .expect(1)
        .mount(&server)
        .await;

    let criada = client(&server)
        .pix()
        .criar_cob(&txid(), &cob())
        .await
        .unwrap();
    assert_eq!(criada.txid.as_deref(), Some(TXID));
    assert_eq!(criada.status, Some(StatusCob::Ativa));
    assert_eq!(criada.revisao, Some(0));
    assert_eq!(criada.loc.and_then(|loc| loc.id), Some(789));
    assert!(
        criada
            .pix_copia_e_cola
            .is_some_and(|texto| texto.starts_with("000201"))
    );
}

#[tokio::test]
async fn creates_a_charge_whose_txid_inter_chooses() {
    let server = setup("cob.write").await;
    Mock::given(method("POST"))
        .and(path("/pix/v2/cob"))
        .and(body_json(corpo()))
        .respond_with(ResponseTemplate::new(201).set_body_json(gerada("ATIVA")))
        .expect(1)
        .mount(&server)
        .await;
    let criada = client(&server)
        .pix()
        .criar_cob_sem_txid(&cob())
        .await
        .unwrap();
    assert_eq!(criada.txid.as_deref(), Some(TXID));
}

/// A creation is repeated only when it surely was not processed: after a
/// `503`, the caller decides (with the same txid, no second charge).
#[tokio::test]
async fn an_uncertain_creation_is_not_repeated() {
    let server = setup("cob.write").await;
    Mock::given(method("PUT"))
        .and(path(format!("/pix/v2/cob/{TXID}")))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&server)
        .await;
    let err = client(&server)
        .pix()
        .criar_cob(&txid(), &cob())
        .await
        .unwrap_err();
    assert!(
        matches!(err, Error::Api(ref api) if api.status == 503),
        "{err}"
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
    let mut invalida = cob();
    invalida.solicitacao_pagador = Some("x".repeat(141));
    let err = client(&server)
        .pix()
        .criar_cob(&txid(), &invalida)
        .await
        .unwrap_err();
    let Error::InvalidInput(source) = err else {
        panic!("{err}")
    };
    let campo = source.downcast_ref::<CobrancaPixError>().unwrap().campo();
    assert_eq!(campo, "solicitacaoPagador");
    let err = client(&server)
        .pix()
        .revisar_cob(&txid(), &CobRevisada::new())
        .await
        .unwrap_err();
    assert!(matches!(err, Error::InvalidInput(_)), "{err}");
}

#[tokio::test]
async fn revises_and_removes_a_charge() {
    let server = setup("cob.write").await;
    let cob_path = format!("/pix/v2/cob/{TXID}");
    Mock::given(method("PATCH"))
        .and(path(cob_path.clone()))
        .and(body_json(json!({"valor": {"original": "567.89"}, "solicitacaoPagador": "Informar cartão fidelidade"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"txid": TXID, "revisao": 1, "status": "ATIVA", "valor": {"original": "567.89"}})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path(cob_path))
        .and(body_json(
            json!({"status": "REMOVIDA_PELO_USUARIO_RECEBEDOR"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"txid": TXID, "revisao": 2, "status": "REMOVIDA_PELO_USUARIO_RECEBEDOR"}),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let pix = client(&server);
    let mut valor = ValorCobRevisada::default();
    valor.original = Some("567.89".parse().unwrap());
    let mut revisao = CobRevisada::new();
    revisao.valor = Some(valor);
    revisao.solicitacao_pagador = Some("Informar cartão fidelidade".to_owned());
    let revisada = pix.pix().revisar_cob(&txid(), &revisao).await.unwrap();
    assert_eq!(revisada.revisao, Some(1));
    assert_eq!(
        revisada.valor.and_then(|valor| valor.original),
        Some("567.89".parse().unwrap())
    );
    let removida = pix
        .pix()
        .revisar_cob(&txid(), &CobRevisada::remocao())
        .await
        .unwrap();
    assert_eq!(
        removida.status,
        Some(StatusCob::RemovidaPeloUsuarioRecebedor)
    );
}

#[tokio::test]
async fn looks_a_charge_up_with_the_pix_that_paid_it() {
    let server = setup("cob.read").await;
    let mut concluida = gerada("CONCLUIDA");
    concluida["pix"] = json!([{
        "endToEndId": "E12345678202609232015abcdef12345",
        "txid": TXID,
        "valor": "37.00",
        "horario": "2026-09-23T20:20:00.000Z",
        "infoPagador": "Obrigado"
    }]);
    Mock::given(method("GET"))
        .and(path(format!("/pix/v2/cob/{TXID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(concluida.clone()))
        .expect(1)
        .mount(&server)
        .await;
    let cob = client(&server).pix().consultar_cob(&txid()).await.unwrap();
    assert_eq!(cob.status, Some(StatusCob::Concluida));
    assert_eq!(cob.pix.len(), 1);
    assert_eq!(cob.pix[0].valor, Some("37.00".parse().unwrap()));
    // Nothing is lost on the way back.
    assert_eq!(serde_json::to_value(&cob).unwrap(), concluida);
}

fn periodo() -> PeriodoPix {
    PeriodoPix::new(
        DateTime::parse_from_rfc3339("2026-09-01T00:00:00-03:00").unwrap(),
        DateTime::parse_from_rfc3339("2026-09-30T23:59:59-03:00").unwrap(),
    )
    .unwrap()
}

fn pagina(numero: u64, paginas: u64, cobs: &[Value]) -> Value {
    json!({
        "parametros": {
            "inicio": "2026-09-01T03:00:00Z",
            "fim": "2026-10-01T02:59:59Z",
            "paginacao": {"paginaAtual": numero, "itensPorPagina": 1000, "quantidadeDePaginas": paginas, "quantidadeTotalDeItens": 3}
        },
        "cobs": cobs
    })
}

#[tokio::test]
async fn lists_every_page_of_the_period() {
    let server = setup("cob.read").await;
    for (numero, cobs) in [
        (0, vec![gerada("ATIVA"), gerada("CONCLUIDA")]),
        (1, vec![gerada("ATIVA")]),
    ] {
        Mock::given(method("GET"))
            .and(path("/pix/v2/cob"))
            .and(query_param("inicio", "2026-09-01T00:00:00-03:00"))
            .and(query_param("fim", "2026-09-30T23:59:59-03:00"))
            .and(query_param("cnpj", "12345678000195"))
            .and(query_param("status", "ATIVA"))
            .and(query_param_is_missing("cpf"))
            .and(query_param(
                "paginacao.paginaAtual",
                numero.to_string().as_str(),
            ))
            .and(query_param("paginacao.itensPorPagina", "1000"))
            .respond_with(ResponseTemplate::new(200).set_body_json(pagina(numero, 2, &cobs)))
            .expect(1)
            .mount(&server)
            .await;
    }
    let mut filtro = FiltroCobs::new(periodo());
    filtro.devedor = Some("12.345.678/0001-95".parse().unwrap());
    filtro.status = Some(StatusCob::Ativa);
    let cobs = client(&server)
        .pix()
        .listar_todas_cobs(&filtro)
        .await
        .unwrap();
    assert_eq!(cobs.len(), 3);
}

#[tokio::test]
async fn one_page_with_its_size() {
    let server = setup("cob.read").await;
    Mock::given(method("GET"))
        .and(path("/pix/v2/cob"))
        .and(query_param("paginacao.paginaAtual", "2"))
        .and(query_param("paginacao.itensPorPagina", "10"))
        .and(query_param("locationPresente", "false"))
        .respond_with(ResponseTemplate::new(200).set_body_json(pagina(2, 5, &[gerada("ATIVA")])))
        .expect(1)
        .mount(&server)
        .await;
    let mut filtro = FiltroCobs::new(periodo());
    filtro.location_presente = Some(false);
    let pagina = client(&server)
        .pix()
        .listar_cobs(&filtro, 2, Some(10))
        .await
        .unwrap();
    assert_eq!(pagina.cobs.len(), 1);
    let paginacao = pagina.parametros.paginacao.unwrap();
    assert_eq!(paginacao.quantidade_de_paginas, Some(5));
    assert!(paginacao.tem_mais(2, 1));

    // Page sizes out of range are not sent.
    for itens in [0, 1001] {
        let err = client(&server)
            .pix()
            .listar_cobs(&filtro, 0, Some(itens))
            .await
            .unwrap_err();
        assert!(matches!(err, Error::InvalidInput(_)), "{err}");
    }
}
