use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use super::darf::{PagamentoDarf, PagamentoDarfError};
use super::pagamento::{PagamentoBoleto, PagamentoBoletoError};
use crate::documento::Documento;
use crate::serde_util::{api_enum, decimal_as_number, lenient, string_serde};

/// Fewest payments in a [`LotePagamentos`].
pub const MIN_PAGAMENTOS_LOTE: usize = 2;

/// Most payments in a [`LotePagamentos`].
pub const MAX_PAGAMENTOS_LOTE: usize = 150;

/// Longest [`LotePagamentos::meu_identificador`], in characters.
pub const MAX_MEU_IDENTIFICADOR: usize = 30;

/// A batch of payments by barcode and DARFs, to send with
/// [`Banking::enviar_lote`](super::Banking::enviar_lote) (`PagarLoteRequest`).
///
/// The API accepts the batch and pays it afterwards: follow it with
/// [`Banking::consultar_lote`](super::Banking::consultar_lote).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LotePagamentos {
    /// Your own identifier of the batch, up to [`MAX_MEU_IDENTIFICADOR`]
    /// characters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meu_identificador: Option<String>,
    /// From [`MIN_PAGAMENTOS_LOTE`] to [`MAX_PAGAMENTOS_LOTE`] payments.
    pub pagamentos: Vec<ItemLote>,
}

impl LotePagamentos {
    /// Sum of the amounts of the payments.
    pub fn valor_total(&self) -> Decimal {
        self.pagamentos.iter().map(ItemLote::valor).sum()
    }

    /// Checks the batch and each payment, as
    /// [`Banking::enviar_lote`](super::Banking::enviar_lote) does before
    /// sending.
    ///
    /// # Errors
    ///
    /// Returns the first problem found. [`ItemLote::validar`] checks the
    /// payments one by one, to report all of them.
    pub fn validar(&self) -> Result<(), LotePagamentosError> {
        if let Some(identificador) = &self.meu_identificador
            && (identificador.trim().is_empty()
                || identificador.chars().count() > MAX_MEU_IDENTIFICADOR)
        {
            return Err(LotePagamentosError::MeuIdentificador);
        }
        let quantidade = self.pagamentos.len();
        if !(MIN_PAGAMENTOS_LOTE..=MAX_PAGAMENTOS_LOTE).contains(&quantidade) {
            return Err(LotePagamentosError::Quantidade(quantidade));
        }
        for (indice, item) in self.pagamentos.iter().enumerate() {
            item.validar()
                .map_err(|erro| LotePagamentosError::Item { indice, erro })?;
        }
        Ok(())
    }
}

/// A payment of a [`LotePagamentos`], sent with its kind in `tipoPagamento`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ItemLote {
    /// `BOLETO`: a boleto, utility bill or tax with a barcode
    /// (`RequestBoletoLote`).
    Boleto(PagamentoBoleto),
    /// `DARF`: a DARF without a barcode (`RequestDarfLote`).
    Darf(PagamentoDarf),
}

impl ItemLote {
    /// Amount taken from the account: [`PagamentoBoleto::valor_pagar`] or
    /// [`PagamentoDarf::valor_total`].
    pub fn valor(&self) -> Decimal {
        match self {
            Self::Boleto(boleto) => boleto.valor_pagar,
            Self::Darf(darf) => darf.valor_total(),
        }
    }

    /// Checks the payment with [`PagamentoBoleto::validar`] or
    /// [`PagamentoDarf::validar`].
    ///
    /// # Errors
    ///
    /// Returns the first problem found.
    pub fn validar(&self) -> Result<(), ItemLoteError> {
        match self {
            Self::Boleto(boleto) => boleto.validar().map_err(ItemLoteError::Boleto),
            Self::Darf(darf) => darf.validar().map_err(ItemLoteError::Darf),
        }
    }
}

impl From<PagamentoBoleto> for ItemLote {
    fn from(boleto: PagamentoBoleto) -> Self {
        Self::Boleto(boleto)
    }
}

impl From<PagamentoDarf> for ItemLote {
    fn from(darf: PagamentoDarf) -> Self {
        Self::Darf(darf)
    }
}

impl Serialize for ItemLote {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        /// `RequestBoletoLote`: the fields of [`PagamentoBoleto`], except
        /// that the batch documents `valorPagar` as a number.
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Boleto<'a> {
            tipo_pagamento: &'static str,
            cod_barra_linha_digitavel: &'a str,
            #[serde(serialize_with = "decimal_as_number::serialize")]
            valor_pagar: Decimal,
            #[serde(skip_serializing_if = "Option::is_none")]
            data_pagamento: Option<NaiveDate>,
            data_vencimento: NaiveDate,
            #[serde(skip_serializing_if = "Option::is_none")]
            cpf_cnpj_beneficiario: Option<&'a Documento>,
        }

        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Darf<'a> {
            tipo_pagamento: &'static str,
            #[serde(flatten)]
            darf: &'a PagamentoDarf,
        }

        match self {
            Self::Boleto(boleto) => Boleto {
                tipo_pagamento: "BOLETO",
                cod_barra_linha_digitavel: boleto.codigo.codigo_barras(),
                valor_pagar: boleto.valor_pagar.round_dp(2),
                data_pagamento: boleto.data_pagamento,
                data_vencimento: boleto.data_vencimento,
                cpf_cnpj_beneficiario: boleto.cpf_cnpj_beneficiario.as_ref(),
            }
            .serialize(serializer),
            Self::Darf(darf) => Darf {
                tipo_pagamento: "DARF",
                darf,
            }
            .serialize(serializer),
        }
    }
}

/// Why a [`LotePagamentos`] cannot be sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum LotePagamentosError {
    /// Fewer than [`MIN_PAGAMENTOS_LOTE`] or more than
    /// [`MAX_PAGAMENTOS_LOTE`] payments.
    #[error("o lote deve ter de 2 a 150 pagamentos, e tem {0}")]
    Quantidade(usize),
    /// The identifier is blank or longer than [`MAX_MEU_IDENTIFICADOR`].
    #[error("o identificador do lote deve ter de 1 a 30 caracteres")]
    MeuIdentificador,
    /// A payment is invalid.
    #[error("pagamento {}: {erro}", .indice + 1)]
    Item {
        /// Position of the payment in [`LotePagamentos::pagamentos`], from 0.
        indice: usize,
        /// What is wrong with it.
        erro: ItemLoteError,
    },
}

/// Why a payment of a batch cannot be sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ItemLoteError {
    /// A payment by barcode.
    #[error(transparent)]
    Boleto(#[from] PagamentoBoletoError),
    /// A DARF.
    #[error(transparent)]
    Darf(#[from] PagamentoDarfError),
}

/// Answer to [`Banking::enviar_lote`](super::Banking::enviar_lote)
/// (`PagarLoteResponse`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct SolicitacaoLote {
    /// Identifier of the batch, 24 characters, to follow it with
    /// [`Banking::consultar_lote`](super::Banking::consultar_lote).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub id_lote: Option<String>,
    /// Status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<StatusLote>,
    /// Your own identifier of the batch.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub meu_identificador: Option<String>,
    /// Payments in the batch.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::u64"
    )]
    pub qtde_pagamentos: Option<u64>,
}

api_enum! {
    /// Status of a batch (`StatusLoteEnum`).
    pub enum StatusLote {
        /// `EMPROCESSAMENTO`: the payments are being processed.
        EmProcessamento => "EMPROCESSAMENTO",
        /// `PROCESSADOCOMERRO`: processed; some payments failed.
        ProcessadoComErro => "PROCESSADOCOMERRO",
        /// `PROCESSADOSEMERRO`: processed without errors.
        ProcessadoSemErro => "PROCESSADOSEMERRO",
    }
}

string_serde!(StatusLote);

impl StatusLote {
    /// Whether the batch was processed, with or without errors. The status
    /// of each payment is in [`Lote::pagamentos`].
    pub fn is_final(&self) -> bool {
        matches!(self, Self::ProcessadoComErro | Self::ProcessadoSemErro)
    }
}

/// A batch and its payments, as returned by
/// [`Banking::consultar_lote`](super::Banking::consultar_lote)
/// (`ObterLoteResponse`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Lote {
    /// Identifier of the batch.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub id_lote: Option<String>,
    /// Status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<StatusLote>,
    /// Your own identifier of the batch.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub meu_identificador: Option<String>,
    /// Payments in the batch.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::u64"
    )]
    pub qtde_pagamentos: Option<u64>,
    /// Account that pays the batch.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub conta_corrente: Option<String>,
    /// When the batch was received, as sent by the API.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub data_criacao: Option<String>,
    /// The payments, in the order the API returns them.
    #[serde(default, deserialize_with = "lenient::vec")]
    pub pagamentos: Vec<PagamentoDoLote>,
}

/// A payment of a [`Lote`], by its kind (`tipoPagamento`).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PagamentoDoLote {
    /// `BOLETO` (`ResponseBoletoLote`).
    Boleto(BoletoDoLote),
    /// `DARF` (`ResponseDarfLote`).
    Darf(DarfDoLote),
    /// A payment of another kind, or that does not fit the documented
    /// shape, as received.
    Outro(Value),
}

impl PagamentoDoLote {
    /// Kind of the payment (`BOLETO`, `DARF`), as sent by the API.
    pub fn tipo(&self) -> Option<&str> {
        match self {
            Self::Boleto(_) => Some(BOLETO),
            Self::Darf(_) => Some(DARF),
            Self::Outro(value) => value.get(TIPO_PAGAMENTO).and_then(Value::as_str),
        }
    }
}

const TIPO_PAGAMENTO: &str = "tipoPagamento";
const BOLETO: &str = "BOLETO";
const DARF: &str = "DARF";

impl<'de> Deserialize<'de> for PagamentoDoLote {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        let typed = match value.get(TIPO_PAGAMENTO).and_then(Value::as_str) {
            Some(BOLETO) => BoletoDoLote::deserialize(&value).ok().map(Self::Boleto),
            Some(DARF) => DarfDoLote::deserialize(&value).ok().map(Self::Darf),
            _ => None,
        };
        Ok(typed.unwrap_or(Self::Outro(value)))
    }
}

impl Serialize for PagamentoDoLote {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct ComTipo<'a, T> {
            #[serde(rename = "tipoPagamento")]
            tipo: &'static str,
            #[serde(flatten)]
            pagamento: &'a T,
        }

        match self {
            Self::Boleto(pagamento) => ComTipo {
                tipo: BOLETO,
                pagamento,
            }
            .serialize(serializer),
            Self::Darf(pagamento) => ComTipo {
                tipo: DARF,
                pagamento,
            }
            .serialize(serializer),
            Self::Outro(value) => value.serialize(serializer),
        }
    }
}

/// A payment by barcode of a [`Lote`] (`ResponseBoletoLote`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct BoletoDoLote {
    /// Position of the payment in the batch, as sent by the API.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub seq_id: Option<String>,
    /// Identifier of the payment, as in
    /// [`Pagamento::codigo_transacao`](super::Pagamento::codigo_transacao).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub codigo_transacao: Option<String>,
    /// Status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<StatusBoletoDoLote>,
    /// Details of the status, such as why the payment failed.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub detalhe: Option<String>,
    /// Barcode or line paid.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub cod_barra_linha_digitavel: Option<String>,
    /// Amount.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::decimal",
        serialize_with = "decimal_as_number::serialize_option"
    )]
    pub valor_pagar: Option<Decimal>,
    /// Day of the payment.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub data_pagamento: Option<String>,
    /// Due date.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub data_vencimento: Option<String>,
    /// Unique sequential number, internal to Inter.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub nsu: Option<String>,
}

/// A DARF of a [`Lote`] (`ResponseDarfLote`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct DarfDoLote {
    /// Position of the payment in the batch, as sent by the API.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub seq_id: Option<String>,
    /// Identifier of the payment, as in
    /// [`Darf::codigo_solicitacao`](super::Darf::codigo_solicitacao).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub codigo_solicitacao: Option<String>,
    /// Status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<StatusDarfDoLote>,
    /// Details of the status, such as why the payment failed.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub detalhe: Option<String>,
    /// Kind of DARF (e.g. `PRETO`).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub tipo_darf: Option<String>,
    /// CPF or CNPJ of the taxpayer.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub cnpj_cpf: Option<String>,
    /// Description.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub descricao: Option<String>,
    /// Name of the taxpayer.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub nome_empresa: Option<String>,
    /// Phone of the taxpayer.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub telefone_empresa: Option<String>,
    /// Principal amount.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::decimal",
        serialize_with = "decimal_as_number::serialize_option"
    )]
    pub valor: Option<Decimal>,
    /// Fine.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::decimal",
        serialize_with = "decimal_as_number::serialize_option"
    )]
    pub valor_multa: Option<Decimal>,
    /// Interest.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::decimal",
        serialize_with = "decimal_as_number::serialize_option"
    )]
    pub valor_juros: Option<Decimal>,
    /// Total.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::decimal",
        serialize_with = "decimal_as_number::serialize_option"
    )]
    pub valor_total: Option<Decimal>,
    /// Tax period.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub periodo_apuracao: Option<String>,
    /// Day of the payment.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub data_pagamento: Option<String>,
    /// Reference number.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub referencia: Option<String>,
    /// Due date.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub data_vencimento: Option<String>,
    /// Revenue code.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub codigo_receita: Option<String>,
}

api_enum! {
    /// Status of a payment by barcode in a batch (`StatusPagamentoBoleto`).
    pub enum StatusBoletoDoLote {
        /// `EMPROCESSAMENTO`: initial status, before the payment is processed.
        EmProcessamento => "EMPROCESSAMENTO",
        /// `REALIZADO`: paid.
        Realizado => "REALIZADO",
        /// `AGENDADO`: scheduled.
        Agendado => "AGENDADO",
        /// `AGUARDANDO_APROVACAO`: waiting for approval in the Internet Banking.
        AguardandoAprovacao => "AGUARDANDO_APROVACAO",
        /// `APROVADO`: approved.
        Aprovado => "APROVADO",
        /// `CANCELADO`: cancelled.
        Cancelado => "CANCELADO",
        /// `REPROVADO`: approval denied.
        Reprovado => "REPROVADO",
        /// `ERRO`: failed.
        Erro => "ERRO",
        /// `NAO_COMPENSADO`: not cleared.
        NaoCompensado => "NAO_COMPENSADO",
        /// `APROVADO_NOVO_PAGAMENTO`: approved, to be paid again.
        AprovadoNovoPagamento => "APROVADO_NOVO_PAGAMENTO",
        /// `APROVADO_AGUARDO_RETENTATIVA`: approved, waiting for a new attempt.
        AprovadoAguardoRetentativa => "APROVADO_AGUARDO_RETENTATIVA",
        /// `AGENDADO_REALIZADO`: scheduled and paid.
        AgendadoRealizado => "AGENDADO_REALIZADO",
        /// `AGENDADO_NAO_REALIZADO`: scheduled, but not paid.
        AgendadoNaoRealizado => "AGENDADO_NAO_REALIZADO",
        /// `AGENDADO_CANCELADO`: scheduled, then cancelled.
        AgendadoCancelado => "AGENDADO_CANCELADO",
        /// `APROVACAO_EXPIRADA`: not approved in time.
        AprovacaoExpirada => "APROVACAO_EXPIRADA",
        /// `ERRO_PAGAMENTO`: the payment failed.
        ErroPagamento => "ERRO_PAGAMENTO",
        /// `PAGO`: paid.
        Pago => "PAGO",
        /// `PAGAMENTO_AGENDADO`: scheduled.
        PagamentoAgendado => "PAGAMENTO_AGENDADO",
        /// `PAGAMENTO_COBRANCA_AGENDADO`: payment of a collection scheduled.
        PagamentoCobrancaAgendado => "PAGAMENTO_COBRANCA_AGENDADO",
    }
}

string_serde!(StatusBoletoDoLote);

api_enum! {
    /// Status of a DARF in a batch (`StatusPagamentoDarf`).
    pub enum StatusDarfDoLote {
        /// `EMPROCESSAMENTO`: initial status, before the payment is processed.
        EmProcessamento => "EMPROCESSAMENTO",
        /// `PAGO`: paid.
        Pago => "PAGO",
        /// `PAGAMENTO_AGENDADO`: scheduled.
        PagamentoAgendado => "PAGAMENTO_AGENDADO",
        /// `AGENDAMENTO_CANCELADO`: scheduled, then cancelled.
        AgendamentoCancelado => "AGENDAMENTO_CANCELADO",
        /// `NAO_COMPENSADO`: not cleared.
        NaoCompensado => "NAO_COMPENSADO",
        /// `ERRO_PAGAMENTO`: the payment failed.
        ErroPagamento => "ERRO_PAGAMENTO",
        /// `AGUARDANDO_APROVACAO`: waiting for approval in the Internet Banking.
        AguardandoAprovacao => "AGUARDANDO_APROVACAO",
        /// `APROVADO`: approved.
        Aprovado => "APROVADO",
        /// `CANCELADO`: cancelled.
        Cancelado => "CANCELADO",
    }
}

string_serde!(StatusDarfDoLote);

/// Whether `id` looks like the identifier of a batch: 24 ASCII letters or
/// digits.
pub(crate) fn is_id_lote(id: &str) -> bool {
    id.len() == 24 && id.bytes().all(|b| b.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const LINHA: &str = "07797777051167847115990071126347192950000003010";

    fn data(ano: i32, mes: u32, dia: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(ano, mes, dia).unwrap()
    }

    fn boleto(valor: &str) -> PagamentoBoleto {
        PagamentoBoleto::new(
            LINHA.parse().unwrap(),
            valor.parse().unwrap(),
            data(2026, 10, 10),
        )
    }

    fn darf() -> PagamentoDarf {
        PagamentoDarf {
            cnpj_cpf: "12.345.678/0001-95".parse().unwrap(),
            codigo_receita: "0220".to_owned(),
            data_vencimento: data(2026, 10, 30),
            descricao: "IRPJ de setembro".to_owned(),
            nome_empresa: "Empresa Exemplo".to_owned(),
            telefone_empresa: None,
            periodo_apuracao: data(2026, 9, 30),
            valor_principal: "47.14".parse().unwrap(),
            valor_multa: None,
            valor_juros: Some("10.11".parse().unwrap()),
            referencia: "13609400849201739".to_owned(),
        }
    }

    fn lote() -> LotePagamentos {
        LotePagamentos {
            meu_identificador: Some("Despesas de outubro".to_owned()),
            pagamentos: vec![boleto("30.1").into(), darf().into()],
        }
    }

    #[test]
    fn serializes_each_payment_with_its_kind() {
        assert_eq!(
            serde_json::to_value(lote()).unwrap(),
            json!({
                "meuIdentificador": "Despesas de outubro",
                "pagamentos": [
                    {
                        "tipoPagamento": "BOLETO",
                        "codBarraLinhaDigitavel": "07791929500000030107777011678471159007112634",
                        "valorPagar": 30.1,
                        "dataVencimento": "2026-10-10"
                    },
                    {
                        "tipoPagamento": "DARF",
                        "cnpjCpf": "12345678000195",
                        "codigoReceita": "0220",
                        "dataVencimento": "2026-10-30",
                        "descricao": "IRPJ de setembro",
                        "nomeEmpresa": "Empresa Exemplo",
                        "periodoApuracao": "2026-09-30",
                        "valorPrincipal": 47.14,
                        "valorJuros": 10.11,
                        "referencia": "13609400849201739"
                    }
                ]
            })
        );
        assert_eq!(lote().valor_total(), "87.35".parse::<Decimal>().unwrap());
    }

    #[test]
    fn validates_the_batch_and_its_payments() {
        assert_eq!(lote().validar(), Ok(()));

        let mut sozinho = lote();
        sozinho.pagamentos.truncate(1);
        assert_eq!(sozinho.validar(), Err(LotePagamentosError::Quantidade(1)));

        let mut cheio = lote();
        cheio.pagamentos = vec![boleto("1").into(); MAX_PAGAMENTOS_LOTE + 1];
        assert_eq!(cheio.validar(), Err(LotePagamentosError::Quantidade(151)));
        cheio.pagamentos.pop();
        assert_eq!(cheio.validar(), Ok(()));

        for identificador in [" ", &"x".repeat(31)] {
            let mut lote = lote();
            lote.meu_identificador = Some(identificador.to_owned());
            assert_eq!(lote.validar(), Err(LotePagamentosError::MeuIdentificador));
        }

        let mut invalido = lote();
        invalido.pagamentos.push(boleto("0").into());
        let erro = invalido.validar().unwrap_err();
        assert_eq!(
            erro,
            LotePagamentosError::Item {
                indice: 2,
                erro: ItemLoteError::Boleto(PagamentoBoletoError::ValorNaoPositivo),
            }
        );
        assert_eq!(
            erro.to_string(),
            "pagamento 3: o valor a pagar deve ser maior que zero"
        );

        let mut darf = darf();
        darf.codigo_receita = "22".to_owned();
        assert_eq!(
            ItemLote::from(darf).validar(),
            Err(ItemLoteError::Darf(PagamentoDarfError::CodigoReceita))
        );
    }

    #[test]
    fn parses_a_batch_with_every_kind_of_payment() {
        let lote: Lote = serde_json::from_value(json!({
            "idLote": "0123456789abcdef01234567",
            "status": "PROCESSADOCOMERRO",
            "meuIdentificador": "Despesas de outubro",
            "qtdePagamentos": "3",
            "contaCorrente": "1234567",
            "dataCriacao": "2026-10-01T10:00:00",
            "pagamentos": [
                {
                    "tipoPagamento": "BOLETO",
                    "seqId": 1,
                    "codigoTransacao": "3414f226-36fb-4d87-811e-cfd99911d845",
                    "status": "PAGO",
                    "codBarraLinhaDigitavel": "07791929500000030107777011678471159007112634",
                    "valorPagar": 30.1,
                    "dataVencimento": "2026-10-10",
                    "nsu": 82_127_399
                },
                {
                    "tipoPagamento": "DARF",
                    "seqId": 2,
                    "status": "ERRO_PAGAMENTO",
                    "detalhe": "Saldo insuficiente",
                    "valorTotal": "57.25",
                    "codigoReceita": 220
                },
                {"tipoPagamento": "PIX", "valor": 1}
            ]
        }))
        .unwrap();
        assert_eq!(lote.status, Some(StatusLote::ProcessadoComErro));
        assert!(lote.status.as_ref().is_some_and(StatusLote::is_final));
        assert_eq!(lote.qtde_pagamentos, Some(3));
        let [boleto, darf, pix] = &lote.pagamentos[..] else {
            panic!("{:?}", lote.pagamentos);
        };

        let PagamentoDoLote::Boleto(boleto) = boleto else {
            panic!("{boleto:?}");
        };
        assert_eq!(boleto.seq_id.as_deref(), Some("1"));
        assert_eq!(boleto.status, Some(StatusBoletoDoLote::Pago));
        assert_eq!(boleto.valor_pagar, Some("30.1".parse().unwrap()));
        assert_eq!(boleto.nsu.as_deref(), Some("82127399"));

        let PagamentoDoLote::Darf(darf) = darf else {
            panic!("{darf:?}");
        };
        assert_eq!(darf.status, Some(StatusDarfDoLote::ErroPagamento));
        assert_eq!(darf.detalhe.as_deref(), Some("Saldo insuficiente"));
        assert_eq!(darf.valor_total, Some("57.25".parse().unwrap()));
        assert_eq!(darf.codigo_receita.as_deref(), Some("220"));

        assert_eq!(pix.tipo(), Some("PIX"));
        assert!(matches!(pix, PagamentoDoLote::Outro(_)));
    }

    #[test]
    fn keeps_payments_that_do_not_fit_and_serializes_them_back() {
        let valores = [
            json!({"tipoPagamento": "BOLETO", "status": ["PAGO"]}),
            json!({"semTipo": true}),
            json!("texto"),
        ];
        for valor in valores {
            let pagamento: PagamentoDoLote = serde_json::from_value(valor.clone()).unwrap();
            assert_eq!(pagamento, PagamentoDoLote::Outro(valor.clone()));
            assert_eq!(serde_json::to_value(&pagamento).unwrap(), valor);
        }

        let boleto: PagamentoDoLote = serde_json::from_value(json!({
            "tipoPagamento": "BOLETO",
            "status": "AGENDADO",
            "valorPagar": "10.50"
        }))
        .unwrap();
        assert_eq!(boleto.tipo(), Some("BOLETO"));
        assert_eq!(
            serde_json::to_value(&boleto).unwrap(),
            json!({"tipoPagamento": "BOLETO", "status": "AGENDADO", "valorPagar": 10.5})
        );

        let lote: Lote = serde_json::from_value(json!({"pagamentos": null})).unwrap();
        assert!(lote.pagamentos.is_empty());
    }

    #[test]
    fn final_statuses_of_a_batch() {
        let finais: Vec<&str> = StatusLote::DOCUMENTADOS
            .iter()
            .filter(|status| status.is_final())
            .map(StatusLote::as_str)
            .collect();
        assert_eq!(finais, ["PROCESSADOCOMERRO", "PROCESSADOSEMERRO"]);
        assert!(!StatusLote::Outro("NOVO".to_owned()).is_final());
    }

    #[test]
    fn batch_identifiers() {
        assert!(is_id_lote("0123456789abcdef01234567"));
        assert!(is_id_lote("ABCDEFGHIJKLMNOPQRSTUVWX"));
        for invalido in [
            "",
            "0123456789abcdef0123456",
            "0123456789abcdef012345678",
            "0123456789abcdef0123456/",
            "0123456789abcdef012345.7",
            "0123456789abcdef01234ç6",
        ] {
            assert!(!is_id_lote(invalido), "{invalido}");
        }
    }
}
