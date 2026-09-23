//! Banking API (`/banking/v2`): balance and, in later versions, statements,
//! payments and outbound Pix.

mod saldo;

use chrono::NaiveDate;

pub use saldo::Saldo;

use crate::client::{ApiRequest, InterClient};
use crate::endpoint;
use crate::error::Result;

/// Operations of the Banking API. Obtained with [`InterClient::banking`].
#[derive(Debug, Clone, Copy)]
pub struct Banking<'a> {
    client: &'a InterClient,
}

impl<'a> Banking<'a> {
    pub(crate) fn new(client: &'a InterClient) -> Self {
        Self { client }
    }

    /// Account balance (`GET /banking/v2/saldo`, scope `extrato.read`).
    ///
    /// Without `data`, returns the current available balance together with
    /// blocked amounts and the credit limit. With `data`, returns only the
    /// available balance at the end of that day.
    ///
    /// # Errors
    ///
    /// Fails when a token cannot be obtained, the request cannot be sent, the
    /// API answers with an error status or the response cannot be decoded.
    pub async fn saldo(&self, data: Option<NaiveDate>) -> Result<Saldo> {
        let mut request = ApiRequest::new(endpoint::banking::SALDO);
        if let Some(data) = data {
            request = request.query("dataSaldo", data.format("%Y-%m-%d").to_string());
        }
        self.client.execute(request).await
    }
}
