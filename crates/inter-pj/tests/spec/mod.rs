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

pub fn operation(endpoint: &Endpoint) -> &'static Value {
    let method = endpoint.method.as_str().to_lowercase();
    let op = &spec()["paths"][endpoint.path][method.as_str()];
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
