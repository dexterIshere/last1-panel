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
    pub games: GamesSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self { version: VERSION, outputs: BTreeMap::new(), games: GamesSettings::default() }
    }
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
    pub fn save_to(&self, path: &Path) -> CoreResult<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let temporary = path.with_extension("json.new");
        fs::write(&temporary, serde_json::to_string_pretty(self).expect("réglages sérialisables") + "\n")?;
        fs::rename(&temporary, path)?;
        Ok(())
    }

    pub fn save(&self) -> CoreResult<()> {
        self.save_to(&Self::path())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        settings.save_to(&path).unwrap();
        assert_eq!(Settings::load_from(&path), settings);
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
