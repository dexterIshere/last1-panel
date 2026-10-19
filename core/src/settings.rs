//! Réglages gardés entre deux sessions (~/.config/last1-panel/settings.json).
//!
//! Ce que les outils gardent eux-mêmes n'est pas recopié ici (écrans de
//! Plasma, configuration de LACT) : seulement ce qu'il faut réappliquer à la
//! connexion (vibrance) ou remettre en partant (profil ICC d'origine), et les
//! réglages des jeux, lus à chaque lancement.

use crate::color::curve::ColorAdjust;
use crate::display::ProfileState;
use crate::error::CoreResult;
use crate::games::profile::GameProfile;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub version: u32,
    /// Par sortie (« DP-1 »).
    pub outputs: BTreeMap<String, OutputSettings>,
    /// Profils de couleurs nommés, pour l'écran ou pour un jeu.
    pub color_profiles: Vec<ColorProfile>,
    pub games: GamesSettings,
    /// Icône près de l'horloge (service de la session, `tray.rs`).
    pub tray: bool,
    /// Rapports de performances partagés avec last1.fun (system/sharing.rs) :
    /// absent tant que le joueur n'a pas répondu ; jamais envoyé sans « oui ».
    pub share_reports: Option<bool>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: VERSION,
            outputs: BTreeMap::new(),
            color_profiles: Vec::new(),
            games: GamesSettings::default(),
            tray: true,
            share_reports: None,
        }
    }
}

/// Profil de couleurs : courbes et vibrance, gardées sous un nom pour les
/// remettre sur l'écran, ou pour un jeu (dans sa seule fenêtre).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColorProfile {
    pub id: String,
    pub name: String,
    pub color: ColorAdjust,
    /// Vibrance numérique, 0 à 100 % (50 : inchangée).
    pub vibrance: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OutputSettings {
    /// Nom de l'écran au moment du réglage (« Dell AW2723DF »), pour le
    /// reconnaître si un autre écran prend ce connecteur.
    pub description: String,
    pub color: ColorAdjust,
    /// Vibrance numérique, 0 à 100 % (50 : inchangée). Absente : jamais réglée.
    pub vibrance: Option<f64>,
    /// Profil de couleur d'avant Last1 Panel, remis au retour aux valeurs par défaut.
    pub previous_profile: Option<ProfileState>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GamesSettings {
    /// Réglages de tous les jeux lancés par Last1 Panel.
    pub global: GameProfile,
    /// Réglages propres à un jeu Steam (par numéro d'application).
    pub per_game: BTreeMap<u32, GameProfile>,
}

impl Settings {
    pub fn color_profile(&self, id: &str) -> Option<&ColorProfile> {
        self.color_profiles.iter().find(|profile| profile.id == id)
    }

    /// Profil remplacé (même identifiant), ou créé sans identifiant.
    pub fn put_color_profile(
        &mut self,
        id: Option<&str>,
        name: &str,
        color: ColorAdjust,
        vibrance: f64,
    ) -> ColorProfile {
        let name = name.trim();
        let name =
            if name.is_empty() { crate::i18n::t("Sans nom", "Untitled") } else { name.chars().take(60).collect() };
        let profile = ColorProfile {
            id: id.map(str::to_string).unwrap_or_else(|| {
                let next = self
                    .color_profiles
                    .iter()
                    .filter_map(|p| p.id.strip_prefix('p').and_then(|n| n.parse::<u32>().ok()))
                    .max()
                    .unwrap_or(0)
                    + 1;
                format!("p{next}")
            }),
            name,
            color,
            vibrance: if vibrance.is_finite() { vibrance.clamp(0.0, 100.0) } else { 50.0 },
        };
        match self.color_profiles.iter_mut().find(|p| p.id == profile.id) {
            Some(existing) => *existing = profile.clone(),
            None => self.color_profiles.push(profile.clone()),
        }
        profile
    }

    /// Profil supprimé, et retiré des jeux qui l'avaient.
    pub fn remove_color_profile(&mut self, id: &str) {
        self.color_profiles.retain(|p| p.id != id);
        for game in std::iter::once(&mut self.games.global).chain(self.games.per_game.values_mut()) {
            if game.color_profile.as_deref() == Some(id) {
                game.color_profile = None;
            }
        }
    }

    /// Profil dont cet écran a les couleurs : `Some(None)` pour les valeurs
    /// par défaut, `None` pour un réglage qui n'est dans aucun profil.
    pub fn active_profile(&self, output: &str) -> Option<Option<&ColorProfile>> {
        let saved = self.outputs.get(output).cloned().unwrap_or_default();
        let vibrance = saved.vibrance.unwrap_or(50.0);
        if saved.color.is_neutral() && (vibrance - 50.0).abs() < 0.5 {
            return Some(None);
        }
        self.color_profiles
            .iter()
            .find(|p| p.color.approx_eq(&saved.color) && (p.vibrance - vibrance).abs() < 0.5)
            .map(Some)
    }

    /// Couleurs de l'écran du jeu : celles du premier écran réglé (un PC de
    /// jeu n'en a d'habitude qu'un), neutres sinon. Courbes et vibrance.
    pub fn screen_colors(&self) -> (ColorAdjust, f64) {
        self.outputs
            .values()
            .find(|o| !o.color.is_neutral() || o.vibrance.is_some_and(|v| (v - 50.0).abs() > f64::EPSILON))
            .map_or((ColorAdjust::default(), 50.0), |o| (o.color, o.vibrance.unwrap_or(50.0)))
    }

    pub fn path() -> PathBuf {
        crate::paths::config_dir().join("settings.json")
    }

    /// Réglages enregistrés ; ceux par défaut si le fichier manque ou est
    /// illisible (jamais d'erreur : le lanceur des jeux en dépend).
    pub fn load_from(path: &Path) -> Self {
        fs::read_to_string(path).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
    }

    pub fn load() -> Self {
        Self::load_from(&Self::path())
    }

    /// Écrit à côté puis remplace : un lancement de jeu au même moment lit
    /// l'ancien fichier ou le nouveau, jamais un fichier à moitié écrit.
    /// Fichier temporaire propre au processus (fenêtre et service écrivent).
    fn save_to(&self, path: &Path) -> CoreResult<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let temporary = path.with_extension(format!("json.{}.new", std::process::id()));
        fs::write(&temporary, serde_json::to_string_pretty(self).expect("réglages sérialisables") + "\n")?;
        fs::rename(&temporary, path)?;
        Ok(())
    }

    /// Lit, modifie et réécrit les réglages sous un verrou de fichier : deux
    /// écritures en même temps (couleurs et vibrance, fenêtre et service de
    /// session) ne s'effacent jamais l'une l'autre.
    pub fn update<T>(path: &Path, change: impl FnOnce(&mut Settings) -> T) -> CoreResult<T> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let lock = fs::OpenOptions::new().create(true).truncate(false).write(true).open(path.with_extension("lock"))?;
        lock.lock()?;
        let mut settings = Self::load_from(path);
        let result = change(&mut settings);
        settings.save_to(path)?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_profiles_are_created_replaced_and_unlinked() {
        let mut settings = Settings::default();
        let warm = settings.put_color_profile(None, "  Arc Raiders ", ColorAdjust::default(), 70.0);
        let cold = settings.put_color_profile(None, "", ColorAdjust::default(), 300.0);
        assert_eq!((warm.id.as_str(), warm.name.as_str()), ("p1", "Arc Raiders"));
        assert_eq!(cold.id, "p2");
        assert_eq!(cold.vibrance, 100.0);
        assert!(!cold.name.is_empty());

        settings.put_color_profile(Some("p1"), "Arc Raiders nuit", ColorAdjust::default(), 60.0);
        assert_eq!(settings.color_profiles.len(), 2);
        assert_eq!(settings.color_profile("p1").unwrap().name, "Arc Raiders nuit");

        settings.games.per_game.entry(1808500).or_default().color_profile = Some("p1".into());
        settings.games.global.color_profile = Some("p1".into());
        settings.remove_color_profile("p1");
        assert!(settings.color_profile("p1").is_none());
        assert_eq!(settings.games.per_game[&1808500].color_profile, None);
        assert_eq!(settings.games.global.color_profile, None);
        assert_eq!(settings.put_color_profile(None, "Nouveau", ColorAdjust::default(), 50.0).id, "p3");
    }

    #[test]
    fn screen_shows_which_profile_it_has() {
        let mut settings = Settings::default();
        let guide = ColorAdjust::all(Channel { brightness: 52.0, contrast: 47.0, gamma: 1.47 });
        let profile = settings.put_color_profile(None, "Guide", guide, 62.0);
        assert_eq!(settings.active_profile("DP-1"), Some(None));

        let screen = settings.outputs.entry("DP-1".into()).or_default();
        screen.color = ColorAdjust::all(Channel { gamma: 1.470_000_000_1, ..guide.red });
        screen.vibrance = Some(62.0);
        assert_eq!(settings.active_profile("DP-1"), Some(Some(&profile)));

        settings.outputs.get_mut("DP-1").unwrap().vibrance = Some(70.0);
        assert_eq!(settings.active_profile("DP-1"), None);
        // Fichier d'avant l'icône : elle est affichée.
        let old: Settings = serde_json::from_str(r#"{"version": 1}"#).unwrap();
        assert!(old.tray);
    }
    use crate::color::curve::Channel;

    #[test]
    fn round_trip_and_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("last1-panel/settings.json");
        assert_eq!(Settings::load_from(&path), Settings::default());

        let mut settings = Settings::default();
        settings.outputs.insert(
            "DP-1".into(),
            OutputSettings {
                description: "Dell AW2723DF".into(),
                color: ColorAdjust::all(Channel { brightness: 52.0, contrast: 55.0, gamma: 1.1 }),
                vibrance: Some(70.0),
                previous_profile: None,
            },
        );
        Settings::update(&path, |saved| *saved = settings.clone()).unwrap();
        assert_eq!(Settings::load_from(&path), settings);
    }

    #[test]
    fn concurrent_updates_keep_every_change() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let threads: Vec<_> = (0..8)
            .map(|i| {
                let path = path.clone();
                std::thread::spawn(move || {
                    Settings::update(&path, |s| {
                        s.outputs.entry(format!("DP-{i}")).or_default().vibrance = Some(f64::from(i));
                    })
                    .unwrap();
                })
            })
            .collect();
        threads.into_iter().for_each(|t| t.join().unwrap());
        assert_eq!(Settings::load_from(&path).outputs.len(), 8);
    }

    #[test]
    fn broken_or_partial_files_fall_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, "{ pas du json").unwrap();
        assert_eq!(Settings::load_from(&path), Settings::default());
        // Fichier d'une version future, avec des clés inconnues : le reste est lu.
        fs::write(&path, r#"{"version": 2, "outputs": {"HDMI-A-1": {"vibrance": 60}}, "plusTard": true}"#).unwrap();
        let settings = Settings::load_from(&path);
        assert_eq!(settings.outputs["HDMI-A-1"].vibrance, Some(60.0));
        assert_eq!(settings.outputs["HDMI-A-1"].color, ColorAdjust::default());
    }
}
