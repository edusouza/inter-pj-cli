//! Output helpers: Brazilian formatting and writing to stdout.

use std::io::{self, Write};

use rust_decimal::{Decimal, RoundingStrategy};
use serde::Serialize;

use crate::error::CliError;

/// Formats a monetary value in Brazilian reais: `R$ 1.234,56` / `-R$ 10,00`.
pub(crate) fn brl(value: Decimal) -> String {
    let rounded = value.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero);
    let sign = if rounded.is_sign_negative() && !rounded.is_zero() {
        "-"
    } else {
        ""
    };
    let digits = format!("{:.2}", rounded.abs());
    let (integer, cents) = digits.split_once('.').unwrap_or((&digits, "00"));
    format!("{sign}R$ {},{cents}", group_thousands(integer))
}

fn group_thousands(integer: &str) -> String {
    let mut out = String::with_capacity(integer.len() + integer.len() / 3);
    for (i, ch) in integer.chars().enumerate() {
        if i > 0 && (integer.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(ch);
    }
    out
}

/// Renders label/value rows with aligned columns (values right-aligned).
pub(crate) fn key_values(rows: &[(&str, String)]) -> String {
    let label_width = rows
        .iter()
        .map(|(label, _)| label.chars().count())
        .max()
        .unwrap_or(0);
    let value_width = rows
        .iter()
        .map(|(_, value)| value.chars().count())
        .max()
        .unwrap_or(0);
    rows.iter()
        .map(|(label, value)| format!("{label:<label_width$}  {value:>value_width$}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Renders label/value rows with values left-aligned (for paths and text).
pub(crate) fn key_values_left(rows: &[(&str, String)]) -> String {
    let label_width = rows
        .iter()
        .map(|(label, _)| label.chars().count())
        .max()
        .unwrap_or(0);
    rows.iter()
        .map(|(label, value)| {
            format!("{label:<label_width$}  {value}")
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Prints `text` and a newline to stdout. A closed pipe is not an error.
pub(crate) fn print(text: &str) -> Result<(), CliError> {
    let mut stdout = io::stdout().lock();
    match writeln!(stdout, "{text}").and_then(|()| stdout.flush()) {
        Err(err) if err.kind() != io::ErrorKind::BrokenPipe => {
            Err(CliError::io("falha ao escrever na saída padrão", err))
        }
        _ => Ok(()),
    }
}

/// Prints `value` as pretty JSON.
pub(crate) fn print_json<T: Serialize>(value: &T) -> Result<(), CliError> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|err| CliError::io("falha ao gerar JSON", io::Error::other(err)))?;
    print(&json)
}

/// Masks all but the last `visible` characters: `*****67`.
pub(crate) fn mask(value: &str, visible: usize) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= visible * 2 {
        return "*".repeat(chars.len().max(4));
    }
    let hidden = chars.len() - visible;
    let tail: String = chars[hidden..].iter().collect();
    format!("{}{tail}", "*".repeat(hidden))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    fn dec(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    #[test]
    fn formats_reais() {
        let cases = [
            ("0", "R$ 0,00"),
            ("0.5", "R$ 0,50"),
            ("2850.55", "R$ 2.850,55"),
            ("1000", "R$ 1.000,00"),
            ("999.999", "R$ 1.000,00"),
            ("1234567.891", "R$ 1.234.567,89"),
            ("-10", "-R$ 10,00"),
            ("-0.004", "R$ 0,00"),
            ("0.005", "R$ 0,01"),
            ("-0.005", "-R$ 0,01"),
            ("100000000", "R$ 100.000.000,00"),
        ];
        for (input, expected) in cases {
            assert_eq!(brl(dec(input)), expected, "{input}");
        }
    }

    #[test]
    fn aligns_columns_with_accents() {
        let rows = [
            ("Saldo disponível", "R$ 2.850,55".to_owned()),
            ("Limite", "R$ 10,00".to_owned()),
        ];
        assert_eq!(
            key_values(&rows),
            "Saldo disponível  R$ 2.850,55\nLimite               R$ 10,00"
        );
    }

    #[test]
    fn left_aligned_rows_trim_trailing_space() {
        let rows = [("Perfil", "padrao".to_owned()), ("Vazio", String::new())];
        assert_eq!(key_values_left(&rows), "Perfil  padrao\nVazio");
    }

    #[test]
    fn masks_sensitive_values() {
        assert_eq!(mask("1234567", 2), "*****67");
        assert_eq!(mask("123", 2), "****");
        assert_eq!(mask("abcd-efgh-ijkl", 4), "**********ijkl");
    }
}
