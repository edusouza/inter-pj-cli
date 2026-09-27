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

    /// `PUT /banking/v2/webhooks/{tipoWebhook}` — registers the webhook of a kind.
    pub const WEBHOOK_CADASTRAR: Endpoint = Endpoint {
        method: Method::Put,
        path: "/banking/v2/webhooks/{tipoWebhook}",
        scopes: &[Scope::WebhookBankingWrite],
    };

    /// `GET /banking/v2/webhooks/{tipoWebhook}` — the webhook of a kind.
    pub const WEBHOOK_CONSULTAR: Endpoint = Endpoint {
        method: Method::Get,
        path: "/banking/v2/webhooks/{tipoWebhook}",
        scopes: &[Scope::WebhookBankingRead],
    };

    /// `DELETE /banking/v2/webhooks/{tipoWebhook}` — removes the webhook of a kind.
    pub const WEBHOOK_EXCLUIR: Endpoint = Endpoint {
        method: Method::Delete,
        path: "/banking/v2/webhooks/{tipoWebhook}",
        scopes: &[Scope::WebhookBankingWrite],
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

    /// `PUT /cobranca/v3/cobrancas/webhook` — registers or changes the webhook.
    pub const WEBHOOK_CADASTRAR: Endpoint = Endpoint {
        method: Method::Put,
        path: "/cobranca/v3/cobrancas/webhook",
        scopes: &[Scope::BoletoCobrancaWrite],
    };

    /// `GET /cobranca/v3/cobrancas/webhook` — the webhook.
    pub const WEBHOOK_CONSULTAR: Endpoint = Endpoint {
        method: Method::Get,
        path: "/cobranca/v3/cobrancas/webhook",
        scopes: &[Scope::BoletoCobrancaRead],
    };

    /// `DELETE /cobranca/v3/cobrancas/webhook` — removes the webhook.
    pub const WEBHOOK_EXCLUIR: Endpoint = Endpoint {
        method: Method::Delete,
        path: "/cobranca/v3/cobrancas/webhook",
        scopes: &[Scope::BoletoCobrancaWrite],
    };
}

/// Operations of the Pix API (`/pix/v2`).
pub mod pix {
    use super::{Endpoint, Method};
    use crate::scope::Scope;

    /// `PUT /pix/v2/cob/{txid}` — creates an immediate charge with your txid.
    pub const CRIAR_COB: Endpoint = Endpoint {
        method: Method::Put,
        path: "/pix/v2/cob/{txid}",
        scopes: &[Scope::CobWrite],
    };

    /// `POST /pix/v2/cob` — creates an immediate charge; Inter chooses the txid.
    pub const CRIAR_COB_SEM_TXID: Endpoint = Endpoint {
        method: Method::Post,
        path: "/pix/v2/cob",
        scopes: &[Scope::CobWrite],
    };

    /// `PATCH /pix/v2/cob/{txid}` — changes or removes an immediate charge.
    pub const REVISAR_COB: Endpoint = Endpoint {
        method: Method::Patch,
        path: "/pix/v2/cob/{txid}",
        scopes: &[Scope::CobWrite],
    };

    /// `GET /pix/v2/cob/{txid}` — an immediate charge.
    pub const CONSULTAR_COB: Endpoint = Endpoint {
        method: Method::Get,
        path: "/pix/v2/cob/{txid}",
        scopes: &[Scope::CobRead],
    };

    /// `GET /pix/v2/cob` — immediate charges of a period, paginated.
    pub const LISTAR_COBS: Endpoint = Endpoint {
        method: Method::Get,
        path: "/pix/v2/cob",
        scopes: &[Scope::CobRead],
    };

    /// `PUT /pix/v2/cobv/{txid}` — creates a charge with a due date.
    pub const CRIAR_COBV: Endpoint = Endpoint {
        method: Method::Put,
        path: "/pix/v2/cobv/{txid}",
        scopes: &[Scope::CobvWrite],
    };

    /// `PATCH /pix/v2/cobv/{txid}` — changes or removes a charge with a due date.
    pub const REVISAR_COBV: Endpoint = Endpoint {
        method: Method::Patch,
        path: "/pix/v2/cobv/{txid}",
        scopes: &[Scope::CobvWrite],
    };

    /// `GET /pix/v2/cobv/{txid}` — a charge with a due date.
    pub const CONSULTAR_COBV: Endpoint = Endpoint {
        method: Method::Get,
        path: "/pix/v2/cobv/{txid}",
        scopes: &[Scope::CobvRead],
    };

    /// `GET /pix/v2/cobv` — charges with a due date of a period, paginated.
    pub const LISTAR_COBVS: Endpoint = Endpoint {
        method: Method::Get,
        path: "/pix/v2/cobv",
        scopes: &[Scope::CobvRead],
    };

    /// `GET /pix/v2/pix` — Pix received in a period, paginated.
    pub const LISTAR_RECEBIDOS: Endpoint = Endpoint {
        method: Method::Get,
        path: "/pix/v2/pix",
        scopes: &[Scope::PixRead],
    };

    /// `GET /pix/v2/pix/{e2eId}` — a Pix received, with its refunds.
    pub const CONSULTAR_RECEBIDO: Endpoint = Endpoint {
        method: Method::Get,
        path: "/pix/v2/pix/{e2eId}",
        scopes: &[Scope::PixRead],
    };

    /// `PUT /pix/v2/pix/{e2eId}/devolucao/{id}` — refunds a Pix received.
    pub const SOLICITAR_DEVOLUCAO: Endpoint = Endpoint {
        method: Method::Put,
        path: "/pix/v2/pix/{e2eId}/devolucao/{id}",
        scopes: &[Scope::PixWrite],
    };

    /// `GET /pix/v2/pix/{e2eId}/devolucao/{id}` — where a refund stands.
    pub const CONSULTAR_DEVOLUCAO: Endpoint = Endpoint {
        method: Method::Get,
        path: "/pix/v2/pix/{e2eId}/devolucao/{id}",
        scopes: &[Scope::PixRead],
    };

    /// `POST /pix/v2/loc` — creates a location for a payload.
    pub const CRIAR_LOC: Endpoint = Endpoint {
        method: Method::Post,
        path: "/pix/v2/loc",
        scopes: &[Scope::PayloadLocationWrite],
    };

    /// `GET /pix/v2/loc` — locations of a period, paginated.
    pub const LISTAR_LOCS: Endpoint = Endpoint {
        method: Method::Get,
        path: "/pix/v2/loc",
        scopes: &[Scope::PayloadLocationRead],
    };

    /// `GET /pix/v2/loc/{id}` — a location.
    pub const CONSULTAR_LOC: Endpoint = Endpoint {
        method: Method::Get,
        path: "/pix/v2/loc/{id}",
        scopes: &[Scope::PayloadLocationRead],
    };

    /// `DELETE /pix/v2/loc/{id}/txid` — unlinks the charge from a location.
    pub const DESVINCULAR_LOC: Endpoint = Endpoint {
        method: Method::Delete,
        path: "/pix/v2/loc/{id}/txid",
        scopes: &[Scope::PayloadLocationWrite],
    };

    /// `PUT /pix/v2/lotecobv/{id}` — creates or replaces a batch of charges with a due date.
    pub const CRIAR_LOTE_COBV: Endpoint = Endpoint {
        method: Method::Put,
        path: "/pix/v2/lotecobv/{id}",
        scopes: &[Scope::LoteCobvWrite],
    };

    /// `PATCH /pix/v2/lotecobv/{id}` — changes charges of a batch.
    pub const REVISAR_LOTE_COBV: Endpoint = Endpoint {
        method: Method::Patch,
        path: "/pix/v2/lotecobv/{id}",
        scopes: &[Scope::LoteCobvWrite],
    };

    /// `GET /pix/v2/lotecobv/{id}` — a batch and its charges.
    pub const CONSULTAR_LOTE_COBV: Endpoint = Endpoint {
        method: Method::Get,
        path: "/pix/v2/lotecobv/{id}",
        scopes: &[Scope::LoteCobvRead],
    };

    /// `GET /pix/v2/lotecobv/{id}/sumario` — totals of a batch.
    pub const SUMARIO_LOTE_COBV: Endpoint = Endpoint {
        method: Method::Get,
        path: "/pix/v2/lotecobv/{id}/sumario",
        scopes: &[Scope::LoteCobvRead],
    };

    /// `GET /pix/v2/lotecobv/{id}/situacao/{situacao}` — charges of a batch in a situation.
    pub const LOTE_COBV_POR_SITUACAO: Endpoint = Endpoint {
        method: Method::Get,
        path: "/pix/v2/lotecobv/{id}/situacao/{situacao}",
        scopes: &[Scope::LoteCobvRead],
    };

    /// `GET /pix/v2/lotecobv` — batches of a period, paginated.
    pub const LISTAR_LOTES_COBV: Endpoint = Endpoint {
        method: Method::Get,
        path: "/pix/v2/lotecobv",
        scopes: &[Scope::LoteCobvRead],
    };

    /// `POST /pix/v2/cob/pagar/{txid}` — pays an immediate charge (sandbox only).
    pub const PAGAR_COB_SANDBOX: Endpoint = Endpoint {
        method: Method::Post,
        path: "/pix/v2/cob/pagar/{txid}",
        scopes: &[Scope::PixWrite],
    };

    /// `POST /pix/v2/cobv/pagar/{txid}` — pays a charge with a due date (sandbox only).
    pub const PAGAR_COBV_SANDBOX: Endpoint = Endpoint {
        method: Method::Post,
        path: "/pix/v2/cobv/pagar/{txid}",
        scopes: &[Scope::PixWrite],
    };

    /// `POST /pix/v2/sandbox/cob/pagamento` — pays a "copia e cola" (sandbox only).
    pub const PAGAR_QR_CODE_SANDBOX: Endpoint = Endpoint {
        method: Method::Post,
        path: "/pix/v2/sandbox/cob/pagamento",
        scopes: &[Scope::PixWrite],
    };

    /// `PUT /pix/v2/webhook/{chave}` — registers the webhook of a Pix key.
    pub const WEBHOOK_CADASTRAR: Endpoint = Endpoint {
        method: Method::Put,
        path: "/pix/v2/webhook/{chave}",
        scopes: &[Scope::WebhookWrite],
    };

    /// `GET /pix/v2/webhook/{chave}` — the webhook of a Pix key.
    pub const WEBHOOK_CONSULTAR: Endpoint = Endpoint {
        method: Method::Get,
        path: "/pix/v2/webhook/{chave}",
        scopes: &[Scope::WebhookRead],
    };

    /// `DELETE /pix/v2/webhook/{chave}` — removes the webhook of a Pix key.
    pub const WEBHOOK_EXCLUIR: Endpoint = Endpoint {
        method: Method::Delete,
        path: "/pix/v2/webhook/{chave}",
        scopes: &[Scope::WebhookWrite],
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
    banking::WEBHOOK_CADASTRAR,
    banking::WEBHOOK_CONSULTAR,
    banking::WEBHOOK_EXCLUIR,
    cobranca::EMITIR,
    cobranca::CONSULTAR,
    cobranca::LISTAR,
    cobranca::SUMARIO,
    cobranca::PDF,
    cobranca::CANCELAR,
    cobranca::EDITAR,
    cobranca::EDICAO,
    cobranca::PAGAR,
    cobranca::WEBHOOK_CADASTRAR,
    cobranca::WEBHOOK_CONSULTAR,
    cobranca::WEBHOOK_EXCLUIR,
    pix::CRIAR_COB,
    pix::CRIAR_COB_SEM_TXID,
    pix::REVISAR_COB,
    pix::CONSULTAR_COB,
    pix::LISTAR_COBS,
    pix::CRIAR_COBV,
    pix::REVISAR_COBV,
    pix::CONSULTAR_COBV,
    pix::LISTAR_COBVS,
    pix::LISTAR_RECEBIDOS,
    pix::CONSULTAR_RECEBIDO,
    pix::SOLICITAR_DEVOLUCAO,
    pix::CONSULTAR_DEVOLUCAO,
    pix::CRIAR_LOC,
    pix::LISTAR_LOCS,
    pix::CONSULTAR_LOC,
    pix::DESVINCULAR_LOC,
    pix::CRIAR_LOTE_COBV,
    pix::REVISAR_LOTE_COBV,
    pix::CONSULTAR_LOTE_COBV,
    pix::SUMARIO_LOTE_COBV,
    pix::LOTE_COBV_POR_SITUACAO,
    pix::LISTAR_LOTES_COBV,
    pix::PAGAR_COB_SANDBOX,
    pix::PAGAR_COBV_SANDBOX,
    pix::PAGAR_QR_CODE_SANDBOX,
    pix::WEBHOOK_CADASTRAR,
    pix::WEBHOOK_CONSULTAR,
    pix::WEBHOOK_EXCLUIR,
];
