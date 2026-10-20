//! Pointeur des souris : accélération et sensibilité, comme « Améliorer la
//! précision du pointeur » et la vitesse du pointeur de Windows. Réglés par
//! le bureau, souris par souris (jamais le pavé tactile d'un portable) :
//! - Hyprland : `device { … }` par souris dans ~/.config/hypr/last1-panel-mouse.conf
//!   (lu par hyprland.conf : la ligne `source` y est ajoutée une fois),
//!   appliqué tout de suite par `hyprctl keyword device[…]:…` ;
//! - KDE Plasma : propriétés des périphériques de KWin (D-Bus), que KWin
//!   garde lui-même.
//!
//! La sensibilité est la vitesse de libinput (-1 à 1, 0 : normale) ; sans
//! accélération, elle multiplie le mouvement par (1 + vitesse).

use crate::error::{CoreError, CoreResult};
use crate::exec::Exec;
use crate::i18n::t;
use crate::settings::MouseSettings;
use crate::system::desktop::Desktop;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

/// Ce que la page Souris montre du pointeur.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PointerState {
    /// Souris vues par le bureau (pas les pavés tactiles).
    pub mice: Vec<String>,
    /// Accélération active.
    pub acceleration: bool,
    /// Vitesse de libinput, -1 à 1.
    pub speed: f64,
    /// Réglable sur ce bureau ; sinon, pourquoi.
    pub unavailable: Option<String>,
}

/// Une souris pour le bureau : son identifiant (nom pour Hyprland, nœud
/// eventN pour KWin) et son nom lisible.
#[derive(Debug, Clone, PartialEq)]
struct Mouse {
    id: String,
    name: String,
}

/// Pavés tactiles, trackpoints, appareils virtuels : jamais touchés.
fn is_mouse(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    !["touchpad", "trackpad", "trackpoint", "virtual", "ydotool"].iter().any(|word| name.contains(word))
}

pub struct Pointer<'a> {
    pub exec: &'a dyn Exec,
    pub desktop: Desktop,
    /// ~/.config/hypr
    pub hypr_dir: PathBuf,
}

impl Pointer<'_> {
    pub fn state(&self, saved: &MouseSettings) -> PointerState {
        let unavailable = |reason: String| PointerState {
            mice: Vec::new(),
            acceleration: saved.acceleration.unwrap_or(true),
            speed: saved.speed.unwrap_or(0.0),
            unavailable: Some(reason),
        };
        let mice = match self.mice() {
            Ok(mice) => mice,
            Err(error) => return unavailable(error.to_string()),
        };
        // Réglés par Last1 Panel : ses valeurs ; sinon, celles du bureau.
        let (acceleration, speed) = match (saved.acceleration, saved.speed) {
            (Some(acceleration), Some(speed)) => (acceleration, speed),
            _ => {
                let (acceleration, speed) = self.current(&mice).unwrap_or((true, 0.0));
                (saved.acceleration.unwrap_or(acceleration), saved.speed.unwrap_or(speed))
            }
        };
        PointerState { mice: mice.into_iter().map(|m| m.name).collect(), acceleration, speed, unavailable: None }
    }

    fn mice(&self) -> CoreResult<Vec<Mouse>> {
        match self.desktop {
            Desktop::Hyprland => self.hyprland_mice(),
            Desktop::Plasma => self.kwin_mice(),
            Desktop::Other => Err(CoreError::message(t(
                "Ce bureau n'est pas encore pris en charge pour le pointeur (Hyprland et KDE Plasma le sont).",
                "This desktop isn't supported yet for the pointer (Hyprland and KDE Plasma are).",
            ))),
        }
    }

    /// Accélération et vitesse actuelles du bureau.
    fn current(&self, mice: &[Mouse]) -> Option<(bool, f64)> {
        match self.desktop {
            Desktop::Hyprland => {
                let option = |name: &str| -> Option<serde_json::Value> {
                    let output = self.exec.run("hyprctl", &["getoption".into(), name.into(), "-j".into()]).ok()?;
                    serde_json::from_str(&output.stdout).ok()
                };
                let profile = option("input:accel_profile")?;
                let speed = option("input:sensitivity").and_then(|v| v["float"].as_f64()).unwrap_or(0.0);
                Some((profile["str"].as_str() != Some("flat"), speed))
            }
            Desktop::Plasma => {
                let mouse = mice.first()?;
                let flat = self.kwin_property(&mouse.id, "pointerAccelerationProfileFlat")?.as_bool()?;
                let speed = self.kwin_property(&mouse.id, "pointerAcceleration")?.as_f64()?;
                Some((!flat, speed))
            }
            Desktop::Other => None,
        }
    }

    /// Applique les réglages à toutes les souris, et les garde.
    pub fn apply(&self, settings: &MouseSettings) -> CoreResult<()> {
        let (Some(acceleration), Some(speed)) = (settings.acceleration, settings.speed) else { return Ok(()) };
        let speed = speed.clamp(-1.0, 1.0);
        let mice = self.mice()?;
        match self.desktop {
            Desktop::Hyprland => self.hyprland_apply(&mice, acceleration, speed),
            Desktop::Plasma => {
                for mouse in &mice {
                    self.kwin_set(
                        &mouse.id,
                        "pointerAccelerationProfileFlat",
                        "b",
                        if acceleration { "false" } else { "true" },
                    )?;
                    self.kwin_set(&mouse.id, "pointerAcceleration", "d", &format!("{speed:.3}"))?;
                }
                Ok(())
            }
            Desktop::Other => Ok(()),
        }
    }

    // --- Hyprland ---

    fn hyprland_mice(&self) -> CoreResult<Vec<Mouse>> {
        let output = self.exec.run_ok("hyprctl", &["devices".into(), "-j".into()])?;
        let value: serde_json::Value = serde_json::from_str(&output.stdout)
            .map_err(|_| CoreError::message(t("réponse de hyprctl illisible", "unreadable hyprctl answer")))?;
        let mut mice: Vec<Mouse> = value["mice"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|mouse| mouse["name"].as_str())
            .filter(|name| is_mouse(name))
            .map(|name| Mouse { id: name.to_string(), name: name.to_string() })
            .collect();
        mice.dedup();
        Ok(mice)
    }

    fn hyprland_conf(&self) -> PathBuf {
        self.hypr_dir.join("last1-panel-mouse.conf")
    }

    fn hyprland_apply(&self, mice: &[Mouse], acceleration: bool, speed: f64) -> CoreResult<()> {
        if self.hypr_dir.join("hyprland.lua").exists() {
            return Err(CoreError::message(t(
                "Votre Hyprland utilise une configuration Lua (hyprland.lua) : Last1 Panel ne sait régler la souris qu'avec hyprland.conf pour l'instant.",
                "Your Hyprland uses a Lua configuration (hyprland.lua): Last1 Panel can only set the mouse with hyprland.conf for now.",
            )));
        }
        let profile = if acceleration { "adaptive" } else { "flat" };
        let speed = format!("{speed:.3}");
        // Gardé : un bloc par souris, lu à chaque démarrage du bureau.
        let mut text = String::from(
            "# Souris réglées avec Last1 Panel (accélération, sensibilité).\n# Last1 Panel réécrit ce fichier à chaque réglage de sa page Souris.\n",
        );
        for mouse in mice {
            text.push_str(&format!(
                "\ndevice {{\n    name = {}\n    accel_profile = {profile}\n    sensitivity = {speed}\n}}\n",
                mouse.id
            ));
        }
        fs::create_dir_all(&self.hypr_dir)?;
        fs::write(self.hyprland_conf(), text)?;
        ensure_sourced(&self.hypr_dir.join("hyprland.conf"), "~/.config/hypr/last1-panel-mouse.conf")?;
        // Tout de suite, sans recharger le bureau.
        for mouse in mice {
            for (key, value) in [("accel_profile", profile), ("sensitivity", speed.as_str())] {
                self.exec
                    .run_ok("hyprctl", &["keyword".into(), format!("device[{}]:{key}", mouse.id), value.into()])?;
            }
        }
        Ok(())
    }

    // --- KDE Plasma (KWin) ---

    fn busctl(&self, args: &[&str]) -> CoreResult<serde_json::Value> {
        let mut all: Vec<String> = vec!["--user".into(), "--json=short".into()];
        all.extend(args.iter().map(|arg| arg.to_string()));
        let output = self.exec.run_ok("busctl", &all)?;
        serde_json::from_str(&output.stdout)
            .map_err(|_| CoreError::message(t("réponse de KWin illisible", "unreadable KWin answer")))
    }

    fn kwin_property(&self, device: &str, property: &str) -> Option<serde_json::Value> {
        let path = format!("/org/kde/KWin/InputDevice/{device}");
        self.busctl(&["get-property", "org.kde.KWin", &path, "org.kde.KWin.InputDevice", property])
            .ok()
            .map(|value| value["data"].clone())
    }

    fn kwin_set(&self, device: &str, property: &str, kind: &str, value: &str) -> CoreResult<()> {
        let path = format!("/org/kde/KWin/InputDevice/{device}");
        let args: Vec<String> =
            ["--user", "set-property", "org.kde.KWin", &path, "org.kde.KWin.InputDevice", property, kind, value]
                .iter()
                .map(|arg| arg.to_string())
                .collect();
        self.exec.run_ok("busctl", &args).map(|_| ())
    }

    fn kwin_mice(&self) -> CoreResult<Vec<Mouse>> {
        let names = self.busctl(&[
            "get-property",
            "org.kde.KWin",
            "/org/kde/KWin/InputDevice",
            "org.kde.KWin.InputDeviceManager",
            "devicesSysNames",
        ])?;
        Ok(names["data"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|id| id.as_str())
            .filter_map(|id| {
                let flag = |property: &str| self.kwin_property(id, property).and_then(|v| v.as_bool()).unwrap_or(false);
                let name = self.kwin_property(id, "name").and_then(|v| v.as_str().map(str::to_string))?;
                (flag("pointer") && !flag("touchpad") && is_mouse(&name)).then(|| Mouse { id: id.to_string(), name })
            })
            .collect())
    }
}

/// Ajoute `source = <file>` à hyprland.conf s'il n'y est pas.
fn ensure_sourced(conf: &Path, file: &str) -> CoreResult<()> {
    let text = fs::read_to_string(conf).unwrap_or_default();
    if text.lines().any(|line| line.trim_start().starts_with("source") && line.contains("last1-panel-mouse.conf")) {
        return Ok(());
    }
    let mut text = text;
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&format!("\n# Souris réglées avec Last1 Panel\nsource = {file}\n"));
    fs::write(conf, text)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::{FakeExec, Output};

    fn ok(stdout: &str) -> Output {
        Output { status: 0, stdout: stdout.into(), ..Default::default() }
    }

    const HYPR_DEVICES: &str = r#"{"mice":[{"address":"0x1","name":"logitech-g-pro-x-superlight","defaultSpeed":0.0},
        {"address":"0x2","name":"elan0001:00-04f3:3138-touchpad","defaultSpeed":0.0}],"keyboards":[]}"#;

    #[test]
    fn hyprland_mouse_without_acceleration() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("hyprland.conf"), "monitor = , preferred, auto, 1\n").unwrap();
        let exec = FakeExec::new()
            .on_args("hyprctl", &["devices", "-j"], ok(HYPR_DEVICES))
            .on_args("hyprctl", &["getoption", "input:accel_profile", "-j"], ok(r#"{"str":"","set":false}"#))
            .on_args("hyprctl", &["getoption", "input:sensitivity", "-j"], ok(r#"{"float":0.0,"set":false}"#));
        let pointer = Pointer { exec: &exec, desktop: Desktop::Hyprland, hypr_dir: dir.path().into() };
        // Jamais réglée : celle du bureau, le pavé tactile laissé de côté.
        let state = pointer.state(&MouseSettings::default());
        assert_eq!(state.mice, ["logitech-g-pro-x-superlight"]);
        assert!(state.acceleration && state.unavailable.is_none());
        let settings = MouseSettings { acceleration: Some(false), speed: Some(-0.25) };
        pointer.apply(&settings).unwrap();
        let conf = fs::read_to_string(dir.path().join("last1-panel-mouse.conf")).unwrap();
        assert!(
            conf.contains("name = logitech-g-pro-x-superlight\n    accel_profile = flat\n    sensitivity = -0.250")
        );
        assert!(!conf.contains("touchpad"));
        // hyprland.conf la lit, une seule fois.
        pointer.apply(&settings).unwrap();
        let main = fs::read_to_string(dir.path().join("hyprland.conf")).unwrap();
        assert_eq!(main.matches("source = ~/.config/hypr/last1-panel-mouse.conf").count(), 1);
        assert!(main.starts_with("monitor = , preferred, auto, 1\n"));
        let calls = exec.calls();
        assert!(calls.contains(&(
            "hyprctl".into(),
            vec!["keyword".into(), "device[logitech-g-pro-x-superlight]:accel_profile".into(), "flat".into()]
        )));
        assert!(calls.contains(&(
            "hyprctl".into(),
            vec!["keyword".into(), "device[logitech-g-pro-x-superlight]:sensitivity".into(), "-0.250".into()]
        )));
        assert_eq!(pointer.state(&settings).speed, -0.25);
    }

    #[test]
    fn kwin_mouse_without_acceleration() {
        let dir = tempfile::tempdir().unwrap();
        fn get<'a>(path: &'a str, property: &'a str) -> Vec<&'a str> {
            vec![
                "--user",
                "--json=short",
                "get-property",
                "org.kde.KWin",
                if path.is_empty() { "/org/kde/KWin/InputDevice" } else { path },
                if path.is_empty() { "org.kde.KWin.InputDeviceManager" } else { "org.kde.KWin.InputDevice" },
                property,
            ]
        }
        let mouse = "/org/kde/KWin/InputDevice/event5";
        let pad = "/org/kde/KWin/InputDevice/event7";
        let exec = FakeExec::new()
            .on_args("busctl", &get("", "devicesSysNames"), ok(r#"{"type":"as","data":["event5","event7"]}"#))
            .on_args("busctl", &get(mouse, "name"), ok(r#"{"type":"s","data":"Razer DeathAdder V3"}"#))
            .on_args("busctl", &get(mouse, "pointer"), ok(r#"{"type":"b","data":true}"#))
            .on_args("busctl", &get(mouse, "touchpad"), ok(r#"{"type":"b","data":false}"#))
            .on_args("busctl", &get(mouse, "pointerAccelerationProfileFlat"), ok(r#"{"type":"b","data":false}"#))
            .on_args("busctl", &get(mouse, "pointerAcceleration"), ok(r#"{"type":"d","data":0.2}"#))
            .on_args("busctl", &get(pad, "name"), ok(r#"{"type":"s","data":"ELAN Touchpad"}"#))
            .on_args("busctl", &get(pad, "pointer"), ok(r#"{"type":"b","data":true}"#))
            .on_args("busctl", &get(pad, "touchpad"), ok(r#"{"type":"b","data":true}"#));
        let pointer = Pointer { exec: &exec, desktop: Desktop::Plasma, hypr_dir: dir.path().into() };
        let state = pointer.state(&MouseSettings::default());
        assert_eq!(
            (state.mice.as_slice(), state.acceleration, state.speed),
            (["Razer DeathAdder V3".to_string()].as_slice(), true, 0.2)
        );
        pointer.apply(&MouseSettings { acceleration: Some(false), speed: Some(0.0) }).unwrap();
        let sets: Vec<Vec<String>> = exec
            .calls()
            .into_iter()
            .filter(|(_, args)| args.get(1).is_some_and(|a| a == "set-property"))
            .map(|(_, a)| a)
            .collect();
        assert_eq!(sets.len(), 2);
        assert_eq!(sets[0][5..], ["pointerAccelerationProfileFlat", "b", "true"]);
        assert_eq!(sets[1][5..], ["pointerAcceleration", "d", "0.000"]);
    }

    #[test]
    fn other_desktops_say_why() {
        let exec = FakeExec::new();
        let pointer = Pointer { exec: &exec, desktop: Desktop::Other, hypr_dir: PathBuf::new() };
        assert!(pointer.state(&MouseSettings::default()).unavailable.is_some());
        assert!(!is_mouse("SynPS/2 Synaptics TouchPad") && is_mouse("Logitech USB Receiver"));
    }
}
