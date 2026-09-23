//! Contract tests: the endpoint registry, the scopes and the models must agree
//! with Inter's OpenAPI specification (`spec/inter-empresas-openapi.json`).

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::OnceLock;

use inter_pj::banking::Saldo;
use inter_pj::endpoint::{self, Endpoint};
use inter_pj::{Environment, Scope};
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
    let op = operation(&endpoint::banking::SALDO);
    let names: Vec<&str> = op["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|p| resolve(p)["name"].as_str())
        .collect();
    assert!(names.contains(&"dataSaldo"), "{names:?}");
    assert!(names.contains(&"x-conta-corrente"), "{names:?}");
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
    example(schema(name), 0)
}

/// Builds an example document from a schema, preferring the documented examples.
fn example(schema: &'static Value, depth: usize) -> Value {
    let schema = resolve(schema);
    if let Some(value) = schema.get("example") {
        return value.clone();
    }
    if let Some(all_of) = schema.get("allOf").and_then(Value::as_array) {
        let mut merged = Map::new();
        for part in all_of {
            if let Value::Object(object) = example(part, depth + 1) {
                merged.extend(object);
            }
        }
        return Value::Object(merged);
    }
    if let Some(value) = schema.get("enum").and_then(|e| e.get(0)) {
        return value.clone();
    }
    match schema.get("type").and_then(Value::as_str) {
        Some("object") | None if schema.get("properties").is_some() && depth < 8 => {
            let object = schema["properties"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(key, property)| (key.clone(), example(property, depth + 1)))
                .collect();
            Value::Object(object)
        }
        Some("array") => json!([example(&schema["items"], depth + 1)]),
        Some("integer") => json!(1),
        Some("number") => json!(1.5),
        Some("boolean") => json!(true),
        _ => json!("texto"),
    }
}

fn numeric(value: &Value) -> Value {
    match value {
        Value::Number(n) => json!(n.as_f64()),
        other => other.clone(),
    }
}
