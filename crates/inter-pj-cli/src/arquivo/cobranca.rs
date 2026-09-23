//! A charge in a JSON file with the API's field names
//! (`EmitirCobrancaRequestBody`), for `cobranca emitir --arquivo`.

use chrono::{Days, NaiveDate};
use inter_pj::cobranca::{
    BeneficiarioFinal, Desconto, EmissaoCobranca, FormaRecebimento, MAX_LINHAS_MENSAGEM, Mora,
    Multa, NotaFiscal, Pagador, TipoPessoa, Uf,
};
use inter_pj::documento::Documento;
use serde_json::Value;

use super::Campos;
use crate::error::CliError;

const CAMPOS: [&str; 12] = [
    "seuNumero",
    "valorNominal",
    "dataVencimento",
    "numDiasAgenda",
    "pagador",
    "desconto",
    "multa",
    "mora",
    "mensagem",
    "beneficiarioFinal",
    "formasRecebimento",
    "notaFiscal",
];
const CAMPOS_PAGADOR: [&str; 13] = [
    "cpfCnpj",
    "tipoPessoa",
    "nome",
    "endereco",
    "numero",
    "complemento",
    "bairro",
    "cidade",
    "uf",
    "cep",
    "email",
    "ddd",
    "telefone",
];
const CAMPOS_BENEFICIARIO: [&str; 8] = [
    "cpfCnpj",
    "tipoPessoa",
    "nome",
    "endereco",
    "bairro",
    "cidade",
    "uf",
    "cep",
];
const CAMPOS_DESCONTO: [&str; 4] = ["codigo", "quantidadeDias", "taxa", "valor"];
const CAMPOS_ENCARGO: [&str; 3] = ["codigo", "taxa", "valor"];
const CAMPOS_MENSAGEM: [&str; MAX_LINHAS_MENSAGEM] =
    ["linha1", "linha2", "linha3", "linha4", "linha5"];
const CAMPOS_NOTA: [&str; 6] = [
    "chaveNFe",
    "numero",
    "serie",
    "dataEmissao",
    "parcela",
    "naturezaOperacao",
];

/// The charge of `valor` (a file's JSON), validated. `numDiasAgenda` may be
/// left out (0, as in the API), and so may `tipoPessoa`, which comes from
/// the document; when given, it must agree with it.
pub(crate) fn cobranca(valor: &Value, onde: &str) -> Result<EmissaoCobranca, CliError> {
    let campos = Campos::de(valor, onde, &CAMPOS)?;
    let pagador = campos
        .objeto("pagador", &CAMPOS_PAGADOR)?
        .ok_or_else(|| campos.erro("pagador", "obrigatório"))?;
    let mut cobranca = EmissaoCobranca::new(
        texto(&campos, "seuNumero")?,
        campos.obrigatorio("valorNominal", campos.valor("valorNominal")?)?,
        campos.obrigatorio("dataVencimento", campos.data("dataVencimento")?)?,
        ler_pagador(&pagador)?,
    );
    cobranca.num_dias_agenda = campos.inteiro("numDiasAgenda")?.unwrap_or(0);
    cobranca.desconto = campos
        .objeto("desconto", &CAMPOS_DESCONTO)?
        .map(|campos| desconto(&campos))
        .transpose()?;
    cobranca.multa = campos
        .objeto("multa", &CAMPOS_ENCARGO)?
        .map(|campos| multa(&campos))
        .transpose()?;
    cobranca.mora = campos
        .objeto("mora", &CAMPOS_ENCARGO)?
        .map(|campos| mora(&campos))
        .transpose()?;
    if let Some(mensagem) = campos.objeto("mensagem", &CAMPOS_MENSAGEM)? {
        cobranca.mensagem = linhas(&mensagem)?;
    }
    cobranca.beneficiario_final = campos
        .objeto("beneficiarioFinal", &CAMPOS_BENEFICIARIO)?
        .map(|campos| beneficiario(&campos))
        .transpose()?;
    cobranca.formas_recebimento = formas(&campos)?;
    cobranca.nota_fiscal = campos
        .objeto("notaFiscal", &CAMPOS_NOTA)?
        .map(|campos| nota_fiscal(&campos))
        .transpose()?;
    cobranca
        .validar()
        .map_err(|err| campos.erro(err.campo(), err))?;
    Ok(cobranca)
}

fn texto(campos: &Campos<'_>, campo: &str) -> Result<String, CliError> {
    campos.obrigatorio(campo, campos.texto(campo)?)
}

/// The document and, when given, a `tipoPessoa` that agrees with it.
fn documento(campos: &Campos<'_>) -> Result<Documento, CliError> {
    let documento = campos.obrigatorio("cpfCnpj", campos.documento("cpfCnpj")?)?;
    let tipo = TipoPessoa::de(&documento);
    if let Some(dado) = campos.texto("tipoPessoa")?
        && !dado.eq_ignore_ascii_case(tipo.as_str())
    {
        let qual = match tipo {
            TipoPessoa::Fisica => "um CPF",
            TipoPessoa::Juridica => "um CNPJ",
        };
        return Err(campos.erro(
            "tipoPessoa",
            format!(
                "\"{dado}\" não corresponde ao documento, {qual} ({}); pode ser omitido",
                tipo.as_str()
            ),
        ));
    }
    Ok(documento)
}

fn uf(campos: &Campos<'_>) -> Result<Uf, CliError> {
    let uf = texto(campos, "uf")?;
    uf.parse().map_err(|err| campos.erro("uf", err))
}

/// The postal code with or without punctuation (`30110-000`).
fn cep(campos: &Campos<'_>) -> Result<String, CliError> {
    Ok(texto(campos, "cep")?
        .chars()
        .filter(|c| !matches!(c, '-' | '.' | ' '))
        .collect())
}

fn ler_pagador(campos: &Campos<'_>) -> Result<Pagador, CliError> {
    let mut pagador = Pagador::new(
        documento(campos)?,
        texto(campos, "nome")?,
        texto(campos, "endereco")?,
        texto(campos, "cidade")?,
        uf(campos)?,
        cep(campos)?,
    );
    pagador.numero = campos.texto("numero")?;
    pagador.complemento = campos.texto("complemento")?;
    pagador.bairro = campos.texto("bairro")?;
    pagador.email = campos.texto("email")?;
    pagador.ddd = campos.texto("ddd")?;
    pagador.telefone = campos
        .texto("telefone")?
        .map(|telefone| telefone.chars().filter(char::is_ascii_digit).collect());
    Ok(pagador)
}

fn beneficiario(campos: &Campos<'_>) -> Result<BeneficiarioFinal, CliError> {
    Ok(BeneficiarioFinal {
        cpf_cnpj: documento(campos)?,
        nome: texto(campos, "nome")?,
        endereco: texto(campos, "endereco")?,
        bairro: campos.texto("bairro")?,
        cidade: texto(campos, "cidade")?,
        uf: uf(campos)?,
        cep: cep(campos)?,
    })
}

/// `taxa` or `valor`, as the code asks, never both.
fn taxa_ou_valor(
    campos: &Campos<'_>,
    codigo: &str,
    com_taxa: bool,
) -> Result<rust_decimal::Decimal, CliError> {
    let (taxa, valor) = (campos.valor("taxa")?, campos.valor("valor")?);
    let (quer, sobra) = if com_taxa {
        ("taxa", valor)
    } else {
        ("valor", taxa)
    };
    if sobra.is_some() {
        let outro = if com_taxa { "valor" } else { "taxa" };
        return Err(campos.erro(outro, format!("não se aplica ao código {codigo}")));
    }
    let dado = if com_taxa { taxa } else { valor };
    dado.ok_or_else(|| campos.erro(quer, format!("obrigatório com o código {codigo}")))
}

fn codigo(campos: &Campos<'_>, aceitos: &[&str]) -> Result<String, CliError> {
    let codigo = texto(campos, "codigo")?.to_ascii_uppercase();
    if aceitos.contains(&codigo.as_str()) {
        Ok(codigo)
    } else {
        Err(campos.erro(
            "codigo",
            format!("\"{codigo}\" não é um dos códigos: {}", aceitos.join(", ")),
        ))
    }
}

fn desconto(campos: &Campos<'_>) -> Result<Desconto, CliError> {
    let codigo = codigo(
        campos,
        &["PERCENTUALDATAINFORMADA", "VALORFIXODATAINFORMADA"],
    )?;
    let quantidade_dias = campos.inteiro("quantidadeDias")?.unwrap_or(0);
    Ok(if codigo == "PERCENTUALDATAINFORMADA" {
        Desconto::Percentual {
            taxa: taxa_ou_valor(campos, &codigo, true)?,
            quantidade_dias,
        }
    } else {
        Desconto::ValorFixo {
            valor: taxa_ou_valor(campos, &codigo, false)?,
            quantidade_dias,
        }
    })
}

fn multa(campos: &Campos<'_>) -> Result<Multa, CliError> {
    let codigo = codigo(campos, &["PERCENTUAL", "VALORFIXO"])?;
    Ok(if codigo == "PERCENTUAL" {
        Multa::Percentual {
            taxa: taxa_ou_valor(campos, &codigo, true)?,
        }
    } else {
        Multa::ValorFixo {
            valor: taxa_ou_valor(campos, &codigo, false)?,
        }
    })
}

fn mora(campos: &Campos<'_>) -> Result<Mora, CliError> {
    let codigo = codigo(campos, &["TAXAMENSAL", "VALORDIA"])?;
    Ok(if codigo == "TAXAMENSAL" {
        Mora::TaxaMensal {
            taxa: taxa_ou_valor(campos, &codigo, true)?,
        }
    } else {
        Mora::ValorDia {
            valor: taxa_ou_valor(campos, &codigo, false)?,
        }
    })
}

/// `linha1` to `linha5`, in order; lines left out in the middle are blank.
fn linhas(campos: &Campos<'_>) -> Result<Vec<String>, CliError> {
    let mut linhas = Vec::new();
    for campo in CAMPOS_MENSAGEM {
        linhas.push(campos.texto(campo)?.unwrap_or_default());
    }
    while linhas.last().is_some_and(String::is_empty) {
        linhas.pop();
    }
    Ok(linhas)
}

fn formas(campos: &Campos<'_>) -> Result<Vec<FormaRecebimento>, CliError> {
    const CAMPO: &str = "formasRecebimento";
    let Some(itens) = campos.lista(CAMPO)? else {
        return Ok(Vec::new());
    };
    itens
        .iter()
        .map(|item| {
            let texto = item
                .as_str()
                .unwrap_or_default()
                .trim()
                .to_ascii_uppercase();
            match texto.as_str() {
                "BOLETO" => Ok(FormaRecebimento::Boleto),
                "PIX" => Ok(FormaRecebimento::Pix),
                "SEM_FORMA_PAGAMENTO" => Ok(FormaRecebimento::SemFormaPagamento),
                _ => Err(campos.erro(
                    CAMPO,
                    format!("{item} não é BOLETO, PIX nem SEM_FORMA_PAGAMENTO"),
                )),
            }
        })
        .collect()
}

fn nota_fiscal(campos: &Campos<'_>) -> Result<NotaFiscal, CliError> {
    Ok(NotaFiscal {
        chave_nfe: texto(campos, "chaveNFe")?,
        numero: campos.obrigatorio("numero", campos.inteiro("numero")?)?,
        serie: campos.obrigatorio("serie", campos.inteiro("serie")?)?,
        data_emissao: campos.obrigatorio("dataEmissao", campos.data("dataEmissao")?)?,
        parcela: campos.inteiro("parcela")?,
        natureza_operacao: campos.texto("naturezaOperacao")?,
    })
}

/// A charge to start from, with synthetic data; the due date goes in
/// `{vencimento}`.
const MODELO: &str = r#"{
  "seuNumero": "NF-123",
  "valorNominal": "150,00",
  "dataVencimento": "{vencimento}",
  "numDiasAgenda": 30,
  "pagador": {
    "cpfCnpj": "12.345.678/0001-95",
    "nome": "Cliente Exemplo Ltda",
    "endereco": "Avenida Brasil",
    "numero": "1200",
    "complemento": "sala 3",
    "bairro": "Centro",
    "cidade": "Belo Horizonte",
    "uf": "MG",
    "cep": "30110-000",
    "email": "financeiro@exemplo.com.br",
    "ddd": "31",
    "telefone": "999999999"
  },
  "desconto": {"codigo": "PERCENTUALDATAINFORMADA", "taxa": 2, "quantidadeDias": 5},
  "multa": {"codigo": "PERCENTUAL", "taxa": 2},
  "mora": {"codigo": "TAXAMENSAL", "taxa": 1},
  "mensagem": {"linha1": "Referente à NF 123", "linha2": "Obrigado pela preferência"},
  "formasRecebimento": ["BOLETO", "PIX"]
}
"#;

/// A charge to start from, with synthetic data, due in 30 days.
pub(crate) fn modelo_cobranca(hoje: NaiveDate) -> String {
    let vencimento = hoje.checked_add_days(Days::new(30)).unwrap_or(hoje);
    MODELO.replace("{vencimento}", &vencimento.format("%Y-%m-%d").to_string())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn hoje() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, 23).unwrap()
    }

    fn ler(valor: &Value) -> Result<EmissaoCobranca, String> {
        cobranca(valor, "cobranca.json").map_err(|err| err.to_string())
    }

    fn modelo() -> Value {
        serde_json::from_str(&modelo_cobranca(hoje())).unwrap()
    }

    #[test]
    fn the_template_is_a_valid_charge() {
        let cobranca = ler(&modelo()).unwrap();
        assert_eq!(
            cobranca.data_vencimento,
            NaiveDate::from_ymd_opt(2026, 10, 23).unwrap()
        );
        assert_eq!(cobranca.pagador.cep, "30110000");
        assert_eq!(cobranca.mensagem.len(), 2);
        assert_eq!(
            cobranca.formas_recebimento,
            [FormaRecebimento::Boleto, FormaRecebimento::Pix]
        );
        // Back in the API's shape, with the kind of person from the document.
        let corpo = serde_json::to_value(&cobranca).unwrap();
        assert_eq!(corpo["pagador"]["tipoPessoa"], "JURIDICA");
        assert_eq!(corpo["valorNominal"], 150);
    }

    #[test]
    fn full_files_of_the_api_are_read() {
        let mut valor = modelo();
        valor["pagador"]["tipoPessoa"] = json!("JURIDICA");
        valor["desconto"] = json!({"codigo": "VALORFIXODATAINFORMADA", "valor": "10,00"});
        valor["multa"] = json!({"codigo": "VALORFIXO", "valor": 4});
        valor["mora"] = json!({"codigo": "VALORDIA", "valor": 0.33});
        valor["mensagem"] = json!({"linha1": "a", "linha3": "c"});
        valor["beneficiarioFinal"] = json!({
            "cpfCnpj": "123.456.789-09", "nome": "Beneficiário Exemplo", "endereco": "Rua Exemplo",
            "cidade": "São Paulo", "uf": "sp", "cep": "01001-000"
        });
        valor["notaFiscal"] = json!({
            "chaveNFe": "31260912345678000195550010000123451123456786",
            "numero": 12345, "serie": "1", "dataEmissao": "2026-09-15"
        });
        let cobranca = ler(&valor).unwrap();
        assert_eq!(
            cobranca.desconto,
            Some(Desconto::ValorFixo {
                valor: "10".parse().unwrap(),
                quantidade_dias: 0
            })
        );
        assert_eq!(cobranca.mensagem, ["a", "", "c"]);
        let beneficiario = cobranca.beneficiario_final.unwrap();
        assert_eq!(
            (beneficiario.uf, beneficiario.cep.as_str()),
            (Uf::Sp, "01001000")
        );
        assert_eq!(cobranca.nota_fiscal.unwrap().serie, 1);
    }

    #[test]
    fn errors_name_the_field_by_its_path() {
        type Alteracao = fn(&mut Value);
        let casos: [(Alteracao, &str); 10] = [
            (
                |v| v["pagador"]["cpf"] = json!("x"),
                "cobranca.json: campo desconhecido \"pagador.cpf\"",
            ),
            (
                |v| v["pagador"]["cep"] = json!("3011"),
                "cobranca.json, campo \"pagador.cep\": o CEP tem 8 dígitos",
            ),
            (
                |v| v["pagador"]["uf"] = json!("XX"),
                "cobranca.json, campo \"pagador.uf\": UF inválida",
            ),
            (
                |v| v["pagador"]["tipoPessoa"] = json!("FISICA"),
                "cobranca.json, campo \"pagador.tipoPessoa\": \"FISICA\" não corresponde ao documento, um CNPJ (JURIDICA)",
            ),
            (
                |v| v["pagador"]["cpfCnpj"] = json!("12.345.678/0001-00"),
                "cobranca.json, campo \"pagador.cpfCnpj\": ",
            ),
            (
                |v| v["desconto"]["valor"] = json!(1),
                "cobranca.json, campo \"desconto.valor\": não se aplica ao código PERCENTUALDATAINFORMADA",
            ),
            (
                |v| v["multa"] = json!({"codigo": "OUTRO"}),
                "cobranca.json, campo \"multa.codigo\": \"OUTRO\" não é um dos códigos: PERCENTUAL, VALORFIXO",
            ),
            (
                |v| v["valorNominal"] = json!("1,00"),
                "cobranca.json, campo \"valorNominal\": o valor da cobrança deve ser",
            ),
            (
                |v| {
                    v.as_object_mut().unwrap().remove("pagador");
                },
                "cobranca.json, campo \"pagador\": obrigatório",
            ),
            (
                |v| v["formasRecebimento"] = json!(["DINHEIRO"]),
                "cobranca.json, campo \"formasRecebimento\": \"DINHEIRO\" não é BOLETO",
            ),
        ];
        for (alterar, esperado) in casos {
            let mut valor = modelo();
            alterar(&mut valor);
            let erro = ler(&valor).unwrap_err();
            assert!(erro.starts_with(esperado), "{erro}\nesperado: {esperado}");
        }
    }
}
