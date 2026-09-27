//! Recurring charges (`/pix/v2/cobr`): each payment of an approved
//! recurrence, one per cycle, which the payer's bank schedules and settles
//! on the due date.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::json;

use super::{
    EncerramentoRec, IdRec, MAX_AGENCIA, MAX_CONTA, PixAutomatico, PoliticaRetentativa,
    RecebedorRec, RejeicaoRec, agencia, conta, convenio,
};
use crate::client::ApiRequest;
use crate::cobranca::Uf;
use crate::documento::Documento;
use crate::endpoint;
use crate::error::{Error, Result};
use crate::pix::{
    CobrancaPixError, ITENS_POR_PAGINA_MAXIMO_PIX, Paginacao, PeriodoPix, PixRecebido, Txid,
    contato, corpo, paginada, texto, todas, valor,
};
use crate::retry::RetryMode;
use crate::serde_util::{api_enum, decimal_texto, lenient, string_serde};

/// Longest additional information of a recurring charge (`infoAdicional`).
pub const MAX_INFO_ADICIONAL: usize = 140;

api_enum! {
    /// Where a recurring charge stands (`status`).
    pub enum StatusCobR {
        /// Created by the receiver.
        Criada => "CRIADA",
        /// Accepted by the payer's bank, which scheduled the payment.
        Ativa => "ATIVA",
        /// Paid.
        Concluida => "CONCLUIDA",
        /// Not paid after every attempt allowed.
        Expirada => "EXPIRADA",
        /// Rejected by the payer's bank.
        Rejeitada => "REJEITADA",
        /// Cancelled.
        Cancelada => "CANCELADA",
    }
}

api_enum! {
    /// Kind of the receiver's account (`tipoConta`).
    pub enum TipoContaRecebedor {
        /// Checking account.
        Corrente => "CORRENTE",
        /// Savings account.
        Poupanca => "POUPANCA",
        /// Payment account.
        Pagamento => "PAGAMENTO",
    }
}

api_enum! {
    /// Kind of an attempt to settle a recurring charge (`tentativas.tipo`).
    pub enum TipoTentativa {
        /// The original scheduling of the debit.
        Agendamento => "AGND",
        /// A new attempt after the due date.
        NovaTentativa => "NTAG",
        /// The payment order sent again after a settlement error.
        Reenvio => "RIFL",
    }
}

api_enum! {
    /// Where an attempt to settle a recurring charge stands
    /// (`tentativas.status`).
    pub enum StatusTentativa {
        /// Scheduled, but not paid by the due date.
        Solicitada => "SOLICITADA",
        /// Accepted by the payer's bank, which scheduled the payment.
        Agendada => "AGENDADA",
        /// Paid.
        Paga => "PAGA",
        /// Cancelled.
        Cancelada => "CANCELADA",
        /// Rejected by the payer's bank.
        Rejeitada => "REJEITADA",
        /// Not paid after every attempt allowed.
        Expirada => "EXPIRADA",
    }
}

string_serde!(
    StatusCobR,
    TipoContaRecebedor,
    TipoTentativa,
    StatusTentativa
);

/// A new recurring charge (`CobRSolicitada`), checked by
/// [`validar`](Self::validar) before being sent. The recurrence must have
/// been approved by the payer, and only one charge is allowed per cycle.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CobRSolicitada {
    /// The recurrence.
    pub id_rec: IdRec,
    /// The due date (`calendario.dataDeVencimento`).
    pub vencimento: NaiveDate,
    /// The amount (`valor.original`).
    pub valor: Decimal,
    /// Whether a due date on a non-working day moves to the next working
    /// day, by the holidays of the payer's city (`ajusteDiaUtil`).
    pub ajuste_dia_util: bool,
    /// The receiver's account that gets the money.
    pub recebedor: ContaRecebedor,
    /// Information about the invoice, up to [`MAX_INFO_ADICIONAL`]
    /// characters.
    pub info_adicional: Option<String>,
    /// The payer's e-mail and address.
    pub devedor: Option<DevedorCobR>,
}

impl CobRSolicitada {
    /// The charge of `valor`, due on `vencimento`, of `id_rec`, into
    /// `recebedor`, moved to the next working day when needed.
    pub fn new(
        id_rec: IdRec,
        vencimento: NaiveDate,
        valor: Decimal,
        recebedor: ContaRecebedor,
    ) -> Self {
        Self {
            id_rec,
            vencimento,
            valor,
            ajuste_dia_util: true,
            recebedor,
            info_adicional: None,
            devedor: None,
        }
    }

    /// Checks what the documentation defines: the amount, the account, the
    /// sizes of the texts and the payer's address.
    ///
    /// # Errors
    ///
    /// The first problem, naming the field as the API does.
    pub fn validar(&self) -> Result<(), CobrancaPixError> {
        valor(self.valor, "valor.original", false)?;
        self.recebedor.validar()?;
        if let Some(info) = &self.info_adicional {
            texto(info, "infoAdicional", MAX_INFO_ADICIONAL)?;
        }
        if let Some(devedor) = &self.devedor {
            devedor.validar()?;
        }
        Ok(())
    }
}

impl Serialize for CobRSolicitada {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("idRec", &self.id_rec)?;
        if let Some(info) = &self.info_adicional {
            map.serialize_entry("infoAdicional", info)?;
        }
        map.serialize_entry(
            "calendario",
            &json!({ "dataDeVencimento": self.vencimento.to_string() }),
        )?;
        map.serialize_entry(
            "valor",
            &json!({ "original": format!("{:.2}", self.valor) }),
        )?;
        map.serialize_entry("ajusteDiaUtil", &self.ajuste_dia_util)?;
        if let Some(devedor) = &self.devedor {
            map.serialize_entry("devedor", devedor)?;
        }
        map.serialize_entry("recebedor", &self.recebedor)?;
        map.end()
    }
}

/// The receiver's account of a recurring charge (`recebedor`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ContaRecebedor {
    /// The account number with its check digit, digits only (the check
    /// digit may be `X`), up to [`MAX_CONTA`] characters.
    pub conta: String,
    /// The kind of account.
    pub tipo_conta: TipoContaRecebedor,
    /// The branch, digits only, up to [`MAX_AGENCIA`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agencia: Option<String>,
}

impl ContaRecebedor {
    /// The account `conta` of the kind `tipo_conta`.
    pub fn new(conta: impl Into<String>, tipo_conta: TipoContaRecebedor) -> Self {
        Self {
            conta: conta.into(),
            tipo_conta,
            agencia: None,
        }
    }

    fn validar(&self) -> Result<(), CobrancaPixError> {
        conta(&self.conta, "recebedor.conta", MAX_CONTA)?;
        if let TipoContaRecebedor::Outro(outro) = &self.tipo_conta {
            return Err(CobrancaPixError::new(
                "recebedor.tipoConta",
                format!(
                    "tipo de conta desconhecido: \"{outro}\"; use CORRENTE, POUPANCA ou PAGAMENTO"
                ),
            ));
        }
        if let Some(numero) = &self.agencia {
            agencia(numero, "recebedor.agencia", MAX_AGENCIA)?;
        }
        Ok(())
    }
}

/// The payer's e-mail and address in a recurring charge (`devedor`); the
/// payer themselves is the one of the recurrence.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct DevedorCobR {
    /// E-mail.
    pub email: Option<String>,
    /// Street and number, up to 200 characters.
    pub logradouro: Option<String>,
    /// City, up to 200 characters.
    pub cidade: Option<String>,
    /// State.
    pub uf: Option<Uf>,
    /// Postal code (CEP), 8 digits.
    pub cep: Option<String>,
}

impl DevedorCobR {
    fn validar(&self) -> Result<(), CobrancaPixError> {
        contato(
            self.email.as_deref(),
            self.logradouro.as_deref(),
            self.cidade.as_deref(),
            self.cep.as_deref(),
        )
    }
}

impl Serialize for DevedorCobR {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        if let Some(email) = &self.email {
            map.serialize_entry("email", email)?;
        }
        if let Some(logradouro) = &self.logradouro {
            map.serialize_entry("logradouro", logradouro)?;
        }
        if let Some(cidade) = &self.cidade {
            map.serialize_entry("cidade", cidade)?;
        }
        if let Some(uf) = &self.uf {
            map.serialize_entry("uf", uf.as_str())?;
        }
        if let Some(cep) = &self.cep {
            map.serialize_entry("cep", cep)?;
        }
        map.end()
    }
}

/// A recurring charge, as received.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct CobR {
    /// The recurrence.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub id_rec: Option<String>,
    /// Its txid.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub txid: Option<String>,
    /// Information about the invoice.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub info_adicional: Option<String>,
    /// When it was created and when it is due.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calendario: Option<CalendarioCobR>,
    /// The amount.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valor: Option<ValorCobR>,
    /// Where it stands.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<StatusCobR>,
    /// Whether it may be tried again, by the recurrence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub politica_retentativa: Option<PoliticaRetentativa>,
    /// Whether a due date on a non-working day moves to the next one.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::bool"
    )]
    pub ajuste_dia_util: Option<bool>,
    /// The payer's e-mail and address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub devedor: Option<DevedorCobRGerado>,
    /// The receiver's account.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recebedor: Option<RecebedorCobR>,
    /// The attempts to settle it.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "lenient::vec"
    )]
    pub tentativas: Vec<TentativaCobR>,
    /// Why it ended: rejected or cancelled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encerramento: Option<EncerramentoRec>,
    /// The Pix that paid it, with their refunds.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "lenient::vec"
    )]
    pub pix: Vec<PixRecebido>,
    /// The changes of status, with their times.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "lenient::vec"
    )]
    pub atualizacao: Vec<AtualizacaoCobR>,
}

/// When a recurring charge was created and when it is due, as received.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct CalendarioCobR {
    /// The day it was created.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub criacao: Option<String>,
    /// The due date.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub data_de_vencimento: Option<String>,
}

/// The amount of a recurring charge, as received.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ValorCobR {
    /// The amount.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::decimal",
        serialize_with = "decimal_texto::serialize_option"
    )]
    pub original: Option<Decimal>,
}

/// The payer's e-mail and address in a recurring charge, as received.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct DevedorCobRGerado {
    /// E-mail.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub email: Option<String>,
    /// Street and number.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub logradouro: Option<String>,
    /// City.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub cidade: Option<String>,
    /// State.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub uf: Option<String>,
    /// Postal code.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub cep: Option<String>,
}

/// The receiver's account of a recurring charge, as received.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct RecebedorCobR {
    /// The account number.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub conta: Option<String>,
    /// The kind of account.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tipo_conta: Option<TipoContaRecebedor>,
    /// The branch.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub agencia: Option<String>,
    /// The receiver's CNPJ.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub cnpj: Option<String>,
    /// The receiver's name.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub nome: Option<String>,
}

/// An attempt to settle a recurring charge, as received.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct TentativaCobR {
    /// The day of the settlement.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub data_liquidacao: Option<String>,
    /// The kind of attempt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tipo: Option<TipoTentativa>,
    /// The end-to-end id of the payment.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub end_to_end_id: Option<String>,
    /// Where it stands.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<StatusTentativa>,
    /// Why the payer's bank rejected it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rejeicao: Option<RejeicaoRec>,
    /// Its changes of status, with their times.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "lenient::vec"
    )]
    pub atualizacao: Vec<AtualizacaoTentativa>,
}

/// A change of status of an attempt, as received.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct AtualizacaoTentativa {
    /// The new status.
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "nome")]
    pub status: Option<StatusTentativa>,
    /// When (RFC 3339).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub data: Option<String>,
}

/// A change of status of a recurring charge, as received.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct AtualizacaoCobR {
    /// The new status.
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "nome")]
    pub status: Option<StatusCobR>,
    /// When (RFC 3339).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub data: Option<String>,
}

/// Which recurring charges to list: those created in a period, with
/// optional filters.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct FiltroCobsR {
    /// When they were created.
    pub periodo: PeriodoPix,
    /// Only those of this recurrence.
    pub id_rec: Option<IdRec>,
    /// Only those of this payer.
    pub devedor: Option<Documento>,
    /// Only those in this status.
    pub status: Option<StatusCobR>,
    /// Only those of this agreement, up to
    /// [`MAX_CONVENIO`](super::MAX_CONVENIO) characters.
    pub convenio: Option<String>,
}

impl FiltroCobsR {
    /// Every recurring charge created in `periodo`.
    pub fn new(periodo: PeriodoPix) -> Self {
        Self {
            periodo,
            id_rec: None,
            devedor: None,
            status: None,
            convenio: None,
        }
    }

    fn query(&self) -> Result<Vec<(&'static str, String)>> {
        let mut query = Vec::from(self.periodo.query());
        if let Some(id) = &self.id_rec {
            query.push(("idRec", id.as_str().to_owned()));
        }
        match &self.devedor {
            Some(Documento::Cpf(cpf)) => query.push(("cpf", cpf.clone())),
            Some(Documento::Cnpj(cnpj)) => query.push(("cnpj", cnpj.clone())),
            None => {}
        }
        if let Some(status) = &self.status {
            query.push(("status", status.as_str().to_owned()));
        }
        if let Some(filtro) = &self.convenio {
            query.push(("convenio", convenio(filtro)?));
        }
        Ok(query)
    }
}

/// A page of recurring charges, as received.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct PaginaCobsR {
    /// The filters and the page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parametros: Option<ParametrosConsultaCobR>,
    /// The recurring charges.
    #[serde(default, deserialize_with = "lenient::vec")]
    pub cobsr: Vec<CobR>,
}

/// The filters and the page of a listing of recurring charges, as
/// received.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ParametrosConsultaCobR {
    /// Start of the period (RFC 3339).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub inicio: Option<String>,
    /// End of the period (RFC 3339).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub fim: Option<String>,
    /// The recurrence of the filter.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub id_rec: Option<String>,
    /// The payer's CPF of the filter.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub cpf: Option<String>,
    /// The payer's CNPJ of the filter.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub cnpj: Option<String>,
    /// The status of the filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<StatusCobR>,
    /// The agreement of the filter (`recebedor.convenio`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recebedor: Option<RecebedorRec>,
    /// The page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paginacao: Option<Paginacao>,
}

impl PixAutomatico<'_> {
    /// Creates a recurring charge with your txid (`PUT
    /// /pix/v2/cobr/{txid}`, scope `cobr.write`). The recurrence must have
    /// been approved by the payer, and only one charge is allowed per cycle
    /// of the recurrence.
    ///
    /// The charge is checked with [`CobRSolicitada::validar`] before
    /// anything is sent. It is repeated automatically only when it surely
    /// was not processed; after an unknown outcome, repeat it with the same
    /// txid, which does not create a second charge.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] with a [`CobrancaPixError`] when the charge is
    /// invalid (nothing is sent); otherwise, failures to obtain a token, to
    /// send the request or to decode the answer, and the API's error
    /// statuses.
    pub async fn criar_cobr(&self, txid: &Txid, cobr: &CobRSolicitada) -> Result<CobR> {
        cobr.validar()
            .map_err(|err| Error::InvalidInput(Box::new(err)))?;
        let request = ApiRequest::new(endpoint::pix_automatico::CRIAR_COBR)
            .path_param("txid", txid.as_str().to_owned())
            .json(corpo(cobr)?)
            .retry(RetryMode::WhenNotProcessed);
        self.client.execute(request).await
    }

    /// Creates a recurring charge whose txid Inter chooses (`POST
    /// /pix/v2/cobr`, scope `cobr.write`).
    ///
    /// There is no idempotency key: the request is repeated automatically
    /// only when it surely was not processed, and after an unknown outcome
    /// the charges of the recurrence should be listed before trying again.
    /// [`criar_cobr`](Self::criar_cobr), with your txid, can be repeated.
    ///
    /// # Errors
    ///
    /// The same as [`criar_cobr`](Self::criar_cobr).
    pub async fn criar_cobr_sem_txid(&self, cobr: &CobRSolicitada) -> Result<CobR> {
        cobr.validar()
            .map_err(|err| Error::InvalidInput(Box::new(err)))?;
        let request = ApiRequest::new(endpoint::pix_automatico::CRIAR_COBR_SEM_TXID)
            .json(corpo(cobr)?)
            .retry(RetryMode::WhenNotProcessed);
        self.client.execute(request).await
    }

    /// A recurring charge (`GET /pix/v2/cobr/{txid}`, scope `cobr.read`),
    /// with its attempts and the Pix that paid it.
    ///
    /// # Errors
    ///
    /// The same as [`criar_cobr`](Self::criar_cobr); unknown charges fail
    /// with status `404`.
    pub async fn consultar_cobr(&self, txid: &Txid) -> Result<CobR> {
        let request = ApiRequest::new(endpoint::pix_automatico::CONSULTAR_COBR)
            .path_param("txid", txid.as_str().to_owned());
        self.client.execute(request).await
    }

    /// One page of the recurring charges created in a period (`GET
    /// /pix/v2/cobr`, scope `cobr.read`).
    ///
    /// `pagina` starts at 0; without `itens_por_pagina`, the API returns 100
    /// per page, and it accepts up to [`ITENS_POR_PAGINA_MAXIMO_PIX`].
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] when `itens_por_pagina` is out of range or
    /// the agreement of the filter is too long (nothing is sent);
    /// otherwise the same as [`consultar_cobr`](Self::consultar_cobr).
    pub async fn listar_cobrs(
        &self,
        filtro: &FiltroCobsR,
        pagina: u32,
        itens_por_pagina: Option<u32>,
    ) -> Result<PaginaCobsR> {
        let request = paginada(
            ApiRequest::new(endpoint::pix_automatico::LISTAR_COBRS).queries(filtro.query()?),
            pagina,
            itens_por_pagina,
        )?;
        self.client.execute(request).await
    }

    /// Every recurring charge created in a period, reading as many pages of
    /// [`ITENS_POR_PAGINA_MAXIMO_PIX`] as needed.
    ///
    /// # Errors
    ///
    /// The same as [`listar_cobrs`](Self::listar_cobrs).
    pub async fn listar_todas_cobrs(&self, filtro: &FiltroCobsR) -> Result<Vec<CobR>> {
        todas("cobranças recorrentes", |pagina| async move {
            let pagina = self
                .listar_cobrs(filtro, pagina, Some(ITENS_POR_PAGINA_MAXIMO_PIX))
                .await?;
            let paginacao = pagina
                .parametros
                .and_then(|parametros| parametros.paginacao)
                .unwrap_or_default();
            Ok((pagina.cobsr, paginacao))
        })
        .await
    }

    /// Cancels a recurring charge (`PATCH /pix/v2/cobr/{txid}` with
    /// `status: CANCELADA`, scope `cobr.write`). By the Central Bank's rules,
    /// it must happen before 22:00 of the day before the settlement.
    /// Repeated automatically only when it surely was not processed.
    ///
    /// # Errors
    ///
    /// The same as [`consultar_cobr`](Self::consultar_cobr).
    pub async fn cancelar_cobr(&self, txid: &Txid) -> Result<CobR> {
        let request = ApiRequest::new(endpoint::pix_automatico::REVISAR_COBR)
            .path_param("txid", txid.as_str().to_owned())
            .json(json!({ "status": StatusCobR::Cancelada }))
            .retry(RetryMode::WhenNotProcessed);
        self.client.execute(request).await
    }

    /// Asks for a new attempt to settle a recurring charge not paid on its
    /// due date, on `data` (`POST /pix/v2/cobr/{txid}/retentativa/{data}`,
    /// scope `cobr.write`), when the policy of the recurrence allows it:
    /// up to 3 attempts, on different days, within 7 days. Repeated
    /// automatically only when it surely was not processed.
    ///
    /// # Errors
    ///
    /// The same as [`consultar_cobr`](Self::consultar_cobr).
    pub async fn solicitar_retentativa(&self, txid: &Txid, data: NaiveDate) -> Result<CobR> {
        let request = ApiRequest::new(endpoint::pix_automatico::RETENTATIVA_COBR)
            .path_param("txid", txid.as_str().to_owned())
            .path_param("data", data.to_string())
            .retry(RetryMode::WhenNotProcessed);
        self.client.execute(request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID_REC: &str = "RR1234567820260924abcdefghijk";

    fn cobr() -> CobRSolicitada {
        let mut recebedor = ContaRecebedor::new("1234567", TipoContaRecebedor::Corrente);
        recebedor.agencia = Some("0001".to_owned());
        let mut cobr = CobRSolicitada::new(
            ID_REC.parse().unwrap(),
            NaiveDate::from_ymd_opt(2026, 10, 10).unwrap(),
            "149.90".parse().unwrap(),
            recebedor,
        );
        cobr.info_adicional = Some("Mensalidade de outubro".to_owned());
        cobr.devedor = Some(DevedorCobR {
            email: Some("cliente@empresa.example".to_owned()),
            logradouro: Some("Rua Exemplo, 100".to_owned()),
            cidade: Some("São Paulo".to_owned()),
            uf: Some(Uf::Sp),
            cep: Some("01001000".to_owned()),
        });
        cobr
    }

    #[test]
    fn a_charge_is_sent_as_the_api_names_it() {
        let cobr = cobr();
        cobr.validar().unwrap();
        assert_eq!(
            serde_json::to_value(&cobr).unwrap(),
            json!({
                "idRec": ID_REC,
                "infoAdicional": "Mensalidade de outubro",
                "calendario": {"dataDeVencimento": "2026-10-10"},
                "valor": {"original": "149.90"},
                "ajusteDiaUtil": true,
                "devedor": {"email": "cliente@empresa.example", "logradouro": "Rua Exemplo, 100", "cidade": "São Paulo", "uf": "SP", "cep": "01001000"},
                "recebedor": {"conta": "1234567", "tipoConta": "CORRENTE", "agencia": "0001"}
            })
        );
    }

    #[test]
    fn what_the_api_would_refuse_is_named() {
        let campo = |mudar: fn(&mut CobRSolicitada)| {
            let mut cobr = cobr();
            mudar(&mut cobr);
            cobr.validar().map_err(|err| err.campo().to_owned())
        };
        let erro = |campo: &str| Err(campo.to_owned());
        assert_eq!(campo(|c| c.valor = Decimal::ZERO), erro("valor.original"));
        assert_eq!(
            campo(|c| c.recebedor.conta = "1234-5".to_owned()),
            erro("recebedor.conta")
        );
        assert_eq!(
            campo(|c| c.recebedor.tipo_conta = TipoContaRecebedor::from("POUPANÇA")),
            erro("recebedor.tipoConta")
        );
        assert_eq!(
            campo(|c| c.recebedor.agencia = Some("12345".to_owned())),
            erro("recebedor.agencia")
        );
        assert_eq!(
            campo(|c| c.info_adicional = Some("x".repeat(141))),
            erro("infoAdicional")
        );
        assert_eq!(
            campo(|c| c.devedor.as_mut().unwrap().cep = Some("01001-000".to_owned())),
            erro("devedor.cep")
        );
        assert_eq!(
            campo(|c| c.devedor.as_mut().unwrap().email = Some("cliente".to_owned())),
            erro("devedor.email")
        );
        assert_eq!(campo(|c| c.devedor = None), Ok(()));
    }

    #[test]
    fn answers_are_read_as_the_documentation_shows_them() {
        let cobr: CobR = serde_json::from_value(json!({
            "idRec": ID_REC,
            "txid": "7978c0c97ea847e78e8849634473c1f1",
            "calendario": {"criacao": "2026-09-24", "dataDeVencimento": "2026-10-10"},
            "valor": {"original": "149.90"},
            "status": "ATIVA",
            "recebedor": {"conta": 1_234_567, "tipoConta": "POUPANÇA", "cnpj": "12345678000195"},
            "tentativas": [{"dataLiquidacao": "2026-10-10", "tipo": "AGND", "status": "AGENDADA", "atualizacao": [{"status": "SOLICITADA", "data": "2026-09-25T10:00:00Z"}]}],
            "atualizacao": [{"status": "CRIADA", "data": "2026-09-24T10:00:00Z"}]
        }))
        .unwrap();
        let recebedor = cobr.recebedor.unwrap();
        assert_eq!(recebedor.conta.as_deref(), Some("1234567"));
        // The documentation's examples also write the kind with a cedilla.
        assert_eq!(
            recebedor.tipo_conta,
            Some(TipoContaRecebedor::Outro("POUPANÇA".to_owned()))
        );
        assert_eq!(cobr.tentativas[0].tipo, Some(TipoTentativa::Agendamento));
        assert_eq!(
            cobr.tentativas[0].atualizacao[0].status,
            Some(StatusTentativa::Solicitada)
        );
        assert_eq!(
            cobr.valor.unwrap().original,
            Some("149.90".parse().unwrap())
        );
    }

    #[test]
    fn filters_go_in_the_query() {
        use chrono::DateTime;
        let mut filtro = FiltroCobsR::new(
            PeriodoPix::new(
                DateTime::parse_from_rfc3339("2026-09-01T00:00:00-03:00").unwrap(),
                DateTime::parse_from_rfc3339("2026-09-30T23:59:59-03:00").unwrap(),
            )
            .unwrap(),
        );
        filtro.id_rec = Some(ID_REC.parse().unwrap());
        filtro.devedor = Some(Documento::parse("123.456.789-09").unwrap());
        filtro.status = Some(StatusCobR::Ativa);
        assert_eq!(
            filtro.query().unwrap(),
            [
                ("inicio", "2026-09-01T00:00:00-03:00".to_owned()),
                ("fim", "2026-09-30T23:59:59-03:00".to_owned()),
                ("idRec", ID_REC.to_owned()),
                ("cpf", "12345678909".to_owned()),
                ("status", "ATIVA".to_owned()),
            ]
        );
    }
}
