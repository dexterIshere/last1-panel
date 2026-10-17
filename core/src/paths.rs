//! Emplacements des fichiers de Last1 Panel (spécification XDG).

use std::path::{Path, PathBuf};

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

fn xdg(var: &str, fallback: &str) -> PathBuf {
    match std::env::var_os(var) {
        Some(value) if Path::new(&value).is_absolute() => PathBuf::from(value),
        _ => home().join(fallback),
    }
}

pub fn home_dir() -> PathBuf {
    home()
}

/// ~/.config/last1-panel : réglages.
pub fn config_dir() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config").join("last1-panel")
}

/// ~/.config : configurations des autres logiciels (Hyprland…).
pub fn user_config_dir() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config")
}

/// ~/.cache/last1-panel : fichiers refaits à volonté (tables de couleurs des jeux).
pub fn cache_dir() -> PathBuf {
    xdg("XDG_CACHE_HOME", ".cache").join("last1-panel")
}

/// Couche Vulkan des couleurs des jeux (vkBasalt livré avec Last1 Panel) :
/// manifeste du paquet, ou celui posé par l'AppImage.
pub const COLOR_LAYER: &str = "last1-panel-color.json";

pub fn color_layer_installed() -> bool {
    Path::new("/usr/share/vulkan/implicit_layer.d").join(COLOR_LAYER).is_file()
        || xdg("XDG_DATA_HOME", ".local/share").join("vulkan/implicit_layer.d").join(COLOR_LAYER).is_file()
}

/// ~/.local/share/icc : emplacement habituel des profils ICC de l'utilisateur
/// (celui que proposent les réglages de Plasma).
pub fn icc_dir() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share").join("icc")
}

/// Outil des jeux et du service installé par le paquet Arch.
pub const SYSTEM_CLI: &str = "/usr/bin/last1-panel-cli";

/// ~/.local/lib/last1-panel : outil des jeux et nvibrant posés par l'AppImage
/// à sa première ouverture (`src-tauri/src/install.rs`).
pub fn user_lib_dir() -> PathBuf {
    home().join(".local/lib/last1-panel")
}

/// L'outil des jeux et du service : celui du paquet s'il est là, sinon celui
/// de l'AppImage.
pub fn cli() -> PathBuf {
    if Path::new(SYSTEM_CLI).is_file() { PathBuf::from(SYSTEM_CLI) } else { user_lib_dir().join("last1-panel-cli") }
}

/// Last1 Panel installé par le paquet Arch (go-to-linux le tient à jour).
pub const SYSTEM_APP: &str = "/usr/bin/last1-panel";

/// ~/.local/bin/last1-panel : l'AppImage installée pour l'utilisateur.
pub fn user_appimage() -> PathBuf {
    home().join(".local/bin/last1-panel")
}
