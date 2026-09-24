//! `inter-pj config init|caminho|mostrar`

use std::fs;

use serde_json::{Value, json};

use super::Context;
use crate::cli::{ConfigCommand, Formato, InitArgs};
use crate::config::{Setting, Settings, TEMPLATE};
use crate::error::CliError;
use crate::files::write_private;
use crate::output;

pub(super) fn run(context: &Context, command: &ConfigCommand) -> Result<(), CliError> {
    match command {
        ConfigCommand::Init(args) => init(context, args),
        ConfigCommand::Caminho => caminho(context),
        ConfigCommand::Mostrar => mostrar(context),
    }
}

fn init(context: &Context, args: &InitArgs) -> Result<(), CliError> {
    let path = context.config_path();
    if path.exists() && !args.forcar {
        return Err(CliError::Config(format!(
            "o arquivo {} já existe; use --forcar para sobrescrevê-lo",
            path.display()
        )));
    }
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .map_err(|err| CliError::io(format!("falha ao criar {}", parent.display()), err))?;
    }
    write_private(path, TEMPLATE.as_bytes())
        .map_err(|err| CliError::io(format!("falha ao gravar {}", path.display()), err))?;
    output::print(&format!(
        "Arquivo de configuração criado em {}\n\
         Próximos passos:\n  \
         1. preencha client_id, certificado e chave_privada;\n  \
         2. defina a variável de ambiente INTER_CLIENT_SECRET;\n  \
         3. teste com: inter-pj saldo",
        path.display()
    ))
}

fn caminho(context: &Context) -> Result<(), CliError> {
    let config = context.config_path();
    let status = if config.exists() {
        ""
    } else {
        " (não existe)"
    };
    match context.formato() {
        Formato::Json => output::print_json(&json!({
            "configuracao": config,
            "configuracaoExiste": config.exists(),
            "cache": context.cache_dir(),
        })),
        // `commands::run` refuses csv for this command.
        Formato::Texto | Formato::Csv => output::print(&output::key_values_left(&[
            ("Configuração", format!("{}{status}", config.display())),
            ("Cache", context.cache_dir().display().to_string()),
        ])),
    }
}

fn mostrar(context: &Context) -> Result<(), CliError> {
    let settings = context.settings()?;
    let rows = describe(&settings);
    match context.formato() {
        Formato::Json => {
            let object: serde_json::Map<String, Value> = rows
                .into_iter()
                .map(|row| {
                    let value = json!({"valor": row.value, "origem": row.source});
                    (row.key.to_owned(), value)
                })
                .collect();
            output::print_json(&object)
        }
        // `commands::run` refuses csv for this command.
        Formato::Texto | Formato::Csv => {
            let lines: Vec<(&str, String)> = rows
                .iter()
                .map(|row| {
                    let text = match (&row.value, &row.source) {
                        (Some(value), Some(source)) => format!("{value} ({source})"),
                        (Some(value), None) => value.clone(),
                        (None, _) => "(não definido)".to_owned(),
                    };
                    (row.label, text)
                })
                .collect();
            output::print(&output::key_values_left(&lines))
        }
    }
}

struct Row {
    key: &'static str,
    label: &'static str,
    value: Option<String>,
    source: Option<String>,
}

fn row<T>(
    key: &'static str,
    label: &'static str,
    setting: Option<&Setting<T>>,
    show: impl Fn(&T) -> String,
) -> Row {
    Row {
        key,
        label,
        value: setting.map(|s| show(&s.value)),
        source: setting.map(|s| s.source.to_string()),
    }
}

fn describe(settings: &Settings) -> Vec<Row> {
    let arquivo = format!(
        "{}{}",
        settings.config_path.display(),
        if settings.config_exists {
            ""
        } else {
            " (não existe)"
        }
    );
    vec![
        row("perfil", "Perfil", Some(&settings.perfil), Clone::clone),
        Row {
            key: "arquivo",
            label: "Arquivo",
            value: Some(arquivo),
            source: None,
        },
        row(
            "ambiente",
            "Ambiente",
            settings.ambiente.as_ref(),
            ToString::to_string,
        ),
        row("clientId", "client_id", settings.client_id.as_ref(), |id| {
            output::mask(id, 4)
        }),
        row(
            "clientSecret",
            "client_secret",
            settings.client_secret.as_ref(),
            |_| "definido (oculto)".to_owned(),
        ),
        row(
            "certificado",
            "Certificado",
            settings.certificado.as_ref(),
            |p| p.display().to_string(),
        ),
        row(
            "chavePrivada",
            "Chave privada",
            settings.chave_privada.as_ref(),
            |p| p.display().to_string(),
        ),
        row(
            "contaCorrente",
            "Conta corrente",
            settings.conta_corrente.as_ref(),
            |c| output::mask(c, 2),
        ),
        row(
            "escopos",
            "Escopos adicionais",
            settings.escopos.as_ref(),
            ToString::to_string,
        ),
        row(
            "urlBase",
            "URL base",
            settings.base_url.as_ref(),
            Clone::clone,
        ),
    ]
}
