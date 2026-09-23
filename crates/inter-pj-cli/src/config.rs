//! Configuration file, profiles and resolution of the effective settings.
//!
//! Precedence: command line flag > environment variable > configuration file.

use std::collections::BTreeMap;
use std::fmt::{self, Write as _};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use inter_pj::{Environment, ScopeSet};
use rust_decimal::Decimal;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};

use crate::error::CliError;
use crate::paths;
use crate::valor::parse_valor;

/// Profile used when none is selected.
pub(crate) const DEFAULT_PROFILE: &str = "padrao";

/// Environment variables read directly (not through flags).
pub(crate) const ENV_CLIENT_SECRET: &str = "INTER_CLIENT_SECRET";
pub(crate) const ENV_BASE_URL: &str = "INTER_BASE_URL";
pub(crate) const ENV_CACHE_DIR: &str = "INTER_CACHE_DIR";

/// Template written by `inter-pj config init`.
pub(crate) const TEMPLATE: &str = r#"# Configuração do inter-pj — CLI não oficial para a conta PJ do Inter Empresas.
# Documentação: https://github.com/edusouza/inter-pj-cli
#
# Este arquivo aponta para as credenciais da sua integração: nunca o versione
# nem o compartilhe. Mantenha a permissão 600 (somente o seu usuário).

perfil_padrao = "padrao"

[perfis.padrao]
# "sandbox" (dados fictícios, para testes) ou "producao" (conta real).
ambiente = "sandbox"

# client_id da integração criada no Internet Banking PJ.
client_id = ""

# Prefira a variável de ambiente INTER_CLIENT_SECRET a gravar o segredo aqui.
# client_secret = ""

# Certificado (.crt) e chave privada (.key) baixados na criação da integração.
# Caminhos absolutos, relativos a este arquivo ou iniciados por ~/.
certificado = ""
chave_privada = ""

# Somente dígitos; necessário apenas se a integração tiver mais de uma conta.
# conta_corrente = ""

# Escopos pedidos em todo token, além dos exigidos por cada comando (opcional).
# Todos precisam estar habilitados na integração.
# escopos = ["extrato.read"]

# Valor máximo de cada Pix ou pagamento feito pela CLI (opcional). Acima dele a
# operação é recusada, mesmo com --sim.
# limite_por_operacao = "1.000,00"
"#;

/// Contents of the configuration file.
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConfigFile {
    pub(crate) perfil_padrao: Option<String>,
    #[serde(default)]
    pub(crate) perfis: BTreeMap<String, Profile>,
}

/// A profile of the configuration file.
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Profile {
    pub(crate) ambiente: Option<String>,
    pub(crate) client_id: Option<String>,
    pub(crate) client_secret: Option<String>,
    pub(crate) certificado: Option<PathBuf>,
    pub(crate) chave_privada: Option<PathBuf>,
    pub(crate) conta_corrente: Option<String>,
    pub(crate) escopos: Option<Vec<String>>,
    pub(crate) limite_por_operacao: Option<ValorArquivo>,
}

/// An amount in the configuration file: `"1.000,00"`, `"1000.00"`, `1000`
/// or `1000.5`.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub(crate) enum ValorArquivo {
    Texto(String),
    Inteiro(i64),
    Real(f64),
}

impl ValorArquivo {
    /// Positive, with at most two decimal places.
    fn decimal(&self) -> Result<Decimal, String> {
        let exato = |texto: String| {
            texto
                .parse::<Decimal>()
                .ok()
                .filter(|v| v.is_sign_positive() && !v.is_zero() && v.normalize().scale() <= 2)
                .ok_or_else(|| {
                    format!("{texto}: use um valor maior que zero, com até 2 casas decimais")
                })
        };
        match self {
            Self::Texto(texto) => parse_valor(texto),
            Self::Inteiro(inteiro) => exato(inteiro.to_string()),
            // Shortest representation that round-trips: 1000.5 stays 1000.5.
            Self::Real(real) => exato(real.to_string()),
        }
    }
}

/// The configuration file as loaded from disk.
#[derive(Debug)]
pub(crate) struct LoadedConfig {
    pub(crate) path: PathBuf,
    pub(crate) exists: bool,
    pub(crate) file: ConfigFile,
}

/// Loads the configuration file; a missing file is an empty configuration.
pub(crate) fn load(path: &Path) -> Result<LoadedConfig, CliError> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            return Ok(LoadedConfig {
                path: path.to_path_buf(),
                exists: false,
                file: ConfigFile::default(),
            });
        }
        Err(err) => {
            return Err(CliError::Config(format!(
                "não foi possível ler o arquivo de configuração {}: {err}",
                path.display()
            )));
        }
    };
    let file = toml::from_str(&text).map_err(|err: toml::de::Error| {
        // Do not print the offending line: it may contain the client_secret.
        let line = err
            .span()
            .map(|span| text[..span.start.min(text.len())].lines().count().max(1));
        let location = line.map(|n| format!(", linha {n}")).unwrap_or_default();
        CliError::Config(format!(
            "arquivo de configuração inválido ({}{location}): {}",
            path.display(),
            err.message()
        ))
    })?;
    Ok(LoadedConfig {
        path: path.to_path_buf(),
        exists: true,
        file,
    })
}

/// Where a setting came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Source {
    Flag,
    Env(&'static str),
    File,
    Default,
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Flag => f.write_str("flag"),
            Self::Env(name) => write!(f, "variável {name}"),
            Self::File => f.write_str("arquivo"),
            Self::Default => f.write_str("padrão"),
        }
    }
}

/// A resolved value together with its origin.
#[derive(Debug, Clone)]
pub(crate) struct Setting<T> {
    pub(crate) value: T,
    pub(crate) source: Source,
}

/// A value given on the command line or in the environment (through clap).
#[derive(Debug, Clone, Default)]
pub(crate) struct Given<T> {
    pub(crate) value: Option<T>,
    pub(crate) source: Option<Source>,
}

#[cfg(test)]
impl<T> Given<T> {
    pub(crate) fn new(value: Option<T>, source: Source) -> Self {
        let source = value.is_some().then_some(source);
        Self { value, source }
    }
}

/// Inputs of the resolution that do not come from the configuration file.
#[derive(Debug, Default)]
pub(crate) struct Inputs {
    pub(crate) perfil: Given<String>,
    pub(crate) ambiente: Given<String>,
    pub(crate) client_id: Given<String>,
    pub(crate) certificado: Given<PathBuf>,
    pub(crate) chave_privada: Given<PathBuf>,
    pub(crate) conta_corrente: Given<String>,
    pub(crate) client_secret: Option<String>,
    pub(crate) base_url: Option<String>,
}

/// Effective settings of the selected profile. Fields are `None` when unset.
#[derive(Debug)]
pub(crate) struct Settings {
    pub(crate) perfil: Setting<String>,
    pub(crate) config_path: PathBuf,
    pub(crate) config_exists: bool,
    pub(crate) ambiente: Option<Setting<Environment>>,
    pub(crate) client_id: Option<Setting<String>>,
    pub(crate) client_secret: Option<Setting<SecretString>>,
    pub(crate) certificado: Option<Setting<PathBuf>>,
    pub(crate) chave_privada: Option<Setting<PathBuf>>,
    pub(crate) conta_corrente: Option<Setting<String>>,
    pub(crate) escopos: Option<Setting<ScopeSet>>,
    pub(crate) limite_por_operacao: Option<Setting<Decimal>>,
    pub(crate) base_url: Option<Setting<String>>,
    pub(crate) warnings: Vec<String>,
}

/// Settings required to build a client.
#[derive(Debug)]
pub(crate) struct ClientSettings<'a> {
    pub(crate) ambiente: Environment,
    pub(crate) client_id: &'a str,
    pub(crate) client_secret: &'a SecretString,
    pub(crate) certificado: &'a Path,
    pub(crate) chave_privada: &'a Path,
}

impl Settings {
    /// Resolves the settings of the selected profile.
    pub(crate) fn resolve(loaded: &LoadedConfig, inputs: Inputs) -> Result<Self, CliError> {
        let file = &loaded.file;
        let base_dir = loaded.path.parent();

        let perfil = match (inputs.perfil.value, inputs.perfil.source) {
            (Some(name), Some(source)) => Setting {
                value: name,
                source,
            },
            _ => match &file.perfil_padrao {
                Some(name) => Setting {
                    value: name.clone(),
                    source: Source::File,
                },
                None => Setting {
                    value: DEFAULT_PROFILE.to_owned(),
                    source: Source::Default,
                },
            },
        };
        let profile = match file.perfis.get(&perfil.value) {
            Some(profile) => profile.clone(),
            None if loaded.exists && perfil.source != Source::Default => {
                let disponiveis = file.perfis.keys().cloned().collect::<Vec<_>>().join(", ");
                return Err(CliError::Config(format!(
                    "perfil \"{}\" não encontrado em {} (perfis disponíveis: {})",
                    perfil.value,
                    loaded.path.display(),
                    if disponiveis.is_empty() {
                        "nenhum"
                    } else {
                        &disponiveis
                    }
                )));
            }
            None => Profile::default(),
        };

        let limite_por_operacao = limite(&profile, &perfil.value)?;
        let escopos = escopos(&profile, &perfil.value)?;
        let ambiente = pick(inputs.ambiente, non_empty(profile.ambiente))
            .map(|setting| {
                setting.value.parse::<Environment>().map(|value| Setting {
                    value,
                    source: setting.source,
                })
            })
            .transpose()
            .map_err(|err| CliError::Config(err.to_string()))?;

        let client_id = pick(inputs.client_id, non_empty(profile.client_id));

        let client_secret = match non_empty(inputs.client_secret) {
            Some(secret) => Some(Setting {
                value: SecretString::from(secret),
                source: Source::Env(ENV_CLIENT_SECRET),
            }),
            None => non_empty(profile.client_secret.clone()).map(|secret| Setting {
                value: SecretString::from(secret),
                source: Source::File,
            }),
        };

        let certificado = pick_path(inputs.certificado, profile.certificado, base_dir);
        let chave_privada = pick_path(inputs.chave_privada, profile.chave_privada, base_dir);
        let conta_corrente = pick(inputs.conta_corrente, non_empty(profile.conta_corrente));

        let base_url = non_empty(inputs.base_url).map(|value| Setting {
            value,
            source: Source::Env(ENV_BASE_URL),
        });

        // Any profile with a secret makes the file sensitive, even when the
        // environment overrides it for this run.
        let secret_in_file = file
            .perfis
            .values()
            .any(|p| non_empty(p.client_secret.clone()).is_some());
        let warnings = permission_warnings(
            secret_in_file.then_some(loaded.path.as_path()),
            chave_privada.as_ref().map(|key| key.value.as_path()),
        );

        Ok(Self {
            perfil,
            config_path: loaded.path.clone(),
            config_exists: loaded.exists,
            ambiente,
            client_id,
            client_secret,
            certificado,
            chave_privada,
            conta_corrente,
            escopos,
            limite_por_operacao,
            base_url,
            warnings,
        })
    }

    /// Base URL used for requests: the override, or the environment's.
    pub(crate) fn effective_base_url(&self) -> Option<String> {
        match (&self.base_url, &self.ambiente) {
            (Some(url), _) => Some(url.value.clone()),
            (None, Some(ambiente)) => Some(ambiente.value.base_url().to_owned()),
            (None, None) => None,
        }
    }

    /// Everything needed to build a client, or an error listing what is missing.
    pub(crate) fn for_client(&self) -> Result<ClientSettings<'_>, CliError> {
        let mut missing = Vec::new();
        if self.ambiente.is_none() {
            missing.push("ambiente: use --ambiente, INTER_AMBIENTE ou `ambiente` no perfil");
        }
        if self.client_id.is_none() {
            missing.push("client_id: use --client-id, INTER_CLIENT_ID ou `client_id` no perfil");
        }
        if self.client_secret.is_none() {
            missing.push("client_secret: defina INTER_CLIENT_SECRET ou `client_secret` no perfil");
        }
        if self.certificado.is_none() {
            missing.push(
                "certificado: use --certificado, INTER_CERTIFICADO ou `certificado` no perfil",
            );
        }
        if self.chave_privada.is_none() {
            missing.push(
                "chave privada: use --chave-privada, INTER_CHAVE_PRIVADA ou `chave_privada` no perfil",
            );
        }
        match (
            &self.ambiente,
            &self.client_id,
            &self.client_secret,
            &self.certificado,
            &self.chave_privada,
        ) {
            (Some(ambiente), Some(client_id), Some(secret), Some(certificado), Some(chave)) => {
                Ok(ClientSettings {
                    ambiente: ambiente.value,
                    client_id: &client_id.value,
                    client_secret: &secret.value,
                    certificado: &certificado.value,
                    chave_privada: &chave.value,
                })
            }
            _ => Err(CliError::Config(self.missing_message(&missing))),
        }
    }

    fn missing_message(&self, missing: &[&str]) -> String {
        let mut message = format!(
            "configuração incompleta (perfil \"{}\"):",
            self.perfil.value
        );
        for item in missing {
            message.push_str("\n  - ");
            message.push_str(item);
        }
        let status = if self.config_exists {
            String::new()
        } else {
            " — não encontrado; crie um com `inter-pj config init`".to_owned()
        };
        let _ = write!(
            message,
            "\narquivo de configuração: {}{status}",
            self.config_path.display()
        );
        message
    }
}

/// `escopos` of the profile.
fn escopos(profile: &Profile, perfil: &str) -> Result<Option<Setting<ScopeSet>>, CliError> {
    let Some(names) = &profile.escopos else {
        return Ok(None);
    };
    let value = names
        .iter()
        .map(|name| name.parse())
        .collect::<Result<ScopeSet, _>>()
        .map_err(|err| CliError::Config(format!("{err} no perfil \"{perfil}\"")))?;
    Ok(Some(Setting {
        value,
        source: Source::File,
    }))
}

/// `limite_por_operacao` of the profile.
fn limite(profile: &Profile, perfil: &str) -> Result<Option<Setting<Decimal>>, CliError> {
    let Some(valor) = &profile.limite_por_operacao else {
        return Ok(None);
    };
    let value = valor.decimal().map_err(|err| {
        CliError::Config(format!(
            "limite_por_operacao inválido no perfil \"{perfil}\": {err}"
        ))
    })?;
    Ok(Some(Setting {
        value,
        source: Source::File,
    }))
}

/// Warnings about secret files readable by other users.
fn permission_warnings(
    config_with_secret: Option<&Path>,
    private_key: Option<&Path>,
) -> Vec<String> {
    let mut warnings = Vec::new();
    if let Some(path) = config_with_secret
        && let Some(mode) = world_accessible(path)
    {
        warnings.push(format!(
            "o arquivo de configuração contém o client_secret e pode ser lido por outros usuários (permissão {mode:o}); execute: chmod 600 {}",
            path.display()
        ));
    }
    if let Some(path) = private_key
        && let Some(mode) = world_accessible(path)
    {
        warnings.push(format!(
            "a chave privada pode ser lida por outros usuários (permissão {mode:o}); execute: chmod 600 {}",
            path.display()
        ));
    }
    warnings
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|v| !v.trim().is_empty())
}

fn pick(given: Given<String>, file: Option<String>) -> Option<Setting<String>> {
    match (non_empty(given.value), given.source) {
        (Some(value), Some(source)) => Some(Setting { value, source }),
        _ => file.map(|value| Setting {
            value,
            source: Source::File,
        }),
    }
}

fn pick_path(
    given: Given<PathBuf>,
    file: Option<PathBuf>,
    base: Option<&Path>,
) -> Option<Setting<PathBuf>> {
    match (
        given.value.filter(|p| !p.as_os_str().is_empty()),
        given.source,
    ) {
        (Some(path), Some(source)) => Some(Setting {
            value: paths::expand(&path, None),
            source,
        }),
        _ => file
            .filter(|p| !p.as_os_str().is_empty())
            .map(|path| Setting {
                value: paths::expand(&path, base),
                source: Source::File,
            }),
    }
}

/// Unix permission bits when group or others can access the file.
#[cfg(unix)]
fn world_accessible(path: &Path) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    let mode = fs::metadata(path).ok()?.permissions().mode() & 0o777;
    (mode & 0o077 != 0).then_some(mode)
}

#[cfg(not(unix))]
fn world_accessible(_path: &Path) -> Option<u32> {
    None
}

#[cfg(test)]
mod tests {
    use secrecy::ExposeSecret;

    use super::*;

    fn loaded(toml: &str) -> LoadedConfig {
        LoadedConfig {
            path: PathBuf::from("/config/inter-pj/config.toml"),
            exists: true,
            file: toml::from_str(toml).unwrap(),
        }
    }

    const FULL: &str = r#"
        perfil_padrao = "empresa"
        [perfis.empresa]
        ambiente = "producao"
        client_id = "id-do-arquivo"
        client_secret = "segredo-do-arquivo"
        certificado = "certs/empresa.crt"
        chave_privada = "/abs/empresa.key"
        conta_corrente = "1234567"
        escopos = ["extrato.read", "pix.read"]

        [perfis.teste]
        ambiente = "sandbox"
    "#;

    #[test]
    fn reads_default_profile_from_file() {
        let settings = Settings::resolve(&loaded(FULL), Inputs::default()).unwrap();
        assert_eq!(settings.perfil.value, "empresa");
        assert_eq!(settings.perfil.source, Source::File);
        assert_eq!(
            settings.ambiente.as_ref().unwrap().value,
            Environment::Production
        );
        assert_eq!(settings.client_id.as_ref().unwrap().value, "id-do-arquivo");
        assert_eq!(
            settings
                .client_secret
                .as_ref()
                .unwrap()
                .value
                .expose_secret(),
            "segredo-do-arquivo"
        );
        assert_eq!(
            settings.certificado.as_ref().unwrap().value,
            PathBuf::from("/config/inter-pj/certs/empresa.crt"),
            "caminho relativo ao arquivo de configuração"
        );
        assert_eq!(
            settings.chave_privada.as_ref().unwrap().value,
            PathBuf::from("/abs/empresa.key")
        );
        assert_eq!(
            settings.escopos.as_ref().unwrap().value.to_string(),
            "extrato.read pix.read"
        );
        assert!(settings.for_client().is_ok());
    }

    #[test]
    fn flags_and_env_override_file() {
        let inputs = Inputs {
            ambiente: Given::new(Some("sandbox".into()), Source::Flag),
            client_id: Given::new(Some("id-da-env".into()), Source::Env("INTER_CLIENT_ID")),
            client_secret: Some("segredo-da-env".into()),
            ..Inputs::default()
        };
        let settings = Settings::resolve(&loaded(FULL), inputs).unwrap();
        let ambiente = settings.ambiente.unwrap();
        assert_eq!(
            (ambiente.value, ambiente.source),
            (Environment::Sandbox, Source::Flag)
        );
        let client_id = settings.client_id.unwrap();
        assert_eq!(client_id.value, "id-da-env");
        assert_eq!(client_id.source, Source::Env("INTER_CLIENT_ID"));
        let secret = settings.client_secret.unwrap();
        assert_eq!(secret.value.expose_secret(), "segredo-da-env");
        assert_eq!(secret.source, Source::Env(ENV_CLIENT_SECRET));
    }

    #[test]
    fn selects_profile_by_name() {
        let inputs = Inputs {
            perfil: Given::new(Some("teste".into()), Source::Flag),
            ..Inputs::default()
        };
        let settings = Settings::resolve(&loaded(FULL), inputs).unwrap();
        assert_eq!(settings.ambiente.unwrap().value, Environment::Sandbox);
        assert!(settings.client_id.is_none());
    }

    #[test]
    fn unknown_profile_lists_available_ones() {
        let inputs = Inputs {
            perfil: Given::new(Some("outro".into()), Source::Flag),
            ..Inputs::default()
        };
        let err = Settings::resolve(&loaded(FULL), inputs)
            .unwrap_err()
            .to_string();
        assert!(err.contains("perfil \"outro\" não encontrado"), "{err}");
        assert!(err.contains("empresa, teste"), "{err}");
    }

    #[test]
    fn missing_settings_are_listed_together() {
        let missing = LoadedConfig {
            path: PathBuf::from("/nao/existe/config.toml"),
            exists: false,
            file: ConfigFile::default(),
        };
        let settings = Settings::resolve(&missing, Inputs::default()).unwrap();
        let err = settings.for_client().unwrap_err().to_string();
        for item in [
            "ambiente",
            "client_id",
            "client_secret",
            "certificado",
            "chave privada",
        ] {
            assert!(err.contains(item), "{item} ausente em: {err}");
        }
        assert!(err.contains("inter-pj config init"), "{err}");
    }

    #[test]
    fn empty_values_count_as_unset() {
        let settings = Settings::resolve(&loaded(TEMPLATE), Inputs::default()).unwrap();
        assert_eq!(
            settings.ambiente.as_ref().unwrap().value,
            Environment::Sandbox
        );
        assert!(settings.client_id.is_none());
        assert!(settings.certificado.is_none());
        assert!(settings.chave_privada.is_none());
    }

    #[test]
    fn invalid_environment_and_scope_are_errors() {
        let inputs = Inputs {
            ambiente: Given::new(Some("homologacao".into()), Source::Flag),
            ..Inputs::default()
        };
        let err = Settings::resolve(&loaded(FULL), inputs)
            .unwrap_err()
            .to_string();
        assert!(err.contains("ambiente inválido"), "{err}");

        let err = Settings::resolve(
            &loaded("[perfis.padrao]\nescopos = [\"banana.read\"]"),
            Inputs::default(),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("banana.read"), "{err}");
    }

    #[test]
    fn operation_limit_accepts_text_and_numbers() {
        for (valor, esperado) in [
            ("\"1.000,00\"", "1000.00"),
            ("\"1000.50\"", "1000.50"),
            ("1000", "1000"),
            ("1000.5", "1000.5"),
        ] {
            let config = format!("[perfis.padrao]\nlimite_por_operacao = {valor}");
            let settings = Settings::resolve(&loaded(&config), Inputs::default()).unwrap();
            let limite = settings.limite_por_operacao.unwrap();
            assert_eq!(
                limite.value,
                esperado.parse::<Decimal>().unwrap(),
                "{valor}"
            );
            assert_eq!(limite.source, Source::File);
        }
        for valor in ["\"1.000\"", "0", "-5", "10.555", "\"dez\""] {
            let config = format!("[perfis.padrao]\nlimite_por_operacao = {valor}");
            let err = Settings::resolve(&loaded(&config), Inputs::default())
                .unwrap_err()
                .to_string();
            assert!(
                err.contains("limite_por_operacao inválido"),
                "{valor}: {err}"
            );
        }
        let settings = Settings::resolve(&loaded(FULL), Inputs::default()).unwrap();
        assert!(settings.limite_por_operacao.is_none());
    }

    #[test]
    fn unknown_keys_are_rejected() {
        let err = toml::from_str::<ConfigFile>("[perfis.padrao]\nclientid = \"x\"").unwrap_err();
        assert!(err.message().contains("clientid"), "{err}");
    }

    #[test]
    fn invalid_file_error_does_not_echo_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            "[perfis.padrao]\nclient_secret = segredo-sem-aspas\n",
        )
        .unwrap();
        let err = load(&path).unwrap_err().to_string();
        assert!(err.contains("linha 2"), "{err}");
        assert!(!err.contains("segredo-sem-aspas"), "{err}");
    }

    #[test]
    fn template_is_valid() {
        let file: ConfigFile = toml::from_str(TEMPLATE).unwrap();
        assert_eq!(file.perfil_padrao.as_deref(), Some(DEFAULT_PROFILE));
        assert!(file.perfis.contains_key(DEFAULT_PROFILE));
    }

    #[test]
    fn base_url_override() {
        let inputs = Inputs {
            base_url: Some("http://127.0.0.1:9999".into()),
            ..Inputs::default()
        };
        let settings = Settings::resolve(&loaded(FULL), inputs).unwrap();
        assert_eq!(
            settings.effective_base_url().as_deref(),
            Some("http://127.0.0.1:9999")
        );
        let settings = Settings::resolve(&loaded(FULL), Inputs::default()).unwrap();
        assert_eq!(
            settings.effective_base_url().as_deref(),
            Some(Environment::Production.base_url())
        );
    }
}
