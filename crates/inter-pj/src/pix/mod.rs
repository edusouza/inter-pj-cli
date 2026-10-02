//! Pix: validation of keys and decoding of "copia e cola" codes (BR Code),
//! done locally so mistakes are caught before any money moves.

mod brcode;
mod chave;

pub use brcode::{BrCode, BrCodeError, crc16};
pub use chave::{ChavePix, ChavePixError};
