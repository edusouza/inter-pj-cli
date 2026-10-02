//! Payment codes of boletos and collection documents (FEBRABAN): the typed
//! line (*linha digitável*) and the barcode, validated and decoded locally.

use std::fmt;
use std::str::FromStr;

use chrono::{Days, NaiveDate};
use rust_decimal::Decimal;

/// First day of each cycle of the due-date factor: factor 1000 was
/// 2000-07-03 and reached 9999 on 2025-02-21; from 2025-02-22 it counts
/// from 1000 again.
const BASE_ANTIGA: NaiveDate = match NaiveDate::from_ymd_opt(1997, 10, 7) {
    Some(data) => data,
    None => panic!("data inválida"),
};
const BASE_NOVA: NaiveDate = match NaiveDate::from_ymd_opt(2025, 2, 22) {
    Some(data) => data,
    None => panic!("data inválida"),
};

/// The code of a boleto or of a collection document, from its typed line or
/// its barcode, with every check digit verified.
///
/// | Kind | Typed line | Barcode |
/// | --- | --- | --- |
/// | boleto (bank slip) | 47 digits | 44 digits |
/// | collection (utilities, taxes, fines) | 48 digits, starting with 8 | 44 digits, starting with 8 |
///
/// ```
/// use inter_pj::boleto::CodigoBarras;
///
/// let codigo: CodigoBarras = "07797.77705 11678.471159 90071.126347 1 92950000003010".parse().unwrap();
/// assert_eq!(codigo.codigo_barras(), "07791929500000030107777011678471159007112634");
/// assert_eq!(codigo.valor(), Some("30.10".parse().unwrap()));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CodigoBarras {
    barras: String,
    linha: String,
    tipo: TipoCodigo,
}

/// Kind of payment code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TipoCodigo {
    /// A bank boleto (bloqueto de cobrança).
    Boleto,
    /// A collection document (arrecadação): utilities, taxes, fines...
    Arrecadacao(Segmento),
}

/// Who issues a collection document (second digit of its code).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Segmento {
    /// `1`: city halls (taxes such as IPTU and ISS).
    Prefeitura,
    /// `2`: water and sewage.
    Saneamento,
    /// `3`: electricity and gas.
    EnergiaEGas,
    /// `4`: telecommunications.
    Telecomunicacoes,
    /// `5`: government bodies.
    OrgaoGovernamental,
    /// `6`: payment books and companies identified by CNPJ.
    Carne,
    /// `7`: traffic fines.
    MultaDeTransito,
    /// `9`: exclusive use of the bank.
    UsoDoBanco,
    /// Another digit, kept as found.
    Outro(u8),
}

impl Segmento {
    fn from_digit(digito: u8) -> Self {
        match digito {
            1 => Self::Prefeitura,
            2 => Self::Saneamento,
            3 => Self::EnergiaEGas,
            4 => Self::Telecomunicacoes,
            5 => Self::OrgaoGovernamental,
            6 => Self::Carne,
            7 => Self::MultaDeTransito,
            9 => Self::UsoDoBanco,
            outro => Self::Outro(outro),
        }
    }
}

impl CodigoBarras {
    /// Parses a typed line (47 or 48 digits) or a barcode (44 digits),
    /// ignoring spaces, dots and hyphens.
    ///
    /// # Errors
    ///
    /// Fails when the size or the characters are wrong, or a check digit
    /// does not match (usually a typing mistake).
    pub fn parse(raw: &str) -> Result<Self, CodigoBarrasError> {
        let digitos: String = raw
            .chars()
            .filter(|c| !matches!(c, ' ' | '.' | '-'))
            .collect();
        if !digitos.bytes().all(|b| b.is_ascii_digit()) {
            return Err(CodigoBarrasError::Caracteres);
        }
        let d = digitos.as_bytes();
        match d.len() {
            44 if d[0] == b'8' => arrecadacao_de_barras(&digitos),
            44 => boleto_de_barras(&digitos),
            47 => boleto_de_linha(&digitos),
            48 if d[0] == b'8' => arrecadacao_de_linha(&digitos),
            48 => Err(CodigoBarrasError::Formato(
                "linhas de 48 dígitos (contas e tributos) começam com 8",
            )),
            tamanho => Err(CodigoBarrasError::Tamanho { tamanho }),
        }
    }

    /// The 44-digit barcode.
    pub fn codigo_barras(&self) -> &str {
        &self.barras
    }

    /// The typed line, digits only: 47 digits for boletos, 48 for
    /// collection documents.
    pub fn linha_digitavel(&self) -> &str {
        &self.linha
    }

    /// The typed line grouped as printed on the document:
    /// `07797.77705 11678.471159 90071.126347 1 92950000003010` or
    /// `84660000002-6 26670296201-7 91010013000-4 00062516992-5`.
    pub fn linha_formatada(&self) -> String {
        let l = &self.linha;
        match self.tipo {
            TipoCodigo::Boleto => format!(
                "{}.{} {}.{} {}.{} {} {}",
                &l[..5],
                &l[5..10],
                &l[10..15],
                &l[15..21],
                &l[21..26],
                &l[26..32],
                &l[32..33],
                &l[33..]
            ),
            TipoCodigo::Arrecadacao(_) => (0..4)
                .map(|i| {
                    format!(
                        "{}-{}",
                        &l[i * 12..i * 12 + 11],
                        &l[i * 12 + 11..i * 12 + 12]
                    )
                })
                .collect::<Vec<_>>()
                .join(" "),
        }
    }

    /// Boleto or collection document.
    pub fn tipo(&self) -> TipoCodigo {
        self.tipo
    }

    /// Code of the bank that issued a boleto (`077` for Inter).
    pub fn banco(&self) -> Option<&str> {
        matches!(self.tipo, TipoCodigo::Boleto).then(|| &self.barras[..3])
    }

    /// Amount the code fixes, in reais. `None` when it leaves the amount to
    /// the payer (zeros), or when it does not express reais (a boleto in
    /// another currency, a collection document with a reference value).
    pub fn valor(&self) -> Option<Decimal> {
        let b = self.barras.as_bytes();
        let campo = match self.tipo {
            TipoCodigo::Boleto if b[3] == b'9' => &self.barras[9..19],
            TipoCodigo::Arrecadacao(_) if matches!(b[2], b'6' | b'8') => &self.barras[4..15],
            _ => return None,
        };
        let centavos: i64 = campo.parse().ok()?;
        (centavos > 0).then(|| Decimal::new(centavos, 2))
    }

    /// Due date of a boleto, read from its due-date factor. The factor
    /// restarted on 2025-02-22, so the date is the one of the cycle closest
    /// to `hoje`. `None` for collection documents and boletos without a due
    /// date (factor `0000`).
    pub fn vencimento(&self, hoje: NaiveDate) -> Option<NaiveDate> {
        if !matches!(self.tipo, TipoCodigo::Boleto) {
            return None;
        }
        let fator: u64 = self.barras[5..9].parse().ok()?;
        if fator == 0 {
            return None;
        }
        let antiga = BASE_ANTIGA.checked_add_days(Days::new(fator));
        let nova = fator
            .checked_sub(1000)
            .and_then(|dias| BASE_NOVA.checked_add_days(Days::new(dias)));
        [antiga, nova]
            .into_iter()
            .flatten()
            .min_by_key(|data| (*data - hoje).num_days().abs())
    }
}

impl FromStr for CodigoBarras {
    type Err = CodigoBarrasError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        Self::parse(raw)
    }
}

impl fmt::Display for CodigoBarras {
    /// The formatted typed line.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.linha_formatada())
    }
}

/// Why a text is not a valid boleto or collection code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CodigoBarrasError {
    /// Not 44 (barcode), 47 (boleto) or 48 (collection) digits.
    #[error(
        "código com {tamanho} dígitos: a linha digitável tem 47 (boletos) ou 48 (contas e tributos), e o código de barras, 44"
    )]
    Tamanho {
        /// Digits found, punctuation excluded.
        tamanho: usize,
    },
    /// Something other than digits, spaces, dots and hyphens.
    #[error("o código deve ter só dígitos (espaços, pontos e hífens são ignorados)")]
    Caracteres,
    /// A check digit does not match.
    #[error("dígito verificador {0} não confere: confira a digitação do código")]
    DigitoVerificador(&'static str),
    /// A structural rule of the standard is broken.
    #[error("código inválido: {0}")]
    Formato(&'static str),
}

fn digitos(texto: &str) -> impl DoubleEndedIterator<Item = u32> + '_ {
    texto.bytes().map(|b| u32::from(b - b'0'))
}

fn digito(texto: &str, posicao: usize) -> u32 {
    u32::from(texto.as_bytes()[posicao] - b'0')
}

/// Module 10: weights 2, 1, 2... from the right, adding the digits of each
/// product.
fn modulo10(texto: &str) -> u32 {
    let soma: u32 = digitos(texto)
        .rev()
        .zip([2, 1].into_iter().cycle())
        .map(|(d, peso)| {
            let produto = d * peso;
            produto / 10 + produto % 10
        })
        .sum();
    (10 - soma % 10) % 10
}

/// Sum of module 11: weights 2 to 9 from the right, cycling.
fn soma_modulo11(texto: &str) -> u32 {
    digitos(texto)
        .rev()
        .zip((2..=9).cycle())
        .map(|(d, peso)| d * peso)
        .sum()
}

/// General check digit of a boleto barcode: never 0 (0, 10 and 11 become 1).
fn modulo11_boleto(texto: &str) -> u32 {
    match 11 - soma_modulo11(texto) % 11 {
        0 | 10 | 11 => 1,
        dv => dv,
    }
}

/// Module 11 of collection documents: remainders 0 and 1 give 0.
fn modulo11_arrecadacao(texto: &str) -> u32 {
    match soma_modulo11(texto) % 11 {
        0 | 1 => 0,
        resto => 11 - resto,
    }
}

fn verificar(esperado: u32, encontrado: u32, campo: &'static str) -> Result<(), CodigoBarrasError> {
    if esperado == encontrado {
        Ok(())
    } else {
        Err(CodigoBarrasError::DigitoVerificador(campo))
    }
}

/// Boleto barcode: bank (3), currency (1), general check digit (1), due
/// factor (4), amount (10) and the bank's free field (25).
fn boleto_de_barras(barras: &str) -> Result<CodigoBarras, CodigoBarrasError> {
    let sem_dv = format!("{}{}", &barras[..4], &barras[5..]);
    verificar(modulo11_boleto(&sem_dv), digito(barras, 4), "geral")?;
    let campo = |texto: &str| format!("{texto}{}", modulo10(texto));
    let linha = format!(
        "{}{}{}{}{}",
        campo(&format!("{}{}", &barras[..4], &barras[19..24])),
        campo(&barras[24..34]),
        campo(&barras[34..44]),
        &barras[4..5],
        &barras[5..19]
    );
    Ok(CodigoBarras {
        barras: barras.to_owned(),
        linha,
        tipo: TipoCodigo::Boleto,
    })
}

/// Boleto line: three fields with their own check digits, the general check
/// digit and the due factor with the amount.
fn boleto_de_linha(linha: &str) -> Result<CodigoBarras, CodigoBarrasError> {
    for (inicio, fim, nome) in [
        (0, 9, "do campo 1"),
        (10, 20, "do campo 2"),
        (21, 31, "do campo 3"),
    ] {
        verificar(modulo10(&linha[inicio..fim]), digito(linha, fim), nome)?;
    }
    let barras = format!(
        "{}{}{}{}{}{}",
        &linha[..4],
        &linha[32..33],
        &linha[33..47],
        &linha[4..9],
        &linha[10..20],
        &linha[21..31]
    );
    let codigo = boleto_de_barras(&barras)?;
    debug_assert_eq!(codigo.linha, linha);
    Ok(codigo)
}

/// Collection barcode: product `8`, segment, value kind, general check
/// digit, amount (11) and the issuer's fields.
fn arrecadacao_de_barras(barras: &str) -> Result<CodigoBarras, CodigoBarrasError> {
    let modulo = modulo_arrecadacao(barras)?;
    let sem_dv = format!("{}{}", &barras[..3], &barras[4..]);
    verificar(modulo(&sem_dv), digito(barras, 3), "geral")?;
    let mut linha = String::with_capacity(48);
    for i in 0..4 {
        let bloco = &barras[i * 11..(i + 1) * 11];
        linha.push_str(bloco);
        linha.push(char::from_digit(modulo(bloco), 10).unwrap_or('0'));
    }
    Ok(CodigoBarras {
        barras: barras.to_owned(),
        linha,
        tipo: TipoCodigo::Arrecadacao(Segmento::from_digit(barras.as_bytes()[1] - b'0')),
    })
}

/// Collection line: four blocks of 11 digits, each with its check digit.
fn arrecadacao_de_linha(linha: &str) -> Result<CodigoBarras, CodigoBarrasError> {
    let modulo = modulo_arrecadacao(linha)?;
    let mut barras = String::with_capacity(44);
    for (i, nome) in ["do bloco 1", "do bloco 2", "do bloco 3", "do bloco 4"]
        .into_iter()
        .enumerate()
    {
        let bloco = &linha[i * 12..i * 12 + 11];
        verificar(modulo(bloco), digito(linha, i * 12 + 11), nome)?;
        barras.push_str(bloco);
    }
    let codigo = arrecadacao_de_barras(&barras)?;
    debug_assert_eq!(codigo.linha, linha);
    Ok(codigo)
}

/// The check digit function a collection code uses, from its third digit:
/// 6 and 7 use module 10; 8 and 9, module 11.
fn modulo_arrecadacao(codigo: &str) -> Result<fn(&str) -> u32, CodigoBarrasError> {
    match codigo.as_bytes()[2] {
        b'6' | b'7' => Ok(modulo10),
        b'8' | b'9' => Ok(modulo11_arrecadacao),
        _ => Err(CodigoBarrasError::Formato(
            "o terceiro dígito de contas e tributos deve ser 6, 7, 8 ou 9",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Boleto examples of the API documentation (spec/inter-empresas-openapi.json).
    const BOLETO: &str = "07795904400000500007777011657373795603057629";
    const BOLETO_LINHA: &str = "07797777051165737379856030576294590440000050000";
    const BOLETO_CAIXA: &str = "10491916300000013453395782000100040000022263";
    const LINHA_DA_API: &str = "07797777051167847115990071126347192950000003010";
    // Collection examples of the same documentation.
    const CONTA: &str = "81670000001283647972020063000000000294909999";
    const CONTA_LINHA: &str = "846600000026266702962017910100130004000625169925";

    fn codigo(raw: &str) -> CodigoBarras {
        CodigoBarras::parse(raw).unwrap_or_else(|err| panic!("{raw}: {err}"))
    }

    fn data(ano: i32, mes: u32, dia: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(ano, mes, dia).unwrap()
    }

    #[test]
    fn check_digit_modules() {
        // Module 10 of the first field of a boleto line.
        assert_eq!(modulo10("077977770"), 5);
        assert_eq!(modulo10("0"), 0);
        // Module 11 never gives 0 for boletos.
        assert_eq!(modulo11_boleto("0"), 1);
        assert_eq!(modulo11_arrecadacao("0"), 0);
    }

    #[test]
    fn boleto_barcode_and_line_are_equivalent() {
        let pela_barra = codigo(BOLETO);
        assert_eq!(pela_barra.linha_digitavel(), BOLETO_LINHA);
        let pela_linha = codigo(BOLETO_LINHA);
        assert_eq!(pela_linha, pela_barra);
        assert_eq!(pela_barra.tipo(), TipoCodigo::Boleto);
        assert_eq!(pela_barra.banco(), Some("077"));
        assert_eq!(pela_barra.valor(), Some(Decimal::new(50_000, 2)));
        assert_eq!(
            pela_barra.vencimento(data(2026, 9, 23)),
            Some(data(2022, 7, 12))
        );

        let caixa = codigo(BOLETO_CAIXA);
        assert_eq!(caixa.banco(), Some("104"));
        assert_eq!(caixa.valor(), Some(Decimal::new(1345, 2)));
        assert_eq!(
            caixa.linha_digitavel(),
            "10493395738200010004800000222638191630000001345"
        );

        let da_api = codigo(LINHA_DA_API);
        assert_eq!(
            da_api.codigo_barras(),
            "07791929500000030107777011678471159007112634"
        );
        assert_eq!(
            da_api.linha_formatada(),
            "07797.77705 11678.471159 90071.126347 1 92950000003010"
        );
        assert_eq!(codigo(&da_api.linha_formatada()), da_api);
    }

    #[test]
    fn collection_barcode_and_line_are_equivalent() {
        let conta = codigo(CONTA);
        assert_eq!(conta.tipo(), TipoCodigo::Arrecadacao(Segmento::Prefeitura));
        assert_eq!(conta.valor(), Some(Decimal::new(12_836, 2)));
        assert_eq!(conta.banco(), None);
        assert_eq!(conta.vencimento(data(2026, 9, 23)), None);
        assert_eq!(
            conta.linha_digitavel(),
            "816700000010283647972027006300000004002949099994"
        );

        let pela_linha = codigo(CONTA_LINHA);
        assert_eq!(
            pela_linha.codigo_barras(),
            "84660000002266702962019101001300000062516992"
        );
        assert_eq!(
            pela_linha.tipo(),
            TipoCodigo::Arrecadacao(Segmento::Telecomunicacoes)
        );
        assert_eq!(pela_linha.valor(), Some(Decimal::new(22_667, 2)));
        assert_eq!(
            pela_linha.linha_formatada(),
            "84660000002-6 26670296201-7 91010013000-4 00062516992-5"
        );
        assert_eq!(codigo(&pela_linha.linha_formatada()), pela_linha);
    }

    fn alterar(codigo: &str, posicao: usize) -> String {
        let mut digitos = codigo.as_bytes().to_vec();
        digitos[posicao] = if digitos[posicao] == b'9' {
            b'0'
        } else {
            digitos[posicao] + 1
        };
        String::from_utf8(digitos).unwrap()
    }

    #[test]
    fn every_check_digit_is_verified() {
        // Any single wrong digit is caught (the general check digit here is 5).
        for posicao in 0..BOLETO_LINHA.len() {
            assert!(
                matches!(
                    CodigoBarras::parse(&alterar(BOLETO_LINHA, posicao)),
                    Err(CodigoBarrasError::DigitoVerificador(_))
                ),
                "posição {posicao}"
            );
        }
        for posicao in 0..CONTA_LINHA.len() {
            assert!(
                CodigoBarras::parse(&alterar(CONTA_LINHA, posicao)).is_err(),
                "posição {posicao}"
            );
        }
        assert_eq!(
            CodigoBarras::parse("07795904400000500007777011657373795603057628"),
            Err(CodigoBarrasError::DigitoVerificador("geral"))
        );
        // The example of the payment request in the documentation is not valid.
        assert_eq!(
            CodigoBarras::parse("07797000000000000004501008460019310001802680"),
            Err(CodigoBarrasError::DigitoVerificador("geral"))
        );
    }

    /// A limit of the standard itself: remainders 0, 1 and 10 all give the
    /// general check digit 1, so a boleto whose check digit is 1 may not
    /// reveal a wrong digit in the due date or amount. That is why the CLI
    /// shows both, decoded, before paying.
    #[test]
    fn general_check_digit_one_is_a_known_blind_spot() {
        assert_eq!(&LINHA_DA_API[32..33], "1");
        assert!(CodigoBarras::parse(&alterar(LINHA_DA_API, 35)).is_ok());
    }

    #[test]
    fn rejects_wrong_sizes_characters_and_structure() {
        assert_eq!(
            CodigoBarras::parse("0779"),
            Err(CodigoBarrasError::Tamanho { tamanho: 4 })
        );
        assert_eq!(
            CodigoBarras::parse(&format!("{BOLETO}x")),
            Err(CodigoBarrasError::Caracteres)
        );
        assert!(matches!(
            CodigoBarras::parse(&format!("0{}", &CONTA_LINHA[1..])),
            Err(CodigoBarrasError::Formato(_))
        ));
        assert!(matches!(
            CodigoBarras::parse(&format!("872{}", &CONTA[3..])),
            Err(CodigoBarrasError::Formato(_))
        ));
    }

    #[test]
    fn amounts_left_to_the_payer() {
        // A boleto with a zero amount (the payer defines it) and no due date.
        let base = "0779000000000000000777701165737379560305762";
        let dv = modulo11_boleto(base);
        let barras = format!("{}{dv}{}", &base[..4], &base[4..]);
        let boleto = codigo(&barras);
        assert_eq!(boleto.valor(), None);
        assert_eq!(boleto.vencimento(data(2026, 9, 23)), None);
    }

    #[test]
    fn due_factor_restarted_in_2025() {
        let com_fator = |fator: &str| {
            let base = format!("0779{fator}00000500007777011657373795603057629");
            let dv = modulo11_boleto(&base);
            codigo(&format!("{}{dv}{}", &base[..4], &base[4..]))
        };
        let hoje = data(2026, 9, 23);
        assert_eq!(com_fator("1000").vencimento(hoje), Some(data(2025, 2, 22)));
        assert_eq!(com_fator("1234").vencimento(hoje), Some(data(2025, 10, 14)));
        assert_eq!(com_fator("9999").vencimento(hoje), Some(data(2025, 2, 21)));
        // Before the restart, factor 1000 was 2000-07-03.
        assert_eq!(
            com_fator("1000").vencimento(data(2000, 7, 1)),
            Some(data(2000, 7, 3))
        );
    }
}
