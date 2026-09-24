//! The Pix the account sends (`/banking/v2/pix`): each send, with its
//! idempotency key, and the query of each Pix sent. In this account, a Pix
//! above R$ 10.000,00 waits for approval in the Internet Banking, and a
//! scheduled one waits for its day.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rust_decimal::Decimal;
use serde_json::{Value, json};
use wiremock::matchers::{path, path_regex};
use wiremock::{MockServer, Request, ResponseTemplate};

use super::{problema, requisicao};
use crate::sessao::HOJE;

/// The idempotency key whose first send is made but whose answer gets lost
/// (a 504 of a gateway), so that the guide shows how to repeat a send
/// without paying twice.
const RESPOSTA_PERDIDA: &str = "7c1e4b9a-2f3d-4a8e-9b6c-5d0e1f2a3b4c";

/// Above this amount, a Pix waits for the approval of someone else.
const APROVACAO_ACIMA_DE: i64 = 10_000;

/// The codes the bank gives to the Pix sent, in order.
const CODIGOS: [&str; 8] = [
    "c42f0787-02cb-4b31-827e-459ec9d7ece1",
    "5b9e2d14-7a3c-4f08-9e61-2c8d4a7b3f90",
    "e7a1c3b5-9d2f-4e6a-8b0c-1f3d5a7c9e2b",
    "2d8f4b6a-1c3e-4a5b-9f7d-8e0a2c4b6d13",
    "9a3c5e7b-4d6f-4b8a-8c1e-3f5a7b9d1c24",
    "6f1b3d5c-8e0a-4c2d-9b4f-6a8c0e2d4f35",
    "b4d6f8a0-2c4e-4d6f-8a0b-2c4e6f8a0b46",
    "3e5a7c9d-6b8f-4e0a-9d2c-4e6a8c0b2d57",
];

/// The end-to-end ids of the Pix paid, in the same order.
const E2E: [&str; 8] = [
    "E12345678202609241030a7Bc9DeF1gH",
    "E12345678202609241037k2Lm4NoP6qR",
    "E12345678202609241044s8Tu0VwX2yZ",
    "E12345678202609241051b3Cd5EfG7hJ",
    "E12345678202609241058m9No1PqR3sT",
    "E12345678202609241105u5Vw7XyZ9aB",
    "E12345678202609241112c1De3FgH5jK",
    "E12345678202609241119n7Pq9RsT1uV",
];

/// A Pix sent, and what the bank answered.
struct Enviado {
    idempotencia: String,
    ordem: usize,
    corpo: Value,
    recebedor: Value,
    resposta: Value,
}

#[derive(Default)]
struct Estado {
    enviados: Vec<Enviado>,
}

pub(super) async fn montar(servidor: &MockServer) {
    let estado = Arc::new(Mutex::new(Estado::default()));
    let envios = Arc::clone(&estado);
    requisicao("POST", path("/banking/v2/pix"))
        .respond_with(move |request: &Request| enviar(&mut envios.lock().unwrap(), request))
        .mount(servidor)
        .await;
    requisicao("GET", path_regex(r"^/banking/v2/pix/[^/]+$"))
        .respond_with(move |request: &Request| consultar(&estado.lock().unwrap(), request))
        .mount(servidor)
        .await;
}

fn enviar(estado: &mut Estado, request: &Request) -> ResponseTemplate {
    let idempotencia = request
        .headers
        .get("x-id-idempotente")
        .and_then(|valor| valor.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    // The same key, the same answer: the bank does not pay twice.
    if let Some(enviado) = estado
        .enviados
        .iter()
        .find(|enviado| enviado.idempotencia == idempotencia)
    {
        return ResponseTemplate::new(200).set_body_json(&enviado.resposta);
    }
    let corpo: Value = serde_json::from_slice(&request.body).unwrap();
    let Some(recebedor) = recebedor(&corpo["destinatario"]) else {
        return problema(
            400,
            "Chave Pix não encontrada",
            "A chave informada não está cadastrada no DICT.",
        );
    };
    let ordem = estado.enviados.len();
    let valor: Decimal = corpo["valor"].to_string().parse().unwrap();
    let agendado = corpo["dataPagamento"].as_str();
    let tipo_retorno = match agendado {
        Some(_) => "AGENDADO",
        None if valor > Decimal::from(APROVACAO_ACIMA_DE) => "APROVACAO",
        None => "PROCESSADO",
    };
    let resposta = json!({
        "tipoRetorno": tipo_retorno,
        "codigoSolicitacao": CODIGOS[ordem],
        "dataPagamento": agendado.unwrap_or(HOJE),
        "dataOperacao": HOJE,
    });
    estado.enviados.push(Enviado {
        idempotencia: idempotencia.clone(),
        ordem,
        corpo,
        recebedor,
        resposta: resposta.clone(),
    });
    // Made, but the answer does not arrive; a repetition gets it above.
    if idempotencia == RESPOSTA_PERDIDA {
        return ResponseTemplate::new(504);
    }
    ResponseTemplate::new(200).set_body_json(resposta)
}

fn consultar(estado: &Estado, request: &Request) -> ResponseTemplate {
    let codigo = request.url.path().rsplit('/').next().unwrap_or_default();
    let Some(enviado) = estado
        .enviados
        .iter()
        .find(|enviado| CODIGOS[enviado.ordem] == codigo)
    else {
        return problema(
            404,
            "Pix não encontrado",
            "Não há Pix com este código de solicitação nos últimos 90 dias.",
        );
    };
    // Each Pix of the guide is sent 7 minutes after the one before.
    let minutos = 10 * 60 + 30 + 7 * enviado.ordem;
    let hora = |segundos: usize| {
        format!(
            "{HOJE}T{:02}:{:02}:{segundos:02}",
            minutos / 60,
            minutos % 60
        )
    };
    let (status, passos): (&str, &[&str]) =
        match enviado.resposta["tipoRetorno"].as_str().unwrap_or_default() {
            "AGENDADO" => ("AGENDADO", &["CRIADO", "AGENDADO"]),
            "APROVACAO" => ("AGUARDANDO_APROVACAO", &["CRIADO", "AGUARDANDO_APROVACAO"]),
            _ => ("PAGO", &["CRIADO", "ENVIADO", "PAGO"]),
        };
    let historico: Vec<Value> = passos
        .iter()
        .enumerate()
        .map(|(segundos, passo)| json!({ "status": passo, "dataHoraEvento": hora(segundos) }))
        .collect();
    let mut transacao = json!({
        "recebedor": enviado.recebedor,
        "erros": [],
        "valor": enviado.corpo["valor"],
        "status": status,
        "dataHoraSolicitacao": hora(0),
        "codigoSolicitacao": codigo,
    });
    if status == "PAGO" {
        transacao["endToEnd"] = json!(E2E[enviado.ordem]);
        transacao["dataHoraMovimento"] = json!(hora(passos.len() - 1));
    }
    if let Some(chave) = enviado.corpo["destinatario"]["chave"].as_str() {
        transacao["chave"] = json!(chave);
    }
    ResponseTemplate::new(200)
        .set_body_json(json!({ "transacaoPix": transacao, "historico": historico }))
}

/// Who receives a Pix to `destinatario`, as the query shows them; `None`
/// for a key no one has.
fn recebedor(destinatario: &Value) -> Option<Value> {
    let texto = |campo: &str| destinatario[campo].as_str().unwrap_or_default();
    let chave = match texto("tipo") {
        "CHAVE" => texto("chave").to_owned(),
        "PIX_COPIA_E_COLA" => campos(texto("pixCopiaECola"))
            .remove("26.01")
            .unwrap_or_default(),
        _ => {
            let documento = texto("cpfCnpj");
            return Some(json!({
                "nome": texto("nome"),
                "cpfCnpj": mascarado(documento),
                "codIspb": destinatario["instituicaoFinanceira"]["ispb"],
                "codAgencia": texto("agencia"),
                "nroConta": texto("contaCorrente"),
                "tipoConta": texto("tipoConta"),
            }));
        }
    };
    let (nome, documento, conta) = match chave.as_str() {
        "financeiro@fornecedor.example" | "12345678000195" => {
            ("Fornecedor Exemplo SA", "12345678000195", "1234567")
        }
        "12345678909" | "123e4567-e12b-12d1-a456-426655440000" => {
            ("Fulano de Tal", "12345678909", "7654321")
        }
        _ => return None,
    };
    Some(json!({
        "nome": nome,
        "cpfCnpj": mascarado(documento),
        "codIspb": "12345678",
        "codAgencia": "0001",
        "nroConta": conta,
        "tipoConta": "CONTA_CORRENTE",
    }))
}

/// A CPF with the first and last digits hidden, as the bank shows it; a
/// CNPJ, formatted.
fn mascarado(documento: &str) -> String {
    let digitos: String = documento.chars().filter(char::is_ascii_digit).collect();
    match digitos.len() {
        11 => format!("***.{}.{}-**", &digitos[3..6], &digitos[6..9]),
        14 => format!(
            "{}.{}.{}/{}-{}",
            &digitos[..2],
            &digitos[2..5],
            &digitos[5..8],
            &digitos[8..12],
            &digitos[12..]
        ),
        _ => documento.to_owned(),
    }
}

/// The fields of a copia e cola code, by id; those of the merchant account
/// (26) as `26.01`, `26.02`...
fn campos(codigo: &str) -> HashMap<String, String> {
    fn ler(texto: &str, prefixo: &str, campos: &mut HashMap<String, String>) {
        let mut resto = texto;
        while resto.len() >= 4 {
            let (id, tamanho) = (&resto[..2], resto[2..4].parse::<usize>().unwrap_or(0));
            let Some(valor) = resto.get(4..4 + tamanho) else {
                return;
            };
            if id == "26" && prefixo.is_empty() {
                ler(valor, "26.", campos);
            }
            campos.insert(format!("{prefixo}{id}"), valor.to_owned());
            resto = &resto[4 + tamanho..];
        }
    }
    let mut todos = HashMap::new();
    ler(codigo, "", &mut todos);
    todos
}
