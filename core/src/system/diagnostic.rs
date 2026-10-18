//! Diagnostic des performances en jeu : ce qui, sur ce PC, fait perdre des
//! images par seconde par rapport à Windows. Il lit le système tel quel
//! (sysfs, /proc, /dev, variables de la session, réglages de Last1 Panel) et
//! ne change rien : les optimisations du système sont l'affaire de
//! go-to-linux (« Jeux et performances »), qui les pose dès l'installation.
//! Seul réglage de Last1 Panel corrigé d'ici : la faible latence forcée.

use crate::exec::Exec;
use crate::games::profile::LowLatency;
use crate::i18n::t;
use crate::settings::Settings;
use crate::system::desktop::Desktop;
use crate::system::gpu::{Gpu, Vendor};
use crate::system::sessions::GameSession;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Problem,
    Warning,
    Info,
    Ok,
}

/// Ce que Last1 Panel corrige lui-même (ses propres réglages).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Fix {
    /// Faible latence rendue aux jeux (« Réglage du jeu »), partout.
    LowLatency,
    /// MangoHud installé (mot de passe), pour mesurer les parties.
    InstallMangoHud,
    /// Option de lancement de Last1 Panel pour tous les jeux Steam (Steam
    /// fermé le temps de l'écrire, puis rouvert).
    WrapGames,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub id: &'static str,
    pub level: Level,
    pub title: String,
    pub detail: String,
    pub fix: Option<Fix>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// Du plus grave au moins grave.
    pub checks: Vec<Check>,
    /// Ce qui a été lu (processeur, carte, pilote, noyau…).
    pub facts: Vec<(String, String)>,
    /// Dernières parties mesurées par MangoHud (images par seconde, limite).
    pub sessions: Vec<GameSession>,
    /// Le tout en texte, à copier pour l'aide.
    pub text: String,
}

/// Ce que lit le diagnostic : le vrai système, ou un faux pour les tests.
pub struct Sources<'a> {
    pub sys: &'a Path,
    pub dev: &'a Path,
    pub proc_dir: &'a Path,
    pub exec: &'a dyn Exec,
    /// Variables de la session (celles dont Steam et ses jeux héritent).
    pub env: &'a dyn Fn(&str) -> Option<String>,
    pub settings: &'a Settings,
    pub gpus: &'a [Gpu],
    pub desktop: Desktop,
    /// Outil de compatibilité de Steam par défaut (« proton-cachyos… »).
    pub default_proton: Option<String>,
    /// Derniers jeux lancés et leur Proton (« ARC Raiders (proton_experimental) »).
    pub recent_games: Vec<String>,
    /// Dernières parties mesurées (journaux de MangoHud, system/sessions.rs).
    pub sessions: Vec<GameSession>,
    /// MangoHud installé (mesure des parties).
    pub mangohud: bool,
    /// Jeux Steam installés : combien passent par Last1 Panel, sur combien.
    pub wrapped_games: Option<(usize, usize)>,
}

/// Variable posée pour toutes les sessions dans /etc/environment (pam_env) :
/// celle dont Steam hérite, même si ce processus vient d'un service du
/// système (go-to-linux) qui ne l'a pas.
pub fn etc_environment(name: &str) -> Option<String> {
    environment_value(&fs::read_to_string("/etc/environment").ok()?, name)
}

fn environment_value(text: &str, name: &str) -> Option<String> {
    text.lines().rev().find_map(|line| {
        let (key, value) = line.trim().strip_prefix("export ").unwrap_or(line.trim()).split_once('=')?;
        (key.trim() == name).then(|| value.trim().trim_matches('"').to_string()).filter(|v| !v.is_empty())
    })
}

fn read(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn check(id: &'static str, level: Level, title: String, detail: String) -> Check {
    Check { id, level, title, detail, fix: None }
}

pub fn run(src: &Sources) -> Report {
    let mut facts: Vec<(String, String)> = Vec::new();
    let mut checks = Vec::new();
    let unknown = || "?".to_string();

    // Processeur.
    let cpu = read(&src.proc_dir.join("cpuinfo")).and_then(|text| {
        text.lines()
            .find_map(|l| l.strip_prefix("model name").map(|v| v.trim_start_matches([' ', '\t', ':']).to_string()))
    });
    facts.push((t("Processeur", "CPU"), cpu.unwrap_or_else(unknown)));
    let cpufreq = src.sys.join("devices/system/cpu/cpu0/cpufreq");
    let governor = read(&cpufreq.join("scaling_governor"));
    facts.push((
        t("Fréquence du processeur", "CPU frequency"),
        format!(
            "{} / {} / {}",
            read(&cpufreq.join("scaling_driver")).unwrap_or_else(unknown),
            governor.clone().unwrap_or_else(unknown),
            read(&cpufreq.join("energy_performance_preference")).unwrap_or_else(|| "-".into())
        ),
    ));
    let power_profile = src
        .exec
        .run("powerprofilesctl", &["get".into()])
        .ok()
        .filter(|o| o.success())
        .map(|o| o.stdout.trim().to_string())
        .filter(|s| !s.is_empty());
    facts.push((t("Profil d'énergie", "Power profile"), power_profile.clone().unwrap_or_else(unknown)));
    let scheduler = (read(&src.sys.join("kernel/sched_ext/state")).as_deref() == Some("enabled"))
        .then(|| read(&src.sys.join("kernel/sched_ext/root/ops")))
        .flatten();
    facts.push((
        t("Ordonnanceur", "Scheduler"),
        scheduler.clone().unwrap_or_else(|| t("celui du noyau", "the kernel's")),
    ));
    facts.push((t("Noyau", "Kernel"), read(&src.proc_dir.join("sys/kernel/osrelease")).unwrap_or_else(unknown)));
    facts.push((t("Bureau", "Desktop"), format!("{:?}", src.desktop)));
    for gpu in src.gpus {
        facts.push((
            format!("{} {}", t("Carte", "Card"), gpu.card),
            format!(
                "{} ({}{})",
                gpu.name.clone().unwrap_or_else(|| format!("{:04x}:{:04x}", gpu.vendor_id, gpu.device_id)),
                gpu.driver.clone().unwrap_or_else(unknown),
                gpu.driver_version.as_ref().map(|v| format!(" {v}")).unwrap_or_default()
            ),
        ));
    }
    if let Some(proton) = &src.default_proton {
        facts.push((t("Proton par défaut (Steam)", "Default Proton (Steam)"), proton.clone()));
    }
    if !src.recent_games.is_empty() {
        facts.push((t("Derniers jeux lancés", "Last games played"), src.recent_games.join(", ")));
    }

    // Mode d'énergie : « performance » en permanence (PC fixe installé par
    // go-to-linux) ou le temps du jeu (gtl-perf, game-performance).
    // Le profil d'énergie compte d'abord (amd-pstate et intel_pstate gardent
    // le gouverneur « powersave » même en mode performance) ; sans lui, le
    // gouverneur. Rien de lu : rien à dire.
    let saving = match (&power_profile, &governor) {
        (Some(profile), _) => profile != "performance",
        (None, Some(governor)) => governor != "performance",
        (None, None) => false,
    };
    // go-to-linux passe en performance le temps des jeux (portable branché).
    let during_games = src
        .exec
        .run("systemctl", &["is-active".into(), "gtl-perf.service".into()])
        .is_ok_and(|o| o.stdout.trim() == "active");
    if saving && during_games {
        checks.push(check(
            "power",
            Level::Ok,
            t("Mode performance pendant les jeux", "Performance mode while playing"),
            t(
                "go-to-linux passe le processeur en mode performance le temps de chaque jeu.",
                "go-to-linux switches the CPU to performance mode for each game.",
            ),
        ));
    } else if saving {
        let current = power_profile.as_deref().or(governor.as_deref()).unwrap_or("?");
        checks.push(check(
            "power",
            Level::Warning,
            t("Processeur en mode économie", "CPU in power-saving mode"),
            t(
                &format!(
                    "Mode « {current} ». Sur un PC fixe, le mode performance tient le processeur à pleine vitesse, comme « Performances élevées » sous Windows : go-to-linux le met dès l'installation (« Jeux et performances »), sinon : powerprofilesctl set performance."
                ),
                &format!(
                    "“{current}” mode. On a desktop PC, performance mode keeps the CPU at full speed, like Windows' “High performance”: go-to-linux sets it at install (“Games and performance”), otherwise: powerprofilesctl set performance."
                ),
            ),
        ));
    } else if power_profile.is_some() || governor.is_some() {
        checks.push(check(
            "power",
            Level::Ok,
            t("Mode performance", "Performance mode"),
            t("Processeur à pleine vitesse.", "CPU at full speed."),
        ));
    }

    // Processeur X3D à deux blocs : les jeux sur celui qui a la 3D V-Cache.
    if let Some(mode) = fs::read_dir(src.sys.join("bus/platform/drivers/amd_x3d_vcache"))
        .into_iter()
        .flatten()
        .flatten()
        .find_map(|entry| read(&entry.path().join("amd_x3d_mode")))
    {
        checks.push(if mode == "cache" {
            check("x3d", Level::Ok, t("Jeux sur la 3D V-Cache", "Games on the 3D V-Cache"), t("Mode « cache ».", "“cache” mode."))
        } else {
            check(
                "x3d",
                Level::Problem,
                t("Les jeux tournent sans la 3D V-Cache", "Games run without the 3D V-Cache"),
                t(
                    &format!("Processeur X3D en mode « {mode} » : les jeux partent sur le bloc sans 3D V-Cache, beaucoup d'images en moins quand le processeur limite. go-to-linux le met en mode « cache » à chaque démarrage."),
                    &format!("X3D CPU in “{mode}” mode: games land on the die without 3D V-Cache, many fewer frames when the CPU is the limit. go-to-linux sets “cache” mode at each boot."),
                ),
            )
        });
    }

    // Le noyau endort les jeux qui font des « split locks ».
    if read(&src.proc_dir.join("sys/kernel/split_lock_mitigate")).is_some_and(|v| v != "0") {
        checks.push(check(
            "split-lock",
            Level::Warning,
            t("Le noyau ralentit certains jeux exprès", "The kernel slows some games down on purpose"),
            t(
                "Les jeux Windows qui font des « split locks » sont endormis à chaque fois (kernel.split_lock_mitigate = 1) : go-to-linux le désactive, comme SteamOS.",
                "Windows games that do “split locks” are put to sleep each time (kernel.split_lock_mitigate = 1): go-to-linux turns it off, like SteamOS.",
            ),
        ));
    }

    // Ordonnanceur sched_ext : à essayer sans en cas de chutes.
    if let Some(name) = &scheduler {
        checks.push(check(
            "scheduler",
            Level::Info,
            t(&format!("Ordonnanceur {name} actif"), &format!("{name} scheduler active")),
            t(
                "Conçu pour le Steam Deck : aucun gain d'images par seconde prouvé, et des chutes rapportées. go-to-linux garde l'ordonnanceur de CachyOS et arrête celui-ci à sa mise à jour.",
                "Designed for the Steam Deck: no proven frame rate gain, and drops reported. go-to-linux keeps CachyOS's scheduler and stops this one at its update.",
            ),
        ));
    }

    // NTSync : la synchronisation de Windows faite par le noyau.
    let ntsync = src.dev.join("ntsync");
    checks.push(if !ntsync.exists() {
        check(
            "ntsync",
            Level::Warning,
            t("NTSync absent", "NTSync missing"),
            t(
                "Proton synchronise alors les fils des jeux sans l'aide du noyau : plus de travail pour le processeur. Module ntsync à charger (CachyOS le fait d'office).",
                "Proton then syncs game threads without the kernel's help: more CPU work. Load the ntsync module (CachyOS does by default).",
            ),
        )
    } else if fs::OpenOptions::new().read(true).write(true).open(&ntsync).is_err() {
        check(
            "ntsync",
            Level::Warning,
            t("NTSync inaccessible", "NTSync not accessible"),
            t("/dev/ntsync existe mais votre compte ne peut pas l'ouvrir : Proton ne s'en sert pas.", "/dev/ntsync exists but your account can't open it: Proton doesn't use it."),
        )
    } else {
        check("ntsync", Level::Ok, "NTSync".into(), t("Disponible pour Proton.", "Available to Proton."))
    });

    // Cache des shaders : 1 Go par défaut, trop peu pour un gros jeu.
    let nvidia = src.gpus.iter().any(|g| g.vendor == Vendor::Nvidia);
    let cache_var = if nvidia { "__GL_SHADER_DISK_CACHE_SIZE" } else { "MESA_SHADER_CACHE_MAX_SIZE" };
    checks.push(match (src.env)(cache_var) {
        Some(value) => check(
            "shader-cache",
            Level::Ok,
            t("Cache des shaders agrandi", "Larger shader cache"),
            format!("{cache_var}={value}"),
        ),
        None => check(
            "shader-cache",
            Level::Warning,
            t("Cache des shaders limité à 1 Go", "Shader cache limited to 1 GB"),
            t(
                &format!("Les shaders d'un gros jeu (Unreal Engine 5) n'y tiennent pas et se recompilent à chaque partie : saccades, chutes d'images par seconde. go-to-linux pose {cache_var} dans /etc/environment (12 Go)."),
                &format!("A big game's shaders (Unreal Engine 5) don't fit and are recompiled every session: stutter, frame rate drops. go-to-linux sets {cache_var} in /etc/environment (12 GB)."),
            ),
        ),
    });

    // Carte graphique : liaison PCI Express, Resizable BAR, pilote.
    for gpu in src.gpus.iter().filter(|g| matches!(g.vendor, Vendor::Nvidia | Vendor::Amd)) {
        let device = src.sys.join("bus/pci/devices").join(&gpu.pci_slot);
        let width = |name: &str| read(&device.join(name)).and_then(|v| v.parse::<u32>().ok());
        if let (Some(current), Some(max)) = (width("current_link_width"), width("max_link_width"))
            && current < max
        {
            checks.push(check(
                "pcie",
                Level::Problem,
                t(&format!("Carte graphique branchée en x{current} au lieu de x{max}"), &format!("Graphics card linked at x{current} instead of x{max}")),
                t(
                    "Elle reçoit ses données moins vite : vérifiez qu'elle est dans le premier emplacement PCI Express de la carte mère, sans rallonge.",
                    "It receives its data more slowly: check it sits in the motherboard's first PCI Express slot, without a riser.",
                ),
            ));
        }
        if let Some(largest) = largest_prefetchable_bar(&device.join("resource")) {
            let mib = largest / (1024 * 1024);
            checks.push(if mib <= 256 {
                check(
                    "rebar",
                    Level::Warning,
                    t("Resizable BAR désactivé", "Resizable BAR disabled"),
                    t(
                        "Le processeur ne voit la mémoire de la carte que par une fenêtre de 256 Mo, ce qui freine les jeux DirectX 12 (VKD3D-Proton). À activer dans le BIOS : Resizable BAR et Above 4G Decoding.",
                        "The CPU only sees the card's memory through a 256 MB window, which slows DirectX 12 games (VKD3D-Proton). Turn on in the BIOS: Resizable BAR and Above 4G Decoding.",
                    ),
                )
            } else {
                check("rebar", Level::Ok, "Resizable BAR".into(), t(&format!("Actif ({mib} Mo)."), &format!("On ({mib} MB).")))
            });
        }
        if gpu.vendor == Vendor::Nvidia
            && let Some(version) = &gpu.driver_version
            && version.split('.').next().and_then(|m| m.parse::<u32>().ok()).is_some_and(|major| major < 590)
        {
            checks.push(check(
                "driver",
                Level::Warning,
                t(&format!("Pilote NVIDIA {version} ancien"), &format!("Old NVIDIA driver {version}")),
                t(
                    "Les pilotes 590 et plus récents améliorent les jeux DirectX 12 sous Proton : mettez le système à jour.",
                    "Drivers 590 and newer improve DirectX 12 games under Proton: update the system.",
                ),
            ));
        }
    }

    // Mesure des parties : MangoHud, dans les jeux lancés par Last1 Panel.
    checks.push(match src.wrapped_games {
        _ if !src.mangohud => Check {
            id: "measure",
            level: Level::Warning,
            title: t("Parties non mesurées : MangoHud absent", "Games not measured: MangoHud missing"),
            detail: t(
                "MangoHud mesure les images par seconde de chaque partie, sans rien afficher : il dit si le jeu est limité par le processeur ou par la carte.",
                "MangoHud measures each game's frame rate, without showing anything: it tells whether the game is limited by the CPU or the graphics card.",
            ),
            fix: Some(Fix::InstallMangoHud),
        },
        Some((wrapped, total)) if wrapped < total => Check {
            id: "measure",
            level: if wrapped == 0 { Level::Warning } else { Level::Info },
            title: t(
                &format!("{} jeux sur {total} passent par Last1 Panel", wrapped),
                &format!("{wrapped} of {total} games go through Last1 Panel"),
            ),
            detail: t(
                "Les autres ne sont ni réglés ni mesurés : son option de lancement Steam leur manque.",
                "The others are neither set nor measured: they lack its Steam launch option.",
            ),
            fix: Some(Fix::WrapGames),
        },
        _ => check(
            "measure",
            Level::Ok,
            t("Parties mesurées", "Games measured"),
            t("MangoHud enregistre chaque partie lancée par Last1 Panel, sans rien afficher.", "MangoHud records each game launched by Last1 Panel, without showing anything."),
        ),
    });

    // Faible latence forcée par Last1 Panel : une seule image d'avance.
    let games = &src.settings.games;
    if games.global.low_latency != LowLatency::App || games.per_game.values().any(|g| g.low_latency != LowLatency::App)
    {
        checks.push(Check {
            id: "low-latency",
            level: Level::Warning,
            title: t("Mode de faible latence forcé (Last1 Panel)", "Low latency mode forced (Last1 Panel)"),
            detail: t(
                "Une seule image préparée d'avance : quand le processeur et la carte prennent chacun autant de temps (jeux Unreal Engine 5), ils s'attendent l'un l'autre et les images par seconde chutent. Les jeux avec NVIDIA Reflex règlent eux-mêmes leur latence.",
                "Only one frame prepared ahead: when the CPU and the GPU take about as long each (Unreal Engine 5 games), they wait for each other and the frame rate drops. Games with NVIDIA Reflex manage their own latency.",
            ),
            fix: Some(Fix::LowLatency),
        });
    }

    // Mémoire.
    if let Some(meminfo) = read(&src.proc_dir.join("meminfo")) {
        let kib = |key: &str| {
            meminfo.lines().find_map(|l| l.strip_prefix(key)?.split_whitespace().next()?.parse::<u64>().ok())
        };
        if let Some(total) = kib("MemTotal:") {
            facts.push((t("Mémoire", "Memory"), format!("{} Go", (total + 524_288) / 1_048_576)));
        }
        if let Some(used) = kib("SwapTotal:")
            .zip(kib("SwapFree:"))
            .map(|(total, free)| total.saturating_sub(free))
            .filter(|used| *used > 1_048_576)
        {
            checks.push(check(
                "swap",
                Level::Warning,
                t("La mémoire déborde sur le disque", "Memory spills to disk"),
                t(
                    &format!(
                        "{} Go en mémoire d'échange : fermez les programmes inutiles (navigateur…) avant de jouer.",
                        used / 1_048_576
                    ),
                    &format!("{} GB in swap: close unneeded programs (browser…) before playing.", used / 1_048_576),
                ),
            ));
        }
    }

    // Steam compile les shaders en arrière-plan.
    if process_running(src.proc_dir, "fossilize_repla") {
        checks.push(check(
            "shader-precache",
            Level::Warning,
            t("Steam compile des shaders en arrière-plan", "Steam is compiling shaders in the background"),
            t(
                "Il occupe des cœurs du processeur pendant le jeu : laissez-le finir avant de jouer (les jeux tournent ensuite mieux).",
                "It takes CPU cores while you play: let it finish before playing (games then run better).",
            ),
        ));
    }

    checks.sort_by_key(|check| check.level);
    let sessions = src.sessions.clone();
    let text = text(&facts, &checks, &sessions);
    Report { checks, facts, sessions, text }
}

fn text(facts: &[(String, String)], checks: &[Check], sessions: &[GameSession]) -> String {
    let mut text = t("Last1 Panel — diagnostic des performances\n\n", "Last1 Panel — performance diagnostic\n\n");
    for (name, value) in facts {
        text.push_str(&format!("{name} : {value}\n"));
    }
    text.push('\n');
    for check in checks {
        let mark = match check.level {
            Level::Problem => "[!!]",
            Level::Warning => "[! ]",
            Level::Info => "[i ]",
            Level::Ok => "[ok]",
        };
        text.push_str(&format!("{mark} {} — {}\n", check.title, check.detail));
    }
    if !sessions.is_empty() {
        text.push_str(&t("\nDernières parties (MangoHud)\n", "\nLast games (MangoHud)\n"));
        for s in sessions {
            let limit = match s.limit {
                "gpu" => t("limité par la carte graphique", "limited by the graphics card"),
                "cpu" => t("limité par le processeur", "limited by the CPU"),
                _ => "?".into(),
            };
            text.push_str(&format!(
                "{} — {}, {} min : {} i/s (1 % : {}), {} {} %, {} {} % à {} MHz → {limit}\n",
                s.program,
                s.started,
                s.minutes,
                s.fps_average,
                s.fps_one_percent_low,
                t("processeur", "CPU"),
                s.cpu_load,
                t("carte", "GPU"),
                s.gpu_load,
                s.gpu_clock
            ));
        }
    }
    text
}

/// Plus grande fenêtre préchargeable de la carte (sa mémoire vue par le
/// processeur) : fichier `resource` de sysfs, une ligne « début fin
/// drapeaux » par BAR.
fn largest_prefetchable_bar(path: &Path) -> Option<u64> {
    const PREFETCH: u64 = 0x2000;
    let hex = |s: &str| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok();
    read(path)?
        .lines()
        .take(6)
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let (start, end, flags) = (hex(parts.next()?)?, hex(parts.next()?)?, hex(parts.next()?)?);
            (end > start && flags & PREFETCH != 0).then(|| end - start + 1)
        })
        .max()
}

fn process_running(proc_dir: &Path, comm: &str) -> bool {
    fs::read_dir(proc_dir).into_iter().flatten().flatten().any(|entry| {
        entry.file_name().to_str().is_some_and(|name| name.chars().all(|c| c.is_ascii_digit()))
            && read(&entry.path().join("comm")).is_some_and(|c| c == comm)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::FakeExec;
    use std::path::PathBuf;

    fn gpu(version: &str) -> Gpu {
        Gpu {
            card: "card1".into(),
            pci_slot: "0000:01:00.0".into(),
            vendor: Vendor::Nvidia,
            vendor_id: 0x10de,
            device_id: 0x2786,
            name: Some("GeForce RTX 4070".into()),
            driver: Some("nvidia".into()),
            driver_version: Some(version.into()),
            boot_vga: true,
            connectors: vec![],
        }
    }

    fn write(path: PathBuf, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    struct Pc {
        _dir: tempfile::TempDir,
        sys: PathBuf,
        dev: PathBuf,
        proc_dir: PathBuf,
    }

    fn pc() -> Pc {
        let dir = tempfile::tempdir().unwrap();
        let (sys, dev, proc_dir) = (dir.path().join("sys"), dir.path().join("dev"), dir.path().join("proc"));
        fs::create_dir_all(&dev).unwrap();
        write(proc_dir.join("cpuinfo"), "processor : 0\nmodel name\t: AMD Ryzen 9 7950X3D 16-Core Processor\n");
        Pc { _dir: dir, sys, dev, proc_dir }
    }

    #[test]
    fn finds_what_costs_frames() {
        let pc = pc();
        write(pc.proc_dir.join("meminfo"), "MemTotal: 32768000 kB\nSwapTotal: 8000000 kB\nSwapFree: 5000000 kB\n");
        write(pc.proc_dir.join("sys/kernel/split_lock_mitigate"), "1\n");
        write(pc.proc_dir.join("4242/comm"), "fossilize_repla\n");
        write(pc.sys.join("kernel/sched_ext/state"), "enabled\n");
        write(pc.sys.join("kernel/sched_ext/root/ops"), "lavd\n");
        write(pc.sys.join("bus/platform/drivers/amd_x3d_vcache/AMDI0101:00/amd_x3d_mode"), "frequency\n");
        let device = pc.sys.join("bus/pci/devices/0000:01:00.0");
        write(device.join("current_link_width"), "4\n");
        write(device.join("max_link_width"), "16\n");
        // BAR0 (registres), BAR1 de 256 Mo préchargeable : pas de Resizable BAR.
        write(
            device.join("resource"),
            "0x00000000fb000000 0x00000000fbffffff 0x0000000000040200\n0x000000e000000000 0x000000e00fffffff 0x000000000014220c\n",
        );
        let mut settings = Settings::default();
        settings.games.global.low_latency = LowLatency::On;
        let exec = FakeExec::new().on("powerprofilesctl", "balanced\n");
        let report = run(&Sources {
            sys: &pc.sys,
            dev: &pc.dev,
            proc_dir: &pc.proc_dir,
            exec: &exec,
            env: &|_| None,
            settings: &settings,
            gpus: &[gpu("580.95.05")],
            desktop: Desktop::Hyprland,
            default_proton: Some("proton-cachyos-slr".into()),
            recent_games: vec!["ARC Raiders (proton_experimental)".into()],
            sessions: vec![GameSession {
                program: "PioneerGame.exe".into(),
                started: "2026-10-18 21:04".into(),
                minutes: 32.0,
                fps_average: 58.0,
                fps_one_percent_low: 31.0,
                cpu_load: 45.0,
                gpu_load: 60.0,
                gpu_clock: 2610.0,
                limit: "cpu",
            }],
            mangohud: false,
            wrapped_games: Some((0, 3)),
        });
        let ids =
            |level: Level| -> Vec<&str> { report.checks.iter().filter(|c| c.level == level).map(|c| c.id).collect() };
        assert_eq!(ids(Level::Problem), ["x3d", "pcie"]);
        assert_eq!(
            ids(Level::Warning),
            [
                "power",
                "split-lock",
                "ntsync",
                "shader-cache",
                "rebar",
                "driver",
                "measure",
                "low-latency",
                "swap",
                "shader-precache"
            ]
        );
        assert_eq!(ids(Level::Info), ["scheduler"]);
        let fixes: Vec<Fix> = report.checks.iter().filter_map(|c| c.fix).collect();
        assert_eq!(fixes, [Fix::InstallMangoHud, Fix::LowLatency]);
        assert!(report.text.contains("AMD Ryzen 9 7950X3D") && report.text.contains("balanced"));
        assert!(
            report.text.contains("proton-cachyos-slr") && report.text.contains("lavd") && report.text.contains("[!!]")
        );
    }

    #[test]
    fn well_set_up_pc_is_all_ok() {
        let pc = pc();
        write(pc.dev.join("ntsync"), "");
        write(pc.proc_dir.join("sys/kernel/split_lock_mitigate"), "0\n");
        write(pc.sys.join("bus/platform/drivers/amd_x3d_vcache/AMDI0101:00/amd_x3d_mode"), "cache\n");
        let device = pc.sys.join("bus/pci/devices/0000:01:00.0");
        write(device.join("current_link_width"), "16\n");
        write(device.join("max_link_width"), "16\n");
        write(
            device.join("resource"),
            "0x00000000fb000000 0x00000000fbffffff 0x0000000000040200\n0x000000c000000000 0x000000c3ffffffff 0x000000000014220c\n",
        );
        let exec = FakeExec::new().on("powerprofilesctl", "performance\n");
        let env = |name: &str| (name == "__GL_SHADER_DISK_CACHE_SIZE").then(|| "12000000000".to_string());
        let report = run(&Sources {
            sys: &pc.sys,
            dev: &pc.dev,
            proc_dir: &pc.proc_dir,
            exec: &exec,
            env: &env,
            settings: &Settings::default(),
            gpus: &[gpu("595.58.03")],
            desktop: Desktop::Plasma,
            default_proton: None,
            recent_games: Vec::new(),
            sessions: Vec::new(),
            mangohud: true,
            wrapped_games: Some((1, 1)),
        });
        assert!(report.checks.iter().all(|c| c.level == Level::Ok), "{:#?}", report.checks);
        assert!(report.checks.iter().any(|c| c.id == "rebar" && c.detail.contains("16384")));
    }

    #[test]
    fn game_time_performance_counts() {
        let pc = pc();
        let power = |exec: &FakeExec| {
            run(&Sources {
                sys: &pc.sys,
                dev: &pc.dev,
                proc_dir: &pc.proc_dir,
                exec,
                env: &|_| None,
                settings: &Settings::default(),
                gpus: &[],
                desktop: Desktop::Plasma,
                default_proton: None,
                recent_games: Vec::new(),
                sessions: Vec::new(),
                mangohud: true,
                wrapped_games: Some((1, 1)),
            })
            .checks
            .into_iter()
            .find(|c| c.id == "power")
            .map(|c| c.level)
        };
        // Portable en « balanced », passé en performance le temps des jeux.
        assert_eq!(
            power(&FakeExec::new().on("powerprofilesctl", "balanced\n").on("systemctl", "active\n")),
            Some(Level::Ok)
        );
        assert_eq!(power(&FakeExec::new().on("powerprofilesctl", "balanced\n")), Some(Level::Warning));
        // Rien de lu (machine virtuelle) : pas d'avis.
        assert_eq!(power(&FakeExec::new().without("powerprofilesctl")), None);
    }

    #[test]
    fn reads_etc_environment() {
        let text = "# go-to-linux\n__GL_SHADER_DISK_CACHE_SIZE=12000000000\nexport EDITOR=\"nano\"\n#MESA_SHADER_CACHE_MAX_SIZE=1G\n";
        assert_eq!(environment_value(text, "__GL_SHADER_DISK_CACHE_SIZE").as_deref(), Some("12000000000"));
        assert_eq!(environment_value(text, "EDITOR").as_deref(), Some("nano"));
        assert_eq!(environment_value(text, "MESA_SHADER_CACHE_MAX_SIZE"), None);
    }

    #[test]
    fn bars_are_read_from_sysfs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("resource");
        fs::write(
            &path,
            "0x0000000000000000 0x0000000000000000 0x0000000000000000\n0x000000e000000000 0x000000e00fffffff 0x000000000014220c\n",
        )
        .unwrap();
        assert_eq!(largest_prefetchable_bar(&path), Some(256 * 1024 * 1024));
        assert_eq!(largest_prefetchable_bar(&dir.path().join("absent")), None);
    }
}
