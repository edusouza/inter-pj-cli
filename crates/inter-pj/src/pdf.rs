//! PDF documents sent in JSON, as base64 text (`{"pdf": "JVBERi0..."}`).

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::Deserialize;

use crate::endpoint::Endpoint;
use crate::error::{Error, Result};

/// The answer of an endpoint that returns a PDF.
#[derive(Deserialize)]
pub(crate) struct RespostaPdf {
    #[serde(default)]
    pdf: Option<String>,
}

impl RespostaPdf {
    /// The decoded document, checked to be a PDF.
    ///
    /// # Errors
    ///
    /// [`Error::Decode`] when the field is missing, is not base64 or does not
    /// hold a PDF. The content is never quoted: it may be a statement.
    pub(crate) fn decodificar(self, endpoint: Endpoint) -> Result<Vec<u8>> {
        let invalid = |message: &str| Error::Decode {
            operation: endpoint.to_string(),
            message: message.to_owned(),
        };
        let encoded: String = self
            .pdf
            .ok_or_else(|| invalid("a resposta não traz o campo pdf"))?
            .chars()
            .filter(|c| !c.is_ascii_whitespace())
            .collect();
        let pdf = BASE64
            .decode(encoded)
            .map_err(|_| invalid("o campo pdf não está em base64"))?;
        if !pdf.starts_with(b"%PDF") {
            return Err(invalid("o conteúdo recebido não é um PDF"));
        }
        Ok(pdf)
    }
}
