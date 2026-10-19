//! Vibrance numérique des cartes NVIDIA, par nvibrant (MIT, Gabriel
//! Tremeschin), qui la règle dans le pilote (interface NVKMS) : elle
//! s'applique à tout l'affichage, jeux en plein écran compris, sans coût.
//!
//! Les binaires de nvibrant (un par version du pilote, compilés par son
//! auteur et publiés sur PyPI) sont livrés dans le paquet de Last1 Panel,
//! dans /usr/lib/last1-panel/nvibrant : on prend, comme son lanceur Python,
//! la version la plus récente qui ne dépasse pas celle du pilote. À défaut,
//! la commande `nvibrant` installée à part.
//!
//! nvibrant prend une valeur par prise physique de la carte, dans l'ordre du
//! pilote, et ne donne pas le nom des sorties (DP-1…). Les écrans branchés
//! qu'il voit sont associés, type de prise par type, à ceux que le noyau voit
//! branchés sur la carte NVIDIA, dans l'ordre des connecteurs DRM (créés dans
//! le même ordre par nvidia-drm) ; les prises vides ne comptent pas (le pilote
//! en a parfois que le noyau ne montre pas). S'ils ne voient pas autant
//! d'écrans, rien n'est réglé.

use crate::error::{CoreError, CoreResult};
use crate::exec::Exec;
use crate::i18n::t;
use crate::system::gpu::{Gpu, Vendor};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const PROGRAM: &str = "nvibrant";
/// Binaires de nvibrant livrés avec Last1 Panel (variable pour les essais).
const BUNDLED_DIR: &str = "/usr/lib/last1-panel/nvibrant";

/// Dossiers des binaires nvibrant, dans l'ordre : celui du paquet, celui posé
/// par l'AppImage dans le dossier de l'utilisateur, celui de l'AppImage lancée.
fn bundled_dirs() -> Vec<PathBuf> {
    if let Some(dir) = std::env::var_os("LAST1_PANEL_NVIBRANT_DIR") {
        return vec![PathBuf::from(dir)];
    }
    let mut dirs = vec![PathBuf::from(BUNDLED_DIR), crate::paths::user_lib_dir().join("nvibrant")];
    if let Some(appdir) = std::env::var_os("APPDIR") {
        dirs.push(PathBuf::from(appdir).join(BUNDLED_DIR.trim_start_matches('/')));
    }
    dirs
}

/// « 595.58.03 » → (595, 58, 3) pour comparer les versions du pilote.
fn version_key(text: &str) -> Option<Vec<u32>> {
    let parts: Option<Vec<u32>> = text.trim().split('.').map(|part| part.parse().ok()).collect();
    parts.filter(|parts| !parts.is_empty())
}

/// Le binaire nvibrant pour cette version du pilote : le plus récent qui ne
/// la dépasse pas (les structures du pilote ne changent qu'à certaines
/// versions).
pub fn bundled_binary(dir: &Path, driver: &str) -> Option<PathBuf> {
    let driver = version_key(driver)?;
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let version = version_key(name.strip_prefix("nvibrant-")?)?;
            (version <= driver).then_some((version, entry.path()))
        })
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, path)| path)
}

/// Ce qui règle la vibrance pour ce pilote : binaire livré, sinon `nvibrant`.
pub fn program(exec: &dyn Exec, driver: Option<&str>) -> Option<String> {
    driver
        .and_then(|version| bundled_dirs().iter().find_map(|dir| bundled_binary(dir, version)))
        .map(|path| path.to_string_lossy().into_owned())
        .or_else(|| exec.which(PROGRAM).map(|_| PROGRAM.to_string()))
}

/// Vibrance du panneau NVIDIA (0 à 100 %, 50 : inchangée) → valeur NVKMS
/// (-1024 à 1023, 0 : inchangée ; -1024 : noir et blanc).
pub fn to_nvkms(percent: f64) -> i32 {
    let percent = if percent.is_finite() { percent.clamp(0.0, 100.0) } else { 50.0 };
    (((percent - 50.0) / 50.0) * 1024.0).round().clamp(-1024.0, 1023.0) as i32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PortKind {
    Dp,
    Hdmi,
    DviI,
    DviD,
    Vga,
    Lvds,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Port {
    pub kind: PortKind,
    /// Écran branché (nvibrant a pu régler la prise, ou a échoué dessus).
    pub connected: bool,
    /// Branché mais refusé par le pilote : écran éteint ou pas encore allumé
    /// par le bureau (juste après la connexion).
    pub failed: bool,
}

/// Prises listées par nvibrant, dans l'ordre de ses arguments.
pub fn parse(stdout: &str) -> Vec<Port> {
    let mut ports = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("• (") {
            let kind = rest.split_once(", ").map(|(_, rest)| rest.get(..4).unwrap_or("").trim()).unwrap_or("");
            let kind = match kind {
                "DP" => PortKind::Dp,
                "HDMI" => PortKind::Hdmi,
                "DVII" => PortKind::DviI,
                "DVID" => PortKind::DviD,
                "VGA" => PortKind::Vga,
                "LVDS" => PortKind::Lvds,
                _ => PortKind::Other,
            };
            ports.push(Port { kind, connected: !line.ends_with("None"), failed: line.ends_with("Failed") });
        } else if line == "QueryConnector ioctl failed" || line == "QueryDpy ioctl failed" {
            // Prise comptée par nvibrant (elle prend un argument), mais illisible.
            ports.push(Port { kind: PortKind::Other, connected: false, failed: false });
        }
    }
    ports
}

/// Préfixes DRM possibles pour un type de prise NVKMS (DP couvre aussi eDP).
fn drm_prefixes(kind: PortKind) -> &'static [&'static str] {
    match kind {
        PortKind::Dp => &["eDP", "DP"],
        PortKind::Hdmi => &["HDMI-A"],
        PortKind::DviI => &["DVI-I"],
        PortKind::DviD => &["DVI-D"],
        PortKind::Vga => &["VGA"],
        PortKind::Lvds => &["LVDS"],
        PortKind::Other => &[],
    }
}

/// Numéro DRM d'un connecteur (« DP-3 » → 3), pour trier dans l'ordre de création.
fn connector_number(name: &str) -> u32 {
    name.rsplit('-').next().and_then(|n| n.parse().ok()).unwrap_or(u32::MAX)
}

/// Type de prise d'un connecteur DRM (« eDP-1 », « HDMI-A-2 »).
fn drm_kind(name: &str) -> Option<PortKind> {
    let (prefix, _) = name.rsplit_once('-')?;
    [PortKind::Dp, PortKind::Hdmi, PortKind::DviI, PortKind::DviD, PortKind::Vga, PortKind::Lvds]
        .into_iter()
        .find(|kind| drm_prefixes(*kind).contains(&prefix))
}

/// Nom DRM de chaque prise (None : rien de branché, ou prise illisible).
/// Seuls les écrans branchés sont associés, type par type, dans l'ordre du
/// pilote : le pilote compte parfois des prises que le noyau ne montre pas
/// (écran interne d'un portable relié à l'autre puce), qui décaleraient tout.
/// Erreur si les deux ne voient pas autant d'écrans d'un type.
pub fn map_ports(ports: &[Port], gpu: &Gpu) -> CoreResult<Vec<Option<String>>> {
    let mut names = vec![None; ports.len()];
    let mut kinds: Vec<PortKind> = ports
        .iter()
        .filter(|port| port.connected)
        .map(|port| port.kind)
        .chain(gpu.connectors.iter().filter(|c| c.connected).filter_map(|c| drm_kind(&c.name)))
        .collect();
    kinds.sort();
    kinds.dedup();
    for kind in kinds {
        // eDP avant DP (même type pour le pilote), puis l'ordre de création.
        let screens: Vec<String> = drm_prefixes(kind)
            .iter()
            .flat_map(|prefix| {
                let mut list: Vec<&str> = gpu
                    .connectors
                    .iter()
                    .filter(|c| c.connected && c.name.rsplit_once('-').is_some_and(|(p, _)| p == *prefix))
                    .map(|c| c.name.as_str())
                    .collect();
                list.sort_by_key(|name| connector_number(name));
                list
            })
            .map(str::to_string)
            .collect();
        let plugged: Vec<usize> =
            ports.iter().enumerate().filter(|(_, port)| port.kind == kind && port.connected).map(|(i, _)| i).collect();
        if plugged.len() != screens.len() {
            return Err(CoreError::message(format!(
                "{} ({})",
                t(
                    "Prises de la carte NVIDIA non reconnues : vibrance désactivée pour ne pas régler le mauvais écran",
                    "NVIDIA card ports not recognised: vibrance disabled so the wrong screen is never changed",
                ),
                if screens.is_empty() { format!("{kind:?}") } else { screens.join(", ") }
            )));
        }
        for (index, name) in plugged.into_iter().zip(screens) {
            names[index] = Some(name);
        }
    }
    Ok(names)
}

/// Règle la vibrance des sorties de `gpu` (pourcentages par nom de sortie ;
/// les autres sorties reviennent à 50 %). Erreur si une des sorties `targets`
/// refuse (écran éteint, ou pas encore allumé par le bureau). Renvoie les
/// sorties réglées.
pub fn apply(
    exec: &dyn Exec,
    gpu: &Gpu,
    gpu_index: usize,
    values: &BTreeMap<String, f64>,
    targets: &[String],
) -> CoreResult<Vec<String>> {
    if gpu.vendor != Vendor::Nvidia {
        return Err(CoreError::message(t(
            "La vibrance numérique ne se règle que sur les sorties d'une carte NVIDIA.",
            "Digital vibrance can only be set on the outputs of an NVIDIA card.",
        )));
    }
    let program = program(exec, gpu.driver_version.as_deref()).ok_or_else(|| CoreError::MissingTool(PROGRAM.into()))?;
    let run = |values: Vec<String>| -> CoreResult<Vec<Port>> {
        // nvibrant choisit la carte et le réglage par ses variables d'environnement.
        let mut args = vec![format!("NVIDIA_GPU={gpu_index}"), "ATTRIBUTE=vibrance".into(), program.clone()];
        args.extend(values);
        let output = exec.run("env", &args)?;
        if !output.success() {
            let detail = output.stdout.lines().chain(output.stderr.lines()).last().unwrap_or_default().to_string();
            return Err(CoreError::ToolFailed { program: PROGRAM.into(), detail: explain(&output.stdout, &detail) });
        }
        Ok(parse(&output.stdout))
    };
    // Premier passage sans argument : toutes les prises à la valeur par
    // défaut (celle du démarrage), et la liste des prises.
    let ports = run(Vec::new())?;
    let names = map_ports(&ports, gpu)?;
    if values.values().all(|v| to_nvkms(*v) == 0) {
        return Ok(Vec::new());
    }
    let args: Vec<String> = names
        .iter()
        .map(|name| name.as_ref().and_then(|n| values.get(n)).map(|v| to_nvkms(*v)).unwrap_or(0).to_string())
        .collect();
    let second = run(args)?;
    let same = second.len() == ports.len()
        && second.iter().zip(&ports).all(|(a, b)| a.kind == b.kind && a.connected == b.connected);
    if !same {
        return Err(CoreError::message(t(
            "Les écrans ont changé pendant le réglage : réessayez.",
            "Screens changed while applying: try again.",
        )));
    }
    let refused: Vec<&String> = names
        .iter()
        .zip(&second)
        .filter_map(|(name, port)| name.as_ref().filter(|n| port.failed && targets.contains(n)))
        .collect();
    if let Some(name) = refused.first() {
        return Err(CoreError::message(format!(
            "{name} : {}",
            t("écran pas encore prêt (éteint ou en cours d'allumage)", "screen not ready yet (off or starting)")
        )));
    }
    Ok(names.into_iter().flatten().filter(|name| values.contains_key(name)).collect())
}

fn explain(stdout: &str, detail: &str) -> String {
    if stdout.contains("/dev/nvidia-modeset") {
        t(
            "le pilote NVIDIA n'est pas en mode « modeset » (paramètre nvidia_drm.modeset=1)",
            "the NVIDIA driver is not in modeset mode (nvidia_drm.modeset=1 parameter)",
        )
    } else if stdout.contains("Driver version mismatch") {
        t("pilote NVIDIA mis à jour : redémarrez le PC", "NVIDIA driver updated: restart the PC")
    } else {
        detail.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::{FakeExec, Output};
    use crate::system::gpu::Connector;

    const OUTPUT: &str = "Driver version: (595.58.03)\n\nDisplay 0:\n\
        • (0, HDMI) • Set vibrance (    0) • Success\n\
        • (1, DP  ) • Set vibrance (    0) • Success\n\
        • (2, DP  ) • Set vibrance (    0) • None\n\
        • (3, DP  ) • Set vibrance (    0) • None\n";

    fn gpu(connectors: &[(&str, bool)]) -> Gpu {
        Gpu {
            card: "card1".into(),
            pci_slot: "0000:01:00.0".into(),
            vendor: Vendor::Nvidia,
            vendor_id: 0x10de,
            device_id: 0x2484,
            name: Some("GeForce RTX 3070".into()),
            driver: Some("nvidia".into()),
            driver_version: Some("595.58.03".into()),
            boot_vga: true,
            integrated: false,
            games: true,
            connectors: connectors.iter().map(|(n, c)| Connector { name: n.to_string(), connected: *c }).collect(),
        }
    }

    #[test]
    fn picks_the_bundled_binary_for_the_driver() {
        let dir = tempfile::tempdir().unwrap();
        for version in ["575.51.02", "580.65.06", "595.44.02", "615.71.09"] {
            std::fs::write(dir.path().join(format!("nvibrant-{version}")), "").unwrap();
        }
        let pick = |driver: &str| bundled_binary(dir.path(), driver).map(|p| p.file_name().unwrap().to_owned());
        assert_eq!(pick("595.58.03").unwrap(), "nvibrant-595.44.02");
        assert_eq!(pick("580.65.06").unwrap(), "nvibrant-580.65.06");
        // 610 n'est pas livré : celui de 595 (le plus récent en dessous).
        assert_eq!(pick("610.43.03").unwrap(), "nvibrant-595.44.02");
        assert_eq!(pick("620.10").unwrap(), "nvibrant-615.71.09");
        assert_eq!(pick("515.43.04"), None, "pilote plus ancien que tous les binaires");
        assert_eq!(pick("pas une version"), None);
        // 595.100 est plus récent que 595.99 (comparaison nombre par nombre).
        std::fs::write(dir.path().join("nvibrant-595.100.01"), "").unwrap();
        assert_eq!(pick("595.101").unwrap(), "nvibrant-595.100.01");
    }

    #[test]
    fn percent_maps_to_nvkms() {
        assert_eq!(to_nvkms(50.0), 0);
        assert_eq!(to_nvkms(100.0), 1023);
        assert_eq!(to_nvkms(0.0), -1024);
        assert_eq!(to_nvkms(75.0), 512);
        assert_eq!(to_nvkms(f64::NAN), 0);
    }

    #[test]
    fn reads_ports_in_argument_order() {
        let ports = parse(OUTPUT);
        assert_eq!(ports.len(), 4);
        assert_eq!(ports[0], Port { kind: PortKind::Hdmi, connected: true, failed: false });
        assert_eq!(ports[1], Port { kind: PortKind::Dp, connected: true, failed: false });
        assert!(!ports[2].connected);
        // Une prise illisible prend quand même un argument.
        let odd = parse("Display 0:\nQueryConnector ioctl failed\n• (1, HDMI) • Set vibrance (    0) • Failed\n");
        assert_eq!(
            odd,
            [
                Port { kind: PortKind::Other, connected: false, failed: false },
                Port { kind: PortKind::Hdmi, connected: true, failed: true }
            ]
        );
    }

    #[test]
    fn maps_ports_to_drm_names_by_type_and_order() {
        // Numérotation DRM partagée entre cartes : ici les DP de la carte NVIDIA
        // commencent à 3 (la puce intégrée a DP-1 et DP-2).
        let card = gpu(&[("DP-3", true), ("DP-4", false), ("DP-5", false), ("HDMI-A-2", true)]);
        let names = map_ports(&parse(OUTPUT), &card).unwrap();
        assert_eq!(names, [Some("HDMI-A-2".into()), Some("DP-3".into()), None, None]);
    }

    #[test]
    fn ports_the_kernel_does_not_show_are_skipped() {
        // Portable : le pilote compte d'abord la prise de l'écran interne
        // (relié à la puce intégrée, absente du noyau côté NVIDIA).
        let laptop = "Driver version: (595.58.03)\n\nDisplay 0:\n\
            • (0, DP  ) • Set vibrance (    0) • None\n\
            • (1, DP  ) • Set vibrance (    0) • Success\n\
            • (2, HDMI) • Set vibrance (    0) • None\n";
        let card = gpu(&[("DP-1", true), ("HDMI-A-1", false)]);
        assert_eq!(map_ports(&parse(laptop), &card).unwrap(), [None, Some("DP-1".into()), None]);
    }

    #[test]
    fn contradiction_disables_vibrance() {
        // Le noyau voit deux écrans DP, nvibrant un seul.
        let card = gpu(&[("DP-1", true), ("DP-2", true), ("DP-3", false), ("HDMI-A-1", true)]);
        assert!(map_ports(&parse(OUTPUT), &card).is_err());
        // Un écran HDMI que nvibrant ne voit pas.
        let card = gpu(&[("DP-1", true), ("HDMI-A-1", true), ("HDMI-A-2", true)]);
        assert!(map_ports(&parse(OUTPUT), &card).is_err());
    }

    #[test]
    fn applies_values_to_the_right_ports() {
        let fake = FakeExec::new().on("env", OUTPUT);
        let card = gpu(&[("DP-1", true), ("DP-2", false), ("DP-3", false), ("HDMI-A-1", true)]);
        let values = BTreeMap::from([("DP-1".to_string(), 80.0), ("HDMI-A-1".to_string(), 50.0)]);
        let set = apply(&fake, &card, 0, &values, &["DP-1".to_string()]).unwrap();
        assert_eq!(set, ["HDMI-A-1", "DP-1"]);
        let calls = fake.calls();
        assert_eq!(calls[0].1, ["NVIDIA_GPU=0", "ATTRIBUTE=vibrance", "nvibrant"]);
        assert_eq!(calls[1].1, ["NVIDIA_GPU=0", "ATTRIBUTE=vibrance", "nvibrant", "0", "614", "0", "0"]);
    }

    #[test]
    fn neutral_values_need_a_single_run() {
        let fake = FakeExec::new().on("env", OUTPUT);
        let card = gpu(&[("DP-1", true), ("DP-2", false), ("DP-3", false), ("HDMI-A-1", true)]);
        apply(&fake, &card, 0, &BTreeMap::from([("DP-1".to_string(), 50.0)]), &[]).unwrap();
        assert_eq!(fake.calls().len(), 1);
    }

    #[test]
    fn screen_not_ready_is_an_error() {
        let refused = OUTPUT
            .replace("• (1, DP  ) • Set vibrance (    0) • Success", "• (1, DP  ) • Set vibrance (  614) • Failed");
        let fake = FakeExec::new().on("env", &refused);
        let card = gpu(&[("DP-1", true), ("DP-2", false), ("DP-3", false), ("HDMI-A-1", true)]);
        let values = BTreeMap::from([("DP-1".to_string(), 80.0), ("HDMI-A-1".to_string(), 60.0)]);
        let error = apply(&fake, &card, 0, &values, &["DP-1".to_string()]).unwrap_err().to_string();
        assert!(error.starts_with("DP-1"), "{error}");
        // Un autre écran éteint (DP-1 refusé) ne bloque pas le réglage de HDMI-A-1.
        assert!(apply(&fake, &card, 0, &values, &["HDMI-A-1".to_string()]).is_ok());
    }

    #[test]
    fn driver_problems_are_explained() {
        let fake = FakeExec::new().on_args(
            "env",
            &["NVIDIA_GPU=0", "ATTRIBUTE=vibrance", "nvibrant"],
            Output {
                status: 1,
                stdout: "Driver version: (595.58.03)\nFailed to open /dev/nvidia-modeset device file for ioctl calls\n"
                    .into(),
                stderr: String::new(),
            },
        );
        let card = gpu(&[("DP-1", true)]);
        let error = apply(&fake, &card, 0, &BTreeMap::new(), &[]).unwrap_err().to_string();
        assert!(error.contains("nvidia_drm.modeset=1"), "{error}");
        let amd = Gpu { vendor: Vendor::Amd, ..card };
        assert!(apply(&fake, &amd, 0, &BTreeMap::new(), &[]).is_err());
        let missing = FakeExec::new().without(PROGRAM);
        assert!(matches!(apply(&missing, &gpu(&[]), 0, &BTreeMap::new(), &[]), Err(CoreError::MissingTool(_))));
    }
}
