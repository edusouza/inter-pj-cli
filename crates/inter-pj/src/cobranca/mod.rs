//! Cobrança API (`/cobranca/v3`): charges issued to the company's clients,
//! as a boleto with a Pix QR Code.

mod alteracao;
mod consulta;
mod emissao;
mod listagem;

pub use alteracao::{
    ConsultaEdicao, EdicaoCobranca, MAX_MOTIVO_CANCELAMENTO, PagarCom, SolicitacaoEdicao,
    StatusEdicao, motivo_cancelamento,
};
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
pub use listagem::{
    FiltrarDataPor, FiltroCobrancas, ITENS_POR_PAGINA_MAXIMO, ItemSumario, OrdenarCobrancasPor,
    PaginaCobrancas,
};

use serde_json::json;

use crate::client::{ApiRequest, InterClient};
use crate::endpoint::{self, Endpoint};
use crate::environment::Environment;
use crate::error::{Error, Result};
use crate::pdf::RespostaPdf;
use crate::retry::RetryMode;

/// Safety net against an API that never reports the last page.
const MAX_PAGINAS: u32 = 10_000;

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

    /// One page of the charges of a period (`GET /cobranca/v3/cobrancas`,
    /// scope `boleto-cobranca.read`).
    ///
    /// `pagina` starts at 0. Without `itens_por_pagina` the API returns 100
    /// charges per page; it accepts up to [`ITENS_POR_PAGINA_MAXIMO`]. See
    /// [`listar_todas`](Self::listar_todas) to read every page.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] when the period ends before it starts or a
    /// text of the filter is too long; otherwise the same as
    /// [`emitir`](Self::emitir).
    pub async fn listar(
        &self,
        filtro: &FiltroCobrancas,
        pagina: u32,
        itens_por_pagina: Option<u32>,
    ) -> Result<PaginaCobrancas> {
        filtro
            .validar()
            .map_err(|err| Error::InvalidInput(err.into()))?;
        let mut request = ApiRequest::new(endpoint::cobranca::LISTAR)
            .queries(filtro.query())
            .queries(filtro.ordem())
            .query("paginacao.paginaAtual", pagina.to_string());
        if let Some(itens) = itens_por_pagina {
            request = request.query("paginacao.itensPorPagina", itens.to_string());
        }
        self.client.execute(request).await
    }

    /// Every charge of the period, reading as many pages of
    /// [`ITENS_POR_PAGINA_MAXIMO`] as needed.
    ///
    /// # Errors
    ///
    /// Same as [`listar`](Self::listar); and a listing that ends before
    /// every charge its pages announce (`totalElementos`), or that goes on
    /// past 10 000 pages, is an [`Error::Decode`], never a partial list.
    /// Without that total, the listing ends at the page the API marks as
    /// the last or at an empty page.
    pub async fn listar_todas(&self, filtro: &FiltroCobrancas) -> Result<Vec<CobrancaDetalhada>> {
        let itens = Some(ITENS_POR_PAGINA_MAXIMO);
        let mut cobrancas = Vec::new();
        let mut numero = 0;
        loop {
            let mut pagina = self.listar(filtro, numero, itens).await?;
            let recebidas = pagina.cobrancas.len();
            let mais = ler_mais(&pagina, numero, cobrancas.len() + recebidas, MAX_PAGINAS);
            cobrancas.append(&mut pagina.cobrancas);
            tracing::info!("cobranças: página {} com {recebidas}", numero + 1);
            if !mais? {
                return Ok(cobrancas);
            }
            numero += 1;
        }
    }

    /// Number and amount of the charges of a period by situation (`GET
    /// /cobranca/v3/cobrancas/sumario`, scope `boleto-cobranca.read`). The
    /// order of the filter is ignored.
    ///
    /// # Errors
    ///
    /// Same as [`listar`](Self::listar).
    pub async fn sumario(&self, filtro: &FiltroCobrancas) -> Result<Vec<ItemSumario>> {
        filtro
            .validar()
            .map_err(|err| Error::InvalidInput(err.into()))?;
        let request = ApiRequest::new(endpoint::cobranca::SUMARIO).queries(filtro.query());
        let itens: Option<Vec<ItemSumario>> = self.client.execute(request).await?;
        Ok(itens.unwrap_or_default())
    }

    /// The charge as a PDF document, with the boleto and the Pix QR Code
    /// (`GET /cobranca/v3/cobrancas/{codigoSolicitacao}/pdf`, scope
    /// `boleto-cobranca.read`).
    ///
    /// # Errors
    ///
    /// Same as [`consultar`](Self::consultar); also fails when the content
    /// received is not a base64-encoded PDF.
    pub async fn pdf(&self, codigo_solicitacao: &str) -> Result<Vec<u8>> {
        const ENDPOINT: Endpoint = endpoint::cobranca::PDF;
        let codigo = codigo(codigo_solicitacao)?;
        let request = ApiRequest::new(ENDPOINT).path_param("codigoSolicitacao", codigo);
        let resposta: RespostaPdf = self.client.execute(request).await?;
        resposta.decodificar(ENDPOINT)
    }

    /// Cancels a charge (`POST
    /// /cobranca/v3/cobrancas/{codigoSolicitacao}/cancelar`, scope
    /// `boleto-cobranca.write`), with a reason of up to
    /// [`MAX_MOTIVO_CANCELAMENTO`] characters (see [`motivo_cancelamento`]).
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] when the code or the reason is invalid
    /// (nothing is sent); otherwise the same as [`emitir`](Self::emitir).
    /// Charges that cannot be cancelled (paid, already cancelled) fail with
    /// the API's status.
    pub async fn cancelar(&self, codigo_solicitacao: &str, motivo: &str) -> Result<()> {
        let codigo = codigo(codigo_solicitacao)?;
        let motivo = motivo_cancelamento(motivo).map_err(|err| Error::InvalidInput(err.into()))?;
        let request = ApiRequest::new(endpoint::cobranca::CANCELAR)
            .path_param("codigoSolicitacao", codigo)
            .json(json!({ "motivoCancelamento": motivo }))
            .retry(RetryMode::WhenNotProcessed);
        self.client.execute_empty(request).await
    }

    /// Changes the due date or the face value of a charge (`PATCH
    /// /cobranca/v3/cobrancas/{codigoSolicitacao}`, scope
    /// `boleto-cobranca.write`).
    ///
    /// The change may still be in progress when this returns: follow it with
    /// [`consultar_edicao`](Self::consultar_edicao). According to the
    /// documentation, the new value may take up to 30 minutes to show.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] when the code or the changes are invalid
    /// (nothing is sent); otherwise the same as [`emitir`](Self::emitir).
    pub async fn editar(
        &self,
        codigo_solicitacao: &str,
        edicao: &EdicaoCobranca,
    ) -> Result<SolicitacaoEdicao> {
        let codigo = codigo(codigo_solicitacao)?;
        edicao
            .validar()
            .map_err(|err| Error::InvalidInput(err.into()))?;
        let body =
            serde_json::to_value(edicao).map_err(|err| Error::InvalidInput(Box::new(err)))?;
        let request = ApiRequest::new(endpoint::cobranca::EDITAR)
            .path_param("codigoSolicitacao", codigo)
            .json(body)
            .retry(RetryMode::WhenNotProcessed);
        self.client.execute(request).await
    }

    /// Where a change made with [`editar`](Self::editar) stands (`GET
    /// /cobranca/v3/cobrancas/edicao/{codigoEdicao}`, scope
    /// `boleto-cobranca.read`).
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] when the code is not in the format of
    /// [`SolicitacaoEdicao::codigo_edicao`]; otherwise the same as
    /// [`emitir`](Self::emitir).
    pub async fn consultar_edicao(&self, codigo_edicao: &str) -> Result<ConsultaEdicao> {
        let codigo = codigo(codigo_edicao)?;
        let request =
            ApiRequest::new(endpoint::cobranca::EDICAO).path_param("codigoEdicao", codigo);
        self.client.execute(request).await
    }

    /// Pays a charge in the sandbox, to test the whole flow (`POST
    /// /cobranca/v3/cobrancas/{codigoSolicitacao}/pagar`, scope
    /// `boleto-cobranca.write`). The operation exists only in the sandbox.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] when the client was not built for
    /// [`Environment::Sandbox`] or the code is invalid (nothing is sent);
    /// otherwise the same as [`emitir`](Self::emitir).
    pub async fn pagar_no_sandbox(&self, codigo_solicitacao: &str, com: PagarCom) -> Result<()> {
        if self.client.environment() != Some(Environment::Sandbox) {
            return Err(Error::InvalidInput(
                "o pagamento de cobranças pela API existe só no sandbox".into(),
            ));
        }
        let codigo = codigo(codigo_solicitacao)?;
        let request = ApiRequest::new(endpoint::cobranca::PAGAR)
            .path_param("codigoSolicitacao", codigo)
            .json(json!({ "pagarCom": com.as_str() }))
            .retry(RetryMode::WhenNotProcessed);
        self.client.execute_empty(request).await
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

/// Whether [`Cobranca::listar_todas`] reads the page after `pagina` (page
/// `numero`, 0-based), with `lidas` charges read up to it. The listing is
/// complete once every charge its pages announce arrived: ending short of
/// that total, or going on past `max_paginas`, is an error.
fn ler_mais(pagina: &PaginaCobrancas, numero: u32, lidas: usize, max_paginas: u32) -> Result<bool> {
    let lidas = lidas as u64;
    let total = pagina.total_elementos;
    if total.is_some_and(|total| lidas >= total) {
        return Ok(false);
    }
    if pagina.cobrancas.is_empty() || !pagina.tem_mais(numero) {
        return match total {
            Some(total) => Err(listagem_error(format!(
                "a listagem terminou com {lidas} de {total} cobranças; execute o comando novamente"
            ))),
            None => Ok(false),
        };
    }
    if numero + 1 >= max_paginas {
        return Err(listagem_error(format!(
            "a API indicou mais cobranças depois de {max_paginas} páginas ({lidas} cobranças); \
             execute o comando novamente"
        )));
    }
    Ok(true)
}

fn listagem_error(message: String) -> Error {
    Error::Decode {
        operation: endpoint::cobranca::LISTAR.to_string(),
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pagina(valor: serde_json::Value) -> PaginaCobrancas {
        serde_json::from_value(valor).unwrap()
    }

    #[test]
    fn a_listing_ends_only_with_every_charge_it_announces() {
        let item = json!({"cobranca": {"seuNumero": "NF-1"}});
        // The total arrived: complete, whatever the page says.
        let completa =
            pagina(json!({"totalElementos": 2, "ultimaPagina": false, "cobrancas": [item]}));
        assert!(!ler_mais(&completa, 1, 2, MAX_PAGINAS).unwrap());
        // Without a total, the last page or an empty one ends the listing.
        let ultima = pagina(json!({"ultimaPagina": true, "cobrancas": [item]}));
        assert!(!ler_mais(&ultima, 0, 1, MAX_PAGINAS).unwrap());
        let vazia = pagina(json!({"cobrancas": []}));
        assert!(!ler_mais(&vazia, 3, 3000, MAX_PAGINAS).unwrap());
        let seguinte = pagina(json!({"ultimaPagina": false, "cobrancas": [item]}));
        assert!(ler_mais(&seguinte, 0, 1, MAX_PAGINAS).unwrap());

        // Ending short of the total is an error, also on an empty page.
        let curta =
            pagina(json!({"totalElementos": 2500, "ultimaPagina": true, "cobrancas": [item]}));
        let erro = ler_mais(&curta, 0, 1000, MAX_PAGINAS).unwrap_err();
        assert!(matches!(erro, Error::Decode { .. }), "{erro:?}");
        assert!(
            erro.to_string().contains("1000 de 2500 cobranças"),
            "{erro}"
        );
        let vazia = pagina(json!({"totalElementos": 2500, "ultimaPagina": false, "cobrancas": []}));
        assert!(ler_mais(&vazia, 1, 1000, MAX_PAGINAS).is_err());

        // So is going on past the ceiling, with or without a total.
        for total in [json!(null), json!(5000)] {
            let sem_fim = pagina(
                json!({"totalElementos": total, "ultimaPagina": false, "cobrancas": [item]}),
            );
            assert!(ler_mais(&sem_fim, 1, 2, 3).unwrap());
            let erro = ler_mais(&sem_fim, 2, 3, 3).unwrap_err();
            assert!(erro.to_string().contains("depois de 3 páginas"), "{erro}");
        }
    }

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
