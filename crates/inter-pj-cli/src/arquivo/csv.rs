//! CSV (RFC 4180) as spreadsheets save it: `,` or `;` (detected from the
//! header), fields quoted with `"`, line breaks inside quotes, `\r\n` or
//! `\n` line endings.

/// A record and the line where it starts, for messages.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Registro {
    pub(crate) linha: usize,
    pub(crate) campos: Vec<String>,
}

/// The header (trimmed) and the records after it, each with as many fields
/// as the header. Blank lines are skipped.
pub(crate) fn ler(texto: &str) -> Result<(Vec<String>, Vec<Registro>), String> {
    let separador = separador(texto);
    let mut registros = registros(texto, separador)?
        .into_iter()
        .filter(|registro| registro.campos.iter().any(|campo| !campo.trim().is_empty()));
    let cabecalho: Vec<String> = registros
        .next()
        .ok_or("arquivo vazio")?
        .campos
        .iter()
        .map(|campo| campo.trim().to_owned())
        .collect();
    let mut linhas = Vec::new();
    for mut registro in registros {
        if registro.campos.len() > cabecalho.len() {
            return Err(format!(
                "linha {}: {} colunas, mas o cabeçalho tem {}",
                registro.linha,
                registro.campos.len(),
                cabecalho.len()
            ));
        }
        registro.campos.resize(cabecalho.len(), String::new());
        linhas.push(registro);
    }
    Ok((cabecalho, linhas))
}

/// `;` when the header has more of them than commas (Excel in Portuguese).
fn separador(texto: &str) -> char {
    let cabecalho = texto
        .lines()
        .find(|linha| !linha.trim().is_empty())
        .unwrap_or_default();
    if cabecalho.matches(';').count() > cabecalho.matches(',').count() {
        ';'
    } else {
        ','
    }
}

fn registros(texto: &str, separador: char) -> Result<Vec<Registro>, String> {
    let mut registros = Vec::new();
    let mut campos = Vec::new();
    let mut campo = String::new();
    let (mut linha, mut inicio) = (1, 1);
    let mut aspas = false;
    let mut chars = texto.chars().peekable();
    while let Some(c) = chars.next() {
        if aspas {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    campo.push('"');
                }
                '"' => aspas = false,
                '\r' if chars.peek() == Some(&'\n') => {}
                '\n' => {
                    linha += 1;
                    campo.push('\n');
                }
                _ => campo.push(c),
            }
            continue;
        }
        match c {
            '"' if campo.is_empty() => aspas = true,
            '\r' if chars.peek() == Some(&'\n') => {}
            '\n' => {
                campos.push(std::mem::take(&mut campo));
                registros.push(Registro {
                    linha: inicio,
                    campos: std::mem::take(&mut campos),
                });
                linha += 1;
                inicio = linha;
            }
            c if c == separador => campos.push(std::mem::take(&mut campo)),
            _ => campo.push(c),
        }
    }
    if aspas {
        return Err(format!(
            "linha {inicio}: aspas abertas e não fechadas até o fim do arquivo"
        ));
    }
    if !campo.is_empty() || !campos.is_empty() {
        campos.push(campo);
        registros.push(Registro {
            linha: inicio,
            campos,
        });
    }
    Ok(registros)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn campos(registro: &Registro) -> Vec<&str> {
        registro.campos.iter().map(String::as_str).collect()
    }

    #[test]
    fn reads_what_spreadsheets_save() {
        let texto = "tipo;valor;descricao\r\nBOLETO;30,10;\"Conta; de luz\"\r\n\r\nDARF;47,14;\"IRPJ\r\n\"\"setembro\"\"\"\r\n";
        let (cabecalho, registros) = ler(texto).unwrap();
        assert_eq!(cabecalho, ["tipo", "valor", "descricao"]);
        assert_eq!(registros.len(), 2);
        assert_eq!(registros[0].linha, 2);
        assert_eq!(campos(&registros[0]), ["BOLETO", "30,10", "Conta; de luz"]);
        assert_eq!(registros[1].linha, 4);
        assert_eq!(
            campos(&registros[1]),
            ["DARF", "47,14", "IRPJ\n\"setembro\""]
        );
    }

    #[test]
    fn commas_short_rows_and_no_final_line_break() {
        let (cabecalho, registros) = ler(" tipo , valor ,extra\nBOLETO,\"1.500,00\"").unwrap();
        assert_eq!(cabecalho, ["tipo", "valor", "extra"]);
        assert_eq!(campos(&registros[0]), ["BOLETO", "1.500,00", ""]);
    }

    #[test]
    fn refuses_broken_files() {
        assert_eq!(ler("").unwrap_err(), "arquivo vazio");
        assert_eq!(ler("\n  \r\n").unwrap_err(), "arquivo vazio");
        // Blank lines before the header do not hide its separator.
        let (cabecalho, _) = ler("\n\na;b\n1;2\n").unwrap();
        assert_eq!(cabecalho, ["a", "b"]);
        assert_eq!(
            ler("a;b\n1;2;3\n").unwrap_err(),
            "linha 2: 3 colunas, mas o cabeçalho tem 2"
        );
        assert_eq!(
            ler("a;b\n1;\"2\n3;4\n").unwrap_err(),
            "linha 2: aspas abertas e não fechadas até o fim do arquivo"
        );
    }
}
