//! Contract tests of Pix Automático (`/pix/v2/rec`...): the models must agree
//! with Inter's OpenAPI specification (`spec/inter-empresas-openapi.json`).
//!
//! The examples of the specification carry document numbers: they are
//! replaced by synthetic ones when read, and the models are built from the
//! examples' own values, so no example data is copied here.

mod spec;

use std::collections::BTreeSet;

use chrono::NaiveDate;
use inter_pj::endpoint;
use inter_pj::pix::{Devedor, ITENS_POR_PAGINA_MAXIMO_PIX, TXID_MAXIMO, TXID_MINIMO};
use inter_pj::pix_automatico::{
    AtivacaoSolicitada, CalendarioRec, ID_REC_TAMANHO, MAX_CONTRATO, MAX_CONVENIO,
    MAX_NOME_DEVEDOR, MAX_OBJETO, PaginaRecs, Periodicidade, PoliticaRetentativa, Rec, RecRevisada,
    RecSolicitada, StatusRec, TipoJornada, ValorRec, VinculoRec,
};
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
/// each change `nome`, as the schema does not.
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
        // The confirmation requests are kept as received.
        .filter(|caminho| !caminho.contains("solicitacao."))
        .collect();
    assert!(
        faltando.is_empty(),
        "campos de {nome} perdidos: {faltando:?}"
    );
    let fora: Vec<&String> = lidos.difference(&esperados).collect();
    assert!(fora.is_empty(), "campos fora de {nome}: {fora:?}");
}

#[test]
fn answers_keep_every_documented_field() {
    let mut completa = example_for_schema("RecCompleta");
    // The specification's "example" of the object is a list of examples.
    completa["vinculo"]["objeto"] = json!("Mensalidade");
    let rec: Rec = serde_json::from_value(completa.clone()).unwrap();
    let de_volta = serde_json::to_value(&rec).unwrap();
    assert_lidos(
        "RecCompleta",
        &de_volta,
        &["vinculo.devedor.cnpj", "pagador.cnpj"],
    );
    assert_eq!(de_volta["solicitacao"], completa["solicitacao"]);

    let mut listada = example_for_schema("RecCompletaPesquisada");
    listada["vinculo"]["objeto"] = json!("Mensalidade");
    let mut pagina = example_for_schema("RecsConsultadas");
    pagina["recs"] = json!([listada]);
    let pagina: PaginaRecs = serde_json::from_value(pagina).unwrap();
    assert_lidos(
        "RecsConsultadas",
        &serde_json::to_value(&pagina).unwrap(),
        &["recs.vinculo.devedor.cnpj", "recs.pagador.cnpj"],
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
    assert_eq!(id["pattern"], format!("[a-zA-Z0-9]{{{ID_REC_TAMANHO}}}"));
    assert_eq!(id["minLength"], ID_REC_TAMANHO);
    assert_eq!(id["maxLength"], ID_REC_TAMANHO);
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
