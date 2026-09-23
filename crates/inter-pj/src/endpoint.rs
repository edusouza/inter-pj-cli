//! Registry of the API operations implemented by this crate.
//!
//! Each [`Endpoint`] declares the HTTP method, the path template and the
//! OAuth scopes the operation requires. The client uses these definitions to
//! build requests and to ask for tokens with the minimum scopes; the contract
//! tests check them against Inter's OpenAPI specification.

use std::fmt;

use crate::scope::{Scope, ScopeSet};

/// HTTP method of an [`Endpoint`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Method {
    /// `GET`
    Get,
    /// `POST`
    Post,
    /// `PUT`
    Put,
    /// `PATCH`
    Patch,
    /// `DELETE`
    Delete,
}

impl Method {
    /// Upper-case method name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
        }
    }

    pub(crate) fn to_reqwest(self) -> reqwest::Method {
        match self {
            Self::Get => reqwest::Method::GET,
            Self::Post => reqwest::Method::POST,
            Self::Put => reqwest::Method::PUT,
            Self::Patch => reqwest::Method::PATCH,
            Self::Delete => reqwest::Method::DELETE,
        }
    }
}

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// An API operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Endpoint {
    /// HTTP method.
    pub method: Method,
    /// Path template relative to the environment's base URL, with
    /// `{placeholders}` for path parameters (e.g. `/banking/v2/saldo`).
    pub path: &'static str,
    /// Scopes the access token must carry.
    pub scopes: &'static [Scope],
}

impl Endpoint {
    /// Required scopes as a set.
    pub fn scope_set(&self) -> ScopeSet {
        ScopeSet::from(self.scopes)
    }
}

impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.method, self.path)
    }
}

/// `POST /oauth/v2/token` — obtains an access token (client credentials).
pub const TOKEN: Endpoint = Endpoint {
    method: Method::Post,
    path: "/oauth/v2/token",
    scopes: &[],
};

/// Operations of the Banking API (`/banking/v2`).
pub mod banking {
    use super::{Endpoint, Method};
    use crate::scope::Scope;

    /// `GET /banking/v2/saldo` — account balance.
    pub const SALDO: Endpoint = Endpoint {
        method: Method::Get,
        path: "/banking/v2/saldo",
        scopes: &[Scope::ExtratoRead],
    };
}

/// Every operation implemented by this crate.
pub const ALL: &[Endpoint] = &[TOKEN, banking::SALDO];
