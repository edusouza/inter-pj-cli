//! Confirmation requests (`/pix/v2/solicrec`) and locations of recurrences
//! (`/pix/v2/locrec`) of Pix Automático against a mock API. All data is
//! synthetic.

mod common;

use chrono::DateTime;
use common::{client, mount_token};
use inter_pj::Error;
use inter_pj::pix::{CobrancaPixError, PeriodoPix};
use inter_pj::pix_automatico::{
    DestinatarioSolicRec, FiltroLocsRec, IdSolicRec, SolicRecSolicitada, StatusRec, StatusSolicRec,
};
use serde_json::{Value, json};
use wiremock::matchers::{any, body_json, body_string, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ID_REC: &str = "RR1234567820260924abcdefghijk";
const ID_SOLIC: &str = "SC1234567820260924abcdefghijk";

fn id_solic() -> IdSolicRec {
    ID_SOLIC.parse().unwrap()
}

fn solicitacao() -> SolicRecSolicitada {
    let mut destinatario =
        DestinatarioSolicRec::new("123.456.789-09".parse().unwrap(), "1234567", "12345678");
    destinatario.agencia = Some("0001".to_owned());
    SolicRecSolicitada::new(
        ID_REC.parse().unwrap(),
        DateTime::parse_from_rfc3339("2026-10-01T23:59:59-03:00").unwrap(),
        destinatario,
    )
}

fn gerada(status: &str) -> Value {
    json!({
        "idSolicRec": ID_SOLIC,
        "idRec": ID_REC,
        "calendario": {"dataExpiracaoSolicitacao": "2026-10-02T02:59:59.000Z"},
        "status": status,
        "destinatario": {"cpf": "12345678909", "conta": "1234567", "ispbParticipante": "12345678", "agencia": "0001"},
        "atualizacao": [{"status": "CRIADA", "data": "2026-09-24T12:00:00.000Z"}],
        "recPayload": {
            "idRec": ID_REC,
            "vinculo": {"contrato": "contrato-001", "devedor": {"cpf": "12345678909", "nome": "Cliente Exemplo"}},
            "calendario": {"dataInicial": "2026-10-10", "periodicidade": "MENSAL"},
            "recebedor": {"cnpj": "12345678000195", "nome": "Empresa Exemplo Ltda", "ispbParticipante": "00416968"},
            "politicaRetentativa": "NAO_PERMITE",
            "atualizacao": [{"status": "CRIADA", "data": "2026-09-24T11:00:00.000Z"}]
        }
    })
}

async fn setup(scope: &str) -> MockServer {
    let server = MockServer::start().await;
    mount_token(&server, "tok", scope, 1).await;
    server
}

#[tokio::test]
async fn asks_the_payer_to_approve_a_recurrence() {
    let server = setup("solicrec.write").await;
    Mock::given(method("POST"))
        .and(path("/pix/v2/solicrec"))
        .and(body_json(json!({
            "idRec": ID_REC,
            "calendario": {"dataExpiracaoSolicitacao": "2026-10-01T23:59:59-03:00"},
            "destinatario": {"cpf": "12345678909", "conta": "1234567", "ispbParticipante": "12345678", "agencia": "0001"}
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(gerada("CRIADA")))
        .expect(1)
        .mount(&server)
        .await;
    let criada = client(&server)
        .pix_automatico()
        .criar_solicitacao(&solicitacao())
        .await
        .unwrap();
    assert_eq!(criada.id_solic_rec.as_deref(), Some(ID_SOLIC));
    assert_eq!(criada.status, Some(StatusSolicRec::Criada));
    let rec = criada.rec_payload.unwrap();
    assert_eq!(rec.atualizacao[0].status, Some(StatusRec::Criada));
    assert_eq!(
        rec.recebedor.unwrap().ispb_participante.as_deref(),
        Some("00416968")
    );
}

#[tokio::test]
async fn looks_a_request_up_and_cancels_it() {
    // One token covers both operations.
    let server = setup("solicrec.read solicrec.write").await;
    Mock::given(method("GET"))
        .and(path(format!("/pix/v2/solicrec/{ID_SOLIC}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(gerada("RECEBIDA")))
        .expect(1)
        .mount(&server)
        .await;
    let mut cancelada = gerada("CANCELADA");
    cancelada["atualizacao"]
        .as_array_mut()
        .unwrap()
        .push(json!({"status": "CANCELADA", "data": "2026-09-24T13:00:00.000Z"}));
    Mock::given(method("PATCH"))
        .and(path(format!("/pix/v2/solicrec/{ID_SOLIC}")))
        .and(body_json(json!({"status": "CANCELADA"})))
        .respond_with(ResponseTemplate::new(201).set_body_json(cancelada))
        .expect(1)
        .mount(&server)
        .await;

    let pix = client(&server);
    let consultada = pix
        .pix_automatico()
        .consultar_solicitacao(&id_solic())
        .await
        .unwrap();
    assert_eq!(consultada.status, Some(StatusSolicRec::Recebida));
    let cancelada = pix
        .pix_automatico()
        .cancelar_solicitacao(&id_solic())
        .await
        .unwrap();
    assert_eq!(cancelada.status, Some(StatusSolicRec::Cancelada));
    assert_eq!(cancelada.atualizacao.len(), 2);
}

#[tokio::test]
async fn an_invalid_request_is_not_sent() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let mut invalida = solicitacao();
    invalida.destinatario.ispb_participante = "416968".to_owned();
    let err = client(&server)
        .pix_automatico()
        .criar_solicitacao(&invalida)
        .await
        .unwrap_err();
    let Error::InvalidInput(source) = err else {
        panic!("{err}")
    };
    let campo = source.downcast_ref::<CobrancaPixError>().unwrap().campo();
    assert_eq!(campo, "destinatario.ispbParticipante");
}

/// A request that cannot be cancelled any more comes back as the API
/// explains it.
#[tokio::test]
async fn a_refused_cancellation_keeps_the_reason() {
    let server = setup("solicrec.write").await;
    Mock::given(method("PATCH"))
        .and(path(format!("/pix/v2/solicrec/{ID_SOLIC}")))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "type": "https://pix.bcb.gov.br/api/v2/error/SolicRecOperacaoInvalida",
            "title": "Operação inválida.",
            "status": 400,
            "detail": "Não é possível cancelar uma solicitação de recorrência com o status diferente de CRIADA ou RECEBIDA."
        })))
        .expect(1)
        .mount(&server)
        .await;
    let err = client(&server)
        .pix_automatico()
        .cancelar_solicitacao(&id_solic())
        .await
        .unwrap_err();
    let Error::Api(api) = err else {
        panic!("{err}")
    };
    assert_eq!(api.status, 400);
    assert!(
        api.problem
            .unwrap()
            .detail
            .unwrap()
            .contains("CRIADA ou RECEBIDA")
    );
}

fn loc(id: u64, id_rec: Option<&str>) -> Value {
    let mut loc = json!({
        "id": id,
        "location": format!("pix.example.com/qr/v2/rec/{id:032x}"),
        "criacao": "2026-09-24T12:00:00.000Z"
    });
    if let Some(id_rec) = id_rec {
        loc["idRec"] = json!(id_rec);
    }
    loc
}

#[tokio::test]
async fn creates_a_location_without_a_body() {
    let server = setup("payloadlocationrec.write").await;
    Mock::given(method("POST"))
        .and(path("/pix/v2/locrec"))
        .and(body_string(""))
        .respond_with(ResponseTemplate::new(201).set_body_json(loc(12, None)))
        .expect(1)
        .mount(&server)
        .await;
    let criada = client(&server)
        .pix_automatico()
        .criar_locrec()
        .await
        .unwrap();
    assert_eq!(criada.id, Some(12));
    assert!(
        criada
            .location
            .unwrap()
            .starts_with("pix.example.com/qr/v2/rec/")
    );
    assert_eq!(criada.id_rec, None);
}

fn periodo() -> PeriodoPix {
    PeriodoPix::new(
        DateTime::parse_from_rfc3339("2026-09-01T00:00:00-03:00").unwrap(),
        DateTime::parse_from_rfc3339("2026-09-30T23:59:59-03:00").unwrap(),
    )
    .unwrap()
}

#[tokio::test]
async fn lists_every_location_of_the_period() {
    let server = setup("payloadlocationrec.read").await;
    for (numero, locs) in [
        (0, vec![loc(1, Some(ID_REC)), loc(2, None)]),
        (1, vec![loc(3, None)]),
    ] {
        Mock::given(method("GET"))
            .and(path("/pix/v2/locrec"))
            .and(query_param("inicio", "2026-09-01T00:00:00-03:00"))
            .and(query_param("fim", "2026-09-30T23:59:59-03:00"))
            .and(query_param("idRecPresente", "false"))
            .and(query_param(
                "paginacao.paginaAtual",
                numero.to_string().as_str(),
            ))
            .and(query_param("paginacao.itensPorPagina", "1000"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "parametros": {
                    "inicio": "2026-09-01T03:00:00Z",
                    "fim": "2026-10-01T02:59:59Z",
                    "idRecPresente": false,
                    "paginacao": {"paginaAtual": numero, "itensPorPagina": 1000, "quantidadeDePaginas": 2, "quantidadeTotalDeItens": 3}
                },
                "loc": locs
            })))
            .expect(1)
            .mount(&server)
            .await;
    }
    let mut filtro = FiltroLocsRec::new(periodo());
    filtro.id_rec_presente = Some(false);
    let locs = client(&server)
        .pix_automatico()
        .listar_todas_locrecs(&filtro)
        .await
        .unwrap();
    assert_eq!(
        locs.iter().map(|loc| loc.id.unwrap()).collect::<Vec<_>>(),
        [1, 2, 3]
    );
}

#[tokio::test]
async fn looks_a_location_up_and_unlinks_its_recurrence() {
    let server = setup("payloadlocationrec.read payloadlocationrec.write").await;
    Mock::given(method("GET"))
        .and(path("/pix/v2/locrec/12"))
        .respond_with(ResponseTemplate::new(200).set_body_json(loc(12, Some(ID_REC))))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/pix/v2/locrec/12/idRec"))
        .respond_with(ResponseTemplate::new(200).set_body_json(loc(12, None)))
        .expect(1)
        .mount(&server)
        .await;

    let pix = client(&server);
    let consultada = pix.pix_automatico().consultar_locrec(12).await.unwrap();
    assert_eq!(consultada.id_rec.as_deref(), Some(ID_REC));
    let livre = pix.pix_automatico().desvincular_locrec(12).await.unwrap();
    assert_eq!(livre.id_rec, None);
}

#[tokio::test]
async fn nothing_invalid_is_listed() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let pix = client(&server);
    let mut filtro = FiltroLocsRec::new(periodo());
    filtro.convenio = Some("x".repeat(61));
    let err = pix
        .pix_automatico()
        .listar_locrecs(&filtro, 0, None)
        .await
        .unwrap_err();
    assert!(matches!(err, Error::InvalidInput(_)), "{err}");
    let filtro = FiltroLocsRec::new(periodo());
    let err = pix
        .pix_automatico()
        .listar_locrecs(&filtro, 0, Some(1001))
        .await
        .unwrap_err();
    assert!(matches!(err, Error::InvalidInput(_)), "{err}");
}
