use std::fmt::{self, Write as _};
use std::str::FromStr;

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize, Serializer};

use crate::documento::Documento;
use crate::pix::{ChavePix, is_uuid};
use crate::serde_util::{api_enum, decimal_as_number, lenient, string_serde};

/// Longest description of a Pix the API accepts.
pub const MAX_DESCRICAO: usize = 140;

/// A Pix to send with [`Banking::enviar_pix`](super::Banking::enviar_pix)
/// (`PagamentoPixRequestBody`).
///
/// ```
/// use inter_pj::banking::{Destinatario, PagamentoPix};
/// use rust_decimal::Decimal;
///
/// let chave = "fornecedor@exemplo.com".parse().unwrap();
/// let mut pagamento = PagamentoPix::new(Decimal::new(15_000, 2), Destinatario::Chave { chave });
/// pagamento.descricao = Some("NF 123".to_owned());
/// assert!(pagamento.validar().is_ok());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PagamentoPix {
    /// Amount in reais: positive, with at most two decimal places.
    #[serde(serialize_with = "decimal_as_number::serialize")]
    pub valor: Decimal,
    /// Day to pay on; today when absent. A later day schedules the payment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_pagamento: Option<NaiveDate>,
    /// Message to the receiver, at most [`MAX_DESCRICAO`] characters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub descricao: Option<String>,
    /// Who receives the Pix.
    pub destinatario: Destinatario,
}

impl PagamentoPix {
    /// A payment of `valor` to `destinatario`, made today, without a description.
    pub fn new(valor: Decimal, destinatario: Destinatario) -> Self {
        Self {
            valor,
            data_pagamento: None,
            descricao: None,
            destinatario,
        }
    }

    /// Checks what can be checked before sending, as
    /// [`Banking::enviar_pix`](super::Banking::enviar_pix) does.
    ///
    /// # Errors
    ///
    /// Returns the first problem found: amount not positive or with more than
    /// two decimal places, description too long, empty copia e cola code or
    /// malformed bank details.
    pub fn validar(&self) -> Result<(), PagamentoPixError> {
        if self.valor <= Decimal::ZERO {
            return Err(PagamentoPixError::ValorNaoPositivo);
        }
        if self.valor.normalize().scale() > 2 {
            return Err(PagamentoPixError::CasasDecimais);
        }
        if let Some(descricao) = &self.descricao {
            let tamanho = descricao.chars().count();
            if tamanho > MAX_DESCRICAO {
                return Err(PagamentoPixError::DescricaoLonga { tamanho });
            }
        }
        match &self.destinatario {
            Destinatario::DadosBancarios(dados) => dados.validar(),
            Destinatario::PixCopiaECola { pix_copia_e_cola }
                if pix_copia_e_cola.trim().is_empty() =>
            {
                Err(PagamentoPixError::CopiaEColaVazio)
            }
            _ => Ok(()),
        }
    }
}

/// Who receives a Pix. Serialized with the `tipo` the API uses to tell the
/// kinds apart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "tipo", rename_all = "SCREAMING_SNAKE_CASE")]
#[non_exhaustive]
pub enum Destinatario {
    /// `CHAVE`: by Pix key.
    Chave {
        /// Key of the receiver.
        chave: ChavePix,
    },
    /// `DADOS_BANCARIOS`: by bank account, for receivers without a key.
    DadosBancarios(DadosBancarios),
    /// `PIX_COPIA_E_COLA`: by a copia e cola code (see [`crate::pix::BrCode`]).
    PixCopiaECola {
        /// The code, as copied.
        #[serde(rename = "pixCopiaECola")]
        pix_copia_e_cola: String,
    },
}

/// Bank account that receives a Pix sent without a key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DadosBancarios {
    /// Name of the account holder.
    pub nome: String,
    /// CPF or CNPJ of the account holder.
    pub cpf_cnpj: Documento,
    /// Institution of the account.
    pub instituicao_financeira: InstituicaoFinanceira,
    /// Branch, digits only, without check digit.
    pub agencia: String,
    /// Account number with its check digit, digits only (the check digit
    /// may be `X`).
    pub conta_corrente: String,
    /// Kind of account.
    pub tipo_conta: TipoConta,
}

impl DadosBancarios {
    fn validar(&self) -> Result<(), PagamentoPixError> {
        let invalid = |message| Err(PagamentoPixError::DadosBancarios(message));
        let digits = |text: &str| !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit());
        if self.nome.trim().is_empty() {
            return invalid("informe o nome do titular");
        }
        let ispb = &self.instituicao_financeira.ispb;
        if ispb.len() != 8 || !digits(ispb) {
            return invalid("o ISPB da instituição tem 8 dígitos");
        }
        if !digits(&self.agencia) {
            return invalid("a agência deve ter apenas dígitos");
        }
        let conta = self
            .conta_corrente
            .strip_suffix(['X', 'x'])
            .unwrap_or(&self.conta_corrente);
        if !digits(conta) {
            return invalid("a conta deve ter apenas dígitos, incluindo o dígito verificador");
        }
        Ok(())
    }
}

/// Institution of a [`DadosBancarios`] account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InstituicaoFinanceira {
    /// ISPB code of the institution, 8 digits (e.g. `00416968` for Inter).
    pub ispb: String,
}

/// Kind of the receiver's account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TipoConta {
    /// `CONTA_CORRENTE`: checking account.
    ContaCorrente,
    /// `CONTA_POUPANCA`: savings account.
    ContaPoupanca,
    /// `CONTA_SALARIO`: salary account.
    ContaSalario,
    /// `CONTA_PAGAMENTO`: payment account (digital banks and wallets).
    ContaPagamento,
}

impl TipoConta {
    /// Every kind of account the API accepts.
    pub const TODOS: [Self; 4] = [
        Self::ContaCorrente,
        Self::ContaPoupanca,
        Self::ContaSalario,
        Self::ContaPagamento,
    ];

    /// Code used by the API (e.g. `CONTA_CORRENTE`).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ContaCorrente => "CONTA_CORRENTE",
            Self::ContaPoupanca => "CONTA_POUPANCA",
            Self::ContaSalario => "CONTA_SALARIO",
            Self::ContaPagamento => "CONTA_PAGAMENTO",
        }
    }
}

impl fmt::Display for TipoConta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for TipoConta {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// Why a [`PagamentoPix`] cannot be sent.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum PagamentoPixError {
    /// The amount is zero or negative.
    #[error("o valor do Pix deve ser maior que zero")]
    ValorNaoPositivo,
    /// The amount has fractions of a cent.
    #[error("o valor do Pix deve ter no máximo 2 casas decimais")]
    CasasDecimais,
    /// The description is longer than [`MAX_DESCRICAO`].
    #[error("a descrição tem {tamanho} caracteres; o máximo é 140")]
    DescricaoLonga {
        /// Characters in the description.
        tamanho: usize,
    },
    /// The copia e cola code is empty.
    #[error("o código Pix copia e cola está vazio")]
    CopiaEColaVazio,
    /// The bank details are malformed.
    #[error("dados bancários inválidos: {0}")]
    DadosBancarios(&'static str),
}

/// Idempotency key of a Pix payment (`x-id-idempotente` header), a UUID.
///
/// The API does not pay twice for the same key: when the outcome of a
/// payment is unknown (a timeout, a dropped connection), sending it again
/// with the same key is safe.
///
/// ```
/// use inter_pj::banking::IdIdempotente;
///
/// let id = IdIdempotente::novo();
/// let mesmo: IdIdempotente = id.as_str().to_uppercase().parse().unwrap();
/// assert_eq!(id, mesmo);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IdIdempotente(String);

impl IdIdempotente {
    /// A new random key (UUID version 4).
    ///
    /// # Panics
    ///
    /// Never in practice: AWS-LC aborts the process itself when the operating
    /// system cannot provide random bytes.
    pub fn novo() -> Self {
        let mut bytes = [0u8; 16];
        aws_lc_rs::rand::fill(&mut bytes)
            .expect("o sistema não forneceu bytes aleatórios para a chave de idempotência");
        bytes[6] = (bytes[6] & 0x0F) | 0x40; // version 4
        bytes[8] = (bytes[8] & 0x3F) | 0x80; // RFC 4122 variant
        let mut uuid = String::with_capacity(36);
        for (i, byte) in bytes.iter().enumerate() {
            if matches!(i, 4 | 6 | 8 | 10) {
                uuid.push('-');
            }
            let _ = write!(uuid, "{byte:02x}");
        }
        Self(uuid)
    }

    /// The key as sent, in lower case.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for IdIdempotente {
    type Err = IdIdempotenteError;

    /// Accepts a UUID in any case.
    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let raw = raw.trim();
        if is_uuid(raw) {
            Ok(Self(raw.to_ascii_lowercase()))
        } else {
            Err(IdIdempotenteError)
        }
    }
}

impl fmt::Display for IdIdempotente {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The text is not a UUID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("chave de idempotência inválida: use um UUID (8-4-4-4-12 dígitos hexadecimais)")]
#[non_exhaustive]
pub struct IdIdempotenteError;

/// Answer to [`Banking::enviar_pix`](super::Banking::enviar_pix)
/// (`PagamentoPixResponse`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct SolicitacaoPix {
    /// Outcome: made, scheduled or waiting for approval.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tipo_retorno: Option<TipoRetornoPix>,
    /// Identifier of the request, for
    /// [`Banking::consultar_pix`](super::Banking::consultar_pix).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub codigo_solicitacao: Option<String>,
    /// Day the payment is made on, as sent by the API (`AAAA-MM-DD`).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub data_pagamento: Option<String>,
    /// Day the request was received (`AAAA-MM-DD`).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::string"
    )]
    pub data_operacao: Option<String>,
}

api_enum! {
    /// Outcome of a Pix payment request.
    pub enum TipoRetornoPix {
        /// `PROCESSADO`: the Pix was made.
        Processado => "PROCESSADO",
        /// `AGENDADO`: scheduled for [`SolicitacaoPix::data_pagamento`].
        Agendado => "AGENDADO",
        /// `APROVACAO`: waiting for approval in the Internet Banking (Aprovar >
        /// Gestão de Aprovações), as the account settings require.
        Aprovacao => "APROVACAO",
    }
}

string_serde!(TipoRetornoPix);

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn dec(s: &str) -> Decimal {
        s.parse().unwrap()
    }

    fn por_chave(valor: &str) -> PagamentoPix {
        PagamentoPix::new(
            dec(valor),
            Destinatario::Chave {
                chave: "fornecedor@exemplo.com".parse().unwrap(),
            },
        )
    }

    fn dados_bancarios() -> DadosBancarios {
        DadosBancarios {
            nome: "Fornecedor Exemplo".to_owned(),
            cpf_cnpj: "12.345.678/0001-95".parse().unwrap(),
            instituicao_financeira: InstituicaoFinanceira {
                ispb: "00000000".to_owned(),
            },
            agencia: "0001".to_owned(),
            conta_corrente: "1234567".to_owned(),
            tipo_conta: TipoConta::ContaCorrente,
        }
    }

    #[test]
    fn serializes_each_kind_of_receiver_with_its_tipo() {
        let mut pagamento = por_chave("150.00");
        pagamento.descricao = Some("NF 123".to_owned());
        pagamento.data_pagamento = NaiveDate::from_ymd_opt(2026, 10, 1);
        assert_eq!(
            serde_json::to_value(&pagamento).unwrap(),
            json!({
                "valor": 150,
                "dataPagamento": "2026-10-01",
                "descricao": "NF 123",
                "destinatario": {"tipo": "CHAVE", "chave": "fornecedor@exemplo.com"}
            })
        );

        let pagamento =
            PagamentoPix::new(dec("0.99"), Destinatario::DadosBancarios(dados_bancarios()));
        assert_eq!(
            serde_json::to_value(&pagamento).unwrap(),
            json!({
                "valor": 0.99,
                "destinatario": {
                    "tipo": "DADOS_BANCARIOS",
                    "nome": "Fornecedor Exemplo",
                    "cpfCnpj": "12345678000195",
                    "instituicaoFinanceira": {"ispb": "00000000"},
                    "agencia": "0001",
                    "contaCorrente": "1234567",
                    "tipoConta": "CONTA_CORRENTE"
                }
            })
        );

        let pagamento = PagamentoPix::new(
            dec("10.5"),
            Destinatario::PixCopiaECola {
                pix_copia_e_cola: "000201...".to_owned(),
            },
        );
        assert_eq!(
            serde_json::to_value(&pagamento).unwrap()["destinatario"],
            json!({"tipo": "PIX_COPIA_E_COLA", "pixCopiaECola": "000201..."})
        );
    }

    #[test]
    fn validates_amount_and_description() {
        assert!(por_chave("0.01").validar().is_ok());
        assert!(por_chave("1000000.00").validar().is_ok());
        assert!(por_chave("10.500").validar().is_ok());
        for (valor, esperado) in [
            ("0", PagamentoPixError::ValorNaoPositivo),
            ("0.00", PagamentoPixError::ValorNaoPositivo),
            ("-1", PagamentoPixError::ValorNaoPositivo),
            ("0.001", PagamentoPixError::CasasDecimais),
            ("10.555", PagamentoPixError::CasasDecimais),
        ] {
            assert_eq!(por_chave(valor).validar(), Err(esperado), "{valor}");
        }

        let mut pagamento = por_chave("1");
        pagamento.descricao = Some("á".repeat(MAX_DESCRICAO));
        assert!(pagamento.validar().is_ok());
        pagamento.descricao = Some("a".repeat(MAX_DESCRICAO + 1));
        assert_eq!(
            pagamento.validar(),
            Err(PagamentoPixError::DescricaoLonga { tamanho: 141 })
        );

        let vazio = PagamentoPix::new(
            dec("1"),
            Destinatario::PixCopiaECola {
                pix_copia_e_cola: "  ".to_owned(),
            },
        );
        assert_eq!(vazio.validar(), Err(PagamentoPixError::CopiaEColaVazio));
    }

    #[test]
    fn validates_bank_details() {
        let valida = |dados: DadosBancarios| {
            PagamentoPix::new(dec("1"), Destinatario::DadosBancarios(dados)).validar()
        };
        assert!(valida(dados_bancarios()).is_ok());
        let mut conta_x = dados_bancarios();
        conta_x.conta_corrente = "1234567X".to_owned();
        assert!(valida(conta_x).is_ok());

        let casos: [fn(&mut DadosBancarios); 6] = [
            |d| d.nome = " ".to_owned(),
            |d| d.instituicao_financeira.ispb = "1234567".to_owned(),
            |d| d.instituicao_financeira.ispb = "1234567A".to_owned(),
            |d| d.agencia = "00-1".to_owned(),
            |d| d.conta_corrente = String::new(),
            |d| d.conta_corrente = "1234567-8".to_owned(),
        ];
        for (i, caso) in casos.iter().enumerate() {
            let mut dados = dados_bancarios();
            caso(&mut dados);
            assert!(
                matches!(valida(dados), Err(PagamentoPixError::DadosBancarios(_))),
                "caso {i}"
            );
        }
    }

    #[test]
    fn idempotency_keys_are_random_v4_uuids() {
        let a = IdIdempotente::novo();
        let b = IdIdempotente::novo();
        assert_ne!(a, b);
        for id in [&a, &b] {
            let text = id.as_str();
            assert!(is_uuid(text), "{text}");
            assert_eq!(text, text.to_ascii_lowercase());
            assert_eq!(&text[14..15], "4", "{text}");
            assert!("89ab".contains(&text[19..20]), "{text}");
        }
        assert_eq!(
            " 123E4567-E89B-42D3-A456-426614174000 "
                .parse::<IdIdempotente>()
                .unwrap()
                .to_string(),
            "123e4567-e89b-42d3-a456-426614174000"
        );
        assert!("123".parse::<IdIdempotente>().is_err());
    }

    #[test]
    fn parses_the_answer_leniently() {
        let solicitacao: SolicitacaoPix = serde_json::from_value(json!({
            "tipoRetorno": "APROVACAO",
            "codigoSolicitacao": "c42f0787-02cb-4b31-827e-459ec9d7ece1",
            "dataPagamento": "2026-09-23",
            "dataOperacao": "2026-09-23",
            "novoCampo": 1
        }))
        .unwrap();
        assert_eq!(solicitacao.tipo_retorno, Some(TipoRetornoPix::Aprovacao));
        let outro: SolicitacaoPix =
            serde_json::from_value(json!({"tipoRetorno": "EM_ANALISE"})).unwrap();
        assert_eq!(
            outro.tipo_retorno,
            Some(TipoRetornoPix::Outro("EM_ANALISE".to_owned()))
        );
        assert_eq!(outro.codigo_solicitacao, None);
    }
}
