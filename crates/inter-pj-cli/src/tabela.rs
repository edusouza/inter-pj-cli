//! Tabular output shared by the listing commands: aligned text for people
//! and CSV (RFC 4180) for spreadsheets and scripts.

use std::fmt::Write as _;

use chrono::NaiveDate;
use rust_decimal::Decimal;

use crate::cores::{self, Tom};
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
    /// A status, colored by its tone in the terminal.
    Situacao(String, Tom),
    /// `R$ 1.234,56` in text, `1234.56` in CSV; red in the terminal when
    /// negative.
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

    /// A status in words, with its tone when it has one.
    pub(crate) fn situacao(texto: Option<&str>, tom: Option<Tom>) -> Self {
        match (Self::texto(texto), tom) {
            (Self::Texto(texto), Some(tom)) => Self::Situacao(texto, tom),
            (celula, _) => celula,
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
            Self::Texto(text) | Self::Situacao(text, _) => limpo(text).into_owned(),
            Self::Dinheiro(value) => brl(*value),
            Self::Data(date) => date.format("%d/%m/%Y").to_string(),
            Self::Vazia => String::new(),
        }
    }

    /// The color in the terminal: the tone of a status, red for an amount
    /// that shows as negative.
    fn cor(&self) -> Option<&'static str> {
        match self {
            Self::Situacao(_, tom) => Some(tom.codigo()),
            Self::Dinheiro(value) if brl(*value).starts_with('-') => Some(cores::VERMELHO),
            _ => None,
        }
    }

    fn para_csv(&self, separador: Separador) -> String {
        match self {
            Self::Texto(text) | Self::Situacao(text, _) => {
                escape_csv(&neutralize_formula(text), separador.campo())
            }
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

    /// Aligned columns separated by two spaces, with a header line. Plain:
    /// what goes to stderr (summaries, errors) or into another text.
    pub(crate) fn texto(&self) -> String {
        self.renderizar(false)
    }

    /// [`Tabela::texto`] for the standard output: with the colors of
    /// [`cores`] when they are on.
    pub(crate) fn texto_colorido(&self) -> String {
        self.renderizar(cores::ativas())
    }

    /// The columns are measured without the colors, which go around each
    /// value, not its padding.
    fn renderizar(&self, com_cores: bool) -> String {
        let linhas: Vec<Vec<(String, Option<&str>)>> = self
            .linhas
            .iter()
            .map(|linha| {
                linha
                    .iter()
                    .zip(&self.colunas)
                    .map(|(celula, coluna)| {
                        (
                            truncate(&celula.para_texto(), coluna.largura_maxima),
                            celula.cor(),
                        )
                    })
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
                    .map(|linha| width(&linha[i].0))
                    .chain([width(coluna.titulo)])
                    .max()
                    .unwrap_or(0)
            })
            .collect();

        let cabecalho: Vec<(String, Option<&str>)> = self
            .colunas
            .iter()
            .map(|c| (c.titulo.to_owned(), Some(cores::NEGRITO)))
            .collect();
        std::iter::once(&cabecalho)
            .chain(&linhas)
            .map(|linha| {
                let mut out = String::new();
                for (i, ((valor, cor), coluna)) in linha.iter().zip(&self.colunas).enumerate() {
                    if i > 0 {
                        out.push_str("  ");
                    }
                    let pad = " ".repeat(larguras[i].saturating_sub(width(valor)));
                    let valor = match cor {
                        Some(cor) if com_cores && !valor.is_empty() => {
                            format!("{cor}{valor}{}", cores::FIM)
                        }
                        _ => valor.clone(),
                    };
                    if coluna.direita {
                        out.push_str(&pad);
                        out.push_str(&valor);
                    } else {
                        out.push_str(&valor);
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
    fn colors_go_around_the_values_and_keep_the_alignment() {
        let mut tabela = exemplo();
        tabela.colunas.push(Coluna::texto("Status", ""));
        for (linha, tom) in tabela.linhas.iter_mut().zip([Tom::Positivo, Tom::Negativo]) {
            linha.push(Celula::situacao(Some("status"), Some(tom)));
        }
        let colorido = tabela.renderizar(true);
        assert_eq!(
            colorido,
            "\
\u{1b}[1mData\u{1b}[0m        \u{1b}[1mDescrição\u{1b}[0m                  \u{1b}[1mValor\u{1b}[0m  \u{1b}[1mStatus\u{1b}[0m
03/08/2026  Pix recebido · João  R$ 1.500,00  \u{1b}[32mstatus\u{1b}[0m
05/08/2026  Tarifa                  \u{1b}[31m-R$ 2,50\u{1b}[0m  \u{1b}[31mstatus\u{1b}[0m"
        );
        // Without the codes, the same text as without colors.
        let mut sem_codigos = colorido;
        for codigo in cores::CODIGOS {
            sem_codigos = sem_codigos.replace(codigo, "");
        }
        assert_eq!(sem_codigos, tabela.texto());
        // Off (as in the tests), the colored text is the plain one; the CSV
        // never has colors.
        assert_eq!(tabela.texto_colorido(), tabela.texto());
        assert!(!tabela.csv(Separador::Virgula).contains('\u{1b}'));
    }

    #[test]
    fn statuses_without_text_or_tone_are_plain() {
        assert_eq!(Celula::situacao(None, Some(Tom::Positivo)), Celula::Vazia);
        assert_eq!(
            Celula::situacao(Some("  "), Some(Tom::Negativo)),
            Celula::Vazia
        );
        assert_eq!(
            Celula::situacao(Some("outro"), None),
            Celula::Texto("outro".into())
        );
        // An amount that rounds to zero is not negative.
        assert_eq!(Celula::Dinheiro(dec("-0.004")).cor(), None);
        assert_eq!(Celula::Dinheiro(dec("-0.005")).cor(), Some(cores::VERMELHO));
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
