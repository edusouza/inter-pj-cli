//! The pages of the site that are copies of files of the repository: the changelog, the
//! security policy, the contribution guide, the architecture, the roadmap and the README of
//! the library.
//!
//! The files stay where they are, for whoever reads them on GitHub, and the site gets a copy
//! with the links rewritten: a link to a file that is a page of the site becomes a link
//! between pages, and any other link to the repository points to GitHub. Nothing generated is
//! committed (`site/.gitignore`), so a copy cannot be out of date.

use std::fs;
use std::path::Path;

pub(crate) const REPOSITORIO: &str = "https://github.com/edusouza/inter-pj-cli";
/// The branch the links to files of the repository point to.
const RAMO: &str = "main";

/// What every generated page starts with. `verificar` skips the pages that do, since the
/// commands they quote are not written for the CLI as it is today.
pub(crate) const MARCA: &str = "<!-- Gerado por `cargo run -p sitegen`";

/// A file of the repository that is published as a page of the site.
pub(crate) struct Derivada {
    /// Path from the root of the repository.
    pub(crate) origem: &'static str,
    /// Path from `site/conteudo/`.
    pub(crate) destino: &'static str,
    /// Replaces the first heading, when the one of the file makes no sense as a page title.
    pub(crate) titulo: Option<&'static str>,
}

pub(crate) const DERIVADAS: &[Derivada] = &[
    Derivada {
        origem: "CHANGELOG.md",
        destino: "changelog.md",
        titulo: None,
    },
    Derivada {
        origem: "SECURITY.md",
        destino: "referencia/seguranca.md",
        titulo: None,
    },
    Derivada {
        origem: "docs/arquitetura.md",
        destino: "por-dentro/arquitetura.md",
        titulo: None,
    },
    Derivada {
        origem: "docs/roadmap.md",
        destino: "por-dentro/roadmap.md",
        titulo: None,
    },
    Derivada {
        origem: "CONTRIBUTING.md",
        destino: "por-dentro/contribuir.md",
        titulo: None,
    },
    Derivada {
        origem: "crates/inter-pj/README.md",
        destino: "por-dentro/biblioteca.md",
        titulo: Some("A biblioteca Rust"),
    },
];

/// The comment on the first line of a generated page.
pub(crate) fn cabecalho(origem: &str) -> String {
    format!("{MARCA} a partir de {origem}; não edite, a próxima geração sobrescreve. -->")
}

/// Whether the text is a page that `sitegen` generated.
pub(crate) fn gerada(texto: &str) -> bool {
    texto.starts_with(MARCA)
}

/// Reads the source of the page and returns the page.
pub(crate) fn gerar(raiz: &Path, derivada: &Derivada) -> Result<String, String> {
    let caminho = raiz.join(derivada.origem);
    let texto = fs::read_to_string(&caminho)
        .map_err(|erro| format!("não consegui ler {}: {erro}", caminho.display()))?;
    Ok(converter(&texto, derivada, &|relativo| {
        raiz.join(relativo).is_dir()
    }))
}

/// Turns the source into a page. `eh_diretorio` says whether a path of the repository is a
/// directory, which GitHub serves under `tree/` and not under `blob/`.
fn converter(texto: &str, derivada: &Derivada, eh_diretorio: &dyn Fn(&str) -> bool) -> String {
    let mut saida = format!("{}\n\n", cabecalho(derivada.origem));
    let mut cerca: Option<Cerca> = None;
    let mut titulo_trocado = derivada.titulo.is_none();

    for linha in texto.lines() {
        if let Some(aberta) = &cerca {
            if aberta.fecha(linha) {
                cerca = None;
            } else if aberta.linguagem == "rust" && oculta_no_rustdoc(linha) {
                continue;
            }
            saida.push_str(linha);
            saida.push('\n');
        } else if let Some(nova) = Cerca::abre(linha) {
            saida.push_str(&nova.abertura);
            saida.push('\n');
            cerca = Some(nova);
        } else if !titulo_trocado && linha.starts_with("# ") {
            titulo_trocado = true;
            saida.push_str("# ");
            saida.push_str(derivada.titulo.unwrap_or_default());
            saida.push('\n');
        } else {
            saida.push_str(&reescrever_linha(linha, derivada, eh_diretorio));
            saida.push('\n');
        }
    }
    saida
}

/// A fenced code block.
struct Cerca {
    /// The opening line, with the info string reduced to the language.
    abertura: String,
    /// The fence character and how many of them opened it.
    marcador: char,
    tamanho: usize,
    linguagem: String,
}

impl Cerca {
    fn abre(linha: &str) -> Option<Self> {
        let sem_recuo = linha.trim_start();
        let marcador = sem_recuo
            .chars()
            .next()
            .filter(|c| matches!(c, '`' | '~'))?;
        let tamanho = sem_recuo.chars().take_while(|c| *c == marcador).count();
        if tamanho < 3 {
            return None;
        }
        let informacao = sem_recuo[tamanho..].trim();
        // `rust,no_run` is rustdoc's way to say how to test the example; the highlighter only
        // knows the language.
        let linguagem = informacao
            .split([',', ' '])
            .next()
            .unwrap_or_default()
            .to_string();
        let recuo = &linha[..linha.len() - sem_recuo.len()];
        let abertura = format!("{recuo}{}{linguagem}", marcador.to_string().repeat(tamanho));
        Some(Self {
            abertura,
            marcador,
            tamanho,
            linguagem,
        })
    }

    fn fecha(&self, linha: &str) -> bool {
        let sem_recuo = linha.trim();
        sem_recuo.chars().count() >= self.tamanho && sem_recuo.chars().all(|c| c == self.marcador)
    }
}

/// A line of a Rust example that rustdoc compiles but does not show (`# use ...;`).
fn oculta_no_rustdoc(linha: &str) -> bool {
    let sem_recuo = linha.trim_start();
    sem_recuo == "#" || sem_recuo.starts_with("# ")
}

/// Rewrites the targets of the links of a line: `[texto](alvo)` and `[rótulo]: alvo`.
fn reescrever_linha(
    linha: &str,
    derivada: &Derivada,
    eh_diretorio: &dyn Fn(&str) -> bool,
) -> String {
    let reescrever = |alvo: &str| reescrever_alvo(alvo, derivada, eh_diretorio);

    // A reference definition takes the whole line: the label ends at the first `]`, which
    // must be followed by a colon (`[^1]: ...` is a footnote, not a link).
    let sem_recuo = linha.trim_start();
    if sem_recuo.starts_with('[')
        && !sem_recuo.starts_with("[^")
        && let Some((rotulo, resto)) = sem_recuo.split_once(']')
        && let Some(resto) = resto.strip_prefix(':')
    {
        let resto = resto.trim_start();
        let (alvo, depois) = resto.split_once(char::is_whitespace).unwrap_or((resto, ""));
        let recuo = &linha[..linha.len() - sem_recuo.len()];
        let espaco = if depois.is_empty() { "" } else { " " };
        return format!("{recuo}{rotulo}]: {}{espaco}{depois}", reescrever(alvo));
    }

    let mut saida = String::with_capacity(linha.len());
    let mut resto = linha;
    while let Some(posicao) = resto.find(['`', ']']) {
        let (antes, a_partir) = resto.split_at(posicao);
        saida.push_str(antes);
        if a_partir.starts_with('`') {
            // An inline code span is copied as is: what looks like a link in it is an example.
            let crases = a_partir.chars().take_while(|c| *c == '`').count();
            let delimitador = "`".repeat(crases);
            let fim = a_partir[crases..]
                .find(&delimitador)
                .map_or(a_partir.len(), |i| crases + i + crases);
            saida.push_str(&a_partir[..fim]);
            resto = &a_partir[fim..];
        } else if let Some(depois) = a_partir.strip_prefix("](") {
            saida.push_str("](");
            let fim = fim_do_alvo(depois);
            saida.push_str(&reescrever(&depois[..fim]));
            resto = &depois[fim..];
        } else {
            saida.push(']');
            resto = &a_partir[1..];
        }
    }
    saida.push_str(resto);
    saida
}

/// Where the target of an inline link ends: at the space that precedes a title, or at the
/// parenthesis that closes the link.
fn fim_do_alvo(texto: &str) -> usize {
    let mut abertos = 0_usize;
    for (posicao, c) in texto.char_indices() {
        match c {
            '(' => abertos += 1,
            ')' if abertos == 0 => return posicao,
            ')' => abertos -= 1,
            c if c.is_whitespace() => return posicao,
            _ => {}
        }
    }
    texto.len()
}

fn reescrever_alvo(alvo: &str, derivada: &Derivada, eh_diretorio: &dyn Fn(&str) -> bool) -> String {
    if alvo.is_empty()
        || alvo.starts_with('#')
        || alvo.contains("://")
        || alvo.starts_with("mailto:")
    {
        return alvo.to_string();
    }
    let (caminho, ancora) = alvo
        .split_once('#')
        .map_or((alvo, String::new()), |(caminho, ancora)| {
            (caminho, format!("#{ancora}"))
        });
    let Some(resolvido) = resolver(derivada.origem, caminho) else {
        // It leaves the repository: nothing to point to.
        return alvo.to_string();
    };

    if let Some(pagina) = DERIVADAS.iter().find(|d| d.origem == resolvido) {
        return format!("{}{ancora}", relativo(derivada.destino, pagina.destino));
    }
    // A page written by hand lives in `site/conteudo/`, and the site is where it is read.
    if let Some(pagina) = resolvido.strip_prefix("site/conteudo/").filter(|p| {
        Path::new(p)
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("md"))
    }) {
        return format!("{}{ancora}", relativo(derivada.destino, pagina));
    }
    if resolvido.is_empty() {
        return format!("{REPOSITORIO}{ancora}");
    }
    let tipo = if eh_diretorio(&resolvido) {
        "tree"
    } else {
        "blob"
    };
    format!("{REPOSITORIO}/{tipo}/{RAMO}/{resolvido}{ancora}")
}

/// The path of `alvo`, written relative to the directory of the file `origem`, from the
/// root of the repository. `None` if it climbs out of the repository.
fn resolver(origem: &str, alvo: &str) -> Option<String> {
    let mut partes: Vec<&str> = origem.split('/').collect();
    partes.pop();
    for parte in alvo.split('/') {
        match parte {
            "" | "." => {}
            ".." => {
                partes.pop()?;
            }
            outra => partes.push(outra),
        }
    }
    Some(partes.join("/"))
}

/// The link from the page `de` to the page `para`, both paths from `site/conteudo/`.
fn relativo(de: &str, para: &str) -> String {
    let de: Vec<&str> = de.split('/').collect();
    let para: Vec<&str> = para.split('/').collect();
    let (pasta_de, pasta_para) = (&de[..de.len() - 1], &para[..para.len() - 1]);
    let comum = pasta_de
        .iter()
        .zip(pasta_para)
        .take_while(|(a, b)| a == b)
        .count();
    let mut caminho = "../".repeat(pasta_de.len() - comum);
    caminho.push_str(&para[comum..].join("/"));
    caminho
}

#[cfg(test)]
mod testes {
    use super::*;

    const CONTRIBUIR: Derivada = Derivada {
        origem: "CONTRIBUTING.md",
        destino: "por-dentro/contribuir.md",
        titulo: None,
    };
    const ARQUITETURA: Derivada = Derivada {
        origem: "docs/arquitetura.md",
        destino: "por-dentro/arquitetura.md",
        titulo: None,
    };

    fn converte(derivada: &Derivada, texto: &str) -> String {
        let pagina = converter(texto, derivada, &|caminho| {
            caminho == "spec" || caminho == "crates/inter-pj"
        });
        pagina
            .strip_prefix(&format!("{}\n\n", cabecalho(derivada.origem)))
            .unwrap()
            .to_string()
    }

    #[test]
    fn a_page_says_it_is_generated_and_from_what() {
        let pagina = converter("# T\n", &CONTRIBUIR, &|_| false);
        assert!(gerada(&pagina));
        assert!(
            pagina
                .starts_with("<!-- Gerado por `cargo run -p sitegen` a partir de CONTRIBUTING.md;")
        );
        assert!(!gerada("# Uma página escrita à mão\n"));
    }

    #[test]
    fn a_link_to_another_page_of_the_site_becomes_a_link_between_pages() {
        let texto = "ver [o roadmap](docs/roadmap.md) e [o guia](CONTRIBUTING.md#testes)\n";
        let saida = converte(&CONTRIBUIR, texto);
        assert_eq!(
            saida,
            "ver [o roadmap](roadmap.md) e [o guia](contribuir.md#testes)\n"
        );
    }

    #[test]
    fn links_between_pages_in_different_folders_climb_to_the_common_one() {
        let saida = converte(
            &ARQUITETURA,
            "[segurança](../SECURITY.md) e [versões](../CHANGELOG.md#x)\n",
        );
        assert_eq!(
            saida,
            "[segurança](../referencia/seguranca.md) e [versões](../changelog.md#x)\n"
        );
    }

    #[test]
    fn a_link_to_any_other_file_points_to_github() {
        let saida = converte(
            &ARQUITETURA,
            "[o registro](../crates/inter-pj/src/endpoint.rs)\n",
        );
        assert_eq!(
            saida,
            "[o registro](https://github.com/edusouza/inter-pj-cli/blob/main/crates/inter-pj/src/endpoint.rs)\n"
        );
    }

    #[test]
    fn a_link_to_a_directory_points_to_tree_and_to_the_root_to_the_repository() {
        let saida = converte(
            &CONTRIBUIR,
            "[spec](spec/) e [crate](crates/inter-pj) e [raiz](./)\n",
        );
        assert_eq!(
            saida,
            "[spec](https://github.com/edusouza/inter-pj-cli/tree/main/spec) e \
             [crate](https://github.com/edusouza/inter-pj-cli/tree/main/crates/inter-pj) e \
             [raiz](https://github.com/edusouza/inter-pj-cli)\n"
        );
    }

    #[test]
    fn a_link_to_a_page_written_by_hand_becomes_a_link_between_pages() {
        let saida = converte(
            &CONTRIBUIR,
            "[como](site/conteudo/por-dentro/desenvolvimento.md#este-site) e [o índice](site/conteudo/index.md) e [o yml](site/mkdocs.yml)\n",
        );
        assert_eq!(
            saida,
            "[como](desenvolvimento.md#este-site) e [o índice](../index.md) e \
             [o yml](https://github.com/edusouza/inter-pj-cli/blob/main/site/mkdocs.yml)\n"
        );
    }

    #[test]
    fn urls_anchors_and_mail_are_left_alone() {
        let texto = "[a](https://exemplo.org/x) [b](#secao) [c](mailto:a@b.c)\n";
        assert_eq!(converte(&CONTRIBUIR, texto), texto);
    }

    #[test]
    fn a_link_that_leaves_the_repository_is_left_alone() {
        let texto = "[fora](../../outro.md)\n";
        assert_eq!(converte(&CONTRIBUIR, texto), texto);
    }

    #[test]
    fn a_title_after_the_target_and_parentheses_inside_it_survive() {
        let saida = converte(
            &CONTRIBUIR,
            "[x](docs/roadmap.md \"título\") [y](a_(b).md)\n",
        );
        assert_eq!(
            saida,
            "[x](roadmap.md \"título\") [y](https://github.com/edusouza/inter-pj-cli/blob/main/a_(b).md)\n"
        );
    }

    #[test]
    fn reference_definitions_are_rewritten() {
        let saida = converte(
            &CONTRIBUIR,
            "[roadmap]: docs/roadmap.md\n[nota]: SECURITY.md \"t\"\n[^1]: nota\n",
        );
        assert_eq!(
            saida,
            "[roadmap]: roadmap.md\n[nota]: ../referencia/seguranca.md \"t\"\n[^1]: nota\n"
        );
    }

    #[test]
    fn a_colon_after_a_later_bracket_does_not_make_a_definition() {
        let saida = converte(&CONTRIBUIR, "[a](SECURITY.md) e [b]: texto\n");
        assert_eq!(saida, "[a](../referencia/seguranca.md) e [b]: texto\n");
    }

    #[test]
    fn code_is_not_rewritten() {
        let texto = "no código `[x](docs/roadmap.md)` e `` [y](CHANGELOG.md) ``\n\
                     ```console\n$ echo '[z](docs/roadmap.md)'\n```\n\
                     ~~~\n[w](docs/roadmap.md)\n~~~\n";
        assert_eq!(converte(&CONTRIBUIR, texto), texto);
    }

    #[test]
    fn a_longer_fence_is_not_closed_by_a_shorter_one() {
        let texto = "````text\n```\n[x](docs/roadmap.md)\n```\n````\n[y](docs/roadmap.md)\n";
        assert_eq!(
            converte(&CONTRIBUIR, texto),
            "````text\n```\n[x](docs/roadmap.md)\n```\n````\n[y](roadmap.md)\n"
        );
    }

    #[test]
    fn a_rust_example_keeps_the_language_and_loses_what_rustdoc_hides() {
        let texto = "```rust,no_run\n# async fn exemplo() {\nlet x = 1; // não oculta\n#[derive(Debug)]\n#\n# Ok(())\n```\n";
        assert_eq!(
            converte(&CONTRIBUIR, texto),
            "```rust\nlet x = 1; // não oculta\n#[derive(Debug)]\n```\n"
        );
    }

    #[test]
    fn a_hash_line_outside_rust_is_kept() {
        let texto = "```console\n# um comentário\n```\n";
        assert_eq!(converte(&CONTRIBUIR, texto), texto);
    }

    #[test]
    fn the_title_can_be_replaced() {
        let derivada = Derivada {
            origem: "README.md",
            destino: "x.md",
            titulo: Some("Outro nome"),
        };
        let saida = converte(
            &derivada,
            "# inter-pj\n\ntexto\n\n```text\n# não é título\n```\n",
        );
        assert_eq!(
            saida,
            "# Outro nome\n\ntexto\n\n```text\n# não é título\n```\n"
        );
    }

    #[test]
    fn relative_paths_between_pages() {
        assert_eq!(
            relativo("por-dentro/a.md", "changelog.md"),
            "../changelog.md"
        );
        assert_eq!(relativo("por-dentro/a.md", "por-dentro/b.md"), "b.md");
        assert_eq!(
            relativo("changelog.md", "por-dentro/b.md"),
            "por-dentro/b.md"
        );
        assert_eq!(
            relativo("referencia/a.md", "por-dentro/b.md"),
            "../por-dentro/b.md"
        );
    }

    #[test]
    fn every_derived_page_has_its_own_source_and_destination() {
        let mut origens: Vec<_> = DERIVADAS.iter().map(|d| d.origem).collect();
        let mut destinos: Vec<_> = DERIVADAS.iter().map(|d| d.destino).collect();
        origens.sort_unstable();
        destinos.sort_unstable();
        origens.dedup();
        destinos.dedup();
        assert_eq!(origens.len(), DERIVADAS.len());
        assert_eq!(destinos.len(), DERIVADAS.len());
    }
}
