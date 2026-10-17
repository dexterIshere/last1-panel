//! Last1 Panel : réglages de la carte graphique et des écrans pour jouer sous
//! Linux (Wayland), à la place du panneau NVIDIA ou d'AMD Adrenalin.
//!
//! Chaque réglage passe par un outil existant, maintenu par d'autres
//! (kscreen-doctor, hyprctl, nvibrant, LACT, Proton…) : ce crate les pilote
//! et ne réimplémente rien de ce qu'ils font déjà (docs/DESIGN.md).

pub mod color;
pub mod display;
pub mod error;
pub mod exec;
pub mod games;
pub mod i18n;
pub mod panel;
pub mod paths;
pub mod settings;
pub mod system;
pub mod tray;
pub mod tuning;
pub mod update;
