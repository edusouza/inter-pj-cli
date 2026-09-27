//! `inter-pj config init --interativo`: the configuration wizard. It asks
//! for the profile, the environment, the `client_id`, the certificate and
//! the key, checked as they are given, and the checking account, and writes
//! the profile. The `client_secret` is never asked: it stays out of the
//! file.
//!
//! Nothing here moves money, so the answers may come from a pipe too.

use std::fmt::Write as _;
use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use chrono::{Local, Utc};
use inter_pj::{CertificateInfo, ClientIdentity, Environment};

use super::Context;
use super::auth::aviso_de_validade;
use crate::cli::InitArgs;
use crate::config::{self, DEFAULT_PROFILE, ENV_CLIENT_SECRET};
use crate::confirmacao::Terminal;
use crate::error::CliError;
use crate::files::write_private;
use crate::output;
use crate::paths;

/// Invalid answers in a row after which the wizard gives up.
const TENTATIVAS: usize = 5;

/// What the wizard asked, as it goes to the file.
#[derive(Debug)]
struct Perfil {
    nome: String,
    ambiente: Environment,
    client_id: String,
    certificado: String,
    chave_privada: String,
    conta_corrente: Option<String>,
}

pub(super) fn run(
    context: &Context,
    args: &InitArgs,
    terminal: &mut dyn Terminal,
) -> Result<(), CliError> {
    let caminho = context.config_path();
    // Without --forcar, an existing file keeps its profiles and gets one
    // more; the default one stays.
    let (existentes, padrao) = if caminho.exists() && !args.forcar {
        let arquivo = config::load(caminho)?.file;
        let padrao = arquivo
            .perfil_padrao
            .unwrap_or_else(|| DEFAULT_PROFILE.to_owned());
        (
            Some(arquivo.perfis.into_keys().collect::<Vec<_>>()),
            Some(padrao),
        )
    } else {
        (None, None)
    };
    eprintln!(
        "Assistente de configuração do inter-pj: {}. O client_secret não é perguntado; ele fica fora do arquivo.",
        caminho.display()
    );
    let base = std::env::current_dir()
        .map_err(|err| CliError::io("não foi possível ler o diretório atual", err))?;
    let perfil = perguntar_perfil(terminal, existentes.as_deref(), &base)?;
    // A new file makes it the default profile.
    let comando = match padrao {
        Some(padrao) if padrao != perfil.nome => {
            format!("inter-pj --perfil {} saldo", perfil.nome)
        }
        _ => "inter-pj saldo".to_owned(),
    };
    if existentes.is_some() {
        acrescentar(caminho, &perfil)?;
    } else {
        if let Some(pasta) = caminho.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(pasta)
                .map_err(|err| CliError::io(format!("falha ao criar {}", pasta.display()), err))?;
        }
        write_private(caminho, arquivo(&perfil).as_bytes())
            .map_err(|err| CliError::io(format!("falha ao gravar {}", caminho.display()), err))?;
    }
    // What was written must read back.
    config::load(caminho)?;
    output::print(&format!(
        "Perfil \"{}\" gravado em {}.\nPróximos passos:\n  1. defina a variável de ambiente {ENV_CLIENT_SECRET} com o client_secret da integração;\n  2. teste com: {comando}",
        perfil.nome,
        caminho.display()
    ))
}

/// Asks everything, each answer checked as it is given; relative paths are
/// taken from `base`, the current directory.
fn perguntar_perfil(
    terminal: &mut dyn Terminal,
    existentes: Option<&[String]>,
    base: &Path,
) -> Result<Perfil, CliError> {
    let padrao = match existentes {
        Some(nomes) if nomes.iter().any(|nome| nome == DEFAULT_PROFILE) => None,
        _ => Some(DEFAULT_PROFILE),
    };
    let nome = perguntar(terminal, "Nome do perfil", padrao, |nome| {
        nome_do_perfil(nome, existentes)
    })?;
    let ambiente = perguntar(
        terminal,
        "Ambiente (sandbox, com dados fictícios, ou producao)",
        Some("sandbox"),
        |texto| texto.parse::<Environment>().map_err(|err| err.to_string()),
    )?;
    let client_id = perguntar(terminal, "client_id da integração", None, |texto| {
        if texto.is_empty() || texto.chars().any(char::is_whitespace) {
            Err("o client_id é o identificador da integração, sem espaços".to_owned())
        } else {
            Ok(texto.to_owned())
        }
    })?;
    let (certificado, certificado_lido) =
        perguntar(terminal, "Certificado (.crt)", None, |texto| {
            let (escrito, lido) = caminho(texto, base)?;
            let pem = fs::read(&lido)
                .map_err(|err| format!("não foi possível ler {}: {err}", lido.display()))?;
            let info = CertificateInfo::from_pem(&pem).map_err(|err| err.to_string())?;
            eprintln!("  {}", descrever(&info));
            Ok((escrito, lido))
        })?;
    let chave_privada = perguntar(terminal, "Chave privada (.key)", None, |texto| {
        let (escrito, lido) = caminho(texto, base)?;
        ClientIdentity::from_pem_files(&certificado_lido, &lido).map_err(|err| err.to_string())?;
        eprintln!("  certificado e chave aceitos");
        Ok(escrito)
    })?;
    let conta_corrente = perguntar(
        terminal,
        "Conta corrente, só se a integração tiver mais de uma conta (Enter para pular)",
        Some(""),
        |texto| match texto {
            "" => Ok(None),
            conta if conta.bytes().all(|b| b.is_ascii_digit()) => Ok(Some(conta.to_owned())),
            _ => Err("a conta corrente vai só com os dígitos".to_owned()),
        },
    )?;
    Ok(Perfil {
        nome,
        ambiente,
        client_id,
        certificado,
        chave_privada,
        conta_corrente,
    })
}

/// Asks `pergunta` until `validar` accepts the answer: an empty one takes
/// `padrao`, the end of the input cancels and [`TENTATIVAS`] invalid ones
/// in a row give up.
fn perguntar<T>(
    terminal: &mut dyn Terminal,
    pergunta: &str,
    padrao: Option<&str>,
    mut validar: impl FnMut(&str) -> Result<T, String>,
) -> Result<T, CliError> {
    let rotulo = match padrao {
        Some(padrao) if !padrao.is_empty() => format!("{pergunta} [{padrao}]: "),
        _ => format!("{pergunta}: "),
    };
    for _ in 0..TENTATIVAS {
        let Some(linha) = terminal
            .perguntar(&rotulo)
            .map_err(|err| CliError::io("falha ao ler a resposta", err))?
        else {
            return Err(CliError::AssistenteInterrompido);
        };
        let resposta = match linha.trim() {
            "" => padrao.unwrap_or_default(),
            resposta => resposta,
        };
        match validar(resposta) {
            Ok(valor) => return Ok(valor),
            Err(problema) => eprintln!("  {problema}"),
        }
    }
    Err(CliError::Usage(format!(
        "{TENTATIVAS} respostas inválidas seguidas: nada foi gravado"
    )))
}

/// A name that is a TOML key without quotes and is not taken.
fn nome_do_perfil(nome: &str, existentes: Option<&[String]>) -> Result<String, String> {
    if nome.is_empty()
        || !nome
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err("o nome do perfil tem letras sem acento, dígitos, _ ou -".to_owned());
    }
    if existentes.is_some_and(|nomes| nomes.iter().any(|existente| existente == nome)) {
        return Err(format!(
            "o perfil \"{nome}\" já existe no arquivo: escolha outro nome, ou recrie o arquivo com --forcar"
        ));
    }
    Ok(nome.to_owned())
}

/// A path as it goes to the file and as it is read now: `~/` kept, a
/// relative one made absolute from `base` (the file's paths are relative to
/// the file, not to the current directory).
fn caminho(texto: &str, base: &Path) -> Result<(String, PathBuf), String> {
    if texto.is_empty() {
        return Err("informe o caminho do arquivo".to_owned());
    }
    let dado = Path::new(texto);
    if dado.starts_with("~") {
        return Ok((texto.to_owned(), paths::expand(dado, None)));
    }
    let absoluto = if dado.is_relative() {
        base.join(dado)
    } else {
        dado.to_path_buf()
    };
    Ok((absoluto.display().to_string(), absoluto))
}

/// `Integração Exemplo, válido até 05/12/2026`, and a warning when it
/// needs attention.
fn descrever(certificado: &CertificateInfo) -> String {
    let nome = certificado
        .subject
        .common_name()
        .map_or_else(|| certificado.subject.to_string(), str::to_owned);
    let mut texto = format!(
        "{}, válido até {}",
        output::limpo(&nome),
        certificado
            .not_after
            .with_timezone(&Local)
            .format("%d/%m/%Y")
    );
    if let Some(aviso) = aviso_de_validade(certificado, Utc::now()) {
        let _ = write!(texto, "\n  aviso: {aviso}");
    }
    texto
}

/// A string as TOML writes it, quoted and escaped.
fn toml(texto: &str) -> String {
    toml::Value::String(texto.to_owned()).to_string()
}

/// The section of the profile.
fn secao(perfil: &Perfil) -> String {
    let ambiente = if perfil.ambiente.is_production() {
        "producao"
    } else {
        "sandbox"
    };
    let mut texto = format!(
        "[perfis.{}]\nambiente = \"{ambiente}\"\nclient_id = {}\ncertificado = {}\nchave_privada = {}\n",
        perfil.nome,
        toml(&perfil.client_id),
        toml(&perfil.certificado),
        toml(&perfil.chave_privada),
    );
    match &perfil.conta_corrente {
        Some(conta) => {
            let _ = writeln!(texto, "conta_corrente = {}", toml(conta));
        }
        None => texto.push_str(
            "# Somente dígitos; necessário apenas se a integração tiver mais de uma conta.\n# conta_corrente = \"\"\n",
        ),
    }
    texto
}

/// A new file with `perfil` as the default one.
fn arquivo(perfil: &Perfil) -> String {
    format!(
        "# Configuração do inter-pj — CLI não oficial para a conta PJ do Inter Empresas.\n\
         # Criada por `inter-pj config init --interativo`. Documentação:\n\
         # https://github.com/edusouza/inter-pj-cli\n\
         #\n\
         # Este arquivo aponta para as credenciais da sua integração: nunca o versione\n\
         # nem o compartilhe. Mantenha a permissão 600 (somente o seu usuário).\n\
         # O client_secret fica na variável de ambiente {ENV_CLIENT_SECRET}.\n\
         \n\
         perfil_padrao = {}\n\
         \n\
         {}",
        toml(&perfil.nome),
        secao(perfil)
    )
}

/// Adds the section of `perfil` at the end of an existing file, which keeps
/// its comments.
fn acrescentar(caminho: &Path, perfil: &Perfil) -> Result<(), CliError> {
    let erro = |err| CliError::io(format!("falha ao gravar {}", caminho.display()), err);
    let atual = fs::read_to_string(caminho).map_err(erro)?;
    let mut texto = String::new();
    if !atual.is_empty() && !atual.ends_with('\n') {
        texto.push('\n');
    }
    texto.push('\n');
    texto.push_str(&secao(perfil));
    OpenOptions::new()
        .append(true)
        .open(caminho)
        .and_then(|mut arquivo| arquivo.write_all(texto.as_bytes()))
        .map_err(erro)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn perfil(conta: Option<&str>) -> Perfil {
        Perfil {
            nome: "padrao".to_owned(),
            ambiente: Environment::Sandbox,
            client_id: "id-de-teste".to_owned(),
            certificado: "~/inter/certificado.crt".to_owned(),
            chave_privada: "C:\\inter\\chave \"nova\".key".to_owned(),
            conta_corrente: conta.map(str::to_owned),
        }
    }

    #[test]
    fn the_file_is_the_profile_in_toml() {
        let texto = arquivo(&perfil(None));
        assert!(
            texto.ends_with(
                "\
perfil_padrao = \"padrao\"

[perfis.padrao]
ambiente = \"sandbox\"
client_id = \"id-de-teste\"
certificado = \"~/inter/certificado.crt\"
chave_privada = 'C:\\inter\\chave \"nova\".key'
# Somente dígitos; necessário apenas se a integração tiver mais de uma conta.
# conta_corrente = \"\"
"
            ),
            "{texto}"
        );
        // What is written reads back, the Windows path as it was given.
        let lido: config::ConfigFile = toml::from_str(&texto).unwrap();
        let padrao = &lido.perfis["padrao"];
        assert_eq!(
            padrao.chave_privada.as_deref(),
            Some(Path::new("C:\\inter\\chave \"nova\".key"))
        );
        assert!(secao(&perfil(Some("1234567"))).ends_with("conta_corrente = \"1234567\"\n"));
    }

    #[test]
    fn names_and_paths() {
        assert!(nome_do_perfil("producao-2", None).is_ok());
        assert!(nome_do_perfil("com espaço", None).is_err());
        assert!(nome_do_perfil("perfis.x", None).is_err());
        assert!(nome_do_perfil("", None).is_err());
        let existentes = ["padrao".to_owned()];
        assert!(nome_do_perfil("padrao", Some(&existentes)).is_err());
        assert!(nome_do_perfil("outro", Some(&existentes)).is_ok());

        let base = Path::new("/home/usuario/projeto");
        let (escrito, lido) = caminho("certificado.crt", base).unwrap();
        let esperado = base.join("certificado.crt");
        assert_eq!(escrito, esperado.display().to_string());
        assert_eq!(lido, esperado);
        let (escrito, _) = caminho("~/inter/certificado.crt", base).unwrap();
        assert_eq!(escrito, "~/inter/certificado.crt");
        assert!(caminho("", base).is_err());
    }
}
