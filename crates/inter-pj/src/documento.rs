//! Brazilian taxpayer documents: CPF (people) and CNPJ (companies).

use std::fmt;
use std::str::FromStr;

/// A CPF or CNPJ with valid check digits.
///
/// Accepts the usual punctuation (`123.456.789-09`, `12.345.678/0001-95`)
/// and the alphanumeric CNPJ introduced in 2026, whose first 12 characters
/// may be letters (`12.ABC.345/01DE-35`).
///
/// ```
/// use inter_pj::documento::Documento;
///
/// let cnpj: Documento = "12.ABC.345/01DE-35".parse().unwrap();
/// assert_eq!(cnpj.as_str(), "12ABC34501DE35");
/// assert!("123.456.789-00".parse::<Documento>().is_err());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Documento {
    /// CPF: 11 digits.
    Cpf(String),
    /// CNPJ: 14 characters, the last two always digits.
    Cnpj(String),
}

impl Documento {
    /// Parses and validates a CPF or CNPJ.
    ///
    /// # Errors
    ///
    /// Fails when the text has the wrong size or characters, repeats a
    /// single character or has wrong check digits.
    pub fn parse(raw: &str) -> Result<Self, DocumentoError> {
        let normalized: String = raw
            .chars()
            .filter(|c| !matches!(c, '.' | '-' | '/' | ' '))
            .map(|c| c.to_ascii_uppercase())
            .collect();
        match normalized.len() {
            11 => {
                if !normalized.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(DocumentoError::Caracteres);
                }
                check(&normalized)?;
                Ok(Self::Cpf(normalized))
            }
            14 => {
                let (base, dv) = normalized.split_at(12);
                if !base
                    .bytes()
                    .all(|b| b.is_ascii_digit() || b.is_ascii_uppercase())
                    || !dv.bytes().all(|b| b.is_ascii_digit())
                {
                    return Err(DocumentoError::Caracteres);
                }
                check(&normalized)?;
                Ok(Self::Cnpj(normalized))
            }
            tamanho => Err(DocumentoError::Tamanho { tamanho }),
        }
    }

    /// The document as the APIs expect it: digits (and letters) only.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Cpf(value) | Self::Cnpj(value) => value,
        }
    }

    /// With the usual punctuation: `123.456.789-09`, `12.345.678/0001-95`.
    pub fn formatado(&self) -> String {
        match self {
            Self::Cpf(v) => format!("{}.{}.{}-{}", &v[..3], &v[3..6], &v[6..9], &v[9..]),
            Self::Cnpj(v) => format!(
                "{}.{}.{}/{}-{}",
                &v[..2],
                &v[2..5],
                &v[5..8],
                &v[8..12],
                &v[12..]
            ),
        }
    }
}

impl FromStr for Documento {
    type Err = DocumentoError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        Self::parse(raw)
    }
}

impl fmt::Display for Documento {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.formatado())
    }
}

/// Why a CPF or CNPJ is invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum DocumentoError {
    /// Neither 11 (CPF) nor 14 (CNPJ) characters.
    #[error("CPF/CNPJ com {tamanho} caracteres: um CPF tem 11 dígitos e um CNPJ, 14")]
    Tamanho {
        /// Characters found, punctuation excluded.
        tamanho: usize,
    },
    /// Letters where only digits are allowed.
    #[error("CPF/CNPJ com caracteres inválidos")]
    Caracteres,
    /// The same character repeated (e.g. `111.111.111-11`).
    #[error("CPF/CNPJ inválido (sequência repetida)")]
    Repetido,
    /// The check digits do not match.
    #[error("CPF/CNPJ inválido (dígitos verificadores não conferem)")]
    DigitoVerificador,
}

/// Checks the two trailing check digits of a normalized CPF or CNPJ.
fn check(document: &str) -> Result<(), DocumentoError> {
    let bytes = document.as_bytes();
    if bytes.iter().all(|&b| b == bytes[0]) {
        return Err(DocumentoError::Repetido);
    }
    // Digits are worth their value; CNPJ letters, their ASCII code minus 48.
    let values: Vec<u32> = bytes.iter().map(|&b| u32::from(b) - 48).collect();
    let size = values.len() - 2;
    let first = check_digit(&values[..size]);
    let mut with_first = values[..size].to_vec();
    with_first.push(first);
    let second = check_digit(&with_first);
    if values[size..] == [first, second] {
        Ok(())
    } else {
        Err(DocumentoError::DigitoVerificador)
    }
}

/// Module 11 check digit, with weights 2 to 9 (CNPJ) or growing (CPF) from the right.
fn check_digit(values: &[u32]) -> u32 {
    let cpf = values.len() <= 10;
    let total: u32 = values
        .iter()
        .rev()
        .zip(2u32..)
        .map(|(value, position)| {
            let weight = if cpf {
                position
            } else {
                (position - 2) % 8 + 2
            };
            value * weight
        })
        .sum();
    match total % 11 {
        0 | 1 => 0,
        rest => 11 - rest,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Appends the correct check digits to a synthetic base.
    fn with_check_digits(base: &str) -> String {
        let values: Vec<u32> = base.bytes().map(|b| u32::from(b) - 48).collect();
        let first = check_digit(&values);
        let mut all = values;
        all.push(first);
        let second = check_digit(&all);
        format!("{base}{first}{second}")
    }

    #[test]
    fn accepts_valid_cpfs_with_or_without_punctuation() {
        assert_eq!(
            Documento::parse("123.456.789-09"),
            Ok(Documento::Cpf("12345678909".into()))
        );
        assert_eq!(
            Documento::parse(" 12345678909 ").unwrap().formatado(),
            "123.456.789-09"
        );
        assert_eq!(with_check_digits("123456789"), "12345678909");
    }

    #[test]
    fn accepts_numeric_and_alphanumeric_cnpjs() {
        let cnpj = Documento::parse("12.345.678/0001-95").unwrap();
        assert_eq!(cnpj, Documento::Cnpj("12345678000195".into()));
        assert_eq!(cnpj.to_string(), "12.345.678/0001-95");
        // Official example of the alphanumeric CNPJ (IN RFB 2.229/2024).
        let alfanumerico = Documento::parse("12.abc.345/01de-35").unwrap();
        assert_eq!(alfanumerico.as_str(), "12ABC34501DE35");
        assert_eq!(alfanumerico.formatado(), "12.ABC.345/01DE-35");
    }

    #[test]
    fn rejects_wrong_check_digits_sizes_and_characters() {
        assert_eq!(
            Documento::parse("123.456.789-00"),
            Err(DocumentoError::DigitoVerificador)
        );
        assert_eq!(
            Documento::parse("12.345.678/0001-96"),
            Err(DocumentoError::DigitoVerificador)
        );
        assert_eq!(
            Documento::parse("12ABC34501DE3A"),
            Err(DocumentoError::Caracteres)
        );
        assert_eq!(
            Documento::parse("1234567890A"),
            Err(DocumentoError::Caracteres)
        );
        assert_eq!(
            Documento::parse("1234"),
            Err(DocumentoError::Tamanho { tamanho: 4 })
        );
        assert_eq!(
            Documento::parse("111.111.111-11"),
            Err(DocumentoError::Repetido)
        );
        assert_eq!(
            Documento::parse("00000000000000"),
            Err(DocumentoError::Repetido)
        );
    }

    #[test]
    fn every_generated_document_validates() {
        for base in ["000000001", "987654321", "246813579"] {
            let cpf = with_check_digits(base);
            assert!(Documento::parse(&cpf).is_ok(), "{cpf}");
        }
        for base in ["000000010001", "ABCDEFGH0001", "Z9Y8X7W60001"] {
            let cnpj = with_check_digits(base);
            assert!(
                matches!(Documento::parse(&cnpj), Ok(Documento::Cnpj(_))),
                "{cnpj}"
            );
        }
    }
}
