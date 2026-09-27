//! Tabular output shared by the listing commands: aligned text for people
//! and CSV (RFC 4180) for spreadsheets and scripts.

use std::fmt::Write as _;

use chrono::NaiveDate;
use rust_decimal::Decimal;

use crate::output::{brl, limpo};

/// Field separator of the CSV output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Separador {
    /// `,` with `.` as decimal point: the RFC 4180 default, for scripts.
    Virgula,
    /// `;` with `,` as decimal point and a UTF-8 BOM: what Excel in
    /// Portuguese expects.
    PontoEVirgula,
}

impl Separador {
    fn campo(self) -> char {
        match self {
            Self::Virgula => ',',
            Self::PontoEVirgula => ';',
        }
    }
}

/// A column: its header in the text output and its field name in the CSV.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Coluna {
    titulo: &'static str,
    campo: &'static str,
    direita: bool,
    largura_maxima: Option<usize>,
}

impl Coluna {
    /// Left-aligned column.
    pub(crate) const fn texto(titulo: &'static str, campo: &'static str) -> Self {
        Self {
            titulo,
            campo,
            direita: false,
            largura_maxima: None,
        }
    }

    /// Right-aligned column, for amounts.
    pub(crate) const fn valor(titulo: &'static str, campo: &'static str) -> Self {
        Self {
            titulo,
            campo,
            direita: true,
            largura_maxima: None,
        }
    }

    /// Truncates longer values in the text output (never in CSV).
    pub(crate) const fn no_maximo(mut self, largura: usize) -> Self {
        self.largura_maxima = Some(largura);
        self
    }
}

/// A value, formatted according to the output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Celula {
    Texto(String),
    /// `R$ 1.234,56` in text, `1234.56` in CSV.
    Dinheiro(Decimal),
    /// `DD/MM/AAAA` in text, `AAAA-MM-DD` in CSV.
    Data(NaiveDate),
    Vazia,
}

impl Celula {
    pub(crate) fn texto(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            Some(text) if !text.is_empty() => Self::Texto(text.to_owned()),
            _ => Self::Vazia,
        }
    }

    pub(crate) fn dinheiro(value: Option<Decimal>) -> Self {
        value.map_or(Self::Vazia, Self::Dinheiro)
    }

    /// A parsed date, or the raw text when it could not be parsed.
    pub(crate) fn data(parsed: Option<NaiveDate>, raw: Option<&str>) -> Self {
        parsed.map_or_else(|| Self::texto(raw), Self::Data)
    }

    fn para_texto(&self) -> String {
        match self {
            Self::Texto(text) => limpo(text).into_owned(),
            Self::Dinheiro(value) => brl(*value),
            Self::Data(date) => date.format("%d/%m/%Y").to_string(),
            Self::Vazia => String::new(),
        }
    }

    fn para_csv(&self, separador: Separador) -> String {
        match self {
            Self::Texto(text) => escape_csv(&neutralize_formula(text), separador.campo()),
            Self::Dinheiro(value) => match separador {
                Separador::Virgula => value.to_string(),
                Separador::PontoEVirgula => value.to_string().replace('.', ","),
            },
            Self::Data(date) => date.format("%Y-%m-%d").to_string(),
            Self::Vazia => String::new(),
        }
    }
}

/// Rows of cells under a fixed set of columns.
#[derive(Debug)]
pub(crate) struct Tabela {
    colunas: Vec<Coluna>,
    linhas: Vec<Vec<Celula>>,
}

impl Tabela {
    pub(crate) fn new(colunas: Vec<Coluna>) -> Self {
        Self {
            colunas,
            linhas: Vec::new(),
        }
    }

    pub(crate) fn linha(&mut self, celulas: Vec<Celula>) {
        debug_assert_eq!(
            celulas.len(),
            self.colunas.len(),
            "linha com colunas a mais ou a menos"
        );
        self.linhas.push(celulas);
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.linhas.is_empty()
    }

    /// Aligned columns separated by two spaces, with a header line.
    pub(crate) fn texto(&self) -> String {
        let linhas: Vec<Vec<String>> = self
            .linhas
            .iter()
            .map(|linha| {
                linha
                    .iter()
                    .zip(&self.colunas)
                    .map(|(celula, coluna)| truncate(&celula.para_texto(), coluna.largura_maxima))
                    .collect()
            })
            .collect();
        let larguras: Vec<usize> = self
            .colunas
            .iter()
            .enumerate()
            .map(|(i, coluna)| {
                linhas
                    .iter()
                    .map(|linha| width(&linha[i]))
                    .chain([width(coluna.titulo)])
                    .max()
                    .unwrap_or(0)
            })
            .collect();

        let cabecalho: Vec<String> = self.colunas.iter().map(|c| c.titulo.to_owned()).collect();
        std::iter::once(&cabecalho)
            .chain(&linhas)
            .map(|linha| {
                let mut out = String::new();
                for (i, (valor, coluna)) in linha.iter().zip(&self.colunas).enumerate() {
                    if i > 0 {
                        out.push_str("  ");
                    }
                    let pad = " ".repeat(larguras[i].saturating_sub(width(valor)));
                    if coluna.direita {
                        out.push_str(&pad);
                        out.push_str(valor);
                    } else {
                        out.push_str(valor);
                        out.push_str(&pad);
                    }
                }
                out.trim_end().to_owned()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// RFC 4180: header line, CRLF line endings, fields quoted when needed.
    /// With `;`, a UTF-8 BOM lets Excel detect the encoding.
    pub(crate) fn csv(&self, separador: Separador) -> String {
        let sep = separador.campo();
        let mut out = String::new();
        if separador == Separador::PontoEVirgula {
            out.push('\u{feff}');
        }
        let cabecalho: Vec<String> = self
            .colunas
            .iter()
            .map(|c| escape_csv(c.campo, sep))
            .collect();
        let _ = write!(out, "{}\r\n", cabecalho.join(&sep.to_string()));
        for linha in &self.linhas {
            let campos: Vec<String> = linha.iter().map(|c| c.para_csv(separador)).collect();
            let _ = write!(out, "{}\r\n", campos.join(&sep.to_string()));
        }
        out
    }
}

/// Display width: characters, so accented letters count as one.
fn width(text: &str) -> usize {
    text.chars().count()
}

fn truncate(text: &str, max: Option<usize>) -> String {
    match max {
        Some(max) if width(text) > max && max > 0 => {
            let kept: String = text.chars().take(max - 1).collect();
            format!("{}…", kept.trim_end())
        }
        _ => text.to_owned(),
    }
}

fn escape_csv(field: &str, separador: char) -> String {
    if field.contains([separador, '"', '\n', '\r']) {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_owned()
    }
}

/// Descriptions come from third parties (e.g. the message of a Pix): a text
/// starting with `=`, `+`, `-` or `@` would run as a formula when the CSV is
/// opened in a spreadsheet, so it is prefixed with an apostrophe.
fn neutralize_formula(text: &str) -> String {
    if text.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{text}")
    } else {
        text.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dec(s: &str) -> Decimal {
        s.parse().unwrap()
    }

    fn exemplo() -> Tabela {
        let mut tabela = Tabela::new(vec![
            Coluna::texto("Data", "data"),
            Coluna::texto("Descrição", "descricao"),
            Coluna::valor("Valor", "valor"),
        ]);
        tabela.linha(vec![
            Celula::Data(NaiveDate::from_ymd_opt(2026, 8, 3).unwrap()),
            Celula::texto(Some("Pix recebido · João")),
            Celula::Dinheiro(dec("1500.00")),
        ]);
        tabela.linha(vec![
            Celula::Data(NaiveDate::from_ymd_opt(2026, 8, 5).unwrap()),
            Celula::texto(Some("Tarifa")),
            Celula::Dinheiro(dec("-2.5")),
        ]);
        tabela
    }

    #[test]
    fn text_cells_stay_on_one_line_without_control_characters() {
        let mut tabela = Tabela::new(vec![
            Coluna::texto("Descrição", "descricao"),
            Coluna::valor("Valor", "valor"),
        ]);
        tabela.linha(vec![
            Celula::texto(Some("Loja\n01/01/2026  Pix recebido  R$ 9.999,00\u{1b}[1A")),
            Celula::Dinheiro(dec("1")),
        ]);
        let texto = tabela.texto();
        assert_eq!(texto.lines().count(), 2, "{texto}");
        assert!(!texto.contains('\u{1b}'), "{texto}");
        // CSV keeps the text as sent, quoted.
        assert!(
            tabela
                .csv(Separador::Virgula)
                .contains("\"Loja\n01/01/2026  Pix recebido  R$ 9.999,00\u{1b}[1A\""),
        );
    }

    #[test]
    fn text_aligns_columns_counting_accents_as_one() {
        assert_eq!(
            exemplo().texto(),
            "\
Data        Descrição                  Valor
03/08/2026  Pix recebido · João  R$ 1.500,00
05/08/2026  Tarifa                  -R$ 2,50"
        );
    }

    #[test]
    fn text_truncates_long_values() {
        let mut tabela = Tabela::new(vec![Coluna::texto("Descrição", "d").no_maximo(10)]);
        tabela.linha(vec![Celula::texto(Some("Pagamento de fornecedor"))]);
        tabela.linha(vec![Celula::texto(Some("curto"))]);
        assert_eq!(tabela.texto(), "Descrição\nPagamento…\ncurto");
    }

    #[test]
    fn csv_follows_rfc_4180() {
        assert_eq!(
            exemplo().csv(Separador::Virgula),
            "data,descricao,valor\r\n2026-08-03,Pix recebido · João,1500.00\r\n2026-08-05,Tarifa,-2.5\r\n"
        );
        let mut tabela = Tabela::new(vec![Coluna::texto("A", "a"), Coluna::texto("B", "b")]);
        tabela.linha(vec![
            Celula::texto(Some("com, vírgula")),
            Celula::texto(Some("com \"aspas\"\ne quebra")),
        ]);
        tabela.linha(vec![Celula::Vazia, Celula::texto(Some("   "))]);
        assert_eq!(
            tabela.csv(Separador::Virgula),
            "a,b\r\n\"com, vírgula\",\"com \"\"aspas\"\"\ne quebra\"\r\n,\r\n"
        );
    }

    #[test]
    fn csv_for_excel_uses_semicolons_decimal_commas_and_bom() {
        let csv = exemplo().csv(Separador::PontoEVirgula);
        assert!(csv.starts_with('\u{feff}'));
        assert_eq!(
            &csv['\u{feff}'.len_utf8()..],
            "data;descricao;valor\r\n2026-08-03;Pix recebido · João;1500,00\r\n2026-08-05;Tarifa;-2,5\r\n"
        );
        let mut tabela = Tabela::new(vec![Coluna::texto("A", "a")]);
        tabela.linha(vec![Celula::texto(Some("a;b"))]);
        assert!(
            tabela
                .csv(Separador::PontoEVirgula)
                .ends_with("\"a;b\"\r\n")
        );
    }

    #[test]
    fn csv_neutralizes_formulas_in_text_but_not_in_amounts() {
        let mut tabela = Tabela::new(vec![Coluna::texto("A", "a"), Coluna::valor("V", "v")]);
        tabela.linha(vec![
            Celula::texto(Some("=HYPERLINK(\"http://exemplo.invalid\")")),
            Celula::Dinheiro(dec("-10")),
        ]);
        tabela.linha(vec![Celula::texto(Some("@SUM(1)")), Celula::Vazia]);
        assert_eq!(
            tabela.csv(Separador::Virgula),
            "a,v\r\n\"'=HYPERLINK(\"\"http://exemplo.invalid\"\")\",-10\r\n'@SUM(1),\r\n"
        );
    }

    #[test]
    fn unparsed_dates_fall_back_to_the_raw_text() {
        assert_eq!(
            Celula::data(None, Some("31/02/2026")),
            Celula::Texto("31/02/2026".into())
        );
        assert_eq!(Celula::data(None, None), Celula::Vazia);
    }
}
