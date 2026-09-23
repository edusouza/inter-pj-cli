//! Error payloads returned by the Inter Empresas APIs.
//!
//! The APIs use the *problem details* format (RFC 7807) with small variations
//! between products, plus a legacy `{"erro": {...}}` shape in some banking
//! endpoints and the standard OAuth `{"error": ...}` shape in the token
//! endpoint. [`Problem::from_body`] normalises all of them.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// Normalised error payload.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Problem {
    /// URI identifying the problem type.
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub type_uri: Option<String>,
    /// Short summary of the problem.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// HTTP status as reported in the body (sometimes sent as a string).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient_u16"
    )]
    pub status: Option<u16>,
    /// Detailed description of the problem.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// When the error happened, as sent by the API.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient_string"
    )]
    pub timestamp: Option<String>,
    /// Identifier to be quoted when contacting Inter's support.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    /// Machine readable error code (e.g. `SCROLL_ALREADY_ACTIVE`).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient_string"
    )]
    pub type_error: Option<String>,
    /// Validation errors, one per offending property.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub violacoes: Vec<Violacao>,
}

/// A validation error on a specific property of the request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Violacao {
    /// Why the value was rejected.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient_string"
    )]
    pub razao: Option<String>,
    /// Name of the offending property.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient_string"
    )]
    pub propriedade: Option<String>,
    /// Value that was rejected.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient_string"
    )]
    pub valor: Option<String>,
}

impl Problem {
    /// Parses an error response body, returning `None` when it carries no
    /// recognisable information.
    pub fn from_body(body: &[u8]) -> Option<Self> {
        let value: Value = serde_json::from_slice(body).ok()?;
        let Value::Object(object) = &value else {
            return None;
        };

        let problem = if let Some(Value::Object(erro)) = object.get("erro") {
            // Legacy banking shape: {"erro": {"mensagem", "mensagemDetalhe", "codigo", "status"}}
            Self {
                title: text(erro.get("mensagem")),
                detail: text(erro.get("mensagemDetalhe")),
                type_error: text(erro.get("codigo")),
                status: erro.get("status").and_then(as_u16),
                ..Self::default()
            }
        } else if object.contains_key("error") && !object.contains_key("title") {
            // OAuth shape: {"error": "invalid_client", "error_description": "..."}
            Self {
                title: text(object.get("error")),
                detail: text(object.get("error_description")),
                ..Self::default()
            }
        } else {
            let mut problem: Self = serde_json::from_value(value.clone()).ok()?;
            if problem.title.is_none() {
                problem.title = text(object.get("message"));
            }
            problem
        };

        (!problem.is_empty()).then_some(problem)
    }

    fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.detail.is_none()
            && self.type_error.is_none()
            && self.violacoes.is_empty()
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.title, &self.detail) {
            (Some(title), Some(detail)) if title != detail => write!(f, "{title} — {detail}")?,
            (Some(text), _) | (None, Some(text)) => f.write_str(text)?,
            (None, None) => match &self.type_error {
                Some(code) => f.write_str(code)?,
                None => f.write_str("erro sem descrição")?,
            },
        }
        for violacao in &self.violacoes {
            write!(f, "\n  • {violacao}")?;
        }
        Ok(())
    }
}

impl fmt::Display for Violacao {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let razao = self.razao.as_deref().unwrap_or("valor inválido");
        match &self.propriedade {
            Some(propriedade) => write!(f, "{propriedade}: {razao}")?,
            None => f.write_str(razao)?,
        }
        if let Some(valor) = &self.valor {
            write!(f, " (valor: {valor})")?;
        }
        Ok(())
    }
}

fn text(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(s) if !s.trim().is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn as_u16(value: &Value) -> Option<u16> {
    match value {
        Value::Number(n) => n.as_u64().and_then(|n| u16::try_from(n).ok()),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn lenient_u16<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<u16>, D::Error> {
    Ok(Option::<Value>::deserialize(deserializer)?
        .as_ref()
        .and_then(as_u16))
}

fn lenient_string<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    Ok(text(Option::<Value>::deserialize(deserializer)?.as_ref()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_banking_problem_with_string_status() {
        let body = r#"{"title":"Acesso Negado","status":"403","detail":"Requisição de participante autenticado que viola alguma regra de autorização."}"#.as_bytes();
        let problem = Problem::from_body(body).unwrap();
        assert_eq!(problem.status, Some(403));
        assert_eq!(problem.title.as_deref(), Some("Acesso Negado"));
    }

    #[test]
    fn parses_violations() {
        let body = r#"{"title":"Dados inválidos.","detail":"Verifique os dados.","timestamp":"2023-01-01T00:00:00-03:00","violacoes":[{"razao":"Não foi possível converter o valor.","propriedade":"dataSaldo"}]}"#.as_bytes();
        let problem = Problem::from_body(body).unwrap();
        assert_eq!(problem.violacoes.len(), 1);
        assert_eq!(
            problem.to_string(),
            "Dados inválidos. — Verifique os dados.\n  • dataSaldo: Não foi possível converter o valor."
        );
    }

    #[test]
    fn parses_pix_problem_with_numeric_violation_value() {
        let body = r#"{"type":"https://pix.bcb.gov.br/api/v2/error/CobOperacaoInvalida","title":"Cobrança inválida.","status":400,"correlationId":"abc-123","violacoes":[{"razao":"Valor não pode ser 0.00","propriedade":"cob.valor.original","valor":0}]}"#.as_bytes();
        let problem = Problem::from_body(body).unwrap();
        assert_eq!(problem.correlation_id.as_deref(), Some("abc-123"));
        assert_eq!(problem.violacoes[0].valor.as_deref(), Some("0"));
    }

    #[test]
    fn parses_scroll_error_code() {
        let body = r#"{"title":"Já existe um scroll ativo para esta conta corrente.","detail":"Aguarde.","typeError":"SCROLL_ALREADY_ACTIVE"}"#.as_bytes();
        let problem = Problem::from_body(body).unwrap();
        assert_eq!(problem.type_error.as_deref(), Some("SCROLL_ALREADY_ACTIVE"));
    }

    #[test]
    fn normalises_legacy_erro_shape() {
        let body = r#"{"erro":{"mensagem":"Requisição inválida","mensagemDetalhe":"CPF/CNPJ não pode ser nulo","codigo":51002,"status":400}}"#.as_bytes();
        let problem = Problem::from_body(body).unwrap();
        assert_eq!(problem.title.as_deref(), Some("Requisição inválida"));
        assert_eq!(
            problem.detail.as_deref(),
            Some("CPF/CNPJ não pode ser nulo")
        );
        assert_eq!(problem.type_error.as_deref(), Some("51002"));
        assert_eq!(problem.status, Some(400));
    }

    #[test]
    fn normalises_oauth_error_shape() {
        let body =
            br#"{"error":"invalid_client","error_description":"Client authentication failed"}"#;
        let problem = Problem::from_body(body).unwrap();
        assert_eq!(
            problem.to_string(),
            "invalid_client — Client authentication failed"
        );
    }

    #[test]
    fn falls_back_to_message_field() {
        let problem = Problem::from_body(br#"{"message":"Too Many Requests"}"#).unwrap();
        assert_eq!(problem.title.as_deref(), Some("Too Many Requests"));
    }

    #[test]
    fn ignores_bodies_without_information() {
        assert_eq!(Problem::from_body(b""), None);
        assert_eq!(Problem::from_body(b"<html>502</html>"), None);
        assert_eq!(Problem::from_body(b"{}"), None);
        assert_eq!(Problem::from_body(b"[1,2]"), None);
    }
}
