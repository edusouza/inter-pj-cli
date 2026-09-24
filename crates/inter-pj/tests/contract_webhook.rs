//! Contract tests of the webhooks of the Banking, Cobrança and Pix APIs: the
//! models must agree with Inter's OpenAPI specification
//! (`spec/inter-empresas-openapi.json`).

mod spec;

use std::collections::BTreeSet;

use inter_pj::endpoint::{self, Endpoint};
use inter_pj::webhook::{
    ITENS_POR_PAGINA_CALLBACKS_MAXIMO, ITENS_POR_PAGINA_CALLBACKS_MINIMO, MAX_IDS_REENVIO,
    PaginaCallbacks, ReenvioCallbacks, TipoWebhookBanking, Webhook, WebhookUrl,
};
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

/// The documented example `nome` of the answer `status` of an operation.
fn exemplo_da_resposta(endpoint: &Endpoint, status: &str, nome: &str) -> &'static Value {
    // The first content, whatever its type: the retries document theirs as
    // "aplication/json".
    let conteudo = resolve(&operation(endpoint)["responses"][status])["content"]
        .as_object()
        .and_then(|conteudos| conteudos.values().next())
        .unwrap_or_else(|| panic!("{endpoint}: resposta {status} sem conteúdo"));
    let exemplo = resolve(&conteudo["examples"][nome]);
    assert!(exemplo.is_object(), "{endpoint}: exemplo {nome}");
    &exemplo["value"]
}

#[test]
fn the_cobranca_retry_uses_the_address_of_its_description() {
    let nosso = endpoint::cobranca::WEBHOOK_REENVIAR.path;
    assert!(
        spec::spec()["paths"][nosso].is_null(),
        "a especificação passou a ter {nosso}: remova a divergência de spec::DIVERGENCIAS"
    );
    let descricao = operation(&endpoint::cobranca::WEBHOOK_REENVIAR)["description"]
        .as_str()
        .unwrap();
    assert!(
        descricao.contains("/cobranca/v3<span class=\"url\">/cobrancas/webhook/callbacks/retry"),
        "{descricao}"
    );
    // The history, the other operation of the same API, is under `/cobrancas`.
    assert!(spec::spec()["paths"][endpoint::cobranca::WEBHOOK_CALLBACKS.path].is_object());
}

#[test]
fn histories_document_the_parameters_we_send() {
    for (endpoint, identificadores) in [
        (
            endpoint::banking::WEBHOOK_CALLBACKS,
            &["endToEnd", "codigoTransacao"][..],
        ),
        (
            endpoint::cobranca::WEBHOOK_CALLBACKS,
            &["codigoSolicitacao"][..],
        ),
        (endpoint::pix::WEBHOOK_CALLBACKS, &["txid"][..]),
    ] {
        for nome in ["dataHoraInicio", "dataHoraFim"] {
            assert_eq!(
                parametro(&endpoint, nome)["required"],
                true,
                "{endpoint}: {nome}"
            );
        }
        for nome in ["pagina", "tamanhoPagina"].iter().chain(identificadores) {
            assert_eq!(
                parametro(&endpoint, nome)["in"],
                "query",
                "{endpoint}: {nome}"
            );
        }
        let tamanho = resolve(&parametro(&endpoint, "tamanhoPagina")["schema"]);
        assert_eq!(
            tamanho["minimum"], ITENS_POR_PAGINA_CALLBACKS_MINIMO,
            "{endpoint}"
        );
        assert_eq!(
            tamanho["maximum"], ITENS_POR_PAGINA_CALLBACKS_MAXIMO,
            "{endpoint}"
        );
    }
}

#[test]
fn histories_keep_every_documented_field() {
    for (endpoint, pagina, item) in [
        (
            endpoint::banking::WEBHOOK_CALLBACKS,
            "Banking_CallbackAttemptPage",
            "Banking_CallbackAttemptPageItem",
        ),
        (
            endpoint::cobranca::WEBHOOK_CALLBACKS,
            "CallbackAttemptPage",
            "CallbackAttemptPageItem",
        ),
        (
            endpoint::pix::WEBHOOK_CALLBACKS,
            "CallbackAttemptPage",
            "CallbackAttemptPageItem",
        ),
    ] {
        assert_eq!(
            nomes(resposta(&endpoint, "200")),
            property_names(pagina),
            "{endpoint}"
        );
        let lida: PaginaCallbacks = serde_json::from_value(example_for_schema(pagina)).unwrap();
        let de_volta = serde_json::to_value(&lida).unwrap();
        assert_eq!(keys(&de_volta), property_names(pagina), "{pagina}");
        assert_eq!(keys(&de_volta["data"][0]), property_names(item), "{item}");
    }
}

#[test]
fn the_documented_history_survives_a_round_trip() {
    let exemplo = exemplo_da_resposta(&endpoint::cobranca::WEBHOOK_CALLBACKS, "200", "exemplo1");
    let pagina: PaginaCallbacks = serde_json::from_value(exemplo.clone()).unwrap();
    assert_eq!(&serde_json::to_value(&pagina).unwrap(), exemplo);
    assert_eq!(pagina.data.len(), 2);
    assert_eq!(pagina.data[1].sucesso, Some(false));
    assert_eq!(pagina.data[1].http_status, Some(403));
    assert_eq!(pagina.data[1].disparo(), Some("2023-09-24T14:15:22Z"));
}

#[test]
fn retries_send_the_documented_bodies() {
    for (endpoint, campos) in [
        (
            endpoint::banking::WEBHOOK_REENVIAR,
            &["codigoSolicitacao"][..],
        ),
        (
            endpoint::cobranca::WEBHOOK_REENVIAR,
            &["codigoSolicitacao"][..],
        ),
        (endpoint::pix::WEBHOOK_REENVIAR, &["txId", "chavePix"][..]),
    ] {
        let schema = corpo(&endpoint);
        let documentados: BTreeSet<&str> = campos.iter().copied().collect();
        let obrigatorios: BTreeSet<&str> = schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert_eq!(obrigatorios, documentados, "{endpoint}");
        assert_eq!(
            nomes(schema),
            documentados
                .iter()
                .map(|campo| (*campo).to_owned())
                .collect(),
            "{endpoint}"
        );
        let lista = &schema["properties"][campos[0]];
        assert_eq!(lista["minItems"], 1, "{endpoint}");
        assert_eq!(lista["maxItems"], MAX_IDS_REENVIO, "{endpoint}");
    }
}

#[test]
fn retry_answers_accept_the_examples() {
    for endpoint in [
        endpoint::banking::WEBHOOK_REENVIAR,
        endpoint::cobranca::WEBHOOK_REENVIAR,
        endpoint::pix::WEBHOOK_REENVIAR,
    ] {
        let exemplo = exemplo_da_resposta(&endpoint, "200", "exemplo1");
        let reenvio: ReenvioCallbacks = serde_json::from_value(exemplo.clone()).unwrap();
        assert_eq!(reenvio.found_ids.len(), 1, "{endpoint}");
        assert_eq!(
            &serde_json::to_value(&reenvio).unwrap(),
            exemplo,
            "{endpoint}"
        );
    }
}
