use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::{CobrancaDetalhada, SituacaoCobranca, TipoCobranca};
use crate::serde_util::{decimal_as_number, lenient};

/// Largest page of the listing the API returns.
pub const ITENS_POR_PAGINA_MAXIMO: u32 = 1000;

/// Which charges [`Cobranca::listar`](super::Cobranca::listar) and
/// [`Cobranca::sumario`](super::Cobranca::sumario) consider.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct FiltroCobrancas {
    /// First day of the period (`dataInicial`).
    pub data_inicial: NaiveDate,
    /// Last day of the period (`dataFinal`).
    pub data_final: NaiveDate,
    /// The date the period refers to; the API's default is the due date.
    pub filtrar_data_por: Option<FiltrarDataPor>,
    /// Only charges in this situation.
    pub situacao: Option<SituacaoCobranca>,
    /// Name of the payer (`pessoaPagadora`).
    pub pessoa_pagadora: Option<String>,
    /// CPF or CNPJ of the payer, up to 18 characters.
    pub cpf_cnpj_pessoa_pagadora: Option<String>,
    /// Your identifier of the charge, up to 15 characters.
    pub seu_numero: Option<String>,
    /// Only charges of this kind.
    pub tipo_cobranca: Option<TipoCobranca>,
    /// Order of the listing; the API's default is by payer. The summary
    /// ignores it.
    pub ordenar_por: Option<OrdenarCobrancasPor>,
    /// Descending order instead of ascending. The summary ignores it.
    pub decrescente: bool,
}

impl FiltroCobrancas {
    /// Every charge of the period.
    pub fn new(data_inicial: NaiveDate, data_final: NaiveDate) -> Self {
        Self {
            data_inicial,
            data_final,
            filtrar_data_por: None,
            situacao: None,
            pessoa_pagadora: None,
            cpf_cnpj_pessoa_pagadora: None,
            seu_numero: None,
            tipo_cobranca: None,
            ordenar_por: None,
            decrescente: false,
        }
    }

    /// Checks what the API would refuse.
    pub(crate) fn validar(&self) -> Result<(), &'static str> {
        if self.data_inicial > self.data_final {
            return Err("o período das cobranças termina antes de começar");
        }
        let longo = |texto: &Option<String>, maximo: usize| {
            texto
                .as_ref()
                .is_some_and(|texto| texto.trim().is_empty() || texto.chars().count() > maximo)
        };
        if longo(&self.cpf_cnpj_pessoa_pagadora, 18) {
            return Err("o CPF/CNPJ do pagador tem até 18 caracteres");
        }
        if longo(&self.seu_numero, super::MAX_SEU_NUMERO) {
            return Err("o seu número tem até 15 caracteres");
        }
        if longo(&self.pessoa_pagadora, 100) {
            return Err("o nome do pagador tem até 100 caracteres");
        }
        Ok(())
    }

    /// Parameters shared by the listing and the summary.
    pub(crate) fn query(&self) -> Vec<(&'static str, String)> {
        let data = |d: NaiveDate| d.format("%Y-%m-%d").to_string();
        let mut query = vec![
            ("dataInicial", data(self.data_inicial)),
            ("dataFinal", data(self.data_final)),
        ];
        if let Some(por) = self.filtrar_data_por {
            query.push(("filtrarDataPor", por.as_str().to_owned()));
        }
        if let Some(situacao) = &self.situacao {
            query.push(("situacao", situacao.as_str().to_owned()));
        }
        for (nome, valor) in [
            ("pessoaPagadora", &self.pessoa_pagadora),
            ("cpfCnpjPessoaPagadora", &self.cpf_cnpj_pessoa_pagadora),
            ("seuNumero", &self.seu_numero),
        ] {
            if let Some(valor) = valor {
                query.push((nome, valor.trim().to_owned()));
            }
        }
        if let Some(tipo) = &self.tipo_cobranca {
            query.push(("tipoCobranca", tipo.as_str().to_owned()));
        }
        query
    }

    /// The order of the listing.
    pub(crate) fn ordem(&self) -> Vec<(&'static str, String)> {
        let mut query = Vec::new();
        if let Some(por) = self.ordenar_por {
            query.push(("ordenarPor", por.as_str().to_owned()));
        }
        if self.decrescente {
            query.push(("tipoOrdenacao", "DESC".to_owned()));
        }
        query
    }
}

/// The date a period of charges refers to (`FiltrarDataPorEnum`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FiltrarDataPor {
    /// `VENCIMENTO`: due date (the API's default).
    Vencimento,
    /// `EMISSAO`: day of issue.
    Emissao,
    /// `PAGAMENTO`: day of payment.
    Pagamento,
}

impl FiltrarDataPor {
    /// Every option, in the order of the API's documentation.
    pub const TODOS: [Self; 3] = [Self::Vencimento, Self::Emissao, Self::Pagamento];

    /// Code used by the API.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Vencimento => "VENCIMENTO",
            Self::Emissao => "EMISSAO",
            Self::Pagamento => "PAGAMENTO",
        }
    }
}

/// Order of the listing (`OrdenarCobrancasPorEnum`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OrdenarCobrancasPor {
    /// `PESSOA_PAGADORA`: payer (the API's default).
    PessoaPagadora,
    /// `TIPO_COBRANCA`: kind of charge.
    TipoCobranca,
    /// `CODIGO_COBRANCA`: code of the charge.
    CodigoCobranca,
    /// `IDENTIFICADOR`: your identifier.
    Identificador,
    /// `DATA_EMISSAO`: day of issue.
    DataEmissao,
    /// `DATA_VENCIMENTO`: due date.
    DataVencimento,
    /// `VALOR`: face value.
    Valor,
    /// `STATUS`: situation.
    Status,
}

impl OrdenarCobrancasPor {
    /// Every option, in the order of the API's documentation.
    pub const TODOS: [Self; 8] = [
        Self::PessoaPagadora,
        Self::TipoCobranca,
        Self::CodigoCobranca,
        Self::Identificador,
        Self::DataEmissao,
        Self::DataVencimento,
        Self::Valor,
        Self::Status,
    ];

    /// Code used by the API.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PessoaPagadora => "PESSOA_PAGADORA",
            Self::TipoCobranca => "TIPO_COBRANCA",
            Self::CodigoCobranca => "CODIGO_COBRANCA",
            Self::Identificador => "IDENTIFICADOR",
            Self::DataEmissao => "DATA_EMISSAO",
            Self::DataVencimento => "DATA_VENCIMENTO",
            Self::Valor => "VALOR",
            Self::Status => "STATUS",
        }
    }
}

/// A page of [`Cobranca::listar`](super::Cobranca::listar)
/// (`CobrancasResponse`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PaginaCobrancas {
    /// Pages available.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::u64"
    )]
    pub total_paginas: Option<u64>,
    /// Charges matching the filter, in every page.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::u64"
    )]
    pub total_elementos: Option<u64>,
    /// Size of the pages asked for.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::u64"
    )]
    pub tamanho_pagina: Option<u64>,
    /// Whether this is the first page.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::bool"
    )]
    pub primeira_pagina: Option<bool>,
    /// Whether this is the last page.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::bool"
    )]
    pub ultima_pagina: Option<bool>,
    /// Charges in this page.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::u64"
    )]
    pub numero_de_elementos: Option<u64>,
    /// The charges.
    #[serde(default, deserialize_with = "lenient::vec")]
    pub cobrancas: Vec<CobrancaDetalhada>,
}

impl PaginaCobrancas {
    /// Whether a page after `numero` (0-based) may exist.
    pub(crate) fn tem_mais(&self, numero: u32) -> bool {
        if let Some(ultima) = self.ultima_pagina {
            return !ultima;
        }
        match self.total_paginas {
            Some(total) => u64::from(numero) + 1 < total,
            None => !self.cobrancas.is_empty(),
        }
    }
}

/// Charges and amount of one situation, in
/// [`Cobranca::sumario`](super::Cobranca::sumario) (`itemSumarioCobrancas`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ItemSumario {
    /// The situation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub situacao: Option<SituacaoCobranca>,
    /// Sum of the face values.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::decimal",
        serialize_with = "decimal_as_number::serialize_option"
    )]
    pub valor: Option<Decimal>,
    /// Number of charges.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient::u64"
    )]
    pub quantidade: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dia(mes: u32, dia: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, mes, dia).unwrap()
    }

    #[test]
    fn filters_become_the_documented_parameters() {
        let mut filtro = FiltroCobrancas::new(dia(9, 1), dia(9, 30));
        assert_eq!(
            filtro.query(),
            [
                ("dataInicial", "2026-09-01".to_owned()),
                ("dataFinal", "2026-09-30".to_owned())
            ]
        );
        assert!(filtro.ordem().is_empty());
        filtro.filtrar_data_por = Some(FiltrarDataPor::Pagamento);
        filtro.situacao = Some(SituacaoCobranca::Recebido);
        filtro.pessoa_pagadora = Some(" Cliente Exemplo ".to_owned());
        filtro.cpf_cnpj_pessoa_pagadora = Some("12345678000195".to_owned());
        filtro.seu_numero = Some("NF-123".to_owned());
        filtro.tipo_cobranca = Some(TipoCobranca::Simples);
        filtro.ordenar_por = Some(OrdenarCobrancasPor::DataVencimento);
        filtro.decrescente = true;
        let query: Vec<(&str, String)> = filtro.query();
        assert_eq!(
            query[2..],
            [
                ("filtrarDataPor", "PAGAMENTO".to_owned()),
                ("situacao", "RECEBIDO".to_owned()),
                ("pessoaPagadora", "Cliente Exemplo".to_owned()),
                ("cpfCnpjPessoaPagadora", "12345678000195".to_owned()),
                ("seuNumero", "NF-123".to_owned()),
                ("tipoCobranca", "SIMPLES".to_owned()),
            ]
        );
        assert_eq!(
            filtro.ordem(),
            [
                ("ordenarPor", "DATA_VENCIMENTO".to_owned()),
                ("tipoOrdenacao", "DESC".to_owned())
            ]
        );
        assert_eq!(filtro.validar(), Ok(()));
    }

    #[test]
    fn invalid_filters_are_refused() {
        let invertido = FiltroCobrancas::new(dia(9, 30), dia(9, 1));
        assert!(invertido.validar().is_err());
        let mut longo = FiltroCobrancas::new(dia(9, 1), dia(9, 30));
        longo.seu_numero = Some("1234567890123456".to_owned());
        assert!(longo.validar().is_err());
        let mut vazio = FiltroCobrancas::new(dia(9, 1), dia(9, 30));
        vazio.cpf_cnpj_pessoa_pagadora = Some(" ".to_owned());
        assert!(vazio.validar().is_err());
    }

    #[test]
    fn the_last_page_is_recognized() {
        let pagina = |ultima: Option<bool>, total: Option<u64>, itens: usize| PaginaCobrancas {
            ultima_pagina: ultima,
            total_paginas: total,
            cobrancas: vec![CobrancaDetalhada::default(); itens],
            ..PaginaCobrancas::default()
        };
        assert!(!pagina(Some(true), Some(5), 10).tem_mais(0));
        assert!(pagina(Some(false), None, 10).tem_mais(0));
        assert!(pagina(None, Some(3), 10).tem_mais(1));
        assert!(!pagina(None, Some(3), 10).tem_mais(2));
        assert!(pagina(None, None, 1).tem_mais(0));
        assert!(!pagina(None, None, 0).tem_mais(0));
    }
}
