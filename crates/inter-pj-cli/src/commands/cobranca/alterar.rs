//! `inter-pj cobranca cancelar|editar|edicao|pagar`

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use inter_pj::cobranca::{
    CobrancaDetalhada, ConsultaEdicao, EdicaoCobranca, PagarCom, SituacaoCobranca, StatusEdicao,
};
use inter_pj::{Environment, Error as InterError, InterClient};
use serde_json::json;

use super::{argumento, data, descrever_situacao, documento};
use crate::cli::{
    CobrancaCancelarArgs, CobrancaEdicaoArgs, CobrancaEditarArgs, CobrancaPagarArgs, FormaArg,
    Formato,
};
use crate::commands::{Context, hoje};
use crate::config::Settings;
use crate::confirmacao::{Terminal, confirmar, descrever_ambiente, pode_confirmar};
use crate::error::CliError;
use crate::output;

/// Between two queries with `--aguardar`: 10 per minute, as the sandbox
/// allows.
const INTERVALO: Duration = Duration::from_secs(6);

pub(super) async fn cancelar(
    context: &Context,
    args: &CobrancaCancelarArgs,
    terminal: &mut dyn Terminal,
) -> Result<(), CliError> {
    // Nothing is looked up when no one could confirm.
    pode_confirmar(terminal, args.sim)?;
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let codigo = args.codigo.trim();
    // Nothing is cancelled unseen.
    let cobranca = client.cobranca().consultar(codigo).await?;
    alteravel(&cobranca, "cancelada")?;
    let ambiente = settings.ambiente.as_ref().map(|setting| setting.value);
    output::eprint(&resumo(
        "Cobrança a cancelar",
        codigo,
        &cobranca,
        None,
        ambiente,
        &[("Motivo", args.motivo.clone())],
    ));
    confirmar(terminal, args.sim, "Cancelar a cobrança?")?;

    client.cobranca().cancelar(codigo, &args.motivo).await?;
    match context.formato() {
        Formato::Json => output::print_json(&json!({
            "codigoSolicitacao": codigo,
            "cancelamentoSolicitado": true,
        })),
        // `commands::run` refuses csv for this command.
        Formato::Texto | Formato::Csv => output::print(&format!(
            "Cancelamento solicitado.\n\nConfira com: inter-pj cobranca consultar {codigo}"
        )),
    }
}

pub(super) async fn editar(
    context: &Context,
    args: &CobrancaEditarArgs,
    terminal: &mut dyn Terminal,
) -> Result<(), CliError> {
    editar_com(context, args, terminal, INTERVALO).await
}

async fn editar_com(
    context: &Context,
    args: &CobrancaEditarArgs,
    terminal: &mut dyn Terminal,
    intervalo: Duration,
) -> Result<(), CliError> {
    let edicao = EdicaoCobranca::new(args.vencimento, args.valor);
    edicao
        .validar()
        .map_err(|err| CliError::Usage(err.to_owned()))?;
    if let Some(vencimento) = args.vencimento
        && vencimento < hoje()
    {
        return Err(CliError::Usage(format!(
            "o novo vencimento ({}) já passou: a cobrança vence hoje ou depois",
            vencimento.format("%d/%m/%Y")
        )));
    }
    pode_confirmar(terminal, args.sim)?;
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let codigo = args.codigo.trim();
    let cobranca = client.cobranca().consultar(codigo).await?;
    alteravel(&cobranca, "alterada")?;
    let ambiente = settings.ambiente.as_ref().map(|setting| setting.value);
    output::eprint(&resumo(
        "Cobrança a alterar",
        codigo,
        &cobranca,
        Some(&edicao),
        ambiente,
        &[],
    ));
    confirmar(terminal, args.sim, "Alterar a cobrança?")?;

    let solicitacao = client.cobranca().editar(codigo, &edicao).await?;
    let codigo_edicao = solicitacao
        .codigo_edicao
        .as_deref()
        .map(str::trim)
        .filter(|codigo| !codigo.is_empty());
    let terminou = solicitacao
        .status
        .as_ref()
        .is_some_and(StatusEdicao::is_final);
    if let Some(codigo_edicao) = codigo_edicao.filter(|_| args.espera.aguardar && !terminou) {
        let (consulta, terminou) =
            aguardar(&client, codigo_edicao, args.espera.timeout, intervalo).await?;
        mostrar_edicao(context, &settings, codigo_edicao, &consulta)?;
        return fim(&consulta, terminou, args.espera.timeout);
    }
    if args.espera.aguardar && !terminou && codigo_edicao.is_none() {
        eprintln!("aviso: a API não informou o código da alteração; não há como aguardar");
    }
    match context.formato() {
        Formato::Json => output::print_json(&solicitacao)?,
        // `commands::run` refuses csv for this command.
        Formato::Texto | Formato::Csv => output::print(&render_edicao(
            codigo_edicao,
            solicitacao.status.as_ref(),
            solicitacao.mensagem.as_deref(),
        ))?,
    }
    match solicitacao.status {
        Some(StatusEdicao::Falha) => Err(CliError::EdicaoNaoFeita {
            motivo: solicitacao
                .mensagem
                .filter(|mensagem| !mensagem.trim().is_empty())
                .unwrap_or_else(|| "a API informou falha".to_owned()),
        }),
        _ => Ok(()),
    }
}

pub(super) async fn edicao(context: &Context, args: &CobrancaEdicaoArgs) -> Result<(), CliError> {
    edicao_com(context, args, INTERVALO).await
}

async fn edicao_com(
    context: &Context,
    args: &CobrancaEdicaoArgs,
    intervalo: Duration,
) -> Result<(), CliError> {
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let codigo = args.codigo_edicao.trim();
    if !args.espera.aguardar {
        let consulta = client.cobranca().consultar_edicao(codigo).await?;
        return mostrar_edicao(context, &settings, codigo, &consulta);
    }
    let (consulta, terminou) = aguardar(&client, codigo, args.espera.timeout, intervalo).await?;
    mostrar_edicao(context, &settings, codigo, &consulta)?;
    fim(&consulta, terminou, args.espera.timeout)
}

pub(super) async fn pagar(context: &Context, args: &CobrancaPagarArgs) -> Result<(), CliError> {
    let settings = context.settings()?;
    // Refused before anything else: in production, the client pays.
    if settings
        .ambiente
        .as_ref()
        .is_some_and(|ambiente| ambiente.value.is_production())
    {
        return Err(CliError::Usage(
            "cobranca pagar existe só no sandbox, para testes: em produção, quem paga é o cliente, com o boleto ou o Pix"
                .to_owned(),
        ));
    }
    let client = context.client(&settings)?;
    let (com, forma) = match args.com {
        FormaArg::Boleto => (PagarCom::Boleto, "o boleto"),
        FormaArg::Pix => (PagarCom::Pix, "o Pix"),
    };
    let codigo = args.codigo.trim();
    client.cobranca().pagar_no_sandbox(codigo, com).await?;
    match context.formato() {
        Formato::Json => output::print_json(&json!({
            "codigoSolicitacao": codigo,
            "pagarCom": com.as_str(),
            "paga": true,
        })),
        // `commands::run` refuses csv for this command.
        Formato::Texto | Formato::Csv => output::print(&format!(
            "Cobrança paga no sandbox, com {forma}.\n\nConfira com: inter-pj cobranca consultar {codigo}"
        )),
    }
}

/// Refuses what can no longer change: a charge paid, cancelled or expired.
fn alteravel(detalhe: &CobrancaDetalhada, operacao: &str) -> Result<(), CliError> {
    let motivo = match &detalhe.cobranca.situacao {
        Some(SituacaoCobranca::Recebido | SituacaoCobranca::MarcadoRecebido) => "já foi paga",
        Some(SituacaoCobranca::Cancelado) => "já está cancelada",
        Some(SituacaoCobranca::Expirado) => "expirou (foi cancelada sem pagamento)",
        _ => return Ok(()),
    };
    Err(CliError::Usage(format!(
        "a cobrança {motivo}: não pode ser {operacao}"
    )))
}

/// The charge about to change and, for an edit, its new values.
fn resumo(
    titulo: &str,
    codigo: &str,
    detalhe: &CobrancaDetalhada,
    edicao: Option<&EdicaoCobranca>,
    ambiente: Option<Environment>,
    extra: &[(&str, String)],
) -> String {
    let cobranca = &detalhe.cobranca;
    let mut linhas = vec![("Ambiente", descrever_ambiente(ambiente))];
    if let Some(numero) = &cobranca.seu_numero {
        linhas.push(("Seu número", numero.clone()));
    }
    if let Some(situacao) = &cobranca.situacao {
        linhas.push(("Situação", descrever_situacao(situacao)));
    }
    let novo_valor = edicao.and_then(|edicao| edicao.valor_nominal);
    if let Some(valor) = antes_e_depois(
        cobranca.valor_nominal.map(output::brl),
        novo_valor.map(output::brl),
    ) {
        linhas.push(("Valor", valor));
    }
    let novo_vencimento = edicao.and_then(|edicao| edicao.data_vencimento);
    if let Some(vencimento) = antes_e_depois(
        cobranca.data_vencimento.as_deref().map(data),
        novo_vencimento.map(|dia| dia.format("%d/%m/%Y").to_string()),
    ) {
        linhas.push(("Vencimento", vencimento));
    }
    if let Some(pagador) = &cobranca.pagador {
        let nome = pagador.nome.clone().unwrap_or_default();
        let doc = pagador
            .cpf_cnpj
            .as_deref()
            .map(|doc| format!(" ({})", documento(doc)))
            .unwrap_or_default();
        linhas.push(("Pagador", format!("{nome}{doc}").trim().to_owned()));
    }
    linhas.push(("Código", codigo.to_owned()));
    linhas.extend_from_slice(extra);

    let mut texto = titulo.to_owned();
    for linha in output::key_values_left(&linhas).lines() {
        let _ = write!(texto, "\n  {linha}");
    }
    if edicao.is_some() {
        texto.push_str(
            "\naviso: a consulta pode levar até 30 minutos para mostrar o novo valor ou vencimento",
        );
    }
    texto
}

/// `R$ 150,00 → R$ 200,00`, or what stays.
fn antes_e_depois(antes: Option<String>, depois: Option<String>) -> Option<String> {
    match (antes, depois) {
        (Some(antes), Some(depois)) => Some(format!("{antes} → {depois}")),
        (None, Some(depois)) => Some(format!("→ {depois}")),
        (antes, None) => antes,
    }
}

/// Queries the change every `intervalo` until it ends or `timeout` passes:
/// the last answer, and whether it ended. Right after the request, the
/// change may not be found yet: that counts as being processed.
async fn aguardar(
    client: &InterClient,
    codigo_edicao: &str,
    timeout: Duration,
    intervalo: Duration,
) -> Result<(ConsultaEdicao, bool), CliError> {
    let prazo = Instant::now() + timeout;
    let mut anunciado = false;
    loop {
        let ultima = match client.cobranca().consultar_edicao(codigo_edicao).await {
            Ok(consulta) if consulta.status.as_ref().is_some_and(StatusEdicao::is_final) => {
                return Ok((consulta, true));
            }
            Ok(consulta) => Some(consulta),
            Err(InterError::Api(api)) if api.status == 404 => None,
            Err(err) => return Err(err.into()),
        };
        let agora = Instant::now();
        if agora >= prazo {
            return ultima
                .map(|consulta| (consulta, false))
                .ok_or_else(|| tempo_esgotado(timeout));
        }
        if !anunciado {
            eprintln!("aguardando a alteração...");
            anunciado = true;
        }
        tokio::time::sleep(intervalo.min(prazo - agora)).await;
    }
}

fn tempo_esgotado(timeout: Duration) -> CliError {
    CliError::TempoEsgotado {
        oque: "a alteração",
        status: "em processamento".to_owned(),
        segundos: timeout.as_secs(),
    }
}

/// How an awaited change ended: 5 when it failed, 8 when time ran out.
fn fim(consulta: &ConsultaEdicao, terminou: bool, timeout: Duration) -> Result<(), CliError> {
    match consulta.status {
        _ if !terminou => Err(tempo_esgotado(timeout)),
        Some(StatusEdicao::Falha) => Err(CliError::EdicaoNaoFeita {
            motivo: "a API informou falha".to_owned(),
        }),
        _ => Ok(()),
    }
}

fn mostrar_edicao(
    context: &Context,
    settings: &Settings,
    codigo_edicao: &str,
    consulta: &ConsultaEdicao,
) -> Result<(), CliError> {
    match context.formato() {
        Formato::Json => output::print_json(&json!({
            "codigoEdicao": codigo_edicao,
            "status": consulta.status,
        })),
        // `commands::run` refuses csv for this command.
        Formato::Texto | Formato::Csv => {
            context.warn_if_sandbox(settings);
            output::print(&render_edicao(
                Some(codigo_edicao),
                consulta.status.as_ref(),
                None,
            ))
        }
    }
}

/// Where a change stands, and how to follow it while it is processed.
fn render_edicao(
    codigo_edicao: Option<&str>,
    status: Option<&StatusEdicao>,
    mensagem: Option<&str>,
) -> String {
    let mut texto = match status {
        Some(StatusEdicao::Sucesso) => "Alteração feita: a consulta pode levar até 30 minutos para mostrar o novo valor ou vencimento.".to_owned(),
        Some(StatusEdicao::Falha) => "A alteração não foi feita.".to_owned(),
        Some(StatusEdicao::Processando) => "Alteração em processamento.".to_owned(),
        Some(outro) => format!("Alteração: {}.", outro.as_str()),
        None => "Alteração solicitada.".to_owned(),
    };
    let mut linhas = Vec::new();
    if let Some(mensagem) = mensagem.filter(|mensagem| !mensagem.trim().is_empty()) {
        linhas.push(("Mensagem", mensagem.to_owned()));
    }
    if let Some(codigo) = codigo_edicao {
        linhas.push(("Código da alteração", codigo.to_owned()));
    }
    if !linhas.is_empty() {
        let _ = write!(texto, "\n{}", output::key_values_left(&linhas));
    }
    if let Some(codigo) = codigo_edicao
        && !status.is_some_and(StatusEdicao::is_final)
    {
        let _ = write!(
            texto,
            "\n\nAcompanhe com: inter-pj cobranca edicao {} --aguardar",
            argumento(codigo)
        );
    }
    texto
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use serde_json::{Value, json};
    use wiremock::matchers::{any, body_json, method, path};
    use wiremock::{Mock, ResponseTemplate};

    use super::super::testes;
    use super::*;
    use crate::cli::CobrancaCommand;
    use crate::confirmacao::testes::TerminalFalso;

    const CODIGO: &str = "0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d";
    const EDICAO: &str = "5a6b7c8d-1e2f-4a3b-8c9d-0e1f2a3b4c5d";

    fn cobranca(situacao: &str) -> Value {
        json!({
            "cobranca": {
                "codigoSolicitacao": CODIGO,
                "seuNumero": "NF-123",
                "situacao": situacao,
                "dataVencimento": "2026-10-20",
                "valorNominal": 150,
                "pagador": {"nome": "Cliente Exemplo Ltda", "cpfCnpj": "12345678000195"}
            }
        })
    }

    fn detalhe(situacao: &str) -> CobrancaDetalhada {
        serde_json::from_value(cobranca(situacao)).unwrap()
    }

    #[test]
    fn what_was_paid_cancelled_or_expired_cannot_change() {
        for (situacao, motivo) in [
            ("RECEBIDO", "já foi paga"),
            ("MARCADO_RECEBIDO", "já foi paga"),
            ("CANCELADO", "já está cancelada"),
            ("EXPIRADO", "expirou (foi cancelada sem pagamento)"),
        ] {
            let err = alteravel(&detalhe(situacao), "cancelada").unwrap_err();
            assert_eq!(
                err.to_string(),
                format!("a cobrança {motivo}: não pode ser cancelada")
            );
            assert_eq!(err.exit_code(), 2);
        }
        for situacao in [
            "A_RECEBER",
            "ATRASADO",
            "EM_PROCESSAMENTO",
            "PROTESTO",
            "OUTRA",
        ] {
            assert!(
                alteravel(&detalhe(situacao), "alterada").is_ok(),
                "{situacao}"
            );
        }
    }

    #[test]
    fn summaries_show_what_changes() {
        let edicao = EdicaoCobranca::new(NaiveDate::from_ymd_opt(2026, 11, 10), None);
        assert_eq!(
            resumo(
                "Cobrança a alterar",
                CODIGO,
                &detalhe("ATRASADO"),
                Some(&edicao),
                Some(Environment::Sandbox),
                &[],
            ),
            format!(
                "\
Cobrança a alterar
  Ambiente    sandbox (dados fictícios)
  Seu número  NF-123
  Situação    atrasada
  Valor       R$ 150,00
  Vencimento  20/10/2026 → 10/11/2026
  Pagador     Cliente Exemplo Ltda (12.345.678/0001-95)
  Código      {CODIGO}
aviso: a consulta pode levar até 30 minutos para mostrar o novo valor ou vencimento"
            )
        );
        let edicao = EdicaoCobranca::new(None, Some("200.5".parse().unwrap()));
        let texto = resumo(
            "Cobrança a alterar",
            CODIGO,
            &detalhe("A_RECEBER"),
            Some(&edicao),
            None,
            &[],
        );
        assert!(
            texto.contains("Valor       R$ 150,00 → R$ 200,50\n  Vencimento  20/10/2026\n"),
            "{texto}"
        );

        let texto = resumo(
            "Cobrança a cancelar",
            CODIGO,
            &detalhe("A_RECEBER"),
            None,
            Some(Environment::Production),
            &[("Motivo", "Pedido cancelado".to_owned())],
        );
        assert!(
            texto.contains("Ambiente    PRODUÇÃO (conta real)"),
            "{texto}"
        );
        assert!(
            texto.ends_with(&format!(
                "Código      {CODIGO}\n  Motivo      Pedido cancelado"
            )),
            "{texto}"
        );
    }

    #[test]
    fn changes_in_words() {
        assert_eq!(
            render_edicao(
                Some(EDICAO),
                Some(&StatusEdicao::Processando),
                Some("Em processamento")
            ),
            format!(
                "Alteração em processamento.\nMensagem             Em processamento\nCódigo da alteração  {EDICAO}\n\nAcompanhe com: inter-pj cobranca edicao {EDICAO} --aguardar"
            )
        );
        assert_eq!(
            render_edicao(Some(EDICAO), Some(&StatusEdicao::Sucesso), None),
            format!(
                "Alteração feita: a consulta pode levar até 30 minutos para mostrar o novo valor ou vencimento.\nCódigo da alteração  {EDICAO}"
            )
        );
        assert_eq!(
            render_edicao(None, Some(&StatusEdicao::Falha), Some("Cobrança já paga")),
            "A alteração não foi feita.\nMensagem  Cobrança já paga"
        );
        assert_eq!(render_edicao(None, None, None), "Alteração solicitada.");
    }

    // --- the commands against a mock API ------------------------------------

    async fn mount_cobranca(cenario: &testes::Cenario, situacao: &str) {
        Mock::given(method("GET"))
            .and(path(format!("/cobranca/v3/cobrancas/{CODIGO}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(cobranca(situacao)))
            .expect(1)
            .mount(&cenario.server)
            .await;
    }

    /// Nothing is sent but the lookup.
    async fn nada_alem_da_consulta(cenario: &testes::Cenario) {
        Mock::given(any())
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&cenario.server)
            .await;
    }

    #[tokio::test]
    async fn a_declined_cancellation_only_looks_the_charge_up() {
        let (cenario, comando) =
            testes::cenario(&["cancelar", CODIGO, "--motivo", "Pedido cancelado"]).await;
        let CobrancaCommand::Cancelar(args) = comando else {
            unreachable!()
        };
        mount_cobranca(&cenario, "A_RECEBER").await;
        nada_alem_da_consulta(&cenario).await;
        let mut terminal = TerminalFalso::respondendo("n\n");
        let err = cancelar(&cenario.context, &args, &mut terminal)
            .await
            .unwrap_err();
        assert!(matches!(err, CliError::Cancelado), "{err}");
        assert_eq!(terminal.perguntas, ["Cancelar a cobrança? [s/N] "]);
    }

    #[tokio::test]
    async fn a_paid_charge_is_not_cancelled() {
        let (cenario, comando) =
            testes::cenario(&["cancelar", CODIGO, "--motivo", "Pedido cancelado", "--sim"]).await;
        let CobrancaCommand::Cancelar(args) = comando else {
            unreachable!()
        };
        mount_cobranca(&cenario, "RECEBIDO").await;
        nada_alem_da_consulta(&cenario).await;
        let err = cancelar(&cenario.context, &args, &mut TerminalFalso::default())
            .await
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "a cobrança já foi paga: não pode ser cancelada"
        );
    }

    async fn editar_com_status(
        extra: &[&str],
        resposta: Value,
    ) -> (testes::Cenario, CobrancaEditarArgs) {
        let mut args = vec!["editar", CODIGO, "--valor", "200,00", "--sim"];
        args.extend_from_slice(extra);
        let (cenario, comando) = testes::cenario(&args).await;
        let CobrancaCommand::Editar(args) = comando else {
            unreachable!()
        };
        mount_cobranca(&cenario, "A_RECEBER").await;
        Mock::given(method("PATCH"))
            .and(path(format!("/cobranca/v3/cobrancas/{CODIGO}")))
            .and(body_json(json!({"valorNominal": 200})))
            .respond_with(ResponseTemplate::new(200).set_body_json(resposta))
            .expect(1)
            .mount(&cenario.server)
            .await;
        (cenario, args)
    }

    #[tokio::test]
    async fn waits_while_the_change_is_processed() {
        let (cenario, args) = editar_com_status(
            &["--aguardar", "--timeout", "10s"],
            json!({"status": "PROCESSANDO", "codigoEdicao": EDICAO}),
        )
        .await;
        let consulta = format!("/cobranca/v3/cobrancas/edicao/{EDICAO}");
        // Not found yet, then processing, then done.
        Mock::given(method("GET"))
            .and(path(consulta.clone()))
            .respond_with(ResponseTemplate::new(404))
            .up_to_n_times(1)
            .expect(1)
            .mount(&cenario.server)
            .await;
        Mock::given(method("GET"))
            .and(path(consulta.clone()))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"status": "PROCESSANDO"})),
            )
            .up_to_n_times(1)
            .expect(1)
            .mount(&cenario.server)
            .await;
        Mock::given(method("GET"))
            .and(path(consulta))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"status": "SUCESSO"})))
            .expect(1)
            .mount(&cenario.server)
            .await;
        editar_com(
            &cenario.context,
            &args,
            &mut TerminalFalso::default(),
            Duration::from_millis(10),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn a_failed_change_is_an_error() {
        let (cenario, args) = editar_com_status(
            &[],
            json!({"status": "FALHA", "mensagem": "Cobrança não pode ser alterada"}),
        )
        .await;
        let err = editar(&cenario.context, &args, &mut TerminalFalso::default())
            .await
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "a alteração da cobrança não foi feita: Cobrança não pode ser alterada"
        );
        assert_eq!(err.exit_code(), 5);
    }

    #[tokio::test]
    async fn waiting_for_a_change_can_time_out() {
        let (cenario, comando) =
            testes::cenario(&["edicao", EDICAO, "--aguardar", "--timeout", "1s"]).await;
        let CobrancaCommand::Edicao(args) = comando else {
            unreachable!()
        };
        Mock::given(method("GET"))
            .and(path(format!("/cobranca/v3/cobrancas/edicao/{EDICAO}")))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"status": "PROCESSANDO"})),
            )
            .mount(&cenario.server)
            .await;
        let err = edicao_com(&cenario.context, &args, Duration::from_millis(300))
            .await
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "tempo de espera esgotado (1 s): a alteração ainda está em processamento"
        );
        assert_eq!(err.exit_code(), 8);
    }
}
