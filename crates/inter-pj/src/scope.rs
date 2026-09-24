use std::collections::BTreeSet;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

macro_rules! scopes {
    ($($variant:ident => $name:literal,)*) => {
        /// An OAuth scope of the Inter Empresas APIs.
        ///
        /// Each operation requires specific scopes, which must also be enabled
        /// for the integration in Internet Banking PJ.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[non_exhaustive]
        pub enum Scope {
            $(
                #[doc = concat!("`", $name, "`")]
                $variant,
            )*
        }

        impl Scope {
            /// Every scope documented by the Inter Empresas APIs.
            pub const ALL: &'static [Scope] = &[$(Scope::$variant,)*];

            /// The scope name as used by the API (e.g. `"extrato.read"`).
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Scope::$variant => $name,)*
                }
            }
        }

        impl FromStr for Scope {
            type Err = UnknownScopeError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s.trim() {
                    $($name => Ok(Scope::$variant),)*
                    other => Err(UnknownScopeError(other.to_owned())),
                }
            }
        }
    };
}

scopes! {
    ExtratoRead => "extrato.read",
    BoletoCobrancaRead => "boleto-cobranca.read",
    BoletoCobrancaWrite => "boleto-cobranca.write",
    PagamentoBoletoRead => "pagamento-boleto.read",
    PagamentoBoletoWrite => "pagamento-boleto.write",
    PagamentoDarfWrite => "pagamento-darf.write",
    PagamentoLoteRead => "pagamento-lote.read",
    PagamentoLoteWrite => "pagamento-lote.write",
    PagamentoPixRead => "pagamento-pix.read",
    PagamentoPixWrite => "pagamento-pix.write",
    WebhookBankingRead => "webhook-banking.read",
    WebhookBankingWrite => "webhook-banking.write",
    CobRead => "cob.read",
    CobWrite => "cob.write",
    CobvRead => "cobv.read",
    CobvWrite => "cobv.write",
    LoteCobvRead => "lotecobv.read",
    LoteCobvWrite => "lotecobv.write",
    PayloadLocationRead => "payloadlocation.read",
    PayloadLocationWrite => "payloadlocation.write",
    PixRead => "pix.read",
    PixWrite => "pix.write",
    WebhookRead => "webhook.read",
    WebhookWrite => "webhook.write",
    RecRead => "rec.read",
    RecWrite => "rec.write",
    SolicRecRead => "solicrec.read",
    SolicRecWrite => "solicrec.write",
    CobrRead => "cobr.read",
    CobrWrite => "cobr.write",
    PayloadLocationRecRead => "payloadlocationrec.read",
    PayloadLocationRecWrite => "payloadlocationrec.write",
    WebhookRecRead => "webhookrec.read",
    WebhookRecWrite => "webhookrec.write",
    WebhookCobrRead => "webhookcobr.read",
    WebhookCobrWrite => "webhookcobr.write",
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Error returned when a scope name is not one of the documented scopes.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("escopo desconhecido: \"{0}\"")]
pub struct UnknownScopeError(String);

/// An ordered set of scopes.
///
/// Formats (and parses) as the space-separated list used by the OAuth token
/// endpoint. Parsing also accepts commas as separators.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct ScopeSet(BTreeSet<Scope>);

impl ScopeSet {
    /// Creates an empty set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Parses a list of scopes, silently ignoring names that are not known.
    ///
    /// Used for data coming from the server (e.g. the `scope` field of a
    /// token response), which may contain scopes this crate does not know yet.
    #[must_use]
    pub fn parse_lenient(s: &str) -> Self {
        split(s).filter_map(|name| name.parse().ok()).collect()
    }

    /// Adds a scope; returns whether it was not already present.
    pub fn insert(&mut self, scope: Scope) -> bool {
        self.0.insert(scope)
    }

    /// Whether the set contains `scope`.
    pub fn contains(&self, scope: Scope) -> bool {
        self.0.contains(&scope)
    }

    /// Whether every scope of `other` is also in `self`.
    pub fn is_superset(&self, other: &ScopeSet) -> bool {
        self.0.is_superset(&other.0)
    }

    /// Scopes of `other` that are missing from `self`.
    #[must_use]
    pub fn missing_from(&self, other: &ScopeSet) -> ScopeSet {
        other.0.difference(&self.0).copied().collect()
    }

    /// Union of both sets.
    #[must_use]
    pub fn union(&self, other: &ScopeSet) -> ScopeSet {
        self.0.union(&other.0).copied().collect()
    }

    /// Whether the set is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Number of scopes in the set.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Iterates over the scopes in a stable order.
    pub fn iter(&self) -> impl Iterator<Item = Scope> + '_ {
        self.0.iter().copied()
    }
}

fn split(s: &str) -> impl Iterator<Item = &str> {
    s.split(|c: char| c.is_whitespace() || c == ',')
        .filter(|name| !name.is_empty())
}

impl fmt::Display for ScopeSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, scope) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str(" ")?;
            }
            f.write_str(scope.as_str())?;
        }
        Ok(())
    }
}

impl FromStr for ScopeSet {
    type Err = UnknownScopeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        split(s).map(str::parse).collect()
    }
}

impl FromIterator<Scope> for ScopeSet {
    fn from_iter<I: IntoIterator<Item = Scope>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl Extend<Scope> for ScopeSet {
    fn extend<I: IntoIterator<Item = Scope>>(&mut self, iter: I) {
        self.0.extend(iter);
    }
}

impl From<Scope> for ScopeSet {
    fn from(scope: Scope) -> Self {
        std::iter::once(scope).collect()
    }
}

impl From<&[Scope]> for ScopeSet {
    fn from(scopes: &[Scope]) -> Self {
        scopes.iter().copied().collect()
    }
}

impl Serialize for ScopeSet {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ScopeSet {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Ok(Self::parse_lenient(&raw))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_scope_round_trips_through_its_name() {
        for scope in Scope::ALL {
            assert_eq!(scope.as_str().parse::<Scope>(), Ok(*scope));
        }
        assert_eq!(Scope::ALL.len(), 36);
    }

    #[test]
    fn set_formats_as_space_separated_list_in_stable_order() {
        let set: ScopeSet = [Scope::PixRead, Scope::ExtratoRead].into_iter().collect();
        assert_eq!(set.to_string(), "extrato.read pix.read");
    }

    #[test]
    fn parses_space_and_comma_separated_lists() {
        let set: ScopeSet = "extrato.read, pix.read  cob.write".parse().unwrap();
        assert_eq!(set.len(), 3);
        assert!(set.contains(Scope::CobWrite));
    }

    #[test]
    fn strict_parse_rejects_unknown_scope() {
        let err = "extrato.read banana.read".parse::<ScopeSet>().unwrap_err();
        assert_eq!(err.to_string(), "escopo desconhecido: \"banana.read\"");
    }

    #[test]
    fn lenient_parse_ignores_unknown_scope() {
        let set = ScopeSet::parse_lenient("extrato.read banana.read");
        assert_eq!(set, ScopeSet::from(Scope::ExtratoRead));
    }

    #[test]
    fn superset_and_missing() {
        let granted: ScopeSet = "extrato.read pix.read".parse().unwrap();
        let wanted: ScopeSet = "extrato.read cob.read".parse().unwrap();
        assert!(!granted.is_superset(&wanted));
        assert_eq!(
            granted.missing_from(&wanted),
            ScopeSet::from(Scope::CobRead)
        );
        assert!(granted.union(&wanted).is_superset(&wanted));
    }
}
