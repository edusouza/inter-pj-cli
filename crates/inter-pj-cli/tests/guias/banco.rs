//! The bank of the guides: a mock of the Inter API with the account of a
//! fictitious company, Empresa Exemplo Ltda, one module per API: [`conta`],
//! the balance and the statements from June to September 2026, [`pix`], the
//! Pix the account sends, [`recebidos`], the Pix it received and their
//! refunds, [`pagamentos`], the boletos, bills and taxes it pays,
//! [`cobranca`], the charges it issues to its clients, [`cob`], the
//! immediate Pix charges, [`cobv`], those with a due date, [`loc`], the
//! locations of their QR Codes, [`lote_cobv`], the batches of charges with a
//! due date, [`sandbox_pix`], the payments of the sandbox, which share one
//! state ([`cobrancas_pix`]), [`rec`], the recurrences of Pix Automático, in
//! a state of their own ([`automatico`]), and [`webhooks`], the addresses
//! the bank notifies and the history of the notifications. The data tell one
//! story: the balance follows from the statement, the Pix received, the
//! bills paid, the charges paid before and the notifications of the webhooks
//! are those of the statement, and what is sent, refunded, paid or issued
//! can be queried. Every name, document, key and amount is synthetic.

mod automatico;
mod cob;
mod cobranca;
mod cobrancas_pix;
mod cobv;
mod conta;
mod loc;
mod lote_cobv;
mod pagamentos;
mod pix;
mod rec;
mod recebidos;
mod sandbox_pix;
mod webhooks;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Match, Mock, MockBuilder, MockServer, Request, ResponseTemplate};

/// The access token the bank gives, and wants in every request.
const TOKEN: &str = "token-dos-guias";

pub(crate) struct Banco {
    servidor: MockServer,
}

impl Banco {
    pub(crate) async fn novo() -> Self {
        let servidor = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth/v2/token"))
            .respond_with(token)
            .mount(&servidor)
            .await;
        conta::montar(&servidor).await;
        pix::montar(&servidor).await;
        recebidos::montar(&servidor).await;
        pagamentos::montar(&servidor).await;
        cobranca::montar(&servidor).await;
        let cobrancas = Arc::new(Mutex::new(cobrancas_pix::Cobrancas::novo()));
        cob::montar(&servidor, &cobrancas).await;
        cobv::montar(&servidor, &cobrancas).await;
        loc::montar(&servidor, &cobrancas).await;
        lote_cobv::montar(&servidor, &cobrancas).await;
        sandbox_pix::montar(&servidor, &cobrancas).await;
        let automatico = Arc::new(Mutex::new(automatico::Automatico::novo()));
        rec::montar(&servidor, &automatico).await;
        webhooks::montar(&servidor).await;
        Self { servidor }
    }

    pub(crate) fn uri(&self) -> String {
        self.servidor.uri()
    }
}

/// A token with the scopes asked for, as the bank gives to an integration
/// that has them all.
fn token(request: &Request) -> ResponseTemplate {
    let formulario: HashMap<String, String> =
        serde_urlencoded::from_bytes(&request.body).unwrap_or_default();
    ResponseTemplate::new(200).set_body_json(json!({
        "access_token": TOKEN,
        "token_type": "Bearer",
        "expires_in": 3600,
        "scope": formulario.get("scope").map_or("", String::as_str),
    }))
}

/// A request of the API, which carries the token.
fn requisicao(metodo: &str, caminho: impl Match + 'static) -> MockBuilder {
    Mock::given(method(metodo))
        .and(caminho)
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
}

fn parametros(request: &Request) -> HashMap<String, String> {
    request.url.query_pairs().into_owned().collect()
}

/// An error of the API, in its format.
fn problema(status: u16, titulo: &str, detalhe: &str) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_json(json!({
        "title": titulo,
        "detail": detalhe,
        "violacoes": [],
    }))
}
