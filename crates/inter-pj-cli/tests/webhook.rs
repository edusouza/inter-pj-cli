//! `inter-pj webhook banking|cobranca|pix cadastrar|consultar|excluir` end
//! to end, against a mock API. All data is synthetic.

mod common;

use common::{TestEnv, stderr_of, stdout_of};
use serde_json::{Value, json};
use wiremock::matchers::{any, body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

const URL: &str = "https://api.empresa.example/inter/webhook";
const NOVA: &str = "https://novo.empresa.example/inter";

async fn env() -> TestEnv {
    let env = TestEnv::new().await;
    env.write_config("");
    env
}

/// The API must not be called at all.
async fn nothing_is_sent(env: &TestEnv) {
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&env.server)
        .await;
}

async fn consulta(env: &TestEnv, caminho: &str, resposta: Option<Value>) {
    let resposta = match resposta {
        Some(webhook) => ResponseTemplate::new(200).set_body_json(webhook),
        None => ResponseTemplate::new(404).set_body_json(json!({
            "title": "Não encontrado", "detail": "Webhook não encontrado."
        })),
    };
    Mock::given(method("GET"))
        .and(path(caminho))
        .respond_with(resposta)
        .mount(&env.server)
        .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn cadastra_o_webhook_de_um_tipo_do_banking() {
    let env = env().await;
    env.mount_token("webhook-banking.read webhook-banking.write", None)
        .await;
    consulta(&env, "/banking/v2/webhooks/pix-pagamento", None).await;
    Mock::given(method("PUT"))
        .and(path("/banking/v2/webhooks/pix-pagamento"))
        .and(body_json(json!({"webhookUrl": URL})))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&env.server)
        .await;
    let assert = env
        .cmd()
        .args([
            "webhook",
            "banking",
            "cadastrar",
            "pix-pagamento",
            "--url",
            URL,
            "--sim",
        ])
        .assert()
        .success();
    assert_eq!(
        stdout_of(&assert),
        format!(
            "Webhook cadastrado: o Inter passa a notificar Pix enviados pela conta em {URL}.\n\nConfira com: inter-pj webhook banking consultar pix-pagamento\n"
        )
    );
    let resumo = stderr_of(&assert);
    assert!(
        resumo.contains(
            "Webhook do tipo pix-pagamento a cadastrar\n  Ambiente  sandbox (dados fictícios)\n  Notifica  Pix enviados pela conta\n  Nova URL  https://api.empresa.example/inter/webhook\n"
        ),
        "{resumo}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn consulta_os_dois_tipos_do_banking() {
    let env = env().await;
    env.mount_token("webhook-banking.read", None).await;
    consulta(
        &env,
        "/banking/v2/webhooks/pix-pagamento",
        Some(json!({"webhookUrl": URL, "criacao": "2026-09-24T13:15:00Z"})),
    )
    .await;
    consulta(&env, "/banking/v2/webhooks/boleto-pagamento", None).await;
    let texto = stdout_of(
        &env.cmd()
            .args(["webhook", "banking", "consultar"])
            .assert()
            .success(),
    );
    assert!(
        texto.starts_with(&format!(
            "Webhook do tipo pix-pagamento\n  Notifica       Pix enviados pela conta\n  URL            {URL}\n  Cadastrado em  "
        )),
        "{texto}"
    );
    assert!(
        texto.ends_with("\n\nWebhook do tipo boleto-pagamento\n  Nenhum webhook cadastrado: o Inter não notifica boletos pagos pela conta.\n  Para cadastrar: inter-pj webhook banking cadastrar boleto-pagamento --url https://...\n"),
        "{texto}"
    );
    let json: Value = serde_json::from_str(&stdout_of(
        &env.cmd()
            .args(["webhook", "banking", "consultar", "--json"])
            .assert()
            .success(),
    ))
    .unwrap();
    assert_eq!(
        json,
        json!({
            "pix-pagamento": {"webhookUrl": URL, "criacao": "2026-09-24T13:15:00Z"},
            "boleto-pagamento": null
        })
    );
    let json: Value = serde_json::from_str(&stdout_of(
        &env.cmd()
            .args([
                "webhook",
                "banking",
                "consultar",
                "boleto-pagamento",
                "--json",
            ])
            .assert()
            .success(),
    ))
    .unwrap();
    assert_eq!(json, Value::Null);
}

#[tokio::test(flavor = "multi_thread")]
async fn troca_a_url_do_webhook_de_cobrancas() {
    let env = env().await;
    env.mount_token("boleto-cobranca.read boleto-cobranca.write", None)
        .await;
    consulta(
        &env,
        "/cobranca/v3/cobrancas/webhook",
        Some(json!({"webhookUrl": URL, "criacao": "2026-09-01T12:00:00Z"})),
    )
    .await;
    Mock::given(method("PUT"))
        .and(path("/cobranca/v3/cobrancas/webhook"))
        .and(body_json(json!({"webhookUrl": NOVA})))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&env.server)
        .await;
    let assert = env
        .cmd()
        .args([
            "webhook",
            "cobranca",
            "cadastrar",
            "--url",
            NOVA,
            "--sim",
            "--json",
        ])
        .assert()
        .success();
    let json: Value = serde_json::from_str(&stdout_of(&assert)).unwrap();
    assert_eq!(json, json!({"webhookUrl": NOVA}));
    let resumo = stderr_of(&assert);
    assert!(
        resumo.contains(&format!(
            "Webhook de cobranças a trocar\n  Ambiente   sandbox (dados fictícios)\n  Notifica   cobranças recebidas, canceladas e expiradas\n  URL atual  {URL}\n  Nova URL   {NOVA}\naviso: as notificações passam a ir para novo.empresa.example, e não mais para api.empresa.example\n"
        )),
        "{resumo}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn exclui_o_webhook_de_uma_chave_pix() {
    let env = env().await;
    env.mount_token("webhook.read webhook.write", None).await;
    // Phones go in the path without the `+`.
    consulta(
        &env,
        "/pix/v2/webhook/5511912345678",
        Some(json!({"webhookUrl": URL, "chave": "+5511912345678", "criacao": "2026-09-24T13:15:00Z"})),
    )
    .await;
    Mock::given(method("DELETE"))
        .and(path("/pix/v2/webhook/5511912345678"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&env.server)
        .await;
    let assert = env
        .cmd()
        .args(["webhook", "pix", "excluir", "+55 (11) 91234-5678", "--sim"])
        .assert()
        .success();
    assert_eq!(
        stdout_of(&assert),
        "Webhook excluído: o Inter deixa de notificar cobranças Pix pagas (imediatas e com vencimento).\n"
    );
    let resumo = stderr_of(&assert);
    assert!(
        resumo.starts_with("Webhook da chave +5511912345678 a excluir\n"),
        "{resumo}"
    );
    assert!(
        resumo.contains(
            "aviso: o Inter deixa de notificar cobranças Pix pagas (imediatas e com vencimento)\n"
        ),
        "{resumo}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn sem_webhook_a_consulta_diz_como_cadastrar() {
    let env = env().await;
    env.mount_token("boleto-cobranca.read", None).await;
    consulta(&env, "/cobranca/v3/cobrancas/webhook", None).await;
    let assert = env
        .cmd()
        .args(["webhook", "cobranca", "consultar"])
        .assert()
        .success();
    assert_eq!(
        stdout_of(&assert),
        "Webhook de cobranças\n  Nenhum webhook cadastrado: o Inter não notifica cobranças recebidas, canceladas e expiradas.\n  Para cadastrar: inter-pj webhook cobranca cadastrar --url https://...\n"
    );
    // The sandbox is announced, as in every lookup.
    assert!(
        stderr_of(&assert).contains("sandbox"),
        "{}",
        stderr_of(&assert)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn sem_terminal_nem_sim_nada_e_enviado() {
    let env = env().await;
    nothing_is_sent(&env).await;
    for args in [
        &["webhook", "cobranca", "cadastrar", "--url", URL][..],
        &["webhook", "cobranca", "excluir"][..],
        &["webhook", "banking", "excluir", "boleto-pagamento"][..],
        &[
            "webhook",
            "pix",
            "cadastrar",
            "pix@empresa.example",
            "--url",
            URL,
        ][..],
    ] {
        let assert = env.cmd().args(args).assert().code(2);
        assert!(
            stderr_of(&assert).contains("confirmação necessária"),
            "{args:?}: {}",
            stderr_of(&assert)
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn enderecos_e_chaves_invalidos_sao_recusados_antes_do_envio() {
    let env = env().await;
    nothing_is_sent(&env).await;
    for (args, mensagem) in [
        (
            &[
                "webhook",
                "cobranca",
                "cadastrar",
                "--url",
                "http://api.empresa.example/inter",
                "--sim",
            ][..],
            "a URL do webhook precisa começar com https://",
        ),
        (
            &[
                "webhook",
                "cobranca",
                "cadastrar",
                "--url",
                "https://",
                "--sim",
            ][..],
            "URL do webhook inválida: falta o servidor",
        ),
        (&["webhook", "pix", "consultar", "11912345678"][..], "+55"),
        (&["webhook", "banking", "consultar", "pix"][..], "pix"),
    ] {
        let assert = env.cmd().args(args).assert().code(2);
        assert!(
            stderr_of(&assert).contains(mensagem),
            "{args:?}: {}",
            stderr_of(&assert)
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn um_cadastro_incerto_diz_como_conferir() {
    let env = env().await;
    env.mount_token("boleto-cobranca.read boleto-cobranca.write", None)
        .await;
    consulta(&env, "/cobranca/v3/cobrancas/webhook", None).await;
    Mock::given(method("PUT"))
        .and(path("/cobranca/v3/cobrancas/webhook"))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&env.server)
        .await;
    let assert = env
        .cmd()
        .args(["webhook", "cobranca", "cadastrar", "--url", URL, "--sim"])
        .assert()
        .failure();
    assert!(
        stderr_of(&assert).contains(
            "dica: o webhook pode ter sido cadastrado: confira com inter-pj webhook cobranca consultar"
        ),
        "{}",
        stderr_of(&assert)
    );
}
