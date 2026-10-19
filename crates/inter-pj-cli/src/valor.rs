//! Amounts in reais typed by people (`150,00`, `1.500,00`, `150.00`) and
//! written in words, as on a cheque.

use std::str::FromStr;

use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;

const FORMATO: &str = "use, por exemplo, 150,00 ou 1.500,00";

/// Parses a positive amount with at most two decimal places, written in
/// Brazilian (`1.500,00`) or international (`1,500.00`) notation, with an
/// optional `R$`.
///
/// A single separator followed by three digits (`1.500`, `1,500`) is
/// refused: it could be thousands or decimals, and guessing wrong moves a
/// thousand times the intended amount.
pub(crate) fn parse_valor(raw: &str) -> Result<Decimal, String> {
    let shown = raw.trim();
    let text = shown.strip_prefix("R$").map_or(shown, str::trim_start);
    let error = |detalhe: &str| format!("valor inválido \"{shown}\": {detalhe}");
    if text.is_empty()
        || !text
            .bytes()
            .all(|b| b.is_ascii_digit() || b == b'.' || b == b',')
    {
        return Err(error(FORMATO));
    }
    let (inteiro, fracao) = split(text).map_err(error)?;
    let numero = if fracao.is_empty() {
        inteiro
    } else {
        format!("{inteiro}.{fracao}")
    };
    let valor = Decimal::from_str(&numero).map_err(|_| error("valor alto demais"))?;
    if valor.is_zero() {
        return Err(error("o valor deve ser maior que zero"));
    }
    Ok(valor)
}

/// Like [`parse_valor`], but also accepts zero (`0`, `0,00`): fines and
/// interest may be zero.
pub(crate) fn parse_valor_ou_zero(raw: &str) -> Result<Decimal, String> {
    let shown = raw.trim();
    let text = shown.strip_prefix("R$").map_or(shown, str::trim_start);
    if text.starts_with('0') && text.bytes().all(|b| matches!(b, b'0' | b'.' | b',')) {
        return Ok(Decimal::ZERO);
    }
    parse_valor(raw)
}

/// Integer digits (thousands separators removed) and decimal digits.
fn split(text: &str) -> Result<(String, &str), &'static str> {
    let pontos = text.matches('.').count();
    let virgulas = text.matches(',').count();
    let (inteiro, fracao) = match (pontos, virgulas) {
        (1, 0) | (0, 1) => {
            let (inteiro, fracao) = text.split_once(['.', ',']).unwrap_or((text, ""));
            match fracao.len() {
                1 | 2 => (inteiro, fracao),
                3 => return Err("ambíguo; use 1500, 1500,00 ou 1.500,00"),
                0 => return Err(FORMATO),
                _ => return Err("use no máximo 2 casas decimais"),
            }
        }
        // No separator, or the same one repeated: thousands only (1.500.000).
        (_, 0) | (0, _) => (text, ""),
        // Both: the last one separates the decimals.
        _ => {
            let decimal = if text.rfind('.') > text.rfind(',') {
                '.'
            } else {
                ','
            };
            let (inteiro, fracao) = text.rsplit_once(decimal).unwrap_or((text, ""));
            if inteiro.contains(decimal) || !(1..=2).contains(&fracao.len()) {
                return Err(FORMATO);
            }
            (inteiro, fracao)
        }
    };
    let grupos: Vec<&str> = inteiro.split(['.', ',']).collect();
    let digitos = grupos
        .iter()
        .all(|g| !g.is_empty() && g.bytes().all(|b| b.is_ascii_digit()));
    let milhares =
        grupos.len() == 1 || (grupos[0].len() <= 3 && grupos[1..].iter().all(|g| g.len() == 3));
    if !digitos || !milhares {
        return Err(FORMATO);
    }
    Ok((grupos.concat(), fracao))
}

/// The amount in words: `mil e quinhentos reais e dez centavos`. `None`
/// for a quadrillion or more.
pub(crate) fn por_extenso(valor: Decimal) -> Option<String> {
    let valor = valor.abs().round_dp(2);
    let reais = valor.trunc().to_u64()?;
    if reais >= 1_000_000_000_000_000 {
        return None;
    }
    let centavos = (valor.fract() * Decimal::ONE_HUNDRED).to_u64()?;
    let mut partes = Vec::new();
    if reais > 0 {
        let unidade = if reais == 1 { "real" } else { "reais" };
        // "um milhão de reais", but "um milhão e dez mil reais".
        let de = if reais % 1_000_000 == 0 { "de " } else { "" };
        partes.push(format!("{} {de}{unidade}", numero(reais)));
    }
    if centavos > 0 {
        let unidade = if centavos == 1 { "centavo" } else { "centavos" };
        partes.push(format!("{} {unidade}", numero(centavos)));
    }
    if partes.is_empty() {
        partes.push("zero real".to_owned());
    }
    Some(partes.join(" e "))
}

/// A whole number below a quadrillion in words (masculine).
fn numero(n: u64) -> String {
    const ESCALAS: [(&str, &str); 5] = [
        ("", ""),
        ("mil", "mil"),
        ("milhão", "milhões"),
        ("bilhão", "bilhões"),
        ("trilhão", "trilhões"),
    ];
    if n == 0 {
        return "zero".to_owned();
    }
    // Non-zero groups of three digits, from the highest.
    let mut grupos = Vec::new();
    let mut resto = n;
    let mut escala = 0;
    while resto > 0 {
        #[allow(clippy::cast_possible_truncation)] // below 1000
        let grupo = (resto % 1000) as usize;
        if grupo > 0 {
            grupos.push((escala, grupo));
        }
        resto /= 1000;
        escala += 1;
    }
    grupos.reverse();

    let ultimo = grupos.last().map_or(0, |&(_, grupo)| grupo);
    let mut texto = String::new();
    for (i, &(escala, grupo)) in grupos.iter().enumerate() {
        if i > 0 {
            // "mil e quinhentos", "mil e vinte", but "mil duzentos e trinta".
            let e = i == grupos.len() - 1 && (ultimo < 100 || ultimo % 100 == 0);
            texto.push_str(if e { " e " } else { " " });
        }
        let (singular, plural) = ESCALAS[escala];
        match (escala, grupo) {
            (0, _) => texto.push_str(&centena(grupo)),
            (1, 1) => texto.push_str("mil"),
            _ => {
                texto.push_str(&centena(grupo));
                texto.push(' ');
                texto.push_str(if grupo == 1 { singular } else { plural });
            }
        }
    }
    texto
}

/// 1 to 999 in words.
fn centena(n: usize) -> String {
    const UNIDADES: [&str; 20] = [
        "zero",
        "um",
        "dois",
        "três",
        "quatro",
        "cinco",
        "seis",
        "sete",
        "oito",
        "nove",
        "dez",
        "onze",
        "doze",
        "treze",
        "quatorze",
        "quinze",
        "dezesseis",
        "dezessete",
        "dezoito",
        "dezenove",
    ];
    const DEZENAS: [&str; 10] = [
        "",
        "",
        "vinte",
        "trinta",
        "quarenta",
        "cinquenta",
        "sessenta",
        "setenta",
        "oitenta",
        "noventa",
    ];
    const CENTENAS: [&str; 10] = [
        "",
        "cento",
        "duzentos",
        "trezentos",
        "quatrocentos",
        "quinhentos",
        "seiscentos",
        "setecentos",
        "oitocentos",
        "novecentos",
    ];
    if n == 100 {
        return "cem".to_owned();
    }
    let mut partes = Vec::new();
    if n >= 100 {
        partes.push(CENTENAS[n / 100]);
    }
    match n % 100 {
        0 => {}
        resto @ 1..20 => partes.push(UNIDADES[resto]),
        resto => {
            partes.push(DEZENAS[resto / 10]);
            if resto % 10 > 0 {
                partes.push(UNIDADES[resto % 10]);
            }
        }
    }
    partes.join(" e ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dec(s: &str) -> Decimal {
        s.parse().unwrap()
    }

    #[test]
    fn fines_and_interest_may_be_zero() {
        for zero in ["0", "0,00", "R$ 0.00", " 0,0 "] {
            assert_eq!(parse_valor_ou_zero(zero), Ok(Decimal::ZERO), "{zero}");
        }
        assert_eq!(parse_valor_ou_zero("27,48"), Ok(dec("27.48")));
        for invalido in ["", ",", "-1", "1.500", "0,001"] {
            assert!(parse_valor_ou_zero(invalido).is_err(), "{invalido}");
        }
        assert!(parse_valor("0").is_err());
    }

    #[test]
    fn parses_brazilian_and_international_notation() {
        for (entrada, esperado) in [
            ("150", "150"),
            ("150,5", "150.5"),
            ("150,00", "150.00"),
            ("150.00", "150.00"),
            ("0,01", "0.01"),
            ("1.500,00", "1500.00"),
            ("1,500.00", "1500.00"),
            ("1.500.000", "1500000"),
            ("1,500,000", "1500000"),
            ("12.345.678,9", "12345678.9"),
            ("R$ 1.500,00", "1500.00"),
            ("R$1,99", "1.99"),
            (" 7 ", "7"),
        ] {
            assert_eq!(parse_valor(entrada), Ok(dec(esperado)), "{entrada}");
        }
    }

    #[test]
    fn refuses_ambiguous_and_malformed_amounts() {
        for entrada in [
            "1.500",
            "1,500",
            "0,500",
            "",
            "R$",
            "abc",
            "-10",
            "10-",
            "1 500",
            "150,",
            ",50",
            "1,5.00",
            "1.2.3,00",
            "12.34.56",
            "1.500.00,5",
            "1,50,00",
            "1.5000",
            "150,001",
            "1e3",
            "0",
            "0,00",
            "R$ 0",
        ] {
            assert!(parse_valor(entrada).is_err(), "{entrada}");
        }
        let err = parse_valor("1.500").unwrap_err();
        assert!(err.contains("ambíguo") && err.contains("1.500,00"), "{err}");
        let err = parse_valor("10,555").unwrap_err();
        assert!(err.contains("ambíguo"), "{err}");
        let err = parse_valor("0").unwrap_err();
        assert!(err.contains("maior que zero"), "{err}");
    }

    #[test]
    fn writes_amounts_in_words() {
        for (valor, esperado) in [
            ("0.01", "um centavo"),
            ("0.5", "cinquenta centavos"),
            ("1", "um real"),
            ("1.01", "um real e um centavo"),
            ("2", "dois reais"),
            ("15", "quinze reais"),
            ("21", "vinte e um reais"),
            ("100", "cem reais"),
            ("101", "cento e um reais"),
            ("150", "cento e cinquenta reais"),
            (
                "999.99",
                "novecentos e noventa e nove reais e noventa e nove centavos",
            ),
            ("1000", "mil reais"),
            ("1001", "mil e um reais"),
            ("1100", "mil e cem reais"),
            ("1234", "mil duzentos e trinta e quatro reais"),
            ("1500", "mil e quinhentos reais"),
            ("2020", "dois mil e vinte reais"),
            ("21000", "vinte e um mil reais"),
            ("100000", "cem mil reais"),
            ("1000000", "um milhão de reais"),
            ("1000001", "um milhão e um reais"),
            ("1100000", "um milhão e cem mil reais"),
            ("1500000", "um milhão e quinhentos mil reais"),
            ("1200300", "um milhão duzentos mil e trezentos reais"),
            ("2000000", "dois milhões de reais"),
            (
                "1234567.89",
                "um milhão duzentos e trinta e quatro mil quinhentos e sessenta e sete reais e oitenta e nove centavos",
            ),
            ("3000000000", "três bilhões de reais"),
            ("0", "zero real"),
        ] {
            assert_eq!(
                por_extenso(dec(valor)).as_deref(),
                Some(esperado),
                "{valor}"
            );
        }
        assert_eq!(por_extenso(dec("1000000000000000")), None);
    }
}
