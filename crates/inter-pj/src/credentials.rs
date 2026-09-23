use std::fmt;

use secrecy::SecretString;

/// OAuth client credentials of an Inter Empresas integration.
///
/// Both values are shown only once, when the integration is created in
/// Internet Banking PJ. The [`Debug`] implementation never reveals them.
#[derive(Clone)]
pub struct Credentials {
    client_id: String,
    client_secret: SecretString,
}

impl Credentials {
    /// Creates credentials from the integration's `client_id` and `client_secret`.
    pub fn new(client_id: impl Into<String>, client_secret: impl Into<String>) -> Self {
        Self {
            client_id: client_id.into(),
            client_secret: SecretString::from(client_secret.into()),
        }
    }

    /// The integration's `client_id`.
    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    pub(crate) fn client_secret(&self) -> &SecretString {
        &self.client_secret
    }
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials")
            .field("client_id", &"[REDACTED]")
            .field("client_secret", &"[REDACTED]")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use secrecy::ExposeSecret;

    use super::*;

    #[test]
    fn debug_never_reveals_credentials() {
        let credentials = Credentials::new("id-visivel-nao", "segredo-muito-secreto");
        let debug = format!("{credentials:?}");
        assert!(!debug.contains("id-visivel-nao"));
        assert!(!debug.contains("segredo-muito-secreto"));
        assert!(debug.contains("[REDACTED]"));
    }

    #[test]
    fn keeps_values() {
        let credentials = Credentials::new("id", "segredo");
        assert_eq!(credentials.client_id(), "id");
        assert_eq!(credentials.client_secret().expose_secret(), "segredo");
    }
}
