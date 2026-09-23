use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::serde_util::{api_enum, decimal_as_number, lenient, string_serde};

/// A Pix sent and its history, as returned by
/// [`Banking::consultar_pix`](super::Banking::consultar_pix)
/// (`ConsultaPixAsyncResponse`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ConsultaPix {
    /// The payment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transacao_pix: Option<TransacaoPix>,
    /// Status changes, in the order the API returns them.
    #[serde(default, deserialize_with = "lenient::vec")]
    pub historico: Vec<EventoPix>,
}

/// A Pix payment (`PixAsyncResponse`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct TransacaoPix {
    /// Account that sent the Pix.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub conta_corrente: Option<String>,
    /// Who received it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recebedor: Option<RecebedorPix>,
    /// Why the payment failed, when it did.
    #[serde(default, deserialize_with = "lenient::vec")]
    pub erros: Vec<ErroPix>,
    /// End-to-end identifier of the Pix in the Brazilian instant payment system.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub end_to_end: Option<String>,
    /// Amount.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::decimal",
        serialize_with = "decimal_as_number::serialize_option"
    )]
    pub valor: Option<Decimal>,
    /// Current status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<StatusPix>,
    /// When the money moved, as sent by the API.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub data_hora_movimento: Option<String>,
    /// When the payment was requested, as sent by the API.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub data_hora_solicitacao: Option<String>,
    /// Pix key of the receiver, for payments by key.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub chave: Option<String>,
    /// Identifier of the request.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub codigo_solicitacao: Option<String>,
}

/// Receiver of a Pix (`DadosConta`). The API masks the CPF of people
/// (`***.777.888-**`) and sends the account only for payments by bank details.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct RecebedorPix {
    /// ISPB of the institution.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub cod_ispb: Option<String>,
    /// Branch.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub cod_agencia: Option<String>,
    /// Account number.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub nro_conta: Option<String>,
    /// CPF (masked) or CNPJ.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub cpf_cnpj: Option<String>,
    /// Name.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub nome: Option<String>,
    /// Kind of account.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub tipo_conta: Option<String>,
}

/// Why a Pix failed (`ErroPagamento`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ErroPix {
    /// Error code.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub codigo_erro: Option<String>,
    /// What went wrong.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub descricao_erro: Option<String>,
    /// Additional code.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub codigo_erro_complementar: Option<String>,
}

/// A status change of a Pix (`HistoricoResponse`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct EventoPix {
    /// Status reached.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<StatusPix>,
    /// When, as sent by the API.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub data_hora_evento: Option<String>,
}

api_enum! {
    /// Status of a Pix sent.
    pub enum StatusPix {
        /// `CRIADO`: request received.
        Criado => "CRIADO",
        /// `AGUARDANDO_APROVACAO`: waiting for approval in the Internet Banking.
        AguardandoAprovacao => "AGUARDANDO_APROVACAO",
        /// `APROVADO`: approved, about to be made.
        Aprovado => "APROVADO",
        /// `REPROVADO`: approval denied.
        Reprovado => "REPROVADO",
        /// `EXPIRADO`: not approved in time.
        Expirado => "EXPIRADO",
        /// `CANCELADO`
        Cancelado => "CANCELADO",
        /// `FALHA`: failed; see [`TransacaoPix::erros`].
        Falha => "FALHA",
        /// `AGENDADO`: scheduled.
        Agendado => "AGENDADO",
        /// `PAGO`: the receiver got the money.
        Pago => "PAGO",
        /// `ENVIADO`: sent to the receiver's institution.
        Enviado => "ENVIADO",
        /// `CANCELADO_SEM_SALDO`: cancelled for lack of balance.
        CanceladoSemSaldo => "CANCELADO_SEM_SALDO",
        /// `DEBITADO`: debited from the account.
        Debitado => "DEBITADO",
        /// `PARCIALMENTE_DEBITADO`
        ParcialmenteDebitado => "PARCIALMENTE_DEBITADO",
        /// `PARCIALMENTE_PAGO`
        ParcialmentePago => "PARCIALMENTE_PAGO",
        /// `NAO_DEBITADO`: not debited, so not paid.
        NaoDebitado => "NAO_DEBITADO",
        /// `AGENDAMENTO_CANCELADO`: scheduled payment cancelled.
        AgendamentoCancelado => "AGENDAMENTO_CANCELADO",
    }
}

string_serde!(StatusPix);

impl StatusPix {
    /// Whether the payment reached a status it does not leave: paid, or
    /// ended without paying. Scheduled payments and payments waiting for
    /// approval are not final.
    pub fn is_final(&self) -> bool {
        matches!(
            self,
            Self::Pago
                | Self::Reprovado
                | Self::Expirado
                | Self::Cancelado
                | Self::Falha
                | Self::CanceladoSemSaldo
                | Self::NaoDebitado
                | Self::AgendamentoCancelado
        )
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn parses_status_history_and_receiver() {
        let consulta: ConsultaPix = serde_json::from_value(json!({
            "transacaoPix": {
                "contaCorrente": "1234567",
                "recebedor": {"nome": "Fornecedor Exemplo", "cpfCnpj": "***.456.789-**"},
                "erros": null,
                "endToEnd": "E00000000202609231200abcdefghijk",
                "valor": 150.1,
                "status": "PAGO",
                "dataHoraMovimento": "2026-09-23T12:00:01",
                "dataHoraSolicitacao": "2026-09-23T12:00:00",
                "chave": "fornecedor@exemplo.com",
                "codigoSolicitacao": "c42f0787-02cb-4b31-827e-459ec9d7ece1"
            },
            "historico": [
                {"status": "CRIADO", "dataHoraEvento": "2026-09-23T12:00:00"},
                {"status": "PAGO", "dataHoraEvento": "2026-09-23T12:00:01"}
            ]
        }))
        .unwrap();
        let transacao = consulta.transacao_pix.unwrap();
        assert_eq!(transacao.status, Some(StatusPix::Pago));
        assert_eq!(transacao.valor, Some("150.1".parse().unwrap()));
        assert!(transacao.erros.is_empty());
        assert_eq!(
            transacao.recebedor.unwrap().nome.as_deref(),
            Some("Fornecedor Exemplo")
        );
        let historico: Vec<_> = consulta
            .historico
            .iter()
            .map(|evento| evento.status.clone().unwrap())
            .collect();
        assert_eq!(historico, [StatusPix::Criado, StatusPix::Pago]);
    }

    #[test]
    fn keeps_failures_and_unknown_statuses() {
        let consulta: ConsultaPix = serde_json::from_value(json!({
            "transacaoPix": {
                "status": "EM_PROCESSAMENTO",
                "erros": [{"codigoErro": "SALDO", "descricaoErro": "Saldo insuficiente"}]
            },
            "historico": null
        }))
        .unwrap();
        let transacao = consulta.transacao_pix.unwrap();
        assert_eq!(
            transacao.status,
            Some(StatusPix::Outro("EM_PROCESSAMENTO".to_owned()))
        );
        assert_eq!(
            transacao.erros[0].descricao_erro.as_deref(),
            Some("Saldo insuficiente")
        );
        assert!(consulta.historico.is_empty());
    }

    #[test]
    fn final_statuses() {
        let finais: Vec<&str> = StatusPix::DOCUMENTADOS
            .iter()
            .filter(|status| status.is_final())
            .map(StatusPix::as_str)
            .collect();
        assert_eq!(
            finais,
            [
                "REPROVADO",
                "EXPIRADO",
                "CANCELADO",
                "FALHA",
                "PAGO",
                "CANCELADO_SEM_SALDO",
                "NAO_DEBITADO",
                "AGENDAMENTO_CANCELADO"
            ]
        );
        assert!(!StatusPix::Outro("NOVO".to_owned()).is_final());
    }
}
