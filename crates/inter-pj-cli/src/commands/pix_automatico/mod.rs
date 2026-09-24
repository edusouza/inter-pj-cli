//! `inter-pj pix-automatico`: the recurrences the payer authorizes once and
//! their recurring charges.

mod rec;
mod solicitacao;

use inter_pj::pix_automatico::{
    AtivacaoRec, CalendarioRecGerado, Periodicidade, PoliticaRetentativa, StatusRec, TipoJornada,
    ValorRecGerado,
};

use super::Context;
use crate::cli::PixAutomaticoCommand;
use crate::error::CliError;
use crate::output::{self, data_br};

pub(super) async fn run(context: &Context, command: PixAutomaticoCommand) -> Result<(), CliError> {
    match command {
        PixAutomaticoCommand::Rec(command) => rec::run(context, command).await,
        PixAutomaticoCommand::Solicitacao(command) => solicitacao::run(context, command).await,
    }
}

/// A status of a recurrence in words: `APROVADA` -> `aprovada (ativa)`.
fn descrever_status(status: &StatusRec) -> &str {
    match status {
        StatusRec::Criada => "criada (aguarda a aprovação do pagador)",
        StatusRec::Aprovada => "aprovada (ativa)",
        StatusRec::Rejeitada => "rejeitada pelo pagador",
        StatusRec::Expirada => "expirada sem aprovação",
        StatusRec::Cancelada => "cancelada",
        outro => outro.as_str(),
    }
}

/// Whether nothing changes a recurrence any more.
fn encerrada(status: Option<&StatusRec>) -> bool {
    matches!(
        status,
        Some(StatusRec::Rejeitada | StatusRec::Expirada | StatusRec::Cancelada)
    )
}

/// `mensal`.
fn descrever_periodicidade(periodicidade: &Periodicidade) -> &str {
    match periodicidade {
        Periodicidade::Semanal => "semanal",
        Periodicidade::Mensal => "mensal",
        Periodicidade::Trimestral => "trimestral",
        Periodicidade::Semestral => "semestral",
        Periodicidade::Anual => "anual",
        outra => outra.as_str(),
    }
}

/// `mensal, de 10/10/2026 a 10/09/2027`, or `..., a partir de 10/10/2026,
/// sem fim`.
fn descrever_calendario(calendario: &CalendarioRecGerado) -> Option<String> {
    let periodicidade = calendario
        .periodicidade
        .as_ref()
        .map(descrever_periodicidade);
    let inicio = calendario.data_inicial.as_deref().map(data_br);
    let fim = calendario.data_final.as_deref().map(data_br);
    let datas = match (inicio, fim) {
        (Some(inicio), Some(fim)) => Some(format!("de {inicio} a {fim}")),
        (Some(inicio), None) => Some(format!("a partir de {inicio}, sem fim")),
        (None, Some(fim)) => Some(format!("até {fim}")),
        (None, None) => None,
    };
    match (periodicidade, datas) {
        (Some(periodicidade), Some(datas)) => Some(format!("{periodicidade}, {datas}")),
        (Some(periodicidade), None) => Some(periodicidade.to_owned()),
        (None, datas) => datas,
    }
}

/// The amount of the payments: fixed, a floor for the payer's limit, or set
/// by each charge.
fn descrever_valor(valor: Option<&ValorRecGerado>) -> String {
    match valor.map(|valor| (valor.valor_rec, valor.valor_minimo_recebedor)) {
        Some((Some(fixo), _)) => format!("{} em cada pagamento", output::brl(fixo)),
        Some((None, Some(minimo))) => format!(
            "o de cada cobrança; o limite do pagador é de pelo menos {}",
            output::brl(minimo)
        ),
        _ => "o de cada cobrança".to_owned(),
    }
}

/// Whether charges not paid may be tried again.
fn descrever_politica(politica: &PoliticaRetentativa) -> &str {
    match politica {
        PoliticaRetentativa::Permite3R7D => "até 3 novas tentativas, em 7 dias",
        PoliticaRetentativa::NaoPermite => "não permitidas",
        outra => outra.as_str(),
    }
}

/// How the payer joined (or will join) the recurrence.
fn descrever_jornada(jornada: &TipoJornada) -> &str {
    match jornada {
        TipoJornada::Jornada1 => "pedido ao banco do pagador (solicitação de confirmação)",
        TipoJornada::Jornada2 => "QR Code da recorrência",
        TipoJornada::Jornada3 => "QR Code composto com uma cobrança imediata",
        TipoJornada::Jornada4 => "QR Code composto com uma cobrança com vencimento",
        TipoJornada::AguardandoDefinicao => "ainda não definida",
        outra => outra.as_str(),
    }
}

/// The activation in words: the path and the charge.
fn descrever_ativacao(ativacao: &AtivacaoRec) -> Option<String> {
    let jornada = ativacao.jornada().map(descrever_jornada);
    let txid = ativacao
        .dados_jornada
        .as_ref()
        .and_then(|dados| dados.txid.as_deref());
    match (jornada, txid) {
        (Some(jornada), Some(txid)) => Some(format!("{jornada} (txid {txid})")),
        (Some(jornada), None) => Some(jornada.to_owned()),
        (None, Some(txid)) => Some(format!("cobrança {txid}")),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recurrences_in_words() {
        let calendario = |inicial: Option<&str>, final_: Option<&str>| {
            let mut calendario = CalendarioRecGerado::default();
            calendario.data_inicial = inicial.map(str::to_owned);
            calendario.data_final = final_.map(str::to_owned);
            calendario.periodicidade = Some(Periodicidade::Mensal);
            calendario
        };
        assert_eq!(
            descrever_calendario(&calendario(Some("2026-10-10"), Some("2027-09-10"))).unwrap(),
            "mensal, de 10/10/2026 a 10/09/2027"
        );
        assert_eq!(
            descrever_calendario(&calendario(Some("2026-10-10"), None)).unwrap(),
            "mensal, a partir de 10/10/2026, sem fim"
        );
        let mut valor = ValorRecGerado::default();
        assert_eq!(descrever_valor(Some(&valor)), "o de cada cobrança");
        valor.valor_minimo_recebedor = Some("50".parse().unwrap());
        assert_eq!(
            descrever_valor(Some(&valor)),
            "o de cada cobrança; o limite do pagador é de pelo menos R$ 50,00"
        );
        valor.valor_rec = Some("149.90".parse().unwrap());
        assert_eq!(descrever_valor(Some(&valor)), "R$ 149,90 em cada pagamento");
        assert!(encerrada(Some(&StatusRec::Cancelada)));
        assert!(!encerrada(Some(&StatusRec::Aprovada)));
        assert!(!encerrada(None));
    }
}
