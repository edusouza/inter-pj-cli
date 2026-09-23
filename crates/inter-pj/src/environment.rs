use std::fmt;
use std::str::FromStr;

/// An Inter Empresas API environment.
///
/// There is deliberately no [`Default`]: talking to production must always be
/// an explicit choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Environment {
    /// Production: real accounts and real money.
    Production,
    /// Sandbox: fictitious data, meant for integration tests.
    Sandbox,
}

impl Environment {
    /// Base URL of the environment, without a trailing slash.
    pub const fn base_url(self) -> &'static str {
        match self {
            Self::Production => "https://cdpj.partners.bancointer.com.br",
            Self::Sandbox => "https://cdpj-sandbox.partners.uatinter.co",
        }
    }

    /// Canonical name, as accepted by [`FromStr`] (`"producao"` or `"sandbox"`).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Production => "producao",
            Self::Sandbox => "sandbox",
        }
    }

    /// Whether this is the production environment.
    pub const fn is_production(self) -> bool {
        matches!(self, Self::Production)
    }
}

impl fmt::Display for Environment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Environment {
    type Err = ParseEnvironmentError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "producao" | "produção" | "production" | "prod" => Ok(Self::Production),
            "sandbox" => Ok(Self::Sandbox),
            _ => Err(ParseEnvironmentError(s.to_owned())),
        }
    }
}

/// Error returned when an environment name is not recognised.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("ambiente inválido: \"{0}\" (use \"sandbox\" ou \"producao\")")]
pub struct ParseEnvironmentError(String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_accepted_names() {
        for name in ["producao", "Produção", "PRODUCTION", " prod "] {
            assert_eq!(name.parse::<Environment>(), Ok(Environment::Production));
        }
        assert_eq!("Sandbox".parse::<Environment>(), Ok(Environment::Sandbox));
    }

    #[test]
    fn rejects_unknown_names_with_helpful_message() {
        let err = "homologacao".parse::<Environment>().unwrap_err();
        assert_eq!(
            err.to_string(),
            "ambiente inválido: \"homologacao\" (use \"sandbox\" ou \"producao\")"
        );
    }

    #[test]
    fn exposes_official_base_urls() {
        assert_eq!(
            Environment::Production.base_url(),
            "https://cdpj.partners.bancointer.com.br"
        );
        assert_eq!(
            Environment::Sandbox.base_url(),
            "https://cdpj-sandbox.partners.uatinter.co"
        );
    }

    #[test]
    fn display_round_trips() {
        for env in [Environment::Production, Environment::Sandbox] {
            assert_eq!(env.to_string().parse::<Environment>(), Ok(env));
        }
    }
}
