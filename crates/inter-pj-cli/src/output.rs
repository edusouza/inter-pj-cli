//! Output helpers: Brazilian formatting and writing to stdout.

use std::borrow::Cow;
use std::io::{self, Write};

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, TimeZone};
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

/// Text from the API or from a code on one line, without control
/// characters: names and descriptions come from third parties, and an
/// escape sequence or a line break in them could rewrite or fake what the
/// terminal shows (e.g. a line with another amount).
pub(crate) fn limpo(texto: &str) -> Cow<'_, str> {
    if texto.chars().any(char::is_control) {
        Cow::Owned(
            texto
                .chars()
                .map(|c| if c.is_control() { '\u{FFFD}' } else { c })
                .collect(),
        )
    } else {
        Cow::Borrowed(texto)
    }
}

/// Like [`limpo`], but keeps the line breaks of a text made of lines.
pub(crate) fn sem_controle(texto: &str) -> Cow<'_, str> {
    let controle = |c: char| c.is_control() && c != '\n';
    if texto.chars().any(controle) {
        Cow::Owned(
            texto
                .chars()
                .map(|c| if controle(c) { '\u{FFFD}' } else { c })
                .collect(),
        )
    } else {
        Cow::Borrowed(texto)
    }
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
        .map(|(label, value)| {
            let value = limpo(value);
            format!("{label:<label_width$}  {value:>value_width$}")
        })
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
            format!("{label:<label_width$}  {}", limpo(value))
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Prints `text` and a newline to stdout. A closed pipe is not an error.
///
/// Control characters other than line breaks never reach the terminal,
/// whatever the renderer did with the data.
pub(crate) fn print(text: &str) -> Result<(), CliError> {
    let text = sem_controle(text);
    let mut stdout = io::stdout().lock();
    match writeln!(stdout, "{text}").and_then(|()| stdout.flush()) {
        Err(err) if err.kind() != io::ErrorKind::BrokenPipe => {
            Err(CliError::io("falha ao escrever na saída padrão", err))
        }
        _ => Ok(()),
    }
}

/// Prints `text` as is (e.g. CSV, which carries its own line endings).
pub(crate) fn print_raw(text: &str) -> Result<(), CliError> {
    let mut stdout = io::stdout().lock();
    match stdout
        .write_all(text.as_bytes())
        .and_then(|()| stdout.flush())
    {
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

/// A date as the APIs send it: `2026-10-09`, `2026-10-09 00:00:00` or
/// `09/10/2026`.
pub(crate) fn parse_data(raw: &str) -> Option<NaiveDate> {
    let raw = raw.trim();
    NaiveDate::parse_from_str(raw.get(..10).unwrap_or(raw), "%Y-%m-%d")
        .or_else(|_| NaiveDate::parse_from_str(raw, "%d/%m/%Y"))
        .ok()
}

/// [`parse_data`] as `09/10/2026`; other formats as received.
pub(crate) fn data_br(raw: &str) -> String {
    parse_data(raw).map_or_else(|| raw.to_owned(), |dia| dia.format("%d/%m/%Y").to_string())
}

/// `2026-09-23T12:00:00(.fff)(±hh:mm)` -> `23/09/2026 12:00:00`; other
/// formats as received.
pub(crate) fn data_hora_br(raw: &str) -> String {
    const FORMATO: &str = "%d/%m/%Y %H:%M:%S";
    if let Ok(data) = DateTime::parse_from_rfc3339(raw) {
        return data.format(FORMATO).to_string();
    }
    ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%d %H:%M:%S%.f"]
        .iter()
        .find_map(|formato| NaiveDateTime::parse_from_str(raw, formato).ok())
        .map_or_else(|| raw.to_owned(), |data| data.format(FORMATO).to_string())
}

/// A moment with offset (`2026-09-23T20:15:00.358Z`, as the Pix API sends
/// it) in the local time zone: `23/09/2026 17:15:00`; other formats as
/// [`data_hora_br`] shows them.
pub(crate) fn horario_local(raw: &str) -> String {
    horario_em(raw, &Local)
}

pub(crate) fn horario_em<Tz: TimeZone>(raw: &str, fuso: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    DateTime::parse_from_rfc3339(raw.trim()).map_or_else(
        |_| data_hora_br(raw),
        |momento| {
            momento
                .with_timezone(fuso)
                .format("%d/%m/%Y %H:%M:%S")
                .to_string()
        },
    )
}

/// A title and its lines, indented.
pub(crate) fn secao(titulo: &str, linhas: &[(&str, String)]) -> String {
    let mut texto = titulo.to_owned();
    for linha in key_values_left(linhas).lines() {
        texto.push_str("\n  ");
        texto.push_str(linha);
    }
    texto
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
    fn third_party_text_cannot_rewrite_the_terminal() {
        assert_eq!(limpo("Fornecedor Exemplo"), "Fornecedor Exemplo");
        assert!(matches!(limpo("Fornecedor"), Cow::Borrowed(_)));
        assert_eq!(
            limpo("Loja\u{1b}[2K\rR$ 0,01\nValor"),
            "Loja\u{FFFD}[2K\u{FFFD}R$ 0,01\u{FFFD}Valor"
        );
        assert_eq!(
            sem_controle("linha 1\nlinha\t2\u{7}"),
            "linha 1\nlinha\u{FFFD}2\u{FFFD}"
        );

        // A value with a line break cannot fake another row.
        let rows = [
            ("Recebedor", "Loja\n  Valor  R$ 0,01".to_owned()),
            ("Valor", "R$ 150,00".to_owned()),
        ];
        assert_eq!(key_values_left(&rows).lines().count(), 2);
        assert_eq!(key_values(&rows).lines().count(), 2);
    }

    #[test]
    fn masks_sensitive_values() {
        assert_eq!(mask("1234567", 2), "*****67");
        assert_eq!(mask("123", 2), "****");
        assert_eq!(mask("abcd-efgh-ijkl", 4), "**********ijkl");
    }
}
