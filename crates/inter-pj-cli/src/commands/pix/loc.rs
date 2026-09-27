//! `inter-pj pix loc criar|listar|consultar|desvincular`: the locations,
//! addresses of the QR Codes of the charges.

use std::fmt::Write as _;

use chrono::{Local, TimeZone};
use inter_pj::Environment;
use inter_pj::pix::{FiltroLocs, LocationPix, TipoCob};
use serde_json::json;

use super::{paginacao, periodo};
use crate::chamada::chamada;
use crate::cli::{
    Formato, PixLocCommand, PixLocConsultarArgs, PixLocCriarArgs, PixLocDesvincularArgs,
    PixLocListarArgs,
};
use crate::commands::Context;
use crate::confirmacao::{Stdio, Terminal, confirmar, descrever_ambiente, pode_confirmar};
use crate::error::CliError;
use crate::output::{self, horario_em, horario_local, secao};
use crate::tabela::{Celula, Coluna, Tabela};

pub(super) async fn run(context: &Context, command: PixLocCommand) -> Result<(), CliError> {
    match command {
        PixLocCommand::Criar(args) => criar(context, &args).await,
        PixLocCommand::Listar(args) => listar(context, &args).await,
        PixLocCommand::Consultar(args) => consultar(context, &args).await,
        PixLocCommand::Desvincular(args) => desvincular(context, &args, &mut Stdio).await,
    }
}

/// `cobrança imediata`, `cobrança com vencimento`.
fn descrever_tipo(tipo: &TipoCob) -> &str {
    match tipo {
        TipoCob::Cob => "cobrança imediata",
        TipoCob::Cobv => "cobrança com vencimento",
        outro => outro.as_str(),
    }
}

/// `cob` or `cobv`: the command that creates the charge of a location.
fn comando(tipo: Option<&TipoCob>) -> &str {
    match tipo {
        Some(TipoCob::Cobv) => "cobv",
        _ => "cob",
    }
}

async fn criar(context: &Context, args: &PixLocCriarArgs) -> Result<(), CliError> {
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let tipo = TipoCob::from(args.tipo);
    let loc = client.pix().criar_loc(&tipo).await?;
    match context.formato() {
        Formato::Json => output::print_json(&loc),
        // `commands::run` refuses csv for this command.
        Formato::Texto | Formato::Csv => {
            let mut texto = format!("Location criada.\n\n{}", render(&loc));
            if let Some(id) = loc.id {
                let _ = write!(
                    texto,
                    "\n\nUse com: {} pix {} criar ... --loc {id}",
                    chamada(),
                    comando(loc.tipo_cob.as_ref().or(Some(&tipo)))
                );
            }
            output::print(&texto)
        }
    }
}

async fn listar(context: &Context, args: &PixLocListarArgs) -> Result<(), CliError> {
    let mut filtro = FiltroLocs::new(periodo(args.periodo)?);
    filtro.tx_id_presente = match (args.vinculo.com_cobranca, args.vinculo.sem_cobranca) {
        (true, _) => Some(true),
        (_, true) => Some(false),
        _ => None,
    };
    filtro.tipo_cob = args.tipo.map(TipoCob::from);
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let (locs, pagina) = match args.pagina {
        Some(numero) => {
            let mut pagina = client
                .pix()
                .listar_locs(&filtro, numero, args.itens_por_pagina)
                .await?;
            if context.formato() == Formato::Json {
                return output::print_json(&pagina);
            }
            let locs = std::mem::take(&mut pagina.loc);
            (locs, Some((numero, pagina)))
        }
        None => (client.pix().listar_todas_locs(&filtro).await?, None),
    };
    match context.formato() {
        Formato::Json => output::print_json(&json!({ "loc": locs })),
        Formato::Csv => output::print_csv(&csv(&locs), context.separador()),
        Formato::Texto => {
            context.warn_if_sandbox(&settings);
            let mut texto = format!("{}\n\n", titulo(&filtro));
            if locs.is_empty() {
                texto.push_str("Nenhuma location encontrada.");
            } else {
                texto.push_str(&tabela(&locs).texto_colorido());
                let vinculadas = locs.iter().filter(|loc| loc.txid.is_some()).count();
                let quantas = match locs.len() {
                    1 => "1 location".to_owned(),
                    n => format!("{n} locations"),
                };
                let _ = write!(texto, "\n\n{quantas} · {vinculadas} com cobrança");
            }
            if let Some((numero, pagina)) = pagina {
                texto.push_str(&paginacao(
                    numero,
                    &pagina.parametros,
                    locs.len(),
                    "locations",
                ));
            }
            output::print(&texto)
        }
    }
}

/// `Locations criadas de 01/09/2026 00:00 a 30/09/2026 23:59`, and the
/// filters.
fn titulo(filtro: &FiltroLocs) -> String {
    let formato = "%d/%m/%Y %H:%M";
    let mut texto = format!(
        "Locations criadas de {} a {}",
        filtro.periodo.inicio.format(formato),
        filtro.periodo.fim.format(formato)
    );
    let mut filtros = Vec::new();
    if let Some(tipo) = &filtro.tipo_cob {
        filtros.push(format!("para {}", descrever_tipo(tipo)));
    }
    match filtro.tx_id_presente {
        Some(true) => filtros.push("com cobrança".to_owned()),
        Some(false) => filtros.push("sem cobrança".to_owned()),
        None => {}
    }
    if !filtros.is_empty() {
        let _ = write!(texto, " ({})", filtros.join(", "));
    }
    texto
}

fn tabela(locs: &[LocationPix]) -> Tabela {
    let mut tabela = Tabela::new(vec![
        Coluna::texto("Criada em", ""),
        Coluna::valor("id", ""),
        Coluna::texto("Tipo", ""),
        Coluna::texto("txid", ""),
        Coluna::texto("Location", ""),
    ]);
    for loc in locs {
        tabela.linha(vec![
            Celula::texto(loc.criacao.as_deref().map(horario_local).as_deref()),
            Celula::texto(loc.id.map(|id| id.to_string()).as_deref()),
            Celula::texto(loc.tipo_cob.as_ref().map(TipoCob::as_str)),
            Celula::texto(loc.txid.as_deref()),
            Celula::texto(loc.location.as_deref()),
        ]);
    }
    tabela
}

/// The fields of the API.
fn csv(locs: &[LocationPix]) -> Tabela {
    let texto = |campo: &'static str| Coluna::texto(campo, campo);
    let mut tabela = Tabela::new(vec![
        texto("id"),
        texto("tipoCob"),
        texto("txid"),
        texto("criacao"),
        texto("location"),
    ]);
    for loc in locs {
        tabela.linha(vec![
            Celula::texto(loc.id.map(|id| id.to_string()).as_deref()),
            Celula::texto(loc.tipo_cob.as_ref().map(TipoCob::as_str)),
            Celula::texto(loc.txid.as_deref()),
            Celula::texto(loc.criacao.as_deref()),
            Celula::texto(loc.location.as_deref()),
        ]);
    }
    tabela
}

async fn consultar(context: &Context, args: &PixLocConsultarArgs) -> Result<(), CliError> {
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let loc = client.pix().consultar_loc(args.id).await?;
    match context.formato() {
        Formato::Json => output::print_json(&loc),
        // `commands::run` refuses csv for this command.
        Formato::Texto | Formato::Csv => {
            context.warn_if_sandbox(&settings);
            output::print(&render(&loc))
        }
    }
}

async fn desvincular(
    context: &Context,
    args: &PixLocDesvincularArgs,
    terminal: &mut dyn Terminal,
) -> Result<(), CliError> {
    // Nothing is looked up when no one could confirm.
    pode_confirmar(terminal, args.sim)?;
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let atual = client.pix().consultar_loc(args.id).await?;
    let Some(txid) = atual.txid.as_deref() else {
        return Err(CliError::Usage(format!(
            "a location {} não tem cobrança vinculada: não há o que desvincular",
            args.id
        )));
    };
    let ambiente = settings.ambiente.as_ref().map(|setting| setting.value);
    output::eprint(&resumo(&atual, txid, ambiente));
    confirmar(terminal, args.sim, "Desvincular a cobrança?")?;
    let loc = client.pix().desvincular_loc(args.id).await?;
    match context.formato() {
        Formato::Json => output::print_json(&loc),
        // `commands::run` refuses csv for this command.
        Formato::Texto | Formato::Csv => output::print(&format!(
            "Cobrança {txid} desvinculada: a location está livre.\n\n{}",
            render(&loc)
        )),
    }
}

/// The location and the charge about to lose it.
fn resumo(loc: &LocationPix, txid: &str, ambiente: Option<Environment>) -> String {
    let mut linhas = vec![("Ambiente", descrever_ambiente(ambiente))];
    if let Some(location) = &loc.location {
        linhas.push(("Location", location.clone()));
    }
    linhas.push(("Cobrança", txid.to_owned()));
    let id = loc.id.map(|id| id.to_string()).unwrap_or_default();
    let mut texto = secao(&format!("Location {id} a desvincular"), &linhas);
    let _ = write!(
        texto,
        "\naviso: o QR Code desta location deixa de levar à cobrança {txid}"
    );
    texto
}

/// A location, with the time in the local time zone.
fn render(loc: &LocationPix) -> String {
    render_em(loc, &Local)
}

fn render_em<Tz: TimeZone>(loc: &LocationPix, fuso: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let mut linhas = Vec::new();
    if let Some(tipo) = &loc.tipo_cob {
        linhas.push(("Tipo", descrever_tipo(tipo).to_owned()));
    }
    if let Some(criacao) = &loc.criacao {
        linhas.push(("Criada em", horario_em(criacao, fuso)));
    }
    if let Some(location) = &loc.location {
        linhas.push(("Location", location.clone()));
    }
    linhas.push((
        "Cobrança",
        loc.txid.clone().unwrap_or_else(|| "nenhuma".to_owned()),
    ));
    let id = loc.id.map(|id| id.to_string()).unwrap_or_default();
    secao(format!("Location {id}").trim(), &linhas)
}

#[cfg(test)]
mod tests {
    use chrono::FixedOffset;
    use serde_json::Value;
    use wiremock::matchers::{any, method, path};
    use wiremock::{Mock, ResponseTemplate};

    use super::*;
    use crate::cli::{Command, PixCommand};
    use crate::commands::testes;
    use crate::confirmacao::testes::TerminalFalso;

    fn loc(txid: Option<&str>) -> LocationPix {
        serde_json::from_value(json!({
            "id": 790,
            "location": "pix.example.com/qr/v2/cobv/5b7e4c1a5d3f4a2b9c8d7e6f5a4b3c2d",
            "tipoCob": "cobv",
            "criacao": "2026-09-24T13:10:00.000Z",
            "txid": txid
        }))
        .unwrap()
    }

    #[test]
    fn a_location_in_detail() {
        let brasilia = FixedOffset::west_opt(3 * 3600).unwrap();
        assert_eq!(
            render_em(&loc(Some("cobvexemplo0000000000000000001")), &brasilia),
            "\
Location 790
  Tipo       cobrança com vencimento
  Criada em  24/09/2026 10:10:00
  Location   pix.example.com/qr/v2/cobv/5b7e4c1a5d3f4a2b9c8d7e6f5a4b3c2d
  Cobrança   cobvexemplo0000000000000000001"
        );
        assert!(render_em(&loc(None), &brasilia).ends_with("Cobrança   nenhuma"));
        assert_eq!(comando(Some(&TipoCob::Cobv)), "cobv");
        assert_eq!(comando(None), "cob");
    }

    #[test]
    fn listings_use_the_api_names_in_csv() {
        let locs = [loc(Some("cobvexemplo0000000000000000001")), loc(None)];
        let csv = csv(&locs).csv(crate::tabela::Separador::Virgula);
        assert!(
            csv.starts_with("id,tipoCob,txid,criacao,location\r\n790,cobv,cobvexemplo0000000000000000001,2026-09-24T13:10:00.000Z,pix.example.com/"),
            "{csv}"
        );
    }

    // --- the commands against a mock API ------------------------------------

    async fn cenario(args: &[&str]) -> (testes::Cenario, PixLocCommand) {
        let mut todos = vec!["pix", "loc"];
        todos.extend_from_slice(args);
        match testes::cenario(&todos, "payloadlocation.write payloadlocation.read").await {
            (cenario, Command::Pix(PixCommand::Loc(comando))) => (cenario, comando),
            (_, outro) => panic!("{outro:?}"),
        }
    }

    async fn apenas_a_consulta(cenario: &testes::Cenario, txid: Option<&str>) {
        let atual: Value = serde_json::to_value(loc(txid)).unwrap();
        Mock::given(method("GET"))
            .and(path("/pix/v2/loc/790"))
            .respond_with(ResponseTemplate::new(200).set_body_json(atual))
            .expect(1)
            .mount(&cenario.server)
            .await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&cenario.server)
            .await;
    }

    fn desvincular_args(comando: PixLocCommand) -> PixLocDesvincularArgs {
        match comando {
            PixLocCommand::Desvincular(args) => args,
            outro => panic!("{outro:?}"),
        }
    }

    #[tokio::test]
    async fn a_declined_unlink_only_looks_the_location_up() {
        let (cenario, comando) = cenario(&["desvincular", "790"]).await;
        apenas_a_consulta(&cenario, Some("cobvexemplo0000000000000000001")).await;
        let mut terminal = TerminalFalso::respondendo("n\n");
        let err = desvincular(&cenario.context, &desvincular_args(comando), &mut terminal)
            .await
            .unwrap_err();
        assert!(matches!(err, CliError::Cancelado), "{err}");
        assert_eq!(terminal.perguntas, ["Desvincular a cobrança? [s/N] "]);
    }

    #[tokio::test]
    async fn a_free_location_has_nothing_to_unlink() {
        let (cenario, comando) = cenario(&["desvincular", "790", "--sim"]).await;
        apenas_a_consulta(&cenario, None).await;
        let err = desvincular(
            &cenario.context,
            &desvincular_args(comando),
            &mut TerminalFalso::respondendo(""),
        )
        .await
        .unwrap_err();
        assert_eq!(
            err.to_string(),
            "a location 790 não tem cobrança vinculada: não há o que desvincular"
        );
    }
}
