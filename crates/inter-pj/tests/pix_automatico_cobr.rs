//! Recurring charges of Pix Automático (`/pix/v2/cobr`) against a mock API.
//! All data is synthetic.

mod common;

use chrono::{DateTime, NaiveDate};
use common::{client, mount_token};
use inter_pj::Error;
use inter_pj::cobranca::Uf;
use inter_pj::pix::{CobrancaPixError, PeriodoPix, Txid};
use inter_pj::pix_automatico::{
    CobRSolicitada, ContaRecebedor, DevedorCobR, FiltroCobsR, StatusCobR, StatusTentativa,
    TipoContaRecebedor, TipoTentativa,
};
use serde_json::{Value, json};
use wiremock::matchers::{any, body_json, body_string, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ID_REC: &str = "RR1234567820260924abcdefghijk";
const TXID: &str = "7978c0c97ea847e78e8849634473c1f1";

fn txid() -> Txid {
    TXID.parse().unwrap()
}

fn dia(ano: i32, mes: u32, dia: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(ano, mes, dia).unwrap()
}

fn cobr() -> CobRSolicitada {
    let mut conta = ContaRecebedor::new("1234567", TipoContaRecebedor::Corrente);
    conta.agencia = Some("0001".to_owned());
    let mut cobr = CobRSolicitada::new(
        ID_REC.parse().unwrap(),
        dia(2026, 10, 10),
        "149.9".parse().unwrap(),
        conta,
    );
    cobr.info_adicional = Some("Mensalidade de outubro".to_owned());
    let mut devedor = DevedorCobR::default();
    devedor.email = Some("cliente@empresa.example".to_owned());
    devedor.uf = Some(Uf::Sp);
    devedor.cep = Some("01001000".to_owned());
    cobr.devedor = Some(devedor);
    cobr
}

fn corpo() -> Value {
    json!({
        "idRec": ID_REC,
        "infoAdicional": "Mensalidade de outubro",
        "calendario": {"dataDeVencimento": "2026-10-10"},
        "valor": {"original": "149.90"},
        "ajusteDiaUtil": true,
        "devedor": {"email": "cliente@empresa.example", "uf": "SP", "cep": "01001000"},
        "recebedor": {"conta": "1234567", "tipoConta": "CORRENTE", "agencia": "0001"}
    })
}

fn gerada(status: &str) -> Value {
    json!({
        "idRec": ID_REC,
        "txid": TXID,
        "infoAdicional": "Mensalidade de outubro",
        "calendario": {"criacao": "2026-09-24", "dataDeVencimento": "2026-10-10"},
        "valor": {"original": "149.90"},
        "status": status,
        "politicaRetentativa": "PERMITE_3R_7D",
        "ajusteDiaUtil": true,
        "devedor": {"email": "cliente@empresa.example", "uf": "SP", "cep": "01001000"},
        "recebedor": {"conta": "1234567", "tipoConta": "CORRENTE", "agencia": "0001"},
        "atualizacao": [{"status": "CRIADA", "data": "2026-09-24T12:00:00.000Z"}]
    })
}

async fn setup(scope: &str) -> MockServer {
    let server = MockServer::start().await;
    mount_token(&server, "tok", scope, 1).await;
    server
}

#[tokio::test]
async fn creates_a_recurring_charge_with_your_txid() {
    let server = setup("cobr.write").await;
    Mock::given(method("PUT"))
        .and(path(format!("/pix/v2/cobr/{TXID}")))
        .and(body_json(corpo()))
        .respond_with(ResponseTemplate::new(201).set_body_json(gerada("CRIADA")))
        .expect(1)
        .mount(&server)
        .await;
    let criada = client(&server)
        .pix_automatico()
        .criar_cobr(&txid(), &cobr())
        .await
        .unwrap();
    assert_eq!(criada.txid.as_deref(), Some(TXID));
    assert_eq!(criada.status, Some(StatusCobR::Criada));
    assert_eq!(
        criada.valor.unwrap().original,
        Some("149.90".parse().unwrap())
    );
}

/// Without your txid there is nothing to repeat safely: a creation is
/// repeated only when it surely was not processed.
#[tokio::test]
async fn a_creation_whose_txid_inter_chooses_is_repeated_only_when_not_processed() {
    let server = setup("cobr.write").await;
    Mock::given(method("POST"))
        .and(path("/pix/v2/cobr"))
        .and(body_json(corpo()))
        .respond_with(ResponseTemplate::new(429))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/pix/v2/cobr"))
        .respond_with(ResponseTemplate::new(201).set_body_json(gerada("CRIADA")))
        .expect(1)
        .mount(&server)
        .await;
    let pix = client(&server);
    let criada = pix
        .pix_automatico()
        .criar_cobr_sem_txid(&cobr())
        .await
        .unwrap();
    assert_eq!(criada.txid.as_deref(), Some(TXID));

    let server = setup("cobr.write").await;
    Mock::given(method("POST"))
        .and(path("/pix/v2/cobr"))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&server)
        .await;
    let err = client(&server)
        .pix_automatico()
        .criar_cobr_sem_txid(&cobr())
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
    let pix = client(&server);
    let mut invalida = cobr();
    invalida.recebedor.conta = "1234-5".to_owned();
    for resultado in [
        pix.pix_automatico().criar_cobr(&txid(), &invalida).await,
        pix.pix_automatico().criar_cobr_sem_txid(&invalida).await,
    ] {
        let Error::InvalidInput(source) = resultado.unwrap_err() else {
            panic!("esperado InvalidInput")
        };
        let campo = source.downcast_ref::<CobrancaPixError>().unwrap().campo();
        assert_eq!(campo, "recebedor.conta");
    }
}

#[tokio::test]
async fn looks_a_charge_up_with_its_attempts_and_the_pix_that_paid_it() {
    let server = setup("cobr.read").await;
    let mut concluida = gerada("CONCLUIDA");
    concluida["tentativas"] = json!([
        {"dataLiquidacao": "2026-10-10", "tipo": "AGND", "endToEndId": "E12345678202610101200abcdef12345", "status": "REJEITADA",
         "rejeicao": {"codigo": "AM04", "descricao": "Saldo insuficiente"}},
        {"dataLiquidacao": "2026-10-12", "tipo": "NTAG", "endToEndId": "E12345678202610121200abcdef12345", "status": "PAGA",
         "atualizacao": [{"status": "SOLICITADA", "data": "2026-10-11T09:00:00.000Z"}, {"status": "PAGA", "data": "2026-10-12T08:00:00.000Z"}]}
    ]);
    concluida["pix"] = json!([{
        "endToEndId": "E12345678202610121200abcdef12345",
        "txid": TXID,
        "valor": "149.90",
        "horario": "2026-10-12T08:00:00.000Z",
        "devolucoes": [{"id": "d1", "rtrId": "D12345678202610151200abcdef12345", "valor": "10.00", "natureza": "MED_PIX_AUTOMATICO", "status": "DEVOLVIDO"}]
    }]);
    Mock::given(method("GET"))
        .and(path(format!("/pix/v2/cobr/{TXID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(concluida.clone()))
        .expect(1)
        .mount(&server)
        .await;
    let cobr = client(&server)
        .pix_automatico()
        .consultar_cobr(&txid())
        .await
        .unwrap();
    assert_eq!(cobr.status, Some(StatusCobR::Concluida));
    assert_eq!(cobr.tentativas[1].tipo, Some(TipoTentativa::NovaTentativa));
    assert_eq!(cobr.tentativas[1].status, Some(StatusTentativa::Paga));
    assert_eq!(
        cobr.tentativas[0]
            .rejeicao
            .as_ref()
            .unwrap()
            .codigo
            .as_deref(),
        Some("AM04")
    );
    assert_eq!(
        cobr.pix[0].devolucoes[0].natureza.as_deref(),
        Some("MED_PIX_AUTOMATICO")
    );
    // Nothing is lost on the way back.
    assert_eq!(serde_json::to_value(&cobr).unwrap(), concluida);
}

fn periodo() -> PeriodoPix {
    PeriodoPix::new(
        DateTime::parse_from_rfc3339("2026-09-01T00:00:00-03:00").unwrap(),
        DateTime::parse_from_rfc3339("2026-09-30T23:59:59-03:00").unwrap(),
    )
    .unwrap()
}

#[tokio::test]
async fn lists_every_charge_of_a_recurrence() {
    let server = setup("cobr.read").await;
    for (numero, cobsr) in [
        (0, vec![gerada("ATIVA"), gerada("CONCLUIDA")]),
        (1, vec![gerada("ATIVA")]),
    ] {
        Mock::given(method("GET"))
            .and(path("/pix/v2/cobr"))
            .and(query_param("inicio", "2026-09-01T00:00:00-03:00"))
            .and(query_param("fim", "2026-09-30T23:59:59-03:00"))
            .and(query_param("idRec", ID_REC))
            .and(query_param("cnpj", "12345678000195"))
            .and(query_param("status", "ATIVA"))
            .and(query_param(
                "paginacao.paginaAtual",
                numero.to_string().as_str(),
            ))
            .and(query_param("paginacao.itensPorPagina", "1000"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "parametros": {
                    "inicio": "2026-09-01T03:00:00Z",
                    "fim": "2026-10-01T02:59:59Z",
                    "paginacao": {"paginaAtual": numero, "itensPorPagina": 1000, "quantidadeDePaginas": 2, "quantidadeTotalDeItens": 3}
                },
                "cobsr": cobsr
            })))
            .expect(1)
            .mount(&server)
            .await;
    }
    let mut filtro = FiltroCobsR::new(periodo());
    filtro.id_rec = Some(ID_REC.parse().unwrap());
    filtro.devedor = Some("12.345.678/0001-95".parse().unwrap());
    filtro.status = Some(StatusCobR::Ativa);
    let cobsr = client(&server)
        .pix_automatico()
        .listar_todas_cobrs(&filtro)
        .await
        .unwrap();
    assert_eq!(cobsr.len(), 3);
}

#[tokio::test]
async fn cancels_a_charge_and_asks_for_a_new_attempt() {
    let server = setup("cobr.write").await;
    let mut cancelada = gerada("CANCELADA");
    cancelada["encerramento"] = json!({"cancelamento": {"solicitante": "USUARIO_RECEBEDOR", "codigo": "SLCR", "descricao": "Cancelamento solicitado pelo usuário recebedor"}});
    Mock::given(method("PATCH"))
        .and(path(format!("/pix/v2/cobr/{TXID}")))
        .and(body_json(json!({"status": "CANCELADA"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(cancelada))
        .expect(1)
        .mount(&server)
        .await;
    let mut com_tentativa = gerada("ATIVA");
    com_tentativa["tentativas"] =
        json!([{"dataLiquidacao": "2026-10-12", "tipo": "NTAG", "status": "SOLICITADA"}]);
    Mock::given(method("POST"))
        .and(path(format!("/pix/v2/cobr/{TXID}/retentativa/2026-10-12")))
        .and(body_string(""))
        .respond_with(ResponseTemplate::new(201).set_body_json(com_tentativa))
        .expect(1)
        .mount(&server)
        .await;

    let pix = client(&server);
    let cancelada = pix.pix_automatico().cancelar_cobr(&txid()).await.unwrap();
    assert_eq!(cancelada.status, Some(StatusCobR::Cancelada));
    assert_eq!(
        cancelada
            .encerramento
            .unwrap()
            .cancelamento
            .unwrap()
            .solicitante
            .as_deref(),
        Some("USUARIO_RECEBEDOR")
    );
    let tentada = pix
        .pix_automatico()
        .solicitar_retentativa(&txid(), dia(2026, 10, 12))
        .await
        .unwrap();
    assert_eq!(
        tentada.tentativas[0].status,
        Some(StatusTentativa::Solicitada)
    );
}
