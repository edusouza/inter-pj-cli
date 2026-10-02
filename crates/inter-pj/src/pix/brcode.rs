use std::str::FromStr;

use rust_decimal::Decimal;

/// GUI identifying the Pix account template of a BR Code.
const PIX_GUI: &str = "br.gov.bcb.pix";

/// What a Pix "copia e cola" code (BR Code, EMV QRCPS-MPM) says, decoded
/// locally so it can be shown before paying.
///
/// Dynamic codes point to a URL where the receiver's institution keeps the
/// charge; their final amount is defined there, not in the code.
///
/// ```
/// use inter_pj::pix::BrCode;
///
/// // Example from the Banco Central manual (a static code).
/// let codigo = "00020126580014br.gov.bcb.pix0136123e4567-e12b-12d1-a456-426655440000\
///               5204000053039865802BR5913Fulano de Tal6008BRASILIA62070503***63041D3D";
/// let brcode: BrCode = codigo.parse().unwrap();
/// assert_eq!(brcode.nome_recebedor, "Fulano de Tal");
/// assert_eq!(brcode.chave.as_deref(), Some("123e4567-e12b-12d1-a456-426655440000"));
/// assert_eq!(brcode.valor, None);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct BrCode {
    /// Pix key of the receiver (static codes).
    pub chave: Option<String>,
    /// Location of the charge (dynamic codes), without the scheme.
    pub url: Option<String>,
    /// Message to the payer (static codes).
    pub info_adicional: Option<String>,
    /// Amount, when the code fixes one.
    pub valor: Option<Decimal>,
    /// Name of the receiver.
    pub nome_recebedor: String,
    /// City of the receiver.
    pub cidade: String,
    /// Transaction identifier (`***` when the receiver defines none).
    pub txid: Option<String>,
}

impl BrCode {
    /// Decodes and validates a BR Code, including its CRC.
    ///
    /// # Errors
    ///
    /// Fails when the text is not a well-formed Pix BR Code or its CRC does
    /// not match (usually a code copied incompletely).
    pub fn parse(payload: &str) -> Result<Self, BrCodeError> {
        let payload = payload.trim();
        let fields = parse_tlv(payload)?;
        let crc = fields
            .last()
            .filter(|(id, _)| *id == "63")
            .map(|(_, value)| value.as_str())
            .ok_or(BrCodeError::CampoAusente("CRC (63)"))?;
        if crc.len() != 4 {
            return Err(BrCodeError::Formato("CRC (63) sem 4 caracteres"));
        }
        let signed = &payload[..payload.len() - crc.len()];
        let calculado = format!("{:04X}", crc16(signed.as_bytes()));
        if !crc.eq_ignore_ascii_case(&calculado) {
            return Err(BrCodeError::Crc {
                informado: crc.to_owned(),
                calculado,
            });
        }

        let get = |id: &str| fields.iter().find(|(key, _)| key == id).map(|(_, v)| v);
        if get("00").map(String::as_str) != Some("01") {
            return Err(BrCodeError::Formato(
                "indicador de formato (00) diferente de 01",
            ));
        }
        let conta = fields
            .iter()
            .filter(|(id, _)| ("26".."52").contains(&id.as_str()))
            .map(|(_, value)| parse_tlv(value))
            .find_map(|template| {
                template.ok().filter(|sub| {
                    sub.iter()
                        .any(|(id, v)| id == "00" && v.eq_ignore_ascii_case(PIX_GUI))
                })
            })
            .ok_or(BrCodeError::NaoPix)?;
        let sub = |id: &str| {
            conta
                .iter()
                .find(|(key, _)| key == id)
                .map(|(_, v)| v.clone())
        };
        let chave = sub("01");
        let url = sub("25");
        if chave.is_none() && url.is_none() {
            return Err(BrCodeError::CampoAusente("chave (26.01) ou URL (26.25)"));
        }
        if get("53").map(String::as_str) != Some("986") {
            return Err(BrCodeError::Formato("moeda (53) diferente de real (986)"));
        }
        let valor = get("54")
            .map(|raw| {
                Decimal::from_str(raw)
                    .ok()
                    .filter(|v| v.is_sign_positive() && !v.is_zero())
                    .ok_or(BrCodeError::Formato("valor (54) inválido"))
            })
            .transpose()?;
        let nome_recebedor = get("59")
            .cloned()
            .ok_or(BrCodeError::CampoAusente("nome do recebedor (59)"))?;
        let cidade = get("60")
            .cloned()
            .ok_or(BrCodeError::CampoAusente("cidade (60)"))?;
        let txid = get("62")
            .and_then(|value| parse_tlv(value).ok())
            .and_then(|additional| {
                additional
                    .into_iter()
                    .find(|(id, _)| id == "05")
                    .map(|(_, v)| v)
            });

        Ok(Self {
            chave,
            url,
            info_adicional: sub("02"),
            valor,
            nome_recebedor,
            cidade,
            txid,
        })
    }

    /// Whether the code is dynamic: the charge lives at [`url`](Self::url)
    /// and its final amount is defined there.
    pub fn dinamico(&self) -> bool {
        self.url.is_some()
    }
}

impl FromStr for BrCode {
    type Err = BrCodeError;

    fn from_str(payload: &str) -> Result<Self, Self::Err> {
        Self::parse(payload)
    }
}

/// Why a text is not a valid Pix BR Code.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum BrCodeError {
    /// The structure (identifier, size, value) is broken.
    #[error("código Pix copia e cola malformado: {0}")]
    Formato(&'static str),
    /// A mandatory field is missing.
    #[error("código Pix copia e cola sem {0}")]
    CampoAusente(&'static str),
    /// The code has no Pix account template.
    #[error("o código não é um Pix copia e cola (falta o identificador br.gov.bcb.pix)")]
    NaoPix,
    /// The checksum does not match: the code was probably copied incompletely.
    #[error(
        "código Pix copia e cola corrompido (CRC {informado}, esperado {calculado}): copie o código novamente"
    )]
    Crc {
        /// CRC found in the code.
        informado: String,
        /// CRC of the content.
        calculado: String,
    },
}

/// Splits `IDLLvalue...` fields. Sizes count characters.
fn parse_tlv(data: &str) -> Result<Vec<(String, String)>, BrCodeError> {
    let chars: Vec<char> = data.chars().collect();
    let mut fields = Vec::new();
    let mut position = 0;
    while position < chars.len() {
        let header: String = chars
            .get(position..position + 4)
            .ok_or(BrCodeError::Formato("campo incompleto"))?
            .iter()
            .collect();
        if !header.bytes().all(|b| b.is_ascii_digit()) {
            return Err(BrCodeError::Formato(
                "identificador ou tamanho não numérico",
            ));
        }
        let size: usize = header[2..].parse().unwrap_or_default();
        let start = position + 4;
        let value: String = chars
            .get(start..start + size)
            .ok_or(BrCodeError::Formato("valor menor que o tamanho declarado"))?
            .iter()
            .collect();
        fields.push((header[..2].to_owned(), value));
        position = start + size;
    }
    Ok(fields)
}

/// CRC-16/CCITT-FALSE (polynomial `0x1021`, initial value `0xFFFF`), as the
/// BR Code specification requires.
pub fn crc16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &byte in data {
        crc ^= u16::from(byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 == 0 {
                crc << 1
            } else {
                (crc << 1) ^ 0x1021
            };
        }
    }
    crc
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use super::*;

    const MANUAL: &str = "00020126580014br.gov.bcb.pix0136123e4567-e12b-12d1-a456-4266554400005204000053039865802BR5913Fulano de Tal6008BRASILIA62070503***63041D3D";

    /// `IDLLvalue` fields, back to back.
    fn tlv<V: AsRef<str>>(fields: &[(&str, V)]) -> String {
        let mut data = String::new();
        for (id, value) in fields {
            let value = value.as_ref();
            let _ = write!(data, "{id}{:02}{value}", value.chars().count());
        }
        data
    }

    /// Builds a BR Code from `(id, value)` fields, appending the CRC.
    fn encode(fields: &[(&str, String)]) -> String {
        let mut payload = tlv(fields);
        payload.push_str("6304");
        let crc = crc16(payload.as_bytes());
        let _ = write!(payload, "{crc:04X}");
        payload
    }

    fn estatico(valor: Option<&str>) -> String {
        let mut fields = vec![
            ("00", "01".to_owned()),
            (
                "26",
                tlv(&[
                    ("00", "BR.GOV.BCB.PIX"),
                    ("01", "fornecedor@exemplo.com"),
                    ("02", "NF 123"),
                ]),
            ),
            ("52", "0000".to_owned()),
            ("53", "986".to_owned()),
        ];
        if let Some(valor) = valor {
            fields.push(("54", valor.to_owned()));
        }
        fields.extend([
            ("58", "BR".to_owned()),
            ("59", "Fornecedor Exemplo".to_owned()),
            ("60", "SAO PAULO".to_owned()),
            ("62", tlv(&[("05", "NF123")])),
        ]);
        encode(&fields)
    }

    #[test]
    fn crc16_matches_the_standard_check_value() {
        assert_eq!(crc16(b"123456789"), 0x29B1);
    }

    #[test]
    fn decodes_the_manual_example() {
        let brcode = BrCode::parse(MANUAL).unwrap();
        assert_eq!(
            brcode.chave.as_deref(),
            Some("123e4567-e12b-12d1-a456-426655440000")
        );
        assert_eq!(brcode.nome_recebedor, "Fulano de Tal");
        assert_eq!(brcode.cidade, "BRASILIA");
        assert_eq!(brcode.txid.as_deref(), Some("***"));
        assert!(!brcode.dinamico());
        assert_eq!(brcode.valor, None);
    }

    #[test]
    fn decodes_amount_message_and_txid_of_static_codes() {
        let brcode = BrCode::parse(&estatico(Some("150.00"))).unwrap();
        assert_eq!(brcode.valor, Some("150.00".parse().unwrap()));
        assert_eq!(brcode.chave.as_deref(), Some("fornecedor@exemplo.com"));
        assert_eq!(brcode.info_adicional.as_deref(), Some("NF 123"));
        assert_eq!(brcode.txid.as_deref(), Some("NF123"));
    }

    #[test]
    fn recognizes_dynamic_codes() {
        let payload = encode(&[
            ("00", "01".to_owned()),
            ("01", "12".to_owned()),
            (
                "26",
                tlv(&[
                    ("00", "br.gov.bcb.pix"),
                    ("25", "qr.exemplo.invalid/v2/cobv/abc"),
                ]),
            ),
            ("52", "0000".to_owned()),
            ("53", "986".to_owned()),
            ("58", "BR".to_owned()),
            ("59", "Loja Exemplo".to_owned()),
            ("60", "CURITIBA".to_owned()),
            ("62", tlv(&[("05", "***")])),
        ]);
        let brcode = BrCode::parse(&payload).unwrap();
        assert!(brcode.dinamico());
        assert_eq!(
            brcode.url.as_deref(),
            Some("qr.exemplo.invalid/v2/cobv/abc")
        );
        assert_eq!(brcode.chave, None);
    }

    #[test]
    fn rejects_corrupted_or_truncated_codes() {
        let mut corrompido = estatico(Some("150.00"));
        corrompido = corrompido.replacen("150.00", "950.00", 1);
        assert!(matches!(
            BrCode::parse(&corrompido),
            Err(BrCodeError::Crc { .. })
        ));
        let truncado = &MANUAL[..MANUAL.len() - 10];
        assert!(BrCode::parse(truncado).is_err());
        assert!(matches!(
            BrCode::parse("não é um código"),
            Err(BrCodeError::Formato(_))
        ));
        // A lower-case CRC is still accepted.
        let minusculo = format!("{}{}", &MANUAL[..MANUAL.len() - 4], "1d3d");
        assert!(BrCode::parse(&minusculo).is_ok());
        let crc_curto = format!("{}6303ABC", &MANUAL[..MANUAL.len() - 8]);
        assert!(matches!(
            BrCode::parse(&crc_curto),
            Err(BrCodeError::Formato(_))
        ));
    }

    #[test]
    fn sizes_count_characters_not_bytes() {
        let payload = encode(&[
            ("00", "01".to_owned()),
            (
                "26",
                tlv(&[("00", "br.gov.bcb.pix"), ("01", "12345678909")]),
            ),
            ("52", "0000".to_owned()),
            ("53", "986".to_owned()),
            ("58", "BR".to_owned()),
            ("59", "Padaria São João".to_owned()),
            ("60", "SÃO PAULO".to_owned()),
        ]);
        let brcode = BrCode::parse(&payload).unwrap();
        assert_eq!(brcode.nome_recebedor, "Padaria São João");
        assert_eq!(brcode.cidade, "SÃO PAULO");
        assert_eq!(brcode.txid, None);
    }

    #[test]
    fn rejects_codes_that_are_not_pix() {
        let payload = encode(&[
            ("00", "01".to_owned()),
            ("26", tlv(&[("00", "com.exemplo.carteira"), ("01", "x")])),
            ("52", "0000".to_owned()),
            ("53", "986".to_owned()),
            ("58", "BR".to_owned()),
            ("59", "Loja".to_owned()),
            ("60", "RIO".to_owned()),
        ]);
        assert_eq!(BrCode::parse(&payload), Err(BrCodeError::NaoPix));
    }

    #[test]
    fn rejects_missing_fields_and_invalid_values() {
        let sem_nome = encode(&[
            ("00", "01".to_owned()),
            (
                "26",
                tlv(&[("00", "br.gov.bcb.pix"), ("01", "12345678909")]),
            ),
            ("53", "986".to_owned()),
            ("60", "RIO".to_owned()),
        ]);
        assert!(matches!(
            BrCode::parse(&sem_nome),
            Err(BrCodeError::CampoAusente(_))
        ));
        assert!(matches!(
            BrCode::parse(&estatico(Some("-5.00"))),
            Err(BrCodeError::Formato(_))
        ));
        assert!(matches!(
            BrCode::parse(&estatico(Some("0.00"))),
            Err(BrCodeError::Formato(_))
        ));
    }
}
