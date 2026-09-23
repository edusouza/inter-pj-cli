use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::detalhe::Detalhe;
use super::periodo::Periodo;
use crate::serde_util::{api_enum, decimal_as_number, lenient, parse_date, string_serde};

/// Direction of a transaction.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TipoOperacao {
    /// `C`: money in.
    Credito,
    /// `D`: money out.
    Debito,
    /// A value the API does not document, kept as received.
    Outro(String),
}

impl TipoOperacao {
    /// Code used by the API (`C` or `D`).
    pub fn as_str(&self) -> &str {
        match self {
            Self::Credito => "C",
            Self::Debito => "D",
            Self::Outro(raw) => raw,
        }
    }
}

impl From<&str> for TipoOperacao {
    fn from(raw: &str) -> Self {
        match raw.trim() {
            code if code.eq_ignore_ascii_case("C") => Self::Credito,
            code if code.eq_ignore_ascii_case("D") => Self::Debito,
            other => Self::Outro(other.to_owned()),
        }
    }
}

api_enum! {
    /// Kind of transaction, as classified by the API.
    ///
    /// Unknown values do not break deserialization: they are kept in
    /// [`TipoTransacao::Outro`].
    pub enum TipoTransacao {
        /// `ANTECIPACAO_RECEBIVEIS`
        AntecipacaoRecebiveis => "ANTECIPACAO_RECEBIVEIS",
        /// `ANTECIPACAO_RECEBIVEIS_CARTAO`
        AntecipacaoRecebiveisCartao => "ANTECIPACAO_RECEBIVEIS_CARTAO",
        /// `BOLETO_COBRANCA`: boleto issued by the account and paid by someone.
        BoletoCobranca => "BOLETO_COBRANCA",
        /// `CAMBIO`
        Cambio => "CAMBIO",
        /// `CASHBACK`
        Cashback => "CASHBACK",
        /// `CHEQUE`
        Cheque => "CHEQUE",
        /// `COMPRA_DEBITO`: debit card purchase.
        CompraDebito => "COMPRA_DEBITO",
        /// `DEBITO_AUTOMATICO`
        DebitoAutomatico => "DEBITO_AUTOMATICO",
        /// `DEBITO_EM_CONTA`
        DebitoEmConta => "DEBITO_EM_CONTA",
        /// `DEPOSITO_BOLETO`: deposit made by paying a boleto.
        DepositoBoleto => "DEPOSITO_BOLETO",
        /// `DOMICILIO_CARTAO`
        DomicilioCartao => "DOMICILIO_CARTAO",
        /// `ESTORNO`: reversal.
        Estorno => "ESTORNO",
        /// `FINANCIAMENTO`
        Financiamento => "FINANCIAMENTO",
        /// `IMPOSTO`: tax payment.
        Imposto => "IMPOSTO",
        /// `INTERPAG`
        Interpag => "INTERPAG",
        /// `INVESTIMENTO`
        Investimento => "INVESTIMENTO",
        /// `JUROS`: interest.
        Juros => "JUROS",
        /// `MAQUININHA_GRANITO`: card machine settlement.
        MaquininhaGranito => "MAQUININHA_GRANITO",
        /// `MULTA`: fine.
        Multa => "MULTA",
        /// `OUTROS`: other.
        Outros => "OUTROS",
        /// `PAGAMENTO`: bill payment.
        Pagamento => "PAGAMENTO",
        /// `PIX`
        Pix => "PIX",
        /// `PROVENTOS`
        Proventos => "PROVENTOS",
        /// `SAQUE`: withdrawal.
        Saque => "SAQUE",
        /// `TARIFA`: bank fee.
        Tarifa => "TARIFA",
        /// `TRANSFERENCIA`: TED/transfer.
        Transferencia => "TRANSFERENCIA",
    }
}

string_serde!(TipoOperacao, TipoTransacao);

/// Amount with the sign of the operation: negative for debits.
fn signed(valor: Option<Decimal>, tipo: Option<&TipoOperacao>) -> Option<Decimal> {
    let valor = valor?;
    Some(match tipo {
        Some(TipoOperacao::Debito) => -valor.abs(),
        Some(TipoOperacao::Credito) => valor.abs(),
        _ => valor,
    })
}

/// A transaction of the statement (`GET /banking/v2/extrato`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct TransacaoSimples {
    /// Date the transaction was posted, as sent by the API (`AAAA-MM-DD`).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub data_entrada: Option<String>,
    /// Kind of transaction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tipo_transacao: Option<TipoTransacao>,
    /// Credit or debit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tipo_operacao: Option<TipoOperacao>,
    /// Amount, always positive: see [`tipo_operacao`](Self::tipo_operacao)
    /// and [`valor_com_sinal`](Self::valor_com_sinal).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::decimal",
        serialize_with = "decimal_as_number::serialize_option"
    )]
    pub valor: Option<Decimal>,
    /// Short title (e.g. "Pix recebido").
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub titulo: Option<String>,
    /// Description, usually with the counterparty.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub descricao: Option<String>,
    /// CPMF (a tax no longer charged), as sent by the API.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub cpmf: Option<String>,
}

impl TransacaoSimples {
    /// [`data_entrada`](Self::data_entrada) as a date, when it can be parsed.
    pub fn data(&self) -> Option<NaiveDate> {
        self.data_entrada.as_deref().and_then(parse_date)
    }

    /// Amount with the sign of the operation: negative for debits.
    pub fn valor_com_sinal(&self) -> Option<Decimal> {
        signed(self.valor, self.tipo_operacao.as_ref())
    }
}

/// A transaction of the enriched statement (`GET /banking/v2/extrato/completo`),
/// with details that depend on its [`TipoTransacao`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", from = "TransacaoCompletaBruta")]
#[non_exhaustive]
pub struct TransacaoCompleta {
    /// Identifier of the transaction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id_transacao: Option<String>,
    /// Date the transaction was recorded, as sent by the API (`AAAA-MM-DD`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_inclusao: Option<String>,
    /// Date the transaction took place, as sent by the API (`AAAA-MM-DD`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_transacao: Option<String>,
    /// Kind of transaction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tipo_transacao: Option<TipoTransacao>,
    /// Credit or debit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tipo_operacao: Option<TipoOperacao>,
    /// Amount, always positive: see [`tipo_operacao`](Self::tipo_operacao)
    /// and [`valor_com_sinal`](Self::valor_com_sinal).
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "decimal_as_number::serialize_option"
    )]
    pub valor: Option<Decimal>,
    /// Short title (e.g. "Pix enviado").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub titulo: Option<String>,
    /// Description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub descricao: Option<String>,
    /// Document number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub numero_documento: Option<String>,
    /// Details specific to the kind of transaction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detalhes: Option<Detalhe>,
}

impl TransacaoCompleta {
    /// Date of the transaction ([`data_transacao`](Self::data_transacao), or
    /// [`data_inclusao`](Self::data_inclusao) when absent), when it can be parsed.
    pub fn data(&self) -> Option<NaiveDate> {
        self.data_transacao
            .as_deref()
            .and_then(parse_date)
            .or_else(|| self.data_inclusao.as_deref().and_then(parse_date))
    }

    /// Amount with the sign of the operation: negative for debits.
    pub fn valor_com_sinal(&self) -> Option<Decimal> {
        signed(self.valor, self.tipo_operacao.as_ref())
    }
}

/// Wire format of [`TransacaoCompleta`]: `detalhes` has no discriminator of
/// its own, its shape follows `tipoTransacao`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransacaoCompletaBruta {
    #[serde(default, deserialize_with = "lenient::string")]
    id_transacao: Option<String>,
    #[serde(default, deserialize_with = "lenient::string")]
    data_inclusao: Option<String>,
    #[serde(default, deserialize_with = "lenient::string")]
    data_transacao: Option<String>,
    #[serde(default)]
    tipo_transacao: Option<TipoTransacao>,
    #[serde(default)]
    tipo_operacao: Option<TipoOperacao>,
    #[serde(default, deserialize_with = "lenient::decimal")]
    valor: Option<Decimal>,
    #[serde(default, deserialize_with = "lenient::string")]
    titulo: Option<String>,
    #[serde(default, deserialize_with = "lenient::string")]
    descricao: Option<String>,
    #[serde(default, deserialize_with = "lenient::string")]
    numero_documento: Option<String>,
    #[serde(default)]
    detalhes: Option<serde_json::Value>,
}

impl From<TransacaoCompletaBruta> for TransacaoCompleta {
    fn from(bruta: TransacaoCompletaBruta) -> Self {
        let detalhes = bruta
            .detalhes
            .filter(|value| !value.is_null())
            .map(|value| Detalhe::from_api(bruta.tipo_transacao.as_ref(), value));
        Self {
            id_transacao: bruta.id_transacao,
            data_inclusao: bruta.data_inclusao,
            data_transacao: bruta.data_transacao,
            tipo_transacao: bruta.tipo_transacao,
            tipo_operacao: bruta.tipo_operacao,
            valor: bruta.valor,
            titulo: bruta.titulo,
            descricao: bruta.descricao,
            numero_documento: bruta.numero_documento,
            detalhes,
        }
    }
}

/// A page of the enriched statement, in the traditional pagination mode.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PaginaExtrato {
    /// Number of pages.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::u64"
    )]
    pub total_paginas: Option<u64>,
    /// Number of transactions in the period, across every page.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::u64"
    )]
    pub total_elementos: Option<u64>,
    /// Whether this is the last page.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::bool"
    )]
    pub ultima_pagina: Option<bool>,
    /// Whether this is the first page.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::bool"
    )]
    pub primeira_pagina: Option<bool>,
    /// Page size requested.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::u64"
    )]
    pub tamanho_pagina: Option<u64>,
    /// Transactions on this page.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::u64"
    )]
    pub numero_de_elementos: Option<u64>,
    /// The transactions.
    #[serde(default)]
    pub transacoes: Vec<TransacaoCompleta>,
}

impl PaginaExtrato {
    /// Whether another page follows page `numero` (0-based), judging by the
    /// counters the API sent; without counters, a full page suggests so.
    pub(crate) fn tem_mais(&self, numero: u32, tamanho_pedido: u32) -> bool {
        if let Some(ultima) = self.ultima_pagina {
            return !ultima;
        }
        if let Some(total) = self.total_paginas {
            return u64::from(numero) + 1 < total;
        }
        !self.transacoes.is_empty()
            && self.transacoes.len() >= usize::try_from(tamanho_pedido).unwrap_or(usize::MAX)
    }
}

/// A batch of the enriched statement in scroll mode.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct LoteScroll {
    /// Number of transactions in the period.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::u64"
    )]
    pub total_elementos: Option<u64>,
    /// Transactions in this batch.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::u64"
    )]
    pub numero_de_elementos: Option<u64>,
    /// Identifier to fetch the next batch; absent once the scroll is over.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub scroll_id: Option<String>,
    /// Whether more batches follow.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::bool"
    )]
    pub has_more: Option<bool>,
    /// The transactions.
    #[serde(default)]
    pub transacoes: Vec<TransacaoCompleta>,
}

/// Filters of the enriched statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FiltroExtrato {
    periodo: Periodo,
    tipo_operacao: Option<TipoOperacao>,
    tipo_transacao: Option<TipoTransacao>,
}

impl FiltroExtrato {
    /// Every transaction of the period.
    pub fn new(periodo: Periodo) -> Self {
        Self {
            periodo,
            tipo_operacao: None,
            tipo_transacao: None,
        }
    }

    /// Only credits or only debits.
    #[must_use]
    pub fn tipo_operacao(mut self, tipo: TipoOperacao) -> Self {
        self.tipo_operacao = Some(tipo);
        self
    }

    /// Only transactions of this kind.
    #[must_use]
    pub fn tipo_transacao(mut self, tipo: TipoTransacao) -> Self {
        self.tipo_transacao = Some(tipo);
        self
    }

    /// Period of the query.
    pub fn periodo(&self) -> Periodo {
        self.periodo
    }

    pub(crate) fn query(&self) -> Vec<(&'static str, String)> {
        let mut query = self.periodo.query().to_vec();
        if let Some(tipo) = &self.tipo_operacao {
            query.push(("tipoOperacao", tipo.as_str().to_owned()));
        }
        if let Some(tipo) = &self.tipo_transacao {
            query.push(("tipoTransacao", tipo.as_str().to_owned()));
        }
        query
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::banking::detalhe::{DetalhePix, DetalheTransferencia};

    fn dec(s: &str) -> Decimal {
        s.parse().unwrap()
    }

    #[test]
    fn operation_codes_round_trip_and_tolerate_unknown_values() {
        assert_eq!(TipoOperacao::from("C"), TipoOperacao::Credito);
        assert_eq!(TipoOperacao::from(" d "), TipoOperacao::Debito);
        assert_eq!(TipoOperacao::from("X"), TipoOperacao::Outro("X".into()));
        let json = serde_json::to_string(&[TipoOperacao::Credito, TipoOperacao::Debito]).unwrap();
        assert_eq!(json, r#"["C","D"]"#);
    }

    #[test]
    fn transaction_types_round_trip_and_tolerate_unknown_values() {
        for tipo in TipoTransacao::DOCUMENTADOS {
            assert_eq!(&TipoTransacao::from(tipo.as_str()), tipo);
            let json = serde_json::to_value(tipo).unwrap();
            assert_eq!(
                serde_json::from_value::<TipoTransacao>(json).unwrap(),
                *tipo
            );
        }
        assert_eq!(TipoTransacao::DOCUMENTADOS.len(), 26);
        let novo: TipoTransacao = serde_json::from_str(r#""CRIPTO""#).unwrap();
        assert_eq!(novo, TipoTransacao::Outro("CRIPTO".into()));
        assert_eq!(novo.to_string(), "CRIPTO");
    }

    #[test]
    fn simple_transaction_parses_string_values_exactly() {
        let transacao: TransacaoSimples = serde_json::from_value(json!({
            "cpmf": "0.00",
            "dataEntrada": "2026-08-03",
            "tipoTransacao": "PIX",
            "tipoOperacao": "D",
            "valor": "1250.10",
            "titulo": "Pix enviado",
            "descricao": "PIX ENVIADO - Fornecedor",
            "campoNovo": 1
        }))
        .unwrap();
        assert_eq!(transacao.valor, Some(dec("1250.10")));
        assert_eq!(transacao.valor_com_sinal(), Some(dec("-1250.10")));
        assert_eq!(transacao.data(), NaiveDate::from_ymd_opt(2026, 8, 3));
        assert_eq!(transacao.tipo_transacao, Some(TipoTransacao::Pix));
        assert_eq!(
            serde_json::to_value(&transacao).unwrap()["valor"],
            json!(1250.1)
        );
    }

    #[test]
    fn details_follow_the_transaction_type() {
        let pix: TransacaoCompleta = serde_json::from_value(json!({
            "tipoTransacao": "PIX",
            "tipoOperacao": "C",
            "valor": "10",
            "detalhes": {"txId": "abc", "nomePagador": "Cliente", "extra": true}
        }))
        .unwrap();
        let Some(Detalhe::Pix(DetalhePix {
            tx_id,
            nome_pagador,
            outros,
            ..
        })) = &pix.detalhes
        else {
            panic!("{:?}", pix.detalhes);
        };
        assert_eq!(tx_id.as_deref(), Some("abc"));
        assert_eq!(nome_pagador.as_deref(), Some("Cliente"));
        assert_eq!(outros["extra"], json!(true));

        let ted: TransacaoCompleta = serde_json::from_value(json!({
            "tipoTransacao": "TRANSFERENCIA",
            "detalhes": {"nomeRecebedor": "Fornecedor", "dataEfetivacao": "21/06/2022"}
        }))
        .unwrap();
        assert!(matches!(
            &ted.detalhes,
            Some(Detalhe::Transferencia(DetalheTransferencia { nome_recebedor: Some(n), .. })) if n == "Fornecedor"
        ));
    }

    #[test]
    fn details_of_undocumented_types_are_kept_as_received() {
        let transacao: TransacaoCompleta = serde_json::from_value(json!({
            "tipoTransacao": "INVESTIMENTO",
            "detalhes": {"produto": "CDB", "taxa": 1.1}
        }))
        .unwrap();
        assert_eq!(
            transacao.detalhes,
            Some(Detalhe::Outro(json!({"produto": "CDB", "taxa": 1.1})))
        );
        // Details that do not fit the documented shape are not lost either.
        let estranho: TransacaoCompleta = serde_json::from_value(json!({
            "tipoTransacao": "PIX",
            "detalhes": {"txId": {"aninhado": true}}
        }))
        .unwrap();
        assert!(matches!(estranho.detalhes, Some(Detalhe::Outro(_))));
        let nulo: TransacaoCompleta =
            serde_json::from_value(json!({"tipoTransacao": "PIX", "detalhes": null})).unwrap();
        assert_eq!(nulo.detalhes, None);
    }

    #[test]
    fn complete_transaction_serializes_with_api_names() {
        let original = json!({
            "idTransacao": "1",
            "dataInclusao": "2026-08-03",
            "dataTransacao": "2026-08-02",
            "tipoTransacao": "PIX",
            "tipoOperacao": "C",
            "valor": 99.9,
            "titulo": "Pix recebido",
            "descricao": "Cliente",
            "numeroDocumento": "0",
            "detalhes": {"txId": "abc", "endToEndId": "E1"}
        });
        let transacao: TransacaoCompleta = serde_json::from_value(original.clone()).unwrap();
        assert_eq!(serde_json::to_value(&transacao).unwrap(), original);
        assert_eq!(transacao.data(), NaiveDate::from_ymd_opt(2026, 8, 2));
    }

    #[test]
    fn page_tells_whether_more_pages_follow() {
        let pagina = |value| serde_json::from_value::<PaginaExtrato>(value).unwrap();
        assert!(pagina(json!({"ultimaPagina": false})).tem_mais(5, 50));
        assert!(!pagina(json!({"ultimaPagina": true, "totalPaginas": 9})).tem_mais(0, 50));
        assert!(pagina(json!({"totalPaginas": "3"})).tem_mais(1, 50));
        assert!(!pagina(json!({"totalPaginas": 3})).tem_mais(2, 50));
        let cheia = json!({"transacoes": [{}, {}]});
        assert!(pagina(cheia.clone()).tem_mais(0, 2));
        assert!(!pagina(cheia).tem_mais(0, 3));
        assert!(!pagina(json!({})).tem_mais(0, 50));
    }

    #[test]
    fn filter_builds_the_query() {
        let periodo = Periodo::new(
            NaiveDate::from_ymd_opt(2026, 8, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 8, 31).unwrap(),
        )
        .unwrap();
        let filtro = FiltroExtrato::new(periodo)
            .tipo_operacao(TipoOperacao::Debito)
            .tipo_transacao(TipoTransacao::Pagamento);
        assert_eq!(
            filtro.query(),
            [
                ("dataInicio", "2026-08-01".to_owned()),
                ("dataFim", "2026-08-31".to_owned()),
                ("tipoOperacao", "D".to_owned()),
                ("tipoTransacao", "PAGAMENTO".to_owned()),
            ]
        );
    }
}
