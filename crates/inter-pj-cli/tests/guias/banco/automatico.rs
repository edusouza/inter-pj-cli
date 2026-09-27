//! Pix Automático in one state: the recurrences ([`rec`](super::rec)), which
//! the payers authorize once. What is created or changed today takes the
//! next time of one clock, 5 minutes after the one before, and each
//! recurrence created today the next id of the account's bank.

use chrono::{DateTime, SecondsFormat, TimeDelta};
use serde_json::Value;

use super::rec;
use crate::sessao::HOJE;

/// The ISPB of the account's bank, in the ids it creates.
pub(super) const ISPB: &str = "12345678";

/// The ends of the ids of the recurrences created today, in order: 11
/// letters and digits, as the API creates them.
const SUFIXOS: [&str; 6] = [
    "Qm4Tz8Kd2Wb",
    "Hx7Rn3Vp5Lc",
    "Bw2Jy6Fs9Nt",
    "Zc5Mg1Qr8Xe",
    "Kt9Ld4Wh6Pa",
    "Vn3Sb7Cx2Ym",
];

pub(super) struct Automatico {
    /// Each recurrence as the API shows it.
    pub(super) recs: Vec<Value>,
    /// What was done today, for the time of the next one.
    feitos: i64,
    /// The recurrences created today, for the id of the next one.
    criadas: usize,
}

impl Automatico {
    /// The recurrences before the guides.
    pub(super) fn novo() -> Self {
        Self {
            recs: rec::iniciais(),
            feitos: 0,
            criadas: 0,
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
        let sufixo = SUFIXOS[self.criadas % SUFIXOS.len()];
        self.criadas += 1;
        format!("{inicio}{ISPB}{}{sufixo}", HOJE.replace('-', ""))
    }
}
