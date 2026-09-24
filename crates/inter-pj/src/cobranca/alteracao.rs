use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::{VALOR_MAXIMO, VALOR_MINIMO};
use crate::serde_util::{api_enum, decimal_as_number, lenient, string_serde};

/// Longest reason for cancelling a charge (`motivoCancelamento`).
pub const MAX_MOTIVO_CANCELAMENTO: usize = 50;

/// Changes to a charge, sent with
/// [`Cobranca::editar`](super::Cobranca::editar) (`UpdateCobrancaRequestBody`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct EdicaoCobranca {
    /// New due date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_vencimento: Option<NaiveDate>,
    /// New face value, from [`VALOR_MINIMO`] to [`VALOR_MAXIMO`].
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "decimal_as_number::serialize_option"
    )]
    pub valor_nominal: Option<Decimal>,
}

impl EdicaoCobranca {
    /// Changes of the due date, the face value, or both.
    pub fn new(data_vencimento: Option<NaiveDate>, valor_nominal: Option<Decimal>) -> Self {
        Self {
            data_vencimento,
            valor_nominal,
        }
    }

    /// Checks what can be checked before sending, as
    /// [`Cobranca::editar`](super::Cobranca::editar) does.
    ///
    /// # Errors
    ///
    /// When nothing changes or the face value is out of the accepted range.
    pub fn validar(&self) -> Result<(), &'static str> {
        if self.data_vencimento.is_none() && self.valor_nominal.is_none() {
            return Err("informe o novo vencimento, o novo valor, ou ambos");
        }
        if let Some(valor) = self.valor_nominal
            && (valor < VALOR_MINIMO || valor > VALOR_MAXIMO || valor.normalize().scale() > 2)
        {
            return Err(
                "o valor da cobrança deve ser de R$ 2,50 a R$ 99.999.999,99, com até 2 casas decimais",
            );
        }
        Ok(())
    }
}

/// Answer to [`Cobranca::editar`](super::Cobranca::editar)
/// (`UpdateCobrancaResponseBody`). The change may still be in progress:
/// follow it with [`Cobranca::consultar_edicao`](super::Cobranca::consultar_edicao).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct SolicitacaoEdicao {
    /// Processing, done or failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<StatusEdicao>,
    /// Message of the API.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub mensagem: Option<String>,
    /// Identifier of the change.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub codigo_edicao: Option<String>,
}

/// Answer to [`Cobranca::consultar_edicao`](super::Cobranca::consultar_edicao)
/// (`GetStatusUpdateResponseBody`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ConsultaEdicao {
    /// Processing, done or failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<StatusEdicao>,
}

api_enum! {
    /// Where a change of a charge stands.
    pub enum StatusEdicao {
        /// `PROCESSANDO`: in progress.
        Processando => "PROCESSANDO",
        /// `SUCESSO`: done; the new value may take up to 30 minutes to show.
        Sucesso => "SUCESSO",
        /// `FALHA`: not done.
        Falha => "FALHA",
    }
}

string_serde!(StatusEdicao);

impl StatusEdicao {
    /// Whether the change ended, well or not.
    pub fn is_final(&self) -> bool {
        matches!(self, Self::Sucesso | Self::Falha)
    }
}

/// How the sandbox pays a charge, in
/// [`Cobranca::pagar_no_sandbox`](super::Cobranca::pagar_no_sandbox)
/// (`PagamentoCobrancaRequestBody`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PagarCom {
    /// `BOLETO`: the barcode.
    Boleto,
    /// `PIX`: the QR Code.
    Pix,
}

impl PagarCom {
    /// Code used by the API.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Boleto => "BOLETO",
            Self::Pix => "PIX",
        }
    }
}

/// The reason for [`Cobranca::cancelar`](super::Cobranca::cancelar),
/// trimmed, as the API accepts it: 1 to [`MAX_MOTIVO_CANCELAMENTO`]
/// characters, on one line.
///
/// # Errors
///
/// When the reason is empty, too long, or has line breaks or other control
/// characters.
pub fn motivo_cancelamento(motivo: &str) -> Result<String, &'static str> {
    let motivo = motivo.trim();
    if motivo.is_empty()
        || motivo.chars().count() > MAX_MOTIVO_CANCELAMENTO
        || motivo.chars().any(char::is_control)
    {
        return Err("o motivo do cancelamento deve ter de 1 a 50 caracteres, sem quebras de linha");
    }
    Ok(motivo.to_owned())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn changes_serialize_only_what_changes() {
        let data = NaiveDate::from_ymd_opt(2026, 11, 10);
        let valor = Some("200.5".parse().unwrap());
        assert_eq!(
            serde_json::to_value(EdicaoCobranca::new(data, None)).unwrap(),
            json!({"dataVencimento": "2026-11-10"})
        );
        assert_eq!(
            serde_json::to_value(EdicaoCobranca::new(data, valor)).unwrap(),
            json!({"dataVencimento": "2026-11-10", "valorNominal": 200.5})
        );
        assert_eq!(EdicaoCobranca::new(data, valor).validar(), Ok(()));
        assert!(EdicaoCobranca::default().validar().is_err());
        for invalido in ["2.49", "100000000", "10.001"] {
            let edicao = EdicaoCobranca::new(None, Some(invalido.parse().unwrap()));
            assert!(edicao.validar().is_err(), "{invalido}");
        }
    }

    #[test]
    fn reasons_have_up_to_50_characters() {
        assert_eq!(
            motivo_cancelamento(" Pedido cancelado ").unwrap(),
            "Pedido cancelado"
        );
        assert!(motivo_cancelamento(&"a".repeat(50)).is_ok());
        for invalido in [
            String::new(),
            " ".to_owned(),
            "a".repeat(51),
            "a\nb".to_owned(),
        ] {
            assert!(motivo_cancelamento(&invalido).is_err(), "{invalido:?}");
        }
    }

    #[test]
    fn statuses_of_a_change() {
        let status: StatusEdicao = serde_json::from_value(json!("SUCESSO")).unwrap();
        assert!(status.is_final());
        assert!(!StatusEdicao::Processando.is_final());
        assert_eq!(
            StatusEdicao::from("OUTRO"),
            StatusEdicao::Outro("OUTRO".to_owned())
        );
    }
}
