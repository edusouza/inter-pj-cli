//! What only the sandbox offers (`/pix/v2/sandbox`): the answers of the
//! payer and of their bank, simulated, so the whole flow of Pix Automático
//! can be tried: approving or cancelling a recurrence, accepting or
//! rejecting a confirmation request, and cancelling or paying a recurring
//! charge.

use rust_decimal::Decimal;
use serde::Serialize;
use serde_json::{Map, json};

use super::{IdRec, PixAutomatico, StatusRec, StatusSolicRec};
use crate::client::ApiRequest;
use crate::documento::Documento;
use crate::endpoint;
use crate::environment::Environment;
use crate::error::{Error, Result};
use crate::pix::{ChavePix, PagamentoSandbox, Txid, corpo, valor};
use crate::retry::RetryMode;
use crate::serde_util::{api_enum, decimal_as_number, string_serde};

api_enum! {
    /// Why a recurrence was cancelled (`razao`), in the sandbox.
    pub enum RazaoCancelamentoRec {
        /// The account was closed.
        Accl => "ACCL",
        /// The company was closed.
        Cpcl => "CPCL",
        /// The payer died.
        Dcsd => "DCSD",
        /// The receiver or their bank asked, for an error in the
        /// confirmation request.
        Ersl => "ERSL",
        /// Fraud.
        Frud => "FRUD",
        /// The receiver's bank asked, as the confirmation request got no
        /// answer in time.
        Pcfd => "PCFD",
        /// The receiver's bank asked, as the same recurrence was confirmed
        /// another way (by the QR Code, say).
        Slcr => "SLCR",
        /// The receiver asked.
        Sldb => "SLDB",
        /// The payer asked.
        Nres => "NRES",
    }
}

api_enum! {
    /// Why a recurring charge was cancelled (`razao`), in the sandbox.
    pub enum RazaoCancelamentoCobR {
        /// No specific reason.
        Unspecified => "UNSPECIFIED",
        /// The account was closed.
        AccountCanceled => "ACCOUNT_CANCELED",
        /// The account is blocked.
        AccountBlocked => "ACCOUNT_BLOCKED",
        /// The recurrence was cancelled.
        RecurrenceCanceled => "RECURRENCE_CANCELED",
        /// The settlement failed.
        SettlementFailed => "SETTLEMENT_FAILED",
        /// Other reasons.
        Other => "OTHER",
        /// The payer asked.
        RequestedByPayer => "REQUESTED_BY_PAYER",
        /// The receiver asked.
        RequestedByReceiver => "REQUESTED_BY_RECEIVER",
    }
}

string_serde!(RazaoCancelamentoRec, RazaoCancelamentoCobR);

/// The payment of a recurring charge in the sandbox (`MakePaymentCobr`).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PagamentoCobRSandbox<'a> {
    #[serde(serialize_with = "decimal_as_number::serialize")]
    valor: Decimal,
    cpf_cnpj: &'a str,
    tx_id: &'a str,
    chave: &'a str,
}

impl PixAutomatico<'_> {
    /// Approves (`APROVADA`) or cancels (`CANCELADA`, optionally with a
    /// reason) a recurrence, as the payer would (`PATCH
    /// /pix/v2/sandbox/rec/{idRec}/status`, scope `pix.write`; sandbox
    /// only). Recurring charges need an approved recurrence.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] outside the sandbox, for another status, or
    /// for a reason without a cancellation (nothing is sent); otherwise
    /// failures to obtain a token or to send the request, and the API's
    /// error statuses.
    pub async fn alterar_status_rec_no_sandbox(
        &self,
        id: &IdRec,
        status: &StatusRec,
        razao: Option<&RazaoCancelamentoRec>,
    ) -> Result<()> {
        self.so_no_sandbox()?;
        if !matches!(status, StatusRec::Aprovada | StatusRec::Cancelada) {
            return Err(Error::InvalidInput(
                format!("no sandbox, a recorrência passa a APROVADA ou CANCELADA, não a {status}")
                    .into(),
            ));
        }
        if razao.is_some() && *status != StatusRec::Cancelada {
            return Err(Error::InvalidInput(
                "a razão acompanha só o cancelamento da recorrência".into(),
            ));
        }
        let mut corpo = Map::new();
        corpo.insert("status".to_owned(), json!(status));
        if let Some(razao) = razao {
            corpo.insert("razao".to_owned(), json!(razao));
        }
        let request = ApiRequest::new(endpoint::pix_automatico::SANDBOX_STATUS_REC)
            .path_param("idRec", id.as_str().to_owned())
            .json(corpo.into())
            .retry(RetryMode::WhenNotProcessed);
        self.client.execute_empty(request).await
    }

    /// Accepts (`ACEITA`) or rejects (`REJEITADA`) the confirmation request
    /// of a recurrence, as the payer would (`PATCH
    /// /pix/v2/sandbox/solicrec/{idRec}/status`, scope `solicrec.write`;
    /// sandbox only). The path takes the recurrence, not the request.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] outside the sandbox or for another status
    /// (nothing is sent); otherwise the same as
    /// [`alterar_status_rec_no_sandbox`](Self::alterar_status_rec_no_sandbox).
    pub async fn alterar_status_solicitacao_no_sandbox(
        &self,
        id_rec: &IdRec,
        status: &StatusSolicRec,
    ) -> Result<()> {
        self.so_no_sandbox()?;
        if !matches!(status, StatusSolicRec::Aceita | StatusSolicRec::Rejeitada) {
            return Err(Error::InvalidInput(
                format!("no sandbox, a solicitação passa a ACEITA ou REJEITADA, não a {status}")
                    .into(),
            ));
        }
        let request = ApiRequest::new(endpoint::pix_automatico::SANDBOX_STATUS_SOLICITACAO)
            .path_param("idRec", id_rec.as_str().to_owned())
            .json(json!({ "status": status }))
            .retry(RetryMode::WhenNotProcessed);
        self.client.execute_empty(request).await
    }

    /// Cancels a recurring charge, as the payer's bank would (`PATCH
    /// /pix/v2/sandbox/cobr/{txId}/status`, scope `cobr.write`; sandbox
    /// only).
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] outside the sandbox (nothing is sent);
    /// otherwise the same as
    /// [`alterar_status_rec_no_sandbox`](Self::alterar_status_rec_no_sandbox).
    pub async fn cancelar_cobr_no_sandbox(
        &self,
        txid: &Txid,
        razao: &RazaoCancelamentoCobR,
    ) -> Result<()> {
        self.so_no_sandbox()?;
        let request = ApiRequest::new(endpoint::pix_automatico::SANDBOX_STATUS_COBR)
            .path_param("txId", txid.as_str().to_owned())
            .json(json!({ "status": "CANCELADA", "razao": razao }))
            .retry(RetryMode::WhenNotProcessed);
        self.client.execute_empty(request).await
    }

    /// Pays a recurring charge (`POST /pix/v2/sandbox/cobr/pagamento`, scope
    /// `cobr.write`; sandbox only), as `pagador` would, to the key `chave`.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] outside the sandbox or for an amount that is
    /// not positive (nothing is sent); otherwise the same as
    /// [`alterar_status_rec_no_sandbox`](Self::alterar_status_rec_no_sandbox),
    /// and failures to decode the answer.
    pub async fn pagar_cobr_no_sandbox(
        &self,
        txid: &Txid,
        valor_pago: Decimal,
        pagador: &Documento,
        chave: &ChavePix,
    ) -> Result<PagamentoSandbox> {
        self.so_no_sandbox()?;
        valor(valor_pago, "valor", false).map_err(|err| Error::InvalidInput(Box::new(err)))?;
        let request = ApiRequest::new(endpoint::pix_automatico::SANDBOX_PAGAR_COBR)
            .json(corpo(&PagamentoCobRSandbox {
                valor: valor_pago,
                cpf_cnpj: pagador.as_str(),
                tx_id: txid.as_str(),
                chave: chave.as_str(),
            })?)
            .retry(RetryMode::WhenNotProcessed);
        self.client.execute(request).await
    }

    fn so_no_sandbox(self) -> Result<()> {
        if self.client.environment() == Some(Environment::Sandbox) {
            Ok(())
        } else {
            Err(Error::InvalidInput(
                "a simulação do Pix Automático pela API existe só no sandbox".into(),
            ))
        }
    }
}
