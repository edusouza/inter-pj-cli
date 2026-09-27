//! The Pix charges of the account, immediate ([`cob`](super::cob)) and with
//! a due date ([`cobv`](super::cobv)), and their locations
//! ([`loc`](super::loc)), in one state: a location serves one charge after
//! another, and a charge created without one gets its own. The ids of the
//! locations follow the order they were created in, and what is created
//! today takes the next time of one clock, 7 minutes after the one before.

use std::collections::HashMap;

use chrono::{DateTime, FixedOffset, TimeDelta};
use serde_json::{Value, json};
use wiremock::{Request, ResponseTemplate};

use super::{cob, cobv, parametros, problema};
use crate::sessao::HOJE;

pub(super) struct Cobrancas {
    pub(super) cobs: Vec<Value>,
    pub(super) cobvs: Vec<Value>,
    /// The locations without a charge.
    livres: Vec<Value>,
    /// The id of the next location.
    proxima: u64,
    /// What was created today, for the time of the next one.
    criadas: i64,
}

impl Cobrancas {
    /// The charges before the guides, with the locations numbered in the
    /// order they were created.
    pub(super) fn novo() -> Self {
        let mut cobs = cob::iniciais();
        let mut cobvs = cobv::iniciais();
        let mut todas: Vec<&mut Value> = cobs.iter_mut().chain(cobvs.iter_mut()).collect();
        todas.sort_by_key(|cobranca| cobranca["loc"]["criacao"].as_str().unwrap().to_owned());
        let mut proxima = 7000;
        for cobranca in todas {
            cobranca["loc"]["id"] = json!(proxima);
            proxima += 1;
        }
        Self {
            cobs,
            cobvs,
            livres: Vec::new(),
            proxima,
            criadas: 0,
        }
    }

    /// When the next thing is created: at 10:05:12 of today in Brasília,
    /// and then 7 minutes apart.
    pub(super) fn agora(&mut self) -> String {
        let inicio = DateTime::parse_from_rfc3339(&format!("{HOJE}T13:05:12.000Z")).unwrap();
        let agora = inicio + TimeDelta::minutes(7 * self.criadas);
        self.criadas += 1;
        agora.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
    }

    /// The location of a charge created now: the free one of its kind it
    /// asks for, or a new one of its own; `None` when the one asked for is
    /// not free.
    pub(super) fn location(
        &mut self,
        tipo: &str,
        txid: &str,
        criacao: &str,
        pedida: Option<&Value>,
    ) -> Option<Value> {
        if let Some(id) = pedida.and_then(|loc| loc["id"].as_u64()) {
            let livre = self
                .livres
                .iter()
                .position(|loc| loc["id"] == id && loc["tipoCob"] == tipo)?;
            return Some(self.livres.remove(livre));
        }
        Some(self.nova(tipo, &format!("{tipo}/{txid}"), criacao))
    }

    /// A location created without a charge, for one later.
    pub(super) fn livre(&mut self, tipo: &str) -> Value {
        let criacao = self.agora();
        let id = self.proxima;
        let loc = self.nova(tipo, &format!("loc/{id}"), &criacao);
        self.livres.push(loc.clone());
        loc
    }

    fn nova(&mut self, tipo: &str, caminho: &str, criacao: &str) -> Value {
        let loc = json!({
            "id": self.proxima,
            "location": format!("qrcodepix.inter.example/qr/v2/{caminho}"),
            "tipoCob": tipo,
            "criacao": criacao,
        });
        self.proxima += 1;
        loc
    }

    /// A location the charge leaves, which becomes free.
    pub(super) fn liberar(&mut self, loc: Value) {
        self.livres.push(loc);
    }

    /// Every location, with the txid of its charge, in the order of the
    /// ids.
    pub(super) fn locs(&self) -> Vec<Value> {
        let vinculadas = self.cobs.iter().chain(&self.cobvs).filter_map(|cobranca| {
            let mut loc = cobranca.get("loc")?.clone();
            loc["txid"] = cobranca["txid"].clone();
            Some(loc)
        });
        let mut locs: Vec<Value> = vinculadas.chain(self.livres.iter().cloned()).collect();
        locs.sort_by_key(|loc| loc["id"].as_u64());
        locs
    }

    /// Unlinks the charge of the location `id`, which loses the location
    /// and its QR Code but keeps its status; the location becomes free.
    pub(super) fn desvincular(&mut self, id: u64) -> Option<Value> {
        let cobranca = self
            .cobs
            .iter_mut()
            .chain(self.cobvs.iter_mut())
            .find(|cobranca| cobranca["loc"]["id"] == id)?;
        let campos = cobranca.as_object_mut().unwrap();
        let loc = campos.remove("loc")?;
        for campo in ["location", "pixCopiaECola"] {
            campos.remove(campo);
        }
        self.livres.push(loc.clone());
        Some(loc)
    }
}

/// The charge a location was asked for does not have.
pub(super) fn location_invalida() -> ResponseTemplate {
    problema(
        400,
        "Location inválida",
        "A location informada não existe, já tem uma cobrança ou é de outro tipo.",
    )
}

/// The dynamic BR Code of a location, with its CRC16.
pub(super) fn copia_e_cola(location: &str) -> String {
    let campo = |id: &str, valor: &str| format!("{id}{:02}{valor}", valor.len());
    let conta = campo("00", "br.gov.bcb.pix") + &campo("25", location);
    let corpo = [
        campo("00", "01"),
        campo("01", "12"),
        campo("26", &conta),
        campo("52", "0000"),
        campo("53", "986"),
        campo("58", "BR"),
        campo("59", "EMPRESA EXEMPLO LTDA"),
        campo("60", "BELO HORIZONTE"),
        campo("62", &campo("05", "***")),
        "6304".to_owned(),
    ]
    .concat();
    let crc = crc16(corpo.as_bytes());
    format!("{corpo}{crc:04X}")
}

/// CRC16-CCITT (polynomial 0x1021, initial value 0xFFFF), as the BR Code.
fn crc16(bytes: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for byte in bytes {
        crc ^= u16::from(*byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 == 0 {
                crc << 1
            } else {
                (crc << 1) ^ 0x1021
            };
        }
    }
    crc
}

/// The period of a listing, in RFC 3339, which the API requires.
pub(super) fn periodo(
    parametros: &HashMap<String, String>,
) -> Option<(DateTime<FixedOffset>, DateTime<FixedOffset>)> {
    let momento = |campo: &str| {
        DateTime::parse_from_rfc3339(parametros.get(campo).map_or("", String::as_str)).ok()
    };
    Some((momento("inicio")?, momento("fim")?))
}

pub(super) fn periodo_invalido() -> ResponseTemplate {
    problema(
        400,
        "Período inválido",
        "inicio e fim são obrigatórios, em RFC 3339.",
    )
}

/// Whether `quando` (RFC 3339) is in the period.
pub(super) fn no_periodo(
    quando: &Value,
    (inicio, fim): (DateTime<FixedOffset>, DateTime<FixedOffset>),
) -> bool {
    let quando = DateTime::parse_from_rfc3339(quando.as_str().unwrap()).unwrap();
    inicio <= quando && quando <= fim
}

/// A page of a listing of `itens`, from 0, in the field `campo`.
pub(super) fn pagina(
    itens: &[&Value],
    parametros: &HashMap<String, String>,
    campo: &str,
) -> ResponseTemplate {
    let pagina: usize = parametros
        .get("paginacao.paginaAtual")
        .map_or(0, |pagina| pagina.parse().unwrap());
    let por_pagina: usize = parametros
        .get("paginacao.itensPorPagina")
        .map_or(100, |itens| itens.parse().unwrap());
    let nesta: Vec<&&Value> = itens
        .iter()
        .skip(pagina * por_pagina)
        .take(por_pagina)
        .collect();
    ResponseTemplate::new(200).set_body_json(json!({
        "parametros": {
            "inicio": parametros["inicio"],
            "fim": parametros["fim"],
            "paginacao": {
                "paginaAtual": pagina,
                "itensPorPagina": por_pagina,
                "quantidadeDePaginas": itens.len().div_ceil(por_pagina),
                "quantidadeTotalDeItens": itens.len(),
            },
        },
        campo: nesta,
    }))
}

/// The charges created in the period, with the filters, in pages from 0:
/// the listing of the immediate charges and of those with a due date.
pub(super) fn listar(cobrancas: &[Value], request: &Request) -> ResponseTemplate {
    let parametros = parametros(request);
    let Some(periodo) = periodo(&parametros) else {
        return periodo_invalido();
    };
    let filtro = |campo: &str, aceita: &dyn Fn(&str) -> bool| {
        parametros
            .get(campo)
            .is_none_or(|valor| aceita(valor.as_str()))
    };
    let cobs: Vec<&Value> = cobrancas
        .iter()
        .filter(|cob| {
            no_periodo(&cob["calendario"]["criacao"], periodo)
                && ["cpf", "cnpj"]
                    .iter()
                    .all(|campo| filtro(campo, &|documento| cob["devedor"][*campo] == documento))
                && filtro("locationPresente", &|presente| {
                    (presente == "true") == cob.get("loc").is_some()
                })
                && filtro("status", &|status| cob["status"] == status)
        })
        .collect();
    pagina(&cobs, &parametros, "cobs")
}
