//! The command tree of the CLI, read from the `--help` of the built binary, and the command
//! reference page rendered from it.
//!
//! Reading the help, instead of restating the commands by hand, is what keeps the reference
//! from going stale: a command or option added to the CLI shows up on the next build.

use std::fmt::Write as _;
use std::path::Path;
use std::process::{Command as Processo, Stdio};

use crate::paginas;

/// The section clap prints in every command with the options that apply to all of them.
const SECAO_GLOBAIS: &str = "Opções globais:";

/// Runs the binary with the given arguments and returns what it printed to stdout.
type Executor<'a> = &'a dyn Fn(&[String]) -> Result<String, String>;

/// An option of a command, as listed in its help.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Opcao {
    /// `perfil` for `--perfil`.
    pub(crate) longa: Option<String>,
    /// `p` for `-p`.
    pub(crate) curta: Option<char>,
    /// Whether it takes a value (`--perfil <NOME>`) or is a flag (`--json`).
    pub(crate) com_valor: bool,
}

/// A command, or the root, as `inter-pj ... --help` describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Comando {
    /// Path from the root: empty for the root, `["extrato", "completo"]` for a nested command.
    pub(crate) caminho: Vec<String>,
    /// The first line of the help: what the command does.
    pub(crate) descricao: String,
    /// The help as the terminal shows it. Only the root keeps the global options.
    pub(crate) texto: String,
    /// Its own options and the global ones: clap lists both in every command.
    pub(crate) opcoes: Vec<Opcao>,
    /// Whether the usage line asks for a subcommand (`<COMANDO>`, not `[COMANDO]`).
    pub(crate) exige_subcomando: bool,
    pub(crate) subcomandos: Vec<Comando>,
}

impl Comando {
    /// `inter-pj extrato completo`.
    pub(crate) fn nome(&self) -> String {
        let mut nome = String::from("inter-pj");
        for parte in &self.caminho {
            nome.push(' ');
            nome.push_str(parte);
        }
        nome
    }

    /// The id of the section of this command in the reference page.
    fn ancora(&self) -> String {
        if self.caminho.is_empty() {
            "inter-pj".to_string()
        } else {
            self.caminho.join("-")
        }
    }
}

/// The whole CLI.
#[derive(Debug)]
pub(crate) struct Arvore {
    /// `0.2.0`, from `inter-pj --version`.
    pub(crate) versao: String,
    pub(crate) raiz: Comando,
}

/// Reads the command tree by running the built binary.
pub(crate) fn carregar(binario: &Path) -> Result<Arvore, String> {
    carregar_com(&|argumentos| executar(binario, argumentos))
}

fn carregar_com(executar: Executor<'_>) -> Result<Arvore, String> {
    let saida = executar(&["--version".to_string()])?;
    let saida = saida.trim();
    let versao = saida.strip_prefix("inter-pj ").unwrap_or(saida).to_string();
    Ok(Arvore {
        versao,
        raiz: ler(&[], executar)?,
    })
}

fn ler(caminho: &[String], executar: Executor<'_>) -> Result<Comando, String> {
    let mut argumentos = caminho.to_vec();
    argumentos.push("--help".to_string());
    let saida = executar(&argumentos)?;
    let secoes = secoes(&saida);

    let mut subcomandos = Vec::new();
    for nome in nomes_dos_subcomandos(&secoes) {
        let mut filho = caminho.to_vec();
        filho.push(nome);
        subcomandos.push(ler(&filho, executar)?);
    }

    let texto = if caminho.is_empty() {
        saida.clone()
    } else {
        sem_globais(&saida)
    };
    Ok(Comando {
        caminho: caminho.to_vec(),
        descricao: saida.lines().next().unwrap_or_default().trim().to_string(),
        texto,
        opcoes: opcoes(&secoes),
        exige_subcomando: saida
            .lines()
            .any(|l| l.starts_with("Uso:") && l.contains(" <COMANDO>")),
        subcomandos,
    })
}

/// Runs the binary the way a script would: no terminal (so clap does not pick up its width,
/// which would change the wrapping of the text), no colors, and none of the `INTER_*`
/// variables, which clap prints next to the options they feed.
fn executar(binario: &Path, argumentos: &[String]) -> Result<String, String> {
    let mut comando = Processo::new(binario);
    comando
        .args(argumentos)
        .stdin(Stdio::null())
        .env("NO_COLOR", "1");
    for (nome, _) in std::env::vars_os() {
        if nome.to_string_lossy().starts_with("INTER_") {
            comando.env_remove(nome);
        }
    }
    let saida = comando.output().map_err(|erro| {
        format!(
            "não consegui executar {}: {erro}; compile o binário antes, com `cargo build -p inter-pj-cli`",
            binario.display()
        )
    })?;
    if !saida.status.success() {
        return Err(format!(
            "`inter-pj {}` terminou com {}: {}",
            argumentos.join(" "),
            saida.status,
            String::from_utf8_lossy(&saida.stderr).trim()
        ));
    }
    let texto = String::from_utf8(saida.stdout)
        .map_err(|_| format!("a saída de `inter-pj {}` não é UTF-8", argumentos.join(" ")))?;
    Ok(texto.replace("\r\n", "\n"))
}

/// A section of a help: the unindented `Título:` line and the indented lines under it.
struct Secao<'a> {
    titulo: &'a str,
    linhas: Vec<&'a str>,
}

fn secoes(texto: &str) -> Vec<Secao<'_>> {
    let mut secoes = Vec::new();
    let mut atual: Option<Secao<'_>> = None;
    for linha in texto.lines() {
        if linha.starts_with(' ') {
            if let Some(secao) = atual.as_mut() {
                secao.linhas.push(linha);
            }
        } else if !linha.is_empty() {
            secoes.extend(atual.take());
            if linha.ends_with(':') {
                atual = Some(Secao {
                    titulo: linha,
                    linhas: Vec::new(),
                });
            }
        }
    }
    secoes.extend(atual);
    secoes
}

fn recuo(linha: &str) -> usize {
    linha.len() - linha.trim_start().len()
}

fn nomes_dos_subcomandos(secoes: &[Secao<'_>]) -> Vec<String> {
    secoes
        .iter()
        .filter(|secao| secao.titulo == "Comandos:")
        .flat_map(|secao| &secao.linhas)
        // The names are indented by two; the descriptions that wrap, by much more.
        .filter(|linha| recuo(linha) == 2)
        .filter_map(|linha| linha.split_whitespace().next())
        .filter(|nome| *nome != "help")
        .map(str::to_string)
        .collect()
}

fn opcoes(secoes: &[Secao<'_>]) -> Vec<Opcao> {
    secoes
        .iter()
        .filter(|secao| secao.titulo.starts_with("Opções"))
        .flat_map(|secao| &secao.linhas)
        .filter_map(|linha| opcao(linha))
        .collect()
}

/// `  -p, --perfil <NOME>   Perfil...` or `      --json   Atalho...`; the lines that wrap
/// a description are indented deeper than any option.
fn opcao(linha: &str) -> Option<Opcao> {
    let corpo = linha.trim_start();
    if recuo(linha) > 6 || !corpo.starts_with('-') {
        return None;
    }
    // The description is separated from the option by two spaces or more.
    let especificacao = corpo.split("  ").next()?;
    let mut opcao = Opcao {
        longa: None,
        curta: None,
        com_valor: false,
    };
    for parte in especificacao.split(", ") {
        let mut palavras = parte.split_whitespace();
        let nome = palavras.next()?;
        if palavras.next().is_some() {
            opcao.com_valor = true;
        }
        if let Some(longa) = nome.strip_prefix("--") {
            opcao.longa = Some(longa.trim_end_matches('.').to_string());
        } else if let Some(curta) = nome.strip_prefix('-') {
            opcao.curta = curta.chars().next();
        }
    }
    Some(opcao)
}

/// The help without the section of global options, which the reference shows once.
fn sem_globais(texto: &str) -> String {
    let mut saida = String::new();
    let mut pulando = false;
    for linha in texto.lines() {
        if linha == SECAO_GLOBAIS {
            pulando = true;
            continue;
        }
        if pulando {
            if linha.is_empty() || linha.starts_with(' ') {
                continue;
            }
            pulando = false;
        }
        saida.push_str(linha);
        saida.push('\n');
    }
    format!("{}\n", saida.trim_end())
}

/// The commands in the order of the help, each followed by its subcommands.
fn em_ordem(comando: &Comando) -> Vec<&Comando> {
    let mut todos = vec![comando];
    for filho in &comando.subcomandos {
        todos.extend(em_ordem(filho));
    }
    todos
}

/// The page `referencia/comandos.md`.
pub(crate) fn pagina(arvore: &Arvore) -> String {
    let comandos = em_ordem(&arvore.raiz);
    let mut pagina = String::new();
    let _ = writeln!(pagina, "{}\n", paginas::cabecalho("`inter-pj --help`"));
    let _ = writeln!(
        pagina,
        "# Comandos\n\n\
         Esta página é gerada a partir da ajuda do próprio programa, a de `inter-pj --help` e \
         a de cada `inter-pj <comando> --help`, na versão **{}**. O que está aqui é o que o \
         terminal mostra. Para conferir a versão que você instalou, rode `inter-pj --version`.\n\n\
         As **opções globais**, as de [`inter-pj`](#inter-pj), valem para todos os comandos e \
         podem vir antes ou depois deles. Elas não se repetem nas seções abaixo.\n\n\
         ## Todos os comandos\n\n\
         | Comando | O que faz |\n| --- | --- |",
        arvore.versao
    );
    for comando in comandos.iter().skip(1) {
        let _ = writeln!(
            pagina,
            "| [`{}`](#{}) | {} |",
            comando.nome(),
            comando.ancora(),
            comando.descricao.replace('|', "\\|")
        );
    }
    for comando in comandos {
        // `##` for `inter-pj` and for the commands of the first level, one more `#` for each
        // level below.
        let nivel = "#".repeat((comando.caminho.len() + 1).clamp(2, 6));
        let _ = write!(
            pagina,
            "\n{nivel} `{}` {{ #{} }}\n\n```text\n{}```\n",
            comando.nome(),
            comando.ancora(),
            comando.texto
        );
    }
    pagina
}

/// A help in the format clap prints, trimmed down, for the tests of this and of the other
/// modules.
#[cfg(test)]
pub(crate) mod exemplo {
    use super::{Arvore, carregar_com};

    const RAIZ: &str = "\
CLI não oficial para a conta PJ do Inter Empresas

Uso: inter-pj [OPÇÕES] <COMANDO>

Comandos:
  saldo    Consulta o saldo da conta
  extrato  Extrato da conta: movimentações de um período
  config   Arquivo de configuração e perfis

Opções globais:
  -p, --perfil <NOME>            Perfil do arquivo de configuração [env: INTER_PERFIL]
      --conta-corrente <NUMERO>  Conta corrente (só quando a integração tem mais de uma conta)
                                 [env: INTER_CONTA_CORRENTE]
      --json                     Atalho para --formato json
  -v, --verbose...               Mostra detalhes da execução em stderr
  -h, --help                     Mostra esta ajuda
  -V, --version                  Mostra a versão

Credenciais:
  O client_secret é lido da variável de ambiente INTER_CLIENT_SECRET.

Projeto não oficial.
";

    const GLOBAIS: &str = "\
Opções globais:
  -p, --perfil <NOME>            Perfil do arquivo de configuração [env: INTER_PERFIL]
      --conta-corrente <NUMERO>  Conta corrente (só quando a integração tem mais de uma conta)
                                 [env: INTER_CONTA_CORRENTE]
      --json                     Atalho para --formato json
  -v, --verbose...               Mostra detalhes da execução em stderr
  -h, --help                     Mostra esta ajuda
";

    fn ajuda(descricao: &str, uso: &str, miolo: &str) -> String {
        format!("{descricao}\n\nUso: inter-pj {uso}\n\n{miolo}\n{GLOBAIS}")
    }

    fn saida(argumentos: &[String]) -> Result<String, String> {
        let argumentos: Vec<&str> = argumentos.iter().map(String::as_str).collect();
        Ok(match argumentos.as_slice() {
            ["--version"] => "inter-pj 0.2.0\n".to_string(),
            ["--help"] => RAIZ.to_string(),
            ["saldo", "--help"] => ajuda(
                "Consulta o saldo da conta",
                "saldo [OPÇÕES]",
                "Opções:\n      --data <AAAA-MM-DD>  Data da consulta (AAAA-MM-DD). Sem data: saldo\n                           atual\n",
            ),
            ["extrato", "--help"] => ajuda(
                "Extrato da conta: movimentações de um período",
                "extrato [OPÇÕES] [COMANDO]",
                "Comandos:\n  completo  Extrato enriquecido\n\nOpções:\n      --inicio <AAAA-MM-DD>  Primeiro dia\n      --dividir-periodo      Divide períodos maiores que 90 dias\n",
            ),
            ["extrato", "completo", "--help"] => ajuda(
                "Extrato enriquecido",
                "extrato completo [OPÇÕES]",
                "Opções:\n      --todas-paginas  Busca todas as páginas\n  -o, --saida <ARQUIVO>  Arquivo a gravar\n",
            ),
            ["config", "--help"] => ajuda(
                "Arquivo de configuração e perfis",
                "config [OPÇÕES] <COMANDO>",
                "Comandos:\n  init  Cria o arquivo de configuração\n",
            ),
            ["config", "init", "--help"] => ajuda(
                "Cria o arquivo de configuração",
                "config init [OPÇÕES]",
                "Opções:\n      --forcar  Sobrescreve o arquivo\n",
            ),
            outros => return Err(format!("comando inesperado: {outros:?}")),
        })
    }

    /// The tree of the sample CLI above.
    pub(crate) fn arvore() -> Arvore {
        carregar_com(&saida).expect("a ajuda de exemplo é válida")
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn reads_the_tree_with_versions_commands_and_options() {
        let arvore = exemplo::arvore();
        assert_eq!(arvore.versao, "0.2.0");

        let raiz = &arvore.raiz;
        let nomes: Vec<_> = raiz.subcomandos.iter().map(Comando::nome).collect();
        assert_eq!(
            nomes,
            ["inter-pj saldo", "inter-pj extrato", "inter-pj config"]
        );
        assert!(raiz.exige_subcomando);

        let extrato = &raiz.subcomandos[1];
        assert!(!extrato.exige_subcomando, "`[COMANDO]` is optional");
        assert_eq!(extrato.subcomandos[0].nome(), "inter-pj extrato completo");
        assert_eq!(
            extrato.descricao,
            "Extrato da conta: movimentações de um período"
        );
    }

    #[test]
    fn tells_options_with_a_value_from_flags() {
        let arvore = exemplo::arvore();
        let busca = |longa: &str| {
            arvore
                .raiz
                .opcoes
                .iter()
                .find(|o| o.longa.as_deref() == Some(longa))
                .cloned()
        };
        let perfil = busca("perfil").expect("--perfil");
        assert_eq!(perfil.curta, Some('p'));
        assert!(perfil.com_valor);
        assert!(!busca("json").expect("--json").com_valor);
        let verbose = busca("verbose").expect("--verbose");
        assert_eq!(verbose.curta, Some('v'));
        assert!(
            !verbose.com_valor,
            "the `...` of a repeatable flag is not a value"
        );
        assert!(busca("conta-corrente").expect("--conta-corrente").com_valor);
    }

    #[test]
    fn a_wrapped_description_is_not_an_option() {
        let arvore = exemplo::arvore();
        let com_env = arvore
            .raiz
            .opcoes
            .iter()
            .filter(|o| o.longa.is_none())
            .count();
        assert_eq!(com_env, 0);
        // 6 options in the global section; `[env: ...]` on the continuation line is no option.
        assert_eq!(arvore.raiz.opcoes.len(), 6);
    }

    #[test]
    fn a_command_lists_its_own_options_and_the_global_ones() {
        let arvore = exemplo::arvore();
        let completo = &arvore.raiz.subcomandos[1].subcomandos[0];
        let longas: Vec<_> = completo
            .opcoes
            .iter()
            .filter_map(|o| o.longa.as_deref())
            .collect();
        assert!(longas.contains(&"todas-paginas"));
        assert!(longas.contains(&"saida"));
        assert!(
            longas.contains(&"perfil"),
            "the global options apply to every command"
        );
    }

    #[test]
    fn only_the_root_keeps_the_global_options_in_its_text() {
        let arvore = exemplo::arvore();
        assert!(arvore.raiz.texto.contains("Opções globais:"));
        assert!(arvore.raiz.texto.contains("Credenciais:"));
        for comando in &arvore.raiz.subcomandos {
            assert!(
                !comando.texto.contains("Opções globais:"),
                "{}",
                comando.nome()
            );
            assert!(!comando.texto.contains("--perfil"), "{}", comando.nome());
        }
        let saldo = &arvore.raiz.subcomandos[0].texto;
        assert!(saldo.contains("--data <AAAA-MM-DD>"));
        assert!(
            saldo.ends_with("atual\n"),
            "no blank line left at the end: {saldo:?}"
        );
    }

    #[test]
    fn dropping_the_globals_keeps_the_sections_that_come_after() {
        let texto = "Descrição\n\nOpções:\n  --a  A\n\nOpções globais:\n  --b  B\n\nExemplos:\n  inter-pj x\n";
        assert_eq!(
            sem_globais(texto),
            "Descrição\n\nOpções:\n  --a  A\n\nExemplos:\n  inter-pj x\n"
        );
    }

    #[test]
    fn the_page_has_an_index_and_a_section_per_command() {
        let pagina = pagina(&exemplo::arvore());
        assert!(pagina.starts_with(crate::paginas::MARCA));
        assert!(pagina.contains("na versão **0.2.0**"));
        assert!(pagina.contains("| [`inter-pj saldo`](#saldo) | Consulta o saldo da conta |"));
        assert!(pagina.contains(
            "| [`inter-pj extrato completo`](#extrato-completo) | Extrato enriquecido |"
        ));
        assert!(pagina.contains("## `inter-pj` { #inter-pj }"));
        assert!(pagina.contains("## `inter-pj extrato` { #extrato }"));
        assert!(pagina.contains("### `inter-pj extrato completo` { #extrato-completo }"));
        assert!(pagina.contains("### `inter-pj config init` { #config-init }"));
        // The global options are in the section of the root and nowhere else.
        assert_eq!(pagina.matches("--perfil <NOME>").count(), 1);
    }

    #[test]
    fn a_failing_run_is_reported_with_the_command() {
        let erro = carregar_com(&|_| Err("falhou".to_string())).unwrap_err();
        assert_eq!(erro, "falhou");
    }
}
