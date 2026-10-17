//! Vibrance numérique des cartes NVIDIA, par nvibrant (paquet AUR
//! `nvibrant-bin`), qui la règle dans le pilote (interface NVKMS) : elle
//! s'applique à tout l'affichage, jeux en plein écran compris, sans coût.
//!
//! nvibrant prend une valeur par prise physique de la carte, dans l'ordre du
//! pilote, et ne donne pas le nom des sorties (DP-1…). La correspondance est
//! faite avec l'ordre des connecteurs DRM de la carte NVIDIA (créés dans le
//! même ordre par nvidia-drm), puis vérifiée par l'état branché de chaque
//! prise : au moindre doute, rien n'est réglé.

use crate::error::{CoreError, CoreResult};
use crate::exec::Exec;
use crate::i18n::t;
use crate::system::gpu::{Gpu, Vendor};
use std::collections::BTreeMap;

const PROGRAM: &str = "nvibrant";

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

/// Nom DRM de chaque prise (None : prise non reconnue). Erreur si la
/// correspondance contredit l'état branché vu par le noyau.
pub fn map_ports(ports: &[Port], gpu: &Gpu) -> CoreResult<Vec<Option<String>>> {
    let mut by_prefix: BTreeMap<&str, Vec<(String, bool)>> = BTreeMap::new();
    for prefix in ["eDP", "DP", "HDMI-A", "DVI-I", "DVI-D", "VGA", "LVDS"] {
        let mut list: Vec<(String, bool)> = gpu
            .connectors
            .iter()
            .filter(|c| c.name.rsplit_once('-').is_some_and(|(p, _)| p == prefix))
            .map(|c| (c.name.clone(), c.connected))
            .collect();
        list.sort_by_key(|(name, _)| connector_number(name));
        by_prefix.insert(prefix, list);
    }
    let mut taken: BTreeMap<PortKind, usize> = BTreeMap::new();
    let mut names = Vec::new();
    for port in ports {
        let prefixes = drm_prefixes(port.kind);
        let candidates: Vec<&(String, bool)> = prefixes.iter().flat_map(|p| by_prefix[p].iter()).collect();
        let index = taken.entry(port.kind).or_insert(0);
        let name = candidates.get(*index).map(|(name, connected)| (name.clone(), *connected));
        *index += 1;
        match name {
            Some((name, connected)) if connected == port.connected => names.push(Some(name)),
            Some((name, _)) => {
                return Err(CoreError::message(format!(
                    "{} ({name})",
                    t(
                        "Prises de la carte NVIDIA non reconnues : vibrance désactivée pour ne pas régler le mauvais écran",
                        "NVIDIA card ports not recognised: vibrance disabled so the wrong screen is never changed",
                    )
                )));
            }
            None => names.push(None),
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
    let run = |values: Vec<String>| -> CoreResult<Vec<Port>> {
        // nvibrant choisit la carte et le réglage par ses variables d'environnement.
        let mut args = vec![format!("NVIDIA_GPU={gpu_index}"), "ATTRIBUTE=vibrance".into(), PROGRAM.into()];
        args.extend(values);
        let output = exec.run("env", &args)?;
        if !output.success() {
            let detail = output.stdout.lines().chain(output.stderr.lines()).last().unwrap_or_default().to_string();
            return Err(CoreError::ToolFailed { program: PROGRAM.into(), detail: explain(&output.stdout, &detail) });
        }
        Ok(parse(&output.stdout))
    };
    if exec.which(PROGRAM).is_none() {
        return Err(CoreError::MissingTool(PROGRAM.into()));
    }
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
            connectors: connectors.iter().map(|(n, c)| Connector { name: n.to_string(), connected: *c }).collect(),
        }
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
        assert_eq!(names, [Some("HDMI-A-2".into()), Some("DP-3".into()), Some("DP-4".into()), Some("DP-5".into())]);
    }

    #[test]
    fn contradiction_disables_vibrance() {
        // Le noyau voit l'écran sur la 2e prise DP, nvibrant sur la 1re.
        let card = gpu(&[("DP-1", false), ("DP-2", true), ("DP-3", false), ("HDMI-A-1", true)]);
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
