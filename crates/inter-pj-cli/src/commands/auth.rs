//! `inter-pj auth token|limpar|certificado`

use std::fs;

use chrono::{DateTime, Local, TimeZone, Utc};
use inter_pj::{CertificateInfo, ScopeSet};
use secrecy::ExposeSecret;
use serde_json::json;

use super::Context;
use crate::chamada::chamada;
use crate::cli::{AuthCommand, CertificadoArgs, Formato, LimparArgs, TokenArgs};
use crate::error::CliError;
use crate::output::{self, limpo, secao};
use crate::token_store::FileTokenStore;

/// Days before the end of the validity from which every command warns.
const DIAS_DE_AVISO: i64 = 30;

/// Days before the end of the validity from which Inter renews it.
const DIAS_DE_RENOVACAO: i64 = 90;

pub(super) async fn run(context: &Context, command: AuthCommand) -> Result<(), CliError> {
    match command {
        AuthCommand::Token(args) => token(context, &args).await,
        AuthCommand::Limpar(args) => limpar(context, &args),
        AuthCommand::Certificado(args) => certificado(context, &args),
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
        // `commands::run` refuses csv for this command.
        Formato::Texto | Formato::Csv => {
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
            "o perfil \"{}\" não tem ambiente e client_id definidos; use `{} auth limpar --todos`",
            settings.perfil.value,
            chamada()
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

/// Where a certificate stands at a moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Situacao {
    /// Its validity has not started.
    AindaNaoVale,
    /// Its validity is over: Inter refuses the connection.
    Vencido,
    /// Less than [`DIAS_DE_AVISO`] days are left.
    Vencendo { dias: i64 },
    /// Less than [`DIAS_DE_RENOVACAO`] days are left: it can be renewed.
    Renovavel { dias: i64 },
    /// Valid, with `dias` days left.
    Valido { dias: i64 },
}

impl Situacao {
    fn de(certificado: &CertificateInfo, agora: DateTime<Utc>) -> Self {
        if agora < certificado.not_before {
            return Self::AindaNaoVale;
        }
        if agora >= certificado.not_after {
            return Self::Vencido;
        }
        match (certificado.not_after - agora).num_days() {
            dias if dias < DIAS_DE_AVISO => Self::Vencendo { dias },
            dias if dias < DIAS_DE_RENOVACAO => Self::Renovavel { dias },
            dias => Self::Valido { dias },
        }
    }

    /// The code of the JSON output.
    fn codigo(self) -> &'static str {
        match self {
            Self::AindaNaoVale => "ainda-nao-vale",
            Self::Vencido => "vencido",
            Self::Vencendo { .. } => "vencendo",
            Self::Renovavel { .. } => "renovavel",
            Self::Valido { .. } => "valido",
        }
    }
}

/// `em 10 dias`, `amanhã`, `hoje`.
fn em(dias: i64) -> String {
    match dias {
        0 => "hoje".to_owned(),
        1 => "amanhã".to_owned(),
        n => format!("em {n} dias"),
    }
}

/// The situation of a certificate in words, with the times in `fuso`.
fn descrever<Tz: TimeZone>(certificado: &CertificateInfo, agora: DateTime<Utc>, fuso: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let quando = |momento: DateTime<Utc>| {
        momento
            .with_timezone(fuso)
            .format("%d/%m/%Y %H:%M:%S")
            .to_string()
    };
    match Situacao::de(certificado, agora) {
        Situacao::AindaNaoVale => format!(
            "ainda não vale: a validade começa em {}",
            quando(certificado.not_before)
        ),
        Situacao::Vencido => format!(
            "vencido em {}: o Inter recusa a conexão; gere um novo certificado no Internet Banking PJ",
            quando(certificado.not_after)
        ),
        Situacao::Vencendo { dias } => format!(
            "vence {}: renove-o no Internet Banking PJ (a renovação mantém o client_id e o client_secret)",
            em(dias)
        ),
        Situacao::Renovavel { dias } => format!(
            "válido; faltam {dias} dias, e a renovação já está aberta no Internet Banking PJ"
        ),
        Situacao::Valido { dias } => format!("válido; faltam {dias} dias"),
    }
}

/// What every command warns on stderr: a certificate that expired, is about
/// to expire or does not hold yet.
pub(super) fn aviso_de_validade(
    certificado: &CertificateInfo,
    agora: DateTime<Utc>,
) -> Option<String> {
    aviso_de_validade_em(certificado, agora, &Local)
}

fn aviso_de_validade_em<Tz: TimeZone>(
    certificado: &CertificateInfo,
    agora: DateTime<Utc>,
    fuso: &Tz,
) -> Option<String>
where
    Tz::Offset: std::fmt::Display,
{
    let quando = |momento: DateTime<Utc>| momento.with_timezone(fuso).format("%d/%m/%Y %H:%M");
    let texto = match Situacao::de(certificado, agora) {
        Situacao::AindaNaoVale => format!(
            "ainda não vale: a validade começa em {}",
            quando(certificado.not_before)
        ),
        Situacao::Vencido => format!(
            "venceu em {}: o Inter recusa a conexão; gere um novo no Internet Banking PJ",
            quando(certificado.not_after)
        ),
        Situacao::Vencendo { dias } => format!(
            "vence {} ({}): renove-o no Internet Banking PJ, o que mantém o client_id e o client_secret",
            em(dias),
            quando(certificado.not_after)
        ),
        Situacao::Renovavel { .. } | Situacao::Valido { .. } => return None,
    };
    Some(format!("o certificado da integração {texto}"))
}

fn certificado(context: &Context, args: &CertificadoArgs) -> Result<(), CliError> {
    let caminho = if let Some(caminho) = &args.arquivo {
        caminho.clone()
    } else {
        let settings = context.settings()?;
        settings
            .certificado
            .as_ref()
            .map(|certificado| certificado.value.clone())
            .ok_or_else(|| {
                CliError::Config(format!(
                    "o perfil \"{}\" não tem certificado definido: configure `certificado` ou informe --arquivo",
                    settings.perfil.value
                ))
            })?
    };
    let pem = fs::read(&caminho).map_err(|err| {
        CliError::io(
            format!("não foi possível ler o certificado {}", caminho.display()),
            err,
        )
    })?;
    let certificado = CertificateInfo::from_pem(&pem).map_err(inter_pj::Error::from)?;
    let agora = Utc::now();
    match context.formato() {
        Formato::Json => {
            let situacao = Situacao::de(&certificado, agora);
            let dias = (certificado.not_after - agora).num_days();
            output::print_json(&json!({
                "arquivo": caminho.display().to_string(),
                "titular": certificado.subject.to_string(),
                "emissor": certificado.issuer.to_string(),
                "numeroSerie": certificado.serial,
                "validoDesde": certificado.not_before.to_rfc3339(),
                "validoAte": certificado.not_after.to_rfc3339(),
                "diasRestantes": dias,
                "situacao": situacao.codigo(),
            }))
        }
        // `commands::run` refuses csv for this command.
        Formato::Texto | Formato::Csv => output::print(&render_certificado_em(
            &certificado,
            &caminho.display().to_string(),
            agora,
            &Local,
        )),
    }
}

/// A certificate in detail, with the times in `fuso`.
fn render_certificado_em<Tz: TimeZone>(
    certificado: &CertificateInfo,
    arquivo: &str,
    agora: DateTime<Utc>,
    fuso: &Tz,
) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let quando = |momento: DateTime<Utc>| {
        momento
            .with_timezone(fuso)
            .format("%d/%m/%Y %H:%M:%S")
            .to_string()
    };
    let linhas = [
        ("Arquivo", arquivo.to_owned()),
        (
            "Titular",
            limpo(&certificado.subject.to_string()).into_owned(),
        ),
        (
            "Emissor",
            limpo(&certificado.issuer.to_string()).into_owned(),
        ),
        ("Número de série", certificado.serial.clone()),
        ("Válido desde", quando(certificado.not_before)),
        ("Válido até", quando(certificado.not_after)),
        ("Situação", descrever(certificado, agora, fuso)),
    ];
    secao("Certificado da integração", &linhas)
}

#[cfg(test)]
mod tests {
    use chrono::{FixedOffset, TimeDelta};
    use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair, date_time_ymd};

    use super::*;

    /// A certificate valid from `inicio` to `fim`, at midnight UTC.
    fn certificado(inicio: (i32, u8, u8), fim: (i32, u8, u8)) -> CertificateInfo {
        let mut params = CertificateParams::new(vec!["cliente.teste".to_owned()]).unwrap();
        params.distinguished_name = DistinguishedName::new();
        params
            .distinguished_name
            .push(DnType::OrganizationName, "Empresa Exemplo Ltda");
        params
            .distinguished_name
            .push(DnType::CommonName, "Integração Exemplo");
        params.not_before = date_time_ymd(inicio.0, inicio.1, inicio.2);
        params.not_after = date_time_ymd(fim.0, fim.1, fim.2);
        let chave = KeyPair::generate().unwrap();
        let pem = params.self_signed(&chave).unwrap().pem();
        CertificateInfo::from_pem(pem.as_bytes()).unwrap()
    }

    fn brasilia() -> FixedOffset {
        FixedOffset::west_opt(3 * 3600).unwrap()
    }

    #[test]
    fn where_a_certificate_stands() {
        let certificado = certificado((2025, 10, 1), (2026, 10, 1));
        let antes_do_fim = |dias| certificado.not_after - TimeDelta::days(dias);
        assert_eq!(
            Situacao::de(&certificado, antes_do_fim(200)),
            Situacao::Valido { dias: 200 }
        );
        assert_eq!(
            Situacao::de(&certificado, antes_do_fim(60)),
            Situacao::Renovavel { dias: 60 }
        );
        assert_eq!(
            Situacao::de(&certificado, antes_do_fim(10)),
            Situacao::Vencendo { dias: 10 }
        );
        // A second before the end is still today.
        assert_eq!(
            Situacao::de(&certificado, certificado.not_after - TimeDelta::seconds(1)),
            Situacao::Vencendo { dias: 0 }
        );
        assert_eq!(
            Situacao::de(&certificado, certificado.not_after),
            Situacao::Vencido
        );
        assert_eq!(
            Situacao::de(&certificado, certificado.not_before - TimeDelta::seconds(1)),
            Situacao::AindaNaoVale
        );
        assert_eq!(Situacao::Renovavel { dias: 1 }.codigo(), "renovavel");
    }

    #[test]
    fn only_what_needs_attention_warns() {
        let certificado = certificado((2025, 10, 1), (2026, 10, 1));
        let aviso = |agora| aviso_de_validade_em(&certificado, agora, &brasilia());
        assert_eq!(aviso(certificado.not_after - TimeDelta::days(200)), None);
        // The renewal is open, but there is time: no warning yet.
        assert_eq!(aviso(certificado.not_after - TimeDelta::days(60)), None);
        assert_eq!(
            aviso(certificado.not_after - TimeDelta::days(10)).unwrap(),
            "o certificado da integração vence em 10 dias (30/09/2026 21:00): renove-o no Internet Banking PJ, o que mantém o client_id e o client_secret"
        );
        assert!(
            aviso(certificado.not_after - TimeDelta::hours(30))
                .unwrap()
                .starts_with("o certificado da integração vence amanhã (")
        );
        assert_eq!(
            aviso(certificado.not_after + TimeDelta::days(3)).unwrap(),
            "o certificado da integração venceu em 30/09/2026 21:00: o Inter recusa a conexão; gere um novo no Internet Banking PJ"
        );
        assert_eq!(
            aviso(certificado.not_before - TimeDelta::days(1)).unwrap(),
            "o certificado da integração ainda não vale: a validade começa em 30/09/2025 21:00"
        );
    }

    #[test]
    fn a_certificate_in_detail() {
        let certificado = certificado((2025, 10, 1), (2026, 10, 1));
        let agora = certificado.not_after - TimeDelta::days(200);
        let texto = render_certificado_em(
            &certificado,
            "/etc/inter/certificado.crt",
            agora,
            &brasilia(),
        );
        assert_eq!(
            texto,
            format!(
                "\
Certificado da integração
  Arquivo          /etc/inter/certificado.crt
  Titular          CN=Integração Exemplo, O=Empresa Exemplo Ltda
  Emissor          CN=Integração Exemplo, O=Empresa Exemplo Ltda
  Número de série  {}
  Válido desde     30/09/2025 21:00:00
  Válido até       30/09/2026 21:00:00
  Situação         válido; faltam 200 dias",
                certificado.serial
            )
        );
        let vencido = render_certificado_em(
            &certificado,
            "certificado.crt",
            certificado.not_after + TimeDelta::days(1),
            &brasilia(),
        );
        assert!(
            vencido.ends_with("Situação         vencido em 30/09/2026 21:00:00: o Inter recusa a conexão; gere um novo certificado no Internet Banking PJ"),
            "{vencido}"
        );
    }
}
