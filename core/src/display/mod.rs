//! Écrans : définition, fréquence, échelle, orientation, G-SYNC/FreeSync,
//! HDR et profondeur de couleur, par l'outil du bureau de la session.

pub mod hyprland;
pub mod kde;
pub mod model;

use crate::color::icc::Transfer;
use crate::error::CoreResult;
use model::{Output, OutputChange};
use serde::{Deserialize, Serialize};

/// Profil de couleur d'une sortie : fichier ICC, et (Plasma) source choisie
/// (« sRGB », « ICC », « EDID »).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileState {
    pub icc: Option<String>,
    pub source: Option<String>,
}

/// Ce que le bureau permet de régler.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Features {
    /// Écran principal (Plasma).
    pub primary: bool,
    /// Profondeurs de couleur proposées (0 : automatique).
    pub bit_depths: Vec<u32>,
    /// Couleurs appliquées à chaque mouvement de curseur (sinon au clic).
    pub live_color: bool,
    /// Fonction de transfert que le compositeur suppose avec un profil ICC.
    #[serde(skip)]
    pub icc_transfer: Transfer,
    /// Réglages impossibles à écrire, et pourquoi.
    pub read_only: Option<String>,
}

pub trait DisplayBackend: Send + Sync {
    fn features(&self) -> Features;
    /// L'échelle que le bureau appliquera vraiment pour ce mode.
    fn normalize_scale(&self, _mode: Option<&model::Mode>, scale: f64) -> f64 {
        scale
    }
    /// Sorties branchées.
    fn outputs(&self) -> CoreResult<Vec<Output>>;
    /// Applique tous les changements d'un coup.
    fn apply(&self, changes: &[OutputChange]) -> CoreResult<()>;
    /// Profil de couleur actuel d'une sortie.
    fn profile(&self, output: &str) -> CoreResult<ProfileState>;
    fn set_profile(&self, output: &str, state: &ProfileState) -> CoreResult<()>;
}
