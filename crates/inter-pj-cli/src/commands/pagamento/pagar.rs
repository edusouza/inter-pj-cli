//! `inter-pj pagamento boleto pagar`

use std::fmt::Write as _;

use chrono::NaiveDate;
use inter_pj::banking::{PagamentoBoleto, SolicitacaoPagamento, StatusPagamento};
use inter_pj::boleto::{CodigoBarras, Segmento, TipoCodigo};
use inter_pj::{Environment, endpoint};

use super::{data_br, descrever_status};
use crate::cli::{BoletoPagarArgs, Formato};
use crate::commands::{Context, hoje, simulacao};
use crate::confirmacao::{Terminal, confirmar, descrever_ambiente, verificar_limite};
use crate::error::{CliError, resultado_incerto};
use crate::output;
use crate::valor::por_extenso;

pub(super) async fn run(
    context: &Context,
    args: &BoletoPagarArgs,
    terminal: &mut dyn Terminal,
) -> Result<(), CliError> {
    let hoje = hoje();
    let pagamento = pagamento(args, hoje)?;
    let settings = context.settings()?;
    verificar_limite(pagamento.valor_pagar, &settings)?;
    // Configuration problems show up before the confirmation, not after it.
    let client = if args.simular {
        None
    } else {
        Some(context.client(&settings)?)
    };
    let ambiente = settings.ambiente.as_ref().map(|setting| setting.value);
    eprintln!("{}", resumo(&pagamento, hoje, ambiente));

    let Some(client) = client else {
        return simulacao::mostrar(
            context,
            &settings,
            endpoint::banking::PAGAMENTO_INCLUIR,
            &[],
            &pagamento,
        );
    };
    confirmar(terminal, args.sim, "Confirmar o pagamento?")?;
    let solicitacao = client
        .banking()
        .pagar_boleto(&pagamento)
        .await
        .map_err(|err| {
            if resultado_incerto(&err) {
                CliError::PagamentoIncerto {
                    source: err,
                    situacao: "o pagamento pode ter sido feito",
                    consulta: format!(
                        "inter-pj pagamento boleto listar --codigo {}",
                        pagamento.codigo.codigo_barras()
                    ),
                }
            } else {
                err.into()
            }
        })?;
    match context.formato() {
        Formato::Json => output::print_json(&solicitacao),
        // `commands::run` refuses csv for this command.
        Formato::Texto | Formato::Csv => output::print(&render(&solicitacao)),
    }
}

/// The payment described by the arguments, validated. Amount and due date
/// come from the code unless given.
fn pagamento(args: &BoletoPagarArgs, hoje: NaiveDate) -> Result<PagamentoBoleto, CliError> {
    if let Some(data) = args.data
        && data < hoje
    {
        return Err(CliError::Usage(format!(
            "o dia {} já passou: agende para hoje ou depois",
            data.format("%d/%m/%Y")
        )));
    }
    let codigo = &args.codigo;
    let valor = args.valor.or_else(|| codigo.valor()).ok_or_else(|| {
        CliError::Usage(
            "o código não traz o valor a pagar: informe --valor (ex.: --valor 150,00)".to_owned(),
        )
    })?;
    let vencimento = args
        .vencimento
        .or_else(|| codigo.vencimento(hoje))
        .ok_or_else(|| {
            CliError::Usage(match codigo.tipo() {
                TipoCodigo::Boleto => {
                    "o boleto não traz o vencimento no código: informe --vencimento (AAAA-MM-DD)"
                        .to_owned()
                }
                _ => "contas e tributos não trazem o vencimento no código: informe --vencimento com a data impressa no documento (AAAA-MM-DD)"
                    .to_owned(),
            })
        })?;
    let mut pagamento = PagamentoBoleto::new(codigo.clone(), valor, vencimento);
    // Without a date the API pays at once: send it only to schedule.
    pagamento.data_pagamento = args.data.filter(|data| *data > hoje);
    pagamento
        .cpf_cnpj_beneficiario
        .clone_from(&args.beneficiario);
    pagamento
        .validar()
        .map_err(|err| CliError::Usage(err.to_string()))?;
    Ok(pagamento)
}

/// What is about to be paid, for the person to check before confirming:
/// amount and due date next to the ones decoded from the code, since a
/// wrong digit there may slip past the check digits.
fn resumo(pagamento: &PagamentoBoleto, hoje: NaiveDate, ambiente: Option<Environment>) -> String {
    let codigo = &pagamento.codigo;
    let dia = |data: NaiveDate| data.format("%d/%m/%Y").to_string();
    let mut linhas = vec![
        ("Ambiente", descrever_ambiente(ambiente)),
        ("Tipo", descrever_tipo(codigo)),
        ("Linha digitável", codigo.linha_formatada()),
    ];
    let extenso = por_extenso(pagamento.valor_pagar)
        .map(|extenso| format!(" ({extenso})"))
        .unwrap_or_default();
    linhas.push((
        "Valor",
        format!("{}{extenso}", output::brl(pagamento.valor_pagar)),
    ));
    match codigo.valor() {
        Some(do_codigo) if do_codigo != pagamento.valor_pagar => {
            linhas.push(("Valor no código", output::brl(do_codigo)));
        }
        Some(_) => {}
        None => linhas.push(("Valor no código", "não informado".to_owned())),
    }

    linhas.push(("Vencimento", dia(pagamento.data_vencimento)));
    if let Some(do_codigo) = codigo.vencimento(hoje)
        && do_codigo != pagamento.data_vencimento
    {
        linhas.push(("Vencimento no código", dia(do_codigo)));
    }
    linhas.push((
        "Quando",
        pagamento.data_pagamento.map_or_else(
            || "agora".to_owned(),
            |data| format!("agendado para {}", dia(data)),
        ),
    ));
    if let Some(documento) = &pagamento.cpf_cnpj_beneficiario {
        linhas.push((
            "Beneficiário",
            format!("{} (a API confere)", documento.formatado()),
        ));
    }

    let mut texto = String::new();
    if ambiente.is_some_and(Environment::is_production) {
        texto.push_str("*** PRODUÇÃO: este pagamento movimenta dinheiro da conta real ***\n");
    }
    texto.push_str("Pagamento a enviar");
    for linha in output::key_values_left(&linhas).lines() {
        let _ = write!(texto, "\n  {linha}");
    }
    for aviso in avisos(pagamento, hoje) {
        let _ = write!(texto, "\naviso: {aviso}");
    }
    texto
}

/// What deserves a second look before paying: an amount or a due date
/// other than the code's, and a payment after the due date.
pub(super) fn avisos(pagamento: &PagamentoBoleto, hoje: NaiveDate) -> Vec<String> {
    let codigo = &pagamento.codigo;
    let dia = |data: NaiveDate| data.format("%d/%m/%Y").to_string();
    let mut avisos = Vec::new();
    if let Some(do_codigo) = codigo.valor()
        && do_codigo != pagamento.valor_pagar
    {
        let (comparacao, confira) = if pagamento.valor_pagar > do_codigo {
            ("maior", "juros e multa")
        } else {
            ("menor", "o desconto")
        };
        avisos.push(format!(
            "o valor a pagar ({}) é {comparacao} que o do código ({}): confira {confira}",
            output::brl(pagamento.valor_pagar),
            output::brl(do_codigo)
        ));
    }
    if let Some(do_codigo) = codigo.vencimento(hoje)
        && do_codigo != pagamento.data_vencimento
    {
        avisos.push(format!(
            "o vencimento informado ({}) difere do vencimento do código ({})",
            dia(pagamento.data_vencimento),
            dia(do_codigo)
        ));
    }
    if pagamento.data_pagamento.unwrap_or(hoje) > pagamento.data_vencimento {
        avisos.push(format!(
            "o pagamento fica para depois do vencimento ({}): pode haver juros e multa, ou recusa",
            dia(pagamento.data_vencimento)
        ));
    }
    avisos
}

/// `boleto do banco 077`, `conta ou tributo: água e esgoto`.
fn descrever_tipo(codigo: &CodigoBarras) -> String {
    match codigo.tipo() {
        TipoCodigo::Boleto => format!("boleto do banco {}", codigo.banco().unwrap_or("?")),
        TipoCodigo::Arrecadacao(segmento) => {
            let segmento = match segmento {
                Segmento::Prefeitura => "prefeitura (IPTU, ISS e outros)".to_owned(),
                Segmento::Saneamento => "água e esgoto".to_owned(),
                Segmento::EnergiaEGas => "energia elétrica e gás".to_owned(),
                Segmento::Telecomunicacoes => "telecomunicações".to_owned(),
                Segmento::OrgaoGovernamental => "órgão governamental".to_owned(),
                Segmento::Carne => "carnê ou empresa identificada pelo CNPJ".to_owned(),
                Segmento::MultaDeTransito => "multa de trânsito".to_owned(),
                Segmento::UsoDoBanco => "uso exclusivo do banco".to_owned(),
                Segmento::Outro(digito) => format!("segmento {digito}"),
                _ => "segmento não identificado".to_owned(),
            };
            format!("conta ou tributo: {segmento}")
        }
        _ => "código de pagamento".to_owned(),
    }
}

fn render(solicitacao: &SolicitacaoPagamento) -> String {
    let agendamento = solicitacao.data_agendamento.as_deref().map(data_br);
    let titulo = match &solicitacao.status_pagamento {
        Some(StatusPagamento::Realizado) => "Pagamento realizado.".to_owned(),
        Some(StatusPagamento::Agendado) => match &agendamento {
            Some(data) => format!("Pagamento agendado para {data}."),
            None => "Pagamento agendado.".to_owned(),
        },
        Some(StatusPagamento::AguardandoAprovacao) => "Pagamento aguardando aprovação no Internet Banking (Aprovar > Gestão de Aprovações): só será feito depois de aprovado.".to_owned(),
        Some(StatusPagamento::EmProcessamento) => "Pagamento em processamento.".to_owned(),
        Some(outro) => format!("Pagamento registrado (status: {}).", descrever_status(outro)),
        None => "Pagamento registrado.".to_owned(),
    };
    let mut linhas = Vec::new();
    if let Some(codigo) = &solicitacao.codigo_transacao {
        linhas.push(("Código da transação", codigo.clone()));
    }
    if let Some(data) = agendamento {
        linhas.push(("Data do agendamento", data));
    }
    if let Some(aprovadores) = solicitacao.quantidade_aprovadores.filter(|n| *n > 0) {
        linhas.push(("Aprovações necessárias", aprovadores.to_string()));
    }
    let mut texto = titulo;
    if !linhas.is_empty() {
        let _ = write!(texto, "\n{}", output::key_values_left(&linhas));
    }
    if let Some(codigo) = &solicitacao.codigo_transacao {
        let _ = write!(
            texto,
            "\n\nAcompanhe com: inter-pj pagamento boleto listar --codigo-transacao {codigo}"
        );
        if solicitacao.status_pagamento == Some(StatusPagamento::Agendado) {
            let _ = write!(
                texto,
                "\nPara cancelar: inter-pj pagamento boleto cancelar {codigo}"
            );
        }
    }
    texto
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;

    use clap::{CommandFactory, FromArgMatches, Parser};
    use inter_pj::Error as InterError;
    use serde_json::json;
    use wiremock::matchers::{any, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::cli::{BoletoCommand, Cli, Command, PagamentoCommand};
    use crate::commands::Env;
    use crate::confirmacao::testes::TerminalFalso;

    /// The boleto of the API documentation with a due date in the current
    /// cycle of the factor: R$ 30,10, due on 2026-10-10 (synthetic, check
    /// digits computed with an independent implementation).
    const BOLETO: &str = "07797777051167847115990071126347115950000003010";
    /// Water bill of the sandbox: R$ 65,33, no due date in the code.
    const CONTA: &str = "82670000000653301602023123106000000002830894";
    /// A boleto that leaves both the amount and the due date to the payer.
    const ABERTO: &str = "00195000000000000000000000000000000000000000";

    fn args(extra: &[&str]) -> BoletoPagarArgs {
        let mut full = vec!["inter-pj", "pagamento", "boleto", "pagar"];
        full.extend_from_slice(extra);
        match Cli::try_parse_from(full).unwrap().command {
            Command::Pagamento(PagamentoCommand::Boleto(BoletoCommand::Pagar(args))) => args,
            other => panic!("comando inesperado: {other:?}"),
        }
    }

    fn dia(ano: i32, mes: u32, dia: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(ano, mes, dia).unwrap()
    }

    fn hoje() -> NaiveDate {
        dia(2026, 9, 23)
    }

    #[test]
    fn amount_and_due_date_come_from_the_code() {
        let construido = pagamento(&args(&[BOLETO]), hoje()).unwrap();
        assert_eq!(construido.valor_pagar, "30.10".parse().unwrap());
        assert_eq!(construido.data_vencimento, dia(2026, 10, 10));
        assert_eq!(construido.data_pagamento, None);
        assert_eq!(construido.cpf_cnpj_beneficiario, None);

        // Given values win; today means now.
        let construido = pagamento(
            &args(&[
                BOLETO,
                "--valor",
                "31,50",
                "--vencimento",
                "2026-10-11",
                "--data",
                "2026-09-23",
                "--beneficiario",
                "12.345.678/0001-95",
            ]),
            hoje(),
        )
        .unwrap();
        assert_eq!(construido.valor_pagar, "31.50".parse().unwrap());
        assert_eq!(construido.data_vencimento, dia(2026, 10, 11));
        assert_eq!(construido.data_pagamento, None);
        assert!(construido.cpf_cnpj_beneficiario.is_some());

        let agendado = pagamento(&args(&[BOLETO, "--data", "2026-10-09"]), hoje()).unwrap();
        assert_eq!(agendado.data_pagamento, Some(dia(2026, 10, 9)));
    }

    #[test]
    fn what_the_code_does_not_bring_must_be_given() {
        let err = pagamento(&args(&[CONTA]), hoje()).unwrap_err().to_string();
        assert!(
            err.contains("contas e tributos não trazem o vencimento no código"),
            "{err}"
        );
        let conta = pagamento(&args(&[CONTA, "--vencimento", "2026-10-10"]), hoje()).unwrap();
        assert_eq!(conta.valor_pagar, "65.33".parse().unwrap());

        let err = pagamento(&args(&[ABERTO]), hoje()).unwrap_err().to_string();
        assert!(err.contains("informe --valor"), "{err}");
        let err = pagamento(&args(&[ABERTO, "--valor", "10"]), hoje())
            .unwrap_err()
            .to_string();
        assert!(err.contains("o boleto não traz o vencimento"), "{err}");
        let aberto = pagamento(
            &args(&[ABERTO, "--valor", "10", "--vencimento", "2026-10-01"]),
            hoje(),
        )
        .unwrap();
        let texto = resumo(&aberto, hoje(), None);
        assert!(texto.contains("Valor no código  não informado"), "{texto}");

        let err = pagamento(&args(&[BOLETO, "--data", "2026-09-22"]), hoje())
            .unwrap_err()
            .to_string();
        assert!(err.contains("22/09/2026 já passou"), "{err}");
        // Amounts that are not positive never get this far.
        assert!(
            Cli::try_parse_from([
                "inter-pj",
                "pagamento",
                "boleto",
                "pagar",
                BOLETO,
                "--valor",
                "0"
            ])
            .is_err()
        );
    }

    #[test]
    fn summary_shows_the_code_and_what_it_says() {
        let construido = pagamento(&args(&[BOLETO]), hoje()).unwrap();
        assert_eq!(
            resumo(&construido, hoje(), Some(Environment::Sandbox)),
            "\
Pagamento a enviar
  Ambiente         sandbox (dados fictícios)
  Tipo             boleto do banco 077
  Linha digitável  07797.77705 11678.471159 90071.126347 1 15950000003010
  Valor            R$ 30,10 (trinta reais e dez centavos)
  Vencimento       10/10/2026
  Quando           agora"
        );
    }

    #[test]
    fn summary_warns_about_divergences_and_late_payments() {
        let construido = pagamento(
            &args(&[
                BOLETO,
                "--valor",
                "32",
                "--vencimento",
                "2026-10-01",
                "--data",
                "2026-10-05",
                "--beneficiario",
                "12345678000195",
            ]),
            hoje(),
        )
        .unwrap();
        let texto = resumo(&construido, hoje(), Some(Environment::Production));
        assert!(
            texto
                .starts_with("*** PRODUÇÃO: este pagamento movimenta dinheiro da conta real ***\n"),
            "{texto}"
        );
        for linha in [
            "Valor                 R$ 32,00 (trinta e dois reais)",
            "Valor no código       R$ 30,10",
            "Vencimento            01/10/2026",
            "Vencimento no código  10/10/2026",
            "Quando                agendado para 05/10/2026",
            "Beneficiário          12.345.678/0001-95 (a API confere)",
            "aviso: o valor a pagar (R$ 32,00) é maior que o do código (R$ 30,10): confira juros e multa",
            "aviso: o vencimento informado (01/10/2026) difere do vencimento do código (10/10/2026)",
            "aviso: o pagamento fica para depois do vencimento (01/10/2026): pode haver juros e multa, ou recusa",
        ] {
            assert!(texto.contains(linha), "{linha}\n{texto}");
        }

        let desconto = pagamento(&args(&[BOLETO, "--valor", "29"]), hoje()).unwrap();
        let texto = resumo(&desconto, hoje(), None);
        assert!(
            texto.contains(
                "o valor a pagar (R$ 29,00) é menor que o do código (R$ 30,10): confira o desconto"
            ),
            "{texto}"
        );

        let conta = pagamento(&args(&[CONTA, "--vencimento", "2026-10-10"]), hoje()).unwrap();
        let texto = resumo(&conta, hoje(), None);
        assert!(
            texto.contains("Tipo             conta ou tributo: água e esgoto"),
            "{texto}"
        );
        assert!(
            texto.contains(
                "Linha digitável  82670000000-1 65330160202-1 31231060000-1 00002830894-8"
            ),
            "{texto}"
        );
        assert!(!texto.contains("aviso"), "{texto}");
    }

    #[test]
    fn renders_each_outcome() {
        let solicitacao = |status: &str| -> SolicitacaoPagamento {
            serde_json::from_value(json!({
                "statusPagamento": status,
                "codigoTransacao": "3414f226-36fb-4d87-811e-cfd99911d845",
                "dataAgendamento": "2026-10-09 00:00:00",
                "quantidadeAprovadores": 0
            }))
            .unwrap()
        };
        assert_eq!(
            render(&solicitacao("AGENDADO")),
            "\
Pagamento agendado para 09/10/2026.
Código da transação  3414f226-36fb-4d87-811e-cfd99911d845
Data do agendamento  09/10/2026

Acompanhe com: inter-pj pagamento boleto listar --codigo-transacao 3414f226-36fb-4d87-811e-cfd99911d845
Para cancelar: inter-pj pagamento boleto cancelar 3414f226-36fb-4d87-811e-cfd99911d845"
        );
        let texto = render(&solicitacao("REALIZADO"));
        assert!(texto.starts_with("Pagamento realizado."), "{texto}");
        assert!(!texto.contains("Para cancelar"), "{texto}");
        assert!(
            render(&solicitacao("AGUARDANDO_APROVACAO"))
                .starts_with("Pagamento aguardando aprovação no Internet Banking")
        );
        assert!(render(&solicitacao("NOVO")).starts_with("Pagamento registrado (status: NOVO)."));

        let aprovacao: SolicitacaoPagamento = serde_json::from_value(json!({
            "statusPagamento": "AGUARDANDO_APROVACAO",
            "quantidadeAprovadores": 2
        }))
        .unwrap();
        assert!(render(&aprovacao).ends_with("Aprovações necessárias  2"));
        assert_eq!(
            render(&SolicitacaoPagamento::default()),
            "Pagamento registrado."
        );
    }

    #[test]
    fn describes_every_segment() {
        for (codigo, esperado) in [
            (BOLETO, "boleto do banco 077"),
            (CONTA, "conta ou tributo: água e esgoto"),
            (
                "846600000026266702962017910100130004000625169925",
                "conta ou tributo: telecomunicações",
            ),
        ] {
            assert_eq!(descrever_tipo(&codigo.parse().unwrap()), esperado);
        }
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
        args: BoletoPagarArgs,
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
        let mut full = vec![
            "inter-pj",
            "--config",
            &config,
            "pagamento",
            "boleto",
            "pagar",
            BOLETO,
        ];
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
        let Command::Pagamento(PagamentoCommand::Boleto(BoletoCommand::Pagar(args))) = cli.command
        else {
            unreachable!("pagamento boleto pagar");
        };
        Cenario {
            server,
            _dir: dir,
            context,
            args,
        }
    }

    async fn mount_token(server: &MockServer) {
        Mock::given(method("POST"))
            .and(path("/oauth/v2/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "tok",
                "expires_in": 3600,
                "scope": "pagamento-boleto.write"
            })))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn declined_confirmation_pays_nothing() {
        let cenario = cenario_em(MockServer::start().await, None, &[]);
        Mock::given(any())
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&cenario.server)
            .await;
        for resposta in ["n\n", "\n", "pagar\n"] {
            let mut terminal = TerminalFalso::respondendo(resposta);
            let err = run(&cenario.context, &cenario.args, &mut terminal)
                .await
                .unwrap_err();
            assert!(matches!(err, CliError::Cancelado), "{resposta:?}: {err}");
            assert_eq!(terminal.perguntas, ["Confirmar o pagamento? [s/N] "]);
        }
    }

    #[tokio::test]
    async fn confirmed_payment_is_sent_once() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("POST"))
            .and(path("/banking/v2/pagamento"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "statusPagamento": "REALIZADO",
                "codigoTransacao": "3414f226-36fb-4d87-811e-cfd99911d845"
            })))
            .expect(1)
            .mount(&server)
            .await;
        let cenario = cenario_em(server, None, &[]);
        let mut terminal = TerminalFalso::respondendo("s\n");
        run(&cenario.context, &cenario.args, &mut terminal)
            .await
            .unwrap();
    }

    /// Errors after which the payment may have been made say how to check
    /// it before trying again; the others do not.
    #[tokio::test]
    async fn uncertain_outcomes_say_how_to_check_the_payment() {
        let consulta = "inter-pj pagamento boleto listar --codigo 07791159500000030107777011678471159007112634";
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
                .and(path("/banking/v2/pagamento"))
                .respond_with(resposta)
                .expect(1)
                .mount(&server)
                .await;
            let cenario = cenario_em(server, None, &["--sim", "--sem-retentativa"]);
            let err = run(
                &cenario.context,
                &cenario.args,
                &mut TerminalFalso::default(),
            )
            .await
            .unwrap_err();
            let hints = err.hints().join("\n");
            assert_eq!(
                matches!(&err, CliError::PagamentoIncerto { consulta: c, .. } if c == consulta),
                incerto,
                "{err:?}"
            );
            assert_eq!(hints.contains(consulta), incerto, "{hints}");
            assert_eq!(hints.contains("pagar duas vezes"), incerto, "{hints}");
        }

        // A refused connection never reached the API.
        let cenario = cenario_em(
            MockServer::start().await,
            Some("http://127.0.0.1:1"),
            &["--sim", "--sem-retentativa"],
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
