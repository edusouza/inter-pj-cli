//! `inter-pj completions` and `inter-pj manual`, end to end: both come from
//! the command definition and need no configuration. The scripts are also
//! checked by the shells installed on the machine, and the pages by groff.

mod common;

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use common::{TestEnv, stderr_of, stdout_of};

/// Each shell and a line only its script has.
const SHELLS: [(&str, &str); 5] = [
    ("bash", "complete -F _inter__pj "),
    ("zsh", "#compdef inter-pj\n"),
    ("fish", "complete -c inter-pj "),
    (
        "powershell",
        "Register-ArgumentCompleter -Native -CommandName 'inter-pj'",
    ),
    ("elvish", "set edit:completion:arg-completer[inter-pj]"),
];

/// Whether `programa` runs here (`--version` answers).
fn instalado(programa: &str) -> bool {
    let instalado = Command::new(programa)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if !instalado {
        eprintln!("{programa} não está instalado: verificação pulada");
    }
    instalado
}

fn script(env: &TestEnv, shell: &str) -> String {
    let assert = env.cmd().args(["completions", shell]).assert().success();
    assert_eq!(stderr_of(&assert), "");
    stdout_of(&assert)
}

#[tokio::test(flavor = "multi_thread")]
async fn gera_o_script_de_cada_shell_sem_configuracao() {
    let env = TestEnv::new().await;
    assert!(!env.config_path().exists());
    for (shell, marca) in SHELLS {
        let script = script(&env, shell);
        assert!(script.contains(marca), "{shell}\n{script}");
        for comando in ["pix-automatico", "cobr", "retentativa", "completions"] {
            assert!(script.contains(comando), "{shell}: {comando}");
        }
    }

    let assert = env.cmd().args(["completions", "nushell"]).assert().code(2);
    assert!(
        stderr_of(&assert).contains("bash, elvish, fish, powershell, zsh"),
        "{}",
        stderr_of(&assert)
    );
}

fn gravar(env: &TestEnv, arquivo: &str, conteudo: &str) -> PathBuf {
    let caminho = env.path(arquivo);
    fs::write(&caminho, conteudo).unwrap();
    caminho
}

/// The standard output of `comando`, failing the test when it fails.
fn rodar(comando: &mut Command) -> String {
    let saida = comando.output().unwrap();
    assert!(
        saida.status.success(),
        "{comando:?}: {}{}",
        String::from_utf8_lossy(&saida.stdout),
        String::from_utf8_lossy(&saida.stderr)
    );
    String::from_utf8(saida.stdout).unwrap()
}

// Each shell reads its script and offers what a Tab would. On Windows,
// `bash` may be the one of WSL, with other paths: only PowerShell there.

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn o_bash_completa_comandos_opcoes_e_valores_em_todos_os_niveis() {
    if !instalado("bash") {
        return;
    }
    let env = TestEnv::new().await;
    let caminho = gravar(&env, "inter-pj.bash", &script(&env, "bash"));
    rodar(Command::new("bash").arg("-n").arg(&caminho));
    for (palavras, oferecido) in [
        ("inter-pj pix-au", "pix-automatico"),
        ("inter-pj pix-automatico co", "cobr"),
        ("inter-pj pix-automatico cobr cr", "criar"),
        ("inter-pj pix enviar --tipo-c", "--tipo-conta"),
        ("inter-pj pix enviar --tipo-conta p", "poupanca pagamento"),
        ("inter-pj extrato --formato c", "csv"),
        ("inter-pj completions z", "zsh"),
    ] {
        let verificacao = gravar(
            &env,
            "completar.bash",
            &format!(
                "source \"$1\"\n\
                 COMP_WORDS=({palavras})\n\
                 COMP_CWORD=$((${{#COMP_WORDS[@]}} - 1))\n\
                 _inter__pj inter-pj \"${{COMP_WORDS[COMP_CWORD]}}\" \"${{COMP_WORDS[COMP_CWORD-1]}}\"\n\
                 echo \"${{COMPREPLY[*]}}\"\n"
            ),
        );
        let saida = rodar(Command::new("bash").arg(&verificacao).arg(&caminho));
        assert_eq!(saida.trim_end(), oferecido, "{palavras}");
    }
}

/// zsh completes only in a terminal: the script is checked as zsh reads it.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn o_zsh_aceita_o_script() {
    if !instalado("zsh") {
        return;
    }
    let env = TestEnv::new().await;
    let caminho = gravar(&env, "_inter-pj", &script(&env, "zsh"));
    rodar(Command::new("zsh").arg("-n").arg(&caminho));
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn o_fish_completa_comandos_e_valores() {
    if !instalado("fish") {
        return;
    }
    let env = TestEnv::new().await;
    let caminho = gravar(&env, "inter-pj.fish", &script(&env, "fish"));
    rodar(Command::new("fish").arg("--no-execute").arg(&caminho));
    let verificacao = gravar(
        &env,
        "completar.fish",
        "source $argv[1]\n\
         complete -C'inter-pj pix-automatico cobr cr'\n\
         complete -C'inter-pj pix enviar --tipo-conta p'\n",
    );
    let oferecido = rodar(Command::new("fish").arg(&verificacao).arg(&caminho));
    for linha in ["criar\tCria a cobrança", "pagamento\t", "poupanca\t"] {
        assert!(oferecido.contains(linha), "{linha}\n{oferecido}");
    }
}

/// elvish has the `edit:` module only in an interactive session: a
/// stand-in collects what the script offers.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn o_elvish_completa_os_comandos() {
    if !instalado("elvish") {
        return;
    }
    let env = TestEnv::new().await;
    let caminho = gravar(
        &env,
        "inter-pj.elv",
        &format!(
            "var edit: = (ns [&completion:=(ns [&arg-completer=[&]]) \
             &complex-candidate~={{|texto &display=$nil| put $texto }}])\n\
             {}\n\
             $edit:completion:arg-completer[inter-pj] inter-pj pix-automatico cobr ''\n",
            script(&env, "elvish")
        ),
    );
    rodar(Command::new("elvish").arg("-compileonly").arg(&caminho));
    let oferecido = rodar(Command::new("elvish").arg(&caminho));
    for comando in ["criar", "retentativa"] {
        assert!(
            oferecido.lines().any(|linha| linha.ends_with(comando)),
            "{comando}\n{oferecido}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn o_powershell_completa_os_comandos() {
    if !instalado("pwsh") {
        return;
    }
    let env = TestEnv::new().await;
    let caminho = gravar(&env, "_inter-pj.ps1", &script(&env, "powershell"));
    let verificacao = gravar(
        &env,
        "completar.ps1",
        "param([string]$Script)\n\
         $erros = $null\n\
         [void][System.Management.Automation.Language.Parser]::ParseFile($Script, [ref]$null, [ref]$erros)\n\
         if ($erros) { $erros | ForEach-Object { $_.Message }; exit 1 }\n\
         . $Script\n\
         $linha = 'inter-pj pix-automatico cobr cr'\n\
         (TabExpansion2 -inputScript $linha -cursorColumn $linha.Length).CompletionMatches | ForEach-Object CompletionText\n",
    );
    let oferecido = rodar(
        Command::new("pwsh")
            .args(["-NoProfile", "-NonInteractive", "-File"])
            .arg(&verificacao)
            .arg(&caminho),
    );
    assert_eq!(oferecido.trim(), "criar");
}

#[tokio::test(flavor = "multi_thread")]
async fn grava_uma_pagina_de_manual_por_comando() {
    let env = TestEnv::new().await;
    let diretorio = env.path("man/man1");
    let stdout = stdout_of(&env.cmd().arg("manual").arg(&diretorio).assert().success());
    let mut arquivos: Vec<String> = fs::read_dir(&diretorio)
        .unwrap()
        .map(|entrada| entrada.unwrap().file_name().into_string().unwrap())
        .collect();
    arquivos.sort();
    assert_eq!(
        stdout,
        format!(
            "{} páginas de manual gravadas em {}\n",
            arquivos.len(),
            diretorio.display()
        )
    );
    for arquivo in [
        "inter-pj.1",
        "inter-pj-saldo.1",
        "inter-pj-pix-enviar.1",
        "inter-pj-pix-automatico-cobr-retentativa.1",
        "inter-pj-completions.1",
        "inter-pj-manual.1",
    ] {
        assert!(arquivos.iter().any(|a| a == arquivo), "{arquivo}");
    }
    assert!(arquivos.iter().all(|arquivo| arquivo.ends_with(".1")));
    let raiz = fs::read_to_string(diretorio.join("inter-pj.1")).unwrap();
    assert!(raiz.contains("\n.TH INTER-PJ 1 "), "{raiz}");
    assert!(raiz.contains("\n.SH COMANDOS\n"), "{raiz}");

    // Again: the pages are updated.
    env.cmd().arg("manual").arg(&diretorio).assert().success();

    // What is not a directory is refused.
    let assert = env
        .cmd()
        .arg("manual")
        .arg(diretorio.join("inter-pj.1"))
        .assert()
        .code(2);
    assert!(
        stderr_of(&assert).contains("inter-pj.1 não é um diretório"),
        "{}",
        stderr_of(&assert)
    );

    // groff reads every page without a warning.
    if cfg!(unix) && instalado("groff") {
        for arquivo in &arquivos {
            let saida = Command::new("groff")
                .args(["-k", "-man", "-Tutf8", "-ww", "-z"])
                .arg(diretorio.join(arquivo))
                .output()
                .unwrap();
            assert!(
                saida.status.success() && saida.stderr.is_empty(),
                "{arquivo}: {}",
                String::from_utf8_lossy(&saida.stderr)
            );
        }
    }
}
