//! Pix Automático (`/pix/v2`): recurring charges the payer authorizes once.
//!
//! The receiver creates a recurrence ([`RecSolicitada`]) with the contract,
//! the period and the frequency; the payer approves it in their bank, by
//! the QR Code of the recurrence or by a confirmation request; then each
//! recurring charge is created against it. The API is available only to
//! CNPJs with at least 6 months of activity.
//!
//! Obtained with [`InterClient::pix_automatico`].

mod rec;

pub use rec::{
    AtivacaoRec, AtivacaoSolicitada, AtualizacaoRec, CalendarioRec, CalendarioRecGerado,
    CancelamentoRec, DadosJornada, DadosQrRec, EncerramentoRec, FiltroRecs, ID_REC_TAMANHO, IdRec,
    IdRecError, LocationRec, MAX_CONTRATO, MAX_CONVENIO, MAX_NOME_DEVEDOR, MAX_OBJETO, PagadorRec,
    PaginaRecs, ParametrosConsultaRec, Periodicidade, PoliticaRetentativa, Rec, RecRevisada,
    RecSolicitada, RecebedorRec, RejeicaoRec, StatusRec, TipoJornada, ValorRec, ValorRecGerado,
    VinculoRec, VinculoRecGerado,
};

use crate::client::InterClient;

/// Operations of Pix Automático. Obtained with
/// [`InterClient::pix_automatico`].
#[derive(Debug, Clone, Copy)]
pub struct PixAutomatico<'a> {
    client: &'a InterClient,
}

impl<'a> PixAutomatico<'a> {
    pub(crate) fn new(client: &'a InterClient) -> Self {
        Self { client }
    }
}
