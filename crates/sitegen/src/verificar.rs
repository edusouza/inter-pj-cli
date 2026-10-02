//! Checks the `inter-pj` commands written in the pages of the site against the CLI.
//!
//! A page that shows `inter-pj extrato --formato json` is making a promise: that the command
//! exists and takes that option. The command tree comes from the help of the built binary, so
//! a command renamed or removed in the CLI fails the build of the site instead of living on
//! in the documentation. Only the commands are checked; the outputs shown are examples with
//! made-up data.

use std::fs;
use std::path::{Path, PathBuf};

use crate::ajuda::Comando;
use crate::paginas;

/// A command that the CLI does not accept.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Problema {
    /// Path of the page from `site/conteudo/`, with `/`.
    pub(crate) pagina: String,
    pub(crate) linha: usize,
    pub(crate) mensagem: String,
}

/// Checks every hand-written page under `conteudo`.
pub(crate) fn paginas(conteudo: &Path, raiz: &Comando) -> Result<Vec<Problema>, String> {
    let mut arquivos = Vec::new();
    listar(conteudo, &mut arquivos)?;
    arquivos.sort();

    let mut problemas = Vec::new();
    for arquivo in arquivos {
        let texto = fs::read_to_string(&arquivo)
            .map_err(|erro| format!("não consegui ler {}: {erro}", arquivo.display()))?;
        if paginas::gerada(&texto) {
            continue;
        }
        let pagina = arquivo
            .strip_prefix(conteudo)
            .unwrap_or(&arquivo)
            .to_string_lossy()
            .replace('\\', "/");
        for (linha, mensagem) in conferir(&texto, raiz) {
            problemas.push(Problema {
                pagina: pagina.clone(),
                linha,
                mensagem,
            });
        }
    }
    Ok(problemas)
}

fn listar(pasta: &Path, arquivos: &mut Vec<PathBuf>) -> Result<(), String> {
    let entradas = fs::read_dir(pasta)
        .map_err(|erro| format!("não consegui listar {}: {erro}", pasta.display()))?;
    for entrada in entradas {
        let caminho = entrada.map_err(|erro| erro.to_string())?.path();
        if caminho.is_dir() {
            listar(&caminho, arquivos)?;
        } else if caminho.extension().is_some_and(|extensao| extensao == "md") {
            arquivos.push(caminho);
        }
    }
    Ok(())
}

/// The commands of a page that the CLI does not accept, with the number of their line.
fn conferir(texto: &str, raiz: &Comando) -> Vec<(usize, String)> {
    let mut problemas = Vec::new();
    let mut linguagem: Option<String> = None;
    let mut continuacao: Option<(usize, String)> = None;

    for (indice, linha) in texto.lines().enumerate() {
        let numero = indice + 1;
        let aparada = linha.trim_start();
        if aparada.starts_with("```") || aparada.starts_with("~~~") {
            linguagem = match linguagem {
                Some(_) => None,
                None => Some(aparada.trim_start_matches(['`', '~']).trim().to_string()),
            };
            continuacao = None;
            continue;
        }
        let Some(linguagem) = linguagem.as_deref() else {
            continue;
        };

        // A command broken over several lines with a backslash is read as one.
        let (primeira, comando) = match continuacao.take() {
            Some((primeira, anterior)) => (primeira, format!("{anterior} {}", aparada.trim_end())),
            None => match comando_da_linha(aparada, linguagem) {
                Some(comando) => (numero, comando),
                None => continue,
            },
        };
        if let Some(sem_barra) = comando.strip_suffix('\\') {
            continuacao = Some((primeira, sem_barra.trim_end().to_string()));
            continue;
        }
        if let Err(mensagem) = validar(&palavras(&comando), raiz) {
            problemas.push((primeira, format!("`{}`: {mensagem}", comando.trim())));
        }
    }
    problemas
}

/// The command a line of a code block runs, if it is an `inter-pj` one. In a `console` block
/// only the lines with a prompt are commands; the rest is what the terminal printed.
fn comando_da_linha(linha: &str, linguagem: &str) -> Option<String> {
    let comando = if let Some(resto) = linha
        .strip_prefix("$ ")
        .or_else(|| linha.strip_prefix("PS> "))
    {
        resto
    } else if matches!(linguagem, "bash" | "sh" | "shell") {
        linha
    } else {
        return None;
    };
    let comando = comando.trim();
    (comando == "inter-pj" || comando.starts_with("inter-pj ")).then(|| comando.to_string())
}

/// Splits a command into words, the way a shell would, up to the first pipe, redirection,
/// separator or comment.
fn palavras(comando: &str) -> Vec<String> {
    let mut palavras = Vec::new();
    let mut atual = String::new();
    let mut aspas: Option<char> = None;
    let mut tem_palavra = false;

    for c in comando.chars() {
        match (aspas, c) {
            (Some(abertura), c) if c == abertura => aspas = None,
            (Some(_), c) => atual.push(c),
            (None, '\'' | '"') => {
                aspas = Some(c);
                tem_palavra = true;
            }
            (None, '|' | '>' | '<' | ';' | '&') => break,
            (None, '#') if !tem_palavra => break,
            (None, c) if c.is_whitespace() => {
                if tem_palavra {
                    palavras.push(std::mem::take(&mut atual));
                    tem_palavra = false;
                }
            }
            (None, c) => {
                atual.push(c);
                tem_palavra = true;
            }
        }
    }
    if tem_palavra {
        palavras.push(atual);
    }
    palavras
}

/// Walks the words of a command down the tree, the way clap would.
fn validar(palavras: &[String], raiz: &Comando) -> Result<(), String> {
    let mut atual = raiz;
    let mut pediu_ajuda = false;
    let mut resto = palavras.iter().skip(1);

    while let Some(palavra) = resto.next() {
        if palavra == "--" {
            break;
        }
        if let Some(longa) = palavra.strip_prefix("--") {
            let (nome, com_igual) = longa
                .split_once('=')
                .map_or((longa, false), |(n, _)| (n, true));
            let opcao = atual
                .opcoes
                .iter()
                .find(|o| o.longa.as_deref() == Some(nome))
                .ok_or_else(|| format!("a opção --{nome} não existe em `{}`", atual.nome()))?;
            pediu_ajuda |= nome == "help" || nome == "version";
            if opcao.com_valor && !com_igual && resto.next().is_none() {
                return Err(format!("falta o valor de --{nome}"));
            }
        } else if palavra.len() > 1 && palavra.starts_with('-') {
            let letras: Vec<char> = palavra[1..].chars().collect();
            for (posicao, letra) in letras.iter().enumerate() {
                let opcao = atual
                    .opcoes
                    .iter()
                    .find(|o| o.curta == Some(*letra))
                    .ok_or_else(|| format!("a opção -{letra} não existe em `{}`", atual.nome()))?;
                pediu_ajuda |= matches!(letra, 'h' | 'V');
                if opcao.com_valor {
                    // The rest of the word is the value; at the end of it, the next word is.
                    if posicao + 1 == letras.len() && resto.next().is_none() {
                        return Err(format!("falta o valor de -{letra}"));
                    }
                    break;
                }
            }
        } else if let Some(filho) = atual
            .subcomandos
            .iter()
            .find(|c| c.caminho.last() == Some(palavra))
        {
            atual = filho;
        } else if !atual.subcomandos.is_empty() {
            return Err(format!(
                "`{palavra}` não é um comando de `{}`",
                atual.nome()
            ));
        }
    }

    if atual.exige_subcomando && !pediu_ajuda {
        return Err(format!("`{}` precisa de um subcomando", atual.nome()));
    }
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::ajuda::exemplo;

    fn problemas(texto: &str) -> Vec<(usize, String)> {
        conferir(texto, &exemplo::arvore().raiz)
    }

    fn comando(texto: &str) -> Result<(), String> {
        validar(&palavras(texto), &exemplo::arvore().raiz)
    }

    #[test]
    fn accepts_commands_the_cli_has() {
        for texto in [
            "inter-pj saldo",
            "inter-pj saldo --data 2026-08-31 --json",
            "inter-pj --perfil producao saldo",
            "inter-pj -p producao saldo",
            "inter-pj -pproducao saldo",
            "inter-pj --perfil=producao saldo",
            "inter-pj extrato --inicio 2026-08-01 --dividir-periodo",
            "inter-pj extrato completo --todas-paginas -o extrato.csv",
            "inter-pj extrato completo -vv",
            "inter-pj config init --forcar",
            "inter-pj --help",
            "inter-pj --version",
            "inter-pj config --help",
        ] {
            assert_eq!(comando(texto), Ok(()), "{texto}");
        }
    }

    #[test]
    fn rejects_what_the_cli_does_not_have() {
        let casos = [
            (
                "inter-pj pix enviar",
                "`pix` não é um comando de `inter-pj`",
            ),
            (
                "inter-pj extrato csv",
                "`csv` não é um comando de `inter-pj extrato`",
            ),
            (
                "inter-pj saldo --dia 2026-08-31",
                "a opção --dia não existe em `inter-pj saldo`",
            ),
            (
                "inter-pj saldo -x",
                "a opção -x não existe em `inter-pj saldo`",
            ),
            (
                "inter-pj extrato --todas-paginas",
                "a opção --todas-paginas não existe em `inter-pj extrato`",
            ),
            ("inter-pj saldo --data", "falta o valor de --data"),
            ("inter-pj saldo -p", "falta o valor de -p"),
            ("inter-pj", "`inter-pj` precisa de um subcomando"),
            (
                "inter-pj config",
                "`inter-pj config` precisa de um subcomando",
            ),
            (
                "inter-pj --version saldo --foo",
                "a opção --foo não existe em `inter-pj saldo`",
            ),
        ];
        for (texto, esperado) in casos {
            assert_eq!(comando(texto), Err(esperado.to_string()), "{texto}");
        }
    }

    #[test]
    fn an_optional_subcommand_may_be_left_out() {
        assert_eq!(comando("inter-pj extrato"), Ok(()));
    }

    #[test]
    fn splits_words_like_a_shell_up_to_a_pipe_or_redirection() {
        assert_eq!(
            palavras("inter-pj saldo --json | jq .disponivel"),
            ["inter-pj", "saldo", "--json"]
        );
        assert_eq!(
            palavras("inter-pj extrato > agosto.csv"),
            ["inter-pj", "extrato"]
        );
        assert_eq!(
            palavras("inter-pj --perfil 'meu perfil' saldo # o de produção"),
            ["inter-pj", "--perfil", "meu perfil", "saldo"]
        );
        assert_eq!(
            palavras("inter-pj --separador ';' extrato"),
            ["inter-pj", "--separador", ";", "extrato"]
        );
        assert_eq!(palavras("inter-pj --x \"\""), ["inter-pj", "--x", ""]);
    }

    #[test]
    fn reads_the_commands_of_a_console_block_by_their_prompt() {
        let texto = "\
```console
$ inter-pj saldo
inter-pj 0.2.0
Saldo disponível  R$ 1,00
$ inter-pj saldo --nada
PS> inter-pj pix
```
";
        let achados = problemas(texto);
        let linhas: Vec<_> = achados.iter().map(|(linha, _)| *linha).collect();
        assert_eq!(linhas, [5, 6], "{achados:?}");
        assert_eq!(
            achados[0].1,
            "`inter-pj saldo --nada`: a opção --nada não existe em `inter-pj saldo`"
        );
    }

    #[test]
    fn a_bash_block_has_commands_without_a_prompt() {
        let texto =
            "```bash\n#!/usr/bin/env bash\ninter-pj saldo --json | jq .\ninter-pj remessa\n```\n";
        let achados = problemas(texto);
        assert_eq!(achados.len(), 1, "{achados:?}");
        assert_eq!(achados[0].0, 4);
    }

    #[test]
    fn joins_a_command_broken_with_backslashes() {
        let texto =
            "```console\n$ inter-pj extrato completo \\\n    --todas-paginas \\\n    --nada\n```\n";
        let achados = problemas(texto);
        assert_eq!(achados.len(), 1, "{achados:?}");
        assert_eq!(
            achados[0].0, 2,
            "reported at the line where the command starts"
        );
        assert!(achados[0].1.contains("--nada"), "{achados:?}");
    }

    #[test]
    fn ignores_what_is_outside_a_code_block_and_other_programs() {
        let texto = "\
Rode `inter-pj pix` para ver.

```console
$ export INTER_CLIENT_SECRET='x'
$ curl -H \"Authorization: Bearer $(inter-pj auth)\" https://exemplo.org
```

$ inter-pj pix
";
        assert_eq!(problemas(texto), []);
    }

    #[test]
    fn a_generated_page_is_not_checked() {
        let gerada = format!(
            "{}\n\n```console\n$ inter-pj pix\n```\n",
            paginas::cabecalho("x")
        );
        assert!(paginas::gerada(&gerada));
    }
}
