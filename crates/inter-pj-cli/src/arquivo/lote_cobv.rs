//! The files of `pix lote-cobv criar|revisar`: JSON as the API's body (or
//! just the list of charges), or CSV with one charge per line and the paths
//! of the API's fields as columns (`valor.multa.valorPerc`).
//!
//! Every charge is checked, and every problem reported at once with its
//! line (CSV) or position (JSON), so that a spreadsheet is fixed in one go.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;

use chrono::{Days, NaiveDate};
use inter_pj::pix::{CobvDoLote, CobvRevisadaDoLote, Txid, TxidError};
use serde_json::{Map, Value};

use super::cobv::{CAMPOS_COBV, CAMPOS_REVISAO_COBV, ler_cobv, ler_revisao_cobv};
use super::{Campos, csv, ler, notacao_cientifica};
use crate::error::CliError;

/// Most problems listed at once.
const MAX_ERROS: usize = 20;

/// Fields of a charge of a batch: its txid and the charge.
const CAMPOS_ITEM: [&str; 8] = [
    "txid",
    CAMPOS_COBV[0],
    CAMPOS_COBV[1],
    CAMPOS_COBV[2],
    CAMPOS_COBV[3],
    CAMPOS_COBV[4],
    CAMPOS_COBV[5],
    CAMPOS_COBV[6],
];

/// Fields of a change to a charge of a batch: its txid and the change.
const CAMPOS_ITEM_REVISAO: [&str; 9] = [
    "txid",
    CAMPOS_REVISAO_COBV[0],
    CAMPOS_REVISAO_COBV[1],
    CAMPOS_REVISAO_COBV[2],
    CAMPOS_REVISAO_COBV[3],
    CAMPOS_REVISAO_COBV[4],
    CAMPOS_REVISAO_COBV[5],
    CAMPOS_REVISAO_COBV[6],
    CAMPOS_REVISAO_COBV[7],
];

/// Columns of the CSV file: the paths of the API's fields. The additional
/// information is only in JSON; `status` only removes, in a revision.
const COLUNAS: [&str; 30] = [
    "txid",
    "calendario.dataDeVencimento",
    "calendario.validadeAposVencimento",
    "devedor.cpf",
    "devedor.cnpj",
    "devedor.nome",
    "devedor.email",
    "devedor.logradouro",
    "devedor.cidade",
    "devedor.uf",
    "devedor.cep",
    "valor.original",
    "valor.multa.modalidade",
    "valor.multa.valorPerc",
    "valor.juros.modalidade",
    "valor.juros.valorPerc",
    "valor.abatimento.modalidade",
    "valor.abatimento.valorPerc",
    "valor.desconto.modalidade",
    "valor.desconto.valorPerc",
    "valor.desconto.descontoDataFixa[0].data",
    "valor.desconto.descontoDataFixa[0].valorPerc",
    "valor.desconto.descontoDataFixa[1].data",
    "valor.desconto.descontoDataFixa[1].valorPerc",
    "valor.desconto.descontoDataFixa[2].data",
    "valor.desconto.descontoDataFixa[2].valorPerc",
    "chave",
    "solicitacaoPagador",
    "loc.id",
    "status",
];

/// What a batch file holds.
#[derive(Debug)]
pub(crate) struct ArquivoLoteCobv<T> {
    /// `descricao` of a JSON file.
    pub(crate) descricao: Option<String>,
    /// Each charge, and where it is in the file (`linha 3`, `cobrança 2`).
    pub(crate) cobsv: Vec<(String, T)>,
}

/// Reads the charges of a new batch (or the standard input for `-`): JSON
/// when it starts with `{` or `[`, CSV otherwise. Due dates before `hoje`
/// are refused.
///
/// # Errors
///
/// [`CliError::Usage`] listing every charge with a problem; nothing is
/// returned when any is invalid.
pub(crate) fn ler_lote_cobv(
    caminho: &Path,
    hoje: NaiveDate,
) -> Result<ArquivoLoteCobv<CobvDoLote>, CliError> {
    ler_arquivo(
        caminho,
        |valor, onde| cobv_do_lote(valor, onde, hoje),
        |item| &item.txid,
    )
}

/// Reads the changes to the charges of a batch, as [`ler_lote_cobv`].
///
/// # Errors
///
/// [`CliError::Usage`] listing every change with a problem.
pub(crate) fn ler_revisao_lote_cobv(
    caminho: &Path,
    hoje: NaiveDate,
) -> Result<ArquivoLoteCobv<CobvRevisadaDoLote>, CliError> {
    ler_arquivo(
        caminho,
        |valor, onde| revisao_do_lote(valor, onde, hoje),
        |item| &item.txid,
    )
}

fn ler_arquivo<T>(
    caminho: &Path,
    item_de: impl Fn(&Value, &str) -> Result<T, CliError>,
    txid: fn(&T) -> &Txid,
) -> Result<ArquivoLoteCobv<T>, CliError> {
    let texto = ler(caminho)?;
    let nome = super::nome(caminho);
    let (descricao, lidos) = if texto.trim_start().starts_with(['{', '[']) {
        json(&nome, &texto, &item_de)?
    } else {
        (None, csv(&nome, &texto, &item_de)?)
    };
    juntar(&nome, descricao, lidos, txid)
}

type Lidos<T> = Vec<(String, Result<T, CliError>)>;

fn json<T>(
    nome: &str,
    texto: &str,
    item_de: &impl Fn(&Value, &str) -> Result<T, CliError>,
) -> Result<(Option<String>, Lidos<T>), CliError> {
    let valor: Value = serde_json::from_str(texto).map_err(|err| {
        CliError::Usage(format!(
            "{nome}: JSON inválido na linha {}, coluna {}",
            err.line(),
            err.column()
        ))
    })?;
    let (descricao, itens) = match &valor {
        Value::Array(itens) => (None, itens),
        Value::Object(objeto) => {
            let campos = Campos::de(&valor, nome, &["descricao", "cobsv"])?;
            let Some(Value::Array(itens)) = objeto.get("cobsv") else {
                return Err(campos.erro("cobsv", "obrigatório: a lista de cobranças"));
            };
            (campos.texto("descricao")?, itens)
        }
        _ => {
            return Err(CliError::Usage(format!(
                "{nome}: esperado um objeto com \"cobsv\", ou a lista de cobranças"
            )));
        }
    };
    let lidos = itens
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let onde = format!("cobrança {}", i + 1);
            let lido = item_de(item, &onde);
            (onde, lido)
        })
        .collect();
    Ok((descricao, lidos))
}

fn csv<T>(
    nome: &str,
    texto: &str,
    item_de: &impl Fn(&Value, &str) -> Result<T, CliError>,
) -> Result<Lidos<T>, CliError> {
    let (cabecalho, registros) =
        csv::ler(texto).map_err(|err| CliError::Usage(format!("{nome}: {err}")))?;
    if let Some(coluna) = cabecalho
        .iter()
        .find(|coluna| !COLUNAS.contains(&coluna.as_str()))
    {
        return Err(CliError::Usage(format!(
            "{nome}: coluna desconhecida \"{coluna}\"; as colunas aceitas são {}",
            COLUNAS.join(", ")
        )));
    }
    if let Some(repetida) = cabecalho
        .iter()
        .enumerate()
        .find_map(|(i, coluna)| cabecalho[..i].contains(coluna).then_some(coluna))
    {
        return Err(CliError::Usage(format!(
            "{nome}: a coluna \"{repetida}\" aparece mais de uma vez"
        )));
    }
    if !cabecalho.iter().any(|coluna| coluna == "txid") {
        return Err(CliError::Usage(format!("{nome}: falta a coluna \"txid\"")));
    }
    Ok(registros
        .into_iter()
        .map(|registro| {
            let celulas: Vec<(&str, String)> = cabecalho
                .iter()
                .map(String::as_str)
                .zip(registro.campos)
                .filter(|(_, celula)| !celula.trim().is_empty())
                .collect();
            let onde = format!("linha {}", registro.linha);
            let lido = match estragado_pelo_excel(&celulas, &onde) {
                Some(erro) => Err(erro),
                None => item_de(&aninhar(celulas), &onde),
            };
            (onde, lido)
        })
        .collect())
}

/// The cells of a line as the objects of the API: `valor.multa.valorPerc`
/// goes to `{"valor": {"multa": {"valorPerc": ...}}}`, and
/// `descontoDataFixa[1]` to the second item of that list.
fn aninhar(celulas: Vec<(&str, String)>) -> Value {
    let mut raiz = Value::Object(Map::new());
    for (caminho, celula) in celulas {
        let partes: Vec<&str> = caminho.split('.').collect();
        let mut atual = &mut raiz;
        for (i, parte) in partes.iter().enumerate() {
            let (nome, indice) = match parte.split_once('[') {
                Some((nome, resto)) => (nome, resto.trim_end_matches(']').parse::<usize>().ok()),
                None => (*parte, None),
            };
            let Some(objeto) = atual.as_object_mut() else {
                break;
            };
            let ultima = i + 1 == partes.len();
            atual = match indice {
                None if ultima => {
                    objeto.insert(nome.to_owned(), Value::String(celula));
                    break;
                }
                None => objeto
                    .entry(nome.to_owned())
                    .or_insert_with(|| Value::Object(Map::new())),
                Some(indice) => {
                    let lista = objeto
                        .entry(nome.to_owned())
                        .or_insert_with(|| Value::Array(Vec::new()));
                    let Some(itens) = lista.as_array_mut() else {
                        break;
                    };
                    // Positions stay as in the columns, so messages match them.
                    while itens.len() <= indice {
                        itens.push(Value::Object(Map::new()));
                    }
                    &mut itens[indice]
                }
            };
        }
    }
    raiz
}

/// What Excel does to a CSV it opens, explained: long numbers lose their
/// digits, and documents and postal codes their leading zeros. All would
/// be refused anyway, less clearly.
fn estragado_pelo_excel(celulas: &[(&str, String)], onde: &str) -> Option<CliError> {
    let erro = |coluna: &str, problema: String| {
        CliError::Usage(format!(
            "{onde}, campo \"{coluna}\": {problema}; formate a coluna como texto e digite de novo"
        ))
    };
    for (coluna, celula) in celulas {
        let texto = celula.trim();
        if matches!(*coluna, "txid" | "devedor.cpf" | "devedor.cnpj" | "loc.id")
            && notacao_cientifica(texto)
        {
            return Some(erro(
                coluna,
                format!(
                    "\"{texto}\" está em notação científica, como o Excel mostra números longos, e os dígitos se perderam"
                ),
            ));
        }
        let digitos = |tamanho: usize| {
            texto.len() < tamanho && !texto.is_empty() && texto.bytes().all(|b| b.is_ascii_digit())
        };
        let curto = match *coluna {
            "devedor.cpf" => digitos(11),
            "devedor.cnpj" => digitos(14),
            "devedor.cep" => digitos(8),
            _ => false,
        };
        if curto {
            return Some(erro(
                coluna,
                format!(
                    "\"{texto}\" tem dígitos de menos: o Excel tira os zeros à esquerda (01001000 vira 1001000)"
                ),
            ));
        }
    }
    None
}

fn txid(campos: &Campos<'_>) -> Result<Txid, CliError> {
    campos
        .obrigatorio("txid", campos.texto("txid")?)?
        .parse()
        .map_err(|err: TxidError| campos.erro("txid", err))
}

fn vencimento_valido(
    campos: &Campos<'_>,
    vencimento: NaiveDate,
    hoje: NaiveDate,
) -> Result<(), CliError> {
    if vencimento < hoje {
        return Err(campos.erro(
            "calendario.dataDeVencimento",
            format!(
                "o vencimento ({}) já passou: a cobrança vence hoje ou depois",
                vencimento.format("%d/%m/%Y")
            ),
        ));
    }
    Ok(())
}

fn cobv_do_lote(valor: &Value, onde: &str, hoje: NaiveDate) -> Result<CobvDoLote, CliError> {
    let campos = Campos::de(valor, onde, &CAMPOS_ITEM)?;
    let txid = txid(&campos)?;
    let cobv = ler_cobv(&campos)?;
    vencimento_valido(&campos, cobv.calendario.data_de_vencimento, hoje)?;
    Ok(CobvDoLote::new(txid, cobv))
}

fn revisao_do_lote(
    valor: &Value,
    onde: &str,
    hoje: NaiveDate,
) -> Result<CobvRevisadaDoLote, CliError> {
    let campos = Campos::de(valor, onde, &CAMPOS_ITEM_REVISAO)?;
    let txid = txid(&campos)?;
    let revisao = ler_revisao_cobv(&campos)?;
    if let Some(calendario) = &revisao.calendario {
        vencimento_valido(&campos, calendario.data_de_vencimento, hoje)?;
    }
    Ok(CobvRevisadaDoLote::new(txid, revisao))
}

fn juntar<T>(
    nome: &str,
    descricao: Option<String>,
    lidos: Lidos<T>,
    txid: fn(&T) -> &Txid,
) -> Result<ArquivoLoteCobv<T>, CliError> {
    let mut cobsv = Vec::new();
    let mut erros = Vec::new();
    let mut vistos: HashMap<String, String> = HashMap::new();
    for (onde, lido) in lidos {
        match lido {
            Ok(item) => {
                let id = txid(&item).to_string();
                if let Some(primeiro) = vistos.get(&id) {
                    erros.push(format!(
                        "{onde}, campo \"txid\": o txid {id} já aparece em {primeiro}"
                    ));
                } else {
                    vistos.insert(id, onde.clone());
                    cobsv.push((onde, item));
                }
            }
            Err(err) => erros.push(err.to_string()),
        }
    }
    if erros.is_empty() {
        if cobsv.is_empty() {
            return Err(CliError::Usage(format!("{nome}: o lote não tem cobranças")));
        }
        return Ok(ArquivoLoteCobv { descricao, cobsv });
    }
    let quantas = if erros.len() == 1 {
        "1 cobrança com problema".to_owned()
    } else {
        format!("{} cobranças com problema", erros.len())
    };
    let mut texto = format!("{nome}: {quantas}; nada foi enviado:");
    for erro in erros.iter().take(MAX_ERROS) {
        let _ = write!(texto, "\n  {erro}");
    }
    if erros.len() > MAX_ERROS {
        let _ = write!(texto, "\n  ... e mais {}", erros.len() - MAX_ERROS);
    }
    Err(CliError::Usage(texto))
}

/// A batch to start from, with synthetic data; the dates go in
/// `{vencimento}` and `{desconto}`.
const MODELO_JSON: &str = r#"{
  "descricao": "Mensalidades de outubro",
  "cobsv": [
    {
      "txid": "mensalidade202610cliente0001",
      "calendario": {"dataDeVencimento": "{vencimento}", "validadeAposVencimento": 30},
      "devedor": {"cnpj": "12.345.678/0001-95", "nome": "Cliente Exemplo Ltda", "email": "financeiro@empresa.example"},
      "valor": {
        "original": "150,00",
        "multa": {"modalidade": 2, "valorPerc": "2,00"},
        "juros": {"modalidade": 3, "valorPerc": "1,00"}
      },
      "chave": "pix@empresa.example",
      "solicitacaoPagador": "Mensalidade de outubro"
    },
    {
      "txid": "mensalidade202610cliente0002",
      "calendario": {"dataDeVencimento": "{vencimento}"},
      "devedor": {"cpf": "123.456.789-09", "nome": "Fulano de Tal"},
      "valor": {
        "original": "89,90",
        "desconto": {"modalidade": 1, "descontoDataFixa": [{"data": "{desconto}", "valorPerc": "5,00"}]}
      },
      "chave": "pix@empresa.example"
    }
  ]
}
"#;

/// The same batch in the columns of the CSV file, separated by `;` as Excel
/// in Portuguese saves them.
const MODELO_CSV: &str = "\u{feff}txid;calendario.dataDeVencimento;calendario.validadeAposVencimento;devedor.cpf;devedor.cnpj;devedor.nome;devedor.email;valor.original;valor.multa.modalidade;valor.multa.valorPerc;valor.juros.modalidade;valor.juros.valorPerc;valor.desconto.modalidade;valor.desconto.descontoDataFixa[0].data;valor.desconto.descontoDataFixa[0].valorPerc;chave;solicitacaoPagador\r
mensalidade202610cliente0001;{vencimento};30;;12.345.678/0001-95;Cliente Exemplo Ltda;financeiro@empresa.example;150,00;2;2,00;3;1,00;;;;pix@empresa.example;Mensalidade de outubro\r
mensalidade202610cliente0002;{vencimento};;123.456.789-09;;Fulano de Tal;;89,90;;;;;1;{desconto};5,00;pix@empresa.example;\r
";

/// A batch to start from, due in 30 days, in JSON or CSV.
pub(crate) fn modelo_lote_cobv(hoje: NaiveDate, csv: bool) -> String {
    let vencimento = hoje.checked_add_days(Days::new(30)).unwrap_or(hoje);
    let desconto = vencimento
        .checked_sub_days(Days::new(5))
        .unwrap_or(vencimento);
    let modelo = if csv { MODELO_CSV } else { MODELO_JSON };
    modelo
        .replace("{vencimento}", &vencimento.format("%Y-%m-%d").to_string())
        .replace("{desconto}", &desconto.format("%Y-%m-%d").to_string())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;

    use super::*;

    fn hoje() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, 23).unwrap()
    }

    fn ler_texto(nome: &str, conteudo: &str) -> Result<ArquivoLoteCobv<CobvDoLote>, String> {
        let dir = tempfile::tempdir().unwrap();
        let caminho = dir.path().join(nome);
        fs::write(&caminho, conteudo).unwrap();
        ler_lote_cobv(&caminho, hoje()).map_err(|err| err.to_string())
    }

    #[test]
    fn both_templates_are_the_same_batch() {
        let json = ler_texto("lote.json", &modelo_lote_cobv(hoje(), false)).unwrap();
        let csv = ler_texto("lote.csv", &modelo_lote_cobv(hoje(), true)).unwrap();
        assert_eq!(json.descricao.as_deref(), Some("Mensalidades de outubro"));
        assert_eq!(csv.descricao, None);
        let corpo = |arquivo: &ArquivoLoteCobv<CobvDoLote>| {
            arquivo
                .cobsv
                .iter()
                .map(|(_, item)| serde_json::to_value(item).unwrap())
                .collect::<Vec<_>>()
        };
        assert_eq!(corpo(&json), corpo(&csv));
        assert_eq!(
            corpo(&csv)[1],
            json!({
                "txid": "mensalidade202610cliente0002",
                "calendario": {"dataDeVencimento": "2026-10-23"},
                "devedor": {"cpf": "12345678909", "nome": "Fulano de Tal"},
                "valor": {"original": "89.90", "desconto": {"modalidade": 1, "descontoDataFixa": [{"data": "2026-10-18", "valorPerc": "5.00"}]}},
                "chave": "pix@empresa.example"
            })
        );
        assert_eq!(csv.cobsv[1].0, "linha 3");
        assert_eq!(json.cobsv[0].0, "cobrança 1");
    }

    #[test]
    fn every_problem_is_listed_with_its_line_and_field() {
        let modelo = modelo_lote_cobv(hoje(), true);
        let mut linhas: Vec<String> = modelo.lines().map(str::to_owned).collect();
        // A past due date, an unknown modality, and the first txid again.
        linhas[1] = linhas[1].replace("2026-10-23", "2026-09-01");
        linhas[2] = linhas[2].replace(";1;2026-10-18;", ";7;2026-10-18;");
        linhas.push(linhas[1].replace("2026-09-01", "2026-10-23"));
        linhas.push(linhas[3].replace("12.345.678/0001-95", "1234567000195"));
        let erro = ler_texto("lote.csv", &linhas.join("\n")).unwrap_err();
        assert!(
            erro.contains("lote.csv: 3 cobranças com problema; nada foi enviado:\n"),
            "{erro}"
        );
        for linha in [
            "  linha 2, campo \"calendario.dataDeVencimento\": o vencimento (01/09/2026) já passou",
            "  linha 3, campo \"valor.desconto.modalidade\": 7 não é uma das modalidades: 1, 2, 3, 4, 5, 6",
            "  linha 5, campo \"devedor.cnpj\": \"1234567000195\" tem dígitos de menos: o Excel tira os zeros à esquerda",
        ] {
            assert!(erro.contains(linha), "{linha}\n{erro}");
        }
        // Line 4 repeats the txid of line 2, which was refused: no duplicate.
        assert!(!erro.contains("linha 4"), "{erro}");

        let erro = ler_texto(
            "lote.json",
            &json!({"descricao": "x", "cobsv": [
                modelo_json_item(0), modelo_json_item(0)
            ]})
            .to_string(),
        )
        .unwrap_err();
        assert!(
            erro.contains("cobrança 2, campo \"txid\": o txid mensalidade202610cliente0001 já aparece em cobrança 1"),
            "{erro}"
        );
    }

    fn modelo_json_item(i: usize) -> Value {
        let modelo: Value = serde_json::from_str(&modelo_lote_cobv(hoje(), false)).unwrap();
        modelo["cobsv"][i].clone()
    }

    #[test]
    fn files_that_are_not_a_batch_are_refused() {
        for (nome, conteudo, esperado) in [
            (
                "lote.csv",
                "txid;valor.origem\r\nx;1\r\n",
                "lote.csv: coluna desconhecida \"valor.origem\"",
            ),
            (
                "lote.csv",
                "chave\r\npix@empresa.example\r\n",
                "lote.csv: falta a coluna \"txid\"",
            ),
            (
                "lote.csv",
                "txid;txid\r\na;b\r\n",
                "lote.csv: a coluna \"txid\" aparece mais de uma vez",
            ),
            (
                "lote.json",
                "{\"cobsv\": []}",
                "lote.json: o lote não tem cobranças",
            ),
            (
                "lote.json",
                "{\"lote\": []}",
                "lote.json: campo desconhecido \"lote\"",
            ),
            ("lote.json", "[1, 2", "lote.json: JSON inválido"),
        ] {
            let erro = ler_texto(nome, conteudo).unwrap_err();
            assert!(erro.contains(esperado), "{esperado}\n{erro}");
        }
        let erro = ler_texto("lote.csv", "txid;devedor.cpf\r\n1,23457E+25;123\r\n").unwrap_err();
        assert!(erro.contains("notação científica"), "{erro}");
    }

    #[test]
    fn revisions_change_only_what_is_given() {
        let dir = tempfile::tempdir().unwrap();
        let caminho = dir.path().join("revisao.csv");
        fs::write(
            &caminho,
            "txid;calendario.dataDeVencimento;valor.original;status\r\nmensalidade202610cliente0001;2026-10-30;160,00;\r\nmensalidade202610cliente0002;;;REMOVIDA_PELO_USUARIO_RECEBEDOR\r\n",
        )
        .unwrap();
        let revisao = ler_revisao_lote_cobv(&caminho, hoje()).unwrap();
        let corpo: Vec<Value> = revisao
            .cobsv
            .iter()
            .map(|(_, item)| serde_json::to_value(item).unwrap())
            .collect();
        assert_eq!(
            corpo,
            [
                json!({"txid": "mensalidade202610cliente0001", "calendario": {"dataDeVencimento": "2026-10-30"}, "valor": {"original": "160.00"}}),
                json!({"txid": "mensalidade202610cliente0002", "status": "REMOVIDA_PELO_USUARIO_RECEBEDOR"})
            ]
        );
        for (conteudo, esperado) in [
            (
                "txid\r\nmensalidade202610cliente0001\r\n",
                "linha 2: informe o que muda na cobrança",
            ),
            (
                "txid;calendario.validadeAposVencimento\r\nmensalidade202610cliente0001;10\r\n",
                "linha 2, campo \"calendario.dataDeVencimento\": obrigatório para mudar o calendário",
            ),
            (
                "txid;valor.original;status\r\nmensalidade202610cliente0001;1;REMOVIDA_PELO_USUARIO_RECEBEDOR\r\n",
                "linha 2, campo \"valor\": uma cobrança removida não muda mais nada",
            ),
            (
                "txid;status\r\nmensalidade202610cliente0001;CONCLUIDA\r\n",
                "linha 2, campo \"status\": \"CONCLUIDA\": a única mudança de status",
            ),
        ] {
            fs::write(&caminho, conteudo).unwrap();
            let erro = ler_revisao_lote_cobv(&caminho, hoje())
                .unwrap_err()
                .to_string();
            assert!(erro.contains(esperado), "{esperado}\n{erro}");
        }
    }
}
