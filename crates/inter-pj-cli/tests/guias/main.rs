//! The examples of the guides, run. Every `console` block of `docs/guias`,
//! `docs/receitas.md` and `docs/faq.md` is a session in a terminal: each
//! command runs against a mock of the API ([`banco`]) that has the data the
//! guides show, and must print what follows it in the guide (stdout and
//! stderr, in the order a terminal shows them) and succeed, or fail when the
//! guide shows an `erro:`.
//!
//! - A line `Pergunta? [s/N] s` is the answer to a confirmation: the command
//!   runs with `--sim`, which prints the same without the question.
//! - `<!-- guia: saída ilustrativa -->` before a block runs its commands
//!   without comparing what they print (the statement of the last 30 days,
//!   the days left of a certificate).
//! - `<!-- guia: não executar -->` before a block skips it (installation, a
//!   pipe to another program).
//! - The keys and txids the CLI generates match any other of the same shape,
//!   the same one wherever the guide repeats it.
//! - No command, in any block, may print the `client_secret`.
//!
//! The session is a home of its own (`/home/voce` in the guides), in Brasília
//! time, with the profiles `padrao` (production) and `sandbox`, whose
//! certificate and key are in `~/inter`. Every name, document and amount of
//! the mock is synthetic.
//!
//! `ATUALIZAR_GUIAS=1 cargo test --test guias` writes in the guides what the
//! commands print, for a new example or a change in the output; review the
//! diff before committing.

#![cfg(unix)]

mod banco;
mod markdown;
mod sessao;

use std::fs;
use std::path::{Path, PathBuf};

use markdown::{Diretiva, Guia};
use sessao::Sessao;

/// The files with examples, from the root of the repository.
fn guias(raiz: &Path) -> Vec<PathBuf> {
    let mut arquivos: Vec<PathBuf> = fs::read_dir(raiz.join("docs/guias"))
        .map(|entradas| {
            entradas
                .map(|entrada| entrada.unwrap().path())
                .filter(|caminho| caminho.extension().is_some_and(|ext| ext == "md"))
                .collect()
        })
        .unwrap_or_default();
    arquivos.sort();
    for avulso in ["docs/receitas.md", "docs/faq.md"] {
        let caminho = raiz.join(avulso);
        if caminho.exists() {
            arquivos.push(caminho);
        }
    }
    arquivos
}

#[tokio::test(flavor = "multi_thread")]
async fn os_exemplos_dos_guias_funcionam() {
    let raiz = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let atualizar = std::env::var_os("ATUALIZAR_GUIAS").is_some();
    let arquivos = guias(&raiz);
    assert!(!arquivos.is_empty(), "nenhum guia em {}", raiz.display());

    let mut falhas = Vec::new();
    let mut comandos = 0;
    for arquivo in &arquivos {
        let nome = arquivo.strip_prefix(&raiz).unwrap().display().to_string();
        let mut guia = Guia::ler(arquivo);
        // Each guide from scratch: its own bank and its own home.
        let banco = banco::Banco::novo().await;
        let sessao = Sessao::nova(&banco.uri());
        for bloco in &mut guia.blocos {
            if bloco.diretiva == Diretiva::NaoExecutar {
                continue;
            }
            for comando in &mut bloco.comandos {
                comandos += 1;
                let execucao = sessao.executar(comando);
                let erro = if bloco.diretiva == Diretiva::Ilustrativa {
                    comando.conferir_so_o_resultado(&execucao)
                } else {
                    comando.conferir(&execucao)
                };
                if let Some(erro) = erro {
                    falhas.push(format!("{nome}:{}: {erro}", comando.linha));
                }
                // No example, whatever it shows, may print the secret.
                assert!(
                    !execucao.saida.contains(sessao::CLIENT_SECRET),
                    "{nome}:{}: `{}` imprimiu o client_secret",
                    comando.linha,
                    comando.texto
                );
                if atualizar && bloco.diretiva == Diretiva::Conferir {
                    comando.atualizar(&execucao);
                }
            }
        }
        if atualizar {
            guia.gravar(arquivo);
        }
    }
    assert!(comandos > 0, "nenhum exemplo nos guias");
    assert!(
        falhas.is_empty() || atualizar,
        "{} de {comandos} exemplos dos guias não conferem (ATUALIZAR_GUIAS=1 grava o que os comandos imprimem):\n\n{}",
        falhas.len(),
        falhas.join("\n\n")
    );
}
