//! Writing files that may hold sensitive data (configuration, tokens,
//! statements): readable by the owner only.

use std::fs;
use std::io::{self, Write};
use std::path::Path;

/// Writes `contents` to `path` with mode 600, replacing the file if it
/// exists (and fixing its permissions).
pub(crate) fn write_private(path: &Path, contents: &[u8]) -> io::Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    write_with(&options, path, contents)
}

/// Like [`write_private`], but fails with `AlreadyExists` instead of
/// replacing an existing file (checked atomically by the OS).
pub(crate) fn create_private(path: &Path, contents: &[u8]) -> io::Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    write_with(&options, path, contents)
}

fn write_with(options: &fs::OpenOptions, path: &Path, contents: &[u8]) -> io::Result<()> {
    let mut options = options.clone();
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    #[cfg(unix)]
    {
        // `mode` only applies to new files; fix permissions of existing ones.
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(contents)?;
    file.sync_all()
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
}
