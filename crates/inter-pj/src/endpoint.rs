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

    /// `GET /banking/v2/extrato` — statement of a period.
    pub const EXTRATO: Endpoint = Endpoint {
        method: Method::Get,
        path: "/banking/v2/extrato",
        scopes: &[Scope::ExtratoRead],
    };

    /// `GET /banking/v2/extrato/completo` — enriched statement, paginated.
    pub const EXTRATO_COMPLETO: Endpoint = Endpoint {
        method: Method::Get,
        path: "/banking/v2/extrato/completo",
        scopes: &[Scope::ExtratoRead],
    };

    /// `GET /banking/v2/extrato/exportar` — statement as a PDF document.
    pub const EXTRATO_EXPORTAR: Endpoint = Endpoint {
        method: Method::Get,
        path: "/banking/v2/extrato/exportar",
        scopes: &[Scope::ExtratoRead],
    };

    /// `POST /banking/v2/pagamento` — pays a boleto, utility bill or tax by barcode.
    pub const PAGAMENTO_INCLUIR: Endpoint = Endpoint {
        method: Method::Post,
        path: "/banking/v2/pagamento",
        scopes: &[Scope::PagamentoBoletoWrite],
    };

    /// `GET /banking/v2/pagamento` — payments by barcode.
    pub const PAGAMENTO_BUSCAR: Endpoint = Endpoint {
        method: Method::Get,
        path: "/banking/v2/pagamento",
        scopes: &[Scope::PagamentoBoletoRead],
    };

    /// `DELETE /banking/v2/pagamento/{codigoTransacao}` — cancels a scheduled payment.
    pub const PAGAMENTO_CANCELAR: Endpoint = Endpoint {
        method: Method::Delete,
        path: "/banking/v2/pagamento/{codigoTransacao}",
        scopes: &[Scope::PagamentoBoletoWrite],
    };

    /// `POST /banking/v2/pagamento/darf` — pays a DARF without a barcode.
    pub const PAGAMENTO_DARF_INCLUIR: Endpoint = Endpoint {
        method: Method::Post,
        path: "/banking/v2/pagamento/darf",
        scopes: &[Scope::PagamentoDarfWrite],
    };

    /// `GET /banking/v2/pagamento/darf` — DARF payments. The API asks for the
    /// scope of payments by barcode.
    pub const PAGAMENTO_DARF_BUSCAR: Endpoint = Endpoint {
        method: Method::Get,
        path: "/banking/v2/pagamento/darf",
        scopes: &[Scope::PagamentoBoletoRead],
    };

    /// `POST /banking/v2/pagamento/lote` — sends a batch of payments by barcode and DARFs.
    pub const PAGAMENTO_LOTE_INCLUIR: Endpoint = Endpoint {
        method: Method::Post,
        path: "/banking/v2/pagamento/lote",
        scopes: &[Scope::PagamentoLoteWrite],
    };

    /// `GET /banking/v2/pagamento/lote/{idLote}` — a batch and the status of its payments.
    pub const PAGAMENTO_LOTE_CONSULTAR: Endpoint = Endpoint {
        method: Method::Get,
        path: "/banking/v2/pagamento/lote/{idLote}",
        scopes: &[Scope::PagamentoLoteRead],
    };

    /// `POST /banking/v2/pix` — sends a Pix (key, bank details or copia e cola).
    pub const PIX_INCLUIR: Endpoint = Endpoint {
        method: Method::Post,
        path: "/banking/v2/pix",
        scopes: &[Scope::PagamentoPixWrite],
    };

    /// `GET /banking/v2/pix/{codigoSolicitacao}` — status of a Pix sent.
    pub const PIX_CONSULTAR: Endpoint = Endpoint {
        method: Method::Get,
        path: "/banking/v2/pix/{codigoSolicitacao}",
        scopes: &[Scope::PagamentoPixRead],
    };
}

/// Operations of the Cobrança API (`/cobranca/v3`).
pub mod cobranca {
    use super::{Endpoint, Method};
    use crate::scope::Scope;

    /// `POST /cobranca/v3/cobrancas` — issues a charge (boleto with Pix).
    pub const EMITIR: Endpoint = Endpoint {
        method: Method::Post,
        path: "/cobranca/v3/cobrancas",
        scopes: &[Scope::BoletoCobrancaWrite],
    };

    /// `GET /cobranca/v3/cobrancas/{codigoSolicitacao}` — a charge in detail.
    pub const CONSULTAR: Endpoint = Endpoint {
        method: Method::Get,
        path: "/cobranca/v3/cobrancas/{codigoSolicitacao}",
        scopes: &[Scope::BoletoCobrancaRead],
    };

    /// `GET /cobranca/v3/cobrancas` — charges of a period, paginated.
    pub const LISTAR: Endpoint = Endpoint {
        method: Method::Get,
        path: "/cobranca/v3/cobrancas",
        scopes: &[Scope::BoletoCobrancaRead],
    };

    /// `GET /cobranca/v3/cobrancas/sumario` — number and amount by situation.
    pub const SUMARIO: Endpoint = Endpoint {
        method: Method::Get,
        path: "/cobranca/v3/cobrancas/sumario",
        scopes: &[Scope::BoletoCobrancaRead],
    };

    /// `GET /cobranca/v3/cobrancas/{codigoSolicitacao}/pdf` — a charge as a PDF.
    pub const PDF: Endpoint = Endpoint {
        method: Method::Get,
        path: "/cobranca/v3/cobrancas/{codigoSolicitacao}/pdf",
        scopes: &[Scope::BoletoCobrancaRead],
    };

    /// `POST /cobranca/v3/cobrancas/{codigoSolicitacao}/cancelar` — cancels a charge.
    pub const CANCELAR: Endpoint = Endpoint {
        method: Method::Post,
        path: "/cobranca/v3/cobrancas/{codigoSolicitacao}/cancelar",
        scopes: &[Scope::BoletoCobrancaWrite],
    };

    /// `PATCH /cobranca/v3/cobrancas/{codigoSolicitacao}` — changes the due date or the value.
    pub const EDITAR: Endpoint = Endpoint {
        method: Method::Patch,
        path: "/cobranca/v3/cobrancas/{codigoSolicitacao}",
        scopes: &[Scope::BoletoCobrancaWrite],
    };

    /// `GET /cobranca/v3/cobrancas/edicao/{codigoEdicao}` — where a change stands.
    pub const EDICAO: Endpoint = Endpoint {
        method: Method::Get,
        path: "/cobranca/v3/cobrancas/edicao/{codigoEdicao}",
        scopes: &[Scope::BoletoCobrancaRead],
    };

    /// `POST /cobranca/v3/cobrancas/{codigoSolicitacao}/pagar` — pays a charge (sandbox only).
    pub const PAGAR: Endpoint = Endpoint {
        method: Method::Post,
        path: "/cobranca/v3/cobrancas/{codigoSolicitacao}/pagar",
        scopes: &[Scope::BoletoCobrancaWrite],
    };
}

/// Every operation implemented by this crate.
pub const ALL: &[Endpoint] = &[
    TOKEN,
    banking::SALDO,
    banking::EXTRATO,
    banking::EXTRATO_COMPLETO,
    banking::EXTRATO_EXPORTAR,
    banking::PIX_INCLUIR,
    banking::PIX_CONSULTAR,
    banking::PAGAMENTO_INCLUIR,
    banking::PAGAMENTO_BUSCAR,
    banking::PAGAMENTO_CANCELAR,
    banking::PAGAMENTO_DARF_INCLUIR,
    banking::PAGAMENTO_DARF_BUSCAR,
    banking::PAGAMENTO_LOTE_INCLUIR,
    banking::PAGAMENTO_LOTE_CONSULTAR,
    cobranca::EMITIR,
    cobranca::CONSULTAR,
    cobranca::LISTAR,
    cobranca::SUMARIO,
    cobranca::PDF,
    cobranca::CANCELAR,
    cobranca::EDITAR,
    cobranca::EDICAO,
    cobranca::PAGAR,
];
