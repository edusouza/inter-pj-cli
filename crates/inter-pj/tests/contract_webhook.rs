//! Contract tests of the webhooks of the Banking, Cobrança and Pix APIs: the
//! models must agree with Inter's OpenAPI specification
//! (`spec/inter-empresas-openapi.json`).

mod spec;

use std::collections::BTreeSet;

use inter_pj::endpoint::{self, Endpoint};
use inter_pj::webhook::{TipoWebhookBanking, Webhook, WebhookUrl};
use serde_json::Value;
use spec::{example_for_schema, keys, operation, property_names, resolve};

/// The schema of the body of an operation.
fn corpo(endpoint: &Endpoint) -> &'static Value {
    resolve(&resolve(&operation(endpoint)["requestBody"])["content"]["application/json"]["schema"])
}

/// The schema of the answer `status` of an operation.
fn resposta(endpoint: &Endpoint, status: &str) -> &'static Value {
    resolve(&operation(endpoint)["responses"][status]["content"]["application/json"]["schema"])
}

/// A parameter of an operation.
fn parametro(endpoint: &Endpoint, nome: &str) -> &'static Value {
    operation(endpoint)["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .map(resolve)
        .find(|parametro| parametro["name"] == nome)
        .unwrap_or_else(|| panic!("{endpoint}: parâmetro {nome}"))
}

fn nomes(schema: &Value) -> BTreeSet<String> {
    schema["properties"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect()
}

#[test]
fn registrations_send_the_documented_address() {
    for endpoint in [
        endpoint::banking::WEBHOOK_CADASTRAR,
        endpoint::cobranca::WEBHOOK_CADASTRAR,
        endpoint::pix::WEBHOOK_CADASTRAR,
    ] {
        let schema = corpo(&endpoint);
        // The only field sent is the only one required.
        assert_eq!(
            schema["required"],
            serde_json::json!(["webhookUrl"]),
            "{endpoint}"
        );
        assert!(nomes(schema).contains("webhookUrl"), "{endpoint}");
        // `WebhookUrl` checks the rule of the documentation.
        assert_eq!(
            schema["properties"]["webhookUrl"]["pattern"], "^https://[^\\s]*$",
            "{endpoint}"
        );
        let exemplo = schema["properties"]["webhookUrl"]["example"]
            .as_str()
            .unwrap();
        assert!(WebhookUrl::parse(exemplo).is_ok(), "{endpoint}: {exemplo}");
    }
}

#[test]
fn lookups_keep_every_documented_field() {
    for (endpoint, schema) in [
        (endpoint::banking::WEBHOOK_CONSULTAR, "WebhookModel"),
        (endpoint::cobranca::WEBHOOK_CONSULTAR, "WebhookCompleto"),
        (endpoint::pix::WEBHOOK_CONSULTAR, "Pix_WebhookCompleto"),
    ] {
        assert_eq!(
            nomes(resposta(&endpoint, "200")),
            property_names(schema),
            "{endpoint}"
        );
        let webhook: Webhook = serde_json::from_value(example_for_schema(schema)).unwrap();
        let de_volta = serde_json::to_value(&webhook).unwrap();
        assert_eq!(keys(&de_volta), property_names(schema), "{schema}");
    }
}

#[test]
fn a_missing_webhook_is_the_documented_404() {
    for endpoint in [
        endpoint::banking::WEBHOOK_CONSULTAR,
        endpoint::cobranca::WEBHOOK_CONSULTAR,
        endpoint::pix::WEBHOOK_CONSULTAR,
    ] {
        assert!(
            operation(&endpoint)["responses"]["404"].is_object(),
            "{endpoint}"
        );
    }
}

#[test]
fn banking_kinds_match_the_spec() {
    let ours: BTreeSet<&str> = TipoWebhookBanking::TODOS
        .iter()
        .map(|tipo| tipo.as_str())
        .collect();
    for endpoint in [
        endpoint::banking::WEBHOOK_CADASTRAR,
        endpoint::banking::WEBHOOK_CONSULTAR,
        endpoint::banking::WEBHOOK_EXCLUIR,
    ] {
        let documentados: BTreeSet<&str> = resolve(&parametro(&endpoint, "tipoWebhook")["schema"])
            ["enum"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert_eq!(documentados, ours, "{endpoint}");
    }
}

#[test]
fn pix_keys_go_in_the_path() {
    for endpoint in [
        endpoint::pix::WEBHOOK_CADASTRAR,
        endpoint::pix::WEBHOOK_CONSULTAR,
        endpoint::pix::WEBHOOK_EXCLUIR,
    ] {
        let chave = parametro(&endpoint, "chave");
        assert_eq!(chave["in"], "path", "{endpoint}");
        // The rule `no_caminho` follows.
        assert!(
            chave["schema"]["description"]
                .as_str()
                .unwrap()
                .contains("**não** informar o caractere \"**+**\""),
            "{endpoint}"
        );
    }
}
