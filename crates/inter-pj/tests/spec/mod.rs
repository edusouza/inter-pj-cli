//! Reading Inter's OpenAPI specification (`spec/inter-empresas-openapi.json`)
//! in the contract tests.

#![allow(dead_code, unreachable_pub)]

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::OnceLock;

use inter_pj::endpoint::Endpoint;
use serde_json::{Map, Value, json};

pub fn spec() -> &'static Value {
    static SPEC: OnceLock<Value> = OnceLock::new();
    SPEC.get_or_init(|| {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/inter-empresas-openapi.json");
        let raw = std::fs::read(&path).unwrap_or_else(|err| panic!("{}: {err}", path.display()));
        serde_json::from_slice(&raw).unwrap()
    })
}

/// Where the specification contradicts itself about an address: our path
/// and the key the specification lists the operation under.
pub const DIVERGENCIAS: &[(&str, &str)] = &[(
    // The operation's own description shows this address, which, like the
    // retries of the Banking and Pix APIs, is the path of the history plus
    // `/retry`; the key lacks `/cobrancas`.
    "/cobranca/v3/cobrancas/webhook/callbacks/retry",
    "/cobranca/v3/webhook/callbacks/retry",
)];

/// Operations the specification lists twice: the key of the copy and the
/// one of the operation, which the registry has.
pub const DUPLICADAS: &[(&str, &str)] = &[(
    // The payment of a QR Code in the sandbox, among the immediate charges
    // and again among the charges with a due date, with a space in the path
    // (same scopes, body and responses).
    "/pix/v2/ sandbox/cob/pagamento",
    "/pix/v2/sandbox/cob/pagamento",
)];

/// The key of the specification for one of our paths.
pub fn spec_path(path: &'static str) -> &'static str {
    DIVERGENCIAS
        .iter()
        .find(|(nosso, _)| *nosso == path)
        .map_or(path, |(_, especificacao)| especificacao)
}

pub fn operation(endpoint: &Endpoint) -> &'static Value {
    let method = endpoint.method.as_str().to_lowercase();
    let op = &spec()["paths"][spec_path(endpoint.path)][method.as_str()];
    assert!(op.is_object(), "{endpoint} não existe na especificação");
    op
}

pub fn spec_scopes(op: &Value) -> BTreeSet<String> {
    op["security"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|requirement| requirement["oauth2"].as_array())
        .flatten()
        .filter_map(|scope| scope.as_str().map(str::to_owned))
        .collect()
}

pub fn schema(name: &str) -> &'static Value {
    let schema = &spec()["components"]["schemas"][name];
    assert!(schema.is_object(), "schema {name} não encontrado");
    schema
}

pub fn resolve(value: &'static Value) -> &'static Value {
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

pub fn example_for_schema(name: &str) -> Value {
    example(schema(name), "", 0)
}

/// Builds an example document from a schema, preferring the documented
/// examples. `name` is the property being built: amounts documented as
/// strings without an example (`valor...`) get a numeric text.
pub fn example(schema: &'static Value, name: &str, depth: usize) -> Value {
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
    // The first option, with what the `allOf` and the properties beside it
    // add to every option.
    if let Some(first) = schema
        .get("oneOf")
        .and_then(Value::as_array)
        .and_then(|options| options.first())
    {
        match example(first, name, depth + 1) {
            Value::Object(object) => merged.extend(object),
            other if merged.is_empty() && schema.get("properties").is_none() => return other,
            _ => {}
        }
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

pub fn parameters(endpoint: &Endpoint) -> Map<String, Value> {
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

/// A parameter of an operation, as the specification has it.
pub fn parametro(endpoint: &Endpoint, nome: &str) -> &'static Value {
    operation(endpoint)["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .map(resolve)
        .find(|parametro| parametro["name"] == nome)
        .unwrap_or_else(|| panic!("{endpoint}: parâmetro {nome}"))
}

/// Properties of a schema, including the ones inherited through `allOf`.
pub fn property_names(name: &str) -> BTreeSet<String> {
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

pub fn keys(value: &Value) -> BTreeSet<String> {
    value.as_object().unwrap().keys().cloned().collect()
}

pub fn enum_values(name: &str) -> BTreeSet<&'static str> {
    schema(name)["enum"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect()
}

pub fn parameter_names(endpoint: &Endpoint) -> BTreeSet<String> {
    parameters(endpoint).keys().cloned().collect()
}

/// Every documented property is serialized back with the same value.
pub fn assert_round_trip(name: &str, example: &Value, back: &Value) {
    assert_same_keys(name, back);
    for key in schema(name)["properties"].as_object().unwrap().keys() {
        assert_eq!(
            numeric(&back[key]),
            numeric_text(&example[key]),
            "{name}.{key}"
        );
    }
}

pub fn assert_same_keys(name: &str, back: &Value) {
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
pub fn numeric_fields(value: &Value) -> Value {
    Value::Object(
        value
            .as_object()
            .unwrap()
            .iter()
            .map(|(key, value)| (key.clone(), numeric_text(value)))
            .collect(),
    )
}

pub fn numeric_text(value: &Value) -> Value {
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

pub fn numeric(value: &Value) -> Value {
    match value {
        Value::Number(n) => json!(n.as_f64()),
        other => other.clone(),
    }
}

/// A property of a schema, looking into `allOf`, `oneOf` and `anyOf`.
pub fn propriedade(schema: &'static Value, nome: &str) -> Option<&'static Value> {
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
pub fn documentados(schema: &'static Value, prefixo: &str, caminhos: &mut BTreeSet<String>) {
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
pub fn caminhos(valor: &Value, prefixo: &str, saida: &mut BTreeSet<String>) {
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

pub fn assert_documentado(nome: &str, enviado: &Value) {
    let mut esperados = BTreeSet::new();
    documentados(schema(nome), "", &mut esperados);
    let mut usados = BTreeSet::new();
    caminhos(enviado, "", &mut usados);
    let fora: Vec<&String> = usados.difference(&esperados).collect();
    assert!(fora.is_empty(), "campos fora do schema {nome}: {fora:?}");
}

pub fn strings(valores: &[impl AsRef<str>]) -> BTreeSet<String> {
    valores
        .iter()
        .map(|valor| valor.as_ref().to_owned())
        .collect()
}

pub fn enum_de(schema: &'static Value) -> BTreeSet<String> {
    schema["enum"]
        .as_array()
        .unwrap_or_else(|| panic!("sem enum: {schema}"))
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}
