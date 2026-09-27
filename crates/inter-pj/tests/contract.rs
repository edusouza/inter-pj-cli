//! Contract tests: the endpoint registry, the scopes and the models must agree
//! with Inter's OpenAPI specification (`spec/inter-empresas-openapi.json`).

mod spec;

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::NaiveDate;
use inter_pj::banking::{
    ConsultaPix, DadosBancarios, Darf, DataDoPagamento, Destinatario, Detalhe, IdIdempotente,
    InstituicaoFinanceira, ItemLote, Lote, LotePagamentos, LoteScroll, MAX_DESCRICAO,
    MAX_MEU_IDENTIFICADOR, MAX_PAGAMENTOS_LOTE, MIN_PAGAMENTOS_LOTE, Pagamento, PagamentoBoleto,
    PagamentoDarf, PagamentoDoLote, PagamentoPix, PaginaExtrato, Saldo, SolicitacaoDarf,
    SolicitacaoLote, SolicitacaoPagamento, SolicitacaoPix, StatusBoletoDoLote, StatusDarfDoLote,
    StatusLote, StatusPagamento, StatusPix, TipoConta, TipoOperacao, TipoRetornoDarf,
    TipoRetornoPix, TipoTransacao, TransacaoCompleta, TransacaoSimples,
};
use inter_pj::documento::Documento;
use inter_pj::endpoint::{self, Endpoint};
use inter_pj::{Environment, Scope};
use rust_decimal::Decimal;
use serde_json::{Map, Value, json};
use spec::*;

/// Prefix of the Forum API, which is not part of the account features (issue #60).
const OUT_OF_SCOPE_PREFIX: &str = "/forum/";

#[test]
fn every_implemented_endpoint_matches_the_spec() {
    for endpoint in endpoint::ALL {
        let op = operation(endpoint);
        let ours: BTreeSet<String> = endpoint
            .scopes
            .iter()
            .map(|s| s.as_str().to_owned())
            .collect();
        assert_eq!(spec_scopes(op), ours, "escopos divergentes em {endpoint}");
    }
}

/// The other side of [`every_implemented_endpoint_matches_the_spec`]: no
/// operation of the specification is left without an endpoint (issue #56).
#[test]
fn every_operation_of_the_spec_is_implemented() {
    const METODOS: [&str; 5] = ["get", "post", "put", "patch", "delete"];
    let ours: BTreeSet<(String, &str)> = endpoint::ALL
        .iter()
        .map(|endpoint| {
            (
                endpoint.method.as_str().to_lowercase(),
                spec_path(endpoint.path),
            )
        })
        .collect();
    let mut cobertas = 0;
    let mut faltam = Vec::new();
    for (path, item) in spec()["paths"].as_object().unwrap() {
        if path.starts_with(OUT_OF_SCOPE_PREFIX) {
            continue;
        }
        let path = DUPLICADAS
            .iter()
            .find(|(copia, _)| copia == path)
            .map_or(path.as_str(), |(_, original)| original);
        for method in item.as_object().unwrap().keys() {
            if !METODOS.contains(&method.as_str()) {
                continue;
            }
            if ours.contains(&(method.clone(), path)) {
                cobertas += 1;
            } else {
                faltam.push(format!("{} {path}", method.to_uppercase()));
            }
        }
    }
    assert!(
        faltam.is_empty(),
        "operações da especificação sem endpoint no registro: {faltam:#?}"
    );
    // The 98 of the specification but for the 6 of the Forum (the copy is
    // one of them), with a registry of 91 endpoints.
    assert_eq!(cobertas, 92);
}

#[test]
fn registry_has_no_duplicates() {
    let unique: BTreeSet<String> = endpoint::ALL.iter().map(ToString::to_string).collect();
    assert_eq!(unique.len(), endpoint::ALL.len());
}

#[test]
fn scope_enum_matches_documented_scopes() {
    let documented: BTreeSet<String> =
        spec()["components"]["securitySchemes"]["oauth2"]["flows"]["clientCredentials"]["scopes"]
            .as_object()
            .unwrap()
            .keys()
            .filter(|name| !name.starts_with("publication."))
            .cloned()
            .collect();
    let ours: BTreeSet<String> = Scope::ALL.iter().map(|s| s.as_str().to_owned()).collect();
    assert_eq!(ours, documented);
}

#[test]
fn every_scope_used_by_the_spec_is_known() {
    let mut used = BTreeSet::new();
    for (path, item) in spec()["paths"].as_object().unwrap() {
        if path.starts_with(OUT_OF_SCOPE_PREFIX) {
            continue;
        }
        for op in item.as_object().unwrap().values() {
            used.extend(spec_scopes(op));
        }
    }
    for scope in used {
        assert!(
            scope.parse::<Scope>().is_ok(),
            "escopo {scope} não mapeado no enum Scope"
        );
    }
}

#[test]
fn environments_match_spec_servers() {
    let servers: BTreeSet<&str> = spec()["servers"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|server| server["url"].as_str())
        .collect();
    for environment in [Environment::Production, Environment::Sandbox] {
        assert!(servers.contains(environment.base_url()), "{environment}");
    }
}

#[test]
fn token_request_fields_match_the_spec() {
    let required: BTreeSet<&str> = spec()["components"]["schemas"]["TokenRequest"]["required"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    // Same fields the client sends (asserted byte by byte in tests/client.rs).
    let sent = BTreeSet::from(["client_id", "client_secret", "grant_type", "scope"]);
    assert_eq!(required, sent);
}

#[test]
fn saldo_model_accepts_schema_example() {
    let example = example_for_schema("Saldo");
    let saldo: Saldo = serde_json::from_value(example.clone()).unwrap();

    // Every documented property must round-trip through the model.
    let back = serde_json::to_value(&saldo).unwrap();
    let documented: BTreeSet<&String> = schema("Saldo")["properties"]
        .as_object()
        .unwrap()
        .keys()
        .collect();
    let serialized: BTreeSet<&String> = back.as_object().unwrap().keys().collect();
    assert_eq!(
        serialized, documented,
        "campos do schema Saldo sem correspondência no modelo"
    );
    for key in documented {
        assert_eq!(numeric(&back[key]), numeric(&example[key]), "campo {key}");
    }
}

#[test]
fn saldo_endpoint_documents_the_date_parameter() {
    let names = parameter_names(&endpoint::banking::SALDO);
    assert!(names.contains("dataSaldo"), "{names:?}");
    assert!(names.contains("x-conta-corrente"), "{names:?}");
}

/// Parameters sent by `inter_pj::banking` (asserted one by one against a mock
/// API in tests/extrato.rs) must be documented for the operation.
#[test]
fn statement_endpoints_document_the_parameters_we_send() {
    let cases: [(Endpoint, &[&str]); 3] = [
        (endpoint::banking::EXTRATO, &["dataInicio", "dataFim"]),
        (
            endpoint::banking::EXTRATO_COMPLETO,
            &[
                "dataInicio",
                "dataFim",
                "pagina",
                "tamanhoPagina",
                "tipoOperacao",
                "tipoTransacao",
                "scrollEnabled",
                "scrollId",
            ],
        ),
        (
            endpoint::banking::EXTRATO_EXPORTAR,
            &["dataInicio", "dataFim"],
        ),
    ];
    for (endpoint, sent) in cases {
        let documented = parameter_names(&endpoint);
        for name in sent {
            assert!(
                documented.contains(*name),
                "{endpoint}: {name} não documentado"
            );
        }
        assert!(documented.contains("x-conta-corrente"), "{endpoint}");
    }

    let completo = parameters(&endpoint::banking::EXTRATO_COMPLETO);
    assert_eq!(completo["scrollEnabled"]["schema"]["enum"], json!(["true"]));
    assert_eq!(
        completo["tamanhoPagina"]["schema"]["maximum"],
        json!(10_000)
    );
    let codes: BTreeSet<&str> = completo["tipoOperacao"]["schema"]["enum"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let ours = BTreeSet::from([
        TipoOperacao::Credito.as_str(),
        TipoOperacao::Debito.as_str(),
    ]);
    assert_eq!(codes, ours);
    assert_eq!(
        schema("PdfModel")["properties"]["pdf"]["type"],
        json!("string")
    );
}

#[test]
fn transaction_types_match_the_documented_list() {
    for name in ["TransacaoSimples", "TransacaoCompleta"] {
        let description = schema(name)["properties"]["tipoTransacao"]["description"]
            .as_str()
            .unwrap();
        let documented: BTreeSet<&str> = description
            .lines()
            .filter_map(|line| line.trim().strip_prefix("* `")?.strip_suffix('`'))
            .collect();
        let ours: BTreeSet<&str> = TipoTransacao::DOCUMENTADOS
            .iter()
            .map(TipoTransacao::as_str)
            .collect();
        assert_eq!(ours, documented, "{name}");
    }
}

#[test]
fn simple_transaction_accepts_the_schema_example() {
    let example = example_for_schema("TransacaoSimples");
    let transacao: TransacaoSimples = serde_json::from_value(example.clone()).unwrap();
    assert_round_trip(
        "TransacaoSimples",
        &example,
        &serde_json::to_value(&transacao).unwrap(),
    );
}

/// Every `Transacao<Tipo>` schema pairs a transaction type with its
/// `Detalhe<Tipo>` schema: the model must pick the typed details for that
/// type and map every documented field (nothing left in `outros`).
#[test]
fn transaction_details_are_typed_for_every_documented_kind() {
    let schemas = spec()["components"]["schemas"].as_object().unwrap();
    let mut pairs: Vec<(String, String)> = schemas
        .iter()
        .filter_map(|(name, schema)| {
            let suffix = name.strip_prefix("Transacao")?;
            let detail = schema["properties"]["detalhes"]["$ref"].as_str()?;
            let detail = detail.rsplit('/').next()?.to_owned();
            Some((screaming_snake(suffix), detail))
        })
        .collect();
    // Fees have a detail schema but no `TransacaoTarifa` pairing it.
    pairs.push(("TARIFA".to_owned(), "DetalheTarifa".to_owned()));

    let detail_schemas: BTreeSet<&str> = schemas
        .keys()
        .filter(|name| name.starts_with("Detalhe"))
        .map(String::as_str)
        .collect();
    let covered: BTreeSet<&str> = pairs.iter().map(|(_, detail)| detail.as_str()).collect();
    assert_eq!(
        covered, detail_schemas,
        "schemas de detalhe sem tipo de transação"
    );

    for (tipo, detail) in &pairs {
        let mut example = example_for_schema("TransacaoCompleta");
        example["tipoTransacao"] = json!(tipo);
        example["detalhes"] = example_for_schema(detail);
        let transacao: TransacaoCompleta = serde_json::from_value(example.clone()).unwrap();

        let (typed, extras) = typed_detail(transacao.detalhes.as_ref().unwrap())
            .unwrap_or_else(|| panic!("{tipo}: detalhes não tipados"));
        assert_eq!(typed, detail, "{tipo}");
        assert!(
            extras.is_empty(),
            "{detail}: campos sem correspondência {extras:?}"
        );

        let back = serde_json::to_value(&transacao).unwrap();
        assert_eq!(back["detalhes"], example["detalhes"], "{detail}");
        let mut base = example.clone();
        base.as_object_mut().unwrap().remove("detalhes");
        let mut back_base = back.clone();
        back_base.as_object_mut().unwrap().remove("detalhes");
        assert_eq!(numeric_fields(&back_base), numeric_fields(&base), "{tipo}");
    }
}

#[test]
fn statement_pages_accept_the_schema_examples() {
    let example = example_for_schema("ListaTransacoesCompletaPadrao");
    let pagina: PaginaExtrato = serde_json::from_value(example.clone()).unwrap();
    let back = serde_json::to_value(&pagina).unwrap();
    assert_same_keys("ListaTransacoesCompletaPadrao", &back);
    assert_eq!(pagina.transacoes.len(), 1);

    let example = example_for_schema("ListaTransacoesCompletaScroll");
    let lote: LoteScroll = serde_json::from_value(example.clone()).unwrap();
    let back = serde_json::to_value(&lote).unwrap();
    assert_same_keys("ListaTransacoesCompletaScroll", &back);
    assert_eq!(lote.has_more, Some(true));
    assert_eq!(
        lote.scroll_id.as_deref(),
        Some("550e8400-e29b-41d4-a716-446655440000")
    );
}

/// The bodies sent by `enviar_pix` reproduce the three examples of
/// `PagamentoPixRequestBody`, one per kind of receiver.
#[test]
fn pix_payment_bodies_reproduce_the_spec_examples() {
    let examples = &schema("PagamentoPixRequestBody")["example"];
    let pagamento = |destinatario, descricao: &str| PagamentoPix {
        valor: Decimal::new(123, 2),
        data_pagamento: NaiveDate::from_ymd_opt(2022, 10, 10),
        descricao: Some(descricao.to_owned()),
        destinatario,
    };
    let mut por_dados = examples["dadosBancarios"].clone();
    // The sanitized example has an all-zeros CPF, which is not a valid one.
    por_dados["destinatario"]["cpfCnpj"] = json!("12345678909");
    let cases = [
        (
            &examples["chavePix"],
            pagamento(
                Destinatario::Chave {
                    chave: "chavepix@example.com".parse().unwrap(),
                },
                "Pix com chave Pix teste",
            ),
        ),
        (
            &por_dados,
            pagamento(
                Destinatario::DadosBancarios(DadosBancarios {
                    nome: "Teste dados bancários".to_owned(),
                    cpf_cnpj: "12345678909".parse().unwrap(),
                    instituicao_financeira: InstituicaoFinanceira {
                        ispb: "00416968".to_owned(),
                    },
                    agencia: "0019".to_owned(),
                    conta_corrente: "0000000".to_owned(),
                    tipo_conta: TipoConta::ContaCorrente,
                }),
                "Pix dados bancários teste",
            ),
        ),
        (
            &examples["pixCopiaECola"],
            pagamento(
                Destinatario::PixCopiaECola {
                    pix_copia_e_cola: "<Código Copia E Cola>".to_owned(),
                },
                "Pix com código Copia e Cola",
            ),
        ),
    ];
    for (example, pagamento) in cases {
        assert!(pagamento.validar().is_ok(), "{pagamento:?}");
        assert_eq!(&serde_json::to_value(&pagamento).unwrap(), example);
        let tipo = example["destinatario"]["tipo"].as_str().unwrap();
        let destinatario = &schema("PagamentoPixRequestBody")["properties"]["destinatario"];
        let reference = &destinatario["discriminator"]["mapping"][tipo];
        let name = reference.as_str().unwrap().rsplit('/').next().unwrap();
        assert_eq!(
            keys(&example["destinatario"]),
            property_names(name),
            "{name}"
        );
    }
    assert_eq!(
        keys(&examples["chavePix"]),
        property_names("PagamentoPixRequestBody")
    );
}

#[test]
fn pix_payment_enums_and_limits_match_the_spec() {
    let ours: BTreeSet<&str> = TipoConta::TODOS.iter().map(|t| t.as_str()).collect();
    assert_eq!(ours, enum_values("TipoConta"));

    let mapping: BTreeSet<&str> =
        schema("PagamentoPixRequestBody")["properties"]["destinatario"]["discriminator"]["mapping"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
    assert_eq!(mapping, enum_values("TipoDestinatario"));

    assert_eq!(
        schema("PagamentoPixRequestBody")["properties"]["descricao"]["maxLength"],
        json!(MAX_DESCRICAO)
    );
    let ours: BTreeSet<&str> = TipoRetornoPix::DOCUMENTADOS
        .iter()
        .map(TipoRetornoPix::as_str)
        .collect();
    assert_eq!(ours, enum_values("TipoRetornoPagamentoPixEnum"));
}

#[test]
fn pix_endpoints_document_the_idempotency_key_and_request_code() {
    let incluir = parameters(&endpoint::banking::PIX_INCLUIR);
    assert!(incluir.contains_key("x-conta-corrente"));
    let pattern = incluir["x-id-idempotente"]["schema"]["pattern"]
        .as_str()
        .unwrap();
    assert_eq!(
        pattern,
        "[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}"
    );
    let id = IdIdempotente::novo();
    let groups: Vec<&str> = id.as_str().split('-').collect();
    assert_eq!(
        groups.iter().map(|g| g.len()).collect::<Vec<_>>(),
        [8, 4, 4, 4, 12]
    );
    assert!(
        groups.iter().all(|g| g
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))),
        "{id}"
    );

    let consultar = parameters(&endpoint::banking::PIX_CONSULTAR);
    assert_eq!(consultar["codigoSolicitacao"]["in"], json!("path"));
    assert!(consultar.contains_key("x-conta-corrente"));
}

#[test]
fn pix_payment_answers_accept_the_spec_examples() {
    let content = &operation(&endpoint::banking::PIX_INCLUIR)["responses"]["200"]["content"]["application/json"];
    let examples = content["examples"].as_object().unwrap();
    assert_eq!(examples.len(), 2);
    for example in examples.values() {
        let value = &resolve(example)["value"];
        let solicitacao: SolicitacaoPix = serde_json::from_value(value.clone()).unwrap();
        assert!(
            !matches!(
                solicitacao.tipo_retorno,
                Some(TipoRetornoPix::Outro(_)) | None
            ),
            "{value}"
        );
        let back = serde_json::to_value(&solicitacao).unwrap();
        assert_eq!(&back, value);
        assert_same_keys("PagamentoPixResponse", &back);
    }
}

#[test]
fn pix_query_model_maps_every_documented_field() {
    let example = example_for_schema("ConsultaPixAsyncResponse");
    let consulta: ConsultaPix = serde_json::from_value(example.clone()).unwrap();
    let back = serde_json::to_value(&consulta).unwrap();
    assert_same_keys("ConsultaPixAsyncResponse", &back);
    let transacao = &back["transacaoPix"];
    assert_same_keys("PixAsyncResponse", transacao);
    assert_same_keys("DadosConta", &transacao["recebedor"]);
    assert_same_keys("ErroPagamento", &transacao["erros"][0]);
    assert_same_keys("HistoricoResponse", &back["historico"][0]);
    assert_eq!(
        numeric(&transacao["valor"]),
        numeric(&example["transacaoPix"]["valor"])
    );
}

#[test]
fn pix_statuses_match_the_documented_list() {
    let ours: BTreeSet<&str> = StatusPix::DOCUMENTADOS
        .iter()
        .map(StatusPix::as_str)
        .collect();
    for name in ["StatusPix", "StatusHistoricoPix"] {
        let documented: BTreeSet<&str> = schema(name)["description"]
            .as_str()
            .unwrap()
            .lines()
            .filter_map(|line| line.trim().strip_prefix("* `")?.strip_suffix('`'))
            .collect();
        assert_eq!(ours, documented, "{name}");
    }
}

/// The documented example of `EfetuarPagamento` has an invalid barcode and
/// beneficiary: a valid boleto line of the same documentation and a
/// synthetic CNPJ stand in for them.
#[test]
fn boleto_payment_body_matches_the_request_schema() {
    let example = example_for_schema("EfetuarPagamento");
    let mut pagamento = PagamentoBoleto::new(
        "07797777051167847115990071126347192950000003010"
            .parse()
            .unwrap(),
        "26.80".parse().unwrap(),
        NaiveDate::from_ymd_opt(2021, 7, 27).unwrap(),
    );
    pagamento.data_pagamento = NaiveDate::from_ymd_opt(2023, 8, 18);
    pagamento.cpf_cnpj_beneficiario = Some("12345678000195".parse().unwrap());
    let body = serde_json::to_value(&pagamento).unwrap();
    assert_eq!(keys(&body), property_names("EfetuarPagamento"));
    for campo in ["valorPagar", "dataPagamento", "dataVencimento"] {
        assert_eq!(body[campo], example[campo], "{campo}");
    }
    for required in schema("EfetuarPagamento")["required"].as_array().unwrap() {
        assert!(body.get(required.as_str().unwrap()).is_some(), "{required}");
    }
    let pattern = schema("EfetuarPagamento")["properties"]["cpfCnpjBeneficiario"]["pattern"]
        .as_str()
        .unwrap();
    assert_eq!(pattern, "^[0-9]{11}$|^[0-9]{14}$");
}

#[test]
fn boleto_payment_models_map_every_documented_field() {
    let example = example_for_schema("EfetuarPagamentoResponse");
    let resposta: SolicitacaoPagamento = serde_json::from_value(example.clone()).unwrap();
    assert_same_keys(
        "EfetuarPagamentoResponse",
        &serde_json::to_value(&resposta).unwrap(),
    );

    let example = example_for_schema("InformacoesPagamento");
    let pagamento: Pagamento = serde_json::from_value(example).unwrap();
    assert_same_keys(
        "InformacoesPagamento",
        &serde_json::to_value(&pagamento).unwrap(),
    );

    let ours: BTreeSet<&str> = StatusPagamento::DOCUMENTADOS
        .iter()
        .map(StatusPagamento::as_str)
        .collect();
    assert_eq!(ours, enum_values("SituacaoPagamento"));
}

#[test]
fn boleto_payment_endpoints_document_the_parameters_we_send() {
    let buscar = parameters(&endpoint::banking::PAGAMENTO_BUSCAR);
    for name in [
        "codBarraLinhaDigitavel",
        "codigoTransacao",
        "dataInicio",
        "dataFim",
        "filtrarDataPor",
        "x-conta-corrente",
    ] {
        assert!(buscar.contains_key(name), "{name}");
    }
    let documented: BTreeSet<&str> = buscar["filtrarDataPor"]["schema"]["enum"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let ours: BTreeSet<&str> = DataDoPagamento::TODAS.iter().map(|d| d.as_str()).collect();
    assert_eq!(ours, documented);

    let cancelar = parameters(&endpoint::banking::PAGAMENTO_CANCELAR);
    assert_eq!(cancelar["codigoTransacao"]["in"], json!("path"));
    assert!(parameters(&endpoint::banking::PAGAMENTO_INCLUIR).contains_key("x-conta-corrente"));
}

/// The documented DARF, rebuilt from the examples of each field.
fn darf_do_exemplo() -> PagamentoDarf {
    let example = example_for_schema("DarfRequest");
    let texto = |campo: &str| example[campo].as_str().unwrap().to_owned();
    let dia = |campo: &str| example[campo].as_str().unwrap().parse().unwrap();
    let valor = |campo: &str| Decimal::try_from(example[campo].as_f64().unwrap()).unwrap();
    PagamentoDarf {
        cnpj_cpf: texto("cnpjCpf").parse().unwrap(),
        codigo_receita: texto("codigoReceita"),
        data_vencimento: dia("dataVencimento"),
        descricao: texto("descricao"),
        nome_empresa: texto("nomeEmpresa"),
        telefone_empresa: Some(texto("telefoneEmpresa")),
        periodo_apuracao: dia("periodoApuracao"),
        valor_principal: valor("valorPrincipal"),
        valor_multa: Some(valor("valorMulta")),
        valor_juros: Some(valor("valorJuros")),
        referencia: texto("referencia"),
    }
}

#[test]
fn darf_body_reproduces_the_spec_example() {
    let darf = darf_do_exemplo();
    assert_eq!(darf.validar(), Ok(()));
    assert_eq!(
        serde_json::to_value(&darf).unwrap(),
        example_for_schema("DarfRequest")
    );
}

#[test]
fn darf_limits_match_the_spec() {
    type Alteracao = fn(&mut PagamentoDarf, String);
    let properties = &schema("DarfRequest")["properties"];
    let maximo =
        |campo: &str| usize::try_from(properties[campo]["maxLength"].as_u64().unwrap()).unwrap();
    let casos: [(&str, Alteracao); 4] = [
        ("descricao", |d, texto| d.descricao = texto),
        ("nomeEmpresa", |d, texto| d.nome_empresa = texto),
        ("telefoneEmpresa", |d, texto| {
            d.telefone_empresa = Some(texto);
        }),
        ("referencia", |d, texto| d.referencia = texto),
    ];
    for (campo, alterar) in casos {
        let mut darf = darf_do_exemplo();
        alterar(&mut darf, "1".repeat(maximo(campo)));
        assert_eq!(darf.validar(), Ok(()), "{campo}");
        alterar(&mut darf, "1".repeat(maximo(campo) + 1));
        assert!(darf.validar().is_err(), "{campo}");
    }
    assert_eq!(properties["codigoReceita"]["minLength"], json!(4));
    assert_eq!(properties["codigoReceita"]["maxLength"], json!(4));
    for required in schema("DarfRequest")["required"].as_array().unwrap() {
        let body = serde_json::to_value(darf_do_exemplo()).unwrap();
        assert!(body.get(required.as_str().unwrap()).is_some(), "{required}");
    }
}

#[test]
fn darf_models_map_every_documented_field() {
    let example = example_for_schema("DarfResponse");
    let resposta: SolicitacaoDarf = serde_json::from_value(example).unwrap();
    assert_same_keys("DarfResponse", &serde_json::to_value(&resposta).unwrap());

    let example = example_for_schema("InformacoesPagamentoDarf");
    let darf: Darf = serde_json::from_value(example).unwrap();
    assert_same_keys(
        "InformacoesPagamentoDarf",
        &serde_json::to_value(&darf).unwrap(),
    );

    let ours: BTreeSet<&str> = TipoRetornoDarf::DOCUMENTADOS
        .iter()
        .map(TipoRetornoDarf::as_str)
        .collect();
    assert_eq!(ours, enum_values("TipoRetornoEnum"));
}

#[test]
fn darf_and_batch_endpoints_document_the_parameters_we_send() {
    let buscar = parameters(&endpoint::banking::PAGAMENTO_DARF_BUSCAR);
    for name in [
        "codigoSolicitacao",
        "codigoReceita",
        "dataInicio",
        "dataFim",
        "x-conta-corrente",
    ] {
        assert!(buscar.contains_key(name), "{name}");
    }
    let consultar = parameters(&endpoint::banking::PAGAMENTO_LOTE_CONSULTAR);
    assert_eq!(consultar["idLote"]["in"], json!("path"));
    assert_eq!(consultar["idLote"]["schema"]["minLength"], json!(24));
    assert_eq!(consultar["idLote"]["schema"]["maxLength"], json!(24));
    for incluir in [
        endpoint::banking::PAGAMENTO_DARF_INCLUIR,
        endpoint::banking::PAGAMENTO_LOTE_INCLUIR,
    ] {
        assert!(
            parameters(&incluir).contains_key("x-conta-corrente"),
            "{incluir}"
        );
    }
    let accepted: Vec<&String> = operation(&endpoint::banking::PAGAMENTO_LOTE_INCLUIR)["responses"]
        .as_object()
        .unwrap()
        .keys()
        .filter(|status| status.starts_with('2'))
        .collect();
    assert_eq!(accepted, ["202"]);
}

#[test]
fn batch_body_matches_the_request_schemas() {
    let pagamentos = &schema("PagarLoteRequest")["properties"]["pagamentos"];
    assert_eq!(pagamentos["minItems"], json!(MIN_PAGAMENTOS_LOTE));
    assert_eq!(pagamentos["maxItems"], json!(MAX_PAGAMENTOS_LOTE));
    assert_eq!(
        schema("PagarLoteRequest")["properties"]["meuIdentificador"]["maxLength"],
        json!(MAX_MEU_IDENTIFICADOR)
    );
    let mapping: BTreeSet<&str> = pagamentos["items"]["discriminator"]["mapping"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(mapping, enum_values("TipoPagamentoEnum"));

    let mut boleto = PagamentoBoleto::new(
        "07797777051167847115990071126347192950000003010"
            .parse()
            .unwrap(),
        "26.80".parse().unwrap(),
        NaiveDate::from_ymd_opt(2026, 10, 10).unwrap(),
    );
    boleto.data_pagamento = NaiveDate::from_ymd_opt(2026, 10, 9);
    boleto.cpf_cnpj_beneficiario = Some("12345678000195".parse().unwrap());
    let lote = LotePagamentos {
        meu_identificador: Some("Lote de teste".to_owned()),
        pagamentos: vec![ItemLote::from(boleto), ItemLote::from(darf_do_exemplo())],
    };
    assert_eq!(lote.validar(), Ok(()));
    let body = serde_json::to_value(&lote).unwrap();
    assert_eq!(keys(&body), property_names("PagarLoteRequest"));
    for (item, nome) in body["pagamentos"]
        .as_array()
        .unwrap()
        .iter()
        .zip(["RequestBoletoLote", "RequestDarfLote"])
    {
        assert_eq!(keys(item), property_names(nome), "{nome}");
        for required in schema(nome)["required"].as_array().unwrap() {
            assert!(
                item.get(required.as_str().unwrap()).is_some(),
                "{nome}.{required}"
            );
        }
    }
    assert_eq!(
        schema("RequestBoletoLote")["properties"]["valorPagar"]["type"],
        json!("number")
    );
    assert_eq!(body["pagamentos"][0]["valorPagar"], json!(26.8));
    assert_eq!(body["pagamentos"][0]["tipoPagamento"], json!("BOLETO"));
    assert_eq!(body["pagamentos"][1]["tipoPagamento"], json!("DARF"));
}

#[test]
fn batch_answers_map_every_documented_field() {
    let example = example_for_schema("PagarLoteResponse");
    let resposta: SolicitacaoLote = serde_json::from_value(example).unwrap();
    assert_same_keys(
        "PagarLoteResponse",
        &serde_json::to_value(&resposta).unwrap(),
    );

    let mut example = example_for_schema("ObterLoteResponse");
    let mut darf = example_for_schema("ResponseDarfLote");
    darf["tipoPagamento"] = json!("DARF");
    example["pagamentos"].as_array_mut().unwrap().push(darf);
    let lote: Lote = serde_json::from_value(example).unwrap();
    let back = serde_json::to_value(&lote).unwrap();
    assert_eq!(keys(&back), property_names("ObterLoteResponse"));
    let [PagamentoDoLote::Boleto(_), PagamentoDoLote::Darf(_)] = &lote.pagamentos[..] else {
        panic!("{:?}", lote.pagamentos);
    };
    for (item, nome) in back["pagamentos"]
        .as_array()
        .unwrap()
        .iter()
        .zip(["ResponseBoletoLote", "ResponseDarfLote"])
    {
        assert_eq!(keys(item), property_names(nome), "{nome}");
    }

    let documented = |ours: Vec<&'static str>, name: &str| {
        assert_eq!(
            ours.into_iter().collect::<BTreeSet<_>>(),
            enum_values(name),
            "{name}"
        );
    };
    documented(
        StatusLote::DOCUMENTADOS
            .iter()
            .map(StatusLote::as_str)
            .collect(),
        "StatusLoteEnum",
    );
    documented(
        StatusBoletoDoLote::DOCUMENTADOS
            .iter()
            .map(StatusBoletoDoLote::as_str)
            .collect(),
        "StatusPagamentoBoleto",
    );
    documented(
        StatusDarfDoLote::DOCUMENTADOS
            .iter()
            .map(StatusDarfDoLote::as_str)
            .collect(),
        "StatusPagamentoDarf",
    );
}

/// The portal's examples carried real-looking CPFs, CNPJs, e-mail
/// addresses, phone numbers and bank accounts; `spec/sanitizar.py` replaces
/// them with synthetic values. This test keeps it that way when the
/// specification is updated.
#[test]
fn spec_examples_contain_no_real_looking_personal_data() {
    const SYNTHETIC_PHONE: &str = "+5500000000000";
    const ACCOUNT_DIGITS: &str = "123456789012345678901234567890";

    let mut problems = BTreeSet::new();
    visit(spec(), &mut Vec::new(), &mut |path, value| {
        let text = match value {
            Value::String(s) => s.clone(),
            Value::Number(n) if n.is_u64() => n.to_string(),
            _ => return,
        };
        let digits: String = text.chars().filter(char::is_ascii_digit).collect();
        let location = path.join("/");

        let in_cpf_field = path.iter().any(|p| p.to_lowercase().contains("cpf"));
        if in_cpf_field
            && looks_like_cpf(&text)
            && cpf_is_valid(&digits)
            && !ALLOWED_CPFS.contains(&digits.as_str())
        {
            problems.insert(format!("CPF em {location}"));
        }
        // Anywhere, descriptions included: the shapes no other code has.
        for problem in documents_and_addresses(&text, false) {
            problems.insert(format!("{problem} em {location}"));
        }
        if text.starts_with("+55") && text != SYNTHETIC_PHONE {
            problems.insert(format!("telefone em {location}"));
        }
        let field = path
            .iter()
            .rev()
            .find(|p| !matches!(p.as_str(), "example" | "value" | "default"));
        let account_field = field.is_some_and(|f| f.to_lowercase().contains("conta"));
        if account_field
            && digits == text
            && text.len() >= 4
            && !text.chars().all(|c| c == '0')
            && !ACCOUNT_DIGITS.starts_with(&text)
        {
            problems.insert(format!("conta em {location}"));
        }
    });
    assert!(
        problems.is_empty(),
        "rode `python3 spec/sanitizar.py`; dados possivelmente reais:\n{problems:#?}"
    );
}

/// No file of the repository (code, tests, documentation, workflows) holds
/// a real-looking CPF or CNPJ, or an e-mail address someone could receive:
/// examples and tests use the synthetic documents and the domains reserved
/// for documentation.
#[test]
fn repository_contains_no_real_looking_personal_data() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut problems = BTreeSet::new();
    let mut files = 0;
    for path in text_files(&root) {
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        files += 1;
        let name = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .display()
            .to_string();
        for (number, line) in text.lines().enumerate() {
            for problem in documents_and_addresses(line, true) {
                problems.insert(format!("{problem} em {name}:{}", number + 1));
            }
        }
    }
    assert!(files > 100, "{files} arquivos lidos em {}", root.display());
    assert!(
        problems.is_empty(),
        "use os dados sintéticos (CPF {}, CNPJ {} ou {}, e-mails em empresa.example):\n{problems:#?}",
        ALLOWED_CPFS[2],
        ALLOWED_CNPJS[0],
        ALLOWED_CNPJS[1],
    );
}

#[test]
fn cpf_check_digits() {
    assert!(cpf_is_valid("12345678909"));
    assert!(!cpf_is_valid("12345678900"));
    assert!(!cpf_is_valid("11111111111"));
    assert!(looks_like_cpf("123.456.789-09"));
    assert!(!looks_like_cpf("1234567890"));
}

#[test]
fn documents_and_addresses_are_found_anywhere_in_a_text() {
    let texto = "CNPJ 12.345.678/0001-95, CPF 123.456.789-09 (ou E12345678000195x).";
    assert_eq!(
        tokens(texto).collect::<Vec<_>>(),
        [
            "CNPJ",
            "12.345.678/0001-95",
            "CPF",
            "123.456.789-09",
            "ou",
            "E12345678000195x"
        ]
    );
    assert!(has_shape("12.345.678/0001-95", "99.999.999/9999-99"));
    assert!(!has_shape("12.345.678/0001-9", "99.999.999/9999-99"));
    assert!(!has_shape("12345678000195x", "99999999999999"));

    // Split, so that the scan of the repository does not take it for an
    // address.
    let texto = concat!("a@", "Mail.com, b@example.com. (c@x.example) @solto d@");
    let dominios: Vec<String> = email_domains(texto).collect();
    assert_eq!(dominios, ["mail.com", "example.com", "x.example"]);
    assert!(!reserved_domain("mail.com"));
    assert!(!reserved_domain("example.com.br"));
    for dominio in ["example.com", "sub.example.org", "x.example", "a.test"] {
        assert!(reserved_domain(dominio), "{dominio}");
    }
}

// --- helpers ---------------------------------------------------------------

fn visit(value: &Value, path: &mut Vec<String>, f: &mut dyn FnMut(&[String], &Value)) {
    match value {
        Value::Object(object) => {
            for (key, item) in object {
                path.push(key.clone());
                visit(item, path, f);
                path.pop();
            }
        }
        Value::Array(items) => {
            for item in items {
                visit(item, path, f);
            }
        }
        other => f(path, other),
    }
}

fn looks_like_cpf(text: &str) -> bool {
    let digits = text.chars().filter(char::is_ascii_digit).count();
    let allowed = text
        .chars()
        .all(|c| c.is_ascii_digit() || c == '.' || c == '-');
    digits == 11 && allowed && (text.len() == 11 || text.len() == 14)
}

/// Obviously synthetic documents with valid check digits: a sequence, a
/// phone number of zeros (for the ambiguity between phones and CPFs) and
/// the CPF of the examples; the CNPJs of the examples, the last one that
/// of the fictitious company of the guides.
const ALLOWED_CPFS: [&str; 3] = ["01234567890", "11900000083", "12345678909"];
const ALLOWED_CNPJS: [&str; 3] = ["12345678000195", "11222333000181", "11444777000161"];

/// What in `text` looks like personal data: CPFs and CNPJs with valid
/// check digits, formatted or not, other than the synthetic ones, and
/// e-mail addresses outside the reserved domains. Plain 11-digit CPFs only
/// with `plain_cpfs`: in the specification they are checked by field.
fn documents_and_addresses(text: &str, plain_cpfs: bool) -> Vec<&'static str> {
    let mut problems = Vec::new();
    for token in tokens(text) {
        let digits: String = token.chars().filter(char::is_ascii_digit).collect();
        let cnpj = has_shape(token, "99999999999999") || has_shape(token, "99.999.999/9999-99");
        if cnpj && Documento::parse(token).is_ok() && !ALLOWED_CNPJS.contains(&digits.as_str()) {
            problems.push("CNPJ");
        }
        let cpf =
            has_shape(token, "999.999.999-99") || (plain_cpfs && has_shape(token, "99999999999"));
        if cpf && cpf_is_valid(&digits) && !ALLOWED_CPFS.contains(&digits.as_str()) {
            problems.push("CPF");
        }
    }
    if email_domains(text).any(|domain| !reserved_domain(&domain)) {
        problems.push("e-mail");
    }
    problems
}

/// The files git could commit: tracked, or new and not ignored (local
/// files the `.gitignore` covers, like payment spreadsheets, stay out).
/// Without git, every file but the build output and git's data.
fn text_files(root: &Path) -> Vec<PathBuf> {
    let listed = Command::new("git")
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ])
        .current_dir(root)
        .output()
        .ok()
        .filter(|output| output.status.success());
    match listed {
        Some(output) => String::from_utf8_lossy(&output.stdout)
            .split('\0')
            .filter(|name| !name.is_empty())
            .map(|name| root.join(name))
            .filter(|path| path.is_file())
            .collect(),
        None => walk(root),
    }
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        if name == "target" || name == ".git" {
            continue;
        }
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => files.extend(walk(&path)),
            Ok(kind) if kind.is_file() => files.push(path),
            _ => {}
        }
    }
    files
}

/// The words of `text`, keeping the punctuation of documents
/// (`12.345.678/0001-95`) and dropping the one that ends a sentence.
fn tokens(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '/' | '-')))
        .map(|token| token.trim_end_matches(['.', '/', '-']))
        .filter(|token| !token.is_empty())
}

/// Whether `text` has the shape of `pattern`, where `9` stands for a digit.
fn has_shape(text: &str, pattern: &str) -> bool {
    text.len() == pattern.len()
        && text.chars().zip(pattern.chars()).all(
            |(c, p)| {
                if p == '9' { c.is_ascii_digit() } else { c == p }
            },
        )
}

/// The domains of the e-mail addresses in `text`, in lowercase.
fn email_domains(text: &str) -> impl Iterator<Item = String> + '_ {
    text.match_indices('@').filter_map(|(at, _)| {
        let local = text[..at]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_ascii_alphanumeric() || "._%+-".contains(c));
        let domain: String = text[at + 1..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
            .collect();
        let domain = domain.trim_end_matches(['.', '-']).to_lowercase();
        (local && domain.contains('.')).then_some(domain)
    })
}

/// The domains reserved for documentation (RFC 2606 and RFC 6761), where no
/// one gets mail.
fn reserved_domain(domain: &str) -> bool {
    const DOMAINS: [&str; 3] = ["example.com", "example.net", "example.org"];
    const TLDS: [&str; 4] = ["example", "test", "invalid", "localhost"];
    DOMAINS.iter().any(|reserved| {
        domain == *reserved
            || domain
                .strip_suffix(reserved)
                .is_some_and(|sub| sub.ends_with('.'))
    }) || domain
        .rsplit('.')
        .next()
        .is_some_and(|tld| TLDS.contains(&tld))
}

fn cpf_is_valid(digits: &str) -> bool {
    let d: Vec<u32> = digits.chars().filter_map(|c| c.to_digit(10)).collect();
    if d.len() != 11 || d.iter().all(|&x| x == d[0]) {
        return false;
    }
    [9usize, 10].iter().all(|&size| {
        let total: u32 = d[..size]
            .iter()
            .zip((2..=u32::try_from(size).unwrap() + 1).rev())
            .map(|(digit, weight)| digit * weight)
            .sum();
        (total * 10) % 11 % 10 == d[size]
    })
}

/// `BoletoCobranca` -> `BOLETO_COBRANCA`.
fn screaming_snake(camel: &str) -> String {
    let mut out = String::new();
    for (i, c) in camel.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(c.to_ascii_uppercase());
    }
    out
}

/// Name of the detail schema a typed variant models, and its unmapped fields.
fn typed_detail(detalhe: &Detalhe) -> Option<(&'static str, &Map<String, Value>)> {
    Some(match detalhe {
        Detalhe::Pix(d) => ("DetalhePix", &d.outros),
        Detalhe::BoletoCobranca(d) => ("DetalheBoletoCobranca", &d.outros),
        Detalhe::Cashback(d) => ("DetalheCashback", &d.outros),
        Detalhe::Cheque(d) => ("DetalheCheque", &d.outros),
        Detalhe::CompraDebito(d) => ("DetalheCompraDebito", &d.outros),
        Detalhe::DepositoBoleto(d) => ("DetalheDepositoBoleto", &d.outros),
        Detalhe::Transferencia(d) => ("DetalheTransferencia", &d.outros),
        Detalhe::Pagamento(d) => ("DetalhePagamento", &d.outros),
        Detalhe::Tarifa(d) => ("DetalheTarifa", &d.outros),
        _ => return None,
    })
}
