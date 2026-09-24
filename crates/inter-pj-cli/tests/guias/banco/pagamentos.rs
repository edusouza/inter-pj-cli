//! The payments of the Banking API: by barcode (`/banking/v2/pagamento`),
//! the boletos, bills and taxes; the DARFs without a barcode; and the
//! batches of both. The account already paid the two bills of the statement
//! (July and August); the ones of the guides join them. As with the Pix, a
//! payment above R$ 10.000,00 waits for approval, and a scheduled one for
//! its day. There is no idempotency key: the answer of one bill gets lost,
//! for the guide to show how to check before paying again. A batch is
//! processed by the next look at it, and a boleto already paid is refused,
//! as the registry of boletos lets a bank do; a bill is paid again.

use std::sync::{Arc, Mutex};

use rust_decimal::Decimal;
use serde_json::{Value, json};
use wiremock::matchers::{path, path_regex};
use wiremock::{MockServer, Request, ResponseTemplate};

use super::{parametros, problema, requisicao};
use crate::sessao::HOJE;

/// Above this amount, a payment waits for the approval of someone else.
const APROVACAO_ACIMA_DE: i64 = 10_000;

/// The bill whose first payment is made but whose answer gets lost.
const RESPOSTA_PERDIDA: &str = "84620000001899002022026091000000000009876543";

/// Who receives the payment of each code (barcode), and their document.
const BENEFICIARIOS: [(&str, &str, Option<&str>); 7] = [
    (
        "07791158500001890000000123456700000000001581",
        "Fornecedor Exemplo SA",
        Some("12345678000195"),
    ),
    (
        "07797160000001890000000123456700000000001602",
        "Fornecedor Exemplo SA",
        Some("12345678000195"),
    ),
    (
        "07799150800000480000000123456700000000001497",
        "Fornecedor Exemplo SA",
        Some("12345678000195"),
    ),
    (
        "07792157500000312400000765432100000000000913",
        "Papelaria Exemplo",
        None,
    ),
    (
        "83640000002483701012026090100000000001234567",
        "Energia Exemplo SA",
        None,
    ),
    (
        "83610000002501001012026080100000000001234567",
        "Energia Exemplo SA",
        None,
    ),
    (
        "84620000001899002022026091000000000009876543",
        "Telefonia Exemplo SA",
        None,
    ),
];

/// The codes the bank gives to the payments of the guides, in order.
const CODIGOS: [&str; 8] = [
    "3414f226-36fb-4d87-811e-cfd99911d845",
    "8c1d2e3f-4a5b-4c6d-8e7f-9a0b1c2d3e4f",
    "a7b9c1d3-e5f7-4a9b-8c1d-3e5f7a9b1c2d",
    "4e6a8c0b-2d4f-4b6a-9c8e-0b2d4f6a8c1e",
    "d2f4a6c8-e0b2-4d4f-8a6c-8e0b2d4f6a8b",
    "6b8d0f2a-4c6e-4a8b-9d0f-2a4c6e8b0d3f",
    "f0a2c4e6-b8d0-4f2a-8c4e-6b8d0f2a4c7e",
    "1c3e5a7b-9d1f-4c3e-8a5b-7d9f1c3e5a60",
];

/// The codes of the DARFs paid, in order.
const SOLICITACOES_DARF: [&str; 4] = [
    "b1c2d3e4-f5a6-4b7c-8d9e-0f1a2b3c4d5e",
    "c2d3e4f5-a6b7-4c8d-9e0f-1a2b3c4d5e6f",
    "d3e4f5a6-b7c8-4d9e-8f1a-2b3c4d5e6f7a",
    "e4f5a6b7-c8d9-4e0f-9a2b-3c4d5e6f7a8b",
];

/// The ids of the batches, in order.
const LOTES: [&str; 2] = ["5f1b2c3d4e5f60718293a4b5", "6a2c3d4e5f60718293a4b5c6"];

struct Estado {
    feitos: Vec<Value>,
    darfs: Vec<Value>,
    /// Each batch sent, and what its processing gave, once someone looked.
    lotes: Vec<(Value, Option<Value>)>,
}

impl Estado {
    /// The payments of the statement, made before the guides.
    fn novo() -> Self {
        let pago = |codigo: &str, transacao: &str, dia: &str, valor: &str, nome: &str| {
            json!({
                "codigoTransacao": transacao,
                "codigoBarra": codigo,
                "tipo": if codigo.starts_with('8') { "CONVENIO" } else { "BOLETO" },
                "dataVencimentoDigitada": dia,
                "dataVencimentoTitulo": dia,
                "dataInclusao": format!("{dia}T08:30:00"),
                "dataPagamento": dia,
                "valorPago": valor,
                "valorNominal": valor,
                "statusPagamento": "REALIZADO",
                "nomeBeneficiario": nome,
                "aprovacoesNecessarias": 0,
                "aprovacoesRealizadas": 0,
            })
        };
        Self {
            feitos: vec![
                pago(
                    "07799150800000480000000123456700000000001497",
                    "0f1e2d3c-4b5a-4968-8776-655443322110",
                    "2026-07-15",
                    "480.00",
                    "Fornecedor Exemplo SA",
                ),
                pago(
                    "83610000002501001012026080100000000001234567",
                    "9e8d7c6b-5a49-4382-9716-05f4e3d2c1b0",
                    "2026-08-05",
                    "250.10",
                    "Energia Exemplo SA",
                ),
            ],
            darfs: Vec::new(),
            lotes: Vec::new(),
        }
    }
}

pub(super) async fn montar(servidor: &MockServer) {
    let estado = Arc::new(Mutex::new(Estado::novo()));
    let pagar = Arc::clone(&estado);
    requisicao("POST", path("/banking/v2/pagamento"))
        .respond_with(move |request: &Request| incluir(&mut pagar.lock().unwrap(), request))
        .mount(servidor)
        .await;
    let listar = Arc::clone(&estado);
    requisicao("GET", path("/banking/v2/pagamento"))
        .respond_with(move |request: &Request| buscar(&listar.lock().unwrap(), request))
        .mount(servidor)
        .await;
    let cancelamento = Arc::clone(&estado);
    requisicao("DELETE", path_regex(r"^/banking/v2/pagamento/[^/]+$"))
        .respond_with(move |request: &Request| cancelar(&mut cancelamento.lock().unwrap(), request))
        .mount(servidor)
        .await;
    let darf = Arc::clone(&estado);
    requisicao("POST", path("/banking/v2/pagamento/darf"))
        .respond_with(move |request: &Request| pagar_darf(&mut darf.lock().unwrap(), request))
        .mount(servidor)
        .await;
    let darfs = Arc::clone(&estado);
    requisicao("GET", path("/banking/v2/pagamento/darf"))
        .respond_with(move |request: &Request| buscar_darfs(&darfs.lock().unwrap(), request))
        .mount(servidor)
        .await;
    let lote = Arc::clone(&estado);
    requisicao("POST", path("/banking/v2/pagamento/lote"))
        .respond_with(move |request: &Request| enviar_lote(&mut lote.lock().unwrap(), request))
        .mount(servidor)
        .await;
    requisicao("GET", path_regex(r"^/banking/v2/pagamento/lote/[^/]+$"))
        .respond_with(move |request: &Request| consultar_lote(&mut estado.lock().unwrap(), request))
        .mount(servidor)
        .await;
}

/// An amount sent as a number or as text.
fn decimal(valor: &Value) -> Decimal {
    if valor.is_null() {
        return Decimal::ZERO;
    }
    valor
        .as_str()
        .map_or_else(|| valor.to_string(), str::to_owned)
        .parse()
        .unwrap()
}

/// An amount as the API sends it: a number.
fn numero(valor: Decimal) -> Value {
    json!(valor.to_string().parse::<f64>().unwrap())
}

/// A DARF paid, as the listing shows it: amounts as numbers, and dates
/// with the time.
fn darf_pago(darf: &Value, codigo: &str) -> Value {
    let principal = decimal(&darf["valorPrincipal"]);
    let multa = decimal(&darf["valorMulta"]);
    let juros = decimal(&darf["valorJuros"]);
    let dia = |campo: &str| format!("{} 00:00:00", darf[campo].as_str().unwrap_or_default());
    json!({
        "codigoSolicitacao": codigo,
        "tipoDarf": "PRETO",
        "tipo": "Darf",
        "valor": numero(principal),
        "valorMulta": numero(multa),
        "valorJuros": numero(juros),
        "valorTotal": numero(principal + multa + juros),
        "periodoApuracao": dia("periodoApuracao"),
        "dataVencimento": dia("dataVencimento"),
        "dataPagamento": format!("{HOJE} 00:00:00"),
        "dataInclusao": format!("{HOJE} 09:30:00"),
        "referencia": darf["referencia"],
        "codigoReceita": darf["codigoReceita"],
        "cnpjCpf": darf["cnpjCpf"],
        "statusPagamento": "REALIZADO",
        "aprovacoesNecessarias": 0,
        "aprovacoesRealizadas": 0,
    })
}

fn pagar_darf(estado: &mut Estado, request: &Request) -> ResponseTemplate {
    let corpo: Value = serde_json::from_slice(&request.body).unwrap();
    let codigo = SOLICITACOES_DARF[estado.darfs.len()];
    let mut pago = darf_pago(&corpo, codigo);
    let aprovacao = decimal(&pago["valorTotal"]) > Decimal::from(APROVACAO_ACIMA_DE);
    if aprovacao {
        pago["statusPagamento"] = json!("AGUARDANDO_APROVACAO");
        pago["aprovacoesNecessarias"] = json!(1);
    }
    estado.darfs.push(pago);
    // This answer has the day as DD/MM/AAAA.
    let (ano, resto) = HOJE.split_once('-').unwrap();
    let (mes, dia) = resto.split_once('-').unwrap();
    ResponseTemplate::new(200).set_body_json(json!({
        "codigoSolicitacao": codigo,
        "tipoRetorno": if aprovacao { "APROVACAO_PAGAMENTO" } else { "PAGAMENTO" },
        "dataPagamento": format!("{dia}/{mes}/{ano}"),
        "autenticacao": format!("{}{:04}", HOJE.replace('-', ""), estado.darfs.len()),
        "quantidadeAprovadores": u8::from(aprovacao),
    }))
}

/// The DARFs paid in the period, of a revenue code or of a request.
fn buscar_darfs(estado: &Estado, request: &Request) -> ResponseTemplate {
    let parametros = parametros(request);
    let achados: Vec<&Value> = estado
        .darfs
        .iter()
        .filter(|darf| {
            let data = darf["dataPagamento"]
                .as_str()
                .unwrap_or_default()
                .get(..10)
                .unwrap_or_default();
            parametros
                .get("dataInicio")
                .is_none_or(|inicio| inicio.as_str() <= data)
                && parametros
                    .get("dataFim")
                    .is_none_or(|fim| data <= fim.as_str())
                && parametros
                    .get("codigoReceita")
                    .is_none_or(|codigo| darf["codigoReceita"] == codigo.as_str())
                && parametros
                    .get("codigoSolicitacao")
                    .is_none_or(|codigo| darf["codigoSolicitacao"] == codigo.as_str())
        })
        .collect();
    ResponseTemplate::new(200).set_body_json(achados)
}

fn enviar_lote(estado: &mut Estado, request: &Request) -> ResponseTemplate {
    let corpo: Value = serde_json::from_slice(&request.body).unwrap();
    let id = LOTES[estado.lotes.len()];
    let resposta = json!({
        "idLote": id,
        "status": "EMPROCESSAMENTO",
        "meuIdentificador": corpo["meuIdentificador"],
        "qtdePagamentos": corpo["pagamentos"].as_array().map_or(0, Vec::len),
    });
    estado.lotes.push((corpo, None));
    ResponseTemplate::new(200).set_body_json(resposta)
}

/// A batch, processed at the first look: each payment is made, but a
/// boleto the account had already paid.
fn consultar_lote(estado: &mut Estado, request: &Request) -> ResponseTemplate {
    let id = request.url.path().rsplit('/').next().unwrap_or_default();
    let Some(posicao) = LOTES
        .iter()
        .take(estado.lotes.len())
        .position(|lote| *lote == id)
    else {
        return problema(
            404,
            "Lote não encontrado",
            "Não há lote com este identificador.",
        );
    };
    if let Some(feito) = &estado.lotes[posicao].1 {
        return ResponseTemplate::new(200).set_body_json(feito);
    }
    let corpo = estado.lotes[posicao].0.clone();
    let pagamentos: Vec<Value> = corpo["pagamentos"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(seq, item)| {
            let seq = (seq + 1).to_string();
            if item["tipoPagamento"] == "DARF" {
                darf_do_lote(estado, item, &seq)
            } else {
                boleto_do_lote(estado, item, &seq)
            }
        })
        .collect();
    let com_erro = pagamentos
        .iter()
        .any(|pagamento| pagamento["status"] == "ERRO");
    let feito = json!({
        "idLote": id,
        "status": if com_erro { "PROCESSADOCOMERRO" } else { "PROCESSADOSEMERRO" },
        "meuIdentificador": corpo["meuIdentificador"],
        "qtdePagamentos": pagamentos.len(),
        "dataCriacao": format!("{HOJE}T10:15:00"),
        "pagamentos": pagamentos,
    });
    estado.lotes[posicao].1 = Some(feito.clone());
    ResponseTemplate::new(200).set_body_json(feito)
}

/// A DARF of a batch, paid.
fn darf_do_lote(estado: &mut Estado, item: &Value, seq: &str) -> Value {
    let codigo = SOLICITACOES_DARF[estado.darfs.len()];
    let pago = darf_pago(item, codigo);
    let resposta = json!({
        "tipoPagamento": "DARF",
        "seqId": seq,
        "codigoSolicitacao": codigo,
        "status": "PAGO",
        "tipoDarf": "PRETO",
        "cnpjCpf": item["cnpjCpf"],
        "nomeEmpresa": item["nomeEmpresa"],
        "descricao": item["descricao"],
        "referencia": item["referencia"],
        "codigoReceita": item["codigoReceita"],
        "valor": pago["valor"],
        "valorMulta": pago["valorMulta"],
        "valorJuros": pago["valorJuros"],
        "valorTotal": pago["valorTotal"],
        "periodoApuracao": item["periodoApuracao"],
        "dataVencimento": item["dataVencimento"],
        "dataPagamento": HOJE,
    });
    estado.darfs.push(pago);
    resposta
}

/// A boleto or bill of a batch: paid, but a boleto already paid.
fn boleto_do_lote(estado: &mut Estado, item: &Value, seq: &str) -> Value {
    let codigo = item["codBarraLinhaDigitavel"].as_str().unwrap_or_default();
    let mut boleto = json!({
        "tipoPagamento": "BOLETO",
        "seqId": seq,
        "codBarraLinhaDigitavel": codigo,
        "valorPagar": numero(decimal(&item["valorPagar"])),
        "dataVencimento": item["dataVencimento"],
        "dataPagamento": HOJE,
    });
    let ja_pago = !codigo.starts_with('8')
        && estado
            .feitos
            .iter()
            .any(|feito| feito["codigoBarra"] == codigo && feito["statusPagamento"] == "REALIZADO");
    if ja_pago {
        boleto["status"] = json!("ERRO");
        boleto["detalhe"] = json!("Pagamento já realizado para este código de barras.");
        return boleto;
    }
    let transacao = CODIGOS[estado.feitos.len() - 2];
    let nome = BENEFICIARIOS
        .iter()
        .find(|(de, ..)| *de == codigo)
        .map_or("Beneficiário", |(_, nome, _)| nome);
    estado.feitos.push(json!({
        "codigoTransacao": transacao,
        "codigoBarra": codigo,
        "tipo": if codigo.starts_with('8') { "CONVENIO" } else { "BOLETO" },
        "dataVencimentoDigitada": item["dataVencimento"],
        "dataVencimentoTitulo": item["dataVencimento"],
        "dataInclusao": format!("{HOJE}T10:15:00"),
        "dataPagamento": HOJE,
        "valorPago": item["valorPagar"],
        "valorNominal": item["valorPagar"],
        "statusPagamento": "REALIZADO",
        "nomeBeneficiario": nome,
        "aprovacoesNecessarias": 0,
        "aprovacoesRealizadas": 0,
    }));
    boleto["codigoTransacao"] = json!(transacao);
    boleto["status"] = json!("REALIZADO");
    boleto
}

fn incluir(estado: &mut Estado, request: &Request) -> ResponseTemplate {
    let corpo: Value = serde_json::from_slice(&request.body).unwrap();
    let codigo = corpo["codBarraLinhaDigitavel"].as_str().unwrap_or_default();
    let Some((_, nome, documento)) = BENEFICIARIOS.iter().find(|(de, ..)| *de == codigo) else {
        return problema(
            400,
            "Código de barras inválido",
            "O banco emissor não encontrou um título com este código.",
        );
    };
    if let Some(informado) = corpo["cpfCnpjBeneficiario"].as_str()
        && Some(informado) != *documento
    {
        return problema(
            400,
            "Beneficiário não confere",
            "O CPF/CNPJ informado não é o do beneficiário do título.",
        );
    }
    let valor = decimal(&corpo["valorPagar"]);
    let agendado = corpo["dataPagamento"].as_str();
    let status = match agendado {
        Some(_) => "AGENDADO",
        None if valor > Decimal::from(APROVACAO_ACIMA_DE) => "AGUARDANDO_APROVACAO",
        None => "REALIZADO",
    };
    let transacao = CODIGOS[estado.feitos.len() - 2];
    estado.feitos.push(json!({
        "codigoTransacao": transacao,
        "codigoBarra": codigo,
        "tipo": if codigo.starts_with('8') { "CONVENIO" } else { "BOLETO" },
        "dataVencimentoDigitada": corpo["dataVencimento"],
        "dataVencimentoTitulo": corpo["dataVencimento"],
        "dataInclusao": format!("{HOJE}T09:00:00"),
        "dataPagamento": agendado.unwrap_or(HOJE),
        "valorPago": format!("{valor:.2}"),
        "valorNominal": format!("{valor:.2}"),
        "statusPagamento": status,
        "nomeBeneficiario": nome,
        "cpfCnpjBeneficiario": documento,
        "aprovacoesNecessarias": u8::from(status == "AGUARDANDO_APROVACAO"),
        "aprovacoesRealizadas": 0,
    }));
    if codigo == RESPOSTA_PERDIDA {
        return ResponseTemplate::new(504);
    }
    let mut resposta = json!({
        "codigoTransacao": transacao,
        "statusPagamento": status,
        "quantidadeAprovadores": u8::from(status == "AGUARDANDO_APROVACAO"),
    });
    if let Some(dia) = agendado {
        resposta["dataAgendamento"] = json!(dia);
    }
    ResponseTemplate::new(200).set_body_json(resposta)
}

/// The payments of the period (by inclusion, payment or due date), or of a
/// code or transaction.
fn buscar(estado: &Estado, request: &Request) -> ResponseTemplate {
    let parametros = parametros(request);
    let campo = match parametros.get("filtrarDataPor").map(String::as_str) {
        Some("PAGAMENTO") => "dataPagamento",
        Some("VENCIMENTO") => "dataVencimentoTitulo",
        _ => "dataInclusao",
    };
    let achados: Vec<&Value> = estado
        .feitos
        .iter()
        .filter(|pagamento| {
            let data = pagamento[campo]
                .as_str()
                .unwrap_or_default()
                .get(..10)
                .unwrap_or_default();
            parametros
                .get("dataInicio")
                .is_none_or(|inicio| inicio.as_str() <= data)
                && parametros
                    .get("dataFim")
                    .is_none_or(|fim| data <= fim.as_str())
                && parametros
                    .get("codBarraLinhaDigitavel")
                    .is_none_or(|codigo| pagamento["codigoBarra"] == codigo.as_str())
                && parametros
                    .get("codigoTransacao")
                    .is_none_or(|codigo| pagamento["codigoTransacao"] == codigo.as_str())
        })
        .collect();
    ResponseTemplate::new(200).set_body_json(achados)
}

/// Only a scheduled payment can be cancelled.
fn cancelar(estado: &mut Estado, request: &Request) -> ResponseTemplate {
    let codigo = request.url.path().rsplit('/').next().unwrap_or_default();
    let Some(pagamento) = estado
        .feitos
        .iter_mut()
        .find(|pagamento| pagamento["codigoTransacao"] == codigo)
    else {
        return problema(
            404,
            "Pagamento não encontrado",
            "Não há pagamento com este código.",
        );
    };
    if pagamento["statusPagamento"] != "AGENDADO" {
        return problema(
            422,
            "Pagamento não pode ser cancelado",
            "Só um pagamento agendado pode ser cancelado.",
        );
    }
    pagamento["statusPagamento"] = json!("AGENDADO_CANCELADO");
    ResponseTemplate::new(204)
}
