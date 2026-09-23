//! Unofficial Rust client for the Inter Empresas (Banco Inter PJ) APIs.
//!
//! Every Inter Empresas API requires two layers of authentication:
//!
//! 1. **Mutual TLS** with the certificate (`.crt`) and private key (`.key`)
//!    issued for the integration in Internet Banking PJ ([`ClientIdentity`]).
//! 2. An **OAuth2 bearer token** obtained with the *client credentials* grant
//!    at `POST /oauth/v2/token` for the scopes each operation requires
//!    ([`Credentials`], [`Scope`]).
//!
//! [`InterClient`] takes care of both: it requests tokens with the minimum
//! scopes needed by each call, caches them (in memory and, optionally, in a
//! [`TokenStore`]) and renews them when they expire or are rejected.
//!
//! ```no_run
//! use inter_pj::{ClientIdentity, Credentials, Environment, InterClient};
//!
//! # async fn example() -> Result<(), inter_pj::Error> {
//! let client = InterClient::builder()
//!     .environment(Environment::Sandbox)
//!     .credentials(Credentials::new("client-id", "client-secret"))
//!     .identity(ClientIdentity::from_pem_files("certificado.crt", "chave.key")?)
//!     .build()?;
//!
//! let saldo = client.banking().saldo(None).await?;
//! println!("{:?}", saldo.disponivel);
//! # Ok(())
//! # }
//! ```
//!
//! This project is **not** affiliated with or endorsed by Banco Inter.

#![warn(missing_docs)]

pub mod auth;
pub mod banking;
pub mod boleto;
mod client;
pub mod cobranca;
mod credentials;
pub mod documento;
pub mod endpoint;
mod environment;
mod error;
mod identity;
mod pdf;
pub mod pix;
pub mod problem;
mod retry;
mod scope;
mod serde_util;

pub use auth::{AccessToken, TokenStore};
pub use client::{InterClient, InterClientBuilder};
pub use credentials::Credentials;
pub use environment::{Environment, ParseEnvironmentError};
pub use error::{ApiError, ApiErrorKind, Error, Result};
pub use identity::{ClientIdentity, IdentityError};
pub use problem::{Problem, Violacao};
pub use retry::RetryPolicy;
pub use scope::{Scope, ScopeSet, UnknownScopeError};

/// Compiles the examples of the README as doctests.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;
