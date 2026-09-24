//! `inter-pj pix cobv criar|modelo|revisar|consultar|listar`: charges with a
//! due date.

use std::fmt::Write as _;

use chrono::{Days, Local, NaiveDate, TimeZone};
use inter_pj::cobranca::Uf;
use inter_pj::pix::{
    CalendarioCobv, CobrancaPixError, Cobv, CobvRevisada, CobvSolicitada, DevedorCobv, FiltroCobvs,
    InfoAdicional, LocCob, PessoaPix, StatusCob, Txid, ValorCobvRevisada,
};
use inter_pj::{Environment, endpoint};
use rust_decimal::Decimal;
use serde_json::json;

use super::encargos::{self, Encargos};
use super::{
    Filtros, alteravel, antes_e_depois, celula_status, copia_e_cola_ativa, descrever_status,
    endereco, incerta, paginacao, pessoa, tabela_pix,
};
use crate::arquivo;
use crate::cli::{
    DevedorCobvArgs, Formato, PixCobvCommand, PixCobvConsultarArgs, PixCobvCriarArgs,
    PixCobvListarArgs, PixCobvRevisarArgs,
};
use crate::commands::qrcode::OpcoesQr;
use crate::commands::{Context, hoje, simulacao};
use crate::confirmacao::{Stdio, Terminal, confirmar, descrever_ambiente, pode_confirmar};
use crate::error::CliError;
use crate::output::{self, data_br, horario_em, parse_data, secao};
use crate::tabela::{Celula, Coluna, Tabela};
use crate::valor::por_extenso;

/// Days after the due date in which a charge can be paid, when not given.
const VALIDADE_PADRAO: u32 = 30;

/// Longest payer name shown in the listing.
const LARGURA_DEVEDOR: usize = 30;

pub(super) async fn run(context: &Context, command: PixCobvCommand) -> Result<(), CliError> {
    match command {
        PixCobvCommand::Criar(args) => criar(context, &args, &mut Stdio).await,
        PixCobvCommand::Modelo(_) => output::print_raw(&arquivo::modelo_cobv(hoje())),
        PixCobvCommand::Revisar(args) => revisar(context, &args, &mut Stdio).await,
        PixCobvCommand::Consultar(args) => consultar(context, &args).await,
        PixCobvCommand::Listar(args) => listar(context, &args).await,
        PixCobvCommand::Pagar(args) => super::sandbox::pagar_cobv(context, &args).await,
    }
}

async fn criar(
    context: &Context,
    args: &PixCobvCriarArgs,
    terminal: &mut dyn Terminal,
) -> Result<(), CliError> {
    let hoje = hoje();
    let opcoes = OpcoesQr::new(
        context,
        args.qr.qrcode,
        args.qr.qrcode_png.clone(),
        args.qr.sobrescrever,
    )?;
    let cobv = match &args.arquivo {
        Some(caminho) => arquivo::cobv(&arquivo::ler_json(caminho)?, &arquivo::nome(caminho))?,
        None => das_opcoes(args, hoje)?,
    };
    vencimento_valido(cobv.calendario.data_de_vencimento, hoje)?;
    let txid = args.txid.clone().unwrap_or_else(Txid::novo);
    let settings = context.settings()?;
    // Configuration problems show up before the confirmation, not after it.
    let client = if args.simular {
        None
    } else {
        Some(context.client(&settings)?)
    };
    let ambiente = settings.ambiente.as_ref().map(|setting| setting.value);
    eprintln!("{}", resumo(&cobv, &txid, hoje, ambiente));

    let Some(client) = client else {
        return simulacao::mostrar_em(
            context,
            &settings,
            endpoint::pix::CRIAR_COBV,
            &[("txid", txid.as_str())],
            &[],
            &cobv,
        );
    };
    confirmar(terminal, args.sim, "Criar a cobrança?")?;
    let criada = client
        .pix()
        .criar_cobv(&txid, &cobv)
        .await
        .map_err(|err| incerta(err, "pix cobv", &txid))?;
    let texto = format!(
        "Cobrança Pix com vencimento criada.\n\n{}\n\nAcompanhe com: inter-pj pix cobv consultar {txid}",
        render_cobv(&criada)
    );
    opcoes.mostrar(context, &settings, &texto, &criada, &copia_e_cola(&criada))
}

/// The charge of the options, checked as the API would.
fn das_opcoes(args: &PixCobvCriarArgs, hoje: NaiveDate) -> Result<CobvSolicitada, CliError> {
    // With --chave, clap requires the amount, the due date and the payer.
    let (Some(chave), Some(valor), Some(vencimento), Some(devedor)) = (
        &args.chave,
        args.valor,
        args.vencimento,
        devedor(&args.devedor),
    ) else {
        return Err(CliError::Usage(
            "informe --chave, --valor, --vencimento, --devedor-documento e --devedor-nome, ou --arquivo"
                .to_owned(),
        ));
    };
    // What depends on the due date makes sense only once it is valid.
    vencimento_valido(vencimento, hoje)?;
    let mut cobv = CobvSolicitada::new(chave.clone(), valor, vencimento, devedor);
    cobv.calendario.validade_apos_vencimento = args.validade_apos_vencimento;
    let encargos = Encargos::de(&args.encargos, vencimento)?;
    cobv.valor.multa = encargos.multa;
    cobv.valor.juros = encargos.juros;
    cobv.valor.abatimento = encargos.abatimento;
    cobv.valor.desconto = encargos.desconto;
    cobv.loc = args.loc.map(LocCob::new);
    cobv.solicitacao_pagador.clone_from(&args.solicitacao);
    cobv.info_adicionais.clone_from(&args.info);
    cobv.validar().map_err(|err| erro_da_opcao(&err))?;
    Ok(cobv)
}

/// The payer of the options, when given.
fn devedor(args: &DevedorCobvArgs) -> Option<DevedorCobv> {
    let (Some(documento), Some(nome)) = (&args.devedor_documento, &args.devedor_nome) else {
        return None;
    };
    let mut devedor = DevedorCobv::new(documento.clone(), nome.clone());
    devedor.email.clone_from(&args.devedor_email);
    devedor.logradouro.clone_from(&args.devedor_endereco);
    devedor.cidade.clone_from(&args.devedor_cidade);
    devedor.uf = args.devedor_uf;
    devedor.cep.clone_from(&args.devedor_cep);
    Some(devedor)
}

/// An error of the library's checks, named by the option.
fn erro_da_opcao(err: &CobrancaPixError) -> CliError {
    match opcao(err.campo()) {
        "" => CliError::Usage(err.to_string()),
        opcao => CliError::Usage(format!("{opcao}: {err}")),
    }
}

/// The option of a field of the API, for the messages.
fn opcao(campo: &str) -> &str {
    match campo {
        "valor.original" => "--valor",
        "devedor.nome" => "--devedor-nome",
        "devedor.email" => "--devedor-email",
        "devedor.logradouro" => "--devedor-endereco",
        "devedor.cidade" => "--devedor-cidade",
        "devedor.cep" => "--devedor-cep",
        "solicitacaoPagador" => "--solicitacao",
        "valor.desconto.valorPerc" => "--desconto-por-dia",
        multa if multa.starts_with("valor.multa") => "--multa",
        juros if juros.starts_with("valor.juros") => "--juros",
        abatimento if abatimento.starts_with("valor.abatimento") => "--abatimento",
        desconto if desconto.starts_with("valor.desconto") => "--desconto",
        info if info.starts_with("infoAdicionais") => "--info",
        outro => outro,
    }
}

fn vencimento_valido(vencimento: NaiveDate, hoje: NaiveDate) -> Result<(), CliError> {
    if vencimento < hoje {
        return Err(CliError::Usage(format!(
            "o vencimento ({}) já passou: a cobrança vence hoje ou depois",
            vencimento.format("%d/%m/%Y")
        )));
    }
    Ok(())
}

/// The charge about to be created, and what deserves a warning.
fn resumo(
    cobv: &CobvSolicitada,
    txid: &Txid,
    hoje: NaiveDate,
    ambiente: Option<Environment>,
) -> String {
    let valor = cobv.valor.original;
    let extenso = por_extenso(valor)
        .map(|extenso| format!(" ({extenso})"))
        .unwrap_or_default();
    let vencimento = cobv.calendario.data_de_vencimento;
    let mut linhas = vec![
        ("Ambiente", descrever_ambiente(ambiente)),
        ("Valor", format!("{}{extenso}", output::brl(valor))),
        ("Vencimento", vencimento.format("%d/%m/%Y").to_string()),
        (
            "Validade",
            validade(vencimento, cobv.calendario.validade_apos_vencimento),
        ),
        (
            "Chave",
            format!("{} ({})", cobv.chave.as_str(), cobv.chave.tipo()),
        ),
    ];
    linhas.extend(linhas_do_devedor(&cobv.devedor));
    linhas.extend(encargos::linhas(
        cobv.valor.multa.as_ref(),
        cobv.valor.juros.as_ref(),
        cobv.valor.abatimento.as_ref(),
        cobv.valor.desconto.as_ref(),
    ));
    if let Some(solicitacao) = &cobv.solicitacao_pagador {
        linhas.push(("Solicitação", solicitacao.clone()));
    }
    if !cobv.info_adicionais.is_empty() {
        linhas.push(("Informações", infos(&cobv.info_adicionais)));
    }
    if let Some(loc) = &cobv.loc {
        linhas.push(("Location", loc.id.to_string()));
    }
    linhas.push(("txid", txid.to_string()));
    let mut texto = String::new();
    if ambiente.is_some_and(Environment::is_production) {
        texto.push_str("*** PRODUÇÃO: a cobrança vale de verdade ***\n");
    }
    texto.push_str(&secao("Cobrança Pix com vencimento a criar", &linhas));
    let multa_ou_juros = cobv.valor.multa.is_some() || cobv.valor.juros.is_some();
    if cobv.calendario.validade_apos_vencimento == Some(0) && multa_ou_juros {
        texto.push_str(
            "\naviso: sem validade após o vencimento, a cobrança não pode ser paga com atraso, e a multa e os juros nunca valem",
        );
    }
    for data in encargos::datas_do_desconto(cobv.valor.desconto.as_ref()) {
        if data < hoje {
            let _ = write!(
                texto,
                "\naviso: o prazo do desconto até {} já passou",
                data.format("%d/%m/%Y")
            );
        }
    }
    texto
}

/// `até 19/11/2026, 30 dias após o vencimento (padrão da API)`.
fn validade(vencimento: NaiveDate, dias: Option<u32>) -> String {
    match dias {
        Some(dias) => validade_em_dias(vencimento, u64::from(dias), ""),
        None => validade_em_dias(vencimento, u64::from(VALIDADE_PADRAO), " (padrão da API)"),
    }
}

fn validade_em_dias(vencimento: NaiveDate, dias: u64, nota: &str) -> String {
    if dias == 0 {
        return format!("só até o vencimento{nota}");
    }
    let ultimo = vencimento
        .checked_add_days(Days::new(dias))
        .unwrap_or(NaiveDate::MAX);
    let unidade = if dias == 1 { "dia" } else { "dias" };
    format!(
        "até {}, {dias} {unidade} após o vencimento{nota}",
        ultimo.format("%d/%m/%Y")
    )
}

/// Name and document, address and e-mail of a payer to be sent.
fn linhas_do_devedor(devedor: &DevedorCobv) -> Vec<(&'static str, String)> {
    let mut linhas = vec![(
        "Devedor",
        format!("{} ({})", devedor.nome, devedor.documento.formatado()),
    )];
    if let Some(endereco) = endereco_do_devedor(devedor) {
        linhas.push(("Endereço", endereco));
    }
    if let Some(email) = &devedor.email {
        linhas.push(("E-mail", email.clone()));
    }
    linhas
}

fn endereco_do_devedor(devedor: &DevedorCobv) -> Option<String> {
    endereco(
        devedor.logradouro.as_deref(),
        devedor.cidade.as_deref(),
        devedor.uf.map(Uf::as_str),
        devedor.cep.as_deref(),
    )
}

fn endereco_da_pessoa(pessoa: &PessoaPix) -> Option<String> {
    endereco(
        pessoa.logradouro.as_deref(),
        pessoa.cidade.as_deref(),
        pessoa.uf.as_deref(),
        pessoa.cep.as_deref(),
    )
}

/// `Pedido: 123 / Loja: Centro`.
fn infos(infos: &[InfoAdicional]) -> String {
    infos
        .iter()
        .map(|info| format!("{}: {}", info.nome, info.valor))
        .collect::<Vec<_>>()
        .join(" / ")
}

async fn revisar(
    context: &Context,
    args: &PixCobvRevisarArgs,
    terminal: &mut dyn Terminal,
) -> Result<(), CliError> {
    let hoje = hoje();
    // Invalid options are refused before any request, the lookup included,
    // and so is a revision no one could confirm.
    revisao_das_opcoes(args, &Atual::default(), hoje)?;
    pode_confirmar(terminal, args.sim)?;
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let cobv = client.pix().consultar_cobv(&args.txid).await?;
    alteravel(cobv.status.as_ref())?;
    let revisao = revisao_das_opcoes(args, &Atual::de(&cobv), hoje)?;
    let ambiente = settings.ambiente.as_ref().map(|setting| setting.value);
    eprintln!("{}", resumo_revisao(&cobv, &revisao, ambiente));
    let pergunta = if revisao.remover {
        "Remover a cobrança?"
    } else {
        "Alterar a cobrança?"
    };
    confirmar(terminal, args.sim, pergunta)?;

    let revisada = client.pix().revisar_cobv(&args.txid, &revisao).await?;
    match context.formato() {
        Formato::Json => output::print_json(&revisada),
        // `commands::run` refuses csv for this command.
        Formato::Texto | Formato::Csv => {
            let titulo = if revisao.remover {
                "Cobrança Pix com vencimento removida: ela não pode mais ser paga.".to_owned()
            } else {
                match revisada.revisao {
                    Some(numero) => {
                        format!("Cobrança Pix com vencimento alterada (revisão {numero}).")
                    }
                    None => "Cobrança Pix com vencimento alterada.".to_owned(),
                }
            };
            output::print(&format!("{titulo}\n\n{}", render_cobv(&revisada)))
        }
    }
}

/// What a revision needs to know of the charge as it is. Before the lookup,
/// nothing: what depends on it is checked afterwards.
#[derive(Debug, Default)]
struct Atual {
    consultada: bool,
    vencimento: Option<NaiveDate>,
    validade: Option<u32>,
    original: Option<Decimal>,
    /// Last days of its fixed-date discounts.
    datas_do_desconto: Vec<NaiveDate>,
}

impl Atual {
    fn de(cobv: &Cobv) -> Self {
        let calendario = cobv.calendario.as_ref();
        let valor = cobv.valor.as_ref();
        Self {
            consultada: true,
            vencimento: calendario
                .and_then(|calendario| calendario.data_de_vencimento.as_deref())
                .and_then(parse_data),
            validade: calendario
                .and_then(|calendario| calendario.validade_apos_vencimento)
                .and_then(|dias| u32::try_from(dias).ok()),
            original: valor.and_then(|valor| valor.original),
            datas_do_desconto: valor
                .and_then(|valor| valor.desconto.as_ref())
                .map(|desconto| {
                    desconto
                        .desconto_data_fixa
                        .iter()
                        .filter_map(|data| data.data.as_deref().and_then(parse_data))
                        .collect()
                })
                .unwrap_or_default(),
        }
    }
}

/// The revision of the options, checked against the charge as it will be:
/// with the new amount and due date, or the current ones.
fn revisao_das_opcoes(
    args: &PixCobvRevisarArgs,
    atual: &Atual,
    hoje: NaiveDate,
) -> Result<CobvRevisada, CliError> {
    if args.remover {
        return Ok(CobvRevisada::remocao());
    }
    if let Some(vencimento) = args.vencimento {
        vencimento_valido(vencimento, hoje)?;
    }
    // Before the lookup, the dates that depend on the charge are not
    // checked yet.
    let vencimento = match args.vencimento.or(atual.vencimento) {
        Some(vencimento) => vencimento,
        None if atual.consultada
            && (args.validade_apos_vencimento.is_some()
                || args
                    .encargos
                    .desconto
                    .iter()
                    .any(|desconto| desconto.ate.is_none())) =>
        {
            return Err(CliError::Usage(
                "a API não informou o vencimento da cobrança: informe --vencimento também"
                    .to_owned(),
            ));
        }
        None => NaiveDate::MAX,
    };
    let mut revisao = CobvRevisada::new();
    if args.vencimento.is_some() || args.validade_apos_vencimento.is_some() {
        // Both go together, so what is not given stays as it is.
        let mut calendario = CalendarioCobv::new(vencimento);
        calendario.validade_apos_vencimento = args.validade_apos_vencimento.or(atual.validade);
        revisao.calendario = Some(calendario);
    }
    revisao.devedor = devedor(&args.devedor);
    revisao.loc = args.loc.map(LocCob::new);
    let encargos = Encargos::de(&args.encargos, vencimento)?;
    if args.valor.is_some() || !encargos.vazio() {
        let mut valor = ValorCobvRevisada::default();
        valor.original = args.valor;
        valor.multa = encargos.multa;
        valor.juros = encargos.juros;
        valor.abatimento = encargos.abatimento;
        valor.desconto = encargos.desconto;
        revisao.valor = Some(valor);
    }
    revisao.chave.clone_from(&args.chave);
    revisao.solicitacao_pagador.clone_from(&args.solicitacao);
    if !args.info.is_empty() {
        revisao.info_adicionais = Some(args.info.clone());
    }

    let mut como_fica = revisao.clone();
    if como_fica.calendario.is_none()
        && let Some(vencimento) = atual.vencimento
    {
        como_fica.calendario = Some(CalendarioCobv::new(vencimento));
    }
    if let Some(valor) = &mut como_fica.valor
        && valor.original.is_none()
    {
        valor.original = atual.original;
    }
    como_fica.validar().map_err(|err| erro_da_opcao(&err))?;
    // A current discount that would end after the new due date.
    let desconto_novo = revisao
        .valor
        .as_ref()
        .is_some_and(|valor| valor.desconto.is_some());
    if let Some(novo) = args.vencimento
        && !desconto_novo
        && let Some(data) = atual.datas_do_desconto.iter().find(|data| **data > novo)
    {
        return Err(CliError::Usage(format!(
            "--vencimento: o desconto atual vale até {}, depois do novo vencimento; informe também o novo --desconto",
            data.format("%d/%m/%Y")
        )));
    }
    Ok(revisao)
}

/// The charge as it is, and what changes.
fn resumo_revisao(cobv: &Cobv, revisao: &CobvRevisada, ambiente: Option<Environment>) -> String {
    let mut linhas = vec![("Ambiente", descrever_ambiente(ambiente))];
    let calendario = cobv.calendario.as_ref();
    let valor = cobv.valor.as_ref();
    let novo_valor = revisao.valor.as_ref();
    let original = valor.and_then(|valor| valor.original).map(output::brl);
    let novo = novo_valor.and_then(|valor| valor.original).map(output::brl);
    if let Some(linha) = muda(original, novo) {
        linhas.push(("Valor", linha));
    }
    let vencimento = calendario.and_then(|calendario| calendario.data_de_vencimento.as_deref());
    let novo_calendario = revisao.calendario.as_ref();
    let novo_vencimento = novo_calendario
        .map(|calendario| calendario.data_de_vencimento.format("%d/%m/%Y").to_string());
    if let Some(linha) = muda(vencimento.map(data_br), novo_vencimento) {
        linhas.push(("Vencimento", linha));
    }
    let validade_atual = calendario.and_then(|calendario| {
        Some(validade_em_dias(
            parse_data(calendario.data_de_vencimento.as_deref()?)?,
            calendario.validade_apos_vencimento?,
            "",
        ))
    });
    let nova_validade = novo_calendario.map(|calendario| {
        validade(
            calendario.data_de_vencimento,
            calendario.validade_apos_vencimento,
        )
    });
    if let Some(linha) = muda(validade_atual, nova_validade) {
        linhas.push(("Validade", linha));
    }
    linhas.extend(devedor_antes_e_depois(
        cobv.devedor.as_ref(),
        revisao.devedor.as_ref(),
    ));
    let encargos_atuais = valor.map(encargos::linhas_geradas).unwrap_or_default();
    let novos_encargos = novo_valor
        .map(|valor| {
            encargos::linhas(
                valor.multa.as_ref(),
                valor.juros.as_ref(),
                valor.abatimento.as_ref(),
                valor.desconto.as_ref(),
            )
        })
        .unwrap_or_default();
    for rotulo in ["Multa", "Juros", "Abatimento", "Desconto"] {
        let de = |linhas: &[(&str, String)]| {
            linhas
                .iter()
                .find(|(nome, _)| *nome == rotulo)
                .map(|(_, texto)| texto.clone())
        };
        if let Some(linha) = muda(de(&encargos_atuais), de(&novos_encargos)) {
            linhas.push((rotulo, linha));
        }
    }
    if let Some(chave) = &revisao.chave {
        linhas.push(("Chave", format!("→ {}", chave.as_str())));
    }
    if let Some(solicitacao) = &revisao.solicitacao_pagador {
        linhas.push(("Solicitação", format!("→ {solicitacao}")));
    }
    if let Some(novas) = &revisao.info_adicionais {
        linhas.push(("Informações", format!("→ {}", infos(novas))));
    }
    if let Some(loc) = &revisao.loc {
        linhas.push(("Location", format!("→ {}", loc.id)));
    }
    if let Some(status) = &cobv.status {
        linhas.push(("Status", descrever_status(status).to_owned()));
    }
    let titulo = format!(
        "Cobrança Pix com vencimento {} a {}",
        cobv.txid.as_deref().unwrap_or_default(),
        if revisao.remover {
            "remover"
        } else {
            "alterar"
        }
    );
    secao(&titulo, &linhas)
}

/// The payer as it is and, when given, the new one, which replaces it
/// with its address and e-mail.
fn devedor_antes_e_depois(
    devedor: Option<&PessoaPix>,
    novo: Option<&DevedorCobv>,
) -> Vec<(&'static str, String)> {
    let nome =
        |devedor: &DevedorCobv| format!("{} ({})", devedor.nome, devedor.documento.formatado());
    let mut linhas: Vec<(&'static str, String)> = muda(devedor.and_then(pessoa), novo.map(nome))
        .map(|linha| ("Devedor", linha))
        .into_iter()
        .collect();
    let Some(novo) = novo else {
        return linhas;
    };
    let atual = devedor.and_then(endereco_da_pessoa);
    let novo_endereco = endereco_do_devedor(novo);
    if atual.is_some() || novo_endereco.is_some() {
        let novo_endereco = novo_endereco.unwrap_or_else(|| "sem endereço".to_owned());
        linhas.extend(muda(atual, Some(novo_endereco)).map(|linha| ("Endereço", linha)));
    }
    let atual = devedor.and_then(|devedor| devedor.email.clone());
    if atual.is_some() || novo.email.is_some() {
        let novo_email = novo
            .email
            .clone()
            .unwrap_or_else(|| "sem e-mail".to_owned());
        linhas.extend(muda(atual, Some(novo_email)).map(|linha| ("E-mail", linha)));
    }
    linhas
}

/// What changes, `antes → depois`, or what stays when both are the same.
fn muda(antes: Option<String>, depois: Option<String>) -> Option<String> {
    if antes.is_some() && antes == depois {
        antes
    } else {
        antes_e_depois(antes, depois)
    }
}

async fn consultar(context: &Context, args: &PixCobvConsultarArgs) -> Result<(), CliError> {
    let opcoes = OpcoesQr::new(
        context,
        args.qr.qrcode,
        args.qr.qrcode_png.clone(),
        args.qr.sobrescrever,
    )?;
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let cobv = client.pix().consultar_cobv(&args.txid).await?;
    opcoes.mostrar(
        context,
        &settings,
        &render_cobv(&cobv),
        &cobv,
        &copia_e_cola(&cobv),
    )
}

/// The "copia e cola" of a charge that can still be paid, or why there is
/// none.
fn copia_e_cola(cobv: &Cobv) -> Result<&str, String> {
    copia_e_cola_ativa(cobv.status.as_ref(), cobv.pix_copia_e_cola.as_deref())
}

/// A charge with a due date in detail, with the times in the local time
/// zone.
fn render_cobv(cobv: &Cobv) -> String {
    render_cobv_em(cobv, &Local)
}

fn render_cobv_em<Tz: TimeZone>(cobv: &Cobv, fuso: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let mut linhas = Vec::new();
    if let Some(status) = &cobv.status {
        linhas.push(("Status", descrever_status(status).to_owned()));
    }
    let valor = cobv.valor.as_ref();
    if let Some(original) = valor.and_then(|valor| valor.original) {
        linhas.push(("Valor", output::brl(original)));
    }
    if let Some(calendario) = &cobv.calendario {
        if let Some(vencimento) = &calendario.data_de_vencimento {
            linhas.push(("Vencimento", data_br(vencimento)));
            if let (Some(dia), Some(dias)) =
                (parse_data(vencimento), calendario.validade_apos_vencimento)
            {
                linhas.push(("Validade", validade_em_dias(dia, dias, "")));
            }
        }
        if let Some(criacao) = &calendario.criacao {
            linhas.push(("Criada em", horario_em(criacao, fuso)));
        }
    }
    if let Some(devedor) = &cobv.devedor {
        linhas.extend(pessoa(devedor).map(|nome| ("Devedor", nome)));
        linhas.extend(endereco_da_pessoa(devedor).map(|endereco| ("Endereço", endereco)));
        linhas.extend(devedor.email.clone().map(|email| ("E-mail", email)));
    }
    if let Some(recebedor) = cobv.recebedor.as_ref().and_then(pessoa) {
        linhas.push(("Recebedor", recebedor));
    }
    if let Some(chave) = &cobv.chave {
        linhas.push(("Chave", chave.clone()));
    }
    linhas.extend(valor.map(encargos::linhas_geradas).unwrap_or_default());
    if let Some(solicitacao) = &cobv.solicitacao_pagador {
        linhas.push(("Solicitação", solicitacao.clone()));
    }
    if let Some(revisao) = cobv.revisao {
        linhas.push(("Revisão", revisao.to_string()));
    }
    if let Some(location) = cobv.loc.as_ref().and_then(|loc| loc.location.as_deref()) {
        linhas.push(("Location", location.to_owned()));
    }
    let titulo = format!(
        "Cobrança Pix com vencimento {}",
        cobv.txid.as_deref().unwrap_or_default()
    );
    let mut texto = secao(titulo.trim(), &linhas);
    if !cobv.info_adicionais.is_empty() {
        let infos: Vec<(&str, String)> = cobv
            .info_adicionais
            .iter()
            .map(|info| (info.nome.as_str(), info.valor.clone()))
            .collect();
        let _ = write!(texto, "\n\n{}", secao("Informações", &infos));
    }
    if !cobv.pix.is_empty() {
        let _ = write!(
            texto,
            "\n\nPix recebidos\n{}",
            tabela_pix(&cobv.pix, fuso).texto_colorido()
        );
    }
    if let Some(copia_e_cola) = cobv
        .pix_copia_e_cola
        .as_deref()
        .filter(|texto| !texto.is_empty())
    {
        let _ = write!(texto, "\n\nCopia e cola  {copia_e_cola}");
    }
    texto
}

async fn listar(context: &Context, args: &PixCobvListarArgs) -> Result<(), CliError> {
    let filtros = Filtros::de(&args.filtros)?;
    let mut filtro = FiltroCobvs::new(filtros.periodo);
    filtro.devedor.clone_from(&filtros.devedor);
    filtro.status.clone_from(&filtros.status);
    filtro.location_presente = filtros.location_presente;
    filtro.lote_cob_v_id = args.lote;
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let (cobs, pagina) = match args.filtros.pagina {
        Some(numero) => {
            let mut pagina = client
                .pix()
                .listar_cobvs(&filtro, numero, args.filtros.itens_por_pagina)
                .await?;
            if context.formato() == Formato::Json {
                return output::print_json(&pagina);
            }
            let cobs = std::mem::take(&mut pagina.cobs);
            (cobs, Some((numero, pagina)))
        }
        None => (client.pix().listar_todas_cobvs(&filtro).await?, None),
    };
    match context.formato() {
        Formato::Json => output::print_json(&json!({ "cobs": cobs })),
        Formato::Csv => output::print_raw(&csv(&cobs).csv(context.separador())),
        Formato::Texto => {
            context.warn_if_sandbox(&settings);
            let lote: Vec<String> = args
                .lote
                .map(|lote| format!("lote {lote}"))
                .into_iter()
                .collect();
            let mut texto = format!(
                "{}\n\n",
                filtros.titulo("Cobranças Pix com vencimento", &lote)
            );
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

fn tabela(cobs: &[Cobv]) -> Tabela {
    let mut tabela = Tabela::new(vec![
        Coluna::texto("Vencimento", ""),
        Coluna::texto("Status", ""),
        Coluna::valor("Valor", ""),
        Coluna::texto("Devedor", "").no_maximo(LARGURA_DEVEDOR),
        Coluna::texto("txid", ""),
    ]);
    for cobv in cobs {
        let vencimento = cobv
            .calendario
            .as_ref()
            .and_then(|calendario| calendario.data_de_vencimento.as_deref())
            .map(data_br);
        tabela.linha(vec![
            Celula::texto(vencimento.as_deref()),
            celula_status(cobv.status.as_ref()),
            Celula::dinheiro(cobv.valor.as_ref().and_then(|valor| valor.original)),
            Celula::texto(
                cobv.devedor
                    .as_ref()
                    .and_then(|devedor| devedor.nome.as_deref()),
            ),
            Celula::texto(cobv.txid.as_deref()),
        ]);
    }
    tabela
}

/// `3 cobranças · R$ 450,00 · pagas R$ 150,00`.
fn totais(cobs: &[Cobv]) -> String {
    super::totais(cobs.iter().map(|cobv| {
        (
            cobv.valor.as_ref().and_then(|valor| valor.original),
            cobv.status.as_ref(),
        )
    }))
}

/// Every field, with the API's names (nested ones with a dot) and codes;
/// the fixed-date discounts are in the JSON.
fn csv(cobs: &[Cobv]) -> Tabela {
    let texto = |campo: &'static str| Coluna::texto(campo, campo);
    let valor = |campo: &'static str| Coluna::valor(campo, campo);
    let mut tabela = Tabela::new(vec![
        texto("txid"),
        texto("status"),
        texto("revisao"),
        texto("calendario.criacao"),
        texto("calendario.dataDeVencimento"),
        texto("calendario.validadeAposVencimento"),
        valor("valor.original"),
        texto("valor.multa.modalidade"),
        valor("valor.multa.valorPerc"),
        texto("valor.juros.modalidade"),
        valor("valor.juros.valorPerc"),
        texto("valor.abatimento.modalidade"),
        valor("valor.abatimento.valorPerc"),
        texto("valor.desconto.modalidade"),
        valor("valor.desconto.valorPerc"),
        texto("devedor.cpf"),
        texto("devedor.cnpj"),
        texto("devedor.nome"),
        texto("chave"),
        texto("solicitacaoPagador"),
        texto("loc.id"),
        texto("loc.location"),
        texto("pixCopiaECola"),
    ]);
    let numero = |numero: Option<u64>| numero.map(|numero| numero.to_string());
    for cobv in cobs {
        let calendario = cobv.calendario.as_ref();
        let valor = cobv.valor.as_ref();
        let devedor = cobv.devedor.as_ref();
        let mut celulas = vec![
            Celula::texto(cobv.txid.as_deref()),
            Celula::texto(cobv.status.as_ref().map(StatusCob::as_str)),
            Celula::texto(numero(cobv.revisao).as_deref()),
            Celula::texto(calendario.and_then(|c| c.criacao.as_deref())),
            Celula::texto(calendario.and_then(|c| c.data_de_vencimento.as_deref())),
            Celula::texto(numero(calendario.and_then(|c| c.validade_apos_vencimento)).as_deref()),
            Celula::dinheiro(valor.and_then(|v| v.original)),
        ];
        for encargo in [
            valor.and_then(|v| v.multa.as_ref()),
            valor.and_then(|v| v.juros.as_ref()),
            valor.and_then(|v| v.abatimento.as_ref()),
            valor.and_then(|v| v.desconto.as_ref()),
        ] {
            celulas.push(Celula::texto(
                numero(encargo.and_then(|e| e.modalidade)).as_deref(),
            ));
            celulas.push(Celula::dinheiro(encargo.and_then(|e| e.valor_perc)));
        }
        celulas.extend([
            Celula::texto(devedor.and_then(|d| d.cpf.as_deref())),
            Celula::texto(devedor.and_then(|d| d.cnpj.as_deref())),
            Celula::texto(devedor.and_then(|d| d.nome.as_deref())),
            Celula::texto(cobv.chave.as_deref()),
            Celula::texto(cobv.solicitacao_pagador.as_deref()),
            Celula::texto(numero(cobv.loc.as_ref().and_then(|loc| loc.id)).as_deref()),
            Celula::texto(cobv.loc.as_ref().and_then(|loc| loc.location.as_deref())),
            Celula::texto(cobv.pix_copia_e_cola.as_deref()),
        ]);
        tabela.linha(celulas);
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

    fn dia(texto: &str) -> NaiveDate {
        texto.parse().unwrap()
    }

    fn comando(args: &[&str]) -> PixCobvCommand {
        let mut todos = vec!["inter-pj", "pix", "cobv"];
        todos.extend_from_slice(args);
        match Cli::try_parse_from(todos).unwrap().command {
            Command::Pix(PixCommand::Cobv(comando)) => comando,
            outro => panic!("{outro:?}"),
        }
    }

    fn criar_args(extra: &[&str]) -> PixCobvCriarArgs {
        let mut todos = vec![
            "criar",
            "--chave",
            "pix@empresa.example",
            "--valor",
            "150,00",
            "--vencimento",
            "2026-10-20",
            "--devedor-documento",
            "123.456.789-09",
            "--devedor-nome",
            "Fulano de Tal",
        ];
        todos.extend_from_slice(extra);
        match comando(&todos) {
            PixCobvCommand::Criar(args) => *args,
            outro => panic!("{outro:?}"),
        }
    }

    fn revisar_args(extra: &[&str]) -> PixCobvRevisarArgs {
        let mut todos = vec!["revisar", TXID];
        todos.extend_from_slice(extra);
        match comando(&todos) {
            PixCobvCommand::Revisar(args) => *args,
            outro => panic!("{outro:?}"),
        }
    }

    fn cobv(status: &str) -> Cobv {
        serde_json::from_value(json!({
            "calendario": {"criacao": "2026-09-23T20:15:00.358Z", "dataDeVencimento": "2026-10-20", "validadeAposVencimento": 10},
            "txid": TXID,
            "revisao": 0,
            "loc": {"id": 790, "location": "pix.example.com/qr/v2/cobv/9d36b84fc70b478fb95c12729b90ca25", "tipoCob": "cobv"},
            "status": status,
            "devedor": {"cpf": "12345678909", "nome": "Fulano de Tal", "cidade": "Belo Horizonte", "uf": "MG"},
            "recebedor": {"cnpj": "11222333000181", "nome": "Empresa Exemplo Ltda"},
            "valor": {
                "original": "150.00",
                "multa": {"modalidade": 2, "valorPerc": "2.00"},
                "desconto": {"modalidade": 1, "descontoDataFixa": [{"data": "2026-10-15", "valorPerc": "10.00"}]}
            },
            "chave": "pix@empresa.example",
            "infoAdicionais": [{"nome": "Pedido", "valor": "123"}],
            "pixCopiaECola": "00020101021226",
            "pix": [{"endToEndId": "E12345678202609232020abcdef12345", "valor": "140.00", "horario": "2026-10-14T20:20:00.000Z"}]
        }))
        .unwrap()
    }

    #[test]
    fn options_become_the_charge() {
        let cobv = das_opcoes(
            &criar_args(&[
                "--validade-apos-vencimento",
                "0",
                "--juros",
                "0,50",
                "--dias-uteis",
                "--desconto-por-dia",
                "0,5%",
                "--info",
                "Pedido=123",
                "--loc",
                "790",
            ]),
            dia("2026-09-23"),
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(&cobv).unwrap(),
            json!({
                "calendario": {"dataDeVencimento": "2026-10-20", "validadeAposVencimento": 0},
                "devedor": {"cpf": "12345678909", "nome": "Fulano de Tal"},
                "loc": {"id": 790},
                "valor": {
                    "original": "150.00",
                    "juros": {"modalidade": 5, "valorPerc": "0.50"},
                    "desconto": {"modalidade": 6, "valorPerc": "0.50"}
                },
                "chave": "pix@empresa.example",
                "infoAdicionais": [{"nome": "Pedido", "valor": "123"}]
            })
        );
        let err =
            das_opcoes(&criar_args(&["--devedor-endereco", ""]), dia("2026-09-23")).unwrap_err();
        assert!(err.to_string().starts_with("--devedor-endereco: "), "{err}");
        let err = das_opcoes(&criar_args(&[]), dia("2026-10-21")).unwrap_err();
        assert!(err.to_string().contains("já passou"), "{err}");
    }

    #[test]
    fn the_summary_shows_the_charges_and_warns() {
        let cobv = das_opcoes(
            &criar_args(&[
                "--validade-apos-vencimento",
                "0",
                "--multa",
                "2%",
                "--desconto",
                "10,00@2026-10-15",
                "--devedor-cidade",
                "Belo Horizonte",
                "--devedor-uf",
                "MG",
            ]),
            dia("2026-09-23"),
        )
        .unwrap();
        assert_eq!(
            resumo(
                &cobv,
                &TXID.parse().unwrap(),
                dia("2026-10-16"),
                Some(Environment::Sandbox)
            ),
            format!(
                "\
Cobrança Pix com vencimento a criar
  Ambiente    sandbox (dados fictícios)
  Valor       R$ 150,00 (cento e cinquenta reais)
  Vencimento  20/10/2026
  Validade    só até o vencimento
  Chave       pix@empresa.example (e-mail)
  Devedor     Fulano de Tal (123.456.789-09)
  Endereço    Belo Horizonte/MG
  Multa       2%
  Desconto    R$ 10,00 até 15/10/2026
  txid        {TXID}
aviso: sem validade após o vencimento, a cobrança não pode ser paga com atraso, e a multa e os juros nunca valem
aviso: o prazo do desconto até 15/10/2026 já passou"
            )
        );
    }

    #[test]
    fn validity_in_words() {
        let vencimento = dia("2026-10-20");
        assert_eq!(
            validade(vencimento, None),
            "até 19/11/2026, 30 dias após o vencimento (padrão da API)"
        );
        assert_eq!(
            validade(vencimento, Some(1)),
            "até 21/10/2026, 1 dia após o vencimento"
        );
        assert_eq!(validade(vencimento, Some(0)), "só até o vencimento");
        assert_eq!(
            endereco(None, None, None, Some("30110000")).as_deref(),
            Some("CEP 30110-000")
        );
        assert_eq!(endereco(None, None, None, None), None);
    }

    #[test]
    fn a_charge_in_detail() {
        let brasilia = FixedOffset::west_opt(3 * 3600).unwrap();
        assert_eq!(
            render_cobv_em(&cobv("CONCLUIDA"), &brasilia),
            format!(
                "\
Cobrança Pix com vencimento {TXID}
  Status      concluída (paga)
  Valor       R$ 150,00
  Vencimento  20/10/2026
  Validade    até 30/10/2026, 10 dias após o vencimento
  Criada em   23/09/2026 17:15:00
  Devedor     Fulano de Tal (123.456.789-09)
  Endereço    Belo Horizonte/MG
  Recebedor   Empresa Exemplo Ltda (11.222.333/0001-81)
  Chave       pix@empresa.example
  Multa       2%
  Desconto    R$ 10,00 até 15/10/2026
  Revisão     0
  Location    pix.example.com/qr/v2/cobv/9d36b84fc70b478fb95c12729b90ca25

Informações
  Pedido  123

Pix recebidos
Horário                  Valor  Devolvido  endToEndId
14/10/2026 17:20:00  R$ 140,00             E12345678202609232020abcdef12345

Copia e cola  00020101021226"
            )
        );
        assert!(copia_e_cola(&cobv("CONCLUIDA")).is_err());
        assert_eq!(copia_e_cola(&cobv("ATIVA")), Ok("00020101021226"));
    }

    #[test]
    fn a_revision_keeps_what_is_not_given() {
        let hoje = dia("2026-09-23");
        let atual = Atual::de(&cobv("ATIVA"));
        let revisao = revisao_das_opcoes(
            &revisar_args(&["--validade-apos-vencimento", "5"]),
            &atual,
            hoje,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(&revisao).unwrap(),
            json!({"calendario": {"dataDeVencimento": "2026-10-20", "validadeAposVencimento": 5}})
        );
        let revisao = revisao_das_opcoes(
            &revisar_args(&["--vencimento", "2026-10-25", "--desconto", "3%"]),
            &atual,
            hoje,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(&revisao).unwrap(),
            json!({
                "calendario": {"dataDeVencimento": "2026-10-25", "validadeAposVencimento": 10},
                "valor": {"desconto": {"modalidade": 2, "descontoDataFixa": [{"data": "2026-10-25", "valorPerc": "3.00"}]}}
            })
        );
        // Before the lookup, only what does not depend on the charge.
        assert!(
            revisao_das_opcoes(
                &revisar_args(&["--vencimento", "2026-10-01"]),
                &Atual::default(),
                hoje
            )
            .is_ok()
        );
        let err = revisao_das_opcoes(&revisar_args(&["--vencimento", "2026-10-01"]), &atual, hoje)
            .unwrap_err();
        assert!(
            err.to_string()
                .contains("o desconto atual vale até 15/10/2026"),
            "{err}"
        );
        let err = revisao_das_opcoes(&revisar_args(&["--vencimento", "2026-09-22"]), &atual, hoje)
            .unwrap_err();
        assert!(err.to_string().contains("já passou"), "{err}");
    }

    #[test]
    fn the_revision_summary_shows_before_and_after() {
        let atual = cobv("ATIVA");
        let revisao = revisao_das_opcoes(
            &revisar_args(&[
                "--valor",
                "160",
                "--multa",
                "4,00",
                "--devedor-documento",
                "12.345.678/0001-95",
                "--devedor-nome",
                "Empresa Exemplo",
            ]),
            &Atual::de(&atual),
            dia("2026-09-23"),
        )
        .unwrap();
        assert_eq!(
            resumo_revisao(&atual, &revisao, Some(Environment::Sandbox)),
            format!(
                "\
Cobrança Pix com vencimento {TXID} a alterar
  Ambiente    sandbox (dados fictícios)
  Valor       R$ 150,00 → R$ 160,00
  Vencimento  20/10/2026
  Validade    até 30/10/2026, 10 dias após o vencimento
  Devedor     Fulano de Tal (123.456.789-09) → Empresa Exemplo (12.345.678/0001-95)
  Endereço    Belo Horizonte/MG → sem endereço
  Multa       2% → R$ 4,00
  Desconto    R$ 10,00 até 15/10/2026
  Status      ativa"
            )
        );
    }

    #[test]
    fn a_new_payer_replaces_address_and_e_mail() {
        let mut atual = cobv("ATIVA");
        atual.devedor.as_mut().unwrap().email = Some("antigo@exemplo.com.br".to_owned());
        let revisao = revisao_das_opcoes(
            &revisar_args(&[
                "--devedor-documento",
                "123.456.789-09",
                "--devedor-nome",
                "Fulano de Tal",
                "--devedor-cidade",
                "Contagem",
            ]),
            &Atual::de(&atual),
            dia("2026-09-23"),
        )
        .unwrap();
        let resumo = resumo_revisao(&atual, &revisao, None);
        assert!(
            resumo.contains("  Devedor     Fulano de Tal (123.456.789-09)\n"),
            "{resumo}"
        );
        assert!(
            resumo.contains("  Endereço    Belo Horizonte/MG → Contagem\n"),
            "{resumo}"
        );
        assert!(
            resumo.contains("  E-mail      antigo@exemplo.com.br → sem e-mail\n"),
            "{resumo}"
        );
    }

    #[test]
    fn listings_add_up() {
        let cobs = [cobv("CONCLUIDA"), cobv("ATIVA")];
        assert_eq!(totais(&cobs), "2 cobranças · R$ 300,00 · pagas R$ 150,00");
        let csv = csv(&cobs).csv(crate::tabela::Separador::PontoEVirgula);
        assert!(
            csv.contains(";CONCLUIDA;0;2026-09-23T20:15:00.358Z;2026-10-20;10;150,00;2;2,00;;;;;1;;12345678909;;Fulano de Tal;"),
            "{csv}"
        );
        assert!(
            csv.contains(";790;pix.example.com/qr/v2/cobv/9d36b84fc70b478fb95c12729b90ca25;"),
            "{csv}"
        );
    }

    // --- the commands against a mock API ------------------------------------

    async fn cenario(args: &[&str]) -> (testes::Cenario, PixCobvCommand) {
        let mut todos = vec!["pix", "cobv"];
        todos.extend_from_slice(args);
        match testes::cenario(&todos, "cobv.write cobv.read").await {
            (cenario, Command::Pix(PixCommand::Cobv(comando))) => (cenario, comando),
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
            "--chave",
            "pix@empresa.example",
            "--valor",
            "10",
            "--vencimento",
            "2099-10-20",
            "--devedor-documento",
            "123.456.789-09",
            "--devedor-nome",
            "Fulano de Tal",
        ])
        .await;
        let PixCobvCommand::Criar(args) = comando else {
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
        let PixCobvCommand::Revisar(args) = comando else {
            unreachable!()
        };
        let atual: Value = serde_json::to_value(cobv("ATIVA")).unwrap();
        Mock::given(method("GET"))
            .and(path(format!("/pix/v2/cobv/{TXID}")))
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
