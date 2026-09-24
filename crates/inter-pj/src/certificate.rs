//! What a client certificate says about itself: whom it identifies, who
//! issued it and when it is valid. Read from its DER encoding (X.509) with a
//! small reader of the few fields shown, instead of a full ASN.1 parser.

use std::fmt::{self, Write as _};

use chrono::{DateTime, NaiveDateTime, Utc};

use crate::identity::{IdentityError, first_certificate};

const INTEGER: u8 = 0x02;
const OID: u8 = 0x06;
const SEQUENCE: u8 = 0x30;
const SET: u8 = 0x31;
const UTC_TIME: u8 = 0x17;
const GENERALIZED_TIME: u8 = 0x18;
/// `[0] EXPLICIT Version` of the `TBSCertificate`.
const VERSION: u8 = 0xA0;

/// The certificate of an integration, as far as it is shown: whom it
/// identifies, who issued it and its validity.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CertificateInfo {
    /// Whom it identifies.
    pub subject: DistinguishedName,
    /// Who issued it.
    pub issuer: DistinguishedName,
    /// Serial number, in hexadecimal.
    pub serial: String,
    /// Valid from.
    pub not_before: DateTime<Utc>,
    /// Valid until.
    pub not_after: DateTime<Utc>,
}

impl CertificateInfo {
    /// The first certificate of a PEM file, the leaf of the chain.
    ///
    /// # Errors
    ///
    /// When the content is not PEM, has no certificate or the certificate is
    /// not valid DER.
    pub fn from_pem(pem: &[u8]) -> Result<Self, IdentityError> {
        Self::from_der(&first_certificate(pem)?)
    }

    /// A certificate encoded in DER.
    ///
    /// # Errors
    ///
    /// [`IdentityError::InvalidCertificate`] when the structure is not that
    /// of an X.509 certificate.
    pub fn from_der(der: &[u8]) -> Result<Self, IdentityError> {
        let mut certificado = Leitor::new(Leitor::new(der).esperado(SEQUENCE, "certificado")?);
        let mut tbs = Leitor::new(certificado.esperado(SEQUENCE, "tbsCertificate")?);
        let (mut tag, mut conteudo) = tbs.proximo()?;
        if tag == VERSION {
            (tag, conteudo) = tbs.proximo()?;
        }
        if tag != INTEGER {
            return Err(invalido("número de série"));
        }
        let serial = hexadecimal(conteudo);
        tbs.esperado(SEQUENCE, "algoritmo da assinatura")?;
        let issuer = nome(tbs.esperado(SEQUENCE, "emissor")?)?;
        let mut validade = Leitor::new(tbs.esperado(SEQUENCE, "validade")?);
        let not_before = momento(validade.proximo()?)?;
        let not_after = momento(validade.proximo()?)?;
        let subject = nome(tbs.esperado(SEQUENCE, "titular")?)?;
        Ok(Self {
            subject,
            issuer,
            serial,
            not_before,
            not_after,
        })
    }
}

/// A distinguished name: its attributes (`CN`, `O`, `C`...), in the order of
/// the certificate.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DistinguishedName(Vec<(String, String)>);

impl DistinguishedName {
    /// The attributes, as (name, value), in the order of the certificate;
    /// unknown ones by their OID.
    pub fn attributes(&self) -> &[(String, String)] {
        &self.0
    }

    /// The common name (`CN`), when there is one.
    pub fn common_name(&self) -> Option<&str> {
        self.0
            .iter()
            .find(|(nome, _)| nome == "CN")
            .map(|(_, valor)| valor.as_str())
    }
}

/// As RFC 4514 writes it: the last attribute first, `CN=Empresa, O=Banco,
/// C=BR`.
impl fmt::Display for DistinguishedName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, (nome, valor)) in self.0.iter().rev().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{nome}=")?;
            for c in valor.chars() {
                if matches!(c, ',' | '+' | '"' | '\\' | '<' | '>' | ';') {
                    f.write_str("\\")?;
                }
                write!(f, "{c}")?;
            }
        }
        Ok(())
    }
}

fn invalido(parte: &'static str) -> IdentityError {
    IdentityError::InvalidCertificate(parte)
}

/// The elements of a DER encoded content, one after the other.
struct Leitor<'a> {
    resto: &'a [u8],
}

impl<'a> Leitor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { resto: bytes }
    }

    fn vazio(&self) -> bool {
        self.resto.is_empty()
    }

    /// The next element: its tag and its content.
    fn proximo(&mut self) -> Result<(u8, &'a [u8]), IdentityError> {
        let truncado = || invalido("conteúdo truncado");
        let (&tag, resto) = self.resto.split_first().ok_or_else(truncado)?;
        let (&primeiro, resto) = resto.split_first().ok_or_else(truncado)?;
        let (tamanho, resto) = if primeiro < 0x80 {
            (usize::from(primeiro), resto)
        } else {
            // The long form, with up to 4 bytes; 0x80 (indefinite) is not
            // DER.
            let bytes = usize::from(primeiro & 0x7f);
            if bytes == 0 || bytes > 4 {
                return Err(invalido("tamanho de um elemento"));
            }
            let (tamanho, resto) = resto.split_at_checked(bytes).ok_or_else(truncado)?;
            let tamanho = tamanho
                .iter()
                .fold(0_usize, |total, &byte| (total << 8) | usize::from(byte));
            (tamanho, resto)
        };
        let (conteudo, resto) = resto.split_at_checked(tamanho).ok_or_else(truncado)?;
        self.resto = resto;
        Ok((tag, conteudo))
    }

    /// The content of the next element, which must have `tag`.
    fn esperado(&mut self, tag: u8, parte: &'static str) -> Result<&'a [u8], IdentityError> {
        match self.proximo()? {
            (lido, conteudo) if lido == tag => Ok(conteudo),
            _ => Err(invalido(parte)),
        }
    }
}

/// `0A1B2C`, without the zero that keeps a DER integer positive.
fn hexadecimal(inteiro: &[u8]) -> String {
    let significativos = match inteiro {
        [0, resto @ ..] if !resto.is_empty() => resto,
        todos => todos,
    };
    significativos
        .iter()
        .fold(String::new(), |mut texto, byte| {
            let _ = write!(texto, "{byte:02X}");
            texto
        })
}

/// A `Name`: a sequence of sets of (OID, value).
fn nome(conteudo: &[u8]) -> Result<DistinguishedName, IdentityError> {
    let mut rdns = Leitor::new(conteudo);
    let mut atributos = Vec::new();
    while !rdns.vazio() {
        let mut conjunto = Leitor::new(rdns.esperado(SET, "nome")?);
        while !conjunto.vazio() {
            let mut par = Leitor::new(conjunto.esperado(SEQUENCE, "nome")?);
            let oid = par.esperado(OID, "nome")?;
            let (tag, valor) = par.proximo()?;
            atributos.push((nome_do_atributo(oid), texto(tag, valor)));
        }
    }
    Ok(DistinguishedName(atributos))
}

/// `CN`, `O`... or the OID of the attributes without a short name.
fn nome_do_atributo(oid: &[u8]) -> String {
    let curto = match oid {
        [0x55, 0x04, 0x03] => "CN",
        [0x55, 0x04, 0x05] => "serialNumber",
        [0x55, 0x04, 0x06] => "C",
        [0x55, 0x04, 0x07] => "L",
        [0x55, 0x04, 0x08] => "ST",
        [0x55, 0x04, 0x09] => "street",
        [0x55, 0x04, 0x0A] => "O",
        [0x55, 0x04, 0x0B] => "OU",
        [0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x01] => "emailAddress",
        outro => return oid_pontilhado(outro),
    };
    curto.to_owned()
}

/// `2.5.4.97`: the subidentifiers in base 128, the first of them holding
/// the first two arcs.
fn oid_pontilhado(oid: &[u8]) -> String {
    let mut subidentificadores = Vec::new();
    let mut atual = 0_u64;
    for &byte in oid {
        atual = (atual << 7) | u64::from(byte & 0x7f);
        if byte & 0x80 == 0 {
            subidentificadores.push(atual);
            atual = 0;
        }
    }
    let Some((&primeiro, resto)) = subidentificadores.split_first() else {
        return String::new();
    };
    let arco = primeiro.min(80) / 40;
    let mut partes = vec![arco, primeiro - arco * 40];
    partes.extend_from_slice(resto);
    partes
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(".")
}

/// The value of an attribute in whichever string type it came.
fn texto(tag: u8, valor: &[u8]) -> String {
    match tag {
        // UTF8String, NumericString, PrintableString, IA5String.
        0x0C | 0x12 | 0x13 | 0x16 => String::from_utf8_lossy(valor).into_owned(),
        // TeletexString, read as Latin-1, as most issuers write it.
        0x14 => valor.iter().map(|&byte| char::from(byte)).collect(),
        // BMPString: UTF-16, big endian.
        0x1E => {
            let unidades: Vec<u16> = valor
                .as_chunks::<2>()
                .0
                .iter()
                .map(|par| u16::from_be_bytes(*par))
                .collect();
            String::from_utf16_lossy(&unidades)
        }
        // UniversalString: UTF-32, big endian.
        0x1C => valor
            .as_chunks::<4>()
            .0
            .iter()
            .map(|quatro| {
                char::from_u32(u32::from_be_bytes(*quatro)).unwrap_or(char::REPLACEMENT_CHARACTER)
            })
            .collect(),
        // Anything else as RFC 4514 writes it: `#` and the DER in hex.
        _ => format!("#{}", hexadecimal(valor)),
    }
}

/// A `Time`: `UTCTime` (`YYMMDDHHMMSSZ`, years from 1950 to 2049) or
/// `GeneralizedTime` (`YYYYMMDDHHMMSSZ`).
fn momento((tag, valor): (u8, &[u8])) -> Result<DateTime<Utc>, IdentityError> {
    let sem_z = valor
        .strip_suffix(b"Z")
        .filter(|digitos| digitos.iter().all(u8::is_ascii_digit))
        .and_then(|digitos| std::str::from_utf8(digitos).ok())
        .ok_or_else(|| invalido("validade"))?;
    let completo = match tag {
        UTC_TIME if sem_z.len() == 12 => {
            let ano: u32 = sem_z[..2].parse().map_err(|_| invalido("validade"))?;
            let seculo = if ano < 50 { "20" } else { "19" };
            format!("{seculo}{sem_z}")
        }
        GENERALIZED_TIME if sem_z.len() == 14 => sem_z.to_owned(),
        _ => return Err(invalido("validade")),
    };
    NaiveDateTime::parse_from_str(&completo, "%Y%m%d%H%M%S")
        .map(|momento| momento.and_utc())
        .map_err(|_| invalido("validade"))
}

#[cfg(test)]
mod tests {
    use rcgen::{CertificateParams, DnType, KeyPair, date_time_ymd};

    use super::*;

    fn gerado(inicio: (i32, u8, u8), fim: (i32, u8, u8)) -> String {
        let mut params = CertificateParams::new(vec!["cliente.teste".to_owned()]).unwrap();
        params.not_before = date_time_ymd(inicio.0, inicio.1, inicio.2);
        params.not_after = date_time_ymd(fim.0, fim.1, fim.2);
        // `new` fills in a CN of its own, which `push` would keep in place.
        params.distinguished_name = rcgen::DistinguishedName::new();
        params.distinguished_name.push(DnType::CountryName, "BR");
        params
            .distinguished_name
            .push(DnType::OrganizationName, "Empresa Exemplo Ltda");
        params
            .distinguished_name
            .push(DnType::CommonName, "Integração, Exemplo");
        let chave = KeyPair::generate().unwrap();
        params.self_signed(&chave).unwrap().pem()
    }

    #[test]
    fn reads_names_serial_and_validity() {
        let pem = gerado((2026, 9, 24), (2027, 9, 24));
        let info = CertificateInfo::from_pem(pem.as_bytes()).unwrap();
        assert_eq!(info.subject.common_name(), Some("Integração, Exemplo"));
        assert_eq!(
            info.subject.to_string(),
            "CN=Integração\\, Exemplo, O=Empresa Exemplo Ltda, C=BR"
        );
        // Self-signed: the issuer is the subject.
        assert_eq!(info.issuer, info.subject);
        assert_eq!(info.not_before.to_rfc3339(), "2026-09-24T00:00:00+00:00");
        assert_eq!(info.not_after.to_rfc3339(), "2027-09-24T00:00:00+00:00");
        assert!(!info.serial.is_empty());
        assert!(info.serial.bytes().all(|b| b.is_ascii_hexdigit()));
        // After 2049, the dates are GeneralizedTime.
        let longo =
            CertificateInfo::from_pem(gerado((2026, 1, 1), (2099, 12, 31)).as_bytes()).unwrap();
        assert_eq!(longo.not_after.to_rfc3339(), "2099-12-31T00:00:00+00:00");
    }

    #[test]
    fn strings_and_oids_of_other_kinds() {
        assert_eq!(texto(0x14, b"S\xe3o Paulo"), "São Paulo");
        assert_eq!(texto(0x1E, &[0x00, 0x53, 0x00, 0xE3, 0x00, 0x6F]), "São");
        assert_eq!(texto(0x04, &[0xAB, 0xCD]), "#ABCD");
        // organizationIdentifier.
        assert_eq!(nome_do_atributo(&[0x55, 0x04, 0x61]), "2.5.4.97");
        assert_eq!(
            oid_pontilhado(&[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x02]),
            "1.2.840.113549.1.9.2"
        );
        // Joint ISO/ITU arcs: the first subidentifier takes two bytes.
        assert_eq!(oid_pontilhado(&[0x88, 0x37, 0x03]), "2.999.3");
        assert_eq!(hexadecimal(&[0x00, 0x8F, 0x01]), "8F01");
        assert_eq!(hexadecimal(&[0x00]), "00");
    }

    #[test]
    fn what_is_not_a_certificate_is_refused() {
        let pem = gerado((2026, 9, 24), (2027, 9, 24));
        let der = first_certificate(pem.as_bytes()).unwrap();
        for tamanho in [0, 1, 2, 10, der.len() / 2, der.len() - 1] {
            assert!(
                CertificateInfo::from_der(&der[..tamanho]).is_err(),
                "{tamanho}"
            );
        }
        // An indefinite length is not DER.
        assert!(CertificateInfo::from_der(&[0x30, 0x80, 0x00, 0x00]).is_err());
        assert!(matches!(
            CertificateInfo::from_pem(b"nada"),
            Err(IdentityError::NoCertificate)
        ));
        assert!(momento((UTC_TIME, b"260924000000")).is_err());
        // Not digits: refused, never a panic on a bad byte boundary.
        assert!(momento((UTC_TIME, "€600924000000Z".as_bytes())).is_err());
        assert!(momento((GENERALIZED_TIME, b"20260924000000+0300")).is_err());
        assert_eq!(
            momento((UTC_TIME, b"490101000000Z")).unwrap().to_rfc3339(),
            "2049-01-01T00:00:00+00:00"
        );
        assert_eq!(
            momento((UTC_TIME, b"500101000000Z")).unwrap().to_rfc3339(),
            "1950-01-01T00:00:00+00:00"
        );
    }
}
