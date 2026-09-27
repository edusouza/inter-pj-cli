//! Models shared by the operations of the Pix API: people, additional
//! information, periods, pages and locations.

use chrono::{DateTime, FixedOffset, SecondsFormat};
use rust_decimal::Decimal;
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize, Serializer};

use crate::documento::Documento;
use crate::serde_util::{api_enum, lenient, string_serde};

/// Largest amount of the Pix API: 10 digits before the decimal point
/// (`\d{1,10}\.\d{2}`).
pub const VALOR_MAXIMO_PIX: Decimal = Decimal::from_parts(3_567_587_327, 232, 0, false, 2);

/// Most items per page of the listings.
pub const ITENS_POR_PAGINA_MAXIMO_PIX: u32 = 1000;

/// A field of a Pix charge that the API would refuse. [`campo`](Self::campo)
/// names it as the API does (`valor.original`, `infoAdicionais[2].nome`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{problema}")]
pub struct CobrancaPixError {
    campo: String,
    problema: String,
}

impl CobrancaPixError {
    pub(crate) fn new(campo: impl Into<String>, problema: impl Into<String>) -> Self {
        Self {
            campo: campo.into(),
            problema: problema.into(),
        }
    }

    /// The field, with the API's name and path.
    pub fn campo(&self) -> &str {
        &self.campo
    }
}

/// A text of 1 to `maximo` characters, on one line.
pub(crate) fn texto(valor: &str, campo: &str, maximo: usize) -> Result<(), CobrancaPixError> {
    let tamanho = valor.chars().count();
    if valor.trim().is_empty() {
        return Err(CobrancaPixError::new(campo, "não pode ficar em branco"));
    }
    if tamanho > maximo {
        return Err(CobrancaPixError::new(
            campo,
            format!("tem {tamanho} caracteres; o máximo é {maximo}"),
        ));
    }
    if valor.chars().any(char::is_control) {
        return Err(CobrancaPixError::new(
            campo,
            "não pode ter quebras de linha nem outros caracteres de controle",
        ));
    }
    Ok(())
}

/// An amount the API accepts: up to [`VALOR_MAXIMO_PIX`], with at most two
/// decimal places, and positive unless `zero` is allowed.
pub(crate) fn valor(valor: Decimal, campo: &str, zero: bool) -> Result<(), CobrancaPixError> {
    let minimo = if zero { "zero" } else { "R$ 0,01" };
    let abaixo = if zero {
        valor.is_sign_negative()
    } else {
        valor <= Decimal::ZERO
    };
    if abaixo || valor > VALOR_MAXIMO_PIX || valor.normalize().scale() > 2 {
        return Err(CobrancaPixError::new(
            campo,
            format!("o valor vai de {minimo} a R$ 9.999.999.999,99, com até 2 casas decimais"),
        ));
    }
    Ok(())
}

/// Who owes a Pix charge: name and CPF or CNPJ (`PessoaFisica` or
/// `PessoaJuridica`).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Devedor {
    /// CPF or CNPJ, sent as `cpf` or `cnpj`.
    pub documento: Documento,
    /// Name, up to 200 characters.
    pub nome: String,
}

impl Devedor {
    /// A payer of a charge.
    pub fn new(documento: Documento, nome: impl Into<String>) -> Self {
        Self {
            documento,
            nome: nome.into(),
        }
    }

    pub(crate) fn validar(&self, campo: &str) -> Result<(), CobrancaPixError> {
        texto(&self.nome, &format!("{campo}.nome"), 200)
    }
}

impl Serialize for Devedor {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(2))?;
        match &self.documento {
            Documento::Cpf(cpf) => map.serialize_entry("cpf", cpf)?,
            Documento::Cnpj(cnpj) => map.serialize_entry("cnpj", cnpj)?,
        }
        map.serialize_entry("nome", &self.nome)?;
        map.end()
    }
}

/// A person or company in an answer (payer, receiver), as received.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PessoaPix {
    /// CPF, for people.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub cpf: Option<String>,
    /// CNPJ, for companies.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub cnpj: Option<String>,
    /// Name.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub nome: Option<String>,
    /// Trade name of a receiver.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub nome_fantasia: Option<String>,
    /// E-mail.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub email: Option<String>,
    /// Street address.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub logradouro: Option<String>,
    /// City.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub cidade: Option<String>,
    /// State (two letters).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub uf: Option<String>,
    /// Postal code (CEP).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub cep: Option<String>,
}

impl PessoaPix {
    /// The CPF or the CNPJ, whichever came.
    pub fn documento(&self) -> Option<&str> {
        self.cpf
            .as_deref()
            .or(self.cnpj.as_deref())
            .filter(|documento| !documento.trim().is_empty())
    }
}

/// A piece of information shown to the payer (`infoAdicionais`): a name of
/// up to 50 characters and a value of up to 200.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct InfoAdicional {
    /// Name of the field.
    #[serde(default, deserialize_with = "texto_ou_vazio")]
    pub nome: String,
    /// Content of the field.
    #[serde(default, deserialize_with = "texto_ou_vazio")]
    pub valor: String,
}

impl InfoAdicional {
    /// A name and its value.
    pub fn new(nome: impl Into<String>, valor: impl Into<String>) -> Self {
        Self {
            nome: nome.into(),
            valor: valor.into(),
        }
    }
}

/// Most [`InfoAdicional`] per charge.
pub const MAX_INFO_ADICIONAIS: usize = 50;

pub(crate) fn validar_info_adicionais(infos: &[InfoAdicional]) -> Result<(), CobrancaPixError> {
    if infos.len() > MAX_INFO_ADICIONAIS {
        return Err(CobrancaPixError::new(
            "infoAdicionais",
            format!(
                "são {} informações; o máximo é {MAX_INFO_ADICIONAIS}",
                infos.len()
            ),
        ));
    }
    for (i, info) in infos.iter().enumerate() {
        texto(&info.nome, &format!("infoAdicionais[{i}].nome"), 50)?;
        texto(&info.valor, &format!("infoAdicionais[{i}].valor"), 200)?;
    }
    Ok(())
}

fn texto_ou_vazio<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    Ok(lenient::string(deserializer)?.unwrap_or_default())
}

api_enum! {
    /// Kind of charge a location is for (`TipoLocationCobEnum`).
    pub enum TipoCob {
        /// `cob`: an immediate charge.
        Cob => "cob",
        /// `cobv`: a charge with a due date.
        Cobv => "cobv",
    }
}

string_serde!(TipoCob);

/// Where the payload of a charge is published (`PayloadLocation`), as
/// received.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct LocationPix {
    /// Identifier of the location.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::u64"
    )]
    pub id: Option<u64>,
    /// Address of the payload, without the scheme.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub location: Option<String>,
    /// Kind of charge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tipo_cob: Option<TipoCob>,
    /// When the location was created (RFC 3339).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub criacao: Option<String>,
}

/// A page of a listing (`Paginacao`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Paginacao {
    /// Number of the page, from 0.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::u64"
    )]
    pub pagina_atual: Option<u64>,
    /// Items per page.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::u64"
    )]
    pub itens_por_pagina: Option<u64>,
    /// Number of pages.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::u64"
    )]
    pub quantidade_de_paginas: Option<u64>,
    /// Number of items of every page.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::u64"
    )]
    pub quantidade_total_de_itens: Option<u64>,
}

impl Paginacao {
    /// Whether pages after `pagina` remain. Without the number of pages,
    /// a full page means there may be more.
    pub fn tem_mais(&self, pagina: u32, recebidos: usize) -> bool {
        match self.quantidade_de_paginas {
            Some(paginas) => u64::from(pagina) + 1 < paginas,
            None => self
                .itens_por_pagina
                .is_some_and(|itens| recebidos as u64 >= itens && itens > 0),
        }
    }
}

/// Period of a listing, by date and time with offset (RFC 3339), as the Pix
/// API filters: the charges created, or the Pix processed, from `inicio` to
/// `fim`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct PeriodoPix {
    /// Start, inclusive.
    pub inicio: DateTime<FixedOffset>,
    /// End, inclusive.
    pub fim: DateTime<FixedOffset>,
}

impl PeriodoPix {
    /// From `inicio` to `fim`.
    ///
    /// # Errors
    ///
    /// When the period ends before it starts.
    pub fn new(
        inicio: DateTime<FixedOffset>,
        fim: DateTime<FixedOffset>,
    ) -> Result<Self, PeriodoPixError> {
        if fim < inicio {
            return Err(PeriodoPixError);
        }
        Ok(Self { inicio, fim })
    }

    pub(crate) fn query(&self) -> [(&'static str, String); 2] {
        [
            ("inicio", data_hora(self.inicio)),
            ("fim", data_hora(self.fim)),
        ]
    }
}

/// A period that ends before it starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("o período termina antes de começar")]
pub struct PeriodoPixError;

/// `2026-09-01T00:00:00-03:00`.
pub(crate) fn data_hora(momento: DateTime<FixedOffset>) -> String {
    momento.to_rfc3339_opts(SecondsFormat::Secs, true)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn the_largest_amount_has_ten_digits() {
        assert_eq!(
            VALOR_MAXIMO_PIX,
            "9999999999.99".parse::<Decimal>().unwrap()
        );
        assert!(valor(VALOR_MAXIMO_PIX, "valor", false).is_ok());
        assert!(valor("0.01".parse().unwrap(), "valor", false).is_ok());
        assert!(valor(Decimal::ZERO, "valor", true).is_ok());
        for invalido in ["0", "-1", "10000000000", "1.001"] {
            let err = valor(invalido.parse().unwrap(), "valor.original", false).unwrap_err();
            assert_eq!(err.campo(), "valor.original");
        }
    }

    #[test]
    fn payers_are_sent_with_cpf_or_cnpj() {
        let pessoa = Devedor::new("123.456.789-09".parse().unwrap(), "Fulano de Tal");
        assert_eq!(
            serde_json::to_value(&pessoa).unwrap(),
            json!({"cpf": "12345678909", "nome": "Fulano de Tal"})
        );
        let empresa = Devedor::new("12.345.678/0001-95".parse().unwrap(), "Empresa Exemplo");
        assert_eq!(
            serde_json::to_value(&empresa).unwrap(),
            json!({"cnpj": "12345678000195", "nome": "Empresa Exemplo"})
        );
        assert_eq!(
            Devedor::new(pessoa.documento.clone(), "a".repeat(201))
                .validar("devedor")
                .unwrap_err()
                .campo(),
            "devedor.nome"
        );
    }

    #[test]
    fn texts_have_limits() {
        assert!(texto("abc", "campo", 3).is_ok());
        assert!(texto("abcd", "campo", 3).is_err());
        assert!(texto("  ", "campo", 3).is_err());
        assert!(texto("a\nb", "campo", 3).is_err());
        let infos: Vec<InfoAdicional> = (0..51)
            .map(|i| InfoAdicional::new("n", i.to_string()))
            .collect();
        assert_eq!(
            validar_info_adicionais(&infos).unwrap_err().campo(),
            "infoAdicionais"
        );
        let err =
            validar_info_adicionais(&[InfoAdicional::new("n", "v"), InfoAdicional::new("", "v")])
                .unwrap_err();
        assert_eq!(err.campo(), "infoAdicionais[1].nome");
    }

    #[test]
    fn pages_say_whether_more_remain() {
        let pagina: Paginacao = serde_json::from_value(json!({
            "paginaAtual": 0, "itensPorPagina": 100, "quantidadeDePaginas": 2, "quantidadeTotalDeItens": 150
        }))
        .unwrap();
        assert!(pagina.tem_mais(0, 100));
        assert!(!pagina.tem_mais(1, 50));
        let sem_total: Paginacao = serde_json::from_value(json!({"itensPorPagina": "2"})).unwrap();
        assert!(sem_total.tem_mais(0, 2));
        assert!(!sem_total.tem_mais(0, 1));
    }

    #[test]
    fn periods_are_rfc3339_with_offset() {
        let inicio = DateTime::parse_from_rfc3339("2026-09-01T00:00:00-03:00").unwrap();
        let fim = DateTime::parse_from_rfc3339("2026-09-30T23:59:59-03:00").unwrap();
        let periodo = PeriodoPix::new(inicio, fim).unwrap();
        assert_eq!(
            periodo.query(),
            [
                ("inicio", "2026-09-01T00:00:00-03:00".to_owned()),
                ("fim", "2026-09-30T23:59:59-03:00".to_owned())
            ]
        );
        assert_eq!(PeriodoPix::new(fim, inicio), Err(PeriodoPixError));
        let utc = DateTime::parse_from_rfc3339("2026-09-01T03:00:00Z").unwrap();
        assert_eq!(data_hora(utc), "2026-09-01T03:00:00Z");
    }
}
