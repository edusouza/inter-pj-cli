//! `inter-pj cobranca emitir|modelo`

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use chrono::{Days, NaiveDate};
use inter_pj::cobranca::{
    CobrancaDetalhada, Desconto, EmissaoCobranca, FormaRecebimento, Mora, Multa, Pagador,
    SituacaoCobranca,
};
use inter_pj::{Environment, Error as InterError, InterClient, endpoint};

use super::consultar::mostrar;
use super::{argumento, descrever_situacao};
use crate::arquivo;
use crate::cli::{CobrancaEmitirArgs, FormaArg, Formato, TaxaOuValor};
use crate::commands::qrcode::OpcoesQr;
use crate::commands::{Context, hoje, simulacao};
use crate::confirmacao::{Terminal, confirmar, descrever_ambiente};
use crate::error::{CliError, resultado_incerto};
use crate::output::{self, percentual};
use crate::valor::por_extenso;

/// Between two queries with `--aguardar`: 10 per minute, the rate limit of
/// the sandbox (production allows 120).
const INTERVALO: Duration = Duration::from_secs(6);

pub(super) async fn emitir(
    context: &Context,
    args: &CobrancaEmitirArgs,
    terminal: &mut dyn Terminal,
) -> Result<(), CliError> {
    emitir_com(context, args, terminal, INTERVALO).await
}

async fn emitir_com(
    context: &Context,
    args: &CobrancaEmitirArgs,
    terminal: &mut dyn Terminal,
    intervalo: Duration,
) -> Result<(), CliError> {
    let hoje = hoje();
    let opcoes = OpcoesQr::new(
        context,
        args.depois.qrcode,
        args.depois.qrcode_png.clone(),
        args.depois.sobrescrever,
    )?;
    let cobranca = match &args.arquivo {
        Some(caminho) => arquivo::cobranca(&arquivo::ler_json(caminho)?, &arquivo::nome(caminho))?,
        None => das_opcoes(args)?,
    };
    if cobranca.data_vencimento < hoje {
        return Err(CliError::Usage(format!(
            "o vencimento ({}) já passou: a cobrança vence hoje ou depois",
            cobranca.data_vencimento.format("%d/%m/%Y")
        )));
    }
    let settings = context.settings()?;
    // Configuration problems show up before the confirmation, not after it.
    let client = if args.simular {
        None
    } else {
        Some(context.client(&settings)?)
    };
    let ambiente = settings.ambiente.as_ref().map(|setting| setting.value);
    eprintln!("{}", resumo(&cobranca, hoje, ambiente));

    let Some(client) = client else {
        return simulacao::mostrar(
            context,
            &settings,
            endpoint::cobranca::EMITIR,
            &[],
            &cobranca,
        );
    };
    confirmar(terminal, args.sim, "Emitir a cobrança?")?;
    let solicitacao = client.cobranca().emitir(&cobranca).await.map_err(|err| {
        if resultado_incerto(&err) {
            CliError::EmissaoIncerta {
                source: err,
                consulta: procurar(&cobranca),
            }
        } else {
            err.into()
        }
    })?;
    let codigo = solicitacao
        .codigo_solicitacao
        .as_deref()
        .filter(|codigo| !codigo.trim().is_empty());
    let codigo = match codigo {
        Some(codigo) if args.depois.aguardar => codigo.to_owned(),
        _ => {
            if args.depois.aguardar {
                eprintln!("aviso: a API não informou o código da cobrança; não há como aguardar");
            }
            return match context.formato() {
                Formato::Json => output::print_json(&solicitacao),
                // `commands::run` refuses csv for this command.
                Formato::Texto | Formato::Csv => {
                    output::print(&render_solicitacao(codigo, &cobranca))
                }
            };
        }
    };

    let (detalhe, emitida) = aguardar(&client, &codigo, args.depois.timeout, intervalo).await?;
    let erro = match detalhe.cobranca.situacao.as_ref() {
        _ if !emitida => CliError::TempoEsgotado {
            oque: "a cobrança",
            status: "em processamento".to_owned(),
            segundos: args.depois.timeout.as_secs(),
        },
        Some(situacao @ SituacaoCobranca::FalhaEmissao) => CliError::CobrancaNaoEmitida {
            situacao: descrever_situacao(situacao),
        },
        _ => return mostrar(context, &settings, &detalhe, &opcoes),
    };
    // No QR Code yet, and nothing for an image on the standard output.
    if !opcoes.png_no_stdout() {
        mostrar(context, &settings, &detalhe, &OpcoesQr::default())?;
    }
    Err(erro)
}

/// The command that finds the charge by its seu número.
fn procurar(cobranca: &EmissaoCobranca) -> String {
    format!(
        "inter-pj cobranca listar --filtrar-por emissao --seu-numero {}",
        argumento(&cobranca.seu_numero)
    )
}

/// The charge of the options. clap requires the mandatory ones together
/// with `--seu-numero`.
fn das_opcoes(args: &CobrancaEmitirArgs) -> Result<EmissaoCobranca, CliError> {
    fn obrigatorio<T: Clone>(valor: Option<&T>, opcao: &str) -> Result<T, CliError> {
        valor
            .cloned()
            .ok_or_else(|| CliError::Usage(format!("{opcao} é obrigatório")))
    }
    let mut pagador = Pagador::new(
        obrigatorio(args.pagador_documento.as_ref(), "--pagador-documento")?,
        obrigatorio(args.pagador_nome.as_ref(), "--pagador-nome")?,
        obrigatorio(args.pagador_endereco.as_ref(), "--pagador-endereco")?,
        obrigatorio(args.pagador_cidade.as_ref(), "--pagador-cidade")?,
        obrigatorio(args.pagador_uf.as_ref(), "--pagador-uf")?,
        obrigatorio(args.pagador_cep.as_ref(), "--pagador-cep")?,
    );
    pagador.numero.clone_from(&args.pagador_numero);
    pagador.complemento.clone_from(&args.pagador_complemento);
    pagador.bairro.clone_from(&args.pagador_bairro);
    pagador.email.clone_from(&args.pagador_email);
    if let Some(telefone) = &args.pagador_telefone {
        pagador.ddd = Some(telefone.ddd.clone());
        pagador.telefone = Some(telefone.numero.clone());
    }
    let mut cobranca = EmissaoCobranca::new(
        obrigatorio(args.seu_numero.as_ref(), "--seu-numero")?,
        obrigatorio(args.valor.as_ref(), "--valor")?,
        obrigatorio(args.vencimento.as_ref(), "--vencimento")?,
        pagador,
    );
    cobranca.num_dias_agenda = args.dias_agenda.unwrap_or(0);
    let dias = args.desconto_dias.unwrap_or(0);
    cobranca.desconto = args.desconto.map(|desconto| match desconto {
        TaxaOuValor::Taxa(taxa) => Desconto::Percentual {
            taxa,
            quantidade_dias: dias,
        },
        TaxaOuValor::Valor(valor) => Desconto::ValorFixo {
            valor,
            quantidade_dias: dias,
        },
    });
    cobranca.multa = args.multa.map(|multa| match multa {
        TaxaOuValor::Taxa(taxa) => Multa::Percentual { taxa },
        TaxaOuValor::Valor(valor) => Multa::ValorFixo { valor },
    });
    cobranca.mora = args.juros.map(|juros| match juros {
        TaxaOuValor::Taxa(taxa) => Mora::TaxaMensal { taxa },
        TaxaOuValor::Valor(valor) => Mora::ValorDia { valor },
    });
    cobranca.mensagem.clone_from(&args.mensagem);
    cobranca.formas_recebimento = args
        .receber_com
        .iter()
        .map(|forma| match forma {
            FormaArg::Boleto => FormaRecebimento::Boleto,
            FormaArg::Pix => FormaRecebimento::Pix,
        })
        .collect();
    cobranca
        .validar()
        .map_err(|err| CliError::Usage(format!("{}: {err}", opcao(err.campo()))))?;
    Ok(cobranca)
}

/// The option of a field of the API, for the messages.
fn opcao(campo: &str) -> String {
    let opcao = match campo {
        "seuNumero" => "--seu-numero",
        "valorNominal" => "--valor",
        "numDiasAgenda" => "--dias-agenda",
        "desconto.taxa" | "desconto.valor" => "--desconto",
        "multa.taxa" | "multa.valor" => "--multa",
        "mora.taxa" | "mora.valor" => "--juros",
        "formasRecebimento" => "--receber-com",
        "pagador.ddd" | "pagador.telefone" => "--pagador-telefone",
        mensagem if mensagem.starts_with("mensagem") => "--mensagem",
        pagador if pagador.starts_with("pagador.") => {
            return format!("--pagador-{}", &pagador["pagador.".len()..]);
        }
        outro => outro,
    };
    opcao.to_owned()
}

/// The charge about to be issued, and what deserves a second look.
fn resumo(cobranca: &EmissaoCobranca, hoje: NaiveDate, ambiente: Option<Environment>) -> String {
    let data = |dia: NaiveDate| dia.format("%d/%m/%Y").to_string();
    let valor = cobranca.valor_nominal;
    let extenso = por_extenso(valor)
        .map(|extenso| format!(" ({extenso})"))
        .unwrap_or_default();
    let pagador = &cobranca.pagador;
    let mut linhas = vec![
        ("Ambiente", descrever_ambiente(ambiente)),
        ("Seu número", cobranca.seu_numero.clone()),
        ("Valor", format!("{}{extenso}", output::brl(valor))),
        ("Vencimento", data(cobranca.data_vencimento)),
        (
            "Pagador",
            format!("{} ({})", pagador.nome, pagador.cpf_cnpj.formatado()),
        ),
        ("Endereço", endereco(pagador)),
    ];
    if let Some(contato) = contato(pagador) {
        linhas.push(("Contato", contato));
    }
    let mut avisos = Vec::new();
    encargos(cobranca, hoje, &mut linhas, &mut avisos);
    let dias = cobranca.num_dias_agenda;
    let cancelamento = cobranca
        .data_vencimento
        .checked_add_days(Days::new(dias.into()))
        .unwrap_or(cobranca.data_vencimento);
    linhas.push((
        "Cancelamento",
        match dias {
            0 => "no vencimento, se não for paga: pagamentos atrasados não são aceitos".to_owned(),
            1 => format!(
                "{}, 1 dia após o vencimento, se não for paga",
                data(cancelamento)
            ),
            _ => format!(
                "{}, {dias} dias após o vencimento, se não for paga",
                data(cancelamento)
            ),
        },
    ));
    if dias == 0 && (cobranca.multa.is_some() || cobranca.mora.is_some()) {
        avisos.push(
            "multa e juros não chegam a valer: sem --dias-agenda (numDiasAgenda), a cobrança é cancelada no vencimento".to_owned(),
        );
    }
    linhas.push(("Recebimento", formas(&cobranca.formas_recebimento)));
    if !cobranca.mensagem.is_empty() {
        linhas.push(("Mensagem", cobranca.mensagem.join(" / ")));
    }
    if let Some(beneficiario) = &cobranca.beneficiario_final {
        linhas.push((
            "Beneficiário final",
            format!(
                "{} ({})",
                beneficiario.nome,
                beneficiario.cpf_cnpj.formatado()
            ),
        ));
    }
    if let Some(nota) = &cobranca.nota_fiscal {
        linhas.push((
            "Nota fiscal",
            format!("{}, série {}", nota.numero, nota.serie),
        ));
    }
    if cobranca.data_vencimento == hoje {
        avisos.push(
            "cobranças que vencem hoje só podem ser emitidas até as 19h59 (horário de Brasília)"
                .to_owned(),
        );
    }

    let mut texto = String::new();
    if ambiente.is_some_and(Environment::is_production) {
        texto.push_str("*** PRODUÇÃO: a cobrança vai para o cliente de verdade ***\n");
    }
    texto.push_str("Cobrança a emitir");
    for linha in output::key_values_left(&linhas).lines() {
        let _ = write!(texto, "\n  {linha}");
    }
    for aviso in avisos {
        let _ = write!(texto, "\naviso: {aviso}");
    }
    texto
}

/// Discount, fine and interest, and a warning if the discount can no longer
/// be had.
fn encargos(
    cobranca: &EmissaoCobranca,
    hoje: NaiveDate,
    linhas: &mut Vec<(&str, String)>,
    avisos: &mut Vec<String>,
) {
    if let Some(desconto) = &cobranca.desconto {
        let ate = cobranca
            .data_vencimento
            .checked_sub_days(Days::new(desconto.quantidade_dias().into()))
            .unwrap_or(cobranca.data_vencimento);
        let ate_br = ate.format("%d/%m/%Y");
        let quanto = match desconto {
            Desconto::Percentual { taxa, .. } => percentual(*taxa),
            Desconto::ValorFixo { valor, .. } => output::brl(*valor),
        };
        linhas.push(("Desconto", format!("{quanto} para pagamentos até {ate_br}")));
        if ate < hoje {
            avisos.push(format!("o desconto valeria até {ate_br}, que já passou"));
        }
    }
    if let Some(multa) = &cobranca.multa {
        linhas.push((
            "Multa",
            match multa {
                Multa::Percentual { taxa } => percentual(*taxa),
                Multa::ValorFixo { valor } => output::brl(*valor),
            },
        ));
    }
    if let Some(mora) = &cobranca.mora {
        linhas.push((
            "Juros",
            match mora {
                Mora::TaxaMensal { taxa } => format!("{} ao mês", percentual(*taxa)),
                Mora::ValorDia { valor } => format!("{} por dia", output::brl(*valor)),
            },
        ));
    }
}

/// `Avenida Brasil, 1200, sala 3 - Centro - Belo Horizonte/MG - CEP 30110-000`.
fn endereco(pagador: &Pagador) -> String {
    let mut texto = pagador.endereco.clone();
    for parte in [&pagador.numero, &pagador.complemento]
        .into_iter()
        .flatten()
    {
        let _ = write!(texto, ", {parte}");
    }
    if let Some(bairro) = &pagador.bairro {
        let _ = write!(texto, " - {bairro}");
    }
    let cep = &pagador.cep;
    let cep = if cep.len() == 8 {
        format!("{}-{}", &cep[..5], &cep[5..])
    } else {
        cep.clone()
    };
    let _ = write!(texto, " - {}/{} - CEP {cep}", pagador.cidade, pagador.uf);
    texto
}

/// `financeiro@empresa.example · (31) 99999-9999`.
fn contato(pagador: &Pagador) -> Option<String> {
    let telefone = match (&pagador.ddd, &pagador.telefone) {
        (Some(ddd), Some(numero)) => {
            let meio = numero.len().saturating_sub(4);
            Some(format!("({ddd}) {}-{}", &numero[..meio], &numero[meio..]))
        }
        _ => None,
    };
    let partes: Vec<String> = [pagador.email.clone(), telefone]
        .into_iter()
        .flatten()
        .collect();
    (!partes.is_empty()).then(|| partes.join(" · "))
}

fn formas(formas: &[FormaRecebimento]) -> String {
    let tem = |forma| formas.contains(&forma);
    match (
        formas.is_empty(),
        tem(FormaRecebimento::Boleto),
        tem(FormaRecebimento::Pix),
    ) {
        (true, _, _) => "boleto e Pix (se a conta tiver chave Pix)".to_owned(),
        (_, true, true) => "boleto e Pix".to_owned(),
        (_, true, false) => "só boleto".to_owned(),
        (_, false, true) => "só Pix".to_owned(),
        _ => "nenhuma forma de pagamento".to_owned(),
    }
}

fn render_solicitacao(codigo: Option<&str>, cobranca: &EmissaoCobranca) -> String {
    match codigo {
        Some(codigo) => format!(
            "Cobrança solicitada: a emissão termina em instantes.\nCódigo  {codigo}\n\nAcompanhe com: inter-pj cobranca consultar {}",
            argumento(codigo)
        ),
        // The API documents the code as always present.
        None => format!(
            "Cobrança solicitada, mas a API não informou o código.\n\nProcure-a com: {}",
            procurar(cobranca)
        ),
    }
}

/// Queries every `intervalo` until the charge leaves `EM_PROCESSAMENTO` or
/// `timeout` passes. Right after the request, the charge may not be found
/// yet: that counts as being issued.
async fn aguardar(
    client: &InterClient,
    codigo: &str,
    timeout: Duration,
    intervalo: Duration,
) -> Result<(CobrancaDetalhada, bool), CliError> {
    let prazo = Instant::now() + timeout;
    let mut anunciado = false;
    loop {
        let ultima = match client.cobranca().consultar(codigo).await {
            Ok(cobranca)
                if cobranca.cobranca.situacao != Some(SituacaoCobranca::EmProcessamento) =>
            {
                return Ok((cobranca, true));
            }
            Ok(cobranca) => Some(cobranca),
            Err(InterError::Api(api)) if api.status == 404 => None,
            Err(err) => return Err(err.into()),
        };
        let agora = Instant::now();
        if agora >= prazo {
            return ultima
                .map(|cobranca| (cobranca, false))
                .ok_or(CliError::TempoEsgotado {
                    oque: "a cobrança",
                    status: "em processamento".to_owned(),
                    segundos: timeout.as_secs(),
                });
        }
        if !anunciado {
            eprintln!("aguardando a emissão...");
            anunciado = true;
        }
        tokio::time::sleep(intervalo.min(prazo - agora)).await;
    }
}

pub(super) fn modelo() -> Result<(), CliError> {
    output::print_raw(&arquivo::modelo_cobranca(hoje()))
}

#[cfg(test)]
mod tests {
    use clap::{CommandFactory, FromArgMatches};
    use serde_json::json;
    use wiremock::matchers::{any, method, path};
    use wiremock::{Mock, ResponseTemplate};

    use super::super::testes::{self, Cenario};
    use super::*;
    use crate::cli::{Cli, CobrancaCommand, Command};
    use crate::confirmacao::testes::TerminalFalso;

    const CODIGO: &str = "0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d";

    /// The options of a complete charge, due far ahead.
    const OPCOES: [&str; 36] = [
        "--seu-numero",
        "NF-123",
        "--valor",
        "150,00",
        "--vencimento",
        "2099-10-20",
        "--pagador-documento",
        "12.345.678/0001-95",
        "--pagador-nome",
        "Cliente Exemplo Ltda",
        "--pagador-endereco",
        "Avenida Brasil",
        "--pagador-numero",
        "1200",
        "--pagador-bairro",
        "Centro",
        "--pagador-cidade",
        "Belo Horizonte",
        "--pagador-uf",
        "mg",
        "--pagador-cep",
        "30110-000",
        "--pagador-telefone",
        "+55 (31) 99999-9999",
        "--desconto",
        "2,5%",
        "--desconto-dias",
        "5",
        "--multa",
        "4,00",
        "--juros",
        "1%",
        "--dias-agenda",
        "30",
        "--receber-com",
        "boleto,pix",
    ];

    fn args(extra: &[&str]) -> CobrancaEmitirArgs {
        let mut full = vec!["inter-pj", "cobranca", "emitir"];
        full.extend_from_slice(extra);
        let matches = Cli::command().try_get_matches_from(&full).unwrap();
        match Cli::from_arg_matches(&matches).unwrap().command {
            Command::Cobranca(CobrancaCommand::Emitir(args)) => *args,
            outro => panic!("{outro:?}"),
        }
    }

    fn dia(ano: i32, mes: u32, dia: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(ano, mes, dia).unwrap()
    }

    #[test]
    fn options_become_the_charge() {
        let cobranca = das_opcoes(&args(&OPCOES)).unwrap();
        assert_eq!(
            serde_json::to_value(&cobranca).unwrap(),
            json!({
                "seuNumero": "NF-123",
                "valorNominal": 150,
                "dataVencimento": "2099-10-20",
                "numDiasAgenda": 30,
                "pagador": {
                    "cpfCnpj": "12345678000195",
                    "tipoPessoa": "JURIDICA",
                    "nome": "Cliente Exemplo Ltda",
                    "endereco": "Avenida Brasil",
                    "numero": "1200",
                    "bairro": "Centro",
                    "cidade": "Belo Horizonte",
                    "uf": "MG",
                    "cep": "30110000",
                    "ddd": "31",
                    "telefone": "999999999"
                },
                "desconto": {"codigo": "PERCENTUALDATAINFORMADA", "quantidadeDias": 5, "taxa": 2.5},
                "multa": {"codigo": "VALORFIXO", "valor": 4},
                "mora": {"codigo": "TAXAMENSAL", "taxa": 1},
                "formasRecebimento": ["BOLETO", "PIX"]
            })
        );
    }

    #[test]
    fn errors_name_the_option() {
        let com = |troca: (&str, &str)| {
            let mut opcoes = OPCOES.to_vec();
            let i = opcoes.iter().position(|o| *o == troca.0).unwrap();
            opcoes[i + 1] = troca.1;
            das_opcoes(&args(&opcoes)).unwrap_err().to_string()
        };
        assert!(com(("--valor", "2,00")).starts_with("--valor: "));
        assert!(com(("--pagador-numero", "12345678901")).starts_with("--pagador-numero: "));
        assert!(com(("--desconto", "101%")).starts_with("--desconto: "));
        assert!(com(("--juros", "150,00")).starts_with("--juros: "));
        assert!(com(("--dias-agenda", "61")).starts_with("--dias-agenda: "));
        assert_eq!(opcao("pagador.email"), "--pagador-email");
        assert_eq!(opcao("mensagem.linha3"), "--mensagem");
        // clap refuses what can be checked while parsing.
        for (opcao, valor) in [
            ("--pagador-cep", "3011"),
            ("--pagador-uf", "XX"),
            ("--pagador-telefone", "9999-9999"),
            ("--desconto", "2x%"),
        ] {
            let mut opcoes = OPCOES.to_vec();
            let i = opcoes.iter().position(|o| *o == opcao).unwrap();
            opcoes[i + 1] = valor;
            let mut full = vec!["inter-pj", "cobranca", "emitir"];
            full.extend_from_slice(&opcoes);
            assert!(
                Cli::command().try_get_matches_from(&full).is_err(),
                "{opcao} {valor}"
            );
        }
    }

    #[test]
    fn summary_shows_what_goes_to_the_client() {
        let mut cobranca = das_opcoes(&args(&OPCOES)).unwrap();
        cobranca.data_vencimento = dia(2026, 10, 20);
        cobranca.pagador.email = Some("financeiro@empresa.example".to_owned());
        cobranca.mensagem = vec!["Referente à NF 123".to_owned(), "Obrigado".to_owned()];
        assert_eq!(
            resumo(&cobranca, dia(2026, 9, 23), Some(Environment::Sandbox)),
            "\
Cobrança a emitir
  Ambiente      sandbox (dados fictícios)
  Seu número    NF-123
  Valor         R$ 150,00 (cento e cinquenta reais)
  Vencimento    20/10/2026
  Pagador       Cliente Exemplo Ltda (12.345.678/0001-95)
  Endereço      Avenida Brasil, 1200 - Centro - Belo Horizonte/MG - CEP 30110-000
  Contato       financeiro@empresa.example · (31) 99999-9999
  Desconto      2,5% para pagamentos até 15/10/2026
  Multa         R$ 4,00
  Juros         1% ao mês
  Cancelamento  19/11/2026, 30 dias após o vencimento, se não for paga
  Recebimento   boleto e Pix
  Mensagem      Referente à NF 123 / Obrigado"
        );
    }

    #[test]
    fn warnings_point_to_what_would_not_work() {
        let mut cobranca = das_opcoes(&args(&OPCOES)).unwrap();
        cobranca.data_vencimento = dia(2026, 9, 25);
        cobranca.num_dias_agenda = 0;
        let texto = resumo(&cobranca, dia(2026, 9, 25), Some(Environment::Production));
        assert!(texto.starts_with("*** PRODUÇÃO"), "{texto}");
        for aviso in [
            "aviso: o desconto valeria até 20/09/2026, que já passou",
            "aviso: multa e juros não chegam a valer",
            "aviso: cobranças que vencem hoje só podem ser emitidas até as 19h59",
        ] {
            assert!(texto.contains(aviso), "{aviso}\n{texto}");
        }
        assert!(
            texto.contains(
                "Cancelamento  no vencimento, se não for paga: pagamentos atrasados não são aceitos"
            ),
            "{texto}"
        );
    }

    // --- the command against a mock API -------------------------------------

    async fn cenario(extra: &[&str]) -> (Cenario, CobrancaEmitirArgs) {
        let mut todos = vec!["emitir"];
        todos.extend_from_slice(&OPCOES);
        todos.extend_from_slice(extra);
        match testes::cenario(&todos).await {
            (cenario, CobrancaCommand::Emitir(args)) => (cenario, *args),
            (_, outro) => panic!("{outro:?}"),
        }
    }

    #[tokio::test]
    async fn declined_confirmation_sends_nothing() {
        let (cenario, args) = cenario(&[]).await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&cenario.server)
            .await;
        let mut terminal = TerminalFalso::respondendo("n\n");
        let err = emitir(&cenario.context, &args, &mut terminal)
            .await
            .unwrap_err();
        assert!(matches!(err, CliError::Cancelado), "{err}");
        assert_eq!(terminal.perguntas, ["Emitir a cobrança? [s/N] "]);
    }

    #[tokio::test]
    async fn waits_while_the_charge_is_issued() {
        let (cenario, args) = cenario(&["--aguardar", "--timeout", "10s"]).await;
        Mock::given(method("POST"))
            .and(path("/cobranca/v3/cobrancas"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"codigoSolicitacao": CODIGO})),
            )
            .expect(1)
            .mount(&cenario.server)
            .await;
        let consulta = format!("/cobranca/v3/cobrancas/{CODIGO}");
        // Not found yet, then being issued, then issued.
        Mock::given(method("GET"))
            .and(path(consulta.clone()))
            .respond_with(ResponseTemplate::new(404))
            .up_to_n_times(1)
            .expect(1)
            .mount(&cenario.server)
            .await;
        Mock::given(method("GET"))
            .and(path(consulta.clone()))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "cobranca": {"codigoSolicitacao": CODIGO, "situacao": "EM_PROCESSAMENTO"}
            })))
            .up_to_n_times(1)
            .expect(1)
            .mount(&cenario.server)
            .await;
        Mock::given(method("GET"))
            .and(path(consulta))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "cobranca": {"codigoSolicitacao": CODIGO, "situacao": "A_RECEBER"}
            })))
            .expect(1)
            .mount(&cenario.server)
            .await;
        let mut terminal = TerminalFalso::respondendo("s\n");
        emitir_com(
            &cenario.context,
            &args,
            &mut terminal,
            Duration::from_millis(10),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn a_failed_issue_is_an_error() {
        // No QR Code to save, and exit 5 rather than a complaint about it.
        let dir = tempfile::tempdir().unwrap();
        let png = dir.path().join("pix.png");
        let png_arg = png.display().to_string();
        let (cenario, args) = cenario(&["--aguardar", "--sim", "--qrcode-png", &png_arg]).await;
        Mock::given(method("POST"))
            .and(path("/cobranca/v3/cobrancas"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"codigoSolicitacao": CODIGO})),
            )
            .mount(&cenario.server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/cobranca/v3/cobrancas/{CODIGO}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "cobranca": {"codigoSolicitacao": CODIGO, "situacao": "FALHA_EMISSAO"}
            })))
            .mount(&cenario.server)
            .await;
        let err = emitir(&cenario.context, &args, &mut TerminalFalso::default())
            .await
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "a cobrança não foi emitida: falha na emissão"
        );
        assert_eq!(err.exit_code(), 5);
        assert!(!png.exists());
    }

    #[tokio::test]
    async fn waiting_too_long_is_a_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let png = dir.path().join("pix.png");
        let png_arg = png.display().to_string();
        let (cenario, args) = cenario(&[
            "--aguardar",
            "--sim",
            "--timeout",
            "1s",
            "--qrcode-png",
            &png_arg,
        ])
        .await;
        Mock::given(method("POST"))
            .and(path("/cobranca/v3/cobrancas"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"codigoSolicitacao": CODIGO})),
            )
            .mount(&cenario.server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/cobranca/v3/cobrancas/{CODIGO}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "cobranca": {"codigoSolicitacao": CODIGO, "situacao": "EM_PROCESSAMENTO"}
            })))
            .mount(&cenario.server)
            .await;
        let err = emitir_com(
            &cenario.context,
            &args,
            &mut TerminalFalso::default(),
            Duration::from_millis(300),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, CliError::TempoEsgotado { .. }), "{err}");
        assert_eq!(err.exit_code(), 8);
        assert!(!png.exists());
    }

    #[test]
    fn hints_can_be_pasted_in_a_shell() {
        assert_eq!(argumento("NF-123"), "NF-123");
        assert_eq!(argumento("NF 123"), "'NF 123'");
        assert_eq!(argumento("D'Ávila"), r"'D'\''Ávila'");
        assert_eq!(argumento("$(x)"), "'$(x)'");
        let mut cobranca = das_opcoes(&args(&OPCOES)).unwrap();
        cobranca.seu_numero = "NF 123".to_owned();
        assert_eq!(
            render_solicitacao(None, &cobranca),
            "Cobrança solicitada, mas a API não informou o código.\n\nProcure-a com: inter-pj cobranca listar --filtrar-por emissao --seu-numero 'NF 123'"
        );
    }
}
