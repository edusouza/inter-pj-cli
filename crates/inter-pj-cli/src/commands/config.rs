//! `inter-pj config init|caminho|mostrar|verificar`

use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use inter_pj::ClientIdentity;
use secrecy::zeroize::Zeroizing;
use serde::Serialize;
use serde_json::{Value, json};

use super::Context;
use crate::chamada::chamada;
use crate::cli::{ConfigCommand, Formato, InitArgs, VerificarArgs};
use crate::config::{ConfigFile, LoadedConfig, Setting, Settings, TEMPLATE};
use crate::confirmacao::Stdio;
use crate::doctor::{self, Aspas};
use crate::error::CliError;
use crate::files::{create_private, write_private};
use crate::output;

pub(super) fn run(context: &Context, command: &ConfigCommand) -> Result<(), CliError> {
    match command {
        ConfigCommand::Init(args) => init(context, args),
        ConfigCommand::Caminho => caminho(context),
        ConfigCommand::Mostrar => mostrar(context),
        ConfigCommand::Verificar(args) => verificar(context, args),
    }
}

fn init(context: &Context, args: &InitArgs) -> Result<(), CliError> {
    if args.interativo {
        return super::assistente::run(context, args, &mut Stdio);
    }
    let path = context.config_path();
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .map_err(|err| CliError::io(format!("falha ao criar {}", parent.display()), err))?;
    }
    // Without --forcar, the file is created only if nothing is there, which
    // the OS checks when creating it (a link included).
    let gravado = if args.forcar {
        write_private(path, TEMPLATE.as_bytes())
    } else {
        create_private(path, TEMPLATE.as_bytes())
    };
    gravado.map_err(|err| {
        if err.kind() == io::ErrorKind::AlreadyExists {
            CliError::Config(format!(
                "o arquivo {} já existe; use --forcar para sobrescrevê-lo",
                path.display()
            ))
        } else {
            CliError::io(format!("falha ao gravar {}", path.display()), err)
        }
    })?;
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
            "limitePorOperacao",
            "Limite por operação",
            settings.limite_por_operacao.as_ref(),
            |valor| output::brl(*valor),
        ),
        row(
            "urlBase",
            "URL base",
            settings.base_url.as_ref(),
            Clone::clone,
        ),
    ]
}

/// How serious a finding of `config verificar` is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Nivel {
    Ok,
    Corrigido,
    Aviso,
    Erro,
}

impl Nivel {
    fn rotulo(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Corrigido => "corrigido",
            Self::Aviso => "aviso",
            Self::Erro => "erro",
        }
    }
}

/// A finding of `config verificar`.
#[derive(Debug, Serialize)]
struct Achado {
    nivel: Nivel,
    item: String,
    mensagem: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    linha: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sugestao: Option<String>,
    /// Whether `--corrigir` fixes it.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    corrigivel: bool,
}

impl Achado {
    fn novo(nivel: Nivel, item: impl Into<String>, mensagem: impl Into<String>) -> Self {
        Self {
            nivel,
            item: item.into(),
            mensagem: mensagem.into(),
            linha: None,
            sugestao: None,
            corrigivel: false,
        }
    }
}

/// Checks the configuration file and the selected profile: the syntax (with
/// Windows paths between double quotes explained line by line), the
/// settings the commands need, and the certificate and the private key.
fn verificar(context: &Context, args: &VerificarArgs) -> Result<(), CliError> {
    let path = context.config_path();
    let mut achados = Vec::new();
    let mut copia = None;
    // The text may carry the client_secret: zeroed when dropped, like the
    // one the other commands read.
    let texto = match fs::read_to_string(path) {
        Ok(texto) => Some(Zeroizing::new(texto)),
        Err(err) if err.kind() == io::ErrorKind::NotFound => None,
        Err(err) => {
            return Err(CliError::io(
                format!("falha ao ler {}", path.display()),
                err,
            ));
        }
    };
    let texto = match texto {
        Some(texto) if args.corrigir => {
            let (corrigido, linhas) = doctor::corrigir(&texto);
            if !linhas.is_empty() {
                let original = guardar_copia(path, &texto)?;
                gravar(path, &corrigido)?;
                achados.extend(linhas.iter().map(|problema| Achado {
                    linha: Some(problema.linha),
                    ..Achado::novo(
                        Nivel::Corrigido,
                        format!("linha {}, {}", problema.linha, problema.chave),
                        "aspas simples no lugar das aspas duplas",
                    )
                }));
                copia = Some(original);
            }
            Some(Zeroizing::new(corrigido))
        }
        texto => texto,
    };

    let loaded = match &texto {
        None => {
            achados.push(Achado::novo(
                Nivel::Aviso,
                "arquivo",
                format!(
                    "não existe: crie com `{} config init`, ou informe tudo por flags e variáveis de ambiente",
                    chamada()
                ),
            ));
            Some(LoadedConfig {
                path: path.clone(),
                exists: false,
                file: ConfigFile::default(),
            })
        }
        Some(texto) => sintaxe(path, texto, &mut achados),
    };
    if let Some(loaded) = loaded {
        match Settings::resolve(&loaded, context.inputs()) {
            Ok(settings) => perfil(&settings, &mut achados),
            Err(err) => achados.push(Achado::novo(Nivel::Erro, "perfil", err.to_string())),
        }
    }
    relatorio(context, texto.is_some(), copia.as_deref(), &achados)
}

/// The findings about the text of the file, and the file when it loads.
fn sintaxe(path: &Path, texto: &str, achados: &mut Vec<Achado>) -> Option<LoadedConfig> {
    let aspas = doctor::aspas_duplas(texto);
    achados.extend(aspas.iter().map(achado_de_aspas));
    match toml::from_str::<ConfigFile>(texto) {
        Ok(file) => {
            achados.push(Achado::novo(
                Nivel::Ok,
                "sintaxe",
                "o arquivo é TOML válido",
            ));
            Some(LoadedConfig {
                path: path.to_path_buf(),
                exists: true,
                file,
            })
        }
        Err(err) => {
            // The parser's message, without the line itself, which may hold
            // the client_secret; an escape of a line already reported is not
            // repeated.
            let linha = err
                .span()
                .map(|span| crate::config::line_of(texto, span.start));
            let explicada = aspas
                .iter()
                .any(|problema| problema.invalida && Some(problema.linha) == linha);
            if !explicada {
                achados.push(Achado {
                    linha,
                    ..Achado::novo(
                        Nivel::Erro,
                        linha.map_or_else(|| "sintaxe".to_owned(), |n| format!("linha {n}")),
                        err.message(),
                    )
                });
            }
            None
        }
    }
}

fn achado_de_aspas(problema: &Aspas) -> Achado {
    let mensagem = match (problema.caminho.is_some(), problema.invalida) {
        (true, true) => {
            "caminho do Windows entre aspas duplas: em TOML, a barra invertida começa um escape (\\U pede 8 dígitos hexadecimais), e o arquivo não é lido"
        }
        (true, false) => {
            "caminho do Windows entre aspas duplas: em TOML, \\n, \\t e outros escapes viram caracteres de controle, e o caminho lido não é o digitado"
        }
        (false, _) => {
            "barra invertida entre aspas duplas forma um escape que o TOML recusa (o valor não é mostrado)"
        }
    };
    let sugestao = match (problema.correcao_visivel(), &problema.correcao) {
        (Some(linha), _) => format!("corrija para: {}", linha.trim()),
        (None, Some(_)) => "use aspas simples no valor".to_owned(),
        (None, None) => "use aspas simples ou dobre as barras invertidas (\\\\)".to_owned(),
    };
    Achado {
        linha: Some(problema.linha),
        sugestao: Some(sugestao),
        corrigivel: problema.correcao.is_some(),
        ..Achado::novo(
            Nivel::Erro,
            format!("linha {}, {}", problema.linha, problema.chave),
            mensagem,
        )
    }
}

/// The findings about the settings of the selected profile.
fn perfil(settings: &Settings, achados: &mut Vec<Achado>) {
    let ok = |item: &str, mensagem: String| Achado::novo(Nivel::Ok, item, mensagem);
    let faltando =
        |item: &str, como: &str| Achado::novo(Nivel::Erro, item, format!("não definido: {como}"));
    achados.push(ok(
        "perfil",
        format!("{} ({})", settings.perfil.value, settings.perfil.source),
    ));
    achados.push(match &settings.ambiente {
        Some(ambiente) => ok(
            "ambiente",
            format!("{} ({})", ambiente.value, ambiente.source),
        ),
        None => faltando(
            "ambiente",
            "use --ambiente, INTER_AMBIENTE ou `ambiente` no perfil",
        ),
    });
    achados.push(match &settings.client_id {
        Some(id) => ok(
            "client_id",
            format!("{} ({})", output::mask(&id.value, 4), id.source),
        ),
        None => faltando(
            "client_id",
            "use --client-id, INTER_CLIENT_ID ou `client_id` no perfil",
        ),
    });
    achados.push(match &settings.client_secret {
        Some(secret) => ok(
            "client_secret",
            format!("definido, oculto ({})", secret.source),
        ),
        None => faltando(
            "client_secret",
            "defina INTER_CLIENT_SECRET ou `client_secret` no perfil",
        ),
    });
    let certificado = arquivo(
        achados,
        "certificado",
        settings.certificado.as_ref(),
        "use --certificado, INTER_CERTIFICADO ou `certificado` no perfil",
    );
    let chave = arquivo(
        achados,
        "chave_privada",
        settings.chave_privada.as_ref(),
        "use --chave-privada, INTER_CHAVE_PRIVADA ou `chave_privada` no perfil",
    );
    if let (Some(certificado), Some(chave)) = (certificado, chave) {
        achados.push(match ClientIdentity::from_pem_files(certificado, chave) {
            Ok(_) => ok(
                "certificado e chave",
                "lidos e aceitos pela biblioteca TLS".to_owned(),
            ),
            Err(err) => Achado::novo(Nivel::Erro, "certificado e chave", err.to_string()),
        });
    }
    if let Some(conta) = &settings.conta_corrente {
        achados.push(if conta_valida(&conta.value) {
            ok(
                "conta_corrente",
                format!("{} ({})", output::mask(conta.value.trim(), 2), conta.source),
            )
        } else {
            // Never echo the value: it is the user's account number.
            Achado::novo(
                Nivel::Erro,
                "conta_corrente",
                format!(
                    "inválida: use apenas dígitos (incluindo o dígito verificador), sem zeros à esquerda ({})",
                    conta.source
                ),
            )
        });
    }
    if let Some(escopos) = &settings.escopos {
        achados.push(ok(
            "escopos",
            format!("{} ({})", escopos.value, escopos.source),
        ));
    }
    if let Some(url) = &settings.base_url {
        achados.push(ok("url_base", format!("{} ({})", url.value, url.source)));
    }
    achados.extend(
        settings
            .warnings
            .iter()
            .map(|warning| Achado::novo(Nivel::Aviso, "permissões", warning.clone())),
    );
}

/// The rule of the library for the account of the requests: digits only,
/// with the check digit and without leading zeros.
fn conta_valida(conta: &str) -> bool {
    let conta = conta.trim();
    !conta.is_empty()
        && conta.len() <= 20
        && conta.bytes().all(|b| b.is_ascii_digit())
        && !conta.starts_with('0')
}

/// The finding about a file of the profile, and the file when it exists.
fn arquivo<'a>(
    achados: &mut Vec<Achado>,
    item: &str,
    setting: Option<&'a Setting<PathBuf>>,
    como: &str,
) -> Option<&'a Path> {
    let Some(setting) = setting else {
        achados.push(Achado::novo(
            Nivel::Erro,
            item,
            format!("não definido: {como}"),
        ));
        return None;
    };
    let caminho = setting.value.display().to_string();
    if caminho.chars().any(char::is_control) {
        achados.push(Achado::novo(
            Nivel::Erro,
            item,
            format!(
                "o caminho lido tem um caractere de controle (\"{}\"): escreva-o entre aspas simples",
                doctor::escapar_controles(&caminho)
            ),
        ));
        None
    } else if setting.value.is_file() {
        achados.push(Achado::novo(
            Nivel::Ok,
            item,
            format!("{caminho} ({})", setting.source),
        ));
        Some(setting.value.as_path())
    } else {
        achados.push(Achado::novo(
            Nivel::Erro,
            item,
            format!("arquivo não encontrado: {caminho} ({})", setting.source),
        ));
        None
    }
}

fn relatorio(
    context: &Context,
    existe: bool,
    copia: Option<&Path>,
    achados: &[Achado],
) -> Result<(), CliError> {
    let conta = |nivel: Nivel| {
        achados
            .iter()
            .filter(|achado| achado.nivel == nivel)
            .count()
    };
    let (erros, avisos) = (conta(Nivel::Erro), conta(Nivel::Aviso));
    let path = context.config_path();
    match context.formato() {
        Formato::Json => output::print_json(&json!({
            "arquivo": path,
            "existe": existe,
            "copia": copia,
            "achados": achados,
            "erros": erros,
            "avisos": avisos,
        }))?,
        Formato::Texto | Formato::Csv => {
            let mut texto = format!("Arquivo: {}\n\n", path.display());
            for achado in achados {
                let _ = writeln!(
                    texto,
                    "{:<9} {}: {}",
                    achado.nivel.rotulo(),
                    achado.item,
                    achado.mensagem
                );
                if let Some(sugestao) = &achado.sugestao {
                    let _ = writeln!(texto, "{:<9} {sugestao}", "");
                }
            }
            if let Some(copia) = copia {
                let _ = write!(texto, "\nCópia do arquivo original: {}\n", copia.display());
            }
            texto.push('\n');
            if erros == 0 && avisos == 0 {
                texto.push_str("Nenhum problema encontrado.");
            } else {
                let _ = write!(
                    texto,
                    "{} e {}.",
                    plural(erros, "erro", "erros"),
                    plural(avisos, "aviso", "avisos")
                );
            }
            if achados.iter().any(|achado| achado.corrigivel) {
                let _ = write!(
                    texto,
                    "\nPara trocar as aspas: {} config verificar --corrigir",
                    chamada()
                );
            }
            output::print(&texto)?;
        }
    }
    if erros > 0 {
        return Err(CliError::Config(format!(
            "a configuração tem {}",
            plural(erros, "erro", "erros")
        )));
    }
    Ok(())
}

fn plural(quantos: usize, singular: &str, varios: &str) -> String {
    format!("{quantos} {}", if quantos == 1 { singular } else { varios })
}

/// Keeps `texto` in `config.toml.bak`, next to `config.toml`, or in
/// `config.toml.bak.2`, `.3`... when an earlier copy exists: a copy is never
/// overwritten, which the OS checks when creating the file (a link
/// included, never followed).
fn guardar_copia(path: &Path, texto: &str) -> Result<PathBuf, CliError> {
    let com_sufixo = |sufixo: &str| {
        let mut nome = path.file_name().unwrap_or_default().to_os_string();
        nome.push(sufixo);
        path.with_file_name(nome)
    };
    let mut copia = com_sufixo(".bak");
    let mut numero = 2;
    loop {
        match create_private(&copia, texto.as_bytes()) {
            Ok(()) => return Ok(copia),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                copia = com_sufixo(&format!(".bak.{numero}"));
                numero += 1;
            }
            Err(err) => {
                return Err(CliError::io(
                    format!("falha ao gravar {}", copia.display()),
                    err,
                ));
            }
        }
    }
}

fn gravar(path: &Path, texto: &str) -> Result<(), CliError> {
    write_private(path, texto.as_bytes())
        .map_err(|err| CliError::io(format!("falha ao gravar {}", path.display()), err))
}
