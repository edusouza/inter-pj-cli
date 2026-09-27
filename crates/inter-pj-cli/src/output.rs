//! Output helpers: Brazilian formatting and writing to stdout.

use std::borrow::Cow;
use std::fmt::Write as _;
use std::io::{self, IsTerminal, Write};

use anstream::AutoStream;
use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, TimeZone};
use rust_decimal::{Decimal, RoundingStrategy};
use serde::Serialize;

use crate::cores;
use crate::error::CliError;
use crate::tabela::{Separador, Tabela};

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

/// A percentage the Brazilian way: `2,5%`.
pub(crate) fn percentual(taxa: Decimal) -> String {
    format!("{}%", taxa.normalize()).replace('.', ",")
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

/// Whether `c` must not reach the terminal from the data: the control
/// characters (escape sequences, line breaks, the C1 controls) and the
/// format characters that reorder or hide text. These are:
/// - the bidirectional marks, embeddings, overrides and isolates (a U+202E
///   in a description would show the rest of the row, the amount included,
///   reversed);
/// - the zero-width space, the word joiner and the invisible operators;
/// - the byte order mark;
/// - the line and paragraph separators.
///
/// The joiners of emoji sequences and of some scripts (U+200C, U+200D)
/// stay.
pub(crate) fn perigoso(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{061C}'
                | '\u{200B}'
                | '\u{200E}'
                | '\u{200F}'
                | '\u{2028}'..='\u{202E}'
                | '\u{2060}'..='\u{2064}'
                | '\u{2066}'..='\u{2069}'
                | '\u{FEFF}'
        )
}

/// Text from the API or from a code on one line, without the characters
/// of [`perigoso`]: names and descriptions come from third parties, and an
/// escape sequence, a line break or a bidirectional override in them could
/// rewrite or fake what the terminal shows (e.g. a line with another
/// amount).
pub(crate) fn limpo(texto: &str) -> Cow<'_, str> {
    if texto.chars().any(perigoso) {
        Cow::Owned(
            texto
                .chars()
                .map(|c| if perigoso(c) { '\u{FFFD}' } else { c })
                .collect(),
        )
    } else {
        Cow::Borrowed(texto)
    }
}

/// Like [`limpo`], but keeps the line breaks of a text made of lines.
pub(crate) fn sem_controle(texto: &str) -> Cow<'_, str> {
    let controle = |c: char| perigoso(c) && c != '\n';
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

/// Like [`sem_controle`], but keeps the color codes the CLI writes
/// ([`cores::CODIGOS`]): any other escape sequence has its escape replaced
/// like the other control characters. A code in the data itself could at
/// most change a color: data reaches the renderers cleaned by [`limpo`].
pub(crate) fn sem_controle_exceto_cores(texto: &str) -> Cow<'_, str> {
    let controle = |c: char| perigoso(c) && c != '\n';
    if !texto.chars().any(controle) {
        return Cow::Borrowed(texto);
    }
    let mut limpo = String::with_capacity(texto.len());
    let mut resto = texto;
    while let Some(c) = resto.chars().next() {
        let codigo = cores::CODIGOS
            .iter()
            .find(|codigo| resto.starts_with(**codigo));
        if let Some(codigo) = codigo {
            limpo.push_str(codigo);
            resto = &resto[codigo.len()..];
        } else {
            limpo.push(if controle(c) { '\u{FFFD}' } else { c });
            resto = &resto[c.len_utf8()..];
        }
    }
    Cow::Owned(limpo)
}

/// Prints `text` and a newline to stdout. A closed pipe is not an error.
///
/// Control characters other than line breaks never reach the terminal,
/// whatever the renderer did with the data; with the colors on, but for
/// the codes of [`cores`]. On Windows, those go through the console's
/// ANSI mode (or its API, in an old console).
pub(crate) fn print(text: &str) -> Result<(), CliError> {
    let escrito = if cores::ativas() {
        let mut stdout = AutoStream::always(io::stdout().lock());
        writeln!(stdout, "{}", sem_controle_exceto_cores(text)).and_then(|()| stdout.flush())
    } else {
        let mut stdout = io::stdout().lock();
        writeln!(stdout, "{}", sem_controle(text)).and_then(|()| stdout.flush())
    };
    match escrito {
        Err(err) if err.kind() != io::ErrorKind::BrokenPipe => {
            Err(CliError::io("falha ao escrever na saída padrão", err))
        }
        _ => Ok(()),
    }
}

/// Writes `texto` and a newline to stderr, like `eprintln!`, without the
/// characters of [`perigoso`] but the line breaks: the summaries, the
/// warnings and the progress quote the data (a name, a status) and the
/// arguments.
pub(crate) fn eprint(texto: &str) {
    eprintln!("{}", sem_controle(texto));
}

/// Like [`eprint`], for a message on one line: a line break in the data (a
/// status, a name, a path) cannot start another line, like a fake `erro:`.
pub(crate) fn eprint_linha(texto: &str) {
    eprintln!("{}", limpo(texto));
}

/// Prints `tabela` as CSV. A file or a pipe gets the text as sent, for the
/// spreadsheets and the scripts; a terminal, the text cleaned as in the
/// text output (the message of a Pix could carry escape sequences), without
/// the BOM.
pub(crate) fn print_csv(tabela: &Tabela, separador: Separador) -> Result<(), CliError> {
    if io::stdout().is_terminal() {
        print_raw(&tabela.csv_para_terminal(separador))
    } else {
        print_raw(&tabela.csv(separador))
    }
}

/// Prints `text` as is (e.g. a completion script, which carries its own
/// line endings). Only for text the CLI wrote.
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
    print(&json_seguro(&json))
}

/// `json` with the characters of [`perigoso`] as `\u` escapes: `serde_json`
/// escapes only the C0 controls, so a DEL, a C1 control or a bidirectional
/// override in a text would reach the terminal. They can only be in the
/// strings (the line breaks outside them are the indentation's), so the
/// data is the same.
fn json_seguro(json: &str) -> Cow<'_, str> {
    let escapar = |c: char| perigoso(c) && c != '\n';
    if !json.chars().any(escapar) {
        return Cow::Borrowed(json);
    }
    let mut seguro = String::with_capacity(json.len() + 16);
    for c in json.chars() {
        if escapar(c) {
            // All of them are in the Basic Multilingual Plane.
            let _ = write!(seguro, "\\u{:04x}", u32::from(c));
        } else {
            seguro.push(c);
        }
    }
    Cow::Owned(seguro)
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

    use serde_json::Value;

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
    fn text_cannot_be_reversed_or_hidden() {
        // An override would show the rest of the row reversed, the amount
        // included; the invisible characters would hide text or break the
        // alignment.
        for c in [
            '\u{202E}', '\u{202A}', '\u{2066}', '\u{2069}', '\u{200E}', '\u{200F}', '\u{061C}',
            '\u{200B}', '\u{2060}', '\u{2064}', '\u{FEFF}', '\u{2028}', '\u{2029}', '\u{85}',
            '\u{7f}', '\u{9b}',
        ] {
            assert!(perigoso(c), "{:04X}", u32::from(c));
            assert_eq!(limpo(&format!("a{c}b")), "a\u{FFFD}b");
        }
        assert_eq!(
            sem_controle("R$ 1,00\u{202E}00,005.1 $R\n"),
            "R$ 1,00\u{FFFD}00,005.1 $R\n"
        );
        // Emoji sequences and the scripts that need the joiners keep them.
        for texto in [
            "Família 👨\u{200D}👩\u{200D}👧",
            "می\u{200C}خواهم",
            "Ação · João — 15%",
        ] {
            assert!(matches!(limpo(texto), Cow::Borrowed(_)), "{texto}");
        }
    }

    #[test]
    fn json_escapes_what_would_reach_the_terminal() {
        let valor = serde_json::json!({
            "descricao": "Loja\u{202E}R$ 9\u{7f}\u{9b}2J\u{1b}[2K\nfim",
            "normal": "sem nada",
        });
        let json = serde_json::to_string_pretty(&valor).unwrap();
        let seguro = json_seguro(&json);
        assert!(
            !seguro.chars().any(|c| perigoso(c) && c != '\n'),
            "{seguro}"
        );
        assert!(
            seguro.contains(r"Loja\u202eR$ 9\u007f\u009b2J\u001b[2K\nfim"),
            "{seguro}"
        );
        // The same data, read back.
        assert_eq!(serde_json::from_str::<Value>(&seguro).unwrap(), valor);
        assert!(matches!(json_seguro("{\n  \"a\": 1\n}"), Cow::Borrowed(_)));
    }

    #[test]
    fn only_the_color_codes_of_the_cli_pass() {
        let texto =
            "\u{1b}[1mData\u{1b}[0m  \u{1b}[31m-R$ 2,50\u{1b}[0m\nLoja\u{1b}[2K\r\u{1b}[8m\t";
        assert_eq!(
            sem_controle_exceto_cores(texto),
            "\u{1b}[1mData\u{1b}[0m  \u{1b}[31m-R$ 2,50\u{1b}[0m\nLoja\u{FFFD}[2K\u{FFFD}\u{FFFD}[8m\u{FFFD}"
        );
        // A code split or with other parameters is not one of them.
        assert_eq!(
            sem_controle_exceto_cores("\u{1b}[31;1m\u{1b}["),
            "\u{FFFD}[31;1m\u{FFFD}["
        );
        assert!(matches!(
            sem_controle_exceto_cores("sem controle"),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn masks_sensitive_values() {
        assert_eq!(mask("1234567", 2), "*****67");
        assert_eq!(mask("123", 2), "****");
        assert_eq!(mask("abcd-efgh-ijkl", 4), "**********ijkl");
    }
}
