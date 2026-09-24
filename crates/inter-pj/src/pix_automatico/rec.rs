//! Recurrences (`/pix/v2/rec`): the payer's authorization of the recurring
//! charges, with the contract, the period, the frequency and the amount.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value, json};

use super::{PixAutomatico, SolicRec, convenio};
use crate::client::ApiRequest;
use crate::documento::Documento;
use crate::endpoint;
use crate::error::{Error, Result};
use crate::pix::{
    CobrancaPixError, Devedor, ITENS_POR_PAGINA_MAXIMO_PIX, Paginacao, PeriodoPix, PessoaPix, Txid,
    corpo, paginada, texto, todas, valor,
};
use crate::retry::RetryMode;
use crate::serde_util::{api_enum, decimal_texto, lenient, string_serde};

/// Longest description of what the payments are for (`vinculo.objeto`).
pub const MAX_OBJETO: usize = 35;

/// Longest contract code (`vinculo.contrato`).
pub const MAX_CONTRATO: usize = 35;

/// Longest name of the payer of a recurrence.
pub const MAX_NOME_DEVEDOR: usize = 140;

identificador! {
    /// Identifier of a recurrence (`idRec`), as the API creates it: 29
    /// letters and digits, case sensitive (`RR1234567820240115abcdefghijk`:
    /// `R`, then `R` or `N` as retries are allowed or not, the ISPB, the
    /// date and 11 characters).
    IdRec, IdRecError, "idRec", "RR1234567820240115abcdefghijk"
}

api_enum! {
    /// How often the payments happen (`periodicidade`).
    pub enum Periodicidade {
        /// Every week.
        Semanal => "SEMANAL",
        /// Every month.
        Mensal => "MENSAL",
        /// Every three months.
        Trimestral => "TRIMESTRAL",
        /// Every six months.
        Semestral => "SEMESTRAL",
        /// Every year.
        Anual => "ANUAL",
    }
}

api_enum! {
    /// Whether a recurring charge not paid may be tried again
    /// (`politicaRetentativa`).
    pub enum PoliticaRetentativa {
        /// No new attempts.
        NaoPermite => "NAO_PERMITE",
        /// Up to 3 new attempts, on different days, within 7 days of the
        /// expected payment, asked for by the receiver.
        Permite3R7D => "PERMITE_3R_7D",
    }
}

api_enum! {
    /// Where a recurrence stands (`status`).
    pub enum StatusRec {
        /// Created, waiting for the payer's approval.
        Criada => "CRIADA",
        /// Approved by the payer and active.
        Aprovada => "APROVADA",
        /// Rejected by the payer.
        Rejeitada => "REJEITADA",
        /// Expired without approval.
        Expirada => "EXPIRADA",
        /// Cancelled by the receiver or the payer.
        Cancelada => "CANCELADA",
    }
}

api_enum! {
    /// How the payer joined the recurrence (`tipoJornada`).
    pub enum TipoJornada {
        /// By a notification from their bank.
        Jornada1 => "JORNADA_1",
        /// By reading the QR Code of the recurrence.
        Jornada2 => "JORNADA_2",
        /// By paying an immediate charge with a composite QR Code.
        Jornada3 => "JORNADA_3",
        /// By a composite QR Code of a charge with a due date.
        Jornada4 => "JORNADA_4",
        /// Not defined yet.
        AguardandoDefinicao => "AGUARDANDO_DEFINICAO",
    }
}

string_serde!(Periodicidade, PoliticaRetentativa, StatusRec, TipoJornada);

/// A new recurrence (`RecSolicitada`), checked by
/// [`validar`](Self::validar) before being sent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct RecSolicitada {
    /// The payer and the contract.
    pub vinculo: VinculoRec,
    /// The first payment, the last one and the frequency.
    pub calendario: CalendarioRec,
    /// A fixed amount or the least the payer may set as their limit;
    /// without it, the amount of each charge is free.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valor: Option<ValorRec>,
    /// Whether charges not paid may be tried again.
    pub politica_retentativa: PoliticaRetentativa,
    /// A location created beforehand for the QR Code of the recurrence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loc: Option<u64>,
    /// The immediate charge whose composite QR Code also activates the
    /// recurrence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ativacao: Option<AtivacaoSolicitada>,
}

impl RecSolicitada {
    /// A recurrence of `vinculo`, from `calendario`, with a policy of
    /// retries.
    pub fn new(
        vinculo: VinculoRec,
        calendario: CalendarioRec,
        politica_retentativa: PoliticaRetentativa,
    ) -> Self {
        Self {
            vinculo,
            calendario,
            valor: None,
            politica_retentativa,
            loc: None,
            ativacao: None,
        }
    }

    /// Checks what the documentation defines: the texts and their sizes,
    /// the last payment not before the first, a known frequency and
    /// policy, and the amount.
    ///
    /// # Errors
    ///
    /// The first problem, naming the field as the API does.
    pub fn validar(&self) -> Result<(), CobrancaPixError> {
        self.vinculo.validar()?;
        self.calendario.validar()?;
        if let Some(valor) = &self.valor {
            valor.validar()?;
        }
        if let PoliticaRetentativa::Outro(outra) = &self.politica_retentativa {
            return Err(CobrancaPixError::new(
                "politicaRetentativa",
                format!("política desconhecida: \"{outra}\"; use NAO_PERMITE ou PERMITE_3R_7D"),
            ));
        }
        Ok(())
    }
}

/// The payer and the contract of a recurrence (`vinculo`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct VinculoRec {
    /// What the payments are for, up to [`MAX_OBJETO`] characters, so the
    /// payer recognizes them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub objeto: Option<String>,
    /// The payer, with a name of up to [`MAX_NOME_DEVEDOR`] characters.
    pub devedor: Devedor,
    /// The code of the contract (or order), up to [`MAX_CONTRATO`]
    /// characters.
    pub contrato: String,
}

impl VinculoRec {
    /// The contract `contrato` of `devedor`.
    pub fn new(devedor: Devedor, contrato: impl Into<String>) -> Self {
        Self {
            objeto: None,
            devedor,
            contrato: contrato.into(),
        }
    }

    fn validar(&self) -> Result<(), CobrancaPixError> {
        if let Some(objeto) = &self.objeto {
            texto(objeto, "vinculo.objeto", MAX_OBJETO)?;
        }
        texto(&self.devedor.nome, "vinculo.devedor.nome", MAX_NOME_DEVEDOR)?;
        texto(&self.contrato, "vinculo.contrato", MAX_CONTRATO)
    }
}

/// When the payments happen (`calendario`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct CalendarioRec {
    /// The expected date of the first payment.
    pub data_inicial: NaiveDate,
    /// The last one, for a recurrence with an end; without it, the
    /// recurrence has no end.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_final: Option<NaiveDate>,
    /// How often.
    pub periodicidade: Periodicidade,
}

impl CalendarioRec {
    /// From `data_inicial`, every `periodicidade`, with no end.
    pub fn new(data_inicial: NaiveDate, periodicidade: Periodicidade) -> Self {
        Self {
            data_inicial,
            data_final: None,
            periodicidade,
        }
    }

    fn validar(&self) -> Result<(), CobrancaPixError> {
        if self
            .data_final
            .is_some_and(|final_| final_ < self.data_inicial)
        {
            return Err(CobrancaPixError::new(
                "calendario.dataFinal",
                "é anterior à data do primeiro pagamento",
            ));
        }
        if let Periodicidade::Outro(outra) = &self.periodicidade {
            return Err(CobrancaPixError::new(
                "calendario.periodicidade",
                format!(
                    "periodicidade desconhecida: \"{outra}\"; use SEMANAL, MENSAL, TRIMESTRAL, SEMESTRAL ou ANUAL"
                ),
            ));
        }
        Ok(())
    }
}

/// The amount of a recurrence (`valor`): fixed, or the least the payer may
/// set as their limit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ValorRec {
    /// The amount of every payment, when it does not change.
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "decimal_texto::serialize_option"
    )]
    pub valor_rec: Option<Decimal>,
    /// The floor of the limit the payer may set, when the amount changes.
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "decimal_texto::serialize_option"
    )]
    pub valor_minimo_recebedor: Option<Decimal>,
}

impl ValorRec {
    /// The same amount in every payment.
    pub fn fixo(valor: Decimal) -> Self {
        Self {
            valor_rec: Some(valor),
            valor_minimo_recebedor: None,
        }
    }

    /// Amounts that change, with the least limit the payer may set.
    pub fn minimo(valor: Decimal) -> Self {
        Self {
            valor_rec: None,
            valor_minimo_recebedor: Some(valor),
        }
    }

    fn validar(&self) -> Result<(), CobrancaPixError> {
        match (self.valor_rec, self.valor_minimo_recebedor) {
            (Some(_), Some(_)) => Err(CobrancaPixError::new(
                "valor.valorMinimoRecebedor",
                "não vale com valorRec: uma recorrência de valor fixo não tem valor mínimo",
            )),
            (None, None) => Err(CobrancaPixError::new(
                "valor",
                "informe valorRec ou valorMinimoRecebedor, ou deixe o valor de fora",
            )),
            (Some(fixo), None) => valor(fixo, "valor.valorRec", false),
            (None, Some(minimo)) => valor(minimo, "valor.valorMinimoRecebedor", false),
        }
    }
}

/// The immediate charge whose composite QR Code also activates the
/// recurrence (`ativacao.dadosJornada.txid`).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AtivacaoSolicitada {
    /// txid of the charge.
    pub txid: Txid,
}

impl AtivacaoSolicitada {
    /// Activation by paying the charge `txid`.
    pub fn new(txid: Txid) -> Self {
        Self { txid }
    }
}

impl Serialize for AtivacaoSolicitada {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        json!({ "dadosJornada": { "txid": self.txid.as_str() } }).serialize(serializer)
    }
}

/// Changes to a recurrence (`RecRevisada`), or its cancellation. The name
/// of the payer and the location may change at any time; the date of the
/// first payment and the activation's txid, only while the payer has not
/// approved it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct RecRevisada {
    /// Cancels the recurrence (`status: CANCELADA`), with no other change.
    pub cancelar: bool,
    /// The payer's new name.
    pub nome_devedor: Option<String>,
    /// A new location.
    pub loc: Option<u64>,
    /// The new date of the first payment.
    pub data_inicial: Option<NaiveDate>,
    /// The txid of a new activation charge.
    pub txid: Option<Txid>,
}

impl RecRevisada {
    /// The cancellation of the recurrence.
    pub fn cancelamento() -> Self {
        Self {
            cancelar: true,
            ..Self::default()
        }
    }

    /// Checks that there is something to change, that a cancellation goes
    /// alone and the size of the name.
    ///
    /// # Errors
    ///
    /// The first problem, naming the field as the API does.
    pub fn validar(&self) -> Result<(), CobrancaPixError> {
        let muda = self.nome_devedor.is_some()
            || self.loc.is_some()
            || self.data_inicial.is_some()
            || self.txid.is_some();
        if self.cancelar && muda {
            return Err(CobrancaPixError::new(
                "status",
                "o cancelamento vai sozinho, sem outras alterações",
            ));
        }
        if !self.cancelar && !muda {
            return Err(CobrancaPixError::new("", "não há o que alterar"));
        }
        if let Some(nome) = &self.nome_devedor {
            texto(nome, "vinculo.devedor.nome", MAX_NOME_DEVEDOR)?;
        }
        Ok(())
    }
}

impl Serialize for RecRevisada {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut corpo = Map::new();
        if self.cancelar {
            corpo.insert("status".to_owned(), json!("CANCELADA"));
        }
        if let Some(nome) = &self.nome_devedor {
            corpo.insert("vinculo".to_owned(), json!({ "devedor": { "nome": nome } }));
        }
        if let Some(loc) = self.loc {
            corpo.insert("loc".to_owned(), json!(loc));
        }
        if let Some(data) = self.data_inicial {
            corpo.insert(
                "calendario".to_owned(),
                json!({ "dataInicial": data.to_string() }),
            );
        }
        if let Some(txid) = &self.txid {
            corpo.insert(
                "ativacao".to_owned(),
                json!({ "dadosJornada": { "txid": txid.as_str() } }),
            );
        }
        Value::Object(corpo).serialize(serializer)
    }
}

/// A recurrence, as received.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Rec {
    /// Its identifier.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub id_rec: Option<String>,
    /// The payer and the contract.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vinculo: Option<VinculoRecGerado>,
    /// The first payment, the last one and the frequency.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calendario: Option<CalendarioRecGerado>,
    /// The amount.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valor: Option<ValorRecGerado>,
    /// Who receives: the company.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recebedor: Option<RecebedorRec>,
    /// Who pays, once they approve it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pagador: Option<PagadorRec>,
    /// Where it stands.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<StatusRec>,
    /// Whether charges not paid may be tried again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub politica_retentativa: Option<PoliticaRetentativa>,
    /// The location of its QR Code.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "location"
    )]
    pub loc: Option<LocationRec>,
    /// The changes of status, with their times.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "lenient::vec"
    )]
    pub atualizacao: Vec<AtualizacaoRec>,
    /// Why it ended: rejected or cancelled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encerramento: Option<EncerramentoRec>,
    /// How the payer joined it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ativacao: Option<AtivacaoRec>,
    /// The confirmation requests sent to the payer.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "lenient::vec"
    )]
    pub solicitacao: Vec<SolicRec>,
    /// The QR Code of the recurrence, in a lookup.
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "dadosQR")]
    pub dados_qr: Option<DadosQrRec>,
}

/// The payer and the contract of a recurrence, as received.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct VinculoRecGerado {
    /// What the payments are for.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub objeto: Option<String>,
    /// The payer: name and CPF or CNPJ.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub devedor: Option<PessoaPix>,
    /// The code of the contract.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub contrato: Option<String>,
}

/// When the payments of a recurrence happen, as received.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct CalendarioRecGerado {
    /// The expected date of the first payment.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub data_inicial: Option<String>,
    /// The last one.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub data_final: Option<String>,
    /// How often.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub periodicidade: Option<Periodicidade>,
}

/// The amount of a recurrence, as received.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ValorRecGerado {
    /// The amount of every payment, when fixed.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::decimal",
        serialize_with = "decimal_texto::serialize_option"
    )]
    pub valor_rec: Option<Decimal>,
    /// The floor of the payer's limit, when the amount changes.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::decimal",
        serialize_with = "decimal_texto::serialize_option"
    )]
    pub valor_minimo_recebedor: Option<Decimal>,
}

/// The receiver of a recurrence, as received.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct RecebedorRec {
    /// CNPJ.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub cnpj: Option<String>,
    /// Name.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub nome: Option<String>,
    /// The agreement between the company and its bank.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub convenio: Option<String>,
    /// The ISPB of its bank.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub ispb_participante: Option<String>,
}

/// The payer of an approved recurrence, as received.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PagadorRec {
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
    /// The ISPB of their bank.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub ispb_participante: Option<String>,
    /// Their city, by the IBGE code.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub cod_mun: Option<String>,
}

/// The location of the QR Code of a recurrence, as received.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct LocationRec {
    /// Its identifier.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::u64"
    )]
    pub id: Option<u64>,
    /// Address of the payload, without the scheme.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub location: Option<String>,
    /// When it was created (RFC 3339).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub criacao: Option<String>,
    /// The recurrence linked to it.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub id_rec: Option<String>,
}

/// A location, or just its id, as some answers bring it.
fn location<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<LocationRec>, D::Error> {
    match Option::<Value>::deserialize(deserializer)? {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(id)) => Ok(Some(LocationRec {
            id: id.as_u64(),
            ..LocationRec::default()
        })),
        Some(outro) => serde_json::from_value(outro)
            .map(Some)
            .map_err(serde::de::Error::custom),
    }
}

/// A change of status of a recurrence, as received. The examples of the
/// documentation call the status `nome`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct AtualizacaoRec {
    /// The new status.
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "nome")]
    pub status: Option<StatusRec>,
    /// When (RFC 3339).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub data: Option<String>,
}

/// Why a recurrence ended, as received.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct EncerramentoRec {
    /// Rejected by the payer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rejeicao: Option<RejeicaoRec>,
    /// Cancelled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancelamento: Option<CancelamentoRec>,
}

/// Why the payer rejected a recurrence, as received.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct RejeicaoRec {
    /// The code of the reason (`AP13`...).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub codigo: Option<String>,
    /// The reason in words.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub descricao: Option<String>,
}

/// Who cancelled a recurrence and why, as received.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct CancelamentoRec {
    /// `PSP_PAGADOR`, `USUARIO_PAGADOR`, `PSP_RECEBEDOR` or
    /// `USUARIO_RECEBEDOR`.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub solicitante: Option<String>,
    /// The code of the reason (`SLCR`...).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub codigo: Option<String>,
    /// The reason in words.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub descricao: Option<String>,
}

/// How the payer joined a recurrence, as received. The examples of the
/// documentation also bring `tipoJornada` inside `dadosJornada`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct AtivacaoRec {
    /// The path the payer took.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tipo_jornada: Option<TipoJornada>,
    /// The charge of the activation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dados_jornada: Option<DadosJornada>,
}

impl AtivacaoRec {
    /// The path the payer took, wherever the API put it.
    pub fn jornada(&self) -> Option<&TipoJornada> {
        self.tipo_jornada.as_ref().or_else(|| {
            self.dados_jornada
                .as_ref()
                .and_then(|dados| dados.tipo_jornada.as_ref())
        })
    }
}

/// The charge of the activation of a recurrence, as received.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct DadosJornada {
    /// Its txid.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub txid: Option<String>,
    /// The path the payer took, as some answers bring it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tipo_jornada: Option<TipoJornada>,
}

/// The QR Code of a recurrence (`dadosQR`), in a lookup.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct DadosQrRec {
    /// The path the QR Code starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jornada: Option<TipoJornada>,
    /// The "copia e cola" of the QR Code.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub pix_copia_e_cola: Option<String>,
}

/// Which recurrences to list: those created in a period, with optional
/// filters.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct FiltroRecs {
    /// When they were created.
    pub periodo: PeriodoPix,
    /// Only those of this payer.
    pub devedor: Option<Documento>,
    /// Only those with (or without) a location.
    pub location_presente: Option<bool>,
    /// Only those in this status.
    pub status: Option<StatusRec>,
    /// Only those of this agreement, up to
    /// [`MAX_CONVENIO`](super::MAX_CONVENIO) characters.
    pub convenio: Option<String>,
}

impl FiltroRecs {
    /// Every recurrence created in `periodo`.
    pub fn new(periodo: PeriodoPix) -> Self {
        Self {
            periodo,
            devedor: None,
            location_presente: None,
            status: None,
            convenio: None,
        }
    }

    fn query(&self) -> Result<Vec<(&'static str, String)>> {
        let mut query = Vec::from(self.periodo.query());
        match &self.devedor {
            Some(Documento::Cpf(cpf)) => query.push(("cpf", cpf.clone())),
            Some(Documento::Cnpj(cnpj)) => query.push(("cnpj", cnpj.clone())),
            None => {}
        }
        if let Some(presente) = self.location_presente {
            query.push(("locationPresente", presente.to_string()));
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

/// A page of recurrences, as received.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PaginaRecs {
    /// The filters and the page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parametros: Option<ParametrosConsultaRec>,
    /// The recurrences.
    #[serde(default, deserialize_with = "lenient::vec")]
    pub recs: Vec<Rec>,
}

/// The filters and the page of a listing of recurrences, as received.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ParametrosConsultaRec {
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
    /// The location filter.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::bool"
    )]
    pub location_presente: Option<bool>,
    /// The status of the filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<StatusRec>,
    /// The agreement of the filter (`recebedor.convenio`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recebedor: Option<RecebedorRec>,
    /// The page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paginacao: Option<Paginacao>,
}

impl PixAutomatico<'_> {
    /// Creates a recurrence (`POST /pix/v2/rec`, scope `rec.write`). The
    /// payer approves it in their bank: by the QR Code of its location, by
    /// a confirmation request, or by paying the charge of `ativacao`.
    ///
    /// The recurrence is checked with [`RecSolicitada::validar`] before
    /// anything is sent. There is no idempotency key: the request is
    /// repeated automatically only when it surely was not processed, and
    /// after an unknown outcome the recurrences of the payer should be
    /// listed before trying again.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] with a [`CobrancaPixError`] when the
    /// recurrence is invalid (nothing is sent); otherwise, failures to
    /// obtain a token, to send the request or to decode the answer, and
    /// the API's error statuses.
    pub async fn criar_rec(&self, rec: &RecSolicitada) -> Result<Rec> {
        rec.validar()
            .map_err(|err| Error::InvalidInput(Box::new(err)))?;
        let request = ApiRequest::new(endpoint::pix_automatico::CRIAR_REC)
            .json(corpo(rec)?)
            .retry(RetryMode::WhenNotProcessed);
        self.client.execute(request).await
    }

    /// One page of the recurrences created in a period (`GET
    /// /pix/v2/rec`, scope `rec.read`).
    ///
    /// `pagina` starts at 0; without `itens_por_pagina`, the API returns 100
    /// per page, and it accepts up to [`ITENS_POR_PAGINA_MAXIMO_PIX`].
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] when `itens_por_pagina` is out of range or
    /// the agreement of the filter is too long (nothing is sent);
    /// otherwise the same as [`criar_rec`](Self::criar_rec).
    pub async fn listar_recs(
        &self,
        filtro: &FiltroRecs,
        pagina: u32,
        itens_por_pagina: Option<u32>,
    ) -> Result<PaginaRecs> {
        let request = paginada(
            ApiRequest::new(endpoint::pix_automatico::LISTAR_RECS).queries(filtro.query()?),
            pagina,
            itens_por_pagina,
        )?;
        self.client.execute(request).await
    }

    /// Every recurrence created in a period, reading as many pages of
    /// [`ITENS_POR_PAGINA_MAXIMO_PIX`] as needed.
    ///
    /// # Errors
    ///
    /// The same as [`listar_recs`](Self::listar_recs).
    pub async fn listar_todas_recs(&self, filtro: &FiltroRecs) -> Result<Vec<Rec>> {
        todas("recorrências", |pagina| async move {
            let pagina = self
                .listar_recs(filtro, pagina, Some(ITENS_POR_PAGINA_MAXIMO_PIX))
                .await?;
            let paginacao = pagina
                .parametros
                .and_then(|parametros| parametros.paginacao)
                .unwrap_or_default();
            Ok((pagina.recs, paginacao))
        })
        .await
    }

    /// A recurrence (`GET /pix/v2/rec/{idRec}`, scope `rec.read`). With
    /// `txid`, the txid of an immediate charge or of a charge with a due
    /// date, the answer brings the composite QR Code of that charge and
    /// the recurrence (`dadosQR`).
    ///
    /// # Errors
    ///
    /// The same as [`criar_rec`](Self::criar_rec); unknown recurrences
    /// fail with status `404`.
    pub async fn consultar_rec(&self, id: &IdRec, txid: Option<&Txid>) -> Result<Rec> {
        let mut request = ApiRequest::new(endpoint::pix_automatico::CONSULTAR_REC)
            .path_param("idRec", id.as_str().to_owned());
        if let Some(txid) = txid {
            request = request.query("txid", txid.as_str().to_owned());
        }
        self.client.execute(request).await
    }

    /// Changes or cancels a recurrence (`PATCH /pix/v2/rec/{idRec}`, scope
    /// `rec.write`).
    ///
    /// The revision is checked with [`RecRevisada::validar`] before
    /// anything is sent. The request is repeated automatically only when
    /// it surely was not processed.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] when the revision is invalid (nothing is
    /// sent); otherwise the same as [`criar_rec`](Self::criar_rec).
    pub async fn revisar_rec(&self, id: &IdRec, revisao: &RecRevisada) -> Result<Rec> {
        revisao
            .validar()
            .map_err(|err| Error::InvalidInput(Box::new(err)))?;
        let request = ApiRequest::new(endpoint::pix_automatico::REVISAR_REC)
            .path_param("idRec", id.as_str().to_owned())
            .json(corpo(revisao)?)
            .retry(RetryMode::WhenNotProcessed);
        self.client.execute(request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "RR1234567820240115abcdefghijk";

    fn devedor() -> Devedor {
        Devedor::new(
            Documento::parse("123.456.789-09").unwrap(),
            "Cliente Exemplo",
        )
    }

    fn rec() -> RecSolicitada {
        let mut vinculo = VinculoRec::new(devedor(), "contrato-001");
        vinculo.objeto = Some("Mensalidade".to_owned());
        let mut calendario = CalendarioRec::new(
            NaiveDate::from_ymd_opt(2026, 10, 10).unwrap(),
            Periodicidade::Mensal,
        );
        calendario.data_final = NaiveDate::from_ymd_opt(2027, 10, 10);
        let mut rec = RecSolicitada::new(vinculo, calendario, PoliticaRetentativa::Permite3R7D);
        rec.valor = Some(ValorRec::fixo("149.90".parse().unwrap()));
        rec
    }

    #[test]
    fn ids_have_29_letters_and_digits() {
        assert_eq!(IdRec::parse(&format!(" {ID} ")).unwrap().as_str(), ID);
        for invalido in [
            "",
            "RR123",
            &format!("{ID}x"),
            "RR1234567820240115abcdefghij-",
        ] {
            assert!(IdRec::parse(invalido).is_err(), "{invalido}");
        }
    }

    #[test]
    fn a_recurrence_is_sent_as_the_api_names_it() {
        let mut rec = rec();
        rec.loc = Some(108);
        rec.ativacao = Some(AtivacaoSolicitada::new(
            "33beb661beda44a8928fef47dbeb2dc5".parse().unwrap(),
        ));
        rec.validar().unwrap();
        assert_eq!(
            serde_json::to_value(&rec).unwrap(),
            json!({
                "vinculo": {"objeto": "Mensalidade", "devedor": {"cpf": "12345678909", "nome": "Cliente Exemplo"}, "contrato": "contrato-001"},
                "calendario": {"dataInicial": "2026-10-10", "dataFinal": "2027-10-10", "periodicidade": "MENSAL"},
                "valor": {"valorRec": "149.90"},
                "politicaRetentativa": "PERMITE_3R_7D",
                "loc": 108,
                "ativacao": {"dadosJornada": {"txid": "33beb661beda44a8928fef47dbeb2dc5"}}
            })
        );
    }

    #[test]
    fn what_the_api_would_refuse_is_named() {
        let campo = |rec: RecSolicitada| rec.validar().unwrap_err().campo().to_owned();
        let mut sem_contrato = rec();
        sem_contrato.vinculo.contrato = " ".to_owned();
        assert_eq!(campo(sem_contrato), "vinculo.contrato");
        let mut objeto_longo = rec();
        objeto_longo.vinculo.objeto = Some("x".repeat(36));
        assert_eq!(campo(objeto_longo), "vinculo.objeto");
        let mut nome_longo = rec();
        nome_longo.vinculo.devedor.nome = "x".repeat(141);
        assert_eq!(campo(nome_longo), "vinculo.devedor.nome");
        let mut ao_contrario = rec();
        ao_contrario.calendario.data_final = NaiveDate::from_ymd_opt(2026, 10, 9);
        assert_eq!(campo(ao_contrario), "calendario.dataFinal");
        let mut dois_valores = rec();
        dois_valores.valor = Some(ValorRec {
            valor_rec: Some(Decimal::ONE),
            valor_minimo_recebedor: Some(Decimal::ONE),
        });
        assert_eq!(campo(dois_valores), "valor.valorMinimoRecebedor");
        let mut sem_valor = rec();
        sem_valor.valor = Some(ValorRec::default());
        assert_eq!(campo(sem_valor), "valor");
        let mut zero = rec();
        zero.valor = Some(ValorRec::minimo(Decimal::ZERO));
        assert_eq!(campo(zero), "valor.valorMinimoRecebedor");
        let mut desconhecida = rec();
        desconhecida.calendario.periodicidade = Periodicidade::from("QUINZENAL");
        assert_eq!(campo(desconhecida), "calendario.periodicidade");
        // Without an amount, each charge has its own.
        let mut livre = rec();
        livre.valor = None;
        livre.validar().unwrap();
    }

    #[test]
    fn revisions_carry_only_what_changes() {
        assert_eq!(
            serde_json::to_value(RecRevisada::cancelamento()).unwrap(),
            json!({"status": "CANCELADA"})
        );
        let revisao = RecRevisada {
            nome_devedor: Some("Cliente Exemplo Ltda".to_owned()),
            loc: Some(108),
            data_inicial: NaiveDate::from_ymd_opt(2026, 11, 1),
            txid: Some("33beb661beda44a8928fef47dbeb2dc5".parse().unwrap()),
            ..RecRevisada::default()
        };
        revisao.validar().unwrap();
        assert_eq!(
            serde_json::to_value(&revisao).unwrap(),
            json!({
                "vinculo": {"devedor": {"nome": "Cliente Exemplo Ltda"}},
                "loc": 108,
                "calendario": {"dataInicial": "2026-11-01"},
                "ativacao": {"dadosJornada": {"txid": "33beb661beda44a8928fef47dbeb2dc5"}}
            })
        );
        assert!(RecRevisada::default().validar().is_err());
        let mut junto = RecRevisada::cancelamento();
        junto.loc = Some(1);
        assert_eq!(junto.validar().unwrap_err().campo(), "status");
    }

    #[test]
    fn answers_are_read_as_the_documentation_shows_them() {
        let rec: Rec = serde_json::from_value(json!({
            "idRec": ID,
            "vinculo": {"contrato": "contrato-001", "devedor": {"cpf": 12_345_678_909_u64, "nome": "Cliente Exemplo"}, "objeto": "Mensalidade"},
            "calendario": {"dataInicial": "2026-10-10", "periodicidade": "MENSAL"},
            "valor": {"valorMinimoRecebedor": "50.00"},
            "recebedor": {"cnpj": 12_345_678_000_195_u64, "nome": "Empresa Exemplo Ltda"},
            "status": "APROVADA",
            "politicaRetentativa": "NAO_PERMITE",
            "loc": 108,
            "atualizacao": [{"data": "2026-09-24T13:10:00Z", "nome": "CRIADA"}, {"data": "2026-09-25T09:00:00Z", "status": "APROVADA"}],
            "ativacao": {"dadosJornada": {"tipoJornada": "JORNADA_3", "txid": "33beb661beda44a8928fef47dbeb2dc5"}}
        }))
        .unwrap();
        assert_eq!(
            rec.vinculo.unwrap().devedor.unwrap().cpf.as_deref(),
            Some("12345678909")
        );
        assert_eq!(rec.loc.unwrap().id, Some(108));
        assert_eq!(rec.atualizacao[0].status, Some(StatusRec::Criada));
        assert_eq!(rec.atualizacao[1].status, Some(StatusRec::Aprovada));
        assert_eq!(
            rec.ativacao.unwrap().jornada(),
            Some(&TipoJornada::Jornada3)
        );
        assert_eq!(
            rec.valor.unwrap().valor_minimo_recebedor,
            Some("50.00".parse().unwrap())
        );
    }

    #[test]
    fn filters_go_in_the_query() {
        use chrono::DateTime;
        let mut filtro = FiltroRecs::new(
            PeriodoPix::new(
                DateTime::parse_from_rfc3339("2026-09-01T00:00:00-03:00").unwrap(),
                DateTime::parse_from_rfc3339("2026-09-30T23:59:59-03:00").unwrap(),
            )
            .unwrap(),
        );
        filtro.devedor = Some(Documento::parse("12.345.678/0001-95").unwrap());
        filtro.location_presente = Some(false);
        filtro.status = Some(StatusRec::Aprovada);
        filtro.convenio = Some("Master".to_owned());
        assert_eq!(
            filtro.query().unwrap(),
            [
                ("inicio", "2026-09-01T00:00:00-03:00".to_owned()),
                ("fim", "2026-09-30T23:59:59-03:00".to_owned()),
                ("cnpj", "12345678000195".to_owned()),
                ("locationPresente", "false".to_owned()),
                ("status", "APROVADA".to_owned()),
                ("convenio", "Master".to_owned()),
            ]
        );
        filtro.convenio = Some("x".repeat(61));
        assert!(filtro.query().is_err());
    }
}
