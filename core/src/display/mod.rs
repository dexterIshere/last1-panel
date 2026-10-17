//! Écrans : définition, fréquence, échelle, orientation, G-SYNC/FreeSync,
//! HDR et profondeur de couleur, par l'outil du bureau de la session.

pub mod kde;
pub mod model;

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

pub trait DisplayBackend: Send + Sync {
    /// Sorties branchées.
    fn outputs(&self) -> CoreResult<Vec<Output>>;
    /// Applique tous les changements d'un coup.
    fn apply(&self, changes: &[OutputChange]) -> CoreResult<()>;
    /// Profil de couleur actuel d'une sortie.
    fn profile(&self, output: &str) -> CoreResult<ProfileState>;
    fn set_profile(&self, output: &str, state: &ProfileState) -> CoreResult<()>;
}
