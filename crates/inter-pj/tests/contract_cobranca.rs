//! Contract tests of the Cobrança API (`/cobranca/v3`): the models must agree
//! with Inter's OpenAPI specification (`spec/inter-empresas-openapi.json`).

mod spec;

use std::collections::BTreeSet;

use chrono::NaiveDate;
use inter_pj::cobranca::{
    BeneficiarioFinal, CobrancaDetalhada, ConsultaEdicao, DadosCobranca, Desconto, EdicaoCobranca,
    EmissaoCobranca, EmissaoCobrancaError, FiltrarDataPor, FormaRecebimento,
    ITENS_POR_PAGINA_MAXIMO, ItemSumario, MAX_CARACTERES_LINHA, MAX_DIAS_AGENDA,
    MAX_LINHAS_MENSAGEM, MAX_MOTIVO_CANCELAMENTO, MAX_SEU_NUMERO, Mora, Multa, NotaFiscal,
    OrdenarCobrancasPor, OrigemRecebimento, Pagador, PagarCom, PaginaCobrancas, SituacaoCobranca,
    SolicitacaoCobranca, SolicitacaoEdicao, StatusEdicao, TipoCobranca, Uf, VALOR_MAXIMO,
    VALOR_MINIMO,
};
use inter_pj::endpoint;
use rust_decimal::Decimal;
use serde_json::{Map, Value, json};
use spec::{
    enum_values, example_for_schema, keys, operation, parameter_names, parameters, property_names,
    resolve, schema, spec,
};

fn example(name: &str) -> &'static Value {
    let example = &spec()["components"]["examples"][name]["value"];
    assert!(!example.is_null(), "exemplo {name} não encontrado");
    example
}

/// Properties of an object schema given inline (`cobranca` inside
/// `CobrancaDetalhadaResponseBody`), including the ones of `allOf`.
fn inline_properties(schema: &'static Value) -> BTreeSet<String> {
    let schema = resolve(schema);
    let mut names: BTreeSet<String> = schema["properties"]
        .as_object()
        .map(|properties| properties.keys().cloned().collect())
        .unwrap_or_default();
    for part in schema["allOf"].as_array().into_iter().flatten() {
        names.extend(inline_properties(part));
    }
    names
}

fn dia(ano: i32, mes: u32, dia: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(ano, mes, dia).unwrap()
}

fn dec(texto: &str) -> Decimal {
    texto.parse().unwrap()
}

/// A charge with every field, all synthetic.
fn completa() -> EmissaoCobranca {
    let mut pagador = Pagador::new(
        "123.456.789-09".parse().unwrap(),
        "Nome do pagador",
        "Avenida Brasil",
        "Belo Horizonte",
        Uf::Mg,
        "30110000",
    );
    pagador.numero = Some("3456".to_owned());
    pagador.complemento = Some("apartamento 3 bloco 4".to_owned());
    pagador.bairro = Some("Centro".to_owned());
    pagador.email = Some("nome.sobrenome@empresa.example".to_owned());
    pagador.ddd = Some("31".to_owned());
    pagador.telefone = Some("999999999".to_owned());
    let mut cobranca = EmissaoCobranca::new("123456", dec("2.5"), dia(2026, 10, 20), pagador);
    cobranca.num_dias_agenda = 60;
    cobranca.desconto = Some(Desconto::Percentual {
        taxa: dec("3"),
        quantidade_dias: 7,
    });
    cobranca.multa = Some(Multa::Percentual { taxa: dec("2") });
    cobranca.mora = Some(Mora::TaxaMensal { taxa: dec("5") });
    cobranca.mensagem = (1..=5).map(|n| format!("mensagem {n}")).collect();
    cobranca.beneficiario_final = Some(BeneficiarioFinal {
        cpf_cnpj: "12.345.678/0001-95".parse().unwrap(),
        nome: "Nome do beneficiário".to_owned(),
        endereco: "Avenida Brasil, 1200".to_owned(),
        bairro: Some("Centro".to_owned()),
        cidade: "Belo Horizonte".to_owned(),
        uf: Uf::Mg,
        cep: "30110000".to_owned(),
    });
    cobranca.formas_recebimento = vec![FormaRecebimento::Boleto, FormaRecebimento::Pix];
    cobranca.nota_fiscal = Some(NotaFiscal {
        chave_nfe: "31260912345678000195550010000123451123456786".to_owned(),
        numero: 12_345,
        serie: 1,
        data_emissao: dia(2026, 1, 15),
        parcela: Some(1),
        natureza_operacao: Some("Venda".to_owned()),
    });
    cobranca
}

#[test]
fn cobranca_endpoints_are_registered() {
    for endpoint in [
        endpoint::cobranca::EMITIR,
        endpoint::cobranca::CONSULTAR,
        endpoint::cobranca::LISTAR,
        endpoint::cobranca::SUMARIO,
        endpoint::cobranca::PDF,
        endpoint::cobranca::CANCELAR,
        endpoint::cobranca::EDITAR,
        endpoint::cobranca::EDICAO,
        endpoint::cobranca::PAGAR,
    ] {
        assert!(endpoint::ALL.contains(&endpoint), "{endpoint}");
        operation(&endpoint);
    }
}

/// Every field of the request, nested ones included, has the name the API
/// documents: the body of a complete charge has the same keys as the
/// specification's example.
#[test]
fn emission_body_has_the_documented_shape() {
    let cobranca = completa();
    assert_eq!(cobranca.validar(), Ok(()));
    let body = serde_json::to_value(&cobranca).unwrap();
    let documentado = example("exemploEmitirCobranca");
    assert_same_shape(&body, documentado, "EmitirCobrancaRequestBody");
    assert_eq!(keys(&body), property_names("EmitirCobrancaRequestBody"));
    assert_eq!(keys(&body["pagador"]), property_names("Pagador"), "pagador");
    assert_eq!(
        keys(&body["beneficiarioFinal"]),
        property_names("BeneficiarioBase")
    );
    assert_eq!(keys(&body["mensagem"]), property_names("Mensagem"));
    assert_eq!(keys(&body["notaFiscal"]), property_names("NotaFiscal"));
    for (campo, schema) in [
        ("desconto", "DescontoTaxa"),
        ("multa", "MultaTaxa"),
        ("mora", "MoraTaxa"),
    ] {
        assert_eq!(keys(&body[campo]), property_names(schema), "{campo}");
    }
    // The fixed-amount forms have `valor` instead of `taxa`.
    let mut valores = completa();
    valores.desconto = Some(Desconto::ValorFixo {
        valor: dec("1"),
        quantidade_dias: 0,
    });
    valores.multa = Some(Multa::ValorFixo { valor: dec("1") });
    valores.mora = Some(Mora::ValorDia { valor: dec("1") });
    let body = serde_json::to_value(&valores).unwrap();
    for (campo, schema) in [
        ("desconto", "DescontoValor"),
        ("multa", "MultaValor"),
        ("mora", "MoraValor"),
    ] {
        assert_eq!(keys(&body[campo]), property_names(schema), "{campo}");
    }
}

/// Same keys, recursively, in two documents.
fn assert_same_shape(ours: &Value, theirs: &Value, onde: &str) {
    match (ours, theirs) {
        (Value::Object(a), Value::Object(b)) => {
            let ka: BTreeSet<&String> = a.keys().collect();
            let kb: BTreeSet<&String> = b.keys().collect();
            assert_eq!(ka, kb, "{onde}");
            for (key, value) in a {
                assert_same_shape(value, &b[key], &format!("{onde}.{key}"));
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if let (Some(a), Some(b)) = (a.first(), b.first()) {
                assert_same_shape(a, b, onde);
            }
        }
        _ => {}
    }
}

#[test]
fn emission_codes_match_the_spec() {
    let codigos = |body: &Value| body["codigo"].as_str().unwrap().to_owned();
    let desconto = |d: Desconto| codigos(&serde_json::to_value(d).unwrap());
    let multa = |m: Multa| codigos(&serde_json::to_value(m).unwrap());
    let mora = |m: Mora| codigos(&serde_json::to_value(m).unwrap());
    let um = dec("1");
    let documentados = |schema_name: &str| -> BTreeSet<String> {
        resolve(&schema(schema_name)["properties"]["codigo"])["enum"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(
        BTreeSet::from([
            desconto(Desconto::Percentual {
                taxa: um,
                quantidade_dias: 0
            }),
            desconto(Desconto::ValorFixo {
                valor: um,
                quantidade_dias: 0
            }),
        ]),
        documentados("Desconto")
    );
    assert_eq!(
        BTreeSet::from([
            multa(Multa::Percentual { taxa: um }),
            multa(Multa::ValorFixo { valor: um })
        ]),
        documentados("Multa")
    );
    assert_eq!(
        BTreeSet::from([
            mora(Mora::TaxaMensal { taxa: um }),
            mora(Mora::ValorDia { valor: um })
        ]),
        documentados("Mora")
    );
    let ufs: BTreeSet<&str> = Uf::TODAS.iter().map(|uf| uf.as_str()).collect();
    assert_eq!(ufs, enum_values("EnumUF"));
    let formas: BTreeSet<&str> = [
        FormaRecebimento::Boleto,
        FormaRecebimento::Pix,
        FormaRecebimento::SemFormaPagamento,
    ]
    .iter()
    .map(|f| f.as_str())
    .collect();
    assert_eq!(formas, enum_values("FormaRecebimentoEnum"));
    let pessoas: BTreeSet<&str> =
        resolve(&schema("PagadorBase")["properties"]["tipoPessoa"])["enum"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
    assert_eq!(pessoas, BTreeSet::from(["FISICA", "JURIDICA"]));
}

#[test]
fn emission_limits_match_the_spec() {
    let corpo = &schema("EmitirCobrancaRequestBody")["properties"];
    let numero = |value: &Value| dec(&value.to_string());
    assert_eq!(VALOR_MINIMO, numero(&corpo["valorNominal"]["minimum"]));
    assert_eq!(VALOR_MAXIMO, numero(&corpo["valorNominal"]["maximum"]));
    assert_eq!(
        MAX_SEU_NUMERO,
        usize::try_from(corpo["seuNumero"]["maxLength"].as_u64().unwrap()).unwrap()
    );
    assert_eq!(
        u64::from(MAX_DIAS_AGENDA),
        corpo["numDiasAgenda"]["maximum"].as_u64().unwrap()
    );
    let mensagem = &schema("Mensagem")["properties"];
    assert_eq!(MAX_LINHAS_MENSAGEM, mensagem.as_object().unwrap().len());
    for linha in mensagem.as_object().unwrap().values() {
        assert_eq!(
            MAX_CARACTERES_LINHA,
            usize::try_from(linha["maxLength"].as_u64().unwrap()).unwrap()
        );
    }
}

/// Every text of the payer accepts its documented maximum and refuses one
/// character more.
#[test]
fn payer_limits_match_the_spec() {
    type Campo = fn(&mut Pagador, String);
    let campos: [(&str, &str, Campo); 7] = [
        ("PagadorBase", "nome", |p, v| p.nome = v),
        ("PagadorBase", "endereco", |p, v| p.endereco = v),
        ("PagadorBase", "bairro", |p, v| p.bairro = Some(v)),
        ("PagadorBase", "cidade", |p, v| p.cidade = v),
        ("Pagador", "numero", |p, v| p.numero = Some(v)),
        ("Pagador", "complemento", |p, v| p.complemento = Some(v)),
        ("Pagador", "email", |p, v| p.email = Some(v)),
    ];
    for (nome_schema, campo, definir) in campos {
        let propriedade = inline_property(nome_schema, campo);
        let maximo = usize::try_from(propriedade["maxLength"].as_u64().unwrap()).unwrap();
        let texto = |tamanho: usize| {
            if campo == "email" {
                const DOMINIO: &str = "@empresa.example";
                format!("{}{DOMINIO}", "a".repeat(tamanho - DOMINIO.len()))
            } else {
                "a".repeat(tamanho)
            }
        };
        for (tamanho, aceito) in [(maximo, true), (maximo + 1, false)] {
            let mut cobranca = completa();
            definir(&mut cobranca.pagador, texto(tamanho));
            let resultado = cobranca.validar();
            assert_eq!(
                resultado.is_ok(),
                aceito,
                "{campo} com {tamanho}: {resultado:?}"
            );
            if let Err(erro) = resultado {
                assert_eq!(erro.campo(), format!("pagador.{campo}"));
            }
        }
    }
    let cep = inline_property("PagadorBase", "cep");
    assert_eq!(cep["minLength"], 8);
    assert_eq!(cep["maxLength"], 8);
    let telefone = inline_property("Pagador", "telefone");
    assert_eq!(telefone["maxLength"], 9);
    let mut cobranca = completa();
    cobranca.pagador.telefone = Some("1234567890".to_owned());
    assert_eq!(cobranca.validar(), Err(EmissaoCobrancaError::Telefone));
}

/// A property of a schema, looking into `allOf`.
fn inline_property(nome_schema: &str, campo: &str) -> &'static Value {
    fn procurar(schema: &'static Value, campo: &str) -> Option<&'static Value> {
        let schema = resolve(schema);
        if let Some(propriedade) = schema["properties"].get(campo) {
            return Some(resolve(propriedade));
        }
        schema["allOf"]
            .as_array()
            .into_iter()
            .flatten()
            .find_map(|parte| procurar(parte, campo))
    }
    procurar(schema(nome_schema), campo)
        .unwrap_or_else(|| panic!("{nome_schema}.{campo} não encontrado"))
}

#[test]
fn issued_charge_answer_accepts_the_example() {
    let resposta: SolicitacaoCobranca =
        serde_json::from_value(example("exemploRetornoEmitirCobranca").clone()).unwrap();
    assert!(resposta.codigo_solicitacao.is_some());
    assert_eq!(
        keys(&serde_json::to_value(&resposta).unwrap()),
        property_names("EmitirCobrancaAsyncResponse")
    );
}

/// The detail example comes back field by field (amounts as numbers).
#[test]
fn charge_detail_maps_every_documented_field() {
    let documentado = example("exemploRetornoRecuperarCobranca");
    let cobranca: CobrancaDetalhada = serde_json::from_value(documentado.clone()).unwrap();
    assert_eq!(cobranca.cobranca.situacao, Some(SituacaoCobranca::Recebido));
    let de_volta = serde_json::to_value(&cobranca).unwrap();
    assert_eq!(normalizar(&de_volta), normalizar(documentado));

    let corpo = schema("CobrancaDetalhadaResponseBody");
    assert_eq!(keys(&de_volta), inline_properties(corpo));
    let mut esperado = inline_properties(&corpo["properties"]["cobranca"]);
    // Only cancelled charges have a reason, and the example is paid.
    esperado.remove("motivoCancelamento");
    assert_eq!(keys(&de_volta["cobranca"]), esperado);
    let cancelada: DadosCobranca =
        serde_json::from_value(json!({"motivoCancelamento": "Pedido cancelado"})).unwrap();
    assert_eq!(
        cancelada.motivo_cancelamento.as_deref(),
        Some("Pedido cancelado")
    );
    for (campo, propriedades) in [
        ("boleto", inline_properties(&corpo["properties"]["boleto"])),
        ("pix", inline_properties(&corpo["properties"]["pix"])),
        (
            "notaFiscal",
            inline_properties(&corpo["properties"]["notaFiscal"]),
        ),
    ] {
        assert_eq!(keys(&de_volta[campo]), propriedades, "{campo}");
    }
    assert_eq!(
        keys(&de_volta["cobranca"]["pagador"]),
        property_names("Pagador")
    );
}

/// The items of the listing have the shape of the detail, with fewer fields.
#[test]
fn listed_charges_accept_the_example() {
    let pagina = example("exemploRetornoListaCobrancas");
    let item: CobrancaDetalhada = serde_json::from_value(pagina["cobrancas"][0].clone()).unwrap();
    assert_eq!(item.cobranca.situacao, Some(SituacaoCobranca::AReceber));
    assert_eq!(item.cobranca.valor_nominal, Some(dec("123.45")));
    assert_eq!(
        normalizar(&serde_json::to_value(&item).unwrap()),
        normalizar(&pagina["cobrancas"][0])
    );
}

#[test]
fn charge_enums_match_the_spec() {
    let documentados = |nome: &str| -> BTreeSet<String> {
        enum_values(nome).into_iter().map(str::to_owned).collect()
    };
    let situacoes = SituacaoCobranca::DOCUMENTADOS
        .iter()
        .map(SituacaoCobranca::as_str);
    let tipos = TipoCobranca::DOCUMENTADOS.iter().map(TipoCobranca::as_str);
    let origens = OrigemRecebimento::DOCUMENTADOS
        .iter()
        .map(OrigemRecebimento::as_str);
    assert_eq!(
        situacoes.map(str::to_owned).collect::<BTreeSet<_>>(),
        documentados("SituacaoCobrancaEnum")
    );
    assert_eq!(
        tipos.map(str::to_owned).collect::<BTreeSet<_>>(),
        documentados("TipoCobrancaEnum")
    );
    assert_eq!(
        origens.map(str::to_owned).collect::<BTreeSet<_>>(),
        documentados("OrigemRecebimentoEnum")
    );
}

/// Amounts sent as text (`"1234.56"`) as numbers, so a document can be
/// compared with our serialization.
fn normalizar(value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| {
                    let value = match value {
                        Value::String(texto) if key.starts_with("valor") => texto
                            .parse::<f64>()
                            .map_or_else(|_| value.clone(), |n| json!(n)),
                        other => normalizar(other),
                    };
                    (key.clone(), value)
                })
                .collect::<Map<String, Value>>(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(normalizar).collect()),
        Value::Number(n) => json!(n.as_f64()),
        other => other.clone(),
    }
}

/// The parameters `listar` and `sumario` send (see `tests/cobranca.rs`) are
/// the documented ones, and the summary has no order nor pages.
#[test]
fn listing_and_summary_parameters_are_documented() {
    let filtro = [
        "dataInicial",
        "dataFinal",
        "filtrarDataPor",
        "situacao",
        "pessoaPagadora",
        "cpfCnpjPessoaPagadora",
        "seuNumero",
        "tipoCobranca",
    ];
    let listagem = [
        "ordenarPor",
        "tipoOrdenacao",
        "paginacao.paginaAtual",
        "paginacao.itensPorPagina",
    ];
    let sem_conta = |endpoint| {
        let mut nomes = parameter_names(&endpoint);
        assert!(nomes.remove("x-conta-corrente"));
        nomes
    };
    let nossos = |nomes: &[&str]| nomes.iter().map(|&n| n.to_owned()).collect::<BTreeSet<_>>();
    assert_eq!(
        sem_conta(endpoint::cobranca::LISTAR),
        nossos(&[&filtro[..], &listagem[..]].concat())
    );
    assert_eq!(sem_conta(endpoint::cobranca::SUMARIO), nossos(&filtro));
    let parametros = parameters(&endpoint::cobranca::LISTAR);
    for obrigatorio in ["dataInicial", "dataFinal"] {
        assert_eq!(parametros[obrigatorio]["required"], true, "{obrigatorio}");
    }
    let itens = &parametros["paginacao.itensPorPagina"]["schema"];
    assert_eq!(
        itens["maximum"].as_u64(),
        Some(u64::from(ITENS_POR_PAGINA_MAXIMO))
    );
}

#[test]
fn listing_enums_match_the_spec() {
    let codigos = |nomes: Vec<&'static str>| nomes.into_iter().collect::<BTreeSet<_>>();
    assert_eq!(
        codigos(FiltrarDataPor::TODOS.iter().map(|f| f.as_str()).collect()),
        enum_values("FiltrarDataPorEnum")
    );
    assert_eq!(
        codigos(
            OrdenarCobrancasPor::TODOS
                .iter()
                .map(|o| o.as_str())
                .collect()
        ),
        enum_values("OrdenarCobrancasPorEnum")
    );
    assert_eq!(
        enum_values("TipoOrdenacaoCobrancasEnum"),
        BTreeSet::from(["ASC", "DESC"])
    );
}

#[test]
fn listing_page_maps_every_documented_field() {
    let documentado = example("exemploRetornoListaCobrancas");
    let pagina: PaginaCobrancas = serde_json::from_value(documentado.clone()).unwrap();
    assert_eq!(pagina.cobrancas.len(), 1);
    let de_volta = serde_json::to_value(&pagina).unwrap();
    assert_eq!(normalizar(&de_volta), normalizar(documentado));
    assert_eq!(keys(&de_volta), property_names("CobrancasResponse"));
}

#[test]
fn summary_maps_every_documented_field() {
    let documentado = example("exemploRetornoSumario");
    let itens: Vec<ItemSumario> = serde_json::from_value(documentado.clone()).unwrap();
    assert_eq!(itens.len(), SituacaoCobranca::DOCUMENTADOS.len());
    assert!(
        itens
            .iter()
            .all(|item| !matches!(item.situacao, Some(SituacaoCobranca::Outro(_))))
    );
    let de_volta = serde_json::to_value(&itens).unwrap();
    assert_eq!(normalizar(&de_volta), normalizar(documentado));
    assert_eq!(keys(&de_volta[0]), property_names("itemSumarioCobrancas"));
}

#[test]
fn pdf_answer_has_the_field_we_read() {
    assert_eq!(
        property_names("PdfResponse"),
        BTreeSet::from(["pdf".to_owned()])
    );
}

#[test]
fn cancel_body_matches_the_spec() {
    assert_eq!(
        property_names("CancelarCobrancaRequestBody"),
        BTreeSet::from(["motivoCancelamento".to_owned()])
    );
    let motivo = &schema("CancelarCobrancaRequestBody")["properties"]["motivoCancelamento"];
    assert_eq!(
        motivo["maxLength"].as_u64(),
        Some(u64::try_from(MAX_MOTIVO_CANCELAMENTO).unwrap())
    );
    assert_eq!(
        keys(example("ExemploCancelarCobranca")),
        property_names("CancelarCobrancaRequestBody")
    );
}

#[test]
fn edit_models_match_the_spec() {
    let edicao = EdicaoCobranca::new(Some(dia(2026, 11, 10)), Some(dec("175.9")));
    assert_eq!(
        keys(&serde_json::to_value(&edicao).unwrap()),
        property_names("UpdateCobrancaRequestBody")
    );
    let valor = &schema("UpdateCobrancaRequestBody")["properties"]["valorNominal"];
    assert_eq!(dec(&valor["minimum"].to_string()), VALOR_MINIMO);
    assert_eq!(dec(&valor["maximum"].to_string()), VALOR_MAXIMO);

    let resposta: SolicitacaoEdicao =
        serde_json::from_value(example_for_schema("UpdateCobrancaResponseBody")).unwrap();
    assert!(resposta.status.is_some() && resposta.codigo_edicao.is_some());
    assert_eq!(
        keys(&serde_json::to_value(&resposta).unwrap()),
        property_names("UpdateCobrancaResponseBody")
    );
    let consulta: ConsultaEdicao =
        serde_json::from_value(example_for_schema("GetStatusUpdateResponseBody")).unwrap();
    assert_eq!(
        keys(&serde_json::to_value(&consulta).unwrap()),
        property_names("GetStatusUpdateResponseBody")
    );
    for nome in ["UpdateCobrancaResponseBody", "GetStatusUpdateResponseBody"] {
        let documentados: BTreeSet<&str> = schema(nome)["properties"]["status"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        let nossos: BTreeSet<&str> = StatusEdicao::DOCUMENTADOS
            .iter()
            .map(StatusEdicao::as_str)
            .collect();
        assert_eq!(nossos, documentados, "{nome}");
    }
}

#[test]
fn sandbox_payment_body_matches_the_spec() {
    let documentados: BTreeSet<&str> =
        schema("PagamentoCobrancaRequestBody")["properties"]["pagarCom"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
    let nossos: BTreeSet<&str> = [PagarCom::Boleto, PagarCom::Pix]
        .iter()
        .map(|c| c.as_str())
        .collect();
    assert_eq!(nossos, documentados);
    for (nome, com) in [
        ("exemploPagarCobranca1", PagarCom::Pix),
        ("exemploPagarCobranca2", PagarCom::Boleto),
    ] {
        assert_eq!(example(nome), &json!({"pagarCom": com.as_str()}), "{nome}");
    }
}
