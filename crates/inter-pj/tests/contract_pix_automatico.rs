//! Contract tests of Pix Automático (`/pix/v2/rec`...): the models must agree
//! with Inter's OpenAPI specification (`spec/inter-empresas-openapi.json`).
//!
//! The examples of the specification carry document numbers: they are
//! replaced by synthetic ones when read, and the models are built from the
//! examples' own values, so no example data is copied here.

mod spec;

use std::collections::BTreeSet;

use chrono::{DateTime, NaiveDate};
use inter_pj::cobranca::Uf;
use inter_pj::endpoint;
use inter_pj::pix::{
    Devedor, ITENS_POR_PAGINA_MAXIMO_PIX, PagamentoSandbox, TXID_MAXIMO, TXID_MINIMO,
};
use inter_pj::pix_automatico::{
    AtivacaoSolicitada, CalendarioRec, CobR, CobRSolicitada, ContaRecebedor, DestinatarioSolicRec,
    DevedorCobR, ID_TAMANHO, LocationRec, MAX_AGENCIA, MAX_CONTA, MAX_CONTRATO, MAX_CONVENIO,
    MAX_INFO_ADICIONAL, MAX_NOME_DEVEDOR, MAX_OBJETO, NotificacaoCobsR, NotificacaoRecs,
    PaginaCobsR, PaginaLocsRec, PaginaRecs, Periodicidade, PoliticaRetentativa,
    RazaoCancelamentoCobR, RazaoCancelamentoRec, Rec, RecRevisada, RecSolicitada, SolicRec,
    SolicRecSolicitada, StatusCobR, StatusRec, StatusSolicRec, StatusTentativa, TipoContaRecebedor,
    TipoJornada, TipoTentativa, TipoWebhookPixAutomatico, ValorRec, VinculoRec,
};
use inter_pj::webhook::Webhook;
use serde_json::{Value, json};
use spec::{
    assert_documentado, caminhos, documentados, enum_de, example_for_schema, parameter_names,
    parameters, parametro, propriedade, schema, spec, strings,
};

const CPF: &str = "12345678909";
const CNPJ: &str = "12345678000195";

/// A documented example, with synthetic document numbers.
fn example(name: &str) -> Value {
    let example = &spec()["components"]["examples"][name]["value"];
    assert!(!example.is_null(), "exemplo {name} não encontrado");
    sinteticos(example)
}

/// `cpf` and `cnpj` replaced by synthetic ones, as text (the examples have
/// them as text and as numbers).
fn sinteticos(valor: &Value) -> Value {
    match valor {
        Value::Object(campos) => Value::Object(
            campos
                .iter()
                .map(|(nome, valor)| {
                    let valor = match nome.as_str() {
                        "cpf" => json!(CPF),
                        "cnpj" => json!(CNPJ),
                        _ => sinteticos(valor),
                    };
                    (nome.clone(), valor)
                })
                .collect(),
        ),
        Value::Array(itens) => Value::Array(itens.iter().map(sinteticos).collect()),
        outro => outro.clone(),
    }
}

/// An answer as the models write it back: the examples call the status of
/// each change `nome`, as the schema does not, and write some account
/// numbers as numbers.
fn como_escrita(valor: &Value) -> Value {
    match valor {
        Value::Object(campos) => Value::Object(
            campos
                .iter()
                .map(|(nome, valor)| {
                    let valor = match (nome.as_str(), valor) {
                        ("atualizacao", Value::Array(itens)) => {
                            Value::Array(itens.iter().map(status_por_nome).collect())
                        }
                        // Account numbers are text, which some examples
                        // write as numbers.
                        ("conta", Value::Number(numero)) => json!(numero.to_string()),
                        _ => como_escrita(valor),
                    };
                    (nome.clone(), valor)
                })
                .collect(),
        ),
        Value::Array(itens) => Value::Array(itens.iter().map(como_escrita).collect()),
        outro => outro.clone(),
    }
}

fn status_por_nome(item: &Value) -> Value {
    let mut item = item.clone();
    if let Some(status) = item
        .as_object_mut()
        .and_then(|campos| campos.remove("nome"))
    {
        item["status"] = status;
    }
    item
}

fn texto(valor: &Value) -> &str {
    valor
        .as_str()
        .unwrap_or_else(|| panic!("esperado texto: {valor}"))
}

fn data(valor: &Value) -> NaiveDate {
    texto(valor).parse().unwrap()
}

/// The recurrence of a documented body, built with the public API.
fn rec_do_exemplo(exemplo: &Value) -> RecSolicitada {
    let vinculo = &exemplo["vinculo"];
    let devedor = Devedor::new(CPF.parse().unwrap(), texto(&vinculo["devedor"]["nome"]));
    let mut vinculo_rec = VinculoRec::new(devedor, texto(&vinculo["contrato"]));
    vinculo_rec.objeto = vinculo["objeto"].as_str().map(str::to_owned);
    let calendario = &exemplo["calendario"];
    let mut calendario_rec = CalendarioRec::new(
        data(&calendario["dataInicial"]),
        Periodicidade::from(texto(&calendario["periodicidade"])),
    );
    calendario_rec.data_final = calendario.get("dataFinal").map(data);
    let mut rec = RecSolicitada::new(
        vinculo_rec,
        calendario_rec,
        PoliticaRetentativa::from(texto(&exemplo["politicaRetentativa"])),
    );
    let valor = &exemplo["valor"];
    rec.valor = match (valor.get("valorRec"), valor.get("valorMinimoRecebedor")) {
        (Some(fixo), None) => Some(ValorRec::fixo(texto(fixo).parse().unwrap())),
        (None, Some(minimo)) => Some(ValorRec::minimo(texto(minimo).parse().unwrap())),
        (None, None) => None,
        (Some(_), Some(_)) => panic!("o exemplo tem os dois valores"),
    };
    rec.loc = exemplo["loc"].as_u64();
    rec.ativacao = exemplo["ativacao"]["dadosJornada"]["txid"]
        .as_str()
        .map(|txid| AtivacaoSolicitada::new(txid.parse().unwrap()));
    rec
}

#[test]
fn requests_are_the_documentation_examples() {
    for nome in ["recBody1", "recBody2"] {
        let exemplo = example(nome);
        let rec = rec_do_exemplo(&exemplo);
        rec.validar().unwrap();
        assert_eq!(serde_json::to_value(&rec).unwrap(), exemplo, "{nome}");
    }

    let exemplo = example("recBody3");
    let mut revisao = RecRevisada::default();
    revisao.nome_devedor = Some(texto(&exemplo["vinculo"]["devedor"]["nome"]).to_owned());
    revisao.loc = exemplo["loc"].as_u64();
    revisao.data_inicial = Some(data(&exemplo["calendario"]["dataInicial"]));
    revisao.txid = Some(
        texto(&exemplo["ativacao"]["dadosJornada"]["txid"])
            .parse()
            .unwrap(),
    );
    revisao.validar().unwrap();
    assert_eq!(serde_json::to_value(&revisao).unwrap(), exemplo);
}

#[test]
fn every_field_sent_is_documented() {
    let mut rec = rec_do_exemplo(&example("recBody1"));
    rec.validar().unwrap();
    assert_documentado("RecSolicitada", &serde_json::to_value(&rec).unwrap());
    rec.vinculo.devedor = Devedor::new(CNPJ.parse().unwrap(), "Empresa Exemplo Ltda");
    rec.valor = Some(ValorRec::minimo("50.00".parse().unwrap()));
    rec.validar().unwrap();
    assert_documentado("RecSolicitada", &serde_json::to_value(&rec).unwrap());

    let cancelamento = serde_json::to_value(RecRevisada::cancelamento()).unwrap();
    assert_documentado("RecRevisada", &cancelamento);
    assert_eq!(
        strings(&[texto(&cancelamento["status"])]),
        enum_de(propriedade(schema("RecRevisada"), "status").unwrap())
    );
}

#[test]
fn documented_answers_survive_a_round_trip() {
    for nome in [
        "recResponse1",
        "recResponse2",
        "recResponse3",
        "recResponse4",
        "recResponse5",
        "recResponse6",
        "recResponse7",
        "recResponse8",
        "recPayload1",
    ] {
        let exemplo = example(nome);
        let rec: Rec = serde_json::from_value(exemplo.clone()).unwrap();
        assert_eq!(
            serde_json::to_value(&rec).unwrap(),
            como_escrita(&exemplo),
            "{nome}"
        );
    }
    let exemplo = example("getRec1");
    let pagina: PaginaRecs = serde_json::from_value(exemplo.clone()).unwrap();
    assert_eq!(pagina.recs.len(), 1);
    assert_eq!(
        serde_json::to_value(&pagina).unwrap(),
        como_escrita(&exemplo)
    );
}

/// Every documented field of an answer is read and written back, except the
/// other option of a person (`cnpj` when the example has a `cpf`).
fn assert_lidos(nome: &str, lido: &Value, alternativas: &[&str]) {
    let mut esperados = BTreeSet::new();
    documentados(schema(nome), "", &mut esperados);
    let mut lidos = BTreeSet::new();
    caminhos(lido, "", &mut lidos);
    let faltando: Vec<&String> = esperados
        .difference(&lidos)
        .filter(|caminho| !alternativas.contains(&caminho.as_str()))
        .collect();
    assert!(
        faltando.is_empty(),
        "campos de {nome} perdidos: {faltando:?}"
    );
    let fora: Vec<&String> = lidos.difference(&esperados).collect();
    assert!(fora.is_empty(), "campos fora de {nome}: {fora:?}");
}

/// The generated example of a recurrence: the documentation's "example" of
/// the object is a list of examples.
fn rec_gerada(nome: &str) -> Value {
    let mut rec = example_for_schema(nome);
    rec["vinculo"]["objeto"] = json!("Mensalidade");
    rec
}

/// The generated example of a confirmation request, with its recurrence
/// generated apart: nested inside a recurrence, the example stops before.
fn solicitacao_gerada() -> Value {
    let mut solicitacao = example_for_schema("SolicRecCompleta");
    solicitacao["recPayload"] = rec_gerada("RecPayload");
    solicitacao
}

#[test]
fn answers_keep_every_documented_field() {
    let mut completa = rec_gerada("RecCompleta");
    completa["solicitacao"] = json!([solicitacao_gerada()]);
    let rec: Rec = serde_json::from_value(completa).unwrap();
    assert_lidos(
        "RecCompleta",
        &serde_json::to_value(&rec).unwrap(),
        &[
            "vinculo.devedor.cnpj",
            "pagador.cnpj",
            "solicitacao.destinatario.cnpj",
            "solicitacao.recPayload.vinculo.devedor.cnpj",
        ],
    );

    let mut listada = rec_gerada("RecCompletaPesquisada");
    listada["solicitacao"] = json!([solicitacao_gerada()]);
    let mut pagina = example_for_schema("RecsConsultadas");
    pagina["recs"] = json!([listada]);
    let pagina: PaginaRecs = serde_json::from_value(pagina).unwrap();
    assert_lidos(
        "RecsConsultadas",
        &serde_json::to_value(&pagina).unwrap(),
        &[
            "recs.vinculo.devedor.cnpj",
            "recs.pagador.cnpj",
            "recs.solicitacao.destinatario.cnpj",
            "recs.solicitacao.recPayload.vinculo.devedor.cnpj",
        ],
    );

    let solicitacao: SolicRec = serde_json::from_value(solicitacao_gerada()).unwrap();
    assert_lidos(
        "SolicRecCompleta",
        &serde_json::to_value(&solicitacao).unwrap(),
        &["destinatario.cnpj", "recPayload.vinculo.devedor.cnpj"],
    );

    let pagina: PaginaLocsRec =
        serde_json::from_value(example_for_schema("PayloadLocationRecConsultadas")).unwrap();
    assert_lidos(
        "PayloadLocationRecConsultadas",
        &serde_json::to_value(&pagina).unwrap(),
        &[],
    );
}

fn codigos<T>(documentados: &[T], codigo: fn(&T) -> &str) -> BTreeSet<String> {
    documentados
        .iter()
        .map(|valor| codigo(valor).to_owned())
        .collect()
}

#[test]
fn codes_are_the_documented_ones() {
    let gerada = schema("RecCompleta");
    let calendario = propriedade(gerada, "calendario").unwrap();
    assert_eq!(
        codigos(Periodicidade::DOCUMENTADOS, Periodicidade::as_str),
        enum_de(propriedade(calendario, "periodicidade").unwrap())
    );
    assert_eq!(
        codigos(
            PoliticaRetentativa::DOCUMENTADOS,
            PoliticaRetentativa::as_str
        ),
        enum_de(propriedade(gerada, "politicaRetentativa").unwrap())
    );
    let status = codigos(StatusRec::DOCUMENTADOS, StatusRec::as_str);
    assert_eq!(status, enum_de(propriedade(gerada, "status").unwrap()));
    let listagem = parametro(&endpoint::pix_automatico::LISTAR_RECS, "status");
    assert_eq!(status, enum_de(&listagem["schema"]));
    let ativacao = propriedade(gerada, "ativacao").unwrap();
    let jornadas = codigos(TipoJornada::DOCUMENTADOS, TipoJornada::as_str);
    assert_eq!(
        jornadas,
        enum_de(propriedade(ativacao, "tipoJornada").unwrap())
    );
    // The QR Code starts only the journeys of a QR Code.
    let qr = propriedade(propriedade(gerada, "dadosQR").unwrap(), "jornada").unwrap();
    assert!(enum_de(qr).is_subset(&jornadas));
}

#[test]
fn limits_are_the_documented_ones() {
    let id = schema("RecId");
    assert_eq!(id["pattern"], format!("[a-zA-Z0-9]{{{ID_TAMANHO}}}"));
    assert_eq!(id["minLength"], ID_TAMANHO);
    assert_eq!(id["maxLength"], ID_TAMANHO);
    let vinculo = propriedade(schema("RecSolicitada"), "vinculo").unwrap();
    assert_eq!(
        propriedade(vinculo, "objeto").unwrap()["maxLength"],
        MAX_OBJETO
    );
    assert_eq!(
        propriedade(vinculo, "contrato").unwrap()["maxLength"],
        MAX_CONTRATO
    );
    let devedor = propriedade(vinculo, "devedor").unwrap();
    assert_eq!(
        propriedade(devedor, "nome").unwrap()["maxLength"],
        MAX_NOME_DEVEDOR
    );
    let revisada = propriedade(schema("RecRevisada"), "vinculo").unwrap();
    let nome = propriedade(propriedade(revisada, "devedor").unwrap(), "nome").unwrap();
    assert_eq!(nome["maxLength"], MAX_NOME_DEVEDOR);
    let valor = propriedade(schema("RecSolicitada"), "valor").unwrap();
    for campo in ["valorRec", "valorMinimoRecebedor"] {
        assert_eq!(
            propriedade(valor, campo).unwrap()["pattern"],
            r"\d{1,10}\.\d{2}"
        );
    }
    let ativacao = propriedade(schema("RecSolicitada"), "ativacao").unwrap();
    let jornada = propriedade(ativacao, "dadosJornada").unwrap();
    assert_eq!(
        propriedade(jornada, "txid").unwrap()["pattern"],
        format!("[a-zA-Z0-9]{{{TXID_MINIMO},{TXID_MAXIMO}}}")
    );
    let listagem = parameters(&endpoint::pix_automatico::LISTAR_RECS);
    assert_eq!(
        listagem["paginacao.itensPorPagina"]["schema"]["maximum"],
        ITENS_POR_PAGINA_MAXIMO_PIX
    );
    assert_eq!(listagem["convenio"]["schema"]["maxLength"], MAX_CONVENIO);
}

#[test]
fn the_parameters_sent_are_documented() {
    let documentados = parameter_names(&endpoint::pix_automatico::LISTAR_RECS);
    for nome in [
        "inicio",
        "fim",
        "cpf",
        "cnpj",
        "locationPresente",
        "status",
        "convenio",
        "paginacao.paginaAtual",
        "paginacao.itensPorPagina",
    ] {
        assert!(documentados.contains(nome), "{nome}");
    }
    for endpoint in [
        endpoint::pix_automatico::CONSULTAR_REC,
        endpoint::pix_automatico::REVISAR_REC,
    ] {
        assert!(parameter_names(&endpoint).contains("idRec"), "{endpoint}");
    }
    assert!(parameter_names(&endpoint::pix_automatico::CONSULTAR_REC).contains("txid"));
}

// --- solicrec ---------------------------------------------------------------------

/// The confirmation request of a documented body, built with the public API.
fn solicitacao_do_exemplo(exemplo: &Value) -> SolicRecSolicitada {
    let destinatario = &exemplo["destinatario"];
    let mut conta = DestinatarioSolicRec::new(
        CPF.parse().unwrap(),
        texto(&destinatario["conta"]),
        texto(&destinatario["ispbParticipante"]),
    );
    conta.agencia = destinatario["agencia"].as_str().map(str::to_owned);
    SolicRecSolicitada::new(
        texto(&exemplo["idRec"]).parse().unwrap(),
        DateTime::parse_from_rfc3339(texto(&exemplo["calendario"]["dataExpiracaoSolicitacao"]))
            .unwrap(),
        conta,
    )
}

#[test]
fn confirmation_requests_are_the_documentation_examples() {
    let exemplo = example("solicRecBody1");
    let solicitacao = solicitacao_do_exemplo(&exemplo);
    solicitacao.validar().unwrap();
    assert_eq!(serde_json::to_value(&solicitacao).unwrap(), exemplo);
    assert_documentado(
        "SolicRecSolicitada",
        &serde_json::to_value(&solicitacao).unwrap(),
    );

    let mut empresa = solicitacao;
    empresa.destinatario.documento = CNPJ.parse().unwrap();
    empresa.destinatario.agencia = None;
    empresa.validar().unwrap();
    assert_documentado(
        "SolicRecSolicitada",
        &serde_json::to_value(&empresa).unwrap(),
    );

    // The only revision is the cancellation, which `cancelar_solicitacao`
    // sends as documented.
    assert_eq!(example("solicRecBody2"), json!({"status": "CANCELADA"}));
    assert_eq!(
        enum_de(propriedade(schema("SolicRecRevisada"), "status").unwrap()),
        strings(&[StatusSolicRec::Cancelada.as_str()])
    );
}

#[test]
fn confirmation_request_answers_survive_a_round_trip() {
    for nome in [
        "solicRecResponse1",
        "solicRecResponse2",
        "solicRecResponse3",
    ] {
        let exemplo = example(nome);
        let solicitacao: SolicRec = serde_json::from_value(exemplo.clone()).unwrap();
        assert_eq!(
            serde_json::to_value(&solicitacao).unwrap(),
            como_escrita(&exemplo),
            "{nome}"
        );
    }
}

#[test]
fn confirmation_request_codes_and_limits_are_the_documented_ones() {
    let completa = schema("SolicRecCompleta");
    let status = codigos(StatusSolicRec::DOCUMENTADOS, StatusSolicRec::as_str);
    assert_eq!(status, enum_de(propriedade(completa, "status").unwrap()));
    let atualizacao = propriedade(completa, "atualizacao").unwrap();
    assert_eq!(
        status,
        enum_de(propriedade(&atualizacao["items"], "status").unwrap())
    );
    let id = propriedade(completa, "idSolicRec").unwrap();
    assert_eq!(id["pattern"], format!("[a-zA-Z0-9]{{{ID_TAMANHO}}}"));
    assert_eq!(id["maxLength"], ID_TAMANHO);
    let destinatario = propriedade(schema("SolicRecSolicitada"), "destinatario").unwrap();
    assert_eq!(
        propriedade(destinatario, "conta").unwrap()["maxLength"],
        MAX_CONTA
    );
    assert_eq!(
        propriedade(destinatario, "agencia").unwrap()["maxLength"],
        MAX_AGENCIA
    );
    assert_eq!(
        propriedade(destinatario, "ispbParticipante").unwrap()["pattern"],
        r"\d{8}"
    );
    for endpoint in [
        endpoint::pix_automatico::CONSULTAR_SOLICITACAO,
        endpoint::pix_automatico::REVISAR_SOLICITACAO,
    ] {
        assert!(
            parameter_names(&endpoint).contains("idSolicRec"),
            "{endpoint}"
        );
    }
}

// --- locrec -----------------------------------------------------------------------

#[test]
fn locations_survive_a_round_trip() {
    for nome in ["payloadLocationRecResponse1", "payloadLocationRecResponse2"] {
        let exemplo = example(nome);
        let loc: LocationRec = serde_json::from_value(exemplo.clone()).unwrap();
        assert_eq!(serde_json::to_value(&loc).unwrap(), exemplo, "{nome}");
    }
}

#[test]
fn location_parameters_are_documented() {
    let documentados = parameter_names(&endpoint::pix_automatico::LISTAR_LOCRECS);
    for nome in [
        "inicio",
        "fim",
        "idRecPresente",
        "convenio",
        "paginacao.paginaAtual",
        "paginacao.itensPorPagina",
    ] {
        assert!(documentados.contains(nome), "{nome}");
    }
    let listagem = parameters(&endpoint::pix_automatico::LISTAR_LOCRECS);
    assert_eq!(listagem["convenio"]["schema"]["maxLength"], MAX_CONVENIO);
    assert_eq!(
        listagem["paginacao.itensPorPagina"]["schema"]["maximum"],
        ITENS_POR_PAGINA_MAXIMO_PIX
    );
    for endpoint in [
        endpoint::pix_automatico::CONSULTAR_LOCREC,
        endpoint::pix_automatico::DESVINCULAR_LOCREC,
    ] {
        assert!(parameter_names(&endpoint).contains("id"), "{endpoint}");
    }
    // A location is created without a body.
    assert!(spec::operation(&endpoint::pix_automatico::CRIAR_LOCREC)["requestBody"].is_null());
}

// --- cobr -------------------------------------------------------------------------

/// The recurring charge of a documented body, built with the public API.
/// The example writes the account as a number and the CEP with a hyphen,
/// which its own pattern (`[0-9]{8}`) refuses; the model sends them as
/// the schema defines.
fn cobr_do_exemplo(exemplo: &Value) -> CobRSolicitada {
    let recebedor = como_escrita(&exemplo["recebedor"]);
    let mut conta = ContaRecebedor::new(
        texto(&recebedor["conta"]),
        TipoContaRecebedor::from(texto(&recebedor["tipoConta"])),
    );
    conta.agencia = recebedor["agencia"].as_str().map(str::to_owned);
    let mut cobr = CobRSolicitada::new(
        texto(&exemplo["idRec"]).parse().unwrap(),
        data(&exemplo["calendario"]["dataDeVencimento"]),
        texto(&exemplo["valor"]["original"]).parse().unwrap(),
        conta,
    );
    cobr.ajuste_dia_util = exemplo["ajusteDiaUtil"].as_bool().unwrap();
    cobr.info_adicional = exemplo["infoAdicional"].as_str().map(str::to_owned);
    let devedor = &exemplo["devedor"];
    let mut contato = DevedorCobR::default();
    contato.email = devedor["email"].as_str().map(str::to_owned);
    contato.logradouro = devedor["logradouro"].as_str().map(str::to_owned);
    contato.cidade = devedor["cidade"].as_str().map(str::to_owned);
    contato.uf = devedor["uf"].as_str().map(|uf| uf.parse::<Uf>().unwrap());
    contato.cep = devedor["cep"].as_str().map(|cep| cep.replace('-', ""));
    cobr.devedor = Some(contato);
    cobr
}

#[test]
fn recurring_charges_are_the_documentation_examples() {
    let exemplo = example("cobRBody1");
    let cobr = cobr_do_exemplo(&exemplo);
    cobr.validar().unwrap();
    let mut esperado = como_escrita(&exemplo);
    esperado["devedor"]["cep"] = json!(texto(&exemplo["devedor"]["cep"]).replace('-', ""));
    let enviado = serde_json::to_value(&cobr).unwrap();
    assert_eq!(enviado, esperado);
    assert_documentado("CobRSolicitada", &enviado);

    // The only revision is the cancellation, which `cancelar_cobr` sends as
    // documented.
    assert_eq!(example("cobRBody2"), json!({"status": "CANCELADA"}));
    assert_eq!(
        enum_de(propriedade(schema("CobRRevisada"), "status").unwrap()),
        strings(&[StatusCobR::Cancelada.as_str()])
    );
}

#[test]
fn recurring_charge_answers_survive_a_round_trip() {
    for nome in [
        "cobRResponse1",
        "cobRResponse2",
        "cobRResponse3",
        "cobRResponse4",
    ] {
        let exemplo = example(nome);
        let cobr: CobR = serde_json::from_value(exemplo.clone()).unwrap();
        assert_eq!(
            serde_json::to_value(&cobr).unwrap(),
            como_escrita(&exemplo),
            "{nome}"
        );
    }
    let exemplo = example("getCobR1");
    let pagina: PaginaCobsR = serde_json::from_value(exemplo.clone()).unwrap();
    assert_eq!(pagina.cobsr.len(), 1);
    assert_eq!(
        serde_json::to_value(&pagina).unwrap(),
        como_escrita(&exemplo)
    );
}

/// The generated example of a recurring charge, with an amount the
/// specification does not exemplify.
fn cobr_gerada(nome: &str) -> Value {
    let mut cobr = example_for_schema(nome);
    cobr["valor"]["original"] = json!("10.50");
    cobr
}

#[test]
fn recurring_charges_keep_every_documented_field() {
    let completa = cobr_gerada("CobRCompleta");
    let cobr: CobR = serde_json::from_value(completa.clone()).unwrap();
    assert_lidos("CobRCompleta", &serde_json::to_value(&cobr).unwrap(), &[]);
    let gerada: CobR = serde_json::from_value(cobr_gerada("CobRGerada")).unwrap();
    assert_lidos("CobRGerada", &serde_json::to_value(&gerada).unwrap(), &[]);

    // The charges of a page are generated apart: nested, the example stops
    // before the refunds of their Pix.
    let mut pagina = example_for_schema("CobsRConsultadas");
    pagina["cobsr"] = json!([completa]);
    let pagina: PaginaCobsR = serde_json::from_value(pagina).unwrap();
    assert_lidos(
        "CobsRConsultadas",
        &serde_json::to_value(&pagina).unwrap(),
        &[],
    );
}

#[test]
fn recurring_charge_codes_and_limits_are_the_documented_ones() {
    let completa = schema("CobRCompleta");
    let status = codigos(StatusCobR::DOCUMENTADOS, StatusCobR::as_str);
    assert_eq!(status, enum_de(propriedade(completa, "status").unwrap()));
    let atualizacao = propriedade(completa, "atualizacao").unwrap();
    assert_eq!(
        status,
        enum_de(propriedade(&atualizacao["items"], "status").unwrap())
    );
    let listagem = parametro(&endpoint::pix_automatico::LISTAR_COBRS, "status");
    assert_eq!(status, enum_de(&listagem["schema"]));
    let tentativas = &propriedade(completa, "tentativas").unwrap()["items"];
    assert_eq!(
        codigos(TipoTentativa::DOCUMENTADOS, TipoTentativa::as_str),
        enum_de(propriedade(tentativas, "tipo").unwrap())
    );
    let status_tentativa = codigos(StatusTentativa::DOCUMENTADOS, StatusTentativa::as_str);
    assert_eq!(
        status_tentativa,
        enum_de(propriedade(tentativas, "status").unwrap())
    );
    let historico = propriedade(tentativas, "atualizacao").unwrap();
    assert_eq!(
        status_tentativa,
        enum_de(propriedade(&historico["items"], "status").unwrap())
    );

    let solicitada = schema("CobRSolicitada");
    let recebedor = propriedade(solicitada, "recebedor").unwrap();
    assert_eq!(
        codigos(TipoContaRecebedor::DOCUMENTADOS, TipoContaRecebedor::as_str),
        enum_de(propriedade(recebedor, "tipoConta").unwrap())
    );
    assert_eq!(
        propriedade(recebedor, "conta").unwrap()["maxLength"],
        MAX_CONTA
    );
    assert_eq!(
        propriedade(recebedor, "agencia").unwrap()["maxLength"],
        MAX_AGENCIA
    );
    assert_eq!(
        propriedade(solicitada, "infoAdicional").unwrap()["maxLength"],
        MAX_INFO_ADICIONAL
    );
    let valor = propriedade(solicitada, "valor").unwrap();
    assert_eq!(
        propriedade(valor, "original").unwrap()["pattern"],
        r"\d{1,10}\.\d{2}"
    );
    let devedor = propriedade(solicitada, "devedor").unwrap();
    assert_eq!(propriedade(devedor, "cep").unwrap()["pattern"], "[0-9]{8}");
    for campo in ["logradouro", "cidade"] {
        assert_eq!(propriedade(devedor, campo).unwrap()["maxLength"], 200);
    }
}

#[test]
fn recurring_charge_parameters_are_documented() {
    let documentados = parameter_names(&endpoint::pix_automatico::LISTAR_COBRS);
    for nome in [
        "inicio",
        "fim",
        "idRec",
        "cpf",
        "cnpj",
        "status",
        "convenio",
        "paginacao.paginaAtual",
        "paginacao.itensPorPagina",
    ] {
        assert!(documentados.contains(nome), "{nome}");
    }
    let listagem = parameters(&endpoint::pix_automatico::LISTAR_COBRS);
    assert_eq!(listagem["convenio"]["schema"]["maxLength"], MAX_CONVENIO);
    assert_eq!(
        listagem["paginacao.itensPorPagina"]["schema"]["maximum"],
        ITENS_POR_PAGINA_MAXIMO_PIX
    );
    for endpoint in [
        endpoint::pix_automatico::CRIAR_COBR,
        endpoint::pix_automatico::CONSULTAR_COBR,
        endpoint::pix_automatico::REVISAR_COBR,
        endpoint::pix_automatico::RETENTATIVA_COBR,
    ] {
        let txid = parametro(&endpoint, "txid");
        assert_eq!(
            txid["schema"]["pattern"],
            format!("[a-zA-Z0-9]{{{TXID_MINIMO},{TXID_MAXIMO}}}"),
            "{endpoint}"
        );
    }
    let data = parametro(&endpoint::pix_automatico::RETENTATIVA_COBR, "data");
    assert_eq!(data["schema"]["format"], "date");
    // A retry is asked for without a body.
    assert!(spec::operation(&endpoint::pix_automatico::RETENTATIVA_COBR)["requestBody"].is_null());
}

// --- webhooks and sandbox -----------------------------------------------------------

fn endpoints_do_webhook(tipo: TipoWebhookPixAutomatico) -> [inter_pj::endpoint::Endpoint; 3] {
    match tipo {
        TipoWebhookPixAutomatico::Recorrencia => [
            endpoint::pix_automatico::WEBHOOK_REC_CADASTRAR,
            endpoint::pix_automatico::WEBHOOK_REC_CONSULTAR,
            endpoint::pix_automatico::WEBHOOK_REC_EXCLUIR,
        ],
        _ => [
            endpoint::pix_automatico::WEBHOOK_COBR_CADASTRAR,
            endpoint::pix_automatico::WEBHOOK_COBR_CONSULTAR,
            endpoint::pix_automatico::WEBHOOK_COBR_EXCLUIR,
        ],
    }
}

#[test]
fn webhooks_are_the_documented_ones() {
    for tipo in TipoWebhookPixAutomatico::TODOS {
        let [cadastrar, consultar, excluir] = endpoints_do_webhook(tipo);
        assert_eq!(cadastrar.path, format!("/pix/v2/{}", tipo.as_str()));
        let operacao = spec::operation(&cadastrar);
        let corpo = spec::resolve(
            &spec::resolve(&operacao["requestBody"])["content"]["application/json"]["schema"],
        );
        let mut campos = BTreeSet::new();
        documentados(corpo, "", &mut campos);
        assert_eq!(campos, strings(&["webhookUrl"]), "{tipo}");
        // Inter posts the notifications to the address plus the suffix.
        let callbacks = operacao["callbacks"].as_object().unwrap();
        let (_, callback) = callbacks.iter().next().unwrap();
        let destino = callback.as_object().unwrap().keys().next().unwrap();
        assert_eq!(
            destino,
            &format!("{{$request.body#/webhookUrl}}{}", tipo.sufixo())
        );

        let resposta = &spec::operation(&consultar)["responses"];
        assert!(resposta["404"].is_object(), "{tipo}: 404 documentado");
        let exemplo = &spec::resolve(&resposta["200"])["content"]["application/json"]["examples"];
        for (_, exemplo) in exemplo.as_object().unwrap() {
            let exemplo = &spec::resolve(exemplo)["value"];
            let webhook: Webhook = serde_json::from_value(exemplo.clone()).unwrap();
            assert_eq!(&serde_json::to_value(&webhook).unwrap(), exemplo, "{tipo}");
        }
        assert!(spec::operation(&excluir)["responses"]["204"].is_object());
    }
}

#[test]
fn notifications_survive_a_round_trip() {
    let exemplo = example("recWebhookNotification1");
    let recs: NotificacaoRecs = serde_json::from_value(exemplo.clone()).unwrap();
    assert_eq!(recs.recs.len(), 1);
    assert_eq!(serde_json::to_value(&recs).unwrap(), como_escrita(&exemplo));
    let exemplo = example("cobRWebhookNotification1");
    let cobsr: NotificacaoCobsR = serde_json::from_value(exemplo.clone()).unwrap();
    assert_eq!(cobsr.cobsr.len(), 1);
    assert_eq!(
        serde_json::to_value(&cobsr).unwrap(),
        como_escrita(&exemplo)
    );
}

#[test]
fn notifications_keep_every_documented_field() {
    let rec: Rec = serde_json::from_value(example_for_schema("RecNotification")).unwrap();
    assert_lidos("RecNotification", &serde_json::to_value(&rec).unwrap(), &[]);
    let cobr: CobR = serde_json::from_value(example_for_schema("CobRNotification")).unwrap();
    assert_lidos(
        "CobRNotification",
        &serde_json::to_value(&cobr).unwrap(),
        &[],
    );
}

#[test]
fn sandbox_bodies_are_the_documented_ones() {
    let rec = schema("ChangeStatusRec");
    assert_eq!(
        enum_de(propriedade(rec, "status").unwrap()),
        strings(&[StatusRec::Aprovada.as_str(), StatusRec::Cancelada.as_str()])
    );
    assert_eq!(
        enum_de(propriedade(rec, "razao").unwrap()),
        codigos(
            RazaoCancelamentoRec::DOCUMENTADOS,
            RazaoCancelamentoRec::as_str
        )
    );
    assert_eq!(
        enum_de(propriedade(schema("ChangeStatusSolicRec"), "status").unwrap()),
        strings(&[
            StatusSolicRec::Aceita.as_str(),
            StatusSolicRec::Rejeitada.as_str()
        ])
    );
    let cobr = schema("ChangeStatusCobr");
    assert_eq!(
        enum_de(propriedade(cobr, "status").unwrap()),
        strings(&[StatusCobR::Cancelada.as_str()])
    );
    assert_eq!(
        enum_de(propriedade(cobr, "razao").unwrap()),
        codigos(
            RazaoCancelamentoCobR::DOCUMENTADOS,
            RazaoCancelamentoCobR::as_str
        )
    );
    assert_eq!(cobr["required"], json!(["status", "razao"]));

    let pagamento = schema("MakePaymentCobr");
    let mut campos = BTreeSet::new();
    documentados(pagamento, "", &mut campos);
    assert_eq!(campos, strings(&["chave", "cpfCnpj", "txId", "valor"]));
    // The amount goes as a number.
    assert_eq!(propriedade(pagamento, "valor").unwrap()["type"], "number");
    let pago: PagamentoSandbox =
        serde_json::from_value(example_for_schema("MakePaymentCobrResponse")).unwrap();
    assert!(pago.end_to_end_id().is_some());

    // The path of the charge names its parameter `txId`, which the
    // parameter list calls `txid`.
    assert!(
        endpoint::pix_automatico::SANDBOX_STATUS_COBR
            .path
            .contains("{txId}")
    );
    assert!(parametro(&endpoint::pix_automatico::SANDBOX_STATUS_COBR, "txid").is_object());
}
