//! `inter-pj pix devolucao solicitar|consultar`: refunds of Pix received.
//! **Money leaves the account.**

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use chrono::{Local, TimeZone};
use inter_pj::pix::{
    CobrancaPixError, Devolucao, DevolucaoSolicitada, IdDevolucao, PixRecebido, StatusDevolucao,
};
use inter_pj::{Environment, Error as InterError, InterClient, endpoint};
use rust_decimal::Decimal;

use super::{descrever_status_devolucao, devolvido, disponivel, em_devolucao};
use crate::cli::{
    Formato, PixDevolucaoCommand, PixDevolucaoConsultarArgs, PixDevolucaoSolicitarArgs,
};
use crate::commands::{Context, simulacao};
use crate::confirmacao::{
    Stdio, Terminal, confirmar, descrever_ambiente, pode_confirmar, verificar_limite,
};
use crate::error::{CliError, resultado_incerto};
use crate::output::{self, horario_em, secao};
use crate::valor::por_extenso;

/// Between two queries with `--aguardar`, within the rate limit.
const INTERVALO: Duration = Duration::from_secs(6);

pub(super) async fn run(context: &Context, command: PixDevolucaoCommand) -> Result<(), CliError> {
    match command {
        PixDevolucaoCommand::Solicitar(args) => {
            solicitar(context, &args, &mut Stdio, INTERVALO).await
        }
        PixDevolucaoCommand::Consultar(args) => consultar(context, &args, INTERVALO).await,
    }
}

async fn solicitar(
    context: &Context,
    args: &PixDevolucaoSolicitarArgs,
    terminal: &mut dyn Terminal,
    intervalo: Duration,
) -> Result<(), CliError> {
    // With --tudo, the amount comes from the Pix; until then, a placeholder
    // lets the other fields be checked before any request.
    let mut devolucao = DevolucaoSolicitada::new(args.valor.unwrap_or(Decimal::ONE));
    devolucao.natureza = args.natureza.map(Into::into);
    devolucao.descricao = args
        .descricao
        .as_deref()
        .map(str::trim)
        .filter(|descricao| !descricao.is_empty())
        .map(str::to_owned);
    devolucao.validar().map_err(|err| erro_da_opcao(&err))?;
    let settings = context.settings()?;
    if let Some(valor) = args.valor {
        verificar_limite(valor, &settings)?;
    }
    let id = args.id.clone().unwrap_or_else(IdDevolucao::novo);
    let ambiente = settings.ambiente.as_ref().map(|setting| setting.value);
    if args.simular {
        output::eprint(&resumo(&args.e2e, None, &devolucao, &id, ambiente));
        return simulacao::mostrar_em(
            context,
            &settings,
            endpoint::pix::SOLICITAR_DEVOLUCAO,
            &[("e2eId", args.e2e.as_str()), ("id", id.as_str())],
            &[],
            &devolucao,
        );
    }
    // Nothing is looked up when no one could confirm.
    pode_confirmar(terminal, args.sim)?;
    let client = context.client(&settings)?;
    let pix = client.pix().consultar_pix_recebido(&args.e2e).await?;
    if let Some(existente) = pix
        .devolucoes
        .iter()
        .find(|existente| existente.id.as_deref() == Some(id.as_str()))
    {
        // Asked before, maybe with an unknown outcome: the API would not
        // refund again, so there is nothing to send.
        output::eprint_linha(&format!(
            "aviso: a devolução {id} já tinha sido solicitada; com o mesmo id, a API não devolve de novo"
        ));
        let terminou = existente
            .status
            .as_ref()
            .is_some_and(StatusDevolucao::is_final);
        if args.espera.aguardar && !terminou {
            let (atual, terminou) =
                aguardar(&client, &args.e2e, &id, args.espera.timeout, intervalo).await?;
            mostrar(context, &args.e2e, &id, &atual)?;
            return desfecho(&atual, terminou, args.espera.timeout);
        }
        mostrar(context, &args.e2e, &id, existente)?;
        return desfecho(existente, true, args.espera.timeout);
    }
    if args.tudo {
        devolucao.valor = tudo(&pix)?;
        verificar_limite(devolucao.valor, &settings)?;
    }
    cabe_no_pix(devolucao.valor, &pix)?;
    output::eprint(&resumo(&args.e2e, Some(&pix), &devolucao, &id, ambiente));
    confirmar(terminal, args.sim, "Devolver o Pix?")?;

    let feita = client
        .pix()
        .devolver(&args.e2e, &id, &devolucao)
        .await
        .map_err(|err| incerta(err, &args.e2e, &id))?;
    let terminou = feita.status.as_ref().is_some_and(StatusDevolucao::is_final);
    if args.espera.aguardar && !terminou {
        let (atual, terminou) =
            aguardar(&client, &args.e2e, &id, args.espera.timeout, intervalo).await?;
        mostrar(context, &args.e2e, &id, &atual)?;
        return desfecho(&atual, terminou, args.espera.timeout);
    }
    match context.formato() {
        Formato::Json => output::print_json(&feita)?,
        // `commands::run` refuses csv for this command.
        Formato::Texto | Formato::Csv => {
            let mut texto = format!(
                "Devolução solicitada.\n\n{}",
                render(&feita, &args.e2e, &id)
            );
            if !terminou {
                let _ = write!(
                    texto,
                    "\n\nAcompanhe com: inter-pj pix devolucao consultar {} {id} --aguardar",
                    args.e2e
                );
            }
            output::print(&texto)?;
        }
    }
    // Without waiting, only a refused refund is an error.
    desfecho(&feita, true, args.espera.timeout)
}

/// An error of the library's checks, named by the option.
fn erro_da_opcao(err: &CobrancaPixError) -> CliError {
    let opcao = match err.campo() {
        "valor" => "--valor",
        "descricao" => "--descricao",
        outro => outro,
    };
    CliError::Usage(format!("{opcao}: {err}"))
}

/// `--tudo`: what remains of the Pix.
fn tudo(pix: &PixRecebido) -> Result<Decimal, CliError> {
    let restante = disponivel(pix).ok_or_else(|| {
        CliError::Usage("a API não informou o valor do Pix: informe --valor".to_owned())
    })?;
    if restante <= Decimal::ZERO {
        return Err(CliError::Usage(format!(
            "nada a devolver: o Pix de {} já foi devolvido, ou está em devolução, por inteiro",
            output::brl(pix.valor.unwrap_or_default())
        )));
    }
    Ok(restante)
}

/// Refuses a refund larger than what remains of the Pix. When the API does
/// not tell the amount, it is left for the API to check.
fn cabe_no_pix(valor: Decimal, pix: &PixRecebido) -> Result<(), CliError> {
    let Some(restante) = disponivel(pix) else {
        eprintln!(
            "aviso: a API não informou o valor do Pix; quem confere o limite da devolução é a API"
        );
        return Ok(());
    };
    if valor <= restante {
        return Ok(());
    }
    let comprometido = devolvido(pix) + em_devolucao(pix);
    let detalhe = if comprometido.is_zero() {
        String::new()
    } else {
        format!(
            ", {} já devolvidos ou em devolução",
            output::brl(comprometido)
        )
    };
    Err(CliError::Usage(format!(
        "a devolução de {} passa do que resta do Pix: {} de {}{detalhe}",
        output::brl(valor),
        output::brl(restante.max(Decimal::ZERO)),
        output::brl(pix.valor.unwrap_or_default()),
    )))
}

/// What is about to be refunded, for the person to check before
/// confirming; `pix` is the Pix looked up, when it was.
fn resumo(
    e2e: &str,
    pix: Option<&PixRecebido>,
    devolucao: &DevolucaoSolicitada,
    id: &IdDevolucao,
    ambiente: Option<Environment>,
) -> String {
    let mut linhas = vec![
        ("Ambiente", descrever_ambiente(ambiente)),
        ("Pix", e2e.to_owned()),
    ];
    if let Some(pix) = pix {
        if let Some(horario) = &pix.horario {
            linhas.push(("Recebido em", output::horario_local(horario)));
        }
        if let Some(valor) = pix.valor {
            linhas.push(("Valor do Pix", output::brl(valor)));
        }
        let devolvido = devolvido(pix);
        if !devolvido.is_zero() {
            linhas.push(("Já devolvido", output::brl(devolvido)));
        }
        let em_devolucao = em_devolucao(pix);
        if !em_devolucao.is_zero() {
            linhas.push(("Em devolução", output::brl(em_devolucao)));
        }
    }
    let extenso = por_extenso(devolucao.valor)
        .map(|extenso| format!(" ({extenso})"))
        .unwrap_or_default();
    linhas.push((
        "Devolução",
        format!("{}{extenso}", output::brl(devolucao.valor)),
    ));
    if let Some(natureza) = devolucao.natureza {
        linhas.push(("Natureza", natureza.as_str().to_lowercase()));
    }
    if let Some(descricao) = &devolucao.descricao {
        linhas.push(("Descrição", descricao.clone()));
    }
    linhas.push(("id", id.to_string()));
    let mut texto = String::new();
    if ambiente.is_some_and(Environment::is_production) {
        texto.push_str("*** PRODUÇÃO: esta devolução tira dinheiro da conta real ***\n");
    }
    texto.push_str(&secao("Devolução a solicitar", &linhas));
    texto
}

/// The error of a refund: with an unknown outcome, how to check and
/// repeat it with the same id.
fn incerta(err: InterError, e2e: &str, id: &IdDevolucao) -> CliError {
    if resultado_incerto(&err) {
        CliError::DevolucaoIncerta {
            source: err,
            e2e: e2e.to_owned(),
            id: id.to_string(),
        }
    } else {
        err.into()
    }
}

async fn consultar(
    context: &Context,
    args: &PixDevolucaoConsultarArgs,
    intervalo: Duration,
) -> Result<(), CliError> {
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    if !args.espera.aguardar {
        let devolucao = client
            .pix()
            .consultar_devolucao(&args.e2e, &args.id)
            .await?;
        return mostrar(context, &args.e2e, &args.id, &devolucao);
    }
    let (devolucao, terminou) =
        aguardar(&client, &args.e2e, &args.id, args.espera.timeout, intervalo).await?;
    mostrar(context, &args.e2e, &args.id, &devolucao)?;
    desfecho(&devolucao, terminou, args.espera.timeout)
}

/// Queries every `intervalo` until the refund ends or `timeout` passes; the
/// last query happens at the deadline. Whether it ended comes along.
async fn aguardar(
    client: &InterClient,
    e2e: &str,
    id: &IdDevolucao,
    timeout: Duration,
    intervalo: Duration,
) -> Result<(Devolucao, bool), CliError> {
    let prazo = Instant::now() + timeout;
    let mut anterior: Option<StatusDevolucao> = None;
    loop {
        let devolucao = client.pix().consultar_devolucao(e2e, id).await?;
        if devolucao
            .status
            .as_ref()
            .is_some_and(StatusDevolucao::is_final)
        {
            return Ok((devolucao, true));
        }
        let agora = Instant::now();
        if agora >= prazo {
            return Ok((devolucao, false));
        }
        if devolucao.status != anterior {
            output::eprint_linha(&format!("aguardando: {}", status(&devolucao)));
            anterior.clone_from(&devolucao.status);
        }
        tokio::time::sleep(intervalo.min(prazo - agora)).await;
    }
}

/// How waiting ended: made, refused, or the time was up. Without waiting
/// (`terminou`), only a refused refund is an error.
fn desfecho(devolucao: &Devolucao, terminou: bool, timeout: Duration) -> Result<(), CliError> {
    match &devolucao.status {
        Some(StatusDevolucao::NaoRealizado) => Err(CliError::DevolucaoNaoRealizada {
            motivo: devolucao
                .motivo
                .clone()
                .unwrap_or_else(|| "a API não informou o motivo".to_owned()),
        }),
        _ if terminou => Ok(()),
        _ => Err(CliError::TempoEsgotado {
            oque: "a devolução",
            status: status(devolucao).to_owned(),
            segundos: timeout.as_secs(),
        }),
    }
}

fn status(devolucao: &Devolucao) -> &str {
    devolucao
        .status
        .as_ref()
        .map_or("sem status", descrever_status_devolucao)
}

fn mostrar(
    context: &Context,
    e2e: &str,
    id: &IdDevolucao,
    devolucao: &Devolucao,
) -> Result<(), CliError> {
    match context.formato() {
        Formato::Json => output::print_json(devolucao),
        // `commands::run` refuses csv for these commands.
        Formato::Texto | Formato::Csv => output::print(&render(devolucao, e2e, id)),
    }
}

/// A refund, with the times in the local time zone.
fn render(devolucao: &Devolucao, e2e: &str, id: &IdDevolucao) -> String {
    render_em(devolucao, e2e, id, &Local)
}

fn render_em<Tz: TimeZone>(devolucao: &Devolucao, e2e: &str, id: &IdDevolucao, fuso: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let mut linhas = vec![("Status", status(devolucao).to_owned())];
    if let Some(valor) = devolucao.valor {
        linhas.push(("Valor", output::brl(valor)));
    }
    linhas.push(("Pix", e2e.to_owned()));
    if let Some(horario) = &devolucao.horario {
        if let Some(solicitacao) = &horario.solicitacao {
            linhas.push(("Solicitada em", horario_em(solicitacao, fuso)));
        }
        if let Some(liquidacao) = &horario.liquidacao {
            linhas.push(("Liquidada em", horario_em(liquidacao, fuso)));
        }
    }
    if let Some(motivo) = &devolucao.motivo {
        linhas.push(("Motivo", motivo.clone()));
    }
    if let Some(rtr_id) = &devolucao.rtr_id {
        linhas.push(("rtrId", rtr_id.clone()));
    }
    let id = devolucao.id.as_deref().unwrap_or(id.as_str());
    secao(&format!("Devolução {id}"), &linhas)
}

#[cfg(test)]
mod tests {
    use chrono::FixedOffset;
    use serde_json::{Value, json};
    use wiremock::matchers::{any, method, path};
    use wiremock::{Mock, ResponseTemplate};

    use super::*;
    use crate::cli::{Command, PixCommand};
    use crate::commands::testes;
    use crate::confirmacao::testes::TerminalFalso;

    const E2E: &str = "E00416968202609181241abcdEFGH123";
    const ID: &str = "D7978c0c97ea847e78e8849634473c1f1";

    fn pix(devolucoes: &Value) -> PixRecebido {
        serde_json::from_value(json!({
            "endToEndId": E2E,
            "valor": "300.00",
            "horario": "2026-09-18T12:41:07.000Z",
            "devolucoes": devolucoes
        }))
        .unwrap()
    }

    fn devolucao(status: &str) -> Devolucao {
        serde_json::from_value(json!({
            "id": ID,
            "rtrId": "D00416968202609181300abcdefghijk",
            "valor": "100.00",
            "horario": {"solicitacao": "2026-09-18T15:00:00Z", "liquidacao": "2026-09-18T15:00:05Z"},
            "status": status
        }))
        .unwrap()
    }

    #[test]
    fn refunds_fit_in_what_remains() {
        let recebido = pix(&json!([
            {"id": "D1", "valor": "50.00", "status": "DEVOLVIDO"},
            {"id": "D2", "valor": "20.00", "status": "EM_PROCESSAMENTO"},
            {"id": "D3", "valor": "300.00", "status": "NAO_REALIZADO"}
        ]));
        assert!(cabe_no_pix(Decimal::new(230, 0), &recebido).is_ok());
        assert_eq!(
            cabe_no_pix(Decimal::new(23001, 2), &recebido)
                .unwrap_err()
                .to_string(),
            "a devolução de R$ 230,01 passa do que resta do Pix: R$ 230,00 de R$ 300,00, R$ 70,00 já devolvidos ou em devolução"
        );
        assert_eq!(tudo(&recebido).unwrap(), Decimal::new(230, 0));
        let inteiro = pix(&json!([{"id": "D1", "valor": "300.00", "status": "DEVOLVIDO"}]));
        assert!(
            tudo(&inteiro)
                .unwrap_err()
                .to_string()
                .starts_with("nada a devolver")
        );
        // Without the amount, the API checks.
        let mut sem_valor = pix(&json!([]));
        sem_valor.valor = None;
        assert!(cabe_no_pix(Decimal::new(1000, 0), &sem_valor).is_ok());
        assert!(tudo(&sem_valor).is_err());
    }

    #[test]
    fn the_summary_shows_the_pix_and_the_refund() {
        let mut solicitada = DevolucaoSolicitada::new(Decimal::new(100, 0));
        solicitada.natureza = Some(inter_pj::pix::NaturezaDevolucao::Original);
        solicitada.descricao = Some("Pedido cancelado".to_owned());
        let recebido = pix(&json!([{"id": "D1", "valor": "50.00", "status": "DEVOLVIDO"}]));
        let texto = resumo(
            E2E,
            Some(&recebido),
            &solicitada,
            &ID.parse().unwrap(),
            Some(Environment::Production),
        );
        assert!(
            texto.starts_with(
                "*** PRODUÇÃO: esta devolução tira dinheiro da conta real ***\nDevolução a solicitar\n  Ambiente      PRODUÇÃO (conta real)\n"
            ),
            "{texto}"
        );
        for linha in [
            &format!("  Pix           {E2E}"),
            "  Valor do Pix  R$ 300,00",
            "  Já devolvido  R$ 50,00",
            "  Devolução     R$ 100,00 (cem reais)",
            "  Natureza      original",
            "  Descrição     Pedido cancelado",
            &format!("  id            {ID}"),
        ] {
            assert!(texto.contains(linha), "{linha}\n{texto}");
        }
    }

    #[test]
    fn a_refund_in_detail() {
        let brasilia = FixedOffset::west_opt(3 * 3600).unwrap();
        assert_eq!(
            render_em(
                &devolucao("DEVOLVIDO"),
                E2E,
                &ID.parse().unwrap(),
                &brasilia
            ),
            format!(
                "\
Devolução {ID}
  Status         devolvida
  Valor          R$ 100,00
  Pix            {E2E}
  Solicitada em  18/09/2026 12:00:00
  Liquidada em   18/09/2026 12:00:05
  rtrId          D00416968202609181300abcdefghijk"
            )
        );
    }

    #[test]
    fn how_waiting_ends() {
        let timeout = Duration::from_secs(60);
        assert!(desfecho(&devolucao("DEVOLVIDO"), true, timeout).is_ok());
        let mut recusada = devolucao("NAO_REALIZADO");
        recusada.motivo = Some("Conta do pagador encerrada".to_owned());
        let err = desfecho(&recusada, true, timeout).unwrap_err();
        assert_eq!(
            err.to_string(),
            "a devolução não foi feita: Conta do pagador encerrada"
        );
        assert_eq!(err.exit_code(), 5);
        let err = desfecho(&devolucao("EM_PROCESSAMENTO"), false, timeout).unwrap_err();
        assert_eq!(err.exit_code(), 8);
        assert!(err.to_string().contains("em processamento"), "{err}");
        // Without waiting, a refund in processing is no error.
        assert!(desfecho(&devolucao("EM_PROCESSAMENTO"), true, timeout).is_ok());
    }

    // --- the commands against a mock API ------------------------------------

    async fn cenario(args: &[&str]) -> (testes::Cenario, PixDevolucaoCommand) {
        let mut todos = vec!["pix", "devolucao"];
        todos.extend_from_slice(args);
        match testes::cenario(&todos, "pix.write pix.read").await {
            (cenario, Command::Pix(PixCommand::Devolucao(comando))) => (cenario, comando),
            (_, outro) => panic!("{outro:?}"),
        }
    }

    async fn mock_pix(cenario: &testes::Cenario, devolucoes: Value) {
        Mock::given(method("GET"))
            .and(path(format!("/pix/v2/pix/{E2E}")))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::to_value(pix(&devolucoes)).unwrap()),
            )
            .expect(1)
            .mount(&cenario.server)
            .await;
    }

    async fn nada_mais(cenario: &testes::Cenario) {
        Mock::given(any())
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&cenario.server)
            .await;
    }

    fn solicitar_args(comando: PixDevolucaoCommand) -> PixDevolucaoSolicitarArgs {
        match comando {
            PixDevolucaoCommand::Solicitar(args) => *args,
            PixDevolucaoCommand::Consultar(_) => unreachable!(),
        }
    }

    #[tokio::test]
    async fn a_declined_refund_only_looks_the_pix_up() {
        let (cenario, comando) = cenario(&["solicitar", E2E, "--valor", "100"]).await;
        mock_pix(&cenario, json!([])).await;
        nada_mais(&cenario).await;
        let mut terminal = TerminalFalso::respondendo("n\n");
        let err = solicitar(
            &cenario.context,
            &solicitar_args(comando),
            &mut terminal,
            Duration::ZERO,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, CliError::Cancelado), "{err}");
        assert_eq!(terminal.perguntas, ["Devolver o Pix? [s/N] "]);
    }

    #[tokio::test]
    async fn the_same_id_again_sends_nothing() {
        let (cenario, comando) = cenario(&["solicitar", E2E, "--valor", "100", "--id", ID]).await;
        mock_pix(
            &cenario,
            json!([{"id": ID, "valor": "100.00", "status": "DEVOLVIDO"}]),
        )
        .await;
        nada_mais(&cenario).await;
        let mut terminal = TerminalFalso::respondendo("s\n");
        solicitar(
            &cenario.context,
            &solicitar_args(comando),
            &mut terminal,
            Duration::ZERO,
        )
        .await
        .unwrap();
        assert!(terminal.perguntas.is_empty(), "{:?}", terminal.perguntas);

        // A refund refused before is still refused: nothing is sent.
        let (cenario, comando) =
            self::cenario(&["solicitar", E2E, "--valor", "100", "--id", ID]).await;
        mock_pix(
            &cenario,
            json!([{"id": ID, "valor": "100.00", "status": "NAO_REALIZADO", "motivo": "Conta encerrada"}]),
        )
        .await;
        nada_mais(&cenario).await;
        let err = solicitar(
            &cenario.context,
            &solicitar_args(comando),
            &mut TerminalFalso::respondendo("s\n"),
            Duration::ZERO,
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, CliError::DevolucaoNaoRealizada { .. }),
            "{err}"
        );
    }

    #[tokio::test]
    async fn waits_until_the_refund_ends() {
        let (cenario, comando) = cenario(&[
            "solicitar",
            E2E,
            "--tudo",
            "--id",
            ID,
            "--aguardar",
            "--sim",
        ])
        .await;
        mock_pix(
            &cenario,
            json!([{"id": "D1", "valor": "200.00", "status": "DEVOLVIDO"}]),
        )
        .await;
        Mock::given(method("PUT"))
            .and(path(format!("/pix/v2/pix/{E2E}/devolucao/{ID}")))
            .and(wiremock::matchers::body_json(json!({"valor": "100.00"})))
            .respond_with(
                ResponseTemplate::new(201)
                    .set_body_json(serde_json::to_value(devolucao("EM_PROCESSAMENTO")).unwrap()),
            )
            .expect(1)
            .mount(&cenario.server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/pix/v2/pix/{E2E}/devolucao/{ID}")))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::to_value(devolucao("EM_PROCESSAMENTO")).unwrap()),
            )
            .up_to_n_times(2)
            .expect(2)
            .mount(&cenario.server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/pix/v2/pix/{E2E}/devolucao/{ID}")))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::to_value(devolucao("NAO_REALIZADO")).unwrap()),
            )
            .expect(1)
            .mount(&cenario.server)
            .await;
        let err = solicitar(
            &cenario.context,
            &solicitar_args(comando),
            &mut TerminalFalso::respondendo(""),
            Duration::ZERO,
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, CliError::DevolucaoNaoRealizada { .. }),
            "{err}"
        );
    }
}
