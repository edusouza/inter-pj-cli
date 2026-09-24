//! `inter-pj auth certificado` and the warning about the validity of the
//! certificate, end to end. The certificates are generated for each test,
//! with synthetic names.

mod common;

use chrono::{Datelike, Days, NaiveDate, Utc};
use common::{TestEnv, saldo_ok, stderr_of, stdout_of, write_private};
use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair, date_time_ymd};
use serde_json::Value;

/// Writes a certificate valid from `inicio` to `fim`, with its key, where
/// the configuration points.
fn certificado(env: &TestEnv, inicio: NaiveDate, fim: NaiveDate) {
    let dia = |data: NaiveDate| {
        date_time_ymd(
            data.year(),
            u8::try_from(data.month()).unwrap(),
            u8::try_from(data.day()).unwrap(),
        )
    };
    let mut params = CertificateParams::new(vec!["cliente.teste".to_owned()]).unwrap();
    params.distinguished_name = DistinguishedName::new();
    params
        .distinguished_name
        .push(DnType::OrganizationName, "Empresa Exemplo Ltda");
    params
        .distinguished_name
        .push(DnType::CommonName, "Integração Exemplo");
    params.not_before = dia(inicio);
    params.not_after = dia(fim);
    let chave = KeyPair::generate().unwrap();
    let certificado = params.self_signed(&chave).unwrap();
    write_private(&env.path("certificado.crt"), &certificado.pem());
    write_private(&env.path("chave.key"), &chave.serialize_pem());
}

fn hoje() -> NaiveDate {
    Utc::now().date_naive()
}

/// The days left until midnight UTC `dias` days from today, as the command
/// counts them: `dias - 1`, or one less if midnight passes meanwhile.
fn restantes(dias: u64) -> [i64; 2] {
    let dias = i64::try_from(dias).unwrap();
    [dias - 1, dias - 2]
}

async fn env() -> TestEnv {
    let env = TestEnv::new().await;
    env.write_config("");
    env
}

#[tokio::test(flavor = "multi_thread")]
async fn mostra_o_certificado_do_perfil() {
    let env = env().await;
    certificado(&env, hoje() - Days::new(165), hoje() + Days::new(200));
    let stdout = stdout_of(&env.cmd().args(["auth", "certificado"]).assert().success());
    assert!(
        stdout.starts_with("Certificado da integração\n  Arquivo          "),
        "{stdout}"
    );
    for linha in [
        "  Titular          CN=Integração Exemplo, O=Empresa Exemplo Ltda\n",
        "  Emissor          CN=Integração Exemplo, O=Empresa Exemplo Ltda\n",
    ] {
        assert!(stdout.contains(linha), "{linha}\n{stdout}");
    }
    assert!(
        restantes(200).iter().any(
            |dias| stdout.contains(&format!("  Situação         válido; faltam {dias} dias\n"))
        ),
        "{stdout}"
    );

    let json: Value = serde_json::from_str(&stdout_of(
        &env.cmd()
            .args(["auth", "certificado", "--json"])
            .assert()
            .success(),
    ))
    .unwrap();
    assert_eq!(json["situacao"], "valido");
    assert!(
        restantes(200).contains(&json["diasRestantes"].as_i64().unwrap()),
        "{json}"
    );
    assert_eq!(
        json["titular"],
        "CN=Integração Exemplo, O=Empresa Exemplo Ltda"
    );
    assert!(json["numeroSerie"].as_str().unwrap().len() >= 2, "{json}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_poucos_dias_do_fim_todo_comando_avisa() {
    let env = env().await;
    certificado(&env, hoje() - Days::new(355), hoje() + Days::new(10));
    let stdout = stdout_of(&env.cmd().args(["auth", "certificado"]).assert().success());
    assert!(
        restantes(10).iter().any(|dias| stdout.contains(&format!(
            "  Situação         vence em {dias} dias: renove-o no Internet Banking PJ"
        ))),
        "{stdout}"
    );

    env.mount_token("extrato.read", Some(1)).await;
    env.mount_saldo(saldo_ok(), 1).await;
    let assert = env.cmd().arg("saldo").assert().success();
    let stderr = stderr_of(&assert);
    assert!(
        restantes(10).iter().any(|dias| stderr.contains(&format!(
            "aviso: o certificado da integração vence em {dias} dias ("
        ))),
        "{stderr}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn um_certificado_vencido_e_apontado() {
    let env = env().await;
    certificado(&env, hoje() - Days::new(368), hoje() - Days::new(3));
    let json: Value = serde_json::from_str(&stdout_of(
        &env.cmd()
            .args(["auth", "certificado", "--json"])
            .assert()
            .success(),
    ))
    .unwrap();
    assert_eq!(json["situacao"], "vencido");
    assert!(json["diasRestantes"].as_i64().unwrap() < 0, "{json}");

    env.mount_token("extrato.read", Some(1)).await;
    env.mount_saldo(saldo_ok(), 1).await;
    let assert = env.cmd().arg("saldo").assert().success();
    assert!(
        stderr_of(&assert).contains("aviso: o certificado da integração venceu em "),
        "{}",
        stderr_of(&assert)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn outro_arquivo_e_um_perfil_sem_certificado() {
    let env = TestEnv::new().await;
    // A profile without the paths of the certificate and of the key.
    std::fs::create_dir_all(env.config_path().parent().unwrap()).unwrap();
    write_private(
        &env.config_path(),
        "perfil_padrao = \"padrao\"\n\n[perfis.padrao]\nambiente = \"sandbox\"\n",
    );
    let assert = env.cmd().args(["auth", "certificado"]).assert().code(3);
    assert!(
        stderr_of(&assert)
            .contains("não tem certificado definido: configure `certificado` ou informe --arquivo"),
        "{}",
        stderr_of(&assert)
    );

    let renovado = env.path("renovado.crt");
    let mut params = CertificateParams::new(vec!["cliente.teste".to_owned()]).unwrap();
    params.not_after = date_time_ymd(2099, 12, 31);
    let chave = KeyPair::generate().unwrap();
    write_private(&renovado, &params.self_signed(&chave).unwrap().pem());
    let stdout = stdout_of(
        &env.cmd()
            .args(["auth", "certificado", "--arquivo"])
            .arg(&renovado)
            .assert()
            .success(),
    );
    assert!(stdout.contains("  Válido até       "), "{stdout}");

    // What is not a certificate is refused.
    let assert = env
        .cmd()
        .args(["auth", "certificado", "--arquivo"])
        .arg(env.config_path())
        .assert()
        .failure();
    assert!(
        stderr_of(&assert).contains("não contém um certificado PEM"),
        "{}",
        stderr_of(&assert)
    );
}
