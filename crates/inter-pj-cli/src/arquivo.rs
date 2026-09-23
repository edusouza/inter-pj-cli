//! Payment files given with `--arquivo`: JSON with the API's field names,
//! read with errors that name the field (and the item, in a batch).
//!
//! Unknown fields are refused: a typo such as `valorMuta` would otherwise
//! drop the fine without anyone noticing.

mod cobranca;
mod csv;
mod lote;

use std::fmt::Display;
use std::io::{self, Read};
use std::path::Path;
use std::str::FromStr;

use chrono::NaiveDate;
use inter_pj::banking::{PagamentoBoleto, PagamentoDarf};
use inter_pj::boleto::CodigoBarras;
use inter_pj::documento::Documento;
use rust_decimal::Decimal;
use serde_json::{Map, Value};

use crate::error::CliError;
use crate::valor::parse_valor_ou_zero;

/// Largest file accepted: a batch of 150 payments takes about 60 KB.
const TAMANHO_MAXIMO: u64 = 1024 * 1024;

pub(crate) use cobranca::{cobranca, modelo_cobranca};
pub(crate) use lote::{ArquivoLote, MODELO_CSV, MODELO_JSON, ler_lote};

/// Fields of a payment by barcode, as in the API (`EfetuarPagamento`).
pub(crate) const CAMPOS_BOLETO: [&str; 5] = [
    "codBarraLinhaDigitavel",
    "valorPagar",
    "dataVencimento",
    "dataPagamento",
    "cpfCnpjBeneficiario",
];

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

/// How messages name a file: its path, or the standard input for `-`.
pub(crate) fn nome(caminho: &Path) -> String {
    if caminho.as_os_str() == "-" {
        "entrada padrão".to_owned()
    } else {
        caminho.display().to_string()
    }
}

/// Reads a text file, or the standard input for `-`.
pub(crate) fn ler(caminho: &Path) -> Result<String, CliError> {
    let erro = |err: io::Error| CliError::io(format!("falha ao ler {}", nome(caminho)), err);
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
            nome(caminho)
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
            nome(caminho),
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
    /// Path of a nested object, for the messages: `pagador.`.
    prefixo: String,
}

impl<'a> Campos<'a> {
    /// The object `valor`, which may only have the fields `aceitos`.
    pub(crate) fn de(valor: &'a Value, onde: &'a str, aceitos: &[&str]) -> Result<Self, CliError> {
        Self::com_prefixo(valor, onde, String::new(), aceitos)
    }

    fn com_prefixo(
        valor: &'a Value,
        onde: &'a str,
        prefixo: String,
        aceitos: &[&str],
    ) -> Result<Self, CliError> {
        let Value::Object(objeto) = valor else {
            let quem = if prefixo.is_empty() {
                onde.to_owned()
            } else {
                format!("{onde}, campo \"{}\"", prefixo.trim_end_matches('.'))
            };
            return Err(CliError::Usage(format!(
                "{quem}: esperado um objeto JSON, com os campos {}",
                aceitos.join(", ")
            )));
        };
        if let Some(campo) = objeto
            .keys()
            .find(|campo| !aceitos.contains(&campo.as_str()))
        {
            return Err(CliError::Usage(format!(
                "{onde}: campo desconhecido \"{prefixo}{campo}\"; os campos aceitos são {}",
                aceitos.join(", ")
            )));
        }
        Ok(Self {
            objeto,
            onde,
            prefixo,
        })
    }

    /// The object in `campo`, if given, which may only have the fields
    /// `aceitos`. Its messages name the fields by their path
    /// (`pagador.cep`).
    pub(crate) fn objeto(
        &self,
        campo: &str,
        aceitos: &[&str],
    ) -> Result<Option<Campos<'a>>, CliError> {
        self.bruto(campo)
            .map(|valor| {
                Self::com_prefixo(
                    valor,
                    self.onde,
                    format!("{}{campo}.", self.prefixo),
                    aceitos,
                )
            })
            .transpose()
    }

    /// An error about `campo`.
    pub(crate) fn erro(&self, campo: &str, problema: impl Display) -> CliError {
        CliError::Usage(format!(
            "{}, campo \"{}{campo}\": {problema}",
            self.onde, self.prefixo
        ))
    }

    /// A whole number: a JSON number or its digits as text.
    pub(crate) fn inteiro(&self, campo: &str) -> Result<Option<u32>, CliError> {
        let erro = || self.erro(campo, "esperado um número inteiro, sem sinal");
        match self.bruto(campo) {
            None => Ok(None),
            Some(Value::Number(numero)) => numero
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .map(Some)
                .ok_or_else(erro),
            Some(Value::String(texto)) if texto.trim().is_empty() => Ok(None),
            Some(Value::String(texto)) => texto.trim().parse().map(Some).map_err(|_| erro()),
            Some(_) => Err(erro()),
        }
    }

    /// A list.
    pub(crate) fn lista(&self, campo: &str) -> Result<Option<&'a Vec<Value>>, CliError> {
        match self.bruto(campo) {
            None => Ok(None),
            Some(Value::Array(itens)) => Ok(Some(itens)),
            Some(_) => Err(self.erro(campo, "esperada uma lista")),
        }
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

/// A payment by barcode from its fields ([`CAMPOS_BOLETO`]), validated. As
/// in `pagamento boleto pagar`, amount and due date come from the code
/// unless given, and today means now.
pub(crate) fn boleto(campos: &Campos<'_>, hoje: NaiveDate) -> Result<PagamentoBoleto, CliError> {
    const CODIGO: &str = "codBarraLinhaDigitavel";
    let texto = campos.obrigatorio(CODIGO, campos.texto(CODIGO)?)?;
    let codigo = CodigoBarras::parse(&texto).map_err(|err| campos.erro(CODIGO, err))?;
    let valor = campos
        .valor("valorPagar")?
        .or_else(|| codigo.valor())
        .ok_or_else(|| campos.erro("valorPagar", "obrigatório: o código não traz o valor"))?;
    let vencimento = campos
        .data("dataVencimento")?
        .or_else(|| codigo.vencimento(hoje))
        .ok_or_else(|| {
            campos.erro(
                "dataVencimento",
                "obrigatório: o código não traz o vencimento (contas e tributos)",
            )
        })?;
    let data_pagamento = campos.data("dataPagamento")?;
    if let Some(data) = data_pagamento
        && data < hoje
    {
        return Err(campos.erro(
            "dataPagamento",
            format!(
                "o dia {} já passou: agende para hoje ou depois",
                data.format("%d/%m/%Y")
            ),
        ));
    }
    let mut pagamento = PagamentoBoleto::new(codigo, valor, vencimento);
    pagamento.data_pagamento = data_pagamento.filter(|data| *data > hoje);
    pagamento.cpf_cnpj_beneficiario = campos.documento("cpfCnpjBeneficiario")?;
    pagamento
        .validar()
        .map_err(|err| campos.erro("valorPagar", err))?;
    Ok(pagamento)
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
    fn names_the_standard_input() {
        assert_eq!(nome(Path::new("-")), "entrada padrão");
        assert_eq!(nome(Path::new("darf.json")), "darf.json");
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
    fn reads_a_payment_by_barcode() {
        let hoje = NaiveDate::from_ymd_opt(2026, 9, 23).unwrap();
        let ler_boleto = |valor: Value| {
            Campos::de(&valor, "pagamento 1", &CAMPOS_BOLETO)
                .and_then(|campos| boleto(&campos, hoje))
                .map_err(|err| err.to_string())
        };
        // Amount and due date from the code.
        let lido = ler_boleto(json!({
            "codBarraLinhaDigitavel": "07797.77705 11678.471159 90071.126347 1 15950000003010"
        }))
        .unwrap();
        assert_eq!(lido.valor_pagar, "30.10".parse().unwrap());
        assert_eq!(
            lido.data_vencimento,
            NaiveDate::from_ymd_opt(2026, 10, 10).unwrap()
        );
        assert_eq!(lido.data_pagamento, None);

        let lido = ler_boleto(json!({
            "codBarraLinhaDigitavel": "82670000000653301602023123106000000002830894",
            "valorPagar": "65,33",
            "dataVencimento": "2026-10-10",
            "dataPagamento": "2026-10-09",
            "cpfCnpjBeneficiario": "12345678000195"
        }))
        .unwrap();
        assert_eq!(lido.data_pagamento, NaiveDate::from_ymd_opt(2026, 10, 9));
        assert!(lido.cpf_cnpj_beneficiario.is_some());

        for (valor, esperado) in [
            (
                json!({}),
                "pagamento 1, campo \"codBarraLinhaDigitavel\": obrigatório",
            ),
            (
                json!({"codBarraLinhaDigitavel": "123"}),
                "pagamento 1, campo \"codBarraLinhaDigitavel\": ",
            ),
            (
                json!({"codBarraLinhaDigitavel": "82670000000653301602023123106000000002830894"}),
                "pagamento 1, campo \"dataVencimento\": obrigatório: o código não traz o vencimento",
            ),
            (
                json!({"codBarraLinhaDigitavel": "00195000000000000000000000000000000000000000"}),
                "pagamento 1, campo \"valorPagar\": obrigatório: o código não traz o valor",
            ),
            (
                json!({
                    "codBarraLinhaDigitavel": "07797777051167847115990071126347115950000003010",
                    "dataPagamento": "2026-09-22"
                }),
                "pagamento 1, campo \"dataPagamento\": o dia 22/09/2026 já passou",
            ),
            (
                json!({
                    "codBarraLinhaDigitavel": "07797777051167847115990071126347115950000003010",
                    "valorPagar": 0
                }),
                "pagamento 1, campo \"valorPagar\": o valor a pagar deve ser maior que zero",
            ),
        ] {
            let erro = ler_boleto(valor).unwrap_err();
            assert!(erro.starts_with(esperado), "{esperado}\n{erro}");
        }
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
