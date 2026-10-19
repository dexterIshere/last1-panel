//! Bureau de la session : Hyprland (bureau Signal de go-to-linux) ou KDE Plasma.

use serde::Serialize;
use std::collections::BTreeMap;
use std::process::{Command, Stdio};

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

/// Variables que le bureau a données à systemd (Hyprland par
/// dbus-update-activation-environment, Plasma de lui-même) : le service de
/// la session, lancé avant le bureau, les relit à chaque usage.
pub fn session_env() -> BTreeMap<String, String> {
    let text = Command::new("systemctl")
        .args(["--user", "show-environment"])
        .stderr(Stdio::null())
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
        .unwrap_or_default();
    text.lines().filter_map(|line| line.split_once('=')).map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

/// Ferme les fenêtres de Last1 Panel ouvertes par ce compte (paquet ou
/// AppImage : le programme s'appelle « last1-panel » ; l'outil
/// « last1-panel-cli » et les jeux ne sont pas touchés).
pub fn close_windows() {
    let mut command = Command::new("pkill");
    command.args(["-TERM", "-x", "last1-panel"]);
    if let Ok(meta) = std::fs::metadata("/proc/self") {
        command.args(["-u", &std::os::unix::fs::MetadataExt::uid(&meta).to_string()]);
    }
    let _ = command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status();
}

/// Notification du bureau, depuis le service de la session.
pub fn notify(message: &str) {
    let _ = Command::new("notify-send")
        .args(["--app-name=Last1 Panel", "--icon=last1-panel", "Last1 Panel", message])
        .envs(session_env())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Notification avec un bouton, qui reste jusqu'à ce que l'utilisateur
/// réponde : vrai s'il a cliqué sur le bouton (notify-send attend la
/// réponse) ; la fermer ne fait rien.
pub fn ask(message: &str, button: &str) -> bool {
    Command::new("notify-send")
        .args(["--app-name=Last1 Panel", "--icon=last1-panel", "--urgency=critical", &format!("--action=yes={button}")])
        .args(["Last1 Panel", message])
        .envs(session_env())
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .is_ok_and(|output| String::from_utf8_lossy(&output.stdout).trim() == "yes")
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
