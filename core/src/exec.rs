//! Lancement des outils du système (kscreen-doctor, hyprctl, nvibrant…).
//!
//! Tout passe par le trait [`Exec`] : les tests remplacent le système par
//! [`FakeExec`], qui rejoue des sorties enregistrées.

use crate::error::{CoreError, CoreResult};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Sortie d'un outil terminé.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Output {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    pub fn success(&self) -> bool {
        self.status == 0
    }
}

pub trait Exec: Send + Sync {
    /// Lance `program` avec `args` et attend sa fin.
    fn run(&self, program: &str, args: &[String]) -> CoreResult<Output>;

    /// Chemin de `program` s'il est installé.
    fn which(&self, program: &str) -> Option<PathBuf>;

    /// Comme [`Exec::run`], mais un code de sortie non nul devient une erreur.
    fn run_ok(&self, program: &str, args: &[String]) -> CoreResult<Output> {
        let output = self.run(program, args)?;
        if output.success() {
            Ok(output)
        } else {
            let detail = if output.stderr.trim().is_empty() { &output.stdout } else { &output.stderr };
            Err(CoreError::ToolFailed { program: program.to_string(), detail: last_lines(detail, 4) })
        }
    }
}

/// Les dernières lignes d'une sortie, pour un message d'erreur lisible.
fn last_lines(text: &str, count: usize) -> String {
    let lines: Vec<&str> = text.trim().lines().collect();
    lines[lines.len().saturating_sub(count)..].join("\n")
}

/// Les vrais outils du système, arrêtés au bout de `timeout`, avec en plus
/// les variables `env` (celles de la session graphique pour le service).
pub struct SystemExec {
    pub timeout: Duration,
    pub env: Vec<(String, String)>,
}

impl Default for SystemExec {
    fn default() -> Self {
        Self { timeout: Duration::from_secs(15), env: Vec::new() }
    }
}

impl Exec for SystemExec {
    fn run(&self, program: &str, args: &[String]) -> CoreResult<Output> {
        let path = self.which(program).ok_or_else(|| CoreError::MissingTool(program.to_string()))?;
        let mut child =
            Command::new(path).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
        // Lecture dans des fils à part : une sortie plus grande que le tampon
        // du tube bloquerait l'outil avant sa fin.
        let mut stdout = child.stdout.take().expect("stdout demandé");
        let mut stderr = child.stderr.take().expect("stderr demandé");
        let out = std::thread::spawn(move || {
            let mut text = Vec::new();
            let _ = stdout.read_to_end(&mut text);
            text
        });
        let err = std::thread::spawn(move || {
            let mut text = Vec::new();
            let _ = stderr.read_to_end(&mut text);
            text
        });
        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if started.elapsed() > self.timeout {
                let _ = child.kill();
                let _ = child.wait();
                return Err(CoreError::ToolFailed {
                    program: program.to_string(),
                    detail: crate::i18n::t("pas de réponse", "no answer"),
                });
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let stdout = String::from_utf8_lossy(&out.join().unwrap_or_default()).into_owned();
        let stderr = String::from_utf8_lossy(&err.join().unwrap_or_default()).into_owned();
        Ok(Output { status: status.code().unwrap_or(-1), stdout, stderr })
    }

    fn which(&self, program: &str) -> Option<PathBuf> {
        which(program)
    }
}

/// Recherche dans `PATH`, comme le shell.
pub fn which(program: &str) -> Option<PathBuf> {
    if program.contains('/') {
        let path = PathBuf::from(program);
        return is_executable(&path).then_some(path);
    }
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths).map(|dir| dir.join(program)).find(|path| is_executable(path))
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata().map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0).unwrap_or(false)
}

/// Réponse enregistrée : programme, arguments attendus (tous si absents), sortie.
type Response = (String, Option<Vec<String>>, Output);

/// Faux système pour les tests : chaque appel est noté, et reçoit la réponse
/// enregistrée pour son programme et ses arguments (ou, à défaut, pour le
/// programme seul).
#[derive(Default)]
pub struct FakeExec {
    responses: std::sync::Mutex<Vec<Response>>,
    pub calls: std::sync::Mutex<Vec<(String, Vec<String>)>>,
    missing: std::sync::Mutex<Vec<String>>,
}

impl FakeExec {
    pub fn new() -> Self {
        Self::default()
    }

    /// Réponse à tout appel de `program`.
    pub fn on(self, program: &str, stdout: &str) -> Self {
        self.responses.lock().unwrap().push((
            program.to_string(),
            None,
            Output { status: 0, stdout: stdout.to_string(), stderr: String::new() },
        ));
        self
    }

    /// Réponse à un appel précis.
    pub fn on_args(self, program: &str, args: &[&str], output: Output) -> Self {
        self.responses.lock().unwrap().push((
            program.to_string(),
            Some(args.iter().map(|arg| arg.to_string()).collect()),
            output,
        ));
        self
    }

    /// Programme absent du système.
    pub fn without(self, program: &str) -> Self {
        self.missing.lock().unwrap().push(program.to_string());
        self
    }

    pub fn calls(&self) -> Vec<(String, Vec<String>)> {
        self.calls.lock().unwrap().clone()
    }
}

impl Exec for FakeExec {
    fn run(&self, program: &str, args: &[String]) -> CoreResult<Output> {
        if self.missing.lock().unwrap().iter().any(|name| name == program) {
            return Err(CoreError::MissingTool(program.to_string()));
        }
        self.calls.lock().unwrap().push((program.to_string(), args.to_vec()));
        let responses = self.responses.lock().unwrap();
        let exact = responses.iter().find(|(name, expected, _)| name == program && expected.as_deref() == Some(args));
        let any = responses.iter().find(|(name, expected, _)| name == program && expected.is_none());
        Ok(exact.or(any).map(|(_, _, output)| output.clone()).unwrap_or_default())
    }

    fn which(&self, program: &str) -> Option<PathBuf> {
        let missing = self.missing.lock().unwrap().iter().any(|name| name == program);
        (!missing).then(|| PathBuf::from("/usr/bin").join(program))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_exec_reads_output_and_status() {
        let exec = SystemExec::default();
        let output = exec.run("sh", &["-c".into(), "echo bonjour; echo oups >&2; exit 3".into()]).unwrap();
        assert_eq!(output.status, 3);
        assert_eq!(output.stdout, "bonjour\n");
        assert_eq!(output.stderr, "oups\n");
    }

    #[test]
    fn run_ok_turns_failure_into_error() {
        let exec = SystemExec::default();
        let error = exec.run_ok("sh", &["-c".into(), "echo raison >&2; exit 1".into()]).unwrap_err();
        assert!(error.to_string().contains("raison"));
    }

    #[test]
    fn missing_program_is_reported() {
        let exec = SystemExec::default();
        let error = exec.run("last1-panel-programme-absent", &[]).unwrap_err();
        assert!(matches!(error, CoreError::MissingTool(_)));
    }

    #[test]
    fn slow_program_is_stopped() {
        let exec = SystemExec { timeout: Duration::from_millis(100), env: Vec::new() };
        let started = Instant::now();
        assert!(exec.run("sleep", &["5".into()]).is_err());
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn fake_prefers_exact_arguments() {
        let fake = FakeExec::new().on("tool", "any").on_args(
            "tool",
            &["-j"],
            Output { status: 0, stdout: "json".into(), stderr: String::new() },
        );
        assert_eq!(fake.run("tool", &["-j".into()]).unwrap().stdout, "json");
        assert_eq!(fake.run("tool", &[]).unwrap().stdout, "any");
        assert_eq!(fake.calls().len(), 2);
    }
}
