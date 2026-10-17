//! Erreurs : des phrases dans la langue des messages, montrées telles quelles.

use crate::i18n::is_english;

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// Outil absent du système (paquet à installer).
    #[error("{}", missing_tool(.0))]
    MissingTool(String),
    /// Outil lancé, mais en échec : sa sortie d'erreur est gardée.
    #[error("{}", tool_failed(.program, .detail))]
    ToolFailed { program: String, detail: String },
    /// Réponse d'un outil qu'on ne sait pas lire (version trop récente ?).
    #[error("{}", unreadable(.0, .1))]
    Unreadable(String, String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Message(String),
}

impl CoreError {
    pub fn message(text: impl Into<String>) -> Self {
        Self::Message(text.into())
    }
}

fn missing_tool(program: &str) -> String {
    if is_english() { format!("{program} is not installed.") } else { format!("{program} n'est pas installé.") }
}

fn tool_failed(program: &str, detail: &str) -> String {
    let detail = detail.trim();
    match (is_english(), detail.is_empty()) {
        (true, true) => format!("{program} failed."),
        (true, false) => format!("{program} failed: {detail}"),
        (false, true) => format!("{program} a échoué."),
        (false, false) => format!("{program} a échoué : {detail}"),
    }
}

fn unreadable(program: &str, detail: &str) -> String {
    if is_english() {
        format!("Unexpected answer from {program} ({detail}).")
    } else {
        format!("Réponse inattendue de {program} ({detail}).")
    }
}

pub type CoreResult<T> = Result<T, CoreError>;
