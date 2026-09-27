//! A Pix charge with a due date in a JSON file with the API's field names
//! (`CobVSolicitada`), for `pix cobv criar --arquivo`.

use chrono::{Days, NaiveDate};
use inter_pj::cobranca::Uf;
use inter_pj::documento::Documento;
use inter_pj::pix::{
    AbatimentoCobv, ChavePix, CobvSolicitada, DescontoCobv, DescontoData, DevedorCobv,
    InfoAdicional, JurosCobv, LocCob, ModalidadeJuros, MultaCobv, TipoCob,
};
use rust_decimal::Decimal;
use serde_json::Value;

use super::Campos;
use crate::error::CliError;

/// Fields of a charge (`CobVSolicitada`).
pub(crate) const CAMPOS_COBV: [&str; 7] = [
    "calendario",
    "devedor",
    "loc",
    "valor",
    "chave",
    "solicitacaoPagador",
    "infoAdicionais",
];
const CAMPOS_CALENDARIO: [&str; 2] = ["dataDeVencimento", "validadeAposVencimento"];
const CAMPOS_DEVEDOR: [&str; 8] = [
    "cpf",
    "cnpj",
    "nome",
    "email",
    "logradouro",
    "cidade",
    "uf",
    "cep",
];
const CAMPOS_LOC: [&str; 2] = ["id", "tipoCob"];
const CAMPOS_VALOR: [&str; 5] = ["original", "multa", "juros", "abatimento", "desconto"];
const CAMPOS_ENCARGO: [&str; 2] = ["modalidade", "valorPerc"];
const CAMPOS_DESCONTO: [&str; 3] = ["modalidade", "valorPerc", "descontoDataFixa"];
const CAMPOS_DESCONTO_DATA: [&str; 2] = ["data", "valorPerc"];
const CAMPOS_INFO: [&str; 2] = ["nome", "valor"];

/// The charge of `valor` (a file's JSON), validated.
pub(crate) fn cobv(valor: &Value, onde: &str) -> Result<CobvSolicitada, CliError> {
    ler_cobv(&Campos::de(valor, onde, &CAMPOS_COBV)?)
}

/// A charge from its fields ([`CAMPOS_COBV`]), validated as the creation
/// is. The due date is not compared with today: the caller does that.
pub(crate) fn ler_cobv(campos: &Campos<'_>) -> Result<CobvSolicitada, CliError> {
    let obrigatorio = |campo: &str, aceitos: &[&str]| {
        campos
            .objeto(campo, aceitos)
            .and_then(|objeto| campos.obrigatorio(campo, objeto))
    };
    let calendario = obrigatorio("calendario", &CAMPOS_CALENDARIO)?;
    let devedor = obrigatorio("devedor", &CAMPOS_DEVEDOR)?;
    let valor = obrigatorio("valor", &CAMPOS_VALOR)?;
    let chave = campos.obrigatorio("chave", campos.texto("chave")?)?;
    let chave: ChavePix = chave.parse().map_err(|err| campos.erro("chave", err))?;
    let mut cobv = CobvSolicitada::new(
        chave,
        valor.obrigatorio("original", valor.valor("original")?)?,
        calendario.obrigatorio("dataDeVencimento", calendario.data("dataDeVencimento")?)?,
        ler_devedor(&devedor)?,
    );
    cobv.calendario.validade_apos_vencimento = calendario.inteiro("validadeAposVencimento")?;
    cobv.valor.multa = valor
        .objeto("multa", &CAMPOS_ENCARGO)?
        .map(|campos| multa(&campos))
        .transpose()?;
    cobv.valor.juros = valor
        .objeto("juros", &CAMPOS_ENCARGO)?
        .map(|campos| juros(&campos))
        .transpose()?;
    cobv.valor.abatimento = valor
        .objeto("abatimento", &CAMPOS_ENCARGO)?
        .map(|campos| abatimento(&campos))
        .transpose()?;
    cobv.valor.desconto = valor
        .objeto("desconto", &CAMPOS_DESCONTO)?
        .map(|campos| desconto(&campos))
        .transpose()?;
    cobv.loc = campos
        .objeto("loc", &CAMPOS_LOC)?
        .map(|campos| loc(&campos))
        .transpose()?;
    cobv.solicitacao_pagador = campos.texto("solicitacaoPagador")?;
    cobv.info_adicionais = infos(campos)?;
    cobv.validar().map_err(|err| {
        let campo = err.campo().to_owned();
        campos.erro(&campo, err)
    })?;
    Ok(cobv)
}

/// `cpf` or `cnpj`, the name and, optionally, e-mail and address.
fn ler_devedor(campos: &Campos<'_>) -> Result<DevedorCobv, CliError> {
    let documento = match (campos.documento("cpf")?, campos.documento("cnpj")?) {
        (Some(cpf @ Documento::Cpf(_)), None) => cpf,
        (None, Some(cnpj @ Documento::Cnpj(_))) => cnpj,
        (Some(_), None) => {
            return Err(campos.erro("cpf", "o documento é um CNPJ: use o campo cnpj"));
        }
        (None, Some(_)) => {
            return Err(campos.erro("cnpj", "o documento é um CPF: use o campo cpf"));
        }
        (Some(_), Some(_)) => return Err(campos.erro("cnpj", "informe cpf ou cnpj, não os dois")),
        (None, None) => return Err(campos.erro("cpf", "obrigatório: informe cpf ou cnpj")),
    };
    let nome = campos.obrigatorio("nome", campos.texto("nome")?)?;
    let mut devedor = DevedorCobv::new(documento, nome);
    devedor.email = campos.texto("email")?;
    devedor.logradouro = campos.texto("logradouro")?;
    devedor.cidade = campos.texto("cidade")?;
    devedor.uf = campos
        .texto("uf")?
        .map(|uf| uf.parse::<Uf>().map_err(|err| campos.erro("uf", err)))
        .transpose()?;
    // The postal code with or without punctuation (`30110-000`).
    devedor.cep = campos.texto("cep")?.map(|cep| {
        cep.chars()
            .filter(|c| !matches!(c, '-' | '.' | ' '))
            .collect()
    });
    Ok(devedor)
}

/// The modality, from 1 to `ultima`, and `valorPerc`.
fn modalidade_e_valor(campos: &Campos<'_>, ultima: u32) -> Result<(u32, Decimal), CliError> {
    let modalidade = modalidade(campos, ultima)?;
    let valor = campos.obrigatorio("valorPerc", campos.valor("valorPerc")?)?;
    Ok((modalidade, valor))
}

/// The modality, from 1 to `ultima`.
fn modalidade(campos: &Campos<'_>, ultima: u32) -> Result<u32, CliError> {
    let modalidade = campos.obrigatorio("modalidade", campos.inteiro("modalidade")?)?;
    if (1..=ultima).contains(&modalidade) {
        Ok(modalidade)
    } else {
        let aceitas: Vec<String> = (1..=ultima).map(|m| m.to_string()).collect();
        Err(campos.erro(
            "modalidade",
            format!(
                "{modalidade} não é uma das modalidades: {}",
                aceitas.join(", ")
            ),
        ))
    }
}

fn multa(campos: &Campos<'_>) -> Result<MultaCobv, CliError> {
    Ok(match modalidade_e_valor(campos, 2)? {
        (1, valor) => MultaCobv::ValorFixo(valor),
        (_, taxa) => MultaCobv::Percentual(taxa),
    })
}

fn juros(campos: &Campos<'_>) -> Result<JurosCobv, CliError> {
    let (codigo, valor) = modalidade_e_valor(campos, 8)?;
    let modalidade = ModalidadeJuros::de_codigo(u64::from(codigo))
        .ok_or_else(|| campos.erro("modalidade", "modalidade desconhecida"))?;
    Ok(JurosCobv::new(modalidade, valor))
}

fn abatimento(campos: &Campos<'_>) -> Result<AbatimentoCobv, CliError> {
    Ok(match modalidade_e_valor(campos, 2)? {
        (1, valor) => AbatimentoCobv::ValorFixo(valor),
        (_, taxa) => AbatimentoCobv::Percentual(taxa),
    })
}

/// Modalities 1 and 2 have `descontoDataFixa`; 3 to 6, `valorPerc`.
fn desconto(campos: &Campos<'_>) -> Result<DescontoCobv, CliError> {
    let modalidade = modalidade(campos, 6)?;
    let (datas, valor) = (
        campos.lista("descontoDataFixa")?,
        campos.valor("valorPerc")?,
    );
    let por_data = matches!(modalidade, 1 | 2);
    if por_data && valor.is_some() {
        return Err(campos.erro(
            "valorPerc",
            format!("não se aplica à modalidade {modalidade}: use descontoDataFixa"),
        ));
    }
    if !por_data && datas.is_some() {
        return Err(campos.erro(
            "descontoDataFixa",
            format!("não se aplica à modalidade {modalidade}: use valorPerc"),
        ));
    }
    if !por_data {
        let valor = valor.ok_or_else(|| {
            campos.erro(
                "valorPerc",
                format!("obrigatório na modalidade {modalidade}"),
            )
        })?;
        return Ok(match modalidade {
            3 => DescontoCobv::ValorPorDiaCorrido(valor),
            4 => DescontoCobv::ValorPorDiaUtil(valor),
            5 => DescontoCobv::PercentualPorDiaCorrido(valor),
            _ => DescontoCobv::PercentualPorDiaUtil(valor),
        });
    }
    let datas = datas.ok_or_else(|| {
        campos.erro(
            "descontoDataFixa",
            format!("obrigatório na modalidade {modalidade}"),
        )
    })?;
    let mut descontos = Vec::with_capacity(datas.len());
    for (i, item) in datas.iter().enumerate() {
        let campo = format!("descontoDataFixa[{i}]");
        let item = campos.objeto_de(item, &campo, &CAMPOS_DESCONTO_DATA)?;
        descontos.push(DescontoData::new(
            item.obrigatorio("data", item.data("data")?)?,
            item.obrigatorio("valorPerc", item.valor("valorPerc")?)?,
        ));
    }
    Ok(if modalidade == 1 {
        DescontoCobv::ValorFixoAteDatas(descontos)
    } else {
        DescontoCobv::PercentualAteDatas(descontos)
    })
}

/// The id of a location created for charges with a due date.
fn loc(campos: &Campos<'_>) -> Result<LocCob, CliError> {
    if let Some(tipo) = campos.texto("tipoCob")?
        && tipo != TipoCob::Cobv.as_str()
    {
        return Err(campos.erro(
            "tipoCob",
            format!("\"{tipo}\": a location de uma cobrança com vencimento é do tipo cobv"),
        ));
    }
    let id = campos.obrigatorio("id", campos.inteiro_longo("id")?)?;
    Ok(LocCob::new(id))
}

fn infos(campos: &Campos<'_>) -> Result<Vec<InfoAdicional>, CliError> {
    const CAMPO: &str = "infoAdicionais";
    let Some(itens) = campos.lista(CAMPO)? else {
        return Ok(Vec::new());
    };
    itens
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let item = campos.objeto_de(item, &format!("{CAMPO}[{i}]"), &CAMPOS_INFO)?;
            Ok(InfoAdicional::new(
                item.obrigatorio("nome", item.texto("nome")?)?,
                item.obrigatorio("valor", item.texto("valor")?)?,
            ))
        })
        .collect()
}

/// A charge to start from, with synthetic data; the dates go in
/// `{vencimento}` and `{desconto}`.
const MODELO: &str = r#"{
  "calendario": {"dataDeVencimento": "{vencimento}", "validadeAposVencimento": 30},
  "devedor": {
    "cnpj": "12.345.678/0001-95",
    "nome": "Cliente Exemplo Ltda",
    "email": "financeiro@exemplo.com.br",
    "logradouro": "Avenida Brasil, 1200",
    "cidade": "Belo Horizonte",
    "uf": "MG",
    "cep": "30110-000"
  },
  "valor": {
    "original": "150,00",
    "multa": {"modalidade": 2, "valorPerc": "2,00"},
    "juros": {"modalidade": 3, "valorPerc": "1,00"},
    "desconto": {"modalidade": 1, "descontoDataFixa": [{"data": "{desconto}", "valorPerc": "10,00"}]}
  },
  "chave": "pix@empresa.example",
  "solicitacaoPagador": "Referente à NF 123",
  "infoAdicionais": [{"nome": "Pedido", "valor": "123"}]
}
"#;

/// A charge to start from, with synthetic data, due in 30 days and with a
/// discount until 5 days before.
pub(crate) fn modelo_cobv(hoje: NaiveDate) -> String {
    let vencimento = hoje.checked_add_days(Days::new(30)).unwrap_or(hoje);
    let desconto = vencimento
        .checked_sub_days(Days::new(5))
        .unwrap_or(vencimento);
    MODELO
        .replace("{vencimento}", &vencimento.format("%Y-%m-%d").to_string())
        .replace("{desconto}", &desconto.format("%Y-%m-%d").to_string())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn hoje() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, 23).unwrap()
    }

    fn ler(valor: &Value) -> Result<CobvSolicitada, String> {
        cobv(valor, "cobv.json").map_err(|err| err.to_string())
    }

    fn modelo() -> Value {
        serde_json::from_str(&modelo_cobv(hoje())).unwrap()
    }

    #[test]
    fn the_template_is_a_valid_charge_in_the_api_shape() {
        let cobv = ler(&modelo()).unwrap();
        assert_eq!(
            serde_json::to_value(&cobv).unwrap(),
            json!({
                "calendario": {"dataDeVencimento": "2026-10-23", "validadeAposVencimento": 30},
                "devedor": {
                    "logradouro": "Avenida Brasil, 1200",
                    "cidade": "Belo Horizonte",
                    "uf": "MG",
                    "cep": "30110000",
                    "cnpj": "12345678000195",
                    "nome": "Cliente Exemplo Ltda",
                    "email": "financeiro@exemplo.com.br"
                },
                "valor": {
                    "original": "150.00",
                    "multa": {"modalidade": 2, "valorPerc": "2.00"},
                    "juros": {"modalidade": 3, "valorPerc": "1.00"},
                    "desconto": {"modalidade": 1, "descontoDataFixa": [{"data": "2026-10-18", "valorPerc": "10.00"}]}
                },
                "chave": "pix@empresa.example",
                "solicitacaoPagador": "Referente à NF 123",
                "infoAdicionais": [{"nome": "Pedido", "valor": "123"}]
            })
        );
    }

    #[test]
    fn every_modality_is_read() {
        let mut valor = modelo();
        valor["devedor"] = json!({"cpf": "123.456.789-09", "nome": "Fulano de Tal"});
        valor["valor"] = json!({
            "original": 150,
            "multa": {"modalidade": "1", "valorPerc": 4},
            "juros": {"modalidade": 8, "valorPerc": "12.00"},
            "abatimento": {"modalidade": 1, "valorPerc": "5,00"},
            "desconto": {"modalidade": 6, "valorPerc": "0.50"}
        });
        valor["loc"] = json!({"id": 7_000_000_000_u64, "tipoCob": "cobv"});
        let cobv = ler(&valor).unwrap();
        assert_eq!(cobv.devedor.documento.as_str(), "12345678909");
        assert_eq!(
            cobv.valor.multa,
            Some(MultaCobv::ValorFixo(Decimal::new(4, 0)))
        );
        assert_eq!(
            cobv.valor.juros,
            Some(JurosCobv::new(
                ModalidadeJuros::PercentualAnoDiasUteis,
                Decimal::new(12, 0)
            ))
        );
        assert_eq!(
            cobv.valor.abatimento,
            Some(AbatimentoCobv::ValorFixo(Decimal::new(5, 0)))
        );
        assert_eq!(
            cobv.valor.desconto,
            Some(DescontoCobv::PercentualPorDiaUtil(Decimal::new(5, 1)))
        );
        assert_eq!(cobv.loc, Some(LocCob::new(7_000_000_000)));
    }

    #[test]
    fn errors_name_the_field_by_its_path() {
        type Alteracao = fn(&mut Value);
        let casos: [(Alteracao, &str); 14] = [
            (
                |v| v["devedor"]["cpfCnpj"] = json!("1"),
                "cobv.json: campo desconhecido \"devedor.cpfCnpj\"",
            ),
            (
                |v| v["devedor"]["cnpj"] = json!("123.456.789-09"),
                "cobv.json, campo \"devedor.cnpj\": o documento é um CPF: use o campo cpf",
            ),
            (
                |v| v["devedor"]["cpf"] = json!("123.456.789-09"),
                "cobv.json, campo \"devedor.cnpj\": informe cpf ou cnpj, não os dois",
            ),
            (
                |v| v["devedor"]["uf"] = json!("XX"),
                "cobv.json, campo \"devedor.uf\": UF inválida",
            ),
            (
                |v| v["devedor"]["cep"] = json!("3011"),
                "cobv.json, campo \"devedor.cep\": o CEP tem 8 dígitos",
            ),
            (
                |v| v["calendario"]["dataDeVencimento"] = json!("20/10/2026"),
                "cobv.json, campo \"calendario.dataDeVencimento\": data inválida",
            ),
            (
                |v| v["valor"]["multa"]["modalidade"] = json!(3),
                "cobv.json, campo \"valor.multa.modalidade\": 3 não é uma das modalidades: 1, 2",
            ),
            (
                |v| v["valor"]["multa"]["valorPerc"] = json!("101"),
                "cobv.json, campo \"valor.multa.valorPerc\": o percentual vai até 100",
            ),
            (
                |v| v["valor"]["juros"]["modalidade"] = json!(9),
                "cobv.json, campo \"valor.juros.modalidade\": 9 não é uma das modalidades",
            ),
            (
                |v| v["valor"]["desconto"]["valorPerc"] = json!(1),
                "cobv.json, campo \"valor.desconto.valorPerc\": não se aplica à modalidade 1",
            ),
            (
                |v| v["valor"]["desconto"]["descontoDataFixa"][0]["data"] = json!("2026-10-24"),
                "cobv.json, campo \"valor.desconto.descontoDataFixa[0].data\": o desconto vale até uma data no vencimento ou antes dele",
            ),
            (
                |v| v["valor"]["desconto"]["descontoDataFixa"][0]["valor"] = json!(1),
                "cobv.json: campo desconhecido \"valor.desconto.descontoDataFixa[0].valor\"",
            ),
            (
                |v| v["loc"] = json!({"id": 1, "tipoCob": "cob"}),
                "cobv.json, campo \"loc.tipoCob\": \"cob\": a location de uma cobrança com vencimento é do tipo cobv",
            ),
            (
                |v| v["infoAdicionais"][0] = json!({"nome": "Pedido"}),
                "cobv.json, campo \"infoAdicionais[0].valor\": obrigatório",
            ),
        ];
        for (alterar, esperado) in casos {
            let mut valor = modelo();
            alterar(&mut valor);
            let erro = ler(&valor).unwrap_err();
            assert!(erro.starts_with(esperado), "{erro}\nesperado: {esperado}");
        }
        let mut valor = modelo();
        valor.as_object_mut().unwrap().remove("devedor");
        assert!(
            ler(&valor)
                .unwrap_err()
                .starts_with("cobv.json, campo \"devedor\": obrigatório")
        );
    }
}
