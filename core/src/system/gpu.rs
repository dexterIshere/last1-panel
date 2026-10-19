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
    /// Carte qui affichait le démarrage (la principale : le bureau y est
    /// dessiné).
    pub boot_vga: bool,
    /// Puce intégrée au processeur : Intel sur le bus du processeur, AMD que
    /// son pilote (KFD) décrit à la fois comme processeur et puce graphique.
    pub integrated: bool,
    /// La carte des jeux (choose_games) : celle où Last1 Panel les fait
    /// tourner, qu'il mesure et qu'il vérifie.
    pub games: bool,
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
                integrated: false,
                games: false,
                connectors,
            })
        })
        .collect();
    let apus = amd_apus(&paths.sys);
    for gpu in &mut gpus {
        gpu.integrated = match gpu.vendor {
            // Puce Intel intégrée : sur le bus du processeur (00:02.0) ; une
            // Arc est derrière un port, sur un autre bus.
            Vendor::Intel => gpu.pci_slot.split(':').nth(1) == Some("00"),
            Vendor::Amd => apus.contains(&gpu.pci_slot),
            Vendor::Nvidia => false,
            // Carte de machine virtuelle, de serveur : rien pour les jeux.
            Vendor::Other => true,
        };
    }
    choose_games(&mut gpus);
    gpus.sort_by_key(|gpu| !gpu.boot_vga);
    gpus
}

impl Gpu {
    /// Carte à part (pas une puce intégrée).
    pub fn discrete(&self) -> bool {
        !self.integrated
    }

    /// Écrans branchés sur elle.
    pub fn screens(&self) -> usize {
        self.connectors.iter().filter(|c| c.connected).count()
    }
}

/// La carte des jeux, pour tout PC : une carte à part plutôt qu'une puce
/// intégrée ; entre cartes à part, celle qui affiche le plus d'écrans, puis
/// NVIDIA, AMD, Intel, puis la principale. Sans carte à part, la puce
/// intégrée.
pub fn choose_games(gpus: &mut [Gpu]) {
    let rank = |gpu: &Gpu| {
        let vendor = match gpu.vendor {
            Vendor::Nvidia => 3,
            Vendor::Amd => 2,
            Vendor::Intel => 1,
            Vendor::Other => 0,
        };
        (gpu.discrete(), gpu.screens(), vendor, gpu.boot_vga)
    };
    for gpu in gpus.iter_mut() {
        gpu.games = false;
    }
    if let Some(best) = (0..gpus.len()).max_by_key(|&i| rank(&gpus[i])) {
        gpus[best].games = true;
    }
}

/// La carte des jeux.
pub fn games_gpu(gpus: &[Gpu]) -> Option<&Gpu> {
    gpus.iter().find(|gpu| gpu.games)
}

/// La carte des jeux, quand ce n'est pas elle qui dessine le bureau (portable
/// à deux puces, puce intégrée choisie au démarrage) : les jeux doivent lui
/// être envoyés, sinon ceux en OpenGL tournent sur l'autre.
pub fn offload(gpus: &[Gpu]) -> Option<&Gpu> {
    let games = games_gpu(gpus)?;
    (!games.boot_vga && gpus.iter().any(|gpu| gpu.boot_vga)).then_some(games)
}

/// Puces AMD intégrées (APU), par emplacement PCI : le pilote (KFD) les
/// décrit comme un nœud qui a à la fois des cœurs de processeur et des
/// unités graphiques ; une carte AMD à part n'a que les secondes.
fn amd_apus(sys: &Path) -> Vec<String> {
    let Ok(nodes) = fs::read_dir(sys.join("class/kfd/kfd/topology/nodes")) else { return Vec::new() };
    nodes
        .flatten()
        .filter_map(|node| {
            let properties = fs::read_to_string(node.path().join("properties")).ok()?;
            let value = |key: &str| {
                properties.lines().find_map(|line| {
                    let (name, value) = line.split_once(' ')?;
                    (name == key).then(|| value.trim().parse::<u64>().ok()).flatten()
                })
            };
            // location_id : bus << 8 | (appareil << 3 | fonction).
            let location = value("location_id")?;
            (value("cpu_cores_count")? > 0 && value("simd_count")? > 0).then(|| {
                format!(
                    "{:04x}:{:02x}:{:02x}.{:x}",
                    value("domain").unwrap_or(0),
                    location >> 8,
                    (location & 0xff) >> 3,
                    location & 7
                )
            })
        })
        .collect()
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

    #[test]
    fn integrated_amd_gpu_is_told_by_its_driver() {
        // Portable Ryzen : Radeon Vega intégrée (écran du portable) + GTX 1650.
        let root = fake_sys(&[
            Card {
                name: "card0",
                slot: "0000:05:00.0",
                vendor: "0x1002",
                device: "0x15d8",
                driver: "amdgpu",
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
                connectors: &[],
            },
        ]);
        // Nœud du pilote AMD avec cœurs et unités graphiques ; 5 << 8 = 1280.
        let node = root.path().join("class/kfd/kfd/topology/nodes/1");
        fs::create_dir_all(&node).unwrap();
        fs::write(node.join("properties"), "cpu_cores_count 8\nsimd_count 32\nlocation_id 1280\ndomain 0\n").unwrap();
        let gpus = detect(&paths(&root));
        let roles: Vec<(&str, bool, bool)> = gpus.iter().map(|g| (g.card.as_str(), g.integrated, g.games)).collect();
        assert_eq!(roles, [("card0", true, false), ("card1", false, true)]);
        assert_eq!(offload(&gpus).map(|g| g.card.as_str()), Some("card1"));
    }

    /// Une carte, sans passer par /sys.
    fn card(vendor: Vendor, slot: &str, boot_vga: bool, integrated: bool, screens: usize) -> Gpu {
        Gpu {
            card: format!("card-{slot}"),
            pci_slot: slot.into(),
            vendor,
            vendor_id: 0,
            device_id: 0,
            name: None,
            driver: None,
            driver_version: None,
            boot_vga,
            integrated,
            games: false,
            connectors: (0..screens).map(|i| Connector { name: format!("DP-{i}"), connected: true }).collect(),
        }
    }

    #[test]
    fn games_gpu_for_every_kind_of_pc() {
        use Vendor::*;
        // (cartes, carte des jeux, envoyée là depuis le bureau)
        let cases: Vec<(&str, Vec<Gpu>, &str, bool)> = vec![
            ("NVIDIA seule", vec![card(Nvidia, "0000:01:00.0", true, false, 1)], "0000:01:00.0", false),
            ("AMD seule", vec![card(Amd, "0000:03:00.0", true, false, 2)], "0000:03:00.0", false),
            ("puce Intel seule", vec![card(Intel, "0000:00:02.0", true, true, 1)], "0000:00:02.0", false),
            (
                "portable Intel + NVIDIA",
                vec![card(Intel, "0000:00:02.0", true, true, 1), card(Nvidia, "0000:01:00.0", false, false, 0)],
                "0000:01:00.0",
                true,
            ),
            (
                "portable Ryzen + NVIDIA",
                vec![card(Amd, "0000:05:00.0", true, true, 1), card(Nvidia, "0000:01:00.0", false, false, 0)],
                "0000:01:00.0",
                true,
            ),
            (
                "portable Ryzen + Radeon",
                vec![card(Amd, "0000:06:00.0", true, true, 1), card(Amd, "0000:03:00.0", false, false, 0)],
                "0000:03:00.0",
                true,
            ),
            (
                "portable en mode carte seule (MUX)",
                vec![card(Intel, "0000:00:02.0", false, true, 0), card(Nvidia, "0000:01:00.0", true, false, 1)],
                "0000:01:00.0",
                false,
            ),
            (
                "PC fixe, puce Intel active, écrans sur la Radeon",
                vec![card(Intel, "0000:00:02.0", false, true, 0), card(Amd, "0000:03:00.0", true, false, 2)],
                "0000:03:00.0",
                false,
            ),
            (
                "PC fixe, démarrage sur la puce Intel, écran sur la NVIDIA",
                vec![card(Intel, "0000:00:02.0", true, true, 0), card(Nvidia, "0000:01:00.0", false, false, 1)],
                "0000:01:00.0",
                true,
            ),
            (
                "Intel Arc + puce Intel",
                vec![card(Intel, "0000:00:02.0", true, true, 1), card(Intel, "0000:03:00.0", false, false, 0)],
                "0000:03:00.0",
                true,
            ),
            (
                "deux cartes : NVIDIA aux écrans, Radeon sans",
                vec![card(Nvidia, "0000:01:00.0", true, false, 2), card(Amd, "0000:04:00.0", false, false, 0)],
                "0000:01:00.0",
                false,
            ),
            (
                "trois : puce Intel, NVIDIA, Radeon aux écrans",
                vec![
                    card(Intel, "0000:00:02.0", false, true, 0),
                    card(Nvidia, "0000:01:00.0", false, false, 0),
                    card(Amd, "0000:04:00.0", true, false, 1),
                ],
                "0000:04:00.0",
                false,
            ),
            ("machine virtuelle", vec![card(Other, "0000:00:01.0", true, true, 1)], "0000:00:01.0", false),
        ];
        for (name, mut gpus, games, sent) in cases {
            choose_games(&mut gpus);
            assert_eq!(games_gpu(&gpus).map(|g| g.pci_slot.as_str()), Some(games), "{name}");
            assert_eq!(gpus.iter().filter(|g| g.games).count(), 1, "{name}");
            assert_eq!(offload(&gpus).is_some(), sent, "{name}");
        }
        assert!(games_gpu(&[]).is_none());
    }
}
