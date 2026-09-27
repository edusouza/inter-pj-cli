//! The charges of the Cobrança API (`/cobranca/v3/cobrancas`): boletos with
//! Pix that the company issues to its clients. The account already has the
//! charges of the months of the statement: the one Beltrana de Tal paid in
//! August, which is in the statement, one that expired unpaid, one overdue
//! and one to receive. A charge the guides issue is in processing when
//! asked for, and issued, with its boleto and its Pix, by the next request;
//! its codes are those of [`CODIGOS`], by its seu número. The answer to the
//! issue of one charge gets lost, and the same charge is refused again, as
//! the API does for 30 minutes. A change is done by the first look at it,
//! though the charge keeps its values, as the API takes up to 30 minutes to
//! show them; a charge is cancelled, or paid in the sandbox, at once.

use std::sync::{Arc, Mutex};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use rust_decimal::Decimal;
use serde_json::{Value, json};
use wiremock::matchers::path_regex;
use wiremock::{MockServer, Request, ResponseTemplate};

use super::{conta, parametros, problema, requisicao};
use crate::sessao::HOJE;

/// The codes of a charge: of the request, of its boleto and of its Pix,
/// generated with the rules of FEBRABAN and of the BR Code.
struct Codigos {
    seu_numero: &'static str,
    codigo: &'static str,
    nosso_numero: &'static str,
    barras: &'static str,
    linha: &'static str,
    txid: &'static str,
    copia_e_cola: &'static str,
}

/// The charge whose issue is made but whose answer gets lost.
const RESPOSTA_PERDIDA: &str = "NF-0927";

/// The codes of the changes asked for, in order.
const EDICOES: [&str; 2] = [
    "3c5e7a9b-1d2f-4a6c-8e0b-2d4f6a8c0e19",
    "4d6f8b0c-2e3a-4b7d-9f1c-3e5a7b9d1f2a",
];

const CODIGOS: [Codigos; 8] = [
    Codigos {
        seu_numero: "NF-0805",
        codigo: "5c2e8a41-7d3b-4f6e-9a1c-2b4d6f8e0a13",
        nosso_numero: "0012345667",
        barras: "07796154400000300000001112001234566700000000",
        linha: "07790001161200123456167000000009615440000030000",
        txid: "cobv0805empresaexemplo20260728",
        copia_e_cola: "00020101021226810014br.gov.bcb.pix2559qrcodepix.inter.example/cobv/cobv0805empresaexemplo202607285204000053039865802BR5920EMPRESA EXEMPLO LTDA6014BELO HORIZONTE62070503***6304FD4B",
    },
    Codigos {
        seu_numero: "NF-0815",
        codigo: "8e1f3a5c-7b9d-4e2f-8a4c-6e8f0a2c4e61",
        nosso_numero: "0012345678",
        barras: "07796153400000890000001112001234567800000000",
        linha: "07790001161200123456178000000006615340000089000",
        txid: "cobv0815empresaexemplo20260731",
        copia_e_cola: "00020101021226810014br.gov.bcb.pix2559qrcodepix.inter.example/cobv/cobv0815empresaexemplo202607315204000053039865802BR5920EMPRESA EXEMPLO LTDA6014BELO HORIZONTE62070503***63046447",
    },
    Codigos {
        seu_numero: "NF-0830",
        codigo: "2a4c6e8f-0b2d-4f6a-8c0e-4a6c8e0f2b43",
        nosso_numero: "0012345689",
        barras: "07795157000000450000001112001234568900000000",
        linha: "07790001161200123456189000000003515700000045000",
        txid: "cobv0830empresaexemplo20260830",
        copia_e_cola: "00020101021226810014br.gov.bcb.pix2559qrcodepix.inter.example/cobv/cobv0830empresaexemplo202608305204000053039865802BR5920EMPRESA EXEMPLO LTDA6014BELO HORIZONTE62070503***63045CE6",
    },
    Codigos {
        seu_numero: "NF-0910",
        codigo: "9d7b5f3e-1c0a-4e8f-9b7d-5f3e1c0a8e25",
        nosso_numero: "0012345690",
        barras: "07797159500002350000001112001234569000000000",
        linha: "07790001161200123456190000000001715950000235000",
        txid: "cobv0910empresaexemplo20260910",
        copia_e_cola: "00020101021226810014br.gov.bcb.pix2559qrcodepix.inter.example/cobv/cobv0910empresaexemplo202609105204000053039865802BR5920EMPRESA EXEMPLO LTDA6014BELO HORIZONTE62070503***63043034",
    },
    Codigos {
        seu_numero: "NF-0924",
        codigo: "0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d",
        nosso_numero: "0012345701",
        barras: "07791160900001200000001112001234570100000000",
        linha: "07790001161200123457901000000008116090000120000",
        txid: "cobv0924empresaexemplo20260924",
        copia_e_cola: "00020101021226810014br.gov.bcb.pix2559qrcodepix.inter.example/cobv/cobv0924empresaexemplo202609245204000053039865802BR5920EMPRESA EXEMPLO LTDA6014BELO HORIZONTE62070503***6304E4FE",
    },
    Codigos {
        seu_numero: "NF-0925",
        codigo: "6f4d2b0e-8c6a-4e4f-9d2b-0e8c6a4f2d17",
        nosso_numero: "0012345712",
        barras: "07797159400000890000001112001234571200000000",
        linha: "07790001161200123457912000000005715940000089000",
        txid: "cobv0925empresaexemplo20260924",
        copia_e_cola: "00020101021226810014br.gov.bcb.pix2559qrcodepix.inter.example/cobv/cobv0925empresaexemplo202609245204000053039865802BR5920EMPRESA EXEMPLO LTDA6014BELO HORIZONTE62070503***6304825F",
    },
    Codigos {
        seu_numero: "NF-0927",
        codigo: "7a9c1e3f-5b7d-4f9a-8c2e-4f6a8c0e2b58",
        nosso_numero: "0012345723",
        barras: "07799161200000640000001112001234572300000000",
        linha: "07790001161200123457923000000002916120000064000",
        txid: "cobv0927empresaexemplo20260924",
        copia_e_cola: "00020101021226810014br.gov.bcb.pix2559qrcodepix.inter.example/cobv/cobv0927empresaexemplo202609245204000053039865802BR5920EMPRESA EXEMPLO LTDA6014BELO HORIZONTE62070503***63044F1D",
    },
    Codigos {
        seu_numero: "TESTE-1",
        codigo: "1e3a5c7d-9f1b-4d3e-8a5c-7e9b1d3f5a74",
        nosso_numero: "0012345734",
        barras: "07793158500000010000001112001234573400000000",
        linha: "07790001161200123457934000000009315850000001000",
        txid: "cobvteste1empresaexemplo20260924",
        copia_e_cola: "00020101021226830014br.gov.bcb.pix2561qrcodepix.inter.example/cobv/cobvteste1empresaexemplo202609245204000053039865802BR5920EMPRESA EXEMPLO LTDA6014BELO HORIZONTE62070503***6304A4D7",
    },
];

/// The order of the situations in the summary.
const SITUACOES: [&str; 9] = [
    "A_RECEBER",
    "ATRASADO",
    "RECEBIDO",
    "MARCADO_RECEBIDO",
    "EM_PROCESSAMENTO",
    "CANCELADO",
    "EXPIRADO",
    "FALHA_EMISSAO",
    "PROTESTO",
];

#[derive(Default)]
struct Estado {
    /// Each charge as the API shows it (`cobranca`, `boleto`, `pix`).
    cobrancas: Vec<Value>,
    /// How many changes were asked for.
    edicoes: usize,
}

impl Estado {
    /// The charges issued before the guides.
    fn novo() -> Self {
        let fulano = json!({
            "cpfCnpj": "12345678909",
            "tipoPessoa": "FISICA",
            "nome": "Fulano de Tal",
            "endereco": "Rua da Bahia",
            "numero": "1000",
            "cidade": "Belo Horizonte",
            "uf": "MG",
            "cep": "30160011",
        });
        let beltrana = json!({
            "cpfCnpj": "01234567890",
            "tipoPessoa": "FISICA",
            "nome": "Beltrana de Tal",
            "endereco": "Rua dos Timbiras",
            "numero": "45",
            "cidade": "Belo Horizonte",
            "uf": "MG",
            "cep": "30140060",
            "email": "beltrana@cliente.example",
        });
        let cliente = json!({
            "cpfCnpj": "11222333000181",
            "tipoPessoa": "JURIDICA",
            "nome": "Cliente Exemplo Ltda",
            "endereco": "Avenida Brasil",
            "numero": "1200",
            "complemento": "sala 3",
            "bairro": "Centro",
            "cidade": "Belo Horizonte",
            "uf": "MG",
            "cep": "30110000",
            "email": "financeiro@cliente.example",
        });
        let multa = json!({"codigo": "PERCENTUAL", "taxa": 2});
        let mora = json!({"codigo": "TAXAMENSAL", "taxa": 1});
        let cobrancas = vec![
            json!({
                "dataEmissao": "2026-07-28",
                "dataVencimento": "2026-08-20",
                "valorNominal": 300,
                "situacao": "EXPIRADO",
                "dataSituacao": "2026-08-21",
                "pagador": fulano,
            }),
            json!({
                "dataEmissao": "2026-07-31",
                "dataVencimento": "2026-08-10",
                "valorNominal": 890,
                "situacao": "RECEBIDO",
                "dataSituacao": "2026-08-10",
                "valorTotalRecebido": 890,
                "origemRecebimento": "BOLETO",
                "multa": multa,
                "mora": mora,
                "pagador": beltrana,
            }),
            json!({
                "dataEmissao": "2026-08-30",
                "dataVencimento": "2026-09-15",
                "valorNominal": 450,
                "situacao": "ATRASADO",
                "dataSituacao": "2026-09-16",
                "multa": multa,
                "mora": mora,
                "pagador": fulano,
            }),
            json!({
                "dataEmissao": "2026-09-10",
                "dataVencimento": "2026-10-10",
                "valorNominal": 2350,
                "situacao": "A_RECEBER",
                "dataSituacao": "2026-09-10",
                "descontos": [
                    {"codigo": "PERCENTUALDATAINFORMADA", "taxa": 2, "quantidadeDias": 5}
                ],
                "multa": multa,
                "mora": mora,
                "pagador": cliente,
            }),
        ];
        Self {
            cobrancas: cobrancas
                .into_iter()
                .zip(&CODIGOS)
                .map(|(cobranca, codigos)| emitida(codigos, cobranca))
                .collect(),
            edicoes: 0,
        }
    }

    /// The charges in processing are issued by the next request.
    fn processar(&mut self) {
        for cobranca in &mut self.cobrancas {
            if cobranca["cobranca"]["situacao"] == "EM_PROCESSAMENTO" {
                let seu_numero = cobranca["cobranca"]["seuNumero"]
                    .as_str()
                    .unwrap_or_default();
                let codigos = CODIGOS
                    .iter()
                    .find(|codigos| codigos.seu_numero == seu_numero)
                    .unwrap();
                let mut dados = cobranca["cobranca"].clone();
                dados["situacao"] = json!("A_RECEBER");
                let formas = cobranca["formasRecebimento"].clone();
                *cobranca = emitida(codigos, dados);
                if formas
                    .as_array()
                    .is_some_and(|formas| !formas.contains(&json!("PIX")))
                {
                    cobranca.as_object_mut().unwrap().remove("pix");
                }
            }
        }
    }

    fn achar(&self, codigo: &str) -> Option<&Value> {
        self.cobrancas
            .iter()
            .find(|cobranca| cobranca["cobranca"]["codigoSolicitacao"] == codigo)
    }

    /// The charge of the path (`/cobranca/v3/cobrancas/{codigo}/...`), to
    /// change: one that is paid, cancelled or expired cannot be.
    fn alteravel(&mut self, request: &Request) -> Result<&mut Value, Recusa> {
        let codigo = request.url.path().split('/').nth(4).unwrap_or_default();
        let cobranca = self
            .cobrancas
            .iter_mut()
            .find(|cobranca| cobranca["cobranca"]["codigoSolicitacao"] == codigo)
            .ok_or(Recusa::NaoEncontrada)?;
        let situacao = &cobranca["cobranca"]["situacao"];
        if ["RECEBIDO", "MARCADO_RECEBIDO", "CANCELADO", "EXPIRADO"]
            .iter()
            .any(|encerrada| situacao == encerrada)
        {
            return Err(Recusa::Encerrada);
        }
        Ok(&mut cobranca["cobranca"])
    }
}

/// Why a charge cannot be changed.
#[derive(Debug, Clone, Copy)]
enum Recusa {
    NaoEncontrada,
    Encerrada,
}

impl Recusa {
    fn resposta(self) -> ResponseTemplate {
        match self {
            Self::NaoEncontrada => nao_encontrada(),
            Self::Encerrada => problema(
                400,
                "Cobrança não pode ser alterada",
                "A cobrança já foi paga, cancelada ou expirou.",
            ),
        }
    }
}

/// A charge issued, with its boleto and its Pix.
fn emitida(codigos: &Codigos, mut cobranca: Value) -> Value {
    cobranca["codigoSolicitacao"] = json!(codigos.codigo);
    cobranca["seuNumero"] = json!(codigos.seu_numero);
    cobranca["tipoCobranca"] = json!("SIMPLES");
    cobranca["arquivada"] = json!(false);
    if cobranca.get("descontos").is_none() {
        cobranca["descontos"] = json!([]);
    }
    json!({
        "cobranca": cobranca,
        "boleto": {
            "nossoNumero": codigos.nosso_numero,
            "codigoBarras": codigos.barras,
            "linhaDigitavel": codigos.linha,
        },
        "pix": {"txid": codigos.txid, "pixCopiaECola": codigos.copia_e_cola},
    })
}

/// What answers a request, with the charges.
type Rota = fn(&mut Estado, &Request) -> ResponseTemplate;

pub(super) async fn montar(servidor: &MockServer) {
    let estado = Arc::new(Mutex::new(Estado::novo()));
    let rotas: [(&str, &str, Rota); 9] = [
        ("POST", r"^/cobranca/v3/cobrancas$", emitir),
        ("GET", r"^/cobranca/v3/cobrancas$", |estado, request| {
            listar(estado, request)
        }),
        (
            "GET",
            r"^/cobranca/v3/cobrancas/sumario$",
            |estado, request| sumario(estado, request),
        ),
        ("GET", r"^/cobranca/v3/cobrancas/[^/]+/pdf$", pdf),
        ("PATCH", r"^/cobranca/v3/cobrancas/[0-9a-f-]+$", editar),
        ("GET", r"^/cobranca/v3/cobrancas/edicao/[^/]+$", edicao),
        ("POST", r"^/cobranca/v3/cobrancas/[^/]+/cancelar$", cancelar),
        ("POST", r"^/cobranca/v3/cobrancas/[^/]+/pagar$", pagar),
        ("GET", r"^/cobranca/v3/cobrancas/[0-9a-f-]+$", consultar),
    ];
    for (metodo, caminho, rota) in rotas {
        let estado = Arc::clone(&estado);
        requisicao(metodo, path_regex(caminho))
            .respond_with(move |request: &Request| {
                let mut estado = estado.lock().unwrap();
                estado.processar();
                rota(&mut estado, request)
            })
            .mount(servidor)
            .await;
    }
}

fn consultar(estado: &mut Estado, request: &Request) -> ResponseTemplate {
    let codigo = request.url.path().rsplit('/').next().unwrap_or_default();
    match estado.achar(codigo) {
        Some(cobranca) => ResponseTemplate::new(200).set_body_json(cobranca),
        None => nao_encontrada(),
    }
}

/// The boleto in PDF, with the seu número in its title.
fn pdf(estado: &mut Estado, request: &Request) -> ResponseTemplate {
    let codigo = request.url.path().split('/').nth(4).unwrap_or_default();
    let Some(cobranca) = estado.achar(codigo) else {
        return nao_encontrada();
    };
    let titulo = format!(
        "Boleto {} - Empresa Exemplo Ltda",
        cobranca["cobranca"]["seuNumero"]
            .as_str()
            .unwrap_or_default()
    );
    ResponseTemplate::new(200).set_body_json(json!({ "pdf": BASE64.encode(conta::pdf(&titulo)) }))
}

/// A change of the value or of the due date: in processing, and done by
/// the first look at it.
fn editar(estado: &mut Estado, request: &Request) -> ResponseTemplate {
    if let Err(recusa) = estado.alteravel(request) {
        return recusa.resposta();
    }
    let codigo = EDICOES[estado.edicoes];
    estado.edicoes += 1;
    ResponseTemplate::new(200)
        .set_body_json(json!({ "codigoEdicao": codigo, "status": "PROCESSANDO" }))
}

fn edicao(estado: &mut Estado, request: &Request) -> ResponseTemplate {
    let codigo = request.url.path().rsplit('/').next().unwrap_or_default();
    if EDICOES[..estado.edicoes].contains(&codigo) {
        ResponseTemplate::new(200).set_body_json(json!({ "status": "SUCESSO" }))
    } else {
        problema(
            404,
            "Alteração não encontrada",
            "Não há alteração com este código.",
        )
    }
}

fn cancelar(estado: &mut Estado, request: &Request) -> ResponseTemplate {
    let corpo: Value = serde_json::from_slice(&request.body).unwrap();
    match estado.alteravel(request) {
        Ok(cobranca) => {
            cobranca["situacao"] = json!("CANCELADO");
            cobranca["dataSituacao"] = json!(HOJE);
            cobranca["motivoCancelamento"] = corpo["motivoCancelamento"].clone();
            ResponseTemplate::new(202)
        }
        Err(recusa) => recusa.resposta(),
    }
}

/// A charge paid in the sandbox, with the boleto or the Pix.
fn pagar(estado: &mut Estado, request: &Request) -> ResponseTemplate {
    let corpo: Value = serde_json::from_slice(&request.body).unwrap();
    match estado.alteravel(request) {
        Ok(cobranca) => {
            cobranca["situacao"] = json!("RECEBIDO");
            cobranca["dataSituacao"] = json!(HOJE);
            cobranca["valorTotalRecebido"] = cobranca["valorNominal"].clone();
            cobranca["origemRecebimento"] = corpo["pagarCom"].clone();
            ResponseTemplate::new(204)
        }
        Err(recusa) => recusa.resposta(),
    }
}

fn nao_encontrada() -> ResponseTemplate {
    problema(
        404,
        "Cobrança não encontrada",
        "Não há cobrança com este código de solicitação.",
    )
}

/// A charge asked for, in processing until the next request. The codes are
/// those of its seu número.
fn emitir(estado: &mut Estado, request: &Request) -> ResponseTemplate {
    let corpo: Value = serde_json::from_slice(&request.body).unwrap();
    let seu_numero = corpo["seuNumero"].as_str().unwrap_or_default();
    let Some(codigos) = CODIGOS
        .iter()
        .find(|codigos| codigos.seu_numero == seu_numero)
    else {
        return problema(
            400,
            "Seu número sem código na simulação",
            "Esta simulação só emite as cobranças dos guias.",
        );
    };
    if estado.achar(codigos.codigo).is_some() {
        return problema(
            409,
            "Cobrança duplicada",
            "Já existe uma cobrança com este seu número, valor, vencimento e pagador.",
        );
    }
    let mut cobranca = json!({
        "codigoSolicitacao": codigos.codigo,
        "seuNumero": seu_numero,
        "dataEmissao": HOJE,
        "dataVencimento": corpo["dataVencimento"],
        "valorNominal": corpo["valorNominal"],
        "tipoCobranca": "SIMPLES",
        "situacao": "EM_PROCESSAMENTO",
        "dataSituacao": HOJE,
        "arquivada": false,
        "descontos": corpo.get("desconto").map_or_else(|| json!([]), |desconto| json!([desconto])),
        "pagador": corpo["pagador"],
    });
    for campo in ["multa", "mora"] {
        if let Some(encargo) = corpo.get(campo) {
            cobranca[campo] = encargo.clone();
        }
    }
    estado.cobrancas.push(json!({
        "cobranca": cobranca,
        "formasRecebimento": corpo["formasRecebimento"],
    }));
    if seu_numero == RESPOSTA_PERDIDA {
        return ResponseTemplate::new(504);
    }
    ResponseTemplate::new(200).set_body_json(json!({ "codigoSolicitacao": codigos.codigo }))
}

/// The charges of the period (by due date, issue or payment) and of the
/// filters, in the order asked for; none without the period.
fn filtradas<'a>(estado: &'a Estado, request: &Request) -> Option<Vec<&'a Value>> {
    let parametros = parametros(request);
    let (Some(inicio), Some(fim)) = (parametros.get("dataInicial"), parametros.get("dataFinal"))
    else {
        return None;
    };
    let pagas = parametros.get("filtrarDataPor").map(String::as_str) == Some("PAGAMENTO");
    let campo = match parametros.get("filtrarDataPor").map(String::as_str) {
        Some("EMISSAO") => "dataEmissao",
        Some("PAGAMENTO") => "dataSituacao",
        _ => "dataVencimento",
    };
    let igual = |nome: &str, valor: &Value| {
        parametros
            .get(nome)
            .is_none_or(|filtro| valor == filtro.as_str())
    };
    let mut cobrancas: Vec<&Value> = estado
        .cobrancas
        .iter()
        .filter(|item| {
            let cobranca = &item["cobranca"];
            let data = cobranca[campo].as_str().unwrap_or_default();
            let nome = cobranca["pagador"]["nome"].as_str().unwrap_or_default();
            inicio.as_str() <= data
                && data <= fim.as_str()
                && (!pagas || cobranca["situacao"] == "RECEBIDO")
                && igual("situacao", &cobranca["situacao"])
                && igual("cpfCnpjPessoaPagadora", &cobranca["pagador"]["cpfCnpj"])
                && igual("seuNumero", &cobranca["seuNumero"])
                && igual("tipoCobranca", &cobranca["tipoCobranca"])
                && parametros
                    .get("pessoaPagadora")
                    .is_none_or(|pagador| nome.to_lowercase().contains(&pagador.to_lowercase()))
        })
        .collect();
    let chave = |item: &Value| -> String {
        let cobranca = &item["cobranca"];
        let texto = |campo: &str| cobranca[campo].as_str().unwrap_or_default().to_owned();
        match parametros.get("ordenarPor").map(String::as_str) {
            Some("PESSOA_PAGADORA") => cobranca["pagador"]["nome"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            Some("DATA_EMISSAO") => texto("dataEmissao"),
            Some("VALOR") => format!("{:015.2}", decimal(&cobranca["valorNominal"])),
            Some("STATUS") => texto("situacao"),
            Some("IDENTIFICADOR") => texto("seuNumero"),
            Some("CODIGO_COBRANCA") => texto("codigoSolicitacao"),
            Some("TIPO_COBRANCA") => texto("tipoCobranca"),
            _ => texto("dataVencimento"),
        }
    };
    cobrancas.sort_by_key(|item| chave(item));
    if parametros.get("tipoOrdenacao").map(String::as_str) == Some("DESC") {
        cobrancas.reverse();
    }
    Some(cobrancas)
}

fn sem_periodo() -> ResponseTemplate {
    problema(
        400,
        "Período obrigatório",
        "dataInicial e dataFinal são obrigatórios.",
    )
}

/// An amount sent as a number or as text.
fn decimal(valor: &Value) -> Decimal {
    valor
        .as_str()
        .map_or_else(|| valor.to_string(), str::to_owned)
        .parse()
        .unwrap_or_default()
}

/// A page of the charges, from 0.
fn listar(estado: &Estado, request: &Request) -> ResponseTemplate {
    let Some(cobrancas) = filtradas(estado, request) else {
        return sem_periodo();
    };
    let parametros = parametros(request);
    let pagina: usize = parametros
        .get("paginacao.paginaAtual")
        .map_or(0, |pagina| pagina.parse().unwrap());
    let itens: usize = parametros
        .get("paginacao.itensPorPagina")
        .map_or(100, |itens| itens.parse().unwrap());
    let paginas = cobrancas.len().div_ceil(itens).max(1);
    let nesta: Vec<&Value> = cobrancas
        .iter()
        .skip(pagina * itens)
        .take(itens)
        .copied()
        .collect();
    ResponseTemplate::new(200).set_body_json(json!({
        "totalPaginas": paginas,
        "totalElementos": cobrancas.len(),
        "ultimaPagina": pagina + 1 >= paginas,
        "primeiraPagina": pagina == 0,
        "tamanhoPagina": itens,
        "numeroDeElementos": nesta.len(),
        "cobrancas": nesta,
    }))
}

/// How many charges of the period are in each situation, and how much.
fn sumario(estado: &Estado, request: &Request) -> ResponseTemplate {
    let Some(cobrancas) = filtradas(estado, request) else {
        return sem_periodo();
    };
    let itens: Vec<Value> = SITUACOES
        .iter()
        .filter_map(|situacao| {
            let destas: Vec<&&Value> = cobrancas
                .iter()
                .filter(|item| item["cobranca"]["situacao"] == *situacao)
                .collect();
            let valor: Decimal = destas
                .iter()
                .map(|item| decimal(&item["cobranca"]["valorNominal"]))
                .sum();
            (!destas.is_empty()).then(|| {
                json!({
                    "situacao": situacao,
                    "valor": valor.to_string().parse::<f64>().unwrap(),
                    "quantidade": destas.len(),
                })
            })
        })
        .collect();
    ResponseTemplate::new(200).set_body_json(itens)
}
