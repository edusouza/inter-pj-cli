//! Pix charges with a due date (`cobv`): a QR Code to pay by a date, with
//! fine, interest, rebate and discounts, like a boleto.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize, Serializer};

use super::ChavePix;
use super::cob::{LocCob, MAX_SOLICITACAO_PAGADOR, ParametrosConsulta, StatusCob};
use super::comum::{
    CobrancaPixError, InfoAdicional, LocationPix, PeriodoPix, PessoaPix, texto,
    validar_info_adicionais, valor,
};
use super::recebido::PixRecebido;
use crate::cobranca::Uf;
use crate::documento::Documento;
use crate::serde_util::{decimal_texto, lenient};

/// Most fixed-date discounts of a charge.
pub const MAX_DESCONTOS_DATA_FIXA: usize = 3;

/// A charge with a due date to create (`CobVSolicitada`), with
/// [`Pix::criar_cobv`](super::Pix::criar_cobv).
///
/// ```
/// use chrono::NaiveDate;
/// use inter_pj::pix::{CobvSolicitada, DevedorCobv, MultaCobv};
/// use rust_decimal::Decimal;
///
/// let vencimento = NaiveDate::from_ymd_opt(2026, 10, 20).unwrap();
/// let devedor = DevedorCobv::new("123.456.789-09".parse().unwrap(), "Fulano de Tal");
/// let mut cobv = CobvSolicitada::new(
///     "pix@empresa.example".parse().unwrap(),
///     Decimal::new(15000, 2),
///     vencimento,
///     devedor,
/// );
/// cobv.calendario.validade_apos_vencimento = Some(30);
/// cobv.valor.multa = Some(MultaCobv::Percentual(Decimal::new(2, 0)));
/// cobv.validar().unwrap();
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct CobvSolicitada {
    /// Due date, and how long after it the charge can still be paid.
    pub calendario: CalendarioCobv,
    /// Who pays.
    pub devedor: DevedorCobv,
    /// Location already created for the payload (see `POST /pix/v2/loc`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loc: Option<LocCob>,
    /// Amount, fine, interest, rebate and discount.
    pub valor: ValorCobv,
    /// Pix key of the account that receives.
    pub chave: ChavePix,
    /// Text shown to the payer, up to [`MAX_SOLICITACAO_PAGADOR`]
    /// characters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub solicitacao_pagador: Option<String>,
    /// Information shown to the payer.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub info_adicionais: Vec<InfoAdicional>,
}

impl CobvSolicitada {
    /// A charge of `valor` to `devedor`, due on `vencimento`, to the key
    /// `chave`.
    pub fn new(
        chave: ChavePix,
        valor: Decimal,
        vencimento: NaiveDate,
        devedor: DevedorCobv,
    ) -> Self {
        Self {
            calendario: CalendarioCobv::new(vencimento),
            devedor,
            loc: None,
            valor: ValorCobv::new(valor),
            chave,
            solicitacao_pagador: None,
            info_adicionais: Vec::new(),
        }
    }

    /// Checks what can be checked before sending, as the creation does. The
    /// due date is not compared with today: the caller knows its clock.
    ///
    /// # Errors
    ///
    /// The first field the API would refuse.
    pub fn validar(&self) -> Result<(), CobrancaPixError> {
        self.devedor.validar()?;
        self.valor.validar(self.calendario.data_de_vencimento)?;
        if let Some(solicitacao) = &self.solicitacao_pagador {
            texto(solicitacao, "solicitacaoPagador", MAX_SOLICITACAO_PAGADOR)?;
        }
        validar_info_adicionais(&self.info_adicionais)
    }
}

/// Due date of a charge (`CobVDataVencimento`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct CalendarioCobv {
    /// The charge can be paid until this day, inclusive.
    pub data_de_vencimento: NaiveDate,
    /// Calendar days after the due date in which the charge can still be
    /// paid; the API uses 30 when not given.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validade_apos_vencimento: Option<u32>,
}

impl CalendarioCobv {
    /// Due on `vencimento`.
    pub fn new(vencimento: NaiveDate) -> Self {
        Self {
            data_de_vencimento: vencimento,
            validade_apos_vencimento: None,
        }
    }
}

/// Who pays a charge with a due date (`PessoaFisicaCobV`,
/// `PessoaJuridicaCobV`): name, CPF or CNPJ, and optionally e-mail and
/// address.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct DevedorCobv {
    /// CPF or CNPJ, sent as `cpf` or `cnpj`.
    pub documento: Documento,
    /// Name, up to 200 characters.
    pub nome: String,
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

impl DevedorCobv {
    /// A payer without e-mail and address.
    pub fn new(documento: Documento, nome: impl Into<String>) -> Self {
        Self {
            documento,
            nome: nome.into(),
            email: None,
            logradouro: None,
            cidade: None,
            uf: None,
            cep: None,
        }
    }

    pub(crate) fn validar(&self) -> Result<(), CobrancaPixError> {
        texto(&self.nome, "devedor.nome", 200)?;
        if let Some(email) = &self.email {
            texto(email, "devedor.email", 200)?;
            let (local, dominio) = email.split_once('@').unwrap_or_default();
            if local.is_empty() || !dominio.contains('.') || email.contains(char::is_whitespace) {
                return Err(CobrancaPixError::new("devedor.email", "e-mail inválido"));
            }
        }
        if let Some(logradouro) = &self.logradouro {
            texto(logradouro, "devedor.logradouro", 200)?;
        }
        if let Some(cidade) = &self.cidade {
            texto(cidade, "devedor.cidade", 200)?;
        }
        if let Some(cep) = &self.cep
            && (cep.len() != 8 || !cep.bytes().all(|b| b.is_ascii_digit()))
        {
            return Err(CobrancaPixError::new(
                "devedor.cep",
                "o CEP tem 8 dígitos, sem pontuação",
            ));
        }
        Ok(())
    }
}

impl Serialize for DevedorCobv {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
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
        match &self.documento {
            Documento::Cpf(cpf) => map.serialize_entry("cpf", cpf)?,
            Documento::Cnpj(cnpj) => map.serialize_entry("cnpj", cnpj)?,
        }
        map.serialize_entry("nome", &self.nome)?;
        if let Some(email) = &self.email {
            map.serialize_entry("email", email)?;
        }
        map.end()
    }
}

/// Amount of a charge with a due date and what changes it (`CobVValor`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ValorCobv {
    /// Amount due.
    #[serde(serialize_with = "decimal_texto::serialize")]
    pub original: Decimal,
    /// Fine for paying after the due date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multa: Option<MultaCobv>,
    /// Interest for paying after the due date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub juros: Option<JurosCobv>,
    /// Rebate, whenever it is paid.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub abatimento: Option<AbatimentoCobv>,
    /// Discount for paying early.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desconto: Option<DescontoCobv>,
}

impl ValorCobv {
    /// An amount without fine, interest, rebate or discount.
    pub fn new(original: Decimal) -> Self {
        Self {
            original,
            multa: None,
            juros: None,
            abatimento: None,
            desconto: None,
        }
    }

    fn validar(&self, vencimento: NaiveDate) -> Result<(), CobrancaPixError> {
        valor(self.original, "valor.original", false)?;
        if let Some(multa) = &self.multa {
            multa.validar()?;
        }
        if let Some(juros) = &self.juros {
            juros.validar()?;
        }
        if let Some(abatimento) = &self.abatimento {
            abatimento.validar(self.original)?;
        }
        if let Some(desconto) = &self.desconto {
            desconto.validar(self.original, vencimento)?;
        }
        Ok(())
    }
}

/// A fixed amount or a percentage: `valorPerc` from R$ 0,01, and a
/// percentage up to 100.
fn valor_perc(
    valor_perc: Decimal,
    percentual: bool,
    campo: &str,
    original: Option<Decimal>,
) -> Result<(), CobrancaPixError> {
    valor(valor_perc, campo, false)?;
    if percentual && valor_perc > Decimal::ONE_HUNDRED {
        return Err(CobrancaPixError::new(campo, "o percentual vai até 100"));
    }
    if !percentual && original.is_some_and(|original| valor_perc >= original) {
        return Err(CobrancaPixError::new(
            campo,
            "o valor tem de ser menor que o valor original da cobrança",
        ));
    }
    Ok(())
}

/// Serializes `{"modalidade": N, "valorPerc": "x.xx"}`.
fn modalidade_e_valor<S: Serializer>(
    serializer: S,
    modalidade: u8,
    valor_perc: Decimal,
) -> Result<S::Ok, S::Error> {
    let mut map = serializer.serialize_map(Some(2))?;
    map.serialize_entry("modalidade", &modalidade)?;
    map.serialize_entry("valorPerc", &format!("{valor_perc:.2}"))?;
    map.end()
}

/// Fine for paying after the due date (`valor.multa`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MultaCobv {
    /// Modality 1: a fixed amount.
    ValorFixo(Decimal),
    /// Modality 2: a percentage of the amount.
    Percentual(Decimal),
}

impl MultaCobv {
    fn validar(self) -> Result<(), CobrancaPixError> {
        match self {
            Self::ValorFixo(valor) => valor_perc(valor, false, "valor.multa.valorPerc", None),
            Self::Percentual(taxa) => valor_perc(taxa, true, "valor.multa.valorPerc", None),
        }
    }
}

impl Serialize for MultaCobv {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match *self {
            Self::ValorFixo(valor) => modalidade_e_valor(serializer, 1, valor),
            Self::Percentual(taxa) => modalidade_e_valor(serializer, 2, taxa),
        }
    }
}

/// Interest for paying after the due date (`valor.juros`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct JurosCobv {
    /// How the interest is counted.
    pub modalidade: ModalidadeJuros,
    /// Amount or percentage, as the modality says.
    pub valor_perc: Decimal,
}

impl JurosCobv {
    /// Interest of `valor_perc`, counted as `modalidade` says.
    pub fn new(modalidade: ModalidadeJuros, valor_perc: Decimal) -> Self {
        Self {
            modalidade,
            valor_perc,
        }
    }

    fn validar(self) -> Result<(), CobrancaPixError> {
        valor_perc(
            self.valor_perc,
            self.modalidade.percentual(),
            "valor.juros.valorPerc",
            None,
        )
    }
}

impl Serialize for JurosCobv {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        modalidade_e_valor(serializer, self.modalidade.codigo(), self.valor_perc)
    }
}

/// How interest is counted (`valor.juros.modalidade`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ModalidadeJuros {
    /// 1: an amount per calendar day.
    ValorDiasCorridos,
    /// 2: a percentage per day, calendar days.
    PercentualDiaDiasCorridos,
    /// 3: a percentage per month, calendar days.
    PercentualMesDiasCorridos,
    /// 4: a percentage per year, calendar days.
    PercentualAnoDiasCorridos,
    /// 5: an amount per business day.
    ValorDiasUteis,
    /// 6: a percentage per day, business days.
    PercentualDiaDiasUteis,
    /// 7: a percentage per month, business days.
    PercentualMesDiasUteis,
    /// 8: a percentage per year, business days.
    PercentualAnoDiasUteis,
}

impl ModalidadeJuros {
    /// Every modality, in the order of their codes.
    pub const TODAS: [Self; 8] = [
        Self::ValorDiasCorridos,
        Self::PercentualDiaDiasCorridos,
        Self::PercentualMesDiasCorridos,
        Self::PercentualAnoDiasCorridos,
        Self::ValorDiasUteis,
        Self::PercentualDiaDiasUteis,
        Self::PercentualMesDiasUteis,
        Self::PercentualAnoDiasUteis,
    ];

    /// Code of the API, 1 to 8.
    pub fn codigo(self) -> u8 {
        match self {
            Self::ValorDiasCorridos => 1,
            Self::PercentualDiaDiasCorridos => 2,
            Self::PercentualMesDiasCorridos => 3,
            Self::PercentualAnoDiasCorridos => 4,
            Self::ValorDiasUteis => 5,
            Self::PercentualDiaDiasUteis => 6,
            Self::PercentualMesDiasUteis => 7,
            Self::PercentualAnoDiasUteis => 8,
        }
    }

    /// The modality of a code of the API.
    pub fn de_codigo(codigo: u64) -> Option<Self> {
        Self::TODAS
            .into_iter()
            .find(|modalidade| u64::from(modalidade.codigo()) == codigo)
    }

    /// Whether the value is a percentage.
    pub fn percentual(self) -> bool {
        !matches!(self, Self::ValorDiasCorridos | Self::ValorDiasUteis)
    }
}

/// Rebate, whenever the charge is paid (`valor.abatimento`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AbatimentoCobv {
    /// Modality 1: a fixed amount.
    ValorFixo(Decimal),
    /// Modality 2: a percentage of the amount.
    Percentual(Decimal),
}

impl AbatimentoCobv {
    fn validar(self, original: Decimal) -> Result<(), CobrancaPixError> {
        match self {
            Self::ValorFixo(valor) => {
                valor_perc(valor, false, "valor.abatimento.valorPerc", Some(original))
            }
            Self::Percentual(taxa) => valor_perc(taxa, true, "valor.abatimento.valorPerc", None),
        }
    }
}

impl Serialize for AbatimentoCobv {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match *self {
            Self::ValorFixo(valor) => modalidade_e_valor(serializer, 1, valor),
            Self::Percentual(taxa) => modalidade_e_valor(serializer, 2, taxa),
        }
    }
}

/// Discount for paying early (`valor.desconto`).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DescontoCobv {
    /// Modality 1: fixed amounts until up to three dates.
    ValorFixoAteDatas(Vec<DescontoData>),
    /// Modality 2: percentages until up to three dates.
    PercentualAteDatas(Vec<DescontoData>),
    /// Modality 3: an amount per calendar day paid early.
    ValorPorDiaCorrido(Decimal),
    /// Modality 4: an amount per business day paid early.
    ValorPorDiaUtil(Decimal),
    /// Modality 5: a percentage per calendar day paid early.
    PercentualPorDiaCorrido(Decimal),
    /// Modality 6: a percentage per business day paid early.
    PercentualPorDiaUtil(Decimal),
}

impl DescontoCobv {
    /// Code of the API, 1 to 6.
    pub fn modalidade(&self) -> u8 {
        match self {
            Self::ValorFixoAteDatas(_) => 1,
            Self::PercentualAteDatas(_) => 2,
            Self::ValorPorDiaCorrido(_) => 3,
            Self::ValorPorDiaUtil(_) => 4,
            Self::PercentualPorDiaCorrido(_) => 5,
            Self::PercentualPorDiaUtil(_) => 6,
        }
    }

    fn validar(&self, original: Decimal, vencimento: NaiveDate) -> Result<(), CobrancaPixError> {
        let (datas, percentual) = match self {
            Self::ValorFixoAteDatas(datas) => (datas, false),
            Self::PercentualAteDatas(datas) => (datas, true),
            Self::ValorPorDiaCorrido(valor) | Self::ValorPorDiaUtil(valor) => {
                return valor_perc(*valor, false, "valor.desconto.valorPerc", Some(original));
            }
            Self::PercentualPorDiaCorrido(taxa) | Self::PercentualPorDiaUtil(taxa) => {
                return valor_perc(*taxa, true, "valor.desconto.valorPerc", None);
            }
        };
        if datas.is_empty() || datas.len() > MAX_DESCONTOS_DATA_FIXA {
            return Err(CobrancaPixError::new(
                "valor.desconto.descontoDataFixa",
                format!("de 1 a {MAX_DESCONTOS_DATA_FIXA} descontos"),
            ));
        }
        for (i, desconto) in datas.iter().enumerate() {
            let campo = format!("valor.desconto.descontoDataFixa[{i}]");
            valor_perc(
                desconto.valor_perc,
                percentual,
                &format!("{campo}.valorPerc"),
                Some(original),
            )?;
            if desconto.data > vencimento {
                return Err(CobrancaPixError::new(
                    format!("{campo}.data"),
                    "o desconto vale até uma data no vencimento ou antes dele",
                ));
            }
            if datas[..i].iter().any(|outro| outro.data == desconto.data) {
                return Err(CobrancaPixError::new(
                    format!("{campo}.data"),
                    "dois descontos na mesma data",
                ));
            }
        }
        Ok(())
    }
}

impl Serialize for DescontoCobv {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::ValorFixoAteDatas(datas) | Self::PercentualAteDatas(datas) => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("modalidade", &self.modalidade())?;
                map.serialize_entry("descontoDataFixa", datas)?;
                map.end()
            }
            Self::ValorPorDiaCorrido(valor)
            | Self::ValorPorDiaUtil(valor)
            | Self::PercentualPorDiaCorrido(valor)
            | Self::PercentualPorDiaUtil(valor) => {
                modalidade_e_valor(serializer, self.modalidade(), *valor)
            }
        }
    }
}

/// A discount until a date (`descontoDataFixa`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct DescontoData {
    /// Last day of the discount.
    pub data: NaiveDate,
    /// Amount or percentage.
    #[serde(serialize_with = "decimal_texto::serialize")]
    pub valor_perc: Decimal,
}

impl DescontoData {
    /// `valor_perc` off until `data`.
    pub fn new(data: NaiveDate, valor_perc: Decimal) -> Self {
        Self { data, valor_perc }
    }
}

/// Changes to a charge with a due date (`CobVRevisada`), sent with
/// [`Pix::revisar_cobv`](super::Pix::revisar_cobv). Only what is given
/// changes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct CobvRevisada {
    /// New due date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub calendario: Option<CalendarioCobv>,
    /// New payer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub devedor: Option<DevedorCobv>,
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
    /// New amount and charges.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valor: Option<ValorCobvRevisada>,
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

impl CobvRevisada {
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

    /// Checks what can be checked before sending. Discounts are compared
    /// with the new due date when it changes too.
    ///
    /// # Errors
    ///
    /// When nothing changes, or the first field the API would refuse.
    pub fn validar(&self) -> Result<(), CobrancaPixError> {
        if *self == Self::default() {
            return Err(CobrancaPixError::new("", "informe o que muda na cobrança"));
        }
        if let Some(devedor) = &self.devedor {
            devedor.validar()?;
        }
        if let Some(valor) = &self.valor {
            valor.validar(
                self.calendario
                    .map(|calendario| calendario.data_de_vencimento),
            )?;
        }
        if let Some(solicitacao) = &self.solicitacao_pagador {
            texto(solicitacao, "solicitacaoPagador", MAX_SOLICITACAO_PAGADOR)?;
        }
        validar_info_adicionais(self.info_adicionais.as_deref().unwrap_or_default())
    }
}

#[allow(clippy::trivially_copy_pass_by_ref)] // signature imposed by `serialize_with`
fn removida<S: Serializer>(_: &bool, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(StatusCob::RemovidaPeloUsuarioRecebedor.as_str())
}

/// New amount and charges of a charge with a due date.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ValorCobvRevisada {
    /// New amount.
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "decimal_texto::serialize_option"
    )]
    pub original: Option<Decimal>,
    /// New fine.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multa: Option<MultaCobv>,
    /// New interest.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub juros: Option<JurosCobv>,
    /// New rebate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub abatimento: Option<AbatimentoCobv>,
    /// New discount.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desconto: Option<DescontoCobv>,
}

impl ValorCobvRevisada {
    fn validar(&self, vencimento: Option<NaiveDate>) -> Result<(), CobrancaPixError> {
        if let Some(original) = self.original {
            valor(original, "valor.original", false)?;
        }
        if let Some(multa) = &self.multa {
            multa.validar()?;
        }
        if let Some(juros) = &self.juros {
            juros.validar()?;
        }
        // Without the amount and the due date, what depends on them is left
        // to the API.
        let original = self.original.unwrap_or(super::comum::VALOR_MAXIMO_PIX);
        if let Some(abatimento) = &self.abatimento {
            abatimento.validar(original)?;
        }
        if let Some(desconto) = &self.desconto {
            desconto.validar(original, vencimento.unwrap_or(NaiveDate::MAX))?;
        }
        Ok(())
    }
}

/// A charge with a due date, as returned by the creation, the revision, the
/// query and the listing (`CobVGerada`, `CobVCompleta`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Cobv {
    /// Creation, due date and validity after it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calendario: Option<CalendarioCobvGerado>,
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
    /// Where the record stands.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<StatusCob>,
    /// Who pays.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub devedor: Option<PessoaPix>,
    /// Who receives.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recebedor: Option<PessoaPix>,
    /// Amount and charges.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valor: Option<ValorCobvGerado>,
    /// The "copia e cola" (BR Code) of the charge.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub pix_copia_e_cola: Option<String>,
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
    /// Pix that paid the charge.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "lenient::vec"
    )]
    pub pix: Vec<PixRecebido>,
}

/// Creation, due date and validity of a charge, as received.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct CalendarioCobvGerado {
    /// When the charge was created (RFC 3339).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub criacao: Option<String>,
    /// Due date (`AAAA-MM-DD`).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub data_de_vencimento: Option<String>,
    /// Calendar days after the due date in which it can still be paid.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::u64"
    )]
    pub validade_apos_vencimento: Option<u64>,
}

/// Amount and charges of a charge with a due date, as received.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ValorCobvGerado {
    /// Amount due.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::decimal",
        serialize_with = "decimal_texto::serialize_option"
    )]
    pub original: Option<Decimal>,
    /// Fine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multa: Option<EncargoCobv>,
    /// Interest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub juros: Option<EncargoCobv>,
    /// Rebate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abatimento: Option<EncargoCobv>,
    /// Discount.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desconto: Option<EncargoCobv>,
}

/// A fine, interest, rebate or discount, as received: the modality and the
/// amount or percentage, or the fixed-date discounts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct EncargoCobv {
    /// Code of the modality, as the API documents for the field.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::u64"
    )]
    pub modalidade: Option<u64>,
    /// Amount or percentage.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::decimal",
        serialize_with = "decimal_texto::serialize_option"
    )]
    pub valor_perc: Option<Decimal>,
    /// Discounts until dates.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "lenient::vec"
    )]
    pub desconto_data_fixa: Vec<DescontoDataGerado>,
}

/// A discount until a date, as received.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct DescontoDataGerado {
    /// Last day of the discount.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub data: Option<String>,
    /// Amount or percentage.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::decimal",
        serialize_with = "decimal_texto::serialize_option"
    )]
    pub valor_perc: Option<Decimal>,
}

/// Filters of [`Pix::listar_cobvs`](super::Pix::listar_cobvs): those of
/// immediate charges, plus the batch.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct FiltroCobvs {
    /// Period of creation.
    pub periodo: PeriodoPix,
    /// CPF or CNPJ of the payer.
    pub devedor: Option<Documento>,
    /// Only charges with (`true`) or without (`false`) a location.
    pub location_presente: Option<bool>,
    /// Only charges in this status.
    pub status: Option<StatusCob>,
    /// Only charges of this batch.
    pub lote_cob_v_id: Option<u32>,
}

impl FiltroCobvs {
    /// Every charge created in `periodo`.
    pub fn new(periodo: PeriodoPix) -> Self {
        Self {
            periodo,
            devedor: None,
            location_presente: None,
            status: None,
            lote_cob_v_id: None,
        }
    }

    pub(crate) fn query(&self) -> Vec<(&'static str, String)> {
        let mut filtro = super::cob::FiltroCobs::new(self.periodo);
        filtro.devedor.clone_from(&self.devedor);
        filtro.location_presente = self.location_presente;
        filtro.status.clone_from(&self.status);
        let mut query = filtro.query();
        if let Some(lote) = self.lote_cob_v_id {
            query.push(("loteCobVId", lote.to_string()));
        }
        query
    }
}

/// A page of [`Pix::listar_cobvs`](super::Pix::listar_cobvs)
/// (`CobsVConsultadas`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PaginaCobvs {
    /// The filters and the page, as the API understood them.
    #[serde(default)]
    pub parametros: ParametrosConsulta,
    /// The charges of the page.
    #[serde(default, deserialize_with = "lenient::vec")]
    pub cobs: Vec<Cobv>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn dia(ano: i32, mes: u32, dia: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(ano, mes, dia).unwrap()
    }

    fn dec(texto: &str) -> Decimal {
        texto.parse().unwrap()
    }

    fn cobv() -> CobvSolicitada {
        CobvSolicitada::new(
            "5f84a4c5-c5cb-4599-9f13-7eb4d419dacc".parse().unwrap(),
            dec("123.45"),
            dia(2026, 12, 31),
            DevedorCobv::new("12345678909".parse().unwrap(), "Francisco da Silva"),
        )
    }

    #[test]
    fn a_minimal_charge_sends_the_required_fields() {
        assert_eq!(
            serde_json::to_value(cobv()).unwrap(),
            json!({
                "calendario": {"dataDeVencimento": "2026-12-31"},
                "devedor": {"cpf": "12345678909", "nome": "Francisco da Silva"},
                "valor": {"original": "123.45"},
                "chave": "5f84a4c5-c5cb-4599-9f13-7eb4d419dacc"
            })
        );
    }

    #[test]
    fn charges_send_modalities_as_documented() {
        let mut cobv = cobv();
        cobv.valor.multa = Some(MultaCobv::ValorFixo(dec("4")));
        cobv.valor.juros = Some(JurosCobv::new(
            ModalidadeJuros::PercentualMesDiasUteis,
            dec("1"),
        ));
        cobv.valor.abatimento = Some(AbatimentoCobv::Percentual(dec("5")));
        cobv.valor.desconto = Some(DescontoCobv::PercentualPorDiaUtil(dec("0.5")));
        cobv.validar().unwrap();
        assert_eq!(
            serde_json::to_value(&cobv).unwrap()["valor"],
            json!({
                "original": "123.45",
                "multa": {"modalidade": 1, "valorPerc": "4.00"},
                "juros": {"modalidade": 7, "valorPerc": "1.00"},
                "abatimento": {"modalidade": 2, "valorPerc": "5.00"},
                "desconto": {"modalidade": 6, "valorPerc": "0.50"}
            })
        );
        assert_eq!(
            ModalidadeJuros::de_codigo(3),
            Some(ModalidadeJuros::PercentualMesDiasCorridos)
        );
        assert_eq!(ModalidadeJuros::de_codigo(9), None);
    }

    #[test]
    fn discounts_until_dates_follow_the_rules() {
        let campo = |desconto: DescontoCobv| {
            let mut cobv = cobv();
            cobv.valor.desconto = Some(desconto);
            cobv.validar().map_err(|err| err.campo().to_owned())
        };
        let ate = |mes, valor: &str| DescontoData::new(dia(2026, mes, 30), dec(valor));
        assert_eq!(
            campo(DescontoCobv::ValorFixoAteDatas(vec![
                ate(11, "10"),
                ate(10, "20")
            ])),
            Ok(())
        );
        assert_eq!(
            campo(DescontoCobv::ValorFixoAteDatas(Vec::new())),
            Err("valor.desconto.descontoDataFixa".to_owned())
        );
        assert_eq!(
            campo(DescontoCobv::ValorFixoAteDatas(vec![
                ate(9, "1"),
                ate(10, "1"),
                ate(11, "1"),
                ate(8, "1")
            ])),
            Err("valor.desconto.descontoDataFixa".to_owned())
        );
        let depois = DescontoData::new(dia(2027, 1, 5), dec("10"));
        assert_eq!(
            campo(DescontoCobv::ValorFixoAteDatas(vec![depois])),
            Err("valor.desconto.descontoDataFixa[0].data".to_owned())
        );
        assert_eq!(
            campo(DescontoCobv::ValorFixoAteDatas(vec![ate(11, "200")])),
            Err("valor.desconto.descontoDataFixa[0].valorPerc".to_owned())
        );
        assert_eq!(
            campo(DescontoCobv::PercentualAteDatas(vec![ate(11, "101")])),
            Err("valor.desconto.descontoDataFixa[0].valorPerc".to_owned())
        );
        assert_eq!(
            campo(DescontoCobv::PercentualAteDatas(vec![
                ate(11, "5"),
                ate(11, "3")
            ])),
            Err("valor.desconto.descontoDataFixa[1].data".to_owned())
        );
    }

    #[test]
    fn payers_carry_an_address() {
        let mut devedor =
            DevedorCobv::new("12.345.678/0001-95".parse().unwrap(), "Empresa Exemplo");
        devedor.email = Some("financeiro@exemplo.com.br".to_owned());
        devedor.logradouro = Some("Avenida Brasil, 1200".to_owned());
        devedor.cidade = Some("Belo Horizonte".to_owned());
        devedor.uf = Some(Uf::Mg);
        devedor.cep = Some("30110000".to_owned());
        devedor.validar().unwrap();
        assert_eq!(
            serde_json::to_value(&devedor).unwrap(),
            json!({
                "logradouro": "Avenida Brasil, 1200",
                "cidade": "Belo Horizonte",
                "uf": "MG",
                "cep": "30110000",
                "cnpj": "12345678000195",
                "nome": "Empresa Exemplo",
                "email": "financeiro@exemplo.com.br"
            })
        );
        devedor.cep = Some("30110-000".to_owned());
        assert_eq!(devedor.validar().unwrap_err().campo(), "devedor.cep");
        devedor.cep = None;
        devedor.email = Some("financeiro".to_owned());
        assert_eq!(devedor.validar().unwrap_err().campo(), "devedor.email");
    }

    #[test]
    fn revisions_send_only_what_changes() {
        assert_eq!(
            serde_json::to_value(CobvRevisada::remocao()).unwrap(),
            json!({"status": "REMOVIDA_PELO_USUARIO_RECEBEDOR"})
        );
        let revisao = CobvRevisada {
            calendario: Some(CalendarioCobv::new(dia(2027, 1, 15))),
            valor: Some(ValorCobvRevisada {
                desconto: Some(DescontoCobv::ValorFixoAteDatas(vec![DescontoData::new(
                    dia(2027, 1, 20),
                    dec("10"),
                )])),
                ..ValorCobvRevisada::default()
            }),
            ..CobvRevisada::new()
        };
        // The discount would end after the new due date.
        assert_eq!(
            revisao.validar().unwrap_err().campo(),
            "valor.desconto.descontoDataFixa[0].data"
        );
        assert!(CobvRevisada::new().validar().is_err());
    }

    #[test]
    fn answers_keep_the_charges() {
        let cobv: Cobv = serde_json::from_value(json!({
            "calendario": {"criacao": "2026-09-23T10:00:00Z", "dataDeVencimento": "2026-12-31", "validadeAposVencimento": 30},
            "txid": "7978c0c97ea847e78e8849634473c1f1",
            "status": "ATIVA",
            "recebedor": {"cnpj": "12345678000195", "nome": "Empresa Exemplo", "nomeFantasia": "Exemplo"},
            "valor": {
                "original": "123.45",
                "multa": {"modalidade": "2", "valorPerc": "15.00"},
                "desconto": {"modalidade": 1, "descontoDataFixa": [{"data": "2026-11-30", "valorPerc": "30.00"}]}
            }
        }))
        .unwrap();
        let valor = cobv.valor.as_ref().unwrap();
        assert_eq!(valor.multa.as_ref().unwrap().modalidade, Some(2));
        assert_eq!(valor.desconto.as_ref().unwrap().desconto_data_fixa.len(), 1);
        assert_eq!(
            cobv.recebedor.as_ref().unwrap().nome_fantasia.as_deref(),
            Some("Exemplo")
        );
        let de_volta = serde_json::to_value(&cobv).unwrap();
        assert_eq!(
            de_volta["valor"]["multa"],
            json!({"modalidade": 2, "valorPerc": "15.00"})
        );
    }
}
