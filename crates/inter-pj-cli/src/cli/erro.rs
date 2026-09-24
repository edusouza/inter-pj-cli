//! The errors of the parser in Portuguese.
//!
//! clap writes them in English and has no localisation, so this formatter
//! writes each kind from the context clap gives, in the layout of clap's
//! own: the problem, the tips, the usage line and where to find help. Like
//! the other errors of the CLI, without colors.

use std::fmt::Write as _;

use clap::builder::StyledStr;
use clap::error::{ContextKind, ContextValue, ErrorFormatter, ErrorKind};

/// Formats the errors of the parser in Portuguese: `erro.apply::<Portugues>()`.
pub(crate) struct Portugues;

type Erro = clap::error::Error<Portugues>;

impl ErrorFormatter for Portugues {
    fn format_error(erro: &Erro) -> StyledStr {
        let mut texto = String::from("erro: ");
        if !problema(erro, &mut texto) {
            match std::error::Error::source(erro) {
                Some(causa) => texto.push_str(&traduzida(&causa.to_string())),
                None => texto.push_str(generico(erro.kind())),
            }
        }
        let dicas = dicas(erro);
        if !dicas.is_empty() {
            texto.push('\n');
            for dica in dicas {
                let _ = write!(texto, "\n  dica: {dica}");
            }
        }
        if let Some(ContextValue::StyledStr(uso)) = erro.get(ContextKind::Usage) {
            let uso = uso.to_string();
            let uso = uso.strip_prefix("Usage: ").unwrap_or(&uso);
            let _ = write!(texto, "\n\nUso: {uso}");
        }
        texto.push_str("\n\nPara mais informações, use '--help'.\n");
        StyledStr::from(texto)
    }
}

/// Writes the problem from the context of the error; false when the error
/// lacks the context its kind needs.
fn problema(erro: &Erro, texto: &mut String) -> bool {
    let texto_de = |tipo| match erro.get(tipo) {
        Some(ContextValue::String(valor)) => Some(valor.as_str()),
        _ => None,
    };
    let numero_de = |tipo| match erro.get(tipo) {
        Some(ContextValue::Number(numero)) => Some(*numero),
        _ => None,
    };
    let argumento = texto_de(ContextKind::InvalidArg);
    let valor = texto_de(ContextKind::InvalidValue);
    match (erro.kind(), argumento, valor) {
        (ErrorKind::ArgumentConflict, ..) => return conflito(erro, texto),
        (ErrorKind::NoEquals, Some(argumento), _) => {
            let _ = write!(texto, "use '=' para dar o valor de '{argumento}'");
        }
        (ErrorKind::InvalidValue, Some(argumento), Some(valor)) => {
            let _ = if valor.is_empty() {
                write!(texto, "falta o valor de '{argumento}'")
            } else {
                write!(texto, "valor inválido '{valor}' para '{argumento}'")
            };
            lista(
                texto,
                "valores possíveis",
                erro.get(ContextKind::ValidValue),
            );
        }
        (ErrorKind::ValueValidation, Some(argumento), Some(valor)) => {
            let _ = write!(texto, "valor inválido '{valor}' para '{argumento}'");
            if let Some(causa) = std::error::Error::source(erro) {
                let _ = write!(texto, ": {}", traduzida(&causa.to_string()));
            }
        }
        (ErrorKind::UnknownArgument, Some(argumento), _) => {
            let _ = write!(texto, "argumento inesperado '{argumento}'");
        }
        (ErrorKind::TooManyValues, Some(argumento), Some(valor)) => {
            let _ = write!(
                texto,
                "valor a mais '{valor}' para '{argumento}': não cabem outros"
            );
        }
        (ErrorKind::TooFewValues | ErrorKind::WrongNumberOfValues, Some(argumento), _) => {
            let esperados = numero_de(ContextKind::MinValues)
                .or_else(|| numero_de(ContextKind::ExpectedNumValues));
            let (Some(esperados), Some(informados)) =
                (esperados, numero_de(ContextKind::ActualNumValues))
            else {
                return false;
            };
            let foram = if informados == 1 {
                "foi informado"
            } else {
                "foram informados"
            };
            let _ = write!(
                texto,
                "'{argumento}' precisa de {esperados} valores, e {foram} {informados}"
            );
        }
        (ErrorKind::MissingRequiredArgument, ..) => {
            let Some(ContextValue::Strings(faltam)) = erro.get(ContextKind::InvalidArg) else {
                return false;
            };
            texto.push_str(if faltam.len() == 1 {
                "falta um argumento obrigatório:"
            } else {
                "faltam argumentos obrigatórios:"
            });
            for falta in faltam {
                let _ = write!(texto, "\n  {falta}");
            }
        }
        (ErrorKind::InvalidSubcommand, ..) => {
            let Some(comando) = texto_de(ContextKind::InvalidSubcommand) else {
                return false;
            };
            let _ = write!(texto, "comando desconhecido '{comando}'");
        }
        (ErrorKind::MissingSubcommand, ..) => {
            let Some(comando) = texto_de(ContextKind::InvalidSubcommand) else {
                return false;
            };
            let _ = write!(texto, "'{comando}' precisa de um comando");
            lista(texto, "comandos", erro.get(ContextKind::ValidSubcommand));
        }
        _ => return false,
    }
    true
}

/// Two arguments that do not go together, or one given twice.
fn conflito(erro: &Erro, texto: &mut String) -> bool {
    let invalido = erro.get(ContextKind::InvalidArg);
    let anterior = erro.get(ContextKind::PriorArg);
    let argumento = match (invalido, erro.get(ContextKind::InvalidSubcommand)) {
        (Some(ContextValue::String(argumento)), _) => format!("'{argumento}'"),
        (_, Some(ContextValue::String(comando))) => format!("o comando '{comando}'"),
        _ => return false,
    };
    if invalido.is_some() && invalido == anterior {
        let _ = write!(texto, "{argumento} só pode ser informado uma vez");
        return true;
    }
    let _ = write!(texto, "não é possível usar {argumento} com");
    match anterior {
        Some(ContextValue::String(outro)) => {
            let _ = write!(texto, " '{outro}'");
        }
        Some(ContextValue::Strings(outros)) => {
            texto.push(':');
            for outro in outros {
                let _ = write!(texto, "\n  {outro}");
            }
        }
        _ => texto.push_str(" as outras opções informadas"),
    }
    true
}

/// `\n  [nome: a, b]`, when there are values.
fn lista(texto: &mut String, nome: &str, valores: Option<&ContextValue>) {
    let Some(ContextValue::Strings(valores)) = valores else {
        return;
    };
    if valores.is_empty() {
        return;
    }
    let valores: Vec<String> = valores
        .iter()
        .map(|valor| {
            if valor.contains(char::is_whitespace) {
                format!("{valor:?}")
            } else {
                valor.clone()
            }
        })
        .collect();
    let _ = write!(texto, "\n  [{nome}: {}]", valores.join(", "));
}

/// The similar commands, options or values clap found, and the tips it
/// writes in full.
fn dicas(erro: &Erro) -> Vec<String> {
    let mut dicas = Vec::new();
    for (tipo, um, varios) in [
        (
            ContextKind::SuggestedSubcommand,
            "há um comando parecido",
            "há comandos parecidos",
        ),
        (
            ContextKind::SuggestedArg,
            "há uma opção parecida",
            "há opções parecidas",
        ),
        (
            ContextKind::SuggestedValue,
            "há um valor parecido",
            "há valores parecidos",
        ),
    ] {
        let parecidos: Vec<String> = match erro.get(tipo) {
            Some(ContextValue::String(parecido)) => vec![format!("'{parecido}'")],
            Some(ContextValue::Strings(parecidos)) => {
                parecidos.iter().map(|p| format!("'{p}'")).collect()
            }
            _ => Vec::new(),
        };
        if !parecidos.is_empty() {
            let rotulo = if parecidos.len() == 1 { um } else { varios };
            dicas.push(format!("{rotulo}: {}", parecidos.join(", ")));
        }
    }
    if let Some(ContextValue::StyledStrs(sugestoes)) = erro.get(ContextKind::Suggested) {
        dicas.extend(sugestoes.iter().map(|s| sugestao(&s.to_string())));
    }
    dicas
}

/// A tip clap writes in full, in Portuguese; one it does not know as is.
fn sugestao(texto: &str) -> String {
    if let Some(resto) = texto.strip_prefix("to pass '")
        && let Some((valor, uso)) = resto.split_once("' as a value, use '")
    {
        let uso = uso.strip_suffix('\'').unwrap_or(uso);
        return format!("para passar '{valor}' como valor, use '{uso}'");
    }
    if let Some(resto) = texto.strip_prefix("subcommand '")
        && let Some((comando, _)) = resto.split_once("' exists")
    {
        return format!("o comando '{comando}' existe; para usá-lo, tire o '--' antes dele");
    }
    texto.to_owned()
}

/// Why a value was refused: the CLI's own reasons are in Portuguese
/// already, but the numbers are checked by clap and by the standard library.
fn traduzida(causa: &str) -> String {
    if let Some((_, faixa)) = causa.split_once(" is not in ")
        && let Some((minimo, maximo)) = faixa.split_once("..=")
    {
        return format!("use um número de {minimo} a {maximo}");
    }
    let traducao = match causa {
        "invalid digit found in string" => "não é um número inteiro",
        "cannot parse integer from empty string" => "falta o número",
        "number too large to fit in target type" => "número grande demais",
        "number too small to fit in target type" => "número pequeno demais",
        _ => causa,
    };
    traducao.to_owned()
}

/// The problem, when the error has no context to tell more.
fn generico(tipo: ErrorKind) -> &'static str {
    match tipo {
        ErrorKind::InvalidValue | ErrorKind::ValueValidation => "valor inválido num dos argumentos",
        ErrorKind::UnknownArgument => "argumento inesperado",
        ErrorKind::InvalidSubcommand => "comando desconhecido",
        ErrorKind::NoEquals => "falta o '=' no valor de um argumento",
        ErrorKind::TooManyValues => "valores a mais num dos argumentos",
        ErrorKind::TooFewValues | ErrorKind::WrongNumberOfValues => {
            "número errado de valores num dos argumentos"
        }
        ErrorKind::ArgumentConflict => "há argumentos que não podem ser usados juntos",
        ErrorKind::MissingRequiredArgument => "falta um argumento obrigatório",
        ErrorKind::MissingSubcommand => "falta o comando",
        ErrorKind::InvalidUtf8 => "um dos argumentos não é um texto UTF-8 válido",
        _ => "argumentos inválidos",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the parser prints for `argumentos`.
    fn erro(argumentos: &[&str]) -> String {
        let erro = super::super::command()
            .try_get_matches_from(std::iter::once("inter-pj").chain(argumentos.iter().copied()))
            .unwrap_err();
        erro.apply::<Portugues>().render().to_string()
    }

    #[test]
    fn a_value_out_of_the_list_shows_the_list() {
        assert_eq!(
            erro(&["saldo", "--formato", "xml"]),
            "\
erro: valor inválido 'xml' para '--formato <FORMATO>'
  [valores possíveis: texto, json, csv]

Para mais informações, use '--help'.
"
        );
    }

    #[test]
    fn a_missing_value_shows_the_list() {
        assert_eq!(
            erro(&["saldo", "--formato"]),
            "\
erro: falta o valor de '--formato <FORMATO>'
  [valores possíveis: texto, json, csv]

Para mais informações, use '--help'.
"
        );
    }

    #[test]
    fn a_value_the_cli_refuses_says_why() {
        assert_eq!(
            erro(&["saldo", "--data", "amanha"]),
            "\
erro: valor inválido 'amanha' para '--data <AAAA-MM-DD>': data inválida \"amanha\": use o formato AAAA-MM-DD

Para mais informações, use '--help'.
"
        );
    }

    #[test]
    fn numbers_checked_by_clap_say_why_in_portuguese() {
        let texto = erro(&["extrato", "completo", "--tamanho-pagina", "0"]);
        assert!(
            texto.starts_with(
                "erro: valor inválido '0' para '--tamanho-pagina <N>': use um número de 1 a 10000\n"
            ),
            "{texto}"
        );
        let texto = erro(&["extrato", "completo", "--pagina", "primeira"]);
        assert!(
            texto.starts_with(
                "erro: valor inválido 'primeira' para '--pagina <N>': não é um número inteiro\n"
            ),
            "{texto}"
        );
    }

    #[test]
    fn an_unknown_option_suggests_the_near_one() {
        assert_eq!(
            erro(&["saldo", "--dat", "2026-08-31"]),
            "\
erro: argumento inesperado '--dat'

  dica: há uma opção parecida: '--data'

Uso: inter-pj saldo [OPÇÕES]

Para mais informações, use '--help'.
"
        );
    }

    #[test]
    fn an_unknown_command_suggests_the_near_one() {
        assert_eq!(
            erro(&["sald"]),
            "\
erro: comando desconhecido 'sald'

  dica: há um comando parecido: 'saldo'

Uso: inter-pj [OPÇÕES] <COMANDO>

Para mais informações, use '--help'.
"
        );
    }

    #[test]
    fn options_that_do_not_go_together() {
        assert_eq!(
            erro(&["extrato", "completo", "--pagina", "1", "--todas-paginas"]),
            "\
erro: não é possível usar '--pagina <N>' com '--todas-paginas'

Uso: inter-pj extrato completo [OPÇÕES]

Para mais informações, use '--help'.
"
        );
    }

    #[test]
    fn an_option_given_twice() {
        assert_eq!(
            erro(&["saldo", "--data", "2026-08-30", "--data", "2026-08-31"]),
            "\
erro: '--data <AAAA-MM-DD>' só pode ser informado uma vez

Uso: inter-pj saldo [OPÇÕES]

Para mais informações, use '--help'.
"
        );
    }

    #[test]
    fn tips_clap_writes_in_full_are_translated() {
        assert_eq!(
            sugestao("to pass '-5' as a value, use '-- -5'"),
            "para passar '-5' como valor, use '-- -5'"
        );
        assert_eq!(
            sugestao("subcommand 'saldo' exists; to use it, remove the '--' before it"),
            "o comando 'saldo' existe; para usá-lo, tire o '--' antes dele"
        );
        assert_eq!(sugestao("uma dica nova"), "uma dica nova");
    }

    #[test]
    fn nothing_in_english_is_left() {
        for argumentos in [
            &["saldo", "--formato", "xml"][..],
            &["saldo", "--dat", "x"],
            &["saldo", "extra"],
            &["sald"],
            &["pix", "consultar"],
            &["pix", "consultar", "-5"],
            &["extrato", "completo", "--pagina", "1", "--todas-paginas"],
            &["extrato", "completo", "--tamanho-pagina", "0"],
            &["extrato", "completo", "--pagina", "-1"],
            &["extrato", "completo", "--pagina", ""],
            &["--tentativas", "99", "saldo"],
        ] {
            let texto = erro(argumentos);
            assert!(texto.starts_with("erro: "), "{argumentos:?}: {texto}");
            for ingles in [
                "error", "tip:", "Usage", "For more", "invalid", "found", "is not", "required",
                "provided",
            ] {
                assert!(!texto.contains(ingles), "{argumentos:?}: {texto}");
            }
        }
    }
}
