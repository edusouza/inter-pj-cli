//! `inter-pj cobranca ...`: charges (boleto with Pix) of the Cobrança API.

mod alterar;
mod consultar;
mod emitir;
mod listar;

use std::fmt::Write as _;

use inter_pj::boleto::CodigoBarras;
use inter_pj::cobranca::{
    CobrancaDetalhada, EncargoCobranca, OrigemRecebimento, SituacaoCobranca, TipoCobranca,
};
use inter_pj::documento::Documento;
use rust_decimal::Decimal;

use super::Context;
use crate::cli::CobrancaCommand;
use crate::confirmacao::Stdio;
use crate::error::CliError;
use crate::output::{self, data_br};

pub(super) async fn run(context: &Context, command: CobrancaCommand) -> Result<(), CliError> {
    match command {
        CobrancaCommand::Emitir(args) => emitir::emitir(context, &args, &mut Stdio).await,
        CobrancaCommand::Modelo(_) => emitir::modelo(),
        CobrancaCommand::Listar(args) => listar::listar(context, &args).await,
        CobrancaCommand::Sumario(args) => listar::sumario(context, &args).await,
        CobrancaCommand::Consultar(args) => consultar::consultar(context, &args).await,
        CobrancaCommand::Pdf(args) => consultar::pdf(context, &args).await,
        CobrancaCommand::Cancelar(args) => alterar::cancelar(context, &args, &mut Stdio).await,
        CobrancaCommand::Editar(args) => alterar::editar(context, &args, &mut Stdio).await,
        CobrancaCommand::Edicao(args) => alterar::edicao(context, &args).await,
        CobrancaCommand::Pagar(args) => alterar::pagar(context, &args).await,
    }
}

/// `texto` as one argument of a shell command: quoted when it has to be.
pub(super) fn argumento(texto: &str) -> String {
    let simples = |c: char| c.is_ascii_alphanumeric() || "-_./:,+=@%".contains(c);
    if !texto.is_empty() && texto.chars().all(simples) {
        texto.to_owned()
    } else {
        format!("'{}'", texto.replace('\'', r"'\''"))
    }
}

/// A situation in words: `A_RECEBER` -> `a receber`.
pub(crate) fn descrever_situacao(situacao: &SituacaoCobranca) -> String {
    match situacao {
        SituacaoCobranca::Recebido => "recebida",
        SituacaoCobranca::AReceber => "a receber",
        SituacaoCobranca::MarcadoRecebido => "marcada como recebida",
        SituacaoCobranca::Atrasado => "atrasada",
        SituacaoCobranca::Cancelado => "cancelada",
        SituacaoCobranca::Expirado => "expirada (cancelada sem pagamento)",
        SituacaoCobranca::FalhaEmissao => "falha na emissão",
        SituacaoCobranca::EmProcessamento => "em processamento (sendo emitida)",
        SituacaoCobranca::Protesto => "em protesto",
        outra => outra.as_str(),
    }
    .to_owned()
}

fn descrever_tipo(tipo: &TipoCobranca) -> &str {
    match tipo {
        TipoCobranca::Simples => "simples",
        TipoCobranca::Parcelado => "parcelada",
        TipoCobranca::Recorrente => "recorrente",
        outro => outro.as_str(),
    }
}

/// `2,5%`.
pub(crate) fn percentual(taxa: Decimal) -> String {
    format!("{}%", taxa.normalize()).replace('.', ",")
}

/// A discount, fine or interest in words: `2% até 5 dias antes do
/// vencimento`, `R$ 4,00`, `1% ao mês`.
fn descrever_encargo(encargo: &EncargoCobranca) -> String {
    let quanto = match (encargo.taxa, encargo.valor) {
        (Some(taxa), _) => percentual(taxa),
        (None, Some(valor)) => output::brl(valor),
        (None, None) => String::new(),
    };
    let codigo = encargo.codigo.as_deref().unwrap_or_default();
    let prazo = match encargo.quantidade_dias {
        Some(0) => " até o vencimento".to_owned(),
        Some(1) => " até 1 dia antes do vencimento".to_owned(),
        Some(dias) => format!(" até {dias} dias antes do vencimento"),
        None => String::new(),
    };
    match codigo {
        "PERCENTUALDATAINFORMADA" | "VALORFIXODATAINFORMADA" => format!("{quanto}{prazo}"),
        "PERCENTUAL" | "VALORFIXO" => quanto,
        "TAXAMENSAL" => format!("{quanto} ao mês"),
        "VALORDIA" => format!("{quanto} por dia"),
        outro => format!("{quanto} ({outro})").trim().to_owned(),
    }
}

/// A CPF or CNPJ with punctuation, or as received.
fn documento(texto: &str) -> String {
    Documento::parse(texto).map_or_else(|_| texto.to_owned(), |doc| doc.formatado())
}

fn data(texto: &str) -> String {
    data_br(texto)
}

/// A charge in detail: the charge, then its boleto and its Pix.
pub(crate) fn render_cobranca(detalhe: &CobrancaDetalhada) -> String {
    let cobranca = &detalhe.cobranca;
    let mut linhas = Vec::new();
    if let Some(situacao) = &cobranca.situacao {
        linhas.push(("Situação", descrever_situacao(situacao)));
    }
    if let Some(valor) = cobranca.valor_nominal {
        linhas.push(("Valor", output::brl(valor)));
    }
    if let Some(vencimento) = &cobranca.data_vencimento {
        linhas.push(("Vencimento", data(vencimento)));
    }
    if let Some(recebido) = cobranca.valor_total_recebido {
        let origem = match &cobranca.origem_recebimento {
            Some(OrigemRecebimento::Pix) => " por Pix",
            Some(OrigemRecebimento::Boleto) => " pelo boleto",
            _ => "",
        };
        let quando = cobranca
            .data_situacao
            .as_deref()
            .map(|dia| format!(" em {}", data(dia)))
            .unwrap_or_default();
        linhas.push((
            "Recebido",
            format!("{}{origem}{quando}", output::brl(recebido)),
        ));
    }
    if let Some(motivo) = &cobranca.motivo_cancelamento {
        linhas.push(("Motivo", motivo.clone()));
    }
    if let Some(pagador) = &cobranca.pagador {
        let nome = pagador.nome.as_deref().unwrap_or("?");
        let texto = match pagador.cpf_cnpj.as_deref() {
            Some(doc) => format!("{nome} ({})", documento(doc)),
            None => nome.to_owned(),
        };
        linhas.push(("Pagador", texto));
    }
    if let Some(emissao) = &cobranca.data_emissao {
        linhas.push(("Emitida em", data(emissao)));
    }
    if let Some(tipo) = &cobranca.tipo_cobranca {
        linhas.push(("Tipo", descrever_tipo(tipo).to_owned()));
    }
    for desconto in &cobranca.descontos {
        linhas.push(("Desconto", descrever_encargo(desconto)));
    }
    if let Some(multa) = &cobranca.multa {
        linhas.push(("Multa", descrever_encargo(multa)));
    }
    if let Some(mora) = &cobranca.mora {
        linhas.push(("Juros", descrever_encargo(mora)));
    }
    if let Some(nota) = &detalhe.nota_fiscal {
        let numero = nota.numero.map(|n| n.to_string()).unwrap_or_default();
        let serie = nota
            .serie
            .map(|s| format!(", série {s}"))
            .unwrap_or_default();
        linhas.push(("Nota fiscal", format!("{numero}{serie}")));
    }
    if let Some(codigo) = &cobranca.codigo_solicitacao {
        linhas.push(("Código", codigo.clone()));
    }
    let titulo = cobranca.seu_numero.as_deref().map_or_else(
        || "Cobrança".to_owned(),
        |numero| format!("Cobrança {numero}"),
    );
    let mut texto = secao(&titulo, &linhas);
    texto.push_str(&boleto_e_pix(detalhe));
    if cobranca.situacao == Some(SituacaoCobranca::EmProcessamento) && detalhe.boleto.is_none() {
        texto.push_str(
            "\n\nA cobrança ainda está sendo emitida: o boleto e o Pix aparecem em instantes.",
        );
    }
    texto
}

/// The sections of the boleto and of the Pix, when the charge has them.
fn boleto_e_pix(detalhe: &CobrancaDetalhada) -> String {
    let mut texto = String::new();
    if let Some(boleto) = &detalhe.boleto {
        let mut linhas = Vec::new();
        if let Some(numero) = &boleto.nosso_numero {
            linhas.push(("Nosso número", numero.clone()));
        }
        let codigo = boleto
            .linha_digitavel
            .as_deref()
            .or(boleto.codigo_barras.as_deref())
            .and_then(|texto| CodigoBarras::parse(texto).ok());
        if let Some(codigo) = codigo {
            linhas.push(("Linha digitável", codigo.linha_formatada()));
            linhas.push(("Código de barras", codigo.codigo_barras().to_owned()));
        } else {
            // Not a valid code: as the API sent it.
            if let Some(linha) = &boleto.linha_digitavel {
                linhas.push(("Linha digitável", linha.clone()));
            }
            if let Some(barras) = &boleto.codigo_barras {
                linhas.push(("Código de barras", barras.clone()));
            }
        }
        let _ = write!(texto, "\n\n{}", secao("Boleto", &linhas));
    }
    if let Some(pix) = &detalhe.pix {
        let mut linhas = Vec::new();
        if let Some(copia_e_cola) = &pix.pix_copia_e_cola {
            linhas.push(("Copia e cola", copia_e_cola.clone()));
        }
        if let Some(txid) = &pix.txid {
            linhas.push(("txid", txid.clone()));
        }
        let _ = write!(texto, "\n\n{}", secao("Pix", &linhas));
    }
    texto
}

/// A title and its lines, indented.
fn secao(titulo: &str, linhas: &[(&str, String)]) -> String {
    let mut texto = titulo.to_owned();
    for linha in output::key_values_left(linhas).lines() {
        let _ = write!(texto, "\n  {linha}");
    }
    texto
}

/// `inter-pj cobranca ...` against a mock API, with a sandbox profile and
/// a token for reading and writing charges.
#[cfg(test)]
mod testes {
    use std::collections::HashMap;
    use std::fs;

    use clap::{CommandFactory, FromArgMatches};
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use crate::cli::{Cli, CobrancaCommand, Command};
    use crate::commands::{Context, Env};

    struct EnvFalso(HashMap<&'static str, String>);

    impl Env for EnvFalso {
        fn var(&self, name: &str) -> Option<String> {
            self.0.get(name).cloned()
        }
    }

    pub(super) struct Cenario {
        pub(super) server: MockServer,
        pub(super) context: Context,
        _dir: tempfile::TempDir,
    }

    /// The scenario and the command of `inter-pj cobranca <args>`.
    pub(super) async fn cenario(args: &[&str]) -> (Cenario, CobrancaCommand) {
        let server = MockServer::start().await;
        let dir = tempfile::tempdir().unwrap();
        let rcgen::CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed(vec!["cliente.teste".to_owned()]).unwrap();
        let certificado = dir.path().join("certificado.crt");
        let chave = dir.path().join("chave.key");
        fs::write(&certificado, cert.pem()).unwrap();
        fs::write(&chave, signing_key.serialize_pem()).unwrap();
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
        let mut full = vec!["inter-pj", "--config", &config, "cobranca"];
        full.extend_from_slice(args);
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
        let Command::Cobranca(comando) = cli.command else {
            unreachable!("inter-pj cobranca");
        };
        Mock::given(method("POST"))
            .and(path("/oauth/v2/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "tok",
                "expires_in": 3600,
                "scope": "boleto-cobranca.write boleto-cobranca.read"
            })))
            .mount(&server)
            .await;
        (
            Cenario {
                server,
                context,
                _dir: dir,
            },
            comando,
        )
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn detalhe(json: serde_json::Value) -> CobrancaDetalhada {
        serde_json::from_value(json).unwrap()
    }

    /// A synthetic boleto of Inter (077) and the example of the Banco
    /// Central's manual as the Pix.
    pub(crate) const LINHA: &str = "07790001161234567800212345678903116050000015000";
    pub(crate) const COPIA_E_COLA: &str = "00020126580014br.gov.bcb.pix0136123e4567-e12b-12d1-a456-4266554400005204000053039865802BR5913Fulano de Tal6008BRASILIA62070503***63041D3D";

    #[test]
    fn renders_a_charge_to_receive() {
        let texto = render_cobranca(&detalhe(json!({
            "cobranca": {
                "codigoSolicitacao": "0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d",
                "seuNumero": "NF-123",
                "dataEmissao": "2026-09-23",
                "dataVencimento": "2026-10-20",
                "valorNominal": 150,
                "tipoCobranca": "SIMPLES",
                "situacao": "A_RECEBER",
                "descontos": [{"codigo": "PERCENTUALDATAINFORMADA", "quantidadeDias": 5, "taxa": 2.5}],
                "multa": {"codigo": "VALORFIXO", "valor": 4},
                "mora": {"codigo": "TAXAMENSAL", "taxa": 1},
                "pagador": {"nome": "Cliente Exemplo Ltda", "cpfCnpj": "12345678000195"}
            },
            "boleto": {"nossoNumero": "12345678", "linhaDigitavel": LINHA},
            "pix": {"txid": "COBRANCAEXEMPLO00000000001", "pixCopiaECola": COPIA_E_COLA},
            "notaFiscal": {"numero": 12345, "serie": 1}
        })));
        assert_eq!(
            texto,
            format!(
                "Cobrança NF-123
  Situação     a receber
  Valor        R$ 150,00
  Vencimento   20/10/2026
  Pagador      Cliente Exemplo Ltda (12.345.678/0001-95)
  Emitida em   23/09/2026
  Tipo         simples
  Desconto     2,5% até 5 dias antes do vencimento
  Multa        R$ 4,00
  Juros        1% ao mês
  Nota fiscal  12345, série 1
  Código       0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d

Boleto
  Nosso número      12345678
  Linha digitável   {}
  Código de barras  07791160500000150000001112345678001234567890

Pix
  Copia e cola  {COPIA_E_COLA}
  txid          COBRANCAEXEMPLO00000000001",
                CodigoBarras::parse(LINHA).unwrap().linha_formatada()
            )
        );
        // What is not a valid code is shown as the API sent it.
        let texto = render_cobranca(&detalhe(json!({
            "boleto": {"linhaDigitavel": "123", "codigoBarras": "456"}
        })));
        assert!(
            texto.ends_with("Boleto\n  Linha digitável   123\n  Código de barras  456"),
            "{texto}"
        );
    }

    #[test]
    fn renders_received_cancelled_and_processing_charges() {
        let recebida = render_cobranca(&detalhe(json!({
            "cobranca": {
                "situacao": "RECEBIDO",
                "valorNominal": 150,
                "valorTotalRecebido": "147.00",
                "origemRecebimento": "PIX",
                "dataSituacao": "2026-10-15"
            }
        })));
        assert!(
            recebida.contains("Recebido  R$ 147,00 por Pix em 15/10/2026"),
            "{recebida}"
        );
        let cancelada = render_cobranca(&detalhe(json!({
            "cobranca": {"situacao": "CANCELADO", "motivoCancelamento": "Pedido cancelado"}
        })));
        assert!(
            cancelada.contains("Motivo    Pedido cancelado"),
            "{cancelada}"
        );
        let emitindo = render_cobranca(&detalhe(json!({
            "cobranca": {"situacao": "EM_PROCESSAMENTO"}
        })));
        assert!(
            emitindo.ends_with("o boleto e o Pix aparecem em instantes."),
            "{emitindo}"
        );
    }

    #[test]
    fn every_documented_situation_is_described() {
        for situacao in SituacaoCobranca::DOCUMENTADOS {
            assert_ne!(descrever_situacao(situacao), situacao.as_str());
        }
        for tipo in TipoCobranca::DOCUMENTADOS {
            assert_ne!(descrever_tipo(tipo), tipo.as_str());
        }
    }

    #[test]
    fn charges_are_described_by_their_code() {
        let encargo = |json: serde_json::Value| {
            descrever_encargo(&serde_json::from_value::<EncargoCobranca>(json).unwrap())
        };
        assert_eq!(
            encargo(json!({"codigo": "VALORFIXODATAINFORMADA", "quantidadeDias": 0, "valor": 10})),
            "R$ 10,00 até o vencimento"
        );
        assert_eq!(
            encargo(json!({"codigo": "PERCENTUALDATAINFORMADA", "quantidadeDias": 1, "taxa": 3})),
            "3% até 1 dia antes do vencimento"
        );
        assert_eq!(encargo(json!({"codigo": "PERCENTUAL", "taxa": 2})), "2%");
        assert_eq!(
            encargo(json!({"codigo": "VALORDIA", "valor": 0.33})),
            "R$ 0,33 por dia"
        );
        assert_eq!(
            encargo(json!({"codigo": "NOVO", "taxa": 1.5})),
            "1,5% (NOVO)"
        );
    }
}
