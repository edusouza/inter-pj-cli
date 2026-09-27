//! Contract tests of the Pix API (`/pix/v2`): the models must agree with
//! Inter's OpenAPI specification (`spec/inter-empresas-openapi.json`).

mod spec;

use std::collections::BTreeSet;

use inter_pj::endpoint;
use inter_pj::pix::{Cob, PaginaCobs, PixRecebido};
use inter_pj::pix::{
    CobRevisada, CobSolicitada, Devedor, ITENS_POR_PAGINA_MAXIMO_PIX, InfoAdicional, LocCob,
    MAX_INFO_ADICIONAIS, MAX_SOLICITACAO_PAGADOR, ModalidadeAgente, Retirada, StatusCob,
    StatusDevolucao, TXID_MAXIMO, TXID_MINIMO, TipoCob, ValorCobRevisada, ValorRetirada,
};
use serde_json::Value;
use spec::{
    enum_values, example_for_schema, keys, parameter_names, parameters, property_names, resolve,
    schema, spec,
};

fn example(name: &str) -> &'static Value {
    let example = &spec()["components"]["examples"][name]["value"];
    assert!(!example.is_null(), "exemplo {name} não encontrado");
    example
}

/// A property of a schema, looking into `allOf`, `oneOf` and `anyOf`.
fn propriedade(schema: &'static Value, nome: &str) -> Option<&'static Value> {
    let schema = resolve(schema);
    if let Some(propriedade) = schema["properties"].get(nome) {
        return Some(resolve(propriedade));
    }
    ["allOf", "oneOf", "anyOf"]
        .iter()
        .flat_map(|chave| schema[*chave].as_array().into_iter().flatten())
        .find_map(|parte| propriedade(parte, nome))
}

/// Every documented path of a schema (`valor.retirada.saque.valor`),
/// following references, compositions and arrays.
fn documentados(schema: &'static Value, prefixo: &str, caminhos: &mut BTreeSet<String>) {
    let schema = resolve(schema);
    for chave in ["allOf", "oneOf", "anyOf"] {
        for parte in schema[chave].as_array().into_iter().flatten() {
            documentados(parte, prefixo, caminhos);
        }
    }
    if let Some(itens) = schema.get("items") {
        documentados(itens, prefixo, caminhos);
    }
    for (nome, propriedade) in schema["properties"].as_object().into_iter().flatten() {
        let caminho = format!("{prefixo}{nome}");
        caminhos.insert(caminho.clone());
        if caminhos.len() < 10_000 {
            documentados(propriedade, &format!("{caminho}."), caminhos);
        }
    }
}

/// The paths of a JSON document, as [`documentados`] names them.
fn caminhos(valor: &Value, prefixo: &str, saida: &mut BTreeSet<String>) {
    match valor {
        Value::Object(campos) => {
            for (nome, valor) in campos {
                let caminho = format!("{prefixo}{nome}");
                saida.insert(caminho.clone());
                caminhos(valor, &format!("{caminho}."), saida);
            }
        }
        Value::Array(itens) => {
            for item in itens {
                caminhos(item, prefixo, saida);
            }
        }
        _ => {}
    }
}

fn assert_documentado(nome: &str, enviado: &Value) {
    let mut esperados = BTreeSet::new();
    documentados(schema(nome), "", &mut esperados);
    let mut usados = BTreeSet::new();
    caminhos(enviado, "", &mut usados);
    let fora: Vec<&String> = usados.difference(&esperados).collect();
    assert!(fora.is_empty(), "campos fora do schema {nome}: {fora:?}");
}

fn strings(valores: &[impl AsRef<str>]) -> BTreeSet<String> {
    valores
        .iter()
        .map(|valor| valor.as_ref().to_owned())
        .collect()
}

fn enum_de(schema: &'static Value) -> BTreeSet<String> {
    schema["enum"]
        .as_array()
        .unwrap_or_else(|| panic!("sem enum: {schema}"))
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

// --- cob -------------------------------------------------------------------------

fn valor_revisado(original: &str) -> ValorCobRevisada {
    let mut valor = ValorCobRevisada::default();
    valor.original = Some(original.parse().unwrap());
    valor
}

/// `cobBody2`, the documented creation of an immediate charge.
fn cob_do_exemplo() -> CobSolicitada {
    let mut cob = CobSolicitada::new(
        "7d9f0335-8dcc-4054-9bf9-0dbd61d36906".parse().unwrap(),
        "37.00".parse().unwrap(),
    );
    cob.calendario.expiracao = Some(3600);
    cob.devedor = Some(Devedor::new(
        "12345678000195".parse().unwrap(),
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

#[test]
fn requests_are_the_documentation_examples() {
    assert_eq!(
        &serde_json::to_value(cob_do_exemplo()).unwrap(),
        example("cobBody2")
    );

    let mut revisao = CobRevisada::new();
    revisao.loc = Some(LocCob::new(7768));
    revisao.devedor = Some(Devedor::new(
        "12345678909".parse().unwrap(),
        "Francisco da Silva",
    ));
    revisao.valor = Some(valor_revisado("123.45"));
    revisao.solicitacao_pagador = Some("Cobrança dos serviços prestados.".to_owned());
    assert_eq!(
        &serde_json::to_value(&revisao).unwrap(),
        example("cobBody3")
    );

    let mut revisao = CobRevisada::new();
    revisao.valor = Some(valor_revisado("567.89"));
    revisao.solicitacao_pagador = Some("Informar cartão fidelidade".to_owned());
    assert_eq!(
        &serde_json::to_value(&revisao).unwrap(),
        example("cobBody4")
    );
    assert_eq!(
        &serde_json::to_value(CobRevisada::remocao()).unwrap(),
        example("cobBody5")
    );
}

#[test]
fn every_field_sent_is_documented() {
    let mut cob = cob_do_exemplo();
    cob.loc = Some(LocCob::new(789));
    cob.valor.original = "0.00".parse().unwrap();
    let mut saque = ValorRetirada::new(
        "20.00".parse().unwrap(),
        ModalidadeAgente::Agpss,
        "12345678",
    );
    saque.modalidade_alteracao = true;
    cob.valor.retirada = Some(Retirada::Saque(saque.clone()));
    cob.validar().unwrap();
    assert_documentado("CobSolicitada", &serde_json::to_value(&cob).unwrap());

    cob.valor.original = "10.00".parse().unwrap();
    saque.modalidade_agente = ModalidadeAgente::Agtec;
    cob.valor.retirada = Some(Retirada::Troco(saque));
    cob.validar().unwrap();
    assert_documentado("CobSolicitada", &serde_json::to_value(&cob).unwrap());

    let mut valor = valor_revisado("10.00");
    valor.modalidade_alteracao = Some(true);
    valor.retirada.clone_from(&cob.valor.retirada);
    let mut revisao = CobRevisada::remocao();
    revisao.calendario = Some(cob.calendario);
    revisao.devedor.clone_from(&cob.devedor);
    revisao.loc = cob.loc;
    revisao.valor = Some(valor);
    revisao.chave = Some(cob.chave.clone());
    revisao
        .solicitacao_pagador
        .clone_from(&cob.solicitacao_pagador);
    revisao.info_adicionais = Some(cob.info_adicionais.clone());
    revisao.validar().unwrap();
    assert_documentado("CobRevisada", &serde_json::to_value(&revisao).unwrap());
}

#[test]
fn documented_answers_survive_a_round_trip() {
    for nome in ["cobResponse1", "cobResponse2", "cobResponse3"] {
        let exemplo = example(nome);
        let cob: Cob = serde_json::from_value(exemplo.clone()).unwrap();
        assert_eq!(&serde_json::to_value(&cob).unwrap(), exemplo, "{nome}");
    }
    let pix: PixRecebido = serde_json::from_value(example("pixResponse1").clone()).unwrap();
    assert_eq!(
        &serde_json::to_value(&pix).unwrap(),
        example("pixResponse1")
    );
}

#[test]
fn answers_keep_every_documented_field() {
    let mut completo = example_for_schema("CobCompleta");
    // An amount without an example in the specification.
    completo["valor"]["original"] = "10.50".into();
    let cob: Cob = serde_json::from_value(completo).unwrap();
    let de_volta = serde_json::to_value(&cob).unwrap();
    assert_eq!(keys(&de_volta), property_names("CobCompleta"));
    assert_eq!(
        keys(&de_volta["pix"][0]),
        property_names("Pix"),
        "campos de Pix"
    );
    assert_eq!(
        keys(&de_volta["pix"][0]["devolucoes"][0]),
        property_names("Devolucao"),
        "campos de Devolucao"
    );

    // The charges were checked above; the generated example stops nesting
    // before their refunds.
    let mut pagina = example_for_schema("CobsConsultadas");
    pagina["cobs"] = Value::Array(Vec::new());
    let pagina: PaginaCobs = serde_json::from_value(pagina).unwrap();
    let de_volta = serde_json::to_value(&pagina).unwrap();
    assert_eq!(keys(&de_volta), property_names("CobsConsultadas"));
    assert_eq!(
        keys(&de_volta["parametros"]),
        property_names("ParametrosConsultaCob")
    );
    assert_eq!(
        keys(&de_volta["parametros"]["paginacao"]),
        property_names("Paginacao")
    );
}

#[test]
fn codes_are_the_documented_ones() {
    let status = propriedade(schema("CobGerada"), "status").unwrap();
    assert_eq!(
        strings(
            &StatusCob::DOCUMENTADOS
                .iter()
                .map(StatusCob::as_str)
                .collect::<Vec<_>>()
        ),
        enum_de(status)
    );
    let removida = propriedade(schema("CobRevisada"), "status").unwrap();
    assert_eq!(
        enum_de(removida),
        strings(&[StatusCob::RemovidaPeloUsuarioRecebedor.as_str()])
    );
    let devolucao = propriedade(schema("Devolucao"), "status").unwrap();
    assert_eq!(
        strings(
            &StatusDevolucao::DOCUMENTADOS
                .iter()
                .map(StatusDevolucao::as_str)
                .collect::<Vec<_>>()
        ),
        enum_de(devolucao)
    );
    assert_eq!(
        strings(
            &TipoCob::DOCUMENTADOS
                .iter()
                .map(TipoCob::as_str)
                .collect::<Vec<_>>()
        ),
        enum_values("TipoLocationCobEnum")
            .into_iter()
            .map(str::to_owned)
            .collect()
    );
    let saque = propriedade(schema("PixValorSaque"), "saque").unwrap();
    assert_eq!(
        strings(
            &ModalidadeAgente::DOCUMENTADOS
                .iter()
                .map(ModalidadeAgente::as_str)
                .collect::<Vec<_>>()
        ),
        enum_de(propriedade(saque, "modalidadeAgente").unwrap())
    );
}

#[test]
fn limits_are_the_documented_ones() {
    let txid = &parameters(&endpoint::pix::CRIAR_COB)["txid"]["schema"]["pattern"];
    assert_eq!(
        txid.as_str().unwrap(),
        format!("[a-zA-Z0-9]{{{TXID_MINIMO},{TXID_MAXIMO}}}")
    );
    let original = propriedade(schema("CobValor"), "original").unwrap();
    assert_eq!(original["pattern"], r"\d{1,10}\.\d{2}");
    let solicitada = schema("CobSolicitada");
    assert_eq!(
        propriedade(solicitada, "solicitacaoPagador").unwrap()["maxLength"],
        MAX_SOLICITACAO_PAGADOR
    );
    // The specification says "maximum" where it means "maxItems".
    assert_eq!(
        propriedade(solicitada, "infoAdicionais").unwrap()["maximum"],
        MAX_INFO_ADICIONAIS
    );
    let item = schema("InfoAdicionaisItem");
    assert_eq!(propriedade(item, "nome").unwrap()["maxLength"], 50);
    assert_eq!(propriedade(item, "valor").unwrap()["maxLength"], 200);
    let pessoa = schema("PessoaFisica");
    assert_eq!(propriedade(pessoa, "nome").unwrap()["maxLength"], 200);
    let itens = &parameters(&endpoint::pix::LISTAR_COBS)["paginacao.itensPorPagina"]["schema"];
    assert_eq!(itens["maximum"], ITENS_POR_PAGINA_MAXIMO_PIX);
}

#[test]
fn listing_filters_are_documented_parameters() {
    let documentados = parameter_names(&endpoint::pix::LISTAR_COBS);
    for nome in [
        "inicio",
        "fim",
        "cpf",
        "cnpj",
        "locationPresente",
        "status",
        "paginacao.paginaAtual",
        "paginacao.itensPorPagina",
    ] {
        assert!(documentados.contains(nome), "{nome}");
    }
}
