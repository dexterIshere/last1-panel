//! Notes de mise à jour : celles des commits embarqués à la construction
//! (tools/release-notes.sh, lignes « Joueurs: » et « Players: » des messages).
//! Après une mise à jour, les notes des commits plus récents que celui de la
//! version d'avant : la notification « à jour » en dit trois, la fenêtre
//! les montre toutes jusqu'à ce que le joueur les ferme.

use crate::i18n::t;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// La liste de cette construction (vide hors des versions publiées).
pub const EMBEDDED: &str = include_str!(concat!(env!("OUT_DIR"), "/release-notes.tsv"));

/// Une nouveauté, dans les deux langues.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Note {
    pub fr: String,
    pub en: String,
}

impl Note {
    pub fn text(&self) -> String {
        t(&self.fr, if self.en.is_empty() { &self.fr } else { &self.en })
    }
}

/// Les commits de la liste (du plus récent au plus ancien) et leurs notes.
pub fn parse(text: &str) -> Vec<(String, Vec<Note>)> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let mut fields = line.split('\t');
            let commit = fields.next().unwrap_or_default().trim().to_string();
            let split = |value: Option<&str>| -> Vec<String> {
                value
                    .unwrap_or_default()
                    .split('\u{1e}')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect()
            };
            let (fr, en) = (split(fields.next()), split(fields.next()));
            let notes = (0..fr.len().max(en.len()))
                .map(|i| Note {
                    fr: fr.get(i).or(en.get(i)).cloned().unwrap_or_default(),
                    en: en.get(i).or(fr.get(i)).cloned().unwrap_or_default(),
                })
                .collect();
            (commit, notes)
        })
        .collect()
}

/// Les notes des commits plus récents que `previous` ; rien si `previous`
/// n'est pas dans la liste (trop ancien, ou construction locale).
pub fn since(text: &str, previous: &str) -> Vec<Note> {
    let list = parse(text);
    let Some(index) = list.iter().position(|(commit, _)| previous.starts_with(commit.as_str())) else {
        return Vec::new();
    };
    list[..index].iter().flat_map(|(_, notes)| notes.clone()).collect()
}

/// Nouveautés à montrer dans la fenêtre.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct News {
    pub version: String,
    pub notes: Vec<Note>,
}

/// Au démarrage du service : la version a changé depuis le dernier ? Ses
/// nouveautés, gardées pour la fenêtre (`news.json` dans `cache`) ; le
/// commit en cours est noté pour la prochaine fois.
pub fn on_start(cache: &Path, text: &str, version: &str) -> Option<News> {
    let current = parse(text).first().map(|(commit, _)| commit.clone())?;
    let seen = cache.join("release-commit");
    let previous = fs::read_to_string(&seen).ok().map(|s| s.trim().to_string());
    let _ = fs::create_dir_all(cache);
    let _ = fs::write(&seen, &current);
    let notes = since(text, &previous.filter(|p| *p != current)?);
    if notes.is_empty() {
        return None;
    }
    let news = News { version: version.to_string(), notes };
    let _ = fs::write(cache.join("news.json"), serde_json::to_string(&news).unwrap_or_default());
    Some(news)
}

/// Les nouveautés pas encore fermées par le joueur.
pub fn pending(cache: &Path) -> Option<News> {
    serde_json::from_str(&fs::read_to_string(cache.join("news.json")).ok()?).ok()
}

pub fn dismiss(cache: &Path) {
    let _ = fs::remove_file(cache.join("news.json"));
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = "c3c3c3c\tCarte libérée d'un clic\x1eTextes plus simples\tCard unlocked in one click\x1eSimpler texts\n\
                        b2b2b2b\n\
                        a1a1a1a\tCourbe de chaque partie\tA chart for each game\n\
                        9090909\tAncienne note\tOld note\n";

    #[test]
    fn notes_newer_than_the_previous_version() {
        let notes = since(LIST, "a1a1a1a4f00");
        assert_eq!(
            notes,
            [
                Note { fr: "Carte libérée d'un clic".into(), en: "Card unlocked in one click".into() },
                Note { fr: "Textes plus simples".into(), en: "Simpler texts".into() },
            ]
        );
        assert!(since(LIST, "c3c3c3c").is_empty(), "déjà à jour");
        assert!(since(LIST, "ffffff0").is_empty(), "version d'avant inconnue : rien");
    }

    #[test]
    fn news_are_kept_until_dismissed() {
        let dir = tempfile::tempdir().unwrap();
        // Première fois : le commit est noté, rien à dire.
        assert_eq!(on_start(dir.path(), "a1a1a1a\tCourbe\tChart\n", "0.1.0.r90"), None);
        // Mise à jour.
        let news = on_start(dir.path(), LIST, "0.1.0.r95").unwrap();
        assert_eq!(news.version, "0.1.0.r95");
        assert_eq!(news.notes.len(), 2);
        assert_eq!(pending(dir.path()), Some(news));
        // Relancé sans mise à jour : rien de neuf, les nouveautés restent.
        assert_eq!(on_start(dir.path(), LIST, "0.1.0.r95"), None);
        assert!(pending(dir.path()).is_some());
        dismiss(dir.path());
        assert_eq!(pending(dir.path()), None);
        // Construction locale (liste vide) : rien.
        assert_eq!(on_start(dir.path(), "", "0.1.0"), None);
    }
}
