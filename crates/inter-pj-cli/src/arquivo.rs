//! Payment files given with `--arquivo`: JSON with the API's field names,
//! read with errors that name the field (and the item, in a batch).
//!
//! Unknown fields are refused: a typo such as `valorMuta` would otherwise
//! drop the fine without anyone noticing.

use std::fmt::Display;
use std::io::{self, Read};
use std::path::Path;
use std::str::FromStr;

use chrono::NaiveDate;
use inter_pj::banking::PagamentoDarf;
use inter_pj::documento::Documento;
use rust_decimal::Decimal;
use serde_json::{Map, Value};

use crate::error::CliError;
use crate::valor::parse_valor_ou_zero;

/// Largest file accepted: a batch of 150 payments takes about 60 KB.
const TAMANHO_MAXIMO: u64 = 1024 * 1024;

/// Fields of a DARF, as in the API (`DarfRequest`).
pub(crate) const CAMPOS_DARF: [&str; 11] = [
    "cnpjCpf",
    "codigoReceita",
    "dataVencimento",
    "descricao",
    "nomeEmpresa",
    "telefoneEmpresa",
    "periodoApuracao",
    "valorPrincipal",
    "valorMulta",
    "valorJuros",
    "referencia",
];

/// Reads a text file, or the standard input for `-`.
pub(crate) fn ler(caminho: &Path) -> Result<String, CliError> {
    let erro = |err: io::Error| CliError::io(format!("falha ao ler {}", caminho.display()), err);
    let mut texto = String::new();
    if caminho.as_os_str() == "-" {
        io::stdin()
            .lock()
            .take(TAMANHO_MAXIMO + 1)
            .read_to_string(&mut texto)
            .map_err(erro)?;
    } else {
        std::fs::File::open(caminho)
            .and_then(|arquivo| arquivo.take(TAMANHO_MAXIMO + 1).read_to_string(&mut texto))
            .map_err(erro)?;
    }
    if texto.len() as u64 > TAMANHO_MAXIMO {
        return Err(CliError::Usage(format!(
            "{}: arquivo grande demais (máximo de 1 MB)",
            caminho.display()
        )));
    }
    // Files saved by some editors start with a byte order mark.
    Ok(texto.strip_prefix('\u{feff}').unwrap_or(&texto).to_owned())
}

/// Reads and parses a JSON file.
pub(crate) fn ler_json(caminho: &Path) -> Result<Value, CliError> {
    let texto = ler(caminho)?;
    serde_json::from_str(&texto).map_err(|err| {
        CliError::Usage(format!(
            "{}: JSON inválido na linha {}, coluna {}",
            caminho.display(),
            err.line(),
            err.column()
        ))
    })
}

/// The fields of a JSON object, read with errors that say where they are.
pub(crate) struct Campos<'a> {
    objeto: &'a Map<String, Value>,
    /// Where the object is: `darf.json`, `pagamento 3`.
    onde: &'a str,
}

impl<'a> Campos<'a> {
    /// The object `valor`, which may only have the fields `aceitos`.
    pub(crate) fn de(valor: &'a Value, onde: &'a str, aceitos: &[&str]) -> Result<Self, CliError> {
        let Value::Object(objeto) = valor else {
            return Err(CliError::Usage(format!(
                "{onde}: esperado um objeto JSON, com os campos {}",
                aceitos.join(", ")
            )));
        };
        if let Some(campo) = objeto
            .keys()
            .find(|campo| !aceitos.contains(&campo.as_str()))
        {
            return Err(CliError::Usage(format!(
                "{onde}: campo desconhecido \"{campo}\"; os campos aceitos são {}",
                aceitos.join(", ")
            )));
        }
        Ok(Self { objeto, onde })
    }

    /// An error about `campo`.
    pub(crate) fn erro(&self, campo: &str, problema: impl Display) -> CliError {
        CliError::Usage(format!("{}, campo \"{campo}\": {problema}", self.onde))
    }

    fn bruto(&self, campo: &str) -> Option<&'a Value> {
        self.objeto.get(campo).filter(|valor| !valor.is_null())
    }

    /// `valor`, or an error saying that `campo` is missing.
    pub(crate) fn obrigatorio<T>(&self, campo: &str, valor: Option<T>) -> Result<T, CliError> {
        valor.ok_or_else(|| self.erro(campo, "obrigatório"))
    }

    /// Text, trimmed; blank means absent. Numbers are taken as their digits.
    pub(crate) fn texto(&self, campo: &str) -> Result<Option<String>, CliError> {
        match self.bruto(campo) {
            None => Ok(None),
            Some(Value::String(texto)) => {
                Ok(Some(texto.trim().to_owned()).filter(|texto| !texto.is_empty()))
            }
            Some(Value::Number(numero)) => Ok(Some(numero.to_string())),
            Some(_) => Err(self.erro(campo, "esperado um texto")),
        }
    }

    /// A date as `AAAA-MM-DD`.
    pub(crate) fn data(&self, campo: &str) -> Result<Option<NaiveDate>, CliError> {
        self.texto(campo)?
            .map(|texto| {
                NaiveDate::parse_from_str(&texto, "%Y-%m-%d").map_err(|_| {
                    self.erro(campo, format!("data inválida \"{texto}\": use AAAA-MM-DD"))
                })
            })
            .transpose()
    }

    /// An amount: a JSON number (`150.5`) or a text as typed by people
    /// (`"150,50"`, `"1.500,00"`). Zero is accepted; the payment checks the
    /// rest.
    pub(crate) fn valor(&self, campo: &str) -> Result<Option<Decimal>, CliError> {
        match self.bruto(campo) {
            None => Ok(None),
            Some(Value::Number(numero)) => {
                let texto = numero.to_string();
                Decimal::from_str(&texto)
                    .or_else(|_| Decimal::from_scientific(&texto))
                    .map(Some)
                    .map_err(|_| self.erro(campo, format!("valor inválido {texto}")))
            }
            Some(Value::String(texto)) if texto.trim().is_empty() => Ok(None),
            Some(Value::String(texto)) => parse_valor_ou_zero(texto)
                .map(Some)
                .map_err(|err| self.erro(campo, err)),
            Some(_) => Err(self.erro(campo, "esperado um número ou um texto com o valor")),
        }
    }

    /// A CPF or CNPJ, with or without punctuation.
    pub(crate) fn documento(&self, campo: &str) -> Result<Option<Documento>, CliError> {
        self.texto(campo)?
            .map(|texto| Documento::parse(&texto).map_err(|err| self.erro(campo, err)))
            .transpose()
    }
}

/// A DARF from its fields ([`CAMPOS_DARF`]), validated.
pub(crate) fn darf(campos: &Campos<'_>) -> Result<PagamentoDarf, CliError> {
    let obrigatorio = |campo: &str| {
        campos
            .texto(campo)
            .and_then(|valor| campos.obrigatorio(campo, valor))
    };
    let darf = PagamentoDarf {
        cnpj_cpf: campos.obrigatorio("cnpjCpf", campos.documento("cnpjCpf")?)?,
        codigo_receita: obrigatorio("codigoReceita")?,
        data_vencimento: campos.obrigatorio("dataVencimento", campos.data("dataVencimento")?)?,
        descricao: obrigatorio("descricao")?,
        nome_empresa: obrigatorio("nomeEmpresa")?,
        telefone_empresa: campos.texto("telefoneEmpresa")?,
        periodo_apuracao: campos.obrigatorio("periodoApuracao", campos.data("periodoApuracao")?)?,
        valor_principal: campos.obrigatorio("valorPrincipal", campos.valor("valorPrincipal")?)?,
        valor_multa: campos.valor("valorMulta")?,
        valor_juros: campos.valor("valorJuros")?,
        referencia: obrigatorio("referencia")?,
    };
    darf.validar()
        .map_err(|err| campos.erro(err.campo(), err))?;
    Ok(darf)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn exemplo() -> Value {
        json!({
            "cnpjCpf": "12.345.678/0001-95",
            "codigoReceita": "0220",
            "dataVencimento": "2026-10-30",
            "descricao": "IRPJ de setembro",
            "nomeEmpresa": "Empresa Exemplo",
            "periodoApuracao": "2026-09-30",
            "valorPrincipal": 47.14,
            "valorMulta": "27,48",
            "valorJuros": 0,
            "referencia": 13_609_400_849_201_739_u64
        })
    }

    fn ler_darf(valor: &Value) -> Result<PagamentoDarf, String> {
        Campos::de(valor, "darf.json", &CAMPOS_DARF)
            .and_then(|campos| darf(&campos))
            .map_err(|err| err.to_string())
    }

    #[test]
    fn reads_a_darf_with_the_api_field_names() {
        let lido = ler_darf(&exemplo()).unwrap();
        assert_eq!(lido.cnpj_cpf.as_str(), "12345678000195");
        assert_eq!(lido.valor_principal, "47.14".parse().unwrap());
        assert_eq!(lido.valor_multa, Some("27.48".parse().unwrap()));
        assert_eq!(lido.valor_juros, Some(Decimal::ZERO));
        assert_eq!(lido.referencia, "13609400849201739");
        assert_eq!(lido.telefone_empresa, None);
        assert_eq!(lido.valor_total(), "74.62".parse().unwrap());
    }

    #[test]
    fn errors_name_the_field() {
        let com = |campo: &str, valor: Value| {
            let mut darf = exemplo();
            darf[campo] = valor;
            ler_darf(&darf).unwrap_err()
        };
        for (campo, valor, esperado) in [
            (
                "cnpjCpf",
                json!(null),
                "darf.json, campo \"cnpjCpf\": obrigatório",
            ),
            (
                "cnpjCpf",
                json!("123.456.789-00"),
                "darf.json, campo \"cnpjCpf\": ",
            ),
            (
                "codigoReceita",
                json!(220),
                "darf.json, campo \"codigoReceita\": o código da receita tem 4 dígitos",
            ),
            (
                "dataVencimento",
                json!("30/10/2026"),
                "darf.json, campo \"dataVencimento\": data inválida \"30/10/2026\": use AAAA-MM-DD",
            ),
            (
                "valorPrincipal",
                json!("1.500"),
                "darf.json, campo \"valorPrincipal\": valor inválido \"1.500\"",
            ),
            (
                "valorPrincipal",
                json!(0),
                "darf.json, campo \"valorPrincipal\": o valor principal deve ser maior que zero",
            ),
            (
                "valorMulta",
                json!(-1),
                "darf.json, campo \"valorMulta\": a multa não pode ser negativa",
            ),
            (
                "valorJuros",
                json!(true),
                "darf.json, campo \"valorJuros\": esperado um número",
            ),
            (
                "descricao",
                json!(" "),
                "darf.json, campo \"descricao\": obrigatório",
            ),
            (
                "nomeEmpresa",
                json!("x".repeat(101)),
                "darf.json, campo \"nomeEmpresa\": o nome da empresa deve ter de 1 a 100 caracteres",
            ),
            (
                "referencia",
                json!(["1"]),
                "darf.json, campo \"referencia\": esperado um texto",
            ),
        ] {
            let erro = com(campo, valor);
            assert!(erro.starts_with(esperado), "{campo}: {erro}");
        }
    }

    #[test]
    fn unknown_fields_and_other_shapes_are_refused() {
        let mut darf = exemplo();
        darf["valorMuta"] = json!(27.48);
        let erro = ler_darf(&darf).unwrap_err();
        assert!(
            erro.starts_with(
                "darf.json: campo desconhecido \"valorMuta\"; os campos aceitos são cnpjCpf, "
            ),
            "{erro}"
        );
        let erro = ler_darf(&json!([exemplo()])).unwrap_err();
        assert!(
            erro.starts_with("darf.json: esperado um objeto JSON"),
            "{erro}"
        );
    }

    #[test]
    fn reads_files_and_reports_invalid_json() {
        let dir = tempfile::tempdir().unwrap();
        let caminho = dir.path().join("darf.json");
        std::fs::write(&caminho, format!("\u{feff}{}", exemplo())).unwrap();
        assert_eq!(ler_json(&caminho).unwrap(), exemplo());

        std::fs::write(&caminho, "{\n  \"cnpjCpf\": \"1\",\n}").unwrap();
        let erro = ler_json(&caminho).unwrap_err().to_string();
        assert!(
            erro.ends_with("darf.json: JSON inválido na linha 3, coluna 1"),
            "{erro}"
        );

        std::fs::write(&caminho, " ".repeat(1024 * 1024 + 1)).unwrap();
        let erro = ler(&caminho).unwrap_err().to_string();
        assert!(erro.contains("arquivo grande demais"), "{erro}");

        let erro = ler(&dir.path().join("nao-existe.json")).unwrap_err();
        assert!(matches!(erro, CliError::Io { .. }), "{erro}");
    }
}
