//! Default locations of the configuration file and the cache.
//!
//! Linux and macOS follow the XDG convention (`~/.config`, `~/.cache`);
//! Windows uses `%APPDATA%` and `%LOCALAPPDATA%`.

use std::path::{Path, PathBuf};

use etcetera::BaseStrategy;

use crate::error::CliError;

const APP_DIR: &str = "inter-pj";

fn strategy() -> Result<impl BaseStrategy, CliError> {
    etcetera::choose_base_strategy().map_err(|_| {
        CliError::Config(
            "não foi possível determinar o diretório do usuário; informe --config e INTER_CACHE_DIR"
                .to_owned(),
        )
    })
}

/// Configuration file: the explicit one, or the platform default.
pub(crate) fn config_file(explicit: Option<&Path>) -> Result<PathBuf, CliError> {
    match explicit {
        Some(path) => Ok(path.to_path_buf()),
        None => Ok(strategy()?.config_dir().join(APP_DIR).join("config.toml")),
    }
}

/// Cache directory: the explicit one (`INTER_CACHE_DIR`), or the platform default.
pub(crate) fn cache_dir(explicit: Option<&str>) -> Result<PathBuf, CliError> {
    match explicit {
        Some(dir) => Ok(PathBuf::from(dir)),
        None => Ok(strategy()?.cache_dir().join(APP_DIR)),
    }
}

/// Expands a leading `~/` and resolves relative paths against `base`.
pub(crate) fn expand(path: &Path, base: Option<&Path>) -> PathBuf {
    let expanded = match path.strip_prefix("~") {
        Ok(rest) => match etcetera::home_dir() {
            Ok(home) => home.join(rest),
            Err(_) => path.to_path_buf(),
        },
        Err(_) => path.to_path_buf(),
    };
    match base {
        Some(base) if expanded.is_relative() => base.join(expanded),
        _ => expanded,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_locations_win() {
        assert_eq!(
            config_file(Some(Path::new("/tmp/x.toml"))).unwrap(),
            PathBuf::from("/tmp/x.toml")
        );
        assert_eq!(
            cache_dir(Some("/tmp/cache")).unwrap(),
            PathBuf::from("/tmp/cache")
        );
    }

    #[test]
    fn default_locations_end_with_app_dir() {
        let config = config_file(None).unwrap();
        assert!(
            config.ends_with(Path::new("inter-pj").join("config.toml")),
            "{config:?}"
        );
        assert!(cache_dir(None).unwrap().ends_with("inter-pj"));
    }

    #[test]
    fn expands_home_and_relative_paths() {
        let home = etcetera::home_dir().unwrap();
        assert_eq!(
            expand(Path::new("~/certs/a.crt"), None),
            home.join("certs/a.crt")
        );
        assert_eq!(
            expand(Path::new("certs/a.crt"), Some(Path::new("/etc/inter-pj"))),
            PathBuf::from("/etc/inter-pj/certs/a.crt")
        );
        assert_eq!(
            expand(Path::new("certs/a.crt"), None),
            PathBuf::from("certs/a.crt")
        );
    }
}
