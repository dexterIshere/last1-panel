//! Ce qui entoure une partie et que MangoHud ne voit pas : secteur ou
//! batterie, mode de puissance du portable, préférence d'énergie du
//! processeur, limite de puissance de la carte des jeux et ce qui la freine,
//! Dynamic Boost des RTX de portable (nvidia-powerd), image qui passe par une
//! autre puce pour arriver à l'écran. Relevé par le service de la session
//! pendant la partie (cli.rs, watch_game) et gardé à côté du journal de
//! MangoHud (`<partie>.context.json`) : c'est ce qui dit, sur un portable
//! surtout, pourquoi un jeu tourne moins vite que sous Windows.

use crate::exec::Exec;
use crate::system::gpu::{self, Gpu, Vendor};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GameContext {
    /// Relevés pris pendant la partie.
    pub samples: u32,
    /// Sur batterie au moins une fois (chargeur débranché).
    pub on_battery: bool,
    /// Modes de puissance de la machine vus (ACPI platform_profile : quiet,
    /// balanced, performance…), ceux que règle l'appli du fabricant sous Windows.
    pub platform_profiles: Vec<String>,
    /// Préférences d'énergie du processeur vues (energy_performance_preference).
    pub cpu_preferences: Vec<String>,
    /// Limite de puissance de la carte des jeux appliquée (la plus haute vue),
    /// et la plus haute qu'elle accepte (W).
    pub gpu_power_limit: Option<f64>,
    pub gpu_power_limit_max: Option<f64>,
    /// Freins vus sur la carte : « power » (limite de puissance), « thermal »
    /// (chaleur), « power-brake » (chargeur trop faible, batterie),
    /// « hw-slowdown ».
    pub gpu_throttle: Vec<String>,
    /// nvidia-powerd actif (Dynamic Boost des RTX de portable).
    pub nvidia_powerd: Option<bool>,
    /// La carte des jeux ne dessine pas le bureau : l'image passe par une
    /// autre puce pour arriver à l'écran (portable hybride).
    pub through_other_gpu: bool,
}

/// Un relevé.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sample {
    pub on_battery: bool,
    pub platform_profile: Option<String>,
    pub cpu_preference: Option<String>,
    pub gpu_power_limit: Option<f64>,
    pub gpu_power_limit_max: Option<f64>,
    pub gpu_throttle: Vec<String>,
    pub nvidia_powerd: Option<bool>,
    pub through_other_gpu: bool,
}

impl GameContext {
    pub fn add(&mut self, sample: &Sample) {
        let push = |list: &mut Vec<String>, value: &str| {
            if !list.iter().any(|v| v == value) {
                list.push(value.to_string());
            }
        };
        self.samples += 1;
        self.on_battery |= sample.on_battery;
        if let Some(profile) = &sample.platform_profile {
            push(&mut self.platform_profiles, profile);
        }
        if let Some(preference) = &sample.cpu_preference {
            push(&mut self.cpu_preferences, preference);
        }
        let highest = |a: Option<f64>, b: Option<f64>| match (a, b) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        };
        self.gpu_power_limit = highest(self.gpu_power_limit, sample.gpu_power_limit);
        self.gpu_power_limit_max = highest(self.gpu_power_limit_max, sample.gpu_power_limit_max);
        for reason in &sample.gpu_throttle {
            push(&mut self.gpu_throttle, reason);
        }
        self.nvidia_powerd = sample.nvidia_powerd.or(self.nvidia_powerd);
        self.through_other_gpu |= sample.through_other_gpu;
    }
}

/// Fichier du relevé d'une partie, à côté de son journal MangoHud.
pub fn file_for(log: &Path) -> PathBuf {
    log.with_extension("context.json")
}

pub fn load(log: &Path) -> Option<GameContext> {
    serde_json::from_str(&fs::read_to_string(file_for(log)).ok()?).ok()
}

pub fn save(log: &Path, context: &GameContext) {
    if let Ok(text) = serde_json::to_string(context) {
        let _ = fs::write(file_for(log), text);
    }
}

/// Journal de la partie en cours : le plus récent, écrit il y a moins de
/// 2 min (MangoHud y écrit tout le temps de la partie).
pub fn current_log(dir: &Path) -> Option<PathBuf> {
    fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "csv")
                && !path.file_name().is_some_and(|n| n.to_string_lossy().ends_with("_summary.csv"))
        })
        .filter_map(|path| Some((fs::metadata(&path).and_then(|m| m.modified()).ok()?, path)))
        .filter(|(at, _)| SystemTime::now().duration_since(*at).unwrap_or_default() < Duration::from_secs(120))
        .max_by_key(|(at, _)| *at)
        .map(|(_, path)| path)
}

fn read(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// Un relevé, maintenant. `sys` : /sys. nvidia-smi n'est lancé que pour une
/// carte des jeux NVIDIA.
pub fn sample(sys: &Path, exec: &dyn Exec, gpus: &[Gpu]) -> Sample {
    // Secteur : une alimentation « Mains » branchée ; sans aucune (PC fixe),
    // jamais sur batterie.
    let mains: Vec<bool> = fs::read_dir(sys.join("class/power_supply"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| read(&entry.path().join("type")).as_deref() == Some("Mains"))
        .map(|entry| read(&entry.path().join("online")).as_deref() == Some("1"))
        .collect();
    let games = gpu::games_gpu(gpus);
    let mut sample = Sample {
        on_battery: !mains.is_empty() && !mains.iter().any(|online| *online),
        platform_profile: read(&sys.join("firmware/acpi/platform_profile")),
        cpu_preference: read(&sys.join("devices/system/cpu/cpu0/cpufreq/energy_performance_preference")),
        through_other_gpu: gpu::offload(gpus).is_some(),
        ..Sample::default()
    };
    match games.map(|g| (g.vendor, g.pci_slot.as_str())) {
        Some((Vendor::Nvidia, slot)) => {
            if let Some((limit, max, reasons)) = nvidia_power(exec, slot) {
                sample.gpu_power_limit = limit;
                sample.gpu_power_limit_max = max;
                sample.gpu_throttle = reasons;
            }
            sample.nvidia_powerd = exec
                .run("systemctl", &["is-active".into(), "nvidia-powerd.service".into()])
                .ok()
                .map(|output| output.stdout.trim() == "active");
        }
        // Carte AMD : sa limite de puissance (hwmon, en µW).
        Some((Vendor::Amd, slot)) => {
            let hwmon = fs::read_dir(sys.join("bus/pci/devices").join(slot).join("hwmon"))
                .into_iter()
                .flatten()
                .flatten()
                .next()
                .map(|entry| entry.path());
            let watts = |name: &str| {
                hwmon
                    .as_ref()
                    .and_then(|dir| read(&dir.join(name)))
                    .and_then(|v| v.parse::<f64>().ok())
                    .map(|uw| uw / 1e6)
            };
            sample.gpu_power_limit = watts("power1_cap");
            sample.gpu_power_limit_max = watts("power1_cap_max");
        }
        _ => {}
    }
    sample
}

/// Limite de puissance appliquée, la plus haute acceptée, et les freins
/// actifs de la carte NVIDIA à l'emplacement `slot`.
fn nvidia_power(exec: &dyn Exec, slot: &str) -> Option<(Option<f64>, Option<f64>, Vec<String>)> {
    // clocks_event_reasons : nom actuel ; clocks_throttle_reasons : pilotes
    // plus anciens.
    ["clocks_event_reasons.active", "clocks_throttle_reasons.active"].iter().find_map(|reasons| {
        let output = exec
            .run(
                "nvidia-smi",
                &[
                    format!("--query-gpu=pci.bus_id,enforced.power.limit,power.max_limit,{reasons}"),
                    "--format=csv,noheader,nounits".into(),
                ],
            )
            .ok()
            .filter(|output| output.success())?;
        parse_nvidia_power(&output.stdout, slot)
    })
}

/// « 00000000:01:00.0, 80.00, 140.00, 0x0000000000000004 » pour la carte de
/// `slot` (« 0000:01:00.0 »).
fn parse_nvidia_power(stdout: &str, slot: &str) -> Option<(Option<f64>, Option<f64>, Vec<String>)> {
    let tail = slot.split_once(':').map_or(slot, |(_, rest)| rest).to_ascii_lowercase();
    let line = stdout
        .lines()
        .find(|line| line.split(',').next().is_some_and(|bus| bus.trim().to_ascii_lowercase().ends_with(&tail)))?;
    let fields: Vec<&str> = line.split(',').map(str::trim).collect();
    let watts = |i: usize| fields.get(i).and_then(|v| v.parse::<f64>().ok());
    let mask = fields.get(3).and_then(|v| u64::from_str_radix(v.trim_start_matches("0x"), 16).ok()).unwrap_or(0);
    // Masque de NVML : 0x4 limite de puissance, 0x8 ralentissement matériel,
    // 0x20 et 0x40 chaleur, 0x80 frein de puissance (chargeur, batterie).
    let reasons = [(0x4, "power"), (0x8, "hw-slowdown"), (0x20, "thermal"), (0x40, "thermal"), (0x80, "power-brake")]
        .iter()
        .filter(|(bit, _)| mask & bit != 0)
        .map(|(_, name)| name.to_string())
        .fold(Vec::new(), |mut list: Vec<String>, name| {
            if !list.contains(&name) {
                list.push(name);
            }
            list
        });
    Some((watts(1), watts(2), reasons))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::FakeExec;

    fn write(path: PathBuf, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn gpu(vendor: Vendor, slot: &str, boot_vga: bool, integrated: bool, games: bool) -> Gpu {
        Gpu {
            card: "card".into(),
            pci_slot: slot.into(),
            vendor,
            vendor_id: 0,
            device_id: 0,
            name: None,
            driver: None,
            driver_version: None,
            boot_vga,
            integrated,
            games,
            connectors: vec![],
        }
    }

    #[test]
    fn nvidia_brakes_are_read() {
        let stdout = "00000000:01:00.0, 80.00, 140.00, 0x00000000000000A4\n";
        let (limit, max, reasons) = parse_nvidia_power(stdout, "0000:01:00.0").unwrap();
        assert_eq!((limit, max), (Some(80.0), Some(140.0)));
        assert_eq!(reasons, ["power", "thermal", "power-brake"]);
        // Valeur inconnue (« [N/A] ») : rien d'inventé.
        let (limit, _, reasons) = parse_nvidia_power("00000000:01:00.0, [N/A], 140.00, 0x0\n", "0000:01:00.0").unwrap();
        assert_eq!((limit, reasons.len()), (None, 0));
        assert!(parse_nvidia_power(stdout, "0000:02:00.0").is_none());
    }

    #[test]
    fn laptop_on_battery_with_dynamic_boost_off() {
        let dir = tempfile::tempdir().unwrap();
        let sys = dir.path();
        write(sys.join("class/power_supply/ACAD/type"), "Mains\n");
        write(sys.join("class/power_supply/ACAD/online"), "0\n");
        write(sys.join("class/power_supply/BAT1/type"), "Battery\n");
        write(sys.join("firmware/acpi/platform_profile"), "balanced\n");
        write(sys.join("devices/system/cpu/cpu0/cpufreq/energy_performance_preference"), "balance_power\n");
        let exec = FakeExec::new()
            .on("nvidia-smi", "00000000:01:00.0, 60.00, 140.00, 0x0000000000000084\n")
            .on("systemctl", "inactive\n");
        let gpus = [
            gpu(Vendor::Intel, "0000:00:02.0", true, true, false),
            gpu(Vendor::Nvidia, "0000:01:00.0", false, false, true),
        ];
        let sample = sample(sys, &exec, &gpus);
        assert_eq!(
            sample,
            Sample {
                on_battery: true,
                platform_profile: Some("balanced".into()),
                cpu_preference: Some("balance_power".into()),
                gpu_power_limit: Some(60.0),
                gpu_power_limit_max: Some(140.0),
                gpu_throttle: vec!["power".into(), "power-brake".into()],
                nvidia_powerd: Some(false),
                through_other_gpu: true,
            }
        );
        // Deux relevés : rien en double, la limite la plus haute gardée.
        let mut context = GameContext::default();
        context.add(&sample);
        context.add(&Sample { on_battery: false, gpu_power_limit: Some(95.0), ..sample.clone() });
        assert_eq!(context.samples, 2);
        assert!(context.on_battery);
        assert_eq!(context.gpu_power_limit, Some(95.0));
        assert_eq!(context.platform_profiles, ["balanced"]);
        // Branché, PC fixe : jamais « sur batterie ».
        write(sys.join("class/power_supply/ACAD/online"), "1\n");
        assert!(!super::sample(sys, &exec, &gpus).on_battery);
        let desktop = tempfile::tempdir().unwrap();
        assert!(!super::sample(desktop.path(), &FakeExec::new(), &[]).on_battery);
    }

    #[test]
    fn context_lives_next_to_its_log() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("PioneerGame_2026-10-19_15-38-00.csv");
        fs::write(&log, "fps\n").unwrap();
        assert_eq!(current_log(dir.path()).as_deref(), Some(log.as_path()));
        let context = GameContext { samples: 3, on_battery: true, ..Default::default() };
        save(&log, &context);
        assert_eq!(file_for(&log), dir.path().join("PioneerGame_2026-10-19_15-38-00.context.json"));
        assert_eq!(load(&log), Some(context));
    }
}
