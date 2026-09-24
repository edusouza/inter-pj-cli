//! Pix Automático (`/pix/v2`): recurring charges the payer authorizes once.
//!
//! The receiver creates a recurrence ([`RecSolicitada`]) with the contract,
//! the period and the frequency; the payer approves it in their bank, by
//! the QR Code of the recurrence (see [`LocationRec`]) or by a confirmation
//! request ([`SolicRecSolicitada`]); then each recurring charge is created
//! against it. The API is available only to CNPJs with at least 6 months
//! of activity.
//!
//! Obtained with [`InterClient::pix_automatico`].

/// Characters of an [`IdRec`] and of an [`IdSolicRec`].
pub const ID_TAMANHO: usize = 29;

/// Longest agreement code (`convenio`) in a filter.
pub const MAX_CONVENIO: usize = 60;

/// An identifier the API creates: [`ID_TAMANHO`] letters and digits, case
/// sensitive, checked before being sent.
macro_rules! identificador {
    (
        $(#[$meta:meta])*
        $nome:ident, $erro:ident, $campo:literal, $exemplo:literal
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $nome(String);

        impl $nome {
            #[doc = concat!("Checks a `", $campo, "`.")]
            ///
            /// # Errors
            ///
            /// When it is not 29 ASCII letters and digits.
            pub fn parse(raw: &str) -> Result<Self, $erro> {
                let id = raw.trim();
                if id.len() == $crate::pix_automatico::ID_TAMANHO
                    && id.bytes().all(|b| b.is_ascii_alphanumeric())
                {
                    Ok(Self(id.to_owned()))
                } else {
                    Err($erro)
                }
            }

            /// The identifier as sent.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::str::FromStr for $nome {
            type Err = $erro;

            fn from_str(raw: &str) -> Result<Self, Self::Err> {
                Self::parse(raw)
            }
        }

        impl std::fmt::Display for $nome {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl serde::Serialize for $nome {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&self.0)
            }
        }

        #[doc = concat!("Not a `", $campo, "`.")]
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct $erro;

        impl std::fmt::Display for $erro {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(concat!(
                    $campo,
                    " inválido: são 29 letras e dígitos, como a API os cria (ex.: ",
                    $exemplo,
                    ")"
                ))
            }
        }

        impl std::error::Error for $erro {}
    };
}

mod cobr;
mod locrec;
mod rec;
mod solicrec;

pub use cobr::{
    AtualizacaoCobR, AtualizacaoTentativa, CalendarioCobR, CobR, CobRSolicitada, ContaRecebedor,
    DevedorCobR, DevedorCobRGerado, FiltroCobsR, MAX_INFO_ADICIONAL, PaginaCobsR,
    ParametrosConsultaCobR, RecebedorCobR, StatusCobR, StatusTentativa, TentativaCobR,
    TipoContaRecebedor, TipoTentativa, ValorCobR,
};
pub use locrec::{FiltroLocsRec, PaginaLocsRec, ParametrosConsultaLocRec};
pub use rec::{
    AtivacaoRec, AtivacaoSolicitada, AtualizacaoRec, CalendarioRec, CalendarioRecGerado,
    CancelamentoRec, DadosJornada, DadosQrRec, EncerramentoRec, FiltroRecs, IdRec, IdRecError,
    LocationRec, MAX_CONTRATO, MAX_NOME_DEVEDOR, MAX_OBJETO, PagadorRec, PaginaRecs,
    ParametrosConsultaRec, Periodicidade, PoliticaRetentativa, Rec, RecRevisada, RecSolicitada,
    RecebedorRec, RejeicaoRec, StatusRec, TipoJornada, ValorRec, ValorRecGerado, VinculoRec,
    VinculoRecGerado,
};
pub use solicrec::{
    AtualizacaoSolicRec, CalendarioSolicRec, DestinatarioSolicRec, DestinatarioSolicRecGerado,
    IdSolicRec, IdSolicRecError, MAX_AGENCIA, MAX_CONTA, SolicRec, SolicRecSolicitada,
    StatusSolicRec,
};

use crate::client::InterClient;
use crate::error::{Error, Result};
use crate::pix::{CobrancaPixError, texto};

/// Operations of Pix Automático. Obtained with
/// [`InterClient::pix_automatico`].
#[derive(Debug, Clone, Copy)]
pub struct PixAutomatico<'a> {
    client: &'a InterClient,
}

impl<'a> PixAutomatico<'a> {
    pub(crate) fn new(client: &'a InterClient) -> Self {
        Self { client }
    }
}

/// An account number with its check digit: digits only (the check digit
/// may be `X`), up to `maximo` characters.
fn conta(conta: &str, campo: &str, maximo: usize) -> Result<(), CobrancaPixError> {
    let sem_dv = conta.strip_suffix(['X', 'x']).unwrap_or(conta);
    if digitos(sem_dv) && conta.len() <= maximo {
        Ok(())
    } else {
        Err(CobrancaPixError::new(
            campo,
            format!(
                "até {maximo} dígitos, com o dígito verificador (que pode ser X), sem pontos nem traços"
            ),
        ))
    }
}

/// A branch: digits only, up to `maximo`, without the check digit.
fn agencia(agencia: &str, campo: &str, maximo: usize) -> Result<(), CobrancaPixError> {
    if digitos(agencia) && agencia.len() <= maximo {
        Ok(())
    } else {
        Err(CobrancaPixError::new(
            campo,
            format!("até {maximo} dígitos, sem o dígito verificador"),
        ))
    }
}

fn digitos(texto: &str) -> bool {
    !texto.is_empty() && texto.bytes().all(|b| b.is_ascii_digit())
}

/// The agreement of a filter, up to [`MAX_CONVENIO`] characters.
fn convenio(convenio: &str) -> Result<String> {
    texto(convenio, "convenio", MAX_CONVENIO).map_err(|err| Error::InvalidInput(Box::new(err)))?;
    Ok(convenio.to_owned())
}
