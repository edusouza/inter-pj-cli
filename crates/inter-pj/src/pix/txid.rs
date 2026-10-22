use std::fmt::{self, Write as _};
use std::str::FromStr;

use serde::{Serialize, Serializer};

/// Shortest [`Txid`].
pub const TXID_MINIMO: usize = 26;

/// Longest [`Txid`].
pub const TXID_MAXIMO: usize = 35;

/// Identifier of a Pix charge (`txid`): 26 to 35 letters and digits, chosen
/// by the receiver. The API does not create a second charge with a txid
/// already used, so a creation whose answer was lost can be repeated with the
/// same txid.
///
/// ```
/// use inter_pj::pix::Txid;
///
/// let txid: Txid = " 7978c0c97ea847e78e8849634473c1f1 ".parse().unwrap();
/// assert_eq!(txid.as_str(), "7978c0c97ea847e78e8849634473c1f1");
/// assert!("curto".parse::<Txid>().is_err());
/// assert!("7978c0c9-7ea8-47e7-8e88-49634473c1f1".parse::<Txid>().is_err());
/// assert_eq!(Txid::novo().as_str().len(), 32);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Txid(String);

impl Txid {
    /// A new random txid: 32 hexadecimal digits in lower case.
    ///
    /// # Panics
    ///
    /// Never in practice: AWS-LC aborts the process itself when the operating
    /// system cannot provide random bytes.
    pub fn novo() -> Self {
        let mut bytes = [0u8; 16];
        aws_lc_rs::rand::fill(&mut bytes)
            .expect("o sistema não forneceu bytes aleatórios para o txid");
        let mut txid = String::with_capacity(32);
        for byte in bytes {
            let _ = write!(txid, "{byte:02x}");
        }
        Self(txid)
    }

    /// The txid as sent.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for Txid {
    type Err = TxidError;

    /// Accepts 26 to 35 ASCII letters and digits, keeping their case.
    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let txid = raw.trim();
        if (TXID_MINIMO..=TXID_MAXIMO).contains(&txid.len())
            && txid.bytes().all(|b| b.is_ascii_alphanumeric())
        {
            Ok(Self(txid.to_owned()))
        } else {
            Err(TxidError)
        }
    }
}

impl fmt::Display for Txid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for Txid {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

/// A text that is not a [`Txid`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("txid inválido: use de 26 a 35 letras e dígitos, sem acentos, espaços, hífens ou símbolos")]
pub struct TxidError;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn txids_have_26_to_35_letters_and_digits() {
        for valido in [
            "a".repeat(26),
            "Z9".repeat(17) + "x",
            "7978c0c97ea847e78e8849634473c1f1".to_owned(),
        ] {
            assert_eq!(valido.parse::<Txid>().unwrap().as_str(), valido);
        }
        for invalido in [
            "a".repeat(25),
            "a".repeat(36),
            "7978c0c97ea847e78e8849634473c1f_".to_owned(),
            "ação".repeat(7),
            "7978c0c97ea847e7 8e8849634473c1f1".to_owned(),
            String::new(),
        ] {
            assert_eq!(invalido.parse::<Txid>(), Err(TxidError), "{invalido}");
        }
    }

    #[test]
    fn new_txids_are_random_and_valid() {
        let um = Txid::novo();
        assert_eq!(um.as_str().parse::<Txid>().unwrap(), um);
        assert_ne!(um, Txid::novo());
        assert!(
            um.as_str()
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        );
    }
}
