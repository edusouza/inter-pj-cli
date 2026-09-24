//! `inter-pj pix` commands of the Pix API (`/pix/v2`): the charges, the Pix
//! received and their refunds, the locations and the batches.
//!
//! Doc comments here are `--help` text too (in Portuguese).

use std::path::PathBuf;
use std::time::Duration;

use chrono::{DateTime, FixedOffset, NaiveDate};
use clap::{ArgAction, ArgGroup, Args, Subcommand, ValueEnum};
use inter_pj::cobranca::Uf;
use inter_pj::documento::Documento;
use inter_pj::pix::{
    ChavePix, IdDevolucao, IdDevolucaoError, InfoAdicional, NaturezaDevolucao, StatusCobvLote,
    TipoCob, Txid, TxidError,
};
use rust_decimal::Decimal;

use super::{
    TaxaOuValor, parse_cep, parse_chave, parse_data, parse_documento, parse_duracao,
    parse_taxa_ou_valor, parse_uf,
};
use crate::valor::parse_valor;

#[derive(Debug, Subcommand)]
pub(crate) enum PixCobCommand {
    /// Cria uma cobrança imediata, após mostrar um resumo e pedir confirmação
    Criar(Box<PixCobCriarArgs>),
    /// Altera ou remove uma cobrança imediata, após mostrar o antes e o depois
    Revisar(Box<PixCobRevisarArgs>),
    /// Mostra uma cobrança imediata e os Pix que a pagaram (com o QR Code, se pedido)
    Consultar(PixCobConsultarArgs),
    /// Cobranças imediatas criadas em um período (padrão: últimos 30 dias), com filtros
    Listar(PixCobListarArgs),
    /// Sandbox: paga uma cobrança imediata, para testar o fluxo completo (recusado em produção)
    Pagar(PixSandboxPagarArgs),
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Cobrança")]
pub(crate) struct PixCobCriarArgs {
    /// Chave Pix da conta que recebe: e-mail, telefone com +55, CPF/CNPJ ou chave aleatória
    #[arg(long, value_name = "CHAVE", value_parser = parse_chave)]
    pub(crate) chave: ChavePix,

    /// Valor: 149,90, 1.500,00 ou 149.90
    #[arg(long, value_name = "VALOR", value_parser = parse_valor)]
    pub(crate) valor: Decimal,

    /// Permite que o pagador altere o valor
    #[arg(long)]
    pub(crate) valor_alteravel: bool,

    /// Tempo até a cobrança expirar, contado da criação: 3600s, 30m, 2h, 7d [padrão da API: 1 dia]
    #[arg(long, value_name = "DURACAO", value_parser = parse_expiracao)]
    pub(crate) expiracao: Option<u32>,

    /// CPF ou CNPJ de quem paga
    #[arg(long, value_name = "CPF/CNPJ", value_parser = parse_documento, requires = "devedor_nome")]
    pub(crate) devedor_documento: Option<Documento>,

    /// Nome de quem paga, até 200 caracteres
    #[arg(long, value_name = "NOME", requires = "devedor_documento")]
    pub(crate) devedor_nome: Option<String>,

    /// Texto mostrado ao pagador, até 140 caracteres
    #[arg(long, value_name = "TEXTO")]
    pub(crate) solicitacao: Option<String>,

    /// Informação mostrada ao pagador, como NOME=VALOR; repita para até 50
    #[arg(long, value_name = "NOME=VALOR", value_parser = parse_info, action = ArgAction::Append)]
    pub(crate) info: Vec<InfoAdicional>,

    /// Location criada antes, para usar nesta cobrança
    #[arg(long, value_name = "ID")]
    pub(crate) loc: Option<u64>,

    /// txid, de 26 a 35 letras e dígitos [padrão: gerado]; repetir o comando com o mesmo txid não cria outra cobrança
    #[arg(long, value_name = "TXID", value_parser = parse_txid)]
    pub(crate) txid: Option<Txid>,

    #[command(flatten)]
    pub(crate) qr: QrCodeArgs,

    /// Confirma sem perguntar (para scripts)
    #[arg(long, conflicts_with = "simular", help_heading = "Segurança")]
    pub(crate) sim: bool,

    /// Mostra a requisição que seria enviada, sem enviar nada
    #[arg(long, conflicts_with_all = ["qrcode", "qrcode_png"], help_heading = "Segurança")]
    pub(crate) simular: bool,
}

/// `--qrcode` and `--qrcode-png` of the Pix charges.
#[derive(Debug, Args)]
#[command(next_help_heading = "QR Code")]
pub(crate) struct QrCodeArgs {
    /// Desenha no terminal o QR Code do Pix, para ler com o celular
    #[arg(long)]
    pub(crate) qrcode: bool,

    /// Grava o QR Code do Pix em uma imagem PNG ("-" para a saída padrão)
    #[arg(long, value_name = "ARQUIVO")]
    pub(crate) qrcode_png: Option<PathBuf>,

    /// Sobrescreve a imagem se ela já existir
    #[arg(long, requires = "qrcode_png")]
    pub(crate) sobrescrever: bool,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "O que muda")]
#[command(group(
    ArgGroup::new("mudanca")
        .required(true)
        .multiple(true)
        .args(["valor", "valor_alteravel", "expiracao", "devedor_documento", "solicitacao", "info", "loc", "chave", "remover"])
))]
pub(crate) struct PixCobRevisarArgs {
    /// txid da cobrança
    #[arg(value_name = "TXID", value_parser = parse_txid)]
    pub(crate) txid: Txid,

    /// Novo valor
    #[arg(long, value_name = "VALOR", value_parser = parse_valor)]
    pub(crate) valor: Option<Decimal>,

    /// Se o pagador pode alterar o valor: sim ou nao
    #[arg(
        long,
        value_name = "SIM|NAO",
        value_enum,
        ignore_case = true,
        hide_possible_values = true
    )]
    pub(crate) valor_alteravel: Option<SimNao>,

    /// Novo tempo até expirar, contado da criação: 3600s, 30m, 2h, 7d
    #[arg(long, value_name = "DURACAO", value_parser = parse_expiracao)]
    pub(crate) expiracao: Option<u32>,

    /// CPF ou CNPJ do novo devedor
    #[arg(long, value_name = "CPF/CNPJ", value_parser = parse_documento, requires = "devedor_nome")]
    pub(crate) devedor_documento: Option<Documento>,

    /// Nome do novo devedor
    #[arg(long, value_name = "NOME", requires = "devedor_documento")]
    pub(crate) devedor_nome: Option<String>,

    /// Novo texto mostrado ao pagador
    #[arg(long, value_name = "TEXTO")]
    pub(crate) solicitacao: Option<String>,

    /// Novas informações ao pagador, como NOME=VALOR, no lugar das atuais; repita para até 50
    #[arg(long, value_name = "NOME=VALOR", value_parser = parse_info, action = ArgAction::Append)]
    pub(crate) info: Vec<InfoAdicional>,

    /// Nova location
    #[arg(long, value_name = "ID")]
    pub(crate) loc: Option<u64>,

    /// Nova chave Pix da conta que recebe
    #[arg(long, value_name = "CHAVE", value_parser = parse_chave)]
    pub(crate) chave: Option<ChavePix>,

    /// Remove a cobrança: ela deixa de poder ser paga
    #[arg(long, conflicts_with_all = ["valor", "valor_alteravel", "expiracao", "devedor_documento", "solicitacao", "info", "loc", "chave"])]
    pub(crate) remover: bool,

    /// Confirma sem perguntar (para scripts)
    #[arg(long, help_heading = "Segurança")]
    pub(crate) sim: bool,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct PixCobConsultarArgs {
    /// txid da cobrança, mostrado por `pix cob criar` e `pix cob listar`
    #[arg(value_name = "TXID", value_parser = parse_txid)]
    pub(crate) txid: Txid,

    #[command(flatten)]
    pub(crate) qr: QrCodeArgs,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct PixCobListarArgs {
    #[command(flatten)]
    pub(crate) periodo: PeriodoPixArgs,

    /// Apenas deste devedor (CPF ou CNPJ)
    #[arg(long, value_name = "CPF/CNPJ", value_parser = parse_documento)]
    pub(crate) documento: Option<Documento>,

    /// Apenas neste status: ativa, concluida, removida-pelo-usuario ou removida-pelo-psp
    #[arg(
        long,
        value_enum,
        ignore_case = true,
        value_name = "STATUS",
        hide_possible_values = true
    )]
    pub(crate) status: Option<StatusCobArg>,

    /// Apenas cobranças com location
    #[arg(long, conflicts_with = "sem_location")]
    pub(crate) com_location: bool,

    /// Apenas cobranças sem location
    #[arg(long)]
    pub(crate) sem_location: bool,

    /// Traz só esta página (a primeira é 0), em vez de todas
    #[arg(long, value_name = "N")]
    pub(crate) pagina: Option<u32>,

    /// Itens por página com --pagina, de 1 a 1000 [padrão da API: 100]
    #[arg(long, value_name = "N", requires = "pagina")]
    pub(crate) itens_por_pagina: Option<u32>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum PixCobvCommand {
    /// Cria uma cobrança com vencimento (pelas opções ou por --arquivo), após mostrar um resumo e pedir confirmação
    Criar(Box<PixCobvCriarArgs>),
    /// Imprime um arquivo JSON de exemplo para `pix cobv criar --arquivo`, com dados fictícios
    Modelo(PixCobvModeloArgs),
    /// Altera ou remove uma cobrança com vencimento, após mostrar o antes e o depois
    Revisar(Box<PixCobvRevisarArgs>),
    /// Mostra uma cobrança com vencimento, seus encargos e os Pix que a pagaram (com o QR Code, se pedido)
    Consultar(PixCobvConsultarArgs),
    /// Cobranças com vencimento criadas em um período (padrão: últimos 30 dias), com filtros
    Listar(PixCobvListarArgs),
    /// Sandbox: paga uma cobrança com vencimento, para testar o fluxo completo (recusado em produção)
    Pagar(PixSandboxPagarArgs),
}

/// The options of a charge with a due date, which `--arquivo` replaces.
const OPCOES_COBV: [&str; 21] = [
    "chave",
    "valor",
    "vencimento",
    "validade_apos_vencimento",
    "devedor_documento",
    "devedor_nome",
    "devedor_email",
    "devedor_endereco",
    "devedor_cidade",
    "devedor_uf",
    "devedor_cep",
    "multa",
    "juros",
    "juros_periodo",
    "abatimento",
    "desconto",
    "desconto_por_dia",
    "dias_uteis",
    "solicitacao",
    "info",
    "loc",
];

/// Where the charge comes from: a file, or the options.
#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("origem")
        .required(true)
        .args(["arquivo", "chave"])
))]
pub(crate) struct PixCobvCriarArgs {
    /// Arquivo JSON com a cobrança, nos campos da API ("-" para a entrada padrão); veja `pix cobv modelo`
    #[arg(
        long,
        value_name = "ARQUIVO",
        conflicts_with_all = OPCOES_COBV,
        help_heading = "Origem (escolha uma)"
    )]
    pub(crate) arquivo: Option<PathBuf>,

    /// Chave Pix da conta que recebe: e-mail, telefone com +55, CPF/CNPJ ou chave aleatória
    #[arg(
        long,
        value_name = "CHAVE",
        value_parser = parse_chave,
        requires_all = ["valor", "vencimento", "devedor_documento"],
        help_heading = "Origem (escolha uma)"
    )]
    pub(crate) chave: Option<ChavePix>,

    /// Valor: 150,00, 1.500,00 ou 150.00
    #[arg(long, value_name = "VALOR", value_parser = parse_valor, help_heading = "Cobrança")]
    pub(crate) valor: Option<Decimal>,

    /// Vencimento (AAAA-MM-DD), hoje ou depois: até esse dia, a cobrança é paga sem multa nem juros
    #[arg(long, value_name = "AAAA-MM-DD", value_parser = parse_data, help_heading = "Cobrança")]
    pub(crate) vencimento: Option<NaiveDate>,

    /// Dias corridos após o vencimento em que a cobrança ainda pode ser paga, com multa e juros [padrão da API: 30]
    #[arg(long, value_name = "DIAS", help_heading = "Cobrança")]
    pub(crate) validade_apos_vencimento: Option<u32>,

    /// Texto mostrado ao pagador, até 140 caracteres
    #[arg(long, value_name = "TEXTO", help_heading = "Cobrança")]
    pub(crate) solicitacao: Option<String>,

    /// Informação mostrada ao pagador, como NOME=VALOR; repita para até 50
    #[arg(
        long,
        value_name = "NOME=VALOR",
        value_parser = parse_info,
        action = ArgAction::Append,
        help_heading = "Cobrança"
    )]
    pub(crate) info: Vec<InfoAdicional>,

    /// Location criada antes, para usar nesta cobrança
    #[arg(long, value_name = "ID", help_heading = "Cobrança")]
    pub(crate) loc: Option<u64>,

    /// txid, de 26 a 35 letras e dígitos [padrão: gerado]; repetir o comando com o mesmo txid não cria outra cobrança
    #[arg(long, value_name = "TXID", value_parser = parse_txid, help_heading = "Cobrança")]
    pub(crate) txid: Option<Txid>,

    #[command(flatten)]
    pub(crate) devedor: DevedorCobvArgs,

    #[command(flatten)]
    pub(crate) encargos: EncargosCobvArgs,

    #[command(flatten)]
    pub(crate) qr: QrCodeArgs,

    /// Confirma sem perguntar (para scripts)
    #[arg(long, conflicts_with = "simular", help_heading = "Segurança")]
    pub(crate) sim: bool,

    /// Mostra a requisição que seria enviada, sem enviar nada
    #[arg(long, conflicts_with_all = ["qrcode", "qrcode_png"], help_heading = "Segurança")]
    pub(crate) simular: bool,
}

/// Who pays a charge with a due date.
#[derive(Debug, Args)]
#[command(next_help_heading = "Devedor")]
#[allow(clippy::struct_field_names)] // the fields are the options, --devedor-*
pub(crate) struct DevedorCobvArgs {
    /// CPF ou CNPJ de quem paga
    #[arg(long, value_name = "CPF/CNPJ", value_parser = parse_documento, requires = "devedor_nome")]
    pub(crate) devedor_documento: Option<Documento>,

    /// Nome de quem paga, até 200 caracteres
    #[arg(long, value_name = "NOME", requires = "devedor_documento")]
    pub(crate) devedor_nome: Option<String>,

    /// E-mail de quem paga
    #[arg(long, value_name = "EMAIL", requires = "devedor_documento")]
    pub(crate) devedor_email: Option<String>,

    /// Endereço de quem paga: rua, número e complemento, até 200 caracteres
    #[arg(long, value_name = "ENDERECO", requires = "devedor_documento")]
    pub(crate) devedor_endereco: Option<String>,

    /// Cidade
    #[arg(long, value_name = "CIDADE", requires = "devedor_documento")]
    pub(crate) devedor_cidade: Option<String>,

    /// UF (sigla do estado)
    #[arg(long, value_name = "UF", value_parser = parse_uf, requires = "devedor_documento")]
    pub(crate) devedor_uf: Option<Uf>,

    /// CEP: 30110-000 ou 30110000
    #[arg(long, value_name = "CEP", value_parser = parse_cep, requires = "devedor_documento")]
    pub(crate) devedor_cep: Option<String>,
}

/// Fine, interest, rebate and discount of a charge with a due date.
#[derive(Debug, Args)]
#[command(next_help_heading = "Encargos")]
#[command(group(
    ArgGroup::new("por_dia")
        .multiple(true)
        .args(["juros", "desconto_por_dia"])
))]
pub(crate) struct EncargosCobvArgs {
    /// Multa por pagar depois do vencimento: percentual (2%) ou valor (4,00)
    #[arg(long, value_name = "TAXA|VALOR", value_parser = parse_taxa_ou_valor)]
    pub(crate) multa: Option<TaxaOuValor>,

    /// Juros por pagar depois do vencimento: percentual (1%, ao mês; veja --juros-periodo) ou valor por dia (0,50)
    #[arg(long, value_name = "TAXA|VALOR", value_parser = parse_taxa_ou_valor)]
    pub(crate) juros: Option<TaxaOuValor>,

    /// Período do percentual de --juros: dia, mes ou ano [padrão: mes]
    #[arg(
        long,
        value_name = "PERIODO",
        value_enum,
        ignore_case = true,
        hide_possible_values = true,
        requires = "juros"
    )]
    pub(crate) juros_periodo: Option<PeriodoJuros>,

    /// Abatimento, qualquer que seja o dia do pagamento: percentual (5%) ou valor (10,00)
    #[arg(long, value_name = "TAXA|VALOR", value_parser = parse_taxa_ou_valor)]
    pub(crate) abatimento: Option<TaxaOuValor>,

    /// Desconto por pagar até uma data: 2% ou 10,00 até o vencimento, ou até a data depois do @ (2%@2026-10-15); repita para até 3 datas
    #[arg(
        long,
        value_name = "TAXA|VALOR[@DATA]",
        value_parser = parse_desconto,
        action = ArgAction::Append,
        conflicts_with = "desconto_por_dia"
    )]
    pub(crate) desconto: Vec<DescontoAte>,

    /// Desconto por dia pago antes do vencimento: percentual (0,5%) ou valor (0,10)
    #[arg(long, value_name = "TAXA|VALOR", value_parser = parse_taxa_ou_valor)]
    pub(crate) desconto_por_dia: Option<TaxaOuValor>,

    /// Juros e desconto por dia contam só os dias úteis [padrão: dias corridos]
    #[arg(long, requires = "por_dia")]
    pub(crate) dias_uteis: bool,
}

/// `--juros-periodo`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum PeriodoJuros {
    Dia,
    #[value(alias = "mês")]
    Mes,
    Ano,
}

/// `--desconto`: a percentage or an amount, until a date (by default, the
/// due date).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DescontoAte {
    pub(crate) valor: TaxaOuValor,
    pub(crate) ate: Option<NaiveDate>,
}

#[derive(Debug, Args)]
pub(crate) struct PixCobvModeloArgs {}

/// What changes in a charge with a due date.
#[derive(Debug, Args)]
#[command(next_help_heading = "O que muda")]
#[command(group(
    ArgGroup::new("mudanca")
        .required(true)
        .multiple(true)
        .args(MUDANCAS_COBV)
        .arg("remover")
))]
pub(crate) struct PixCobvRevisarArgs {
    /// txid da cobrança
    #[arg(value_name = "TXID", value_parser = parse_txid)]
    pub(crate) txid: Txid,

    /// Novo valor
    #[arg(long, value_name = "VALOR", value_parser = parse_valor, help_heading = "O que muda")]
    pub(crate) valor: Option<Decimal>,

    /// Novo vencimento (AAAA-MM-DD), hoje ou depois
    #[arg(long, value_name = "AAAA-MM-DD", value_parser = parse_data, help_heading = "O que muda")]
    pub(crate) vencimento: Option<NaiveDate>,

    /// Novos dias corridos após o vencimento em que a cobrança ainda pode ser paga
    #[arg(long, value_name = "DIAS", help_heading = "O que muda")]
    pub(crate) validade_apos_vencimento: Option<u32>,

    /// Novo texto mostrado ao pagador
    #[arg(long, value_name = "TEXTO", help_heading = "O que muda")]
    pub(crate) solicitacao: Option<String>,

    /// Novas informações ao pagador, como NOME=VALOR, no lugar das atuais; repita para até 50
    #[arg(
        long,
        value_name = "NOME=VALOR",
        value_parser = parse_info,
        action = ArgAction::Append,
        help_heading = "O que muda"
    )]
    pub(crate) info: Vec<InfoAdicional>,

    /// Nova location
    #[arg(long, value_name = "ID", help_heading = "O que muda")]
    pub(crate) loc: Option<u64>,

    /// Nova chave Pix da conta que recebe
    #[arg(long, value_name = "CHAVE", value_parser = parse_chave, help_heading = "O que muda")]
    pub(crate) chave: Option<ChavePix>,

    /// Remove a cobrança: ela deixa de poder ser paga
    #[arg(long, conflicts_with_all = MUDANCAS_COBV, help_heading = "O que muda")]
    pub(crate) remover: bool,

    // The payer given replaces the current one, e-mail and address included.
    #[command(flatten)]
    pub(crate) devedor: DevedorCobvArgs,

    #[command(flatten)]
    pub(crate) encargos: EncargosCobvArgs,

    /// Confirma sem perguntar (para scripts)
    #[arg(long, help_heading = "Segurança")]
    pub(crate) sim: bool,
}

/// What a revision can change, besides removing the charge.
const MUDANCAS_COBV: [&str; 13] = [
    "valor",
    "vencimento",
    "validade_apos_vencimento",
    "devedor_documento",
    "multa",
    "juros",
    "abatimento",
    "desconto",
    "desconto_por_dia",
    "solicitacao",
    "info",
    "loc",
    "chave",
];

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct PixCobvConsultarArgs {
    /// txid da cobrança, mostrado por `pix cobv criar` e `pix cobv listar`
    #[arg(value_name = "TXID", value_parser = parse_txid)]
    pub(crate) txid: Txid,

    #[command(flatten)]
    pub(crate) qr: QrCodeArgs,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct PixCobvListarArgs {
    #[command(flatten)]
    pub(crate) filtros: PixCobListarArgs,

    /// Apenas as cobranças deste lote
    #[arg(long, value_name = "ID")]
    pub(crate) lote: Option<u32>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum PixRecebidosCommand {
    /// Pix recebidos em um período (padrão: últimos 30 dias), com filtros
    Listar(PixRecebidosListarArgs),
    /// Mostra um Pix recebido e as suas devoluções
    Consultar(PixRecebidoConsultarArgs),
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct PixRecebidosListarArgs {
    #[command(flatten)]
    pub(crate) periodo: PeriodoPixArgs,

    /// Apenas os Pix desta cobrança
    #[arg(long, value_name = "TXID", value_parser = parse_txid)]
    pub(crate) txid: Option<Txid>,

    #[command(flatten)]
    pub(crate) cobranca: ComSemCobrancaArgs,

    #[command(flatten)]
    pub(crate) devolucao: ComSemDevolucaoArgs,

    /// Apenas deste pagador (CPF ou CNPJ)
    #[arg(long, value_name = "CPF/CNPJ", value_parser = parse_documento)]
    pub(crate) documento: Option<Documento>,

    /// Traz só esta página (a primeira é 0), em vez de todas
    #[arg(long, value_name = "N")]
    pub(crate) pagina: Option<u32>,

    /// Itens por página com --pagina, de 1 a 1000 [padrão da API: 100]
    #[arg(long, value_name = "N", requires = "pagina")]
    pub(crate) itens_por_pagina: Option<u32>,
}

/// `--com-cobranca` or `--sem-cobranca`.
#[derive(Debug, Clone, Copy, Args)]
pub(crate) struct ComSemCobrancaArgs {
    /// Apenas os Pix de cobranças (com txid)
    #[arg(long, conflicts_with_all = ["sem_cobranca", "txid"])]
    pub(crate) com_cobranca: bool,

    /// Apenas os Pix sem cobrança (sem txid)
    #[arg(long, conflicts_with = "txid")]
    pub(crate) sem_cobranca: bool,
}

/// `--com-devolucao` or `--sem-devolucao`.
#[derive(Debug, Clone, Copy, Args)]
pub(crate) struct ComSemDevolucaoArgs {
    /// Apenas os Pix com alguma devolução
    #[arg(long, conflicts_with = "sem_devolucao")]
    pub(crate) com_devolucao: bool,

    /// Apenas os Pix sem devolução
    #[arg(long)]
    pub(crate) sem_devolucao: bool,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct PixRecebidoConsultarArgs {
    /// endToEndId do Pix, mostrado por `pix recebidos listar` e no extrato
    #[arg(value_name = "E2EID", value_parser = parse_e2e)]
    pub(crate) e2e: String,
}

#[derive(Debug, Subcommand)]
pub(crate) enum PixDevolucaoCommand {
    /// Devolve um Pix recebido, todo ou em parte, após mostrar um resumo e pedir confirmação
    Solicitar(Box<PixDevolucaoSolicitarArgs>),
    /// Mostra em que pé está uma devolução (com --aguardar, até o fim)
    Consultar(PixDevolucaoConsultarArgs),
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Devolução")]
#[command(group(
    ArgGroup::new("quanto")
        .required(true)
        .args(["valor", "tudo"])
))]
pub(crate) struct PixDevolucaoSolicitarArgs {
    /// endToEndId do Pix recebido, mostrado por `pix recebidos listar`
    #[arg(value_name = "E2EID", value_parser = parse_e2e)]
    pub(crate) e2e: String,

    /// Valor a devolver: 150,00, 1.500,00 ou 150.00
    #[arg(long, value_name = "VALOR", value_parser = parse_valor)]
    pub(crate) valor: Option<Decimal>,

    /// Devolve tudo o que ainda não foi devolvido do Pix
    #[arg(long, conflicts_with = "simular")]
    pub(crate) tudo: bool,

    /// O que devolver: original (um Pix comum, ou a compra de um Pix Troco) ou retirada (o dinheiro de um Pix Saque, ou o troco) [padrão da API: original]
    #[arg(
        long,
        value_name = "NATUREZA",
        value_enum,
        ignore_case = true,
        hide_possible_values = true
    )]
    pub(crate) natureza: Option<NaturezaArg>,

    /// Mensagem ao pagador, até 140 caracteres
    #[arg(long, value_name = "TEXTO")]
    pub(crate) descricao: Option<String>,

    /// id da devolução, de 1 a 35 letras e dígitos [padrão: gerado]; repetir o comando com o mesmo id não devolve de novo
    #[arg(long, value_name = "ID", value_parser = parse_id_devolucao)]
    pub(crate) id: Option<IdDevolucao>,

    #[command(flatten)]
    pub(crate) espera: EsperaDevolucaoArgs,

    /// Confirma sem perguntar (para scripts)
    #[arg(long, conflicts_with = "simular", help_heading = "Segurança")]
    pub(crate) sim: bool,

    /// Mostra a requisição que seria enviada, sem enviar nada
    #[arg(long, conflicts_with = "aguardar", help_heading = "Segurança")]
    pub(crate) simular: bool,
}

/// `--natureza` of a refund.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum NaturezaArg {
    #[value(alias = "ORIGINAL")]
    Original,
    #[value(alias = "RETIRADA")]
    Retirada,
}

impl From<NaturezaArg> for NaturezaDevolucao {
    fn from(natureza: NaturezaArg) -> Self {
        match natureza {
            NaturezaArg::Original => Self::Original,
            NaturezaArg::Retirada => Self::Retirada,
        }
    }
}

/// `--aguardar` and `--timeout` of the refunds.
#[derive(Debug, Clone, Copy, Args)]
#[command(next_help_heading = "Espera")]
pub(crate) struct EsperaDevolucaoArgs {
    /// Consulta a cada 6 segundos até a devolução terminar, feita ou não
    #[arg(long)]
    pub(crate) aguardar: bool,

    /// Tempo máximo de espera com --aguardar: 60s, 5m [padrão: 60s]
    #[arg(
        long,
        value_name = "DURACAO",
        value_parser = parse_duracao,
        default_value = "60s",
        hide_default_value = true,
        requires = "aguardar"
    )]
    pub(crate) timeout: Duration,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct PixDevolucaoConsultarArgs {
    /// endToEndId do Pix devolvido
    #[arg(value_name = "E2EID", value_parser = parse_e2e)]
    pub(crate) e2e: String,

    /// id da devolução, mostrado por `pix devolucao solicitar`
    #[arg(value_name = "ID", value_parser = parse_id_devolucao)]
    pub(crate) id: IdDevolucao,

    #[command(flatten)]
    pub(crate) espera: EsperaDevolucaoArgs,
}

#[derive(Debug, Subcommand)]
pub(crate) enum PixLocCommand {
    /// Cria uma location, o endereço do QR Code de uma cobrança, para usar depois com --loc
    Criar(PixLocCriarArgs),
    /// Locations criadas em um período (padrão: últimos 30 dias), com filtros
    Listar(PixLocListarArgs),
    /// Mostra uma location e a cobrança vinculada a ela
    Consultar(PixLocConsultarArgs),
    /// Desvincula a cobrança de uma location, após mostrá-la e pedir confirmação
    Desvincular(PixLocDesvincularArgs),
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct PixLocCriarArgs {
    /// Para que cobrança: cob (imediata) ou cobv (com vencimento)
    #[arg(
        long,
        value_name = "TIPO",
        value_enum,
        ignore_case = true,
        hide_possible_values = true
    )]
    pub(crate) tipo: TipoCobArg,
}

/// `--tipo` of the locations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum TipoCobArg {
    Cob,
    Cobv,
}

impl From<TipoCobArg> for TipoCob {
    fn from(tipo: TipoCobArg) -> Self {
        match tipo {
            TipoCobArg::Cob => Self::Cob,
            TipoCobArg::Cobv => Self::Cobv,
        }
    }
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct PixLocListarArgs {
    #[command(flatten)]
    pub(crate) periodo: PeriodoPixArgs,

    /// Apenas as locations para este tipo de cobrança: cob ou cobv
    #[arg(
        long,
        value_name = "TIPO",
        value_enum,
        ignore_case = true,
        hide_possible_values = true
    )]
    pub(crate) tipo: Option<TipoCobArg>,

    #[command(flatten)]
    pub(crate) vinculo: ComSemVinculoArgs,

    /// Traz só esta página (a primeira é 0), em vez de todas
    #[arg(long, value_name = "N")]
    pub(crate) pagina: Option<u32>,

    /// Itens por página com --pagina, de 1 a 1000 [padrão da API: 100]
    #[arg(long, value_name = "N", requires = "pagina")]
    pub(crate) itens_por_pagina: Option<u32>,
}

/// `--com-cobranca` or `--sem-cobranca` of the locations.
#[derive(Debug, Clone, Copy, Args)]
pub(crate) struct ComSemVinculoArgs {
    /// Apenas as locations com uma cobrança vinculada
    #[arg(long, conflicts_with = "sem_cobranca")]
    pub(crate) com_cobranca: bool,

    /// Apenas as locations livres, sem cobrança
    #[arg(long)]
    pub(crate) sem_cobranca: bool,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct PixLocConsultarArgs {
    /// id da location, mostrado por `pix loc criar` e `pix loc listar`
    #[arg(value_name = "ID")]
    pub(crate) id: u64,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct PixLocDesvincularArgs {
    /// id da location
    #[arg(value_name = "ID")]
    pub(crate) id: u64,

    /// Confirma sem perguntar (para scripts)
    #[arg(long, help_heading = "Segurança")]
    pub(crate) sim: bool,
}

#[derive(Debug, Subcommand)]
pub(crate) enum PixLoteCobvCommand {
    /// Cria um lote de cobranças com vencimento a partir de um arquivo JSON ou CSV, após mostrar um resumo e pedir confirmação
    Criar(PixLoteCobvArquivoArgs),
    /// Altera cobranças de um lote a partir de um arquivo JSON ou CSV, após mostrar um resumo e pedir confirmação
    Revisar(PixLoteCobvArquivoArgs),
    /// Mostra um lote e em que pé está cada cobrança (com --aguardar, até o fim do processamento)
    Consultar(PixLoteCobvConsultarArgs),
    /// Lotes criados em um período (padrão: últimos 30 dias)
    Listar(PixLoteCobvListarArgs),
    /// Totais do processamento de um lote
    Sumario(PixLoteCobvIdArgs),
    /// As cobranças de um lote em uma situação: em-processamento, criada ou negada
    Situacao(PixLoteCobvSituacaoArgs),
    /// Imprime um arquivo de exemplo para `pix lote-cobv criar`: json (padrão) ou csv
    Modelo(super::LoteModeloArgs),
}

/// `criar` and `revisar`: the batch and its file.
#[derive(Debug, Args)]
#[command(next_help_heading = "Lote")]
pub(crate) struct PixLoteCobvArquivoArgs {
    /// id do lote, um número escolhido por você
    #[arg(value_name = "ID")]
    pub(crate) id: u64,

    /// Arquivo JSON, nos campos da API, ou CSV, uma cobrança por linha com os nomes da API nas colunas ("-" para a entrada padrão); veja `pix lote-cobv modelo`
    #[arg(long, value_name = "ARQUIVO")]
    pub(crate) arquivo: PathBuf,

    /// Descrição do lote; obrigatória na criação quando o arquivo não a tem (CSV)
    #[arg(long, value_name = "TEXTO")]
    pub(crate) descricao: Option<String>,

    /// Confirma sem perguntar (para scripts)
    #[arg(long, conflicts_with = "simular", help_heading = "Segurança")]
    pub(crate) sim: bool,

    /// Mostra a requisição que seria enviada, sem enviar nada
    #[arg(long, help_heading = "Segurança")]
    pub(crate) simular: bool,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct PixLoteCobvConsultarArgs {
    /// id do lote
    #[arg(value_name = "ID")]
    pub(crate) id: u64,

    /// Consulta a cada 6 segundos até nenhuma cobrança do lote estar em processamento
    #[arg(long)]
    pub(crate) aguardar: bool,

    /// Tempo máximo de espera com --aguardar: 60s, 5m [padrão: 60s]
    #[arg(
        long,
        value_name = "DURACAO",
        value_parser = parse_duracao,
        default_value = "60s",
        hide_default_value = true,
        requires = "aguardar"
    )]
    pub(crate) timeout: Duration,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct PixLoteCobvListarArgs {
    #[command(flatten)]
    pub(crate) periodo: PeriodoPixArgs,

    /// Traz só esta página (a primeira é 0), em vez de todas
    #[arg(long, value_name = "N")]
    pub(crate) pagina: Option<u32>,

    /// Itens por página com --pagina, de 1 a 1000 [padrão da API: 100]
    #[arg(long, value_name = "N", requires = "pagina")]
    pub(crate) itens_por_pagina: Option<u32>,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct PixLoteCobvIdArgs {
    /// id do lote
    #[arg(value_name = "ID")]
    pub(crate) id: u64,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct PixLoteCobvSituacaoArgs {
    /// id do lote
    #[arg(value_name = "ID")]
    pub(crate) id: u64,

    /// em-processamento, criada ou negada
    #[arg(
        value_name = "SITUACAO",
        value_enum,
        ignore_case = true,
        hide_possible_values = true
    )]
    pub(crate) situacao: SituacaoLoteArg,
}

/// The situation of a charge of a batch, with the API's codes as aliases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum SituacaoLoteArg {
    #[value(alias = "EM_PROCESSAMENTO")]
    EmProcessamento,
    #[value(alias = "CRIADA")]
    Criada,
    #[value(alias = "NEGADA")]
    Negada,
}

impl From<SituacaoLoteArg> for StatusCobvLote {
    fn from(situacao: SituacaoLoteArg) -> Self {
        match situacao {
            SituacaoLoteArg::EmProcessamento => Self::EmProcessamento,
            SituacaoLoteArg::Criada => Self::Criada,
            SituacaoLoteArg::Negada => Self::Negada,
        }
    }
}

/// `pix cob pagar` and `pix cobv pagar`, in the sandbox.
#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct PixSandboxPagarArgs {
    /// txid da cobrança
    #[arg(value_name = "TXID", value_parser = parse_txid)]
    pub(crate) txid: Txid,

    /// Valor pago: 150,00, 1.500,00 ou 150.00 [padrão: o valor da cobrança]
    #[arg(long, value_name = "VALOR", value_parser = parse_valor)]
    pub(crate) valor: Option<Decimal>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum PixSandboxCommand {
    /// Sandbox: paga um Pix copia e cola, como um cliente pagaria o QR Code (recusado em produção)
    #[command(name = "pagar-qrcode")]
    PagarQrcode(PixPagarQrcodeArgs),
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Opções")]
pub(crate) struct PixPagarQrcodeArgs {
    /// O código copia e cola (o texto do QR Code) de uma cobrança do sandbox
    #[arg(long, value_name = "CODIGO", value_parser = super::parse_copia_e_cola)]
    pub(crate) copia_e_cola: super::CopiaECola,

    /// Valor pago [padrão: o do código; obrigatório quando o código não o traz]
    #[arg(long, value_name = "VALOR", value_parser = parse_valor)]
    pub(crate) valor: Option<Decimal>,
}

/// `--inicio` and `--fim` of the Pix listings.
#[derive(Debug, Clone, Copy, Args)]
pub(crate) struct PeriodoPixArgs {
    /// Início: AAAA-MM-DD (desde o começo do dia, no fuso local) ou data e hora com fuso (2026-09-01T08:00:00-03:00) [padrão: 30 dias atrás]
    #[arg(long, value_name = "DATA", value_parser = parse_momento)]
    pub(crate) inicio: Option<Momento>,

    /// Fim: AAAA-MM-DD (até o fim do dia) ou data e hora com fuso [padrão: agora]
    #[arg(long, value_name = "DATA", value_parser = parse_momento)]
    pub(crate) fim: Option<Momento>,
}

/// A date (the whole day, in the local time zone) or a moment with offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Momento {
    Dia(NaiveDate),
    Instante(DateTime<FixedOffset>),
}

/// `sim` or `nao`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum SimNao {
    Sim,
    #[value(alias = "não")]
    Nao,
}

/// `--status` of the Pix charges, with the API's codes as aliases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum StatusCobArg {
    #[value(alias = "ATIVA")]
    Ativa,
    #[value(alias = "CONCLUIDA")]
    Concluida,
    #[value(
        alias = "removida-pelo-usuario-recebedor",
        alias = "REMOVIDA_PELO_USUARIO_RECEBEDOR"
    )]
    RemovidaPeloUsuario,
    #[value(alias = "REMOVIDA_PELO_PSP")]
    RemovidaPeloPsp,
}

pub(super) fn parse_txid(value: &str) -> Result<Txid, String> {
    value.parse().map_err(|err: TxidError| err.to_string())
}

/// `NOME=VALOR`.
fn parse_info(value: &str) -> Result<InfoAdicional, String> {
    let (nome, valor) = value
        .split_once('=')
        .ok_or_else(|| "use NOME=VALOR, como Pedido=123".to_owned())?;
    Ok(InfoAdicional::new(nome.trim(), valor.trim()))
}

/// `3600s`, `30m`, `2h` or `7d` (a number alone is seconds).
fn parse_expiracao(value: &str) -> Result<u32, String> {
    let texto = value.trim();
    let (numero, unidade) = texto
        .find(|c: char| !c.is_ascii_digit())
        .map_or((texto, "s"), |i| texto.split_at(i));
    let segundos = match (numero.parse::<u32>(), unidade) {
        (Ok(n), "s") => Some(n),
        (Ok(n), "m") => n.checked_mul(60),
        (Ok(n), "h") => n.checked_mul(3600),
        (Ok(n), "d") => n.checked_mul(86_400),
        _ => None,
    };
    segundos.filter(|s| *s > 0).ok_or_else(|| {
        format!("duração inválida \"{texto}\": use, por exemplo, 3600s, 30m, 2h ou 7d")
    })
}

/// `2026-09-01` or `2026-09-01T08:00:00-03:00`.
fn parse_momento(value: &str) -> Result<Momento, String> {
    let texto = value.trim();
    if let Ok(instante) = DateTime::parse_from_rfc3339(texto) {
        return Ok(Momento::Instante(instante));
    }
    parse_data(texto).map(Momento::Dia).map_err(|_| {
        format!("data inválida \"{texto}\": use AAAA-MM-DD ou data e hora com fuso, como 2026-09-01T08:00:00-03:00")
    })
}

/// `2%`, `10,00` or `2%@2026-10-15`.
fn parse_desconto(value: &str) -> Result<DescontoAte, String> {
    let (valor, ate) = match value.split_once('@') {
        Some((valor, data)) => (valor, Some(parse_data(data.trim())?)),
        None => (value, None),
    };
    Ok(DescontoAte {
        valor: parse_taxa_ou_valor(valor)?,
        ate,
    })
}

/// The end-to-end id of a Pix: letters and digits (`E1234...`).
pub(super) fn parse_e2e(value: &str) -> Result<String, String> {
    let id = value.trim();
    if (1..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric()) {
        Ok(id.to_owned())
    } else {
        Err("endToEndId inválido: use as letras e os dígitos do identificador (ex.: E12345678202609231200abcdef12345)".to_owned())
    }
}

fn parse_id_devolucao(value: &str) -> Result<IdDevolucao, String> {
    value
        .parse()
        .map_err(|err: IdDevolucaoError| err.to_string())
}
