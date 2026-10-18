use std::fmt;
use std::str::FromStr;

use crate::documento::{Documento, DocumentoError};

/// Longest e-mail accepted as a Pix key by the DICT (Banco Central).
const MAX_EMAIL: usize = 77;

/// A Pix key, validated and normalized as the DICT stores it.
///
/// | Kind | Accepted input | Sent as |
/// | --- | --- | --- |
/// | CPF / CNPJ | with or without punctuation | digits (and CNPJ letters) |
/// | e-mail | any case | lower case |
/// | phone | `+55 (11) 91234-5678` | `+5511912345678` |
/// | random (EVP) | UUID in any case | lower case |
///
/// Phones must start with `+55`: without it, eleven digits are a CPF.
///
/// ```
/// use inter_pj::pix::ChavePix;
///
/// let chave: ChavePix = "+55 (11) 91234-5678".parse().unwrap();
/// assert_eq!(chave.as_str(), "+5511912345678");
/// assert_eq!(chave.tipo(), "telefone");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ChavePix {
    /// CPF of a person.
    Cpf(String),
    /// CNPJ of a company.
    Cnpj(String),
    /// E-mail address.
    Email(String),
    /// Brazilian mobile phone, `+55` + area code + 9 digits.
    Telefone(String),
    /// Random key (EVP), a UUID.
    Aleatoria(String),
}

impl ChavePix {
    /// Recognizes, validates and normalizes a Pix key.
    ///
    /// # Errors
    ///
    /// Fails when the text is not a valid key of any kind.
    pub fn parse(raw: &str) -> Result<Self, ChavePixError> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err(ChavePixError::Vazia);
        }
        if raw.contains('@') {
            return email(raw);
        }
        if raw.starts_with('+') {
            return telefone(raw);
        }
        if is_uuid(raw) {
            return Ok(Self::Aleatoria(raw.to_ascii_lowercase()));
        }
        match Documento::parse(raw) {
            Ok(Documento::Cpf(cpf)) => Ok(Self::Cpf(cpf)),
            Ok(Documento::Cnpj(cnpj)) => Ok(Self::Cnpj(cnpj)),
            Err(_) if is_local_phone(raw) => Err(ChavePixError::TelefoneSemDdi),
            Err(DocumentoError::Tamanho { .. } | DocumentoError::Caracteres) => {
                Err(ChavePixError::Desconhecida)
            }
            Err(err) => Err(ChavePixError::Documento(err)),
        }
    }

    /// The key as sent to the API.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Cpf(v)
            | Self::Cnpj(v)
            | Self::Email(v)
            | Self::Telefone(v)
            | Self::Aleatoria(v) => v,
        }
    }

    /// Kind of key, in Portuguese (`CPF`, `CNPJ`, `e-mail`, `telefone`, `chave aleatória`).
    pub fn tipo(&self) -> &'static str {
        match self {
            Self::Cpf(_) => "CPF",
            Self::Cnpj(_) => "CNPJ",
            Self::Email(_) => "e-mail",
            Self::Telefone(_) => "telefone",
            Self::Aleatoria(_) => "chave aleatória",
        }
    }
}

impl FromStr for ChavePix {
    type Err = ChavePixError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        Self::parse(raw)
    }
}

impl fmt::Display for ChavePix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why a text is not a valid Pix key.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ChavePixError {
    /// Nothing was given.
    #[error("chave Pix vazia")]
    Vazia,
    /// Not a CPF, CNPJ, e-mail, phone or random key.
    #[error(
        "chave Pix não reconhecida: use CPF, CNPJ, e-mail, telefone (+55DDNNNNNNNNN) ou chave aleatória"
    )]
    Desconhecida,
    /// An invalid e-mail address.
    #[error("e-mail inválido para chave Pix: {0}")]
    Email(&'static str),
    /// An invalid phone number.
    #[error("telefone inválido para chave Pix: use +55, o DDD e o celular com 9 dígitos")]
    Telefone,
    /// A mobile number without the country code, which is not a valid CPF.
    #[error(
        "chave Pix inválida: parece um celular sem o código do país; use +55 e o DDD (+55DD9NNNNNNNN)"
    )]
    TelefoneSemDdi,
    /// A CPF or CNPJ with wrong check digits.
    #[error(transparent)]
    Documento(DocumentoError),
}

fn email(raw: &str) -> Result<ChavePix, ChavePixError> {
    let email = raw.to_lowercase();
    if email.chars().count() > MAX_EMAIL {
        return Err(ChavePixError::Email("no máximo 77 caracteres"));
    }
    if email.chars().any(char::is_whitespace) {
        return Err(ChavePixError::Email("não pode ter espaços"));
    }
    let Some((local, domain)) = email.split_once('@') else {
        return Err(ChavePixError::Email("falta o @"));
    };
    if local.is_empty() || domain.contains('@') {
        return Err(ChavePixError::Email(
            "use exatamente um @, com o usuário antes dele",
        ));
    }
    // Characters the DICT accepts before the @.
    let local_ok = local
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || ".!#$&'*+/=?^_`{|}~-".contains(c));
    if !local_ok {
        return Err(ChavePixError::Email("caractere não permitido antes do @"));
    }
    let labels: Vec<&str> = domain.split('.').collect();
    let valid_label = |label: &&str| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    };
    if labels.len() < 2 || !labels.iter().all(valid_label) {
        return Err(ChavePixError::Email("domínio inválido"));
    }
    Ok(ChavePix::Email(email))
}

fn telefone(raw: &str) -> Result<ChavePix, ChavePixError> {
    let phone: String = raw
        .chars()
        .filter(|c| !matches!(c, ' ' | '(' | ')' | '-'))
        .collect();
    let digits = &phone[1..];
    let valid = digits.len() == 13
        && digits.bytes().all(|b| b.is_ascii_digit())
        && digits.starts_with("55")
        && digits.as_bytes()[2] != b'0'
        && digits.as_bytes()[4] == b'9';
    if valid {
        Ok(ChavePix::Telefone(phone))
    } else {
        Err(ChavePixError::Telefone)
    }
}

/// Area code and a 9-digit mobile number, without `+55`: `(11) 91234-5678`.
fn is_local_phone(raw: &str) -> bool {
    let digits: Vec<u8> = raw
        .bytes()
        .filter(|b| !matches!(b, b' ' | b'(' | b')' | b'-'))
        .collect();
    digits.len() == 11
        && digits.iter().all(u8::is_ascii_digit)
        && digits[0] != b'0'
        && digits[2] == b'9'
}

/// `8-4-4-4-12` hexadecimal digits.
pub(crate) fn is_uuid(raw: &str) -> bool {
    let groups: Vec<&str> = raw.split('-').collect();
    groups.len() == 5
        && groups.iter().zip([8, 4, 4, 4, 12]).all(|(group, size)| {
            group.len() == size && group.bytes().all(|b| b.is_ascii_hexdigit())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chave(raw: &str) -> ChavePix {
        ChavePix::parse(raw).unwrap_or_else(|err| panic!("{raw}: {err}"))
    }

    #[test]
    fn recognizes_every_kind_of_key() {
        assert_eq!(chave("123.456.789-09"), ChavePix::Cpf("12345678909".into()));
        assert_eq!(
            chave("12.345.678/0001-95"),
            ChavePix::Cnpj("12345678000195".into())
        );
        assert_eq!(
            chave(" Financeiro@Exemplo.COM.br "),
            ChavePix::Email("financeiro@exemplo.com.br".into())
        );
        assert_eq!(
            chave("+55 (11) 91234-5678"),
            ChavePix::Telefone("+5511912345678".into())
        );
        assert_eq!(
            chave("123E4567-E89B-12D3-A456-426655440000"),
            ChavePix::Aleatoria("123e4567-e89b-12d3-a456-426655440000".into())
        );
    }

    #[test]
    fn phones_without_country_code_are_not_taken_as_cpfs() {
        // Eleven digits are a CPF; a phone needs +55.
        assert_eq!(chave("11987654374"), ChavePix::Cpf("11987654374".into()));
        for phone in ["11912345678", "(11) 91234-5678"] {
            assert_eq!(
                ChavePix::parse(phone),
                Err(ChavePixError::TelefoneSemDdi),
                "{phone}"
            );
        }
        assert_eq!(
            ChavePix::parse("123.456.789-00"),
            Err(ChavePixError::Documento(DocumentoError::DigitoVerificador))
        );
    }

    #[test]
    fn rejects_invalid_phones() {
        for phone in [
            "+5511812345678",  // not a mobile number
            "+551191234567",   // one digit short
            "+5501912345678",  // area code starting with 0
            "+1 202 555 0100", // not Brazilian
            "+55119123456789", // one digit too many
        ] {
            assert_eq!(
                ChavePix::parse(phone),
                Err(ChavePixError::Telefone),
                "{phone}"
            );
        }
    }

    #[test]
    fn rejects_invalid_emails() {
        for email in [
            "@exemplo.com",
            "fulano@",
            "fulano@exemplo",
            "fulano@@exemplo.com",
            "ful ano@exemplo.com",
            "fulano@-exemplo.com",
            "fulano@exemplo..com",
            "joão@exemplo.com",
            "fulano(a)@exemplo.com",
        ] {
            assert!(
                matches!(ChavePix::parse(email), Err(ChavePixError::Email(_))),
                "{email}"
            );
        }
        let longo = format!("{}@exemplo.com", "a".repeat(70));
        assert!(matches!(
            ChavePix::parse(&longo),
            Err(ChavePixError::Email(_))
        ));
    }

    #[test]
    fn rejects_unknown_keys() {
        assert_eq!(ChavePix::parse(""), Err(ChavePixError::Vazia));
        assert_eq!(
            ChavePix::parse("fornecedor"),
            Err(ChavePixError::Desconhecida)
        );
        assert_eq!(ChavePix::parse("123-456"), Err(ChavePixError::Desconhecida));
        assert!(!is_uuid("123e4567-e89b-12d3-a456-42665544000g"));
    }

    #[test]
    fn describes_the_kind() {
        assert_eq!(chave("+5511912345678").tipo(), "telefone");
        assert_eq!(chave("12345678909").to_string(), "12345678909");
    }
}
