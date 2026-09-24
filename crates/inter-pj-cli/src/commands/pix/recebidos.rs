//! `inter-pj pix recebidos listar|consultar`: the Pix received by the
//! account, and their refunds.

use std::fmt::Write as _;

use chrono::{Local, TimeZone};
use inter_pj::pix::{FiltroPixRecebidos, PixRecebido};
use rust_decimal::Decimal;
use serde_json::{Value, json};

use super::{descrever_status_devolucao, devolvido, disponivel, em_devolucao, paginacao, periodo};
use crate::cli::{Formato, PixRecebidoConsultarArgs, PixRecebidosCommand, PixRecebidosListarArgs};
use crate::commands::Context;
use crate::error::CliError;
use crate::output::{self, horario_em, horario_local, secao};
use crate::tabela::{Celula, Coluna, Tabela};

/// Longest reason of a refund shown in a table.
const LARGURA_MOTIVO: usize = 40;

pub(super) async fn run(context: &Context, command: PixRecebidosCommand) -> Result<(), CliError> {
    match command {
        PixRecebidosCommand::Listar(args) => listar(context, &args).await,
        PixRecebidosCommand::Consultar(args) => consultar(context, &args).await,
    }
}

/// `Some(true)` for `--com-...`, `Some(false)` for `--sem-...`.
fn com_ou_sem(com: bool, sem: bool) -> Option<bool> {
    match (com, sem) {
        (true, _) => Some(true),
        (_, true) => Some(false),
        _ => None,
    }
}

async fn listar(context: &Context, args: &PixRecebidosListarArgs) -> Result<(), CliError> {
    let mut filtro = FiltroPixRecebidos::new(periodo(args.periodo)?);
    filtro.txid.clone_from(&args.txid);
    filtro.tx_id_presente = com_ou_sem(args.cobranca.com_cobranca, args.cobranca.sem_cobranca);
    filtro.devolucao_presente =
        com_ou_sem(args.devolucao.com_devolucao, args.devolucao.sem_devolucao);
    filtro.devedor.clone_from(&args.documento);
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let (pix, pagina) = match args.pagina {
        Some(numero) => {
            let mut pagina = client
                .pix()
                .listar_pix_recebidos(&filtro, numero, args.itens_por_pagina)
                .await?;
            if context.formato() == Formato::Json {
                return output::print_json(&pagina);
            }
            let pix = std::mem::take(&mut pagina.pix);
            (pix, Some((numero, pagina)))
        }
        None => (
            client.pix().listar_todos_pix_recebidos(&filtro).await?,
            None,
        ),
    };
    match context.formato() {
        Formato::Json => output::print_json(&json!({ "pix": pix })),
        Formato::Csv => output::print_raw(&csv(&pix).csv(context.separador())),
        Formato::Texto => {
            context.warn_if_sandbox(&settings);
            let mut texto = format!("{}\n\n", titulo(&filtro));
            if pix.is_empty() {
                texto.push_str("Nenhum Pix encontrado.");
            } else {
                texto.push_str(&tabela(&pix).texto());
                let _ = write!(texto, "\n\n{}", totais(&pix));
            }
            if let Some((numero, pagina)) = pagina {
                texto.push_str(&paginacao(numero, &pagina.parametros, pix.len(), "Pix"));
            }
            output::print(&texto)
        }
    }
}

/// `Pix recebidos de 01/09/2026 00:00 a 30/09/2026 23:59`, and the filters.
fn titulo(filtro: &FiltroPixRecebidos) -> String {
    let formato = "%d/%m/%Y %H:%M";
    let mut texto = format!(
        "Pix recebidos de {} a {}",
        filtro.periodo.inicio.format(formato),
        filtro.periodo.fim.format(formato)
    );
    let mut filtros = Vec::new();
    if let Some(txid) = &filtro.txid {
        filtros.push(format!("cobrança {txid}"));
    }
    match filtro.tx_id_presente {
        Some(true) => filtros.push("de cobranças".to_owned()),
        Some(false) => filtros.push("sem cobrança".to_owned()),
        None => {}
    }
    match filtro.devolucao_presente {
        Some(true) => filtros.push("com devolução".to_owned()),
        Some(false) => filtros.push("sem devolução".to_owned()),
        None => {}
    }
    if let Some(documento) = &filtro.devedor {
        filtros.push(format!("pagador {}", documento.formatado()));
    }
    if !filtros.is_empty() {
        let _ = write!(texto, " ({})", filtros.join(", "));
    }
    texto
}

fn tabela(pix: &[PixRecebido]) -> Tabela {
    let mut tabela = Tabela::new(vec![
        Coluna::texto("Horário", ""),
        Coluna::valor("Valor", ""),
        Coluna::valor("Devolvido", ""),
        Coluna::texto("endToEndId", ""),
        Coluna::texto("txid", ""),
    ]);
    for recebido in pix {
        let devolvido = devolvido(recebido);
        tabela.linha(vec![
            Celula::texto(recebido.horario.as_deref().map(horario_local).as_deref()),
            Celula::dinheiro(recebido.valor),
            Celula::dinheiro((!devolvido.is_zero()).then_some(devolvido)),
            Celula::texto(recebido.end_to_end_id.as_deref()),
            Celula::texto(recebido.txid.as_deref()),
        ]);
    }
    tabela
}

/// `3 Pix · R$ 450,00 · devolvidos R$ 50,00`.
fn totais(pix: &[PixRecebido]) -> String {
    let total: Decimal = pix.iter().filter_map(|recebido| recebido.valor).sum();
    let devolvidos: Decimal = pix.iter().map(devolvido).sum();
    let mut texto = format!("{} Pix · {}", pix.len(), output::brl(total));
    if !devolvidos.is_zero() {
        let _ = write!(texto, " · devolvidos {}", output::brl(devolvidos));
    }
    texto
}

/// The fields of the API, plus `valorDevolvido`: what was refunded.
fn csv(pix: &[PixRecebido]) -> Tabela {
    let texto = |campo: &'static str| Coluna::texto(campo, campo);
    let mut tabela = Tabela::new(vec![
        texto("endToEndId"),
        texto("txid"),
        Coluna::valor("valor", "valor"),
        texto("horario"),
        texto("chave"),
        texto("infoPagador"),
        Coluna::valor("valorDevolvido", "valorDevolvido"),
    ]);
    for recebido in pix {
        let mut devolvido = devolvido(recebido);
        devolvido.rescale(2);
        tabela.linha(vec![
            Celula::texto(recebido.end_to_end_id.as_deref()),
            Celula::texto(recebido.txid.as_deref()),
            Celula::dinheiro(recebido.valor),
            Celula::texto(recebido.horario.as_deref()),
            Celula::texto(recebido.chave.as_deref()),
            Celula::texto(recebido.info_pagador.as_deref()),
            Celula::dinheiro(Some(devolvido)),
        ]);
    }
    tabela
}

async fn consultar(context: &Context, args: &PixRecebidoConsultarArgs) -> Result<(), CliError> {
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let pix = client.pix().consultar_pix_recebido(&args.e2e).await?;
    match context.formato() {
        Formato::Json => output::print_json(&pix),
        // `commands::run` refuses csv for this command.
        Formato::Texto | Formato::Csv => {
            context.warn_if_sandbox(&settings);
            output::print(&render_pix(&pix, &args.e2e))
        }
    }
}

/// A Pix received and its refunds, with the times in the local time zone.
fn render_pix(pix: &PixRecebido, e2e: &str) -> String {
    render_pix_em(pix, e2e, &Local)
}

fn render_pix_em<Tz: TimeZone>(pix: &PixRecebido, e2e: &str, fuso: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let mut linhas = Vec::new();
    if let Some(valor) = pix.valor {
        linhas.push(("Valor", output::brl(valor)));
    }
    if let Some(horario) = &pix.horario {
        linhas.push(("Recebido em", horario_em(horario, fuso)));
    }
    if let Some(componentes) = pix.componentes_valor.as_ref().and_then(componentes) {
        linhas.push(("Componentes", componentes));
    }
    let devolvido = devolvido(pix);
    if !devolvido.is_zero() {
        linhas.push(("Devolvido", output::brl(devolvido)));
    }
    let em_devolucao = em_devolucao(pix);
    if !em_devolucao.is_zero() {
        linhas.push(("Em devolução", output::brl(em_devolucao)));
    }
    let restante = disponivel(pix);
    if let Some(restante) = restante
        && restante != pix.valor.unwrap_or_default()
    {
        linhas.push(("Pode devolver", output::brl(restante)));
    }
    if let Some(txid) = &pix.txid {
        linhas.push(("txid", txid.clone()));
    }
    if let Some(chave) = &pix.chave {
        linhas.push(("Chave", chave.clone()));
    }
    if let Some(mensagem) = &pix.info_pagador {
        linhas.push(("Mensagem", mensagem.clone()));
    }
    let id = pix.end_to_end_id.as_deref().unwrap_or(e2e);
    let mut texto = secao(&format!("Pix recebido {id}"), &linhas);
    if !pix.devolucoes.is_empty() {
        let _ = write!(
            texto,
            "\n\nDevoluções\n{}",
            tabela_devolucoes(pix, fuso).texto()
        );
    }
    if restante.is_none_or(|restante| restante > Decimal::ZERO) {
        let _ = write!(
            texto,
            "\n\nPara devolver: inter-pj pix devolucao solicitar {id} --valor VALOR (ou --tudo)"
        );
    }
    texto
}

/// The refunds of a Pix; the reason, only when some refund has one.
fn tabela_devolucoes<Tz: TimeZone>(pix: &PixRecebido, fuso: &Tz) -> Tabela
where
    Tz::Offset: std::fmt::Display,
{
    let com_motivo = pix
        .devolucoes
        .iter()
        .any(|devolucao| devolucao.motivo.is_some());
    let mut colunas = vec![
        Coluna::texto("id", ""),
        Coluna::texto("Status", ""),
        Coluna::valor("Valor", ""),
        Coluna::texto("Solicitada em", ""),
    ];
    if com_motivo {
        colunas.push(Coluna::texto("Motivo", "").no_maximo(LARGURA_MOTIVO));
    }
    let mut tabela = Tabela::new(colunas);
    for devolucao in &pix.devolucoes {
        let solicitacao = devolucao
            .horario
            .as_ref()
            .and_then(|horario| horario.solicitacao.as_deref())
            .map(|horario| horario_em(horario, fuso));
        let mut celulas = vec![
            Celula::texto(devolucao.id.as_deref()),
            Celula::texto(devolucao.status.as_ref().map(descrever_status_devolucao)),
            Celula::dinheiro(devolucao.valor),
            Celula::texto(solicitacao.as_deref()),
        ];
        if com_motivo {
            celulas.push(Celula::texto(devolucao.motivo.as_deref()));
        }
        tabela.linha(celulas);
    }
    tabela
}

/// `original R$ 10,00 · saque R$ 50,00`: the parts of a Pix Saque or Troco,
/// as far as they can be read.
fn componentes(valor: &Value) -> Option<String> {
    let partes: Vec<String> = valor
        .as_object()?
        .iter()
        .filter_map(|(nome, componente)| {
            let valor = match componente.get("valor")? {
                Value::String(texto) => texto.parse::<Decimal>().ok()?,
                Value::Number(numero) => numero.to_string().parse::<Decimal>().ok()?,
                _ => return None,
            };
            Some(format!("{nome} {}", output::brl(valor)))
        })
        .collect();
    (!partes.is_empty()).then(|| partes.join(" · "))
}

#[cfg(test)]
mod tests {
    use chrono::FixedOffset;

    use super::*;

    const E2E: &str = "E00416968202609181241abcdEFGH123";

    fn pix(devolucoes: &Value) -> PixRecebido {
        serde_json::from_value(json!({
            "endToEndId": E2E,
            "txid": "a1b2c3d4e5f60718293a4b5c6d7e8f90",
            "valor": "300.00",
            "chave": "pix@empresa.example",
            "horario": "2026-09-18T12:41:07.000Z",
            "infoPagador": "Pedido 123",
            "devolucoes": devolucoes
        }))
        .unwrap()
    }

    fn brasilia() -> FixedOffset {
        FixedOffset::west_opt(3 * 3600).unwrap()
    }

    #[test]
    fn a_pix_and_its_refunds() {
        let recebido = pix(&json!([
            {"id": "D1", "rtrId": "D00416968202609181300abcdefghijk", "valor": "50.00", "status": "DEVOLVIDO",
             "horario": {"solicitacao": "2026-09-18T15:00:00Z", "liquidacao": "2026-09-18T15:00:05Z"}},
            {"id": "D2", "valor": "20.00", "status": "EM_PROCESSAMENTO", "horario": {"solicitacao": "2026-09-19T15:00:00Z"}},
            {"id": "D3", "valor": "300.00", "status": "NAO_REALIZADO", "motivo": "Saldo insuficiente"}
        ]));
        assert_eq!(
            render_pix_em(&recebido, E2E, &brasilia()),
            format!(
                "\
Pix recebido {E2E}
  Valor          R$ 300,00
  Recebido em    18/09/2026 09:41:07
  Devolvido      R$ 50,00
  Em devolução   R$ 20,00
  Pode devolver  R$ 230,00
  txid           a1b2c3d4e5f60718293a4b5c6d7e8f90
  Chave          pix@empresa.example
  Mensagem       Pedido 123

Devoluções
id  Status                Valor  Solicitada em        Motivo
D1  devolvida          R$ 50,00  18/09/2026 12:00:00
D2  em processamento   R$ 20,00  19/09/2026 12:00:00
D3  não realizada     R$ 300,00                       Saldo insuficiente

Para devolver: inter-pj pix devolucao solicitar {E2E} --valor VALOR (ou --tudo)"
            )
        );
        let devolvido_por_inteiro =
            pix(&json!([{"id": "D1", "valor": "300.00", "status": "DEVOLVIDO"}]));
        let texto = render_pix_em(&devolvido_por_inteiro, E2E, &brasilia());
        assert!(texto.contains("Pode devolver  R$ 0,00"), "{texto}");
        assert!(!texto.contains("Para devolver"), "{texto}");
    }

    #[test]
    fn the_parts_of_a_pix_saque() {
        assert_eq!(
            componentes(
                &json!({"original": {"valor": "0.00"}, "saque": {"valor": 50, "modalidadeAgente": "AGTEC"}})
            ),
            Some("original R$ 0,00 · saque R$ 50,00".to_owned())
        );
        assert_eq!(componentes(&json!({"saque": {}})), None);
        assert_eq!(componentes(&json!("texto")), None);
    }

    #[test]
    fn listings_add_up() {
        let lista = [
            pix(&json!([{"id": "D1", "valor": "50.00", "status": "DEVOLVIDO"}])),
            pix(&json!([])),
        ];
        assert_eq!(totais(&lista), "2 Pix · R$ 600,00 · devolvidos R$ 50,00");
        let csv = csv(&lista).csv(crate::tabela::Separador::Virgula);
        assert!(
            csv.starts_with("endToEndId,txid,valor,horario,chave,infoPagador,valorDevolvido\r\n"),
            "{csv}"
        );
        assert!(
            csv.contains(&format!("{E2E},a1b2c3d4e5f60718293a4b5c6d7e8f90,300.00,2026-09-18T12:41:07.000Z,pix@empresa.example,Pedido 123,50.00")),
            "{csv}"
        );
        assert!(csv.ends_with(",Pedido 123,0.00\r\n"), "{csv}");
    }
}
