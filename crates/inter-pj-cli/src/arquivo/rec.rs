//! A recurrence of Pix Automático in a JSON file with the API's field names
//! (`RecSolicitada`), for `pix-automatico rec criar --arquivo`.

use chrono::{Days, Months, NaiveDate};
use inter_pj::pix::{CobrancaPixError, Devedor, Txid, TxidError};
use inter_pj::pix_automatico::{
    AtivacaoSolicitada, CalendarioRec, Periodicidade, PoliticaRetentativa, RecSolicitada, ValorRec,
    VinculoRec,
};
use serde_json::Value;

use super::Campos;
use crate::error::CliError;

/// Fields of a recurrence (`RecSolicitada`).
const CAMPOS_REC: [&str; 6] = [
    "vinculo",
    "calendario",
    "valor",
    "politicaRetentativa",
    "loc",
    "ativacao",
];
const CAMPOS_VINCULO: [&str; 3] = ["objeto", "devedor", "contrato"];
const CAMPOS_DEVEDOR: [&str; 3] = ["cpf", "cnpj", "nome"];
const CAMPOS_CALENDARIO: [&str; 3] = ["dataInicial", "dataFinal", "periodicidade"];
const CAMPOS_VALOR: [&str; 2] = ["valorRec", "valorMinimoRecebedor"];
const CAMPOS_ATIVACAO: [&str; 1] = ["dadosJornada"];
const CAMPOS_JORNADA: [&str; 1] = ["txid"];

/// The recurrence of `valor` (a file's JSON), validated as the creation
/// is.
pub(crate) fn rec(valor: &Value, onde: &str) -> Result<RecSolicitada, CliError> {
    let campos = Campos::de(valor, onde, &CAMPOS_REC)?;
    let obrigatorio = |campo: &str, aceitos: &[&str], problema: &str| {
        campos
            .objeto(campo, aceitos)?
            .ok_or_else(|| campos.erro(campo, problema))
    };
    let vinculo = obrigatorio(
        "vinculo",
        &CAMPOS_VINCULO,
        "obrigatório: o devedor e o contrato",
    )?;
    let devedor = vinculo
        .objeto("devedor", &CAMPOS_DEVEDOR)?
        .ok_or_else(|| vinculo.erro("devedor", "obrigatório: nome e cpf ou cnpj"))?;
    let devedor = Devedor::new(
        devedor.cpf_ou_cnpj()?,
        devedor.obrigatorio("nome", devedor.texto("nome")?)?,
    );
    let mut vinculo_rec = VinculoRec::new(
        devedor,
        vinculo.obrigatorio("contrato", vinculo.texto("contrato")?)?,
    );
    vinculo_rec.objeto = vinculo.texto("objeto")?;

    let calendario = obrigatorio(
        "calendario",
        &CAMPOS_CALENDARIO,
        "obrigatório: dataInicial e periodicidade",
    )?;
    let periodicidade =
        calendario.obrigatorio("periodicidade", calendario.texto("periodicidade")?)?;
    let mut calendario_rec = CalendarioRec::new(
        calendario.obrigatorio("dataInicial", calendario.data("dataInicial")?)?,
        Periodicidade::from(periodicidade.to_uppercase().as_str()),
    );
    calendario_rec.data_final = calendario.data("dataFinal")?;

    let politica =
        campos.obrigatorio("politicaRetentativa", campos.texto("politicaRetentativa")?)?;
    let mut rec = RecSolicitada::new(
        vinculo_rec,
        calendario_rec,
        PoliticaRetentativa::from(politica.to_uppercase().as_str()),
    );
    if let Some(valor) = campos.objeto("valor", &CAMPOS_VALOR)? {
        let mut valor_rec = ValorRec::default();
        valor_rec.valor_rec = valor.valor("valorRec")?;
        valor_rec.valor_minimo_recebedor = valor.valor("valorMinimoRecebedor")?;
        rec.valor = Some(valor_rec);
    }
    rec.loc = campos.inteiro_longo("loc")?;
    if let Some(ativacao) = campos.objeto("ativacao", &CAMPOS_ATIVACAO)? {
        let jornada = ativacao
            .objeto("dadosJornada", &CAMPOS_JORNADA)?
            .ok_or_else(|| {
                ativacao.erro(
                    "dadosJornada",
                    "obrigatório: o txid da cobrança de ativação",
                )
            })?;
        let txid: Txid = jornada
            .obrigatorio("txid", jornada.texto("txid")?)?
            .parse()
            .map_err(|err: TxidError| jornada.erro("txid", err))?;
        rec.ativacao = Some(AtivacaoSolicitada::new(txid));
    }
    rec.validar().map_err(|err| erro(&campos, &err))?;
    Ok(rec)
}

/// An error of the library's checks, named by its field.
fn erro(campos: &Campos<'_>, err: &CobrancaPixError) -> CliError {
    match err.campo() {
        "" => campos.erro_geral(err),
        campo => campos.erro(campo, err),
    }
}

const MODELO: &str = r#"{
  "vinculo": {
    "objeto": "Mensalidade do plano",
    "devedor": {"cpf": "123.456.789-09", "nome": "Cliente Exemplo"},
    "contrato": "contrato-001"
  },
  "calendario": {"dataInicial": "{inicio}", "dataFinal": "{fim}", "periodicidade": "MENSAL"},
  "valor": {"valorRec": "149,90"},
  "politicaRetentativa": "PERMITE_3R_7D"
}
"#;

/// A recurrence to start from, with synthetic data: monthly, for a year,
/// from 30 days on.
pub(crate) fn modelo_rec(hoje: NaiveDate) -> String {
    let inicio = hoje.checked_add_days(Days::new(30)).unwrap_or(hoje);
    let fim = inicio.checked_add_months(Months::new(11)).unwrap_or(inicio);
    MODELO
        .replace("{inicio}", &inicio.format("%Y-%m-%d").to_string())
        .replace("{fim}", &fim.format("%Y-%m-%d").to_string())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn modelo() -> Value {
        serde_json::from_str(&modelo_rec(NaiveDate::from_ymd_opt(2026, 9, 24).unwrap())).unwrap()
    }

    fn ler(valor: &Value) -> Result<RecSolicitada, String> {
        rec(valor, "rec.json").map_err(|err| err.to_string())
    }

    #[test]
    fn the_template_is_a_valid_recurrence_in_the_api_shape() {
        let rec = ler(&modelo()).unwrap();
        assert_eq!(
            serde_json::to_value(&rec).unwrap(),
            json!({
                "vinculo": {
                    "objeto": "Mensalidade do plano",
                    "devedor": {"cpf": "12345678909", "nome": "Cliente Exemplo"},
                    "contrato": "contrato-001"
                },
                "calendario": {"dataInicial": "2026-10-24", "dataFinal": "2027-09-24", "periodicidade": "MENSAL"},
                "valor": {"valorRec": "149.90"},
                "politicaRetentativa": "PERMITE_3R_7D"
            })
        );
    }

    #[test]
    fn everything_the_api_takes_is_read() {
        let mut valor = modelo();
        valor["valor"] = json!({"valorMinimoRecebedor": 50});
        valor["calendario"]["periodicidade"] = json!("anual");
        valor["loc"] = json!("108");
        valor["ativacao"] = json!({"dadosJornada": {"txid": "7978c0c97ea847e78e8849634473c1f1"}});
        let rec = serde_json::to_value(ler(&valor).unwrap()).unwrap();
        assert_eq!(rec["valor"], json!({"valorMinimoRecebedor": "50.00"}));
        assert_eq!(rec["calendario"]["periodicidade"], "ANUAL");
        assert_eq!(rec["loc"], 108);
        assert_eq!(
            rec["ativacao"]["dadosJornada"]["txid"],
            "7978c0c97ea847e78e8849634473c1f1"
        );
    }

    #[test]
    fn errors_name_the_field() {
        type Mudanca = fn(&mut Value);
        let casos: [(Mudanca, &str); 9] = [
            (
                |v| v["vinculo"]["cor"] = json!("azul"),
                "rec.json: campo desconhecido \"vinculo.cor\"",
            ),
            (
                |v| {
                    v.as_object_mut().unwrap().remove("vinculo");
                },
                "rec.json, campo \"vinculo\": obrigatório",
            ),
            (
                |v| v["vinculo"]["devedor"] = json!({"cnpj": "123.456.789-09", "nome": "X"}),
                "rec.json, campo \"vinculo.devedor.cnpj\": o documento é um CPF",
            ),
            (
                |v| v["vinculo"]["contrato"] = json!("x".repeat(36)),
                "rec.json, campo \"vinculo.contrato\"",
            ),
            (
                |v| v["calendario"]["dataInicial"] = json!("10/10/2026"),
                "rec.json, campo \"calendario.dataInicial\": data inválida",
            ),
            (
                |v| v["calendario"]["dataFinal"] = json!("2026-01-01"),
                "rec.json, campo \"calendario.dataFinal\": é anterior",
            ),
            (
                |v| v["calendario"]["periodicidade"] = json!("QUINZENAL"),
                "rec.json, campo \"calendario.periodicidade\": periodicidade desconhecida",
            ),
            (
                |v| v["valor"] = json!({"valorRec": "10", "valorMinimoRecebedor": "5"}),
                "rec.json, campo \"valor.valorMinimoRecebedor\"",
            ),
            (
                |v| v["ativacao"] = json!({"dadosJornada": {"txid": "curto"}}),
                "rec.json, campo \"ativacao.dadosJornada.txid\"",
            ),
        ];
        for (mudar, inicio) in casos {
            let mut valor = modelo();
            mudar(&mut valor);
            let err = ler(&valor).unwrap_err();
            assert!(err.starts_with(inicio), "{inicio}\n{err}");
        }
    }
}
