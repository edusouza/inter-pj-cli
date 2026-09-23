//! `inter-pj saldo`

use chrono::NaiveDate;
use inter_pj::banking::Saldo;

use super::Context;
use crate::cli::{Formato, SaldoArgs};
use crate::error::CliError;
use crate::output;
use crate::tabela::{Celula, Coluna, Tabela};

pub(super) async fn run(context: &Context, args: &SaldoArgs) -> Result<(), CliError> {
    let settings = context.settings()?;
    let client = context.client(&settings)?;
    let saldo = client.banking().saldo(args.data).await?;
    match context.formato() {
        Formato::Json => output::print_json(&saldo),
        Formato::Csv => output::print_raw(&csv(&saldo, args.data).csv(context.separador())),
        Formato::Texto => {
            context.warn_if_sandbox(&settings);
            output::print(&render(&saldo, args.data))
        }
    }
}

fn render(saldo: &Saldo, data: Option<NaiveDate>) -> String {
    let mut rows = Vec::new();
    if let Some(data) = data {
        rows.push(("Data da consulta", data.format("%d/%m/%Y").to_string()));
    }
    let values = [
        ("Saldo disponível", saldo.disponivel),
        ("Bloqueado em cheque", saldo.bloqueado_cheque),
        ("Bloqueado judicialmente", saldo.bloqueado_judicialmente),
        ("Bloqueado administrativo", saldo.bloqueado_administrativo),
        ("Limite", saldo.limite),
    ];
    for (label, value) in values {
        if let Some(value) = value {
            rows.push((label, output::brl(value)));
        }
    }
    if let Some(referencia) = &saldo.data_referencia {
        rows.push(("Data de referência", referencia.clone()));
    }
    if rows.iter().all(|(label, _)| *label == "Data da consulta") {
        rows.push(("Saldo", "não informado pela API".to_owned()));
    }
    output::key_values(&rows)
}

/// One line with the API field names (plus the date queried, if any).
fn csv(saldo: &Saldo, data: Option<NaiveDate>) -> Tabela {
    let valores = [
        ("disponivel", saldo.disponivel),
        ("bloqueadoCheque", saldo.bloqueado_cheque),
        ("bloqueadoJudicialmente", saldo.bloqueado_judicialmente),
        ("bloqueadoAdministrativo", saldo.bloqueado_administrativo),
        ("limite", saldo.limite),
    ];
    let mut colunas = vec![Coluna::texto("dataSaldo", "dataSaldo")];
    colunas.extend(valores.iter().map(|(campo, _)| Coluna::valor(campo, campo)));
    colunas.push(Coluna::texto("dataReferencia", "dataReferencia"));
    let mut linha = vec![data.map_or(Celula::Vazia, Celula::Data)];
    linha.extend(valores.iter().map(|(_, valor)| Celula::dinheiro(*valor)));
    linha.push(Celula::texto(saldo.data_referencia.as_deref()));

    let mut tabela = Tabela::new(colunas);
    tabela.linha(linha);
    tabela
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saldo(json: &str) -> Saldo {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn renders_every_returned_field() {
        let text = render(
            &saldo(
                r#"{"disponivel":2850.55,"bloqueadoCheque":240.25,"bloqueadoJudicialmente":0,"bloqueadoAdministrativo":1.5,"limite":1000}"#,
            ),
            None,
        );
        assert_eq!(
            text,
            "\
Saldo disponível          R$ 2.850,55
Bloqueado em cheque         R$ 240,25
Bloqueado judicialmente       R$ 0,00
Bloqueado administrativo      R$ 1,50
Limite                    R$ 1.000,00"
        );
    }

    #[test]
    fn renders_balance_for_a_date() {
        let data = NaiveDate::from_ymd_opt(2026, 1, 3).unwrap();
        let text = render(
            &saldo(r#"{"disponivel":-5,"dataReferencia":"02/01/2026"}"#),
            Some(data),
        );
        assert_eq!(
            text,
            "\
Data da consulta    03/01/2026
Saldo disponível      -R$ 5,00
Data de referência  02/01/2026"
        );
    }

    #[test]
    fn csv_has_one_line_with_api_names() {
        let saldo = saldo(r#"{"disponivel":2850.55,"limite":1000}"#);
        let data = NaiveDate::from_ymd_opt(2026, 1, 3);
        assert_eq!(
            csv(&saldo, data).csv(crate::tabela::Separador::Virgula),
            "dataSaldo,disponivel,bloqueadoCheque,bloqueadoJudicialmente,bloqueadoAdministrativo,limite,dataReferencia\r\n2026-01-03,2850.55,,,,1000,\r\n"
        );
    }

    #[test]
    fn explains_empty_response() {
        assert_eq!(render(&saldo("{}"), None), "Saldo  não informado pela API");
    }
}
