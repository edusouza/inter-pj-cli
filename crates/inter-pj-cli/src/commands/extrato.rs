//! `inter-pj extrato [completo|pdf]`

use std::borrow::Cow;
use std::fmt::Write as _;
use std::io::{self, IsTerminal};
use std::path::PathBuf;

use chrono::{NaiveDate, NaiveTime, Utc};
use inter_pj::InterClient;
use inter_pj::banking::{
    Detalhe, FiltroExtrato, PaginaExtrato, Periodo, PeriodoError, TipoOperacao, TipoTransacao,
    TransacaoCompleta, TransacaoSimples,
};
use rust_decimal::Decimal;
use serde_json::json;

use super::{BRASILIA, Context, hoje, intervalo};
use crate::cli::{
    ExtratoArgs, ExtratoCommand, ExtratoCompletoArgs, ExtratoPdfArgs, Formato, SaidaOfxArgs,
};
use crate::config::Settings;
use crate::error::CliError;
use crate::output;
use crate::saida::{Saida, tamanho};
use crate::tabela::{Celula, Coluna, Tabela};

mod ofx;

/// Longest description shown in the text output.
const LARGURA_DESCRICAO: usize = 60;
const LARGURA_CONTRAPARTE: usize = 30;

pub(super) async fn run(context: &Context, args: ExtratoArgs) -> Result<(), CliError> {
    match &args.comando {
        None => simples(context, &args).await,
        Some(ExtratoCommand::Completo(args)) => completo(context, args).await,
        Some(ExtratoCommand::Pdf(args)) => pdf(context, args).await,
    }
}

// --- extrato -----------------------------------------------------------------

async fn simples(context: &Context, args: &ExtratoArgs) -> Result<(), CliError> {
    let (inicio, fim) = intervalo(args.periodo, hoje());
    let periodos = periodos(inicio, fim, args.dividir_periodo)?;
    let destino = DestinoOfx::de(context, &args.ofx)?;
    let settings = context.settings()?;
    if destino.is_some() {
        // The account goes into the OFX: without it, nothing is requested.
        settings.for_conta()?;
    }
    let client = context.client(&settings)?;

    let mut transacoes = Vec::new();
    for periodo in periodos {
        transacoes.extend(client.banking().extrato(periodo).await?);
    }

    if let Some(destino) = destino {
        let transacoes = transacoes.iter().map(ofx::Transacao::simples).collect();
        return destino
            .gravar(context, &settings, &client, (inicio, fim), transacoes)
            .await;
    }
    match context.formato() {
        Formato::Json => output::print_json(&json!({ "transacoes": transacoes })),
        Formato::Csv => output::print_raw(&csv_simples(&transacoes).csv(context.separador())),
        Formato::Texto => {
            context.warn_if_sandbox(&settings);
            let valores = transacoes.iter().map(TransacaoSimples::valor_com_sinal);
            output::print(&render(
                inicio,
                fim,
                &texto_simples(&transacoes),
                &Totais::de(valores),
            ))
        }
    }
}

fn texto_simples(transacoes: &[TransacaoSimples]) -> Tabela {
    let mut tabela = Tabela::new(vec![
        Coluna::texto("Data", ""),
        Coluna::texto("Tipo", ""),
        Coluna::texto("Descrição", "").no_maximo(LARGURA_DESCRICAO),
        Coluna::valor("Valor", ""),
    ]);
    for t in transacoes {
        tabela.linha(vec![
            Celula::data(t.data(), t.data_entrada.as_deref()),
            Celula::texto(t.tipo_transacao.as_ref().map(rotulo_tipo).as_deref()),
            Celula::texto(Some(&descricao(
                t.titulo.as_deref(),
                t.descricao.as_deref(),
            ))),
            Celula::dinheiro(t.valor_com_sinal()),
        ]);
    }
    tabela
}

/// API codes and field names; `valor` is negative for debits.
fn csv_simples(transacoes: &[TransacaoSimples]) -> Tabela {
    let mut tabela = Tabela::new(
        [
            "dataEntrada",
            "tipoTransacao",
            "tipoOperacao",
            "titulo",
            "descricao",
        ]
        .into_iter()
        .map(|campo| Coluna::texto(campo, campo))
        .chain([Coluna::valor("valor", "valor")])
        .collect(),
    );
    for t in transacoes {
        tabela.linha(vec![
            Celula::data(t.data(), t.data_entrada.as_deref()),
            Celula::texto(t.tipo_transacao.as_ref().map(TipoTransacao::as_str)),
            Celula::texto(t.tipo_operacao.as_ref().map(TipoOperacao::as_str)),
            Celula::texto(t.titulo.as_deref()),
            Celula::texto(t.descricao.as_deref()),
            Celula::dinheiro(t.valor_com_sinal()),
        ]);
    }
    tabela
}

// --- extrato completo --------------------------------------------------------

async fn completo(context: &Context, args: &ExtratoCompletoArgs) -> Result<(), CliError> {
    let (inicio, fim) = intervalo(args.periodo, hoje());
    let periodos = periodos(inicio, fim, args.dividir_periodo)?;
    if context.ofx() && !args.todas_paginas {
        return Err(CliError::Usage(
            "o OFX traz o extrato inteiro do período: use --todas-paginas".to_owned(),
        ));
    }
    let destino = DestinoOfx::de(context, &args.ofx)?;
    let filtro = |periodo: Periodo| {
        let mut filtro = FiltroExtrato::new(periodo);
        if let Some(tipo) = args.tipo_operacao {
            filtro = filtro.tipo_operacao(tipo.into());
        }
        if let Some(tipo) = &args.tipo_transacao {
            filtro = filtro.tipo_transacao(tipo.clone());
        }
        filtro
    };
    let settings = context.settings()?;
    if destino.is_some() {
        // The account goes into the OFX: without it, nothing is requested.
        settings.for_conta()?;
    }
    let client = context.client(&settings)?;

    if !args.todas_paginas {
        // Without --dividir-periodo there is exactly one period.
        let pagina = client
            .banking()
            .extrato_completo(
                &filtro(periodos[0]),
                args.pagina.unwrap_or(0),
                args.tamanho_pagina,
            )
            .await?;
        return mostrar_pagina(
            context,
            &settings,
            inicio,
            fim,
            args.pagina.unwrap_or(0),
            &pagina,
        );
    }

    let mut transacoes = Vec::new();
    for periodo in periodos {
        transacoes.extend(
            client
                .banking()
                .extrato_completo_todas(&filtro(periodo))
                .await?,
        );
    }
    if let Some(destino) = destino {
        let transacoes = transacoes.iter().map(ofx::Transacao::completa).collect();
        return destino
            .gravar(context, &settings, &client, (inicio, fim), transacoes)
            .await;
    }
    match context.formato() {
        Formato::Json => output::print_json(&json!({ "transacoes": transacoes })),
        Formato::Csv => output::print_raw(&csv_completo(&transacoes).csv(context.separador())),
        Formato::Texto => {
            context.warn_if_sandbox(&settings);
            let valores = transacoes.iter().map(TransacaoCompleta::valor_com_sinal);
            output::print(&render(
                inicio,
                fim,
                &texto_completo(&transacoes),
                &Totais::de(valores),
            ))
        }
    }
}

fn mostrar_pagina(
    context: &Context,
    settings: &crate::config::Settings,
    inicio: NaiveDate,
    fim: NaiveDate,
    numero: u32,
    pagina: &PaginaExtrato,
) -> Result<(), CliError> {
    let aviso = aviso_paginacao(numero, pagina);
    match context.formato() {
        Formato::Json => output::print_json(pagina),
        Formato::Csv => {
            if let Some(aviso) = &aviso {
                eprintln!("aviso: {aviso}");
            }
            output::print_raw(&csv_completo(&pagina.transacoes).csv(context.separador()))
        }
        Formato::Texto => {
            context.warn_if_sandbox(settings);
            let tabela = texto_completo(&pagina.transacoes);
            let mut texto = format!(
                "Extrato completo de {} a {}\n\n",
                inicio.format("%d/%m/%Y"),
                fim.format("%d/%m/%Y")
            );
            if tabela.is_empty() {
                texto.push_str("Nenhuma transação nesta página.");
            } else {
                texto.push_str(&tabela.texto());
            }
            texto.push_str("\n\n");
            texto.push_str(&resumo_pagina(numero, pagina));
            if let Some(aviso) = aviso {
                texto.push('\n');
                texto.push_str(&aviso);
            }
            output::print(&texto)
        }
    }
}

/// `Página 1 de 3 · 50 de 120 transações`.
fn resumo_pagina(numero: u32, pagina: &PaginaExtrato) -> String {
    let mut resumo = format!("Página {}", u64::from(numero) + 1);
    if let Some(total) = pagina.total_paginas {
        let _ = write!(resumo, " de {total}");
    }
    let nesta = pagina.transacoes.len();
    let _ = match pagina.total_elementos {
        Some(total) => write!(resumo, " · {nesta} de {total} transações"),
        None => write!(resumo, " · {nesta} transações"),
    };
    resumo
}

fn aviso_paginacao(numero: u32, pagina: &PaginaExtrato) -> Option<String> {
    let tem_mais = match (pagina.ultima_pagina, pagina.total_paginas) {
        (Some(ultima), _) => !ultima,
        (None, Some(total)) => u64::from(numero) + 1 < total,
        (None, None) => false,
    };
    tem_mais.then(|| {
        format!(
            "há mais páginas: use --pagina {} ou --todas-paginas",
            u64::from(numero) + 1
        )
    })
}

fn texto_completo(transacoes: &[TransacaoCompleta]) -> Tabela {
    let mut tabela = Tabela::new(vec![
        Coluna::texto("Data", ""),
        Coluna::texto("Tipo", ""),
        Coluna::texto("Descrição", "").no_maximo(LARGURA_DESCRICAO),
        Coluna::texto("Contraparte", "").no_maximo(LARGURA_CONTRAPARTE),
        Coluna::valor("Valor", ""),
    ]);
    for t in transacoes {
        tabela.linha(vec![
            Celula::data(t.data(), t.data_transacao.as_deref()),
            Celula::texto(t.tipo_transacao.as_ref().map(rotulo_tipo).as_deref()),
            Celula::texto(Some(&descricao(
                t.titulo.as_deref(),
                t.descricao.as_deref(),
            ))),
            Celula::texto(Contraparte::de(t).nome),
            Celula::dinheiro(t.valor_com_sinal()),
        ]);
    }
    tabela
}

/// API codes and field names, plus the main fields of `detalhes`; `valor` is
/// negative for debits.
fn csv_completo(transacoes: &[TransacaoCompleta]) -> Tabela {
    let campos = [
        "idTransacao",
        "dataTransacao",
        "dataInclusao",
        "tipoTransacao",
        "tipoOperacao",
        "titulo",
        "descricao",
        "numeroDocumento",
    ];
    let extras = [
        "contraparte",
        "documentoContraparte",
        "endToEndId",
        "codigoBarras",
    ];
    let mut tabela = Tabela::new(
        campos
            .into_iter()
            .map(|campo| Coluna::texto(campo, campo))
            .chain([Coluna::valor("valor", "valor")])
            .chain(extras.into_iter().map(|campo| Coluna::texto(campo, campo)))
            .collect(),
    );
    for t in transacoes {
        let contraparte = Contraparte::de(t);
        tabela.linha(vec![
            Celula::texto(t.id_transacao.as_deref()),
            Celula::data(t.data(), t.data_transacao.as_deref()),
            Celula::data(
                t.data_inclusao.as_deref().and_then(parse_iso),
                t.data_inclusao.as_deref(),
            ),
            Celula::texto(t.tipo_transacao.as_ref().map(TipoTransacao::as_str)),
            Celula::texto(t.tipo_operacao.as_ref().map(TipoOperacao::as_str)),
            Celula::texto(t.titulo.as_deref()),
            Celula::texto(t.descricao.as_deref()),
            Celula::texto(t.numero_documento.as_deref()),
            Celula::dinheiro(t.valor_com_sinal()),
            Celula::texto(contraparte.nome),
            Celula::texto(contraparte.documento),
            Celula::texto(contraparte.end_to_end_id),
            Celula::texto(contraparte.codigo_barras),
        ]);
    }
    tabela
}

/// The other party of a transaction, when its details tell.
#[derive(Debug, Default, PartialEq, Eq)]
struct Contraparte<'a> {
    nome: Option<&'a str>,
    documento: Option<&'a str>,
    end_to_end_id: Option<&'a str>,
    codigo_barras: Option<&'a str>,
}

impl<'a> Contraparte<'a> {
    fn de(transacao: &'a TransacaoCompleta) -> Self {
        let saida = transacao.tipo_operacao == Some(TipoOperacao::Debito);
        let escolher = |pagador: &'a Option<String>, recebedor: &'a Option<String>| {
            if saida { recebedor } else { pagador }.as_deref()
        };
        match &transacao.detalhes {
            Some(Detalhe::Pix(d)) => Self {
                nome: escolher(&d.nome_pagador, &d.nome_recebedor),
                documento: escolher(&d.cpf_cnpj_pagador, &d.cpf_cnpj_recebedor),
                end_to_end_id: d.end_to_end_id.as_deref(),
                codigo_barras: None,
            },
            Some(Detalhe::Transferencia(d)) => Self {
                nome: escolher(&d.nome_pagador, &d.nome_recebedor),
                documento: escolher(&d.cpf_cnpj_pagador, &d.cpf_cnpj_recebedor),
                ..Self::default()
            },
            Some(Detalhe::Pagamento(d)) => Self {
                nome: d
                    .nome_destinatario
                    .as_deref()
                    .or(d.empresa_emissora.as_deref()),
                documento: d.cpf_cnpj.as_deref(),
                codigo_barras: d.cod_barras.as_deref().or(d.linha_digitavel.as_deref()),
                ..Self::default()
            },
            Some(Detalhe::BoletoCobranca(d)) => Self {
                nome: d.nome.as_deref(),
                documento: d.cpf_cnpj.as_deref(),
                codigo_barras: d.cod_barras.as_deref(),
                ..Self::default()
            },
            Some(Detalhe::DepositoBoleto(d)) => Self {
                codigo_barras: d.cod_barras.as_deref(),
                ..Self::default()
            },
            Some(Detalhe::CompraDebito(d)) => Self {
                nome: d.estabelecimento.as_deref(),
                ..Self::default()
            },
            Some(Detalhe::Cheque(d)) => Self {
                nome: d.nome_empresa.as_deref(),
                ..Self::default()
            },
            Some(Detalhe::Cashback(d)) => Self {
                nome: d.produto.as_deref(),
                ..Self::default()
            },
            Some(Detalhe::Tarifa(d)) => Self {
                end_to_end_id: d.end_to_end_id.as_deref(),
                codigo_barras: d.cod_barras.as_deref(),
                ..Self::default()
            },
            _ => Self::default(),
        }
    }
}

// --- extrato pdf ---------------------------------------------------------------

async fn pdf(context: &Context, args: &ExtratoPdfArgs) -> Result<(), CliError> {
    let (inicio, fim) = intervalo(args.periodo, hoje());
    let periodo = Periodo::new(inicio, fim).map_err(|erro| CliError::Periodo {
        dica: matches!(erro, PeriodoError::MuitoLongo { .. })
            .then_some("gere um PDF para cada período de até 90 dias"),
        erro,
    })?;
    let saida = args.saida.clone().unwrap_or_else(|| {
        PathBuf::from(format!(
            "extrato-{}-a-{}.pdf",
            inicio.format("%Y-%m-%d"),
            fim.format("%Y-%m-%d")
        ))
    });
    let saida = Saida::new(saida, args.sobrescrever, "extrato.pdf", "--saida");
    saida.conferir()?;

    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let documento = client.banking().extrato_pdf(periodo).await?;
    saida.gravar(&documento)?;
    if saida.stdout() {
        return Ok(());
    }

    match context.formato() {
        Formato::Json => output::print_json(&json!({
            "arquivo": saida.caminho().display().to_string(),
            "bytes": documento.len(),
            "dataInicio": inicio.format("%Y-%m-%d").to_string(),
            "dataFim": fim.format("%Y-%m-%d").to_string(),
        })),
        // `commands::run` refuses csv for this command.
        Formato::Texto | Formato::Csv => {
            context.warn_if_sandbox(&settings);
            output::print(&format!(
                "Extrato de {periodo} salvo em {} ({})",
                saida.caminho().display(),
                tamanho(documento.len())
            ))
        }
    }
}

// --- extrato --formato ofx -----------------------------------------------------

/// The last second of a day, the end of a period that ended before today.
const FIM_DO_DIA: NaiveTime = match NaiveTime::from_hms_opt(23, 59, 59) {
    Some(hora) => hora,
    None => panic!("fim do dia"),
};

/// `--formato ofx`: where the OFX goes, checked before any request.
#[derive(Debug)]
struct DestinoOfx {
    /// The file of `--saida`; without it (or with `-`), the standard output.
    arquivo: Option<Saida>,
}

impl DestinoOfx {
    /// `None` without `--formato ofx`, which `--saida` requires.
    fn de(context: &Context, args: &SaidaOfxArgs) -> Result<Option<Self>, CliError> {
        if !context.ofx() {
            return match args.saida {
                Some(_) => Err(CliError::Usage(
                    "--saida vale apenas com --formato ofx; nos outros formatos, redirecione a saída (> arquivo)"
                        .to_owned(),
                )),
                None => Ok(None),
            };
        }
        let arquivo = args
            .saida
            .as_ref()
            .filter(|caminho| caminho.as_os_str() != "-")
            .map(|caminho| {
                Saida::new(caminho.clone(), args.sobrescrever, "extrato.ofx", "--saida")
            });
        if let Some(arquivo) = &arquivo {
            arquivo.conferir()?;
        }
        Ok(Some(Self { arquivo }))
    }

    /// Writes the OFX of `transacoes`, with the balance at the end of the
    /// period. The transactions without a date or an amount (`None`), which
    /// OFX requires, are left out, with a warning.
    async fn gravar(
        &self,
        context: &Context,
        settings: &Settings,
        client: &InterClient,
        (inicio, fim): (NaiveDate, NaiveDate),
        transacoes: Vec<Option<ofx::Transacao<'_>>>,
    ) -> Result<(), CliError> {
        let saldo = client
            .banking()
            .saldo(Some(fim))
            .await?
            .disponivel
            .ok_or_else(|| {
                CliError::RespostaIncompleta(format!(
                    "a API não informou o saldo de {}, que o OFX exige",
                    fim.format("%d/%m/%Y")
                ))
            })?;
        let agora = Utc::now().with_timezone(&BRASILIA).naive_local();

        let recebidas = transacoes.len();
        let transacoes: Vec<_> = transacoes.into_iter().flatten().collect();
        match recebidas - transacoes.len() {
            0 => {}
            1 => eprintln!("aviso: 1 transação sem data ou sem valor ficou fora do OFX"),
            fora => eprintln!("aviso: {fora} transações sem data ou sem valor ficaram fora do OFX"),
        }
        let quantidade = transacoes.len();
        let documento = ofx::Extrato {
            conta: settings.for_conta()?,
            inicio,
            // Up to today, the balance is the one of the moment of the query.
            fim: if fim < agora.date() {
                fim.and_time(FIM_DO_DIA)
            } else {
                agora
            },
            saldo,
            agora,
            transacoes,
        }
        .render();

        let Some(arquivo) = &self.arquivo else {
            // A terminal shows the text; a file or a program gets the bytes.
            return if io::stdout().is_terminal() {
                output::print(documento.replace("\r\n", "\n").trim_end())
            } else {
                output::print_bytes(&ofx::cp1252(&documento))
            };
        };
        let bytes = ofx::cp1252(&documento);
        arquivo.gravar(&bytes)?;
        context.warn_if_sandbox(settings);
        output::print(&format!(
            "Extrato de {} a {} salvo em {} ({quantidade} {}, {})",
            inicio.format("%d/%m/%Y"),
            fim.format("%d/%m/%Y"),
            arquivo.caminho().display(),
            if quantidade == 1 {
                "transação"
            } else {
                "transações"
            },
            tamanho(bytes.len())
        ))
    }
}

// --- shared --------------------------------------------------------------------

fn periodos(inicio: NaiveDate, fim: NaiveDate, dividir: bool) -> Result<Vec<Periodo>, CliError> {
    let resultado = if dividir {
        Periodo::dividir(inicio, fim)
    } else {
        Periodo::new(inicio, fim).map(|periodo| vec![periodo])
    };
    resultado.map_err(|erro| CliError::Periodo {
        dica: matches!(erro, PeriodoError::MuitoLongo { .. })
            .then_some("use --dividir-periodo para consultar em partes de até 90 dias"),
        erro,
    })
}

/// Money in, money out and the net result of a list of signed amounts.
#[derive(Debug, Default, PartialEq, Eq)]
struct Totais {
    entradas: Decimal,
    saidas: Decimal,
    quantidade: usize,
}

impl Totais {
    fn de(valores: impl Iterator<Item = Option<Decimal>>) -> Self {
        let mut totais = Self::default();
        for valor in valores {
            totais.quantidade += 1;
            match valor {
                Some(v) if v.is_sign_negative() => totais.saidas += v,
                Some(v) => totais.entradas += v,
                None => {}
            }
        }
        totais
    }
}

fn render(inicio: NaiveDate, fim: NaiveDate, tabela: &Tabela, totais: &Totais) -> String {
    let mut texto = format!(
        "Extrato de {} a {}\n\n",
        inicio.format("%d/%m/%Y"),
        fim.format("%d/%m/%Y")
    );
    if tabela.is_empty() {
        texto.push_str("Nenhuma transação no período.");
        return texto;
    }
    texto.push_str(&tabela.texto());
    texto.push_str("\n\n");
    texto.push_str(&output::key_values(&[
        ("Entradas", output::brl(totais.entradas)),
        ("Saídas", output::brl(totais.saidas)),
        (
            "Resultado do período",
            output::brl(totais.entradas + totais.saidas),
        ),
    ]));
    let plural = if totais.quantidade == 1 {
        "transação"
    } else {
        "transações"
    };
    let _ = write!(texto, "\n{} {plural}", totais.quantidade);
    texto
}

/// `Pix recebido · Cliente`, or whichever of the two exists.
fn descricao(titulo: Option<&str>, descricao: Option<&str>) -> String {
    fn limpo(text: Option<&str>) -> Option<&str> {
        text.map(str::trim).filter(|text| !text.is_empty())
    }
    match (limpo(titulo), limpo(descricao)) {
        (Some(t), Some(d)) if !t.eq_ignore_ascii_case(d) => format!("{t} · {d}"),
        (Some(t), _) => t.to_owned(),
        (None, Some(d)) => d.to_owned(),
        (None, None) => String::new(),
    }
}

fn parse_iso(raw: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(raw.get(..10).unwrap_or(raw), "%Y-%m-%d").ok()
}

fn rotulo_tipo(tipo: &TipoTransacao) -> Cow<'static, str> {
    let rotulo = match tipo {
        TipoTransacao::AntecipacaoRecebiveis => "Antecipação de recebíveis",
        TipoTransacao::AntecipacaoRecebiveisCartao => "Antecipação (cartão)",
        TipoTransacao::BoletoCobranca => "Cobrança (boleto)",
        TipoTransacao::Cambio => "Câmbio",
        TipoTransacao::Cashback => "Cashback",
        TipoTransacao::Cheque => "Cheque",
        TipoTransacao::CompraDebito => "Compra no débito",
        TipoTransacao::DebitoAutomatico => "Débito automático",
        TipoTransacao::DebitoEmConta => "Débito em conta",
        TipoTransacao::DepositoBoleto => "Depósito por boleto",
        TipoTransacao::DomicilioCartao => "Domicílio de cartão",
        TipoTransacao::Estorno => "Estorno",
        TipoTransacao::Financiamento => "Financiamento",
        TipoTransacao::Imposto => "Imposto",
        TipoTransacao::Interpag => "Interpag",
        TipoTransacao::Investimento => "Investimento",
        TipoTransacao::Juros => "Juros",
        TipoTransacao::MaquininhaGranito => "Maquininha",
        TipoTransacao::Multa => "Multa",
        TipoTransacao::Outros => "Outros",
        TipoTransacao::Pagamento => "Pagamento",
        TipoTransacao::Pix => "Pix",
        TipoTransacao::Proventos => "Proventos",
        TipoTransacao::Saque => "Saque",
        TipoTransacao::Tarifa => "Tarifa",
        TipoTransacao::Transferencia => "Transferência",
        other => return Cow::Owned(other.as_str().to_owned()),
    };
    Cow::Borrowed(rotulo)
}
