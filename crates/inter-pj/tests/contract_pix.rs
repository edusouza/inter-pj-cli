//! Contract tests of the Pix API (`/pix/v2`): the models must agree with
//! Inter's OpenAPI specification (`spec/inter-empresas-openapi.json`).

mod spec;

use std::collections::BTreeSet;

use inter_pj::cobranca::Uf;
use inter_pj::endpoint;
use inter_pj::pix::LocationPix;
use inter_pj::pix::{
    AbatimentoCobv, Cob, Cobv, CobvRevisada, CobvSolicitada, DescontoCobv, DescontoData,
    DevedorCobv, JurosCobv, MAX_DESCONTOS_DATA_FIXA, ModalidadeJuros, MultaCobv, PaginaCobs,
    PaginaCobvs, PixRecebido, ValorCobvRevisada,
};
use inter_pj::pix::{
    CobRevisada, CobSolicitada, Devedor, ITENS_POR_PAGINA_MAXIMO_PIX, InfoAdicional, LocCob,
    MAX_INFO_ADICIONAIS, MAX_SOLICITACAO_PAGADOR, ModalidadeAgente, Retirada, StatusCob,
    StatusDevolucao, TXID_MAXIMO, TXID_MINIMO, TipoCob, ValorCobRevisada, ValorRetirada,
};
use inter_pj::pix::{
    CobvDoLote, CobvRevisadaDoLote, LoteCobv, LoteCobvRevisado, LoteCobvSolicitado, PaginaLocs,
    PaginaLotesCobv, StatusCobvLote, SumarioLoteCobv, Txid,
};
use inter_pj::pix::{
    Devolucao, DevolucaoSolicitada, ID_DEVOLUCAO_MAXIMO, MAX_DESCRICAO_DEVOLUCAO,
    NaturezaDevolucao, PaginaPixRecebidos,
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

// --- cobv ------------------------------------------------------------------------

fn dia(ano: i32, mes: u32, dia: u32) -> chrono::NaiveDate {
    chrono::NaiveDate::from_ymd_opt(ano, mes, dia).unwrap()
}

/// The documented creation of a charge with a due date (`cobBody1`), whose
/// codes the example writes as text.
fn cobv_do_exemplo() -> CobvSolicitada {
    let mut devedor = DevedorCobv::new("12345678909".parse().unwrap(), "Francisco da Silva");
    devedor.logradouro = Some("Alameda Souza, Numero 80, Bairro Braz".to_owned());
    devedor.cidade = Some("Recife".to_owned());
    devedor.uf = Some(Uf::Pe);
    devedor.cep = Some("70011750".to_owned());
    let mut cobv = CobvSolicitada::new(
        "5f84a4c5-c5cb-4599-9f13-7eb4d419dacc".parse().unwrap(),
        "123.45".parse().unwrap(),
        dia(2020, 12, 31),
        devedor,
    );
    cobv.calendario.validade_apos_vencimento = Some(30);
    cobv.loc = Some(LocCob::new(789));
    cobv.valor.multa = Some(MultaCobv::Percentual("15".parse().unwrap()));
    cobv.valor.juros = Some(JurosCobv::new(
        ModalidadeJuros::PercentualDiaDiasCorridos,
        "2".parse().unwrap(),
    ));
    cobv.valor.desconto = Some(DescontoCobv::ValorFixoAteDatas(vec![DescontoData::new(
        dia(2020, 11, 30),
        "30".parse().unwrap(),
    )]));
    cobv.solicitacao_pagador = Some("Cobrança dos serviços prestados.".to_owned());
    cobv
}

/// Codes written as text (`"2"`) and as numbers (`2`) compare equal.
fn codigos_como_numeros(valor: &Value) -> Value {
    match valor {
        Value::Object(campos) => Value::Object(
            campos
                .iter()
                .map(|(nome, valor)| {
                    let valor = match (nome.as_str(), valor) {
                        ("modalidade", Value::String(texto)) => texto
                            .parse::<u64>()
                            .map_or_else(|_| valor.clone(), Value::from),
                        _ => codigos_como_numeros(valor),
                    };
                    (nome.clone(), valor)
                })
                .collect(),
        ),
        Value::Array(itens) => Value::Array(itens.iter().map(codigos_como_numeros).collect()),
        outro => outro.clone(),
    }
}

#[test]
fn charges_with_a_due_date_are_the_documentation_examples() {
    let cobv = cobv_do_exemplo();
    cobv.validar().unwrap();
    assert_eq!(
        serde_json::to_value(&cobv).unwrap(),
        codigos_como_numeros(example("cobBody1"))
    );
    // Revisions share the examples of the immediate charge.
    let mut valor = ValorCobvRevisada::default();
    valor.original = Some("567.89".parse().unwrap());
    let mut revisao = CobvRevisada::new();
    revisao.valor = Some(valor);
    revisao.solicitacao_pagador = Some("Informar cartão fidelidade".to_owned());
    assert_eq!(
        &serde_json::to_value(&revisao).unwrap(),
        example("cobBody4")
    );
    assert_eq!(
        &serde_json::to_value(CobvRevisada::remocao()).unwrap(),
        example("cobBody5")
    );
}

#[test]
fn every_charge_with_a_due_date_field_is_documented() {
    let mut cobv = cobv_do_exemplo();
    cobv.devedor.email = Some("financeiro@exemplo.com.br".to_owned());
    cobv.valor.abatimento = Some(AbatimentoCobv::ValorFixo("5".parse().unwrap()));
    cobv.info_adicionais = vec![InfoAdicional::new("Pedido", "123")];
    for desconto in [
        DescontoCobv::PercentualAteDatas(vec![DescontoData::new(
            dia(2020, 11, 30),
            "5".parse().unwrap(),
        )]),
        DescontoCobv::ValorPorDiaCorrido("0.5".parse().unwrap()),
    ] {
        cobv.valor.desconto = Some(desconto);
        cobv.validar().unwrap();
        assert_documentado("CobVSolicitada", &serde_json::to_value(&cobv).unwrap());
    }
    let mut revisao = CobvRevisada::remocao();
    revisao.calendario = Some(cobv.calendario);
    revisao.devedor = Some(cobv.devedor.clone());
    revisao.loc = cobv.loc;
    let mut valor = ValorCobvRevisada::default();
    valor.original = Some(cobv.valor.original);
    valor.multa = cobv.valor.multa;
    valor.juros = cobv.valor.juros;
    valor.abatimento = cobv.valor.abatimento;
    valor.desconto.clone_from(&cobv.valor.desconto);
    revisao.valor = Some(valor);
    revisao.chave = Some(cobv.chave.clone());
    revisao
        .solicitacao_pagador
        .clone_from(&cobv.solicitacao_pagador);
    revisao.info_adicionais = Some(cobv.info_adicionais.clone());
    revisao.validar().unwrap();
    assert_documentado("CobVRevisada", &serde_json::to_value(&revisao).unwrap());
}

#[test]
fn charges_with_a_due_date_keep_every_documented_field() {
    let exemplo = example("cobResponse4");
    let cobv: Cobv = serde_json::from_value(exemplo.clone()).unwrap();
    assert_eq!(&serde_json::to_value(&cobv).unwrap(), exemplo);

    let mut completo = example_for_schema("CobVCompleta");
    completo["valor"]["original"] = "10.50".into();
    let cobv: Cobv = serde_json::from_value(completo).unwrap();
    let de_volta = serde_json::to_value(&cobv).unwrap();
    assert_eq!(keys(&de_volta), property_names("CobVCompleta"));
    assert_eq!(
        keys(&de_volta["valor"]),
        ["abatimento", "desconto", "juros", "multa", "original"]
            .map(str::to_owned)
            .into_iter()
            .collect()
    );
    let mut pagina = example_for_schema("CobsVConsultadas");
    pagina["cobs"] = Value::Array(Vec::new());
    let pagina: PaginaCobvs = serde_json::from_value(pagina).unwrap();
    assert_eq!(
        keys(&serde_json::to_value(&pagina).unwrap()),
        property_names("CobsVConsultadas")
    );
}

#[test]
fn charge_with_a_due_date_limits_are_the_documented_ones() {
    let valor = resolve(propriedade(schema("CobVSolicitada"), "valor").unwrap());
    let desconto = propriedade(valor, "desconto").unwrap();
    let datas = propriedade(desconto, "descontoDataFixa").unwrap();
    assert_eq!(datas["maxItems"], MAX_DESCONTOS_DATA_FIXA);
    let juros = propriedade(valor, "juros").unwrap();
    assert_eq!(
        propriedade(juros, "modalidade").unwrap()["maximum"],
        ModalidadeJuros::TODAS.len()
    );
    let multa = propriedade(valor, "multa").unwrap();
    assert_eq!(propriedade(multa, "modalidade").unwrap()["maximum"], 2);
    let abatimento = propriedade(valor, "abatimento").unwrap();
    assert_eq!(propriedade(abatimento, "modalidade").unwrap()["maximum"], 2);
    let listagem = parameter_names(&endpoint::pix::LISTAR_COBVS);
    assert!(listagem.contains("loteCobVId"));
}

// --- Pix recebidos e devoluções ---------------------------------------------------

#[test]
fn refunds_are_the_documentation_examples() {
    let devolucao = DevolucaoSolicitada::new("7.89".parse().unwrap());
    assert_eq!(
        &serde_json::to_value(&devolucao).unwrap(),
        example("devolucaoSolicitada1")
    );
    let mut completa = devolucao;
    completa.natureza = Some(NaturezaDevolucao::Retirada);
    completa.descricao = Some("Troco devolvido".to_owned());
    assert_documentado(
        "DevolucaoSolicitada",
        &serde_json::to_value(&completa).unwrap(),
    );
    for nome in ["devolucaoResponse1", "devolucaoResponse2"] {
        let exemplo = example(nome);
        let devolucao: Devolucao = serde_json::from_value(exemplo.clone()).unwrap();
        assert_eq!(
            &serde_json::to_value(&devolucao).unwrap(),
            exemplo,
            "{nome}"
        );
    }
    let pix: PixRecebido = serde_json::from_value(example("pixResponse2").clone()).unwrap();
    assert_eq!(
        &serde_json::to_value(&pix).unwrap(),
        example("pixResponse2")
    );
}

#[test]
fn pages_of_pix_received_keep_every_documented_field() {
    let mut pagina = example_for_schema("PixConsultados");
    pagina["pix"] = Value::Array(Vec::new());
    let pagina: PaginaPixRecebidos = serde_json::from_value(pagina).unwrap();
    let de_volta = serde_json::to_value(&pagina).unwrap();
    assert_eq!(keys(&de_volta), property_names("PixConsultados"));
    assert_eq!(
        keys(&de_volta["parametros"]),
        property_names("ParametrosConsultaPix")
    );
}

#[test]
fn refund_codes_and_limits_are_the_documented_ones() {
    assert_eq!(
        strings(&NaturezaDevolucao::TODAS.map(NaturezaDevolucao::as_str)),
        enum_values("DevolucaoSolicitadaNatureza")
            .into_iter()
            .map(str::to_owned)
            .collect()
    );
    assert_eq!(
        schema("DevolucaoId")["pattern"],
        format!("[a-zA-Z0-9]{{1,{ID_DEVOLUCAO_MAXIMO}}}")
    );
    assert_eq!(
        propriedade(schema("DevolucaoSolicitada"), "descricao").unwrap()["maxLength"],
        MAX_DESCRICAO_DEVOLUCAO
    );
    let listagem = parameter_names(&endpoint::pix::LISTAR_RECEBIDOS);
    for nome in [
        "inicio",
        "fim",
        "txId",
        "txIdPresente",
        "devolucaoPresente",
        "cpf",
        "cnpj",
    ] {
        assert!(listagem.contains(nome), "{nome}");
    }
}

// --- locations, lotes e sandbox ---------------------------------------------------

#[test]
fn locations_are_the_documented_ones() {
    for nome in [
        "payloadLocationResponse1",
        "payloadLocationResponse2",
        "payloadLocationResponse3",
    ] {
        let exemplo = example(nome);
        let loc: LocationPix = serde_json::from_value(exemplo.clone()).unwrap();
        assert_eq!(&serde_json::to_value(&loc).unwrap(), exemplo, "{nome}");
    }
    let completa: LocationPix =
        serde_json::from_value(example_for_schema("PayloadLocationCompleta")).unwrap();
    assert_eq!(
        keys(&serde_json::to_value(&completa).unwrap()),
        property_names("PayloadLocationCompleta")
    );
    let mut pagina = example_for_schema("PayloadLocationConsultadas");
    pagina["loc"] = Value::Array(Vec::new());
    let pagina: PaginaLocs = serde_json::from_value(pagina).unwrap();
    let de_volta = serde_json::to_value(&pagina).unwrap();
    assert_eq!(
        keys(&de_volta),
        property_names("PayloadLocationConsultadas")
    );
    let listagem = parameter_names(&endpoint::pix::LISTAR_LOCS);
    for nome in ["inicio", "fim", "txIdPresente", "tipoCob"] {
        assert!(listagem.contains(nome), "{nome}");
    }
}

fn cobv_do_lote(txid: &str, nome: &str, cep: &str, loc: u64) -> CobvDoLote {
    let mut devedor = DevedorCobv::new("12345678909".parse().unwrap(), nome);
    devedor.cidade = Some("Recife".to_owned());
    devedor.uf = Some(Uf::Pe);
    devedor.cep = Some(cep.to_owned());
    let mut cobv = CobvSolicitada::new(
        "7c084cd4-54af-4172-a516-a7d1a12b75cc".parse().unwrap(),
        "100.00".parse().unwrap(),
        dia(2020, 12, 31),
        devedor,
    );
    cobv.calendario.validade_apos_vencimento = Some(30);
    cobv.loc = Some(LocCob::new(loc));
    cobv.solicitacao_pagador = Some("Informar matrícula".to_owned());
    CobvDoLote::new(txid.parse().unwrap(), cobv)
}

#[test]
fn batches_are_the_documentation_examples() {
    let mut um = cobv_do_lote(
        "fb2761260e554ad593c7226beb5cb650",
        "João Souza",
        "70011750",
        789,
    );
    um.cobv.devedor.logradouro = Some("Alameda Souza, Numero 80, Bairro Braz".to_owned());
    let mut dois = cobv_do_lote(
        "7978c0c97ea847e78e8849634473c1f1",
        "Manoel Silva",
        "70055751",
        57221,
    );
    dois.cobv.devedor.logradouro = Some("Rua 15, Numero 1, Bairro Campo Grande".to_owned());
    let lote = LoteCobvSolicitado::new("Cobranças dos alunos do turno vespertino", vec![um, dois]);
    lote.validar().unwrap();
    assert_eq!(
        &serde_json::to_value(&lote).unwrap(),
        example("loteCobVBody1")
    );

    let revisao = |txid: &str| {
        let mut revisao = CobvRevisada::new();
        revisao.calendario = Some(inter_pj::pix::CalendarioCobv::new(dia(2020, 1, 10)));
        let mut valor = ValorCobvRevisada::default();
        valor.original = Some("110.00".parse().unwrap());
        revisao.valor = Some(valor);
        CobvRevisadaDoLote::new(txid.parse::<Txid>().unwrap(), revisao)
    };
    let revisado = LoteCobvRevisado::new(vec![
        revisao("fb2761260e554ad593c7226beb5cb650"),
        revisao("7978c0c97ea847e78e8849634473c1f1"),
    ]);
    revisado.validar().unwrap();
    assert_eq!(
        &serde_json::to_value(&revisado).unwrap(),
        example("loteCobVBodyRevisado1")
    );
}

#[test]
fn batch_answers_survive_a_round_trip() {
    for nome in [
        "loteCobVResponse1",
        "loteCobVResponse2",
        "loteCobVByIdStatusResponse1",
    ] {
        let exemplo = example(nome);
        let lote: LoteCobv = serde_json::from_value(exemplo.clone()).unwrap();
        assert_eq!(&serde_json::to_value(&lote).unwrap(), exemplo, "{nome}");
    }
    let sumario: SumarioLoteCobv =
        serde_json::from_value(example_for_schema("SummaryLoteCobV")).unwrap();
    assert_eq!(
        keys(&serde_json::to_value(&sumario).unwrap()),
        property_names("SummaryLoteCobV")
    );
    let mut pagina = example_for_schema("LotesCobVConsultados");
    pagina["lotes"] = Value::Array(Vec::new());
    let pagina: PaginaLotesCobv = serde_json::from_value(pagina).unwrap();
    assert_eq!(
        keys(&serde_json::to_value(&pagina).unwrap()),
        property_names("LotesCobVConsultados")
    );
    assert_eq!(
        strings(
            &StatusCobvLote::DOCUMENTADOS
                .iter()
                .map(StatusCobvLote::as_str)
                .collect::<Vec<_>>()
        ),
        enum_values("SituacaoCobranca")
            .into_iter()
            .map(str::to_owned)
            .collect()
    );
}

#[test]
fn sandbox_payments_are_documented() {
    for endpoint in [
        endpoint::pix::PAGAR_COB_SANDBOX,
        endpoint::pix::PAGAR_COBV_SANDBOX,
        endpoint::pix::PAGAR_QR_CODE_SANDBOX,
    ] {
        let operacao = spec::operation(&endpoint);
        let descricao = format!(
            "{} {}",
            operacao["summary"].as_str().unwrap_or_default(),
            operacao["description"].as_str().unwrap_or_default()
        );
        assert!(descricao.contains("Sandbox"), "{endpoint}: {descricao}");
    }
    assert_eq!(property_names("PagarCobrancaPix"), strings(&["valor"]));
    assert_eq!(
        property_names("MakePaymentCobCobv"),
        strings(&["qrCode", "valor"])
    );
    assert_eq!(
        property_names("PagarCobrancaPixResponse"),
        strings(&["e2e"])
    );
    assert_eq!(
        property_names("MakePaymentCobCobvResponse"),
        strings(&["endToEnd"])
    );
}
