//! Diagnosis of the configuration file as text: the lines whose basic
//! (double-quoted) strings hold backslashes, the usual form of a Windows
//! path, and their correction.
//!
//! In TOML, a backslash inside double quotes starts an escape: `\U` asks for
//! eight hexadecimal digits (hence the error on `"C:\Users\..."`), and a valid
//! one such as `\n` or `\t` silently becomes a control character in the path.
//! Single quotes (literal strings) keep every character as typed.

/// Keys whose values are paths: a backslash in them is always a Windows
/// separator, even where it happens to form a valid escape.
const CHAVES_DE_CAMINHO: [&str; 2] = ["certificado", "chave_privada"];

/// A basic string of the file that should not be one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Aspas {
    /// Line of the file, from 1.
    pub(crate) linha: usize,
    /// Key of the line (`certificado`, `chave_privada`...).
    pub(crate) chave: String,
    /// The path as typed between the quotes; `None` for other keys, whose
    /// values (the `client_secret` among them) are never shown.
    pub(crate) caminho: Option<String>,
    /// Whether TOML rejects the value, so that the file does not load;
    /// otherwise its escapes are valid and change the value silently.
    pub(crate) invalida: bool,
    /// The line rewritten, or `None` when it cannot be corrected safely.
    pub(crate) correcao: Option<String>,
}

impl Aspas {
    /// The corrected line, when it may be shown: only for paths.
    pub(crate) fn correcao_visivel(&self) -> Option<&str> {
        self.caminho.as_ref().and(self.correcao.as_deref())
    }
}

/// The lines whose double-quoted value holds a backslash that should be a
/// literal character: in a path key, any backslash; in another key, one that
/// forms an escape TOML rejects.
pub(crate) fn aspas_duplas(texto: &str) -> Vec<Aspas> {
    texto
        .lines()
        .enumerate()
        .filter_map(|(indice, linha)| analisar(indice + 1, linha))
        .collect()
}

/// The file with every correctable line rewritten, and the lines changed.
pub(crate) fn corrigir(texto: &str) -> (String, Vec<Aspas>) {
    let corrigidas: Vec<Aspas> = aspas_duplas(texto)
        .into_iter()
        .filter(|problema| problema.correcao.is_some())
        .collect();
    let mut saida = String::with_capacity(texto.len() + corrigidas.len());
    for (indice, linha) in texto.split_inclusive('\n').enumerate() {
        let nova = corrigidas
            .iter()
            .find(|problema| problema.linha == indice + 1)
            .and_then(|problema| problema.correcao.as_deref());
        match nova {
            Some(nova) => {
                saida.push_str(nova);
                saida.push_str(&linha[linha.trim_end_matches(['\r', '\n']).len()..]);
            }
            None => saida.push_str(linha),
        }
    }
    (saida, corrigidas)
}

/// The text with its control characters written as escapes (`\n`, `\t`,
/// `\u{8}`...) and every other character as is: unlike `escape_debug`, a
/// Windows separator stays a single backslash.
pub(crate) fn escapar_controles(texto: &str) -> String {
    texto
        .chars()
        .map(|c| {
            if c.is_control() {
                c.escape_default().to_string()
            } else {
                c.to_string()
            }
        })
        .collect()
}

fn analisar(numero: usize, linha: &str) -> Option<Aspas> {
    // A byte order mark, as some Windows editors write, stays in the
    // correction with the indentation.
    let conteudo = linha.trim_start_matches('\u{feff}').trim_start();
    if conteudo.starts_with('#') || conteudo.starts_with('[') {
        return None;
    }
    let (chave, resto) = conteudo.split_once('=')?;
    // The last part of a dotted key (`perfis.padrao.certificado`).
    let chave = chave
        .rsplit('.')
        .next()
        .unwrap_or(chave)
        .trim()
        .trim_matches(['"', '\''])
        .to_owned();
    let valor = resto.trim_start();
    // Only a single basic string: not a literal one, a multi-line one, an
    // array or an inline table.
    if !valor.starts_with('"') || valor.starts_with("\"\"\"") {
        return None;
    }
    let (bruto, depois) = fechamento(&valor[1..])?;
    if !bruto.contains('\\') {
        return None;
    }
    let invalida = toml::from_str::<toml::Table>(&format!("v = \"{bruto}\"")).is_err();
    let caminho = CHAVES_DE_CAMINHO.contains(&chave.as_str());
    if !invalida && !caminho {
        return None;
    }
    let recuo = &linha[..linha.len() - conteudo.len()];
    let antes_do_valor = &conteudo[..conteudo.len() - valor.len()];
    // A quote of its own (escaped or not) has a meaning the correction
    // cannot guess: that line is only reported.
    let correcao = (!bruto.contains('"')).then(|| {
        let novo = if bruto.contains('\'') {
            // No literal string can hold it: double quotes, with the
            // backslashes doubled.
            format!("\"{}\"", bruto.replace('\\', "\\\\"))
        } else {
            format!("'{bruto}'")
        };
        format!("{recuo}{antes_do_valor}{novo}{depois}")
    });
    Some(Aspas {
        linha: numero,
        caminho: caminho.then(|| bruto.to_owned()),
        chave,
        invalida,
        correcao,
    })
}

/// Splits what follows the opening quote into the value as typed and the
/// rest of the line after the closing quote. The closing quote is the first
/// one followed only by blanks or a comment: in a Windows path, a backslash
/// before it (`"C:\inter\"`) is a separator, not an escape.
fn fechamento(depois_da_aspa: &str) -> Option<(&str, &str)> {
    depois_da_aspa
        .char_indices()
        .filter(|&(_, c)| c == '"')
        .map(|(posicao, _)| posicao)
        .find(|&posicao| {
            let resto = depois_da_aspa[posicao + 1..].trim_start();
            resto.is_empty() || resto.starts_with('#')
        })
        .map(|posicao| (&depois_da_aspa[..posicao], &depois_da_aspa[posicao + 1..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ler(texto: &str) -> toml::Table {
        toml::from_str(texto).unwrap()
    }

    #[test]
    fn windows_paths_become_literal_strings() {
        let texto = "[perfis.padrao]\n\
                     certificado = \"C:\\Users\\teste\\inter\\certificado.crt\"\n\
                     chave_privada = \"C:\\inter\\chave.key\"  # a chave\n\
                     client_id = \"abc\"\n";
        assert!(toml::from_str::<toml::Table>(texto).is_err());
        let problemas = aspas_duplas(texto);
        assert_eq!(problemas.len(), 2, "{problemas:?}");
        assert_eq!(problemas[0].linha, 2);
        assert_eq!(problemas[0].chave, "certificado");
        assert!(problemas[0].invalida, "\\U without hexadecimal digits");
        assert_eq!(
            problemas[0].caminho.as_deref(),
            Some("C:\\Users\\teste\\inter\\certificado.crt")
        );
        assert_eq!(problemas[1].linha, 3);
        assert!(problemas[1].invalida, "\\i is no escape");

        let (corrigido, corrigidas) = corrigir(texto);
        assert_eq!(corrigidas.len(), 2);
        assert_eq!(
            corrigido,
            "[perfis.padrao]\n\
             certificado = 'C:\\Users\\teste\\inter\\certificado.crt'\n\
             chave_privada = 'C:\\inter\\chave.key'  # a chave\n\
             client_id = \"abc\"\n"
        );
        assert_eq!(
            ler(&corrigido)["perfis"]["padrao"]["certificado"].as_str(),
            Some("C:\\Users\\teste\\inter\\certificado.crt")
        );
    }

    #[test]
    fn a_valid_escape_in_a_path_is_also_corrected() {
        // "\n" and "\t" load, as a line break and a tab inside the path.
        let texto = "certificado = \"C:\\novo\\teste.crt\"\n";
        assert_eq!(
            ler(texto)["certificado"].as_str(),
            Some("C:\novo\teste.crt")
        );
        let problemas = aspas_duplas(texto);
        assert_eq!(problemas.len(), 1);
        assert!(!problemas[0].invalida);
        assert_eq!(corrigir(texto).0, "certificado = 'C:\\novo\\teste.crt'\n");
    }

    #[test]
    fn other_keys_are_reported_only_when_the_file_does_not_load() {
        assert!(aspas_duplas("client_id = \"a\\tb\"\n").is_empty());
        let problemas = aspas_duplas("client_id = \"a\\qb\"\n");
        assert_eq!(problemas.len(), 1);
        assert!(problemas[0].invalida);
        assert_eq!(problemas[0].caminho, None);
        assert_eq!(problemas[0].correcao_visivel(), None);
    }

    #[test]
    fn the_value_of_other_keys_is_never_kept() {
        let problemas = aspas_duplas("client_secret = \"se\\gredo\"\n");
        assert_eq!(problemas.len(), 1);
        assert_eq!(problemas[0].chave, "client_secret");
        assert_eq!(problemas[0].caminho, None);
        assert_eq!(problemas[0].correcao_visivel(), None);
        assert_eq!(
            corrigir("client_secret = \"se\\gredo\"\n").0,
            "client_secret = 'se\\gredo'\n"
        );
    }

    #[test]
    fn dotted_keys_are_recognized() {
        let problemas = aspas_duplas("perfis.padrao.certificado = \"C:\\novo.crt\"\n");
        assert_eq!(problemas.len(), 1);
        assert_eq!(problemas[0].chave, "certificado");
    }

    #[test]
    fn a_trailing_backslash_does_not_hide_the_closing_quote() {
        let (corrigido, _) = corrigir("certificado = \"C:\\inter\\\" # pasta\n");
        assert_eq!(corrigido, "certificado = 'C:\\inter\\' # pasta\n");
    }

    #[test]
    fn a_value_with_a_single_quote_keeps_double_quotes_and_doubles_backslashes() {
        let (corrigido, _) = corrigir("certificado = \"C:\\d'agua\\c.crt\"\n");
        assert_eq!(corrigido, "certificado = \"C:\\\\d'agua\\\\c.crt\"\n");
        assert_eq!(
            ler(&corrigido)["certificado"].as_str(),
            Some("C:\\d'agua\\c.crt")
        );
    }

    #[test]
    fn a_value_with_a_double_quote_is_only_reported() {
        let problemas = aspas_duplas("certificado = \"C:\\\"x\\c.crt\"\n");
        assert_eq!(problemas.len(), 1);
        assert_eq!(problemas[0].correcao, None);
    }

    #[test]
    fn unc_paths_keep_both_leading_backslashes() {
        let (corrigido, _) = corrigir("certificado = \"\\\\servidor\\inter\\c.crt\"\n");
        assert_eq!(corrigido, "certificado = '\\\\servidor\\inter\\c.crt'\n");
    }

    #[test]
    fn other_lines_are_left_alone() {
        let texto = "# certificado = \"C:\\Users\\x\"\n\
                     certificado = 'C:\\Users\\x.crt'\n\
                     chave_privada = \"/home/teste/chave.key\"\n\
                     escopos = [\"extrato.read\"]\n\
                     notas = \"\"\"\nC:\\Users\n\"\"\"\n\
                     [perfis.\"C:\\\\x\"]\n";
        assert!(aspas_duplas(texto).is_empty(), "{:?}", aspas_duplas(texto));
        assert_eq!(corrigir(texto).0, texto);
    }

    #[test]
    fn a_byte_order_mark_is_kept() {
        let texto = "\u{feff}certificado = \"C:\\Users\\c.crt\"\n";
        assert_eq!(aspas_duplas(texto).len(), 1);
        assert_eq!(
            corrigir(texto).0,
            "\u{feff}certificado = 'C:\\Users\\c.crt'\n"
        );
    }

    #[test]
    fn control_characters_are_escaped_and_backslashes_kept() {
        assert_eq!(
            escapar_controles("C:\\Users\\x\\C:\novo\teste\u{8}.crt"),
            "C:\\Users\\x\\C:\\novo\\teste\\u{8}.crt"
        );
    }

    #[test]
    fn line_endings_and_indentation_are_kept() {
        let (corrigido, _) = corrigir("  certificado = \"C:\\Users\\c.crt\"\r\n");
        assert_eq!(corrigido, "  certificado = 'C:\\Users\\c.crt'\r\n");
    }
}
