//! `inter-pj pix cob criar|revisar|consultar|listar`: immediate charges.

use std::fmt::Write as _;

use chrono::{DateTime, Local, TimeDelta, TimeZone};
use inter_pj::endpoint;
use inter_pj::pix::{
    CalendarioCobGerado, Cob, CobRevisada, CobSolicitada, Devedor, FiltroCobs, LocCob, StatusCob,
    Txid, ValorCobRevisada,
};
use serde_json::json;

use super::{
    Filtros, alteravel, antes_e_depois, celula_status, copia_e_cola_ativa, descrever_status,
    incerta, paginacao, pessoa, tabela_pix,
};
use crate::chamada::chamada;
use crate::cli::{
    Formato, PixCobCommand, PixCobConsultarArgs, PixCobCriarArgs, PixCobListarArgs,
    PixCobRevisarArgs, SimNao,
};
use crate::commands::qrcode::OpcoesQr;
use crate::commands::{Context, simulacao};
use crate::confirmacao::{Stdio, Terminal, confirmar, descrever_ambiente, pode_confirmar};
use crate::error::CliError;
use crate::output::{self, horario_em, horario_local, secao};
use crate::tabela::{Celula, Coluna, Tabela};
use crate::valor::por_extenso;

/// Longest payer name shown in the listing.
const LARGURA_DEVEDOR: usize = 30;

pub(super) async fn run(context: &Context, command: PixCobCommand) -> Result<(), CliError> {
    match command {
        PixCobCommand::Criar(args) => criar(context, &args, &mut Stdio).await,
        PixCobCommand::Revisar(args) => revisar(context, &args, &mut Stdio).await,
        PixCobCommand::Consultar(args) => consultar(context, &args).await,
        PixCobCommand::Listar(args) => listar(context, &args).await,
        PixCobCommand::Pagar(args) => super::sandbox::pagar_cob(context, &args).await,
    }
}

async fn criar(
    context: &Context,
    args: &PixCobCriarArgs,
    terminal: &mut dyn Terminal,
) -> Result<(), CliError> {
    let opcoes = OpcoesQr::new(
        context,
        args.qr.qrcode,
        args.qr.qrcode_png.clone(),
        args.qr.sobrescrever,
    )?;
    let cob = das_opcoes(args)?;
    let txid = args.txid.clone().unwrap_or_else(Txid::novo);
    let settings = context.settings()?;
    // Configuration problems show up before the confirmation, not after it.
    let client = if args.simular {
        None
    } else {
        Some(context.client(&settings)?)
    };
    let ambiente = settings.ambiente.as_ref().map(|setting| setting.value);
    output::eprint(&resumo(&cob, &txid, ambiente));

    let Some(client) = client else {
        return simulacao::mostrar_em(
            context,
            &settings,
            endpoint::pix::CRIAR_COB,
            &[("txid", txid.as_str())],
            &[],
            &cob,
        );
    };
    confirmar(terminal, args.sim, "Criar a cobrança?")?;
    let criada = client
        .pix()
        .criar_cob(&txid, &cob)
        .await
        .map_err(|err| incerta(err, "pix cob", &txid))?;
    let texto = format!(
        "Cobrança Pix criada.\n\n{}\n\nAcompanhe com: {} pix cob consultar {txid}",
        render_cob(&criada),
        chamada()
    );
    opcoes.mostrar(context, &settings, &texto, &criada, &copia_e_cola(&criada))
}

/// The charge of the options, checked as the API would.
fn das_opcoes(args: &PixCobCriarArgs) -> Result<CobSolicitada, CliError> {
    let mut cob = CobSolicitada::new(args.chave.clone(), args.valor);
    cob.calendario.expiracao = args.expiracao;
    cob.valor.modalidade_alteracao = args.valor_alteravel;
    if let (Some(documento), Some(nome)) = (&args.devedor_documento, &args.devedor_nome) {
        cob.devedor = Some(Devedor::new(documento.clone(), nome.clone()));
    }
    cob.loc = args.loc.map(LocCob::new);
    cob.solicitacao_pagador.clone_from(&args.solicitacao);
    cob.info_adicionais.clone_from(&args.info);
    cob.validar()
        .map_err(|err| CliError::Usage(format!("{}: {err}", opcao(err.campo()))))?;
    Ok(cob)
}

/// The option of a field of the API, for the messages.
fn opcao(campo: &str) -> &str {
    match campo {
        "valor.original" => "--valor",
        "calendario.expiracao" => "--expiracao",
        "devedor.nome" => "--devedor-nome",
        "solicitacaoPagador" => "--solicitacao",
        info if info.starts_with("infoAdicionais") => "--info",
        outro => outro,
    }
}

/// The charge about to be created.
fn resumo(cob: &CobSolicitada, txid: &Txid, ambiente: Option<inter_pj::Environment>) -> String {
    let valor = cob.valor.original;
    let extenso = por_extenso(valor)
        .map(|extenso| format!(" ({extenso})"))
        .unwrap_or_default();
    let alteravel = if cob.valor.modalidade_alteracao {
        "; o pagador pode alterar"
    } else {
        ""
    };
    let mut linhas = vec![
        ("Ambiente", descrever_ambiente(ambiente)),
        (
            "Valor",
            format!("{}{extenso}{alteravel}", output::brl(valor)),
        ),
        (
            "Chave",
            format!("{} ({})", cob.chave.as_str(), cob.chave.tipo()),
        ),
        ("Expira", expiracao(cob.calendario.expiracao)),
    ];
    if let Some(devedor) = &cob.devedor {
        linhas.push((
            "Devedor",
            format!("{} ({})", devedor.nome, devedor.documento.formatado()),
        ));
    }
    if let Some(solicitacao) = &cob.solicitacao_pagador {
        linhas.push(("Solicitação", solicitacao.clone()));
    }
    if !cob.info_adicionais.is_empty() {
        let infos: Vec<String> = cob
            .info_adicionais
            .iter()
            .map(|info| format!("{}: {}", info.nome, info.valor))
            .collect();
        linhas.push(("Informações", infos.join(" / ")));
    }
    if let Some(loc) = &cob.loc {
        linhas.push(("Location", loc.id.to_string()));
    }
    linhas.push(("txid", txid.to_string()));
    let mut texto = String::new();
    if ambiente.is_some_and(inter_pj::Environment::is_production) {
        texto.push_str("*** PRODUÇÃO: a cobrança vale de verdade ***\n");
    }
    texto.push_str(&secao("Cobrança Pix a criar", &linhas));
    texto
}

/// `1 hora após a criação`, `30 minutos após a criação`.
fn expiracao(segundos: Option<u32>) -> String {
    let Some(segundos) = segundos else {
        return "1 dia após a criação (padrão da API)".to_owned();
    };
    let (quantidade, unidade) = match segundos {
        s if s % 86_400 == 0 => (s / 86_400, ("dia", "dias")),
        s if s % 3600 == 0 => (s / 3600, ("hora", "horas")),
        s if s % 60 == 0 => (s / 60, ("minuto", "minutos")),
        s => (s, ("segundo", "segundos")),
    };
    let unidade = if quantidade == 1 {
        unidade.0
    } else {
        unidade.1
    };
    format!("{quantidade} {unidade} após a criação")
}

async fn revisar(
    context: &Context,
    args: &PixCobRevisarArgs,
    terminal: &mut dyn Terminal,
) -> Result<(), CliError> {
    let revisao = revisao_das_opcoes(args)?;
    // Nothing is looked up when no one could confirm.
    pode_confirmar(terminal, args.sim)?;
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let atual = client.pix().consultar_cob(&args.txid).await?;
    alteravel(atual.status.as_ref())?;
    let ambiente = settings.ambiente.as_ref().map(|setting| setting.value);
    output::eprint(&resumo_revisao(&atual, &revisao, ambiente));
    let pergunta = if revisao.remover {
        "Remover a cobrança?"
    } else {
        "Alterar a cobrança?"
    };
    confirmar(terminal, args.sim, pergunta)?;

    let revisada = client.pix().revisar_cob(&args.txid, &revisao).await?;
    match context.formato() {
        Formato::Json => output::print_json(&revisada),
        // `commands::run` refuses csv for this command.
        Formato::Texto | Formato::Csv => {
            let titulo = if revisao.remover {
                "Cobrança Pix removida: ela não pode mais ser paga.".to_owned()
            } else {
                match revisada.revisao {
                    Some(numero) => format!("Cobrança Pix alterada (revisão {numero})."),
                    None => "Cobrança Pix alterada.".to_owned(),
                }
            };
            output::print(&format!("{titulo}\n\n{}", render_cob(&revisada)))
        }
    }
}

fn revisao_das_opcoes(args: &PixCobRevisarArgs) -> Result<CobRevisada, CliError> {
    if args.remover {
        return Ok(CobRevisada::remocao());
    }
    let mut revisao = CobRevisada::new();
    if let Some(expiracao) = args.expiracao {
        let mut calendario = inter_pj::pix::CalendarioCob::default();
        calendario.expiracao = Some(expiracao);
        revisao.calendario = Some(calendario);
    }
    if let (Some(documento), Some(nome)) = (&args.devedor_documento, &args.devedor_nome) {
        revisao.devedor = Some(Devedor::new(documento.clone(), nome.clone()));
    }
    revisao.loc = args.loc.map(LocCob::new);
    if args.valor.is_some() || args.valor_alteravel.is_some() {
        let mut valor = ValorCobRevisada::default();
        valor.original = args.valor;
        valor.modalidade_alteracao = args.valor_alteravel.map(|sim| sim == SimNao::Sim);
        revisao.valor = Some(valor);
    }
    revisao.chave.clone_from(&args.chave);
    revisao.solicitacao_pagador.clone_from(&args.solicitacao);
    if !args.info.is_empty() {
        revisao.info_adicionais = Some(args.info.clone());
    }
    revisao
        .validar()
        .map_err(|err| CliError::Usage(format!("{}: {err}", opcao(err.campo()))))?;
    Ok(revisao)
}

/// The charge as it is, and what changes.
fn resumo_revisao(
    atual: &Cob,
    revisao: &CobRevisada,
    ambiente: Option<inter_pj::Environment>,
) -> String {
    let mut linhas = vec![("Ambiente", descrever_ambiente(ambiente))];
    let original = atual.valor.as_ref().and_then(|valor| valor.original);
    let novo = revisao.valor.as_ref().and_then(|valor| valor.original);
    if let Some(valor) = antes_e_depois(original.map(output::brl), novo.map(output::brl)) {
        linhas.push(("Valor", valor));
    }
    if let Some(alteravel) = revisao
        .valor
        .as_ref()
        .and_then(|valor| valor.modalidade_alteracao)
    {
        let sim = if alteravel { "sim" } else { "não" };
        linhas.push(("Valor alterável", format!("→ {sim}")));
    }
    let expiracao_atual = atual
        .calendario
        .as_ref()
        .and_then(|calendario| calendario.expiracao)
        .map(|segundos| expiracao(u32::try_from(segundos).ok()));
    let nova_expiracao = revisao
        .calendario
        .and_then(|calendario| calendario.expiracao)
        .map(|segundos| expiracao(Some(segundos)));
    if let Some(linha) = antes_e_depois(expiracao_atual, nova_expiracao) {
        linhas.push(("Expira", linha));
    }
    let devedor_atual = atual.devedor.as_ref().and_then(pessoa);
    let novo_devedor = revisao
        .devedor
        .as_ref()
        .map(|devedor| format!("{} ({})", devedor.nome, devedor.documento.formatado()));
    if let Some(linha) = antes_e_depois(devedor_atual, novo_devedor) {
        linhas.push(("Devedor", linha));
    }
    if let Some(chave) = &revisao.chave {
        linhas.push(("Chave", format!("→ {}", chave.as_str())));
    }
    if let Some(solicitacao) = &revisao.solicitacao_pagador {
        linhas.push(("Solicitação", format!("→ {solicitacao}")));
    }
    if let Some(infos) = &revisao.info_adicionais {
        let infos: Vec<String> = infos
            .iter()
            .map(|info| format!("{}: {}", info.nome, info.valor))
            .collect();
        linhas.push(("Informações", format!("→ {}", infos.join(" / "))));
    }
    if let Some(status) = &atual.status {
        linhas.push(("Status", descrever_status(status).to_owned()));
    }
    let titulo = format!(
        "Cobrança Pix {} a {}",
        atual.txid.as_deref().unwrap_or_default(),
        if revisao.remover {
            "remover"
        } else {
            "alterar"
        }
    );
    secao(&titulo, &linhas)
}

async fn consultar(context: &Context, args: &PixCobConsultarArgs) -> Result<(), CliError> {
    let opcoes = OpcoesQr::new(
        context,
        args.qr.qrcode,
        args.qr.qrcode_png.clone(),
        args.qr.sobrescrever,
    )?;
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let cob = client.pix().consultar_cob(&args.txid).await?;
    opcoes.mostrar(
        context,
        &settings,
        &render_cob(&cob),
        &cob,
        &copia_e_cola(&cob),
    )
}

/// The "copia e cola" of a charge that can still be paid, or why there is
/// none.
fn copia_e_cola(cob: &Cob) -> Result<&str, String> {
    copia_e_cola_ativa(cob.status.as_ref(), cob.pix_copia_e_cola.as_deref())
}

/// An immediate charge in detail, with the times in the local time zone.
fn render_cob(cob: &Cob) -> String {
    render_cob_em(cob, &Local)
}

fn render_cob_em<Tz: TimeZone>(cob: &Cob, fuso: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let mut linhas = Vec::new();
    if let Some(status) = &cob.status {
        linhas.push(("Status", descrever_status(status).to_owned()));
    }
    if let Some(valor) = cob.valor.as_ref()
        && let Some(original) = valor.original
    {
        let alteravel = if valor.modalidade_alteracao == Some(1) {
            " (o pagador pode alterar)"
        } else {
            ""
        };
        linhas.push(("Valor", format!("{}{alteravel}", output::brl(original))));
    }
    if let Some(calendario) = &cob.calendario {
        if let Some(criacao) = &calendario.criacao {
            linhas.push(("Criada em", horario_em(criacao, fuso)));
        }
        if let Some(expira) = expira_em(calendario, fuso) {
            linhas.push(("Expira em", expira));
        }
    }
    if let Some(devedor) = cob.devedor.as_ref().and_then(pessoa) {
        linhas.push(("Devedor", devedor));
    }
    if let Some(chave) = &cob.chave {
        linhas.push(("Chave", chave.clone()));
    }
    if let Some(solicitacao) = &cob.solicitacao_pagador {
        linhas.push(("Solicitação", solicitacao.clone()));
    }
    if let Some(revisao) = cob.revisao {
        linhas.push(("Revisão", revisao.to_string()));
    }
    let location = cob
        .location
        .as_deref()
        .or_else(|| cob.loc.as_ref().and_then(|loc| loc.location.as_deref()));
    if let Some(location) = location {
        linhas.push(("Location", location.to_owned()));
    }
    let titulo = format!("Cobrança Pix {}", cob.txid.as_deref().unwrap_or_default());
    let mut texto = secao(titulo.trim(), &linhas);
    if !cob.info_adicionais.is_empty() {
        let infos: Vec<(&str, String)> = cob
            .info_adicionais
            .iter()
            .map(|info| (info.nome.as_str(), info.valor.clone()))
            .collect();
        let _ = write!(texto, "\n\n{}", secao("Informações", &infos));
    }
    if !cob.pix.is_empty() {
        let _ = write!(
            texto,
            "\n\nPix recebidos\n{}",
            tabela_pix(&cob.pix, fuso).texto_colorido()
        );
    }
    if let Some(copia_e_cola) = cob
        .pix_copia_e_cola
        .as_deref()
        .filter(|texto| !texto.is_empty())
    {
        let _ = write!(texto, "\n\nCopia e cola  {copia_e_cola}");
    }
    texto
}

/// When the charge expires: its creation plus the expiration, in the local
/// time zone.
fn expira_em<Tz: TimeZone>(calendario: &CalendarioCobGerado, fuso: &Tz) -> Option<String>
where
    Tz::Offset: std::fmt::Display,
{
    let criacao = DateTime::parse_from_rfc3339(calendario.criacao.as_deref()?).ok()?;
    let segundos = i64::try_from(calendario.expiracao?).ok()?;
    let expira = criacao.checked_add_signed(TimeDelta::try_seconds(segundos)?)?;
    Some(
        expira
            .with_timezone(fuso)
            .format("%d/%m/%Y %H:%M:%S")
            .to_string(),
    )
}

async fn listar(context: &Context, args: &PixCobListarArgs) -> Result<(), CliError> {
    let filtros = Filtros::de(args)?;
    let mut filtro = FiltroCobs::new(filtros.periodo);
    filtro.devedor.clone_from(&filtros.devedor);
    filtro.status.clone_from(&filtros.status);
    filtro.location_presente = filtros.location_presente;
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let (cobs, pagina) = match args.pagina {
        Some(numero) => {
            let mut pagina = client
                .pix()
                .listar_cobs(&filtro, numero, args.itens_por_pagina)
                .await?;
            if context.formato() == Formato::Json {
                return output::print_json(&pagina);
            }
            let cobs = std::mem::take(&mut pagina.cobs);
            (cobs, Some((numero, pagina)))
        }
        None => (client.pix().listar_todas_cobs(&filtro).await?, None),
    };
    match context.formato() {
        Formato::Json => output::print_json(&json!({ "cobs": cobs })),
        Formato::Csv => output::print_csv(&csv(&cobs), context.separador()),
        Formato::Texto => {
            context.warn_if_sandbox(&settings);
            let mut texto = format!("{}\n\n", filtros.titulo("Cobranças Pix imediatas", &[]));
            if cobs.is_empty() {
                texto.push_str("Nenhuma cobrança encontrada.");
            } else {
                texto.push_str(&tabela(&cobs).texto_colorido());
                let _ = write!(texto, "\n\n{}", totais(&cobs));
            }
            if let Some((numero, pagina)) = pagina {
                texto.push_str(&paginacao(
                    numero,
                    &pagina.parametros,
                    cobs.len(),
                    "cobranças",
                ));
            }
            output::print(&texto)
        }
    }
}

fn tabela(cobs: &[Cob]) -> Tabela {
    let mut tabela = Tabela::new(vec![
        Coluna::texto("Criada em", ""),
        Coluna::texto("Status", ""),
        Coluna::valor("Valor", ""),
        Coluna::texto("Devedor", "").no_maximo(LARGURA_DEVEDOR),
        Coluna::texto("txid", ""),
    ]);
    for cob in cobs {
        let criacao = cob
            .calendario
            .as_ref()
            .and_then(|calendario| calendario.criacao.as_deref())
            .map(horario_local);
        tabela.linha(vec![
            Celula::texto(criacao.as_deref()),
            celula_status(cob.status.as_ref()),
            Celula::dinheiro(cob.valor.as_ref().and_then(|valor| valor.original)),
            Celula::texto(
                cob.devedor
                    .as_ref()
                    .and_then(|devedor| devedor.nome.as_deref()),
            ),
            Celula::texto(cob.txid.as_deref()),
        ]);
    }
    tabela
}

/// `3 cobranças · R$ 450,00 · pagas R$ 150,00`.
fn totais(cobs: &[Cob]) -> String {
    super::totais(cobs.iter().map(|cob| {
        (
            cob.valor.as_ref().and_then(|valor| valor.original),
            cob.status.as_ref(),
        )
    }))
}

/// Every field, with the API's names (nested ones with a dot) and codes.
fn csv(cobs: &[Cob]) -> Tabela {
    let texto = |campo: &'static str| Coluna::texto(campo, campo);
    let mut tabela = Tabela::new(vec![
        texto("txid"),
        texto("status"),
        texto("revisao"),
        texto("calendario.criacao"),
        texto("calendario.expiracao"),
        Coluna::valor("valor.original", "valor.original"),
        texto("valor.modalidadeAlteracao"),
        texto("devedor.cpf"),
        texto("devedor.cnpj"),
        texto("devedor.nome"),
        texto("chave"),
        texto("solicitacaoPagador"),
        texto("location"),
        texto("pixCopiaECola"),
    ]);
    for cob in cobs {
        let calendario = cob.calendario.as_ref();
        let valor = cob.valor.as_ref();
        let devedor = cob.devedor.as_ref();
        tabela.linha(vec![
            Celula::texto(cob.txid.as_deref()),
            Celula::texto(cob.status.as_ref().map(StatusCob::as_str)),
            Celula::texto(cob.revisao.map(|n| n.to_string()).as_deref()),
            Celula::texto(calendario.and_then(|c| c.criacao.as_deref())),
            Celula::texto(
                calendario
                    .and_then(|c| c.expiracao)
                    .map(|n| n.to_string())
                    .as_deref(),
            ),
            Celula::dinheiro(valor.and_then(|v| v.original)),
            Celula::texto(
                valor
                    .and_then(|v| v.modalidade_alteracao)
                    .map(|n| n.to_string())
                    .as_deref(),
            ),
            Celula::texto(devedor.and_then(|d| d.cpf.as_deref())),
            Celula::texto(devedor.and_then(|d| d.cnpj.as_deref())),
            Celula::texto(devedor.and_then(|d| d.nome.as_deref())),
            Celula::texto(cob.chave.as_deref()),
            Celula::texto(cob.solicitacao_pagador.as_deref()),
            Celula::texto(cob.location.as_deref()),
            Celula::texto(cob.pix_copia_e_cola.as_deref()),
        ]);
    }
    tabela
}

#[cfg(test)]
mod tests {
    use chrono::FixedOffset;
    use clap::Parser;
    use serde_json::{Value, json};
    use wiremock::matchers::{any, method, path};
    use wiremock::{Mock, ResponseTemplate};

    use super::*;
    use crate::cli::{Cli, Command, PixCommand};
    use crate::commands::testes;
    use crate::confirmacao::testes::TerminalFalso;

    const TXID: &str = "7978c0c97ea847e78e8849634473c1f1";

    fn criar_args(extra: &[&str]) -> PixCobCriarArgs {
        let mut todos = vec![
            "inter-pj",
            "pix",
            "cob",
            "criar",
            "--chave",
            "pix@empresa.example",
            "--valor",
            "149,90",
        ];
        todos.extend_from_slice(extra);
        match Cli::try_parse_from(todos).unwrap().command {
            Command::Pix(PixCommand::Cob(PixCobCommand::Criar(args))) => *args,
            outro => panic!("{outro:?}"),
        }
    }

    fn brasilia() -> FixedOffset {
        FixedOffset::west_opt(3 * 3600).unwrap()
    }

    fn cob(status: &str) -> Cob {
        serde_json::from_value(json!({
            "calendario": {"criacao": "2026-09-23T20:15:00.358Z", "expiracao": 3600},
            "txid": TXID,
            "revisao": 0,
            "status": status,
            "devedor": {"cpf": "12345678909", "nome": "Fulano de Tal"},
            "valor": {"original": "149.90", "modalidadeAlteracao": 1},
            "chave": "pix@empresa.example",
            "solicitacaoPagador": "Pedido 123",
            "infoAdicionais": [{"nome": "Pedido", "valor": "123"}],
            "location": "pix.example.com/qr/v2/9d36b84fc70b478fb95c12729b90ca25",
            "pixCopiaECola": "00020101021226",
            "pix": [{"endToEndId": "E12345678202609232020abcdef12345", "valor": "149.90", "horario": "2026-09-23T20:20:00.000Z", "devolucoes": [{"id": "D1", "valor": "10.00", "status": "DEVOLVIDO"}]}]
        }))
        .unwrap()
    }

    #[test]
    fn options_become_the_charge() {
        let cob = das_opcoes(&criar_args(&[
            "--expiracao",
            "1h",
            "--valor-alteravel",
            "--devedor-documento",
            "123.456.789-09",
            "--devedor-nome",
            "Fulano de Tal",
            "--solicitacao",
            "Pedido 123",
            "--info",
            "Pedido=123",
            "--info",
            "Loja = Centro",
            "--loc",
            "789",
        ]))
        .unwrap();
        assert_eq!(
            serde_json::to_value(&cob).unwrap(),
            json!({
                "calendario": {"expiracao": 3600},
                "devedor": {"cpf": "12345678909", "nome": "Fulano de Tal"},
                "loc": {"id": 789},
                "valor": {"original": "149.90", "modalidadeAlteracao": 1},
                "chave": "pix@empresa.example",
                "solicitacaoPagador": "Pedido 123",
                "infoAdicionais": [{"nome": "Pedido", "valor": "123"}, {"nome": "Loja", "valor": "Centro"}]
            })
        );
    }

    #[test]
    fn errors_name_the_option() {
        let longa = "x".repeat(141);
        let err = das_opcoes(&criar_args(&["--solicitacao", &longa])).unwrap_err();
        assert!(err.to_string().starts_with("--solicitacao: "), "{err}");
        let err = das_opcoes(&criar_args(&["--info", "=sem nome"])).unwrap_err();
        assert!(err.to_string().starts_with("--info: "), "{err}");
        for (opcao, valor) in [
            ("--expiracao", "0"),
            ("--expiracao", "1 semana"),
            ("--txid", "curto"),
            ("--info", "sem-igual"),
        ] {
            let todos = [
                "inter-pj",
                "pix",
                "cob",
                "criar",
                "--chave",
                "pix@empresa.example",
                "--valor",
                "1",
                opcao,
                valor,
            ];
            assert!(Cli::try_parse_from(todos).is_err(), "{opcao} {valor}");
        }
    }

    #[test]
    fn expirations_in_words() {
        assert_eq!(expiracao(None), "1 dia após a criação (padrão da API)");
        assert_eq!(expiracao(Some(3600)), "1 hora após a criação");
        assert_eq!(expiracao(Some(1800)), "30 minutos após a criação");
        assert_eq!(expiracao(Some(7 * 86_400)), "7 dias após a criação");
        assert_eq!(expiracao(Some(90)), "90 segundos após a criação");
    }

    #[test]
    fn the_summary_shows_what_the_payer_sees() {
        let cob = das_opcoes(&criar_args(&[
            "--expiracao",
            "30m",
            "--devedor-documento",
            "12.345.678/0001-95",
            "--devedor-nome",
            "Empresa Exemplo",
            "--info",
            "Pedido=123",
        ]))
        .unwrap();
        assert_eq!(
            resumo(
                &cob,
                &TXID.parse().unwrap(),
                Some(inter_pj::Environment::Sandbox)
            ),
            format!(
                "\
Cobrança Pix a criar
  Ambiente     sandbox (dados fictícios)
  Valor        R$ 149,90 (cento e quarenta e nove reais e noventa centavos)
  Chave        pix@empresa.example (e-mail)
  Expira       30 minutos após a criação
  Devedor      Empresa Exemplo (12.345.678/0001-95)
  Informações  Pedido: 123
  txid         {TXID}"
            )
        );
        let producao = resumo(
            &cob,
            &TXID.parse().unwrap(),
            Some(inter_pj::Environment::Production),
        );
        assert!(producao.starts_with("*** PRODUÇÃO"), "{producao}");
    }

    #[test]
    fn a_charge_in_detail() {
        assert_eq!(
            render_cob_em(&cob("CONCLUIDA"), &brasilia()),
            format!(
                "\
Cobrança Pix {TXID}
  Status       concluída (paga)
  Valor        R$ 149,90 (o pagador pode alterar)
  Criada em    23/09/2026 17:15:00
  Expira em    23/09/2026 18:15:00
  Devedor      Fulano de Tal (123.456.789-09)
  Chave        pix@empresa.example
  Solicitação  Pedido 123
  Revisão      0
  Location     pix.example.com/qr/v2/9d36b84fc70b478fb95c12729b90ca25

Informações
  Pedido  123

Pix recebidos
Horário                  Valor  Devolvido  endToEndId
23/09/2026 17:20:00  R$ 149,90   R$ 10,00  E12345678202609232020abcdef12345

Copia e cola  00020101021226"
            )
        );
    }

    #[test]
    fn only_active_charges_have_a_qr_code() {
        assert_eq!(copia_e_cola(&cob("ATIVA")), Ok("00020101021226"));
        assert_eq!(
            copia_e_cola(&cob("CONCLUIDA")).unwrap_err(),
            "a cobrança está concluída (paga): o QR Code não serve mais para pagar"
        );
        let mut sem = cob("ATIVA");
        sem.pix_copia_e_cola = None;
        assert!(copia_e_cola(&sem).is_err());
        assert!(alteravel(cob("CONCLUIDA").status.as_ref()).is_err());
        assert!(alteravel(cob("REMOVIDA_PELO_PSP").status.as_ref()).is_err());
        assert!(alteravel(cob("ATIVA").status.as_ref()).is_ok());
    }

    #[test]
    fn listings_add_up() {
        let cobs = [cob("CONCLUIDA"), cob("ATIVA")];
        assert_eq!(totais(&cobs), "2 cobranças · R$ 299,80 · pagas R$ 149,90");
        let csv = csv(&cobs).csv(crate::tabela::Separador::Virgula);
        assert!(
            csv.starts_with("txid,status,revisao,calendario.criacao,"),
            "{csv}"
        );
        assert!(
            csv.contains(",CONCLUIDA,0,2026-09-23T20:15:00.358Z,3600,149.90,1,12345678909,"),
            "{csv}"
        );
    }

    // --- the commands against a mock API ------------------------------------

    async fn cenario(args: &[&str]) -> (testes::Cenario, PixCobCommand) {
        let mut todos = vec!["pix", "cob"];
        todos.extend_from_slice(args);
        match testes::cenario(&todos, "cob.write cob.read").await {
            (cenario, Command::Pix(PixCommand::Cob(comando))) => (cenario, comando),
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
        let (cenario, comando) =
            cenario(&["criar", "--chave", "pix@empresa.example", "--valor", "10"]).await;
        let PixCobCommand::Criar(args) = comando else {
            unreachable!()
        };
        nada_e_enviado(&cenario).await;
        let mut terminal = TerminalFalso::respondendo("n\n");
        let err = criar(&cenario.context, &args, &mut terminal)
            .await
            .unwrap_err();
        assert!(matches!(err, CliError::Cancelado), "{err}");
        assert_eq!(terminal.perguntas, ["Criar a cobrança? [s/N] "]);
    }

    #[tokio::test]
    async fn a_declined_revision_only_looks_the_charge_up() {
        let (cenario, comando) = cenario(&["revisar", TXID, "--remover"]).await;
        let PixCobCommand::Revisar(args) = comando else {
            unreachable!()
        };
        let atual: Value = serde_json::to_value(cob("ATIVA")).unwrap();
        Mock::given(method("GET"))
            .and(path(format!("/pix/v2/cob/{TXID}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(atual))
            .expect(1)
            .mount(&cenario.server)
            .await;
        nada_e_enviado(&cenario).await;
        let mut terminal = TerminalFalso::respondendo("n\n");
        let err = revisar(&cenario.context, &args, &mut terminal)
            .await
            .unwrap_err();
        assert!(matches!(err, CliError::Cancelado), "{err}");
        assert_eq!(terminal.perguntas, ["Remover a cobrança? [s/N] "]);
    }
}
