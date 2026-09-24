//! The `console` blocks of a guide: the commands, what the guide shows after
//! each one, and the comparison with what they printed.

use std::fs;
use std::ops::Range;
use std::path::Path;

use crate::sessao::Execucao;

/// What the test does with a block, by the comment before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Diretiva {
    /// Runs the commands and compares what they print.
    Conferir,
    /// `<!-- guia: saída ilustrativa -->`: runs them, and only checks that
    /// they succeed (or fail) as the guide shows.
    Ilustrativa,
    /// `<!-- guia: não executar -->`.
    NaoExecutar,
}

#[derive(Debug)]
pub(crate) struct Guia {
    linhas: Vec<String>,
    pub(crate) blocos: Vec<Bloco>,
}

#[derive(Debug)]
pub(crate) struct Bloco {
    pub(crate) diretiva: Diretiva,
    pub(crate) comandos: Vec<Comando>,
}

#[derive(Debug)]
pub(crate) struct Comando {
    /// The line of the `$`, from 1.
    pub(crate) linha: usize,
    /// The command, with the continued lines (`\`) joined.
    pub(crate) texto: String,
    /// What the guide shows after it, up to the next command.
    esperado: Vec<String>,
    /// Where `esperado` is in the file, without the blank lines after it.
    saida: Range<usize>,
    /// What replaces `esperado` when updating the guide.
    novo: Option<Vec<String>>,
}

/// The answer to a confirmation, as a terminal shows it.
fn confirmacao(linha: &str) -> bool {
    linha.ends_with("[s/N] s") || linha.ends_with("[s/N] sim")
}

impl Guia {
    pub(crate) fn ler(caminho: &Path) -> Self {
        let texto = fs::read_to_string(caminho).unwrap();
        let linhas: Vec<String> = texto.lines().map(str::to_owned).collect();
        let mut blocos = Vec::new();
        let mut i = 0;
        while i < linhas.len() {
            if linhas[i].trim_end() != "```console" {
                i += 1;
                continue;
            }
            let diretiva = diretiva(&linhas[..i]);
            let fim = (i + 1..linhas.len())
                .find(|&j| linhas[j].trim_end() == "```")
                .unwrap_or_else(|| {
                    panic!("{}: bloco sem fim na linha {}", caminho.display(), i + 1)
                });
            blocos.push(Bloco {
                diretiva,
                comandos: comandos(&linhas, i + 1..fim),
            });
            i = fim + 1;
        }
        Self { linhas, blocos }
    }

    /// Writes the guide back, with the outputs updated.
    pub(crate) fn gravar(&self, caminho: &Path) {
        let mut novas = Vec::with_capacity(self.linhas.len());
        let mut i = 0;
        for comando in self.blocos.iter().flat_map(|bloco| &bloco.comandos) {
            let Some(novo) = &comando.novo else { continue };
            novas.extend_from_slice(&self.linhas[i..comando.saida.start]);
            novas.extend(novo.iter().cloned());
            i = comando.saida.end;
        }
        novas.extend_from_slice(&self.linhas[i..]);
        let mut texto = novas.join("\n");
        texto.push('\n');
        fs::write(caminho, texto).unwrap();
    }
}

/// The comment right before a block, if it is a directive.
fn diretiva(antes: &[String]) -> Diretiva {
    match antes.iter().rev().find(|linha| !linha.trim().is_empty()) {
        Some(linha) if linha.trim() == "<!-- guia: saída ilustrativa -->" => Diretiva::Ilustrativa,
        Some(linha) if linha.trim() == "<!-- guia: não executar -->" => Diretiva::NaoExecutar,
        _ => Diretiva::Conferir,
    }
}

fn comandos(linhas: &[String], bloco: Range<usize>) -> Vec<Comando> {
    let mut comandos = Vec::new();
    let mut i = bloco.start;
    while i < bloco.end {
        let Some(inicio) = linhas[i].strip_prefix("$ ") else {
            // Only blank lines before the first command: the others are the
            // output of the command before them.
            assert!(
                linhas[i].trim().is_empty(),
                "linha {}: texto antes do primeiro comando do bloco",
                i + 1
            );
            i += 1;
            continue;
        };
        let linha = i + 1;
        let mut texto = inicio.trim_end().to_owned();
        while texto.ends_with('\\') && i + 1 < bloco.end {
            texto.pop();
            i += 1;
            texto.push(' ');
            texto.push_str(linhas[i].trim());
        }
        i += 1;
        let inicio_saida = i;
        while i < bloco.end && !linhas[i].starts_with("$ ") {
            i += 1;
        }
        // The blank lines before the next command separate the examples.
        let mut fim_saida = i;
        while fim_saida > inicio_saida && linhas[fim_saida - 1].trim().is_empty() {
            fim_saida -= 1;
        }
        comandos.push(Comando {
            linha,
            texto,
            esperado: linhas[inicio_saida..fim_saida].to_vec(),
            saida: inicio_saida..fim_saida,
            novo: None,
        });
    }
    comandos
}

impl Comando {
    /// Whether the guide shows a confirmation answered with yes.
    pub(crate) fn confirma(&self) -> bool {
        self.esperado.iter().any(|linha| confirmacao(linha))
    }

    /// Whether the guide shows the command failing.
    fn falha(&self) -> bool {
        self.esperado
            .iter()
            .any(|linha| linha.starts_with("erro: "))
    }

    pub(crate) fn conferir_so_o_resultado(&self, execucao: &Execucao) -> Option<String> {
        match (self.falha(), execucao.sucesso) {
            (false, false) => Some(format!("`{}` falhou:\n{}", self.texto, execucao.saida)),
            (true, true) => Some(format!(
                "`{}` deveria falhar, e funcionou:\n{}",
                self.texto, execucao.saida
            )),
            _ => None,
        }
    }

    pub(crate) fn conferir(&self, execucao: &Execucao) -> Option<String> {
        if let Some(erro) = self.conferir_so_o_resultado(execucao) {
            return Some(erro);
        }
        let esperado: Vec<&str> = self
            .esperado
            .iter()
            .map(String::as_str)
            .filter(|linha| !confirmacao(linha))
            .collect();
        let obtido: Vec<&str> = execucao.linhas();
        if iguais(&esperado.join("\n"), &obtido.join("\n")) {
            return None;
        }
        Some(format!(
            "`{}` imprimiu outra coisa.\n--- o guia mostra\n{}\n--- o comando imprimiu\n{}",
            self.texto,
            esperado.join("\n"),
            obtido.join("\n")
        ))
    }

    /// Takes what the command printed as what the guide shows. The answer
    /// to a confirmation stays where it was, or, in a new example, goes
    /// after the summary; the generated keys stay the ones of the guide.
    pub(crate) fn atualizar(&mut self, execucao: &Execucao) {
        let mut novo: Vec<String> = execucao.linhas().iter().map(|l| (*l).to_owned()).collect();
        if let Some(posicao) = self.esperado.iter().position(|linha| confirmacao(linha)) {
            let destino = if self.esperado.len() == 1 {
                depois_do_resumo(&novo)
            } else {
                posicao.min(novo.len())
            };
            novo.insert(destino, self.esperado[posicao].clone());
        }
        let antigos = ids(&self.esperado.join("\n")).1;
        let mut texto = novo.join("\n");
        let (_, atuais) = ids(&texto);
        if antigos.len() == atuais.len() {
            for (atual, antigo) in atuais.iter().zip(&antigos) {
                texto = texto.replacen(atual.as_str(), antigo, 1);
            }
            novo = texto.lines().map(str::to_owned).collect();
        }
        self.novo = Some(novo);
    }
}

/// Where a terminal shows the question of a confirmation: after the
/// summary, which is the banner of production, if any, a title, and its
/// indented lines and warnings.
fn depois_do_resumo(linhas: &[String]) -> usize {
    let banner = usize::from(linhas.first().is_some_and(|linha| linha.starts_with("***")));
    let titulo = (banner + 1).min(linhas.len());
    titulo
        + linhas[titulo..]
            .iter()
            .take_while(|linha| linha.starts_with("  ") || linha.starts_with("aviso: "))
            .count()
}

/// Whether the texts are the same, but for the generated keys and txids,
/// which must be repeated where the guide repeats them.
fn iguais(esperado: &str, obtido: &str) -> bool {
    let (esperado, ids_esperados) = ids(esperado);
    let (obtido, ids_obtidos) = ids(obtido);
    esperado == obtido
        && ids_esperados.len() == ids_obtidos.len()
        && (0..ids_esperados.len()).all(|i| {
            (0..ids_esperados.len()).all(|j| {
                (ids_esperados[i] == ids_esperados[j]) == (ids_obtidos[i] == ids_obtidos[j])
            })
        })
}

/// `texto` with each UUID and each 32-digit hexadecimal txid as `⟨id⟩`, and
/// the ones replaced, in order.
fn ids(texto: &str) -> (String, Vec<String>) {
    let bytes = texto.as_bytes();
    let hex = |b: u8| b.is_ascii_digit() || (b'a'..=b'f').contains(&b);
    let limite = |i: usize| bytes.get(i).is_none_or(|b| !b.is_ascii_alphanumeric());
    let formato = |inicio: usize, forma: &[usize]| -> Option<usize> {
        let mut i = inicio;
        for (parte, &tamanho) in forma.iter().enumerate() {
            if parte > 0 {
                if bytes.get(i) != Some(&b'-') {
                    return None;
                }
                i += 1;
            }
            if i + tamanho > bytes.len() || !bytes[i..i + tamanho].iter().all(|&b| hex(b)) {
                return None;
            }
            i += tamanho;
        }
        limite(i).then_some(i)
    };
    let mut saida = String::with_capacity(texto.len());
    let mut encontrados = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if (i == 0 || !bytes[i - 1].is_ascii_alphanumeric())
            && let Some(fim) = formato(i, &[8, 4, 4, 4, 12]).or_else(|| formato(i, &[32]))
        {
            encontrados.push(texto[i..fim].to_owned());
            saida.push_str("⟨id⟩");
            i = fim;
            continue;
        }
        let c = texto[i..].chars().next().unwrap();
        saida.push(c);
        i += c.len_utf8();
    }
    (saida, encontrados)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_question_goes_after_the_summary() {
        let linhas = |texto: &str| texto.lines().map(str::to_owned).collect::<Vec<_>>();
        let saida = linhas(
            "*** PRODUÇÃO ***\nPix a enviar\n  Valor  R$ 1,00\naviso: confira\nPix enviado.\nCódigo  1",
        );
        assert_eq!(depois_do_resumo(&saida), 4);
        assert_eq!(
            depois_do_resumo(&linhas("Devolução\n  Valor  R$ 1,00\nFeito.")),
            2
        );
        assert_eq!(depois_do_resumo(&[]), 0);
    }

    #[test]
    fn generated_ids_match_any_other_consistently() {
        let guia = "Chave  9b2f6c1e-5d0a-4c1b-8f3e-2a7d4e6b8c90\ntxid 0123456789abcdef0123456789abcdef\nChave  9b2f6c1e-5d0a-4c1b-8f3e-2a7d4e6b8c90";
        let outra = "Chave  11111111-2222-4333-8444-555555555555\ntxid ffffffffffffffffffffffffffffffff\nChave  11111111-2222-4333-8444-555555555555";
        assert!(iguais(guia, outra));
        // The same key twice in the guide, two different ones in the output.
        let diferentes = "Chave  11111111-2222-4333-8444-555555555555\ntxid ffffffffffffffffffffffffffffffff\nChave  11111111-2222-4333-8444-666666666666";
        assert!(!iguais(guia, diferentes));
        // Not part of a longer code.
        assert_eq!(ids("E0123456789abcdef0123456789abcdef").1.len(), 0);
    }
}
