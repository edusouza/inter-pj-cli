use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::serde_util::{api_enum, decimal_as_number, lenient, string_serde};

/// Answer to [`Cobranca::emitir`](super::Cobranca::emitir)
/// (`EmitirCobrancaAsyncResponse`). The charge is issued afterwards: follow
/// it with [`Cobranca::consultar`](super::Cobranca::consultar).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct SolicitacaoCobranca {
    /// Identifier of the charge in the API.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub codigo_solicitacao: Option<String>,
}

/// A charge, as returned by [`Cobranca::consultar`](super::Cobranca::consultar)
/// (`CobrancaDetalhadaResponseBody`) and in the pages of the listing
/// (`CobrancaResponse`, with fewer fields).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct CobrancaDetalhada {
    /// The charge itself.
    #[serde(default)]
    pub cobranca: DadosCobranca,
    /// Barcode of the boleto; absent while the charge is being issued.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boleto: Option<BoletoCobranca>,
    /// Pix QR Code; absent when the account has no Pix key or the Pix could
    /// not be created.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pix: Option<PixCobranca>,
    /// Invoice the charge refers to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nota_fiscal: Option<NotaFiscalCobranca>,
}

/// The data of a charge (`cobranca`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct DadosCobranca {
    /// Identifier of the charge in the API.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub codigo_solicitacao: Option<String>,
    /// Your identifier of the charge.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub seu_numero: Option<String>,
    /// Day it was issued (`AAAA-MM-DD`).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub data_emissao: Option<String>,
    /// Due date (`AAAA-MM-DD`).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub data_vencimento: Option<String>,
    /// Face value.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::decimal",
        serialize_with = "decimal_as_number::serialize_option"
    )]
    pub valor_nominal: Option<Decimal>,
    /// Simple, in installments or recurring.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tipo_cobranca: Option<TipoCobranca>,
    /// Where the charge stands: to receive, received, overdue...
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub situacao: Option<SituacaoCobranca>,
    /// Day of the last change of [`situacao`](Self::situacao).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub data_situacao: Option<String>,
    /// Amount received, when paid.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::decimal",
        serialize_with = "decimal_as_number::serialize_option"
    )]
    pub valor_total_recebido: Option<Decimal>,
    /// How it was paid: boleto or Pix.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origem_recebimento: Option<OrigemRecebimento>,
    /// Why it was cancelled.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub motivo_cancelamento: Option<String>,
    /// Whether it is archived.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::bool"
    )]
    pub arquivada: Option<bool>,
    /// Discounts for paying early.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "lenient::vec"
    )]
    pub descontos: Vec<EncargoCobranca>,
    /// Fine for paying late.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multa: Option<EncargoCobranca>,
    /// Interest for paying late.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mora: Option<EncargoCobranca>,
    /// Who pays; the listing brings only the name and the document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pagador: Option<PessoaCobranca>,
}

/// A discount, fine or interest of a charge (`DescontoResponse`,
/// `MultaMoraResponse`): the code and a rate or an amount.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct EncargoCobranca {
    /// `PERCENTUALDATAINFORMADA`, `VALORFIXO`, `TAXAMENSAL`...
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub codigo: Option<String>,
    /// Discounts: days before the due date until which it applies.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::u64"
    )]
    pub quantidade_dias: Option<u64>,
    /// Percentage.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::decimal",
        serialize_with = "decimal_as_number::serialize_option"
    )]
    pub taxa: Option<Decimal>,
    /// Amount.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::decimal",
        serialize_with = "decimal_as_number::serialize_option"
    )]
    pub valor: Option<Decimal>,
}

/// The payer of a charge, as returned by the API (`Pagador`,
/// `pagadorCobranca`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PessoaCobranca {
    /// CPF or CNPJ.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub cpf_cnpj: Option<String>,
    /// `FISICA` or `JURIDICA`.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub tipo_pessoa: Option<String>,
    /// Name.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub nome: Option<String>,
    /// Street.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub endereco: Option<String>,
    /// Number in the street.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub numero: Option<String>,
    /// Complement of the address.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub complemento: Option<String>,
    /// District.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub bairro: Option<String>,
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
    /// E-mail.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub email: Option<String>,
    /// Area code of the phone.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub ddd: Option<String>,
    /// Phone.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub telefone: Option<String>,
}

/// The boleto of a charge (`boleto`, `boletoCobranca`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct BoletoCobranca {
    /// Bank's number of the boleto (*nosso número*).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub nosso_numero: Option<String>,
    /// Barcode, 44 digits.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub codigo_barras: Option<String>,
    /// Typeable line, 47 digits without punctuation.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub linha_digitavel: Option<String>,
}

/// The Pix QR Code of a charge (`pix`, `pixCobranca`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PixCobranca {
    /// Identifier of the Pix charge.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub txid: Option<String>,
    /// The QR Code as text (*copia e cola*).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub pix_copia_e_cola: Option<String>,
}

/// The invoice of a charge, as returned by the API (`NotaFiscalResponse`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct NotaFiscalCobranca {
    /// Access key.
    #[serde(
        rename = "chaveNFe",
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub chave_nfe: Option<String>,
    /// Number.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::u64"
    )]
    pub numero: Option<u64>,
    /// Series.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::u64"
    )]
    pub serie: Option<u64>,
    /// Day it was issued.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub data_emissao: Option<String>,
    /// Installment.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::u64"
    )]
    pub parcela: Option<u64>,
    /// Nature of the operation.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub natureza_operacao: Option<String>,
}

api_enum! {
    /// Where a charge stands (`SituacaoCobrancaEnum`).
    pub enum SituacaoCobranca {
        /// `RECEBIDO`: paid.
        Recebido => "RECEBIDO",
        /// `A_RECEBER`: waiting for the payment.
        AReceber => "A_RECEBER",
        /// `MARCADO_RECEBIDO`: marked as received by hand.
        MarcadoRecebido => "MARCADO_RECEBIDO",
        /// `ATRASADO`: past the due date, unpaid.
        Atrasado => "ATRASADO",
        /// `CANCELADO`: cancelled.
        Cancelado => "CANCELADO",
        /// `EXPIRADO`: cancelled automatically, unpaid.
        Expirado => "EXPIRADO",
        /// `FALHA_EMISSAO`: could not be issued.
        FalhaEmissao => "FALHA_EMISSAO",
        /// `EM_PROCESSAMENTO`: being issued; the boleto and the Pix come later.
        EmProcessamento => "EM_PROCESSAMENTO",
        /// `PROTESTO`: protested.
        Protesto => "PROTESTO",
    }
}

api_enum! {
    /// Kind of charge (`TipoCobrancaEnum`).
    pub enum TipoCobranca {
        /// `SIMPLES`: a single charge.
        Simples => "SIMPLES",
        /// `PARCELADO`: an installment.
        Parcelado => "PARCELADO",
        /// `RECORRENTE`: recurring.
        Recorrente => "RECORRENTE",
    }
}

api_enum! {
    /// How a charge was paid (`OrigemRecebimentoEnum`).
    pub enum OrigemRecebimento {
        /// `BOLETO`: barcode.
        Boleto => "BOLETO",
        /// `PIX`: QR Code.
        Pix => "PIX",
    }
}

string_serde!(SituacaoCobranca, TipoCobranca, OrigemRecebimento);

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn detail_keeps_every_field() {
        let json = json!({
            "cobranca": {
                "codigoSolicitacao": "0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d",
                "seuNumero": "NF-123",
                "dataEmissao": "2026-09-23",
                "dataVencimento": "2026-10-20",
                "valorNominal": 150.5,
                "tipoCobranca": "SIMPLES",
                "situacao": "RECEBIDO",
                "dataSituacao": "2026-10-01",
                "valorTotalRecebido": "150.50",
                "origemRecebimento": "PIX",
                "arquivada": false,
                "descontos": [{"codigo": "PERCENTUALDATAINFORMADA", "quantidadeDias": 5, "taxa": 2}],
                "multa": {"codigo": "VALORFIXO", "valor": 4},
                "mora": {"codigo": "TAXAMENSAL", "taxa": 1},
                "pagador": {"cpfCnpj": "12345678909", "tipoPessoa": "FISICA", "nome": "Cliente Exemplo"}
            },
            "boleto": {"nossoNumero": "12345678", "codigoBarras": "0".repeat(44), "linhaDigitavel": "0".repeat(47)},
            "pix": {"txid": "a".repeat(26), "pixCopiaECola": "000201..."},
            "notaFiscal": {"chaveNFe": "1".repeat(44), "numero": 1, "serie": 1, "dataEmissao": "2026-09-15"}
        });
        let cobranca: CobrancaDetalhada = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(cobranca.cobranca.situacao, Some(SituacaoCobranca::Recebido));
        assert_eq!(
            cobranca.cobranca.valor_total_recebido,
            Some("150.50".parse().unwrap())
        );
        assert_eq!(cobranca.cobranca.descontos[0].quantidade_dias, Some(5));
        // Back as sent, except amounts, which are always numbers.
        let mut esperado = json;
        esperado["cobranca"]["valorTotalRecebido"] = json!(150.5);
        assert_eq!(serde_json::to_value(&cobranca).unwrap(), esperado);
    }

    #[test]
    fn unknown_codes_are_kept() {
        let cobranca: CobrancaDetalhada = serde_json::from_value(json!({
            "cobranca": {"situacao": "NOVA_SITUACAO", "tipoCobranca": "OUTRO"}
        }))
        .unwrap();
        assert_eq!(
            cobranca.cobranca.situacao,
            Some(SituacaoCobranca::Outro("NOVA_SITUACAO".to_owned()))
        );
        assert!(cobranca.boleto.is_none() && cobranca.pix.is_none());
    }
}
