//! Pix Automático in one state: the recurrences ([`rec`](super::rec)), which
//! the payers authorize once, the confirmation requests
//! ([`solicrec`](super::solicrec)) that ask them to, the recurring charges
//! ([`cobr`](super::cobr)) of each cycle and the locations of the QR Codes
//! of the recurrences ([`locrec`](super::locrec)), each in its recurrence
//! or free for the next; the sandbox answers for the payers
//! ([`sandbox_automatico`](super::sandbox_automatico)). What is created or
//! changed today takes the next time of one clock, 5 minutes after the one
//! before, and each recurrence or request created today the next id of the
//! account's bank.

use chrono::{DateTime, SecondsFormat, TimeDelta};
use serde_json::{Value, json};

use super::{cobr, locrec, rec, solicrec};
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
    /// The locations without a recurrence; the others are in theirs.
    locrecs_livres: Vec<Value>,
    /// The id of the next location.
    proxima_locrec: u64,
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
        let proxima_locrec = recs
            .iter()
            .filter_map(|rec| rec["loc"]["id"].as_u64())
            .max()
            .map_or(8100, |id| id + 1);
        Self {
            recs,
            solicitacoes,
            cobrs: cobr::iniciais(),
            agendadas: 0,
            locrecs_livres: Vec::new(),
            proxima_locrec,
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

    /// A recurrence as its lookup shows it: with its requests and, when it
    /// has a location, its QR Code.
    pub(super) fn consulta(&self, rec: &Value) -> Value {
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
        if let Some(loc) = rec.get("loc") {
            rec["dadosQR"] = locrec::dados_qr(loc);
        }
        rec
    }

    /// A location created now, free for a recurrence.
    pub(super) fn nova_locrec(&mut self) -> Value {
        let id = self.proxima_locrec;
        self.proxima_locrec += 1;
        let loc = json!({
            "id": id,
            "location": locrec::location(id),
            "criacao": self.agora(),
        });
        self.locrecs_livres.push(loc.clone());
        loc
    }

    /// The free location `id`, taken for a recurrence; `None` when there is
    /// no such location or it has a recurrence.
    pub(super) fn locrec_livre(&mut self, id: u64) -> Option<Value> {
        let livre = self.locrecs_livres.iter().position(|loc| loc["id"] == id)?;
        Some(self.locrecs_livres.remove(livre))
    }

    /// A location a recurrence leaves, which becomes free.
    pub(super) fn liberar_locrec(&mut self, mut loc: Value) {
        loc.as_object_mut().unwrap().remove("idRec");
        self.locrecs_livres.push(loc);
    }

    /// Every location, with its recurrence, in the order of the ids.
    pub(super) fn locrecs(&self) -> Vec<Value> {
        let mut locs: Vec<Value> = self
            .recs
            .iter()
            .filter_map(|rec| rec.get("loc").cloned())
            .chain(self.locrecs_livres.iter().cloned())
            .collect();
        locs.sort_by_key(|loc| loc["id"].as_u64());
        locs
    }

    /// The location `id`, with its recurrence.
    pub(super) fn locrec(&self, id: u64) -> Option<Value> {
        self.locrecs().into_iter().find(|loc| loc["id"] == id)
    }

    /// Unlinks the recurrence of the location `id`: the recurrence loses
    /// the location and its QR Code but keeps its status, and the location
    /// becomes free.
    pub(super) fn desvincular_locrec(&mut self, id: u64) {
        let Some(rec) = self.recs.iter_mut().find(|rec| rec["loc"]["id"] == id) else {
            return;
        };
        let loc = rec.as_object_mut().unwrap().remove("loc").unwrap();
        self.liberar_locrec(loc);
    }
}

/// `segundos` after `momento` (RFC 3339), in the same format.
pub(super) fn depois(momento: &str, segundos: i64) -> String {
    let momento = DateTime::parse_from_rfc3339(momento).unwrap();
    (momento + TimeDelta::seconds(segundos)).to_rfc3339_opts(SecondsFormat::Millis, true)
}
