//! Rapports de performances partagés avec last1.fun, seulement avec l'accord
//! du joueur (réglage `share_reports`, demandé une fois dans la fenêtre,
//! révocable dans Performances), et seulement hors go-to-linux, qui envoie
//! lui-même les siens. Contenu : le diagnostic de ce PC (composants, réglages
//! lus, parties mesurées), pour optimiser Last1 Panel et rien d'autre. Ni nom
//! ni compte : le nom du compte et celui du PC sont masqués, le PC n'est
//! désigné que par une empreinte. Au plus un par jour, après des parties, et
//! seulement s'il a changé ; le service de la session l'envoie (cli.rs).

use crate::exec::Exec;
use crate::system::diagnostic::Report;
use crate::system::gpu::Gpu;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub const URL: &str = "https://last1.fun/api/public/performance";
/// Un rapport par jour au plus.
pub const EVERY: i64 = 24 * 3600;
/// Après un échec (pas d'Internet, site qui refuse) : nouvel essai 6 h plus tard.
pub const RETRY: i64 = 6 * 3600;

/// Dernier envoi : sa date, l'empreinte de son contenu, le prochain essai.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub sent_at: i64,
    pub sum: String,
    pub retry_at: i64,
}

impl State {
    pub fn load(path: &Path) -> Self {
        fs::read_to_string(path).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) {
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(path, serde_json::to_string(self).unwrap_or_default());
    }
}

/// Empreinte de ce PC (/etc/machine-id).
pub fn this_pc() -> String {
    pc_id(&fs::read_to_string("/etc/machine-id").unwrap_or_default())
}

/// Empreinte du PC : jamais son identifiant (machine-id) lui-même.
pub fn pc_id(machine_id: &str) -> String {
    let hash = Sha256::digest(format!("last1-panel:{}", machine_id.trim()));
    hash.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

/// Nom du PC puis nom du compte masqués partout dans le texte, en mots
/// entiers (« max » ne touche pas « maxFrameRate »). Le nom de PC d'origine
/// d'une distribution n'est pas personnel (et le masquer abîmerait des noms
/// de paquets).
pub fn scrub(text: &str, user: &str, host: &str) -> String {
    let mut text = text.to_string();
    for (name, mask) in [(host, "<pc>"), (user, "<utilisateur>")] {
        let generic = ["", "localhost", "cachyos", "archlinux", "root"].contains(&name);
        if !generic && name.len() >= 3 && name.chars().all(|c| c.is_ascii_alphanumeric() || "_-".contains(c)) {
            text = replace_word(&text, name, mask);
        }
    }
    text
}

fn replace_word(text: &str, word: &str, mask: &str) -> String {
    let inside = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(word) {
        // Caractère avant : dans ce qui reste, ou à la fin de ce qui est écrit.
        let before = rest[..at].chars().last().or_else(|| out.chars().last());
        let after = rest[at + word.len()..].chars().next();
        out.push_str(&rest[..at]);
        if before.is_some_and(inside) || after.is_some_and(inside) {
            out.push_str(word);
        } else {
            out.push_str(mask);
        }
        rest = &rest[at + word.len()..];
    }
    out.push_str(rest);
    out
}

/// Le rapport à envoyer, s'il est dû : un jour depuis le dernier, contenu
/// changé, pas d'échec récent ; après des parties (« game »), sauf le tout
/// premier, envoyé dès l'accord pour le matériel (« firstboot »). Rend son
/// empreinte et son occasion.
pub fn due(report: &Report, state: &State, now: i64) -> Option<(String, &'static str)> {
    let first = state.sent_at == 0;
    if (report.sessions.is_empty() && !first) || now < state.retry_at || now - state.sent_at < EVERY {
        return None;
    }
    let sum = content_sum(report);
    let occasion = if report.sessions.is_empty() { "firstboot" } else { "game" };
    (sum != state.sum).then_some((sum, occasion))
}

/// Empreinte de ce qui compte (constats, éléments lus, parties) ; le texte à
/// copier redit le reste.
pub fn content_sum(report: &Report) -> String {
    let content = serde_json::json!({ "checks": report.checks, "facts": report.facts, "sessions": report.sessions });
    Sha256::digest(content.to_string()).iter().take(8).map(|b| format!("{b:02x}")).collect()
}

/// Le matériel du PC sous des clés stables (les `facts` du diagnostic sont
/// traduits) : la table du matériel par PC de last1.fun. `root` : « / », ou
/// un dossier de test.
pub fn system(root: &Path, gpus: &[Gpu]) -> BTreeMap<&'static str, String> {
    let read = |path: &str| fs::read_to_string(root.join(path)).map(|s| s.trim().to_string()).unwrap_or_default();
    let mut system = BTreeMap::new();
    let cpuinfo = read("proc/cpuinfo");
    if let Some(cpu) = cpuinfo
        .lines()
        .find_map(|l| l.strip_prefix("model name").map(|v| v.trim_start_matches([' ', '\t', ':']).to_string()))
    {
        system.insert("cpu", cpu);
    }
    // La carte principale : celle du démarrage, sinon la première.
    if let Some(gpu) = gpus.iter().find(|g| g.boot_vga).or(gpus.first()) {
        system
            .insert("gpu", gpu.name.clone().unwrap_or_else(|| format!("{:04x}:{:04x}", gpu.vendor_id, gpu.device_id)));
    }
    if let Some(kib) = read("proc/meminfo")
        .lines()
        .find_map(|l| l.strip_prefix("MemTotal:")?.split_whitespace().next()?.parse::<u64>().ok())
    {
        system.insert("ram", format!("{} Gio", (kib + 524_288) / 1_048_576));
    }
    // Modèle du PC ou de la carte mère, sauf les textes par défaut des fabricants.
    let machine = read("sys/class/dmi/id/product_name");
    let placeholder =
        ["", "To Be Filled By O.E.M.", "System Product Name", "Default string"].contains(&machine.as_str());
    if !placeholder {
        system.insert("machine", machine);
    }
    let os = os_name(&read("etc/os-release"));
    if !os.is_empty() {
        system.insert("os", os);
    }
    let kernel = read("proc/sys/kernel/osrelease");
    if !kernel.is_empty() {
        system.insert("kernel", kernel);
    }
    system
}

/// Envoie le rapport (`body`) ; vrai s'il est gardé (201) ou si celui du jour
/// l'est déjà (204). `pending` : fichier du corps le temps de l'envoi.
pub fn post(exec: &dyn Exec, body: &str, pending: &Path) -> bool {
    let sent = fs::write(pending, body).is_ok()
        && exec
            .run_ok(
                "curl",
                &[
                    "-fsS".to_string(),
                    "--proto".into(),
                    "=https".into(),
                    "--max-time".into(),
                    "30".into(),
                    "-H".into(),
                    "Content-Type: application/json".into(),
                    "--data-binary".into(),
                    format!("@{}", pending.display()),
                    "-o".into(),
                    "/dev/null".into(),
                    URL.into(),
                ],
            )
            .is_ok();
    let _ = fs::remove_file(pending);
    sent
}

/// Accord retiré : last1.fun efface tous les rapports de ce PC.
pub fn withdraw(exec: &dyn Exec, pc: &str) -> bool {
    let args = ["-fsS", "--proto", "=https", "--max-time", "30", "-X", "DELETE", "-o", "/dev/null"]
        .map(String::from)
        .into_iter()
        .chain([format!("{URL}/{pc}")])
        .collect::<Vec<_>>();
    exec.run_ok("curl", &args).is_ok()
}

/// Corps de la requête (`POST URL`), masqué.
pub fn body(
    report: &Report,
    occasion: &str,
    version: &str,
    pc: &str,
    system: &BTreeMap<&'static str, String>,
    user: &str,
    host: &str,
) -> String {
    let diagnostic = serde_json::json!({ "checks": report.checks, "facts": report.facts, "sessions": report.sessions });
    let body = serde_json::json!({
        "product": "last1-panel",
        "appVersion": version,
        "occasion": occasion,
        "pc": pc,
        "consent": true,
        "diagnostic": diagnostic,
        "system": system,
    });
    scrub(&body.to_string(), user, host)
}

/// Nom de la distribution (/etc/os-release, PRETTY_NAME).
pub fn os_name(os_release: &str) -> String {
    os_release
        .lines()
        .find_map(|line| line.strip_prefix("PRETTY_NAME="))
        .map(|value| value.trim().trim_matches('"').to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::sessions::GameSession;

    fn report(sessions: usize) -> Report {
        Report {
            checks: Vec::new(),
            facts: vec![
                ("Processeur".into(), "Intel(R) Core(TM) i5-10400".into()),
                ("Derniers jeux lancés".into(), "ARC Raiders".into()),
                ("Dossier".into(), "/home/nelly/Jeux sur nelly-pc".into()),
            ],
            sessions: (0..sessions)
                .map(|_| GameSession {
                    program: "PioneerGame.exe".into(),
                    started: "2026-10-19 21:04".into(),
                    minutes: 32.0,
                    fps_average: 58.0,
                    fps_one_percent_low: 31.0,
                    cpu_load: 45.0,
                    gpu_load: 60.0,
                    gpu_clock: 1900.0,
                    limit: "cpu",
                    ..Default::default()
                })
                .collect(),
            text: "nelly".into(),
            go_to_linux: false,
        }
    }

    #[test]
    fn nothing_personal_is_sent() {
        let system = BTreeMap::from([("os", "Arch Linux".to_string()), ("kernel", "7.2.7-arch1-1".to_string())]);
        let body = body(&report(1), "game", "0.1.0.r40", &pc_id("0123456789abcdef\n"), &system, "nelly", "nelly-pc");
        assert!(!body.contains("nelly"), "{body}");
        assert!(body.contains("/home/<utilisateur>/Jeux sur <pc>"), "{body}");
        assert_eq!(scrub("maxFrameRate max", "max", ""), "maxFrameRate <utilisateur>");
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(json["product"], "last1-panel");
        assert_eq!(json["consent"], true);
        assert_eq!(json["pc"].as_str().unwrap().len(), 16);
        assert_eq!(json["diagnostic"]["sessions"][0]["fpsAverage"], 58.0);
        // Le texte à copier n'est pas envoyé.
        assert!(json["diagnostic"].get("text").is_none());
        // Noms génériques : laissés.
        assert_eq!(scrub("cachyos-settings", "j", "cachyos"), "cachyos-settings");
    }

    #[test]
    fn at_most_once_a_day_after_games_when_changed() {
        let now = 1_800_000_000;
        // Tout premier rapport, dès l'accord : le matériel, sans partie.
        assert_eq!(due(&report(0), &State::default(), now).unwrap().1, "firstboot");
        let (sum, occasion) = due(&report(1), &State::default(), now).unwrap();
        assert_eq!(occasion, "game");
        let sent = State { sent_at: now, sum: sum.clone(), retry_at: 0 };
        assert_eq!(due(&report(0), &sent, now + EVERY), None, "ensuite, seulement après des parties");
        assert_eq!(due(&report(2), &sent, now + 3600), None, "moins d'un jour");
        assert_eq!(due(&report(1), &sent, now + EVERY), None, "rien de changé");
        assert!(due(&report(2), &sent, now + EVERY).is_some());
        let failed = State { retry_at: now + RETRY, ..State::default() };
        assert_eq!(due(&report(1), &failed, now + 60), None, "échec récent");
    }

    #[test]
    fn hardware_under_stable_keys() {
        let dir = tempfile::tempdir().unwrap();
        let write = |path: &str, text: &str| {
            let path = dir.path().join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        };
        write("proc/cpuinfo", "processor\t: 0\nmodel name\t: AMD Ryzen 7 5800X3D 8-Core Processor\n");
        write("proc/meminfo", "MemTotal:       32768000 kB\n");
        write("sys/class/dmi/id/product_name", "To Be Filled By O.E.M.\n");
        write("etc/os-release", "PRETTY_NAME=\"CachyOS\"\n");
        write("proc/sys/kernel/osrelease", "6.17.2-2-cachyos\n");
        let gpu = Gpu {
            card: "card1".into(),
            pci_slot: "0000:03:00.0".into(),
            vendor: crate::system::gpu::Vendor::Amd,
            vendor_id: 0x1002,
            device_id: 0x744c,
            name: Some("Radeon RX 7900 XTX".into()),
            driver: Some("amdgpu".into()),
            driver_version: None,
            boot_vga: true,
            integrated: false,
            games: true,
            connectors: vec![],
        };
        let system = system(dir.path(), &[gpu]);
        assert_eq!(system["cpu"], "AMD Ryzen 7 5800X3D 8-Core Processor");
        assert_eq!(system["gpu"], "Radeon RX 7900 XTX");
        assert_eq!(system["ram"], "31 Gio");
        assert_eq!(system["os"], "CachyOS");
        assert_eq!(system["kernel"], "6.17.2-2-cachyos");
        // Texte par défaut du fabricant : pas de modèle.
        assert!(!system.contains_key("machine"));
    }

    #[test]
    fn withdrawing_erases_this_pc_reports() {
        let exec = crate::exec::FakeExec::new().on("curl", "");
        assert!(withdraw(&exec, "0123456789abcdef"));
        let (program, args) = exec.calls().pop().unwrap();
        assert_eq!(program, "curl");
        assert!(args.windows(2).any(|w| w == ["-X", "DELETE"]));
        assert_eq!(args.last().unwrap(), "https://last1.fun/api/public/performance/0123456789abcdef");
    }

    #[test]
    fn reads_the_distribution_and_keeps_state() {
        assert_eq!(os_name("NAME=\"Arch Linux\"\nPRETTY_NAME=\"Arch Linux\"\nID=arch\n"), "Arch Linux");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sous/sharing.json");
        let state = State { sent_at: 5, sum: "ab".into(), retry_at: 7 };
        state.save(&path);
        assert_eq!(State::load(&path), state);
        assert_eq!(State::load(&dir.path().join("absent")), State::default());
    }
}
