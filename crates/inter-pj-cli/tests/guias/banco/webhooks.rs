//! The webhooks of the account, one for each kind of the Banking API, one
//! for the charges, one for each Pix key and one for each kind of Pix
//! Automático (`/banking/v2/webhooks`, `/cobranca/v3/cobrancas/webhook`,
//! `/pix/v2/webhook`, `/pix/v2/webhookrec`, `/pix/v2/webhookcobr`), and the
//! history of the attempts to send their callbacks. Since August, the
//! company's server has had the webhooks of the Pix the account sends, of
//! its charges and of its Pix key, and the history has the attempts of the
//! notifications of the statement: the charge Beltrana de Tal paid, delivered
//! on the second attempt, the one that expired, which the server refused
//! every time, the Pix sent in August and in September and the Pix charge
//! paid in September. A retry sends the callbacks of the operations found
//! again, at once and delivered, to the address the webhook has then. What
//! is registered or changed today takes the next time of a clock, 6 minutes
//! after the one before.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, FixedOffset, SecondsFormat, TimeDelta};
use serde_json::{Value, json};
use wiremock::matchers::{path, path_regex};
use wiremock::{MockServer, Request, ResponseTemplate};

use super::{parametros, problema, requisicao};
use crate::sessao::HOJE;

/// Where the company's server receives the notifications.
const SERVIDOR: &str = "https://api.empresa.example/inter";

/// The Pix key of the account.
const CHAVE: &str = "pix@empresa.example";

/// The webhooks, by the path of their operations.
const PIX_PAGAMENTO: &str = "/banking/v2/webhooks/pix-pagamento";
const BOLETO_PAGAMENTO: &str = "/banking/v2/webhooks/boleto-pagamento";
const COBRANCA: &str = "/cobranca/v3/cobrancas/webhook";
const PIX: &str = "/pix/v2/webhook/";

/// Callbacks per page when the request does not say.
const POR_PAGINA: usize = 20;

/// Whose history: of a kind of the Banking API, of the charges or of the Pix
/// keys.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Historico {
    PixPagamento,
    BoletoPagamento,
    Cobranca,
    Pix,
}

impl Historico {
    /// The field of the payload a listing filters by, as its parameter.
    fn filtro(self) -> &'static str {
        match self {
            Self::PixPagamento => "endToEnd",
            Self::BoletoPagamento => "codigoTransacao",
            Self::Cobranca => "codigoSolicitacao",
            Self::Pix => "txid",
        }
    }

    /// The field of the body of a retry, and the one of the payload it names.
    fn reenvio(self) -> (&'static str, &'static str) {
        match self {
            Self::PixPagamento | Self::Cobranca => ("codigoSolicitacao", "codigoSolicitacao"),
            Self::BoletoPagamento => ("codigoTransacao", "codigoTransacao"),
            Self::Pix => ("txId", "txid"),
        }
    }

    /// The field of the time of an attempt.
    fn disparo(self) -> &'static str {
        match self {
            Self::PixPagamento | Self::BoletoPagamento => "dataEnvio",
            Self::Cobranca | Self::Pix => "dataHoraDisparo",
        }
    }
}

struct Estado {
    /// Each webhook, by the path of its operations.
    webhooks: HashMap<String, Value>,
    /// The attempts of each history, in the order they were made.
    tentativas: HashMap<Historico, Vec<Value>>,
    /// What was done today, for the time of the next one.
    feitos: i64,
}

impl Estado {
    /// The webhooks and the attempts before the guides.
    fn novo() -> Self {
        let webhooks = HashMap::from([
            (
                PIX_PAGAMENTO.to_owned(),
                json!({
                    "webhookUrl": format!("{SERVIDOR}/pix-enviados"),
                    "criacao": "2026-08-01T12:00:00.000Z",
                }),
            ),
            (
                COBRANCA.to_owned(),
                json!({
                    "webhookUrl": format!("{SERVIDOR}/cobrancas"),
                    "criacao": "2026-08-01T12:02:10.000Z",
                }),
            ),
            (
                format!("{PIX}{CHAVE}"),
                json!({
                    "webhookUrl": format!("{SERVIDOR}/pix-cobrancas"),
                    "chave": CHAVE,
                    "criacao": "2026-08-01T12:04:45.000Z",
                }),
            ),
        ]);
        Self {
            webhooks,
            tentativas: HashMap::from([
                (Historico::PixPagamento, pix_enviados()),
                (Historico::Cobranca, cobrancas()),
                (Historico::Pix, cobrancas_pix()),
            ]),
            feitos: 0,
        }
    }

    /// The time of what is done now, today.
    fn agora(&mut self) -> String {
        let inicio = DateTime::parse_from_rfc3339(&format!("{HOJE}T14:02:27.000Z")).unwrap();
        let agora = inicio + TimeDelta::minutes(6 * self.feitos);
        self.feitos += 1;
        instante(agora)
    }

    /// A webhook registered, or its address changed: the one of the
    /// charges keeps when it was registered and tells when it changed.
    fn cadastrar(&mut self, request: &Request) -> ResponseTemplate {
        let corpo: Value = serde_json::from_slice(&request.body).unwrap_or_default();
        let Some(url) = corpo["webhookUrl"]
            .as_str()
            .filter(|url| url.starts_with("https://"))
        else {
            return problema(
                400,
                "URL inválida",
                "A URL do webhook deve começar com https://.",
            );
        };
        let caminho = request.url.path().to_owned();
        let agora = self.agora();
        match self.webhooks.get_mut(&caminho) {
            Some(webhook) if caminho == COBRANCA => {
                webhook["webhookUrl"] = json!(url);
                webhook["atualizacao"] = json!(agora);
            }
            _ => {
                let mut webhook = json!({"webhookUrl": url, "criacao": agora});
                if let Some(chave) = caminho.strip_prefix(PIX) {
                    webhook["chave"] = json!(chave);
                }
                self.webhooks.insert(caminho, webhook);
            }
        }
        ResponseTemplate::new(204)
    }

    fn consultar(&self, request: &Request) -> ResponseTemplate {
        match self.webhooks.get(request.url.path()) {
            Some(webhook) => ResponseTemplate::new(200).set_body_json(webhook),
            None => nao_encontrado(),
        }
    }

    fn excluir(&mut self, request: &Request) -> ResponseTemplate {
        match self.webhooks.remove(request.url.path()) {
            Some(_) => ResponseTemplate::new(204),
            None => nao_encontrado(),
        }
    }
}

pub(super) async fn montar(servidor: &MockServer) {
    let estado = Arc::new(Mutex::new(Estado::novo()));
    let webhook = r"^/(banking/v2/webhooks/(pix|boleto)-pagamento|cobranca/v3/cobrancas/webhook|pix/v2/webhook/[^/]+|pix/v2/webhook(rec|cobr))$";
    for metodo in ["PUT", "GET", "DELETE"] {
        let estado = Arc::clone(&estado);
        requisicao(metodo, path_regex(webhook))
            .respond_with(move |request: &Request| {
                let mut estado = estado.lock().unwrap();
                match metodo {
                    "PUT" => estado.cadastrar(request),
                    "GET" => estado.consultar(request),
                    _ => estado.excluir(request),
                }
            })
            .mount(servidor)
            .await;
    }
    for (historico, caminho) in [
        (
            Historico::PixPagamento,
            format!("{PIX_PAGAMENTO}/callbacks"),
        ),
        (
            Historico::BoletoPagamento,
            format!("{BOLETO_PAGAMENTO}/callbacks"),
        ),
        (Historico::Cobranca, format!("{COBRANCA}/callbacks")),
        (Historico::Pix, format!("{PIX}callbacks")),
    ] {
        let lista = Arc::clone(&estado);
        // Before the webhook of a Pix key named `callbacks`.
        requisicao("GET", path(caminho.as_str()))
            .respond_with(move |request: &Request| {
                listar(&lista.lock().unwrap(), historico, request)
            })
            .with_priority(1)
            .mount(servidor)
            .await;
        let reenvio = Arc::clone(&estado);
        requisicao("POST", path(format!("{caminho}/retry")))
            .respond_with(move |request: &Request| {
                reenviar(&mut reenvio.lock().unwrap(), historico, request)
            })
            .mount(servidor)
            .await;
    }
}

fn nao_encontrado() -> ResponseTemplate {
    problema(404, "Webhook não encontrado", "Não há webhook cadastrado.")
}

/// `2026-08-10T19:22:33.000Z`.
fn instante(momento: DateTime<FixedOffset>) -> String {
    momento.to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// An attempt to send `payload`, accepted by the server when it answered
/// 200.
fn tentativa(
    historico: Historico,
    url: &str,
    payload: &Value,
    numero: u64,
    quando: &str,
    status: u16,
) -> Value {
    let mut tentativa = json!({
        "webhookUrl": url,
        "payload": payload,
        "numeroTentativa": numero,
        historico.disparo(): quando,
        "sucesso": status == 200,
        "httpStatus": status,
    });
    let erro = match status {
        200 => None,
        400 => Some("Bad Request"),
        503 => Some("Service Unavailable"),
        _ => Some("Gateway Timeout"),
    };
    if let Some(erro) = erro {
        tentativa["mensagemErro"] = json!(erro);
    }
    tentativa
}

/// The attempts of `payload` from `primeira`, as Inter makes them: the next
/// ones `intervalos` minutes after the one before, with the status the
/// server answered to each.
fn tentativas(
    historico: Historico,
    url: &str,
    payload: &Value,
    primeira: &str,
    intervalos: &[i64],
    status: &[u16],
) -> Vec<Value> {
    let mut quando = DateTime::parse_from_rfc3339(primeira).unwrap();
    let mut feitas = Vec::new();
    for (numero, &status) in (1..).zip(status) {
        feitas.push(tentativa(
            historico,
            url,
            payload,
            numero,
            &instante(quando),
            status,
        ));
        if let Some(minutos) = intervalos.get(feitas.len() - 1) {
            quando += TimeDelta::minutes(*minutos);
        }
    }
    feitas
}

/// The retries of the Cobrança and Pix APIs: 20, 30, 60 and 120 minutes
/// after the attempt before.
const INTERVALOS: [i64; 4] = [20, 30, 60, 120];

/// The retries of the Banking API: 5, 10, 30 and 60 minutes after.
const INTERVALOS_BANKING: [i64; 4] = [5, 10, 30, 60];

/// The notifications of the charges: Beltrana de Tal's, paid while the
/// server restarted, and Fulano de Tal's, expired, which the server refused
/// every time, as it did not know expired charges.
fn cobrancas() -> Vec<Value> {
    let url = format!("{SERVIDOR}/cobrancas");
    let recebida = json!([{
        "codigoSolicitacao": "8e1f3a5c-7b9d-4e2f-8a4c-6e8f0a2c4e61",
        "seuNumero": "NF-0815",
        "situacao": "RECEBIDO",
        "dataHoraSituacao": "2026-08-10T19:22:31.000Z",
        "valorTotalRecebido": "890.00",
        "origemRecebimento": "BOLETO",
        "nossoNumero": "0012345678",
    }]);
    let expirada = json!([{
        "codigoSolicitacao": "5c2e8a41-7d3b-4f6e-9a1c-2b4d6f8e0a13",
        "seuNumero": "NF-0805",
        "situacao": "EXPIRADO",
        "dataHoraSituacao": "2026-08-21T05:10:00.000Z",
        "nossoNumero": "0012345667",
    }]);
    let historico = Historico::Cobranca;
    let mut feitas = tentativas(
        historico,
        &url,
        &recebida,
        "2026-08-10T19:22:33.000Z",
        &INTERVALOS,
        &[503, 200],
    );
    feitas.extend(tentativas(
        historico,
        &url,
        &expirada,
        "2026-08-21T05:10:02.000Z",
        &INTERVALOS,
        &[400; 5],
    ));
    feitas
}

/// The notifications of the Pix sent to Fornecedor Exemplo SA: August's,
/// delivered at once, and September's, on the second attempt.
fn pix_enviados() -> Vec<Value> {
    let url = format!("{SERVIDOR}/pix-enviados");
    let pix = |codigo: &str, e2e: &str, solicitacao: &str, movimento: &str| {
        json!({
            "tipoMovimentacao": "PAGAMENTO",
            "codigoSolicitacao": codigo,
            "chave": "financeiro@fornecedor.example",
            "dataHoraSolicitacao": solicitacao,
            "dataHoraMovimento": movimento,
            "endToEnd": e2e,
            "valor": "1200.00",
            "status": "EFETIVADO",
            "recebedor": {"nome": "Fornecedor Exemplo SA"},
        })
    };
    let historico = Historico::PixPagamento;
    let mut feitas = tentativas(
        historico,
        &url,
        &pix(
            "5f1c7a2e-8b3d-4e6f-9a0b-1c2d3e4f5a6b",
            "E12345678202608121303Qw8eR4tY6uI",
            "2026-08-12T13:03:45.000Z",
            "2026-08-12T13:03:47.000Z",
        ),
        "2026-08-12T13:03:49.000Z",
        &INTERVALOS_BANKING,
        &[200],
    );
    feitas.extend(tentativas(
        historico,
        &url,
        &pix(
            "0b7e3d9c-2a4f-4c8e-b1d6-7f5a9e3c2b10",
            "E12345678202609081344Mn6bV5cX4zQ",
            "2026-09-08T13:44:00.000Z",
            "2026-09-08T13:44:02.000Z",
        ),
        "2026-09-08T13:44:04.000Z",
        &INTERVALOS_BANKING,
        &[504, 200],
    ));
    feitas
}

/// The notification of the immediate charge of order 1053, paid by the
/// Cliente Exemplo Ltda, delivered at once.
fn cobrancas_pix() -> Vec<Value> {
    let pago = json!({"pix": [{
        "endToEndId": "E12345678202609021215Po0iU9yT8rE",
        "txid": "pedido1053empresaexemplo2026",
        "valor": "1500.00",
        "chave": CHAVE,
        "horario": "2026-09-02T12:15:38.000Z",
        "infoPagador": "Pedido 1053",
    }]});
    tentativas(
        Historico::Pix,
        &format!("{SERVIDOR}/pix-cobrancas"),
        &pago,
        "2026-09-02T12:15:40.000Z",
        &INTERVALOS,
        &[200],
    )
}

/// Whether `valor` has the field `campo` equal to `esperado`, in itself or
/// in its lists and objects.
fn tem(valor: &Value, campo: &str, esperado: &str) -> bool {
    match valor {
        Value::Object(objeto) => {
            objeto
                .get(campo)
                .and_then(Value::as_str)
                .is_some_and(|achado| achado.eq_ignore_ascii_case(esperado))
                || objeto.values().any(|filho| tem(filho, campo, esperado))
        }
        Value::Array(itens) => itens.iter().any(|item| tem(item, campo, esperado)),
        _ => false,
    }
}

/// The attempts of the period, with the filter, latest first, in pages
/// from 0.
fn listar(estado: &Estado, historico: Historico, request: &Request) -> ResponseTemplate {
    let parametros = parametros(request);
    let momento = |campo: &str| {
        parametros
            .get(campo)
            .and_then(|texto| DateTime::parse_from_rfc3339(texto).ok())
    };
    let (Some(inicio), Some(fim)) = (momento("dataHoraInicio"), momento("dataHoraFim")) else {
        return problema(
            400,
            "Período inválido",
            "dataHoraInicio e dataHoraFim são obrigatórios, em RFC 3339.",
        );
    };
    let quando = |tentativa: &Value| {
        DateTime::parse_from_rfc3339(tentativa[historico.disparo()].as_str().unwrap()).unwrap()
    };
    let filtro = parametros.get(historico.filtro());
    let mut feitas: Vec<&Value> = estado
        .tentativas
        .get(&historico)
        .into_iter()
        .flatten()
        .filter(|tentativa| {
            (inicio..=fim).contains(&quando(tentativa))
                && filtro.is_none_or(|id| tem(&tentativa["payload"], historico.filtro(), id))
        })
        .collect();
    feitas.sort_by_key(|tentativa| std::cmp::Reverse(quando(tentativa)));
    let pagina: usize = parametros
        .get("pagina")
        .map_or(0, |pagina| pagina.parse().unwrap());
    let por_pagina: usize = parametros
        .get("tamanhoPagina")
        .map_or(POR_PAGINA, |itens| itens.parse().unwrap());
    let paginas = feitas.len().div_ceil(por_pagina);
    let nesta: Vec<&Value> = feitas
        .iter()
        .skip(pagina * por_pagina)
        .take(por_pagina)
        .copied()
        .collect();
    ResponseTemplate::new(200).set_body_json(json!({
        "totalElementos": feitas.len(),
        "totalPaginas": paginas,
        "primeiraPagina": pagina == 0,
        "ultimaPagina": pagina + 1 >= paginas,
        "data": nesta,
    }))
}

/// The callbacks of the operations of the body sent again, now and
/// delivered, to the address of the webhook; the answer names those found in
/// the history.
fn reenviar(estado: &mut Estado, historico: Historico, request: &Request) -> ResponseTemplate {
    let corpo: Value = serde_json::from_slice(&request.body).unwrap_or_default();
    let (campo, no_payload) = historico.reenvio();
    let Some(ids) = corpo[campo].as_array() else {
        return problema(
            400,
            "Requisição inválida",
            &format!("O campo {campo} é obrigatório."),
        );
    };
    let chave = corpo["chavePix"].as_str().unwrap_or_default();
    let webhook = match historico {
        Historico::PixPagamento => PIX_PAGAMENTO.to_owned(),
        Historico::BoletoPagamento => BOLETO_PAGAMENTO.to_owned(),
        Historico::Cobranca => COBRANCA.to_owned(),
        Historico::Pix => format!("{PIX}{chave}"),
    };
    let Some(url) = estado
        .webhooks
        .get(&webhook)
        .and_then(|webhook| webhook["webhookUrl"].as_str())
        .map(str::to_owned)
    else {
        return nao_encontrado();
    };
    let agora = estado.agora();
    let feitas = estado.tentativas.entry(historico).or_default();
    let mut encontrados = Vec::new();
    for id in ids.iter().filter_map(Value::as_str) {
        let ultima = feitas
            .iter()
            .rfind(|feita| {
                tem(&feita["payload"], no_payload, id)
                    && (historico != Historico::Pix || tem(&feita["payload"], "chave", chave))
            })
            .cloned();
        if let Some(ultima) = ultima {
            let numero = ultima["numeroTentativa"].as_u64().unwrap() + 1;
            feitas.push(tentativa(
                historico,
                &url,
                &ultima["payload"],
                numero,
                &agora,
                200,
            ));
            encontrados.push(id);
        }
    }
    ResponseTemplate::new(200).set_body_json(json!({"foundIds": encontrados}))
}
