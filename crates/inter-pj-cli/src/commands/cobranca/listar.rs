//! `inter-pj cobranca listar|sumario`

use std::fmt::Write as _;

use chrono::NaiveDate;
use inter_pj::cobranca::{
    CobrancaDetalhada, FiltrarDataPor, FiltroCobrancas, ItemSumario, OrdenarCobrancasPor,
    OrigemRecebimento, SituacaoCobranca, TipoCobranca,
};
use rust_decimal::Decimal;
use serde_json::json;

use super::{celula_situacao, descrever_situacao};
use crate::cli::{
    CobrancaListarArgs, CobrancaSumarioArgs, FiltrarDataPorArg, FiltroCobrancaArgs, Formato,
    OrdenarPorArg, SituacaoArg, TipoCobrancaArg,
};
use crate::commands::{Context, hoje, intervalo};
use crate::error::CliError;
use crate::output::{self, parse_data};
use crate::tabela::{Celula, Coluna, Tabela};

/// Longest payer name shown in the text output.
const LARGURA_PAGADOR: usize = 30;

pub(super) async fn listar(context: &Context, args: &CobrancaListarArgs) -> Result<(), CliError> {
    let mut filtro = filtro(&args.filtro, hoje());
    filtro.ordenar_por = args.ordenar_por.map(ordenar_por);
    filtro.decrescente = args.decrescente;
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let (cobrancas, pagina) = match args.pagina {
        Some(numero) => {
            let mut pagina = client
                .cobranca()
                .listar(&filtro, numero, args.itens_por_pagina)
                .await?;
            if context.formato() == Formato::Json {
                return output::print_json(&pagina);
            }
            let cobrancas = std::mem::take(&mut pagina.cobrancas);
            (cobrancas, Some((numero, pagina)))
        }
        None => (client.cobranca().listar_todas(&filtro).await?, None),
    };
    match context.formato() {
        Formato::Json => output::print_json(&json!({ "cobrancas": cobrancas })),
        Formato::Csv => output::print_raw(&csv(&cobrancas).csv(context.separador())),
        Formato::Texto => {
            context.warn_if_sandbox(&settings);
            let mut texto = format!("{}\n\n", titulo(&filtro));
            if cobrancas.is_empty() {
                texto.push_str("Nenhuma cobrança encontrada.");
            } else {
                texto.push_str(&tabela(&cobrancas).texto_colorido());
                let _ = write!(texto, "\n\n{}", totais(&cobrancas));
            }
            if let Some((numero, pagina)) = pagina {
                texto.push_str(&paginacao(numero, &pagina));
            }
            output::print(&texto)
        }
    }
}

pub(super) async fn sumario(context: &Context, args: &CobrancaSumarioArgs) -> Result<(), CliError> {
    let filtro = filtro(&args.filtro, hoje());
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let itens = client.cobranca().sumario(&filtro).await?;
    match context.formato() {
        Formato::Json => output::print_json(&json!({ "sumario": itens })),
        Formato::Csv => output::print_raw(&csv_sumario(&itens).csv(context.separador())),
        Formato::Texto => {
            context.warn_if_sandbox(&settings);
            output::print(&render_sumario(&filtro, &itens))
        }
    }
}

/// The filter of the arguments; by default, the charges due in the last 30
/// days.
fn filtro(args: &FiltroCobrancaArgs, hoje: NaiveDate) -> FiltroCobrancas {
    let (inicio, fim) = intervalo(args.periodo, hoje);
    let mut filtro = FiltroCobrancas::new(inicio, fim);
    filtro.filtrar_data_por = args.filtrar_por.map(|por| match por {
        FiltrarDataPorArg::Vencimento => FiltrarDataPor::Vencimento,
        FiltrarDataPorArg::Emissao => FiltrarDataPor::Emissao,
        FiltrarDataPorArg::Pagamento => FiltrarDataPor::Pagamento,
    });
    filtro.situacao = args.situacao.map(situacao);
    filtro.pessoa_pagadora.clone_from(&args.pagador);
    filtro.cpf_cnpj_pessoa_pagadora = args.documento.as_ref().map(|doc| doc.as_str().to_owned());
    filtro.seu_numero.clone_from(&args.seu_numero);
    filtro.tipo_cobranca = args.tipo.map(|tipo| match tipo {
        TipoCobrancaArg::Simples => TipoCobranca::Simples,
        TipoCobrancaArg::Parcelada => TipoCobranca::Parcelado,
        TipoCobrancaArg::Recorrente => TipoCobranca::Recorrente,
    });
    filtro
}

fn situacao(situacao: SituacaoArg) -> SituacaoCobranca {
    match situacao {
        SituacaoArg::AReceber => SituacaoCobranca::AReceber,
        SituacaoArg::Recebida => SituacaoCobranca::Recebido,
        SituacaoArg::Atrasada => SituacaoCobranca::Atrasado,
        SituacaoArg::Cancelada => SituacaoCobranca::Cancelado,
        SituacaoArg::Expirada => SituacaoCobranca::Expirado,
        SituacaoArg::MarcadaRecebida => SituacaoCobranca::MarcadoRecebido,
        SituacaoArg::EmProcessamento => SituacaoCobranca::EmProcessamento,
        SituacaoArg::FalhaEmissao => SituacaoCobranca::FalhaEmissao,
        SituacaoArg::Protesto => SituacaoCobranca::Protesto,
    }
}

fn ordenar_por(ordem: OrdenarPorArg) -> OrdenarCobrancasPor {
    match ordem {
        OrdenarPorArg::Pagador => OrdenarCobrancasPor::PessoaPagadora,
        OrdenarPorArg::Vencimento => OrdenarCobrancasPor::DataVencimento,
        OrdenarPorArg::Emissao => OrdenarCobrancasPor::DataEmissao,
        OrdenarPorArg::Valor => OrdenarCobrancasPor::Valor,
        OrdenarPorArg::Situacao => OrdenarCobrancasPor::Status,
        OrdenarPorArg::SeuNumero => OrdenarCobrancasPor::Identificador,
        OrdenarPorArg::Tipo => OrdenarCobrancasPor::TipoCobranca,
        OrdenarPorArg::Codigo => OrdenarCobrancasPor::CodigoCobranca,
    }
}

/// `Cobranças com vencimento de 25/08/2026 a 23/09/2026`, and the filters.
fn titulo(filtro: &FiltroCobrancas) -> String {
    let quais = match filtro.filtrar_data_por {
        Some(FiltrarDataPor::Emissao) => "emitidas",
        Some(FiltrarDataPor::Pagamento) => "pagas",
        _ => "com vencimento",
    };
    let mut texto = format!(
        "Cobranças {quais} de {} a {}",
        filtro.data_inicial.format("%d/%m/%Y"),
        filtro.data_final.format("%d/%m/%Y")
    );
    let mut filtros = Vec::new();
    if let Some(situacao) = &filtro.situacao {
        filtros.push(descrever_situacao(situacao));
    }
    if let Some(pagador) = &filtro.pessoa_pagadora {
        filtros.push(format!("pagador \"{}\"", output::limpo(pagador)));
    }
    if let Some(documento) = &filtro.cpf_cnpj_pessoa_pagadora {
        filtros.push(format!("CPF/CNPJ {documento}"));
    }
    if let Some(numero) = &filtro.seu_numero {
        filtros.push(format!("seu número {}", output::limpo(numero)));
    }
    if !filtros.is_empty() {
        let _ = write!(texto, " ({})", filtros.join(", "));
    }
    texto
}

fn tabela(cobrancas: &[CobrancaDetalhada]) -> Tabela {
    let mut tabela = Tabela::new(vec![
        Coluna::texto("Vencimento", ""),
        Coluna::texto("Seu número", ""),
        Coluna::texto("Pagador", "").no_maximo(LARGURA_PAGADOR),
        Coluna::texto("Situação", ""),
        Coluna::valor("Valor", ""),
        Coluna::texto("Código", ""),
    ]);
    for item in cobrancas {
        let c = &item.cobranca;
        tabela.linha(vec![
            data(c.data_vencimento.as_deref()),
            Celula::texto(c.seu_numero.as_deref()),
            Celula::texto(c.pagador.as_ref().and_then(|p| p.nome.as_deref())),
            celula_situacao(c.situacao.as_ref()),
            Celula::dinheiro(c.valor_nominal),
            Celula::texto(c.codigo_solicitacao.as_deref()),
        ]);
    }
    tabela
}

/// `3 cobranças · R$ 450,00 · recebido R$ 150,00`.
fn totais(cobrancas: &[CobrancaDetalhada]) -> String {
    let soma =
        |valores: &mut dyn Iterator<Item = Option<Decimal>>| -> Decimal { valores.flatten().sum() };
    let total = soma(&mut cobrancas.iter().map(|c| c.cobranca.valor_nominal));
    let recebido = soma(&mut cobrancas.iter().map(|c| c.cobranca.valor_total_recebido));
    let quantas = match cobrancas.len() {
        1 => "1 cobrança".to_owned(),
        n => format!("{n} cobranças"),
    };
    let mut texto = format!("{quantas} · {}", output::brl(total));
    if !recebido.is_zero() {
        let _ = write!(texto, " · recebido {}", output::brl(recebido));
    }
    texto
}

/// Where the page asked for with `--pagina` stands among the others.
fn paginacao(numero: u32, pagina: &inter_pj::cobranca::PaginaCobrancas) -> String {
    let mut texto = format!("\n\nPágina {numero}");
    if let Some(total) = pagina.total_paginas {
        let _ = write!(texto, " de {} (a primeira é 0)", total.saturating_sub(1));
    }
    if let Some(total) = pagina.total_elementos {
        let _ = write!(texto, "; {total} cobranças no período");
    }
    if pagina.ultima_pagina == Some(false) {
        let _ = write!(texto, "; a próxima é --pagina {}", numero + 1);
    }
    texto
}

fn data(raw: Option<&str>) -> Celula {
    Celula::data(raw.and_then(parse_data), raw)
}

/// Every field, with the API's names (nested ones with a dot) and codes.
fn csv(cobrancas: &[CobrancaDetalhada]) -> Tabela {
    let texto = |campo: &'static str| Coluna::texto(campo, campo);
    let valor = |campo: &'static str| Coluna::valor(campo, campo);
    let mut tabela = Tabela::new(vec![
        texto("codigoSolicitacao"),
        texto("seuNumero"),
        texto("situacao"),
        texto("dataSituacao"),
        texto("dataEmissao"),
        texto("dataVencimento"),
        valor("valorNominal"),
        valor("valorTotalRecebido"),
        texto("origemRecebimento"),
        texto("tipoCobranca"),
        texto("pagador.nome"),
        texto("pagador.cpfCnpj"),
        texto("boleto.nossoNumero"),
        texto("boleto.linhaDigitavel"),
        texto("pix.txid"),
        texto("pix.pixCopiaECola"),
    ]);
    for item in cobrancas {
        let c = &item.cobranca;
        let pagador = c.pagador.as_ref();
        let boleto = item.boleto.as_ref();
        let pix = item.pix.as_ref();
        tabela.linha(vec![
            Celula::texto(c.codigo_solicitacao.as_deref()),
            Celula::texto(c.seu_numero.as_deref()),
            Celula::texto(c.situacao.as_ref().map(SituacaoCobranca::as_str)),
            data(c.data_situacao.as_deref()),
            data(c.data_emissao.as_deref()),
            data(c.data_vencimento.as_deref()),
            Celula::dinheiro(c.valor_nominal),
            Celula::dinheiro(c.valor_total_recebido),
            Celula::texto(c.origem_recebimento.as_ref().map(OrigemRecebimento::as_str)),
            Celula::texto(c.tipo_cobranca.as_ref().map(TipoCobranca::as_str)),
            Celula::texto(pagador.and_then(|p| p.nome.as_deref())),
            Celula::texto(pagador.and_then(|p| p.cpf_cnpj.as_deref())),
            Celula::texto(boleto.and_then(|b| b.nosso_numero.as_deref())),
            Celula::texto(boleto.and_then(|b| b.linha_digitavel.as_deref())),
            Celula::texto(pix.and_then(|p| p.txid.as_deref())),
            Celula::texto(pix.and_then(|p| p.pix_copia_e_cola.as_deref())),
        ]);
    }
    tabela
}

fn render_sumario(filtro: &FiltroCobrancas, itens: &[ItemSumario]) -> String {
    let mut texto = format!("{}\n\n", titulo(filtro));
    let com_cobrancas: Vec<&ItemSumario> = itens
        .iter()
        .filter(|item| item.quantidade.unwrap_or_default() > 0)
        .collect();
    if com_cobrancas.is_empty() {
        texto.push_str("Nenhuma cobrança encontrada.");
        return texto;
    }
    let mut tabela = Tabela::new(vec![
        Coluna::texto("Situação", ""),
        Coluna::valor("Quantidade", ""),
        Coluna::valor("Valor", ""),
    ]);
    let (mut quantidade, mut valor) = (0u64, Decimal::ZERO);
    for item in com_cobrancas {
        quantidade += item.quantidade.unwrap_or_default();
        valor += item.valor.unwrap_or_default();
        tabela.linha(vec![
            celula_situacao(item.situacao.as_ref()),
            Celula::texto(item.quantidade.map(|n| n.to_string()).as_deref()),
            Celula::dinheiro(item.valor),
        ]);
    }
    tabela.linha(vec![
        Celula::texto(Some("Total")),
        Celula::texto(Some(&quantidade.to_string())),
        Celula::dinheiro(Some(valor)),
    ]);
    texto.push_str(&tabela.texto_colorido());
    texto
}

fn csv_sumario(itens: &[ItemSumario]) -> Tabela {
    let mut tabela = Tabela::new(vec![
        Coluna::texto("situacao", "situacao"),
        Coluna::valor("quantidade", "quantidade"),
        Coluna::valor("valor", "valor"),
    ]);
    for item in itens {
        tabela.linha(vec![
            Celula::texto(item.situacao.as_ref().map(SituacaoCobranca::as_str)),
            Celula::texto(item.quantidade.map(|n| n.to_string()).as_deref()),
            Celula::dinheiro(item.valor),
        ]);
    }
    tabela
}

#[cfg(test)]
mod tests {
    use clap::Parser;
    use serde_json::json;

    use super::*;
    use crate::cli::{Cli, CobrancaCommand, Command};

    fn dia(mes: u32, dia: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, mes, dia).unwrap()
    }

    fn listar_args(args: &[&str]) -> CobrancaListarArgs {
        let mut full = vec!["inter-pj", "cobranca", "listar"];
        full.extend_from_slice(args);
        match Cli::try_parse_from(full).unwrap().command {
            Command::Cobranca(CobrancaCommand::Listar(args)) => args,
            outro => panic!("{outro:?}"),
        }
    }

    fn cobrancas() -> Vec<CobrancaDetalhada> {
        serde_json::from_value(json!([
            {"cobranca": {
                "codigoSolicitacao": "0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d",
                "seuNumero": "NF-123", "situacao": "A_RECEBER", "dataVencimento": "2026-10-20",
                "valorNominal": "150.00",
                "pagador": {"nome": "Cliente Exemplo Ltda", "cpfCnpj": "12345678000195"}
            }},
            {"cobranca": {
                "codigoSolicitacao": "5a6b7c8d-1e2f-4a3b-8c9d-0e1f2a3b4c5d",
                "seuNumero": "NF-124", "situacao": "RECEBIDO", "dataVencimento": "2026-09-10",
                "valorNominal": "300.00", "valorTotalRecebido": "300.00", "origemRecebimento": "PIX",
                "pagador": {"nome": "Outro Cliente", "cpfCnpj": "12345678909"}
            }}
        ]))
        .unwrap()
    }

    #[test]
    fn arguments_become_the_filter() {
        let args = listar_args(&[
            "--inicio",
            "2026-09-01",
            "--fim",
            "2026-09-30",
            "--filtrar-por",
            "emissao",
            "--situacao",
            "A_RECEBER",
            "--documento",
            "12.345.678/0001-95",
            "--tipo",
            "parcelada",
        ]);
        let filtro = filtro(&args.filtro, dia(10, 1));
        assert_eq!(filtro.data_inicial, dia(9, 1));
        assert_eq!(filtro.data_final, dia(9, 30));
        assert_eq!(filtro.filtrar_data_por, Some(FiltrarDataPor::Emissao));
        assert_eq!(filtro.situacao, Some(SituacaoCobranca::AReceber));
        assert_eq!(
            filtro.cpf_cnpj_pessoa_pagadora.as_deref(),
            Some("12345678000195")
        );
        assert_eq!(filtro.tipo_cobranca, Some(TipoCobranca::Parcelado));
        // The last 30 days by default.
        let padrao = filtro_padrao(dia(9, 23));
        assert_eq!(
            (padrao.data_inicial, padrao.data_final),
            (dia(8, 25), dia(9, 23))
        );
    }

    fn filtro_padrao(hoje: NaiveDate) -> FiltroCobrancas {
        filtro(&listar_args(&[]).filtro, hoje)
    }

    #[test]
    fn every_situation_and_order_has_an_argument() {
        use clap::ValueEnum;
        let situacoes: Vec<SituacaoCobranca> = SituacaoArg::value_variants()
            .iter()
            .map(|s| situacao(*s))
            .collect();
        for documentada in SituacaoCobranca::DOCUMENTADOS {
            assert!(situacoes.contains(documentada), "{documentada:?}");
        }
        let ordens: Vec<OrdenarCobrancasPor> = OrdenarPorArg::value_variants()
            .iter()
            .map(|o| ordenar_por(*o))
            .collect();
        for documentada in OrdenarCobrancasPor::TODOS {
            assert!(ordens.contains(&documentada), "{documentada:?}");
        }
    }

    #[test]
    fn renders_the_list_with_totals() {
        let mut filtro = filtro_padrao(dia(9, 23));
        filtro.situacao = Some(SituacaoCobranca::AReceber);
        assert_eq!(
            titulo(&filtro),
            "Cobranças com vencimento de 25/08/2026 a 23/09/2026 (a receber)"
        );
        let cobrancas = cobrancas();
        assert_eq!(
            tabela(&cobrancas).texto(),
            "\
Vencimento  Seu número  Pagador               Situação       Valor  Código
20/10/2026  NF-123      Cliente Exemplo Ltda  a receber  R$ 150,00  0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d
10/09/2026  NF-124      Outro Cliente         recebida   R$ 300,00  5a6b7c8d-1e2f-4a3b-8c9d-0e1f2a3b4c5d"
        );
        assert_eq!(
            totais(&cobrancas),
            "2 cobranças · R$ 450,00 · recebido R$ 300,00"
        );
    }

    #[test]
    fn csv_keeps_the_api_names_and_codes() {
        let csv = csv(&cobrancas()).csv(crate::tabela::Separador::Virgula);
        let mut linhas = csv.lines();
        assert!(linhas.next().unwrap().starts_with(
            "codigoSolicitacao,seuNumero,situacao,dataSituacao,dataEmissao,dataVencimento,valorNominal"
        ));
        assert_eq!(
            linhas.nth(1).unwrap(),
            "5a6b7c8d-1e2f-4a3b-8c9d-0e1f2a3b4c5d,NF-124,RECEBIDO,,,2026-09-10,300.00,300.00,PIX,,Outro Cliente,12345678909,,,,"
        );
    }

    #[test]
    fn summary_shows_only_situations_with_charges_and_the_total() {
        let itens: Vec<ItemSumario> = serde_json::from_value(json!([
            {"situacao": "A_RECEBER", "valor": 1000, "quantidade": 30},
            {"situacao": "RECEBIDO", "valor": 4000.5, "quantidade": 65},
            {"situacao": "CANCELADO", "valor": 0, "quantidade": 0}
        ]))
        .unwrap();
        let texto = render_sumario(&filtro_padrao(dia(9, 23)), &itens);
        assert!(
            texto.ends_with(
                "\
Situação   Quantidade        Valor
a receber          30  R$ 1.000,00
recebida           65  R$ 4.000,50
Total              95  R$ 5.000,50"
            ),
            "{texto}"
        );
        let vazio = render_sumario(&filtro_padrao(dia(9, 23)), &[]);
        assert!(vazio.ends_with("Nenhuma cobrança encontrada."));
    }

    #[test]
    fn page_mode_says_where_it_stands() {
        let pagina: inter_pj::cobranca::PaginaCobrancas = serde_json::from_value(json!({
            "totalPaginas": 3, "totalElementos": 250, "ultimaPagina": false, "cobrancas": []
        }))
        .unwrap();
        assert_eq!(
            paginacao(1, &pagina),
            "\n\nPágina 1 de 2 (a primeira é 0); 250 cobranças no período; a próxima é --pagina 2"
        );
    }
}
