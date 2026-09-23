use std::fmt;

use chrono::{Days, NaiveDate};

/// Date range of a statement query, validated against the API's limit of
/// [`Periodo::MAX_DIAS`] days.
///
/// ```
/// use chrono::NaiveDate;
/// use inter_pj::banking::Periodo;
///
/// let data = |m, d| NaiveDate::from_ymd_opt(2026, m, d).unwrap();
/// let periodo = Periodo::new(data(1, 1), data(3, 31)).unwrap();
/// assert_eq!(periodo.dias(), 90);
/// assert!(Periodo::new(data(1, 1), data(4, 1)).is_err());
///
/// // Longer ranges can be split into consecutive periods.
/// let partes = Periodo::dividir(data(1, 1), data(12, 31)).unwrap();
/// assert_eq!(partes.len(), 5);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Periodo {
    inicio: NaiveDate,
    fim: NaiveDate,
}

impl Periodo {
    /// Longest period accepted by the statement endpoints, in days, counting
    /// both the first and the last day.
    ///
    /// The API documents "at most 90 days between the dates"; counting both
    /// ends is the stricter reading, so no request built here is refused.
    pub const MAX_DIAS: u64 = 90;

    /// The period from `inicio` to `fim`, both included.
    ///
    /// # Errors
    ///
    /// Fails when `fim` is before `inicio` or the period is longer than
    /// [`MAX_DIAS`](Self::MAX_DIAS) days.
    pub fn new(inicio: NaiveDate, fim: NaiveDate) -> Result<Self, PeriodoError> {
        let periodo = Self::unchecked(inicio, fim)?;
        if periodo.dias() > Self::MAX_DIAS {
            return Err(PeriodoError::MuitoLongo {
                dias: periodo.dias(),
            });
        }
        Ok(periodo)
    }

    /// Splits the range from `inicio` to `fim` (both included) into
    /// consecutive periods of at most [`MAX_DIAS`](Self::MAX_DIAS) days,
    /// in chronological order.
    ///
    /// # Errors
    ///
    /// Fails when `fim` is before `inicio`.
    pub fn dividir(inicio: NaiveDate, fim: NaiveDate) -> Result<Vec<Self>, PeriodoError> {
        Self::unchecked(inicio, fim)?;
        let mut partes = Vec::new();
        let mut inicio_parte = inicio;
        loop {
            let fim_parte = inicio_parte
                .checked_add_days(Days::new(Self::MAX_DIAS - 1))
                .map_or(fim, |limite| limite.min(fim));
            partes.push(Self {
                inicio: inicio_parte,
                fim: fim_parte,
            });
            match fim_parte.succ_opt() {
                Some(proximo) if fim_parte < fim => inicio_parte = proximo,
                _ => return Ok(partes),
            }
        }
    }

    fn unchecked(inicio: NaiveDate, fim: NaiveDate) -> Result<Self, PeriodoError> {
        if fim < inicio {
            return Err(PeriodoError::FimAntesDoInicio { inicio, fim });
        }
        Ok(Self { inicio, fim })
    }

    /// First day.
    pub fn inicio(&self) -> NaiveDate {
        self.inicio
    }

    /// Last day.
    pub fn fim(&self) -> NaiveDate {
        self.fim
    }

    /// Number of days, the first and the last included.
    pub fn dias(&self) -> u64 {
        // `fim >= inicio` is an invariant, so the difference is never negative.
        (self.fim - self.inicio).num_days().unsigned_abs() + 1
    }

    pub(crate) fn query(self) -> [(&'static str, String); 2] {
        [
            ("dataInicio", self.inicio.format("%Y-%m-%d").to_string()),
            ("dataFim", self.fim.format("%Y-%m-%d").to_string()),
        ]
    }
}

impl fmt::Display for Periodo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} a {}",
            self.inicio.format("%d/%m/%Y"),
            self.fim.format("%d/%m/%Y")
        )
    }
}

/// Why a [`Periodo`] could not be built.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum PeriodoError {
    /// The last day comes before the first one.
    #[error(
        "a data final ({}) é anterior à inicial ({})",
        fim.format("%d/%m/%Y"),
        inicio.format("%d/%m/%Y")
    )]
    FimAntesDoInicio {
        /// First day given.
        inicio: NaiveDate,
        /// Last day given.
        fim: NaiveDate,
    },
    /// The period is longer than the API accepts.
    #[error(
        "o período tem {dias} dias; a API aceita no máximo {} dias por consulta",
        Periodo::MAX_DIAS
    )]
    MuitoLongo {
        /// Days in the requested period.
        dias: u64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(ano: i32, mes: u32, dia: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(ano, mes, dia).unwrap()
    }

    #[test]
    fn accepts_up_to_ninety_days_counting_both_ends() {
        let periodo = Periodo::new(data(2026, 1, 1), data(2026, 3, 31)).unwrap();
        assert_eq!(periodo.dias(), 90);
        assert_eq!(
            Periodo::new(data(2026, 1, 1), data(2026, 4, 1)),
            Err(PeriodoError::MuitoLongo { dias: 91 })
        );
        assert_eq!(
            Periodo::new(data(2026, 5, 5), data(2026, 5, 5))
                .unwrap()
                .dias(),
            1
        );
    }

    #[test]
    fn leap_years_are_counted() {
        // 31 (jan) + 29 (fev/2024) + 30 = 90 dias.
        assert!(Periodo::new(data(2024, 1, 1), data(2024, 3, 30)).is_ok());
        assert_eq!(
            Periodo::new(data(2024, 1, 1), data(2024, 3, 31)),
            Err(PeriodoError::MuitoLongo { dias: 91 })
        );
        // Sem 29/02 em 2025, o mesmo intervalo de datas cabe.
        assert!(Periodo::new(data(2025, 1, 1), data(2025, 3, 31)).is_ok());
    }

    #[test]
    fn rejects_end_before_start() {
        let err = Periodo::new(data(2026, 2, 1), data(2026, 1, 31)).unwrap_err();
        assert_eq!(
            err.to_string(),
            "a data final (31/01/2026) é anterior à inicial (01/02/2026)"
        );
        assert!(Periodo::dividir(data(2026, 2, 1), data(2026, 1, 31)).is_err());
    }

    #[test]
    fn splits_long_ranges_into_consecutive_periods() {
        let partes = Periodo::dividir(data(2024, 1, 1), data(2024, 12, 31)).unwrap();
        let dias: Vec<u64> = partes.iter().map(Periodo::dias).collect();
        assert_eq!(dias, [90, 90, 90, 90, 6], "2024 tem 366 dias");
        assert_eq!(partes[0].inicio(), data(2024, 1, 1));
        assert_eq!(partes[4].fim(), data(2024, 12, 31));
        for par in partes.windows(2) {
            assert_eq!(par[0].fim().succ_opt(), Some(par[1].inicio()));
        }
    }

    #[test]
    fn split_keeps_short_and_exact_ranges_whole() {
        let curto = Periodo::dividir(data(2026, 1, 10), data(2026, 1, 20)).unwrap();
        assert_eq!(
            curto,
            [Periodo::new(data(2026, 1, 10), data(2026, 1, 20)).unwrap()]
        );
        let exato = Periodo::dividir(data(2026, 1, 1), data(2026, 3, 31)).unwrap();
        assert_eq!(exato.len(), 1);
        let um_a_mais = Periodo::dividir(data(2026, 1, 1), data(2026, 4, 1)).unwrap();
        assert_eq!(
            um_a_mais.iter().map(Periodo::dias).collect::<Vec<_>>(),
            [90, 1]
        );
    }

    #[test]
    fn query_and_display_formats() {
        let periodo = Periodo::new(data(2026, 8, 1), data(2026, 8, 31)).unwrap();
        assert_eq!(
            periodo.query(),
            [
                ("dataInicio", "2026-08-01".to_owned()),
                ("dataFim", "2026-08-31".to_owned())
            ]
        );
        assert_eq!(periodo.to_string(), "01/08/2026 a 31/08/2026");
    }
}
