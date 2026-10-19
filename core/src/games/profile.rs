//! Réglages 3D d'un jeu, comme les « Paramètres 3D » du panneau NVIDIA.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GameProfile {
    /// Images par seconde au maximum (absent : sans limite).
    pub fps_limit: Option<u32>,
    pub vsync: Toggle,
    pub low_latency: LowLatency,
    /// Filtrage anisotrope 2, 4, 8 ou 16× (absent : réglage du jeu).
    pub anisotropic: Option<u32>,
    /// DLSS remplacé par la dernière version (NVIDIA RTX).
    pub dlss_latest: Option<bool>,
    /// Indicateur DLSS à l'écran (vérifier que la nouvelle version est prise).
    pub dlss_indicator: Option<bool>,
    /// Images générées par le pilote (NVIDIA RTX 40 et plus récentes).
    pub smooth_motion: Option<bool>,
    /// HDR dans le jeu (écran en HDR).
    pub hdr: Option<bool>,
    /// Performances à l'écran (MangoHud).
    pub overlay: Option<bool>,
    /// Profil de couleurs (identifiant), dans la seule fenêtre du jeu.
    pub color_profile: Option<String>,
}

/// Réglage laissé au jeu, ou forcé.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Toggle {
    #[default]
    App,
    On,
    Off,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LowLatency {
    /// Réglage du jeu.
    #[default]
    App,
    /// Une seule image préparée d'avance.
    On,
    /// Comme On, plus NVIDIA Reflex pour les jeux qui le proposent.
    Ultra,
}

impl GameProfile {
    /// Ce jeu, avec les réglages globaux pour ce qu'il ne règle pas lui-même.
    pub fn merged_over(&self, global: &GameProfile) -> GameProfile {
        GameProfile {
            fps_limit: self.fps_limit.or(global.fps_limit),
            vsync: if self.vsync == Toggle::App { global.vsync } else { self.vsync },
            low_latency: if self.low_latency == LowLatency::App { global.low_latency } else { self.low_latency },
            anisotropic: self.anisotropic.or(global.anisotropic),
            dlss_latest: self.dlss_latest.or(global.dlss_latest),
            dlss_indicator: self.dlss_indicator.or(global.dlss_indicator),
            smooth_motion: self.smooth_motion.or(global.smooth_motion),
            hdr: self.hdr.or(global.hdr),
            overlay: self.overlay.or(global.overlay),
            color_profile: self.color_profile.clone().or_else(|| global.color_profile.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_settings_win_over_global_ones() {
        let global =
            GameProfile { fps_limit: Some(141), vsync: Toggle::Off, overlay: Some(true), ..Default::default() };
        let game = GameProfile {
            fps_limit: Some(60),
            overlay: Some(false),
            low_latency: LowLatency::Ultra,
            ..Default::default()
        };
        let merged = game.merged_over(&global);
        assert_eq!(merged.fps_limit, Some(60));
        assert_eq!(merged.vsync, Toggle::Off);
        assert_eq!(merged.overlay, Some(false));
        assert_eq!(merged.low_latency, LowLatency::Ultra);
        assert_eq!(GameProfile::default().merged_over(&global), global);
    }
}
