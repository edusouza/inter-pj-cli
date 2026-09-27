//! `inter-pj pix consultar`

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use inter_pj::InterClient;
use inter_pj::banking::{ConsultaPix, StatusPix};

use crate::cli::{Formato, PixConsultarArgs};
use crate::commands::Context;
use crate::error::CliError;
use crate::output::{self, data_hora_br};

/// Between two queries with `--aguardar`: 10 per minute, within the rate
/// limit of both environments (20/min in production, 10/min in sandbox).
const INTERVALO: Duration = Duration::from_secs(6);

pub(super) async fn run(context: &Context, args: &PixConsultarArgs) -> Result<(), CliError> {
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let codigo = args.codigo.trim();
    if !args.aguardar {
        let consulta = client.banking().consultar_pix(codigo).await?;
        return mostrar(context, &consulta);
    }

    let (consulta, desfecho) = aguardar(&client, codigo, args.timeout, INTERVALO).await?;
    mostrar(context, &consulta)?;
    let status = status(&consulta).map_or_else(|| "sem status".to_owned(), descrever);
    match desfecho {
        Some(Desfecho::Concluido) => Ok(()),
        Some(Desfecho::NaoPago) => Err(CliError::PixNaoPago { status }),
        None => Err(CliError::TempoEsgotado {
            oque: "o Pix",
            status,
            segundos: args.timeout.as_secs(),
        }),
    }
}

/// How waiting ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Desfecho {
    /// Paid, or scheduled (nothing changes until the day).
    Concluido,
    /// Ended without paying.
    NaoPago,
}

fn desfecho(status: &StatusPix) -> Option<Desfecho> {
    match status {
        StatusPix::Pago | StatusPix::Agendado => Some(Desfecho::Concluido),
        status if status.is_final() => Some(Desfecho::NaoPago),
        _ => None,
    }
}

fn status(consulta: &ConsultaPix) -> Option<&StatusPix> {
    consulta.transacao_pix.as_ref()?.status.as_ref()
}

/// Queries every `intervalo` until the payment reaches an outcome or
/// `timeout` passes; the last query happens at the deadline. Status changes
/// are reported on stderr.
async fn aguardar(
    client: &InterClient,
    codigo: &str,
    timeout: Duration,
    intervalo: Duration,
) -> Result<(ConsultaPix, Option<Desfecho>), CliError> {
    let prazo = Instant::now() + timeout;
    let mut anterior: Option<StatusPix> = None;
    loop {
        let consulta = client.banking().consultar_pix(codigo).await?;
        let atual = status(&consulta).cloned();
        if let Some(desfecho) = atual.as_ref().and_then(desfecho) {
            return Ok((consulta, Some(desfecho)));
        }
        let agora = Instant::now();
        if agora >= prazo {
            return Ok((consulta, None));
        }
        if atual != anterior {
            let texto = atual
                .as_ref()
                .map_or_else(|| "sem status".to_owned(), descrever);
            output::eprint_linha(&format!("aguardando: {texto}"));
            anterior = atual;
        }
        tokio::time::sleep(intervalo.min(prazo - agora)).await;
    }
}

fn mostrar(context: &Context, consulta: &ConsultaPix) -> Result<(), CliError> {
    match context.formato() {
        Formato::Json => output::print_json(consulta),
        // `commands::run` refuses csv for this command.
        Formato::Texto | Formato::Csv => output::print(&render(consulta)),
    }
}

/// A status in words: `PAGO` -> `pago`.
fn descrever(status: &StatusPix) -> String {
    let texto = match status {
        StatusPix::Criado => "criado",
        StatusPix::AguardandoAprovacao => "aguardando aprovação no Internet Banking",
        StatusPix::Aprovado => "aprovado",
        StatusPix::Reprovado => "reprovado",
        StatusPix::Expirado => "expirado (não aprovado a tempo)",
        StatusPix::Cancelado => "cancelado",
        StatusPix::Falha => "falhou",
        StatusPix::Agendado => "agendado",
        StatusPix::Pago => "pago",
        StatusPix::Enviado => "enviado ao banco do recebedor",
        StatusPix::CanceladoSemSaldo => "cancelado por falta de saldo",
        StatusPix::Debitado => "debitado da conta",
        StatusPix::ParcialmenteDebitado => "parcialmente debitado",
        StatusPix::ParcialmentePago => "parcialmente pago",
        StatusPix::NaoDebitado => "não debitado",
        StatusPix::AgendamentoCancelado => "agendamento cancelado",
        other => other.as_str(),
    };
    texto.to_owned()
}

fn render(consulta: &ConsultaPix) -> String {
    let mut texto = String::new();
    let Some(transacao) = &consulta.transacao_pix else {
        return "Pix sem dados na resposta da API".to_owned();
    };
    let mut linhas = Vec::new();
    if let Some(status) = &transacao.status {
        linhas.push(("Status", descrever(status)));
    }
    if let Some(valor) = transacao.valor {
        linhas.push(("Valor", output::brl(valor)));
    }
    if let Some(recebedor) = &transacao.recebedor {
        let nome = recebedor.nome.as_deref().unwrap_or("?");
        let documento = recebedor
            .cpf_cnpj
            .as_deref()
            .map(|documento| format!(" ({documento})"))
            .unwrap_or_default();
        linhas.push(("Recebedor", format!("{nome}{documento}")));
        let conta = [
            recebedor
                .cod_ispb
                .as_deref()
                .map(|ispb| format!("ISPB {ispb}")),
            recebedor
                .cod_agencia
                .as_deref()
                .map(|agencia| format!("agência {agencia}")),
            recebedor
                .nro_conta
                .as_deref()
                .map(|conta| format!("conta {conta}")),
        ];
        let conta: Vec<String> = conta.into_iter().flatten().collect();
        if !conta.is_empty() {
            linhas.push(("Conta do recebedor", conta.join(", ")));
        }
    }
    if let Some(chave) = &transacao.chave {
        linhas.push(("Chave Pix", chave.clone()));
    }
    if let Some(end_to_end) = &transacao.end_to_end {
        linhas.push(("End-to-end", end_to_end.clone()));
    }
    if let Some(data) = &transacao.data_hora_solicitacao {
        linhas.push(("Solicitado em", data_hora_br(data)));
    }
    if let Some(data) = &transacao.data_hora_movimento {
        linhas.push(("Movimentado em", data_hora_br(data)));
    }
    if let Some(codigo) = &transacao.codigo_solicitacao {
        linhas.push(("Código da solicitação", codigo.clone()));
    }
    texto.push_str("Pix");
    for linha in output::key_values_left(&linhas).lines() {
        let _ = write!(texto, "\n  {linha}");
    }

    if !transacao.erros.is_empty() {
        texto.push_str("\n\nErros");
        for erro in &transacao.erros {
            let codigo = output::limpo(erro.codigo_erro.as_deref().unwrap_or("?"));
            let descricao =
                output::limpo(erro.descricao_erro.as_deref().unwrap_or("sem descrição"));
            let _ = write!(texto, "\n  {codigo}: {descricao}");
            if let Some(complementar) = &erro.codigo_erro_complementar {
                let _ = write!(texto, " ({})", output::limpo(complementar));
            }
        }
    }
    if !consulta.historico.is_empty() {
        let eventos: Vec<(String, String)> = consulta
            .historico
            .iter()
            .map(|evento| {
                (
                    evento
                        .data_hora_evento
                        .as_deref()
                        .map(data_hora_br)
                        .unwrap_or_default(),
                    evento.status.as_ref().map(descrever).unwrap_or_default(),
                )
            })
            .collect();
        let largura = eventos
            .iter()
            .map(|(data, _)| data.chars().count())
            .max()
            .unwrap_or(0);
        texto.push_str("\n\nHistórico");
        for (data, status) in eventos {
            let (data, status) = (output::limpo(&data), output::limpo(&status));
            let _ = write!(texto, "\n  {data:<largura$}  {status}");
        }
    }
    texto
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn consulta(value: serde_json::Value) -> ConsultaPix {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn outcomes_of_waiting() {
        assert_eq!(desfecho(&StatusPix::Pago), Some(Desfecho::Concluido));
        assert_eq!(desfecho(&StatusPix::Agendado), Some(Desfecho::Concluido));
        for status in [
            StatusPix::Reprovado,
            StatusPix::Expirado,
            StatusPix::Falha,
            StatusPix::CanceladoSemSaldo,
        ] {
            assert_eq!(desfecho(&status), Some(Desfecho::NaoPago), "{status}");
        }
        for status in [
            StatusPix::Criado,
            StatusPix::AguardandoAprovacao,
            StatusPix::Enviado,
            StatusPix::Outro("NOVO".to_owned()),
        ] {
            assert_eq!(desfecho(&status), None, "{status}");
        }
        // Every documented status has a description.
        for status in StatusPix::DOCUMENTADOS {
            assert_ne!(descrever(status), status.as_str(), "{status}");
        }
    }

    #[test]
    fn renders_payment_errors_and_history() {
        let texto = render(&consulta(json!({
            "transacaoPix": {
                "contaCorrente": "1234567",
                "recebedor": {
                    "nome": "Fornecedor Exemplo",
                    "cpfCnpj": "***.456.789-**",
                    "codIspb": "00000000",
                    "codAgencia": "0001",
                    "nroConta": "7654321"
                },
                "erros": [{"codigoErro": "PIX-001", "descricaoErro": "Conta de destino encerrada", "codigoErroComplementar": "AC03"}],
                "endToEnd": "E00000000202609231200abcdefghijk",
                "valor": 150.1,
                "status": "FALHA",
                "dataHoraSolicitacao": "2026-09-23T12:00:00",
                "dataHoraMovimento": "2026-09-23T12:00:01.123-03:00",
                "codigoSolicitacao": "c42f0787-02cb-4b31-827e-459ec9d7ece1"
            },
            "historico": [
                {"status": "CRIADO", "dataHoraEvento": "2026-09-23 12:00:00"},
                {"status": "FALHA", "dataHoraEvento": "2026-09-23T12:00:01"}
            ]
        })));
        assert_eq!(
            texto,
            "\
Pix
  Status                 falhou
  Valor                  R$ 150,10
  Recebedor              Fornecedor Exemplo (***.456.789-**)
  Conta do recebedor     ISPB 00000000, agência 0001, conta 7654321
  End-to-end             E00000000202609231200abcdefghijk
  Solicitado em          23/09/2026 12:00:00
  Movimentado em         23/09/2026 12:00:01
  Código da solicitação  c42f0787-02cb-4b31-827e-459ec9d7ece1

Erros
  PIX-001: Conta de destino encerrada (AC03)

Histórico
  23/09/2026 12:00:00  criado
  23/09/2026 12:00:01  falhou"
        );
        // The account of the query is the user's own: never shown.
        assert!(!texto.contains("1234567"), "{texto}");
    }

    #[test]
    fn text_from_the_api_stays_on_its_line() {
        let texto = render(&consulta(json!({
            "transacaoPix": {
                "recebedor": {"nome": "Loja\n  Valor                  R$ 0,01"},
                "erros": [{"codigoErro": "X", "descricaoErro": "falhou\u{1b}[2K\nPix enviado."}],
                "valor": 150
            }
        })));
        assert_eq!(texto.lines().count(), 6, "{texto}");
        assert!(!texto.contains('\u{1b}'), "{texto}");
        assert!(texto.contains("Loja\u{FFFD}  Valor"), "{texto}");
    }

    #[test]
    fn renders_sparse_answers() {
        assert_eq!(
            render(&consulta(json!({}))),
            "Pix sem dados na resposta da API"
        );
        assert_eq!(
            render(&consulta(json!({"transacaoPix": {"status": "NOVO"}}))),
            "Pix\n  Status  NOVO"
        );
        assert_eq!(data_hora_br("ontem"), "ontem");
    }
}
