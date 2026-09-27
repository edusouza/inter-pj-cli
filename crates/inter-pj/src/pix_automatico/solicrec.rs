//! Confirmation requests (`/pix/v2/solicrec`): a recurrence sent to the
//! payer's bank, which asks them to approve it. The answer shows in the
//! status of the recurrence: `APROVADA` when the payer accepts it,
//! `REJEITADA` (with the reason in `encerramento`) when they refuse it.

use chrono::{DateTime, FixedOffset, SecondsFormat, Timelike};
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::json;

use super::{IdRec, PixAutomatico, Rec};
use crate::client::ApiRequest;
use crate::documento::Documento;
use crate::endpoint;
use crate::error::{Error, Result};
use crate::pix::{CobrancaPixError, corpo};
use crate::retry::RetryMode;
use crate::serde_util::{api_enum, lenient, string_serde};

/// Longest account number of the payer (`destinatario.conta`).
pub const MAX_CONTA: usize = 20;

/// Longest branch of the payer (`destinatario.agencia`).
pub const MAX_AGENCIA: usize = 4;

identificador! {
    /// Identifier of a confirmation request (`idSolicRec`), as the API
    /// creates it: 29 letters and digits, case sensitive
    /// (`SC1234567820240115abcdefghijk`: `SC`, the ISPB, the date and 11
    /// characters).
    IdSolicRec, IdSolicRecError, "idSolicRec", "SC1234567820240115abcdefghijk"
}

api_enum! {
    /// Where a confirmation request stands (`status`).
    pub enum StatusSolicRec {
        /// Created, waiting to be sent.
        Criada => "CRIADA",
        /// Sent to the payer.
        Enviada => "ENVIADA",
        /// Received by the payer.
        Recebida => "RECEBIDA",
        /// Rejected by the payer.
        Rejeitada => "REJEITADA",
        /// Accepted by the payer.
        Aceita => "ACEITA",
        /// Expired without an answer.
        Expirada => "EXPIRADA",
        /// Cancelled.
        Cancelada => "CANCELADA",
    }
}

string_serde!(StatusSolicRec);

/// A new confirmation request (`SolicRecSolicitada`): the recurrence, until
/// when the payer may answer and the account of the payer, checked by
/// [`validar`](Self::validar) before being sent.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SolicRecSolicitada {
    /// The recurrence to approve.
    pub id_rec: IdRec,
    /// Until when the payer may answer (`calendario.dataExpiracaoSolicitacao`).
    pub expiracao: DateTime<FixedOffset>,
    /// The account of the payer, whose bank asks them.
    pub destinatario: DestinatarioSolicRec,
}

impl SolicRecSolicitada {
    /// The request that `destinatario` approves `id_rec` until `expiracao`.
    pub fn new(
        id_rec: IdRec,
        expiracao: DateTime<FixedOffset>,
        destinatario: DestinatarioSolicRec,
    ) -> Self {
        Self {
            id_rec,
            expiracao,
            destinatario,
        }
    }

    /// Checks the account of the payer: digits, sizes and the ISPB.
    ///
    /// # Errors
    ///
    /// The first problem, naming the field as the API does.
    pub fn validar(&self) -> Result<(), CobrancaPixError> {
        self.destinatario.validar()
    }
}

impl Serialize for SolicRecSolicitada {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        json!({
            "idRec": self.id_rec,
            "calendario": {"dataExpiracaoSolicitacao": momento(self.expiracao)},
            "destinatario": self.destinatario,
        })
        .serialize(serializer)
    }
}

/// RFC 3339, with milliseconds only when there are any
/// (`2026-10-01T12:00:00-03:00`, `2026-10-01T12:00:00.250Z`).
fn momento(momento: DateTime<FixedOffset>) -> String {
    let formato = if momento.nanosecond() == 0 {
        SecondsFormat::Secs
    } else {
        SecondsFormat::Millis
    };
    momento.to_rfc3339_opts(formato, true)
}

/// The account of the payer of a confirmation request (`destinatario`).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct DestinatarioSolicRec {
    /// CPF or CNPJ of the payer, sent as `cpf` or `cnpj`.
    pub documento: Documento,
    /// The account number with its check digit, digits only (the check
    /// digit may be `X`), up to [`MAX_CONTA`] characters.
    pub conta: String,
    /// The ISPB of the payer's bank: 8 digits.
    pub ispb_participante: String,
    /// The branch, digits only, up to [`MAX_AGENCIA`].
    pub agencia: Option<String>,
}

impl DestinatarioSolicRec {
    /// The account `conta` of `documento` at the bank `ispb_participante`.
    pub fn new(
        documento: Documento,
        conta: impl Into<String>,
        ispb_participante: impl Into<String>,
    ) -> Self {
        Self {
            documento,
            conta: conta.into(),
            ispb_participante: ispb_participante.into(),
            agencia: None,
        }
    }

    fn validar(&self) -> Result<(), CobrancaPixError> {
        let digitos = |texto: &str| !texto.is_empty() && texto.bytes().all(|b| b.is_ascii_digit());
        let sem_dv = self.conta.strip_suffix(['X', 'x']).unwrap_or(&self.conta);
        if !digitos(sem_dv) || self.conta.len() > MAX_CONTA {
            return Err(CobrancaPixError::new(
                "destinatario.conta",
                format!(
                    "até {MAX_CONTA} dígitos, com o dígito verificador (que pode ser X), sem pontos nem traços"
                ),
            ));
        }
        if self.ispb_participante.len() != 8 || !digitos(&self.ispb_participante) {
            return Err(CobrancaPixError::new(
                "destinatario.ispbParticipante",
                "o ISPB do banco tem 8 dígitos",
            ));
        }
        if let Some(agencia) = &self.agencia
            && (!digitos(agencia) || agencia.len() > MAX_AGENCIA)
        {
            return Err(CobrancaPixError::new(
                "destinatario.agencia",
                format!("até {MAX_AGENCIA} dígitos, sem o dígito verificador"),
            ));
        }
        Ok(())
    }
}

impl Serialize for DestinatarioSolicRec {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        match &self.documento {
            Documento::Cpf(cpf) => map.serialize_entry("cpf", cpf)?,
            Documento::Cnpj(cnpj) => map.serialize_entry("cnpj", cnpj)?,
        }
        map.serialize_entry("conta", &self.conta)?;
        map.serialize_entry("ispbParticipante", &self.ispb_participante)?;
        if let Some(agencia) = &self.agencia {
            map.serialize_entry("agencia", agencia)?;
        }
        map.end()
    }
}

/// A confirmation request, as received.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct SolicRec {
    /// Its identifier.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub id_solic_rec: Option<String>,
    /// The recurrence.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub id_rec: Option<String>,
    /// Until when the payer may answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calendario: Option<CalendarioSolicRec>,
    /// Where it stands.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<StatusSolicRec>,
    /// The account of the payer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destinatario: Option<DestinatarioSolicRecGerado>,
    /// The changes of status, with their times.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "lenient::vec"
    )]
    pub atualizacao: Vec<AtualizacaoSolicRec>,
    /// The recurrence as the payer sees it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rec_payload: Option<Rec>,
}

/// Until when the payer may answer a confirmation request, as received.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct CalendarioSolicRec {
    /// When the request expires (RFC 3339).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub data_expiracao_solicitacao: Option<String>,
}

/// The account of the payer of a confirmation request, as received.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct DestinatarioSolicRecGerado {
    /// CPF, for people.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub cpf: Option<String>,
    /// CNPJ, for companies.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub cnpj: Option<String>,
    /// The account number.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub conta: Option<String>,
    /// The ISPB of the bank.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub ispb_participante: Option<String>,
    /// The branch.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub agencia: Option<String>,
}

/// A change of status of a confirmation request, as received.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct AtualizacaoSolicRec {
    /// The new status.
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "nome")]
    pub status: Option<StatusSolicRec>,
    /// When (RFC 3339).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub data: Option<String>,
}

impl PixAutomatico<'_> {
    /// Sends a recurrence to the payer's bank, which asks them to approve
    /// it (`POST /pix/v2/solicrec`, scope `solicrec.write`). Follow the
    /// answer in the status of the request or of the recurrence.
    ///
    /// The request is checked with [`SolicRecSolicitada::validar`] before
    /// anything is sent, and repeated automatically only when it surely
    /// was not processed.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] with a [`CobrancaPixError`] when the request
    /// is invalid (nothing is sent); otherwise, failures to obtain a token,
    /// to send the request or to decode the answer, and the API's error
    /// statuses: `404` for an unknown recurrence.
    pub async fn criar_solicitacao(&self, solicitacao: &SolicRecSolicitada) -> Result<SolicRec> {
        solicitacao
            .validar()
            .map_err(|err| Error::InvalidInput(Box::new(err)))?;
        let request = ApiRequest::new(endpoint::pix_automatico::CRIAR_SOLICITACAO)
            .json(corpo(solicitacao)?)
            .retry(RetryMode::WhenNotProcessed);
        self.client.execute(request).await
    }

    /// A confirmation request (`GET /pix/v2/solicrec/{idSolicRec}`, scope
    /// `solicrec.read`).
    ///
    /// # Errors
    ///
    /// The same as [`criar_solicitacao`](Self::criar_solicitacao); unknown
    /// requests fail with status `404`.
    pub async fn consultar_solicitacao(&self, id: &IdSolicRec) -> Result<SolicRec> {
        let request = ApiRequest::new(endpoint::pix_automatico::CONSULTAR_SOLICITACAO)
            .path_param("idSolicRec", id.as_str().to_owned());
        self.client.execute(request).await
    }

    /// Cancels a confirmation request (`PATCH /pix/v2/solicrec/{idSolicRec}`
    /// with `status: CANCELADA`, scope `solicrec.write`); the API cancels
    /// only requests `CRIADA` or `RECEBIDA`. Repeated automatically only
    /// when it surely was not processed.
    ///
    /// # Errors
    ///
    /// The same as [`consultar_solicitacao`](Self::consultar_solicitacao).
    pub async fn cancelar_solicitacao(&self, id: &IdSolicRec) -> Result<SolicRec> {
        let request = ApiRequest::new(endpoint::pix_automatico::REVISAR_SOLICITACAO)
            .path_param("idSolicRec", id.as_str().to_owned())
            .json(json!({ "status": StatusSolicRec::Cancelada }))
            .retry(RetryMode::WhenNotProcessed);
        self.client.execute(request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID_REC: &str = "RR1234567820260924abcdefghijk";

    fn solicitacao() -> SolicRecSolicitada {
        let mut destinatario = DestinatarioSolicRec::new(
            Documento::parse("123.456.789-09").unwrap(),
            "1234567",
            "12345678",
        );
        destinatario.agencia = Some("0001".to_owned());
        SolicRecSolicitada::new(
            ID_REC.parse().unwrap(),
            DateTime::parse_from_rfc3339("2026-10-01T12:00:00-03:00").unwrap(),
            destinatario,
        )
    }

    #[test]
    fn ids_have_29_letters_and_digits() {
        let id = "SC1234567820260924abcdefghijk";
        assert_eq!(IdSolicRec::parse(id).unwrap().to_string(), id);
        let err = IdSolicRec::parse("SC123").unwrap_err();
        assert!(err.to_string().starts_with("idSolicRec inválido"), "{err}");
    }

    #[test]
    fn a_request_is_sent_as_the_api_names_it() {
        let solicitacao = solicitacao();
        solicitacao.validar().unwrap();
        assert_eq!(
            serde_json::to_value(&solicitacao).unwrap(),
            json!({
                "idRec": ID_REC,
                "calendario": {"dataExpiracaoSolicitacao": "2026-10-01T12:00:00-03:00"},
                "destinatario": {"cpf": "12345678909", "conta": "1234567", "ispbParticipante": "12345678", "agencia": "0001"}
            })
        );
        let expiracao = DateTime::parse_from_rfc3339("2026-10-01T15:00:00.250Z").unwrap();
        assert_eq!(momento(expiracao), "2026-10-01T15:00:00.250Z");
    }

    #[test]
    fn the_account_is_checked() {
        let campo = |mudar: fn(&mut DestinatarioSolicRec)| {
            let mut solicitacao = solicitacao();
            mudar(&mut solicitacao.destinatario);
            solicitacao.validar().map_err(|err| err.campo().to_owned())
        };
        assert_eq!(
            campo(|d| d.conta = "12345-6".to_owned()),
            Err("destinatario.conta".to_owned())
        );
        assert_eq!(
            campo(|d| d.conta = "1".repeat(21)),
            Err("destinatario.conta".to_owned())
        );
        assert_eq!(
            campo(|d| d.conta = String::new()),
            Err("destinatario.conta".to_owned())
        );
        assert_eq!(campo(|d| d.conta = "123456X".to_owned()), Ok(()));
        assert_eq!(
            campo(|d| d.ispb_participante = "1234567".to_owned()),
            Err("destinatario.ispbParticipante".to_owned())
        );
        assert_eq!(
            campo(|d| d.agencia = Some("00011".to_owned())),
            Err("destinatario.agencia".to_owned())
        );
        assert_eq!(campo(|d| d.agencia = None), Ok(()));
    }

    #[test]
    fn answers_are_read_as_the_documentation_shows_them() {
        let solicitacao: SolicRec = serde_json::from_value(json!({
            "idSolicRec": "SC1234567820260924abcdefghijk",
            "idRec": ID_REC,
            "calendario": {"dataExpiracaoSolicitacao": "2026-10-01T15:00:00.000Z"},
            "status": "REJEITADA",
            "destinatario": {"cpf": 12_345_678_909_u64, "conta": "1234567", "ispbParticipante": "12345678"},
            "atualizacao": [{"data": "2026-09-24T12:00:00Z", "status": "CRIADA"}, {"data": "2026-09-25T12:00:00Z", "nome": "REJEITADA"}],
            "recPayload": {"idRec": ID_REC, "calendario": {"dataInicial": "2026-10-10", "periodicidade": "MENSAL"}}
        }))
        .unwrap();
        assert_eq!(solicitacao.status, Some(StatusSolicRec::Rejeitada));
        assert_eq!(
            solicitacao.destinatario.unwrap().cpf.as_deref(),
            Some("12345678909")
        );
        assert_eq!(
            solicitacao.atualizacao[1].status,
            Some(StatusSolicRec::Rejeitada)
        );
        assert_eq!(
            solicitacao.rec_payload.unwrap().id_rec.as_deref(),
            Some(ID_REC)
        );
    }
}
