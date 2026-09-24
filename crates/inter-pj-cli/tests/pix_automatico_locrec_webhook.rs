//! `inter-pj pix-automatico locrec ...` and `inter-pj webhook
//! recorrencia|cobranca-recorrente ...` end to end, against a mock API. All
//! data is synthetic.

mod common;

use common::{TestEnv, stderr_of, stdout_of};
use serde_json::{Value, json};
use wiremock::matchers::{any, body_json, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

const ID_REC: &str = "RR1234567820260924abcdefghijk";
const LOCATION: &str = "pix.example.com/qr/v2/rec/2353c790eefb11eaadc10242ac120002";
const URL: &str = "https://api.empresa.example/inter/pix-automatico";

async fn env() -> TestEnv {
    let env = TestEnv::new().await;
    env.write_config("");
    env
}

fn loc(id_rec: Option<&str>) -> Value {
    json!({
        "id": 108,
        "location": LOCATION,
        "criacao": "2099-09-24T13:10:00.000Z",
        "idRec": id_rec
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn cria_lista_e_consulta_locations_de_recorrencias() {
    let env = env().await;
    env.mount_token("payloadlocationrec.write payloadlocationrec.read", Some(1))
        .await;
    Mock::given(method("POST"))
        .and(path("/pix/v2/locrec"))
        .respond_with(ResponseTemplate::new(201).set_body_json(loc(None)))
        .expect(1)
        .mount(&env.server)
        .await;
    let mut outra = loc(Some(ID_REC));
    outra["id"] = json!(109);
    Mock::given(method("GET"))
        .and(path("/pix/v2/locrec"))
        .and(query_param("inicio", "2099-09-01T00:00:00-03:00"))
        .and(query_param("fim", "2099-09-30T23:59:59-03:00"))
        .and(query_param("idRecPresente", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "parametros": {"paginacao": {"paginaAtual": 0, "itensPorPagina": 1000, "quantidadeDePaginas": 1, "quantidadeTotalDeItens": 1}},
            "loc": [outra]
        })))
        .expect(2)
        .mount(&env.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/pix/v2/locrec/108"))
        .respond_with(ResponseTemplate::new(200).set_body_json(loc(None)))
        .expect(1)
        .mount(&env.server)
        .await;

    let stdout = stdout_of(
        &env.cmd()
            .args(["pix-automatico", "locrec", "criar"])
            .assert()
            .success(),
    );
    assert!(
        stdout.starts_with("Location criada.\n\nLocation 108\n"),
        "{stdout}"
    );
    assert!(
        stdout.ends_with(&format!(
            "  Location     {LOCATION}\n  Recorrência  nenhuma\n\nUse com: inter-pj pix-automatico rec criar ... --loc 108\n"
        )),
        "{stdout}"
    );

    let listar = [
        "pix-automatico",
        "locrec",
        "listar",
        "--inicio",
        "2099-09-01T00:00:00-03:00",
        "--fim",
        "2099-09-30T23:59:59-03:00",
        "--com-recorrencia",
    ];
    let stdout = stdout_of(&env.cmd().args(listar).assert().success());
    assert!(
        stdout.starts_with(
            "Locations de recorrências criadas de 01/09/2099 00:00 a 30/09/2099 23:59 (com recorrência)\n\n"
        ),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("  109  {ID_REC}  {LOCATION}")),
        "{stdout}"
    );
    assert!(
        stdout.ends_with("1 location · 1 com recorrência\n"),
        "{stdout}"
    );
    let csv = stdout_of(
        &env.cmd()
            .args(listar)
            .args(["--formato", "csv"])
            .assert()
            .success(),
    );
    assert_eq!(
        csv,
        format!(
            "id,idRec,criacao,location\r\n109,{ID_REC},2099-09-24T13:10:00.000Z,{LOCATION}\r\n"
        )
    );

    let json: Value = serde_json::from_str(&stdout_of(
        &env.cmd()
            .args(["pix-automatico", "locrec", "consultar", "108", "--json"])
            .assert()
            .success(),
    ))
    .unwrap();
    assert_eq!(json["location"], LOCATION);
}

#[tokio::test(flavor = "multi_thread")]
async fn desvincula_a_recorrencia_de_uma_location() {
    let env = env().await;
    env.mount_token("payloadlocationrec.write payloadlocationrec.read", Some(1))
        .await;
    Mock::given(method("GET"))
        .and(path("/pix/v2/locrec/108"))
        .respond_with(ResponseTemplate::new(200).set_body_json(loc(Some(ID_REC))))
        .expect(1)
        .mount(&env.server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/pix/v2/locrec/108/idRec"))
        .respond_with(ResponseTemplate::new(200).set_body_json(loc(None)))
        .expect(1)
        .mount(&env.server)
        .await;
    let assert = env
        .cmd()
        .args(["pix-automatico", "locrec", "desvincular", "108", "--sim"])
        .assert()
        .success();
    let stdout = stdout_of(&assert);
    assert!(
        stdout.starts_with(&format!(
            "Recorrência {ID_REC} desvinculada: a location está livre.\n\nLocation 108\n"
        )),
        "{stdout}"
    );
    let stderr = stderr_of(&assert);
    for linha in [
        "Location 108 a desvincular".to_owned(),
        format!("aviso: o QR Code desta location deixa de levar à recorrência {ID_REC}"),
    ] {
        assert!(stderr.contains(&linha), "{linha}\n{stderr}");
    }
    // Without a terminal to confirm, nothing is looked up.
    env.cmd()
        .args(["pix-automatico", "locrec", "desvincular", "108"])
        .assert()
        .code(2);
}

#[tokio::test(flavor = "multi_thread")]
async fn cadastra_o_webhook_de_recorrencias() {
    let env = env().await;
    env.mount_token("webhookrec.read webhookrec.write", Some(1))
        .await;
    Mock::given(method("GET"))
        .and(path("/pix/v2/webhookrec"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({
            "title": "Não encontrado", "detail": "Webhook não encontrado."
        })))
        .expect(1)
        .mount(&env.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/pix/v2/webhookrec"))
        .and(body_json(json!({"webhookUrl": URL})))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&env.server)
        .await;
    let assert = env
        .cmd()
        .args(["webhook", "recorrencia", "cadastrar", "--url", URL, "--sim"])
        .assert()
        .success();
    assert_eq!(
        stdout_of(&assert),
        format!(
            "Webhook cadastrado: o Inter passa a notificar mudanças de status das recorrências do Pix Automático em {URL}/rec.\n\nConfira com: inter-pj webhook recorrencia consultar\n"
        )
    );
    let resumo = stderr_of(&assert);
    assert!(
        resumo.contains(&format!(
            "Webhook de recorrências a cadastrar\n  Ambiente    sandbox (dados fictícios)\n  Notifica    mudanças de status das recorrências do Pix Automático\n  Nova URL    {URL}\n  Entrega em  {URL}/rec\n"
        )),
        "{resumo}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn consulta_e_exclui_o_webhook_de_cobrancas_recorrentes() {
    let env = env().await;
    env.mount_token("webhookcobr.read webhookcobr.write", Some(1))
        .await;
    Mock::given(method("GET"))
        .and(path("/pix/v2/webhookcobr"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "webhookUrl": URL, "criacao": "2099-09-01T12:00:00Z"
        })))
        .expect(2)
        .mount(&env.server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/pix/v2/webhookcobr"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&env.server)
        .await;
    let stdout = stdout_of(
        &env.cmd()
            .args(["webhook", "cobranca-recorrente", "consultar"])
            .assert()
            .success(),
    );
    assert!(
        stdout.starts_with(&format!(
            "Webhook de cobranças recorrentes\n  Notifica       mudanças de status das cobranças recorrentes do Pix Automático\n  URL            {URL}\n  Entrega em     {URL}/cobr\n  Cadastrado em  "
        )),
        "{stdout}"
    );
    let assert = env
        .cmd()
        .args(["webhook", "cobranca-recorrente", "excluir", "--sim"])
        .assert()
        .success();
    assert_eq!(
        stdout_of(&assert),
        "Webhook excluído: o Inter deixa de notificar mudanças de status das cobranças recorrentes do Pix Automático.\n"
    );
    assert!(
        stderr_of(&assert).contains("Webhook de cobranças recorrentes a excluir"),
        "{}",
        stderr_of(&assert)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn um_cadastro_incerto_orienta_a_conferir() {
    let env = env().await;
    env.mount_token("webhookcobr.read webhookcobr.write", Some(1))
        .await;
    Mock::given(method("GET"))
        .and(path("/pix/v2/webhookcobr"))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&env.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/pix/v2/webhookcobr"))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&env.server)
        .await;
    let assert = env
        .cmd()
        .args([
            "webhook",
            "cobranca-recorrente",
            "cadastrar",
            "--url",
            URL,
            "--sim",
        ])
        .assert()
        .code(6);
    assert!(
        stderr_of(&assert).contains(
            "dica: o webhook pode ter sido cadastrado: confira com inter-pj webhook cobranca-recorrente consultar"
        ),
        "{}",
        stderr_of(&assert)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn sem_terminal_nada_e_consultado() {
    let env = env().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&env.server)
        .await;
    for args in [
        &["webhook", "recorrencia", "cadastrar", "--url", URL][..],
        &["webhook", "recorrencia", "excluir"],
        &["webhook", "cobranca-recorrente", "excluir"],
    ] {
        env.cmd().args(args).assert().code(2);
    }
}
