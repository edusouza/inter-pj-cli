//! The file of `pagamento lote enviar`: JSON as the API's body (or just the
//! list of payments), or CSV with one payment per line.
//!
//! Every payment is checked, and every problem reported at once with its
//! line (CSV) or position (JSON), so that a spreadsheet is fixed in one go.

use std::fmt::Write as _;
use std::path::Path;

use chrono::NaiveDate;
use inter_pj::banking::ItemLote;
use serde_json::{Map, Value};

use super::{CAMPOS_BOLETO, CAMPOS_DARF, Campos, boleto, csv, darf, ler, notacao_cientifica};
use crate::error::CliError;

/// Most problems listed at once.
const MAX_ERROS: usize = 20;

const TIPO: &str = "tipoPagamento";

/// Fields of each kind of payment, besides its kind.
const CAMPOS_ITEM_BOLETO: [&str; 6] = [
    TIPO,
    CAMPOS_BOLETO[0],
    CAMPOS_BOLETO[1],
    CAMPOS_BOLETO[2],
    CAMPOS_BOLETO[3],
    CAMPOS_BOLETO[4],
];
const CAMPOS_ITEM_DARF: [&str; 12] = [
    TIPO,
    CAMPOS_DARF[0],
    CAMPOS_DARF[1],
    CAMPOS_DARF[2],
    CAMPOS_DARF[3],
    CAMPOS_DARF[4],
    CAMPOS_DARF[5],
    CAMPOS_DARF[6],
    CAMPOS_DARF[7],
    CAMPOS_DARF[8],
    CAMPOS_DARF[9],
    CAMPOS_DARF[10],
];

/// Columns of the CSV file: the kind, then the fields of both kinds
/// (`dataVencimento` is shared).
const COLUNAS: [&str; 16] = [
    TIPO,
    "codBarraLinhaDigitavel",
    "valorPagar",
    "dataVencimento",
    "dataPagamento",
    "cpfCnpjBeneficiario",
    "cnpjCpf",
    "codigoReceita",
    "periodoApuracao",
    "valorPrincipal",
    "valorMulta",
    "valorJuros",
    "referencia",
    "descricao",
    "nomeEmpresa",
    "telefoneEmpresa",
];

/// `pagamento lote modelo`: a JSON file with fictitious data and the
/// sandbox codes of the API documentation.
pub(crate) const MODELO_JSON: &str = r#"{
  "meuIdentificador": "Pagamentos de outubro",
  "pagamentos": [
    {
      "tipoPagamento": "BOLETO",
      "codBarraLinhaDigitavel": "03399.20142 93990.000379 28300.301026 5 98850000066653"
    },
    {
      "tipoPagamento": "BOLETO",
      "codBarraLinhaDigitavel": "82670000000-1 65330160202-1 31231060000-1 00002830894-8",
      "valorPagar": "65,33",
      "dataVencimento": "2026-10-10"
    },
    {
      "tipoPagamento": "DARF",
      "cnpjCpf": "12.345.678/0001-95",
      "codigoReceita": "0220",
      "nomeEmpresa": "Empresa Exemplo",
      "periodoApuracao": "2026-09-30",
      "dataVencimento": "2026-10-30",
      "referencia": "13609400849201739",
      "descricao": "IRPJ de setembro",
      "valorPrincipal": "47,14"
    }
  ]
}
"#;

/// `pagamento lote modelo csv`: the same payments, in the columns of the
/// CSV file, separated by `;` as Excel in Portuguese saves them.
pub(crate) const MODELO_CSV: &str = "\u{feff}tipoPagamento;codBarraLinhaDigitavel;valorPagar;dataVencimento;dataPagamento;cpfCnpjBeneficiario;cnpjCpf;codigoReceita;periodoApuracao;valorPrincipal;valorMulta;valorJuros;referencia;descricao;nomeEmpresa;telefoneEmpresa\r
BOLETO;03399.20142 93990.000379 28300.301026 5 98850000066653;;;;;;;;;;;;;;\r
BOLETO;82670000000-1 65330160202-1 31231060000-1 00002830894-8;65,33;2026-10-10;;;;;;;;;;;;\r
DARF;;;2026-10-30;;;12.345.678/0001-95;0220;2026-09-30;47,14;;;13609400849201739;IRPJ de setembro;Empresa Exemplo;\r
";

/// What a batch file holds.
#[derive(Debug)]
pub(crate) struct ArquivoLote {
    /// `meuIdentificador` of a JSON file.
    pub(crate) meu_identificador: Option<String>,
    /// Each payment, and where it is in the file (`linha 3`, `pagamento 2`).
    pub(crate) pagamentos: Vec<(String, ItemLote)>,
}

/// Reads a batch file (or the standard input for `-`): JSON when it starts
/// with `{` or `[`, CSV otherwise.
///
/// # Errors
///
/// [`CliError::Usage`] listing every payment with a problem; nothing is
/// returned when any is invalid.
pub(crate) fn ler_lote(caminho: &Path, hoje: NaiveDate) -> Result<ArquivoLote, CliError> {
    let texto = ler(caminho)?;
    let nome = super::nome(caminho);
    if texto.trim_start().starts_with(['{', '[']) {
        json(&nome, &texto, hoje)
    } else {
        csv(&nome, &texto, hoje)
    }
}

fn json(nome: &str, texto: &str, hoje: NaiveDate) -> Result<ArquivoLote, CliError> {
    let valor: Value = serde_json::from_str(texto).map_err(|err| {
        CliError::Usage(format!(
            "{nome}: JSON inválido na linha {}, coluna {}",
            err.line(),
            err.column()
        ))
    })?;
    let (meu_identificador, itens) = match &valor {
        Value::Array(itens) => (None, itens),
        Value::Object(objeto) => {
            let campos = Campos::de(&valor, nome, &["meuIdentificador", "pagamentos"])?;
            let Some(Value::Array(itens)) = objeto.get("pagamentos") else {
                return Err(campos.erro("pagamentos", "obrigatório: a lista de pagamentos"));
            };
            (campos.texto("meuIdentificador")?, itens)
        }
        _ => {
            return Err(CliError::Usage(format!(
                "{nome}: esperado um objeto com \"pagamentos\", ou a lista de pagamentos"
            )));
        }
    };
    let lidos = itens.iter().enumerate().map(|(i, item)| {
        let onde = format!("pagamento {}", i + 1);
        let lido = item_de(item, &onde, hoje);
        (onde, lido)
    });
    juntar(nome, meu_identificador, lidos)
}

fn csv(nome: &str, texto: &str, hoje: NaiveDate) -> Result<ArquivoLote, CliError> {
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
    if !cabecalho.iter().any(|coluna| coluna == TIPO) {
        return Err(CliError::Usage(format!(
            "{nome}: falta a coluna \"{TIPO}\" (BOLETO ou DARF)"
        )));
    }
    let lidos = registros.into_iter().map(|registro| {
        let objeto: Map<String, Value> = cabecalho
            .iter()
            .zip(registro.campos)
            .filter(|(_, celula)| !celula.trim().is_empty())
            .map(|(coluna, celula)| (coluna.clone(), Value::String(celula)))
            .collect();
        let onde = format!("linha {}", registro.linha);
        let lido = match estragado_pelo_excel(&objeto, &onde) {
            Some(erro) => Err(erro),
            None => item_de(&Value::Object(objeto), &onde, hoje),
        };
        (onde, lido)
    });
    juntar(nome, None, lidos)
}

/// Columns of digits too long for Excel, which shows them in scientific
/// notation (and saves them so, without the digits).
const NUMEROS_LONGOS: [&str; 5] = [
    "codBarraLinhaDigitavel",
    "cpfCnpjBeneficiario",
    "cnpjCpf",
    "referencia",
    "telefoneEmpresa",
];

/// What Excel does to a CSV it opens, explained: long numbers lose their
/// digits and revenue codes their leading zero. Both would be refused
/// anyway, less clearly.
fn estragado_pelo_excel(objeto: &Map<String, Value>, onde: &str) -> Option<CliError> {
    let erro = |coluna: &str, problema: String| {
        CliError::Usage(format!(
            "{onde}, campo \"{coluna}\": {problema}; formate a coluna como texto e digite de novo"
        ))
    };
    let celula = |coluna: &str| objeto.get(coluna).and_then(Value::as_str).map(str::trim);
    for coluna in NUMEROS_LONGOS {
        if let Some(texto) = celula(coluna).filter(|texto| notacao_cientifica(texto)) {
            return Some(erro(
                coluna,
                format!(
                    "\"{texto}\" está em notação científica, como o Excel mostra números longos, e os dígitos se perderam"
                ),
            ));
        }
    }
    let darf = celula(TIPO).is_some_and(|tipo| tipo.eq_ignore_ascii_case("DARF"));
    let receita = celula("codigoReceita")
        .filter(|receita| receita.len() < 4 && receita.bytes().all(|b| b.is_ascii_digit()));
    match receita {
        Some(receita) if darf => Some(erro(
            "codigoReceita",
            format!(
                "\"{receita}\": o código da receita tem 4 dígitos, e o Excel tira os zeros à esquerda (0220 vira 220)"
            ),
        )),
        _ => None,
    }
}

fn item_de(valor: &Value, onde: &str, hoje: NaiveDate) -> Result<ItemLote, CliError> {
    let tipo = valor
        .get(TIPO)
        .and_then(Value::as_str)
        .map(|tipo| tipo.trim().to_ascii_uppercase());
    match tipo.as_deref() {
        Some("BOLETO") => {
            let campos = Campos::de(valor, onde, &CAMPOS_ITEM_BOLETO)?;
            Ok(ItemLote::Boleto(boleto(&campos, hoje)?))
        }
        Some("DARF") => {
            let campos = Campos::de(valor, onde, &CAMPOS_ITEM_DARF)?;
            Ok(ItemLote::Darf(darf(&campos)?))
        }
        Some(outro) => Err(CliError::Usage(format!(
            "{onde}, campo \"{TIPO}\": \"{outro}\" não é BOLETO nem DARF"
        ))),
        None => Err(CliError::Usage(format!(
            "{onde}, campo \"{TIPO}\": obrigatório (BOLETO ou DARF)"
        ))),
    }
}

fn juntar(
    nome: &str,
    meu_identificador: Option<String>,
    lidos: impl Iterator<Item = (String, Result<ItemLote, CliError>)>,
) -> Result<ArquivoLote, CliError> {
    let mut pagamentos = Vec::new();
    let mut erros = Vec::new();
    for (onde, lido) in lidos {
        match lido {
            Ok(item) => pagamentos.push((onde, item)),
            Err(err) => erros.push(err.to_string()),
        }
    }
    if erros.is_empty() {
        return Ok(ArquivoLote {
            meu_identificador,
            pagamentos,
        });
    }
    let quantos = if erros.len() == 1 {
        "1 pagamento com problema".to_owned()
    } else {
        format!("{} pagamentos com problema", erros.len())
    };
    let mut texto = format!("{nome}: {quantos}; nada foi enviado:");
    for erro in erros.iter().take(MAX_ERROS) {
        let _ = write!(texto, "\n  {erro}");
    }
    if erros.len() > MAX_ERROS {
        let _ = write!(texto, "\n  ... e mais {}", erros.len() - MAX_ERROS);
    }
    Err(CliError::Usage(texto))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;

    use super::*;

    fn hoje() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, 23).unwrap()
    }

    fn ler_texto(nome: &str, conteudo: &str) -> Result<ArquivoLote, String> {
        let dir = tempfile::tempdir().unwrap();
        let caminho = dir.path().join(nome);
        fs::write(&caminho, conteudo).unwrap();
        ler_lote(&caminho, hoje()).map_err(|err| {
            // Messages start with the path of the temporary file.
            err.to_string()
                .replace(&caminho.display().to_string(), nome)
        })
    }

    #[test]
    fn templates_are_valid_batches() {
        for (nome, modelo) in [("lote.json", MODELO_JSON), ("lote.csv", MODELO_CSV)] {
            let lote = ler_texto(nome, modelo).unwrap();
            let tipos: Vec<bool> = lote
                .pagamentos
                .iter()
                .map(|(_, item)| matches!(item, ItemLote::Boleto(_)))
                .collect();
            assert_eq!(tipos, [true, true, false], "{nome}");
            let total: rust_decimal::Decimal =
                lote.pagamentos.iter().map(|(_, item)| item.valor()).sum();
            assert_eq!(total, "779.00".parse().unwrap(), "{nome}");
        }
        let json = ler_texto("lote.json", MODELO_JSON).unwrap();
        assert_eq!(
            json.meu_identificador.as_deref(),
            Some("Pagamentos de outubro")
        );
        assert_eq!(json.pagamentos[2].0, "pagamento 3");
        let csv = ler_texto("lote.csv", MODELO_CSV).unwrap();
        assert_eq!(csv.meu_identificador, None);
        assert_eq!(csv.pagamentos[2].0, "linha 4");
    }

    #[test]
    fn json_may_be_the_list_of_payments() {
        let lista = json!([
            {"tipoPagamento": "boleto", "codBarraLinhaDigitavel": "07797777051167847115990071126347115950000003010"},
            {"tipoPagamento": "BOLETO", "codBarraLinhaDigitavel": "82670000000653301602023123106000000002830894", "dataVencimento": "2026-10-10"}
        ]);
        let lote = ler_texto("lote.json", &lista.to_string()).unwrap();
        assert_eq!(lote.pagamentos.len(), 2);
    }

    #[test]
    fn every_problem_is_reported_with_its_line_and_field() {
        let csv = "\
tipoPagamento,codBarraLinhaDigitavel,valorPagar,dataVencimento,codigoReceita
BOLETO,07797777051167847115990071126347115950000003010,,,
BOLETO,17797777051167847115990071126347115950000003010,,,
PIX,,,,
BOLETO,82670000000653301602023123106000000002830894,,,
BOLETO,07797777051167847115990071126347115950000003010,,,0220
,,,,
";
        let erro = ler_texto("lote.csv", csv).unwrap_err();
        let linhas: Vec<&str> = erro.lines().collect();
        assert_eq!(
            linhas[0],
            "lote.csv: 4 pagamentos com problema; nada foi enviado:"
        );
        assert!(
            linhas[1].starts_with("  linha 3, campo \"codBarraLinhaDigitavel\": "),
            "{erro}"
        );
        assert_eq!(
            linhas[2],
            "  linha 4, campo \"tipoPagamento\": \"PIX\" não é BOLETO nem DARF"
        );
        assert!(
            linhas[3].starts_with("  linha 5, campo \"dataVencimento\": obrigatório"),
            "{erro}"
        );
        assert!(
            linhas[4].starts_with("  linha 6: campo desconhecido \"codigoReceita\""),
            "{erro}"
        );
        assert_eq!(linhas.len(), 5, "{erro}");
    }

    #[test]
    fn explains_what_excel_did_to_the_cells() {
        let csv = "\
tipoPagamento;codBarraLinhaDigitavel;valorPagar;dataVencimento;cnpjCpf;codigoReceita;periodoApuracao;valorPrincipal;referencia;descricao;nomeEmpresa
BOLETO;8,267E+43;65,33;2026-10-10;;;;;;;
DARF;;;2026-10-30;12.345.678/0001-95;220;2026-09-30;47,14;13609400849201739;IRPJ;Empresa Exemplo
DARF;;;2026-10-30;12.345.678/0001-95;0220;2026-09-30;47,14;1,36094E+16;IRPJ;Empresa Exemplo
DARF;;;2026-10-30;12.345.678/0001-95;0220;2026-09-30;47,14;13609400849201739;1E5;Empresa Exemplo
BOLETO;82670000000653301602023123106000000002830894;65,33;2026-10-10;;;;;;;
";
        let erro = ler_texto("lote.csv", csv).unwrap_err();
        let linhas: Vec<&str> = erro.lines().collect();
        let dica = "; formate a coluna como texto e digite de novo";
        assert_eq!(
            linhas[1..],
            [
                format!(
                    "  linha 2, campo \"codBarraLinhaDigitavel\": \"8,267E+43\" está em notação científica, como o Excel mostra números longos, e os dígitos se perderam{dica}"
                ),
                format!(
                    "  linha 3, campo \"codigoReceita\": \"220\": o código da receita tem 4 dígitos, e o Excel tira os zeros à esquerda (0220 vira 220){dica}"
                ),
                format!(
                    "  linha 4, campo \"referencia\": \"1,36094E+16\" está em notação científica, como o Excel mostra números longos, e os dígitos se perderam{dica}"
                ),
            ],
            "{erro}"
        );
    }

    #[test]
    fn recognizes_scientific_notation() {
        for texto in ["1,36094E+16", "1.36094e+16", "8E+43", "1,2E16"] {
            assert!(notacao_cientifica(texto), "{texto}");
        }
        for texto in [
            "13609400849201739",
            "E+16",
            "1,36094E+",
            "1,36094E-2x",
            "IRPJ E+1",
            "",
        ] {
            assert!(!notacao_cientifica(texto), "{texto}");
        }
    }

    #[test]
    fn long_lists_of_problems_are_cut() {
        let mut json = String::from("[");
        for i in 0..25 {
            if i > 0 {
                json.push(',');
            }
            json.push_str(r#"{"tipoPagamento": "DARF"}"#);
        }
        json.push(']');
        let erro = ler_texto("lote.json", &json).unwrap_err();
        assert!(
            erro.starts_with("lote.json: 25 pagamentos com problema"),
            "{erro}"
        );
        assert_eq!(erro.lines().count(), 1 + MAX_ERROS + 1, "{erro}");
        assert!(erro.ends_with("  ... e mais 5"), "{erro}");
    }

    #[test]
    fn broken_files_are_refused() {
        for (nome, conteudo, esperado) in [
            (
                "lote.json",
                "{\"pagamentos\": [",
                "lote.json: JSON inválido na linha 1",
            ),
            (
                "lote.json",
                r#"{"lote": []}"#,
                "lote.json: campo desconhecido \"lote\"",
            ),
            (
                "lote.json",
                r#"{"meuIdentificador": "x"}"#,
                "lote.json, campo \"pagamentos\": obrigatório",
            ),
            // Not JSON, so read as CSV, whose header is wrong.
            (
                "lote.json",
                "\"pagamentos\"",
                "lote.json: coluna desconhecida \"pagamentos\"",
            ),
            (
                "lote.csv",
                "tipo;valor\nBOLETO;1\n",
                "lote.csv: coluna desconhecida \"tipo\"",
            ),
            (
                "lote.csv",
                "valorPagar\n1\n",
                "lote.csv: falta a coluna \"tipoPagamento\"",
            ),
            (
                "lote.csv",
                "tipoPagamento;tipoPagamento\n",
                "lote.csv: a coluna \"tipoPagamento\" aparece mais de uma vez",
            ),
            ("lote.csv", "", "lote.csv: arquivo vazio"),
        ] {
            let erro = ler_texto(nome, conteudo).unwrap_err();
            assert!(erro.starts_with(esperado), "{esperado}\n{erro}");
        }
    }
}
