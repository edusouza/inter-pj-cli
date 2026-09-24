//! Recurrences of Pix Automático (`/pix/v2/rec`) against a mock API. All data
//! is synthetic.

mod common;

use chrono::{DateTime, NaiveDate};
use common::{client, mount_token};
use inter_pj::Error;
use inter_pj::pix::{CobrancaPixError, Devedor, PeriodoPix, Txid};
use inter_pj::pix_automatico::{
    AtivacaoSolicitada, CalendarioRec, FiltroRecs, IdRec, Periodicidade, PoliticaRetentativa,
    RecRevisada, RecSolicitada, StatusRec, TipoJornada, ValorRec, VinculoRec,
};
use serde_json::{Value, json};
use wiremock::matchers::{any, body_json, method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ID: &str = "RR1234567820260924abcdefghijk";
const TXID: &str = "7978c0c97ea847e78e8849634473c1f1";

fn id() -> IdRec {
    ID.parse().unwrap()
}

fn txid() -> Txid {
    TXID.parse().unwrap()
}

fn dia(ano: i32, mes: u32, dia: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(ano, mes, dia).unwrap()
}

fn rec() -> RecSolicitada {
    let devedor = Devedor::new("123.456.789-09".parse().unwrap(), "Cliente Exemplo");
    let mut vinculo = VinculoRec::new(devedor, "contrato-001");
    vinculo.objeto = Some("Mensalidade do plano".to_owned());
    let mut calendario = CalendarioRec::new(dia(2026, 10, 10), Periodicidade::Mensal);
    calendario.data_final = Some(dia(2027, 9, 10));
    let mut rec = RecSolicitada::new(vinculo, calendario, PoliticaRetentativa::Permite3R7D);
    rec.valor = Some(ValorRec::fixo("149.90".parse().unwrap()));
    rec.loc = Some(108);
    rec.ativacao = Some(AtivacaoSolicitada::new(txid()));
    rec
}

fn corpo() -> Value {
    json!({
        "vinculo": {
            "objeto": "Mensalidade do plano",
            "devedor": {"cpf": "12345678909", "nome": "Cliente Exemplo"},
            "contrato": "contrato-001"
        },
        "calendario": {"dataInicial": "2026-10-10", "dataFinal": "2027-09-10", "periodicidade": "MENSAL"},
        "valor": {"valorRec": "149.90"},
        "politicaRetentativa": "PERMITE_3R_7D",
        "loc": 108,
        "ativacao": {"dadosJornada": {"txid": TXID}}
    })
}

fn gerada(status: &str) -> Value {
    json!({
        "idRec": ID,
        "vinculo": {
            "objeto": "Mensalidade do plano",
            "devedor": {"cpf": "12345678909", "nome": "Cliente Exemplo"},
            "contrato": "contrato-001"
        },
        "calendario": {"dataInicial": "2026-10-10", "dataFinal": "2027-09-10", "periodicidade": "MENSAL"},
        "valor": {"valorRec": "149.90"},
        "recebedor": {"cnpj": "12345678000195", "nome": "Empresa Exemplo Ltda"},
        "status": status,
        "politicaRetentativa": "PERMITE_3R_7D",
        "loc": {
            "id": 108,
            "location": "pix.example.com/qr/v2/rec/2353c790eefb11eaadc10242ac120002",
            "criacao": "2026-09-24T10:00:00.000Z",
            "idRec": ID
        },
        "atualizacao": [{"status": "CRIADA", "data": "2026-09-24T10:00:00.000Z"}],
        "ativacao": {"tipoJornada": "AGUARDANDO_DEFINICAO", "dadosJornada": {"txid": TXID}}
    })
}

async fn setup(scope: &str) -> MockServer {
    let server = MockServer::start().await;
    mount_token(&server, "tok", scope, 1).await;
    server
}

#[tokio::test]
async fn creates_a_recurrence() {
    let server = setup("rec.write").await;
    Mock::given(method("POST"))
        .and(path("/pix/v2/rec"))
        .and(body_json(corpo()))
        .respond_with(ResponseTemplate::new(201).set_body_json(gerada("CRIADA")))
        .expect(1)
        .mount(&server)
        .await;
    let criada = client(&server)
        .pix_automatico()
        .criar_rec(&rec())
        .await
        .unwrap();
    assert_eq!(criada.id_rec.as_deref(), Some(ID));
    assert_eq!(criada.status, Some(StatusRec::Criada));
    let loc = criada.loc.unwrap();
    assert_eq!(loc.id, Some(108));
    assert!(loc.location.unwrap().contains("/rec/"));
    assert_eq!(
        criada.ativacao.unwrap().jornada(),
        Some(&TipoJornada::AguardandoDefinicao)
    );
}

/// There is no idempotency key: a creation is repeated only when it surely
/// was not processed.
#[tokio::test]
async fn a_creation_is_repeated_only_when_surely_not_processed() {
    let server = setup("rec.write").await;
    Mock::given(method("POST"))
        .and(path("/pix/v2/rec"))
        .respond_with(ResponseTemplate::new(429))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/pix/v2/rec"))
        .respond_with(ResponseTemplate::new(201).set_body_json(gerada("CRIADA")))
        .expect(1)
        .mount(&server)
        .await;
    let pix = client(&server);
    let criada = pix.pix_automatico().criar_rec(&rec()).await.unwrap();
    assert_eq!(criada.id_rec.as_deref(), Some(ID));

    let server = setup("rec.write").await;
    Mock::given(method("POST"))
        .and(path("/pix/v2/rec"))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&server)
        .await;
    let err = client(&server)
        .pix_automatico()
        .criar_rec(&rec())
        .await
        .unwrap_err();
    assert!(
        matches!(err, Error::Api(ref api) if api.status == 503),
        "{err}"
    );
}

fn campo(err: Error) -> String {
    let Error::InvalidInput(source) = err else {
        panic!("{err}")
    };
    source
        .downcast_ref::<CobrancaPixError>()
        .unwrap_or_else(|| panic!("{source}"))
        .campo()
        .to_owned()
}

#[tokio::test]
async fn nothing_invalid_is_sent() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let pix = client(&server);
    let pix = pix.pix_automatico();

    let mut sem_contrato = rec();
    sem_contrato.vinculo.contrato = String::new();
    let err = pix.criar_rec(&sem_contrato).await.unwrap_err();
    assert_eq!(campo(err), "vinculo.contrato");

    let err = pix
        .revisar_rec(&id(), &RecRevisada::default())
        .await
        .unwrap_err();
    assert!(matches!(err, Error::InvalidInput(_)), "{err}");

    let mut filtro = FiltroRecs::new(periodo());
    filtro.convenio = Some("x".repeat(61));
    let err = pix.listar_recs(&filtro, 0, None).await.unwrap_err();
    assert_eq!(campo(err), "convenio");

    let filtro = FiltroRecs::new(periodo());
    for itens in [0, 1001] {
        let err = pix.listar_recs(&filtro, 0, Some(itens)).await.unwrap_err();
        assert!(matches!(err, Error::InvalidInput(_)), "{err}");
    }
}

fn periodo() -> PeriodoPix {
    PeriodoPix::new(
        DateTime::parse_from_rfc3339("2026-09-01T00:00:00-03:00").unwrap(),
        DateTime::parse_from_rfc3339("2026-09-30T23:59:59-03:00").unwrap(),
    )
    .unwrap()
}

fn pagina(numero: u64, paginas: u64, recs: &[Value]) -> Value {
    json!({
        "parametros": {
            "inicio": "2026-09-01T03:00:00Z",
            "fim": "2026-10-01T02:59:59Z",
            "paginacao": {"paginaAtual": numero, "itensPorPagina": 1000, "quantidadeDePaginas": paginas, "quantidadeTotalDeItens": 3}
        },
        "recs": recs
    })
}

#[tokio::test]
async fn lists_every_page_of_the_period() {
    let server = setup("rec.read").await;
    for (numero, recs) in [
        (0, vec![gerada("CRIADA"), gerada("APROVADA")]),
        (1, vec![gerada("APROVADA")]),
    ] {
        Mock::given(method("GET"))
            .and(path("/pix/v2/rec"))
            .and(query_param("inicio", "2026-09-01T00:00:00-03:00"))
            .and(query_param("fim", "2026-09-30T23:59:59-03:00"))
            .and(query_param("cpf", "12345678909"))
            .and(query_param("status", "APROVADA"))
            .and(query_param_is_missing("cnpj"))
            .and(query_param(
                "paginacao.paginaAtual",
                numero.to_string().as_str(),
            ))
            .and(query_param("paginacao.itensPorPagina", "1000"))
            .respond_with(ResponseTemplate::new(200).set_body_json(pagina(numero, 2, &recs)))
            .expect(1)
            .mount(&server)
            .await;
    }
    let mut filtro = FiltroRecs::new(periodo());
    filtro.devedor = Some("123.456.789-09".parse().unwrap());
    filtro.status = Some(StatusRec::Aprovada);
    let recs = client(&server)
        .pix_automatico()
        .listar_todas_recs(&filtro)
        .await
        .unwrap();
    assert_eq!(recs.len(), 3);
    assert_eq!(recs[2].status, Some(StatusRec::Aprovada));
}

#[tokio::test]
async fn one_page_with_its_filters() {
    let server = setup("rec.read").await;
    Mock::given(method("GET"))
        .and(path("/pix/v2/rec"))
        .and(query_param("paginacao.paginaAtual", "2"))
        .and(query_param_is_missing("paginacao.itensPorPagina"))
        .and(query_param("locationPresente", "true"))
        .and(query_param("convenio", "convenio-01"))
        .respond_with(ResponseTemplate::new(200).set_body_json(pagina(2, 5, &[gerada("CRIADA")])))
        .expect(1)
        .mount(&server)
        .await;
    let mut filtro = FiltroRecs::new(periodo());
    filtro.location_presente = Some(true);
    filtro.convenio = Some("convenio-01".to_owned());
    let pagina = client(&server)
        .pix_automatico()
        .listar_recs(&filtro, 2, None)
        .await
        .unwrap();
    assert_eq!(pagina.recs.len(), 1);
    let paginacao = pagina.parametros.unwrap().paginacao.unwrap();
    assert_eq!(paginacao.quantidade_de_paginas, Some(5));
    assert!(paginacao.tem_mais(2, 1));
}

#[tokio::test]
async fn looks_a_recurrence_up_with_the_qr_code_of_a_charge() {
    let server = setup("rec.read").await;
    let mut aprovada = gerada("APROVADA");
    aprovada["pagador"] =
        json!({"cpf": "12345678909", "ispbParticipante": "00000000", "codMun": "3550308"});
    Mock::given(method("GET"))
        .and(path(format!("/pix/v2/rec/{ID}")))
        .and(query_param_is_missing("txid"))
        .respond_with(ResponseTemplate::new(200).set_body_json(aprovada.clone()))
        .expect(1)
        .mount(&server)
        .await;
    let mut composta = gerada("CRIADA");
    composta["dadosQR"] = json!({"jornada": "JORNADA_3", "pixCopiaECola": "00020101021226760014br.gov.bcb.pix2554pix.example.com/qr/v2/cob6304ABCD"});
    Mock::given(method("GET"))
        .and(path(format!("/pix/v2/rec/{ID}")))
        .and(query_param("txid", TXID))
        .respond_with(ResponseTemplate::new(200).set_body_json(composta))
        .expect(1)
        .mount(&server)
        .await;

    let pix = client(&server);
    let rec = pix
        .pix_automatico()
        .consultar_rec(&id(), None)
        .await
        .unwrap();
    assert_eq!(rec.pagador.unwrap().cod_mun.as_deref(), Some("3550308"));
    let rec = pix
        .pix_automatico()
        .consultar_rec(&id(), Some(&txid()))
        .await
        .unwrap();
    let qr = rec.dados_qr.unwrap();
    assert_eq!(qr.jornada, Some(TipoJornada::Jornada3));
    assert!(qr.pix_copia_e_cola.unwrap().starts_with("000201"));
}

#[tokio::test]
async fn an_unknown_recurrence_is_reported_as_the_api_explains_it() {
    let server = setup("rec.read").await;
    Mock::given(method("GET"))
        .and(path(format!("/pix/v2/rec/{ID}")))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({
            "type": "https://pix.bcb.gov.br/api/v2/error/NaoEncontrado",
            "title": "Não Encontrado",
            "status": 404,
            "detail": "Entidade não encontrada."
        })))
        .expect(1)
        .mount(&server)
        .await;
    let err = client(&server)
        .pix_automatico()
        .consultar_rec(&id(), None)
        .await
        .unwrap_err();
    let Error::Api(api) = err else {
        panic!("{err}")
    };
    assert_eq!(api.status, 404);
    assert_eq!(
        api.problem.unwrap().detail.as_deref(),
        Some("Entidade não encontrada.")
    );
}

#[tokio::test]
async fn revises_and_cancels_a_recurrence() {
    let server = setup("rec.write").await;
    Mock::given(method("PATCH"))
        .and(path(format!("/pix/v2/rec/{ID}")))
        .and(body_json(json!({
            "vinculo": {"devedor": {"nome": "Cliente Exemplo Ltda"}},
            "calendario": {"dataInicial": "2026-11-10"}
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(gerada("CRIADA")))
        .expect(1)
        .mount(&server)
        .await;
    let mut cancelada = gerada("CANCELADA");
    cancelada["encerramento"] = json!({"cancelamento": {"solicitante": "USUARIO_RECEBEDOR", "codigo": "SLCR", "descricao": "Cancelamento solicitado pelo usuário recebedor"}});
    Mock::given(method("PATCH"))
        .and(path(format!("/pix/v2/rec/{ID}")))
        .and(body_json(json!({"status": "CANCELADA"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(cancelada))
        .expect(1)
        .mount(&server)
        .await;

    let pix = client(&server);
    let mut revisao = RecRevisada::default();
    revisao.nome_devedor = Some("Cliente Exemplo Ltda".to_owned());
    revisao.data_inicial = Some(dia(2026, 11, 10));
    let revisada = pix
        .pix_automatico()
        .revisar_rec(&id(), &revisao)
        .await
        .unwrap();
    assert_eq!(revisada.status, Some(StatusRec::Criada));
    let cancelada = pix
        .pix_automatico()
        .revisar_rec(&id(), &RecRevisada::cancelamento())
        .await
        .unwrap();
    assert_eq!(cancelada.status, Some(StatusRec::Cancelada));
    let cancelamento = cancelada.encerramento.unwrap().cancelamento.unwrap();
    assert_eq!(cancelamento.codigo.as_deref(), Some("SLCR"));
}
