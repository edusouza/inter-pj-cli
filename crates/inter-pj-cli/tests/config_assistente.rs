//! `inter-pj config init --interativo` end to end, with the answers on
//! stdin. The certificate and the key are the ones generated for each test.

mod common;

use std::fs;

use common::{TestEnv, stderr_of, stdout_of};
use serde_json::Value;

/// The answers, one per line, with the certificate and the key of `env`.
fn respostas(env: &TestEnv, linhas: &[&str]) -> String {
    let certificado = env.path("certificado.crt").display().to_string();
    let chave = env.path("chave.key").display().to_string();
    let mut texto = String::new();
    for linha in linhas {
        texto.push_str(
            &linha
                .replace("{crt}", &certificado)
                .replace("{key}", &chave),
        );
        texto.push('\n');
    }
    texto
}

fn mostrar(env: &TestEnv, perfil: &str) -> Value {
    serde_json::from_str(&stdout_of(
        &env.cmd()
            .args(["--perfil", perfil, "config", "mostrar", "--json"])
            .assert()
            .success(),
    ))
    .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn cria_o_arquivo_com_as_respostas() {
    let env = TestEnv::new().await;
    let assert = env
        .cmd()
        .args(["config", "init", "--interativo"])
        .write_stdin(respostas(
            &env,
            &["", "", "id-de-teste", "{crt}", "{key}", ""],
        ))
        .assert()
        .success();
    assert_eq!(
        stdout_of(&assert),
        format!(
            "Perfil \"padrao\" gravado em {}.\nPróximos passos:\n  1. defina a variável de ambiente INTER_CLIENT_SECRET com o client_secret da integração;\n  2. teste com: inter-pj saldo\n",
            env.config_path().display()
        )
    );
    let stderr = stderr_of(&assert);
    for trecho in [
        "O client_secret não é perguntado",
        "Nome do perfil [padrao]: ",
        "Ambiente (sandbox, com dados fictícios, ou producao) [sandbox]: ",
        "client_id da integração: ",
        "Certificado (.crt): ",
        ", válido até ",
        "Chave privada (.key): ",
        "  certificado e chave aceitos",
    ] {
        assert!(stderr.contains(trecho), "{trecho}\n{stderr}");
    }
    let config = mostrar(&env, "padrao");
    assert_eq!(config["ambiente"]["valor"], "sandbox");
    assert_eq!(
        config["certificado"]["valor"],
        env.path("certificado.crt").display().to_string()
    );
    assert_eq!(
        config["chavePrivada"]["valor"],
        env.path("chave.key").display().to_string()
    );
    assert!(config["contaCorrente"]["valor"].is_null(), "{config}");
    let texto = fs::read_to_string(env.config_path()).unwrap();
    assert!(texto.contains("client_id = \"id-de-teste\"\n"), "{texto}");
    assert!(!texto.contains("client_secret ="), "{texto}");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let modo = fs::metadata(env.config_path())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(modo & 0o777, 0o600);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn respostas_invalidas_sao_perguntadas_de_novo() {
    let env = TestEnv::new().await;
    let assert = env
        .cmd()
        .args(["config", "init", "--interativo"])
        .write_stdin(respostas(
            &env,
            &[
                "perfil com espaço",
                "empresa",
                "teste",
                "producao",
                "",
                "id-de-teste",
                "/nao/existe.crt",
                "{key}",
                "{crt}",
                "{crt}",
                "{key}",
                "12-3",
                "1234567",
            ],
        ))
        .assert()
        .success();
    let stderr = stderr_of(&assert);
    for problema in [
        "  o nome do perfil tem letras sem acento, dígitos, _ ou -",
        "  ambiente inválido: \"teste\"",
        "  o client_id é o identificador da integração, sem espaços",
        "  não foi possível ler /nao/existe.crt",
        "  o arquivo de certificado não contém um certificado PEM",
        "  o arquivo de chave privada não contém uma chave PEM",
        "  a conta corrente vai só com os dígitos",
    ] {
        assert!(stderr.contains(problema), "{problema}\n{stderr}");
    }
    let config = mostrar(&env, "empresa");
    assert_eq!(config["ambiente"]["valor"], "producao");
    assert_eq!(config["contaCorrente"]["valor"], "*****67");
}

#[tokio::test(flavor = "multi_thread")]
async fn num_arquivo_existente_acrescenta_o_perfil() {
    let env = TestEnv::new().await;
    env.write_config("");
    let antes = fs::read_to_string(env.config_path()).unwrap();
    let assert = env
        .cmd()
        .args(["config", "init", "--interativo"])
        .write_stdin(respostas(
            &env,
            &[
                "padrao",
                "producao",
                "producao",
                "id-de-producao",
                "{crt}",
                "{key}",
                "",
            ],
        ))
        .assert()
        .success();
    assert!(
        stderr_of(&assert).contains(
            "o perfil \"padrao\" já existe no arquivo: escolha outro nome, ou recrie o arquivo com --forcar"
        ),
        "{}",
        stderr_of(&assert)
    );
    assert!(
        stdout_of(&assert).ends_with("teste com: inter-pj --perfil producao saldo\n"),
        "{}",
        stdout_of(&assert)
    );
    let depois = fs::read_to_string(env.config_path()).unwrap();
    assert!(depois.starts_with(&antes), "{depois}");
    assert_eq!(mostrar(&env, "producao")["ambiente"]["valor"], "producao");
    assert_eq!(mostrar(&env, "padrao")["ambiente"]["valor"], "sandbox");
}

#[tokio::test(flavor = "multi_thread")]
async fn caminhos_relativos_viram_absolutos() {
    let env = TestEnv::new().await;
    env.cmd()
        .current_dir(env.path(""))
        .args(["config", "init", "--interativo"])
        .write_stdin(respostas(
            &env,
            &["", "", "id-de-teste", "certificado.crt", "chave.key", ""],
        ))
        .assert()
        .success();
    // The current directory may come resolved (macOS: /private/var/...).
    let config = mostrar(&env, "padrao");
    let gravado = config["certificado"]["valor"].as_str().unwrap();
    assert!(std::path::Path::new(gravado).is_absolute(), "{gravado}");
    assert_eq!(
        fs::canonicalize(gravado).unwrap(),
        fs::canonicalize(env.path("certificado.crt")).unwrap()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn interromper_ou_errar_demais_nao_grava_nada() {
    let env = TestEnv::new().await;
    // The end of the input in the middle.
    let assert = env
        .cmd()
        .args(["config", "init", "--interativo"])
        .write_stdin("padrao\nsandbox\n")
        .assert()
        .code(7);
    assert!(
        stderr_of(&assert).contains("assistente interrompido: nada foi gravado"),
        "{}",
        stderr_of(&assert)
    );
    assert!(!env.config_path().exists());

    let assert = env
        .cmd()
        .args(["config", "init", "--interativo"])
        .write_stdin("\nnuvem\nnuvem\nnuvem\nnuvem\nnuvem\n")
        .assert()
        .code(2);
    assert!(
        stderr_of(&assert).contains("5 respostas inválidas seguidas: nada foi gravado"),
        "{}",
        stderr_of(&assert)
    );
    assert!(!env.config_path().exists());
}
