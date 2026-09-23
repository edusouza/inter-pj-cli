use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::extrato::TipoTransacao;
use crate::serde_util::lenient;

/// Details of a [`TransacaoCompleta`](super::TransacaoCompleta), whose shape
/// depends on the kind of transaction.
///
/// Every documented field is text, as in the API (amounts included, e.g.
/// `"13.45"`). Fields the API adds later are kept in `outros`, and details of
/// kinds without a documented shape in [`Detalhe::Outro`], so nothing the API
/// sends is lost when re-serializing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
#[non_exhaustive]
// Boxing the larger variants would save a few hundred bytes per transaction
// at the cost of patterns like `Detalhe::Pix(DetalhePix { tx_id, .. })`.
#[allow(clippy::large_enum_variant)]
pub enum Detalhe {
    /// [`TipoTransacao::Pix`]
    Pix(DetalhePix),
    /// [`TipoTransacao::BoletoCobranca`]
    BoletoCobranca(DetalheBoletoCobranca),
    /// [`TipoTransacao::Cashback`]
    Cashback(DetalheCashback),
    /// [`TipoTransacao::Cheque`]
    Cheque(DetalheCheque),
    /// [`TipoTransacao::CompraDebito`]
    CompraDebito(DetalheCompraDebito),
    /// [`TipoTransacao::DepositoBoleto`]
    DepositoBoleto(DetalheDepositoBoleto),
    /// [`TipoTransacao::Transferencia`]
    Transferencia(DetalheTransferencia),
    /// [`TipoTransacao::Pagamento`]
    Pagamento(DetalhePagamento),
    /// [`TipoTransacao::Tarifa`]
    Tarifa(DetalheTarifa),
    /// Details of any other kind, or that do not fit the documented shape,
    /// as received.
    Outro(Value),
}

impl Detalhe {
    /// Interprets `value` according to the kind of transaction.
    pub(crate) fn from_api(tipo: Option<&TipoTransacao>, value: Value) -> Self {
        fn typed<T: DeserializeOwned>(value: &Value, wrap: fn(T) -> Detalhe) -> Option<Detalhe> {
            T::deserialize(value).ok().map(wrap)
        }
        let typed = match tipo {
            Some(TipoTransacao::Pix) => typed(&value, Self::Pix),
            Some(TipoTransacao::BoletoCobranca) => typed(&value, Self::BoletoCobranca),
            Some(TipoTransacao::Cashback) => typed(&value, Self::Cashback),
            Some(TipoTransacao::Cheque) => typed(&value, Self::Cheque),
            Some(TipoTransacao::CompraDebito) => typed(&value, Self::CompraDebito),
            Some(TipoTransacao::DepositoBoleto) => typed(&value, Self::DepositoBoleto),
            Some(TipoTransacao::Transferencia) => typed(&value, Self::Transferencia),
            Some(TipoTransacao::Pagamento) => typed(&value, Self::Pagamento),
            Some(TipoTransacao::Tarifa) => typed(&value, Self::Tarifa),
            _ => None,
        };
        typed.unwrap_or(Self::Outro(value))
    }
}

macro_rules! detalhe {
    (
        $(#[$meta:meta])*
        $name:ident {
            $( $(#[$field_meta:meta])* $field:ident = $api:literal, )*
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
        #[non_exhaustive]
        pub struct $name {
            $(
                $(#[$field_meta])*
                #[serde(
                    rename = $api,
                    default,
                    skip_serializing_if = "Option::is_none",
                    deserialize_with = "lenient::string"
                )]
                pub $field: Option<String>,
            )*
            /// Fields the API does not document, kept as received.
            #[serde(flatten)]
            pub outros: Map<String, Value>,
        }
    };
}

detalhe! {
    /// Details of a Pix transaction.
    DetalhePix {
        /// Transaction identifier (`txid`) of the charge, when there is one.
        tx_id = "txId",
        /// Name of the payer.
        nome_pagador = "nomePagador",
        /// Message sent with the Pix.
        descricao_pix = "descricaoPix",
        /// CPF or CNPJ of the payer.
        cpf_cnpj_pagador = "cpfCnpjPagador",
        /// Account of the receiver.
        conta_bancaria_recebedor = "contaBancariaRecebedor",
        /// Institution of the payer.
        nome_empresa_pagador = "nomeEmpresaPagador",
        /// Level of detail.
        tipo_detalhe = "tipoDetalhe",
        /// End-to-end identifier of the Pix.
        end_to_end_id = "endToEndId",
        /// Pix key of the receiver.
        chave_pix_recebedor = "chavePixRecebedor",
        /// Institution of the receiver.
        nome_empresa_recebedor = "nomeEmpresaRecebedor",
        /// Name of the receiver.
        nome_recebedor = "nomeRecebedor",
        /// Branch of the receiver.
        agencia_recebedor = "agenciaRecebedor",
        /// CPF or CNPJ of the receiver.
        cpf_cnpj_recebedor = "cpfCnpjRecebedor",
        /// How the Pix was started (key, QR code, manual...).
        origem_movimentacao = "origemMovimentacao",
        /// Request code of an outgoing Pix.
        codigo_solicitacao = "codigoSolicitacao",
    }
}

detalhe! {
    /// Details of a boleto issued by the account and paid by someone.
    DetalheBoletoCobranca {
        /// Due date.
        data_vencimento = "dataVencimento",
        /// Date and time of the payment.
        data_transacao = "dataTransacao",
        /// Bank identifier of the boleto.
        nosso_numero = "nossoNumero",
        /// Identifier given by the account.
        seu_numero = "seuNumero",
        /// Barcode.
        cod_barras = "codBarras",
        /// Interest charged.
        juros = "juros",
        /// Fine charged.
        multa = "multa",
        /// First discount.
        desconto1 = "desconto1",
        /// Second discount.
        desconto2 = "desconto2",
        /// Third discount.
        desconto3 = "desconto3",
        /// Name of the payer.
        nome = "nome",
        /// Last day for payment.
        data_limite = "dataLimite",
        /// Level of detail.
        tipo_detalhe = "tipoDetalhe",
        /// CPF or CNPJ of the payer.
        cpf_cnpj = "cpfCnpj",
        /// Issue date.
        data_emissao = "dataEmissao",
        /// Rebate.
        abatimento = "abatimento",
    }
}

detalhe! {
    /// Details of a cashback credit.
    DetalheCashback {
        /// Amount of the purchase that generated the cashback.
        valor_compra = "valorCompra",
        /// Product purchased.
        produto = "produto",
        /// Level of detail.
        tipo_detalhe = "tipoDetalhe",
    }
}

detalhe! {
    /// Details of a cheque.
    DetalheCheque {
        /// Branch.
        agencia = "agencia",
        /// Cheque number.
        numero_cheque_bancario = "numeroChequeBancario",
        /// Account.
        conta_bancaria = "contaBancaria",
        /// Date the cheque was returned.
        data_retorno = "dataRetorno",
        /// Why the cheque was returned.
        motivo_retorno = "motivoRetorno",
        /// Description.
        descricao_cheque_bancario = "descricaoChequeBancario",
        /// Bank of the cheque.
        nome_empresa = "nomeEmpresa",
        /// Level of detail.
        tipo_detalhe = "tipoDetalhe",
        /// Bank code.
        codigo_afiliado = "codigoAfiliado",
    }
}

detalhe! {
    /// Details of a debit card purchase.
    DetalheCompraDebito {
        /// Merchant.
        estabelecimento = "estabelecimento",
        /// Level of detail.
        tipo_detalhe = "tipoDetalhe",
    }
}

detalhe! {
    /// Details of a deposit made by paying a boleto.
    DetalheDepositoBoleto {
        /// Due date.
        data_vencimento = "dataVencimento",
        /// Level of detail.
        tipo_detalhe = "tipoDetalhe",
        /// Issue date.
        data_emissao = "dataEmissao",
        /// Bank identifier of the boleto.
        nosso_numero = "nossoNumero",
        /// Barcode.
        cod_barras = "codBarras",
    }
}

detalhe! {
    /// Details of a transfer (TED or between accounts).
    DetalheTransferencia {
        /// Account of the payer.
        conta_bancaria_pagador = "contaBancariaPagador",
        /// Description.
        descricao_transferencia = "descricaoTransferencia",
        /// Branch of the payer.
        agencia_pagador = "agenciaPagador",
        /// Bank of the receiver.
        banco_recebedor = "bancoRecebedor",
        /// Account of the receiver.
        conta_bancaria_recebedor = "contaBancariaRecebedor",
        /// CPF or CNPJ of the receiver.
        cpf_cnpj_recebedor = "cpfCnpjRecebedor",
        /// CPF or CNPJ of the payer.
        cpf_cnpj_pagador = "cpfCnpjPagador",
        /// Name of the payer.
        nome_pagador = "nomePagador",
        /// Institution of the payer.
        nome_empresa_pagador = "nomeEmpresaPagador",
        /// Name of the receiver.
        nome_recebedor = "nomeRecebedor",
        /// Level of detail.
        tipo_detalhe = "tipoDetalhe",
        /// Identifier of the transfer.
        id_transferencia = "idTransferencia",
        /// Branch of the receiver.
        agencia_recebedor = "agenciaRecebedor",
        /// Date the transfer was completed (`DD/MM/AAAA`).
        data_efetivacao = "dataEfetivacao",
    }
}

detalhe! {
    /// Details of a bill or tax payment.
    DetalhePagamento {
        /// Total amount paid.
        valor_total = "valorTotal",
        /// Description.
        detalhe_descricao = "detalheDescricao",
        /// Account.
        conta_bancaria = "contaBancaria",
        /// Branch.
        agencia = "agencia",
        /// Amount added.
        adicionado = "adicionado",
        /// Due date.
        data_vencimento = "dataVencimento",
        /// Bank code.
        codigo_afiliado = "codigoAfiliado",
        /// Issuer of the bill.
        empresa_emissora = "empresaEmissora",
        /// Original amount.
        valor_original = "valorOriginal",
        /// Discount.
        desconto = "desconto",
        /// CPF or CNPJ of the beneficiary.
        cpf_cnpj = "cpfCnpj",
        /// Principal amount (taxes).
        valor_principal = "valorPrincipal",
        /// Assessment period (taxes).
        periodo_apuracao = "periodoApuracao",
        /// Amount increased.
        valor_aumentado = "valorAumentado",
        /// Barcode.
        cod_barras = "codBarras",
        /// Partial amount.
        valor_parcial = "valorParcial",
        /// Time of the payment.
        hora = "hora",
        /// Interest.
        juros = "juros",
        /// Fine.
        multa = "multa",
        /// Institution of the payer.
        empresa_origem = "empresaOrigem",
        /// Name of the beneficiary.
        nome_destinatario = "nomeDestinatario",
        /// Level of detail.
        tipo_detalhe = "tipoDetalhe",
        /// Name of the payer.
        nome_origem = "nomeOrigem",
        /// Revenue code (taxes).
        codigo_receita = "codigoReceita",
        /// Typeable line.
        linha_digitavel = "linhaDigitavel",
        /// Bank authentication of the payment.
        autenticacao = "autenticacao",
    }
}

detalhe! {
    /// Details of a bank fee.
    DetalheTarifa {
        /// Due date.
        data_vencimento = "dataVencimento",
        /// Issue date.
        data_emissao = "dataEmissao",
        /// Date of the charge.
        data_transacao = "dataTransacao",
        /// Bank identifier of the related boleto.
        nosso_numero = "nossoNumero",
        /// Identifier given by the account.
        seu_numero = "seuNumero",
        /// Barcode.
        cod_barras = "codBarras",
        /// End-to-end identifier of the related Pix.
        end_to_end_id = "endToEndId",
    }
}
