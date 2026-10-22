//! `inter-pj pagamento lote enviar|consultar|modelo`

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use chrono::NaiveDate;
use inter_pj::banking::{
    ItemLote, Lote, LotePagamentos, PagamentoDoLote, SolicitacaoLote, StatusBoletoDoLote,
    StatusDarfDoLote, StatusLote,
};
use inter_pj::boleto::TipoCodigo;
use inter_pj::{Environment, InterClient, endpoint};

use super::{darf, pagar};
use crate::arquivo::{self, ArquivoLote, MODELO_CSV, MODELO_JSON};
use crate::chamada::{argumento, chamada};
use crate::cli::{
    Formato, LoteCommand, LoteConsultarArgs, LoteEnviarArgs, LoteModeloArgs, TipoArquivo,
};
use crate::commands::{Context, hoje, passageiro, simulacao};
use crate::confirmacao::{Terminal, confirmar, descrever_ambiente, verificar_limite};
use crate::error::CliError;
use crate::output::{self, data_hora_br};
use crate::tabela::{Celula, Coluna, Tabela};
use crate::valor::por_extenso;

/// Between two queries with `--aguardar`: 10 per minute, within the rate
/// limit of both environments (20/min in production, 10/min in sandbox).
const INTERVALO: Duration = Duration::from_secs(6);

pub(super) async fn run(
    context: &Context,
    command: LoteCommand,
    terminal: &mut dyn Terminal,
) -> Result<(), CliError> {
    match command {
        LoteCommand::Enviar(args) => enviar(context, &args, terminal).await,
        LoteCommand::Consultar(args) => consultar(context, &args).await,
        LoteCommand::Modelo(args) => modelo(&args),
    }
}

// --- enviar ----------------------------------------------------------------------

async fn enviar(
    context: &Context,
    args: &LoteEnviarArgs,
    terminal: &mut dyn Terminal,
) -> Result<(), CliError> {
    let hoje = hoje();
    let arquivo = arquivo::ler_lote(&args.arquivo, hoje)?;
    let lote = LotePagamentos {
        meu_identificador: args
            .identificador
            .clone()
            .or_else(|| arquivo.meu_identificador.clone()),
        pagamentos: arquivo
            .pagamentos
            .iter()
            .map(|(_, item)| item.clone())
            .collect(),
    };
    lote.validar()
        .map_err(|err| CliError::Usage(err.to_string()))?;
    // With --sim, a warning would not stop a payment made twice.
    let repetidos = repetidos(&arquivo.pagamentos);
    if !repetidos.is_empty() && !args.permitir_repetidos {
        return Err(CliError::PagamentosRepetidos {
            arquivo: arquivo::nome(&args.arquivo),
            repetidos,
        });
    }
    let settings = context.settings()?;
    // The limit is per payment, as for single payments.
    for (onde, item) in &arquivo.pagamentos {
        verificar_limite(item.valor(), &settings)
            .map_err(|err| CliError::Usage(format!("{onde}: {err}")))?;
    }
    // Configuration problems show up before the confirmation, not after it.
    let client = if args.simular {
        None
    } else {
        Some(context.client(&settings)?)
    };
    let ambiente = settings.ambiente.as_ref().map(|setting| setting.value);
    let origem = arquivo::nome(&args.arquivo);
    eprintln!("{}", resumo(&origem, &arquivo, &lote, hoje, ambiente));

    let Some(client) = client else {
        return simulacao::mostrar(
            context,
            &settings,
            endpoint::banking::PAGAMENTO_LOTE_INCLUIR,
            &[],
            &lote,
        );
    };
    let pergunta = format!(
        "Enviar o lote de {} pagamentos ({})?",
        lote.pagamentos.len(),
        output::brl(lote.valor_total())
    );
    confirmar(terminal, args.sim, &pergunta)?;
    let solicitacao = client.banking().enviar_lote(&lote).await.map_err(|err| {
        if err.may_have_been_processed() {
            CliError::PagamentoIncerto {
                source: err,
                situacao: "o lote pode ter sido recebido e pago",
                // No endpoint lists batches: their payments show up in the
                // listings of each kind.
                consulta: format!(
                    "{0} pagamento boleto listar e {0} pagamento darf listar",
                    chamada()
                ),
                // Both listings ask for the scope of payments by barcode.
                escopos: "pagamento-boleto.read",
            }
        } else {
            err.into()
        }
    })?;
    match context.formato() {
        Formato::Json => output::print_json(&solicitacao)?,
        // `commands::run` refuses csv for this command.
        Formato::Texto | Formato::Csv => output::print(&render_envio(&solicitacao))?,
    }
    if solicitacao.status == Some(StatusLote::ProcessadoComErro) {
        return Err(CliError::LoteComErro {
            detalhe: "a resposta do envio diz que houve erro: confira cada pagamento antes de qualquer reenvio".to_owned(),
        });
    }
    Ok(())
}

/// The batch about to be sent: totals by kind, every payment and what
/// deserves a second look.
fn resumo(
    origem: &str,
    arquivo: &ArquivoLote,
    lote: &LotePagamentos,
    hoje: NaiveDate,
    ambiente: Option<Environment>,
) -> String {
    let total = lote.valor_total();
    let extenso = por_extenso(total)
        .map(|extenso| format!(" ({extenso})"))
        .unwrap_or_default();
    let mut linhas = vec![
        ("Ambiente", descrever_ambiente(ambiente)),
        ("Arquivo", origem.to_owned()),
    ];
    if let Some(identificador) = &lote.meu_identificador {
        linhas.push(("Identificador", identificador.clone()));
    }
    linhas.push(("Pagamentos", por_tipo(lote)));
    linhas.push(("Total", format!("{}{extenso}", output::brl(total))));
    let (tabela, avisos) = pagamentos(arquivo, hoje);

    let mut texto = String::new();
    if ambiente.is_some_and(Environment::is_production) {
        texto.push_str("*** PRODUÇÃO: este lote movimenta dinheiro da conta real ***\n");
    }
    texto.push_str("Lote a enviar");
    for linha in output::key_values_left(&linhas).lines() {
        let _ = write!(texto, "\n  {linha}");
    }
    texto.push('\n');
    for linha in tabela.texto().lines() {
        let _ = write!(texto, "\n  {linha}");
    }
    for aviso in avisos {
        let _ = write!(texto, "\naviso: {aviso}");
    }
    texto
}

/// `2 boletos e contas (R$ 95,43) e 1 DARF (R$ 47,14)`.
fn por_tipo(lote: &LotePagamentos) -> String {
    let (boletos, darfs): (Vec<&ItemLote>, Vec<&ItemLote>) = lote
        .pagamentos
        .iter()
        .partition(|item| matches!(item, ItemLote::Boleto(_)));
    let parte = |itens: &[&ItemLote], um: &str, varios: &str| {
        let soma = output::brl(itens.iter().map(|item| item.valor()).sum());
        match itens.len() {
            0 => None,
            1 => Some(format!("1 {um} ({soma})")),
            n => Some(format!("{n} {varios} ({soma})")),
        }
    };
    [
        parte(&boletos, "boleto ou conta", "boletos e contas"),
        parte(&darfs, "DARF", "DARFs"),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" e ")
}

/// The payments, one per row, and the warnings about them.
fn pagamentos(arquivo: &ArquivoLote, hoje: NaiveDate) -> (Tabela, Vec<String>) {
    let mut tabela = Tabela::new(vec![
        Coluna::texto("Onde", ""),
        Coluna::texto("Tipo", ""),
        Coluna::texto("Documento", ""),
        Coluna::texto("Vencimento", ""),
        Coluna::texto("Quando", ""),
        Coluna::valor("Valor", ""),
    ]);
    let mut avisos = Vec::new();
    for (onde, item) in &arquivo.pagamentos {
        let (tipo, documento, vencimento, quando, deste) = match item {
            ItemLote::Boleto(boleto) => (
                match boleto.codigo.tipo() {
                    TipoCodigo::Boleto => "boleto",
                    _ => "conta/tributo",
                },
                boleto.codigo.linha_formatada(),
                boleto.data_vencimento,
                boleto.data_pagamento,
                pagar::avisos(boleto, hoje),
            ),
            ItemLote::Darf(item) => (
                "DARF",
                format!(
                    "receita {} · {} ({})",
                    item.codigo_receita,
                    item.nome_empresa,
                    item.cnpj_cpf.formatado()
                ),
                item.data_vencimento,
                None,
                darf::avisos(item, hoje),
            ),
            _ => continue,
        };
        tabela.linha(vec![
            Celula::texto(Some(onde)),
            Celula::texto(Some(tipo)),
            Celula::texto(Some(&documento)),
            Celula::Data(vencimento),
            quando.map_or_else(|| Celula::texto(Some("agora")), Celula::Data),
            Celula::Dinheiro(item.valor()),
        ]);
        avisos.extend(deste.into_iter().map(|aviso| format!("{onde}: {aviso}")));
    }
    avisos.extend(repetidos(&arquivo.pagamentos));
    (tabela, avisos)
}

/// Payments that appear more than once: the same code, or the same DARF.
fn repetidos(pagamentos: &[(String, ItemLote)]) -> Vec<String> {
    let chave = |item: &ItemLote| match item {
        ItemLote::Boleto(boleto) => Some(boleto.codigo.codigo_barras().to_owned()),
        ItemLote::Darf(darf) => Some(format!(
            "{}|{}|{}|{}|{}",
            darf.cnpj_cpf.as_str(),
            darf.codigo_receita,
            darf.periodo_apuracao,
            darf.referencia,
            darf.valor_total()
        )),
        _ => None,
    };
    let mut grupos: Vec<(String, Vec<&str>)> = Vec::new();
    for (onde, item) in pagamentos {
        let Some(chave) = chave(item) else { continue };
        match grupos.iter_mut().find(|(outra, _)| *outra == chave) {
            Some((_, ondes)) => ondes.push(onde),
            None => grupos.push((chave, vec![onde])),
        }
    }
    grupos
        .into_iter()
        .filter(|(_, ondes)| ondes.len() > 1)
        .map(|(_, ondes)| {
            let (ultimo, primeiros) = ondes.split_last().unwrap_or((&"", &[]));
            format!(
                "{} e {ultimo}: o mesmo pagamento aparece mais de uma vez; confira se não é um pagamento em dobro",
                primeiros.join(", ")
            )
        })
        .collect()
}

fn render_envio(solicitacao: &SolicitacaoLote) -> String {
    let quantos = solicitacao
        .qtde_pagamentos
        .map_or_else(|| "pagamentos".to_owned(), |n| format!("{n} pagamentos"));
    let status = solicitacao
        .status
        .as_ref()
        .map_or_else(|| "sem status".to_owned(), descrever_status_lote);
    let mut texto = format!("Lote recebido: {quantos}, {status}.");
    // The id of the API goes cleaned, and quoted into the command to paste.
    let id = solicitacao
        .id_lote
        .as_deref()
        .map(|id| output::limpo(id).into_owned());
    let mut linhas = Vec::new();
    if let Some(id) = &id {
        linhas.push(("Identificador do lote", id.clone()));
    }
    if let Some(identificador) = &solicitacao.meu_identificador {
        linhas.push(("Meu identificador", identificador.clone()));
    }
    if !linhas.is_empty() {
        let _ = write!(texto, "\n{}", output::key_values_left(&linhas));
    }
    if let Some(id) = &id {
        let _ = write!(
            texto,
            "\n\nAcompanhe com: {} pagamento lote consultar {} --aguardar",
            chamada(),
            argumento(id)
        );
    }
    texto
}

// --- consultar ---------------------------------------------------------------------

async fn consultar(context: &Context, args: &LoteConsultarArgs) -> Result<(), CliError> {
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let id = args.id_lote.trim();
    if !args.aguardar {
        let lote = client.banking().consultar_lote(id).await?;
        return mostrar(context, &lote);
    }
    let (lote, terminou) = match aguardar(&client, id, args.timeout, INTERVALO).await {
        Ok(espera) => espera,
        Err((ultimo, err)) => {
            // What was known before the error: the batch may be paying.
            if let Some(lote) = ultimo {
                mostrar(context, &lote)?;
            }
            return Err(err);
        }
    };
    mostrar(context, &lote)?;
    let contagem = Contagem::de(&lote);
    if !terminou {
        let status = if lote.status.as_ref().is_some_and(StatusLote::is_final) {
            format!(
                "com {} de {} pagamentos aguardando aprovação ou em processamento",
                contagem.pendentes,
                contagem.total()
            )
        } else {
            lote.status
                .as_ref()
                .map_or_else(|| "sem status".to_owned(), descrever_status_lote)
        };
        return Err(CliError::TempoEsgotado {
            oque: "o lote",
            status,
            segundos: args.timeout.as_secs(),
        });
    }
    if contagem.nao_feitos > 0 || lote.status == Some(StatusLote::ProcessadoComErro) {
        return Err(CliError::LoteComErro {
            detalhe: falhas(&contagem),
        });
    }
    if contagem.agendados > 0 {
        let (um, varios) = ("pagamento agendado sai", "pagamentos agendados saem");
        eprintln!(
            "aviso: {} {} da conta na data do agendamento",
            contagem.agendados,
            if contagem.agendados == 1 { um } else { varios }
        );
    }
    Ok(())
}

/// Queries every `intervalo` until the batch is processed and none of its
/// payments is still pending, or `timeout` passes; the last query happens
/// at the deadline. Status changes are reported on stderr.
///
/// A query that fails for a while (a `429`, a `5xx`, the network) does not
/// end the wait: the batch goes on being paid, and the next query may
/// answer. Any other error ends it, with the last batch seen.
async fn aguardar(
    client: &InterClient,
    id: &str,
    timeout: Duration,
    intervalo: Duration,
) -> Result<(Lote, bool), (Option<Box<Lote>>, CliError)> {
    let prazo = Instant::now() + timeout;
    let mut anterior: Option<String> = None;
    let mut ultimo: Option<Lote> = None;
    loop {
        let resposta = client.banking().consultar_lote(id).await;
        let agora = Instant::now();
        match resposta {
            Ok(lote) => {
                if terminou(&lote) {
                    return Ok((lote, true));
                }
                if agora >= prazo {
                    return Ok((lote, false));
                }
                let texto = descrever_espera(&lote);
                if anterior.as_ref() != Some(&texto) {
                    eprintln!("aguardando: {texto}");
                    anterior = Some(texto);
                }
                ultimo = Some(lote);
            }
            Err(err) if passageiro(&err) => {
                if agora >= prazo {
                    return match ultimo {
                        Some(lote) => Ok((lote, false)),
                        None => Err((None, err.into())),
                    };
                }
                eprintln!(
                    "aguardando: a consulta falhou ({}); nova tentativa em {} s",
                    output::limpo(&err.to_string()),
                    intervalo.min(prazo - agora).as_secs()
                );
            }
            Err(err) => return Err((ultimo.map(Box::new), err.into())),
        }
        tokio::time::sleep(intervalo.min(prazo - agora)).await;
    }
}

/// Whether the wait is over: the batch is processed, and none of its
/// payments is still waiting for an approval or being processed.
fn terminou(lote: &Lote) -> bool {
    lote.status.as_ref().is_some_and(StatusLote::is_final) && Contagem::de(lote).pendentes == 0
}

/// The status of a batch still awaited, for the progress on stderr.
fn descrever_espera(lote: &Lote) -> String {
    let status = lote
        .status
        .as_ref()
        .map_or_else(|| "sem status".to_owned(), descrever_status_lote);
    let pendentes = Contagem::de(lote).pendentes;
    if lote.status.as_ref().is_some_and(StatusLote::is_final) && pendentes > 0 {
        format!("{status}, {pendentes} pagamentos aguardando aprovação ou em processamento")
    } else {
        status
    }
}

/// Where a payment of the batch stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Situacao {
    /// Paid.
    Pago,
    /// Scheduled: it leaves the account on its day.
    Agendado,
    /// Not paid yet: being processed, approved but not paid, or waiting
    /// for an approval in the Internet Banking.
    Pendente,
    /// Not paid: an error, refused, cancelled, expired, not cleared.
    NaoFeito,
    /// A status the CLI does not know, or none: not confirmed.
    Desconhecida,
}

fn situacao(pagamento: &PagamentoDoLote) -> Situacao {
    use StatusBoletoDoLote as B;
    use StatusDarfDoLote as D;
    match pagamento {
        PagamentoDoLote::Boleto(boleto) => match &boleto.status {
            Some(B::Realizado | B::Pago | B::AgendadoRealizado) => Situacao::Pago,
            Some(B::Agendado | B::PagamentoAgendado | B::PagamentoCobrancaAgendado) => {
                Situacao::Agendado
            }
            Some(
                B::EmProcessamento
                | B::AguardandoAprovacao
                | B::Aprovado
                | B::AprovadoNovoPagamento
                | B::AprovadoAguardoRetentativa,
            ) => Situacao::Pendente,
            Some(
                B::Cancelado
                | B::Reprovado
                | B::Erro
                | B::NaoCompensado
                | B::AgendadoNaoRealizado
                | B::AgendadoCancelado
                | B::AprovacaoExpirada
                | B::ErroPagamento,
            ) => Situacao::NaoFeito,
            _ => Situacao::Desconhecida,
        },
        PagamentoDoLote::Darf(darf) => match &darf.status {
            Some(D::Pago) => Situacao::Pago,
            Some(D::PagamentoAgendado) => Situacao::Agendado,
            Some(D::EmProcessamento | D::AguardandoAprovacao | D::Aprovado) => Situacao::Pendente,
            Some(D::AgendamentoCancelado | D::NaoCompensado | D::ErroPagamento | D::Cancelado) => {
                Situacao::NaoFeito
            }
            _ => Situacao::Desconhecida,
        },
        _ => Situacao::Desconhecida,
    }
}

/// How many payments of the batch stand where.
#[derive(Debug, Default, PartialEq, Eq)]
struct Contagem {
    pagos: usize,
    agendados: usize,
    /// Still being processed or waiting for an approval, and those the API
    /// counted (`qtdePagamentos`) but did not list yet.
    pendentes: usize,
    /// Not paid, or of a status not known.
    nao_feitos: usize,
    desconhecidos: usize,
}

impl Contagem {
    fn de(lote: &Lote) -> Self {
        let mut contagem = Self::default();
        for pagamento in &lote.pagamentos {
            match situacao(pagamento) {
                Situacao::Pago => contagem.pagos += 1,
                Situacao::Agendado => contagem.agendados += 1,
                Situacao::Pendente => contagem.pendentes += 1,
                Situacao::NaoFeito => contagem.nao_feitos += 1,
                Situacao::Desconhecida => {
                    contagem.nao_feitos += 1;
                    contagem.desconhecidos += 1;
                }
            }
        }
        let listados = lote.pagamentos.len();
        let contados = lote
            .qtde_pagamentos
            .and_then(|n| usize::try_from(n).ok())
            .unwrap_or(listados);
        contagem.pendentes += contados.saturating_sub(listados);
        contagem
    }

    fn total(&self) -> usize {
        self.pagos + self.agendados + self.pendentes + self.nao_feitos
    }
}

/// How many payments of a processed batch were not made, or not confirmed.
fn falhas(contagem: &Contagem) -> String {
    let total = contagem.total();
    let feitos = match contagem.nao_feitos - contagem.desconhecidos {
        0 => None,
        1 => Some(format!("1 de {total} pagamentos não foi feito")),
        n => Some(format!("{n} de {total} pagamentos não foram feitos")),
    };
    let desconhecidos = match contagem.desconhecidos {
        0 => None,
        1 => Some("1 tem um status que a CLI não conhece".to_owned()),
        n => Some(format!("{n} têm um status que a CLI não conhece")),
    };
    match (feitos, desconhecidos) {
        (None, None) => "a API diz que houve erro: confira o status de cada pagamento".to_owned(),
        (Some(feitos), None) => feitos,
        (None, Some(desconhecidos)) => {
            format!("de {total} pagamentos, {desconhecidos}: confira no Internet Banking")
        }
        (Some(feitos), Some(desconhecidos)) => {
            format!("{feitos}, e {desconhecidos}: confira no Internet Banking")
        }
    }
}

fn mostrar(context: &Context, lote: &Lote) -> Result<(), CliError> {
    match context.formato() {
        Formato::Json => output::print_json(lote),
        // `commands::run` refuses csv for this command.
        Formato::Texto | Formato::Csv => output::print(&render_lote(lote)),
    }
}

fn render_lote(lote: &Lote) -> String {
    let mut linhas = Vec::new();
    if let Some(status) = &lote.status {
        linhas.push(("Status", descrever_status_lote(status)));
    }
    if let Some(identificador) = &lote.meu_identificador {
        linhas.push(("Meu identificador", identificador.clone()));
    }
    if let Some(data) = &lote.data_criacao {
        linhas.push(("Criado em", data_hora_br(data)));
    }
    if let Some(quantidade) = lote.qtde_pagamentos {
        linhas.push(("Pagamentos", quantidade.to_string()));
    }
    let mut texto = match &lote.id_lote {
        Some(id) => format!("Lote {id}"),
        None => "Lote".to_owned(),
    };
    for linha in output::key_values_left(&linhas).lines() {
        let _ = write!(texto, "\n  {linha}");
    }
    if lote.pagamentos.is_empty() {
        return texto;
    }
    let mut tabela = Tabela::new(vec![
        Coluna::texto("Tipo", ""),
        Coluna::texto("Status", ""),
        Coluna::valor("Valor", ""),
        Coluna::texto("Código", ""),
        Coluna::texto("Detalhe", "").no_maximo(60),
    ]);
    for pagamento in &lote.pagamentos {
        let (status, valor, codigo, detalhe) = match pagamento {
            PagamentoDoLote::Boleto(boleto) => (
                boleto.status.as_ref().map(descrever_status_boleto),
                boleto.valor_pagar,
                boleto.codigo_transacao.as_deref(),
                boleto.detalhe.as_deref(),
            ),
            PagamentoDoLote::Darf(darf) => (
                darf.status.as_ref().map(descrever_status_darf),
                darf.valor_total.or(darf.valor),
                darf.codigo_solicitacao.as_deref(),
                darf.detalhe.as_deref(),
            ),
            _ => (None, None, None, None),
        };
        let tipo = match pagamento {
            PagamentoDoLote::Boleto(_) => "boleto",
            PagamentoDoLote::Darf(_) => "DARF",
            outro => outro.tipo().unwrap_or("?"),
        };
        tabela.linha(vec![
            Celula::texto(Some(tipo)),
            Celula::texto(status),
            Celula::dinheiro(valor),
            Celula::texto(codigo),
            Celula::texto(detalhe),
        ]);
    }
    let _ = write!(texto, "\n\n{}", tabela.texto());
    texto
}

fn descrever_status_lote(status: &StatusLote) -> String {
    match status {
        StatusLote::EmProcessamento => "em processamento",
        StatusLote::ProcessadoComErro => "processado com erro",
        StatusLote::ProcessadoSemErro => "processado sem erro",
        other => other.as_str(),
    }
    .to_owned()
}

fn descrever_status_boleto(status: &StatusBoletoDoLote) -> &str {
    match status {
        StatusBoletoDoLote::EmProcessamento => "em processamento",
        StatusBoletoDoLote::Realizado | StatusBoletoDoLote::Pago => "pago",
        StatusBoletoDoLote::Agendado | StatusBoletoDoLote::PagamentoAgendado => "agendado",
        StatusBoletoDoLote::AguardandoAprovacao => "aguardando aprovação",
        StatusBoletoDoLote::Aprovado => "aprovado",
        StatusBoletoDoLote::Cancelado => "cancelado",
        StatusBoletoDoLote::Reprovado => "reprovado",
        StatusBoletoDoLote::Erro => "erro",
        StatusBoletoDoLote::NaoCompensado => "não compensado",
        StatusBoletoDoLote::AprovadoNovoPagamento => "aprovado para novo pagamento",
        StatusBoletoDoLote::AprovadoAguardoRetentativa => "aprovado, aguardando nova tentativa",
        StatusBoletoDoLote::AgendadoRealizado => "agendado e pago",
        StatusBoletoDoLote::AgendadoNaoRealizado => "agendado e não pago",
        StatusBoletoDoLote::AgendadoCancelado => "agendamento cancelado",
        StatusBoletoDoLote::AprovacaoExpirada => "aprovação expirada",
        StatusBoletoDoLote::ErroPagamento => "erro no pagamento",
        StatusBoletoDoLote::PagamentoCobrancaAgendado => "cobrança agendada",
        other => other.as_str(),
    }
}

fn descrever_status_darf(status: &StatusDarfDoLote) -> &str {
    match status {
        StatusDarfDoLote::EmProcessamento => "em processamento",
        StatusDarfDoLote::Pago => "pago",
        StatusDarfDoLote::PagamentoAgendado => "agendado",
        StatusDarfDoLote::AgendamentoCancelado => "agendamento cancelado",
        StatusDarfDoLote::NaoCompensado => "não compensado",
        StatusDarfDoLote::ErroPagamento => "erro no pagamento",
        StatusDarfDoLote::AguardandoAprovacao => "aguardando aprovação",
        StatusDarfDoLote::Aprovado => "aprovado",
        StatusDarfDoLote::Cancelado => "cancelado",
        other => other.as_str(),
    }
}

// --- modelo ------------------------------------------------------------------------

fn modelo(args: &LoteModeloArgs) -> Result<(), CliError> {
    output::print_raw(match args.tipo {
        TipoArquivo::Json => MODELO_JSON,
        TipoArquivo::Csv => MODELO_CSV,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;

    use clap::{CommandFactory, FromArgMatches};
    use serde_json::json;
    use wiremock::matchers::{any, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::cli::{Cli, Command, PagamentoCommand};
    use crate::commands::Env;
    use crate::confirmacao::testes::TerminalFalso;

    const ID_LOTE: &str = "0123456789abcdef01234567";
    /// R$ 30,10 due on 2026-10-10; the water bill of the sandbox, R$ 65,33.
    const CSV: &str = "\
tipoPagamento;codBarraLinhaDigitavel;valorPagar;dataVencimento;dataPagamento;cnpjCpf;codigoReceita;periodoApuracao;valorPrincipal;referencia;descricao;nomeEmpresa
BOLETO;07797.77705 11678.471159 90071.126347 1 15950000003010;;;;;;;;;;
BOLETO;82670000000653301602023123106000000002830894;65,33;2026-10-10;2026-10-09;;;;;;;
DARF;;;2026-10-30;;12.345.678/0001-95;0220;2026-09-30;47,14;13609400849201739;IRPJ de setembro;Empresa Exemplo
";

    fn dia(mes: u32, dia: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, mes, dia).unwrap()
    }

    fn ler(conteudo: &str) -> (tempfile::TempDir, ArquivoLote) {
        let dir = tempfile::tempdir().unwrap();
        let caminho = dir.path().join("lote.csv");
        fs::write(&caminho, conteudo).unwrap();
        let arquivo = arquivo::ler_lote(&caminho, dia(9, 23)).unwrap();
        (dir, arquivo)
    }

    fn lote_de(arquivo: &ArquivoLote) -> LotePagamentos {
        LotePagamentos {
            meu_identificador: Some("Outubro".to_owned()),
            pagamentos: arquivo
                .pagamentos
                .iter()
                .map(|(_, item)| item.clone())
                .collect(),
        }
    }

    #[test]
    fn summary_shows_totals_every_payment_and_warnings() {
        let (_dir, arquivo) = ler(CSV);
        let lote = lote_de(&arquivo);
        assert_eq!(
            resumo("lote.csv", &arquivo, &lote, dia(9, 23), Some(Environment::Sandbox)),
            "\
Lote a enviar
  Ambiente       sandbox (dados fictícios)
  Arquivo        lote.csv
  Identificador  Outubro
  Pagamentos     2 boletos e contas (R$ 95,43) e 1 DARF (R$ 47,14)
  Total          R$ 142,57 (cento e quarenta e dois reais e cinquenta e sete centavos)

  Onde     Tipo           Documento                                                Vencimento  Quando         Valor
  linha 2  boleto         07797.77705 11678.471159 90071.126347 1 15950000003010   10/10/2026  agora       R$ 30,10
  linha 3  conta/tributo  82670000000-1 65330160202-1 31231060000-1 00002830894-8  10/10/2026  09/10/2026  R$ 65,33
  linha 4  DARF           receita 0220 · Empresa Exemplo (12.345.678/0001-95)      30/10/2026  agora       R$ 47,14"
        );

        // Late payments, and the same code twice.
        let repetido =
            format!("{CSV}BOLETO;07797777051167847115990071126347115950000003010;31,00;;;;;;;;;\n");
        let (_dir, arquivo) = ler(&repetido);
        let lote = lote_de(&arquivo);
        let texto = resumo(
            "lote.csv",
            &arquivo,
            &lote,
            dia(11, 3),
            Some(Environment::Production),
        );
        assert!(
            texto.starts_with("*** PRODUÇÃO: este lote movimenta dinheiro da conta real ***\n"),
            "{texto}"
        );
        for aviso in [
            "aviso: linha 2: o pagamento fica para depois do vencimento (10/10/2026): pode haver juros e multa, ou recusa",
            "aviso: linha 4: o DARF venceu em 30/10/2026 e não tem multa nem juros",
            "aviso: linha 5: o valor a pagar (R$ 31,00) é maior que o do código (R$ 30,10): confira juros e multa",
            "aviso: linha 2 e linha 5: o mesmo pagamento aparece mais de uma vez; confira se não é um pagamento em dobro",
        ] {
            assert!(texto.contains(aviso), "{aviso}\n{texto}");
        }
    }

    #[test]
    fn repeated_payments_are_grouped() {
        let (_dir, arquivo) = ler(&format!(
            "{CSV}{}",
            "BOLETO;07797777051167847115990071126347115950000003010;;;;;;;;;;\n\
             DARF;;;2026-10-30;;12345678000195;0220;2026-09-30;47.14;13609400849201739;Outra descrição;Outro nome\n\
             BOLETO;07797777051167847115990071126347115950000003010;;;;;;;;;;\n"
        ));
        assert_eq!(
            repetidos(&arquivo.pagamentos),
            [
                "linha 2, linha 5 e linha 7: o mesmo pagamento aparece mais de uma vez; confira se não é um pagamento em dobro",
                "linha 4 e linha 6: o mesmo pagamento aparece mais de uma vez; confira se não é um pagamento em dobro",
            ]
        );
        let (_dir, arquivo) = ler(CSV);
        assert_eq!(repetidos(&arquivo.pagamentos), Vec::<String>::new());
    }

    #[test]
    fn renders_the_answers() {
        let solicitacao: SolicitacaoLote = serde_json::from_value(json!({
            "idLote": ID_LOTE,
            "status": "EMPROCESSAMENTO",
            "meuIdentificador": "Outubro",
            "qtdePagamentos": 3
        }))
        .unwrap();
        assert_eq!(
            render_envio(&solicitacao),
            format!(
                "\
Lote recebido: 3 pagamentos, em processamento.
Identificador do lote  {ID_LOTE}
Meu identificador      Outubro

Acompanhe com: inter-pj pagamento lote consultar {ID_LOTE} --aguardar"
            )
        );

        let lote: Lote = serde_json::from_value(json!({
            "idLote": ID_LOTE,
            "status": "PROCESSADOCOMERRO",
            "meuIdentificador": "Outubro",
            "qtdePagamentos": 3,
            "contaCorrente": "7654321",
            "dataCriacao": "2026-10-01T10:00:00",
            "pagamentos": [
                {"tipoPagamento": "BOLETO", "status": "AGENDADO", "valorPagar": 30.1, "codigoTransacao": "3414f226-36fb-4d87-811e-cfd99911d845"},
                {"tipoPagamento": "DARF", "status": "ERRO_PAGAMENTO", "valorTotal": 47.14, "detalhe": "Saldo insuficiente"},
                {"tipoPagamento": "PIX", "valor": 1}
            ]
        }))
        .unwrap();
        let texto = render_lote(&lote);
        assert_eq!(
            texto,
            format!(
                "\
Lote {ID_LOTE}
  Status             processado com erro
  Meu identificador  Outubro
  Criado em          01/10/2026 10:00:00
  Pagamentos         3

Tipo    Status                Valor  Código                                Detalhe
boleto  agendado           R$ 30,10  3414f226-36fb-4d87-811e-cfd99911d845
DARF    erro no pagamento  R$ 47,14                                        Saldo insuficiente
PIX"
            )
        );
        // The account of the batch is the user's own: never shown.
        assert!(!texto.contains("7654321"), "{texto}");
        assert_eq!(
            falhas(&Contagem::de(&lote)),
            "1 de 3 pagamentos não foi feito, e 1 tem um status que a CLI não conhece: confira no Internet Banking"
        );
    }

    #[test]
    fn every_documented_status_is_described() {
        for status in StatusLote::DOCUMENTADOS {
            assert_ne!(descrever_status_lote(status), status.as_str(), "{status}");
        }
        for status in StatusBoletoDoLote::DOCUMENTADOS {
            assert_ne!(descrever_status_boleto(status), status.as_str(), "{status}");
        }
        for status in StatusDarfDoLote::DOCUMENTADOS {
            assert_ne!(descrever_status_darf(status), status.as_str(), "{status}");
        }
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
        dir: tempfile::TempDir,
        context: Context,
        command: LoteCommand,
    }

    async fn cenario(args: &[&str]) -> Cenario {
        let server = MockServer::start().await;
        let dir = tempfile::tempdir().unwrap();
        let rcgen::CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed(vec!["cliente.teste".to_owned()]).unwrap();
        let certificado = dir.path().join("certificado.crt");
        let chave = dir.path().join("chave.key");
        fs::write(&certificado, cert.pem()).unwrap();
        fs::write(&chave, signing_key.serialize_pem()).unwrap();
        // The command takes today from the clock: the scheduling of the file
        // goes to tomorrow, never to a day already past.
        let amanha = (crate::commands::hoje() + chrono::Days::new(1))
            .format("%Y-%m-%d")
            .to_string();
        let csv = CSV.replace("2026-10-10;2026-10-09", &format!("{amanha};{amanha}"));
        fs::write(dir.path().join("lote.csv"), csv).unwrap();
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
        let arquivo = dir.path().join("lote.csv").display().to_string();
        let mut full = vec!["inter-pj", "--config", &config, "pagamento", "lote"];
        for arg in args {
            full.push(if *arg == "<arquivo>" { &arquivo } else { arg });
        }
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
        let Command::Pagamento(PagamentoCommand::Lote(command)) = cli.command else {
            unreachable!("pagamento lote");
        };
        Mock::given(method("POST"))
            .and(path("/oauth/v2/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "tok",
                "expires_in": 3600,
                "scope": "pagamento-lote.write pagamento-lote.read"
            })))
            .mount(&server)
            .await;
        Cenario {
            server,
            dir,
            context,
            command,
        }
    }

    #[tokio::test]
    async fn declined_confirmation_sends_nothing() {
        let cenario = cenario(&["enviar", "--arquivo", "<arquivo>"]).await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&cenario.server)
            .await;
        let LoteCommand::Enviar(args) = &cenario.command else {
            unreachable!()
        };
        for resposta in ["n\n", "\n", "enviar\n"] {
            let mut terminal = TerminalFalso::respondendo(resposta);
            let err = enviar(&cenario.context, args, &mut terminal)
                .await
                .unwrap_err();
            assert!(matches!(err, CliError::Cancelado), "{resposta:?}: {err}");
            assert_eq!(
                terminal.perguntas,
                ["Enviar o lote de 3 pagamentos (R$ 142,57)? [s/N] "]
            );
        }
        assert!(cenario.dir.path().join("lote.csv").exists());
    }

    #[tokio::test]
    async fn confirmed_batch_is_sent_once() {
        let cenario = cenario(&[
            "enviar",
            "--arquivo",
            "<arquivo>",
            "--identificador",
            "Outubro",
        ])
        .await;
        Mock::given(method("POST"))
            .and(path("/banking/v2/pagamento/lote"))
            .and(wiremock::matchers::body_partial_json(
                json!({"meuIdentificador": "Outubro"}),
            ))
            .respond_with(ResponseTemplate::new(202).set_body_json(json!({
                "idLote": ID_LOTE,
                "status": "EMPROCESSAMENTO",
                "qtdePagamentos": 3
            })))
            .expect(1)
            .mount(&cenario.server)
            .await;
        let LoteCommand::Enviar(args) = &cenario.command else {
            unreachable!()
        };
        enviar(
            &cenario.context,
            args,
            &mut TerminalFalso::respondendo("s\n"),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn uncertain_outcome_says_how_to_check_the_payments() {
        let cenario = cenario(&["enviar", "--arquivo", "<arquivo>", "--sim"]).await;
        // Sent at most once: without an idempotency key, a retry could pay twice.
        Mock::given(method("POST"))
            .and(path("/banking/v2/pagamento/lote"))
            .respond_with(ResponseTemplate::new(504))
            .expect(1)
            .mount(&cenario.server)
            .await;
        let LoteCommand::Enviar(args) = &cenario.command else {
            unreachable!()
        };
        let err = enviar(&cenario.context, args, &mut TerminalFalso::default())
            .await
            .unwrap_err();
        assert!(
            matches!(
                &err,
                CliError::PagamentoIncerto { situacao, consulta, .. }
                    if *situacao == "o lote pode ter sido recebido e pago"
                        && consulta == "inter-pj pagamento boleto listar e inter-pj pagamento darf listar"
            ),
            "{err:?}"
        );
    }

    fn lote_com(status: &str) -> serde_json::Value {
        consulta_com(status, &[("DARF", "ERRO_PAGAMENTO")])
    }

    fn consulta_com(status: &str, pagamentos: &[(&str, &str)]) -> serde_json::Value {
        let pagamentos: Vec<_> = pagamentos
            .iter()
            .map(|(tipo, status)| json!({"tipoPagamento": tipo, "status": status}))
            .collect();
        json!({"idLote": ID_LOTE, "status": status, "pagamentos": pagamentos})
    }

    async fn mount_consulta(cenario: &Cenario, resposta: ResponseTemplate, vezes: Option<u64>) {
        let mock = Mock::given(method("GET"))
            .and(path(format!("/banking/v2/pagamento/lote/{ID_LOTE}")))
            .respond_with(resposta);
        match vezes {
            Some(vezes) => mock.up_to_n_times(vezes).mount(&cenario.server).await,
            None => mock.mount(&cenario.server).await,
        }
    }

    async fn esperar(
        cenario: &Cenario,
        timeout: Duration,
    ) -> Result<(Lote, bool), (Option<Box<Lote>>, CliError)> {
        let client = cenario
            .context
            .client(&cenario.context.settings().unwrap())
            .unwrap();
        aguardar(&client, ID_LOTE, timeout, Duration::from_millis(10)).await
    }

    #[tokio::test]
    async fn waiting_ends_with_the_processing() {
        let cenario = cenario(&["consultar", ID_LOTE, "--aguardar"]).await;
        let ok = |corpo| ResponseTemplate::new(200).set_body_json(corpo);
        mount_consulta(&cenario, ok(lote_com("EMPROCESSAMENTO")), Some(1)).await;
        mount_consulta(&cenario, ok(lote_com("PROCESSADOCOMERRO")), None).await;
        let (lote, terminou) = esperar(&cenario, Duration::from_secs(5)).await.unwrap();
        assert!(terminou);
        assert_eq!(lote.status, Some(StatusLote::ProcessadoComErro));
        assert_eq!(
            falhas(&Contagem::de(&lote)),
            "1 de 1 pagamentos não foi feito"
        );
    }

    #[tokio::test]
    async fn waiting_stops_at_the_deadline() {
        let cenario = cenario(&["consultar", ID_LOTE, "--aguardar"]).await;
        let corpo = lote_com("EMPROCESSAMENTO");
        mount_consulta(
            &cenario,
            ResponseTemplate::new(200).set_body_json(corpo),
            None,
        )
        .await;
        let (lote, terminou) = esperar(&cenario, Duration::from_millis(50)).await.unwrap();
        assert!(!terminou);
        assert_eq!(lote.status, Some(StatusLote::EmProcessamento));
    }

    /// A processed batch with a payment still waiting for an approval has
    /// not paid it: the wait goes on until the deadline.
    #[tokio::test]
    async fn a_pending_payment_keeps_the_wait() {
        let cenario = cenario(&["consultar", ID_LOTE, "--aguardar"]).await;
        let corpo = consulta_com(
            "PROCESSADOSEMERRO",
            &[("BOLETO", "PAGO"), ("DARF", "AGUARDANDO_APROVACAO")],
        );
        mount_consulta(
            &cenario,
            ResponseTemplate::new(200).set_body_json(corpo),
            None,
        )
        .await;
        let (lote, terminou) = esperar(&cenario, Duration::from_millis(50)).await.unwrap();
        assert!(!terminou);
        assert_eq!(Contagem::de(&lote).pendentes, 1);
    }

    /// A query that fails past the retries of the library (a 503 here, with
    /// --sem-retentativa) does not end the wait; the batch goes on.
    #[tokio::test]
    async fn a_failed_query_does_not_end_the_wait() {
        let args = ["consultar", ID_LOTE, "--aguardar", "--sem-retentativa"];
        let primeiro = cenario(&args).await;
        let ok = |corpo| ResponseTemplate::new(200).set_body_json(corpo);
        mount_consulta(&primeiro, ok(lote_com("EMPROCESSAMENTO")), Some(1)).await;
        mount_consulta(&primeiro, ResponseTemplate::new(503), Some(2)).await;
        let pago = consulta_com("PROCESSADOSEMERRO", &[("BOLETO", "PAGO"), ("DARF", "PAGO")]);
        mount_consulta(&primeiro, ok(pago), None).await;
        let (lote, terminou) = esperar(&primeiro, Duration::from_secs(5)).await.unwrap();
        assert!(terminou);
        assert_eq!(Contagem::de(&lote).pagos, 2);

        // Until the deadline: the last batch seen.
        let segundo = cenario(&args).await;
        mount_consulta(&segundo, ok(lote_com("EMPROCESSAMENTO")), Some(1)).await;
        mount_consulta(&segundo, ResponseTemplate::new(429), None).await;
        let (lote, terminou) = esperar(&segundo, Duration::from_millis(80)).await.unwrap();
        assert!(!terminou);
        assert_eq!(lote.status, Some(StatusLote::EmProcessamento));

        // Another error ends the wait at once, with the last batch seen.
        let terceiro = cenario(&args).await;
        mount_consulta(&terceiro, ok(lote_com("EMPROCESSAMENTO")), Some(1)).await;
        mount_consulta(&terceiro, ResponseTemplate::new(404), None).await;
        let (ultimo, err) = esperar(&terceiro, Duration::from_secs(5))
            .await
            .unwrap_err();
        assert_eq!(ultimo.unwrap().status, Some(StatusLote::EmProcessamento));
        assert_eq!(err.exit_code(), crate::error::exit::REJECTED);
    }

    /// Every status of the documentation has a place; only the unknown ones
    /// are "not confirmed".
    #[test]
    fn every_status_of_a_payment_has_its_place() {
        let boleto = |status: &str| {
            situacao(
                &serde_json::from_value(json!({"tipoPagamento": "BOLETO", "status": status}))
                    .unwrap(),
            )
        };
        let darf = |status: &str| {
            situacao(
                &serde_json::from_value(json!({"tipoPagamento": "DARF", "status": status}))
                    .unwrap(),
            )
        };
        for status in StatusBoletoDoLote::DOCUMENTADOS {
            assert_ne!(boleto(status.as_str()), Situacao::Desconhecida, "{status}");
        }
        for status in StatusDarfDoLote::DOCUMENTADOS {
            assert_ne!(darf(status.as_str()), Situacao::Desconhecida, "{status}");
        }
        for (status, esperada) in [
            ("PAGO", Situacao::Pago),
            ("AGENDADO_REALIZADO", Situacao::Pago),
            ("AGENDADO", Situacao::Agendado),
            ("AGUARDANDO_APROVACAO", Situacao::Pendente),
            ("APROVADO", Situacao::Pendente),
            ("CANCELADO", Situacao::NaoFeito),
            ("AGENDADO_CANCELADO", Situacao::NaoFeito),
            ("ERRO_PAGAMENTO", Situacao::NaoFeito),
            ("NOVO", Situacao::Desconhecida),
        ] {
            assert_eq!(boleto(status), esperada, "{status}");
        }
        for (status, esperada) in [
            ("PAGO", Situacao::Pago),
            ("PAGAMENTO_AGENDADO", Situacao::Agendado),
            ("AGUARDANDO_APROVACAO", Situacao::Pendente),
            ("AGENDAMENTO_CANCELADO", Situacao::NaoFeito),
            ("CANCELADO", Situacao::NaoFeito),
        ] {
            assert_eq!(darf(status), esperada, "{status}");
        }
        let sem_status: PagamentoDoLote =
            serde_json::from_value(json!({"tipoPagamento": "BOLETO"})).unwrap();
        assert_eq!(situacao(&sem_status), Situacao::Desconhecida);
    }

    #[test]
    fn the_failures_say_how_many_and_which() {
        let contagem = |status: &str, pagamentos: &[(&str, &str)]| {
            Contagem::de(&serde_json::from_value(consulta_com(status, pagamentos)).unwrap())
        };
        let cancelados = contagem(
            "PROCESSADOCOMERRO",
            &[("BOLETO", "CANCELADO"), ("DARF", "AGENDAMENTO_CANCELADO")],
        );
        assert_eq!(falhas(&cancelados), "2 de 2 pagamentos não foram feitos");
        let estranho = contagem(
            "PROCESSADOSEMERRO",
            &[("BOLETO", "PAGO"), ("BOLETO", "NOVO")],
        );
        assert_eq!(
            falhas(&estranho),
            "de 2 pagamentos, 1 tem um status que a CLI não conhece: confira no Internet Banking"
        );
        assert_eq!(
            falhas(&contagem("PROCESSADOCOMERRO", &[("BOLETO", "PAGO")])),
            "a API diz que houve erro: confira o status de cada pagamento"
        );
        // Counted by the API but not listed: still pending.
        let mut lote: Lote =
            serde_json::from_value(consulta_com("PROCESSADOSEMERRO", &[("BOLETO", "PAGO")]))
                .unwrap();
        lote.qtde_pagamentos = Some(3);
        assert_eq!(Contagem::de(&lote).pendentes, 2);
        assert!(!terminou(&lote));
    }
}
