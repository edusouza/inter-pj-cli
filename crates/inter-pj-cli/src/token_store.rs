//! File-based [`TokenStore`]: one JSON file per integration, readable only by
//! the current user, written atomically.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use inter_pj::{AccessToken, TokenStore};
use serde::{Deserialize, Serialize};

const FORMAT_VERSION: u32 = 1;

#[derive(Debug)]
pub(crate) struct FileTokenStore {
    dir: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct CacheFile {
    versao: u32,
    tokens: Vec<AccessToken>,
}

impl FileTokenStore {
    pub(crate) fn new(cache_dir: &Path) -> Self {
        Self {
            dir: cache_dir.join("tokens"),
        }
    }

    pub(crate) fn path_for(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.json"))
    }

    /// Removes the tokens of one integration; returns whether a file existed.
    pub(crate) fn remove(&self, key: &str) -> io::Result<bool> {
        match fs::remove_file(self.path_for(key)) {
            Ok(()) => Ok(true),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(err) => Err(err),
        }
    }

    /// Removes every cached token file; returns how many were removed.
    pub(crate) fn remove_all(&self) -> io::Result<usize> {
        let entries = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(0),
            Err(err) => return Err(err),
        };
        let mut removed = 0;
        for entry in entries {
            let path = entry?.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                fs::remove_file(path)?;
                removed += 1;
            }
        }
        Ok(removed)
    }

    fn create_dir(&self) -> io::Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(&self.dir)
        }
        #[cfg(not(unix))]
        {
            fs::create_dir_all(&self.dir)
        }
    }
}

impl TokenStore for FileTokenStore {
    fn load(&self, key: &str) -> io::Result<Vec<AccessToken>> {
        match fs::read(self.path_for(key)) {
            Ok(bytes) => {
                let file: CacheFile = serde_json::from_slice(&bytes)
                    .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
                Ok(if file.versao == FORMAT_VERSION {
                    file.tokens
                } else {
                    Vec::new()
                })
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(err) => Err(err),
        }
    }

    fn save(&self, key: &str, tokens: &[AccessToken]) -> io::Result<()> {
        if tokens.is_empty() {
            return self.remove(key).map(|_| ());
        }
        self.create_dir()?;
        let json = serde_json::to_vec(&CacheFile {
            versao: FORMAT_VERSION,
            tokens: tokens.to_vec(),
        })
        .map_err(io::Error::other)?;

        let target = self.path_for(key);
        let temporary = self.dir.join(format!(".{key}.{}.tmp", std::process::id()));
        let result =
            write_private(&temporary, &json).and_then(|()| fs::rename(&temporary, &target));
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}

/// Writes a file that only the current user can read (mode 600 on Unix).
pub(crate) fn write_private(path: &Path, contents: &[u8]) -> io::Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
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
    use chrono::{TimeDelta, Utc};
    use inter_pj::ScopeSet;
    use secrecy::ExposeSecret;

    use super::*;

    fn token(secret: &str) -> AccessToken {
        AccessToken::new(
            secret,
            "extrato.read".parse::<ScopeSet>().unwrap(),
            Utc::now() + TimeDelta::hours(1),
        )
    }

    #[test]
    fn round_trips_tokens() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileTokenStore::new(dir.path());
        assert!(store.load("chave").unwrap().is_empty());

        store.save("chave", &[token("a"), token("b")]).unwrap();
        let loaded = store.load("chave").unwrap();
        let secrets: Vec<&str> = loaded.iter().map(|t| t.secret().expose_secret()).collect();
        assert_eq!(secrets, ["a", "b"]);
    }

    #[cfg(unix)]
    #[test]
    fn files_are_private() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let store = FileTokenStore::new(dir.path());
        store.save("chave", &[token("a")]).unwrap();
        let file_mode = fs::metadata(store.path_for("chave"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        let dir_mode = fs::metadata(dir.path().join("tokens"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(file_mode, 0o600);
        assert_eq!(dir_mode, 0o700);
    }

    #[test]
    fn saving_nothing_removes_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileTokenStore::new(dir.path());
        store.save("chave", &[token("a")]).unwrap();
        store.save("chave", &[]).unwrap();
        assert!(!store.path_for("chave").exists());
    }

    #[test]
    fn remove_and_remove_all() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileTokenStore::new(dir.path());
        assert_eq!(store.remove_all().unwrap(), 0);
        store.save("a", &[token("1")]).unwrap();
        store.save("b", &[token("2")]).unwrap();
        assert!(store.remove("a").unwrap());
        assert!(!store.remove("a").unwrap());
        assert_eq!(store.remove_all().unwrap(), 1);
    }

    #[test]
    fn corrupted_file_is_an_error_not_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileTokenStore::new(dir.path());
        fs::create_dir_all(dir.path().join("tokens")).unwrap();
        fs::write(store.path_for("chave"), b"lixo").unwrap();
        assert_eq!(
            store.load("chave").unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn unknown_format_version_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileTokenStore::new(dir.path());
        fs::create_dir_all(dir.path().join("tokens")).unwrap();
        fs::write(store.path_for("chave"), br#"{"versao":99,"tokens":[]}"#).unwrap();
        assert!(store.load("chave").unwrap().is_empty());
    }
}
