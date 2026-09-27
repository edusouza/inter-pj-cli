//! Pix Automático in one state: the recurrences ([`rec`](super::rec)), which
//! the payers authorize once, the confirmation requests
//! ([`solicrec`](super::solicrec)) that ask them to, and the recurring
//! charges ([`cobr`](super::cobr)) of each cycle. What is created or changed
//! today takes the next time of one clock, 5 minutes after the one before,
//! and each recurrence or request created today the next id of the account's
//! bank.

use chrono::{DateTime, SecondsFormat, TimeDelta};
use serde_json::{Value, json};

use super::{cobr, rec, solicrec};
use crate::sessao::HOJE;

/// The ISPB of the account's bank, in the ids it creates.
pub(super) const ISPB: &str = "12345678";

/// The ends of the ids of the recurrences created today, in order: 11
/// letters and digits, as the API creates them.
const SUFIXOS_REC: [&str; 6] = [
    "Qm4Tz8Kd2Wb",
    "Hx7Rn3Vp5Lc",
    "Bw2Jy6Fs9Nt",
    "Zc5Mg1Qr8Xe",
    "Kt9Ld4Wh6Pa",
    "Vn3Sb7Cx2Ym",
];

/// The same, of the confirmation requests.
const SUFIXOS_SOLICITACAO: [&str; 4] = ["Tq6Wn2Hy8Kd", "Lp3Rv9Jc5Xs", "Mz7Fb1Qt4Gw", "Yh5Ck8Nd2Pr"];

pub(super) struct Automatico {
    /// Each recurrence as the API shows it, without its requests.
    pub(super) recs: Vec<Value>,
    /// Each confirmation request as the API shows it.
    pub(super) solicitacoes: Vec<Value>,
    /// Each recurring charge as the API shows it.
    pub(super) cobrs: Vec<Value>,
    /// The debits the payers' banks scheduled today, for the id of the next.
    pub(super) agendadas: usize,
    /// What was done today, for the time of the next one.
    feitos: i64,
    /// The recurrences created today, for the id of the next one.
    criadas: usize,
    /// The requests created today, for the id of the next one.
    solicitadas: usize,
}

impl Automatico {
    /// The recurrences and the requests before the guides.
    pub(super) fn novo() -> Self {
        let recs = rec::iniciais();
        let solicitacoes = solicrec::iniciais(&recs);
        Self {
            recs,
            solicitacoes,
            cobrs: cobr::iniciais(),
            agendadas: 0,
            feitos: 0,
            criadas: 0,
            solicitadas: 0,
        }
    }

    /// The time of what is done now, today: from 10:20 in Brasília.
    pub(super) fn agora(&mut self) -> String {
        let inicio = DateTime::parse_from_rfc3339(&format!("{HOJE}T13:20:00.000Z")).unwrap();
        let agora = inicio + TimeDelta::minutes(5 * self.feitos);
        self.feitos += 1;
        agora.to_rfc3339_opts(SecondsFormat::Millis, true)
    }

    /// The id of the next recurrence created today: `RR` when its charges
    /// may be tried again, `RN` otherwise, the ISPB, the date and 11
    /// characters.
    pub(super) fn id_rec(&mut self, retentativas: bool) -> String {
        let inicio = if retentativas { "RR" } else { "RN" };
        let sufixo = SUFIXOS_REC[self.criadas % SUFIXOS_REC.len()];
        self.criadas += 1;
        format!("{inicio}{ISPB}{}{sufixo}", HOJE.replace('-', ""))
    }

    /// The id of the next request created today: `SC`, the ISPB, the date
    /// and 11 characters.
    pub(super) fn id_solicitacao(&mut self) -> String {
        let sufixo = SUFIXOS_SOLICITACAO[self.solicitadas % SUFIXOS_SOLICITACAO.len()];
        self.solicitadas += 1;
        format!("SC{ISPB}{}{sufixo}", HOJE.replace('-', ""))
    }

    /// The requests created, sent and received by the payer's bank by the
    /// time anyone looks: 2 and 5 seconds after they were created.
    pub(super) fn andamento(&mut self) {
        for solicitacao in &mut self.solicitacoes {
            if solicitacao["status"] != "CRIADA" {
                continue;
            }
            let criacao = solicitacao["atualizacao"][0]["data"]
                .as_str()
                .unwrap()
                .to_owned();
            let atualizacao = solicitacao["atualizacao"].as_array_mut().unwrap();
            atualizacao.push(json!({"status": "ENVIADA", "data": depois(&criacao, 2)}));
            atualizacao.push(json!({"status": "RECEBIDA", "data": depois(&criacao, 5)}));
            solicitacao["status"] = json!("RECEBIDA");
        }
    }

    /// A recurrence as its lookup shows it, with its requests.
    pub(super) fn com_solicitacoes(&self, rec: &Value) -> Value {
        let mut rec = rec.clone();
        let solicitacoes: Vec<Value> = self
            .solicitacoes
            .iter()
            .filter(|solicitacao| solicitacao["idRec"] == rec["idRec"])
            .map(|solicitacao| {
                json!({
                    "idSolicRec": solicitacao["idSolicRec"],
                    "status": solicitacao["status"],
                    "calendario": solicitacao["calendario"],
                })
            })
            .collect();
        if !solicitacoes.is_empty() {
            rec["solicitacao"] = json!(solicitacoes);
        }
        rec
    }
}

/// `segundos` after `momento` (RFC 3339), in the same format.
pub(super) fn depois(momento: &str, segundos: i64) -> String {
    let momento = DateTime::parse_from_rfc3339(momento).unwrap();
    (momento + TimeDelta::seconds(segundos)).to_rfc3339_opts(SecondsFormat::Millis, true)
}
