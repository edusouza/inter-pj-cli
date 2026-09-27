//! Writing files that may hold sensitive data (configuration, tokens,
//! statements): readable by the owner only.
//!
//! A file is never written through a path that already exists: a new one
//! is created exclusively (`O_EXCL`, which a symbolic link also fails), and
//! a replacement goes to a new file in the same directory, renamed over the
//! old one. A link at the path, which another user could point at a file of
//! yours in a shared directory, is replaced and never followed, and a
//! failure halfway leaves the old file as it was.

use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

/// Writes `contents` to `path` with mode 600, replacing the file (or the
/// symbolic link) if it exists.
pub(crate) fn write_private(path: &Path, contents: &[u8]) -> io::Result<()> {
    replace(path, contents, true)
}

/// Like [`write_private`], with the permissions of the directory's other
/// files (the umask's): for what is public, like the man pages.
pub(crate) fn write_public(path: &Path, contents: &[u8]) -> io::Result<()> {
    replace(path, contents, false)
}

/// Like [`write_private`], but fails with `AlreadyExists` instead of
/// replacing an existing file (checked atomically by the OS).
pub(crate) fn create_private(path: &Path, contents: &[u8]) -> io::Result<()> {
    create(path, contents, true)
}

fn replace(path: &Path, contents: &[u8], private: bool) -> io::Result<()> {
    let name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} não é o caminho de um arquivo", path.display()),
        )
    })?;
    // A name nobody else is writing: the process, and a counter for the
    // files it already left behind (a crash) or is writing.
    for attempt in 0..100 {
        let mut temporary = OsString::from(".");
        temporary.push(name);
        temporary.push(format!(".{}.{attempt}.tmp", std::process::id()));
        let temporary = path.with_file_name(temporary);
        match create(&temporary, contents, private) {
            Ok(()) => {
                return fs::rename(&temporary, path).inspect_err(|_| {
                    let _ = fs::remove_file(&temporary);
                });
            }
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {}
            Err(err) => return Err(err),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!("arquivos temporários demais ao lado de {}", path.display()),
    ))
}

fn create(path: &Path, contents: &[u8], private: bool) -> io::Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    if private {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    #[cfg(not(unix))]
    let _ = private;
    let mut file = options.open(path)?;
    // What was created is removed if the writing fails.
    file.write_all(contents)
        .and_then(|()| file.sync_all())
        .inspect_err(|_| {
            let _ = fs::remove_file(path);
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_refuses_existing_files_and_write_replaces_them() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("extrato.pdf");
        create_private(&path, b"primeiro").unwrap();
        let err = create_private(&path, b"segundo").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(&path).unwrap(), b"primeiro");
        write_private(&path, b"terceiro").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"terceiro");
    }

    #[cfg(unix)]
    #[test]
    fn files_are_private_even_when_they_already_existed() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "x").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        write_private(&path, b"y").unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let novo = dir.path().join("novo.pdf");
        create_private(&novo, b"z").unwrap();
        assert_eq!(
            fs::metadata(&novo).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    /// Only the file is left in the directory, whatever happened.
    fn nomes(dir: &Path) -> Vec<String> {
        let mut nomes: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|entrada| entrada.unwrap().file_name().into_string().unwrap())
            .collect();
        nomes.sort();
        nomes
    }

    #[test]
    fn a_replacement_leaves_no_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        write_private(&path, b"um").unwrap();
        write_private(&path, b"dois").unwrap();
        write_public(&dir.path().join("inter-pj.1"), b"man").unwrap();
        assert_eq!(nomes(dir.path()), ["config.toml", "inter-pj.1"]);
        // A temporary file left by a crash does not stop the next one.
        fs::write(
            dir.path()
                .join(format!(".config.toml.{}.0.tmp", std::process::id())),
            "antigo",
        )
        .unwrap();
        write_private(&path, b"tres").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"tres");
    }

    /// Another user could point a link at a file of yours in a shared
    /// directory: it is replaced, and its target is never written.
    #[cfg(unix)]
    #[test]
    fn links_are_replaced_and_never_followed() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let dir = tempfile::tempdir().unwrap();
        let alvo = dir.path().join("authorized_keys");
        fs::write(&alvo, "da vítima").unwrap();
        let link = dir.path().join("extrato.pdf");
        symlink(&alvo, &link).unwrap();

        // Creating refuses it, even when the link points nowhere.
        let err = create_private(&link, b"pdf").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        let pendente = dir.path().join("config.toml");
        symlink(dir.path().join("nao-existe"), &pendente).unwrap();
        let err = create_private(&pendente, b"x").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        assert!(!dir.path().join("nao-existe").exists());

        // Replacing it writes a new file where the link was.
        write_private(&link, b"pdf").unwrap();
        assert_eq!(fs::read_to_string(&alvo).unwrap(), "da vítima");
        assert!(!fs::symlink_metadata(&link).unwrap().is_symlink());
        assert_eq!(fs::read(&link).unwrap(), b"pdf");
        assert_eq!(
            fs::metadata(&link).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
