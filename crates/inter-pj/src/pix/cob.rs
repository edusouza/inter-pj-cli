//! Immediate Pix charges (`cob`): a dynamic QR Code, without a due date, to
//! be paid now — at a counter, in an online checkout.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;

use super::ChavePix;
use super::comum::{
    CobrancaPixError, Devedor, InfoAdicional, LocationPix, Paginacao, PeriodoPix, PessoaPix,
    validar_info_adicionais, valor,
};
use super::recebido::PixRecebido;
use crate::documento::Documento;
use crate::serde_util::{api_enum, decimal_texto, lenient, string_serde};

/// Longest [`CobSolicitada::solicitacao_pagador`].
pub const MAX_SOLICITACAO_PAGADOR: usize = 140;

/// An immediate charge to create (`CobSolicitada`), with
/// [`Pix::criar_cob`](super::Pix::criar_cob) (your txid) or
/// [`Pix::criar_cob_sem_txid`](super::Pix::criar_cob_sem_txid).
///
/// ```
/// use inter_pj::pix::{CobSolicitada, Devedor};
/// use rust_decimal::Decimal;
///
/// let mut cob = CobSolicitada::new("pix@empresa.example".parse().unwrap(), Decimal::new(14990, 2));
/// cob.calendario.expiracao = Some(3600);
/// cob.devedor = Some(Devedor::new("123.456.789-09".parse().unwrap(), "Fulano de Tal"));
/// cob.validar().unwrap();
/// assert_eq!(
///     serde_json::to_value(&cob).unwrap(),
///     serde_json::json!({
///         "calendario": {"expiracao": 3600},
///         "devedor": {"cpf": "12345678909", "nome": "Fulano de Tal"},
///         "valor": {"original": "149.90"},
///         "chave": "pix@empresa.example"
///     })
/// );
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct CobSolicitada {
    /// How long the charge lasts.
    pub calendario: CalendarioCob,
    /// Who pays.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub devedor: Option<Devedor>,
    /// Location already created for the payload (see `POST /pix/v2/loc`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loc: Option<LocCob>,
    /// Amount.
    pub valor: ValorCob,
    /// Pix key of the account that receives.
    pub chave: ChavePix,
    /// Text shown to the payer, up to [`MAX_SOLICITACAO_PAGADOR`] characters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub solicitacao_pagador: Option<String>,
    /// Information shown to the payer, up to
    /// [`MAX_INFO_ADICIONAIS`](super::MAX_INFO_ADICIONAIS).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub info_adicionais: Vec<InfoAdicional>,
}

impl CobSolicitada {
    /// A charge of `valor` to the key `chave`, lasting a day (the API's
    /// default).
    pub fn new(chave: ChavePix, valor: Decimal) -> Self {
        Self {
            calendario: CalendarioCob::default(),
            devedor: None,
            loc: None,
            valor: ValorCob::new(valor),
            chave,
            solicitacao_pagador: None,
            info_adicionais: Vec::new(),
        }
    }

    /// Checks what can be checked before sending, as the creation does.
    ///
    /// # Errors
    ///
    /// The first field the API would refuse.
    pub fn validar(&self) -> Result<(), CobrancaPixError> {
        self.calendario.validar()?;
        if let Some(devedor) = &self.devedor {
            devedor.validar("devedor")?;
        }
        self.valor.validar()?;
        validar_solicitacao(self.solicitacao_pagador.as_deref())?;
        validar_info_adicionais(&self.info_adicionais)
    }
}

/// How long an immediate charge lasts (`calendario`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct CalendarioCob {
    /// Seconds from the creation until the charge expires; without it, the
    /// API uses 86400 (a day).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiracao: Option<u32>,
}

impl CalendarioCob {
    fn validar(self) -> Result<(), CobrancaPixError> {
        if self.expiracao == Some(0) {
            return Err(CobrancaPixError::new(
                "calendario.expiracao",
                "a expiração é de pelo menos 1 segundo",
            ));
        }
        Ok(())
    }
}

/// A location created beforehand, to be used by the charge (`loc`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct LocCob {
    /// Identifier of the location.
    pub id: u64,
}

impl LocCob {
    /// The location `id`.
    pub fn new(id: u64) -> Self {
        Self { id }
    }
}

/// Amount of an immediate charge (`CobValor`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ValorCob {
    /// Amount to pay; zero only in a Pix Saque.
    #[serde(serialize_with = "decimal_texto::serialize")]
    pub original: Decimal,
    /// Whether the payer may change the amount (`modalidadeAlteracao` 1).
    #[serde(
        skip_serializing_if = "std::ops::Not::not",
        serialize_with = "modalidade"
    )]
    pub modalidade_alteracao: bool,
    /// Cash handed to the payer: Pix Saque or Pix Troco.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retirada: Option<Retirada>,
}

impl ValorCob {
    /// An amount the payer cannot change.
    pub fn new(original: Decimal) -> Self {
        Self {
            original,
            modalidade_alteracao: false,
            retirada: None,
        }
    }

    fn validar(&self) -> Result<(), CobrancaPixError> {
        let saque = matches!(self.retirada, Some(Retirada::Saque(_)));
        valor(self.original, "valor.original", saque)?;
        if saque && !self.original.is_zero() {
            return Err(CobrancaPixError::new(
                "valor.original",
                "no Pix Saque, o valor original é zero: o valor do saque vai em retirada.saque.valor",
            ));
        }
        if let Some(retirada) = &self.retirada {
            retirada.validar()?;
        }
        Ok(())
    }
}

/// `modalidadeAlteracao`: 1 when the amount may change (0, the default,
/// is not sent).
#[allow(clippy::trivially_copy_pass_by_ref)] // signature imposed by `serialize_with`
fn modalidade<S: Serializer>(alteravel: &bool, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_u8(u8::from(*alteravel))
}

/// Cash handed to the payer with the Pix (`retirada`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum Retirada {
    /// Pix Saque: the payer withdraws cash; the charge's original amount is
    /// zero.
    Saque(ValorRetirada),
    /// Pix Troco: the payer pays a purchase and takes change.
    Troco(ValorRetirada),
}

impl Retirada {
    fn validar(&self) -> Result<(), CobrancaPixError> {
        let (campo, retirada) = match self {
            Self::Saque(retirada) => ("valor.retirada.saque", retirada),
            Self::Troco(retirada) => ("valor.retirada.troco", retirada),
        };
        valor(retirada.valor, &format!("{campo}.valor"), false)?;
        let ispb = &retirada.prestador_do_servico_de_saque;
        if ispb.len() != 8 || !ispb.bytes().all(|b| b.is_ascii_digit()) {
            return Err(CobrancaPixError::new(
                format!("{campo}.prestadorDoServicoDeSaque"),
                "o ISPB do prestador do serviço de saque tem 8 dígitos",
            ));
        }
        if matches!(self, Self::Troco(_)) && retirada.modalidade_agente == ModalidadeAgente::Agpss {
            return Err(CobrancaPixError::new(
                format!("{campo}.modalidadeAgente"),
                "o Pix Troco aceita só AGTEC ou AGTOT",
            ));
        }
        Ok(())
    }
}

/// Cash of a Pix Saque or Pix Troco (`Saque`, `Troco`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ValorRetirada {
    /// Cash handed over.
    #[serde(serialize_with = "decimal_texto::serialize")]
    pub valor: Decimal,
    /// Whether the payer may change it (`modalidadeAlteracao` 1).
    #[serde(
        skip_serializing_if = "std::ops::Not::not",
        serialize_with = "modalidade"
    )]
    pub modalidade_alteracao: bool,
    /// Who hands the cash over.
    pub modalidade_agente: ModalidadeAgente,
    /// ISPB (8 digits) of the withdrawal service provider.
    pub prestador_do_servico_de_saque: String,
}

impl ValorRetirada {
    /// `valor` handed over by `agente`, with the provider's ISPB.
    pub fn new(
        valor: Decimal,
        agente: ModalidadeAgente,
        prestador_do_servico_de_saque: impl Into<String>,
    ) -> Self {
        Self {
            valor,
            modalidade_alteracao: false,
            modalidade_agente: agente,
            prestador_do_servico_de_saque: prestador_do_servico_de_saque.into(),
        }
    }
}

api_enum! {
    /// Who hands the cash over in a Pix Saque or Troco.
    pub enum ModalidadeAgente {
        /// `AGTEC`: a commercial establishment.
        Agtec => "AGTEC",
        /// `AGTOT`: another legal entity, or a banking correspondent.
        Agtot => "AGTOT",
        /// `AGPSS`: a withdrawal service facilitator (only Pix Saque).
        Agpss => "AGPSS",
    }
}

string_serde!(ModalidadeAgente);

/// Changes to an immediate charge (`CobRevisada`), sent with
/// [`Pix::revisar_cob`](super::Pix::revisar_cob). Only what is given
/// changes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct CobRevisada {
    /// New duration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub calendario: Option<CalendarioCob>,
    /// New payer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub devedor: Option<Devedor>,
    /// New location.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loc: Option<LocCob>,
    /// `true` removes the charge: its status becomes
    /// `REMOVIDA_PELO_USUARIO_RECEBEDOR`, and it can no longer be paid.
    #[serde(
        rename = "status",
        skip_serializing_if = "std::ops::Not::not",
        serialize_with = "removida"
    )]
    pub remover: bool,
    /// New amount.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valor: Option<ValorCobRevisada>,
    /// New key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chave: Option<ChavePix>,
    /// New text for the payer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub solicitacao_pagador: Option<String>,
    /// New information for the payer, replacing the old.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub info_adicionais: Option<Vec<InfoAdicional>>,
}

impl CobRevisada {
    /// Changes nothing yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Removes the charge.
    pub fn remocao() -> Self {
        Self {
            remover: true,
            ..Self::default()
        }
    }

    /// Checks what can be checked before sending.
    ///
    /// # Errors
    ///
    /// When nothing changes, or the first field the API would refuse.
    pub fn validar(&self) -> Result<(), CobrancaPixError> {
        if *self == Self::default() {
            return Err(CobrancaPixError::new("", "informe o que muda na cobrança"));
        }
        if let Some(calendario) = self.calendario {
            calendario.validar()?;
        }
        if let Some(devedor) = &self.devedor {
            devedor.validar("devedor")?;
        }
        if let Some(valor) = &self.valor {
            valor.validar()?;
        }
        validar_solicitacao(self.solicitacao_pagador.as_deref())?;
        validar_info_adicionais(self.info_adicionais.as_deref().unwrap_or_default())
    }
}

#[allow(clippy::trivially_copy_pass_by_ref)] // signature imposed by `serialize_with`
fn removida<S: Serializer>(_: &bool, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(StatusCob::RemovidaPeloUsuarioRecebedor.as_str())
}

/// New amount of an immediate charge (`CobValorRevisada`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ValorCobRevisada {
    /// New amount.
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "decimal_texto::serialize_option"
    )]
    pub original: Option<Decimal>,
    /// Whether the payer may change the amount.
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "modalidade_opcional"
    )]
    pub modalidade_alteracao: Option<bool>,
    /// New cash handed to the payer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retirada: Option<Retirada>,
}

impl ValorCobRevisada {
    fn validar(&self) -> Result<(), CobrancaPixError> {
        let saque = matches!(self.retirada, Some(Retirada::Saque(_)));
        if let Some(original) = self.original {
            valor(original, "valor.original", saque)?;
        }
        if let Some(retirada) = &self.retirada {
            retirada.validar()?;
        }
        Ok(())
    }
}

#[allow(clippy::ref_option, clippy::trivially_copy_pass_by_ref)] // signature imposed by `serialize_with`
fn modalidade_opcional<S: Serializer>(
    alteravel: &Option<bool>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match alteravel {
        Some(alteravel) => serializer.serialize_u8(u8::from(*alteravel)),
        None => serializer.serialize_none(),
    }
}

fn validar_solicitacao(solicitacao: Option<&str>) -> Result<(), CobrancaPixError> {
    match solicitacao {
        Some(texto) => super::comum::texto(texto, "solicitacaoPagador", MAX_SOLICITACAO_PAGADOR),
        None => Ok(()),
    }
}

api_enum! {
    /// Where the record of a charge stands (`status`). An expired charge
    /// stays `ATIVA`: the status is about the record, not about the payment
    /// deadline.
    pub enum StatusCob {
        /// `ATIVA`: can be paid.
        Ativa => "ATIVA",
        /// `CONCLUIDA`: paid.
        Concluida => "CONCLUIDA",
        /// `REMOVIDA_PELO_USUARIO_RECEBEDOR`: removed by the receiver.
        RemovidaPeloUsuarioRecebedor => "REMOVIDA_PELO_USUARIO_RECEBEDOR",
        /// `REMOVIDA_PELO_PSP`: removed by the bank.
        RemovidaPeloPsp => "REMOVIDA_PELO_PSP",
    }
}

string_serde!(StatusCob);

/// An immediate charge, as returned by the creation, the revision, the
/// query and the listing (`CobGerada`, `CobCompleta`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Cob {
    /// Creation and duration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calendario: Option<CalendarioCobGerado>,
    /// Identifier of the charge.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub txid: Option<String>,
    /// Revision: 0 when created, plus 1 at each change.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::u64"
    )]
    pub revisao: Option<u64>,
    /// Location of the payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loc: Option<LocationPix>,
    /// Address of the payload.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub location: Option<String>,
    /// Where the record stands.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<StatusCob>,
    /// Who pays.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub devedor: Option<PessoaPix>,
    /// Amount.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valor: Option<ValorCobGerado>,
    /// Pix key that receives.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub chave: Option<String>,
    /// Text shown to the payer.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub solicitacao_pagador: Option<String>,
    /// Information shown to the payer.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "lenient::vec"
    )]
    pub info_adicionais: Vec<InfoAdicional>,
    /// The "copia e cola" (BR Code) of the charge.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub pix_copia_e_cola: Option<String>,
    /// Pix that paid the charge.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "lenient::vec"
    )]
    pub pix: Vec<PixRecebido>,
}

/// Creation and duration of an immediate charge.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct CalendarioCobGerado {
    /// When the charge was created (RFC 3339).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub criacao: Option<String>,
    /// Seconds from the creation until the charge expires.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::u64"
    )]
    pub expiracao: Option<u64>,
}

/// Amount of an immediate charge, as received.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ValorCobGerado {
    /// Amount to pay.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::decimal",
        serialize_with = "decimal_texto::serialize_option"
    )]
    pub original: Option<Decimal>,
    /// 1 when the payer may change the amount.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::u64"
    )]
    pub modalidade_alteracao: Option<u64>,
    /// Pix Saque or Troco, as received.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retirada: Option<Value>,
}

/// Filters of [`Pix::listar_cobs`](super::Pix::listar_cobs): the period of
/// creation and, optionally, the payer, whether a location is linked and the
/// status.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct FiltroCobs {
    /// Period of creation.
    pub periodo: PeriodoPix,
    /// CPF or CNPJ of the payer.
    pub devedor: Option<Documento>,
    /// Only charges with (`true`) or without (`false`) a location.
    pub location_presente: Option<bool>,
    /// Only charges in this status.
    pub status: Option<StatusCob>,
}

impl FiltroCobs {
    /// Every charge created in `periodo`.
    pub fn new(periodo: PeriodoPix) -> Self {
        Self {
            periodo,
            devedor: None,
            location_presente: None,
            status: None,
        }
    }

    pub(crate) fn query(&self) -> Vec<(&'static str, String)> {
        let mut query: Vec<(&'static str, String)> = self.periodo.query().into();
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
        query
    }
}

/// A page of [`Pix::listar_cobs`](super::Pix::listar_cobs)
/// (`CobsConsultadas`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PaginaCobs {
    /// The filters and the page, as the API understood them.
    #[serde(default)]
    pub parametros: ParametrosConsulta,
    /// The charges of the page.
    #[serde(default, deserialize_with = "lenient::vec")]
    pub cobs: Vec<Cob>,
}

/// Filters and page of a listing, as the API echoes them (`parametros`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ParametrosConsulta {
    /// Start of the period.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub inicio: Option<String>,
    /// End of the period.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub fim: Option<String>,
    /// CPF filter.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub cpf: Option<String>,
    /// CNPJ filter.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub cnpj: Option<String>,
    /// Location filter.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::bool"
    )]
    pub location_presente: Option<bool>,
    /// Status filter.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub status: Option<String>,
    /// txid filter, in the listing of the Pix received.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub txid: Option<String>,
    /// The page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paginacao: Option<Paginacao>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn cob() -> CobSolicitada {
        CobSolicitada::new(
            "7d9f0335-8dcc-4054-9bf9-0dbd61d36906".parse().unwrap(),
            "37.00".parse().unwrap(),
        )
    }

    #[test]
    fn a_minimal_charge_sends_the_required_fields() {
        assert_eq!(
            serde_json::to_value(cob()).unwrap(),
            json!({
                "calendario": {},
                "valor": {"original": "37.00"},
                "chave": "7d9f0335-8dcc-4054-9bf9-0dbd61d36906"
            })
        );
    }

    #[test]
    fn charges_are_checked_field_by_field() {
        let campo = |cob: &CobSolicitada| cob.validar().unwrap_err().campo().to_owned();
        let mut invalida = cob();
        invalida.valor.original = Decimal::ZERO;
        assert_eq!(campo(&invalida), "valor.original");
        let mut invalida = cob();
        invalida.calendario.expiracao = Some(0);
        assert_eq!(campo(&invalida), "calendario.expiracao");
        let mut invalida = cob();
        invalida.solicitacao_pagador = Some("x".repeat(141));
        assert_eq!(campo(&invalida), "solicitacaoPagador");
        let mut invalida = cob();
        invalida.info_adicionais = vec![InfoAdicional::new("n", "v".repeat(201))];
        assert_eq!(campo(&invalida), "infoAdicionais[0].valor");
        let mut invalida = cob();
        invalida.devedor = Some(Devedor::new("123.456.789-09".parse().unwrap(), " "));
        assert_eq!(campo(&invalida), "devedor.nome");
    }

    #[test]
    fn withdrawals_and_change_follow_their_rules() {
        let saque = |original: &str, ispb: &str| {
            let mut cob = cob();
            cob.valor.original = original.parse().unwrap();
            cob.valor.retirada = Some(Retirada::Saque(ValorRetirada::new(
                "20.00".parse().unwrap(),
                ModalidadeAgente::Agpss,
                ispb,
            )));
            cob
        };
        assert!(saque("0.00", "12345678").validar().is_ok());
        assert_eq!(
            saque("10.00", "12345678").validar().unwrap_err().campo(),
            "valor.original"
        );
        assert_eq!(
            saque("0", "1234").validar().unwrap_err().campo(),
            "valor.retirada.saque.prestadorDoServicoDeSaque"
        );
        let mut troco = cob();
        troco.valor.retirada = Some(Retirada::Troco(ValorRetirada::new(
            "5.00".parse().unwrap(),
            ModalidadeAgente::Agpss,
            "12345678",
        )));
        assert_eq!(
            troco.validar().unwrap_err().campo(),
            "valor.retirada.troco.modalidadeAgente"
        );
        let Some(Retirada::Troco(retirada)) = &mut troco.valor.retirada else {
            unreachable!()
        };
        retirada.modalidade_agente = ModalidadeAgente::Agtec;
        retirada.modalidade_alteracao = true;
        troco.validar().unwrap();
        assert_eq!(
            serde_json::to_value(&troco).unwrap()["valor"],
            json!({
                "original": "37.00",
                "retirada": {"troco": {
                    "valor": "5.00",
                    "modalidadeAlteracao": 1,
                    "modalidadeAgente": "AGTEC",
                    "prestadorDoServicoDeSaque": "12345678"
                }}
            })
        );
    }

    #[test]
    fn revisions_send_only_what_changes() {
        assert_eq!(
            serde_json::to_value(CobRevisada::remocao()).unwrap(),
            json!({"status": "REMOVIDA_PELO_USUARIO_RECEBEDOR"})
        );
        let revisao = CobRevisada {
            valor: Some(ValorCobRevisada {
                original: Some("567.89".parse().unwrap()),
                modalidade_alteracao: Some(false),
                ..ValorCobRevisada::default()
            }),
            ..CobRevisada::new()
        };
        revisao.validar().unwrap();
        assert_eq!(
            serde_json::to_value(&revisao).unwrap(),
            json!({"valor": {"original": "567.89", "modalidadeAlteracao": 0}})
        );
        assert!(CobRevisada::new().validar().is_err());
    }

    #[test]
    fn filters_become_the_query() {
        use chrono::DateTime;
        let periodo = PeriodoPix::new(
            DateTime::parse_from_rfc3339("2026-09-01T00:00:00-03:00").unwrap(),
            DateTime::parse_from_rfc3339("2026-09-30T23:59:59-03:00").unwrap(),
        )
        .unwrap();
        let mut filtro = FiltroCobs::new(periodo);
        filtro.devedor = Some("12.345.678/0001-95".parse().unwrap());
        filtro.location_presente = Some(true);
        filtro.status = Some(StatusCob::Concluida);
        assert_eq!(
            filtro.query(),
            [
                ("inicio", "2026-09-01T00:00:00-03:00".to_owned()),
                ("fim", "2026-09-30T23:59:59-03:00".to_owned()),
                ("cnpj", "12345678000195".to_owned()),
                ("locationPresente", "true".to_owned()),
                ("status", "CONCLUIDA".to_owned()),
            ]
        );
    }
}
