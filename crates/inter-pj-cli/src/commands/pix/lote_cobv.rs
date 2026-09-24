//! `inter-pj pix lote-cobv criar|revisar|consultar|listar|sumario|situacao|
//! modelo`: batches of charges with a due date, created or changed from a
//! file and processed afterwards.

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use chrono::{Local, NaiveDate, TimeZone};
use inter_pj::pix::{
    CobvDoLote, CobvNoLote, CobvRevisadaDoLote, LoteCobv, LoteCobvRevisado, LoteCobvSolicitado,
    StatusCobvLote, SumarioLoteCobv,
};
use inter_pj::{Environment, Error as InterError, InterClient, Problem, endpoint};
use rust_decimal::Decimal;
use serde_json::json;

use super::{paginacao, periodo};
use crate::arquivo;
use crate::cli::{
    Formato, PixLoteCobvArquivoArgs, PixLoteCobvCommand, PixLoteCobvConsultarArgs,
    PixLoteCobvIdArgs, PixLoteCobvListarArgs, PixLoteCobvSituacaoArgs, TipoArquivo,
};
use crate::commands::{Context, hoje, simulacao};
use crate::config::Settings;
use crate::confirmacao::{Stdio, Terminal, confirmar, descrever_ambiente};
use crate::cores::Tom;
use crate::error::{CliError, resultado_incerto};
use crate::output::{self, horario_em, horario_local, limpo, secao};
use crate::tabela::{Celula, Coluna, Tabela};

/// Between two queries with `--aguardar`, within the rate limit.
const INTERVALO: Duration = Duration::from_secs(6);

/// Charges listed in the summary of a batch; the others are counted.
const MAX_NO_RESUMO: usize = 10;

/// Longest payer name and description shown in the tables.
const LARGURA_TEXTO: usize = 30;

pub(super) async fn run(context: &Context, command: PixLoteCobvCommand) -> Result<(), CliError> {
    match command {
        PixLoteCobvCommand::Criar(args) => criar(context, &args, &mut Stdio).await,
        PixLoteCobvCommand::Revisar(args) => revisar(context, &args, &mut Stdio).await,
        PixLoteCobvCommand::Consultar(args) => consultar(context, &args, INTERVALO).await,
        PixLoteCobvCommand::Listar(args) => listar(context, &args).await,
        PixLoteCobvCommand::Sumario(args) => sumario(context, &args).await,
        PixLoteCobvCommand::Situacao(args) => situacao(context, &args).await,
        PixLoteCobvCommand::Modelo(args) => output::print_raw(&arquivo::modelo_lote_cobv(
            hoje(),
            args.tipo == TipoArquivo::Csv,
        )),
    }
}

async fn criar(
    context: &Context,
    args: &PixLoteCobvArquivoArgs,
    terminal: &mut dyn Terminal,
) -> Result<(), CliError> {
    let arquivo = arquivo::ler_lote_cobv(&args.arquivo, hoje())?;
    let descricao = args
        .descricao
        .clone()
        .or(arquivo.descricao)
        .ok_or_else(|| {
            CliError::Usage(
                "informe a descrição do lote com --descricao: o arquivo não a tem".to_owned(),
            )
        })?;
    let cobsv: Vec<CobvDoLote> = arquivo.cobsv.into_iter().map(|(_, item)| item).collect();
    let lote = LoteCobvSolicitado::new(descricao, cobsv);
    lote.validar().map_err(|err| erro_do_lote(&err))?;
    let settings = context.settings()?;
    // Configuration problems show up before the confirmation, not after it.
    let client = if args.simular {
        None
    } else {
        Some(context.client(&settings)?)
    };
    let ambiente = settings.ambiente.as_ref().map(|setting| setting.value);
    output::eprint(&resumo_criacao(args.id, &lote, ambiente));
    let id = args.id.to_string();
    let Some(client) = client else {
        return simulacao::mostrar_em(
            context,
            &settings,
            endpoint::pix::CRIAR_LOTE_COBV,
            &[("id", id.as_str())],
            &[],
            &lote,
        );
    };
    let quantas = quantas(lote.cobsv.len());
    confirmar(terminal, args.sim, &format!("Criar o lote de {quantas}?"))?;
    client
        .pix()
        .criar_lote_cobv(args.id, &lote)
        .await
        .map_err(|err| incerto(err, args.id))?;
    recebido(
        context,
        args.id,
        &format!("as {quantas} são criadas em instantes"),
    )
}

async fn revisar(
    context: &Context,
    args: &PixLoteCobvArquivoArgs,
    terminal: &mut dyn Terminal,
) -> Result<(), CliError> {
    let arquivo = arquivo::ler_revisao_lote_cobv(&args.arquivo, hoje())?;
    let cobsv: Vec<CobvRevisadaDoLote> = arquivo.cobsv.into_iter().map(|(_, item)| item).collect();
    let mut revisao = LoteCobvRevisado::new(cobsv);
    revisao.descricao = args.descricao.clone().or(arquivo.descricao);
    revisao.validar().map_err(|err| erro_do_lote(&err))?;
    let settings = context.settings()?;
    let client = if args.simular {
        None
    } else {
        Some(context.client(&settings)?)
    };
    let ambiente = settings.ambiente.as_ref().map(|setting| setting.value);
    output::eprint(&resumo_revisao(args.id, &revisao, ambiente));
    let id = args.id.to_string();
    let Some(client) = client else {
        return simulacao::mostrar_em(
            context,
            &settings,
            endpoint::pix::REVISAR_LOTE_COBV,
            &[("id", id.as_str())],
            &[],
            &revisao,
        );
    };
    let quantas = quantas(revisao.cobsv.len());
    confirmar(terminal, args.sim, &format!("Alterar {quantas} do lote?"))?;
    client
        .pix()
        .revisar_lote_cobv(args.id, &revisao)
        .await
        .map_err(|err| incerto(err, args.id))?;
    recebido(
        context,
        args.id,
        &format!("as alterações de {quantas} são feitas em instantes"),
    )
}

/// `1 cobrança`, `25 cobranças`.
fn quantas(quantas: usize) -> String {
    match quantas {
        1 => "1 cobrança".to_owned(),
        n => format!("{n} cobranças"),
    }
}

/// An error of the library's checks of a batch: the items were checked
/// with the file, so it is about the description.
fn erro_do_lote(err: &inter_pj::pix::CobrancaPixError) -> CliError {
    match err.campo() {
        "descricao" => CliError::Usage(format!("--descricao: {err}")),
        _ => CliError::Usage(err.to_string()),
    }
}

/// The error of a request: with an unknown outcome, how to check it.
fn incerto(err: InterError, id: u64) -> CliError {
    if resultado_incerto(&err) {
        CliError::LoteCobvIncerto { source: err, id }
    } else {
        err.into()
    }
}

/// The request was accepted: the batch is processed afterwards.
fn recebido(context: &Context, id: u64, depois: &str) -> Result<(), CliError> {
    match context.formato() {
        Formato::Json => output::print_json(&json!({ "id": id })),
        // `commands::run` refuses csv for these commands.
        Formato::Texto | Formato::Csv => output::print(&format!(
            "Lote {id} recebido: {depois}.\n\nAcompanhe com: inter-pj pix lote-cobv consultar {id} --aguardar"
        )),
    }
}

/// The batch about to be created: totals and the first charges.
fn resumo_criacao(id: u64, lote: &LoteCobvSolicitado, ambiente: Option<Environment>) -> String {
    let total: Decimal = lote.cobsv.iter().map(|item| item.cobv.valor.original).sum();
    let vencimentos: Vec<NaiveDate> = lote
        .cobsv
        .iter()
        .map(|item| item.cobv.calendario.data_de_vencimento)
        .collect();
    let mut linhas = vec![
        ("Ambiente", descrever_ambiente(ambiente)),
        ("Lote", id.to_string()),
        ("Descrição", lote.descricao.clone()),
        ("Cobranças", lote.cobsv.len().to_string()),
        ("Valor total", output::brl(total)),
    ];
    if let (Some(primeiro), Some(ultimo)) = (vencimentos.iter().min(), vencimentos.iter().max()) {
        let formato = "%d/%m/%Y";
        let texto = if primeiro == ultimo {
            primeiro.format(formato).to_string()
        } else {
            format!(
                "de {} a {}",
                primeiro.format(formato),
                ultimo.format(formato)
            )
        };
        linhas.push(("Vencimentos", texto));
    }
    let mut tabela = Tabela::new(vec![
        Coluna::texto("txid", ""),
        Coluna::texto("Vencimento", ""),
        Coluna::valor("Valor", ""),
        Coluna::texto("Devedor", "").no_maximo(LARGURA_TEXTO),
    ]);
    for item in lote.cobsv.iter().take(MAX_NO_RESUMO) {
        let vencimento = item
            .cobv
            .calendario
            .data_de_vencimento
            .format("%d/%m/%Y")
            .to_string();
        tabela.linha(vec![
            Celula::texto(Some(item.txid.as_str())),
            Celula::texto(Some(&vencimento)),
            Celula::dinheiro(Some(item.cobv.valor.original)),
            Celula::texto(Some(&item.cobv.devedor.nome)),
        ]);
    }
    let mut texto = String::new();
    if ambiente.is_some_and(Environment::is_production) {
        texto.push_str("*** PRODUÇÃO: as cobranças valem de verdade ***\n");
    }
    let _ = write!(
        texto,
        "{}\n\n{}",
        secao("Lote de cobranças com vencimento a criar", &linhas),
        tabela.texto()
    );
    if lote.cobsv.len() > MAX_NO_RESUMO {
        let _ = write!(
            texto,
            "\n... e mais {}",
            quantas(lote.cobsv.len() - MAX_NO_RESUMO)
        );
    }
    texto
}

/// The changes about to be sent: what changes in each charge.
fn resumo_revisao(id: u64, revisao: &LoteCobvRevisado, ambiente: Option<Environment>) -> String {
    let removidas = revisao
        .cobsv
        .iter()
        .filter(|item| item.revisao.remover)
        .count();
    let mut linhas = vec![
        ("Ambiente", descrever_ambiente(ambiente)),
        ("Lote", id.to_string()),
    ];
    if let Some(descricao) = &revisao.descricao {
        linhas.push(("Descrição", format!("→ {descricao}")));
    }
    linhas.push(("Cobranças", revisao.cobsv.len().to_string()));
    if removidas > 0 {
        linhas.push(("Removidas", removidas.to_string()));
    }
    let mut tabela = Tabela::new(vec![
        Coluna::texto("txid", ""),
        Coluna::texto("O que muda", ""),
    ]);
    for item in revisao.cobsv.iter().take(MAX_NO_RESUMO) {
        tabela.linha(vec![
            Celula::texto(Some(item.txid.as_str())),
            Celula::texto(Some(&o_que_muda(item))),
        ]);
    }
    let mut texto = String::new();
    if ambiente.is_some_and(Environment::is_production) {
        texto.push_str("*** PRODUÇÃO: as cobranças valem de verdade ***\n");
    }
    let _ = write!(
        texto,
        "{}\n\n{}",
        secao("Lote de cobranças com vencimento a alterar", &linhas),
        tabela.texto()
    );
    if revisao.cobsv.len() > MAX_NO_RESUMO {
        let _ = write!(
            texto,
            "\n... e mais {}",
            quantas(revisao.cobsv.len() - MAX_NO_RESUMO)
        );
    }
    texto
}

/// `valor R$ 160,00, vencimento 30/10/2026`, or `remover`.
fn o_que_muda(item: &CobvRevisadaDoLote) -> String {
    let revisao = &item.revisao;
    if revisao.remover {
        return "remover (deixa de poder ser paga)".to_owned();
    }
    let mut partes = Vec::new();
    if let Some(valor) = &revisao.valor {
        match valor.original {
            Some(original) => partes.push(format!("valor {}", output::brl(original))),
            None => partes.push("encargos".to_owned()),
        }
    }
    if let Some(calendario) = &revisao.calendario {
        partes.push(format!(
            "vencimento {}",
            calendario.data_de_vencimento.format("%d/%m/%Y")
        ));
    }
    for (muda, nome) in [
        (revisao.devedor.is_some(), "devedor"),
        (revisao.chave.is_some(), "chave"),
        (revisao.solicitacao_pagador.is_some(), "solicitação"),
        (revisao.info_adicionais.is_some(), "informações"),
        (revisao.loc.is_some(), "location"),
    ] {
        if muda {
            partes.push(nome.to_owned());
        }
    }
    partes.join(", ")
}

async fn consultar(
    context: &Context,
    args: &PixLoteCobvConsultarArgs,
    intervalo: Duration,
) -> Result<(), CliError> {
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    if !args.aguardar {
        let lote = client.pix().consultar_lote_cobv(args.id).await?;
        return mostrar(context, &settings, &lote, args.id);
    }
    let (lote, terminou) = aguardar(&client, args.id, args.timeout, intervalo).await?;
    mostrar(context, &settings, &lote, args.id)?;
    desfecho(&lote, terminou, args.timeout)
}

/// Whether no charge of the batch is still being processed.
fn processado(lote: &LoteCobv) -> bool {
    !lote.cobsv.is_empty()
        && lote.cobsv.iter().all(|cobv| {
            matches!(
                cobv.status,
                Some(StatusCobvLote::Criada | StatusCobvLote::Negada)
            )
        })
}

/// Queries every `intervalo` until the batch is processed or `timeout`
/// passes; the last query happens at the deadline.
async fn aguardar(
    client: &InterClient,
    id: u64,
    timeout: Duration,
    intervalo: Duration,
) -> Result<(LoteCobv, bool), CliError> {
    let prazo = Instant::now() + timeout;
    let mut anterior = None;
    loop {
        let lote = client.pix().consultar_lote_cobv(id).await?;
        if processado(&lote) {
            return Ok((lote, true));
        }
        let agora = Instant::now();
        if agora >= prazo {
            return Ok((lote, false));
        }
        let situacao = contagem(&lote);
        if anterior.as_ref() != Some(&situacao) {
            output::eprint_linha(&format!("aguardando: {situacao}"));
            anterior = Some(situacao);
        }
        tokio::time::sleep(intervalo.min(prazo - agora)).await;
    }
}

/// How waiting ended: every charge created, some refused, or the time was
/// up.
fn desfecho(lote: &LoteCobv, terminou: bool, timeout: Duration) -> Result<(), CliError> {
    let negadas = lote
        .cobsv
        .iter()
        .filter(|cobv| cobv.status == Some(StatusCobvLote::Negada))
        .count();
    match (terminou, negadas) {
        (true, 0) => Ok(()),
        (true, negadas) => Err(CliError::LoteComErro {
            detalhe: format!("{negadas} de {} cobranças negadas", lote.cobsv.len()),
        }),
        (false, _) => Err(CliError::TempoEsgotado {
            oque: "o lote",
            status: format!("com {}", contagem(lote)),
            segundos: timeout.as_secs(),
        }),
    }
}

/// `23 criadas · 1 negada · 1 em processamento`.
fn contagem(lote: &LoteCobv) -> String {
    let conta = |status: StatusCobvLote| {
        lote.cobsv
            .iter()
            .filter(|cobv| cobv.status.as_ref() == Some(&status))
            .count()
    };
    let criadas = conta(StatusCobvLote::Criada);
    let negadas = conta(StatusCobvLote::Negada);
    let em_processamento = lote.cobsv.len() - criadas - negadas;
    let mut partes = Vec::new();
    let plural = |n: usize, um: &str, varios: &str| {
        if n == 1 {
            format!("1 {um}")
        } else {
            format!("{n} {varios}")
        }
    };
    if criadas > 0 {
        partes.push(plural(criadas, "criada", "criadas"));
    }
    if negadas > 0 {
        partes.push(plural(negadas, "negada", "negadas"));
    }
    if em_processamento > 0 || partes.is_empty() {
        partes.push(format!("{em_processamento} em processamento"));
    }
    partes.join(" · ")
}

fn mostrar(
    context: &Context,
    settings: &Settings,
    lote: &LoteCobv,
    id: u64,
) -> Result<(), CliError> {
    match context.formato() {
        Formato::Json => output::print_json(lote),
        // `commands::run` refuses csv for these commands.
        Formato::Texto | Formato::Csv => {
            context.warn_if_sandbox(settings);
            output::print(&render_lote(lote, id))
        }
    }
}

/// A situation in words: `EM_PROCESSAMENTO` -> `em processamento`.
fn descrever_situacao(status: &StatusCobvLote) -> &str {
    match status {
        StatusCobvLote::EmProcessamento => "em processamento",
        StatusCobvLote::Criada => "criada",
        StatusCobvLote::Negada => "negada",
        outra => outra.as_str(),
    }
}

fn celula_situacao(status: Option<&StatusCobvLote>) -> Celula {
    let tom = status.and_then(|status| match status {
        StatusCobvLote::EmProcessamento => Some(Tom::Pendente),
        StatusCobvLote::Criada => Some(Tom::Positivo),
        StatusCobvLote::Negada => Some(Tom::Negativo),
        _ => None,
    });
    Celula::situacao(status.map(descrever_situacao), tom)
}

/// `Cobrança inválida. cobv.valor.desconto.data: não respeita o schema`.
fn problema(problema: &Problem) -> String {
    let mut texto = problema
        .title
        .clone()
        .or_else(|| problema.detail.clone())
        .unwrap_or_default();
    for violacao in &problema.violacoes {
        let razao = violacao.razao.as_deref().unwrap_or_default();
        match violacao.propriedade.as_deref() {
            Some(propriedade) => {
                let _ = write!(texto, " {propriedade}: {razao}");
            }
            None => {
                let _ = write!(texto, " {razao}");
            }
        }
    }
    texto.trim().to_owned()
}

/// A batch and where each charge stands, with the times in the local time
/// zone.
fn render_lote(lote: &LoteCobv, id: u64) -> String {
    render_lote_em(lote, id, &Local)
}

fn render_lote_em<Tz: TimeZone>(lote: &LoteCobv, id: u64, fuso: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let id = lote.id.unwrap_or(id);
    let titulo = match &lote.descricao {
        Some(descricao) => format!("Lote {id}: {}", limpo(descricao)),
        None => format!("Lote {id}"),
    };
    let mut linhas = Vec::new();
    if let Some(criacao) = &lote.criacao {
        linhas.push(("Criado em", horario_em(criacao, fuso)));
    }
    if let Some(status) = &lote.status {
        linhas.push(("Situação", descrever_situacao(status).to_owned()));
    }
    linhas.push((
        "Cobranças",
        format!("{} · {}", lote.cobsv.len(), contagem(lote)),
    ));
    let mut texto = secao(&titulo, &linhas);
    if lote.cobsv.is_empty() {
        return texto;
    }
    let mut tabela = Tabela::new(vec![
        Coluna::texto("txid", ""),
        Coluna::texto("Situação", ""),
        Coluna::texto("Criada em", ""),
    ]);
    for cobv in &lote.cobsv {
        tabela.linha(vec![
            Celula::texto(cobv.txid.as_deref()),
            celula_situacao(cobv.status.as_ref()),
            Celula::texto(
                cobv.criacao
                    .as_deref()
                    .map(|criacao| horario_em(criacao, fuso))
                    .as_deref(),
            ),
        ]);
    }
    let _ = write!(texto, "\n\n{}", tabela.texto_colorido());
    let negadas: Vec<&CobvNoLote> = lote
        .cobsv
        .iter()
        .filter(|cobv| cobv.problema.is_some())
        .collect();
    if !negadas.is_empty() {
        let linhas: Vec<(&str, String)> = negadas
            .iter()
            .map(|cobv| {
                (
                    cobv.txid.as_deref().unwrap_or("?"),
                    cobv.problema.as_ref().map(problema).unwrap_or_default(),
                )
            })
            .collect();
        let _ = write!(texto, "\n\n{}", secao("Problemas", &linhas));
    }
    texto
}

async fn listar(context: &Context, args: &PixLoteCobvListarArgs) -> Result<(), CliError> {
    let periodo = periodo(args.periodo)?;
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let (lotes, pagina) = match args.pagina {
        Some(numero) => {
            let mut pagina = client
                .pix()
                .listar_lotes_cobv(&periodo, numero, args.itens_por_pagina)
                .await?;
            if context.formato() == Formato::Json {
                return output::print_json(&pagina);
            }
            let lotes = std::mem::take(&mut pagina.lotes);
            (lotes, Some((numero, pagina)))
        }
        None => (client.pix().listar_todos_lotes_cobv(&periodo).await?, None),
    };
    match context.formato() {
        Formato::Json => output::print_json(&json!({ "lotes": lotes })),
        Formato::Csv => output::print_csv(&csv(&lotes), context.separador()),
        Formato::Texto => {
            context.warn_if_sandbox(&settings);
            let formato = "%d/%m/%Y %H:%M";
            let mut texto = format!(
                "Lotes de cobranças com vencimento criados de {} a {}\n\n",
                periodo.inicio.format(formato),
                periodo.fim.format(formato)
            );
            if lotes.is_empty() {
                texto.push_str("Nenhum lote encontrado.");
            } else {
                texto.push_str(&tabela(&lotes).texto_colorido());
                let cobrancas: usize = lotes.iter().map(|lote| lote.cobsv.len()).sum();
                let quantos = match lotes.len() {
                    1 => "1 lote".to_owned(),
                    n => format!("{n} lotes"),
                };
                let _ = write!(texto, "\n\n{quantos} · {}", quantas(cobrancas));
            }
            if let Some((numero, pagina)) = pagina {
                texto.push_str(&paginacao(numero, &pagina.parametros, lotes.len(), "lotes"));
            }
            output::print(&texto)
        }
    }
}

/// Charges of a batch in each situation: created, refused, in processing.
fn situacoes(lote: &LoteCobv) -> (usize, usize, usize) {
    let conta = |status: StatusCobvLote| {
        lote.cobsv
            .iter()
            .filter(|cobv| cobv.status.as_ref() == Some(&status))
            .count()
    };
    let criadas = conta(StatusCobvLote::Criada);
    let negadas = conta(StatusCobvLote::Negada);
    (criadas, negadas, lote.cobsv.len() - criadas - negadas)
}

fn tabela(lotes: &[LoteCobv]) -> Tabela {
    let mut tabela = Tabela::new(vec![
        Coluna::texto("Criado em", ""),
        Coluna::valor("id", ""),
        Coluna::texto("Descrição", "").no_maximo(LARGURA_TEXTO),
        Coluna::valor("Cobranças", ""),
        Coluna::valor("Criadas", ""),
        Coluna::valor("Negadas", ""),
    ]);
    for lote in lotes {
        let (criadas, negadas, _) = situacoes(lote);
        tabela.linha(vec![
            Celula::texto(lote.criacao.as_deref().map(horario_local).as_deref()),
            Celula::texto(lote.id.map(|id| id.to_string()).as_deref()),
            Celula::texto(lote.descricao.as_deref()),
            Celula::texto(Some(&lote.cobsv.len().to_string())),
            Celula::texto(Some(&criadas.to_string())),
            Celula::texto(Some(&negadas.to_string())),
        ]);
    }
    tabela
}

/// The fields of the API, and the charges counted by situation.
fn csv(lotes: &[LoteCobv]) -> Tabela {
    let texto = |campo: &'static str| Coluna::texto(campo, campo);
    let mut tabela = Tabela::new(vec![
        texto("id"),
        texto("descricao"),
        texto("criacao"),
        texto("cobrancas"),
        texto("criadas"),
        texto("negadas"),
        texto("emProcessamento"),
    ]);
    for lote in lotes {
        let (criadas, negadas, em_processamento) = situacoes(lote);
        tabela.linha(vec![
            Celula::texto(lote.id.map(|id| id.to_string()).as_deref()),
            Celula::texto(lote.descricao.as_deref()),
            Celula::texto(lote.criacao.as_deref()),
            Celula::texto(Some(&lote.cobsv.len().to_string())),
            Celula::texto(Some(&criadas.to_string())),
            Celula::texto(Some(&negadas.to_string())),
            Celula::texto(Some(&em_processamento.to_string())),
        ]);
    }
    tabela
}

async fn sumario(context: &Context, args: &PixLoteCobvIdArgs) -> Result<(), CliError> {
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let sumario = client.pix().sumario_lote_cobv(args.id).await?;
    match context.formato() {
        Formato::Json => output::print_json(&sumario),
        // `commands::run` refuses csv for this command.
        Formato::Texto | Formato::Csv => {
            context.warn_if_sandbox(&settings);
            output::print(&render_sumario(&sumario, args.id))
        }
    }
}

fn render_sumario(sumario: &SumarioLoteCobv, id: u64) -> String {
    let mut linhas = Vec::new();
    if let Some(status) = &sumario.status_processamento {
        let status = match status.as_str() {
            "FINALIZADO" => "finalizado",
            "EM_PROCESSAMENTO" => "em processamento",
            outro => outro,
        };
        linhas.push(("Processamento", status.to_owned()));
    }
    if let Some(data) = &sumario.data_criacao_processamento {
        linhas.push(("Iniciado em", horario_local(data)));
    }
    for (rotulo, total) in [
        ("Cobranças", sumario.total_cobrancas),
        ("Criadas", sumario.total_cobrancas_criadas),
        ("Negadas", sumario.total_cobrancas_negadas),
    ] {
        if let Some(total) = total {
            linhas.push((rotulo, total.to_string()));
        }
    }
    let mut texto = secao(&format!("Lote {id}"), &linhas);
    if sumario
        .total_cobrancas_negadas
        .is_some_and(|negadas| negadas > 0)
    {
        let _ = write!(
            texto,
            "\n\nVeja por quê: inter-pj pix lote-cobv situacao {id} negada"
        );
    }
    texto
}

async fn situacao(context: &Context, args: &PixLoteCobvSituacaoArgs) -> Result<(), CliError> {
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let lote = client
        .pix()
        .consultar_lote_cobv_por_situacao(args.id, &args.situacao.into())
        .await?;
    mostrar(context, &settings, &lote, args.id)
}

#[cfg(test)]
mod tests {
    use chrono::FixedOffset;
    use serde_json::Value;

    use super::*;

    #[test]
    fn every_documented_situation_has_a_tone() {
        for status in StatusCobvLote::DOCUMENTADOS {
            assert!(
                matches!(celula_situacao(Some(status)), Celula::Situacao(..)),
                "{status:?}"
            );
        }
        assert_eq!(
            celula_situacao(Some(&StatusCobvLote::Negada)),
            Celula::Situacao("negada".into(), Tom::Negativo)
        );
    }

    fn lote(cobsv: &Value) -> LoteCobv {
        serde_json::from_value(json!({
            "id": 42,
            "descricao": "Mensalidades de outubro",
            "criacao": "2026-09-24T13:10:00.358Z",
            "cobsv": cobsv
        }))
        .unwrap()
    }

    fn negada() -> Value {
        json!({
            "txid": "mensalidade202610cliente0002",
            "status": "NEGADA",
            "problema": {
                "type": "https://pix.bcb.gov.br/api/v2/error/CobVOperacaoInvalida",
                "title": "Cobrança inválida.",
                "status": 400,
                "violacoes": [{"razao": "O campo cobv.valor.original não respeita o schema.", "propriedade": "cobv.valor.original"}]
            }
        })
    }

    #[test]
    fn a_batch_in_detail() {
        let brasilia = FixedOffset::west_opt(3 * 3600).unwrap();
        let lote = lote(&json!([
            {"txid": "mensalidade202610cliente0001", "status": "CRIADA", "criacao": "2026-09-24T13:10:03.000Z"},
            negada()
        ]));
        assert_eq!(
            render_lote_em(&lote, 42, &brasilia),
            "\
Lote 42: Mensalidades de outubro
  Criado em  24/09/2026 10:10:00
  Cobranças  2 · 1 criada · 1 negada

txid                          Situação  Criada em
mensalidade202610cliente0001  criada    24/09/2026 10:10:03
mensalidade202610cliente0002  negada

Problemas
  mensalidade202610cliente0002  Cobrança inválida. cobv.valor.original: O campo cobv.valor.original não respeita o schema."
        );
    }

    #[test]
    fn how_waiting_ends() {
        let timeout = Duration::from_secs(60);
        let criadas = lote(&json!([{"txid": "a", "status": "CRIADA"}]));
        assert!(processado(&criadas));
        assert!(desfecho(&criadas, true, timeout).is_ok());
        let com_negada = lote(&json!([{"txid": "a", "status": "CRIADA"}, negada()]));
        let err = desfecho(&com_negada, true, timeout).unwrap_err();
        assert_eq!(
            err.to_string(),
            "o lote foi processado com erro: 1 de 2 cobranças negadas"
        );
        assert_eq!(err.exit_code(), 5);
        let processando = lote(
            &json!([{"txid": "a", "status": "CRIADA"}, {"txid": "b", "status": "EM_PROCESSAMENTO"}]),
        );
        assert!(!processado(&processando));
        let err = desfecho(&processando, false, timeout).unwrap_err();
        assert_eq!(
            err.to_string(),
            "tempo de espera esgotado (60 s): o lote ainda está com 1 criada · 1 em processamento"
        );
        // A batch just received may have no charges yet.
        assert!(!processado(&lote(&json!([]))));
        assert_eq!(contagem(&lote(&json!([]))), "0 em processamento");
    }

    #[test]
    fn a_summary_points_at_the_refused() {
        let sumario: SumarioLoteCobv = serde_json::from_value(json!({
            "dataCriacaoProcessamento": "2026-09-24T13:10:00Z",
            "statusProcessamento": "FINALIZADO",
            "totalCobrancas": 5,
            "totalCobrancasNegadas": 1,
            "totalCobrancasCriadas": 4
        }))
        .unwrap();
        let texto = render_sumario(&sumario, 42);
        assert!(
            texto.starts_with("Lote 42\n  Processamento  finalizado\n"),
            "{texto}"
        );
        assert!(texto.contains("  Negadas        1"), "{texto}");
        assert!(
            texto.ends_with("Veja por quê: inter-pj pix lote-cobv situacao 42 negada"),
            "{texto}"
        );
    }

    #[test]
    fn listings_count_by_situation() {
        let lotes = [lote(
            &json!([{"txid": "a", "status": "CRIADA"}, negada(), {"txid": "c"}]),
        )];
        let csv = csv(&lotes).csv(crate::tabela::Separador::Virgula);
        assert_eq!(
            csv,
            "id,descricao,criacao,cobrancas,criadas,negadas,emProcessamento\r\n42,Mensalidades de outubro,2026-09-24T13:10:00.358Z,3,1,1,1\r\n"
        );
    }
}
