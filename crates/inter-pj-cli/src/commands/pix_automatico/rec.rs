//! `inter-pj pix-automatico rec criar|modelo|listar|consultar|revisar|cancelar`:
//! the recurrences, the payer's authorization of the recurring charges.

use std::fmt::Write as _;

use chrono::{Local, NaiveDate, TimeZone};
use inter_pj::pix::Devedor;
use inter_pj::pix_automatico::{
    AtivacaoSolicitada, CalendarioRec, CalendarioRecGerado, FiltroRecs, PagadorRec, Periodicidade,
    PoliticaRetentativa, Rec, RecRevisada, RecSolicitada, RecebedorRec, StatusRec, ValorRec,
    ValorRecGerado, VinculoRec,
};
use inter_pj::{Environment, Error as InterError, endpoint};
use serde_json::json;

use super::solicitacao::descrever_status_solicitacao;
use super::{
    descrever_ativacao, descrever_calendario, descrever_politica, descrever_status,
    descrever_valor, encerrada, encerramento,
};
use crate::arquivo;
use crate::cli::{
    Formato, RecCancelarArgs, RecCommand, RecConsultarArgs, RecCriarArgs, RecListarArgs,
    RecRevisarArgs,
};
use crate::commands::pix::{antes_e_depois, documento, pagina, periodo, pessoa};
use crate::commands::qrcode::OpcoesQr;
use crate::commands::{Context, simulacao};
use crate::confirmacao::{Stdio, Terminal, confirmar, descrever_ambiente, pode_confirmar};
use crate::error::{CliError, resultado_incerto};
use crate::output::{self, data_br, horario_em, secao};
use crate::tabela::{Celula, Coluna, Tabela};
use crate::valor::por_extenso;

/// Longest payer name shown in the listing.
const LARGURA_DEVEDOR: usize = 30;

pub(super) async fn run(context: &Context, command: RecCommand) -> Result<(), CliError> {
    match command {
        RecCommand::Criar(args) => criar(context, &args, &mut Stdio).await,
        RecCommand::Modelo(_) => output::print_raw(&arquivo::modelo_rec(hoje())),
        RecCommand::Listar(args) => listar(context, &args).await,
        RecCommand::Consultar(args) => consultar(context, &args).await,
        RecCommand::Revisar(args) => revisar(context, &args, &mut Stdio).await,
        RecCommand::Cancelar(args) => cancelar(context, &args, &mut Stdio).await,
    }
}

fn hoje() -> NaiveDate {
    Local::now().date_naive()
}

async fn criar(
    context: &Context,
    args: &RecCriarArgs,
    terminal: &mut dyn Terminal,
) -> Result<(), CliError> {
    let rec = match &args.arquivo {
        Some(caminho) => arquivo::rec(&arquivo::ler_json(caminho)?, &arquivo::nome(caminho))?,
        None => das_opcoes(args)?,
    };
    inicio_valido(rec.calendario.data_inicial, hoje())?;
    let settings = context.settings()?;
    // Configuration problems show up before the confirmation, not after it.
    let client = if args.simular {
        None
    } else {
        Some(context.client(&settings)?)
    };
    let ambiente = settings.ambiente.as_ref().map(|setting| setting.value);
    eprintln!("{}", resumo(&rec, ambiente));

    let Some(client) = client else {
        return simulacao::mostrar(
            context,
            &settings,
            endpoint::pix_automatico::CRIAR_REC,
            &[],
            &rec,
        );
    };
    confirmar(terminal, args.sim, "Criar a recorrência?")?;
    let criada = client
        .pix_automatico()
        .criar_rec(&rec)
        .await
        .map_err(|err| incerta(err, &rec))?;
    match context.formato() {
        Formato::Json => output::print_json(&criada),
        // `commands::run` refuses csv for this command.
        Formato::Texto | Formato::Csv => output::print(&criada_em_texto(&criada, &rec)),
    }
}

/// The recurrence created, and how the payer approves it.
fn criada_em_texto(criada: &Rec, rec: &RecSolicitada) -> String {
    let id = criada.id_rec.as_deref().unwrap_or_default();
    let mut texto = format!(
        "Recorrência criada: aguarda a aprovação do pagador.\n\n{}\n\nAcompanhe com: inter-pj pix-automatico rec consultar {id}",
        render_rec(criada)
    );
    let _ = write!(
        texto,
        "\nO pagador aprova no banco dele: peça com inter-pj pix-automatico solicitacao criar --rec {id}"
    );
    if rec.loc.is_some() {
        let _ = write!(
            texto,
            ", ou mostre o QR Code de inter-pj pix-automatico rec consultar {id} --qrcode"
        );
    }
    texto
}

/// The recurrence of the options, checked as the API would.
fn das_opcoes(args: &RecCriarArgs) -> Result<RecSolicitada, CliError> {
    // With --devedor-documento, clap requires the name, the contract, the
    // first date and the frequency.
    let (Some(documento), Some(nome), Some(contrato), Some(inicio), Some(periodicidade)) = (
        &args.devedor_documento,
        &args.devedor_nome,
        &args.contrato,
        args.data_inicial,
        args.periodicidade,
    ) else {
        return Err(CliError::Usage(
            "informe --arquivo, ou --devedor-documento, --devedor-nome, --contrato, --data-inicial e --periodicidade"
                .to_owned(),
        ));
    };
    let mut vinculo = VinculoRec::new(
        Devedor::new(documento.clone(), nome.clone()),
        contrato.clone(),
    );
    vinculo.objeto.clone_from(&args.objeto);
    let mut calendario = CalendarioRec::new(inicio, periodicidade.into());
    calendario.data_final = args.data_final;
    let politica = if args.retentativas {
        PoliticaRetentativa::Permite3R7D
    } else {
        PoliticaRetentativa::NaoPermite
    };
    let mut rec = RecSolicitada::new(vinculo, calendario, politica);
    rec.valor = match (args.valor, args.valor_minimo) {
        (Some(fixo), _) => Some(ValorRec::fixo(fixo)),
        (None, Some(minimo)) => Some(ValorRec::minimo(minimo)),
        (None, None) => None,
    };
    rec.loc = args.loc;
    rec.ativacao = args.txid_ativacao.clone().map(AtivacaoSolicitada::new);
    rec.validar()
        .map_err(|err| CliError::Usage(format!("{}: {err}", opcao(err.campo()))))?;
    Ok(rec)
}

/// The option of a field of the API, for the messages.
fn opcao(campo: &str) -> &str {
    match campo {
        "vinculo.objeto" => "--objeto",
        "vinculo.devedor.nome" => "--devedor-nome",
        "vinculo.contrato" => "--contrato",
        "calendario.dataFinal" => "--data-final",
        "valor.valorRec" => "--valor",
        "valor.valorMinimoRecebedor" => "--valor-minimo",
        outro => outro,
    }
}

/// Refuses a first payment in the past.
fn inicio_valido(inicio: NaiveDate, hoje: NaiveDate) -> Result<(), CliError> {
    if inicio < hoje {
        return Err(CliError::Usage(format!(
            "a data do primeiro pagamento ({}) já passou",
            inicio.format("%d/%m/%Y")
        )));
    }
    Ok(())
}

/// The error of a creation: with an unknown outcome, how to check before
/// trying again, as the API has no idempotency key.
fn incerta(err: InterError, rec: &RecSolicitada) -> CliError {
    if resultado_incerto(&err) {
        CliError::CriacaoIncerta {
            source: err,
            situacao: "a recorrência pode ter sido criada",
            consulta: format!(
                "inter-pj pix-automatico rec listar --documento {}",
                rec.vinculo.devedor.documento.as_str()
            ),
        }
    } else {
        err.into()
    }
}

/// The recurrence about to be created.
fn resumo(rec: &RecSolicitada, ambiente: Option<Environment>) -> String {
    let devedor = &rec.vinculo.devedor;
    let mut linhas = vec![
        ("Ambiente", descrever_ambiente(ambiente)),
        (
            "Devedor",
            format!("{} ({})", devedor.nome, devedor.documento.formatado()),
        ),
        ("Contrato", rec.vinculo.contrato.clone()),
    ];
    if let Some(objeto) = &rec.vinculo.objeto {
        linhas.push(("Objeto", objeto.clone()));
    }
    let mut calendario = CalendarioRecGerado::default();
    calendario.data_inicial = Some(rec.calendario.data_inicial.to_string());
    calendario.data_final = rec.calendario.data_final.map(|data| data.to_string());
    calendario.periodicidade = Some(rec.calendario.periodicidade.clone());
    if let Some(periodo) = descrever_calendario(&calendario) {
        linhas.push(("Periodicidade", periodo));
    }
    let valor = if let Some(fixo) = rec.valor.and_then(|valor| valor.valor_rec) {
        let extenso = por_extenso(fixo)
            .map(|extenso| format!(" ({extenso})"))
            .unwrap_or_default();
        format!("{}{extenso} em cada pagamento", output::brl(fixo))
    } else {
        let mut gerado = ValorRecGerado::default();
        gerado.valor_minimo_recebedor = rec.valor.and_then(|valor| valor.valor_minimo_recebedor);
        descrever_valor(Some(&gerado))
    };
    linhas.push(("Valor", valor));
    linhas.push((
        "Retentativas",
        descrever_politica(&rec.politica_retentativa).to_owned(),
    ));
    if let Some(loc) = rec.loc {
        linhas.push(("Location", loc.to_string()));
    }
    if let Some(ativacao) = &rec.ativacao {
        linhas.push((
            "Ativação",
            format!("pelo pagamento da cobrança imediata {}", ativacao.txid),
        ));
    }
    let mut texto = String::new();
    if ambiente.is_some_and(Environment::is_production) {
        texto.push_str("*** PRODUÇÃO: a recorrência vale de verdade ***\n");
    }
    texto.push_str(&secao("Recorrência a criar", &linhas));
    texto
}

async fn consultar(context: &Context, args: &RecConsultarArgs) -> Result<(), CliError> {
    let opcoes = OpcoesQr::new(
        context,
        args.qr.qrcode,
        args.qr.qrcode_png.clone(),
        args.qr.sobrescrever,
    )?;
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let rec = client
        .pix_automatico()
        .consultar_rec(&args.id_rec, args.txid.as_ref())
        .await?;
    opcoes.mostrar(
        context,
        &settings,
        &render_rec(&rec),
        &rec,
        &copia_e_cola(&rec),
    )
}

/// The "copia e cola" of the QR Code of a recurrence that can still be
/// approved, or why there is none.
fn copia_e_cola(rec: &Rec) -> Result<&str, String> {
    if let Some(status) = rec.status.as_ref().filter(|status| encerrada(Some(status))) {
        return Err(format!(
            "a recorrência está {}: o QR Code não serve mais",
            descrever_status(status)
        ));
    }
    rec.dados_qr
        .as_ref()
        .and_then(|qr| qr.pix_copia_e_cola.as_deref())
        .filter(|texto| !texto.trim().is_empty())
        .ok_or_else(|| {
            "a API não trouxe o QR Code desta recorrência: ele precisa de uma location, ou de --txid com uma cobrança"
                .to_owned()
        })
}

/// A recurrence in detail, with the times in the local time zone.
fn render_rec(rec: &Rec) -> String {
    render_rec_em(rec, &Local)
}

fn render_rec_em<Tz: TimeZone>(rec: &Rec, fuso: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let mut linhas = Vec::new();
    if let Some(status) = &rec.status {
        linhas.push(("Status", descrever_status(status).to_owned()));
    }
    if let Some(vinculo) = &rec.vinculo {
        if let Some(devedor) = vinculo.devedor.as_ref().and_then(pessoa) {
            linhas.push(("Devedor", devedor));
        }
        if let Some(contrato) = &vinculo.contrato {
            linhas.push(("Contrato", contrato.clone()));
        }
        if let Some(objeto) = &vinculo.objeto {
            linhas.push(("Objeto", objeto.clone()));
        }
    }
    if let Some(periodo) = rec.calendario.as_ref().and_then(descrever_calendario) {
        linhas.push(("Periodicidade", periodo));
    }
    linhas.push(("Valor", descrever_valor(rec.valor.as_ref())));
    if let Some(politica) = &rec.politica_retentativa {
        linhas.push(("Retentativas", descrever_politica(politica).to_owned()));
    }
    if let Some(recebedor) = rec.recebedor.as_ref().and_then(recebedor) {
        linhas.push(("Recebedor", recebedor));
    }
    if let Some(pagador) = rec.pagador.as_ref().and_then(pagador) {
        linhas.push(("Pagador", pagador));
    }
    if let Some(loc) = &rec.loc {
        let location = loc
            .location
            .clone()
            .or_else(|| loc.id.map(|id| id.to_string()));
        if let Some(location) = location {
            linhas.push(("Location", location));
        }
    }
    if let Some(ativacao) = rec.ativacao.as_ref().and_then(descrever_ativacao) {
        linhas.push(("Ativação", ativacao));
    }
    if let Some(encerramento) = rec.encerramento.as_ref().and_then(encerramento) {
        linhas.push(("Encerramento", encerramento));
    }
    let titulo = format!("Recorrência {}", rec.id_rec.as_deref().unwrap_or_default());
    let mut texto = secao(titulo.trim(), &linhas);
    for secao in [historico(rec, fuso), solicitacoes(rec, fuso)]
        .into_iter()
        .flatten()
    {
        let _ = write!(texto, "\n\n{secao}");
    }
    if let Some(copia_e_cola) = rec
        .dados_qr
        .as_ref()
        .and_then(|qr| qr.pix_copia_e_cola.as_deref())
        .filter(|texto| !texto.is_empty())
    {
        let _ = write!(texto, "\n\nCopia e cola  {copia_e_cola}");
    }
    texto
}

/// The changes of status of a recurrence, with their times in `fuso`.
fn historico<Tz: TimeZone>(rec: &Rec, fuso: &Tz) -> Option<String>
where
    Tz::Offset: std::fmt::Display,
{
    let historico: Vec<(String, String)> = rec
        .atualizacao
        .iter()
        .map(|atualizacao| {
            (
                atualizacao
                    .data
                    .as_deref()
                    .map(|data| horario_em(data, fuso))
                    .unwrap_or_default(),
                atualizacao
                    .status
                    .as_ref()
                    .map(|status| curto(status).to_owned())
                    .unwrap_or_default(),
            )
        })
        .collect();
    let linhas: Vec<(&str, String)> = historico
        .iter()
        .map(|(quando, oque)| (quando.as_str(), oque.clone()))
        .collect();
    (!linhas.is_empty()).then(|| secao("Histórico", &linhas))
}

/// The confirmation requests of a recurrence, with their status.
fn solicitacoes<Tz: TimeZone>(rec: &Rec, fuso: &Tz) -> Option<String>
where
    Tz::Offset: std::fmt::Display,
{
    let linhas: Vec<(&str, String)> = rec
        .solicitacao
        .iter()
        .map(|solicitacao| {
            let mut situacao = solicitacao
                .status
                .as_ref()
                .map(|status| descrever_status_solicitacao(status).to_owned())
                .unwrap_or_default();
            if let Some(expira) = solicitacao
                .calendario
                .as_ref()
                .and_then(|calendario| calendario.data_expiracao_solicitacao.as_deref())
            {
                let _ = write!(situacao, "; expira em {}", horario_em(expira, fuso));
            }
            (
                solicitacao.id_solic_rec.as_deref().unwrap_or_default(),
                situacao,
            )
        })
        .collect();
    (!linhas.is_empty()).then(|| secao("Solicitações de confirmação", &linhas))
}

/// `Empresa Exemplo Ltda (12.345.678/0001-95), convênio X`.
fn recebedor(recebedor: &RecebedorRec) -> Option<String> {
    let mut texto = match (recebedor.nome.as_deref(), recebedor.cnpj.as_deref()) {
        (Some(nome), Some(cnpj)) => format!("{nome} ({})", documento(cnpj)),
        (Some(nome), None) => nome.to_owned(),
        (None, Some(cnpj)) => documento(cnpj),
        (None, None) => return None,
    };
    if let Some(convenio) = &recebedor.convenio {
        let _ = write!(texto, ", convênio {convenio}");
    }
    Some(texto)
}

/// `123.456.789-09 (banco 12345678)`: the payer who approved it.
fn pagador(pagador: &PagadorRec) -> Option<String> {
    let documento = pagador
        .cpf
        .as_deref()
        .or(pagador.cnpj.as_deref())
        .map(documento);
    match (documento, pagador.ispb_participante.as_deref()) {
        (Some(documento), Some(ispb)) => Some(format!("{documento} (ISPB do banco {ispb})")),
        (Some(documento), None) => Some(documento),
        (None, Some(ispb)) => Some(format!("ISPB do banco {ispb}")),
        (None, None) => None,
    }
}

/// A status in one word: `aprovada`.
fn curto(status: &StatusRec) -> &str {
    match status {
        StatusRec::Criada => "criada",
        StatusRec::Aprovada => "aprovada",
        StatusRec::Rejeitada => "rejeitada",
        StatusRec::Expirada => "expirada",
        StatusRec::Cancelada => "cancelada",
        outro => outro.as_str(),
    }
}

async fn listar(context: &Context, args: &RecListarArgs) -> Result<(), CliError> {
    let mut filtro = FiltroRecs::new(periodo(args.periodo)?);
    filtro.devedor.clone_from(&args.documento);
    filtro.status = args.status.map(StatusRec::from);
    filtro.location_presente = match (args.com_location, args.sem_location) {
        (true, _) => Some(true),
        (_, true) => Some(false),
        _ => None,
    };
    filtro.convenio.clone_from(&args.convenio);
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let (recs, pagina_pedida) = match args.pagina {
        Some(numero) => {
            let mut resposta = client
                .pix_automatico()
                .listar_recs(&filtro, numero, args.itens_por_pagina)
                .await?;
            if context.formato() == Formato::Json {
                return output::print_json(&resposta);
            }
            let recs = std::mem::take(&mut resposta.recs);
            let paginacao = resposta
                .parametros
                .and_then(|parametros| parametros.paginacao)
                .unwrap_or_default();
            (recs, Some((numero, paginacao)))
        }
        None => (
            client.pix_automatico().listar_todas_recs(&filtro).await?,
            None,
        ),
    };
    match context.formato() {
        Formato::Json => output::print_json(&json!({ "recs": recs })),
        Formato::Csv => output::print_raw(&csv(&recs).csv(context.separador())),
        Formato::Texto => {
            context.warn_if_sandbox(&settings);
            let mut texto = format!("{}\n\n", titulo(&filtro));
            if recs.is_empty() {
                texto.push_str("Nenhuma recorrência encontrada.");
            } else {
                texto.push_str(&tabela(&recs).texto());
                let _ = write!(texto, "\n\n{}", totais(&recs));
            }
            if let Some((numero, paginacao)) = pagina_pedida {
                texto.push_str(&pagina(numero, paginacao, recs.len(), "recorrências"));
            }
            output::print(&texto)
        }
    }
}

/// `Recorrências criadas de 25/08/2026 00:00 a 24/09/2026 23:59`, and the
/// filters.
fn titulo(filtro: &FiltroRecs) -> String {
    let formato = "%d/%m/%Y %H:%M";
    let mut texto = format!(
        "Recorrências criadas de {} a {}",
        filtro.periodo.inicio.format(formato),
        filtro.periodo.fim.format(formato)
    );
    let mut filtros = Vec::new();
    if let Some(status) = &filtro.status {
        filtros.push(curto(status).to_owned());
    }
    if let Some(documento) = &filtro.devedor {
        filtros.push(format!("devedor {}", documento.formatado()));
    }
    match filtro.location_presente {
        Some(true) => filtros.push("com location".to_owned()),
        Some(false) => filtros.push("sem location".to_owned()),
        None => {}
    }
    if let Some(convenio) = &filtro.convenio {
        filtros.push(format!("convênio {convenio}"));
    }
    if !filtros.is_empty() {
        let _ = write!(texto, " ({})", filtros.join(", "));
    }
    texto
}

fn tabela(recs: &[Rec]) -> Tabela {
    let mut tabela = Tabela::new(vec![
        Coluna::texto("Status", ""),
        Coluna::texto("Devedor", "").no_maximo(LARGURA_DEVEDOR),
        Coluna::texto("Periodicidade", ""),
        Coluna::texto("Início", ""),
        Coluna::valor("Valor", ""),
        Coluna::valor("Mínimo", ""),
        Coluna::texto("idRec", ""),
    ]);
    for rec in recs {
        let calendario = rec.calendario.as_ref();
        tabela.linha(vec![
            Celula::texto(rec.status.as_ref().map(curto)),
            Celula::texto(
                rec.vinculo
                    .as_ref()
                    .and_then(|vinculo| vinculo.devedor.as_ref())
                    .and_then(|devedor| devedor.nome.as_deref()),
            ),
            Celula::texto(
                calendario
                    .and_then(|calendario| calendario.periodicidade.as_ref())
                    .map(super::descrever_periodicidade),
            ),
            Celula::texto(
                calendario
                    .and_then(|calendario| calendario.data_inicial.as_deref())
                    .map(data_br)
                    .as_deref(),
            ),
            Celula::dinheiro(rec.valor.as_ref().and_then(|valor| valor.valor_rec)),
            Celula::dinheiro(
                rec.valor
                    .as_ref()
                    .and_then(|valor| valor.valor_minimo_recebedor),
            ),
            Celula::texto(rec.id_rec.as_deref()),
        ]);
    }
    tabela
}

/// `3 recorrências · 2 aprovadas · 1 criada`.
fn totais(recs: &[Rec]) -> String {
    let mut texto = match recs.len() {
        1 => "1 recorrência".to_owned(),
        n => format!("{n} recorrências"),
    };
    for status in StatusRec::DOCUMENTADOS {
        let quantas = recs
            .iter()
            .filter(|rec| rec.status.as_ref() == Some(status))
            .count();
        match quantas {
            0 => {}
            1 => {
                let _ = write!(texto, " · 1 {}", curto(status));
            }
            n => {
                let _ = write!(texto, " · {n} {}s", curto(status));
            }
        }
    }
    texto
}

/// Every field, with the API's names (nested ones with a dot) and codes.
fn csv(recs: &[Rec]) -> Tabela {
    let texto = |campo: &'static str| Coluna::texto(campo, campo);
    let mut tabela = Tabela::new(vec![
        texto("idRec"),
        texto("status"),
        texto("vinculo.contrato"),
        texto("vinculo.objeto"),
        texto("vinculo.devedor.cpf"),
        texto("vinculo.devedor.cnpj"),
        texto("vinculo.devedor.nome"),
        texto("calendario.dataInicial"),
        texto("calendario.dataFinal"),
        texto("calendario.periodicidade"),
        Coluna::valor("valor.valorRec", "valor.valorRec"),
        Coluna::valor("valor.valorMinimoRecebedor", "valor.valorMinimoRecebedor"),
        texto("politicaRetentativa"),
        texto("loc.id"),
        texto("loc.location"),
        texto("recebedor.cnpj"),
        texto("recebedor.nome"),
    ]);
    for rec in recs {
        let vinculo = rec.vinculo.as_ref();
        let devedor = vinculo.and_then(|vinculo| vinculo.devedor.as_ref());
        let calendario = rec.calendario.as_ref();
        let loc = rec.loc.as_ref();
        let recebedor = rec.recebedor.as_ref();
        tabela.linha(vec![
            Celula::texto(rec.id_rec.as_deref()),
            Celula::texto(rec.status.as_ref().map(StatusRec::as_str)),
            Celula::texto(vinculo.and_then(|v| v.contrato.as_deref())),
            Celula::texto(vinculo.and_then(|v| v.objeto.as_deref())),
            Celula::texto(devedor.and_then(|d| d.cpf.as_deref())),
            Celula::texto(devedor.and_then(|d| d.cnpj.as_deref())),
            Celula::texto(devedor.and_then(|d| d.nome.as_deref())),
            Celula::texto(calendario.and_then(|c| c.data_inicial.as_deref())),
            Celula::texto(calendario.and_then(|c| c.data_final.as_deref())),
            Celula::texto(
                calendario
                    .and_then(|c| c.periodicidade.as_ref())
                    .map(Periodicidade::as_str),
            ),
            Celula::dinheiro(rec.valor.as_ref().and_then(|v| v.valor_rec)),
            Celula::dinheiro(rec.valor.as_ref().and_then(|v| v.valor_minimo_recebedor)),
            Celula::texto(
                rec.politica_retentativa
                    .as_ref()
                    .map(PoliticaRetentativa::as_str),
            ),
            Celula::texto(loc.and_then(|l| l.id).map(|id| id.to_string()).as_deref()),
            Celula::texto(loc.and_then(|l| l.location.as_deref())),
            Celula::texto(recebedor.and_then(|r| r.cnpj.as_deref())),
            Celula::texto(recebedor.and_then(|r| r.nome.as_deref())),
        ]);
    }
    tabela
}

async fn revisar(
    context: &Context,
    args: &RecRevisarArgs,
    terminal: &mut dyn Terminal,
) -> Result<(), CliError> {
    let mut revisao = RecRevisada::default();
    revisao.nome_devedor.clone_from(&args.devedor_nome);
    revisao.loc = args.loc;
    revisao.data_inicial = args.data_inicial;
    revisao.txid.clone_from(&args.txid_ativacao);
    revisao.validar().map_err(|err| {
        let opcao = match err.campo() {
            "vinculo.devedor.nome" => "--devedor-nome",
            outro => outro,
        };
        CliError::Usage(format!("{opcao}: {err}"))
    })?;
    if let Some(inicio) = revisao.data_inicial {
        inicio_valido(inicio, hoje())?;
    }
    // Nothing is looked up when no one could confirm.
    pode_confirmar(terminal, args.sim)?;
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let atual = client
        .pix_automatico()
        .consultar_rec(&args.id_rec, None)
        .await?;
    alteravel(&atual, &revisao)?;
    let ambiente = settings.ambiente.as_ref().map(|setting| setting.value);
    eprintln!("{}", resumo_revisao(&atual, &revisao, ambiente));
    confirmar(terminal, args.sim, "Alterar a recorrência?")?;

    let revisada = client
        .pix_automatico()
        .revisar_rec(&args.id_rec, &revisao)
        .await?;
    match context.formato() {
        Formato::Json => output::print_json(&revisada),
        // `commands::run` refuses csv for this command.
        Formato::Texto | Formato::Csv => output::print(&format!(
            "Recorrência alterada.\n\n{}",
            render_rec(&revisada)
        )),
    }
}

/// Refuses what can no longer change: a recurrence that ended, and, once
/// approved, its first payment and its activation.
fn alteravel(atual: &Rec, revisao: &RecRevisada) -> Result<(), CliError> {
    if let Some(status) = atual
        .status
        .as_ref()
        .filter(|status| encerrada(Some(status)))
    {
        return Err(CliError::Usage(format!(
            "a recorrência está {}: não pode ser alterada",
            descrever_status(status)
        )));
    }
    if atual.status == Some(StatusRec::Aprovada)
        && (revisao.data_inicial.is_some() || revisao.txid.is_some())
    {
        return Err(CliError::Usage(
            "a recorrência já foi aprovada: a data do primeiro pagamento e a cobrança de ativação não mudam mais"
                .to_owned(),
        ));
    }
    Ok(())
}

/// The recurrence as it is, and what changes.
fn resumo_revisao(atual: &Rec, revisao: &RecRevisada, ambiente: Option<Environment>) -> String {
    let mut linhas = vec![("Ambiente", descrever_ambiente(ambiente))];
    if let Some(status) = &atual.status {
        linhas.push(("Status", descrever_status(status).to_owned()));
    }
    let devedor = atual
        .vinculo
        .as_ref()
        .and_then(|vinculo| vinculo.devedor.as_ref());
    if let Some(linha) = antes_e_depois(
        devedor.and_then(|devedor| devedor.nome.clone()),
        revisao.nome_devedor.clone(),
    ) {
        linhas.push(("Devedor", linha));
    }
    let loc_atual = atual
        .loc
        .as_ref()
        .and_then(|loc| loc.id)
        .map(|id| id.to_string());
    if revisao.loc.is_some()
        && let Some(linha) = antes_e_depois(loc_atual, revisao.loc.map(|id| id.to_string()))
    {
        linhas.push(("Location", linha));
    }
    if let Some(inicio) = revisao.data_inicial {
        let atual = atual
            .calendario
            .as_ref()
            .and_then(|calendario| calendario.data_inicial.as_deref())
            .map(data_br);
        if let Some(linha) = antes_e_depois(atual, Some(inicio.format("%d/%m/%Y").to_string())) {
            linhas.push(("Primeiro pagamento", linha));
        }
    }
    if let Some(txid) = &revisao.txid {
        linhas.push(("Ativação", format!("→ pela cobrança imediata {txid}")));
    }
    let titulo = format!(
        "Recorrência {} a alterar",
        atual.id_rec.as_deref().unwrap_or_default()
    );
    secao(&titulo, &linhas)
}

async fn cancelar(
    context: &Context,
    args: &RecCancelarArgs,
    terminal: &mut dyn Terminal,
) -> Result<(), CliError> {
    // Nothing is looked up when no one could confirm.
    pode_confirmar(terminal, args.sim)?;
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let atual = client
        .pix_automatico()
        .consultar_rec(&args.id_rec, None)
        .await?;
    if let Some(status) = atual
        .status
        .as_ref()
        .filter(|status| encerrada(Some(status)))
    {
        return Err(CliError::Usage(format!(
            "a recorrência já está {}: não há o que cancelar",
            descrever_status(status)
        )));
    }
    let ambiente = settings.ambiente.as_ref().map(|setting| setting.value);
    eprintln!("{}", resumo_cancelamento(&atual, ambiente));
    confirmar(terminal, args.sim, "Cancelar a recorrência?")?;

    let cancelada = client
        .pix_automatico()
        .revisar_rec(&args.id_rec, &RecRevisada::cancelamento())
        .await?;
    match context.formato() {
        Formato::Json => output::print_json(&cancelada),
        // `commands::run` refuses csv for this command.
        Formato::Texto | Formato::Csv => output::print(&format!(
            "Recorrência cancelada: ela não aceita mais cobranças.\n\n{}",
            render_rec(&cancelada)
        )),
    }
}

/// The recurrence about to be cancelled.
fn resumo_cancelamento(atual: &Rec, ambiente: Option<Environment>) -> String {
    let mut linhas = vec![("Ambiente", descrever_ambiente(ambiente))];
    if let Some(status) = &atual.status {
        linhas.push(("Status", descrever_status(status).to_owned()));
    }
    if let Some(vinculo) = &atual.vinculo {
        if let Some(devedor) = vinculo.devedor.as_ref().and_then(pessoa) {
            linhas.push(("Devedor", devedor));
        }
        if let Some(contrato) = &vinculo.contrato {
            linhas.push(("Contrato", contrato.clone()));
        }
    }
    if let Some(periodo) = atual.calendario.as_ref().and_then(descrever_calendario) {
        linhas.push(("Periodicidade", periodo));
    }
    linhas.push(("Valor", descrever_valor(atual.valor.as_ref())));
    let titulo = format!(
        "Recorrência {} a cancelar",
        atual.id_rec.as_deref().unwrap_or_default()
    );
    secao(&titulo, &linhas)
}

#[cfg(test)]
mod tests {
    use chrono::FixedOffset;
    use clap::Parser;
    use inter_pj::pix_automatico::EncerramentoRec;
    use serde_json::{Value, json};
    use wiremock::matchers::{any, method, path};
    use wiremock::{Mock, ResponseTemplate};

    use super::*;
    use crate::cli::{Cli, Command, PixAutomaticoCommand};
    use crate::commands::testes;
    use crate::confirmacao::testes::TerminalFalso;

    const ID: &str = "RR1234567820260924abcdefghijk";

    fn criar_args(extra: &[&str]) -> RecCriarArgs {
        let mut todos = vec![
            "inter-pj",
            "pix-automatico",
            "rec",
            "criar",
            "--devedor-documento",
            "123.456.789-09",
            "--devedor-nome",
            "Cliente Exemplo",
            "--contrato",
            "contrato-001",
            "--data-inicial",
            "2026-10-10",
            "--periodicidade",
            "mensal",
        ];
        todos.extend_from_slice(extra);
        match Cli::try_parse_from(todos).unwrap().command {
            Command::PixAutomatico(PixAutomaticoCommand::Rec(RecCommand::Criar(args))) => *args,
            outro => panic!("{outro:?}"),
        }
    }

    fn brasilia() -> FixedOffset {
        FixedOffset::west_opt(3 * 3600).unwrap()
    }

    fn rec(status: &str) -> Rec {
        serde_json::from_value(json!({
            "idRec": ID,
            "vinculo": {"objeto": "Mensalidade", "devedor": {"cpf": "12345678909", "nome": "Cliente Exemplo"}, "contrato": "contrato-001"},
            "calendario": {"dataInicial": "2026-10-10", "dataFinal": "2027-09-10", "periodicidade": "MENSAL"},
            "valor": {"valorRec": "149.90"},
            "recebedor": {"cnpj": "12345678000195", "nome": "Empresa Exemplo Ltda"},
            "pagador": {"cpf": "12345678909", "ispbParticipante": "12345678"},
            "status": status,
            "politicaRetentativa": "PERMITE_3R_7D",
            "loc": {"id": 108, "location": "pix.example.com/qr/v2/rec/2353c790eefb11eaadc10242ac120002"},
            "atualizacao": [{"status": "CRIADA", "data": "2026-09-24T13:00:00.000Z"}, {"status": "APROVADA", "data": "2026-09-25T12:30:00.000Z"}],
            "ativacao": {"tipoJornada": "JORNADA_2"},
            "solicitacao": [{"idSolicRec": "SC1234567820260924abcdefghijk", "status": "ACEITA", "calendario": {"dataExpiracaoSolicitacao": "2026-10-01T13:00:00.000Z"}}],
            "dadosQR": {"jornada": "JORNADA_2", "pixCopiaECola": "00020126180014br.gov.bcb.pix"}
        }))
        .unwrap()
    }

    #[test]
    fn options_become_the_recurrence() {
        let rec = das_opcoes(&criar_args(&[
            "--objeto",
            "Mensalidade",
            "--data-final",
            "2027-09-10",
            "--valor",
            "149,90",
            "--retentativas",
            "--loc",
            "108",
            "--txid-ativacao",
            "7978c0c97ea847e78e8849634473c1f1",
        ]))
        .unwrap();
        assert_eq!(
            serde_json::to_value(&rec).unwrap(),
            json!({
                "vinculo": {"objeto": "Mensalidade", "devedor": {"cpf": "12345678909", "nome": "Cliente Exemplo"}, "contrato": "contrato-001"},
                "calendario": {"dataInicial": "2026-10-10", "dataFinal": "2027-09-10", "periodicidade": "MENSAL"},
                "valor": {"valorRec": "149.90"},
                "politicaRetentativa": "PERMITE_3R_7D",
                "loc": 108,
                "ativacao": {"dadosJornada": {"txid": "7978c0c97ea847e78e8849634473c1f1"}}
            })
        );
        let livre = das_opcoes(&criar_args(&[])).unwrap();
        let livre = serde_json::to_value(&livre).unwrap();
        assert_eq!(livre["politicaRetentativa"], "NAO_PERMITE");
        assert!(livre.get("valor").is_none(), "{livre}");
        let minimo = das_opcoes(&criar_args(&["--valor-minimo", "50"])).unwrap();
        assert_eq!(
            serde_json::to_value(&minimo).unwrap()["valor"],
            json!({"valorMinimoRecebedor": "50.00"})
        );
    }

    #[test]
    fn errors_name_the_option() {
        let longo = "x".repeat(36);
        let err = das_opcoes(&criar_args(&["--objeto", &longo])).unwrap_err();
        assert!(err.to_string().starts_with("--objeto: "), "{err}");
        let err = das_opcoes(&criar_args(&["--data-final", "2026-10-01"])).unwrap_err();
        assert!(err.to_string().starts_with("--data-final: "), "{err}");
        for extra in [
            &["--valor", "10", "--valor-minimo", "5"][..],
            &["--valor", "0"],
            &["--periodicidade", "quinzenal"],
            &["--txid-ativacao", "curto"],
            &["--arquivo", "rec.json"],
        ] {
            let mut todos = vec![
                "inter-pj",
                "pix-automatico",
                "rec",
                "criar",
                "--devedor-documento",
                "123.456.789-09",
                "--devedor-nome",
                "Cliente Exemplo",
                "--contrato",
                "contrato-001",
                "--data-inicial",
                "2026-10-10",
                "--periodicidade",
                "mensal",
            ];
            todos.extend_from_slice(extra);
            assert!(Cli::try_parse_from(todos).is_err(), "{extra:?}");
        }
        let hoje = NaiveDate::from_ymd_opt(2026, 9, 24).unwrap();
        assert!(inicio_valido(hoje, hoje).is_ok());
        assert!(inicio_valido(hoje.pred_opt().unwrap(), hoje).is_err());
    }

    #[test]
    fn the_summary_shows_the_contract() {
        let rec = das_opcoes(&criar_args(&[
            "--objeto",
            "Mensalidade",
            "--valor",
            "149,90",
            "--retentativas",
        ]))
        .unwrap();
        assert_eq!(
            resumo(&rec, Some(Environment::Sandbox)),
            "\
Recorrência a criar
  Ambiente       sandbox (dados fictícios)
  Devedor        Cliente Exemplo (123.456.789-09)
  Contrato       contrato-001
  Objeto         Mensalidade
  Periodicidade  mensal, a partir de 10/10/2026, sem fim
  Valor          R$ 149,90 (cento e quarenta e nove reais e noventa centavos) em cada pagamento
  Retentativas   até 3 novas tentativas, em 7 dias"
        );
        let producao = resumo(&rec, Some(Environment::Production));
        assert!(producao.starts_with("*** PRODUÇÃO"), "{producao}");
    }

    #[test]
    fn a_recurrence_in_detail() {
        assert_eq!(
            render_rec_em(&rec("APROVADA"), &brasilia()),
            format!(
                "\
Recorrência {ID}
  Status         aprovada (ativa)
  Devedor        Cliente Exemplo (123.456.789-09)
  Contrato       contrato-001
  Objeto         Mensalidade
  Periodicidade  mensal, de 10/10/2026 a 10/09/2027
  Valor          R$ 149,90 em cada pagamento
  Retentativas   até 3 novas tentativas, em 7 dias
  Recebedor      Empresa Exemplo Ltda (12.345.678/0001-95)
  Pagador        123.456.789-09 (ISPB do banco 12345678)
  Location       pix.example.com/qr/v2/rec/2353c790eefb11eaadc10242ac120002
  Ativação       QR Code da recorrência

Histórico
  24/09/2026 10:00:00  criada
  25/09/2026 09:30:00  aprovada

Solicitações de confirmação
  SC1234567820260924abcdefghijk  aceita pelo pagador; expira em 01/10/2026 10:00:00

Copia e cola  00020126180014br.gov.bcb.pix"
            )
        );
    }

    #[test]
    fn endings_in_words() {
        let mut cancelada = rec("CANCELADA");
        cancelada.encerramento = serde_json::from_value(json!({
            "cancelamento": {"solicitante": "USUARIO_RECEBEDOR", "codigo": "SLCR", "descricao": "Cancelamento solicitado pelo usuário recebedor"}
        }))
        .unwrap();
        assert_eq!(
            encerramento(cancelada.encerramento.as_ref().unwrap()).unwrap(),
            "cancelada pelo recebedor: SLCR, Cancelamento solicitado pelo usuário recebedor"
        );
        let rejeitada: EncerramentoRec = serde_json::from_value(
            json!({"rejeicao": {"codigo": "AP13", "descricao": "Pagador recusou"}}),
        )
        .unwrap();
        assert_eq!(
            encerramento(&rejeitada).unwrap(),
            "rejeitada: AP13, Pagador recusou"
        );
        assert!(copia_e_cola(&cancelada).is_err());
        assert_eq!(
            copia_e_cola(&rec("CRIADA")),
            Ok("00020126180014br.gov.bcb.pix")
        );
    }

    #[test]
    fn listings_add_up() {
        let recs = [rec("APROVADA"), rec("APROVADA"), rec("CRIADA")];
        assert_eq!(totais(&recs), "3 recorrências · 1 criada · 2 aprovadas");
        let csv = csv(&recs).csv(crate::tabela::Separador::Virgula);
        assert!(
            csv.starts_with("idRec,status,vinculo.contrato,vinculo.objeto,"),
            "{csv}"
        );
        assert!(
            csv.contains(&format!(
                "{ID},APROVADA,contrato-001,Mensalidade,12345678909,,"
            )),
            "{csv}"
        );
    }

    #[test]
    fn what_can_no_longer_change_is_refused() {
        let mut data = RecRevisada::default();
        data.data_inicial = NaiveDate::from_ymd_opt(2026, 11, 10);
        assert!(alteravel(&rec("APROVADA"), &data).is_err());
        assert!(alteravel(&rec("CRIADA"), &data).is_ok());
        let mut nome = RecRevisada::default();
        nome.nome_devedor = Some("Cliente Exemplo Ltda".to_owned());
        assert!(alteravel(&rec("APROVADA"), &nome).is_ok());
        assert!(alteravel(&rec("CANCELADA"), &nome).is_err());
    }

    // --- the commands against a mock API ------------------------------------

    async fn cenario(args: &[&str]) -> (testes::Cenario, RecCommand) {
        let mut todos = vec!["pix-automatico", "rec"];
        todos.extend_from_slice(args);
        match testes::cenario(&todos, "rec.write rec.read").await {
            (cenario, Command::PixAutomatico(PixAutomaticoCommand::Rec(comando))) => {
                (cenario, comando)
            }
            (_, outro) => panic!("{outro:?}"),
        }
    }

    async fn nada_e_enviado(cenario: &testes::Cenario) {
        Mock::given(any())
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&cenario.server)
            .await;
    }

    #[tokio::test]
    async fn a_declined_creation_sends_nothing() {
        let (cenario, comando) = cenario(&[
            "criar",
            "--devedor-documento",
            "123.456.789-09",
            "--devedor-nome",
            "Cliente Exemplo",
            "--contrato",
            "contrato-001",
            "--data-inicial",
            "2099-10-10",
            "--periodicidade",
            "mensal",
        ])
        .await;
        let RecCommand::Criar(args) = comando else {
            unreachable!()
        };
        nada_e_enviado(&cenario).await;
        let mut terminal = TerminalFalso::respondendo("n\n");
        let err = criar(&cenario.context, &args, &mut terminal)
            .await
            .unwrap_err();
        assert!(matches!(err, CliError::Cancelado), "{err}");
        assert_eq!(terminal.perguntas, ["Criar a recorrência? [s/N] "]);
    }

    #[tokio::test]
    async fn a_declined_cancellation_only_looks_the_recurrence_up() {
        let (cenario, comando) = cenario(&["cancelar", ID]).await;
        let RecCommand::Cancelar(args) = comando else {
            unreachable!()
        };
        let atual: Value = serde_json::to_value(rec("APROVADA")).unwrap();
        Mock::given(method("GET"))
            .and(path(format!("/pix/v2/rec/{ID}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(atual))
            .expect(1)
            .mount(&cenario.server)
            .await;
        nada_e_enviado(&cenario).await;
        let mut terminal = TerminalFalso::respondendo("n\n");
        let err = cancelar(&cenario.context, &args, &mut terminal)
            .await
            .unwrap_err();
        assert!(matches!(err, CliError::Cancelado), "{err}");
        assert_eq!(terminal.perguntas, ["Cancelar a recorrência? [s/N] "]);
    }
}
