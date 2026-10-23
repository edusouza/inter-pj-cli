//! The statement in OFX 1.02, the SGML version that accounting, ERP and
//! personal finance software import from Brazilian banks: the account, the
//! transactions of the period and the balance at its end.
//!
//! The file is in Windows-1252 (`CHARSET:1252`), the encoding the
//! importers expect from a Brazilian bank: [`Extrato::render`] writes only
//! characters it has, and [`cp1252`] turns them into its bytes.

use std::collections::HashMap;
use std::fmt::Write as _;

use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use inter_pj::banking::{TipoTransacao, TransacaoCompleta, TransacaoSimples};
use rust_decimal::{Decimal, RoundingStrategy};

use super::{Contraparte, descricao};
use crate::commands::conta::{AGENCIA, BANCO, NOME_BANCO};
use crate::config::NumeroConta;

/// Longest values of OFX 1.02 (`A-32` and `A-255`), in characters.
const MAXIMO_NOME: usize = 32;
const MAXIMO_TEXTO: usize = 255;

/// The time of the transactions, of which the API gives only the day:
/// noon, so that an importer that takes it to another time zone keeps the
/// day.
const MEIO_DIA: NaiveTime = match NaiveTime::from_hms_opt(12, 0, 0) {
    Some(hora) => hora,
    None => panic!("meio-dia"),
};

const CABECALHO: &str = "\
OFXHEADER:100\r
DATA:OFXSGML\r
VERSION:102\r
SECURITY:NONE\r
ENCODING:USASCII\r
CHARSET:1252\r
COMPRESSION:NONE\r
OLDFILEUID:NONE\r
NEWFILEUID:NONE\r
\r
";

/// The statement of the account in a period. Times are Brasília's.
#[derive(Debug)]
pub(super) struct Extrato<'a> {
    pub(super) conta: &'a NumeroConta,
    pub(super) inicio: NaiveDate,
    /// The end of the period: the end of its last day, or the moment of
    /// the query when the period ends today.
    pub(super) fim: NaiveDateTime,
    /// The balance at [`fim`](Self::fim).
    pub(super) saldo: Decimal,
    /// When the file was made.
    pub(super) agora: NaiveDateTime,
    pub(super) transacoes: Vec<Transacao<'a>>,
}

/// A transaction, with what OFX needs from it.
#[derive(Debug)]
pub(super) struct Transacao<'a> {
    data: NaiveDate,
    /// Negative for debits.
    valor: Decimal,
    tipo: Option<&'a TipoTransacao>,
    titulo: Option<&'a str>,
    descricao: Option<&'a str>,
    /// `idTransacao`, which only the enriched statement has.
    id: Option<&'a str>,
    /// Who paid or received, when the details of the transaction tell.
    contraparte: Option<&'a str>,
}

impl<'a> Transacao<'a> {
    /// `None` without a date or an amount, which OFX requires.
    pub(super) fn simples(transacao: &'a TransacaoSimples) -> Option<Self> {
        Some(Self {
            data: transacao.data()?,
            valor: transacao.valor_com_sinal()?,
            tipo: transacao.tipo_transacao.as_ref(),
            titulo: transacao.titulo.as_deref(),
            descricao: transacao.descricao.as_deref(),
            id: None,
            contraparte: None,
        })
    }

    /// `None` without a date or an amount, which OFX requires.
    pub(super) fn completa(transacao: &'a TransacaoCompleta) -> Option<Self> {
        Some(Self {
            data: transacao.data()?,
            valor: transacao.valor_com_sinal()?,
            tipo: transacao.tipo_transacao.as_ref(),
            titulo: transacao.titulo.as_deref(),
            descricao: transacao.descricao.as_deref(),
            id: transacao.id_transacao.as_deref(),
            contraparte: Contraparte::de(transacao).nome,
        })
    }
}

impl Extrato<'_> {
    /// The OFX file, with CRLF line breaks and only characters of
    /// Windows-1252.
    pub(super) fn render(&self) -> String {
        let mut ofx = Sgml(CABECALHO.to_owned());
        ofx.abre("OFX");
        ofx.abre("SIGNONMSGSRSV1");
        ofx.abre("SONRS");
        ofx.status();
        ofx.valor("DTSERVER", &momento(self.agora));
        ofx.valor("LANGUAGE", "POR");
        ofx.abre("FI");
        ofx.valor("ORG", NOME_BANCO);
        ofx.valor("FID", BANCO);
        ofx.fecha("FI");
        ofx.fecha("SONRS");
        ofx.fecha("SIGNONMSGSRSV1");

        ofx.abre("BANKMSGSRSV1");
        ofx.abre("STMTTRNRS");
        ofx.valor("TRNUID", "1");
        ofx.status();
        ofx.abre("STMTRS");
        ofx.valor("CURDEF", "BRL");
        ofx.abre("BANKACCTFROM");
        ofx.valor("BANKID", BANCO);
        ofx.valor("BRANCHID", AGENCIA);
        ofx.valor("ACCTID", &self.conta.to_string());
        ofx.valor("ACCTTYPE", "CHECKING");
        ofx.fecha("BANKACCTFROM");

        ofx.abre("BANKTRANLIST");
        ofx.valor("DTSTART", &momento(self.inicio.and_time(NaiveTime::MIN)));
        ofx.valor("DTEND", &momento(self.fim));
        let mut fitids = Fitids::default();
        for transacao in &self.transacoes {
            ofx.abre("STMTTRN");
            ofx.valor("TRNTYPE", tipo(transacao.tipo, transacao.valor));
            ofx.valor("DTPOSTED", &momento(transacao.data.and_time(MEIO_DIA)));
            ofx.valor("TRNAMT", &decimal(transacao.valor));
            ofx.valor("FITID", &texto(&fitids.de(transacao), MAXIMO_TEXTO));
            ofx.opcional(
                "NAME",
                transacao.contraparte.map(|nome| texto(nome, MAXIMO_NOME)),
            );
            ofx.opcional(
                "MEMO",
                Some(texto(
                    &descricao(transacao.titulo, transacao.descricao),
                    MAXIMO_TEXTO,
                )),
            );
            ofx.fecha("STMTTRN");
        }
        ofx.fecha("BANKTRANLIST");

        ofx.abre("LEDGERBAL");
        ofx.valor("BALAMT", &decimal(self.saldo));
        ofx.valor("DTASOF", &momento(self.fim));
        ofx.fecha("LEDGERBAL");
        ofx.fecha("STMTRS");
        ofx.fecha("STMTTRNRS");
        ofx.fecha("BANKMSGSRSV1");
        ofx.fecha("OFX");
        ofx.0
    }
}

/// The body of the file, one tag per line. Values are written without the
/// closing tag, as OFX 1.02 allows and the banks do.
struct Sgml(String);

impl Sgml {
    fn abre(&mut self, tag: &str) {
        let _ = write!(self.0, "<{tag}>\r\n");
    }

    fn fecha(&mut self, tag: &str) {
        let _ = write!(self.0, "</{tag}>\r\n");
    }

    fn valor(&mut self, tag: &str, valor: &str) {
        let _ = write!(self.0, "<{tag}>{valor}\r\n");
    }

    /// A value only when there is one: an empty element is invalid.
    fn opcional(&mut self, tag: &str, valor: Option<String>) {
        if let Some(valor) = valor.filter(|valor| !valor.is_empty()) {
            self.valor(tag, &valor);
        }
    }

    fn status(&mut self) {
        self.abre("STATUS");
        self.valor("CODE", "0");
        self.valor("SEVERITY", "INFO");
        self.fecha("STATUS");
    }
}

/// The FITIDs, by which an importer skips the transactions it already has.
#[derive(Debug, Default)]
struct Fitids(HashMap<String, u32>);

impl Fitids {
    /// The `idTransacao` of the enriched statement. Without it, a hash of
    /// the date, the amount and the texts of the API, numbered among the
    /// equal transactions of the day, so that two equal Pix on the same
    /// day are not taken as one. It is the same in every run while the API
    /// returns the same transactions; changing what goes into it changes
    /// the FITIDs of the files already imported, and the importers would
    /// take their transactions again.
    fn de(&mut self, transacao: &Transacao<'_>) -> String {
        if let Some(id) = transacao.id.map(str::trim).filter(|id| !id.is_empty()) {
            return id.to_owned();
        }
        let chave = [
            transacao.data.format("%Y-%m-%d").to_string(),
            decimal(transacao.valor),
            transacao.titulo.unwrap_or_default().to_owned(),
            transacao.descricao.unwrap_or_default().to_owned(),
        ]
        .join("\u{1f}");
        let vez = self.0.entry(chave.clone()).or_default();
        *vez += 1;
        // The number first: FNV mixes the bytes that come before the last.
        format!("{:016x}", fnv1a(format!("{vez}\u{1f}{chave}").as_bytes()))
    }
}

/// FNV-1a of 64 bits: a fixed function, unlike the hashers of the
/// standard library, which may change between versions of Rust.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// `TRNTYPE`: the more specific type of OFX when the kind of the
/// transaction has one in its direction, else `CREDIT` or `DEBIT`.
fn tipo(tipo: Option<&TipoTransacao>, valor: Decimal) -> &'static str {
    let saida = valor < Decimal::ZERO;
    let especifico = match tipo {
        Some(TipoTransacao::Pix | TipoTransacao::Transferencia) => Some("XFER"),
        Some(TipoTransacao::Juros) => Some("INT"),
        Some(TipoTransacao::Tarifa) if saida => Some("FEE"),
        Some(TipoTransacao::Pagamento | TipoTransacao::Imposto) if saida => Some("PAYMENT"),
        Some(TipoTransacao::DebitoAutomatico) if saida => Some("DIRECTDEBIT"),
        Some(TipoTransacao::CompraDebito) if saida => Some("POS"),
        Some(TipoTransacao::Saque) if saida => Some("CASH"),
        Some(TipoTransacao::Cheque) if saida => Some("CHECK"),
        Some(TipoTransacao::DepositoBoleto) if !saida => Some("DEP"),
        _ => None,
    };
    especifico.unwrap_or(if saida { "DEBIT" } else { "CREDIT" })
}

/// A moment in Brasília, as OFX writes it: `20260930235959[-3:BRT]`.
fn momento(momento: NaiveDateTime) -> String {
    format!("{}[-3:BRT]", momento.format("%Y%m%d%H%M%S"))
}

/// An amount with a point and two decimal places: `-250.10`.
fn decimal(valor: Decimal) -> String {
    let valor = valor.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero);
    if valor.is_zero() {
        // Without the sign of a zero debit.
        return "0.00".to_owned();
    }
    format!("{valor:.2}")
}

/// A text of the API as an OFX value: on one line, without control
/// characters, with at most `maximo` characters, only those of
/// Windows-1252 (`?` for the others) and `&`, `<` and `>` escaped.
fn texto(valor: &str, maximo: usize) -> String {
    let mut limpo = String::with_capacity(valor.len());
    for c in valor.chars() {
        if c.is_control() || c.is_whitespace() {
            if !limpo.is_empty() && !limpo.ends_with(' ') {
                limpo.push(' ');
            }
        } else if !('\u{300}'..='\u{36f}').contains(&c) {
            // Combining accents (a decomposed "ã") are dropped: Windows-1252
            // has only the composed letters.
            limpo.push(if byte(c).is_some() { c } else { '?' });
        }
    }
    let mut escapado = String::with_capacity(limpo.len());
    for c in limpo.trim_end().chars().take(maximo) {
        match c {
            '&' => escapado.push_str("&amp;"),
            '<' => escapado.push_str("&lt;"),
            '>' => escapado.push_str("&gt;"),
            c => escapado.push(c),
        }
    }
    escapado
}

/// The bytes of `texto` in Windows-1252; a character it does not have
/// becomes `?` ([`Extrato::render`] writes none).
pub(super) fn cp1252(texto: &str) -> Vec<u8> {
    texto.chars().map(|c| byte(c).unwrap_or(b'?')).collect()
}

/// The byte of `c` in Windows-1252, if it has one: Latin-1, but for the
/// typographic characters in the place of the C1 controls.
fn byte(c: char) -> Option<u8> {
    if let Ok(byte) = u8::try_from(u32::from(c))
        && !(0x80..0xa0).contains(&byte)
    {
        return Some(byte);
    }
    Some(match c {
        '€' => 0x80,
        '‚' => 0x82,
        'ƒ' => 0x83,
        '„' => 0x84,
        '…' => 0x85,
        '†' => 0x86,
        '‡' => 0x87,
        'ˆ' => 0x88,
        '‰' => 0x89,
        'Š' => 0x8a,
        '‹' => 0x8b,
        'Œ' => 0x8c,
        'Ž' => 0x8e,
        '‘' => 0x91,
        '’' => 0x92,
        '“' => 0x93,
        '”' => 0x94,
        '•' => 0x95,
        '–' => 0x96,
        '—' => 0x97,
        '˜' => 0x98,
        '™' => 0x99,
        'š' => 0x9a,
        '›' => 0x9b,
        'œ' => 0x9c,
        'ž' => 0x9e,
        'Ÿ' => 0x9f,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    fn dec(valor: &str) -> Decimal {
        Decimal::from_str(valor).unwrap()
    }

    fn dia(texto: &str) -> NaiveDate {
        NaiveDate::parse_from_str(texto, "%Y-%m-%d").unwrap()
    }

    fn transacao<'a>(
        data: &str,
        valor: &str,
        tipo: Option<&'a TipoTransacao>,
        titulo: &'a str,
        descricao: &'a str,
    ) -> Transacao<'a> {
        Transacao {
            data: dia(data),
            valor: dec(valor),
            tipo,
            titulo: Some(titulo),
            descricao: Some(descricao),
            id: None,
            contraparte: None,
        }
    }

    /// The OFX of [`renders_a_statement`], with LF line breaks.
    const SETEMBRO: &str = "\
OFXHEADER:100
DATA:OFXSGML
VERSION:102
SECURITY:NONE
ENCODING:USASCII
CHARSET:1252
COMPRESSION:NONE
OLDFILEUID:NONE
NEWFILEUID:NONE

<OFX>
<SIGNONMSGSRSV1>
<SONRS>
<STATUS>
<CODE>0
<SEVERITY>INFO
</STATUS>
<DTSERVER>20261023143000[-3:BRT]
<LANGUAGE>POR
<FI>
<ORG>Banco Inter S.A.
<FID>077
</FI>
</SONRS>
</SIGNONMSGSRSV1>
<BANKMSGSRSV1>
<STMTTRNRS>
<TRNUID>1
<STATUS>
<CODE>0
<SEVERITY>INFO
</STATUS>
<STMTRS>
<CURDEF>BRL
<BANKACCTFROM>
<BANKID>077
<BRANCHID>0001
<ACCTID>7654321-0
<ACCTTYPE>CHECKING
</BANKACCTFROM>
<BANKTRANLIST>
<DTSTART>20260901000000[-3:BRT]
<DTEND>20260930235959[-3:BRT]
<STMTTRN>
<TRNTYPE>XFER
<DTPOSTED>20260903120000[-3:BRT]
<TRNAMT>1500.00
<FITID>19360e3ab1f51ae3
<MEMO>Pix recebido · Cliente Exemplo Ltda
</STMTTRN>
<STMTTRN>
<TRNTYPE>PAYMENT
<DTPOSTED>20260905120000[-3:BRT]
<TRNAMT>-250.10
<FITID>98765
<NAME>Distribuidora de Energia Exemplo
<MEMO>Pagamento efetuado · Energia &amp; Luz &lt;Ltda&gt;
</STMTTRN>
</BANKTRANLIST>
<LEDGERBAL>
<BALAMT>1249.90
<DTASOF>20260930235959[-3:BRT]
</LEDGERBAL>
</STMTRS>
</STMTTRNRS>
</BANKMSGSRSV1>
</OFX>
";

    #[test]
    fn renders_a_statement() {
        let conta = NumeroConta {
            numero: "7654321".to_owned(),
            digito: "0".to_owned(),
        };
        let mut pagamento = transacao(
            "2026-09-05",
            "-250.1",
            Some(&TipoTransacao::Pagamento),
            "Pagamento efetuado",
            "Energia & Luz <Ltda>",
        );
        pagamento.id = Some("98765");
        pagamento.contraparte = Some("Distribuidora de Energia Exemplo S.A.");
        let extrato = Extrato {
            conta: &conta,
            inicio: dia("2026-09-01"),
            fim: dia("2026-09-30").and_hms_opt(23, 59, 59).unwrap(),
            saldo: dec("1249.9"),
            agora: dia("2026-10-23").and_hms_opt(14, 30, 0).unwrap(),
            transacoes: vec![
                transacao(
                    "2026-09-03",
                    "1500",
                    Some(&TipoTransacao::Pix),
                    "Pix recebido",
                    "Cliente Exemplo Ltda",
                ),
                pagamento,
            ],
        };
        assert_eq!(extrato.render().replace("\r\n", "\n"), SETEMBRO);
        // Every line ends in CRLF.
        assert!(!extrato.render().replace("\r\n", "").contains(['\r', '\n']));
    }

    /// The hash is a promise to the importers: if it changed, the
    /// transactions of a month already imported would come in again.
    #[test]
    fn fitid_is_stable_and_tells_equal_transactions_apart() {
        let pix = |descricao| {
            transacao(
                "2026-09-03",
                "1500.00",
                Some(&TipoTransacao::Pix),
                "Pix recebido",
                descricao,
            )
        };
        let mut fitids = Fitids::default();
        let primeiro = fitids.de(&pix("Cliente Exemplo Ltda"));
        let segundo = fitids.de(&pix("Cliente Exemplo Ltda"));
        let outro = fitids.de(&pix("Outro Cliente"));
        assert_eq!(primeiro, "19360e3ab1f51ae3");
        assert_eq!(segundo, "094437f242e8de70");
        assert_ne!(outro, primeiro);

        // The same in another run, whatever the transactions of other days,
        // and whatever the zeros of the amount the API sends.
        let mut outra_execucao = Fitids::default();
        outra_execucao.de(&pix("Cliente Exemplo Ltda").com_data("2026-09-02"));
        let mut igual = pix("Cliente Exemplo Ltda");
        igual.valor = dec("1500");
        assert_eq!(outra_execucao.de(&igual), primeiro);

        // The type of the transaction, which the API may refine, is not in it.
        igual.tipo = Some(&TipoTransacao::Transferencia);
        assert_eq!(Fitids::default().de(&igual), primeiro);

        // The id of the enriched statement, when there is one.
        igual.id = Some(" 123 ");
        assert_eq!(Fitids::default().de(&igual), "123");
        igual.id = Some("");
        assert_eq!(Fitids::default().de(&igual), primeiro);
    }

    impl Transacao<'_> {
        fn com_data(mut self, data: &str) -> Self {
            self.data = dia(data);
            self
        }
    }

    #[test]
    fn types_follow_the_kind_and_the_direction() {
        let casos = [
            (Some(TipoTransacao::Pix), "-1", "XFER"),
            (Some(TipoTransacao::Transferencia), "1", "XFER"),
            (Some(TipoTransacao::Juros), "1", "INT"),
            (Some(TipoTransacao::Tarifa), "-1", "FEE"),
            (Some(TipoTransacao::Tarifa), "1", "CREDIT"),
            (Some(TipoTransacao::Pagamento), "-1", "PAYMENT"),
            (Some(TipoTransacao::Imposto), "-1", "PAYMENT"),
            (Some(TipoTransacao::Pagamento), "1", "CREDIT"),
            (Some(TipoTransacao::DebitoAutomatico), "-1", "DIRECTDEBIT"),
            (Some(TipoTransacao::CompraDebito), "-1", "POS"),
            (Some(TipoTransacao::Saque), "-1", "CASH"),
            (Some(TipoTransacao::Cheque), "-1", "CHECK"),
            (Some(TipoTransacao::DepositoBoleto), "1", "DEP"),
            (Some(TipoTransacao::BoletoCobranca), "1", "CREDIT"),
            (Some(TipoTransacao::Estorno), "-1", "DEBIT"),
            (Some(TipoTransacao::Outro("NOVO".to_owned())), "1", "CREDIT"),
            (None, "-1", "DEBIT"),
            (None, "0", "CREDIT"),
        ];
        for (tipo_transacao, valor, esperado) in casos {
            assert_eq!(
                tipo(tipo_transacao.as_ref(), dec(valor)),
                esperado,
                "{tipo_transacao:?} {valor}"
            );
        }
    }

    #[test]
    fn amounts_have_two_decimal_places() {
        assert_eq!(decimal(dec("1500")), "1500.00");
        assert_eq!(decimal(dec("-250.1")), "-250.10");
        assert_eq!(decimal(dec("0.005")), "0.01");
        assert_eq!(decimal(-dec("0")), "0.00");
        assert_eq!(decimal(dec("-0.00")), "0.00");
    }

    #[test]
    fn texts_fit_on_one_line_of_windows_1252() {
        assert_eq!(
            texto("  Cliente\u{1b}[2K\r\n03/08  R$ 9,00 \t", 255),
            "Cliente [2K 03/08 R$ 9,00"
        );
        assert_eq!(
            texto("Ação & <Cia> “Ltda” – €", 255),
            "Ação &amp; &lt;Cia&gt; “Ltda” – €"
        );
        assert_eq!(texto("Sa\u{303}o Paulo 🙂", 255), "Sao Paulo ?");
        assert_eq!(texto("abc\u{202e}def", 255), "abc?def");
        assert_eq!(texto("Fornecedor & Filhos", 12), "Fornecedor &amp;");
        assert_eq!(texto(" \n ", 255), "");
    }

    #[test]
    fn windows_1252_bytes() {
        assert_eq!(cp1252("Ação · R$"), b"A\xe7\xe3o \xb7 R$");
        assert_eq!(cp1252("\u{80}\u{9b}🙂"), b"???");
        for b in (0..0x80u8).chain(0xa0..=0xff) {
            assert_eq!(byte(char::from(b)), Some(b));
        }
        // The typographic characters in the place of the C1 controls, but
        // for five places that have none (`?`).
        let tipograficos = "€?‚ƒ„…†‡ˆ‰Š‹Œ?Ž??‘’“”•–—˜™š›œ?žŸ";
        assert_eq!(tipograficos.chars().count(), 32);
        for (b, c) in (0x80..=0x9fu8).zip(tipograficos.chars()) {
            if c != '?' {
                assert_eq!(byte(c), Some(b), "{c}");
            }
        }
    }
}
