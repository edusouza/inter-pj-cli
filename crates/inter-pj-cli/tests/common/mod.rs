//! Helpers shared by the end-to-end tests: the real `inter-pj` binary against
//! a mock Inter API, with configuration, cache and home directories isolated
//! in a temporary folder. Every credential, certificate and account number
//! here is synthetic.

#![allow(dead_code, unreachable_pub)]

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use assert_cmd::assert::Assert;
use serde_json::json;
use tempfile::TempDir;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

pub const CLIENT_ID: &str = "client-id-e2e-0000-1111";
pub const CLIENT_SECRET: &str = "segredo-e2e-que-nunca-deve-vazar";
pub const TOKEN: &str = "token-e2e-que-nunca-deve-vazar";

/// The example of the Banco Central's manual of the BR Code, with a valid
/// CRC.
pub const COPIA_E_COLA: &str = "00020126580014br.gov.bcb.pix0136123e4567-e12b-12d1-a456-4266554400005204000053039865802BR5913Fulano de Tal6008BRASILIA62070503***63041D3D";

pub struct TestEnv {
    pub dir: TempDir,
    pub server: MockServer,
}

impl TestEnv {
    pub async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let rcgen::CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed(vec!["cliente.teste".to_owned()]).unwrap();
        write_private(&dir.path().join("certificado.crt"), &cert.pem());
        write_private(&dir.path().join("chave.key"), &signing_key.serialize_pem());
        let server = MockServer::start().await;
        Self { dir, server }
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    pub fn config_path(&self) -> PathBuf {
        self.path("config").join("config.toml")
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.path("cache")
    }

    /// Writes a complete sandbox profile, plus `extra` lines.
    pub fn write_config(&self, extra: &str) {
        let content = format!(
            "perfil_padrao = \"padrao\"\n\n[perfis.padrao]\nambiente = \"sandbox\"\nclient_id = \"{CLIENT_ID}\"\ncertificado = '{}'\nchave_privada = '{}'\n{extra}\n",
            self.path("certificado.crt").display(),
            self.path("chave.key").display(),
        );
        fs::create_dir_all(self.config_path().parent().unwrap()).unwrap();
        write_private(&self.config_path(), &content);
    }

    /// The binary with an isolated environment.
    pub fn cmd(&self) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_inter-pj"));
        for (key, _) in std::env::vars() {
            if key.starts_with("INTER_") {
                cmd.env_remove(key);
            }
        }
        let home = self.path("home");
        cmd.env("HOME", &home)
            .env("USERPROFILE", &home)
            .env("XDG_CONFIG_HOME", home.join(".config"))
            .env("XDG_CACHE_HOME", home.join(".cache"))
            .env("INTER_CONFIG", self.config_path())
            .env("INTER_CACHE_DIR", self.cache_dir())
            .env("INTER_BASE_URL", self.server.uri())
            .env("INTER_CLIENT_SECRET", CLIENT_SECRET)
            .env("NO_COLOR", "1");
        cmd
    }

    pub async fn mount_token(&self, scope: &str, calls: Option<u64>) {
        let mock = Mock::given(method("POST"))
            .and(path("/oauth/v2/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": TOKEN,
                "token_type": "Bearer",
                "expires_in": 3600,
                "scope": scope,
            })));
        match calls {
            Some(calls) => mock.expect(calls).mount(&self.server).await,
            None => mock.mount(&self.server).await,
        }
    }

    pub async fn mount_saldo(&self, response: ResponseTemplate, calls: u64) {
        Mock::given(method("GET"))
            .and(path("/banking/v2/saldo"))
            .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
            .respond_with(response)
            .expect(calls)
            .mount(&self.server)
            .await;
    }

    pub fn cache_files(&self) -> Vec<PathBuf> {
        match fs::read_dir(self.cache_dir().join("tokens")) {
            Ok(entries) => entries.map(|e| e.unwrap().path()).collect(),
            Err(_) => Vec::new(),
        }
    }
}

pub fn write_private(path: &Path, content: &str) {
    fs::write(path, content).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }
}

pub fn saldo_ok() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "bloqueadoCheque": 240.25,
        "disponivel": 2850.55,
        "bloqueadoJudicialmente": 510.35,
        "bloqueadoAdministrativo": 0,
        "limite": 1000.00
    }))
}

pub fn stdout_of(assert: &Assert) -> String {
    String::from_utf8(assert.get_output().stdout.clone()).unwrap()
}

pub fn stderr_of(assert: &Assert) -> String {
    String::from_utf8(assert.get_output().stderr.clone()).unwrap()
}

/// The modules of the QR Code drawn without colors (light modules drawn),
/// read back as a phone would.
pub fn ler_qr_code(texto: &str) -> String {
    const PX: usize = 4;
    let linhas: Vec<&str> = texto
        .lines()
        .filter(|linha| !linha.is_empty() && linha.chars().all(|c| "█▀▄ ".contains(c)))
        .collect();
    assert!(!linhas.is_empty(), "sem QR Code:\n{texto}");
    let lado = linhas[0].chars().count();
    let mut escuros = vec![vec![false; lado]; linhas.len() * 2];
    for (i, linha) in linhas.iter().enumerate() {
        for (x, c) in linha.chars().enumerate() {
            let (cima, baixo) = match c {
                '█' => (true, true),
                '▀' => (true, false),
                '▄' => (false, true),
                _ => (false, false),
            };
            escuros[2 * i][x] = !cima;
            escuros[2 * i + 1][x] = !baixo;
        }
    }
    let mut imagem = rqrr::PreparedImage::prepare_from_greyscale(lado * PX, lado * PX, |x, y| {
        if escuros[y / PX][x / PX] { 0 } else { 255 }
    });
    let grades = imagem.detect_grids();
    assert_eq!(grades.len(), 1, "QR Code não encontrado:\n{texto}");
    grades[0].decode().unwrap().1
}
