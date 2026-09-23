//! `inter-pj auth token|limpar`

use chrono::{Local, Utc};
use inter_pj::ScopeSet;
use secrecy::ExposeSecret;
use serde_json::json;

use super::Context;
use crate::cli::{AuthCommand, Formato, LimparArgs, TokenArgs};
use crate::error::CliError;
use crate::output;
use crate::token_store::FileTokenStore;

pub(super) async fn run(context: &Context, command: AuthCommand) -> Result<(), CliError> {
    match command {
        AuthCommand::Token(args) => token(context, &args).await,
        AuthCommand::Limpar(args) => limpar(context, &args),
    }
}

async fn token(context: &Context, args: &TokenArgs) -> Result<(), CliError> {
    let settings = context.settings()?;
    let scopes = if args.escopos.is_empty() {
        settings
            .escopos
            .as_ref()
            .map(|escopos| escopos.value.clone())
            .filter(|escopos| !escopos.is_empty())
            .ok_or_else(|| {
                CliError::Usage(
                    "informe ao menos um --escopo (ex.: --escopo extrato.read) ou defina `escopos` no perfil"
                        .to_owned(),
                )
            })?
    } else {
        args.escopos
            .iter()
            .map(|name| name.parse())
            .collect::<Result<ScopeSet, _>>()
            .map_err(|err| CliError::Usage(err.to_string()))?
    };

    let client = context.client(&settings)?;
    let token = if args.renovar {
        client.renew_access_token(&scopes).await?
    } else {
        client.access_token(&scopes).await?
    };

    if args.exibir {
        return output::print(token.secret().expose_secret());
    }

    let ambiente = settings.ambiente.as_ref().map(|a| a.value.to_string());
    let expira_em = token.expires_at();
    match context.formato() {
        Formato::Json => output::print_json(&json!({
            "perfil": settings.perfil.value,
            "ambiente": ambiente,
            "escopos": token.scopes().to_string(),
            "expiraEm": expira_em.to_rfc3339(),
        })),
        Formato::Texto => {
            let minutos = (expira_em - Utc::now()).num_minutes().max(0);
            let rows = [
                ("Perfil", settings.perfil.value.clone()),
                ("Ambiente", ambiente.unwrap_or_default()),
                ("Escopos", token.scopes().to_string()),
                (
                    "Expira em",
                    format!(
                        "{} (em {minutos} min)",
                        expira_em.with_timezone(&Local).format("%d/%m/%Y %H:%M:%S")
                    ),
                ),
            ];
            output::print(&format!(
                "Token de acesso válido.\n{}",
                output::key_values_left(&rows)
            ))
        }
    }
}

fn limpar(context: &Context, args: &LimparArgs) -> Result<(), CliError> {
    let store = FileTokenStore::new(context.cache_dir());
    if args.todos {
        let removed = store
            .remove_all()
            .map_err(|err| CliError::io("falha ao limpar o cache de tokens", err))?;
        return output::print(&format!(
            "{removed} arquivo(s) de cache de tokens removido(s)."
        ));
    }

    let settings = context.settings()?;
    let (Some(base_url), Some(client_id)) = (settings.effective_base_url(), &settings.client_id)
    else {
        return Err(CliError::Config(format!(
            "o perfil \"{}\" não tem ambiente e client_id definidos; use `inter-pj auth limpar --todos`",
            settings.perfil.value
        )));
    };
    let key = inter_pj::auth::cache_key(&base_url, &client_id.value);
    let removed = store
        .remove(&key)
        .map_err(|err| CliError::io("falha ao limpar o cache de tokens", err))?;
    output::print(&if removed {
        format!(
            "Tokens em cache do perfil \"{}\" removidos.",
            settings.perfil.value
        )
    } else {
        format!(
            "Não havia tokens em cache para o perfil \"{}\".",
            settings.perfil.value
        )
    })
}
