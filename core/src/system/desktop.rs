//! Bureau de la session : Hyprland (bureau Signal de go-to-linux) ou KDE Plasma.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Desktop {
    Hyprland,
    Plasma,
    /// Autre bureau Wayland ou session X11 : réglages d'écran indisponibles.
    Other,
}

impl Desktop {
    /// D'après les variables de la session.
    pub fn detect() -> Self {
        Self::from_env(|name| std::env::var(name).ok())
    }

    pub fn from_env(var: impl Fn(&str) -> Option<String>) -> Self {
        if var("HYPRLAND_INSTANCE_SIGNATURE").is_some_and(|value| !value.is_empty()) {
            return Self::Hyprland;
        }
        let current = var("XDG_CURRENT_DESKTOP").unwrap_or_default();
        let wayland = var("WAYLAND_DISPLAY").is_some_and(|value| !value.is_empty())
            || var("XDG_SESSION_TYPE").as_deref() == Some("wayland");
        if wayland && current.split(':').any(|name| name.eq_ignore_ascii_case("KDE")) {
            Self::Plasma
        } else if current.split(':').any(|name| name == "Hyprland") {
            Self::Hyprland
        } else {
            Self::Other
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn desktop(vars: &[(&str, &str)]) -> Desktop {
        let vars: HashMap<String, String> = vars.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        Desktop::from_env(|name| vars.get(name).cloned())
    }

    #[test]
    fn recognises_sessions() {
        assert_eq!(
            desktop(&[("HYPRLAND_INSTANCE_SIGNATURE", "abc"), ("XDG_CURRENT_DESKTOP", "Hyprland")]),
            Desktop::Hyprland
        );
        assert_eq!(desktop(&[("XDG_CURRENT_DESKTOP", "KDE"), ("WAYLAND_DISPLAY", "wayland-0")]), Desktop::Plasma);
        // Plasma sous X11 : KWin X11 ne lit pas les profils de la même façon.
        assert_eq!(desktop(&[("XDG_CURRENT_DESKTOP", "KDE"), ("XDG_SESSION_TYPE", "x11")]), Desktop::Other);
        assert_eq!(desktop(&[("XDG_CURRENT_DESKTOP", "GNOME"), ("WAYLAND_DISPLAY", "wayland-0")]), Desktop::Other);
        assert_eq!(desktop(&[]), Desktop::Other);
    }
}
