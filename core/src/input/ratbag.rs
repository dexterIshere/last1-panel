//! Réglages enregistrés dans la souris elle-même, comme G HUB : paliers de
//! DPI, palier actif, fréquence d'interrogation. Par ratbagd (libratbag,
//! le moteur de Piper), qui connaît les souris de jeu Logitech G, SteelSeries,
//! Roccat et d'autres ; interrogé par D-Bus (busctl, réponses en JSON).
//! Ce que la souris garde dans sa mémoire reste vrai sur tout PC, Windows
//! compris.

use crate::error::{CoreError, CoreResult};
use crate::exec::Exec;
use crate::i18n::t;
use serde::Serialize;
use serde_json::Value;

const SERVICE: &str = "org.freedesktop.ratbag1";
const MANAGER: &str = "/org/freedesktop/ratbag1";

/// Une souris réglable.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GamingMouse {
    /// Chemin D-Bus de la souris (identifiant pour la régler).
    pub id: String,
    pub name: String,
    /// Chemin D-Bus du profil actif.
    pub profile: String,
    /// Paliers de DPI du profil actif, dans l'ordre de la souris.
    pub stages: Vec<DpiStage>,
    /// DPI que la souris accepte.
    pub dpi_choices: Vec<u32>,
    /// Fréquence d'interrogation (Hz) et celles qu'elle accepte.
    pub rate: Option<u32>,
    pub rate_choices: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DpiStage {
    pub id: String,
    pub dpi: u32,
    pub active: bool,
}

/// État de libratbag sur ce PC.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ratbag {
    /// ratbagd répond (libratbag installé).
    pub installed: bool,
    pub mice: Vec<GamingMouse>,
}

pub struct RatbagClient<'a> {
    pub exec: &'a dyn Exec,
}

impl RatbagClient<'_> {
    fn get(&self, path: &str, interface: &str, property: &str) -> CoreResult<Value> {
        let args: Vec<String> = ["--system", "--json=short", "get-property", SERVICE, path, interface, property]
            .iter()
            .map(|arg| arg.to_string())
            .collect();
        let output = self.exec.run_ok("busctl", &args)?;
        let value: Value = serde_json::from_str(&output.stdout)
            .map_err(|_| CoreError::message(t("réponse de ratbagd illisible", "unreadable ratbagd answer")))?;
        Ok(value["data"].clone())
    }

    fn paths(&self, path: &str, interface: &str, property: &str) -> Vec<String> {
        self.get(path, interface, property)
            .ok()
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect()
    }

    fn numbers(value: &Value) -> Vec<u32> {
        value.as_array().into_iter().flatten().filter_map(|v| v.as_u64()).map(|v| v as u32).collect()
    }

    /// Les souris réglables (vide si ratbagd ne répond pas).
    pub fn state(&self) -> Ratbag {
        let Ok(devices) = self.get(MANAGER, "org.freedesktop.ratbag1.Manager", "Devices") else {
            return Ratbag { installed: false, mice: Vec::new() };
        };
        let mice = devices.as_array().into_iter().flatten().filter_map(|device| self.mouse(device.as_str()?)).collect();
        Ratbag { installed: true, mice }
    }

    fn mouse(&self, device: &str) -> Option<GamingMouse> {
        let name = self.get(device, "org.freedesktop.ratbag1.Device", "Name").ok()?.as_str()?.to_string();
        // Le profil actif (celui que la souris utilise).
        let profile = self.paths(device, "org.freedesktop.ratbag1.Device", "Profiles").into_iter().find(|profile| {
            self.get(profile, "org.freedesktop.ratbag1.Profile", "IsActive").ok().and_then(|v| v.as_bool())
                == Some(true)
        })?;
        let mut dpi_choices = Vec::new();
        let stages = self
            .paths(&profile, "org.freedesktop.ratbag1.Profile", "Resolutions")
            .into_iter()
            .filter_map(|resolution| {
                let interface = "org.freedesktop.ratbag1.Resolution";
                let disabled = self.get(&resolution, interface, "IsDisabled").ok().and_then(|v| v.as_bool());
                if disabled == Some(true) {
                    return None;
                }
                // « u » ou « (uu) » (DPI horizontal et vertical séparés).
                let value = self.get(&resolution, interface, "Resolution").ok()?;
                let dpi = match &value["data"] {
                    Value::Number(n) => n.as_u64()? as u32,
                    Value::Array(xy) => xy.first()?.as_u64()? as u32,
                    _ => return None,
                };
                if dpi_choices.is_empty() {
                    dpi_choices = Self::numbers(&self.get(&resolution, interface, "Resolutions").ok()?);
                }
                let active = self.get(&resolution, interface, "IsActive").ok().and_then(|v| v.as_bool()) == Some(true);
                Some(DpiStage { id: resolution, dpi, active })
            })
            .collect();
        let rate = self.get(&profile, "org.freedesktop.ratbag1.Profile", "ReportRate").ok().and_then(|v| v.as_u64());
        let rate_choices =
            Self::numbers(&self.get(&profile, "org.freedesktop.ratbag1.Profile", "ReportRates").unwrap_or_default());
        Some(GamingMouse {
            id: device.to_string(),
            name,
            profile: profile.clone(),
            stages,
            dpi_choices,
            rate: rate.map(|r| r as u32).filter(|r| *r > 0),
            rate_choices,
        })
    }

    /// Paliers de DPI, palier actif et fréquence d'interrogation, écrits
    /// dans la mémoire de la souris.
    pub fn apply(&self, mouse: &GamingMouse, dpis: &[u32], active: usize, rate: Option<u32>) -> CoreResult<()> {
        let set = |path: &str, interface: &str, property: &str, signature: &[&str]| -> CoreResult<()> {
            let mut args: Vec<String> = ["--system", "set-property", SERVICE, path, interface, property]
                .iter()
                .map(|a| a.to_string())
                .collect();
            args.extend(signature.iter().map(|a| a.to_string()));
            self.exec.run_ok("busctl", &args).map(|_| ())
        };
        for (stage, dpi) in mouse.stages.iter().zip(dpis) {
            if !mouse.dpi_choices.is_empty() && !mouse.dpi_choices.contains(dpi) {
                return Err(CoreError::message(format!(
                    "{dpi} DPI : {}",
                    t("valeur que cette souris n'accepte pas", "a value this mouse doesn't accept")
                )));
            }
            if stage.dpi != *dpi {
                set(&stage.id, "org.freedesktop.ratbag1.Resolution", "Resolution", &["v", "u", &dpi.to_string()])?;
            }
        }
        if let Some(stage) = mouse.stages.get(active).filter(|stage| !stage.active) {
            let args: Vec<String> =
                ["--system", "call", SERVICE, &stage.id, "org.freedesktop.ratbag1.Resolution", "SetActive"]
                    .iter()
                    .map(|a| a.to_string())
                    .collect();
            self.exec.run_ok("busctl", &args)?;
        }
        if let Some(rate) = rate.filter(|rate| Some(*rate) != mouse.rate) {
            if !mouse.rate_choices.is_empty() && !mouse.rate_choices.contains(&rate) {
                return Err(CoreError::message(format!(
                    "{rate} Hz : {}",
                    t("valeur que cette souris n'accepte pas", "a value this mouse doesn't accept")
                )));
            }
            set(&mouse.profile, "org.freedesktop.ratbag1.Profile", "ReportRate", &["u", &rate.to_string()])?;
        }
        // Écrit dans la mémoire de la souris.
        let args: Vec<String> = ["--system", "call", SERVICE, &mouse.id, "org.freedesktop.ratbag1.Device", "Commit"]
            .iter()
            .map(|a| a.to_string())
            .collect();
        self.exec.run_ok("busctl", &args).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::{FakeExec, Output};

    fn ok(stdout: &str) -> Output {
        Output { status: 0, stdout: stdout.into(), ..Default::default() }
    }

    fn get(path: &str, interface: &str, property: &str) -> Vec<String> {
        ["--system", "--json=short", "get-property", SERVICE, path, interface, property]
            .iter()
            .map(|a| a.to_string())
            .collect()
    }

    fn logitech() -> FakeExec {
        let device = "/org/freedesktop/ratbag1/device/hidraw3";
        let profile = "/org/freedesktop/ratbag1/profile/hidraw3/p0";
        let r = |i: u32| format!("/org/freedesktop/ratbag1/resolution/hidraw3/p0/r{i}");
        let mut exec = FakeExec::new().on_args(
            "busctl",
            &get(MANAGER, "org.freedesktop.ratbag1.Manager", "Devices").iter().map(String::as_str).collect::<Vec<_>>(),
            ok(&format!(r#"{{"type":"ao","data":["{device}"]}}"#)),
        );
        let mut answer = |path: &str, interface: &str, property: &str, json: String| {
            let args = get(path, interface, property);
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            exec = std::mem::take(&mut exec).on_args("busctl", &args, ok(&json));
        };
        answer(device, "org.freedesktop.ratbag1.Device", "Name", r#"{"type":"s","data":"Logitech G Pro"}"#.into());
        answer(
            device,
            "org.freedesktop.ratbag1.Device",
            "Profiles",
            format!(r#"{{"type":"ao","data":["{profile}"]}}"#),
        );
        answer(profile, "org.freedesktop.ratbag1.Profile", "IsActive", r#"{"type":"b","data":true}"#.into());
        answer(
            profile,
            "org.freedesktop.ratbag1.Profile",
            "Resolutions",
            format!(r#"{{"type":"ao","data":["{}","{}"]}}"#, r(0), r(1)),
        );
        answer(profile, "org.freedesktop.ratbag1.Profile", "ReportRate", r#"{"type":"u","data":500}"#.into());
        answer(
            profile,
            "org.freedesktop.ratbag1.Profile",
            "ReportRates",
            r#"{"type":"au","data":[125,250,500,1000]}"#.into(),
        );
        for (i, dpi, active) in [(0, "800", true), (1, "[1600,1600]", false)] {
            let interface = "org.freedesktop.ratbag1.Resolution";
            let kind = if dpi.starts_with('[') { "(uu)" } else { "u" };
            answer(
                &r(i),
                interface,
                "Resolution",
                format!(r#"{{"type":"v","data":{{"type":"{kind}","data":{dpi}}}}}"#),
            );
            answer(&r(i), interface, "Resolutions", r#"{"type":"au","data":[400,800,1600,3200]}"#.into());
            answer(&r(i), interface, "IsActive", format!(r#"{{"type":"b","data":{active}}}"#));
            answer(&r(i), interface, "IsDisabled", r#"{"type":"b","data":false}"#.into());
        }
        exec
    }

    #[test]
    fn mouse_is_read_like_piper() {
        let exec = logitech();
        let state = RatbagClient { exec: &exec }.state();
        assert!(state.installed);
        let mouse = &state.mice[0];
        assert_eq!(mouse.name, "Logitech G Pro");
        let stages: Vec<(u32, bool)> = mouse.stages.iter().map(|s| (s.dpi, s.active)).collect();
        assert_eq!(stages, [(800, true), (1600, false)]);
        assert_eq!(mouse.dpi_choices, [400, 800, 1600, 3200]);
        assert_eq!((mouse.rate, mouse.rate_choices.as_slice()), (Some(500), [125, 250, 500, 1000].as_slice()));
    }

    #[test]
    fn settings_are_written_to_the_mouse() {
        let exec = logitech();
        let client = RatbagClient { exec: &exec };
        let mouse = client.state().mice.remove(0);
        client.apply(&mouse, &[400, 1600], 1, Some(1000)).unwrap();
        let writes: Vec<Vec<String>> = exec
            .calls()
            .into_iter()
            .filter(|(_, args)| args.get(1).is_some_and(|a| a == "set-property" || a == "call"))
            .map(|(_, args)| args)
            .collect();
        assert_eq!(
            writes[0][3..],
            [
                "/org/freedesktop/ratbag1/resolution/hidraw3/p0/r0",
                "org.freedesktop.ratbag1.Resolution",
                "Resolution",
                "v",
                "u",
                "400"
            ]
        );
        assert_eq!(
            writes[1][3..],
            ["/org/freedesktop/ratbag1/resolution/hidraw3/p0/r1", "org.freedesktop.ratbag1.Resolution", "SetActive"]
        );
        assert_eq!(
            writes[2][3..],
            [
                "/org/freedesktop/ratbag1/profile/hidraw3/p0",
                "org.freedesktop.ratbag1.Profile",
                "ReportRate",
                "u",
                "1000"
            ]
        );
        assert_eq!(
            writes[3][3..],
            ["/org/freedesktop/ratbag1/device/hidraw3", "org.freedesktop.ratbag1.Device", "Commit"]
        );
        // Une valeur que la souris n'accepte pas : rien n'est écrit.
        assert!(client.apply(&mouse, &[1234], 0, None).is_err());
    }

    #[test]
    fn without_ratbagd() {
        let exec = FakeExec::new().on_args(
            "busctl",
            &get(MANAGER, "org.freedesktop.ratbag1.Manager", "Devices").iter().map(String::as_str).collect::<Vec<_>>(),
            Output {
                status: 1,
                stderr: "The name org.freedesktop.ratbag1 was not provided by any .service files".into(),
                ..Default::default()
            },
        );
        assert_eq!(RatbagClient { exec: &exec }.state(), Ratbag { installed: false, mice: Vec::new() });
    }
}
