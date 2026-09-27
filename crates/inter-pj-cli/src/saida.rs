//! Documents written by the CLI (PDFs, images): a private file that is
//! never overwritten by accident, or the standard output.

use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

use crate::error::CliError;
use crate::files::{create_private, write_private};

/// Where a document goes.
#[derive(Debug)]
pub(crate) struct Saida {
    caminho: PathBuf,
    sobrescrever: bool,
}

impl Saida {
    /// `-` is the standard output.
    pub(crate) fn new(caminho: PathBuf, sobrescrever: bool) -> Self {
        Self {
            caminho,
            sobrescrever,
        }
    }

    pub(crate) fn caminho(&self) -> &Path {
        &self.caminho
    }

    pub(crate) fn stdout(&self) -> bool {
        self.caminho.as_os_str() == "-"
    }

    /// Checks, before calling the API, what would make the write fail: the
    /// standard output is a terminal (binary content would garble it), or
    /// the file already exists. The write itself is checked again, atomically.
    pub(crate) fn conferir(&self) -> Result<(), CliError> {
        if self.stdout() && io::stdout().is_terminal() {
            return Err(CliError::Usage(format!(
                "a saída padrão é um terminal: redirecione (> arquivo.{}) ou use --saida ARQUIVO",
                self.extensao()
            )));
        }
        if !self.stdout() && !self.sobrescrever && self.caminho.exists() {
            return Err(self.ja_existe());
        }
        Ok(())
    }

    /// Writes the document: to the standard output, or to a file with
    /// permission 600 that is replaced only with `sobrescrever`.
    pub(crate) fn gravar(&self, bytes: &[u8]) -> Result<(), CliError> {
        if self.stdout() {
            let mut stdout = io::stdout().lock();
            return match stdout.write_all(bytes).and_then(|()| stdout.flush()) {
                Err(err) if err.kind() != io::ErrorKind::BrokenPipe => {
                    Err(CliError::io("falha ao escrever na saída padrão", err))
                }
                _ => Ok(()),
            };
        }
        let gravado = if self.sobrescrever {
            write_private(&self.caminho, bytes)
        } else {
            create_private(&self.caminho, bytes)
        };
        gravado.map_err(|err| match err.kind() {
            io::ErrorKind::AlreadyExists => self.ja_existe(),
            _ => CliError::io(format!("falha ao gravar {}", self.caminho.display()), err),
        })
    }

    fn ja_existe(&self) -> CliError {
        CliError::Usage(format!(
            "o arquivo {} já existe; use --sobrescrever para substituí-lo",
            self.caminho.display()
        ))
    }

    fn extensao(&self) -> &str {
        self.caminho
            .extension()
            .and_then(|extensao| extensao.to_str())
            .unwrap_or("pdf")
    }
}

/// `12,3 KB`.
pub(crate) fn tamanho(bytes: usize) -> String {
    #[allow(clippy::cast_precision_loss)] // display only
    let kb = bytes as f64 / 1024.0;
    if kb < 1024.0 {
        format!("{kb:.1} KB").replace('.', ",")
    } else {
        format!("{:.1} MB", kb / 1024.0).replace('.', ",")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_are_not_overwritten_by_accident() {
        let dir = tempfile::tempdir().unwrap();
        let caminho = dir.path().join("doc.pdf");
        let saida = Saida::new(caminho.clone(), false);
        saida.conferir().unwrap();
        saida.gravar(b"%PDF-1").unwrap();
        let erro = saida.conferir().unwrap_err().to_string();
        assert!(
            erro.ends_with("já existe; use --sobrescrever para substituí-lo"),
            "{erro}"
        );
        assert!(saida.gravar(b"%PDF-2").is_err());
        assert_eq!(std::fs::read(&caminho).unwrap(), b"%PDF-1");

        let sobrescrever = Saida::new(caminho.clone(), true);
        sobrescrever.conferir().unwrap();
        sobrescrever.gravar(b"%PDF-2").unwrap();
        assert_eq!(std::fs::read(&caminho).unwrap(), b"%PDF-2");
    }

    #[test]
    fn sizes_are_readable() {
        assert_eq!(tamanho(1536), "1,5 KB");
        assert_eq!(tamanho(3 * 1024 * 1024), "3,0 MB");
    }
}
