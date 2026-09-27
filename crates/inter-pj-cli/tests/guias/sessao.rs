//! A terminal session of the guides: a home of its own, `/home/voce` in
//! what the commands print, and each command line parsed as a shell would.

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, Issuer, KeyPair,
    date_time_ymd,
};
use tempfile::TempDir;

use crate::markdown::Comando;

/// The `client_secret` of the session, synthetic.
pub(crate) const CLIENT_SECRET: &str = "segredo-dos-guias-que-nunca-deve-vazar";

/// Where the session's home is in what the commands print.
const CASA: &str = "/home/voce";

/// Today in the guides (`INTER_HOJE`), so that their dates do not go stale.
pub(crate) const HOJE: &str = "2026-09-24";

#[derive(Debug)]
pub(crate) struct Sessao {
    dir: TempDir,
    servidor: String,
}

/// What a command printed, stdout and stderr in order, with the session's
/// home as `/home/voce`.
#[derive(Debug)]
pub(crate) struct Execucao {
    pub(crate) saida: String,
    pub(crate) sucesso: bool,
}

impl Execucao {
    /// The lines, without the spaces at the end and the blank lines after.
    pub(crate) fn linhas(&self) -> Vec<&str> {
        let mut linhas: Vec<&str> = self.saida.lines().map(str::trim_end).collect();
        while linhas.last().is_some_and(|linha| linha.is_empty()) {
            linhas.pop();
        }
        linhas
    }
}

impl Sessao {
    pub(crate) fn nova(servidor: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let sessao = Self {
            dir,
            servidor: servidor.to_owned(),
        };
        let casa = sessao.casa();
        fs::create_dir_all(casa.join(".config/inter-pj")).unwrap();
        fs::create_dir_all(casa.join("inter")).unwrap();
        certificado(&casa.join("inter"));
        privado(
            &casa.join(".config/inter-pj/config.toml"),
            "perfil_padrao = \"padrao\"\n\n\
             [perfis.padrao]\n\
             ambiente = \"producao\"\n\
             client_id = \"id-da-integracao-dos-guias\"\n\
             certificado = \"~/inter/certificado.crt\"\n\
             chave_privada = \"~/inter/chave.key\"\n\
             limite_por_operacao = \"20.000,00\"\n\n\
             [perfis.sandbox]\n\
             ambiente = \"sandbox\"\n\
             client_id = \"id-da-integracao-dos-guias\"\n\
             certificado = \"~/inter/certificado.crt\"\n\
             chave_privada = \"~/inter/chave.key\"\n",
        );
        sessao
    }

    /// Writes a file the guide shows, in the session's home.
    pub(crate) fn gravar(&self, nome: &str, conteudo: &str) {
        let caminho = self.casa().join(nome);
        if let Some(pasta) = caminho.parent() {
            fs::create_dir_all(pasta).unwrap();
        }
        fs::write(caminho, conteudo).unwrap();
    }

    /// The real directory that the commands see as `/home/voce`.
    fn casa(&self) -> PathBuf {
        self.dir.path().join(CASA.trim_start_matches('/'))
    }

    pub(crate) fn executar(&self, comando: &Comando) -> Execucao {
        let linha = match Linha::interpretar(&comando.texto) {
            Ok(linha) => linha,
            Err(erro) => {
                return Execucao {
                    saida: format!("o teste não sabe executar `{}`: {erro}", comando.texto),
                    sucesso: false,
                };
            }
        };
        match linha.programa.as_str() {
            "inter-pj" => self.inter_pj(&linha, comando.confirma()),
            "cat" => self.cat(&linha),
            outro => Execucao {
                saida: format!("o teste não executa `{outro}`: use <!-- guia: não executar -->"),
                sucesso: false,
            },
        }
    }

    fn cat(&self, linha: &Linha) -> Execucao {
        let mut saida = String::new();
        for arquivo in &linha.argumentos {
            match fs::read_to_string(self.casa().join(arquivo)) {
                Ok(texto) => saida.push_str(&texto),
                Err(err) => {
                    return Execucao {
                        saida: format!("cat: {arquivo}: {err}"),
                        sucesso: false,
                    };
                }
            }
        }
        Execucao {
            saida,
            sucesso: true,
        }
    }

    fn inter_pj(&self, linha: &Linha, confirma: bool) -> Execucao {
        let casa = self.casa();
        let mut comando = Command::new(env!("CARGO_BIN_EXE_inter-pj"));
        comando.args(&linha.argumentos);
        if confirma
            && !linha
                .argumentos
                .iter()
                .any(|a| a == "--sim" || a == "--simular")
        {
            comando.arg("--sim");
        }
        for (nome, _) in std::env::vars() {
            if nome.starts_with("INTER_") || nome.starts_with("XDG_") || nome.contains("COLOR") {
                comando.env_remove(nome);
            }
        }
        comando
            .current_dir(&casa)
            .env("HOME", &casa)
            .env("TZ", "America/Sao_Paulo")
            .env("NO_COLOR", "1")
            .env("INTER_BASE_URL", &self.servidor)
            .env("INTER_HOJE", HOJE)
            .env("INTER_CLIENT_SECRET", CLIENT_SECRET)
            .envs(linha.variaveis.iter().map(|(nome, valor)| (nome, valor)));
        comando.stdin(match &linha.entrada {
            Some(arquivo) => Stdio::from(File::open(casa.join(arquivo)).unwrap()),
            None => Stdio::null(),
        });
        // One pipe for both, as a terminal shows them: in the order written.
        let (mut leitor, escritor) = std::io::pipe().unwrap();
        match &linha.redirecionada {
            Some(arquivo) => comando.stdout(File::create(casa.join(arquivo)).unwrap()),
            None => comando.stdout(escritor.try_clone().unwrap()),
        };
        comando.stderr(escritor);
        let mut filho = comando.spawn().unwrap();
        // The copies of the writing end in `comando` must go, or the reading
        // would never end.
        drop(comando);
        let mut bytes = Vec::new();
        leitor.read_to_end(&mut bytes).unwrap();
        let status = filho.wait().unwrap();
        let raiz = self.dir.path().display().to_string();
        let saida = String::from_utf8_lossy(&bytes)
            .replace(&raiz, "")
            .replace(&self.servidor, endereco_da_api(linha));
        Execucao {
            saida: sem_a_url_do_banco(&saida),
            sucesso: status.success(),
        }
    }
}

/// What `config mostrar` shows without `INTER_BASE_URL`, which only the
/// mock needs: the URL of the API is not a setting of the guides.
fn sem_a_url_do_banco(saida: &str) -> String {
    let mut texto = String::with_capacity(saida.len());
    for linha in saida.split_inclusive('\n') {
        let url_do_banco = linha.starts_with("URL base ")
            && linha.trim_end().ends_with("(variável INTER_BASE_URL)");
        if url_do_banco {
            let valor = linha["URL base".len()..].trim_start();
            texto.push_str(&linha[..linha.len() - valor.len()]);
            texto.push_str("(não definido)\n");
        } else {
            texto.push_str(linha);
        }
    }
    texto
}

/// The address of the API that the profile of `linha` calls, which the
/// commands print (in a simulation) in place of the mock's.
fn endereco_da_api(linha: &Linha) -> &'static str {
    let variavel = |nome: &str| {
        linha
            .variaveis
            .iter()
            .find(|(variavel, _)| variavel == nome)
            .map(|(_, valor)| valor.as_str())
    };
    let mut perfil = variavel("INTER_PERFIL");
    let mut ambiente = variavel("INTER_AMBIENTE");
    let mut argumentos = linha.argumentos.iter().map(String::as_str);
    while let Some(argumento) = argumentos.next() {
        match argumento.split_once('=') {
            Some(("--perfil", valor)) => perfil = Some(valor),
            Some(("--ambiente", valor)) => ambiente = Some(valor),
            _ if argumento == "-p" || argumento == "--perfil" => perfil = argumentos.next(),
            _ if argumento == "--ambiente" => ambiente = argumentos.next(),
            _ => {}
        }
    }
    if ambiente.map_or(perfil == Some("sandbox"), |ambiente| ambiente == "sandbox") {
        "https://cdpj-sandbox.partners.uatinter.co"
    } else {
        "https://cdpj.partners.bancointer.com.br"
    }
}

/// A command line of the guides: variables, the program, its arguments and
/// the redirections, as a shell would read them.
#[derive(Debug, Default)]
struct Linha {
    variaveis: Vec<(String, String)>,
    programa: String,
    argumentos: Vec<String>,
    /// `< arquivo`.
    entrada: Option<String>,
    /// `> arquivo`.
    redirecionada: Option<String>,
}

impl Linha {
    fn interpretar(texto: &str) -> Result<Self, String> {
        let mut linha = Self::default();
        let mut palavras = palavras(texto)?.into_iter().peekable();
        while let Some((palavra, _)) = palavras.next_if(|(palavra, citada)| {
            !citada
                && palavra.split_once('=').is_some_and(|(nome, _)| {
                    !nome.is_empty() && nome.bytes().all(|b| b.is_ascii_uppercase() || b == b'_')
                })
        }) {
            let (nome, valor) = palavra.split_once('=').unwrap();
            linha.variaveis.push((nome.to_owned(), valor.to_owned()));
        }
        linha.programa = palavras.next().map(|(p, _)| p).ok_or("linha vazia")?;
        while let Some((palavra, citada)) = palavras.next() {
            let destino = |palavras: &mut std::iter::Peekable<_>| -> Result<String, String> {
                palavras
                    .next()
                    .map(|(arquivo, _): (String, bool)| arquivo)
                    .ok_or_else(|| format!("`{palavra}` sem arquivo"))
            };
            match (palavra.as_str(), citada) {
                (">", false) => linha.redirecionada = Some(destino(&mut palavras)?),
                ("<", false) => linha.entrada = Some(destino(&mut palavras)?),
                ("|" | ">>" | "2>" | "&&" | ";", false) => {
                    return Err(format!("`{palavra}` não é aceito nos exemplos testados"));
                }
                _ => linha.argumentos.push(palavra),
            }
        }
        Ok(linha)
    }
}

/// The words of `texto`, with the quotes and escapes of a shell, and whether
/// each had quotes. An unquoted `#` at the start of a word begins a comment.
fn palavras(texto: &str) -> Result<Vec<(String, bool)>, String> {
    let mut palavras = Vec::new();
    let mut atual: Option<(String, bool)> = None;
    let mut caracteres = texto.chars();
    while let Some(c) = caracteres.next() {
        match c {
            ' ' | '\t' => {
                if let Some(palavra) = atual.take() {
                    palavras.push(palavra);
                }
            }
            '#' if atual.is_none() => break,
            '\'' => {
                let palavra = atual.get_or_insert_with(|| (String::new(), true));
                palavra.1 = true;
                loop {
                    match caracteres.next() {
                        Some('\'') => break,
                        Some(c) => palavra.0.push(c),
                        None => return Err("aspas simples sem fim".to_owned()),
                    }
                }
            }
            '"' => {
                let palavra = atual.get_or_insert_with(|| (String::new(), true));
                palavra.1 = true;
                loop {
                    match caracteres.next() {
                        Some('"') => break,
                        Some('\\') => match caracteres.next() {
                            Some(c @ ('"' | '\\' | '$' | '`')) => palavra.0.push(c),
                            Some(c) => {
                                palavra.0.push('\\');
                                palavra.0.push(c);
                            }
                            None => return Err("aspas duplas sem fim".to_owned()),
                        },
                        Some(c) => palavra.0.push(c),
                        None => return Err("aspas duplas sem fim".to_owned()),
                    }
                }
            }
            '\\' => {
                let c = caracteres.next().ok_or("\\ no fim da linha")?;
                atual.get_or_insert_with(|| (String::new(), true)).0.push(c);
            }
            '$' | '`' | '*' | '?' | '~' if atual.is_none() => {
                return Err(format!("`{c}` precisaria do shell"));
            }
            c => atual
                .get_or_insert_with(|| (String::new(), false))
                .0
                .push(c),
        }
    }
    if let Some(palavra) = atual {
        palavras.push(palavra);
    }
    Ok(palavras)
}

fn privado(caminho: &Path, texto: &str) {
    use std::os::unix::fs::PermissionsExt;
    fs::write(caminho, texto).unwrap();
    fs::set_permissions(caminho, fs::Permissions::from_mode(0o600)).unwrap();
}

/// The certificate of the integration, signed by an authority of example,
/// and its key.
fn certificado(pasta: &Path) {
    let nome = |organizacao: &str, comum: &str| {
        let mut nome = DistinguishedName::new();
        nome.push(DnType::OrganizationName, organizacao);
        nome.push(DnType::CommonName, comum);
        nome
    };
    let mut ac = CertificateParams::new(Vec::new()).unwrap();
    ac.distinguished_name = nome("Banco Exemplo", "AC Exemplo");
    ac.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    let emissor = Issuer::new(ac, KeyPair::generate().unwrap());

    let mut params = CertificateParams::new(vec!["integracao.empresa.example".to_owned()]).unwrap();
    params.distinguished_name = nome("Empresa Exemplo Ltda", "Integração Exemplo");
    params.not_before = date_time_ymd(2025, 12, 5);
    params.not_after = date_time_ymd(2099, 12, 5);
    let chave = KeyPair::generate().unwrap();
    let certificado = params.signed_by(&chave, &emissor).unwrap();
    privado(&pasta.join("certificado.crt"), &certificado.pem());
    privado(&pasta.join("chave.key"), &chave.serialize_pem());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_url_of_the_mock_is_not_a_setting() {
        let saida = "Limite por operação  R$ 20.000,00 (arquivo)\n\
                     URL base             https://cdpj.partners.bancointer.com.br (variável INTER_BASE_URL)\n";
        assert_eq!(
            sem_a_url_do_banco(saida),
            "Limite por operação  R$ 20.000,00 (arquivo)\nURL base             (não definido)\n"
        );
        let outra = "URL base  https://api.empresa.example (arquivo)\n";
        assert_eq!(sem_a_url_do_banco(outra), outra);
    }

    #[test]
    fn lines_are_read_as_a_shell_would() {
        let linha = Linha::interpretar(
            "INTER_PERFIL=sandbox inter-pj pix enviar --descricao \"NF #1\" --chave 'a b' --valor 1,00 > saida.txt   # comentário",
        )
        .unwrap();
        assert_eq!(
            linha.variaveis,
            [("INTER_PERFIL".to_owned(), "sandbox".to_owned())]
        );
        assert_eq!(linha.programa, "inter-pj");
        assert_eq!(
            linha.argumentos,
            [
                "pix",
                "enviar",
                "--descricao",
                "NF #1",
                "--chave",
                "a b",
                "--valor",
                "1,00"
            ]
        );
        assert_eq!(linha.redirecionada.as_deref(), Some("saida.txt"));
        assert!(Linha::interpretar("inter-pj saldo | jq .").is_err());
        assert!(Linha::interpretar("inter-pj extrato \"sem fim").is_err());
        assert!(Linha::interpretar("inter-pj saldo --data $DATA").is_err());
    }
}
