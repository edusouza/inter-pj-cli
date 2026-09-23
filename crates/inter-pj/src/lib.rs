//! Unofficial Rust client for the Inter Empresas (Banco Inter PJ) APIs.
//!
//! This project is **not** affiliated with or endorsed by Banco Inter.

#![warn(missing_docs)]

mod credentials;
mod environment;
mod error;
mod identity;
pub mod problem;
mod scope;

pub use credentials::Credentials;
pub use environment::{Environment, ParseEnvironmentError};
pub use error::{ApiError, ApiErrorKind, Error, Result};
pub use identity::{ClientIdentity, IdentityError};
pub use problem::{Problem, Violacao};
pub use scope::{Scope, ScopeSet, UnknownScopeError};
