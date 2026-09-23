//! Cobrança API (`/cobranca/v3`): charges issued to the company's clients,
//! as a boleto with a Pix QR Code.

mod consulta;
mod emissao;

pub use consulta::{
    BoletoCobranca, CobrancaDetalhada, DadosCobranca, EncargoCobranca, NotaFiscalCobranca,
    OrigemRecebimento, PessoaCobranca, PixCobranca, SituacaoCobranca, SolicitacaoCobranca,
    TipoCobranca,
};
pub use emissao::{
    BeneficiarioFinal, Desconto, EmissaoCobranca, EmissaoCobrancaError, FormaRecebimento,
    MAX_CARACTERES_LINHA, MAX_DIAS_AGENDA, MAX_LINHAS_MENSAGEM, MAX_SEU_NUMERO, Mora, Multa,
    NotaFiscal, Pagador, TipoPessoa, Uf, UfError, VALOR_MAXIMO, VALOR_MINIMO,
};

use crate::client::{ApiRequest, InterClient};
use crate::endpoint;
use crate::error::{Error, Result};
use crate::retry::RetryMode;

/// Operations of the Cobrança API. Obtained with [`InterClient::cobranca`].
#[derive(Debug, Clone, Copy)]
pub struct Cobranca<'a> {
    client: &'a InterClient,
}

impl<'a> Cobranca<'a> {
    pub(crate) fn new(client: &'a InterClient) -> Self {
        Self { client }
    }

    /// Issues a charge: a boleto with a Pix QR Code (`POST
    /// /cobranca/v3/cobrancas`, scope `boleto-cobranca.write`).
    ///
    /// The charge is checked with [`EmissaoCobranca::validar`] before
    /// anything is sent. The API answers with the request code and issues the
    /// charge afterwards: [`consultar`](Self::consultar) shows
    /// [`SituacaoCobranca::EmProcessamento`] until the boleto and the Pix are
    /// ready. The Pix QR Code uses one of the account's Pix keys; without
    /// one, the boleto comes without it.
    ///
    /// For 30 minutes, the API refuses a second charge with the same
    /// `seuNumero`, amount, due date and payer. Still, the request is
    /// repeated automatically only when it surely was not processed (`429`,
    /// connection refused): after an unknown outcome, look the charge up
    /// before trying again. Charges due today can be issued until 19:59.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] with an [`EmissaoCobrancaError`] when the
    /// charge is invalid (nothing is sent); otherwise, failures to obtain a
    /// token, to send the request or to decode the answer, and the API's
    /// error statuses.
    pub async fn emitir(&self, cobranca: &EmissaoCobranca) -> Result<SolicitacaoCobranca> {
        cobranca
            .validar()
            .map_err(|err| Error::InvalidInput(Box::new(err)))?;
        let body =
            serde_json::to_value(cobranca).map_err(|err| Error::InvalidInput(Box::new(err)))?;
        let request = ApiRequest::new(endpoint::cobranca::EMITIR)
            .json(body)
            .retry(RetryMode::WhenNotProcessed);
        self.client.execute(request).await
    }

    /// A charge with its boleto, Pix QR Code and invoice (`GET
    /// /cobranca/v3/cobrancas/{codigoSolicitacao}`, scope
    /// `boleto-cobranca.read`).
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] when the code is not in the format of
    /// [`SolicitacaoCobranca::codigo_solicitacao`]; otherwise the same as
    /// [`emitir`](Self::emitir). Unknown charges fail with status `404`.
    pub async fn consultar(&self, codigo_solicitacao: &str) -> Result<CobrancaDetalhada> {
        let codigo = codigo(codigo_solicitacao)?;
        let request =
            ApiRequest::new(endpoint::cobranca::CONSULTAR).path_param("codigoSolicitacao", codigo);
        self.client.execute(request).await
    }
}

/// The code of a charge: a UUID, as documented. The examples of the
/// documentation are not always well-formed UUIDs, so any groups of
/// hexadecimal digits separated by hyphens are accepted.
fn codigo(codigo: &str) -> Result<String> {
    let codigo = codigo.trim();
    let valido = (8..=64).contains(&codigo.len())
        && codigo.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
        && !codigo.starts_with('-')
        && !codigo.ends_with('-');
    if valido {
        Ok(codigo.to_owned())
    } else {
        Err(Error::InvalidInput(
            "código da cobrança inválido: esperado um UUID (dígitos hexadecimais e hífens)".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_uuids_even_malformed_ones() {
        assert_eq!(
            codigo(" 0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d ").unwrap(),
            "0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d"
        );
        // The example of the documentation, with a short last group.
        assert!(codigo("183e982a-34e5-4bc0-9643-def5432a").is_ok());
        for invalido in [
            "",
            "../../banking/v2/saldo",
            "abc",
            "-0b7e4c1a",
            "0b7e4c1a-",
            "zzzzzzzz",
        ] {
            assert!(codigo(invalido).is_err(), "{invalido}");
        }
    }
}
