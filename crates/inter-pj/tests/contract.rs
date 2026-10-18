//! Contract tests: the endpoint registry, the scopes and the models must agree
//! with Inter's OpenAPI specification (`spec/inter-empresas-openapi.json`).

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::OnceLock;

use chrono::NaiveDate;
use inter_pj::banking::{
    ConsultaPix, DadosBancarios, Destinatario, Detalhe, IdIdempotente, InstituicaoFinanceira,
    LoteScroll, MAX_DESCRICAO, PagamentoPix, PaginaExtrato, Saldo, SolicitacaoPix, StatusPix,
    TipoConta, TipoOperacao, TipoRetornoPix, TipoTransacao, TransacaoCompleta, TransacaoSimples,
};
use inter_pj::endpoint::{self, Endpoint};
use inter_pj::{Environment, Scope};
use rust_decimal::Decimal;
use serde_json::{Map, Value, json};

/// Prefix of the Forum API, which is not part of the account features (issue #60).
const OUT_OF_SCOPE_PREFIX: &str = "/forum/";

fn spec() -> &'static Value {
    static SPEC: OnceLock<Value> = OnceLock::new();
    SPEC.get_or_init(|| {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/inter-empresas-openapi.json");
        let raw = std::fs::read(&path).unwrap_or_else(|err| panic!("{}: {err}", path.display()));
        serde_json::from_slice(&raw).unwrap()
    })
}

fn operation(endpoint: &Endpoint) -> &'static Value {
    let method = endpoint.method.as_str().to_lowercase();
    let op = &spec()["paths"][endpoint.path][method.as_str()];
    assert!(op.is_object(), "{endpoint} não existe na especificação");
    op
}

fn spec_scopes(op: &Value) -> BTreeSet<String> {
    op["security"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|requirement| requirement["oauth2"].as_array())
        .flatten()
        .filter_map(|scope| scope.as_str().map(str::to_owned))
        .collect()
}

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
                    chave: "chavepix@teste.com".parse().unwrap(),
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

/// The portal's examples carried real-looking CPFs, phone numbers and bank
/// accounts; `spec/sanitizar.py` replaces them with synthetic values. This
/// test keeps it that way when the specification is updated.
#[test]
fn spec_examples_contain_no_real_looking_personal_data() {
    const ALLOWED_CPFS: [&str; 2] = ["01234567890", "12345678909"];
    const SYNTHETIC_PHONE: &str = "+5500000000000";
    const ACCOUNT_DIGITS: &str = "123456789012345678901234567890";

    let mut problems = Vec::new();
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
            problems.push(format!("CPF em {location}"));
        }
        if text.starts_with("+55") && text != SYNTHETIC_PHONE {
            problems.push(format!("telefone em {location}"));
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
            problems.push(format!("conta em {location}"));
        }
    });
    assert!(
        problems.is_empty(),
        "rode `python3 spec/sanitizar.py`; dados possivelmente reais:\n{problems:#?}"
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

fn schema(name: &str) -> &'static Value {
    let schema = &spec()["components"]["schemas"][name];
    assert!(schema.is_object(), "schema {name} não encontrado");
    schema
}

fn resolve(value: &'static Value) -> &'static Value {
    match value.get("$ref").and_then(Value::as_str) {
        Some(reference) => {
            let pointer = reference.trim_start_matches('#');
            resolve(
                spec()
                    .pointer(pointer)
                    .unwrap_or_else(|| panic!("referência {reference}")),
            )
        }
        None => value,
    }
}

fn example_for_schema(name: &str) -> Value {
    example(schema(name), "", 0)
}

/// Builds an example document from a schema, preferring the documented
/// examples. `name` is the property being built: amounts documented as
/// strings without an example (`valor...`) get a numeric text.
fn example(schema: &'static Value, name: &str, depth: usize) -> Value {
    let schema = resolve(schema);
    if let Some(value) = schema.get("example") {
        return value.clone();
    }
    let mut merged = Map::new();
    if let Some(all_of) = schema.get("allOf").and_then(Value::as_array) {
        for part in all_of {
            if let Value::Object(object) = example(part, name, depth + 1) {
                merged.extend(object);
            }
        }
    }
    if let Some(first) = schema
        .get("oneOf")
        .and_then(Value::as_array)
        .and_then(|options| options.first())
    {
        return example(first, name, depth + 1);
    }
    if let Some(value) = schema.get("enum").and_then(|e| e.get(0)) {
        return value.clone();
    }
    match schema.get("type").and_then(Value::as_str) {
        _ if schema.get("properties").is_some() && depth < 8 => {
            for (key, property) in schema["properties"].as_object().unwrap() {
                merged.insert(key.clone(), example(property, key, depth + 1));
            }
            Value::Object(merged)
        }
        _ if !merged.is_empty() => Value::Object(merged),
        // `items` alone also means an array (`ConsultaPixAsyncResponse.historico`).
        _ if schema.get("items").is_some() => json!([example(&schema["items"], name, depth + 1)]),
        Some("integer") => json!(1),
        Some("number") => json!(1.5),
        Some("boolean") => json!(true),
        _ => match schema.get("format").and_then(Value::as_str) {
            Some("date") => json!("2026-01-02"),
            Some("date-time") => json!("2026-01-02T10:00:00-03:00"),
            _ if name.starts_with("valor") => json!("10.50"),
            _ => json!("texto"),
        },
    }
}

fn parameters(endpoint: &Endpoint) -> Map<String, Value> {
    operation(endpoint)["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            let p = resolve(p);
            (p["name"].as_str().unwrap().to_owned(), p.clone())
        })
        .collect()
}

/// Properties of a schema, including the ones inherited through `allOf`.
fn property_names(name: &str) -> BTreeSet<String> {
    fn collect(schema: &'static Value, names: &mut BTreeSet<String>) {
        let schema = resolve(schema);
        for part in schema["allOf"].as_array().into_iter().flatten() {
            collect(part, names);
        }
        if let Some(properties) = schema["properties"].as_object() {
            names.extend(properties.keys().cloned());
        }
    }
    let mut names = BTreeSet::new();
    collect(schema(name), &mut names);
    names
}

fn keys(value: &Value) -> BTreeSet<String> {
    value.as_object().unwrap().keys().cloned().collect()
}

fn enum_values(name: &str) -> BTreeSet<&'static str> {
    schema(name)["enum"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect()
}

fn parameter_names(endpoint: &Endpoint) -> BTreeSet<String> {
    parameters(endpoint).keys().cloned().collect()
}

/// Every documented property is serialized back with the same value.
fn assert_round_trip(name: &str, example: &Value, back: &Value) {
    assert_same_keys(name, back);
    for key in schema(name)["properties"].as_object().unwrap().keys() {
        assert_eq!(
            numeric(&back[key]),
            numeric_text(&example[key]),
            "{name}.{key}"
        );
    }
}

fn assert_same_keys(name: &str, back: &Value) {
    let documented: BTreeSet<&String> = resolve(schema(name))["properties"]
        .as_object()
        .unwrap()
        .keys()
        .collect();
    let serialized: BTreeSet<&String> = back.as_object().unwrap().keys().collect();
    assert_eq!(
        serialized, documented,
        "campos do schema {name} sem correspondência no modelo"
    );
}

/// Top-level fields with numeric text (amounts are strings in the spec and
/// numbers in our JSON) normalised to numbers.
fn numeric_fields(value: &Value) -> Value {
    Value::Object(
        value
            .as_object()
            .unwrap()
            .iter()
            .map(|(key, value)| (key.clone(), numeric_text(value)))
            .collect(),
    )
}

fn numeric_text(value: &Value) -> Value {
    match value {
        Value::String(text) => text
            .parse::<f64>()
            .ok()
            .filter(|_| {
                text.chars()
                    .all(|c| c.is_ascii_digit() || c == '.' || c == '-')
            })
            .map_or_else(|| value.clone(), |n| json!(n)),
        other => numeric(other),
    }
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

fn numeric(value: &Value) -> Value {
    match value {
        Value::Number(n) => json!(n.as_f64()),
        other => other.clone(),
    }
}
