use std::fmt;
use std::str::FromStr;

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Serialize, Serializer};

use crate::documento::Documento;
use crate::serde_util::decimal_as_number;

/// Smallest face value the API accepts: R$ 2,50.
pub const VALOR_MINIMO: Decimal = Decimal::from_parts(250, 0, 0, false, 2);

/// Largest face value the API accepts: R$ 99.999.999,99.
pub const VALOR_MAXIMO: Decimal = Decimal::from_parts(1_410_065_407, 2, 0, false, 2);

/// Longest [`EmissaoCobranca::seu_numero`].
pub const MAX_SEU_NUMERO: usize = 15;

/// Most days after the due date before an unpaid charge is cancelled
/// ([`EmissaoCobranca::num_dias_agenda`]).
pub const MAX_DIAS_AGENDA: u32 = 60;

/// Lines of [`EmissaoCobranca::mensagem`] printed on the boleto.
pub const MAX_LINHAS_MENSAGEM: usize = 5;

/// Longest line of [`EmissaoCobranca::mensagem`].
pub const MAX_CARACTERES_LINHA: usize = 78;

/// A charge to issue: a boleto with a Pix QR Code, sent with
/// [`Cobranca::emitir`](super::Cobranca::emitir) (`EmitirCobrancaRequestBody`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmissaoCobranca {
    /// Your identifier of the charge (*seu número*), up to
    /// [`MAX_SEU_NUMERO`] characters.
    pub seu_numero: String,
    /// Face value, from [`VALOR_MINIMO`] to [`VALOR_MAXIMO`].
    #[serde(serialize_with = "decimal_as_number::serialize")]
    pub valor_nominal: Decimal,
    /// Due date.
    pub data_vencimento: NaiveDate,
    /// Days after the due date until the charge is cancelled if unpaid, up
    /// to [`MAX_DIAS_AGENDA`] (0: the day after the due date).
    pub num_dias_agenda: u32,
    /// Who pays.
    pub pagador: Pagador,
    /// Discount for paying early.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desconto: Option<Desconto>,
    /// Fine for paying late.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multa: Option<Multa>,
    /// Interest for paying late (*mora*).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mora: Option<Mora>,
    /// Lines printed on the boleto, up to [`MAX_LINHAS_MENSAGEM`] of
    /// [`MAX_CARACTERES_LINHA`] characters.
    #[serde(
        skip_serializing_if = "Vec::is_empty",
        serialize_with = "serialize_mensagem"
    )]
    pub mensagem: Vec<String>,
    /// Who actually receives, when not the account holder.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub beneficiario_final: Option<BeneficiarioFinal>,
    /// How the charge can be paid; empty means the API's default, boleto
    /// and Pix (Pix only when the account has a Pix key).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub formas_recebimento: Vec<FormaRecebimento>,
    /// Invoice the charge refers to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nota_fiscal: Option<NotaFiscal>,
}

impl EmissaoCobranca {
    /// A charge with only the required fields.
    pub fn new(
        seu_numero: impl Into<String>,
        valor_nominal: Decimal,
        data_vencimento: NaiveDate,
        pagador: Pagador,
    ) -> Self {
        Self {
            seu_numero: seu_numero.into(),
            valor_nominal,
            data_vencimento,
            num_dias_agenda: 0,
            pagador,
            desconto: None,
            multa: None,
            mora: None,
            mensagem: Vec::new(),
            beneficiario_final: None,
            formas_recebimento: Vec::new(),
            nota_fiscal: None,
        }
    }

    /// Checks what can be checked before sending, as
    /// [`Cobranca::emitir`](super::Cobranca::emitir) does. Dates are not
    /// compared with today: that depends on the clock of whoever calls.
    ///
    /// # Errors
    ///
    /// Returns the first problem found.
    pub fn validar(&self) -> Result<(), EmissaoCobrancaError> {
        use EmissaoCobrancaError as E;
        texto(Some(&self.seu_numero), "seuNumero", 1, MAX_SEU_NUMERO)?;
        if self.valor_nominal < VALOR_MINIMO
            || self.valor_nominal > VALOR_MAXIMO
            || !centavos(self.valor_nominal)
        {
            return Err(E::ValorNominal);
        }
        if self.num_dias_agenda > MAX_DIAS_AGENDA {
            return Err(E::DiasAgenda);
        }
        self.pagador.validar()?;
        if let Some(desconto) = &self.desconto {
            desconto.validar(self.valor_nominal)?;
        }
        if let Some(multa) = &self.multa {
            multa.validar(self.valor_nominal)?;
        }
        if let Some(mora) = &self.mora {
            mora.validar(self.valor_nominal)?;
        }
        if self.mensagem.len() > MAX_LINHAS_MENSAGEM {
            return Err(E::LinhasMensagem);
        }
        for (linha, campo) in self.mensagem.iter().zip(LINHAS) {
            texto(Some(linha), campo, 0, MAX_CARACTERES_LINHA)?;
        }
        if let Some(beneficiario) = &self.beneficiario_final {
            beneficiario.validar()?;
        }
        let formas = &self.formas_recebimento;
        let repetida = formas
            .iter()
            .enumerate()
            .any(|(i, forma)| formas[..i].contains(forma));
        let sem_forma = formas.contains(&FormaRecebimento::SemFormaPagamento);
        if repetida || (sem_forma && formas.len() > 1) {
            return Err(E::FormasRecebimento);
        }
        if let Some(nota) = &self.nota_fiscal {
            nota.validar()?;
        }
        Ok(())
    }
}

const LINHAS: [&str; MAX_LINHAS_MENSAGEM] = [
    "mensagem.linha1",
    "mensagem.linha2",
    "mensagem.linha3",
    "mensagem.linha4",
    "mensagem.linha5",
];

#[allow(clippy::ptr_arg)] // signature imposed by `serialize_with`
fn serialize_mensagem<S: Serializer>(
    linhas: &Vec<String>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeMap;
    let mut map = serializer.serialize_map(Some(linhas.len()))?;
    for (linha, campo) in linhas.iter().zip(LINHAS) {
        map.serialize_entry(campo.trim_start_matches("mensagem."), linha)?;
    }
    map.end()
}

/// Kind of person, derived from the document: CPF for people, CNPJ for
/// companies (`tipoPessoa`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TipoPessoa {
    /// `FISICA`: a person, with a CPF.
    Fisica,
    /// `JURIDICA`: a company, with a CNPJ.
    Juridica,
}

impl TipoPessoa {
    /// Kind of the holder of `documento`.
    pub fn de(documento: &Documento) -> Self {
        match documento {
            Documento::Cpf(_) => Self::Fisica,
            Documento::Cnpj(_) => Self::Juridica,
        }
    }

    /// Code used by the API.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fisica => "FISICA",
            Self::Juridica => "JURIDICA",
        }
    }
}

/// Who pays a charge (`Pagador`). The kind of person (`tipoPessoa`) is
/// derived from the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pagador {
    /// CPF or CNPJ.
    pub cpf_cnpj: Documento,
    /// Name, up to 100 characters.
    pub nome: String,
    /// Street, up to 100 characters.
    pub endereco: String,
    /// Number in the street, up to 10 characters.
    pub numero: Option<String>,
    /// Complement of the address, up to 30 characters.
    pub complemento: Option<String>,
    /// District, up to 60 characters.
    pub bairro: Option<String>,
    /// City, up to 60 characters.
    pub cidade: String,
    /// State.
    pub uf: Uf,
    /// Postal code, 8 digits.
    pub cep: String,
    /// E-mail, up to 50 characters.
    pub email: Option<String>,
    /// Area code of the phone, 2 digits.
    pub ddd: Option<String>,
    /// Phone, 8 or 9 digits.
    pub telefone: Option<String>,
}

impl Pagador {
    /// A payer with only the required fields.
    pub fn new(
        cpf_cnpj: Documento,
        nome: impl Into<String>,
        endereco: impl Into<String>,
        cidade: impl Into<String>,
        uf: Uf,
        cep: impl Into<String>,
    ) -> Self {
        Self {
            cpf_cnpj,
            nome: nome.into(),
            endereco: endereco.into(),
            numero: None,
            complemento: None,
            bairro: None,
            cidade: cidade.into(),
            uf,
            cep: cep.into(),
            email: None,
            ddd: None,
            telefone: None,
        }
    }

    fn validar(&self) -> Result<(), EmissaoCobrancaError> {
        use EmissaoCobrancaError as E;
        texto(Some(&self.nome), "pagador.nome", 1, 100)?;
        texto(Some(&self.endereco), "pagador.endereco", 1, 100)?;
        texto(self.numero.as_ref(), "pagador.numero", 1, 10)?;
        texto(self.complemento.as_ref(), "pagador.complemento", 1, 30)?;
        texto(self.bairro.as_ref(), "pagador.bairro", 1, 60)?;
        texto(Some(&self.cidade), "pagador.cidade", 1, 60)?;
        if !digitos(&self.cep, 8..=8) {
            return Err(E::Cep {
                campo: "pagador.cep",
            });
        }
        if let Some(email) = &self.email {
            texto(Some(email), "pagador.email", 3, 50)?;
            let (local, dominio) = email.split_once('@').unwrap_or_default();
            if local.is_empty() || !dominio.contains('.') || email.contains(char::is_whitespace) {
                return Err(E::Email);
            }
        }
        if self.ddd.as_deref().is_some_and(|ddd| !digitos(ddd, 2..=2)) {
            return Err(E::Ddd);
        }
        if self
            .telefone
            .as_deref()
            .is_some_and(|telefone| !digitos(telefone, 8..=9))
        {
            return Err(E::Telefone);
        }
        if self.telefone.is_some() != self.ddd.is_some() {
            return Err(E::Ddd);
        }
        Ok(())
    }
}

impl Serialize for Pagador {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Wire<'a> {
            cpf_cnpj: &'a str,
            tipo_pessoa: &'static str,
            nome: &'a str,
            endereco: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            numero: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            complemento: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            bairro: Option<&'a str>,
            cidade: &'a str,
            uf: Uf,
            cep: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            email: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            ddd: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            telefone: Option<&'a str>,
        }
        Wire {
            cpf_cnpj: self.cpf_cnpj.as_str(),
            tipo_pessoa: TipoPessoa::de(&self.cpf_cnpj).as_str(),
            nome: &self.nome,
            endereco: &self.endereco,
            numero: self.numero.as_deref(),
            complemento: self.complemento.as_deref(),
            bairro: self.bairro.as_deref(),
            cidade: &self.cidade,
            uf: self.uf,
            cep: &self.cep,
            email: self.email.as_deref(),
            ddd: self.ddd.as_deref(),
            telefone: self.telefone.as_deref(),
        }
        .serialize(serializer)
    }
}

/// Who actually receives a charge, when not the account holder
/// (`BeneficiarioBase`). The kind of person is derived from the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeneficiarioFinal {
    /// CPF or CNPJ.
    pub cpf_cnpj: Documento,
    /// Name, up to 100 characters.
    pub nome: String,
    /// Street, up to 100 characters.
    pub endereco: String,
    /// District, up to 60 characters.
    pub bairro: Option<String>,
    /// City, up to 60 characters.
    pub cidade: String,
    /// State.
    pub uf: Uf,
    /// Postal code, 8 digits.
    pub cep: String,
}

impl BeneficiarioFinal {
    fn validar(&self) -> Result<(), EmissaoCobrancaError> {
        texto(Some(&self.nome), "beneficiarioFinal.nome", 1, 100)?;
        texto(Some(&self.endereco), "beneficiarioFinal.endereco", 1, 100)?;
        texto(self.bairro.as_ref(), "beneficiarioFinal.bairro", 1, 60)?;
        texto(Some(&self.cidade), "beneficiarioFinal.cidade", 1, 60)?;
        if !digitos(&self.cep, 8..=8) {
            return Err(EmissaoCobrancaError::Cep {
                campo: "beneficiarioFinal.cep",
            });
        }
        Ok(())
    }
}

impl Serialize for BeneficiarioFinal {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Wire<'a> {
            cpf_cnpj: &'a str,
            tipo_pessoa: &'static str,
            nome: &'a str,
            endereco: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            bairro: Option<&'a str>,
            cidade: &'a str,
            uf: Uf,
            cep: &'a str,
        }
        Wire {
            cpf_cnpj: self.cpf_cnpj.as_str(),
            tipo_pessoa: TipoPessoa::de(&self.cpf_cnpj).as_str(),
            nome: &self.nome,
            endereco: &self.endereco,
            bairro: self.bairro.as_deref(),
            cidade: &self.cidade,
            uf: self.uf,
            cep: &self.cep,
        }
        .serialize(serializer)
    }
}

/// Discount for paying early (`DescontoTaxa`, `DescontoValor`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Desconto {
    /// `PERCENTUALDATAINFORMADA`: a percentage of the face value, for
    /// payments up to `quantidade_dias` days before the due date.
    Percentual {
        /// Percentage, with up to 2 decimals.
        taxa: Decimal,
        /// Days before the due date.
        quantidade_dias: u32,
    },
    /// `VALORFIXODATAINFORMADA`: a fixed amount, for payments up to
    /// `quantidade_dias` days before the due date.
    ValorFixo {
        /// Amount, with up to 2 decimals.
        valor: Decimal,
        /// Days before the due date.
        quantidade_dias: u32,
    },
}

impl Desconto {
    /// Days before the due date until which the discount applies.
    pub fn quantidade_dias(&self) -> u32 {
        match *self {
            Self::Percentual {
                quantidade_dias, ..
            }
            | Self::ValorFixo {
                quantidade_dias, ..
            } => quantidade_dias,
        }
    }

    fn validar(&self, valor_nominal: Decimal) -> Result<(), EmissaoCobrancaError> {
        match *self {
            Self::Percentual { taxa, .. } => percentual(taxa, "desconto.taxa"),
            Self::ValorFixo { valor, .. } => menor_que(valor, valor_nominal, "desconto.valor"),
        }
    }
}

impl Serialize for Desconto {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let (codigo, quantidade_dias, taxa, valor) = match *self {
            Self::Percentual {
                taxa,
                quantidade_dias,
            } => ("PERCENTUALDATAINFORMADA", quantidade_dias, Some(taxa), None),
            Self::ValorFixo {
                valor,
                quantidade_dias,
            } => ("VALORFIXODATAINFORMADA", quantidade_dias, None, Some(valor)),
        };
        Encargo {
            codigo,
            quantidade_dias: Some(quantidade_dias),
            taxa,
            valor,
        }
        .serialize(serializer)
    }
}

/// Fine for paying late (`MultaTaxa`, `MultaValor`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Multa {
    /// `PERCENTUAL`: a percentage of the face value.
    Percentual {
        /// Percentage, with up to 2 decimals.
        taxa: Decimal,
    },
    /// `VALORFIXO`: a fixed amount.
    ValorFixo {
        /// Amount, with up to 2 decimals.
        valor: Decimal,
    },
}

impl Multa {
    fn validar(&self, valor_nominal: Decimal) -> Result<(), EmissaoCobrancaError> {
        match *self {
            Self::Percentual { taxa } => percentual(taxa, "multa.taxa"),
            Self::ValorFixo { valor } => menor_que(valor, valor_nominal, "multa.valor"),
        }
    }
}

impl Serialize for Multa {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let (codigo, taxa, valor) = match *self {
            Self::Percentual { taxa } => ("PERCENTUAL", Some(taxa), None),
            Self::ValorFixo { valor } => ("VALORFIXO", None, Some(valor)),
        };
        Encargo {
            codigo,
            quantidade_dias: None,
            taxa,
            valor,
        }
        .serialize(serializer)
    }
}

/// Interest for paying late (`MoraTaxa`, `MoraValor`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mora {
    /// `TAXAMENSAL`: a percentage of the face value per month.
    TaxaMensal {
        /// Percentage per month, with up to 2 decimals.
        taxa: Decimal,
    },
    /// `VALORDIA`: a fixed amount per day.
    ValorDia {
        /// Amount per day, with up to 2 decimals.
        valor: Decimal,
    },
}

impl Mora {
    fn validar(&self, valor_nominal: Decimal) -> Result<(), EmissaoCobrancaError> {
        match *self {
            Self::TaxaMensal { taxa } => percentual(taxa, "mora.taxa"),
            Self::ValorDia { valor } => menor_que(valor, valor_nominal, "mora.valor"),
        }
    }
}

impl Serialize for Mora {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let (codigo, taxa, valor) = match *self {
            Self::TaxaMensal { taxa } => ("TAXAMENSAL", Some(taxa), None),
            Self::ValorDia { valor } => ("VALORDIA", None, Some(valor)),
        };
        Encargo {
            codigo,
            quantidade_dias: None,
            taxa,
            valor,
        }
        .serialize(serializer)
    }
}

/// The shape shared by discounts, fines and interest in the API.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Encargo {
    codigo: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    quantidade_dias: Option<u32>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "decimal_as_number::serialize_option"
    )]
    taxa: Option<Decimal>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "decimal_as_number::serialize_option"
    )]
    valor: Option<Decimal>,
}

/// How a charge can be paid (`FormaRecebimentoEnum`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FormaRecebimento {
    /// `BOLETO`: barcode.
    Boleto,
    /// `PIX`: QR Code, when the account has a Pix key.
    Pix,
    /// `SEM_FORMA_PAGAMENTO`: neither; alone.
    SemFormaPagamento,
}

impl FormaRecebimento {
    /// Code used by the API.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Boleto => "BOLETO",
            Self::Pix => "PIX",
            Self::SemFormaPagamento => "SEM_FORMA_PAGAMENTO",
        }
    }
}

impl Serialize for FormaRecebimento {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// The invoice (NF-e) a charge refers to (`NotaFiscal`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotaFiscal {
    /// Access key, 44 digits with a valid check digit.
    #[serde(rename = "chaveNFe")]
    pub chave_nfe: String,
    /// Number of the invoice, the one in the access key.
    pub numero: u32,
    /// Series of the invoice, the one in the access key.
    pub serie: u32,
    /// Day the invoice was issued.
    pub data_emissao: NaiveDate,
    /// Installment, when the invoice is paid in parts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parcela: Option<u32>,
    /// Nature of the operation (*natureza da operação*).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub natureza_operacao: Option<String>,
}

impl NotaFiscal {
    fn validar(&self) -> Result<(), EmissaoCobrancaError> {
        use EmissaoCobrancaError as E;
        let chave = &self.chave_nfe;
        if !digitos(chave, 44..=44) || !dv_chave_nfe(chave) {
            return Err(E::ChaveNfe);
        }
        // The key carries the series (3 digits) and the number (9 digits).
        let serie: u32 = chave[22..25].parse().unwrap_or(u32::MAX);
        let numero: u32 = chave[25..34].parse().unwrap_or(u32::MAX);
        if self.serie != serie || self.numero != numero {
            return Err(E::NumeroNfe { numero, serie });
        }
        texto(
            self.natureza_operacao.as_ref(),
            "notaFiscal.naturezaOperacao",
            1,
            60,
        )
    }
}

/// Check digit of an NF-e access key: modulo 11 with weights 2 to 9 from
/// the right.
fn dv_chave_nfe(chave: &str) -> bool {
    let (base, dv) = chave.split_at(43);
    let soma: u32 = base
        .bytes()
        .rev()
        .zip([2, 3, 4, 5, 6, 7, 8, 9].into_iter().cycle())
        .map(|(digito, peso)| u32::from(digito - b'0') * peso)
        .sum();
    let resto = soma % 11;
    let esperado = if resto < 2 { 0 } else { 11 - resto };
    dv.bytes().next().map(|b| u32::from(b - b'0')) == Some(esperado)
}

/// The 27 federative units (`EnumUF`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum Uf {
    Ac,
    Al,
    Ap,
    Am,
    Ba,
    Ce,
    Df,
    Es,
    Go,
    Ma,
    Mt,
    Ms,
    Mg,
    Pa,
    Pb,
    Pr,
    Pe,
    Pi,
    Rj,
    Rn,
    Rs,
    Ro,
    Rr,
    Sc,
    Sp,
    Se,
    To,
}

impl Uf {
    /// Every federative unit, in the order of the API's documentation.
    pub const TODAS: [Uf; 27] = [
        Self::Ac,
        Self::Al,
        Self::Ap,
        Self::Am,
        Self::Ba,
        Self::Ce,
        Self::Df,
        Self::Es,
        Self::Go,
        Self::Ma,
        Self::Mt,
        Self::Ms,
        Self::Mg,
        Self::Pa,
        Self::Pb,
        Self::Pr,
        Self::Pe,
        Self::Pi,
        Self::Rj,
        Self::Rn,
        Self::Rs,
        Self::Ro,
        Self::Rr,
        Self::Sc,
        Self::Sp,
        Self::Se,
        Self::To,
    ];

    /// Two-letter code (`SP`).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ac => "AC",
            Self::Al => "AL",
            Self::Ap => "AP",
            Self::Am => "AM",
            Self::Ba => "BA",
            Self::Ce => "CE",
            Self::Df => "DF",
            Self::Es => "ES",
            Self::Go => "GO",
            Self::Ma => "MA",
            Self::Mt => "MT",
            Self::Ms => "MS",
            Self::Mg => "MG",
            Self::Pa => "PA",
            Self::Pb => "PB",
            Self::Pr => "PR",
            Self::Pe => "PE",
            Self::Pi => "PI",
            Self::Rj => "RJ",
            Self::Rn => "RN",
            Self::Rs => "RS",
            Self::Ro => "RO",
            Self::Rr => "RR",
            Self::Sc => "SC",
            Self::Sp => "SP",
            Self::Se => "SE",
            Self::To => "TO",
        }
    }
}

impl FromStr for Uf {
    type Err = UfError;

    /// Accepts the two-letter code in any case (`sp`, `SP`).
    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let raw = raw.trim();
        Self::TODAS
            .into_iter()
            .find(|uf| uf.as_str().eq_ignore_ascii_case(raw))
            .ok_or(UfError)
    }
}

impl fmt::Display for Uf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for Uf {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// A text that is not the code of a federative unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("UF inválida: use a sigla do estado (ex.: SP, MG, DF)")]
pub struct UfError;

/// Why an [`EmissaoCobranca`] cannot be sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum EmissaoCobrancaError {
    /// A text is blank, too long or has control characters.
    #[error(
        "{} deve ter de {minimo} a {maximo} caracteres, sem quebras de linha",
        rotulo(campo)
    )]
    Texto {
        /// The field, by its path in the API (`pagador.nome`).
        campo: &'static str,
        /// Shortest accepted.
        minimo: usize,
        /// Longest accepted.
        maximo: usize,
    },
    /// The face value is out of the accepted range or has fractions of a
    /// cent.
    #[error("o valor da cobrança deve ser de R$ 2,50 a R$ 99.999.999,99, com até 2 casas decimais")]
    ValorNominal,
    /// Too many days until the automatic cancellation.
    #[error("o prazo para cancelamento automático vai de 0 a 60 dias após o vencimento")]
    DiasAgenda,
    /// The postal code is not 8 digits.
    #[error("o CEP tem 8 dígitos")]
    Cep {
        /// `pagador.cep` or `beneficiarioFinal.cep`.
        campo: &'static str,
    },
    /// The e-mail is not an address.
    #[error("e-mail inválido")]
    Email,
    /// The area code is not 2 digits, or only one of area code and phone
    /// was given.
    #[error("o DDD tem 2 dígitos e acompanha o telefone")]
    Ddd,
    /// The phone is not 8 or 9 digits.
    #[error("o telefone tem 8 ou 9 dígitos, sem o DDD")]
    Telefone,
    /// A percentage is not positive, above 100% or has more than 2
    /// decimals.
    #[error("a taxa deve ser maior que zero e até 100%, com até 2 casas decimais")]
    Taxa {
        /// `desconto.taxa`, `multa.taxa` or `mora.taxa`.
        campo: &'static str,
    },
    /// An amount is not positive, not below the face value or has
    /// fractions of a cent.
    #[error("o valor deve ser maior que zero e menor que o da cobrança, com até 2 casas decimais")]
    Valor {
        /// `desconto.valor`, `multa.valor` or `mora.valor`.
        campo: &'static str,
    },
    /// More message lines than the boleto prints.
    #[error("a mensagem tem no máximo 5 linhas")]
    LinhasMensagem,
    /// A payment method repeated, or "none" together with another.
    #[error("formas de recebimento repetidas, ou SEM_FORMA_PAGAMENTO junto com outra")]
    FormasRecebimento,
    /// The access key of the invoice is not 44 digits with a valid check
    /// digit.
    #[error("a chave de acesso da nota fiscal tem 44 dígitos, e o último confere os demais")]
    ChaveNfe,
    /// The number or series of the invoice differ from the ones in its
    /// access key.
    #[error(
        "número e série da nota fiscal diferem dos da chave de acesso (número {numero}, série {serie})"
    )]
    NumeroNfe {
        /// Number in the access key.
        numero: u32,
        /// Series in the access key.
        serie: u32,
    },
}

impl EmissaoCobrancaError {
    /// The field with the problem, by its path in the API (`valorNominal`,
    /// `pagador.cep`...).
    pub fn campo(&self) -> &'static str {
        match self {
            Self::Texto { campo, .. }
            | Self::Cep { campo }
            | Self::Taxa { campo }
            | Self::Valor { campo } => campo,
            Self::ValorNominal => "valorNominal",
            Self::DiasAgenda => "numDiasAgenda",
            Self::Email => "pagador.email",
            Self::Ddd => "pagador.ddd",
            Self::Telefone => "pagador.telefone",
            Self::LinhasMensagem => "mensagem",
            Self::FormasRecebimento => "formasRecebimento",
            Self::ChaveNfe => "notaFiscal.chaveNFe",
            Self::NumeroNfe { .. } => "notaFiscal.numero",
        }
    }
}

fn rotulo(campo: &str) -> &'static str {
    match campo {
        "seuNumero" => "o seu número",
        "pagador.nome" | "beneficiarioFinal.nome" => "o nome",
        "pagador.endereco" | "beneficiarioFinal.endereco" => "o endereço",
        "pagador.numero" => "o número do endereço",
        "pagador.complemento" => "o complemento",
        "pagador.bairro" | "beneficiarioFinal.bairro" => "o bairro",
        "pagador.cidade" | "beneficiarioFinal.cidade" => "a cidade",
        "pagador.email" => "o e-mail",
        "notaFiscal.naturezaOperacao" => "a natureza da operação",
        _ => "cada linha da mensagem",
    }
}

/// `Some(texto)` has from `minimo` to `maximo` characters and no control
/// characters; `None` passes.
fn texto(
    texto: Option<&String>,
    campo: &'static str,
    minimo: usize,
    maximo: usize,
) -> Result<(), EmissaoCobrancaError> {
    match texto {
        Some(texto)
            if texto.trim().chars().count() < minimo
                || texto.chars().count() > maximo
                || texto.chars().any(char::is_control) =>
        {
            Err(EmissaoCobrancaError::Texto {
                campo,
                minimo,
                maximo,
            })
        }
        _ => Ok(()),
    }
}

fn digitos(texto: &str, tamanho: std::ops::RangeInclusive<usize>) -> bool {
    tamanho.contains(&texto.len()) && texto.bytes().all(|b| b.is_ascii_digit())
}

fn centavos(valor: Decimal) -> bool {
    valor.normalize().scale() <= 2
}

fn percentual(taxa: Decimal, campo: &'static str) -> Result<(), EmissaoCobrancaError> {
    if taxa <= Decimal::ZERO || taxa > Decimal::ONE_HUNDRED || !centavos(taxa) {
        return Err(EmissaoCobrancaError::Taxa { campo });
    }
    Ok(())
}

fn menor_que(
    valor: Decimal,
    valor_nominal: Decimal,
    campo: &'static str,
) -> Result<(), EmissaoCobrancaError> {
    if valor <= Decimal::ZERO || valor >= valor_nominal || !centavos(valor) {
        return Err(EmissaoCobrancaError::Valor { campo });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn dia(ano: i32, mes: u32, dia: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(ano, mes, dia).unwrap()
    }

    fn dec(texto: &str) -> Decimal {
        texto.parse().unwrap()
    }

    fn pagador() -> Pagador {
        Pagador::new(
            "123.456.789-09".parse().unwrap(),
            "Cliente Exemplo",
            "Avenida Brasil",
            "Belo Horizonte",
            Uf::Mg,
            "30110000",
        )
    }

    fn cobranca() -> EmissaoCobranca {
        EmissaoCobranca::new("NF-123", dec("150.00"), dia(2026, 10, 20), pagador())
    }

    /// A synthetic NF-e key (MG, 2026-09, series 1, number 12345).
    const CHAVE: &str = "31260912345678000195550010000123451123456786";

    fn nota() -> NotaFiscal {
        NotaFiscal {
            chave_nfe: CHAVE.to_owned(),
            numero: 12_345,
            serie: 1,
            data_emissao: dia(2026, 9, 15),
            parcela: Some(1),
            natureza_operacao: Some("Venda".to_owned()),
        }
    }

    #[test]
    fn limits_are_the_documented_ones() {
        assert_eq!(VALOR_MINIMO, dec("2.50"));
        assert_eq!(VALOR_MAXIMO, dec("99999999.99"));
    }

    #[test]
    fn minimal_charge_serializes_the_required_fields() {
        let json = serde_json::to_value(cobranca()).unwrap();
        assert_eq!(
            json,
            json!({
                "seuNumero": "NF-123",
                "valorNominal": 150,
                "dataVencimento": "2026-10-20",
                "numDiasAgenda": 0,
                "pagador": {
                    "cpfCnpj": "12345678909",
                    "tipoPessoa": "FISICA",
                    "nome": "Cliente Exemplo",
                    "endereco": "Avenida Brasil",
                    "cidade": "Belo Horizonte",
                    "uf": "MG",
                    "cep": "30110000"
                }
            })
        );
        assert_eq!(cobranca().validar(), Ok(()));
    }

    #[test]
    fn full_charge_serializes_every_field() {
        let mut completa = cobranca();
        completa.valor_nominal = dec("1234.56");
        completa.num_dias_agenda = 30;
        completa.pagador.cpf_cnpj = "12.345.678/0001-95".parse().unwrap();
        completa.pagador.numero = Some("1200".to_owned());
        completa.pagador.complemento = Some("sala 3".to_owned());
        completa.pagador.bairro = Some("Centro".to_owned());
        completa.pagador.email = Some("financeiro@exemplo.com.br".to_owned());
        completa.pagador.ddd = Some("31".to_owned());
        completa.pagador.telefone = Some("999999999".to_owned());
        completa.desconto = Some(Desconto::Percentual {
            taxa: dec("2.5"),
            quantidade_dias: 5,
        });
        completa.multa = Some(Multa::ValorFixo { valor: dec("10") });
        completa.mora = Some(Mora::TaxaMensal { taxa: dec("1") });
        completa.mensagem = vec!["Referente à NF 12345".to_owned(), "Obrigado".to_owned()];
        completa.beneficiario_final = Some(BeneficiarioFinal {
            cpf_cnpj: "123.456.789-09".parse().unwrap(),
            nome: "Beneficiário Exemplo".to_owned(),
            endereco: "Rua Exemplo".to_owned(),
            bairro: None,
            cidade: "São Paulo".to_owned(),
            uf: Uf::Sp,
            cep: "01001000".to_owned(),
        });
        completa.formas_recebimento = vec![FormaRecebimento::Boleto, FormaRecebimento::Pix];
        completa.nota_fiscal = Some(nota());
        assert_eq!(completa.validar(), Ok(()));
        let json = serde_json::to_value(&completa).unwrap();
        assert_eq!(json["pagador"]["tipoPessoa"], "JURIDICA");
        assert_eq!(json["pagador"]["telefone"], "999999999");
        assert_eq!(
            json["desconto"],
            json!({"codigo": "PERCENTUALDATAINFORMADA", "quantidadeDias": 5, "taxa": 2.5})
        );
        assert_eq!(json["multa"], json!({"codigo": "VALORFIXO", "valor": 10}));
        assert_eq!(json["mora"], json!({"codigo": "TAXAMENSAL", "taxa": 1}));
        assert_eq!(
            json["mensagem"],
            json!({"linha1": "Referente à NF 12345", "linha2": "Obrigado"})
        );
        assert_eq!(
            json["beneficiarioFinal"],
            json!({
                "cpfCnpj": "12345678909",
                "tipoPessoa": "FISICA",
                "nome": "Beneficiário Exemplo",
                "endereco": "Rua Exemplo",
                "cidade": "São Paulo",
                "uf": "SP",
                "cep": "01001000"
            })
        );
        assert_eq!(json["formasRecebimento"], json!(["BOLETO", "PIX"]));
        assert_eq!(
            json["notaFiscal"],
            json!({
                "chaveNFe": CHAVE,
                "numero": 12345,
                "serie": 1,
                "dataEmissao": "2026-09-15",
                "parcela": 1,
                "naturezaOperacao": "Venda"
            })
        );
    }

    #[test]
    fn every_kind_of_charge_serializes_its_code() {
        for (desconto, esperado) in [
            (
                Desconto::ValorFixo {
                    valor: dec("5.00"),
                    quantidade_dias: 0,
                },
                json!({"codigo": "VALORFIXODATAINFORMADA", "quantidadeDias": 0, "valor": 5}),
            ),
            (
                Desconto::Percentual {
                    taxa: dec("3"),
                    quantidade_dias: 7,
                },
                json!({"codigo": "PERCENTUALDATAINFORMADA", "quantidadeDias": 7, "taxa": 3}),
            ),
        ] {
            assert_eq!(serde_json::to_value(desconto).unwrap(), esperado);
        }
        assert_eq!(
            serde_json::to_value(Multa::Percentual { taxa: dec("2") }).unwrap(),
            json!({"codigo": "PERCENTUAL", "taxa": 2})
        );
        assert_eq!(
            serde_json::to_value(Mora::ValorDia { valor: dec("0.33") }).unwrap(),
            json!({"codigo": "VALORDIA", "valor": 0.33})
        );
    }

    type Alteracao = fn(&mut EmissaoCobranca);

    #[test]
    fn invalid_charges_name_the_field() {
        let casos: Vec<(Alteracao, &str)> = vec![
            (|c| c.seu_numero = String::new(), "seuNumero"),
            (
                |c| c.seu_numero = "1234567890123456".to_owned(),
                "seuNumero",
            ),
            (|c| c.seu_numero = "NF\n1".to_owned(), "seuNumero"),
            (|c| c.valor_nominal = dec("2.49"), "valorNominal"),
            (|c| c.valor_nominal = dec("100000000"), "valorNominal"),
            (|c| c.valor_nominal = dec("10.001"), "valorNominal"),
            (|c| c.num_dias_agenda = 61, "numDiasAgenda"),
            (|c| c.pagador.nome = " ".to_owned(), "pagador.nome"),
            (|c| c.pagador.endereco = "a".repeat(101), "pagador.endereco"),
            (|c| c.pagador.cidade = String::new(), "pagador.cidade"),
            (
                |c| c.pagador.numero = Some("12345678901".to_owned()),
                "pagador.numero",
            ),
            (|c| c.pagador.cep = "30110-000".to_owned(), "pagador.cep"),
            (
                |c| c.pagador.email = Some("sem-arroba".to_owned()),
                "pagador.email",
            ),
            (|c| c.pagador.ddd = Some("031".to_owned()), "pagador.ddd"),
            (
                |c| {
                    c.pagador.ddd = Some("31".to_owned());
                    c.pagador.telefone = Some("1234567".to_owned());
                },
                "pagador.telefone",
            ),
            (
                |c| c.pagador.telefone = Some("999999999".to_owned()),
                "pagador.ddd",
            ),
            (
                |c| {
                    c.desconto = Some(Desconto::Percentual {
                        taxa: dec("101"),
                        quantidade_dias: 1,
                    });
                },
                "desconto.taxa",
            ),
            (
                |c| {
                    c.desconto = Some(Desconto::ValorFixo {
                        valor: dec("150"),
                        quantidade_dias: 1,
                    });
                },
                "desconto.valor",
            ),
            (
                |c| c.multa = Some(Multa::Percentual { taxa: dec("0") }),
                "multa.taxa",
            ),
            (
                |c| {
                    c.multa = Some(Multa::ValorFixo {
                        valor: dec("1.555"),
                    });
                },
                "multa.valor",
            ),
            (
                |c| c.mora = Some(Mora::TaxaMensal { taxa: dec("-1") }),
                "mora.taxa",
            ),
            (
                |c| c.mora = Some(Mora::ValorDia { valor: dec("0") }),
                "mora.valor",
            ),
            (|c| c.mensagem = vec![String::new(); 6], "mensagem"),
            (|c| c.mensagem = vec!["a".repeat(79)], "mensagem.linha1"),
            (
                |c| {
                    c.formas_recebimento = vec![FormaRecebimento::Pix, FormaRecebimento::Pix];
                },
                "formasRecebimento",
            ),
            (
                |c| {
                    c.formas_recebimento = vec![
                        FormaRecebimento::Boleto,
                        FormaRecebimento::SemFormaPagamento,
                    ];
                },
                "formasRecebimento",
            ),
        ];
        for (alterar, campo) in casos {
            let mut invalida = cobranca();
            alterar(&mut invalida);
            let erro = invalida.validar().unwrap_err();
            assert_eq!(erro.campo(), campo, "{erro}");
        }
    }

    #[test]
    fn invoices_are_checked_against_their_access_key() {
        let com = |alterar: fn(&mut NotaFiscal)| {
            let mut c = cobranca();
            let mut n = nota();
            alterar(&mut n);
            c.nota_fiscal = Some(n);
            c.validar()
        };
        assert_eq!(com(|_| {}), Ok(()));
        assert_eq!(
            com(|n| n.chave_nfe.replace_range(43.., "7")),
            Err(EmissaoCobrancaError::ChaveNfe)
        );
        assert_eq!(
            com(|n| n.chave_nfe.truncate(43)),
            Err(EmissaoCobrancaError::ChaveNfe)
        );
        let erro = com(|n| n.numero = 12_346).unwrap_err();
        assert_eq!(
            erro.to_string(),
            "número e série da nota fiscal diferem dos da chave de acesso (número 12345, série 1)"
        );
        assert_eq!(
            com(|n| n.serie = 2).unwrap_err().campo(),
            "notaFiscal.numero"
        );
        // Published example of the NF-e manual.
        assert!(dv_chave_nfe("52060433009911002506550120000007800267301615"));
    }

    #[test]
    fn states_are_parsed_in_any_case() {
        assert_eq!("sp".parse::<Uf>(), Ok(Uf::Sp));
        assert_eq!(" DF ".parse::<Uf>(), Ok(Uf::Df));
        assert_eq!("XX".parse::<Uf>(), Err(UfError));
        let siglas: Vec<&str> = Uf::TODAS.iter().map(|uf| uf.as_str()).collect();
        assert_eq!(siglas.len(), 27);
        assert!(Uf::TODAS.iter().all(|uf| uf.as_str().parse() == Ok(*uf)));
    }

    #[test]
    fn messages_are_in_portuguese() {
        let mut c = cobranca();
        c.pagador.nome = String::new();
        assert_eq!(
            c.validar().unwrap_err().to_string(),
            "o nome deve ter de 1 a 100 caracteres, sem quebras de linha"
        );
    }
}
