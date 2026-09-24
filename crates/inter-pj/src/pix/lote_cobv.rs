//! Batches of charges with a due date (`lotecobv`): many charges created or
//! changed at once, processed afterwards, each accepted or refused.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::cob::ParametrosConsulta;
use super::cobv::{CobvRevisada, CobvSolicitada};
use super::comum::CobrancaPixError;
use super::txid::Txid;
use crate::problem::Problem;
use crate::serde_util::{api_enum, lenient, string_serde};

/// A batch of charges with a due date to create (`LoteCobVSolicitado`),
/// with [`Pix::criar_lote_cobv`](super::Pix::criar_lote_cobv).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct LoteCobvSolicitado {
    /// Description of the batch.
    pub descricao: String,
    /// The charges, each with its txid.
    pub cobsv: Vec<CobvDoLote>,
}

impl LoteCobvSolicitado {
    /// A batch described as `descricao`.
    pub fn new(descricao: impl Into<String>, cobsv: Vec<CobvDoLote>) -> Self {
        Self {
            descricao: descricao.into(),
            cobsv,
        }
    }

    /// Checks every charge before sending, as its creation would, and that
    /// no txid repeats.
    ///
    /// # Errors
    ///
    /// The first field the API would refuse, in the item it belongs to
    /// (`cobsv[2].valor.original`).
    pub fn validar(&self) -> Result<(), CobrancaPixError> {
        validar_descricao(&self.descricao)?;
        validar_itens(
            self.cobsv
                .iter()
                .map(|item| (&item.txid, item.cobv.validar())),
        )
    }
}

/// A charge of a batch: its txid and the charge (`CobVSolicitadaLote`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct CobvDoLote {
    /// Identifier of the charge.
    pub txid: Txid,
    /// The charge.
    #[serde(flatten)]
    pub cobv: CobvSolicitada,
}

impl CobvDoLote {
    /// The charge `cobv`, with the txid `txid`.
    pub fn new(txid: Txid, cobv: CobvSolicitada) -> Self {
        Self { txid, cobv }
    }
}

/// Changes to charges of a batch (`LoteCobVRevisada`), sent with
/// [`Pix::revisar_lote_cobv`](super::Pix::revisar_lote_cobv).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct LoteCobvRevisado {
    /// New description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub descricao: Option<String>,
    /// The charges that change, each with its txid.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub cobsv: Vec<CobvRevisadaDoLote>,
}

impl LoteCobvRevisado {
    /// Changes of `cobsv`.
    pub fn new(cobsv: Vec<CobvRevisadaDoLote>) -> Self {
        Self {
            descricao: None,
            cobsv,
        }
    }

    /// Checks every change before sending, and that no txid repeats.
    ///
    /// # Errors
    ///
    /// When nothing changes, or the first field the API would refuse.
    pub fn validar(&self) -> Result<(), CobrancaPixError> {
        if self.descricao.is_none() && self.cobsv.is_empty() {
            return Err(CobrancaPixError::new("", "informe o que muda no lote"));
        }
        if let Some(descricao) = &self.descricao {
            validar_descricao(descricao)?;
        }
        validar_itens(
            self.cobsv
                .iter()
                .map(|item| (&item.txid, item.revisao.validar())),
        )
    }
}

/// A change to a charge of a batch: its txid and the change
/// (`CobVRevisadaItem`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct CobvRevisadaDoLote {
    /// Identifier of the charge.
    pub txid: Txid,
    /// What changes.
    #[serde(flatten)]
    pub revisao: CobvRevisada,
}

impl CobvRevisadaDoLote {
    /// The change `revisao` to the charge `txid`.
    pub fn new(txid: Txid, revisao: CobvRevisada) -> Self {
        Self { txid, revisao }
    }
}

fn validar_descricao(descricao: &str) -> Result<(), CobrancaPixError> {
    if descricao.trim().is_empty() || descricao.chars().any(char::is_control) {
        return Err(CobrancaPixError::new(
            "descricao",
            "a descrição do lote não pode ficar em branco nem ter quebras de linha",
        ));
    }
    Ok(())
}

fn validar_itens<'a>(
    itens: impl Iterator<Item = (&'a Txid, Result<(), CobrancaPixError>)>,
) -> Result<(), CobrancaPixError> {
    let mut txids = HashSet::new();
    let mut vazio = true;
    for (i, (txid, validacao)) in itens.enumerate() {
        vazio = false;
        let item = format!("cobsv[{i}]");
        validacao.map_err(|err| err.no_item(&item))?;
        if !txids.insert(txid.as_str()) {
            return Err(CobrancaPixError::new(
                format!("{item}.txid"),
                format!("o txid {txid} aparece mais de uma vez no lote"),
            ));
        }
    }
    if vazio {
        return Err(CobrancaPixError::new("cobsv", "o lote não tem cobranças"));
    }
    Ok(())
}

api_enum! {
    /// Where the request of a charge of a batch stands.
    pub enum StatusCobvLote {
        /// `EM_PROCESSAMENTO`: being processed.
        EmProcessamento => "EM_PROCESSAMENTO",
        /// `CRIADA`: created or changed.
        Criada => "CRIADA",
        /// `NEGADA`: refused; the problem says why.
        Negada => "NEGADA",
    }
}

string_serde!(StatusCobvLote);

/// A batch of charges with a due date, as queried (`LoteCobVConsultado`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct LoteCobv {
    /// Identifier of the batch.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::u64"
    )]
    pub id: Option<u64>,
    /// Description of the batch.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub descricao: Option<String>,
    /// Status of the batch, sent by the API in the query by situation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<StatusCobvLote>,
    /// When the batch was created (RFC 3339).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub criacao: Option<String>,
    /// The charges of the batch and where each stands.
    #[serde(default, deserialize_with = "lenient::vec")]
    pub cobsv: Vec<CobvNoLote>,
}

/// A charge of a batch and where its request stands.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct CobvNoLote {
    /// When the charge was created (RFC 3339).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub criacao: Option<String>,
    /// Identifier of the charge.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub txid: Option<String>,
    /// Where the request stands.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<StatusCobvLote>,
    /// Why the charge was refused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problema: Option<Problem>,
}

/// Summary of the processing of a batch (`SummaryLoteCobV`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct SumarioLoteCobv {
    /// When the processing started (RFC 3339).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub data_criacao_processamento: Option<String>,
    /// Where the processing stands.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub status_processamento: Option<String>,
    /// Charges in the batch.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::u64"
    )]
    pub total_cobrancas: Option<u64>,
    /// Charges refused.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::u64"
    )]
    pub total_cobrancas_negadas: Option<u64>,
    /// Charges created.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::u64"
    )]
    pub total_cobrancas_criadas: Option<u64>,
}

/// A page of [`Pix::listar_lotes_cobv`](super::Pix::listar_lotes_cobv)
/// (`LotesCobVConsultados`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PaginaLotesCobv {
    /// The filters and the page, as the API understood them.
    #[serde(default)]
    pub parametros: ParametrosConsulta,
    /// The batches of the page.
    #[serde(default, deserialize_with = "lenient::vec")]
    pub lotes: Vec<LoteCobv>,
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use serde_json::json;

    use super::*;
    use crate::pix::DevedorCobv;

    fn cobv(valor: &str) -> CobvSolicitada {
        CobvSolicitada::new(
            "7c084cd4-54af-4172-a516-a7d1a12b75cc".parse().unwrap(),
            valor.parse().unwrap(),
            NaiveDate::from_ymd_opt(2026, 12, 31).unwrap(),
            DevedorCobv::new("12345678909".parse().unwrap(), "João Souza"),
        )
    }

    fn txid(texto: &str) -> Txid {
        texto.parse().unwrap()
    }

    #[test]
    fn batches_send_the_txid_with_each_charge() {
        let lote = LoteCobvSolicitado::new(
            "Mensalidades",
            vec![CobvDoLote::new(
                txid("fb2761260e554ad593c7226beb5cb650"),
                cobv("100.00"),
            )],
        );
        lote.validar().unwrap();
        let corpo = serde_json::to_value(&lote).unwrap();
        assert_eq!(corpo["descricao"], "Mensalidades");
        assert_eq!(
            corpo["cobsv"][0]["txid"],
            "fb2761260e554ad593c7226beb5cb650"
        );
        assert_eq!(corpo["cobsv"][0]["valor"], json!({"original": "100.00"}));
    }

    #[test]
    fn every_charge_is_checked_and_txids_do_not_repeat() {
        let um = txid("fb2761260e554ad593c7226beb5cb650");
        let dois = txid("7978c0c97ea847e78e8849634473c1f1");
        let lote = LoteCobvSolicitado::new(
            "Mensalidades",
            vec![
                CobvDoLote::new(um.clone(), cobv("100.00")),
                CobvDoLote::new(dois, cobv("0")),
            ],
        );
        assert_eq!(
            lote.validar().unwrap_err().campo(),
            "cobsv[1].valor.original"
        );
        let repetido = LoteCobvSolicitado::new(
            "Mensalidades",
            vec![
                CobvDoLote::new(um.clone(), cobv("100.00")),
                CobvDoLote::new(um, cobv("100.00")),
            ],
        );
        assert_eq!(repetido.validar().unwrap_err().campo(), "cobsv[1].txid");
        assert_eq!(
            LoteCobvSolicitado::new("Vazio", Vec::new())
                .validar()
                .unwrap_err()
                .campo(),
            "cobsv"
        );
        assert_eq!(
            LoteCobvSolicitado::new(" ", Vec::new())
                .validar()
                .unwrap_err()
                .campo(),
            "descricao"
        );
        assert!(LoteCobvRevisado::default().validar().is_err());
    }

    #[test]
    fn refused_charges_keep_the_problem() {
        let lote: LoteCobv = serde_json::from_value(json!({
            "id": 13,
            "criacao": "2026-09-23T15:12:55.745Z",
            "cobsv": [{
                "txid": "7978c0c97ea847e78e8849634473c1f1",
                "status": "NEGADA",
                "problema": {"type": "https://pix.bcb.gov.br/api/v2/error/CobVOperacaoInvalida", "title": "Cobrança inválida.", "status": 400, "violacoes": [{"razao": "O objeto cobv.devedor não respeita o schema.", "propriedade": "cobv.devedor"}]}
            }]
        }))
        .unwrap();
        let negada = &lote.cobsv[0];
        assert_eq!(negada.status, Some(StatusCobvLote::Negada));
        assert_eq!(
            negada.problema.as_ref().unwrap().violacoes[0]
                .propriedade
                .as_deref(),
            Some("cobv.devedor")
        );
    }
}
