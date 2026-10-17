//! Langue des messages : français si la langue du système est le français,
//! anglais sinon (comme go-to-linux et last1.fun). L'interface peut la changer.

use std::sync::atomic::{AtomicBool, Ordering};

static ENGLISH: AtomicBool = AtomicBool::new(false);

/// Langue d'après `LC_ALL`, `LC_MESSAGES` puis `LANG`.
pub fn init_from_env() {
    let lang = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .find(|value| !value.is_empty())
        .unwrap_or_default();
    set_english(!lang.starts_with("fr"));
}

pub fn set_english(english: bool) {
    ENGLISH.store(english, Ordering::Relaxed);
}

pub fn is_english() -> bool {
    ENGLISH.load(Ordering::Relaxed)
}

/// Le texte dans la langue des messages.
pub fn t(fr: &str, en: &str) -> String {
    if is_english() { en } else { fr }.to_string()
}
