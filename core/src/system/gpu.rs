//! Cartes graphiques du PC, lues dans /sys (sans outil ni droits).

use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Vendor {
    Nvidia,
    Amd,
    Intel,
    Other,
}

impl Vendor {
    fn from_pci(id: u32) -> Self {
        match id {
            0x10de => Self::Nvidia,
            0x1002 => Self::Amd,
            0x8086 => Self::Intel,
            _ => Self::Other,
        }
    }
}

/// Une carte graphique (un nœud /sys/class/drm/cardN).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Gpu {
    /// Nom DRM : « card0 ».
    pub card: String,
    /// Adresse PCI : « 0000:01:00.0 ».
    pub pci_slot: String,
    pub vendor: Vendor,
    pub vendor_id: u32,
    pub device_id: u32,
    /// Nom commercial (base pci.ids de hwdata), s'il est connu.
    pub name: Option<String>,
    /// Pilote du noyau : nvidia, amdgpu, i915, xe, nouveau…
    pub driver: Option<String>,
    /// Version du pilote NVIDIA (module nvidia), pour nvibrant.
    pub driver_version: Option<String>,
    /// Carte qui affichait le démarrage (la principale).
    pub boot_vga: bool,
    /// Sorties de la carte (noms des connecteurs : DP-1, HDMI-A-1, eDP-1).
    pub connectors: Vec<Connector>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Connector {
    /// Nom vu par les compositeurs Wayland : « DP-1 ».
    pub name: String,
    pub connected: bool,
}

/// Où lire : `/sys` et la base des noms PCI, changés par les tests.
#[derive(Debug, Clone)]
pub struct SysPaths {
    pub sys: PathBuf,
    pub pci_ids: Vec<PathBuf>,
}

impl Default for SysPaths {
    fn default() -> Self {
        Self {
            sys: PathBuf::from("/sys"),
            pci_ids: vec!["/usr/share/hwdata/pci.ids".into(), "/usr/share/misc/pci.ids".into()],
        }
    }
}

/// Toutes les cartes graphiques, la principale en premier.
pub fn detect(paths: &SysPaths) -> Vec<Gpu> {
    let drm = paths.sys.join("class/drm");
    let Ok(entries) = fs::read_dir(&drm) else { return Vec::new() };
    let mut names: Vec<String> = entries.filter_map(|entry| entry.ok()?.file_name().into_string().ok()).collect();
    names.sort();
    let cards: Vec<&String> = names.iter().filter(|name| is_card(name)).collect();
    let pci_ids = paths.pci_ids.iter().find_map(|path| fs::read_to_string(path).ok());

    let mut gpus: Vec<Gpu> = cards
        .into_iter()
        .filter_map(|card| {
            let device = drm.join(card).join("device");
            let vendor_id = read_hex(&device.join("vendor"))?;
            let device_id = read_hex(&device.join("device"))?;
            let vendor = Vendor::from_pci(vendor_id);
            let driver = link_name(&device.join("driver"));
            let driver_version = match vendor {
                Vendor::Nvidia => read_trimmed(&paths.sys.join("module/nvidia/version")),
                _ => None,
            };
            let connectors = names
                .iter()
                .filter_map(|name| {
                    let connector = name.strip_prefix(card.as_str())?.strip_prefix('-')?;
                    let status = read_trimmed(&drm.join(name).join("status"));
                    Some(Connector { name: connector.to_string(), connected: status.as_deref() == Some("connected") })
                })
                .collect();
            Some(Gpu {
                card: card.clone(),
                pci_slot: fs::canonicalize(&device)
                    .ok()
                    .and_then(|path| path.file_name()?.to_str().map(str::to_string))
                    .unwrap_or_default(),
                vendor,
                vendor_id,
                device_id,
                name: pci_ids.as_deref().and_then(|ids| pci_name(ids, vendor_id, device_id)),
                driver,
                driver_version,
                boot_vga: read_trimmed(&device.join("boot_vga")).as_deref() == Some("1"),
                connectors,
            })
        })
        .collect();
    gpus.sort_by_key(|gpu| !gpu.boot_vga);
    gpus
}

/// La carte qui porte la sortie `connector` (DP-1…).
pub fn gpu_for_connector<'a>(gpus: &'a [Gpu], connector: &str) -> Option<&'a Gpu> {
    gpus.iter().find(|gpu| gpu.connectors.iter().any(|c| c.name == connector))
}

fn is_card(name: &str) -> bool {
    name.strip_prefix("card").is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()))
}

fn read_trimmed(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

fn read_hex(path: &Path) -> Option<u32> {
    let text = read_trimmed(path)?;
    u32::from_str_radix(text.trim_start_matches("0x"), 16).ok()
}

fn link_name(path: &Path) -> Option<String> {
    fs::read_link(path).ok()?.file_name()?.to_str().map(str::to_string)
}

/// Nom d'un périphérique dans pci.ids : lignes « vvvv  Fabricant » puis
/// « \tdddd  Modèle ». Le nom entre crochets, quand il existe, est le nom
/// commercial (« GA104 [GeForce RTX 3070] » → « GeForce RTX 3070 »).
fn pci_name(ids: &str, vendor_id: u32, device_id: u32) -> Option<String> {
    let vendor = format!("{vendor_id:04x}");
    let device = format!("{device_id:04x}");
    let mut in_vendor = false;
    for line in ids.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if !line.starts_with('\t') {
            in_vendor = line.starts_with(&vendor) && line[vendor.len()..].starts_with("  ");
            continue;
        }
        if in_vendor && !line.starts_with("\t\t") {
            let entry = &line[1..];
            if entry.starts_with(&device) && entry[device.len()..].starts_with("  ") {
                let name = entry[device.len()..].trim();
                let marketing = name.rsplit_once('[').and_then(|(_, rest)| rest.strip_suffix(']'));
                return Some(marketing.unwrap_or(name).to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    const PCI_IDS: &str = "\
# extrait de pci.ids
10de  NVIDIA Corporation
\t2484  GA104 [GeForce RTX 3070]
\t\t1458 4036  carte d'un fabricant
\t2786  AD104 [GeForce RTX 4070]
1002  Advanced Micro Devices, Inc. [AMD/ATI]
\t744c  Navi 31 [Radeon RX 7900 XT/7900 XTX/7900 GRE/7900M]
8086  Intel Corporation
\ta780  Raptor Lake-S GT1 [UHD Graphics 770]
";

    struct Card<'a> {
        name: &'a str,
        slot: &'a str,
        vendor: &'a str,
        device: &'a str,
        driver: &'a str,
        boot_vga: bool,
        connectors: &'a [(&'a str, &'a str)],
    }

    /// Un faux /sys : /sys/class/drm/cardN → ../../devices/pci…/<slot>/drm/cardN.
    fn fake_sys(cards: &[Card]) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        let sys = root.path();
        fs::create_dir_all(sys.join("class/drm")).unwrap();
        fs::create_dir_all(sys.join("bus/pci/drivers")).unwrap();
        for card in cards {
            let device = sys.join("devices/pci0000:00").join(card.slot);
            fs::create_dir_all(device.join("drm").join(card.name)).unwrap();
            fs::write(device.join("vendor"), format!("{}\n", card.vendor)).unwrap();
            fs::write(device.join("device"), format!("{}\n", card.device)).unwrap();
            fs::write(device.join("boot_vga"), if card.boot_vga { "1\n" } else { "0\n" }).unwrap();
            let driver = sys.join("bus/pci/drivers").join(card.driver);
            fs::create_dir_all(&driver).unwrap();
            symlink(&driver, device.join("driver")).unwrap();
            let card_dir = device.join("drm").join(card.name);
            fs::create_dir_all(&card_dir).unwrap();
            symlink(&device, card_dir.join("device")).unwrap();
            symlink(&card_dir, sys.join("class/drm").join(card.name)).unwrap();
            for (connector, status) in card.connectors {
                let name = format!("{}-{connector}", card.name);
                let dir = card_dir.join(&name);
                fs::create_dir_all(&dir).unwrap();
                fs::write(dir.join("status"), format!("{status}\n")).unwrap();
                symlink(&dir, sys.join("class/drm").join(&name)).unwrap();
            }
        }
        fs::create_dir_all(sys.join("class/drm/renderD128")).unwrap();
        fs::write(sys.join("pci.ids"), PCI_IDS).unwrap();
        root
    }

    fn paths(root: &tempfile::TempDir) -> SysPaths {
        SysPaths { sys: root.path().to_path_buf(), pci_ids: vec![root.path().join("pci.ids")] }
    }

    #[test]
    fn desktop_with_nvidia_card() {
        let root = fake_sys(&[Card {
            name: "card1",
            slot: "0000:01:00.0",
            vendor: "0x10de",
            device: "0x2484",
            driver: "nvidia",
            boot_vga: true,
            connectors: &[("DP-1", "connected"), ("DP-2", "disconnected"), ("HDMI-A-1", "connected")],
        }]);
        fs::create_dir_all(root.path().join("module/nvidia")).unwrap();
        fs::write(root.path().join("module/nvidia/version"), "595.58.03\n").unwrap();

        let gpus = detect(&paths(&root));
        assert_eq!(gpus.len(), 1);
        let gpu = &gpus[0];
        assert_eq!(gpu.card, "card1");
        assert_eq!(gpu.pci_slot, "0000:01:00.0");
        assert_eq!(gpu.vendor, Vendor::Nvidia);
        assert_eq!(gpu.name.as_deref(), Some("GeForce RTX 3070"));
        assert_eq!(gpu.driver.as_deref(), Some("nvidia"));
        assert_eq!(gpu.driver_version.as_deref(), Some("595.58.03"));
        assert_eq!(
            gpu.connectors,
            vec![
                Connector { name: "DP-1".into(), connected: true },
                Connector { name: "DP-2".into(), connected: false },
                Connector { name: "HDMI-A-1".into(), connected: true },
            ]
        );
    }

    #[test]
    fn laptop_screen_on_integrated_gpu() {
        let root = fake_sys(&[
            Card {
                name: "card0",
                slot: "0000:00:02.0",
                vendor: "0x8086",
                device: "0xa780",
                driver: "i915",
                boot_vga: true,
                connectors: &[("eDP-1", "connected")],
            },
            Card {
                name: "card1",
                slot: "0000:01:00.0",
                vendor: "0x10de",
                device: "0x2786",
                driver: "nvidia",
                boot_vga: false,
                connectors: &[("HDMI-A-1", "disconnected")],
            },
        ]);
        let gpus = detect(&paths(&root));
        assert_eq!(gpus[0].vendor, Vendor::Intel);
        assert_eq!(gpus[0].name.as_deref(), Some("UHD Graphics 770"));
        assert_eq!(gpus[1].name.as_deref(), Some("GeForce RTX 4070"));
        // L'écran du portable n'est pas branché sur la carte NVIDIA.
        assert_eq!(gpu_for_connector(&gpus, "eDP-1").unwrap().vendor, Vendor::Intel);
        assert_eq!(gpu_for_connector(&gpus, "HDMI-A-1").unwrap().vendor, Vendor::Nvidia);
        assert!(gpu_for_connector(&gpus, "DP-9").is_none());
        // Pas de version de pilote NVIDIA sans module chargé.
        assert_eq!(gpus[1].driver_version, None);
    }

    #[test]
    fn amd_name_keeps_marketing_part() {
        assert_eq!(pci_name(PCI_IDS, 0x1002, 0x744c).as_deref(), Some("Radeon RX 7900 XT/7900 XTX/7900 GRE/7900M"));
        assert_eq!(pci_name(PCI_IDS, 0x10de, 0x9999), None);
        // Une sous-ligne (sous-système) n'est jamais prise pour un modèle.
        assert_eq!(pci_name(PCI_IDS, 0x10de, 0x1458), None);
    }

    #[test]
    fn no_drm_means_no_gpu() {
        let root = tempfile::tempdir().unwrap();
        assert!(detect(&SysPaths { sys: root.path().into(), pci_ids: vec![] }).is_empty());
    }
}
