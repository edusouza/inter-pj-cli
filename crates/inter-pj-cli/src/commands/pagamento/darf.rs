//! `inter-pj pagamento darf pagar|listar`

use std::fmt::Write as _;

use chrono::NaiveDate;
use inter_pj::banking::{
    Darf, FiltroDarf, PagamentoDarf, SolicitacaoDarf, StatusPagamento, TipoRetornoDarf,
};
use inter_pj::{Environment, endpoint};
use serde_json::json;

use super::{celula_status, data, data_br};
use crate::arquivo::{self, CAMPOS_DARF, Campos};
use crate::chamada::chamada;
use crate::cli::{DarfCommand, DarfListarArgs, DarfPagarArgs, Formato, PeriodoArgs};
use crate::commands::{Context, hoje, intervalo, simulacao};
use crate::confirmacao::{Terminal, confirmar, descrever_ambiente, verificar_limite};
use crate::error::{CliError, resultado_incerto};
use crate::output;
use crate::tabela::{Celula, Coluna, Tabela};
use crate::valor::por_extenso;

pub(super) async fn run(
    context: &Context,
    command: DarfCommand,
    terminal: &mut dyn Terminal,
) -> Result<(), CliError> {
    match command {
        DarfCommand::Pagar(args) => pagar(context, &args, terminal).await,
        DarfCommand::Listar(args) => listar(context, &args).await,
    }
}

// --- pagar -----------------------------------------------------------------------

async fn pagar(
    context: &Context,
    args: &DarfPagarArgs,
    terminal: &mut dyn Terminal,
) -> Result<(), CliError> {
    let darf = darf(args)?;
    let settings = context.settings()?;
    verificar_limite(darf.valor_total(), &settings)?;
    // Configuration problems show up before the confirmation, not after it.
    let client = if args.simular {
        None
    } else {
        Some(context.client(&settings)?)
    };
    let ambiente = settings.ambiente.as_ref().map(|setting| setting.value);
    output::eprint(&resumo(&darf, hoje(), ambiente));

    let Some(client) = client else {
        return simulacao::mostrar(
            context,
            &settings,
            endpoint::banking::PAGAMENTO_DARF_INCLUIR,
            &[],
            &darf,
        );
    };
    confirmar(terminal, args.sim, "Confirmar o pagamento do DARF?")?;
    let solicitacao = client.banking().pagar_darf(&darf).await.map_err(|err| {
        if resultado_incerto(&err) {
            CliError::PagamentoIncerto {
                source: err,
                situacao: "o pagamento pode ter sido feito",
                consulta: format!(
                    "{} pagamento darf listar --codigo-receita {}",
                    chamada(),
                    darf.codigo_receita
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

/// The DARF of the file or of the options, validated.
fn darf(args: &DarfPagarArgs) -> Result<PagamentoDarf, CliError> {
    if let Some(caminho) = &args.arquivo {
        let valor = arquivo::ler_json(caminho)?;
        let onde = arquivo::nome(caminho);
        return arquivo::darf(&Campos::de(&valor, &onde, &CAMPOS_DARF)?);
    }
    // clap requires every mandatory option along with --codigo-receita.
    let obrigatorio = |valor: Option<&String>, opcao: &str| {
        valor
            .map(|texto| texto.trim().to_owned())
            .ok_or_else(|| CliError::Usage(format!("informe {opcao}")))
    };
    let (Some(cnpj_cpf), Some(periodo_apuracao), Some(data_vencimento), Some(valor_principal)) = (
        args.contribuinte.clone(),
        args.periodo_apuracao,
        args.vencimento,
        args.valor_principal,
    ) else {
        return Err(CliError::Usage(
            "informe o DARF por --arquivo ou pelas opções --codigo-receita, --contribuinte, --nome-empresa, --periodo-apuracao, --vencimento, --referencia, --descricao e --valor-principal"
                .to_owned(),
        ));
    };
    let darf = PagamentoDarf {
        cnpj_cpf,
        codigo_receita: obrigatorio(args.codigo_receita.as_ref(), "--codigo-receita")?,
        data_vencimento,
        descricao: obrigatorio(args.descricao.as_ref(), "--descricao")?,
        nome_empresa: obrigatorio(args.nome_empresa.as_ref(), "--nome-empresa")?,
        telefone_empresa: args
            .telefone
            .as_deref()
            .map(str::trim)
            .filter(|telefone| !telefone.is_empty())
            .map(str::to_owned),
        periodo_apuracao,
        valor_principal,
        valor_multa: args.multa,
        valor_juros: args.juros,
        referencia: obrigatorio(args.referencia.as_ref(), "--referencia")?,
    };
    darf.validar()
        .map_err(|err| CliError::Usage(err.to_string()))?;
    Ok(darf)
}

/// What is about to be paid, for the person to check before confirming.
fn resumo(darf: &PagamentoDarf, hoje: NaiveDate, ambiente: Option<Environment>) -> String {
    let dia = |data: NaiveDate| data.format("%d/%m/%Y").to_string();
    let mut linhas = vec![
        ("Ambiente", descrever_ambiente(ambiente)),
        (
            "Contribuinte",
            format!("{} ({})", darf.nome_empresa, darf.cnpj_cpf.formatado()),
        ),
        ("Código da receita", darf.codigo_receita.clone()),
        ("Período de apuração", dia(darf.periodo_apuracao)),
        ("Vencimento", dia(darf.data_vencimento)),
        ("Referência", darf.referencia.clone()),
        ("Descrição", darf.descricao.clone()),
        ("Valor principal", output::brl(darf.valor_principal)),
    ];
    if let Some(multa) = darf.valor_multa {
        linhas.push(("Multa", output::brl(multa)));
    }
    if let Some(juros) = darf.valor_juros {
        linhas.push(("Juros", output::brl(juros)));
    }
    let total = darf.valor_total();
    let extenso = por_extenso(total)
        .map(|extenso| format!(" ({extenso})"))
        .unwrap_or_default();
    linhas.push(("Total", format!("{}{extenso}", output::brl(total))));
    linhas.push(("Quando", "agora".to_owned()));

    let mut texto = String::new();
    if ambiente.is_some_and(Environment::is_production) {
        texto.push_str("*** PRODUÇÃO: este pagamento movimenta dinheiro da conta real ***\n");
    }
    texto.push_str("DARF a pagar");
    for linha in output::key_values_left(&linhas).lines() {
        let _ = write!(texto, "\n  {linha}");
    }
    for aviso in avisos(darf, hoje) {
        let _ = write!(texto, "\naviso: {aviso}");
    }
    texto
}

/// What deserves a second look before paying: a DARF past its due date
/// without the additions, which the API does not compute.
pub(super) fn avisos(darf: &PagamentoDarf, hoje: NaiveDate) -> Vec<String> {
    let sem_acrescimos = darf.valor_multa.is_none_or(|multa| multa.is_zero())
        && darf.valor_juros.is_none_or(|juros| juros.is_zero());
    if darf.data_vencimento < hoje && sem_acrescimos {
        vec![format!(
            "o DARF venceu em {} e não tem multa nem juros: pago depois do vencimento, ele precisa dos acréscimos calculados",
            darf.data_vencimento.format("%d/%m/%Y")
        )]
    } else {
        Vec::new()
    }
}

fn render(solicitacao: &SolicitacaoDarf) -> String {
    let data = solicitacao.data_pagamento.as_deref().map(data_br);
    let titulo = match &solicitacao.tipo_retorno {
        Some(TipoRetornoDarf::Pagamento) => "DARF pago.".to_owned(),
        Some(TipoRetornoDarf::Agendamento) => match &data {
            Some(data) => format!("DARF agendado para {data}."),
            None => "DARF agendado.".to_owned(),
        },
        Some(TipoRetornoDarf::AprovacaoPagamento | TipoRetornoDarf::AprovacaoAgendamento) => {
            "DARF aguardando aprovação no Internet Banking (Aprovar > Gestão de Aprovações): só será pago depois de aprovado."
                .to_owned()
        }
        Some(outro) => format!("DARF registrado (retorno da API: {outro})."),
        None => "DARF registrado.".to_owned(),
    };
    let mut linhas = Vec::new();
    if let Some(codigo) = &solicitacao.codigo_solicitacao {
        linhas.push(("Código da solicitação", codigo.clone()));
    }
    if let Some(data) = data {
        linhas.push(("Data do pagamento", data));
    }
    if let Some(autenticacao) = &solicitacao.autenticacao {
        linhas.push(("Autenticação", autenticacao.clone()));
    }
    if let Some(aprovadores) = solicitacao.quantidade_aprovadores.filter(|n| *n > 0) {
        linhas.push(("Aprovações necessárias", aprovadores.to_string()));
    }
    let mut texto = titulo;
    if !linhas.is_empty() {
        let _ = write!(texto, "\n{}", output::key_values_left(&linhas));
    }
    if let Some(codigo) = &solicitacao.codigo_solicitacao {
        let _ = write!(
            texto,
            "\n\nAcompanhe com: {} pagamento darf listar --codigo-solicitacao {codigo}",
            chamada()
        );
    }
    texto
}

// --- listar ----------------------------------------------------------------------

async fn listar(context: &Context, args: &DarfListarArgs) -> Result<(), CliError> {
    let filtro = filtro(args, hoje());
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let darfs = client.banking().darfs(&filtro).await?;
    match context.formato() {
        Formato::Json => output::print_json(&json!({ "darfs": darfs })),
        Formato::Csv => output::print_csv(&csv(&darfs), context.separador()),
        Formato::Texto => {
            context.warn_if_sandbox(&settings);
            output::print(&render_lista(&filtro, &darfs))
        }
    }
}

/// With a date, the period of payment (the other date by default as in
/// the statement); without dates, the API's own default: the DARFs
/// requested in the last 30 days.
fn filtro(args: &DarfListarArgs, hoje: NaiveDate) -> FiltroDarf {
    let periodo = (args.inicio.is_some() || args.fim.is_some()).then(|| {
        intervalo(
            PeriodoArgs {
                inicio: args.inicio,
                fim: args.fim,
            },
            hoje,
        )
    });
    FiltroDarf {
        periodo,
        codigo_receita: args.codigo_receita.clone(),
        codigo_solicitacao: args.codigo_solicitacao.clone(),
    }
}

fn render_lista(filtro: &FiltroDarf, darfs: &[Darf]) -> String {
    let mut texto = match filtro.periodo {
        Some((inicio, fim)) => format!(
            "DARFs pagos de {} a {}",
            inicio.format("%d/%m/%Y"),
            fim.format("%d/%m/%Y")
        ),
        None => "DARFs incluídos nos últimos 30 dias".to_owned(),
    };
    if let Some(codigo) = &filtro.codigo_receita {
        let _ = write!(texto, "\nCódigo da receita: {codigo}");
    }
    texto.push_str("\n\n");
    if darfs.is_empty() {
        texto.push_str("Nenhum DARF encontrado.");
        return texto;
    }
    let mut tabela = Tabela::new(vec![
        Coluna::texto("Pagamento", ""),
        Coluna::texto("Receita", ""),
        Coluna::texto("Apuração", ""),
        Coluna::texto("Vencimento", ""),
        Coluna::texto("Status", ""),
        Coluna::valor("Total", ""),
        Coluna::texto("Código da solicitação", ""),
    ]);
    for d in darfs {
        tabela.linha(vec![
            data(d.data_pagamento.as_deref()),
            Celula::texto(d.codigo_receita.as_deref()),
            data(d.periodo_apuracao.as_deref()),
            data(d.data_vencimento.as_deref()),
            celula_status(d.status_pagamento.as_ref()),
            Celula::dinheiro(d.valor_total.or(d.valor)),
            Celula::texto(d.codigo_solicitacao.as_deref()),
        ]);
    }
    texto.push_str(&tabela.texto_colorido());
    let plural = if darfs.len() == 1 { "DARF" } else { "DARFs" };
    let _ = write!(texto, "\n\n{} {plural}", darfs.len());
    texto
}

/// Every field, with the API's names and codes.
fn csv(darfs: &[Darf]) -> Tabela {
    let texto = |campo: &'static str| Coluna::texto(campo, campo);
    let valor = |campo: &'static str| Coluna::valor(campo, campo);
    let mut tabela = Tabela::new(vec![
        texto("codigoSolicitacao"),
        texto("tipoDarf"),
        texto("tipo"),
        texto("codigoReceita"),
        texto("periodoApuracao"),
        texto("dataVencimento"),
        texto("dataPagamento"),
        texto("dataInclusao"),
        valor("valor"),
        valor("valorMulta"),
        valor("valorJuros"),
        valor("valorTotal"),
        texto("referencia"),
        texto("statusPagamento"),
        texto("cnpjCpf"),
        texto("aprovacoesNecessarias"),
        texto("aprovacoesRealizadas"),
    ]);
    let numero = |n: Option<u64>| Celula::texto(n.map(|n| n.to_string()).as_deref());
    for d in darfs {
        tabela.linha(vec![
            Celula::texto(d.codigo_solicitacao.as_deref()),
            Celula::texto(d.tipo_darf.as_deref()),
            Celula::texto(d.tipo.as_deref()),
            Celula::texto(d.codigo_receita.as_deref()),
            data(d.periodo_apuracao.as_deref()),
            data(d.data_vencimento.as_deref()),
            data(d.data_pagamento.as_deref()),
            data(d.data_inclusao.as_deref()),
            Celula::dinheiro(d.valor),
            Celula::dinheiro(d.valor_multa),
            Celula::dinheiro(d.valor_juros),
            Celula::dinheiro(d.valor_total),
            Celula::texto(d.referencia.as_deref()),
            Celula::texto(d.status_pagamento.as_ref().map(StatusPagamento::as_str)),
            Celula::texto(d.cnpj_cpf.as_deref()),
            numero(d.aprovacoes_necessarias),
            numero(d.aprovacoes_realizadas),
        ]);
    }
    tabela
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;

    use clap::{CommandFactory, FromArgMatches, Parser};
    use wiremock::matchers::{any, body_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::cli::{Cli, Command, PagamentoCommand};
    use crate::commands::Env;
    use crate::confirmacao::testes::TerminalFalso;

    const OPCOES: [&str; 20] = [
        "--codigo-receita",
        "0220",
        "--contribuinte",
        "12.345.678/0001-95",
        "--nome-empresa",
        "Empresa Exemplo",
        "--periodo-apuracao",
        "2026-09-30",
        "--vencimento",
        "2026-10-30",
        "--referencia",
        "13609400849201739",
        "--descricao",
        " IRPJ de setembro ",
        "--valor-principal",
        "47,14",
        "--multa",
        "27,48",
        "--juros",
        "10,11",
    ];

    fn pagar_args(extra: &[&str]) -> DarfPagarArgs {
        let mut full = vec!["inter-pj", "pagamento", "darf", "pagar"];
        full.extend_from_slice(extra);
        match Cli::try_parse_from(full).unwrap().command {
            Command::Pagamento(PagamentoCommand::Darf(DarfCommand::Pagar(args))) => *args,
            other => panic!("comando inesperado: {other:?}"),
        }
    }

    fn listar_args(extra: &[&str]) -> DarfListarArgs {
        let mut full = vec!["inter-pj", "pagamento", "darf", "listar"];
        full.extend_from_slice(extra);
        match Cli::try_parse_from(full).unwrap().command {
            Command::Pagamento(PagamentoCommand::Darf(DarfCommand::Listar(args))) => args,
            other => panic!("comando inesperado: {other:?}"),
        }
    }

    fn dia(mes: u32, dia: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, mes, dia).unwrap()
    }

    #[test]
    fn builds_the_darf_from_the_options() {
        let construido = darf(&pagar_args(&OPCOES)).unwrap();
        assert_eq!(construido.cnpj_cpf.as_str(), "12345678000195");
        assert_eq!(construido.descricao, "IRPJ de setembro");
        assert_eq!(construido.valor_total(), "84.73".parse().unwrap());
        assert_eq!(construido.telefone_empresa, None);

        let mut opcoes = OPCOES.to_vec();
        opcoes[1] = "220";
        let err = darf(&pagar_args(&opcoes)).unwrap_err().to_string();
        assert_eq!(err, "o código da receita tem 4 dígitos (ex.: 0220)");
    }

    #[test]
    fn builds_the_darf_from_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let caminho = dir.path().join("darf.json");
        fs::write(
            &caminho,
            json!({
                "cnpjCpf": "12345678000195",
                "codigoReceita": "0220",
                "dataVencimento": "2026-10-30",
                "descricao": "IRPJ de setembro",
                "nomeEmpresa": "Empresa Exemplo",
                "periodoApuracao": "2026-09-30",
                "valorPrincipal": 47.14,
                "referencia": "13609400849201739"
            })
            .to_string(),
        )
        .unwrap();
        let caminho = caminho.display().to_string();
        let lido = darf(&pagar_args(&["--arquivo", &caminho])).unwrap();
        assert_eq!(lido.valor_total(), "47.14".parse().unwrap());

        fs::write(&caminho, r#"{"codigoReceita": "0220"}"#).unwrap();
        let err = darf(&pagar_args(&["--arquivo", &caminho]))
            .unwrap_err()
            .to_string();
        assert!(
            err.ends_with("darf.json, campo \"cnpjCpf\": obrigatório"),
            "{err}"
        );
    }

    #[test]
    fn summary_shows_every_amount_and_the_total() {
        let construido = darf(&pagar_args(&OPCOES)).unwrap();
        assert_eq!(
            resumo(&construido, dia(9, 23), Some(Environment::Sandbox)),
            "\
DARF a pagar
  Ambiente             sandbox (dados fictícios)
  Contribuinte         Empresa Exemplo (12.345.678/0001-95)
  Código da receita    0220
  Período de apuração  30/09/2026
  Vencimento           30/10/2026
  Referência           13609400849201739
  Descrição            IRPJ de setembro
  Valor principal      R$ 47,14
  Multa                R$ 27,48
  Juros                R$ 10,11
  Total                R$ 84,73 (oitenta e quatro reais e setenta e três centavos)
  Quando               agora"
        );
    }

    #[test]
    fn summary_warns_about_a_late_darf_without_additions() {
        let mut construido = darf(&pagar_args(&OPCOES)).unwrap();
        let texto = resumo(&construido, dia(11, 3), Some(Environment::Production));
        assert!(texto.starts_with("*** PRODUÇÃO"), "{texto}");
        assert!(!texto.contains("aviso"), "{texto}");

        construido.valor_multa = None;
        construido.valor_juros = Some(rust_decimal::Decimal::ZERO);
        let texto = resumo(&construido, dia(11, 3), None);
        assert!(
            texto.ends_with("aviso: o DARF venceu em 30/10/2026 e não tem multa nem juros: pago depois do vencimento, ele precisa dos acréscimos calculados"),
            "{texto}"
        );
        assert!(!resumo(&construido, dia(10, 30), None).contains("aviso"));
    }

    #[test]
    fn renders_each_outcome() {
        let solicitacao = |tipo: &str| -> SolicitacaoDarf {
            serde_json::from_value(json!({
                "tipoRetorno": tipo,
                "codigoSolicitacao": "8bbdede4-35db-4ec9-b652-e176841e62c8",
                "dataPagamento": "01/10/2026",
                "autenticacao": "AUTENTICACAO-DE-TESTE",
                "quantidadeAprovadores": 0
            }))
            .unwrap()
        };
        assert_eq!(
            render(&solicitacao("PAGAMENTO")),
            "\
DARF pago.
Código da solicitação  8bbdede4-35db-4ec9-b652-e176841e62c8
Data do pagamento      01/10/2026
Autenticação           AUTENTICACAO-DE-TESTE

Acompanhe com: inter-pj pagamento darf listar --codigo-solicitacao 8bbdede4-35db-4ec9-b652-e176841e62c8"
        );
        assert!(render(&solicitacao("AGENDAMENTO")).starts_with("DARF agendado para 01/10/2026."));
        for tipo in ["APROVACAO_PAGAMENTO", "APROVACAO_AGENDAMENTO"] {
            assert!(
                render(&solicitacao(tipo)).starts_with("DARF aguardando aprovação"),
                "{tipo}"
            );
        }
        assert!(
            render(&solicitacao("NOVO")).starts_with("DARF registrado (retorno da API: NOVO).")
        );
        assert_eq!(render(&SolicitacaoDarf::default()), "DARF registrado.");
    }

    fn darfs() -> Vec<Darf> {
        serde_json::from_value(json!([{
            "codigoSolicitacao": "8bbdede4-35db-4ec9-b652-e176841e62c8",
            "tipoDarf": "PRETO",
            "valor": 47.14,
            "valorMulta": 0,
            "valorJuros": 10.11,
            "valorTotal": 57.25,
            "tipo": "DARF",
            "periodoApuracao": "2026-09-30 00:00:00",
            "dataPagamento": "2026-10-01 10:00:00",
            "referencia": "13609400849201739",
            "dataVencimento": "2026-10-30 00:00:00",
            "codigoReceita": "0220",
            "statusPagamento": "REALIZADO",
            "dataInclusao": "2026-10-01 09:59:00",
            "cnpjCpf": "12345678000195"
        }]))
        .unwrap()
    }

    #[test]
    fn the_period_is_sent_only_when_given() {
        let sem_datas = filtro(&listar_args(&[]), dia(9, 23));
        assert_eq!(sem_datas.periodo, None);
        let com_inicio = filtro(&listar_args(&["--inicio", "2026-09-01"]), dia(9, 23));
        assert_eq!(com_inicio.periodo, Some((dia(9, 1), dia(9, 23))));
        let com_fim = filtro(
            &listar_args(&["--fim", "2026-09-30", "--codigo-receita", "0220"]),
            dia(9, 23),
        );
        assert_eq!(com_fim.periodo, Some((dia(9, 1), dia(9, 30))));
        assert_eq!(com_fim.codigo_receita.as_deref(), Some("0220"));
    }

    #[test]
    fn renders_the_listing() {
        let construido = filtro(
            &listar_args(&["--inicio", "2026-10-01", "--fim", "2026-10-31"]),
            dia(9, 23),
        );
        assert_eq!(
            render_lista(&construido, &darfs()),
            "\
DARFs pagos de 01/10/2026 a 31/10/2026

Pagamento   Receita  Apuração    Vencimento  Status     Total  Código da solicitação
01/10/2026  0220     30/09/2026  30/10/2026  pago    R$ 57,25  8bbdede4-35db-4ec9-b652-e176841e62c8

1 DARF"
        );
        let sem_datas = filtro(&listar_args(&["--codigo-receita", "0220"]), dia(9, 23));
        assert_eq!(
            render_lista(&sem_datas, &[]),
            "DARFs incluídos nos últimos 30 dias\nCódigo da receita: 0220\n\nNenhum DARF encontrado."
        );
    }

    #[test]
    fn csv_has_every_field_with_the_api_names() {
        let csv = csv(&darfs()).csv(crate::tabela::Separador::Virgula);
        let linhas: Vec<&str> = csv.split("\r\n").collect();
        assert_eq!(
            linhas[0],
            "codigoSolicitacao,tipoDarf,tipo,codigoReceita,periodoApuracao,dataVencimento,dataPagamento,dataInclusao,valor,valorMulta,valorJuros,valorTotal,referencia,statusPagamento,cnpjCpf,aprovacoesNecessarias,aprovacoesRealizadas"
        );
        assert_eq!(
            linhas[1],
            "8bbdede4-35db-4ec9-b652-e176841e62c8,PRETO,DARF,0220,2026-09-30,2026-10-30,2026-10-01,2026-10-01,47.14,0,10.11,57.25,13609400849201739,REALIZADO,12345678000195,,"
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
        args: DarfPagarArgs,
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
            "darf",
            "pagar",
        ];
        full.extend_from_slice(&OPCOES);
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
        let Command::Pagamento(PagamentoCommand::Darf(DarfCommand::Pagar(args))) = cli.command
        else {
            unreachable!("pagamento darf pagar");
        };
        Cenario {
            server,
            _dir: dir,
            context,
            args: *args,
        }
    }

    #[tokio::test]
    async fn declined_confirmation_pays_nothing() {
        let cenario = cenario(&[]).await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&cenario.server)
            .await;
        for resposta in ["n\n", "\n", "pagar\n"] {
            let mut terminal = TerminalFalso::respondendo(resposta);
            let err = pagar(&cenario.context, &cenario.args, &mut terminal)
                .await
                .unwrap_err();
            assert!(matches!(err, CliError::Cancelado), "{resposta:?}: {err}");
            assert_eq!(
                terminal.perguntas,
                ["Confirmar o pagamento do DARF? [s/N] "]
            );
        }
    }

    #[tokio::test]
    async fn confirmed_darf_is_sent_once_with_the_documented_body() {
        let cenario = cenario(&[]).await;
        Mock::given(method("POST"))
            .and(path("/oauth/v2/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "tok",
                "expires_in": 3600,
                "scope": "pagamento-darf.write"
            })))
            .mount(&cenario.server)
            .await;
        Mock::given(method("POST"))
            .and(path("/banking/v2/pagamento/darf"))
            .and(body_json(json!({
                "cnpjCpf": "12345678000195",
                "codigoReceita": "0220",
                "dataVencimento": "2026-10-30",
                "descricao": "IRPJ de setembro",
                "nomeEmpresa": "Empresa Exemplo",
                "periodoApuracao": "2026-09-30",
                "valorPrincipal": 47.14,
                "valorMulta": 27.48,
                "valorJuros": 10.11,
                "referencia": "13609400849201739"
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "tipoRetorno": "PAGAMENTO",
                "codigoSolicitacao": "8bbdede4-35db-4ec9-b652-e176841e62c8"
            })))
            .expect(1)
            .mount(&cenario.server)
            .await;
        let mut terminal = TerminalFalso::respondendo("s\n");
        pagar(&cenario.context, &cenario.args, &mut terminal)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn uncertain_outcome_says_how_to_check_the_darf() {
        let cenario = cenario(&["--sim", "--sem-retentativa"]).await;
        Mock::given(method("POST"))
            .and(path("/oauth/v2/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "tok",
                "expires_in": 3600,
                "scope": "pagamento-darf.write"
            })))
            .mount(&cenario.server)
            .await;
        Mock::given(method("POST"))
            .and(path("/banking/v2/pagamento/darf"))
            .respond_with(ResponseTemplate::new(504))
            .expect(1)
            .mount(&cenario.server)
            .await;
        let err = pagar(
            &cenario.context,
            &cenario.args,
            &mut TerminalFalso::default(),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(&err, CliError::PagamentoIncerto { consulta, .. } if consulta == "inter-pj pagamento darf listar --codigo-receita 0220"),
            "{err:?}"
        );
    }
}
