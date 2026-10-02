//! `inter-pj pix enviar`

use std::fmt::Write as _;

use chrono::{Local, NaiveDate};
use inter_pj::banking::{
    Destinatario, IdIdempotente, PagamentoPix, SolicitacaoPix, TipoRetornoPix,
};
use inter_pj::documento::Documento;
use inter_pj::pix::ChavePix;
use inter_pj::{Environment, Error as InterError, endpoint};
use serde_json::{Map, json};

use crate::cli::{Formato, PixEnviarArgs};
use crate::commands::Context;
use crate::config::Settings;
use crate::confirmacao::{Terminal, confirmar};
use crate::error::CliError;
use crate::output;
use crate::valor::por_extenso;

pub(super) async fn run(
    context: &Context,
    args: &PixEnviarArgs,
    terminal: &mut dyn Terminal,
) -> Result<(), CliError> {
    let pagamento = pagamento(args, Local::now().date_naive())?;
    let settings = context.settings()?;
    verificar_limite(&pagamento, &settings)?;
    // Configuration problems show up before the confirmation, not after it.
    let client = if args.simular {
        None
    } else {
        Some(context.client(&settings)?)
    };
    let id = args
        .id_idempotente
        .clone()
        .unwrap_or_else(IdIdempotente::novo);
    let ambiente = settings.ambiente.as_ref().map(|setting| setting.value);
    eprintln!("{}", resumo(&pagamento, ambiente, &id));

    let Some(client) = client else {
        return simulacao(context, &settings, &pagamento, &id);
    };
    confirmar(terminal, args.sim, "Enviar o Pix?")?;
    let solicitacao = client
        .banking()
        .enviar_pix(&pagamento, &id)
        .await
        .map_err(|err| {
            if resultado_incerto(&err) {
                CliError::ResultadoIncerto {
                    source: err,
                    id_idempotente: id.to_string(),
                }
            } else {
                err.into()
            }
        })?;
    match context.formato() {
        Formato::Json => {
            let mut value = serde_json::to_value(&solicitacao).unwrap_or_else(|_| json!({}));
            value["idIdempotente"] = json!(id.as_str());
            output::print_json(&value)
        }
        // `commands::run` refuses csv for this command.
        Formato::Texto | Formato::Csv => output::print(&render(&solicitacao, &id)),
    }
}

/// The payment described by the arguments, validated.
fn pagamento(args: &PixEnviarArgs, hoje: NaiveDate) -> Result<PagamentoPix, CliError> {
    if let Some(data) = args.data
        && data < hoje
    {
        return Err(CliError::Usage(format!(
            "o dia {} já passou: agende para hoje ou depois",
            data.format("%d/%m/%Y")
        )));
    }
    let mut pagamento = PagamentoPix::new(
        args.valor,
        Destinatario::Chave {
            chave: args.chave.clone(),
        },
    );
    // Without a date the API pays at once: send it only to schedule.
    pagamento.data_pagamento = args.data.filter(|data| *data > hoje);
    pagamento.descricao = args
        .descricao
        .as_deref()
        .map(str::trim)
        .filter(|descricao| !descricao.is_empty())
        .map(str::to_owned);
    pagamento
        .validar()
        .map_err(|err| CliError::Usage(err.to_string()))?;
    Ok(pagamento)
}

/// Refuses amounts above the profile's `limite_por_operacao`, even with `--sim`.
fn verificar_limite(pagamento: &PagamentoPix, settings: &Settings) -> Result<(), CliError> {
    match &settings.limite_por_operacao {
        Some(limite) if pagamento.valor > limite.value => Err(CliError::Usage(format!(
            "{} passa do limite por operação do perfil \"{}\" ({}); para enviar, ajuste limite_por_operacao em {}",
            output::brl(pagamento.valor),
            settings.perfil.value,
            output::brl(limite.value),
            settings.config_path.display()
        ))),
        _ => Ok(()),
    }
}

/// What is about to be sent, for the person to check before confirming.
fn resumo(pagamento: &PagamentoPix, ambiente: Option<Environment>, id: &IdIdempotente) -> String {
    let producao = ambiente.is_some_and(Environment::is_production);
    let mut linhas = vec![(
        "Ambiente",
        match ambiente {
            Some(_) if producao => "PRODUÇÃO (conta real)".to_owned(),
            Some(_) => "sandbox (dados fictícios)".to_owned(),
            None => "não definido".to_owned(),
        },
    )];
    let mut avisos = Vec::new();
    if let Destinatario::Chave { chave } = &pagamento.destinatario {
        linhas.push(("Chave Pix", descrever_chave(chave)));
        if let ChavePix::Cpf(cpf) = chave
            && parece_celular(cpf)
        {
            avisos.push(
                "a chave foi lida como CPF; se for um celular, use +55 e o DDD (+55DD9NNNNNNNN)",
            );
        }
    }
    let extenso = por_extenso(pagamento.valor)
        .map(|extenso| format!(" ({extenso})"))
        .unwrap_or_default();
    linhas.push((
        "Valor",
        format!("{}{extenso}", output::brl(pagamento.valor)),
    ));
    linhas.push((
        "Quando",
        pagamento.data_pagamento.map_or_else(
            || "agora".to_owned(),
            |data| format!("agendado para {}", data.format("%d/%m/%Y")),
        ),
    ));
    if let Some(descricao) = &pagamento.descricao {
        linhas.push(("Descrição", descricao.clone()));
    }
    linhas.push(("Chave de idempotência", id.to_string()));

    let mut texto = String::new();
    if producao {
        texto.push_str("*** PRODUÇÃO: este Pix movimenta dinheiro da conta real ***\n");
    }
    texto.push_str("Pix a enviar");
    for linha in output::key_values_left(&linhas).lines() {
        let _ = write!(texto, "\n  {linha}");
    }
    for aviso in avisos {
        let _ = write!(texto, "\naviso: {aviso}");
    }
    texto
}

/// The key with its kind, formatted for reading: `123.456.789-09 (CPF)`.
fn descrever_chave(chave: &ChavePix) -> String {
    let (valor, tipo) = match chave {
        ChavePix::Cpf(cpf) => (formatar_documento(cpf), "CPF"),
        ChavePix::Cnpj(cnpj) => (formatar_documento(cnpj), "CNPJ"),
        ChavePix::Email(email) => (email.clone(), "e-mail"),
        ChavePix::Telefone(telefone) => (formatar_telefone(telefone), "celular"),
        ChavePix::Aleatoria(evp) => (evp.clone(), "chave aleatória"),
    };
    format!("{valor} ({tipo})")
}

fn formatar_documento(digitos: &str) -> String {
    Documento::parse(digitos).map_or_else(|_| digitos.to_owned(), |doc| doc.formatado())
}

/// `+5511912345678` -> `+55 (11) 91234-5678`.
fn formatar_telefone(telefone: &str) -> String {
    match (telefone.get(3..5), telefone.get(5..10), telefone.get(10..)) {
        (Some(ddd), Some(inicio), Some(fim)) if telefone.len() == 14 => {
            format!("+55 ({ddd}) {inicio}-{fim}")
        }
        _ => telefone.to_owned(),
    }
}

/// Eleven digits that are also an area code and a mobile number.
fn parece_celular(cpf: &str) -> bool {
    let digitos = cpf.as_bytes();
    digitos.len() == 11 && digitos[0] != b'0' && digitos[1] != b'0' && digitos[2] == b'9'
}

/// `--simular`: the request that would be sent, without sending it.
fn simulacao(
    context: &Context,
    settings: &Settings,
    pagamento: &PagamentoPix,
    id: &IdIdempotente,
) -> Result<(), CliError> {
    let base = settings
        .effective_base_url()
        .unwrap_or_else(|| "<URL do ambiente>".to_owned());
    let url = format!(
        "{}{}",
        base.trim_end_matches('/'),
        endpoint::banking::PIX_INCLUIR.path
    );
    let corpo = serde_json::to_value(pagamento)
        .map_err(|err| CliError::io("falha ao gerar JSON", std::io::Error::other(err)))?;
    let mut cabecalhos = Map::new();
    cabecalhos.insert("x-id-idempotente".to_owned(), json!(id.as_str()));
    if let Some(conta) = &settings.conta_corrente {
        cabecalhos.insert(
            "x-conta-corrente".to_owned(),
            json!(output::mask(&conta.value, 2)),
        );
    }
    match context.formato() {
        Formato::Json => output::print_json(&json!({
            "simulacao": true,
            "metodo": "POST",
            "url": url,
            "cabecalhos": cabecalhos,
            "corpo": corpo,
        })),
        Formato::Texto | Formato::Csv => {
            let mut texto = format!("Simulação: nada foi enviado.\n\nPOST {url}");
            for (nome, valor) in &cabecalhos {
                let _ = write!(texto, "\n{nome}: {}", valor.as_str().unwrap_or_default());
            }
            let corpo = serde_json::to_string_pretty(&corpo).unwrap_or_default();
            let _ = write!(texto, "\n\n{corpo}");
            output::print(&texto)
        }
    }
}

/// Whether the payment may have been made despite the error.
fn resultado_incerto(err: &InterError) -> bool {
    match err {
        // A refused connection never reached the API.
        InterError::Transport(source) => !source.is_connect(),
        InterError::Api(api) => api.status >= 500,
        // A success status with a body that could not be read.
        InterError::Decode { .. } => true,
        _ => false,
    }
}

fn render(solicitacao: &SolicitacaoPix, id: &IdIdempotente) -> String {
    let data_pagamento = solicitacao.data_pagamento.as_deref().map(data_br);
    let titulo = match &solicitacao.tipo_retorno {
        Some(TipoRetornoPix::Processado) => "Pix enviado.".to_owned(),
        Some(TipoRetornoPix::Agendado) => match &data_pagamento {
            Some(data) => format!("Pix agendado para {data}."),
            None => "Pix agendado.".to_owned(),
        },
        Some(TipoRetornoPix::Aprovacao) => "Pix aguardando aprovação no Internet Banking (Aprovar > Gestão de Aprovações): só será enviado depois de aprovado.".to_owned(),
        Some(outro) => format!("Pix registrado (retorno da API: {outro})."),
        None => "Pix registrado.".to_owned(),
    };
    let mut linhas = Vec::new();
    if let Some(codigo) = &solicitacao.codigo_solicitacao {
        linhas.push(("Código da solicitação", codigo.clone()));
    }
    if let Some(data) = data_pagamento {
        linhas.push(("Data do pagamento", data));
    }
    if let Some(data) = &solicitacao.data_operacao {
        linhas.push(("Data da operação", data_br(data)));
    }
    linhas.push(("Chave de idempotência", id.to_string()));
    format!("{titulo}\n{}", output::key_values_left(&linhas))
}

/// `2026-09-23` -> `23/09/2026`; other formats as received.
fn data_br(raw: &str) -> String {
    raw.get(..10)
        .and_then(|iso| NaiveDate::parse_from_str(iso, "%Y-%m-%d").ok())
        .map_or_else(
            || raw.to_owned(),
            |data| data.format("%d/%m/%Y").to_string(),
        )
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;

    use clap::{CommandFactory, FromArgMatches, Parser};
    use wiremock::matchers::{any, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::cli::{Cli, Command, PixCommand};
    use crate::commands::Env;
    use crate::confirmacao::testes::TerminalFalso;

    const ID: &str = "123e4567-e89b-42d3-a456-426614174000";

    fn args(extra: &[&str]) -> PixEnviarArgs {
        let mut full = vec!["inter-pj", "pix", "enviar"];
        full.extend_from_slice(extra);
        match Cli::try_parse_from(full).unwrap().command {
            Command::Pix(PixCommand::Enviar(args)) => args,
            other => panic!("comando inesperado: {other:?}"),
        }
    }

    fn hoje() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, 23).unwrap()
    }

    fn id() -> IdIdempotente {
        ID.parse().unwrap()
    }

    #[test]
    fn builds_and_validates_the_payment() {
        let construido = pagamento(
            &args(&[
                "--chave",
                "fornecedor@exemplo.com",
                "--valor",
                "150,00",
                "--descricao",
                "  NF 123 ",
                "--data",
                "2026-10-01",
            ]),
            hoje(),
        )
        .unwrap();
        assert_eq!(construido.valor, "150.00".parse().unwrap());
        assert_eq!(construido.descricao.as_deref(), Some("NF 123"));
        assert_eq!(
            construido.data_pagamento,
            NaiveDate::from_ymd_opt(2026, 10, 1)
        );

        // Today means now; a blank description is no description.
        let construido = pagamento(
            &args(&[
                "--chave",
                "fornecedor@exemplo.com",
                "--valor",
                "1",
                "--data",
                "2026-09-23",
                "--descricao",
                " ",
            ]),
            hoje(),
        )
        .unwrap();
        assert_eq!(construido.data_pagamento, None);
        assert_eq!(construido.descricao, None);

        let passado = args(&[
            "--chave",
            "fornecedor@exemplo.com",
            "--valor",
            "1",
            "--data",
            "2026-09-22",
        ]);
        let err = pagamento(&passado, hoje()).unwrap_err().to_string();
        assert!(err.contains("22/09/2026 já passou"), "{err}");

        let longa = "x".repeat(141);
        let err = pagamento(
            &args(&[
                "--chave",
                "fornecedor@exemplo.com",
                "--valor",
                "1",
                "--descricao",
                &longa,
            ]),
            hoje(),
        )
        .unwrap_err();
        assert!(matches!(err, CliError::Usage(_)), "{err}");
    }

    #[test]
    fn summary_shows_what_will_be_sent() {
        let mut pagamento = PagamentoPix::new(
            "1500".parse().unwrap(),
            Destinatario::Chave {
                chave: "+5511912345678".parse().unwrap(),
            },
        );
        pagamento.descricao = Some("NF 123".to_owned());
        pagamento.data_pagamento = NaiveDate::from_ymd_opt(2026, 10, 1);
        assert_eq!(
            resumo(&pagamento, Some(Environment::Sandbox), &id()),
            "\
Pix a enviar
  Ambiente               sandbox (dados fictícios)
  Chave Pix              +55 (11) 91234-5678 (celular)
  Valor                  R$ 1.500,00 (mil e quinhentos reais)
  Quando                 agendado para 01/10/2026
  Descrição              NF 123
  Chave de idempotência  123e4567-e89b-42d3-a456-426614174000"
        );
    }

    #[test]
    fn production_is_highlighted_and_phone_like_cpfs_warned() {
        let pagamento = PagamentoPix::new(
            "0.5".parse().unwrap(),
            Destinatario::Chave {
                chave: "11987654374".parse().unwrap(),
            },
        );
        let texto = resumo(&pagamento, Some(Environment::Production), &id());
        assert!(
            texto.starts_with("*** PRODUÇÃO: este Pix movimenta dinheiro da conta real ***\n"),
            "{texto}"
        );
        assert!(texto.contains("PRODUÇÃO (conta real)"), "{texto}");
        assert!(texto.contains("119.876.543-74 (CPF)"), "{texto}");
        assert!(texto.contains("R$ 0,50 (cinquenta centavos)"), "{texto}");
        assert!(texto.contains("Quando                 agora"), "{texto}");
        assert!(
            texto.ends_with("se for um celular, use +55 e o DDD (+55DD9NNNNNNNN)"),
            "{texto}"
        );

        let cpf = PagamentoPix::new(
            "1".parse().unwrap(),
            Destinatario::Chave {
                chave: "123.456.789-09".parse().unwrap(),
            },
        );
        let texto = resumo(&cpf, None, &id());
        assert!(!texto.contains("aviso"), "{texto}");
        assert!(
            texto.contains("Ambiente               não definido"),
            "{texto}"
        );
    }

    #[test]
    fn describes_every_kind_of_key() {
        for (chave, esperado) in [
            ("12.345.678/0001-95", "12.345.678/0001-95 (CNPJ)"),
            ("Fornecedor@Exemplo.com", "fornecedor@exemplo.com (e-mail)"),
            (
                "123E4567-E89B-42D3-A456-426614174000",
                "123e4567-e89b-42d3-a456-426614174000 (chave aleatória)",
            ),
        ] {
            assert_eq!(descrever_chave(&chave.parse().unwrap()), esperado);
        }
    }

    #[test]
    fn renders_each_outcome() {
        let solicitacao = |tipo: &str| -> SolicitacaoPix {
            serde_json::from_value(json!({
                "tipoRetorno": tipo,
                "codigoSolicitacao": "c42f0787-02cb-4b31-827e-459ec9d7ece1",
                "dataPagamento": "2026-10-01",
                "dataOperacao": "2026-09-23"
            }))
            .unwrap()
        };
        assert_eq!(
            render(&solicitacao("PROCESSADO"), &id()),
            "\
Pix enviado.
Código da solicitação  c42f0787-02cb-4b31-827e-459ec9d7ece1
Data do pagamento      01/10/2026
Data da operação       23/09/2026
Chave de idempotência  123e4567-e89b-42d3-a456-426614174000"
        );
        assert!(
            render(&solicitacao("AGENDADO"), &id()).starts_with("Pix agendado para 01/10/2026.")
        );
        assert!(
            render(&solicitacao("APROVACAO"), &id())
                .starts_with("Pix aguardando aprovação no Internet Banking")
        );
        assert!(
            render(&solicitacao("NOVO"), &id())
                .starts_with("Pix registrado (retorno da API: NOVO).")
        );
        assert_eq!(data_br("23/09/2026"), "23/09/2026");
    }

    // --- the command against a mock API -------------------------------------

    struct EnvFalso(HashMap<&'static str, String>);

    impl Env for EnvFalso {
        fn var(&self, name: &str) -> Option<String> {
            self.0.get(name).cloned()
        }
    }

    struct Cenario {
        server: MockServer,
        _dir: tempfile::TempDir,
        context: Context,
        args: PixEnviarArgs,
    }

    async fn cenario(extra: &[&str]) -> Cenario {
        let server = MockServer::start().await;
        cenario_em(server, None, extra)
    }

    fn cenario_em(server: MockServer, base_url: Option<&str>, extra: &[&str]) -> Cenario {
        let dir = tempfile::tempdir().unwrap();
        let rcgen::CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed(vec!["cliente.teste".to_owned()]).unwrap();
        let certificado = dir.path().join("certificado.crt");
        let chave = dir.path().join("chave.key");
        fs::write(&certificado, cert.pem()).unwrap();
        fs::write(&chave, signing_key.serialize_pem()).unwrap();
        let config = dir.path().join("config.toml");
        fs::write(
            &config,
            format!(
                "[perfis.padrao]\nambiente = \"sandbox\"\nclient_id = \"id-de-teste\"\ncertificado = '{}'\nchave_privada = '{}'\n",
                certificado.display(),
                chave.display()
            ),
        )
        .unwrap();

        let config = config.display().to_string();
        let mut full = vec!["inter-pj", "--config", &config, "pix", "enviar"];
        full.extend_from_slice(extra);
        let matches = Cli::command().try_get_matches_from(&full).unwrap();
        let cli = Cli::from_arg_matches(&matches).unwrap();
        let env = EnvFalso(HashMap::from([
            ("INTER_CLIENT_SECRET", "segredo-de-teste".to_owned()),
            (
                "INTER_BASE_URL",
                base_url.map_or_else(|| server.uri(), str::to_owned),
            ),
            (
                "INTER_CACHE_DIR",
                dir.path().join("cache").display().to_string(),
            ),
        ]));
        let context = Context::new(cli.global, &matches, &env).unwrap();
        let Command::Pix(PixCommand::Enviar(args)) = cli.command else {
            unreachable!("pix enviar");
        };
        Cenario {
            server,
            _dir: dir,
            context,
            args,
        }
    }

    const PAGAMENTO: [&str; 4] = ["--chave", "fornecedor@exemplo.com", "--valor", "10"];

    #[tokio::test]
    async fn declined_confirmation_sends_nothing() {
        let cenario = cenario(&PAGAMENTO).await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&cenario.server)
            .await;
        for resposta in ["n\n", "\n", "enviar\n"] {
            let mut terminal = TerminalFalso::respondendo(resposta);
            let err = run(&cenario.context, &cenario.args, &mut terminal)
                .await
                .unwrap_err();
            assert!(matches!(err, CliError::Cancelado), "{resposta:?}: {err}");
            assert_eq!(terminal.perguntas, ["Enviar o Pix? [s/N] "]);
        }
    }

    #[tokio::test]
    async fn confirmed_payment_is_sent_once() {
        let cenario = cenario(&PAGAMENTO).await;
        Mock::given(method("POST"))
            .and(path("/oauth/v2/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "tok",
                "expires_in": 3600,
                "scope": "pagamento-pix.write"
            })))
            .expect(1)
            .mount(&cenario.server)
            .await;
        Mock::given(method("POST"))
            .and(path("/banking/v2/pix"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "tipoRetorno": "PROCESSADO",
                "codigoSolicitacao": "c42f0787-02cb-4b31-827e-459ec9d7ece1"
            })))
            .expect(1)
            .mount(&cenario.server)
            .await;
        let mut terminal = TerminalFalso::respondendo("s\n");
        run(&cenario.context, &cenario.args, &mut terminal)
            .await
            .unwrap();
    }

    async fn mount_token(server: &MockServer) {
        Mock::given(method("POST"))
            .and(path("/oauth/v2/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "tok",
                "expires_in": 3600,
                "scope": "pagamento-pix.write"
            })))
            .mount(server)
            .await;
    }

    /// Errors after which the payment may have been made carry the key that
    /// repeats it safely; the others do not.
    #[tokio::test]
    async fn uncertain_outcomes_carry_the_idempotency_key() {
        let resposta_ilegivel = ResponseTemplate::new(200).set_body_string("<html>");
        for (resposta, incerto) in [
            (ResponseTemplate::new(503), true),
            (ResponseTemplate::new(500), true),
            (resposta_ilegivel, true),
            (ResponseTemplate::new(400), false),
            (ResponseTemplate::new(429), false),
        ] {
            let server = MockServer::start().await;
            mount_token(&server).await;
            Mock::given(method("POST"))
                .and(path("/banking/v2/pix"))
                .respond_with(resposta)
                .expect(1)
                .mount(&server)
                .await;
            let cenario = cenario_em(
                server,
                None,
                &[
                    &PAGAMENTO[..],
                    &["--sim", "--sem-retentativa", "--id-idempotente", ID],
                ]
                .concat(),
            );
            let err = run(
                &cenario.context,
                &cenario.args,
                &mut TerminalFalso::default(),
            )
            .await
            .unwrap_err();
            let hints = err.hints().join("\n");
            assert_eq!(
                matches!(&err, CliError::ResultadoIncerto { id_idempotente, .. } if id_idempotente == ID),
                incerto,
                "{err:?}"
            );
            assert_eq!(
                hints.contains(&format!("--id-idempotente {ID}")),
                incerto,
                "{hints}"
            );
        }

        // A refused connection never reached the API.
        let server = MockServer::start().await;
        let cenario = cenario_em(
            server,
            Some("http://127.0.0.1:1"),
            &[&PAGAMENTO[..], &["--sim", "--sem-retentativa"]].concat(),
        );
        let err = run(
            &cenario.context,
            &cenario.args,
            &mut TerminalFalso::default(),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, CliError::Inter(InterError::Transport(_))),
            "{err:?}"
        );
    }
}
