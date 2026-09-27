//! `inter-pj pagamento boleto listar|cancelar`

use std::fmt::Write as _;

use chrono::NaiveDate;
use inter_pj::Environment;
use inter_pj::banking::{
    DataDoPagamento, FiltroPagamentos, Pagamento, Periodo, PeriodoError, StatusPagamento,
};
use inter_pj::documento::Documento;
use serde_json::json;

use super::{data, descrever_status};
use crate::cli::{BoletoCancelarArgs, BoletoCommand, BoletoListarArgs, Formato};
use crate::commands::{Context, hoje, intervalo};
use crate::confirmacao::{Terminal, confirmar, descrever_ambiente, pode_confirmar};
use crate::error::CliError;
use crate::output;
use crate::tabela::{Celula, Coluna, Tabela};

/// Longest beneficiary name shown in the text output.
const LARGURA_BENEFICIARIO: usize = 30;

pub(super) async fn run(
    context: &Context,
    command: BoletoCommand,
    terminal: &mut dyn Terminal,
) -> Result<(), CliError> {
    match command {
        BoletoCommand::Pagar(args) => super::pagar::run(context, &args, terminal).await,
        BoletoCommand::Listar(args) => listar(context, &args).await,
        BoletoCommand::Cancelar(args) => cancelar(context, &args, terminal).await,
    }
}

// --- listar --------------------------------------------------------------------

async fn listar(context: &Context, args: &BoletoListarArgs) -> Result<(), CliError> {
    let filtro = filtro(args, hoje())?;
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let pagamentos = client.banking().pagamentos(&filtro).await?;
    match context.formato() {
        Formato::Json => output::print_json(&json!({ "pagamentos": pagamentos })),
        Formato::Csv => output::print_raw(&csv(&pagamentos).csv(context.separador())),
        Formato::Texto => {
            context.warn_if_sandbox(&settings);
            output::print(&render(&filtro, &pagamentos))
        }
    }
}

/// The filters of the arguments. The period is always sent, so that it can
/// be shown: by default, the last 30 days (the API's own default).
fn filtro(args: &BoletoListarArgs, hoje: NaiveDate) -> Result<FiltroPagamentos, CliError> {
    let (inicio, fim) = intervalo(args.periodo, hoje);
    let periodo = Periodo::new(inicio, fim).map_err(|erro| CliError::Periodo {
        dica: matches!(erro, PeriodoError::MuitoLongo { .. })
            .then_some("consulte um período de até 90 dias por vez"),
        erro,
    })?;
    Ok(FiltroPagamentos {
        periodo: Some((periodo.inicio(), periodo.fim())),
        filtrar_por: args.filtrar_por.map(Into::into),
        codigo: args.codigo.clone(),
        codigo_transacao: args.codigo_transacao.clone(),
    })
}

fn render(filtro: &FiltroPagamentos, pagamentos: &[Pagamento]) -> String {
    let mut texto = String::from("Pagamentos");
    if let Some((inicio, fim)) = filtro.periodo {
        let quais = match filtro.filtrar_por {
            Some(DataDoPagamento::Pagamento) => "realizados",
            Some(DataDoPagamento::Vencimento) => "com vencimento",
            _ => "incluídos",
        };
        let _ = write!(
            texto,
            " {quais} de {} a {}",
            inicio.format("%d/%m/%Y"),
            fim.format("%d/%m/%Y")
        );
    }
    if let Some(codigo) = &filtro.codigo {
        let _ = write!(texto, "\nCódigo: {}", codigo.linha_formatada());
    }
    texto.push_str("\n\n");
    if pagamentos.is_empty() {
        texto.push_str("Nenhum pagamento encontrado.");
        return texto;
    }
    texto.push_str(&tabela(pagamentos).texto());
    let plural = if pagamentos.len() == 1 {
        "pagamento"
    } else {
        "pagamentos"
    };
    let _ = write!(texto, "\n\n{} {plural}", pagamentos.len());
    texto
}

fn tabela(pagamentos: &[Pagamento]) -> Tabela {
    let mut tabela = Tabela::new(vec![
        Coluna::texto("Vencimento", ""),
        Coluna::texto("Pagamento", ""),
        Coluna::texto("Beneficiário", "").no_maximo(LARGURA_BENEFICIARIO),
        Coluna::texto("Status", ""),
        Coluna::valor("Valor", ""),
        Coluna::texto("Código da transação", ""),
    ]);
    for p in pagamentos {
        tabela.linha(vec![
            data(
                p.data_vencimento_titulo
                    .as_deref()
                    .or(p.data_vencimento_digitada.as_deref()),
            ),
            data(p.data_pagamento.as_deref()),
            Celula::texto(p.nome_beneficiario.as_deref()),
            Celula::texto(p.status_pagamento.as_ref().map(descrever_status)),
            Celula::dinheiro(p.valor_pago.or(p.valor_nominal)),
            Celula::texto(p.codigo_transacao.as_deref()),
        ]);
    }
    tabela
}

/// Every field, with the API's names and codes.
fn csv(pagamentos: &[Pagamento]) -> Tabela {
    let texto = |campo: &'static str| Coluna::texto(campo, campo);
    let valor = |campo: &'static str| Coluna::valor(campo, campo);
    let mut tabela = Tabela::new(vec![
        texto("codigoTransacao"),
        texto("codigoBarra"),
        texto("tipo"),
        texto("dataInclusao"),
        texto("dataPagamento"),
        texto("dataVencimentoTitulo"),
        texto("dataVencimentoDigitada"),
        valor("valorPago"),
        valor("valorNominal"),
        texto("statusPagamento"),
        texto("nomeBeneficiario"),
        texto("cpfCnpjBeneficiario"),
        texto("aprovacoesNecessarias"),
        texto("aprovacoesRealizadas"),
        texto("autenticacao"),
        texto("nsu"),
    ]);
    let numero = |n: Option<u64>| Celula::texto(n.map(|n| n.to_string()).as_deref());
    for p in pagamentos {
        tabela.linha(vec![
            Celula::texto(p.codigo_transacao.as_deref()),
            Celula::texto(p.codigo_barra.as_deref()),
            Celula::texto(p.tipo.as_deref()),
            data(p.data_inclusao.as_deref()),
            data(p.data_pagamento.as_deref()),
            data(p.data_vencimento_titulo.as_deref()),
            data(p.data_vencimento_digitada.as_deref()),
            Celula::dinheiro(p.valor_pago),
            Celula::dinheiro(p.valor_nominal),
            Celula::texto(p.status_pagamento.as_ref().map(StatusPagamento::as_str)),
            Celula::texto(p.nome_beneficiario.as_deref()),
            Celula::texto(p.cpf_cnpj_beneficiario.as_deref()),
            numero(p.aprovacoes_necessarias),
            numero(p.aprovacoes_realizadas),
            Celula::texto(p.autenticacao.as_deref()),
            Celula::texto(p.nsu.as_deref()),
        ]);
    }
    tabela
}

// --- cancelar ------------------------------------------------------------------

async fn cancelar(
    context: &Context,
    args: &BoletoCancelarArgs,
    terminal: &mut dyn Terminal,
) -> Result<(), CliError> {
    // Nothing is looked up when no one could confirm.
    pode_confirmar(terminal, args.sim)?;
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let codigo = &args.codigo_transacao;

    // What is about to be cancelled, when the API finds it (by default it
    // looks among the payments requested in the last 30 days).
    let busca = FiltroPagamentos {
        codigo_transacao: Some(codigo.clone()),
        ..FiltroPagamentos::default()
    };
    let pagamento = match client.banking().pagamentos(&busca).await {
        Ok(pagamentos) => pagamentos.into_iter().find(|p| {
            p.codigo_transacao
                .as_deref()
                .is_some_and(|c| c.eq_ignore_ascii_case(codigo))
        }),
        Err(err) => {
            eprintln!(
                "aviso: não foi possível consultar o pagamento antes de cancelar: {}",
                output::limpo(&err.to_string())
            );
            None
        }
    };
    let ambiente = settings.ambiente.as_ref().map(|setting| setting.value);
    eprintln!(
        "{}",
        resumo_cancelamento(codigo, pagamento.as_ref(), ambiente)
    );
    confirmar(terminal, args.sim, "Cancelar o agendamento?")?;

    client.banking().cancelar_pagamento(codigo).await?;
    match context.formato() {
        Formato::Json => output::print_json(&json!({
            "codigoTransacao": codigo,
            "cancelado": true,
        })),
        // `commands::run` refuses csv for this command.
        Formato::Texto | Formato::Csv => output::print(&format!("Agendamento cancelado: {codigo}")),
    }
}

/// The payment about to be cancelled, for the person to check.
fn resumo_cancelamento(
    codigo: &str,
    pagamento: Option<&Pagamento>,
    ambiente: Option<Environment>,
) -> String {
    let mut linhas = vec![
        ("Ambiente", descrever_ambiente(ambiente)),
        ("Código da transação", codigo.to_owned()),
    ];
    let mut aviso = None;
    match pagamento {
        Some(p) => {
            if let Some(nome) = &p.nome_beneficiario {
                let documento = p
                    .cpf_cnpj_beneficiario
                    .as_deref()
                    .map(|doc| {
                        let doc = Documento::parse(doc)
                            .map_or_else(|_| doc.to_owned(), |d| d.formatado());
                        format!(" ({doc})")
                    })
                    .unwrap_or_default();
                linhas.push(("Beneficiário", format!("{nome}{documento}")));
            }
            if let Some(valor) = p.valor_pago.or(p.valor_nominal) {
                linhas.push(("Valor", output::brl(valor)));
            }
            if let Celula::Data(dia) = data(p.data_pagamento.as_deref()) {
                linhas.push(("Pagamento em", dia.format("%d/%m/%Y").to_string()));
            }
            if let Some(status) = &p.status_pagamento {
                linhas.push(("Status", descrever_status(status).to_owned()));
                if !cancelavel(status) {
                    aviso = Some(format!(
                        "o pagamento está {}: a API deve recusar o cancelamento",
                        descrever_status(status)
                    ));
                }
            }
        }
        None => linhas.push((
            "Pagamento",
            "não encontrado entre os incluídos nos últimos 30 dias; confira o código".to_owned(),
        )),
    }
    let mut texto = String::from("Agendamento a cancelar");
    for linha in output::key_values_left(&linhas).lines() {
        let _ = write!(texto, "\n  {linha}");
    }
    if let Some(aviso) = aviso {
        let _ = write!(texto, "\naviso: {aviso}");
    }
    texto
}

/// Statuses that still may be cancelled (unknown ones included: the API
/// decides).
fn cancelavel(status: &StatusPagamento) -> bool {
    !matches!(
        status,
        StatusPagamento::Realizado
            | StatusPagamento::Cancelado
            | StatusPagamento::AgendadoCancelado
            | StatusPagamento::Reprovado
            | StatusPagamento::AprovacaoExpirada
            | StatusPagamento::Erro
    )
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;

    use clap::{CommandFactory, FromArgMatches, Parser};
    use serde_json::json;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::cli::{Cli, Command, PagamentoCommand};
    use crate::commands::Env;
    use crate::confirmacao::testes::TerminalFalso;

    const TRANSACAO: &str = "3414f226-36fb-4d87-811e-cfd99911d845";

    fn listar_args(extra: &[&str]) -> BoletoListarArgs {
        let mut full = vec!["inter-pj", "pagamento", "boleto", "listar"];
        full.extend_from_slice(extra);
        match Cli::try_parse_from(full).unwrap().command {
            Command::Pagamento(PagamentoCommand::Boleto(BoletoCommand::Listar(args))) => args,
            other => panic!("comando inesperado: {other:?}"),
        }
    }

    fn dia(mes: u32, dia: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, mes, dia).unwrap()
    }

    fn pagamentos() -> Vec<Pagamento> {
        serde_json::from_value(json!([
            {
                "codigoTransacao": TRANSACAO,
                "codigoBarra": "07791929500000030107777011678471159007112634",
                "tipo": "BOLETO",
                "dataInclusao": "2026-09-20 10:00:00",
                "dataPagamento": "2026-10-09",
                "dataVencimentoTitulo": "2026-10-10",
                "valorNominal": 30.1,
                "statusPagamento": "AGENDADO",
                "nomeBeneficiario": "Fornecedor Exemplo",
                "cpfCnpjBeneficiario": "12345678000195",
                "aprovacoesNecessarias": 0
            },
            {
                "codigoTransacao": "8bbdede4-35db-4ec9-b652-e176841e62c8",
                "dataVencimentoDigitada": "2026-09-15",
                "dataPagamento": "15/09/2026",
                "valorPago": "250.1",
                "statusPagamento": "REALIZADO",
                "nomeBeneficiario": "Companhia de Energia Exemplo com um Nome Bem Comprido"
            }
        ]))
        .unwrap()
    }

    #[test]
    fn the_period_is_always_sent_and_checked() {
        let construido = filtro(&listar_args(&[]), dia(9, 23)).unwrap();
        assert_eq!(construido.periodo, Some((dia(8, 25), dia(9, 23))));
        assert_eq!(construido.filtrar_por, None);

        let args = listar_args(&[
            "--fim",
            "2026-12-31",
            "--filtrar-por",
            "vencimento",
            "--codigo-transacao",
            TRANSACAO,
        ]);
        let construido = filtro(&args, dia(9, 23)).unwrap();
        assert_eq!(construido.periodo, Some((dia(12, 2), dia(12, 31))));
        assert_eq!(construido.filtrar_por, Some(DataDoPagamento::Vencimento));
        assert_eq!(construido.codigo_transacao.as_deref(), Some(TRANSACAO));

        for (inicio, fim) in [("2026-01-01", "2026-06-30"), ("2026-09-02", "2026-09-01")] {
            let args = listar_args(&["--inicio", inicio, "--fim", fim]);
            let err = filtro(&args, dia(9, 23)).unwrap_err();
            assert!(matches!(err, CliError::Periodo { .. }), "{err}");
        }
    }

    #[test]
    fn renders_the_payments_of_the_period() {
        let construido = filtro(
            &listar_args(&["--inicio", "2026-09-01", "--fim", "2026-09-30"]),
            dia(9, 23),
        )
        .unwrap();
        assert_eq!(
            render(&construido, &pagamentos()),
            "\
Pagamentos incluídos de 01/09/2026 a 30/09/2026

Vencimento  Pagamento   Beneficiário                   Status        Valor  Código da transação
10/10/2026  09/10/2026  Fornecedor Exemplo             agendado   R$ 30,10  3414f226-36fb-4d87-811e-cfd99911d845
15/09/2026  15/09/2026  Companhia de Energia Exemplo…  pago      R$ 250,10  8bbdede4-35db-4ec9-b652-e176841e62c8

2 pagamentos"
        );

        let args = listar_args(&[
            "--filtrar-por",
            "pagamento",
            "--codigo",
            "07797777051167847115990071126347192950000003010",
        ]);
        let construido = filtro(&args, dia(9, 23)).unwrap();
        assert_eq!(
            render(&construido, &[]),
            "\
Pagamentos realizados de 25/08/2026 a 23/09/2026
Código: 07797.77705 11678.471159 90071.126347 1 92950000003010

Nenhum pagamento encontrado."
        );
    }

    #[test]
    fn csv_has_every_field_with_the_api_names() {
        let csv = csv(&pagamentos()).csv(crate::tabela::Separador::Virgula);
        let linhas: Vec<&str> = csv.split("\r\n").collect();
        assert_eq!(
            linhas[0],
            "codigoTransacao,codigoBarra,tipo,dataInclusao,dataPagamento,dataVencimentoTitulo,dataVencimentoDigitada,valorPago,valorNominal,statusPagamento,nomeBeneficiario,cpfCnpjBeneficiario,aprovacoesNecessarias,aprovacoesRealizadas,autenticacao,nsu"
        );
        assert_eq!(
            linhas[1],
            "3414f226-36fb-4d87-811e-cfd99911d845,07791929500000030107777011678471159007112634,BOLETO,2026-09-20,2026-10-09,2026-10-10,,,30.1,AGENDADO,Fornecedor Exemplo,12345678000195,0,,,"
        );
        assert_eq!(
            linhas[2],
            "8bbdede4-35db-4ec9-b652-e176841e62c8,,,,2026-09-15,,2026-09-15,250.1,,REALIZADO,Companhia de Energia Exemplo com um Nome Bem Comprido,,,,,"
        );
    }

    #[test]
    fn summary_of_a_cancellation() {
        let pagamentos = pagamentos();
        assert_eq!(
            resumo_cancelamento(TRANSACAO, Some(&pagamentos[0]), Some(Environment::Sandbox)),
            "\
Agendamento a cancelar
  Ambiente             sandbox (dados fictícios)
  Código da transação  3414f226-36fb-4d87-811e-cfd99911d845
  Beneficiário         Fornecedor Exemplo (12.345.678/0001-95)
  Valor                R$ 30,10
  Pagamento em         09/10/2026
  Status               agendado"
        );
        let texto = resumo_cancelamento(TRANSACAO, Some(&pagamentos[1]), None);
        assert!(
            texto.ends_with("aviso: o pagamento está pago: a API deve recusar o cancelamento"),
            "{texto}"
        );
        let texto = resumo_cancelamento(TRANSACAO, None, Some(Environment::Production));
        assert!(texto.contains("PRODUÇÃO (conta real)"), "{texto}");
        assert!(
            texto.contains("não encontrado entre os incluídos nos últimos 30 dias"),
            "{texto}"
        );
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
        args: BoletoCancelarArgs,
    }

    async fn cenario(extra: &[&str]) -> Cenario {
        let server = MockServer::start().await;
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
            "cancelar",
            TRANSACAO,
        ];
        full.extend_from_slice(extra);
        let matches = Cli::command().try_get_matches_from(&full).unwrap();
        let cli = Cli::from_arg_matches(&matches).unwrap();
        let env = EnvFalso(HashMap::from([
            ("INTER_CLIENT_SECRET", "segredo-de-teste".to_owned()),
            ("INTER_BASE_URL", server.uri()),
            (
                "INTER_CACHE_DIR",
                dir.path().join("cache").display().to_string(),
            ),
        ]));
        let context = Context::new(cli.global, &matches, &env).unwrap();
        let Command::Pagamento(PagamentoCommand::Boleto(BoletoCommand::Cancelar(args))) =
            cli.command
        else {
            unreachable!("pagamento boleto cancelar");
        };
        Mock::given(method("POST"))
            .and(path("/oauth/v2/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "tok",
                "expires_in": 3600,
                "scope": "pagamento-boleto.read pagamento-boleto.write"
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/banking/v2/pagamento"))
            .and(query_param("codigoTransacao", TRANSACAO))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"codigoTransacao": TRANSACAO, "statusPagamento": "AGENDADO", "valorNominal": 30.1}
            ])))
            .mount(&server)
            .await;
        Cenario {
            server,
            _dir: dir,
            context,
            args,
        }
    }

    async fn mount_cancelar(server: &MockServer, calls: u64) {
        Mock::given(method("DELETE"))
            .and(path(format!("/banking/v2/pagamento/{TRANSACAO}")))
            .respond_with(ResponseTemplate::new(204))
            .expect(calls)
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn declined_confirmation_cancels_nothing() {
        let cenario = cenario(&[]).await;
        mount_cancelar(&cenario.server, 0).await;
        for resposta in ["n\n", "\n", "cancelar\n"] {
            let mut terminal = TerminalFalso::respondendo(resposta);
            let err = run_cancelar(&cenario, &mut terminal).await.unwrap_err();
            assert!(matches!(err, CliError::Cancelado), "{resposta:?}: {err}");
            assert_eq!(terminal.perguntas, ["Cancelar o agendamento? [s/N] "]);
        }
    }

    #[tokio::test]
    async fn confirmed_cancellation_is_sent_once() {
        let cenario = cenario(&[]).await;
        mount_cancelar(&cenario.server, 1).await;
        let mut terminal = TerminalFalso::respondendo("sim\n");
        run_cancelar(&cenario, &mut terminal).await.unwrap();
    }

    /// Without a terminal nor `--sim`, not even the lookup is made.
    #[tokio::test]
    async fn without_a_terminal_nothing_is_requested() {
        let cenario = cenario(&[]).await;
        mount_cancelar(&cenario.server, 0).await;
        let err = run_cancelar(&cenario, &mut TerminalFalso::default())
            .await
            .unwrap_err();
        assert!(matches!(err, CliError::Usage(_)), "{err}");
        assert!(cenario.server.received_requests().await.unwrap().is_empty());
    }

    async fn run_cancelar(cenario: &Cenario, terminal: &mut TerminalFalso) -> Result<(), CliError> {
        cancelar(&cenario.context, &cenario.args, terminal).await
    }

    #[test]
    fn statuses_that_may_still_be_cancelled() {
        let cancelaveis: Vec<&str> = StatusPagamento::DOCUMENTADOS
            .iter()
            .filter(|status| cancelavel(status))
            .map(StatusPagamento::as_str)
            .collect();
        assert_eq!(
            cancelaveis,
            [
                "EMPROCESSAMENTO",
                "AGUARDANDO_APROVACAO",
                "APROVADO",
                "AGENDADO",
                "NAO_COMPENSADO"
            ]
        );
        assert!(cancelavel(&StatusPagamento::Outro("NOVO".to_owned())));
    }

    #[test]
    fn listing_arguments_reach_the_filter() {
        let args = listar_args(&[
            "--codigo",
            "846600000026266702962017910100130004000625169925",
        ]);
        let construido = filtro(&args, dia(9, 23)).unwrap();
        assert_eq!(
            construido
                .codigo
                .map(|c| c.codigo_barras().to_owned())
                .as_deref(),
            Some("84660000002266702962019101001300000062516992")
        );
        // clap's own parsing is covered in `cli`.
        assert!(
            Cli::try_parse_from(["inter-pj", "pagamento", "boleto", "listar", "--codigo", "1"])
                .is_err()
        );
    }
}
