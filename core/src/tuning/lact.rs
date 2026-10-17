//! Carte graphique par le service LACT (`lactd`, paquet `lact`) : limite de
//! puissance, ventilateurs, fréquences, mesures. NVIDIA (NVML), AMD et Intel.
//!
//! Protocole : une requête JSON par ligne sur /run/lactd.sock (accessible au
//! groupe wheel), une réponse par ligne `{"status":"ok","data":…}` ou
//! `{"status":"error","data":{"description":…}}`. Un changement de
//! configuration est annulé par LACT s'il n'est pas confirmé à temps
//! (5 s par défaut) : une carte qui plante revient seule à ses réglages.

use crate::error::{CoreError, CoreResult};
use crate::i18n::t;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    /// Identifiant LACT : « 10DE:2704-1462:5110-0000:09:00.0 ».
    pub id: String,
    pub name: String,
    pub integrated: bool,
    /// Adresse PCI, la fin de l'identifiant.
    pub pci_slot: String,
}

/// Mesures de la carte, ce que la fenêtre affiche.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    /// Températures en °C (« GPU », « GPU Hotspot », « VRAM »… ou edge/junction/mem).
    pub temperatures: BTreeMap<String, f64>,
    /// Ventilateurs en % (0 à 100) et en tr/min.
    pub fan_percent: Option<f64>,
    pub fan_rpm: Option<u64>,
    pub fan_auto: bool,
    /// Puissance en W : actuelle, limite, bornes et valeur d'usine.
    pub power: Option<f64>,
    pub power_cap: Option<f64>,
    pub power_cap_min: Option<f64>,
    pub power_cap_max: Option<f64>,
    pub power_cap_default: Option<f64>,
    /// Fréquences en MHz.
    pub gpu_clock: Option<u64>,
    pub vram_clock: Option<u64>,
    /// Mémoire vidéo en octets.
    pub vram_used: Option<u64>,
    pub vram_total: Option<u64>,
    /// Charge en %.
    pub busy_percent: Option<u64>,
}

/// Décalage de fréquence réglable (MHz) d'une carte NVIDIA.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OffsetRange {
    pub current: i32,
    pub min: i32,
    pub max: i32,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ClockRanges {
    /// Décalages du cœur et de la mémoire à l'état P0 (NVIDIA).
    pub gpu_offset: Option<OffsetRange>,
    pub mem_offset: Option<OffsetRange>,
}

/// Ventilateurs : pilotés par la carte, à vitesse fixe, ou selon une courbe.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", tag = "mode")]
pub enum Fans {
    Auto,
    /// Vitesse en % (0 à 100).
    Fixed {
        percent: f64,
    },
    /// Points (température en °C, vitesse en %).
    Curve {
        points: Vec<(i32, f64)>,
    },
}

/// Réglages proposés par la fenêtre ; le reste de la configuration de LACT
/// (profils, réglages avancés faits dans LACT) est gardé tel quel.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Tuning {
    /// Limite de puissance en W (absente : valeur d'usine).
    pub power_cap: Option<f64>,
    pub fans: Fans,
    /// Décalages en MHz (NVIDIA).
    pub gpu_offset: Option<i32>,
    pub mem_offset: Option<i32>,
}

pub struct Lact {
    socket: PathBuf,
}

/// Socket du service : celui du système, sinon celui d'un service lancé
/// par l'utilisateur.
pub fn find_socket() -> Option<PathBuf> {
    let uid = std::fs::metadata("/proc/self").map(|m| std::os::unix::fs::MetadataExt::uid(&m)).unwrap_or(0);
    [PathBuf::from("/run/lactd.sock"), PathBuf::from(format!("/run/user/{uid}/lactd.sock"))]
        .into_iter()
        .find(|path| path.exists())
}

impl Lact {
    pub fn new(socket: PathBuf) -> Self {
        Self { socket }
    }

    pub fn connect() -> CoreResult<Self> {
        find_socket().map(Self::new).ok_or_else(not_running)
    }

    pub fn socket(&self) -> &Path {
        &self.socket
    }

    /// Une requête, une réponse.
    pub fn request(&self, command: &str, args: Option<Value>) -> CoreResult<Value> {
        let mut stream = UnixStream::connect(&self.socket).map_err(|error| match error.kind() {
            std::io::ErrorKind::PermissionDenied => CoreError::message(t(
                "Accès refusé au service LACT : votre compte doit être dans le groupe wheel.",
                "Access to the LACT service denied: your account must be in the wheel group.",
            )),
            _ => not_running(),
        })?;
        stream.set_read_timeout(Some(Duration::from_secs(10)))?;
        let mut request = json!({ "command": command });
        if let Some(args) = args {
            request["args"] = args;
        }
        let mut line = serde_json::to_vec(&request).expect("requête sérialisable");
        line.push(b'\n');
        stream.write_all(&line)?;
        let mut answer = String::new();
        BufReader::new(stream).read_line(&mut answer)?;
        let answer: Value =
            serde_json::from_str(&answer).map_err(|error| CoreError::Unreadable("LACT".into(), error.to_string()))?;
        match answer.get("status").and_then(Value::as_str) {
            Some("ok") => Ok(answer.get("data").cloned().unwrap_or(Value::Null)),
            Some("error") => Err(CoreError::ToolFailed { program: "LACT".into(), detail: error_text(&answer["data"]) }),
            _ => Err(CoreError::Unreadable("LACT".into(), answer.to_string())),
        }
    }

    pub fn devices(&self) -> CoreResult<Vec<Device>> {
        let data = self.request("list_devices", None)?;
        let list = data.as_array().cloned().unwrap_or_default();
        Ok(list
            .iter()
            .filter_map(|entry| {
                let id = entry.get("id")?.as_str()?.to_string();
                Some(Device {
                    name: entry.get("name").and_then(Value::as_str).unwrap_or(&id).to_string(),
                    integrated: entry.get("device_type").and_then(Value::as_str) == Some("Integrated"),
                    pci_slot: id.rsplit('-').next().unwrap_or_default().to_string(),
                    id,
                })
            })
            .collect())
    }

    pub fn stats(&self, id: &str) -> CoreResult<Stats> {
        Ok(parse_stats(&self.request("device_stats", Some(json!({ "id": id })))?))
    }

    pub fn clock_ranges(&self, id: &str) -> CoreResult<ClockRanges> {
        Ok(parse_clocks(&self.request("device_clocks_info", Some(json!({ "id": id })))?))
    }

    /// Configuration complète de la carte (objet JSON de LACT, gardé tel quel).
    pub fn config(&self, id: &str) -> CoreResult<Value> {
        let config = self.request("get_gpu_config", Some(json!({ "id": id })))?;
        Ok(if config.is_null() { json!({}) } else { config })
    }

    /// Essaie la configuration : renvoie les secondes laissées pour confirmer.
    pub fn try_config(&self, id: &str, config: &Value) -> CoreResult<u64> {
        let data = self.request("set_gpu_config", Some(json!({ "id": id, "config": config })))?;
        Ok(data.as_u64().unwrap_or(5))
    }

    pub fn confirm(&self, keep: bool) -> CoreResult<()> {
        let command = if keep { "confirm" } else { "revert" };
        self.request("confirm_pending_config", Some(json!({ "command": command })))?;
        Ok(())
    }
}

fn not_running() -> CoreError {
    CoreError::message(t(
        "Le service LACT ne tourne pas (paquet lact, service lactd).",
        "The LACT service is not running (lact package, lactd service).",
    ))
}

/// Message d'erreur de LACT, causes comprises (« contexte : cause »).
fn error_text(data: &Value) -> String {
    if let Some(text) = data.as_str() {
        return text.to_string();
    }
    let mut parts = Vec::new();
    let mut current = Some(data);
    while let Some(error) = current {
        if let Some(description) = error.get("description").and_then(Value::as_str) {
            parts.push(description.to_string());
        }
        current = error.get("source").filter(|source| !source.is_null());
    }
    parts.join(" : ")
}

fn parse_stats(data: &Value) -> Stats {
    let f = |path: &[&str]| path.iter().try_fold(data, |value, key| value.get(key)).and_then(Value::as_f64);
    let u = |path: &[&str]| path.iter().try_fold(data, |value, key| value.get(key)).and_then(Value::as_u64);
    let temperatures = data
        .get("temps")
        .and_then(Value::as_object)
        .map(|temps| {
            temps.iter().filter_map(|(name, entry)| Some((name.clone(), entry.get("current")?.as_f64()?))).collect()
        })
        .unwrap_or_default();
    // pwm sur 255 ; NVIDIA : moyenne des ventilateurs en % × 2,55.
    let fan_percent = f(&["fan", "pwm_current"]).map(|pwm| (pwm / 2.55).round().clamp(0.0, 100.0));
    Stats {
        temperatures,
        fan_percent,
        fan_rpm: u(&["fan", "speed_current"]),
        fan_auto: !data.get("fan").and_then(|fan| fan.get("control_enabled")).and_then(Value::as_bool).unwrap_or(false),
        power: f(&["power", "current"]).or_else(|| f(&["power", "average"])),
        power_cap: f(&["power", "cap_current"]),
        power_cap_min: f(&["power", "cap_min"]),
        power_cap_max: f(&["power", "cap_max"]),
        power_cap_default: f(&["power", "cap_default"]),
        gpu_clock: u(&["clockspeed", "gpu_clockspeed"]),
        vram_clock: u(&["clockspeed", "vram_clockspeed"]),
        vram_used: u(&["vram", "used"]),
        vram_total: u(&["vram", "total"]),
        busy_percent: u(&["busy_percent"]),
    }
}

fn parse_clocks(data: &Value) -> ClockRanges {
    let table = data.get("table").filter(|table| table.get("type").and_then(Value::as_str) == Some("nvidia"));
    let offset = |key: &str| -> Option<OffsetRange> {
        let offsets = table?.get("value")?.get(key)?.as_object()?;
        serde_json::from_value(offsets.get("0").or_else(|| offsets.values().next())?.clone()).ok()
    };
    ClockRanges { gpu_offset: offset("gpu_offsets"), mem_offset: offset("mem_offsets") }
}

/// Nos réglages lus dans la configuration de LACT.
pub fn tuning_from_config(config: &Value) -> Tuning {
    let settings = config.get("fan_control_settings");
    let fans = if !config.get("fan_control_enabled").and_then(Value::as_bool).unwrap_or(false) {
        Fans::Auto
    } else if settings.and_then(|s| s.get("mode")).and_then(Value::as_str) == Some("static") {
        let speed = settings.and_then(|s| s.get("static_speed")).and_then(Value::as_f64).unwrap_or(0.5);
        Fans::Fixed { percent: (speed * 100.0).round() }
    } else {
        let mut points: Vec<(i32, f64)> = settings
            .and_then(|s| s.get("curve"))
            .and_then(Value::as_object)
            .map(|curve| {
                curve
                    .iter()
                    .filter_map(|(temp, speed)| Some((temp.parse().ok()?, (speed.as_f64()? * 100.0).round())))
                    .collect()
            })
            .unwrap_or_default();
        points.sort_by_key(|(temp, _)| *temp);
        Fans::Curve { points }
    };
    let offset = |key: &str| config.get(key).and_then(|o| o.get("0")).and_then(Value::as_i64).map(|v| v as i32);
    Tuning {
        power_cap: config.get("power_cap").and_then(Value::as_f64),
        fans,
        gpu_offset: offset("gpu_clock_offsets"),
        mem_offset: offset("mem_clock_offsets"),
    }
}

/// La configuration de LACT avec nos réglages : seules nos clés changent.
/// `nvidia` : clé de température des courbes (« GPU » ; AMD : « edge »).
pub fn apply_tuning(config: &Value, tuning: &Tuning, nvidia: bool) -> Value {
    let mut config = if config.is_object() { config.clone() } else { json!({}) };
    let object = config.as_object_mut().expect("objet");
    match tuning.power_cap {
        Some(cap) => object.insert("power_cap".into(), json!(cap)),
        None => object.remove("power_cap"),
    };
    let mut settings = object.get("fan_control_settings").cloned().filter(Value::is_object).unwrap_or_else(|| {
        json!({ "mode": "curve", "static_speed": 0.5, "temperature_key": if nvidia { "GPU" } else { "edge" }, "interval_ms": 500, "curve": {} })
    });
    match &tuning.fans {
        Fans::Auto => {
            object.insert("fan_control_enabled".into(), json!(false));
        }
        Fans::Fixed { percent } => {
            settings["mode"] = json!("static");
            settings["static_speed"] = json!((percent / 100.0).clamp(0.0, 1.0));
            object.insert("fan_control_enabled".into(), json!(true));
            object.insert("fan_control_settings".into(), settings);
        }
        Fans::Curve { points } => {
            let curve: serde_json::Map<String, Value> =
                points.iter().map(|(temp, speed)| (temp.to_string(), json!((speed / 100.0).clamp(0.0, 1.0)))).collect();
            settings["mode"] = json!("curve");
            settings["curve"] = Value::Object(curve);
            object.insert("fan_control_enabled".into(), json!(true));
            object.insert("fan_control_settings".into(), settings);
        }
    }
    for (key, value) in [("gpu_clock_offsets", tuning.gpu_offset), ("mem_clock_offsets", tuning.mem_offset)] {
        let mut offsets = object.get(key).and_then(Value::as_object).cloned().unwrap_or_default();
        match value {
            Some(0) | None => offsets.remove("0"),
            Some(offset) => offsets.insert("0".into(), json!(offset)),
        };
        if offsets.is_empty() {
            object.remove(key);
        } else {
            object.insert(key.into(), Value::Object(offsets));
        }
    }
    config
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;
    use std::sync::{Arc, Mutex};

    /// Faux service LACT : répond à chaque ligne par la réponse prévue pour
    /// sa commande, et garde les requêtes reçues.
    fn fake_daemon(responses: Vec<(&'static str, Value)>) -> (tempfile::TempDir, Lact, Arc<Mutex<Vec<Value>>>) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lactd.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { break };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                while reader.read_line(&mut line).unwrap_or(0) > 0 {
                    let request: Value = serde_json::from_str(&line).unwrap();
                    let command = request["command"].as_str().unwrap().to_string();
                    log.lock().unwrap().push(request);
                    let answer = responses
                        .iter()
                        .find(|(name, _)| *name == command)
                        .map(|(_, answer)| answer.clone())
                        .unwrap_or(json!({"status": "error", "data": {"description": "Failed to deserialize", "source": null}}));
                    let mut out = stream.try_clone().unwrap();
                    writeln!(out, "{answer}").unwrap();
                    line.clear();
                }
            }
        });
        let lact = Lact::new(path);
        (dir, lact, seen)
    }

    const STATS: &str = r#"{"status":"ok","data":{"fan":{"control_enabled":false,"pwm_current":115,"speed_current":1450,"pwm_max":255,"pwm_min":76,"pmfw_info":{}},
        "clockspeed":{"gpu_clockspeed":2610,"vram_clockspeed":10501,"sensors":{}},"voltage":{},"vram":{"total":8589934592,"used":2147483648},
        "power":{"current":187.4,"cap_current":220.0,"cap_max":270.0,"cap_min":100.0,"cap_default":220.0},
        "temps":{"GPU":{"current":64.0,"primary":true,"display_only":false},"GPU Hotspot":{"current":77.0,"primary":false,"display_only":false}},
        "busy_percent":97}}"#;

    #[test]
    fn talks_to_the_daemon() {
        let (_dir, lact, seen) = fake_daemon(vec![
            (
                "list_devices",
                json!({"status":"ok","data":[{"id":"10DE:2484-1458:4036-0000:01:00.0","name":"GeForce RTX 3070","device_type":"Dedicated"}]}),
            ),
            ("device_stats", serde_json::from_str(STATS).unwrap()),
            (
                "device_clocks_info",
                json!({"status":"ok","data":{"table":{"type":"nvidia","value":{"gpu_offsets":{"0":{"current":0,"min":-1000,"max":1000}},"mem_offsets":{"0":{"current":500,"min":-2000,"max":6000}},"gpu_vf_curve":[]}}}}),
            ),
        ]);
        let devices = lact.devices().unwrap();
        assert_eq!(devices[0].name, "GeForce RTX 3070");
        assert_eq!(devices[0].pci_slot, "0000:01:00.0");
        assert!(!devices[0].integrated);

        let stats = lact.stats(&devices[0].id).unwrap();
        assert_eq!(stats.temperatures["GPU"], 64.0);
        assert_eq!(stats.fan_percent, Some(45.0));
        assert_eq!(stats.fan_rpm, Some(1450));
        assert!(stats.fan_auto);
        assert_eq!(stats.power, Some(187.4));
        assert_eq!(stats.power_cap_max, Some(270.0));
        assert_eq!(stats.vram_used, Some(2147483648));
        assert_eq!(stats.busy_percent, Some(97));

        let clocks = lact.clock_ranges(&devices[0].id).unwrap();
        assert_eq!(clocks.mem_offset, Some(OffsetRange { current: 500, min: -2000, max: 6000 }));

        let requests = seen.lock().unwrap();
        assert_eq!(requests[0], json!({"command": "list_devices"}));
        assert_eq!(requests[1], json!({"command": "device_stats", "args": {"id": "10DE:2484-1458:4036-0000:01:00.0"}}));
    }

    #[test]
    fn errors_keep_their_causes() {
        let (_dir, lact, _) = fake_daemon(vec![(
            "set_gpu_config",
            json!({"status":"error","data":{"description":"Could not apply settings","source":{"description":"There is an unconfirmed configuration change pending","source":null}}}),
        )]);
        let error = lact.try_config("id", &json!({})).unwrap_err().to_string();
        assert!(
            error.contains("Could not apply settings : There is an unconfirmed configuration change pending"),
            "{error}"
        );
    }

    #[test]
    fn confirm_and_revert_requests() {
        let (_dir, lact, seen) = fake_daemon(vec![
            ("set_gpu_config", json!({"status":"ok","data":5})),
            ("confirm_pending_config", json!({"status":"ok","data":null})),
        ]);
        assert_eq!(lact.try_config("id", &json!({"power_cap": 200.0})).unwrap(), 5);
        lact.confirm(true).unwrap();
        lact.confirm(false).unwrap();
        let requests = seen.lock().unwrap();
        assert_eq!(requests[0], json!({"command":"set_gpu_config","args":{"id":"id","config":{"power_cap":200.0}}}));
        assert_eq!(requests[1], json!({"command":"confirm_pending_config","args":{"command":"confirm"}}));
        assert_eq!(requests[2]["args"]["command"], "revert");
    }

    #[test]
    fn missing_daemon_is_explained() {
        let lact = Lact::new(PathBuf::from("/nonexistent/lactd.sock"));
        assert!(lact.devices().unwrap_err().to_string().contains("lactd"));
    }

    #[test]
    fn tuning_changes_only_our_keys() {
        // Configuration écrite par LACT (exemple de ses tests), avec une clé
        // que nous ne connaissons pas.
        let config: Value = serde_json::from_str(r#"{"fan_control_enabled":false,"fan_control_settings":{"mode":"curve","static_speed":0.5938412,"temperature_key":"edge","interval_ms":500,"curve":{"40":0.3,"50":0.35,"60":0.5,"70":0.75,"80":1.0},"spindown_delay_ms":1000,"change_threshold":2},"power_cap":318.0,"gpu_clock_offsets":{"0":-64},"uma_carveout":512}"#).unwrap();
        let tuning = tuning_from_config(&config);
        assert_eq!(tuning.power_cap, Some(318.0));
        assert_eq!(tuning.fans, Fans::Auto);
        assert_eq!(tuning.gpu_offset, Some(-64));
        assert_eq!(tuning.mem_offset, None);

        let next = Tuning {
            power_cap: Some(250.0),
            fans: Fans::Curve { points: vec![(30, 25.0), (60, 50.0), (85, 100.0)] },
            gpu_offset: Some(150),
            mem_offset: Some(800),
        };
        let applied = apply_tuning(&config, &next, false);
        assert_eq!(applied["power_cap"], json!(250.0));
        assert_eq!(applied["fan_control_enabled"], json!(true));
        assert_eq!(applied["fan_control_settings"]["curve"], json!({"30": 0.25, "60": 0.5, "85": 1.0}));
        assert_eq!(applied["fan_control_settings"]["spindown_delay_ms"], json!(1000), "réglage de LACT gardé");
        assert_eq!(applied["gpu_clock_offsets"], json!({"0": 150}));
        assert_eq!(applied["mem_clock_offsets"], json!({"0": 800}));
        assert_eq!(applied["uma_carveout"], json!(512), "clé inconnue gardée");
        assert_eq!(tuning_from_config(&applied), next);

        let reset = apply_tuning(
            &applied,
            &Tuning { power_cap: None, fans: Fans::Fixed { percent: 60.0 }, gpu_offset: Some(0), mem_offset: None },
            false,
        );
        assert!(reset.get("power_cap").is_none());
        assert!(reset.get("gpu_clock_offsets").is_none());
        assert_eq!(reset["fan_control_settings"]["mode"], json!("static"));
        assert_eq!(reset["fan_control_settings"]["static_speed"], json!(0.6));
    }

    #[test]
    fn new_curve_uses_the_nvidia_sensor() {
        let applied = apply_tuning(
            &json!(null),
            &Tuning {
                power_cap: None,
                fans: Fans::Curve { points: vec![(50, 40.0)] },
                gpu_offset: None,
                mem_offset: None,
            },
            true,
        );
        assert_eq!(applied["fan_control_settings"]["temperature_key"], json!("GPU"));
        assert_eq!(applied["fan_control_settings"]["interval_ms"], json!(500));
    }
}
