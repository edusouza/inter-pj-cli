//! Pix: validation of keys and decoding of "copia e cola" codes (BR Code),
//! done locally so mistakes are caught before any money moves, and the Pix
//! API ([`Pix`]): charges with a dynamic QR Code, the Pix received and
//! their refunds.

mod api;
mod brcode;
mod chave;
mod cob;
mod cobv;
mod comum;
mod recebido;
mod txid;

pub use api::Pix;
pub use brcode::{BrCode, BrCodeError, crc16};
pub(crate) use chave::is_uuid;
pub use chave::{ChavePix, ChavePixError};
pub use cob::{
    CalendarioCob, CalendarioCobGerado, Cob, CobRevisada, CobSolicitada, FiltroCobs, LocCob,
    MAX_SOLICITACAO_PAGADOR, ModalidadeAgente, PaginaCobs, ParametrosConsulta, Retirada, StatusCob,
    ValorCob, ValorCobGerado, ValorCobRevisada, ValorRetirada,
};
pub use cobv::{
    AbatimentoCobv, CalendarioCobv, CalendarioCobvGerado, Cobv, CobvRevisada, CobvSolicitada,
    DescontoCobv, DescontoData, DescontoDataGerado, DevedorCobv, EncargoCobv, FiltroCobvs,
    JurosCobv, MAX_DESCONTOS_DATA_FIXA, ModalidadeJuros, MultaCobv, PaginaCobvs, ValorCobv,
    ValorCobvGerado, ValorCobvRevisada,
};
pub use comum::{
    CobrancaPixError, Devedor, ITENS_POR_PAGINA_MAXIMO_PIX, InfoAdicional, LocationPix,
    MAX_INFO_ADICIONAIS, Paginacao, PeriodoPix, PeriodoPixError, PessoaPix, TipoCob,
    VALOR_MAXIMO_PIX,
};
pub use recebido::{
    Devolucao, DevolucaoSolicitada, FiltroPixRecebidos, HorarioDevolucao, ID_DEVOLUCAO_MAXIMO,
    IdDevolucao, IdDevolucaoError, MAX_DESCRICAO_DEVOLUCAO, NaturezaDevolucao, PaginaPixRecebidos,
    PixRecebido, StatusDevolucao,
};
pub use txid::{TXID_MAXIMO, TXID_MINIMO, Txid, TxidError};
