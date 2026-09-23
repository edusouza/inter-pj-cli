//! End-to-end tests of the `inter-pj` binary (see `common` for the isolated
//! environment). Every credential, certificate and account number here is
//! synthetic.

mod common;

use std::fs;

use common::{CLIENT_ID, CLIENT_SECRET, TOKEN, TestEnv, saldo_ok, stderr_of, stdout_of};
use predicates::prelude::*;
use serde_json::{Value, json};
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

// --- saldo -------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn saldo_em_texto() {
    let env = TestEnv::new().await;
    env.write_config("");
    env.mount_token("extrato.read", Some(1)).await;
    env.mount_saldo(saldo_ok(), 1).await;

    env.cmd()
        .arg("saldo")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Saldo disponível          R$ 2.850,55")
                .and(predicate::str::contains(
                    "Bloqueado em cheque         R$ 240,25",
                ))
                .and(predicate::str::contains(
                    "Limite                    R$ 1.000,00",
                )),
        )
        .stderr(predicate::str::contains("ambiente sandbox"));
}

#[tokio::test(flavor = "multi_thread")]
async fn saldo_em_json_preserva_valores_exatos() {
    let env = TestEnv::new().await;
    env.write_config("");
    env.mount_token("extrato.read", Some(1)).await;
    env.mount_saldo(saldo_ok(), 1).await;

    let assert = env.cmd().args(["saldo", "--json"]).assert().success();
    let json: Value = serde_json::from_str(&stdout_of(&assert)).unwrap();
    assert_eq!(
        json,
        json!({
            "disponivel": 2850.55,
            "bloqueadoCheque": 240.25,
            "bloqueadoJudicialmente": 510.35,
            "bloqueadoAdministrativo": 0,
            "limite": 1000
        })
    );
    assert!(
        !stderr_of(&assert).contains("sandbox"),
        "aviso só na saída em texto"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn saldo_em_uma_data() {
    let env = TestEnv::new().await;
    env.write_config("");
    env.mount_token("extrato.read", Some(1)).await;
    Mock::given(method("GET"))
        .and(path("/banking/v2/saldo"))
        .and(query_param("dataSaldo", "2026-01-02"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"disponivel": 12.3})))
        .expect(1)
        .mount(&env.server)
        .await;

    env.cmd()
        .args(["saldo", "--data", "2026-01-02"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Data da consulta  02/01/2026")
                .and(predicate::str::contains("R$ 12,30"))
                .and(predicate::str::contains("Limite").not()),
        );
}

#[tokio::test(flavor = "multi_thread")]
async fn data_em_formato_invalido_e_erro_de_uso() {
    let env = TestEnv::new().await;
    env.write_config("");
    env.cmd()
        .args(["saldo", "--data", "02/01/2026"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("AAAA-MM-DD"));
}

#[tokio::test(flavor = "multi_thread")]
async fn conta_corrente_do_perfil_e_da_flag_vao_no_cabecalho() {
    let env = TestEnv::new().await;
    env.write_config("conta_corrente = \"7654321\"");
    env.mount_token("extrato.read", None).await;
    for conta in ["7654321", "1234567"] {
        Mock::given(method("GET"))
            .and(path("/banking/v2/saldo"))
            .and(header("x-conta-corrente", conta))
            .respond_with(saldo_ok())
            .expect(1)
            .mount(&env.server)
            .await;
    }

    env.cmd().arg("saldo").assert().success();
    env.cmd()
        .args(["saldo", "--conta-corrente", "1234567"])
        .assert()
        .success();
}

// --- cache de token -------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn token_em_cache_e_reaproveitado_entre_execucoes() {
    let env = TestEnv::new().await;
    env.write_config("");
    env.mount_token("extrato.read", Some(1)).await;
    env.mount_saldo(saldo_ok(), 2).await;

    env.cmd().arg("saldo").assert().success();
    env.cmd().arg("saldo").assert().success();

    let files = env.cache_files();
    assert_eq!(files.len(), 1, "{files:?}");
    let name = files[0].file_name().unwrap().to_string_lossy().into_owned();
    assert!(
        !name.contains(CLIENT_ID),
        "o client_id não pode aparecer no nome do arquivo"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&files[0]).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn sem_cache_pede_token_a_cada_execucao() {
    let env = TestEnv::new().await;
    env.write_config("");
    env.mount_token("extrato.read", Some(2)).await;
    env.mount_saldo(saldo_ok(), 2).await;

    env.cmd().args(["saldo", "--sem-cache"]).assert().success();
    env.cmd().args(["saldo", "--sem-cache"]).assert().success();
    assert!(env.cache_files().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn auth_limpar_remove_o_cache() {
    let env = TestEnv::new().await;
    env.write_config("");
    env.mount_token("extrato.read", Some(2)).await;
    env.mount_saldo(saldo_ok(), 2).await;

    env.cmd().arg("saldo").assert().success();
    assert_eq!(env.cache_files().len(), 1);
    env.cmd()
        .args(["auth", "limpar"])
        .assert()
        .success()
        .stdout(predicate::str::contains("removidos"));
    assert!(env.cache_files().is_empty());
    env.cmd()
        .args(["auth", "limpar"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Não havia tokens"));

    env.cmd().arg("saldo").assert().success();
    env.cmd()
        .args(["auth", "limpar", "--todos"])
        .assert()
        .success()
        .stdout(predicate::str::contains("1 arquivo(s)"));
}

// --- auth token ------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn auth_token_mostra_escopos_e_validade_sem_revelar_o_token() {
    let env = TestEnv::new().await;
    env.write_config("");
    env.mount_token("extrato.read", Some(1)).await;

    let assert = env
        .cmd()
        .args(["auth", "token", "--escopo", "extrato.read"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Token de acesso válido")
                .and(predicate::str::contains("extrato.read"))
                .and(predicate::str::contains("Expira em")),
        );
    assert!(!stdout_of(&assert).contains(TOKEN));

    let assert = env
        .cmd()
        .args(["auth", "token", "--escopo", "extrato.read", "--json"])
        .assert()
        .success();
    let json: Value = serde_json::from_str(&stdout_of(&assert)).unwrap();
    assert_eq!(json["escopos"], "extrato.read");
    assert_eq!(json["ambiente"], "sandbox");
    assert!(json["expiraEm"].is_string());
    assert!(!stdout_of(&assert).contains(TOKEN));
}

#[tokio::test(flavor = "multi_thread")]
async fn auth_token_exibir_imprime_somente_o_token() {
    let env = TestEnv::new().await;
    env.write_config("");
    env.mount_token("extrato.read", Some(1)).await;

    env.cmd()
        .args(["auth", "token", "--escopo", "extrato.read", "--exibir"])
        .assert()
        .success()
        .stdout(format!("{TOKEN}\n"));
}

#[tokio::test(flavor = "multi_thread")]
async fn auth_token_exige_escopo_valido() {
    let env = TestEnv::new().await;
    env.write_config("");
    env.cmd()
        .args(["auth", "token"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("--escopo"));
    env.cmd()
        .args(["auth", "token", "--escopo", "banana.read"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("escopo desconhecido"));
}

#[tokio::test(flavor = "multi_thread")]
async fn auth_token_usa_escopos_do_perfil() {
    let env = TestEnv::new().await;
    env.write_config("escopos = [\"extrato.read\", \"pix.read\"]");
    env.mount_token("extrato.read pix.read", Some(1)).await;

    env.cmd()
        .args(["auth", "token"])
        .assert()
        .success()
        .stdout(predicate::str::contains("extrato.read pix.read"));
}

// --- segredos ---------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn segredos_nunca_aparecem_na_saida_nem_nos_logs() {
    let env = TestEnv::new().await;
    env.write_config("conta_corrente = \"7654321\"");
    env.mount_token("extrato.read", None).await;
    Mock::given(method("GET"))
        .and(path("/banking/v2/saldo"))
        .respond_with(saldo_ok())
        .mount(&env.server)
        .await;

    let runs: [&[&str]; 6] = [
        &["-vv", "saldo"],
        &["-vv", "saldo", "--json", "--sem-cache"],
        &[
            "-vvv",
            "auth",
            "token",
            "--escopo",
            "extrato.read",
            "--renovar",
        ],
        &["-vv", "config", "mostrar"],
        &["config", "mostrar", "--json"],
        &["-vv", "saldo", "--client-id", "outro-client-id-e2e"],
    ];
    for args in runs {
        let assert = env.cmd().args(args).assert().success();
        let output = format!("{}{}", stdout_of(&assert), stderr_of(&assert));
        for secret in [
            CLIENT_SECRET,
            TOKEN,
            "7654321",
            CLIENT_ID,
            "outro-client-id-e2e",
        ] {
            assert!(
                !output.contains(secret),
                "{secret:?} vazou em {args:?}:\n{output}"
            );
        }
    }

    // Sanity check: -vv really produces logs.
    let assert = env.cmd().args(["-vv", "saldo"]).assert().success();
    assert!(
        stderr_of(&assert).contains("GET /banking/v2/saldo → 200"),
        "{}",
        stderr_of(&assert)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn segredo_nao_vaza_em_falha_de_autenticacao() {
    let env = TestEnv::new().await;
    env.write_config("");
    Mock::given(method("POST"))
        .and(path("/oauth/v2/token"))
        .respond_with(ResponseTemplate::new(401).set_body_json(
            json!({"error": "invalid_client", "error_description": "Client authentication failed"}),
        ))
        .mount(&env.server)
        .await;

    let assert = env.cmd().args(["-vv", "saldo"]).assert().code(4).stderr(
        predicate::str::contains("falha ao obter o token de acesso")
            .and(predicate::str::contains("invalid_client"))
            .and(predicate::str::contains("dica: confira o client_id")),
    );
    let output = format!("{}{}", stdout_of(&assert), stderr_of(&assert));
    assert!(!output.contains(CLIENT_SECRET), "{output}");
}

// --- erros da API -------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn erros_da_api_viram_mensagens_e_codigos_de_saida() {
    let env = TestEnv::new().await;
    env.write_config("");
    let cases = [
        (
            ResponseTemplate::new(400).set_body_json(json!({
                "title": "Dados inválidos.",
                "detail": "Verifique os dados informados.",
                "violacoes": [{"razao": "Não foi possível converter o valor.", "propriedade": "dataSaldo"}]
            })),
            5,
            "dataSaldo: Não foi possível converter o valor.",
        ),
        (
            ResponseTemplate::new(403).set_body_json(json!({"title": "Acesso Negado", "status": "403"})),
            4,
            "escopos necessários",
        ),
        (ResponseTemplate::new(429), 6, "aguarde um minuto"),
        (
            ResponseTemplate::new(503).set_body_json(json!({"title": "Serviço Indisponível"})),
            6,
            "manutenção",
        ),
    ];
    for (response, code, message) in cases {
        env.server.reset().await;
        env.mount_token("extrato.read", None).await;
        env.mount_saldo(response, 1).await;
        env.cmd()
            .args(["saldo", "--sem-cache", "--sem-retentativa"])
            .assert()
            .code(code)
            .stderr(predicate::str::contains(message).and(predicate::str::starts_with("erro: ")));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn falhas_temporarias_do_saldo_sao_repetidas() {
    let env = TestEnv::new().await;
    env.write_config("");
    env.mount_token("extrato.read", None).await;
    Mock::given(method("GET"))
        .and(path("/banking/v2/saldo"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .expect(1)
        .mount(&env.server)
        .await;
    env.mount_saldo(saldo_ok(), 1).await;

    let assert = env
        .cmd()
        .args(["saldo", "-v", "--tentativas", "2"])
        .assert()
        .success();
    let err = stderr_of(&assert);
    assert!(err.contains("resposta 503; tentativa 2 de 2"), "{err}");
    assert!(stdout_of(&assert).contains("R$ 2.850,55"));
}

#[tokio::test(flavor = "multi_thread")]
async fn escopo_nao_concedido_e_falha_de_autenticacao() {
    let env = TestEnv::new().await;
    env.write_config("");
    env.mount_token("pix.read", Some(1)).await;

    env.cmd()
        .arg("saldo")
        .assert()
        .code(4)
        .stderr(predicate::str::contains("sem os escopos extrato.read"));
}

// --- configuração ---------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn configuracao_incompleta_lista_o_que_falta() {
    let env = TestEnv::new().await;
    env.cmd().arg("saldo").assert().code(3).stderr(
        predicate::str::contains("configuração incompleta")
            .and(predicate::str::contains("client_id"))
            .and(predicate::str::contains("inter-pj config init")),
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn valores_invalidos_sao_erros_de_configuracao() {
    let env = TestEnv::new().await;
    env.write_config("");
    let cases: [(&[&str], &str); 4] = [
        (&["saldo", "--ambiente", "homologacao"], "ambiente inválido"),
        (
            &["saldo", "--certificado", "nao-existe.crt"],
            "nao-existe.crt",
        ),
        (
            &["saldo", "--perfil", "outro"],
            "perfil \"outro\" não encontrado",
        ),
        (
            &["saldo", "--conta-corrente", "00123"],
            "conta corrente inválida",
        ),
    ];
    for (args, message) in cases {
        env.cmd()
            .args(args)
            .assert()
            .code(3)
            .stderr(predicate::str::contains(message));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn url_base_http_externa_e_recusada() {
    let env = TestEnv::new().await;
    env.write_config("");
    env.cmd()
        .env("INTER_BASE_URL", "http://exemplo.com.br")
        .arg("saldo")
        .assert()
        .code(3)
        .stderr(predicate::str::contains("https"));
}

#[tokio::test(flavor = "multi_thread")]
async fn config_init_cria_modelo_privado_e_nao_sobrescreve() {
    let env = TestEnv::new().await;
    env.cmd()
        .args(["config", "init"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Arquivo de configuração criado"));
    let content = fs::read_to_string(env.config_path()).unwrap();
    assert!(content.contains("perfil_padrao = \"padrao\""));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(env.config_path())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    env.cmd()
        .args(["config", "init"])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("já existe"));
    env.cmd()
        .args(["config", "init", "--forcar"])
        .assert()
        .success();

    // The template alone is not enough to call the API, and says why.
    env.cmd()
        .arg("saldo")
        .assert()
        .code(3)
        .stderr(predicate::str::contains("client_id").and(predicate::str::contains("certificado")));
}

#[tokio::test(flavor = "multi_thread")]
async fn config_caminho_mostra_configuracao_e_cache() {
    let env = TestEnv::new().await;
    let assert = env
        .cmd()
        .args(["config", "caminho", "--json"])
        .assert()
        .success();
    let json: Value = serde_json::from_str(&stdout_of(&assert)).unwrap();
    assert_eq!(
        json["configuracao"],
        env.config_path().display().to_string()
    );
    assert_eq!(json["configuracaoExiste"], false);
    assert_eq!(json["cache"], env.cache_dir().display().to_string());

    env.cmd()
        .args(["config", "caminho"])
        .assert()
        .success()
        .stdout(predicate::str::contains("(não existe)"));
}

#[tokio::test(flavor = "multi_thread")]
async fn config_mostrar_indica_origem_e_oculta_segredos() {
    let env = TestEnv::new().await;
    env.write_config(
        "client_secret = \"segredo-gravado-no-arquivo\"\nconta_corrente = \"7654321\"",
    );

    let assert = env
        .cmd()
        .env_remove("INTER_CLIENT_SECRET")
        .env("INTER_AMBIENTE", "producao")
        .args(["config", "mostrar"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("producao (variável INTER_AMBIENTE)")
                .and(predicate::str::contains("definido (oculto) (arquivo)"))
                .and(predicate::str::contains("*****21 (arquivo)"))
                .and(predicate::str::contains(
                    "Escopos adicionais   (não definido)",
                ))
                .and(predicate::str::contains(
                    "Limite por operação  (não definido)",
                )),
        );
    let output = format!("{}{}", stdout_of(&assert), stderr_of(&assert));
    assert!(!output.contains("segredo-gravado-no-arquivo"), "{output}");
    assert!(!output.contains("7654321"), "{output}");

    env.cmd()
        .args(["config", "mostrar", "--ambiente", "sandbox"])
        .assert()
        .success()
        .stdout(predicate::str::contains("sandbox (flag)"));
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn avisa_quando_arquivos_com_segredos_estao_abertos() {
    use std::os::unix::fs::PermissionsExt;

    let env = TestEnv::new().await;
    env.write_config("client_secret = \"segredo-gravado-no-arquivo\"");
    fs::set_permissions(env.config_path(), fs::Permissions::from_mode(0o644)).unwrap();
    fs::set_permissions(env.path("chave.key"), fs::Permissions::from_mode(0o644)).unwrap();

    env.cmd()
        .args(["config", "mostrar"])
        .assert()
        .success()
        .stderr(
            predicate::str::contains("aviso: o arquivo de configuração contém o client_secret")
                .and(predicate::str::contains(
                    "aviso: a chave privada pode ser lida",
                )),
        );
}

// --- ajuda ------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn ajuda_e_versao() {
    let env = TestEnv::new().await;
    env.cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Comandos:").and(predicate::str::contains("saldo")));
    env.cmd()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
    env.cmd()
        .args(["saldo", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Uso: inter-pj saldo [OPÇÕES]"));
}
