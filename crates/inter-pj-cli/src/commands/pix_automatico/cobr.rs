//! `inter-pj pix-automatico cobr criar|listar|consultar|cancelar|retentativa`:
//! the recurring charges, one per cycle of an approved recurrence, which the
//! payer's bank debits on the due date.

use std::fmt::Write as _;

use chrono::{Days, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use inter_pj::cobranca::Uf;
use inter_pj::pix::Txid;
use inter_pj::pix_automatico::{
    CobR, CobRSolicitada, ContaRecebedor, DevedorCobR, FiltroCobsR, PoliticaRetentativa, Rec,
    RecebedorCobR, StatusCobR, StatusRec, StatusTentativa, TentativaCobR, TipoContaRecebedor,
    TipoTentativa,
};
use inter_pj::{Environment, Error as InterError, endpoint};
use rust_decimal::Decimal;
use serde_json::json;

use super::{descrever_politica, descrever_status, encerramento};
use crate::cli::{
    CobrCancelarArgs, CobrCommand, CobrConsultarArgs, CobrCriarArgs, CobrListarArgs,
    CobrRetentativaArgs, ContatoDevedorArgs, Formato,
};
use crate::commands::pix::{documento, endereco, incerta, pagina, periodo, pessoa, tabela_pix};
use crate::commands::{BRASILIA, Context, hoje, simulacao};
use crate::confirmacao::{Stdio, Terminal, confirmar, descrever_ambiente, pode_confirmar};
use crate::cores::Tom;
use crate::error::{CliError, resultado_incerto};
use crate::output::{self, data_br, horario_em, limpo, parse_data, secao};
use crate::tabela::{Celula, Coluna, Tabela};
use crate::valor::por_extenso;

/// The new attempts the policy `PERMITE_3R_7D` allows.
const MAX_RETENTATIVAS: usize = 3;

/// Days after the settlement planned in which the new attempts can be.
const DIAS_DE_RETENTATIVA: u64 = 7;

/// Hour of the day before the settlement until which a charge can be
/// cancelled, by the Central Bank's rules.
const HORA_LIMITE_CANCELAMENTO: u32 = 22;

pub(super) async fn run(context: &Context, command: CobrCommand) -> Result<(), CliError> {
    match command {
        CobrCommand::Criar(args) => criar(context, &args, &mut Stdio).await,
        CobrCommand::Listar(args) => listar(context, &args).await,
        CobrCommand::Consultar(args) => consultar(context, &args).await,
        CobrCommand::Cancelar(args) => cancelar(context, &args, &mut Stdio).await,
        CobrCommand::Retentativa(args) => retentativa(context, &args, &mut Stdio).await,
    }
}

async fn criar(
    context: &Context,
    args: &CobrCriarArgs,
    terminal: &mut dyn Terminal,
) -> Result<(), CliError> {
    let settings = context.settings()?;
    let conta_configurada = settings
        .conta_corrente
        .as_ref()
        .map(|setting| setting.value.as_str());
    let cobr = das_opcoes(args, conta_configurada)?;
    vencimento_valido(cobr.vencimento, hoje())?;
    let txid = args.txid.clone().unwrap_or_else(Txid::novo);
    let ambiente = settings.ambiente.as_ref().map(|setting| setting.value);
    if args.simular {
        output::eprint(&resumo(&cobr, &txid, None, ambiente));
        return simulacao::mostrar_em(
            context,
            &settings,
            endpoint::pix_automatico::CRIAR_COBR,
            &[("txid", txid.as_str())],
            &[],
            &cobr,
        );
    }
    // Nothing is looked up when no one could confirm.
    pode_confirmar(terminal, args.sim)?;
    let client = context.client(&settings)?;
    let rec = client
        .pix_automatico()
        .consultar_rec(&cobr.id_rec, None)
        .await?;
    aceita_cobrancas(&rec)?;
    output::eprint(&resumo(&cobr, &txid, Some(&rec), ambiente));
    confirmar(terminal, args.sim, "Criar a cobrança recorrente?")?;

    let criada = client
        .pix_automatico()
        .criar_cobr(&txid, &cobr)
        .await
        .map_err(|err| incerta(err, "pix-automatico cobr", &txid))?;
    match context.formato() {
        Formato::Json => output::print_json(&criada),
        // `commands::run` refuses csv for this command.
        Formato::Texto | Formato::Csv => output::print(&format!(
            "Cobrança recorrente criada: o banco do pagador agenda o débito para o vencimento.\n\n{}\n\nAcompanhe com: inter-pj pix-automatico cobr consultar {txid}",
            render_cobr(&criada)
        )),
    }
}

/// The charge of the options, checked as the API would; without `--conta`,
/// into the account of `--conta-corrente` (or the configuration).
fn das_opcoes(
    args: &CobrCriarArgs,
    conta_configurada: Option<&str>,
) -> Result<CobRSolicitada, CliError> {
    let conta = args
        .conta
        .as_deref()
        .or(conta_configurada)
        .map(str::trim)
        .filter(|conta| !conta.is_empty())
        .ok_or_else(|| {
            CliError::Usage(
                "informe --conta, a conta que recebe, com o dígito verificador (ou --conta-corrente, que pode vir da configuração)"
                    .to_owned(),
            )
        })?;
    let mut recebedor = ContaRecebedor::new(conta, args.tipo_conta.into());
    recebedor.agencia = args
        .agencia
        .as_deref()
        .map(|agencia| agencia.trim().to_owned());
    let mut cobr = CobRSolicitada::new(args.rec.clone(), args.vencimento, args.valor, recebedor);
    cobr.ajuste_dia_util = !args.sem_ajuste_dia_util;
    cobr.info_adicional.clone_from(&args.info);
    cobr.devedor = devedor(&args.devedor);
    cobr.validar()
        .map_err(|err| CliError::Usage(format!("{}: {err}", opcao(err.campo()))))?;
    Ok(cobr)
}

/// The payer's e-mail and address of the options, when any is given.
fn devedor(args: &ContatoDevedorArgs) -> Option<DevedorCobR> {
    let mut devedor = DevedorCobR::default();
    devedor.email.clone_from(&args.devedor_email);
    devedor.logradouro.clone_from(&args.devedor_endereco);
    devedor.cidade.clone_from(&args.devedor_cidade);
    devedor.uf = args.devedor_uf;
    devedor.cep.clone_from(&args.devedor_cep);
    (devedor != DevedorCobR::default()).then_some(devedor)
}

/// The option of a field of the API, for the messages.
fn opcao(campo: &str) -> &str {
    match campo {
        "valor.original" => "--valor",
        "recebedor.conta" => "--conta",
        "recebedor.tipoConta" => "--tipo-conta",
        "recebedor.agencia" => "--agencia",
        "infoAdicional" => "--info",
        "devedor.email" => "--devedor-email",
        "devedor.logradouro" => "--devedor-endereco",
        "devedor.cidade" => "--devedor-cidade",
        "devedor.cep" => "--devedor-cep",
        outro => outro,
    }
}

fn vencimento_valido(vencimento: NaiveDate, hoje: NaiveDate) -> Result<(), CliError> {
    if vencimento < hoje {
        return Err(CliError::Usage(format!(
            "o vencimento ({}) já passou",
            vencimento.format("%d/%m/%Y")
        )));
    }
    Ok(())
}

/// Refuses a recurrence that takes no charges: only one the payer approved
/// does.
fn aceita_cobrancas(rec: &Rec) -> Result<(), CliError> {
    match &rec.status {
        Some(StatusRec::Aprovada | StatusRec::Outro(_)) | None => Ok(()),
        Some(status) => Err(CliError::Usage(format!(
            "a recorrência está {}: só uma recorrência aprovada pelo pagador aceita cobranças",
            descrever_status(status)
        ))),
    }
}

/// The charge about to be created, with the recurrence it belongs to, and
/// what deserves a warning.
fn resumo(
    cobr: &CobRSolicitada,
    txid: &Txid,
    rec: Option<&Rec>,
    ambiente: Option<Environment>,
) -> String {
    let mut linhas = vec![
        ("Ambiente", descrever_ambiente(ambiente)),
        ("Recorrência", cobr.id_rec.to_string()),
    ];
    if let Some(vinculo) = rec.and_then(|rec| rec.vinculo.as_ref()) {
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
    let extenso = por_extenso(cobr.valor)
        .map(|extenso| format!(" ({extenso})"))
        .unwrap_or_default();
    linhas.push(("Valor", format!("{}{extenso}", output::brl(cobr.valor))));
    linhas.push((
        "Vencimento",
        vencimento(
            &cobr.vencimento.format("%d/%m/%Y").to_string(),
            Some(cobr.ajuste_dia_util),
        ),
    ));
    let recebedor = &cobr.recebedor;
    linhas.extend(
        conta(
            Some(&recebedor.tipo_conta),
            Some(&recebedor.conta),
            recebedor.agencia.as_deref(),
        )
        .map(|conta| ("Conta que recebe", conta)),
    );
    if let Some(info) = &cobr.info_adicional {
        linhas.push(("Informação", info.clone()));
    }
    if let Some(devedor) = &cobr.devedor {
        linhas.extend(
            endereco(
                devedor.logradouro.as_deref(),
                devedor.cidade.as_deref(),
                devedor.uf.map(Uf::as_str),
                devedor.cep.as_deref(),
            )
            .map(|endereco| ("Endereço", endereco)),
        );
        linhas.extend(devedor.email.clone().map(|email| ("E-mail", email)));
    }
    if let Some(politica) = rec.and_then(|rec| rec.politica_retentativa.as_ref()) {
        linhas.push(("Retentativas", descrever_politica(politica).to_owned()));
    }
    linhas.push(("txid", txid.to_string()));
    let mut texto = String::new();
    if ambiente.is_some_and(Environment::is_production) {
        texto.push_str("*** PRODUÇÃO: o débito na conta do pagador é de verdade ***\n");
    }
    texto.push_str(&secao("Cobrança recorrente a criar", &linhas));
    for aviso in rec.map(|rec| avisos(cobr, rec)).unwrap_or_default() {
        let _ = write!(texto, "\naviso: {aviso}");
    }
    texto
}

/// What of the charge does not match its recurrence: the fixed amount and
/// the period.
fn avisos(cobr: &CobRSolicitada, rec: &Rec) -> Vec<String> {
    let mut avisos = Vec::new();
    if let Some(fixo) = rec.valor.as_ref().and_then(|valor| valor.valor_rec)
        && fixo != cobr.valor
    {
        avisos.push(format!(
            "a recorrência tem valor fixo de {}, e esta cobrança é de {}",
            output::brl(fixo),
            output::brl(cobr.valor)
        ));
    }
    let calendario = rec.calendario.as_ref();
    let data = |data: Option<&String>| data.and_then(|data| parse_data(data));
    if let Some(inicio) = data(calendario.and_then(|c| c.data_inicial.as_ref()))
        && cobr.vencimento < inicio
    {
        avisos.push(format!(
            "o vencimento é anterior ao primeiro pagamento da recorrência ({})",
            inicio.format("%d/%m/%Y")
        ));
    }
    if let Some(fim) = data(calendario.and_then(|c| c.data_final.as_ref()))
        && cobr.vencimento > fim
    {
        avisos.push(format!(
            "o vencimento é posterior ao fim da recorrência ({})",
            fim.format("%d/%m/%Y")
        ));
    }
    avisos
}

/// `10/10/2026, ou o próximo dia útil`.
fn vencimento(data: &str, ajuste_dia_util: Option<bool>) -> String {
    match ajuste_dia_util {
        Some(true) => format!("{data}, ou o próximo dia útil"),
        Some(false) => format!("{data}, mesmo que não seja dia útil"),
        None => data.to_owned(),
    }
}

/// `conta corrente 1234567, agência 0001`.
fn conta(
    tipo: Option<&TipoContaRecebedor>,
    numero: Option<&str>,
    agencia: Option<&str>,
) -> Option<String> {
    let tipo = tipo.map_or("conta", |tipo| match tipo {
        TipoContaRecebedor::Corrente => "conta corrente",
        TipoContaRecebedor::Poupanca => "conta poupança",
        TipoContaRecebedor::Pagamento => "conta de pagamento",
        outro => outro.as_str(),
    });
    let mut partes: Vec<String> = numero
        .map(|numero| format!("{tipo} {numero}"))
        .into_iter()
        .collect();
    partes.extend(agencia.map(|agencia| format!("agência {agencia}")));
    (!partes.is_empty()).then(|| partes.join(", "))
}

/// `Empresa Exemplo Ltda (12.345.678/0001-95), conta corrente 1234567,
/// agência 0001`.
fn recebedor(recebedor: &RecebedorCobR) -> Option<String> {
    let quem = match (recebedor.nome.as_deref(), recebedor.cnpj.as_deref()) {
        (Some(nome), Some(cnpj)) => Some(format!("{nome} ({})", documento(cnpj))),
        (Some(nome), None) => Some(nome.to_owned()),
        (None, Some(cnpj)) => Some(documento(cnpj)),
        (None, None) => None,
    };
    let conta = conta(
        recebedor.tipo_conta.as_ref(),
        recebedor.conta.as_deref(),
        recebedor.agencia.as_deref(),
    );
    match (quem, conta) {
        (Some(quem), Some(conta)) => Some(format!("{quem}, {conta}")),
        (Some(texto), None) | (None, Some(texto)) => Some(texto),
        (None, None) => None,
    }
}

/// A status in words: `ATIVA` -> `ativa (débito agendado)`.
fn descrever(status: &StatusCobR) -> &str {
    match status {
        StatusCobR::Criada => "criada (aguarda o banco do pagador)",
        StatusCobR::Ativa => "ativa (débito agendado)",
        StatusCobR::Concluida => "concluída (paga)",
        StatusCobR::Expirada => "expirada sem pagamento",
        StatusCobR::Rejeitada => "rejeitada pelo banco do pagador",
        StatusCobR::Cancelada => "cancelada",
        outro => outro.as_str(),
    }
}

/// A status in one word: `concluída`.
fn curto(status: &StatusCobR) -> &str {
    match status {
        StatusCobR::Criada => "criada",
        StatusCobR::Ativa => "ativa",
        StatusCobR::Concluida => "concluída",
        StatusCobR::Expirada => "expirada",
        StatusCobR::Rejeitada => "rejeitada",
        StatusCobR::Cancelada => "cancelada",
        outro => outro.as_str(),
    }
}

/// The status of a recurring charge in a table: awaiting the debit, paid
/// or ended without it.
fn celula_status(status: Option<&StatusCobR>) -> Celula {
    let tom = status.and_then(|status| match status {
        StatusCobR::Criada | StatusCobR::Ativa => Some(Tom::Pendente),
        StatusCobR::Concluida => Some(Tom::Positivo),
        StatusCobR::Expirada | StatusCobR::Rejeitada | StatusCobR::Cancelada => Some(Tom::Negativo),
        _ => None,
    });
    Celula::situacao(status.map(curto), tom)
}

/// Whether nothing changes a charge any more.
fn encerrada(status: &StatusCobR) -> bool {
    matches!(
        status,
        StatusCobR::Concluida
            | StatusCobR::Expirada
            | StatusCobR::Rejeitada
            | StatusCobR::Cancelada
    )
}

async fn consultar(context: &Context, args: &CobrConsultarArgs) -> Result<(), CliError> {
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let cobr = client.pix_automatico().consultar_cobr(&args.txid).await?;
    match context.formato() {
        Formato::Json => output::print_json(&cobr),
        // `commands::run` refuses csv for this command.
        Formato::Texto | Formato::Csv => {
            context.warn_if_sandbox(&settings);
            output::print(&render_cobr(&cobr))
        }
    }
}

/// A recurring charge in detail, with the times in the local time zone.
fn render_cobr(cobr: &CobR) -> String {
    render_cobr_em(cobr, &Local)
}

fn render_cobr_em<Tz: TimeZone>(cobr: &CobR, fuso: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let mut linhas = resumo_da_cobranca(cobr);
    if let Some(criacao) = cobr
        .calendario
        .as_ref()
        .and_then(|calendario| calendario.criacao.as_deref())
    {
        linhas.push(("Criada em", data_br(criacao)));
    }
    if let Some(politica) = &cobr.politica_retentativa {
        linhas.push(("Retentativas", descrever_politica(politica).to_owned()));
    }
    if let Some(recebedor) = cobr.recebedor.as_ref().and_then(recebedor) {
        linhas.push(("Recebedor", recebedor));
    }
    if let Some(devedor) = &cobr.devedor {
        linhas.extend(
            endereco(
                devedor.logradouro.as_deref(),
                devedor.cidade.as_deref(),
                devedor.uf.as_deref(),
                devedor.cep.as_deref(),
            )
            .map(|endereco| ("Endereço", endereco)),
        );
        linhas.extend(devedor.email.clone().map(|email| ("E-mail", email)));
    }
    if let Some(info) = &cobr.info_adicional {
        linhas.push(("Informação", info.clone()));
    }
    if let Some(encerramento) = cobr.encerramento.as_ref().and_then(encerramento) {
        linhas.push(("Encerramento", encerramento));
    }
    let titulo = format!(
        "Cobrança recorrente {}",
        limpo(cobr.txid.as_deref().unwrap_or_default())
    );
    let mut texto = secao(titulo.trim(), &linhas);
    if !cobr.tentativas.is_empty() {
        let _ = write!(
            texto,
            "\n\nTentativas de liquidação\n{}",
            tabela_de_tentativas(&cobr.tentativas).texto_colorido()
        );
    }
    if let Some(historico) = historico(cobr, fuso) {
        let _ = write!(texto, "\n\n{historico}");
    }
    if !cobr.pix.is_empty() {
        let _ = write!(
            texto,
            "\n\nPix recebidos\n{}",
            tabela_pix(&cobr.pix, fuso).texto_colorido()
        );
    }
    texto
}

/// Status, recurrence, amount and due date: what identifies a charge.
fn resumo_da_cobranca(cobr: &CobR) -> Vec<(&'static str, String)> {
    let mut linhas = Vec::new();
    if let Some(status) = &cobr.status {
        linhas.push(("Status", descrever(status).to_owned()));
    }
    if let Some(id_rec) = &cobr.id_rec {
        linhas.push(("Recorrência", id_rec.clone()));
    }
    if let Some(original) = cobr.valor.as_ref().and_then(|valor| valor.original) {
        linhas.push(("Valor", output::brl(original)));
    }
    if let Some(data) = cobr
        .calendario
        .as_ref()
        .and_then(|calendario| calendario.data_de_vencimento.as_deref())
    {
        linhas.push((
            "Vencimento",
            vencimento(&data_br(data), cobr.ajuste_dia_util),
        ));
    }
    linhas
}

/// The attempts to settle a charge: the scheduling and the new attempts.
fn tabela_de_tentativas(tentativas: &[TentativaCobR]) -> Tabela {
    let mut tabela = Tabela::new(vec![
        Coluna::texto("Liquidação", ""),
        Coluna::texto("Tipo", ""),
        Coluna::texto("Status", ""),
        Coluna::texto("endToEndId", ""),
        Coluna::texto("Motivo", ""),
    ]);
    for tentativa in tentativas {
        let motivo = tentativa.rejeicao.as_ref().and_then(|rejeicao| {
            match (rejeicao.codigo.as_deref(), rejeicao.descricao.as_deref()) {
                (Some(codigo), Some(descricao)) => Some(format!("{codigo}, {descricao}")),
                (Some(texto), None) | (None, Some(texto)) => Some(texto.to_owned()),
                (None, None) => None,
            }
        });
        tabela.linha(vec![
            Celula::texto(tentativa.data_liquidacao.as_deref().map(data_br).as_deref()),
            Celula::texto(tentativa.tipo.as_ref().map(descrever_tipo)),
            celula_tentativa(tentativa.status.as_ref()),
            Celula::texto(tentativa.end_to_end_id.as_deref()),
            Celula::texto(motivo.as_deref()),
        ]);
    }
    tabela
}

/// `agendamento`, `nova tentativa`.
fn descrever_tipo(tipo: &TipoTentativa) -> &str {
    match tipo {
        TipoTentativa::Agendamento => "agendamento",
        TipoTentativa::NovaTentativa => "nova tentativa",
        TipoTentativa::Reenvio => "reenvio",
        outro => outro.as_str(),
    }
}

/// `agendada`, `paga`.
fn descrever_tentativa(status: &StatusTentativa) -> &str {
    match status {
        StatusTentativa::Solicitada => "solicitada",
        StatusTentativa::Agendada => "agendada",
        StatusTentativa::Paga => "paga",
        StatusTentativa::Cancelada => "cancelada",
        StatusTentativa::Rejeitada => "rejeitada",
        StatusTentativa::Expirada => "expirada",
        outro => outro.as_str(),
    }
}

fn celula_tentativa(status: Option<&StatusTentativa>) -> Celula {
    let tom = status.and_then(|status| match status {
        StatusTentativa::Solicitada | StatusTentativa::Agendada => Some(Tom::Pendente),
        StatusTentativa::Paga => Some(Tom::Positivo),
        StatusTentativa::Cancelada | StatusTentativa::Rejeitada | StatusTentativa::Expirada => {
            Some(Tom::Negativo)
        }
        _ => None,
    });
    Celula::situacao(status.map(descrever_tentativa), tom)
}

/// The changes of status of a charge, with their times in `fuso`.
fn historico<Tz: TimeZone>(cobr: &CobR, fuso: &Tz) -> Option<String>
where
    Tz::Offset: std::fmt::Display,
{
    let historico: Vec<(String, String)> = cobr
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

async fn listar(context: &Context, args: &CobrListarArgs) -> Result<(), CliError> {
    let mut filtro = FiltroCobsR::new(periodo(args.periodo)?);
    filtro.id_rec.clone_from(&args.rec);
    filtro.devedor.clone_from(&args.documento);
    filtro.status = args.status.map(StatusCobR::from);
    filtro.convenio.clone_from(&args.convenio);
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let (cobrs, pagina_pedida) = match args.pagina {
        Some(numero) => {
            let mut resposta = client
                .pix_automatico()
                .listar_cobrs(&filtro, numero, args.itens_por_pagina)
                .await?;
            if context.formato() == Formato::Json {
                return output::print_json(&resposta);
            }
            let cobrs = std::mem::take(&mut resposta.cobsr);
            let paginacao = resposta
                .parametros
                .and_then(|parametros| parametros.paginacao)
                .unwrap_or_default();
            (cobrs, Some((numero, paginacao)))
        }
        None => (
            client.pix_automatico().listar_todas_cobrs(&filtro).await?,
            None,
        ),
    };
    match context.formato() {
        Formato::Json => output::print_json(&json!({ "cobsr": cobrs })),
        Formato::Csv => output::print_csv(&csv(&cobrs), context.separador()),
        Formato::Texto => {
            context.warn_if_sandbox(&settings);
            let mut texto = format!("{}\n\n", titulo(&filtro));
            if cobrs.is_empty() {
                texto.push_str("Nenhuma cobrança recorrente encontrada.");
            } else {
                texto.push_str(&tabela(&cobrs).texto_colorido());
                let _ = write!(texto, "\n\n{}", totais(&cobrs));
            }
            if let Some((numero, paginacao)) = pagina_pedida {
                texto.push_str(&pagina(
                    numero,
                    paginacao,
                    cobrs.len(),
                    "cobranças recorrentes",
                ));
            }
            output::print(&texto)
        }
    }
}

/// `Cobranças recorrentes criadas de 25/08/2026 00:00 a 24/09/2026 23:59`,
/// and the filters.
fn titulo(filtro: &FiltroCobsR) -> String {
    let formato = "%d/%m/%Y %H:%M";
    let mut texto = format!(
        "Cobranças recorrentes criadas de {} a {}",
        filtro.periodo.inicio.format(formato),
        filtro.periodo.fim.format(formato)
    );
    let mut filtros = Vec::new();
    if let Some(status) = &filtro.status {
        filtros.push(curto(status).to_owned());
    }
    if let Some(id_rec) = &filtro.id_rec {
        filtros.push(format!("recorrência {id_rec}"));
    }
    if let Some(documento) = &filtro.devedor {
        filtros.push(format!("devedor {}", documento.formatado()));
    }
    if let Some(convenio) = &filtro.convenio {
        filtros.push(format!("convênio {convenio}"));
    }
    if !filtros.is_empty() {
        let _ = write!(texto, " ({})", filtros.join(", "));
    }
    texto
}

fn tabela(cobrs: &[CobR]) -> Tabela {
    let mut tabela = Tabela::new(vec![
        Coluna::texto("Vencimento", ""),
        Coluna::texto("Status", ""),
        Coluna::valor("Valor", ""),
        Coluna::texto("Recorrência", ""),
        Coluna::texto("txid", ""),
    ]);
    for cobr in cobrs {
        let vencimento = cobr
            .calendario
            .as_ref()
            .and_then(|calendario| calendario.data_de_vencimento.as_deref())
            .map(data_br);
        tabela.linha(vec![
            Celula::texto(vencimento.as_deref()),
            celula_status(cobr.status.as_ref()),
            Celula::dinheiro(cobr.valor.as_ref().and_then(|valor| valor.original)),
            Celula::texto(cobr.id_rec.as_deref()),
            Celula::texto(cobr.txid.as_deref()),
        ]);
    }
    tabela
}

/// `3 cobranças · R$ 449,70 · pagas R$ 149,90`.
fn totais(cobrs: &[CobR]) -> String {
    let valor = |cobr: &CobR| {
        cobr.valor
            .as_ref()
            .and_then(|valor| valor.original)
            .unwrap_or_default()
    };
    let total: Decimal = cobrs.iter().map(valor).sum();
    let pagas: Decimal = cobrs
        .iter()
        .filter(|cobr| cobr.status == Some(StatusCobR::Concluida))
        .map(valor)
        .sum();
    let mut texto = match cobrs.len() {
        1 => "1 cobrança".to_owned(),
        n => format!("{n} cobranças"),
    };
    let _ = write!(texto, " · {}", output::brl(total));
    if !pagas.is_zero() {
        let _ = write!(texto, " · pagas {}", output::brl(pagas));
    }
    texto
}

/// Every field, with the API's names (nested ones with a dot) and codes;
/// the attempts and the Pix are in the JSON.
fn csv(cobrs: &[CobR]) -> Tabela {
    let texto = |campo: &'static str| Coluna::texto(campo, campo);
    let mut tabela = Tabela::new(vec![
        texto("txid"),
        texto("idRec"),
        texto("status"),
        texto("calendario.criacao"),
        texto("calendario.dataDeVencimento"),
        Coluna::valor("valor.original", "valor.original"),
        texto("ajusteDiaUtil"),
        texto("politicaRetentativa"),
        texto("infoAdicional"),
        texto("recebedor.cnpj"),
        texto("recebedor.nome"),
        texto("recebedor.conta"),
        texto("recebedor.tipoConta"),
        texto("recebedor.agencia"),
        texto("devedor.email"),
        texto("devedor.logradouro"),
        texto("devedor.cidade"),
        texto("devedor.uf"),
        texto("devedor.cep"),
    ]);
    for cobr in cobrs {
        let calendario = cobr.calendario.as_ref();
        let recebedor = cobr.recebedor.as_ref();
        let devedor = cobr.devedor.as_ref();
        tabela.linha(vec![
            Celula::texto(cobr.txid.as_deref()),
            Celula::texto(cobr.id_rec.as_deref()),
            Celula::texto(cobr.status.as_ref().map(StatusCobR::as_str)),
            Celula::texto(calendario.and_then(|c| c.criacao.as_deref())),
            Celula::texto(calendario.and_then(|c| c.data_de_vencimento.as_deref())),
            Celula::dinheiro(cobr.valor.as_ref().and_then(|v| v.original)),
            Celula::texto(
                cobr.ajuste_dia_util
                    .map(|ajuste| if ajuste { "true" } else { "false" }),
            ),
            Celula::texto(
                cobr.politica_retentativa
                    .as_ref()
                    .map(PoliticaRetentativa::as_str),
            ),
            Celula::texto(cobr.info_adicional.as_deref()),
            Celula::texto(recebedor.and_then(|r| r.cnpj.as_deref())),
            Celula::texto(recebedor.and_then(|r| r.nome.as_deref())),
            Celula::texto(recebedor.and_then(|r| r.conta.as_deref())),
            Celula::texto(
                recebedor
                    .and_then(|r| r.tipo_conta.as_ref())
                    .map(TipoContaRecebedor::as_str),
            ),
            Celula::texto(recebedor.and_then(|r| r.agencia.as_deref())),
            Celula::texto(devedor.and_then(|d| d.email.as_deref())),
            Celula::texto(devedor.and_then(|d| d.logradouro.as_deref())),
            Celula::texto(devedor.and_then(|d| d.cidade.as_deref())),
            Celula::texto(devedor.and_then(|d| d.uf.as_deref())),
            Celula::texto(devedor.and_then(|d| d.cep.as_deref())),
        ]);
    }
    tabela
}

async fn cancelar(
    context: &Context,
    args: &CobrCancelarArgs,
    terminal: &mut dyn Terminal,
) -> Result<(), CliError> {
    // Nothing is looked up when no one could confirm.
    pode_confirmar(terminal, args.sim)?;
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let atual = client.pix_automatico().consultar_cobr(&args.txid).await?;
    if let Some(status) = atual.status.as_ref().filter(|status| encerrada(status)) {
        return Err(CliError::Usage(format!(
            "a cobrança já está {}: não há o que cancelar",
            descrever(status)
        )));
    }
    let ambiente = settings.ambiente.as_ref().map(|setting| setting.value);
    let mut linhas = vec![("Ambiente", descrever_ambiente(ambiente))];
    linhas.extend(resumo_da_cobranca(&atual));
    let mut resumo = secao(
        &format!("Cobrança recorrente {} a cancelar", args.txid),
        &linhas,
    );
    // The 22h of the Banco Central are those of Brasília.
    let agora = Utc::now().with_timezone(&BRASILIA).naive_local();
    if let Some(aviso) = prazo_do_cancelamento(&atual, agora) {
        let _ = write!(resumo, "\naviso: {aviso}");
    }
    output::eprint(&resumo);
    confirmar(terminal, args.sim, "Cancelar a cobrança recorrente?")?;

    let cancelada = client.pix_automatico().cancelar_cobr(&args.txid).await?;
    match context.formato() {
        Formato::Json => output::print_json(&cancelada),
        // `commands::run` refuses csv for this command.
        Formato::Texto | Formato::Csv => output::print(&format!(
            "Cobrança recorrente cancelada: o débito não será feito.\n\n{}",
            render_cobr(&cancelada)
        )),
    }
}

/// A warning when it is past the time to cancel: by the Central Bank's
/// rules, 22:00 of the day before the settlement.
fn prazo_do_cancelamento(cobr: &CobR, agora: NaiveDateTime) -> Option<String> {
    let pendente = cobr
        .tentativas
        .iter()
        .filter(|tentativa| {
            matches!(
                tentativa.status,
                Some(StatusTentativa::Solicitada | StatusTentativa::Agendada)
            )
        })
        .filter_map(|tentativa| tentativa.data_liquidacao.as_deref().and_then(parse_data))
        .max();
    let liquidacao = pendente.or_else(|| {
        cobr.calendario
            .as_ref()
            .and_then(|calendario| calendario.data_de_vencimento.as_deref())
            .and_then(parse_data)
    })?;
    let limite =
        liquidacao
            .pred_opt()?
            .and_time(NaiveTime::from_hms_opt(HORA_LIMITE_CANCELAMENTO, 0, 0)?);
    (agora >= limite).then(|| {
        format!(
            "pelas regras do Banco Central, o cancelamento é até as 22h do dia anterior à liquidação ({}): o banco pode recusá-lo",
            liquidacao.format("%d/%m/%Y")
        )
    })
}

async fn retentativa(
    context: &Context,
    args: &CobrRetentativaArgs,
    terminal: &mut dyn Terminal,
) -> Result<(), CliError> {
    if args.data < hoje() {
        return Err(CliError::Usage(format!(
            "--data: {} já passou",
            args.data.format("%d/%m/%Y")
        )));
    }
    // Nothing is looked up when no one could confirm.
    pode_confirmar(terminal, args.sim)?;
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let atual = client.pix_automatico().consultar_cobr(&args.txid).await?;
    retentavel(&atual, args.data)?;
    let ambiente = settings.ambiente.as_ref().map(|setting| setting.value);
    output::eprint(&resumo_retentativa(&atual, args, ambiente));
    confirmar(terminal, args.sim, "Pedir a nova tentativa?")?;

    let cobr = client
        .pix_automatico()
        .solicitar_retentativa(&args.txid, args.data)
        .await
        .map_err(|err| retentativa_incerta(err, &args.txid))?;
    match context.formato() {
        Formato::Json => output::print_json(&cobr),
        // `commands::run` refuses csv for this command.
        Formato::Texto | Formato::Csv => output::print(&format!(
            "Nova tentativa pedida para {}.\n\n{}",
            args.data.format("%d/%m/%Y"),
            render_cobr(&cobr)
        )),
    }
}

/// Refuses what the policy of the recurrence does not allow: a charge that
/// ended, a recurrence without new attempts, and a day out of the 7 after
/// the settlement planned.
fn retentavel(cobr: &CobR, data: NaiveDate) -> Result<(), CliError> {
    if let Some(status) = cobr.status.as_ref().filter(|status| encerrada(status)) {
        return Err(CliError::Usage(format!(
            "a cobrança está {}: não há o que tentar de novo",
            descrever(status)
        )));
    }
    if cobr.politica_retentativa == Some(PoliticaRetentativa::NaoPermite) {
        return Err(CliError::Usage(
            "a recorrência desta cobrança não permite novas tentativas".to_owned(),
        ));
    }
    if let Some(prevista) = liquidacao_prevista(cobr) {
        let limite = prevista
            .checked_add_days(Days::new(DIAS_DE_RETENTATIVA))
            .unwrap_or(NaiveDate::MAX);
        if data <= prevista || data > limite {
            return Err(CliError::Usage(format!(
                "--data: a nova tentativa é de {} a {}, até {DIAS_DE_RETENTATIVA} dias depois da liquidação prevista ({})",
                prevista.succ_opt().unwrap_or(prevista).format("%d/%m/%Y"),
                limite.format("%d/%m/%Y"),
                prevista.format("%d/%m/%Y")
            )));
        }
    }
    Ok(())
}

/// The day of the settlement of the original payment order: that of its
/// scheduling.
fn liquidacao_prevista(cobr: &CobR) -> Option<NaiveDate> {
    cobr.tentativas
        .iter()
        .filter(|tentativa| tentativa.tipo == Some(TipoTentativa::Agendamento))
        .find_map(|tentativa| tentativa.data_liquidacao.as_deref().and_then(parse_data))
}

/// The new attempts already asked for.
fn novas_tentativas(cobr: &CobR) -> impl Iterator<Item = &TentativaCobR> {
    cobr.tentativas
        .iter()
        .filter(|tentativa| tentativa.tipo == Some(TipoTentativa::NovaTentativa))
}

/// The charge and the new attempt, and what deserves a warning: another
/// attempt on the same day, or all those allowed already asked for.
fn resumo_retentativa(
    cobr: &CobR,
    args: &CobrRetentativaArgs,
    ambiente: Option<Environment>,
) -> String {
    let mut linhas = vec![("Ambiente", descrever_ambiente(ambiente))];
    linhas.extend(resumo_da_cobranca(cobr));
    if let Some(prevista) = liquidacao_prevista(cobr) {
        linhas.push((
            "Liquidação prevista",
            prevista.format("%d/%m/%Y").to_string(),
        ));
    }
    let pedidas = novas_tentativas(cobr).count();
    if pedidas > 0 {
        linhas.push((
            "Já pedidas",
            match pedidas {
                1 => "1 nova tentativa".to_owned(),
                n => format!("{n} novas tentativas"),
            },
        ));
    }
    linhas.push(("Nova tentativa", args.data.format("%d/%m/%Y").to_string()));
    let mut texto = secao(
        &format!("Nova tentativa da cobrança recorrente {}", args.txid),
        &linhas,
    );
    if pedidas >= MAX_RETENTATIVAS {
        let _ = write!(
            texto,
            "\naviso: a política da recorrência permite até {MAX_RETENTATIVAS} novas tentativas"
        );
    }
    if novas_tentativas(cobr).any(|tentativa| {
        tentativa.data_liquidacao.as_deref().and_then(parse_data) == Some(args.data)
    }) {
        let _ = write!(
            texto,
            "\naviso: já há uma tentativa em {}, e elas são em dias diferentes",
            args.data.format("%d/%m/%Y")
        );
    }
    texto
}

/// The error of a request for a new attempt: with an unknown outcome, how
/// to check before asking again.
fn retentativa_incerta(err: InterError, txid: &Txid) -> CliError {
    if resultado_incerto(&err) {
        CliError::CriacaoIncerta {
            source: err,
            situacao: "a nova tentativa pode ter sido pedida",
            consulta: format!("inter-pj pix-automatico cobr consultar {txid}"),
        }
    } else {
        err.into()
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, FixedOffset};
    use clap::Parser;
    use serde_json::{Value, json};
    use wiremock::matchers::{any, method, path};
    use wiremock::{Mock, ResponseTemplate};

    use super::*;
    use crate::cli::{Cli, Command, PixAutomaticoCommand};

    #[test]
    fn every_documented_status_has_a_tone() {
        for status in StatusCobR::DOCUMENTADOS {
            assert!(
                matches!(celula_status(Some(status)), Celula::Situacao(..)),
                "{status:?}"
            );
        }
        for status in StatusTentativa::DOCUMENTADOS {
            assert!(
                matches!(celula_tentativa(Some(status)), Celula::Situacao(..)),
                "{status:?}"
            );
        }
        assert_eq!(
            celula_status(Some(&StatusCobR::Ativa)),
            Celula::Situacao("ativa".into(), Tom::Pendente)
        );
        assert_eq!(
            celula_tentativa(Some(&StatusTentativa::Rejeitada)),
            Celula::Situacao("rejeitada".into(), Tom::Negativo)
        );
    }
    use crate::commands::testes;
    use crate::confirmacao::testes::TerminalFalso;
    use crate::tabela::Separador;

    const ID_REC: &str = "RR1234567820260924abcdefghijk";
    const TXID: &str = "7978c0c97ea847e78e8849634473c1f1";
    const E2E: &str = "E12345678202610101300abcdef12345";

    fn comando(args: &[&str]) -> CobrCommand {
        let mut todos = vec!["inter-pj", "pix-automatico", "cobr"];
        todos.extend_from_slice(args);
        match Cli::try_parse_from(todos).unwrap().command {
            Command::PixAutomatico(PixAutomaticoCommand::Cobr(comando)) => comando,
            outro => panic!("{outro:?}"),
        }
    }

    fn criar_args(extra: &[&str]) -> CobrCriarArgs {
        let mut todos = vec![
            "criar",
            "--rec",
            ID_REC,
            "--valor",
            "149,90",
            "--vencimento",
            "2026-10-10",
        ];
        todos.extend_from_slice(extra);
        match comando(&todos) {
            CobrCommand::Criar(args) => *args,
            outro => panic!("{outro:?}"),
        }
    }

    fn retentativa_args(data: &str) -> CobrRetentativaArgs {
        match comando(&["retentativa", TXID, "--data", data]) {
            CobrCommand::Retentativa(args) => args,
            outro => panic!("{outro:?}"),
        }
    }

    fn dia(dia: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, dia).unwrap()
    }

    fn rec(status: &str) -> Rec {
        serde_json::from_value(json!({
            "idRec": ID_REC,
            "vinculo": {"objeto": "Mensalidade", "devedor": {"cpf": "12345678909", "nome": "Cliente Exemplo"}, "contrato": "contrato-001"},
            "calendario": {"dataInicial": "2026-10-10", "dataFinal": "2027-09-10", "periodicidade": "MENSAL"},
            "valor": {"valorRec": "149.90"},
            "status": status,
            "politicaRetentativa": "PERMITE_3R_7D"
        }))
        .unwrap()
    }

    fn cobr(status: &str) -> CobR {
        serde_json::from_value(json!({
            "idRec": ID_REC,
            "txid": TXID,
            "infoAdicional": "Mensalidade de outubro",
            "calendario": {"criacao": "2026-09-24T13:00:00.000Z", "dataDeVencimento": "2026-10-10"},
            "valor": {"original": "149.90"},
            "status": status,
            "politicaRetentativa": "PERMITE_3R_7D",
            "ajusteDiaUtil": true,
            "devedor": {"email": "cliente@empresa.example", "logradouro": "Rua Exemplo, 100", "cidade": "São Paulo", "uf": "SP", "cep": "01001000"},
            "recebedor": {"cnpj": "12345678000195", "nome": "Empresa Exemplo Ltda", "conta": "1234567", "tipoConta": "CORRENTE", "agencia": "0001"},
            "tentativas": [{"dataLiquidacao": "2026-10-10", "tipo": "AGND", "status": "AGENDADA", "endToEndId": E2E}],
            "atualizacao": [{"status": "CRIADA", "data": "2026-09-24T13:00:00.000Z"}, {"status": "ATIVA", "data": "2026-09-25T12:30:00.000Z"}]
        }))
        .unwrap()
    }

    /// `cobr("ATIVA")` whose scheduled debit failed, with new attempts on
    /// `dias` of October.
    fn nao_paga(dias: &[u32]) -> CobR {
        let mut cobr = cobr("ATIVA");
        cobr.tentativas[0].status = Some(StatusTentativa::Rejeitada);
        for dia in dias {
            cobr.tentativas.push(
                serde_json::from_value(json!({
                    "dataLiquidacao": format!("2026-10-{dia:02}"), "tipo": "NTAG", "status": "SOLICITADA"
                }))
                .unwrap(),
            );
        }
        cobr
    }

    #[test]
    fn options_become_the_charge() {
        let cobr = das_opcoes(
            &criar_args(&[
                "--conta",
                "1234567",
                "--agencia",
                "0001",
                "--info",
                "Mensalidade de outubro",
                "--devedor-email",
                "cliente@empresa.example",
                "--devedor-endereco",
                "Rua Exemplo, 100",
                "--devedor-cidade",
                "São Paulo",
                "--devedor-uf",
                "SP",
                "--devedor-cep",
                "01001-000",
            ]),
            None,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(&cobr).unwrap(),
            json!({
                "idRec": ID_REC,
                "infoAdicional": "Mensalidade de outubro",
                "calendario": {"dataDeVencimento": "2026-10-10"},
                "valor": {"original": "149.90"},
                "ajusteDiaUtil": true,
                "devedor": {"email": "cliente@empresa.example", "logradouro": "Rua Exemplo, 100", "cidade": "São Paulo", "uf": "SP", "cep": "01001000"},
                "recebedor": {"conta": "1234567", "tipoConta": "CORRENTE", "agencia": "0001"}
            })
        );
        // Without --conta, the configured account; another kind of account,
        // and a due date kept on a non-working day.
        let configurada = das_opcoes(
            &criar_args(&["--tipo-conta", "pagamento", "--sem-ajuste-dia-util"]),
            Some("7654321"),
        )
        .unwrap();
        let configurada = serde_json::to_value(&configurada).unwrap();
        assert_eq!(
            configurada["recebedor"],
            json!({"conta": "7654321", "tipoConta": "PAGAMENTO"})
        );
        assert_eq!(configurada["ajusteDiaUtil"], false);
        assert!(configurada.get("devedor").is_none(), "{configurada}");
        let dada = das_opcoes(&criar_args(&["--conta", "1234567"]), Some("7654321")).unwrap();
        assert_eq!(dada.recebedor.conta, "1234567");
    }

    #[test]
    fn errors_name_the_option() {
        let err = das_opcoes(&criar_args(&[]), None).unwrap_err();
        assert!(err.to_string().starts_with("informe --conta, "), "{err}");
        let longa = "x".repeat(141);
        for (extra, inicio) in [
            (&["--conta", "1234-5"][..], "--conta: "),
            (&["--conta", "1234567", "--agencia", "12345"], "--agencia: "),
            (&["--conta", "1234567", "--info", &longa], "--info: "),
            (
                &["--conta", "1234567", "--devedor-email", "cliente"],
                "--devedor-email: e-mail inválido",
            ),
        ] {
            let err = das_opcoes(&criar_args(extra), None).unwrap_err();
            assert!(err.to_string().starts_with(inicio), "{inicio}\n{err}");
        }
        for extra in [
            &["--valor", "0"][..],
            &["--vencimento", "10/10/2026"],
            &["--rec", "RR123"],
            &["--tipo-conta", "salario"],
            &["--txid", "curto"],
            &["--devedor-cep", "0100100"],
            &["--sim", "--simular"],
        ] {
            let mut todos = vec![
                "inter-pj",
                "pix-automatico",
                "cobr",
                "criar",
                "--rec",
                ID_REC,
                "--valor",
                "149,90",
                "--vencimento",
                "2026-10-10",
            ];
            // An option given twice is refused too: replace it instead.
            if let Some(i) = todos.iter().position(|arg| *arg == extra[0]) {
                todos.drain(i..i + 2);
            }
            todos.extend_from_slice(extra);
            assert!(Cli::try_parse_from(todos).is_err(), "{extra:?}");
        }
        assert!(vencimento_valido(dia(10), dia(10)).is_ok());
        assert!(vencimento_valido(dia(9), dia(10)).is_err());
    }

    #[test]
    fn only_an_approved_recurrence_takes_charges() {
        assert!(aceita_cobrancas(&rec("APROVADA")).is_ok());
        let err = aceita_cobrancas(&rec("CRIADA")).unwrap_err();
        assert_eq!(
            err.to_string(),
            "a recorrência está criada (aguarda a aprovação do pagador): só uma recorrência aprovada pelo pagador aceita cobranças"
        );
        assert!(aceita_cobrancas(&rec("CANCELADA")).is_err());
    }

    #[test]
    fn the_summary_shows_the_recurrence_and_warns() {
        let cobr = das_opcoes(
            &criar_args(&[
                "--conta",
                "1234567",
                "--agencia",
                "0001",
                "--info",
                "Mensalidade de outubro",
            ]),
            None,
        )
        .unwrap();
        let txid: Txid = TXID.parse().unwrap();
        assert_eq!(
            resumo(
                &cobr,
                &txid,
                Some(&rec("APROVADA")),
                Some(Environment::Sandbox)
            ),
            format!(
                "\
Cobrança recorrente a criar
  Ambiente          sandbox (dados fictícios)
  Recorrência       {ID_REC}
  Devedor           Cliente Exemplo (123.456.789-09)
  Contrato          contrato-001
  Objeto            Mensalidade
  Valor             R$ 149,90 (cento e quarenta e nove reais e noventa centavos)
  Vencimento        10/10/2026, ou o próximo dia útil
  Conta que recebe  conta corrente 1234567, agência 0001
  Informação        Mensalidade de outubro
  Retentativas      até 3 novas tentativas, em 7 dias
  txid              {TXID}"
            )
        );
        // With --simular, the recurrence is not looked up.
        let simulada = resumo(&cobr, &txid, None, Some(Environment::Sandbox));
        assert!(!simulada.contains("Devedor"), "{simulada}");

        let mut outra = cobr.clone();
        outra.valor = "150".parse().unwrap();
        outra.vencimento = dia(5);
        assert_eq!(
            avisos(&outra, &rec("APROVADA")),
            [
                "a recorrência tem valor fixo de R$ 149,90, e esta cobrança é de R$ 150,00",
                "o vencimento é anterior ao primeiro pagamento da recorrência (10/10/2026)",
            ]
        );
        outra.vencimento = NaiveDate::from_ymd_opt(2027, 10, 10).unwrap();
        let producao = resumo(
            &outra,
            &txid,
            Some(&rec("APROVADA")),
            Some(Environment::Production),
        );
        assert!(producao.starts_with("*** PRODUÇÃO"), "{producao}");
        assert!(
            producao
                .ends_with("\naviso: o vencimento é posterior ao fim da recorrência (10/09/2027)"),
            "{producao}"
        );
    }

    #[test]
    fn a_charge_in_detail() {
        let brasilia = FixedOffset::west_opt(3 * 3600).unwrap();
        let mut cobr = cobr("CONCLUIDA");
        cobr.tentativas[0].status = Some(StatusTentativa::Paga);
        cobr.pix = serde_json::from_value(json!([
            {"endToEndId": E2E, "txid": TXID, "valor": "149.90", "horario": "2026-10-10T13:00:00.000Z"}
        ]))
        .unwrap();
        assert_eq!(
            render_cobr_em(&cobr, &brasilia),
            format!(
                "\
Cobrança recorrente {TXID}
  Status        concluída (paga)
  Recorrência   {ID_REC}
  Valor         R$ 149,90
  Vencimento    10/10/2026, ou o próximo dia útil
  Criada em     24/09/2026
  Retentativas  até 3 novas tentativas, em 7 dias
  Recebedor     Empresa Exemplo Ltda (12.345.678/0001-95), conta corrente 1234567, agência 0001
  Endereço      Rua Exemplo, 100 - São Paulo/SP - CEP 01001-000
  E-mail        cliente@empresa.example
  Informação    Mensalidade de outubro

Tentativas de liquidação
Liquidação  Tipo         Status  endToEndId                        Motivo
10/10/2026  agendamento  paga    {E2E}

Histórico
  24/09/2026 10:00:00  criada
  25/09/2026 09:30:00  ativa

Pix recebidos
Horário                  Valor  Devolvido  endToEndId
10/10/2026 10:00:00  R$ 149,90             {E2E}"
            )
        );
    }

    #[test]
    fn endings_and_rejections_in_words() {
        let mut cancelada = cobr("CANCELADA");
        cancelada.encerramento = serde_json::from_value(json!({
            "cancelamento": {"solicitante": "USUARIO_RECEBEDOR", "codigo": "SLCR", "descricao": "Cancelada pelo recebedor"}
        }))
        .unwrap();
        let texto = render_cobr_em(&cancelada, &FixedOffset::west_opt(0).unwrap());
        assert!(
            texto.contains(
                "\n  Encerramento  cancelada pelo recebedor: SLCR, Cancelada pelo recebedor\n"
            ),
            "{texto}"
        );
        let mut rejeitada = cobr("ATIVA");
        rejeitada.tentativas[0] = serde_json::from_value(json!({
            "dataLiquidacao": "2026-10-10", "tipo": "AGND", "status": "REJEITADA",
            "rejeicao": {"codigo": "AM04", "descricao": "Saldo insuficiente"}
        }))
        .unwrap();
        let tentativas = tabela_de_tentativas(&rejeitada.tentativas).texto();
        assert!(
            tentativas.ends_with(
                "10/10/2026  agendamento  rejeitada              AM04, Saldo insuficiente"
            ),
            "{tentativas}"
        );
    }

    #[test]
    fn listings_add_up() {
        let cobrs = [cobr("CONCLUIDA"), cobr("ATIVA"), cobr("ATIVA")];
        assert_eq!(totais(&cobrs), "3 cobranças · R$ 449,70 · pagas R$ 149,90");
        assert_eq!(totais(&cobrs[1..2]), "1 cobrança · R$ 149,90");
        let csv = csv(&cobrs).csv(Separador::Virgula);
        assert!(
            csv.starts_with("txid,idRec,status,calendario.criacao,calendario.dataDeVencimento,valor.original,ajusteDiaUtil,politicaRetentativa,"),
            "{csv}"
        );
        assert!(
            csv.contains(&format!(
                "{TXID},{ID_REC},CONCLUIDA,2026-09-24T13:00:00.000Z,2026-10-10,149.90,true,PERMITE_3R_7D,Mensalidade de outubro,12345678000195,"
            )),
            "{csv}"
        );
        let momento = |texto| DateTime::parse_from_rfc3339(texto).unwrap();
        let mut filtro = FiltroCobsR::new(
            inter_pj::pix::PeriodoPix::new(
                momento("2026-09-01T00:00:00-03:00"),
                momento("2026-09-30T23:59:59-03:00"),
            )
            .unwrap(),
        );
        filtro.status = Some(StatusCobR::Ativa);
        filtro.id_rec = Some(ID_REC.parse().unwrap());
        assert_eq!(
            titulo(&filtro),
            format!(
                "Cobranças recorrentes criadas de 01/09/2026 00:00 a 30/09/2026 23:59 (ativa, recorrência {ID_REC})"
            )
        );
    }

    #[test]
    fn cancelling_is_until_22h_of_the_day_before() {
        let as_ = |dia_do_mes: u32, hora: u32| dia(dia_do_mes).and_hms_opt(hora, 0, 0).unwrap();
        assert_eq!(prazo_do_cancelamento(&cobr("ATIVA"), as_(9, 21)), None);
        assert_eq!(
            prazo_do_cancelamento(&cobr("ATIVA"), as_(9, 22)).unwrap(),
            "pelas regras do Banco Central, o cancelamento é até as 22h do dia anterior à liquidação (10/10/2026): o banco pode recusá-lo"
        );
        // A new attempt pending moves the settlement.
        assert_eq!(prazo_do_cancelamento(&nao_paga(&[12]), as_(10, 23)), None);
        assert!(prazo_do_cancelamento(&nao_paga(&[12]), as_(11, 22)).is_some());
    }

    #[test]
    fn new_attempts_follow_the_policy_of_the_recurrence() {
        let cobr_ = nao_paga(&[]);
        assert!(retentavel(&cobr_, dia(11)).is_ok());
        assert!(retentavel(&cobr_, dia(17)).is_ok());
        for fora in [dia(10), dia(18)] {
            assert_eq!(
                retentavel(&cobr_, fora).unwrap_err().to_string(),
                "--data: a nova tentativa é de 11/10/2026 a 17/10/2026, até 7 dias depois da liquidação prevista (10/10/2026)"
            );
        }
        let mut sem = nao_paga(&[]);
        sem.politica_retentativa = Some(PoliticaRetentativa::NaoPermite);
        assert_eq!(
            retentavel(&sem, dia(11)).unwrap_err().to_string(),
            "a recorrência desta cobrança não permite novas tentativas"
        );
        assert!(
            retentavel(&cobr("CONCLUIDA"), dia(11))
                .unwrap_err()
                .to_string()
                .starts_with("a cobrança está concluída (paga): ")
        );

        let resumo = resumo_retentativa(&nao_paga(&[11]), &retentativa_args("2026-10-12"), None);
        assert!(
            resumo.contains("\n  Liquidação prevista  10/10/2026\n  Já pedidas           1 nova tentativa\n  Nova tentativa       12/10/2026"),
            "{resumo}"
        );
        assert!(!resumo.contains("aviso"), "{resumo}");
        let resumo = resumo_retentativa(
            &nao_paga(&[11, 12, 13]),
            &retentativa_args("2026-10-13"),
            None,
        );
        assert!(
            resumo.ends_with("\naviso: a política da recorrência permite até 3 novas tentativas\naviso: já há uma tentativa em 13/10/2026, e elas são em dias diferentes"),
            "{resumo}"
        );
    }

    // --- the commands against a mock API ------------------------------------

    async fn cenario(args: &[&str], escopos: &str) -> (testes::Cenario, CobrCommand) {
        let mut todos = vec!["pix-automatico", "cobr"];
        todos.extend_from_slice(args);
        match testes::cenario(&todos, escopos).await {
            (cenario, Command::PixAutomatico(PixAutomaticoCommand::Cobr(comando))) => {
                (cenario, comando)
            }
            (_, outro) => panic!("{outro:?}"),
        }
    }

    /// Mounts `resposta` for `GET caminho`, and refuses anything else.
    async fn so_consulta(cenario: &testes::Cenario, caminho: String, resposta: Value) {
        Mock::given(method("GET"))
            .and(path(caminho))
            .respond_with(ResponseTemplate::new(200).set_body_json(resposta))
            .expect(1)
            .mount(&cenario.server)
            .await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&cenario.server)
            .await;
    }

    #[tokio::test]
    async fn a_declined_creation_only_looks_the_recurrence_up() {
        let (cenario, comando) = cenario(
            &[
                "criar",
                "--rec",
                ID_REC,
                "--valor",
                "149,90",
                "--vencimento",
                "2099-10-10",
                "--conta",
                "1234567",
            ],
            "rec.read cobr.write",
        )
        .await;
        let CobrCommand::Criar(args) = comando else {
            unreachable!()
        };
        let aprovada = serde_json::to_value(rec("APROVADA")).unwrap();
        so_consulta(&cenario, format!("/pix/v2/rec/{ID_REC}"), aprovada).await;
        let mut terminal = TerminalFalso::respondendo("n\n");
        let err = criar(&cenario.context, &args, &mut terminal)
            .await
            .unwrap_err();
        assert!(matches!(err, CliError::Cancelado), "{err}");
        assert_eq!(terminal.perguntas, ["Criar a cobrança recorrente? [s/N] "]);
    }

    #[tokio::test]
    async fn a_declined_retry_only_looks_the_charge_up() {
        let (cenario, comando) = cenario(
            &["retentativa", TXID, "--data", "2099-10-12"],
            "cobr.write cobr.read",
        )
        .await;
        let CobrCommand::Retentativa(args) = comando else {
            unreachable!()
        };
        // The dates of the charge are near the new attempt's.
        let mut atual = serde_json::to_value(nao_paga(&[])).unwrap();
        atual["calendario"]["dataDeVencimento"] = json!("2099-10-10");
        atual["tentativas"][0]["dataLiquidacao"] = json!("2099-10-10");
        so_consulta(&cenario, format!("/pix/v2/cobr/{TXID}"), atual).await;
        let mut terminal = TerminalFalso::respondendo("n\n");
        let err = retentativa(&cenario.context, &args, &mut terminal)
            .await
            .unwrap_err();
        assert!(matches!(err, CliError::Cancelado), "{err}");
        assert_eq!(terminal.perguntas, ["Pedir a nova tentativa? [s/N] "]);
    }
}
