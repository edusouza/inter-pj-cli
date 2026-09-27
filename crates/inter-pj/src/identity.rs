use std::fmt::{self, Write as _};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use rustls_pki_types::pem::{self, SectionKind};
use secrecy::{ExposeSecret, SecretSlice};

use crate::certificate::CertificateInfo;

/// Client certificate and private key used for mutual TLS.
///
/// Inter issues a certificate (`.crt`) and a private key (`.key`) in PEM
/// format for each integration. Both are validated when loaded, so that
/// problems surface with a clear message instead of an obscure TLS failure.
/// The key material is zeroed from memory when dropped.
#[derive(Clone)]
pub struct ClientIdentity {
    /// Private key followed by the certificate chain, PEM encoded.
    pem: SecretSlice<u8>,
    certificates: usize,
    /// The leaf certificate, DER encoded: public, read without touching
    /// the key.
    leaf: Vec<u8>,
}

impl ClientIdentity {
    /// Loads and validates the certificate and the private key from files.
    ///
    /// # Errors
    ///
    /// Fails when a file cannot be read or its content is not accepted by
    /// [`from_pem`](Self::from_pem).
    pub fn from_pem_files(
        certificate: impl AsRef<Path>,
        private_key: impl AsRef<Path>,
    ) -> Result<Self, IdentityError> {
        let certificate_pem = read(certificate.as_ref())?;
        let private_key_pem = read(private_key.as_ref())?;
        Self::from_pem(&certificate_pem, &private_key_pem)
    }

    /// Validates PEM encoded certificate chain and private key.
    ///
    /// The private key may be PKCS#8 (`PRIVATE KEY`), PKCS#1 (`RSA PRIVATE
    /// KEY`) or SEC1 (`EC PRIVATE KEY`). Password-protected keys are not
    /// supported.
    ///
    /// # Errors
    ///
    /// Fails when the input is not valid PEM, the certificate or the key is
    /// missing, the key is encrypted, there is more than one key, the files
    /// look swapped or the TLS backend rejects the material.
    pub fn from_pem(certificate_pem: &[u8], private_key_pem: &[u8]) -> Result<Self, IdentityError> {
        let certificate_sections = sections(certificate_pem)?;
        let key_sections = sections(private_key_pem)?;

        let certificates: Vec<&Vec<u8>> = certificate_sections
            .iter()
            .filter(|(kind, _)| *kind == SectionKind::Certificate)
            .map(|(_, der)| der)
            .collect();
        let keys: Vec<(SectionKind, &Vec<u8>)> = key_sections
            .iter()
            .filter(|(kind, _)| is_private_key(*kind))
            .map(|(kind, der)| (*kind, der))
            .collect();

        if certificates.is_empty() {
            let swapped = certificate_sections
                .iter()
                .any(|(kind, _)| is_private_key(*kind))
                && key_sections
                    .iter()
                    .any(|(kind, _)| *kind == SectionKind::Certificate);
            return Err(if swapped {
                IdentityError::Swapped
            } else {
                IdentityError::NoCertificate
            });
        }
        let (key_kind, key_der) = match keys.as_slice() {
            [] if is_encrypted(private_key_pem) => return Err(IdentityError::EncryptedPrivateKey),
            [] => return Err(IdentityError::NoPrivateKey),
            [key] => *key,
            _ => return Err(IdentityError::MultiplePrivateKeys),
        };

        let mut combined = encode(key_kind, key_der);
        for der in &certificates {
            combined.push_str(&encode(SectionKind::Certificate, der));
        }

        let identity = Self {
            pem: SecretSlice::from(combined.into_bytes()),
            certificates: certificates.len(),
            leaf: certificates[0].clone(),
        };
        // Fail early if the TLS backend does not accept the material.
        identity.to_reqwest()?;
        Ok(identity)
    }

    /// Number of certificates in the chain (leaf first).
    pub fn certificate_count(&self) -> usize {
        self.certificates
    }

    /// The leaf certificate: whom it identifies, who issued it and its
    /// validity.
    ///
    /// # Errors
    ///
    /// [`IdentityError::InvalidCertificate`] when it cannot be read.
    pub fn certificate(&self) -> Result<CertificateInfo, IdentityError> {
        CertificateInfo::from_der(&self.leaf)
    }

    pub(crate) fn to_reqwest(&self) -> Result<reqwest::Identity, IdentityError> {
        reqwest::Identity::from_pem(self.pem.expose_secret())
            .map_err(|err| IdentityError::Tls(err.to_string()))
    }
}

impl fmt::Debug for ClientIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClientIdentity")
            .field("certificates", &self.certificates)
            .field("private_key", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

/// Error loading the mTLS certificate or private key.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum IdentityError {
    /// A file could not be read.
    #[error("não foi possível ler o arquivo {}: {source}", path.display())]
    Read {
        /// File that could not be read.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },
    /// The content is not valid PEM.
    #[error("conteúdo PEM inválido: {0}")]
    InvalidPem(String),
    /// No `CERTIFICATE` section was found.
    #[error(
        "o arquivo de certificado não contém um certificado PEM (\"-----BEGIN CERTIFICATE-----\")"
    )]
    NoCertificate,
    /// No private key section was found.
    #[error(
        "o arquivo de chave privada não contém uma chave PEM (\"PRIVATE KEY\", \"RSA PRIVATE KEY\" ou \"EC PRIVATE KEY\")"
    )]
    NoPrivateKey,
    /// Certificate and key files appear to be swapped.
    #[error(
        "o certificado e a chave privada parecem estar trocados; confira os caminhos informados"
    )]
    Swapped,
    /// The private key is password protected.
    #[error(
        "a chave privada está protegida por senha, o que não é suportado; gere uma cópia sem senha com: openssl pkey -in chave.key -out chave-sem-senha.key"
    )]
    EncryptedPrivateKey,
    /// More than one private key was found.
    #[error("o arquivo de chave privada contém mais de uma chave")]
    MultiplePrivateKeys,
    /// The TLS backend rejected the certificate or key.
    #[error("certificado ou chave rejeitados pela biblioteca TLS: {0}")]
    Tls(String),
    /// The certificate is not a valid X.509 structure: the part that failed.
    #[error("certificado inválido: não foi possível ler {0}")]
    InvalidCertificate(&'static str),
}

fn read(path: &Path) -> Result<Vec<u8>, IdentityError> {
    fs::read(path).map_err(|source| IdentityError::Read {
        path: path.to_path_buf(),
        source,
    })
}

fn sections(pem_bytes: &[u8]) -> Result<Vec<(SectionKind, Vec<u8>)>, IdentityError> {
    let mut reader = io::BufReader::new(pem_bytes);
    let mut found = Vec::new();
    loop {
        match pem::from_buf(&mut reader) {
            Ok(Some(section)) => found.push(section),
            Ok(None) => return Ok(found),
            Err(err) => return Err(IdentityError::InvalidPem(err.to_string())),
        }
    }
}

/// The DER of the first certificate of a PEM file, the leaf of the chain.
pub(crate) fn first_certificate(pem_bytes: &[u8]) -> Result<Vec<u8>, IdentityError> {
    sections(pem_bytes)?
        .into_iter()
        .find(|(kind, _)| *kind == SectionKind::Certificate)
        .map(|(_, der)| der)
        .ok_or(IdentityError::NoCertificate)
}

fn is_private_key(kind: SectionKind) -> bool {
    matches!(
        kind,
        SectionKind::PrivateKey | SectionKind::RsaPrivateKey | SectionKind::EcPrivateKey
    )
}

fn is_encrypted(pem_bytes: &[u8]) -> bool {
    let text = String::from_utf8_lossy(pem_bytes);
    text.contains("ENCRYPTED PRIVATE KEY") || text.contains("Proc-Type: 4,ENCRYPTED")
}

fn encode(kind: SectionKind, der: &[u8]) -> String {
    let label = match kind {
        SectionKind::Certificate => "CERTIFICATE",
        SectionKind::RsaPrivateKey => "RSA PRIVATE KEY",
        SectionKind::EcPrivateKey => "EC PRIVATE KEY",
        _ => "PRIVATE KEY",
    };
    let body = BASE64.encode(der);
    let mut out = format!("-----BEGIN {label}-----\n");
    for line in body.as_bytes().chunks(64) {
        // Base64 output is always ASCII.
        out.push_str(std::str::from_utf8(line).unwrap_or_default());
        out.push('\n');
    }
    let _ = writeln!(out, "-----END {label}-----");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generated() -> (String, String) {
        let rcgen::CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed(vec!["cliente.teste".to_owned()]).unwrap();
        (cert.pem(), signing_key.serialize_pem())
    }

    #[test]
    fn accepts_valid_certificate_and_key() {
        let (cert, key) = generated();
        let identity = ClientIdentity::from_pem(cert.as_bytes(), key.as_bytes()).unwrap();
        assert_eq!(identity.certificate_count(), 1);
    }

    #[test]
    fn accepts_files_with_both_sections_in_any_order() {
        let (cert, key) = generated();
        let bundle = format!("{key}{cert}");
        ClientIdentity::from_pem(bundle.as_bytes(), bundle.as_bytes()).unwrap();
    }

    #[test]
    fn detects_swapped_files() {
        let (cert, key) = generated();
        let err = ClientIdentity::from_pem(key.as_bytes(), cert.as_bytes()).unwrap_err();
        assert!(matches!(err, IdentityError::Swapped), "{err:?}");
    }

    #[test]
    fn detects_missing_certificate() {
        let err = ClientIdentity::from_pem(b"nada aqui", b"nem aqui").unwrap_err();
        assert!(matches!(err, IdentityError::NoCertificate), "{err:?}");
    }

    #[test]
    fn detects_missing_key() {
        let (cert, _) = generated();
        let err = ClientIdentity::from_pem(cert.as_bytes(), cert.as_bytes()).unwrap_err();
        assert!(matches!(err, IdentityError::NoPrivateKey), "{err:?}");
    }

    #[test]
    fn detects_encrypted_key() {
        let (cert, _) = generated();
        let encrypted = "-----BEGIN ENCRYPTED PRIVATE KEY-----\nMIIBAA==\n-----END ENCRYPTED PRIVATE KEY-----\n";
        let err = ClientIdentity::from_pem(cert.as_bytes(), encrypted.as_bytes()).unwrap_err();
        assert!(matches!(err, IdentityError::EncryptedPrivateKey), "{err:?}");
        assert!(err.to_string().contains("openssl pkey"));
    }

    #[test]
    fn detects_multiple_keys() {
        let (cert, key) = generated();
        let (_, other_key) = generated();
        let keys = format!("{key}{other_key}");
        let err = ClientIdentity::from_pem(cert.as_bytes(), keys.as_bytes()).unwrap_err();
        assert!(matches!(err, IdentityError::MultiplePrivateKeys), "{err:?}");
    }

    #[test]
    fn reports_unreadable_file_with_path() {
        let err = ClientIdentity::from_pem_files("/nao/existe.crt", "/nao/existe.key").unwrap_err();
        assert!(err.to_string().contains("/nao/existe.crt"), "{err}");
    }

    #[test]
    fn the_leaf_certificate_is_read_without_the_key() {
        let (cert, key) = generated();
        let identity = ClientIdentity::from_pem(cert.as_bytes(), key.as_bytes()).unwrap();
        let certificado = identity.certificate().unwrap();
        assert_eq!(
            certificado,
            CertificateInfo::from_pem(cert.as_bytes()).unwrap()
        );
        assert!(certificado.not_before < certificado.not_after);
    }

    #[test]
    fn debug_hides_key_material() {
        let (cert, key) = generated();
        let identity = ClientIdentity::from_pem(cert.as_bytes(), key.as_bytes()).unwrap();
        let debug = format!("{identity:?}");
        assert!(!debug.contains("BEGIN"));
        assert!(debug.contains("[REDACTED]"));
    }
}
