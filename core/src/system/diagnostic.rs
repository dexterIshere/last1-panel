//! Diagnostic des performances en jeu : ce qui, sur ce PC, fait perdre des
//! images par seconde par rapport à Windows. Il lit le système tel quel
//! (sysfs, /proc, /dev, variables de la session, réglages de Last1 Panel) et
//! ne change rien tout seul. go-to-linux (« Jeux et performances ») pose les
//! optimisations du système dès l'installation ; ailleurs, ou s'il en manque
//! une, chaque point réglable a son bouton (Fix, system/repair.rs).

use crate::exec::Exec;
use crate::games::profile::LowLatency;
use crate::i18n::t;
use crate::settings::Settings;
use crate::system::desktop::Desktop;
use crate::system::gpu::{Gpu, Vendor};
use crate::system::packages;
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

/// Ce que Last1 Panel corrige d'un clic : ses propres réglages, et ceux du
/// système (mot de passe demandé une fois).
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
    /// Réglages du système, comme go-to-linux les pose (system/repair.rs) :
    /// mode performance, module ntsync, cache des shaders de 12 Go, split
    /// locks non freinés, jeux sur la 3D V-Cache, ordonnanceur du noyau.
    Performance,
    Ntsync,
    ShaderCache,
    SplitLock,
    X3dCache,
    Scheduler,
    /// Mémoire compressée (zram-generator, comme CachyOS).
    Zram,
    /// Redémarrage du PC (cache des shaders posé pour la prochaine connexion,
    /// nouveau pilote installé).
    Restart,
    /// Pilotes Vulkan installés (64 et 32 bits).
    VulkanDriver,
    /// Système mis à jour (pilote graphique compris) : `pacman -Syu`.
    SystemUpdate,
    /// GameMode prêt à passer le processeur en performance pendant les
    /// jeux : installé (pacman) et le compte dans son groupe « gamemode ».
    GameMode,
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
    /// PC installé par go-to-linux, qui pose lui-même les optimisations ;
    /// sinon, chaque point dit comment le corriger.
    pub go_to_linux: bool,
}

/// Ce que lit le diagnostic : le vrai système, ou un faux pour les tests.
pub struct Sources<'a> {
    pub sys: &'a Path,
    pub dev: &'a Path,
    pub proc_dir: &'a Path,
    pub exec: &'a dyn Exec,
    /// Variables de la session (celles dont Steam et ses jeux héritent).
    pub env: &'a dyn Fn(&str) -> Option<String>,
    /// Variables de la prochaine connexion (/etc/environment).
    pub next_login_env: &'a dyn Fn(&str) -> Option<String>,
    pub settings: &'a Settings,
    pub gpus: &'a [Gpu],
    pub desktop: Desktop,
    /// Outil de compatibilité de Steam par défaut (« proton-cachyos… »).
    pub default_proton: Option<String>,
    /// Derniers jeux lancés et leur Proton (« ARC Raiders (proton_experimental) »).
    pub recent_games: Vec<String>,
    /// Dernier jeu lancé par Last1 Panel (paths::last_launch_file).
    pub last_launch: Option<LastLaunch>,
    /// Version de Last1 Panel (update::installed_version).
    pub version: String,
    /// Version de go-to-linux (/usr/local/lib/gtl/VERSION).
    pub go_to_linux_version: Option<String>,
    /// Liaison PCI Express la plus large mesurée pendant la dernière partie,
    /// par carte (emplacement PCI) : au repos, le pilote peut la réduire.
    pub pcie_in_game: Vec<GameLink>,
    /// Le compte peut se servir de GameMode (gamemode_allowed).
    pub gamemode_allowed: bool,
    /// Shaders compilés par Steam en ce moment (games/shaders.rs).
    pub precache: Option<crate::games::shaders::Precache>,
    /// Dernières parties mesurées (journaux de MangoHud, system/sessions.rs).
    pub sessions: Vec<GameSession>,
    /// MangoHud installé (mesure des parties).
    pub mangohud: bool,
    /// Jeux Steam installés : combien passent par Last1 Panel, sur combien.
    pub wrapped_games: Option<(usize, usize)>,
    /// PC installé par go-to-linux (/var/lib/gtl).
    pub go_to_linux: bool,
    /// zram-generator installé, ou installable (pacman).
    pub can_zram: bool,
    /// pacman (Arch, CachyOS) : pilotes vérifiés par rapport aux dépôts.
    pub pacman: bool,
    /// Paquets qui ont une version plus récente (system/packages.rs).
    pub updates: Vec<String>,
}

/// Variable posée pour toutes les sessions dans /etc/environment (pam_env) :
/// celle dont Steam hérite, même si ce processus vient d'un service du
/// système (go-to-linux) qui ne l'a pas.
pub fn etc_environment(name: &str) -> Option<String> {
    environment_value(&fs::read_to_string("/etc/environment").ok()?, name)
}

/// Variable telle que les jeux la reçoivent : dans l'environnement de Steam
/// s'il tourne (les jeux en héritent), sinon dans celui du bureau (Hyprland,
/// KWin, GNOME Shell), sinon dans celui de ce programme. Seuls les processus
/// de ce compte se lisent : ceux des autres sont sautés.
pub fn game_env(proc_dir: &Path, name: &str) -> Option<String> {
    for comm in ["steam", "Hyprland", "kwin_wayland", "gnome-shell"] {
        let Ok(entries) = fs::read_dir(proc_dir) else { break };
        let environ = entries.flatten().find_map(|entry| {
            let numeric = entry.file_name().to_str().is_some_and(|n| n.chars().all(|c| c.is_ascii_digit()));
            (numeric && read(&entry.path().join("comm")).as_deref() == Some(comm))
                .then(|| fs::read(entry.path().join("environ")).ok())
                .flatten()
        });
        if let Some(environ) = environ {
            let prefix = format!("{name}=");
            return environ
                .split(|&b| b == 0)
                .find_map(|var| var.strip_prefix(prefix.as_bytes()))
                .map(|value| String::from_utf8_lossy(value).into_owned())
                .filter(|value| !value.is_empty());
        }
    }
    std::env::var(name).ok()
}

pub(crate) fn environment_value(text: &str, name: &str) -> Option<String> {
    text.lines().rev().find_map(|line| {
        let (key, value) = line.trim().strip_prefix("export ").unwrap_or(line.trim()).split_once('=')?;
        (key.trim() == name).then(|| value.trim().trim_matches('"').to_string()).filter(|v| !v.is_empty())
    })
}

fn read(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Liaison PCI Express d'une carte graphique avec la carte mère.
#[derive(Debug, PartialEq)]
struct SlotLink {
    /// Largeur actuelle (nombre de voies).
    current: u32,
    /// Ce que la carte sait faire.
    card: u32,
    /// Ce que l'emplacement sait faire (port du processeur ou du chipset).
    slot: Option<u32>,
    /// Vitesse actuelle, en GT/s (16 : PCIe 4.0).
    speed: Option<f64>,
}

/// Liaison la plus large (et la plus rapide) mesurée pendant la dernière
/// partie, pour une carte (paths::pcie_in_game_file).
#[derive(Debug, Clone, PartialEq)]
pub struct GameLink {
    /// Emplacement PCI de la carte (« 0000:01:00.0 »).
    pub slot: String,
    pub width: u32,
    /// GT/s.
    pub speed: Option<f64>,
}

/// « x8 en PCIe 4.0 » (16 GT/s), ou « x8 » sans la vitesse.
fn link_text(width: u32, speed: Option<f64>) -> String {
    let generation = speed.and_then(|gts| {
        [(2.5, "1.0"), (5.0, "2.0"), (8.0, "3.0"), (16.0, "4.0"), (32.0, "5.0"), (64.0, "6.0")]
            .iter()
            .find(|(value, _)| (gts - value).abs() < 0.1)
            .map(|(_, name)| *name)
    });
    match generation {
        Some(generation) => t(&format!("x{width} en PCIe {generation}"), &format!("x{width} at PCIe {generation}")),
        None => format!("x{width}"),
    }
}

/// Une carte AMD ou Intel a son propre commutateur PCI Express : la liaison
/// avec l'emplacement est en haut de ce commutateur (même fabricant que la
/// puce), pas celle de la puce, interne à la carte.
fn slot_link(device: &Path, vendor_id: u32) -> Option<SlotLink> {
    // Carte en veille (portable, puce intégrée à l'écran) : sa liaison est
    // coupée, la lire ne donnerait rien de vrai.
    if read(&device.join("power/runtime_status")).as_deref() == Some("suspended") {
        return None;
    }
    let is_pci = |path: &Path| {
        path.file_name().and_then(|name| name.to_str()).is_some_and(|name| {
            name.len() == 12 && name.as_bytes()[4] == b':' && name.as_bytes()[7] == b':' && name.as_bytes()[10] == b'.'
        })
    };
    let vendor =
        |path: &Path| read(&path.join("vendor")).and_then(|v| u32::from_str_radix(v.trim_start_matches("0x"), 16).ok());
    // Largeurs possibles en PCI Express ; une carte éteinte répond 63.
    let width = |path: &Path, name: &str| {
        read(&path.join(name)).and_then(|v| v.parse::<u32>().ok()).filter(|w| [1, 2, 4, 8, 12, 16, 32].contains(w))
    };
    let mut top = fs::canonicalize(device).unwrap_or_else(|_| device.to_path_buf());
    while let Some(parent) = top.parent().filter(|p| is_pci(p) && vendor(p) == Some(vendor_id)) {
        top = parent.to_path_buf();
    }
    Some(SlotLink {
        current: width(&top, "current_link_width")?,
        card: width(&top, "max_link_width")?,
        slot: top.parent().filter(|p| is_pci(p)).and_then(|p| width(p, "max_link_width")),
        // « 16.0 GT/s PCIe » ; « Unknown » quand le noyau ne sait pas.
        speed: read(&top.join("current_link_speed"))
            .and_then(|v| v.split_whitespace().next()?.parse::<f64>().ok())
            .filter(|gts| *gts > 0.0),
    })
}

/// Puces graphiques AMD intégrées au processeur (APU), par emplacement PCI :
/// le pilote AMD (KFD) les décrit comme un nœud qui a à la fois des cœurs de
/// processeur et des unités graphiques ; une carte AMD à part n'a que les
/// secondes.
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

/// Largeur et vitesse actuelles de la liaison d'une carte avec la carte
/// mère (relevées pendant les parties par le service de la session).
pub fn link_now(sys: &Path, gpu: &Gpu) -> Option<(u32, Option<f64>)> {
    slot_link(&sys.join("bus/pci/devices").join(&gpu.pci_slot), gpu.vendor_id).map(|link| (link.current, link.speed))
}

/// Pilote AMD : la liaison change avec la charge (pp_dpm_pcie, niveau en
/// cours marqué « * ») ; réduite au repos, elle s'élargit en jeu.
fn amd_idle_link(device: &Path) -> bool {
    let Some(levels) = read(&device.join("pp_dpm_pcie")) else { return false };
    let levels: Vec<(u32, bool)> = levels
        .lines()
        .filter_map(|line| {
            let width =
                line.split_whitespace().find_map(|word| word.trim_end_matches(',').strip_prefix('x')?.parse().ok())?;
            Some((width, line.trim_end().ends_with('*')))
        })
        .collect();
    let widest = levels.iter().map(|(width, _)| *width).max();
    levels.iter().any(|(width, current)| *current && Some(*width) < widest)
}

/// Le compte peut-il se servir de GameMode ? Sa règle polkit ne laisse
/// changer le mode du processeur qu'aux membres du groupe « gamemode ». Pas
/// de groupe (GameMode absent, autre distribution) : rien à dire.
pub fn gamemode_allowed(group_file: &Path, user: &str) -> bool {
    let Ok(text) = fs::read_to_string(group_file) else { return true };
    text.lines()
        .find(|line| line.starts_with("gamemode:"))
        .is_none_or(|line| line.rsplit(':').next().unwrap_or_default().split(',').any(|member| member.trim() == user))
}

/// Dernier jeu lancé par Last1 Panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastLaunch {
    pub name: String,
    /// Il y a combien de secondes.
    pub seconds: u64,
    /// MangoHud devait le mesurer (« autostart_log=1 » dans MANGOHUD_CONFIG).
    pub measured: bool,
    /// Un journal de MangoHud a été écrit depuis (il l'écrit pendant la partie).
    pub logged: bool,
}

/// « il y a 5 min ».
fn ago(seconds: u64) -> String {
    match seconds {
        0..60 => t("à l'instant", "just now"),
        60..3600 => t(&format!("il y a {} min", seconds / 60), &format!("{} min ago", seconds / 60)),
        3600..86400 => t(&format!("il y a {} h", seconds / 3600), &format!("{} h ago", seconds / 3600)),
        _ => t(&format!("il y a {} j", seconds / 86400), &format!("{} days ago", seconds / 86400)),
    }
}

fn check(id: &'static str, level: Level, title: String, detail: String) -> Check {
    Check { id, level, title, detail, fix: None }
}

pub fn run(src: &Sources) -> Report {
    let mut facts: Vec<(String, String)> = Vec::new();
    // En tête du rapport : on voit tout de suite si le PC est à jour.
    facts.push(("Last1 Panel".to_string(), src.version.clone()));
    if let Some(version) = &src.go_to_linux_version {
        facts.push(("go-to-linux".to_string(), version.clone()));
    }
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
    facts.push((
        t("Profil d'énergie", "Power profile"),
        power_profile.clone().unwrap_or_else(|| {
            // Sans power-profiles-daemon, le gouverneur (ci-dessus) décide
            // seul : rien ne manque, il n'est qu'un sélecteur de mode.
            if src.exec.which("powerprofilesctl").is_none() {
                if governor.as_deref() == Some("performance") {
                    t(
                        "inutile : processeur réglé directement en performance",
                        "not needed: CPU set straight to performance",
                    )
                } else {
                    t("aucun : le gouverneur décide", "none: the governor decides")
                }
            } else {
                unknown()
            }
        }),
    ));
    let scheduler = (read(&src.sys.join("kernel/sched_ext/state")).as_deref() == Some("enabled"))
        .then(|| read(&src.sys.join("kernel/sched_ext/root/ops")))
        .flatten();
    facts.push((
        t("Ordonnanceur", "Scheduler"),
        scheduler.clone().unwrap_or_else(|| t("celui du noyau", "the kernel's")),
    ));
    let osrelease = read(&src.proc_dir.join("sys/kernel/osrelease"));
    // « 7.2.7-arch1-1 » : (7, 2).
    let kernel = osrelease.as_deref().and_then(|release| {
        let mut numbers = release.split(|c: char| !c.is_ascii_digit()).map(|n| n.parse::<u32>().ok());
        Some((numbers.next()??, numbers.next()??))
    });
    facts.push((t("Noyau", "Kernel"), osrelease.unwrap_or_else(unknown)));
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
    if src.wrapped_games.is_some() || src.last_launch.is_some() {
        facts.push((
            t("Dernier jeu lancé par Last1 Panel", "Last game launched by Last1 Panel"),
            src.last_launch
                .as_ref()
                .map(|last| format!("{}, {}", last.name, ago(last.seconds)))
                .unwrap_or_else(|| t("aucun pour l'instant", "none yet")),
        ));
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
    // Portable : pas de mode performance permanent sans power-profiles-daemon
    // (qui laisse revenir en arrière depuis le bureau).
    let battery = fs::read_dir(src.sys.join("class/power_supply"))
        .into_iter()
        .flatten()
        .flatten()
        .any(|entry| read(&entry.path().join("type")).as_deref() == Some("Battery"));
    // Last1 Panel aussi, dans les jeux qui passent par lui (games/launch.rs) :
    // game-performance (power-profiles-daemon), sinon GameMode, qui ne change
    // le mode du processeur que pour un compte de son groupe (règle polkit).
    let wrapped = src.wrapped_games.is_some_and(|(wrapped, _)| wrapped > 0);
    let ppd_tool = src.exec.which("game-performance").is_some() && src.exec.which("powerprofilesctl").is_some();
    let gamemode = src.exec.which("gamemoderun").is_some();
    let gamemode_ready = gamemode && src.gamemode_allowed;
    facts.push((
        t("Pleine vitesse pendant les jeux", "Full speed while playing"),
        if during_games {
            "go-to-linux (gtl-perf)".to_string()
        } else if ppd_tool {
            "game-performance (power-profiles-daemon)".to_string()
        } else if gamemode_ready {
            "GameMode".to_string()
        } else if gamemode {
            t("GameMode, compte hors de son groupe « gamemode »", "GameMode, account outside its “gamemode” group")
        } else {
            t(
                "aucun outil (ni power-profiles-daemon ni GameMode)",
                "no tool (neither power-profiles-daemon nor GameMode)",
            )
        },
    ));
    let by_panel = wrapped && (ppd_tool || gamemode_ready);
    if saving && (during_games || by_panel) {
        checks.push(check(
            "power",
            Level::Ok,
            t("Mode performance pendant les jeux", "Performance mode while playing"),
            if during_games {
                t(
                    "go-to-linux passe le processeur en mode performance le temps de chaque jeu.",
                    "go-to-linux switches the CPU to performance mode for each game.",
                )
            } else {
                t(
                    "Last1 Panel passe le processeur en mode performance le temps de chaque jeu.",
                    "Last1 Panel switches the CPU to performance mode for each game.",
                )
            },
        ));
    } else if saving {
        let current = power_profile.as_deref().or(governor.as_deref()).unwrap_or("?");
        // Ce qui manque pour la pleine vitesse, et le bouton qui le règle.
        let (reason, fix) = if gamemode && !src.gamemode_allowed {
            (
                t(
                    "GameMode est installé, mais votre compte n'est pas dans son groupe « gamemode » : sans lui, GameMode n'a pas le droit de passer le processeur en performance pendant les jeux.",
                    "GameMode is installed, but your account is not in its “gamemode” group: without it, GameMode may not switch the CPU to performance while playing.",
                ),
                Some(Fix::GameMode),
            )
        } else if power_profile.is_some() || !battery {
            (
                t(
                    &format!(
                        "{}Sur un PC fixe, le mode performance tient le processeur à pleine vitesse, comme « Performances élevées » sous Windows.",
                        if power_profile.is_none() {
                            "Ni GameMode ni power-profiles-daemon : rien ne le passe en performance le temps des jeux. "
                        } else {
                            ""
                        }
                    ),
                    &format!(
                        "{}On a desktop PC, performance mode keeps the CPU at full speed, like Windows' “High performance”.",
                        if power_profile.is_none() {
                            "Neither GameMode nor power-profiles-daemon: nothing switches it to performance while playing. "
                        } else {
                            ""
                        }
                    ),
                ),
                Some(Fix::Performance),
            )
        } else if gamemode {
            // Le bouton est celui de la mesure des parties (même page).
            (
                t(
                    "GameMode est prêt, mais aucun jeu ne passe encore par Last1 Panel pour s'en servir : le bouton « Activer pour tous les jeux » de cette page le règle.",
                    "GameMode is ready, but no game goes through Last1 Panel yet to use it: this page's “Turn on for all games” button sets it.",
                ),
                None,
            )
        } else {
            (
                t(
                    "Ni GameMode ni power-profiles-daemon sur ce portable : GameMode le passerait en performance le temps de chaque jeu, sans vider la batterie le reste du temps.",
                    "Neither GameMode nor power-profiles-daemon on this laptop: GameMode would switch it to performance for each game, without draining the battery the rest of the time.",
                ),
                src.pacman.then_some(Fix::GameMode),
            )
        };
        checks.push(Check {
            fix,
            ..check(
                "power",
                Level::Warning,
                t("Processeur en mode économie", "CPU in power-saving mode"),
                t(&format!("Mode « {current} ». {reason}"), &format!("“{current}” mode. {reason}")),
            )
        });
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
            Check {
                fix: Some(Fix::X3dCache),
                ..check(
                    "x3d",
                    Level::Problem,
                    t("Les jeux tournent sans la 3D V-Cache", "Games run without the 3D V-Cache"),
                    t(
                        &format!("Processeur X3D en mode « {mode} » : les jeux partent sur le bloc sans 3D V-Cache, beaucoup d'images en moins quand le processeur limite."),
                        &format!("X3D CPU in “{mode}” mode: games land on the die without 3D V-Cache, many fewer frames when the CPU is the limit."),
                    ),
                )
            }
        });
    }

    // Le noyau endort les jeux qui font des « split locks ».
    if read(&src.proc_dir.join("sys/kernel/split_lock_mitigate")).is_some_and(|v| v != "0") {
        checks.push(Check {
            fix: Some(Fix::SplitLock),
            ..check(
                "split-lock",
                Level::Warning,
                t("Le noyau ralentit certains jeux exprès", "The kernel slows some games down on purpose"),
                t(
                    "Les jeux Windows qui font des « split locks » sont endormis à chaque fois (kernel.split_lock_mitigate = 1). SteamOS le désactive.",
                    "Windows games that do “split locks” are put to sleep each time (kernel.split_lock_mitigate = 1). SteamOS turns it off.",
                ),
            )
        });
    }

    // Ordonnanceur sched_ext : aucun gain prouvé, des chutes rapportées. Le
    // service qui l'a lancé (cgroup de son processus) est dit, pour le
    // rapport : c'est lui que le bouton arrête.
    let launcher = scheduler.as_ref().and_then(|_| scheduler_unit(src.proc_dir));
    if let Some(unit) = &launcher {
        facts.push((t("Ordonnanceur lancé par", "Scheduler started by"), unit.clone()));
    }
    if let Some(name) = &scheduler {
        checks.push(Check {
            fix: Some(Fix::Scheduler),
            ..check(
                "scheduler",
                Level::Info,
                t(&format!("Ordonnanceur {name} actif"), &format!("{name} scheduler active")),
                t(
                    "Conçu pour le Steam Deck : aucun gain d'images par seconde prouvé, et des chutes rapportées. go-to-linux garde celui du noyau.",
                    "Designed for the Steam Deck: no proven frame rate gain, and drops reported. go-to-linux keeps the kernel's.",
                ),
            )
        });
    }

    // NTSync : la synchronisation de Windows faite par le noyau.
    let ntsync = src.dev.join("ntsync");
    checks.push(if !ntsync.exists() {
        let mut detail = t(
            "Proton synchronise alors les fils des jeux sans l'aide du noyau : plus de travail pour le processeur.",
            "Proton then syncs game threads without the kernel's help: more CPU work.",
        );
        // Déjà réglé pour chaque démarrage (bouton), mais pas chargé à
        // celui-ci : pourquoi, d'après modinfo et le journal du démarrage.
        if Path::new("/etc/modules-load.d/last1-panel-ntsync.conf").exists() {
            let module = src.exec.run("modinfo", &["-n".into(), "ntsync".into()]).is_ok_and(|o| o.success());
            let blocked = ntsync_blocker(
                &["/etc/modprobe.d", "/usr/lib/modprobe.d", "/run/modprobe.d"].map(Path::new),
                &read(&src.proc_dir.join("cmdline")).unwrap_or_default(),
            );
            let journal = src
                .exec
                .run(
                    "journalctl",
                    &["-b", "-u", "systemd-modules-load", "--no-pager", "-q", "-o", "cat"].map(String::from),
                )
                .map(|o| o.stdout.lines().filter(|l| l.contains("ntsync")).collect::<Vec<_>>().join(" ; "))
                .unwrap_or_default();
            detail = format!(
                "{detail} {}",
                if !module {
                    t(
                        "Réglé pour chaque démarrage, mais ce noyau n'a pas le module ntsync.",
                        "Set for every boot, but this kernel has no ntsync module.",
                    )
                } else if let Some(blocker) = &blocked {
                    t(
                        &format!("Bloqué au démarrage par {blocker} : le bouton pose un service de démarrage qui le charge quand même."),
                        &format!("Blocked at boot by {blocker}: the button sets a boot service that loads it anyway."),
                    )
                } else if journal.is_empty() {
                    t(
                        "Réglé pour chaque démarrage, mais pas chargé à celui-ci.",
                        "Set for every boot, but not loaded at this one.",
                    )
                } else {
                    t(
                        &format!("Réglé pour chaque démarrage, mais pas chargé à celui-ci : {journal}"),
                        &format!("Set for every boot, but not loaded at this one: {journal}"),
                    )
                }
            );
        }
        Check {
            fix: Some(Fix::Ntsync),
            ..check("ntsync", Level::Warning, t("NTSync absent", "NTSync missing"), detail)
        }
    } else if fs::OpenOptions::new().read(true).write(true).open(&ntsync).is_err() {
        Check {
            fix: Some(Fix::Ntsync),
            ..check(
                "ntsync",
                Level::Warning,
                t("NTSync inaccessible", "NTSync not accessible"),
                t(
                    "/dev/ntsync existe mais votre compte ne peut pas l'ouvrir : Proton ne s'en sert pas.",
                    "/dev/ntsync exists but your account can't open it: Proton doesn't use it.",
                ),
            )
        }
    } else {
        check("ntsync", Level::Ok, "NTSync".into(), t("Disponible pour Proton.", "Available to Proton."))
    });

    // Cache des shaders : 1 Go par défaut, trop peu pour un gros jeu. Posé
    // dans /etc/environment mais pas encore dans la session : il vaudra à la
    // prochaine connexion.
    let nvidia = src.gpus.iter().any(|g| g.vendor == Vendor::Nvidia);
    let cache_var = if nvidia { "__GL_SHADER_DISK_CACHE_SIZE" } else { "MESA_SHADER_CACHE_MAX_SIZE" };
    checks.push(match ((src.env)(cache_var), (src.next_login_env)(cache_var)) {
        (Some(value), _) => check(
            "shader-cache",
            Level::Ok,
            t("Cache des shaders agrandi", "Larger shader cache"),
            format!("{cache_var}={value}"),
        ),
        (None, Some(_)) => Check {
            fix: Some(Fix::Restart),
            ..check(
                "shader-cache",
                Level::Info,
                t("Cache des shaders agrandi à la prochaine connexion", "Larger shader cache from the next login"),
                t(
                    "C'est fait : Steam et ses jeux le prendront après un redémarrage du PC (ou une déconnexion-reconnexion).",
                    "Done: Steam and its games will use it after the PC restarts (or after logging out and back in).",
                ),
            )
        },
        (None, None) => Check {
            fix: Some(Fix::ShaderCache),
            ..check(
                "shader-cache",
                Level::Warning,
                t("Cache des shaders limité à 1 Go", "Shader cache limited to 1 GB"),
                t(
                    "Les shaders d'un gros jeu (Unreal Engine 5) n'y tiennent pas et se recompilent à chaque partie : saccades, chutes d'images par seconde.",
                    "A big game's shaders (Unreal Engine 5) don't fit and are recompiled every session: stutter, frame rate drops.",
                ),
            )
        },
    });

    // Carte graphique : liaison PCI Express, Resizable BAR, pilote. Pas la
    // puce AMD intégrée au processeur : ni emplacement ni mémoire à elle.
    let apus = amd_apus(src.sys);
    for gpu in
        src.gpus.iter().filter(|g| matches!(g.vendor, Vendor::Nvidia | Vendor::Amd) && !apus.contains(&g.pci_slot))
    {
        let device = src.sys.join("bus/pci/devices").join(&gpu.pci_slot);
        // Portable : la carte est soudée, souvent câblée en x8 ; rien à
        // déplacer ni à renfoncer, la liaison n'est qu'un fait.
        let laptop = battery
            || read(&src.sys.join("class/dmi/id/chassis_type"))
                .is_some_and(|chassis| ["8", "9", "10", "14", "31", "32"].contains(&chassis.as_str()))
            || gpu
                .name
                .as_deref()
                .is_some_and(|name| ["Mobile", "Max-Q", "Laptop"].iter().any(|word| name.contains(word)));
        if let Some(link) = slot_link(&device, gpu.vendor_id) {
            // Au repos, le pilote peut réduire la liaison pour économiser
            // l'énergie : on juge sur celle mesurée pendant une partie.
            let measured = src.pcie_in_game.iter().find(|game| game.slot == gpu.pci_slot);
            let in_game = measured.map(|game| game.width);
            let current = in_game.unwrap_or(link.current);
            let sides = format!(
                "{} x{}, {} {}",
                t("carte", "card"),
                link.card,
                t("emplacement", "slot"),
                link.slot.map_or_else(|| "?".to_string(), |slot| format!("x{slot}"))
            );
            facts.push((
                format!("{} ({})", t("Liaison PCI Express", "PCI Express link"), gpu.card),
                match measured {
                    Some(game) if game.width != link.current => t(
                        &format!("x{} au repos, {} en jeu ({sides})", link.current, link_text(game.width, game.speed)),
                        &format!("x{} at rest, {} in game ({sides})", link.current, link_text(game.width, game.speed)),
                    ),
                    Some(game) => t(
                        &format!("{}, même en jeu ({sides})", link_text(game.width, game.speed)),
                        &format!("{}, even in game ({sides})", link_text(game.width, game.speed)),
                    ),
                    None => t(
                        &format!("x{} ({sides} ; pas encore mesurée en jeu)", link.current),
                        &format!("x{} ({sides}; not measured in game yet)", link.current),
                    ),
                },
            ));
            let possible = link.slot.map_or(link.card, |slot| slot.min(link.card));
            if laptop {
                // Carte soudée : rien ne se règle, mais le joueur le sait,
                // avec ce que ça coûte (vitesse mesurée en jeu).
                if let Some(game) = measured.filter(|game| game.width < possible) {
                    let cost = match game.speed {
                        Some(gts) if gts >= 15.9 => t(
                            " D'après les tests publiés, l'écart est faible en PCIe 4.0 : de l'ordre de 1 %.",
                            " According to published tests, the gap is small at PCIe 4.0: around 1 %.",
                        ),
                        Some(gts) if gts >= 7.9 => t(
                            " D'après les tests publiés, l'écart est de quelques % en PCIe 3.0 (environ 4 %).",
                            " According to published tests, the gap is a few % at PCIe 3.0 (around 4 %).",
                        ),
                        _ => String::new(),
                    };
                    checks.push(check(
                        "pcie",
                        Level::Info,
                        t(
                            &format!("Carte graphique en x{} : câblage du portable", game.width),
                            &format!("Graphics card at x{}: the laptop's wiring", game.width),
                        ),
                        t(
                            &format!(
                                "Même en jeu, la liaison reste {} : le fabricant du portable n'a relié la carte que par {} voies (souvent pour garder des voies à un second SSD). Rien ne se règle, ni depuis le PC ni dans le BIOS.{cost}",
                                link_text(game.width, game.speed),
                                game.width
                            ),
                            &format!(
                                "Even in game, the link stays {}: the laptop maker wired the card with only {} lanes (often to keep lanes for a second SSD). Nothing can be set, neither from the PC nor in the BIOS.{cost}",
                                link_text(game.width, game.speed),
                                game.width
                            ),
                        ),
                    ));
                }
            } else if current < possible && in_game.is_none() {
                // Liaison réduite au repos par le pilote AMD : pas un défaut.
                if !amd_idle_link(&device) {
                    checks.push(check(
                        "pcie",
                        Level::Info,
                        t(
                            &format!("Carte graphique en x{current} au repos"),
                            &format!("Graphics card at x{current} at rest"),
                        ),
                        t(
                            &format!(
                                "La carte et son emplacement savent faire x{possible}. Au repos, le pilote réduit parfois la liaison pour économiser l'énergie : Last1 Panel la mesurera pendant votre prochaine partie. Si elle reste en x{current}, ce sera un problème de branchement."
                            ),
                            &format!(
                                "The card and its slot can do x{possible}. At rest, the driver sometimes narrows the link to save power: Last1 Panel will measure it during your next game. If it stays at x{current}, it will be a connection problem."
                            ),
                        ),
                    ));
                }
            } else if current < possible {
                // x4 ou moins : les jeux le sentent nettement.
                let level = if current * 4 <= possible { Level::Problem } else { Level::Warning };
                checks.push(check(
                    "pcie",
                    level,
                    t(
                        &format!("Carte graphique branchée en x{current} au lieu de x{possible}"),
                        &format!("Graphics card linked at x{current} instead of x{possible}"),
                    ),
                    t(
                        &format!(
                            "La carte et son emplacement savent faire x{possible}, mais en jeu la liaison reste en x{current} : elle reçoit ses données moins vite. Rien ne se règle depuis le PC, c'est le branchement : carte pas tout à fait enfoncée, rallonge (riser), ou emplacement qui partage ses voies avec un SSD M.2 (voir le manuel de la carte mère). PC éteint, renfoncez la carte jusqu'au clic, sans rallonge."
                        ),
                        &format!(
                            "The card and its slot can do x{possible}, but in game the link stays at x{current}: it receives its data more slowly. Nothing can be set from the PC, it's the connection: card not fully seated, riser, or a slot sharing its lanes with an M.2 SSD (see the motherboard manual). With the PC off, push the card back in until it clicks, without a riser."
                        ),
                    ),
                ));
            } else if link.slot.is_some_and(|slot| slot < link.card) {
                // PC fixe : emplacement câblé plus étroit que la carte.
                let slot = link.slot.unwrap_or_default();
                checks.push(check(
                    "pcie",
                    Level::Warning,
                    t(
                        &format!("Carte graphique dans un emplacement x{slot}"),
                        &format!("Graphics card in an x{slot} slot"),
                    ),
                    t(
                        &format!(
                            "La carte sait faire x{}, mais son emplacement est câblé en x{slot} : elle reçoit ses données moins vite. PC éteint, mettez-la dans l'emplacement du haut (le plus proche du processeur), câblé en x16.",
                            link.card
                        ),
                        &format!(
                            "The card can do x{}, but its slot is wired x{slot}: it receives its data more slowly. With the PC off, move it to the top slot (closest to the CPU), wired x16.",
                            link.card
                        ),
                    ),
                ));
            }
        }
        if let Some((bar, largest)) = largest_prefetchable_bar(&device.join("resource")) {
            let mib = largest / (1024 * 1024);
            // Tailles que la carte accepte pour cette fenêtre (bit n : 2^n Mo,
            // lisible par tous depuis le noyau 6.1) ; sans ce fichier, elle ne
            // se redimensionne pas (GeForce RTX 20 et plus anciennes) : rien à
            // régler. Noyau plus ancien : on ne sait pas, l'alerte reste.
            let resizable = match read(&device.join(format!("resource{bar}_resize"))) {
                Some(hex) => u64::from_str_radix(hex.trim_start_matches("0x"), 16)
                    .ok()
                    .filter(|sizes| *sizes != 0)
                    .map(|sizes| 1u64 << (63 - sizes.leading_zeros()))
                    .is_some_and(|max_mib| max_mib > mib),
                None => kernel.is_some_and(|version| version < (6, 1)),
            };
            checks.push(if mib <= 256 && !resizable {
                check(
                    "rebar",
                    Level::Ok,
                    t("Resizable BAR : rien à régler", "Resizable BAR: nothing to set"),
                    t("Cette carte graphique ne le prend pas en charge.", "This graphics card doesn't support it."),
                )
            } else if mib <= 256 {
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
        // Sans pacman, on ne sait pas ce que proposent les dépôts : une
        // GeForce RTX sous la branche 590 (les GTX 10 et plus anciennes
        // restent normalement sur la 580).
        if gpu.vendor == Vendor::Nvidia
            && !src.pacman
            && gpu.name.as_deref().is_some_and(|name| name.contains("RTX"))
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

    // Pilotes, d'après pacman : ceux de Vulkan (sans eux, les jeux Proton ne
    // démarrent pas, ou pas en 32 bits), une version plus récente dans les
    // dépôts, et un pilote NVIDIA installé mais pas encore chargé.
    if src.pacman {
        let vulkan = packages::vulkan_packages(src.gpus);
        let missing = packages::missing(src.exec, &vulkan);
        if missing.is_empty() && !vulkan.is_empty() {
            checks.push(check(
                "vulkan",
                Level::Ok,
                t("Pilotes Vulkan installés", "Vulkan drivers installed"),
                t(
                    &format!("En 64 et 32 bits, pour les jeux Proton ({}).", vulkan.join(", ")),
                    &format!("In 64 and 32 bits, for Proton games ({}).", vulkan.join(", ")),
                ),
            ));
        }
        if !missing.is_empty() {
            let only_32 = missing.iter().all(|name| name.starts_with("lib32-"));
            checks.push(Check {
                fix: Some(Fix::VulkanDriver),
                ..check(
                    "vulkan",
                    if only_32 { Level::Warning } else { Level::Problem },
                    if only_32 {
                        t("Pilote Vulkan 32 bits absent", "32-bit Vulkan driver missing")
                    } else {
                        t("Pilote Vulkan absent", "Vulkan driver missing")
                    },
                    if only_32 {
                        t(
                            &format!(
                                "Les jeux 32 bits (et une partie de Steam) ne démarrent pas sans lui ({}).",
                                missing.join(", ")
                            ),
                            &format!(
                                "32-bit games (and part of Steam) don't start without it ({}).",
                                missing.join(", ")
                            ),
                        )
                    } else {
                        t(
                            &format!(
                                "Proton traduit DirectX en Vulkan : sans lui, les jeux Windows ne démarrent pas ({}).",
                                missing.join(", ")
                            ),
                            &format!(
                                "Proton translates DirectX to Vulkan: without it, Windows games don't start ({}).",
                                missing.join(", ")
                            ),
                        )
                    },
                )
            });
        }
        let driver_updates: Vec<&str> =
            src.updates.iter().map(String::as_str).filter(|name| packages::is_driver(name)).collect();
        if driver_updates.is_empty() && !vulkan.is_empty() {
            // Celui de la carte des jeux : NVIDIA, sinon AMD hors puce intégrée.
            let driver = src
                .gpus
                .iter()
                .find(|g| g.vendor == Vendor::Nvidia)
                .or_else(|| src.gpus.iter().find(|g| g.vendor == Vendor::Amd && !apus.contains(&g.pci_slot)))
                .or_else(|| src.gpus.iter().find(|g| g.vendor == Vendor::Amd))
                .map(|g| match (&g.driver, &g.driver_version) {
                    (Some(name), Some(version)) => format!("{name} {version}"),
                    (Some(name), None) => name.clone(),
                    _ => String::new(),
                })
                .unwrap_or_default();
            checks.push(check(
                "driver-update",
                Level::Ok,
                t("Pilote graphique à jour", "Graphics driver up to date"),
                if src.exec.which("checkupdates").is_some() {
                    t(
                        &format!("{driver} : la version la plus récente des dépôts."),
                        &format!("{driver}: the newest version in the repositories."),
                    )
                } else {
                    t(
                        &format!("{driver} : le plus récent d'après les listes de paquets du PC (sans pacman-contrib, pas vérifié en ligne)."),
                        &format!("{driver}: the newest according to the PC's package lists (without pacman-contrib, not checked online)."),
                    )
                },
            ));
        }
        if !driver_updates.is_empty() {
            checks.push(Check {
                fix: (!src.go_to_linux).then_some(Fix::SystemUpdate),
                ..check(
                    "driver-update",
                    Level::Warning,
                    t("Pilote graphique pas à jour", "Graphics driver not up to date"),
                    if src.go_to_linux {
                        // go-to-linux la propose lui-même (bulle « Mettre à
                        // jour ») : pas deux propositions pour la même chose.
                        t(
                            &format!("Une version plus récente est dans les dépôts ({}) : go-to-linux la propose dans sa bulle « Mettre à jour ».", driver_updates.join(", ")),
                            &format!("A newer version is in the repositories ({}): go-to-linux offers it in its “Update” bubble.", driver_updates.join(", ")),
                        )
                    } else {
                        t(
                            &format!("Une version plus récente est dans les dépôts ({}). Elle s'installe avec le reste du système, comme le veut Arch (jamais à moitié).", driver_updates.join(", ")),
                            &format!("A newer version is in the repositories ({}). It installs with the rest of the system, as Arch requires (never halfway).", driver_updates.join(", ")),
                        )
                    },
                )
            });
        }
        if let Some(running) =
            src.gpus.iter().find(|g| g.vendor == Vendor::Nvidia).and_then(|g| g.driver_version.clone())
            && let Some(installed) = packages::installed_version(src.exec, "nvidia-utils")
            && installed != running
        {
            // Sous go-to-linux, c'est lui qui propose « Redémarrer maintenant ».
            checks.push(Check {
                fix: (!src.go_to_linux).then_some(Fix::Restart),
                ..check(
                    "driver-restart",
                    Level::Info,
                    t(&format!("Pilote NVIDIA {installed} installé"), &format!("NVIDIA driver {installed} installed")),
                    if src.go_to_linux {
                        t(
                            &format!("Le PC utilise encore le {running} : il prendra le nouveau au redémarrage, que go-to-linux propose."),
                            &format!("The PC still uses {running}: it will load the new one at the restart go-to-linux offers."),
                        )
                    } else {
                        t(
                            &format!("Le PC utilise encore le {running} : il prendra le nouveau au redémarrage."),
                            &format!("The PC still uses {running}: it will load the new one when it restarts."),
                        )
                    },
                )
            });
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
        // MangoHud écrit son journal dès les premières images : deux minutes
        // après le lancement sans journal, la mesure n'a pas pris.
        _ if let Some(last) = src.last_launch.as_ref().filter(|l| l.measured && !l.logged && l.seconds >= 120) => check(
            "measure",
            Level::Warning,
            t("Dernière partie non mesurée", "Last game not measured"),
            t(
                &format!(
                    "{} est passé par Last1 Panel ({}), mais MangoHud n'a rien enregistré : il ne mesure que les jeux qui affichent par Vulkan ou OpenGL (un lanceur seul n'en fait pas partie).",
                    last.name,
                    ago(last.seconds)
                ),
                &format!(
                    "{} went through Last1 Panel ({}), but MangoHud recorded nothing: it only measures games that draw with Vulkan or OpenGL (a launcher alone does not).",
                    last.name,
                    ago(last.seconds)
                ),
            ),
        ),
        // Tout est prêt, mais « mesurées » seulement quand une partie l'a été.
        _ if src.sessions.is_empty() => check(
            "measure",
            Level::Info,
            t("Aucune partie mesurée pour l'instant", "No game measured yet"),
            match &src.last_launch {
                Some(last) => t(
                    &format!(
                        "{} est passé par Last1 Panel ({}) : MangoHud l'enregistre sans rien afficher, sa partie apparaîtra ici après une minute de jeu.",
                        last.name,
                        ago(last.seconds)
                    ),
                    &format!(
                        "{} went through Last1 Panel ({}): MangoHud records it without showing anything, the game will appear here after one minute of play.",
                        last.name,
                        ago(last.seconds)
                    ),
                ),
                None => t(
                    "Aucun jeu n'est encore passé par Last1 Panel. MangoHud enregistrera le prochain (une minute au moins), sans rien afficher : il apparaîtra ici.",
                    "No game has gone through Last1 Panel yet. MangoHud will record the next one (one minute at least), without showing anything: it will appear here.",
                ),
            },
        ),
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
    }
    // Mémoire d'échange : zram (mémoire compressée, en RAM : CachyOS y range
    // exprès les pages froides pour laisser la RAM au jeu) ou disque.
    // /proc/swaps, en Kio : nom, type, taille, utilisé, priorité.
    let (mut zram_kib, mut disk_kib, mut zram_on) = (0u64, 0u64, false);
    for line in read(&src.proc_dir.join("swaps")).unwrap_or_default().lines().skip(1) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let used = fields.get(3).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
        if fields.first().is_some_and(|name| name.starts_with("/dev/zram")) {
            zram_on = true;
            zram_kib += used;
        } else {
            disk_kib += used;
        }
    }
    let gib = |kib: u64| (kib + 524_288) / 1_048_576;
    if zram_kib >= 1_048_576 {
        facts.push((t("Mémoire compressée (zram)", "Compressed memory (zram)"), format!("{} Go", gib(zram_kib))));
    }
    // zram en service : le disque ne sert que quand il est plein (ou garde
    // ce qui y était avant) ; rien à faire, un fait.
    if disk_kib > 1_048_576 && zram_on {
        facts.push((t("Mémoire d'échange sur le disque", "Swap on disk"), format!("{} Go", gib(disk_kib))));
    } else if disk_kib > 1_048_576 {
        let used = gib(disk_kib);
        checks.push(Check {
            fix: src.can_zram.then_some(Fix::Zram),
            ..check(
                "swap",
                Level::Info,
                t("Conseil : mémoire bien remplie pendant les parties", "Tip: memory is quite full while playing"),
                t(
                    &format!("{used} Go de programmes en arrière-plan sont sur le disque pour laisser la mémoire au jeu. La mémoire compressée (zram, comme CachyOS) les garderait en mémoire, bien plus vite à reprendre."),
                    &format!("{used} GB of background programs are on the disk to leave memory to the game. Compressed memory (zram, like CachyOS) would keep them in memory, much faster to bring back."),
                ),
            )
        });
    }

    // Steam compile les shaders en arrière-plan.
    if let Some(precache) = &src.precache {
        let percent = precache
            .done
            .zip(precache.total)
            .filter(|(_, total)| *total > 0)
            .map(|(done, total)| format!(" ({} %)", u64::from(done) * 100 / u64::from(total)))
            .unwrap_or_default();
        checks.push(check(
            "shader-precache",
            Level::Warning,
            match &precache.game {
                Some(game) => t(
                    &format!("Steam compile les shaders de {game}{percent}"),
                    &format!("Steam is compiling the shaders of {game}{percent}"),
                ),
                None => t(
                    &format!("Steam compile des shaders{percent}"),
                    &format!("Steam is compiling shaders{percent}"),
                ),
            },
            t(
                &format!(
                    "C'est Steam (« Traitement des shaders Vulkan »), après une mise à jour d'un jeu ou du pilote : il occupe le processeur, les jeux peuvent saccader jusqu'à la fin. Ensuite, ils tournent mieux.{}",
                    if precache.readable { "" } else { " (Avancement illisible par Last1 Panel.)" }
                ),
                &format!(
                    "It's Steam (“Vulkan shader processing”), after a game or driver update: it keeps the CPU busy, games may stutter until it ends. Afterwards, they run better.{}",
                    if precache.readable { "" } else { " (Progress unreadable by Last1 Panel.)" }
                ),
            ),
        ));
    }

    checks.sort_by_key(|check| check.level);
    let sessions = src.sessions.clone();
    let text = text(&facts, &checks, &sessions);
    Report { checks, facts, sessions, text, go_to_linux: src.go_to_linux }
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
                "loading" => t(
                    "presque que du chargement ou de la compilation de shaders, pas une vraie partie",
                    "almost only loading or shader compilation, not a real game",
                ),
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
/// processeur), numéro et taille : fichier `resource` de sysfs, une ligne
/// « début fin drapeaux » par BAR.
fn largest_prefetchable_bar(path: &Path) -> Option<(usize, u64)> {
    const PREFETCH: u64 = 0x2000;
    let hex = |s: &str| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok();
    read(path)?
        .lines()
        .take(6)
        .enumerate()
        .filter_map(|(bar, line)| {
            let mut parts = line.split_whitespace();
            let (start, end, flags) = (hex(parts.next()?)?, hex(parts.next()?)?, hex(parts.next()?)?);
            (end > start && flags & PREFETCH != 0).then(|| (bar, end - start + 1))
        })
        .max_by_key(|(_, size)| *size)
}

/// Ce qui empêche ntsync de se charger au démarrage : un fichier de
/// modprobe.d (« blacklist ntsync », « install ntsync … »), ou la ligne de
/// commande du noyau (module_blacklist=, modprobe.blacklist=).
fn ntsync_blocker(dirs: &[&Path], cmdline: &str) -> Option<String> {
    for dir in dirs {
        let mut files: Vec<_> = fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).collect();
        files.sort();
        for file in files {
            let text = fs::read_to_string(&file).unwrap_or_default();
            let blocks = text.lines().any(|line| {
                let words: Vec<&str> = line.split_whitespace().collect();
                matches!(words.as_slice(), ["blacklist", "ntsync", ..] | ["install", "ntsync", ..])
            });
            if blocks {
                return Some(file.display().to_string());
            }
        }
    }
    cmdline
        .split_whitespace()
        .filter_map(|arg| arg.strip_prefix("module_blacklist=").or_else(|| arg.strip_prefix("modprobe.blacklist=")))
        .any(|list| list.split(',').any(|name| name == "ntsync"))
        .then(|| t("la ligne de commande du noyau", "the kernel command line"))
}

/// Service qui fait tourner l'ordonnanceur sched_ext (processus « scx_… »),
/// d'après son cgroup : « scx_loader.service », ou le chemin entier s'il
/// n'est pas un service du système.
fn scheduler_unit(proc_dir: &Path) -> Option<String> {
    fs::read_dir(proc_dir).into_iter().flatten().flatten().find_map(|entry| {
        let numeric = entry.file_name().to_str().is_some_and(|n| n.chars().all(|c| c.is_ascii_digit()));
        let comm = read(&entry.path().join("comm"))?;
        if !numeric || !comm.starts_with("scx_") || comm == "scx_loader" {
            return None;
        }
        let cgroup = read(&entry.path().join("cgroup"))?;
        let path = cgroup.lines().find_map(|line| line.strip_prefix("0::"))?.to_string();
        Some(match path.strip_prefix("/system.slice/").and_then(|rest| rest.split('/').next()) {
            Some(unit) if unit.ends_with(".service") => unit.to_string(),
            _ => format!("{comm} ({path})"),
        })
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
        write(pc.proc_dir.join("meminfo"), "MemTotal: 32768000 kB\n");
        // Échange sur disque seul, 3 Go utilisés : conseil, et zram d'un clic.
        write(
            pc.proc_dir.join("swaps"),
            "Filename\tType\tSize\tUsed\tPriority\n/swap/swapfile                          file\t8388604\t3145728\t-2\n",
        );
        write(pc.proc_dir.join("sys/kernel/split_lock_mitigate"), "1\n");
        write(pc.sys.join("kernel/sched_ext/state"), "enabled\n");
        write(pc.sys.join("kernel/sched_ext/root/ops"), "lavd\n");
        write(pc.sys.join("bus/platform/drivers/amd_x3d_vcache/AMDI0101:00/amd_x3d_mode"), "frequency\n");
        let device = pc.sys.join("bus/pci/devices/0000:01:00.0");
        write(device.join("current_link_width"), "4\n");
        write(device.join("max_link_width"), "16\n");
        // BAR0 (registres), BAR1 de 256 Mo préchargeable, que la carte
        // pourrait passer à 16 Go : Resizable BAR désactivé dans le BIOS.
        write(
            device.join("resource"),
            "0x00000000fb000000 0x00000000fbffffff 0x0000000000040200\n0x000000e000000000 0x000000e00fffffff 0x000000000014220c\n",
        );
        write(device.join("resource1_resize"), "0000000000007fc0\n");
        let mut settings = Settings::default();
        settings.games.global.low_latency = LowLatency::On;
        let exec = FakeExec::new().on("powerprofilesctl", "balanced\n");
        let report = run(&Sources {
            sys: &pc.sys,
            dev: &pc.dev,
            proc_dir: &pc.proc_dir,
            exec: &exec,
            env: &|_| None,
            next_login_env: &|_| None,
            settings: &settings,
            gpus: &[gpu("580.95.05")],
            desktop: Desktop::Hyprland,
            default_proton: Some("proton-cachyos-slr".into()),
            recent_games: vec!["ARC Raiders (proton_experimental)".into()],
            last_launch: None,
            version: "0.1.0.r56".into(),
            go_to_linux_version: Some("2026.10.19-cf9724f".into()),
            pcie_in_game: vec![GameLink { slot: "0000:01:00.0".into(), width: 4, speed: None }],
            gamemode_allowed: true,
            precache: Some(crate::games::shaders::Precache {
                game: Some("ARC Raiders".into()),
                done: Some(42),
                total: Some(100),
                readable: true,
                processes: 4,
                cpu_ticks: 120_000,
                running_secs: Some(1800),
            }),
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
            go_to_linux: true,
            can_zram: true,
            pacman: false,
            updates: Vec::new(),
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
                "shader-precache"
            ]
        );
        assert_eq!(ids(Level::Info), ["scheduler", "swap"]);
        let fixes: Vec<Fix> = report.checks.iter().filter_map(|c| c.fix).collect();
        assert_eq!(
            fixes,
            [
                Fix::X3dCache,
                Fix::Performance,
                Fix::SplitLock,
                Fix::Ntsync,
                Fix::ShaderCache,
                Fix::InstallMangoHud,
                Fix::LowLatency,
                Fix::Scheduler,
                Fix::Zram
            ]
        );
        assert!(report.text.contains("AMD Ryzen 9 7950X3D") && report.text.contains("balanced"));
        // Versions en tête : on voit tout de suite si le PC est à jour.
        assert!(
            report.text.starts_with(
                "Last1 Panel — diagnostic des performances\n\nLast1 Panel : 0.1.0.r56\ngo-to-linux : 2026.10.19-cf9724f\n"
            ),
            "{}",
            report.text
        );
        assert!(report.checks.iter().any(|c| c.title == "Steam compile les shaders de ARC Raiders (42 %)"));
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
        let mut sources = Sources {
            sys: &pc.sys,
            dev: &pc.dev,
            proc_dir: &pc.proc_dir,
            exec: &exec,
            env: &env,
            next_login_env: &|_| None,
            settings: &Settings::default(),
            gpus: &[gpu("595.58.03")],
            desktop: Desktop::Plasma,
            default_proton: None,
            recent_games: Vec::new(),
            last_launch: Some(LastLaunch {
                name: "Cyberpunk 2077".into(),
                seconds: 3 * 3600,
                measured: true,
                logged: true,
            }),
            version: "0.1.0.r56".into(),
            go_to_linux_version: None,
            pcie_in_game: Vec::new(),
            gamemode_allowed: true,
            precache: None,
            sessions: vec![GameSession {
                program: "Cyberpunk2077.exe".into(),
                started: "2026-10-18 21:04".into(),
                minutes: 75.0,
                fps_average: 96.0,
                fps_one_percent_low: 71.0,
                cpu_load: 38.0,
                gpu_load: 98.0,
                gpu_clock: 2745.0,
                limit: "gpu",
            }],
            mangohud: true,
            wrapped_games: Some((1, 1)),
            go_to_linux: true,
            can_zram: true,
            pacman: false,
            updates: Vec::new(),
        };
        let report = run(&sources);
        assert!(report.checks.iter().all(|c| c.level == Level::Ok), "{:#?}", report.checks);
        assert!(report.checks.iter().any(|c| c.id == "rebar" && c.detail.contains("16384")));
        assert!(report.text.contains("Cyberpunk 2077, il y a 3 h"), "{}", report.text);
        // Lancé par Last1 Panel il y a 5 min, sans journal de MangoHud : dit.
        sources.last_launch =
            Some(LastLaunch { name: "ARC Raiders".into(), seconds: 300, measured: true, logged: false });
        let measure = run(&sources).checks.into_iter().find(|c| c.id == "measure").unwrap();
        assert_eq!(measure.level, Level::Warning);
        assert!(measure.detail.contains("ARC Raiders") && measure.detail.contains("il y a 5 min"));
        // Le jeu vient de démarrer : pas encore d'alerte.
        sources.last_launch =
            Some(LastLaunch { name: "ARC Raiders".into(), seconds: 30, measured: true, logged: false });
        assert!(run(&sources).checks.iter().all(|c| c.level == Level::Ok));
    }

    #[test]
    fn each_fixable_point_has_its_button() {
        // Arch Linux, GeForce RTX 2060 : BAR1 de 256 Mo, que la carte ne
        // sait pas agrandir (pas de resource1_resize).
        let pc = pc();
        write(pc.sys.join("kernel/sched_ext/state"), "enabled\n");
        write(pc.sys.join("kernel/sched_ext/root/ops"), "lavd_1.1.3_x86_64_unknown_linux_gnu\n");
        write(pc.proc_dir.join("777/comm"), "scx_lavd\n");
        write(pc.proc_dir.join("777/cgroup"), "0::/system.slice/scx-lavd-maison.service\n");
        write(pc.sys.join("devices/system/cpu/cpu0/cpufreq/scaling_governor"), "powersave\n");
        write(pc.proc_dir.join("sys/kernel/split_lock_mitigate"), "1\n");
        let device = pc.sys.join("bus/pci/devices/0000:01:00.0");
        write(
            device.join("resource"),
            "0x00000000fb000000 0x00000000fbffffff 0x0000000000040200\n0x000000e000000000 0x000000e00fffffff 0x000000000014220c\n",
        );
        let exec = FakeExec::new().without("powerprofilesctl").without("gamemoderun");
        let report = |next_login: Option<&str>| {
            let next_login = next_login.map(str::to_string);
            run(&Sources {
                sys: &pc.sys,
                dev: &pc.dev,
                proc_dir: &pc.proc_dir,
                exec: &exec,
                env: &|_| None,
                next_login_env: &|_| next_login.clone(),
                settings: &Settings::default(),
                gpus: &[Gpu { name: Some("GeForce RTX 2060 Rev. A".into()), ..gpu("615.71.09") }],
                desktop: Desktop::Hyprland,
                default_proton: None,
                recent_games: Vec::new(),
                last_launch: None,
                version: "0.1.0.r56".into(),
                go_to_linux_version: None,
                pcie_in_game: Vec::new(),
                gamemode_allowed: true,
                precache: None,
                sessions: Vec::new(),
                mangohud: true,
                wrapped_games: Some((1, 1)),
                go_to_linux: false,
                can_zram: true,
                pacman: false,
                updates: Vec::new(),
            })
        };
        let arch = report(None);
        let fix = |id: &str| arch.checks.iter().find(|c| c.id == id).unwrap().fix;
        assert_eq!(fix("ntsync"), Some(Fix::Ntsync));
        assert_eq!(fix("shader-cache"), Some(Fix::ShaderCache));
        assert_eq!(fix("scheduler"), Some(Fix::Scheduler));
        // Le service qui le lance, pour le rapport.
        assert!(arch.facts.iter().any(|(_, value)| value == "scx-lavd-maison.service"), "{:?}", arch.facts);
        assert_eq!(fix("split-lock"), Some(Fix::SplitLock));
        // PC fixe sans power-profiles-daemon : le gouverneur, d'un clic.
        assert_eq!(fix("power"), Some(Fix::Performance));
        // Aucune commande à taper.
        assert!(arch.checks.iter().all(|c| !c.detail.contains("sudo")), "{:#?}", arch.checks);
        // Portable sans power-profiles-daemon : pas de mode performance
        // permanent.
        write(pc.sys.join("class/power_supply/BAT0/type"), "Battery\n");
        assert_eq!(report(None).checks.iter().find(|c| c.id == "power").unwrap().fix, None);
        // Cache posé pour la prochaine connexion : plus de bouton, la consigne.
        let pending = report(Some("12000000000"));
        let cache = pending.checks.iter().find(|c| c.id == "shader-cache").unwrap();
        assert_eq!((cache.level, cache.fix), (Level::Info, Some(Fix::Restart)));
        // CachyOS : 6 Go dans zram (en mémoire) ; rien à signaler, un fait.
        write(
            pc.proc_dir.join("swaps"),
            "Filename\tType\tSize\tUsed\tPriority\n/dev/zram0 partition\t16318460\t6291456\t100\n/swap/swapfile file\t8388604\t0\t10\n",
        );
        let cachyos = report(None);
        assert!(cachyos.checks.iter().all(|c| c.id != "swap"));
        assert!(cachyos.facts.iter().any(|(name, value)| name.contains("zram") && value == "6 Go"));
        // zram en service, et ce qui était déjà sur le disque y reste : plus
        // rien à faire (le bouton a marché), un fait.
        write(
            pc.proc_dir.join("swaps"),
            "Filename\tType\tSize\tUsed\tPriority\n/dev/zram0 partition\t16318460\t0\t100\n/swap/swapfile file\t8388604\t7340032\t-2\n",
        );
        let after = report(None);
        assert!(after.checks.iter().all(|c| c.id != "swap"));
        assert!(after.facts.iter().any(|(name, value)| name.contains("disque") && value == "7 Go"));
        // La carte ne se redimensionne pas : rien à régler dans le BIOS.
        let rebar = arch.checks.iter().find(|c| c.id == "rebar").unwrap();
        assert_eq!(rebar.level, Level::Ok);
        // Noyau d'avant 6.1, sans ce fichier pour aucune carte : l'alerte reste.
        write(pc.proc_dir.join("sys/kernel/osrelease"), "5.15.0-91-generic\n");
        assert_eq!(report(None).checks.iter().find(|c| c.id == "rebar").unwrap().level, Level::Warning);
        write(pc.proc_dir.join("sys/kernel/osrelease"), "7.2.7-arch1-1\n");
        assert_eq!(report(None).checks.iter().find(|c| c.id == "rebar").unwrap().level, Level::Ok);
        // Jusqu'à 256 Mo seulement : pareil.
        write(device.join("resource1_resize"), "00000000000001c0\n");
        assert_eq!(report(None).checks.iter().find(|c| c.id == "rebar").unwrap().level, Level::Ok);
    }

    #[test]
    fn drivers_are_checked_against_the_repositories() {
        use crate::exec::Output;
        let pc = pc();
        let out = |status: i32, stdout: &str| Output { status, stdout: stdout.into(), ..Default::default() };
        let exec = FakeExec::new()
            .without("powerprofilesctl")
            .on_args("pacman", &["-T", "nvidia-utils", "lib32-nvidia-utils"], out(127, "lib32-nvidia-utils\n"))
            .on_args("pacman", &["-Q", "nvidia-utils"], out(0, "nvidia-utils 600.10-1\n"));
        let report = run(&Sources {
            sys: &pc.sys,
            dev: &pc.dev,
            proc_dir: &pc.proc_dir,
            exec: &exec,
            env: &|_| None,
            next_login_env: &|_| None,
            settings: &Settings::default(),
            // Pilote 580 chargé : avec pacman, plus de seuil fixe (une GTX 10
            // reste à bon droit sur la branche 580).
            gpus: &[gpu("580.95.05")],
            desktop: Desktop::Hyprland,
            default_proton: None,
            recent_games: Vec::new(),
            last_launch: None,
            version: "0.1.0.r56".into(),
            go_to_linux_version: None,
            pcie_in_game: Vec::new(),
            gamemode_allowed: true,
            precache: None,
            sessions: Vec::new(),
            mangohud: true,
            wrapped_games: Some((1, 1)),
            go_to_linux: false,
            can_zram: true,
            pacman: true,
            updates: vec!["firefox".into(), "nvidia-utils".into(), "lib32-nvidia-utils".into()],
        });
        let find = |id: &str| report.checks.iter().find(|c| c.id == id);
        assert!(find("driver").is_none());
        let vulkan = find("vulkan").unwrap();
        assert_eq!((vulkan.level, vulkan.fix), (Level::Warning, Some(Fix::VulkanDriver)));
        assert!(vulkan.detail.contains("lib32-nvidia-utils"));
        let update = find("driver-update").unwrap();
        assert_eq!((update.level, update.fix), (Level::Warning, Some(Fix::SystemUpdate)));
        assert!(update.detail.contains("nvidia-utils, lib32-nvidia-utils") && !update.detail.contains("firefox"));
        // 600.10 installé, 580.95.05 chargé : redémarrer.
        let restart = find("driver-restart").unwrap();
        assert_eq!((restart.level, restart.fix), (Level::Info, Some(Fix::Restart)));
        assert!(!report.text.contains("go-to-linux"));
    }

    #[test]
    fn gamemode_needs_its_group() {
        // Arch, PC fixe sans power-profiles-daemon, gouverneur « powersave ».
        let pc = pc();
        write(pc.sys.join("devices/system/cpu/cpu0/cpufreq/scaling_governor"), "powersave\n");
        let gamemode = FakeExec::new().without("powerprofilesctl").without("game-performance");
        let power = |exec: &FakeExec, allowed: bool, battery: bool, pacman: bool| {
            if battery {
                write(pc.sys.join("class/power_supply/BAT0/type"), "Battery\n");
            }
            let report = run(&Sources {
                sys: &pc.sys,
                dev: &pc.dev,
                proc_dir: &pc.proc_dir,
                exec,
                env: &|_| None,
                next_login_env: &|_| None,
                settings: &Settings::default(),
                gpus: &[],
                desktop: Desktop::Plasma,
                default_proton: None,
                recent_games: Vec::new(),
                last_launch: None,
                version: "0.1.0.r56".into(),
                go_to_linux_version: None,
                pcie_in_game: Vec::new(),
                gamemode_allowed: allowed,
                precache: None,
                sessions: Vec::new(),
                mangohud: true,
                wrapped_games: Some((3, 3)),
                go_to_linux: false,
                can_zram: true,
                pacman,
                updates: Vec::new(),
            });
            let check = report.checks.into_iter().find(|c| c.id == "power").unwrap();
            (check.level, check.fix)
        };
        // GameMode installé, compte hors de son groupe : le bouton l'y met.
        assert_eq!(power(&gamemode, false, false, true), (Level::Warning, Some(Fix::GameMode)));
        // Dans le groupe : pleine vitesse le temps des jeux.
        assert_eq!(power(&gamemode, true, false, true).0, Level::Ok);
        // Sans GameMode : mode performance permanent sur un PC fixe…
        let nothing = FakeExec::new().without("powerprofilesctl").without("game-performance").without("gamemoderun");
        assert_eq!(power(&nothing, true, false, true), (Level::Warning, Some(Fix::Performance)));
        // … GameMode installé sur un portable.
        assert_eq!(power(&nothing, true, true, true), (Level::Warning, Some(Fix::GameMode)));
        assert_eq!(power(&nothing, true, true, false), (Level::Warning, None));
    }

    #[test]
    fn pcie_link_is_read_at_the_slot() {
        // Radeon : port du processeur (AMD 1022), commutateur de la carte
        // (1002, deux ponts), puce graphique. La puce voit x16 (interne), la
        // liaison avec l'emplacement est en x8.
        let dir = tempfile::tempdir().unwrap();
        let port = dir.path().join("devices/pci0000:00/0000:00:01.1");
        let up = port.join("0000:01:00.0");
        let down = up.join("0000:02:00.0");
        let gpu = down.join("0000:03:00.0");
        let device = |path: &PathBuf, vendor: &str, current: &str, max: &str| {
            write(path.join("vendor"), &format!("{vendor}\n"));
            write(path.join("current_link_width"), &format!("{current}\n"));
            write(path.join("max_link_width"), &format!("{max}\n"));
        };
        device(&port, "0x1022", "8", "16");
        device(&up, "0x1002", "8", "16");
        device(&down, "0x1002", "16", "16");
        device(&gpu, "0x1002", "16", "16");
        let link = dir.path().join("bus/pci/devices/0000:03:00.0");
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&gpu, &link).unwrap();
        assert_eq!(slot_link(&link, 0x1002), Some(SlotLink { current: 8, card: 16, slot: Some(16), speed: None }));
        // Portable : port du processeur câblé en x8, rien d'anormal.
        device(&port, "0x1022", "8", "8");
        assert_eq!(slot_link(&link, 0x1002), Some(SlotLink { current: 8, card: 16, slot: Some(8), speed: None }));
        // GeForce, sans commutateur : sa propre liaison.
        assert_eq!(slot_link(&gpu, 0x10de), Some(SlotLink { current: 16, card: 16, slot: Some(16), speed: None }));
        // En veille, ou éteinte (elle répond 63) : rien.
        write(gpu.join("current_link_width"), "63\n");
        assert_eq!(slot_link(&gpu, 0x10de), None);
        write(gpu.join("current_link_width"), "16\n");
        write(gpu.join("power/runtime_status"), "suspended\n");
        assert_eq!(slot_link(&gpu, 0x10de), None);
    }

    #[test]
    fn laptop_link_is_only_a_fact() {
        // RTX 3080 Laptop câblée en x8 : un fait, pas de constat.
        let pc = pc();
        let device = pc.sys.join("bus/pci/devices/0000:01:00.0");
        write(device.join("current_link_width"), "8\n");
        write(device.join("max_link_width"), "16\n");
        let exec = FakeExec::new();
        let pcie = |gpu: &Gpu, in_game: Option<(u32, Option<f64>)>| {
            let report = run(&Sources {
                sys: &pc.sys,
                dev: &pc.dev,
                proc_dir: &pc.proc_dir,
                exec: &exec,
                env: &|_| None,
                next_login_env: &|_| None,
                settings: &Settings::default(),
                gpus: std::slice::from_ref(gpu),
                desktop: Desktop::Plasma,
                default_proton: None,
                recent_games: Vec::new(),
                last_launch: None,
                version: "0.1.0.r56".into(),
                go_to_linux_version: None,
                pcie_in_game: in_game
                    .map(|(width, speed)| GameLink { slot: "0000:01:00.0".into(), width, speed })
                    .into_iter()
                    .collect(),
                gamemode_allowed: true,
                precache: None,
                sessions: Vec::new(),
                mangohud: true,
                wrapped_games: Some((1, 1)),
                go_to_linux: true,
                can_zram: true,
                pacman: false,
                updates: Vec::new(),
            });
            let fact = report
                .facts
                .iter()
                .find(|(name, _)| name.starts_with("Liaison PCI Express"))
                .map(|(_, value)| value.clone())
                .unwrap_or_default();
            (report.checks.iter().find(|c| c.id == "pcie").map(|c| c.level), fact)
        };
        // PC fixe en x8 au repos : à confirmer pendant une partie.
        let desktop = gpu("595.58.03");
        assert_eq!(
            pcie(&desktop, None),
            (Some(Level::Info), "x8 (carte x16, emplacement ? ; pas encore mesurée en jeu)".to_string())
        );
        // En jeu, x16 : réduite au repos seulement, rien à dire.
        assert_eq!(
            pcie(&desktop, Some((16, Some(16.0)))),
            (None, "x8 au repos, x16 en PCIe 4.0 en jeu (carte x16, emplacement ?)".to_string())
        );
        // En jeu, toujours x8 : le branchement.
        assert_eq!(
            pcie(&desktop, Some((8, None))),
            (Some(Level::Warning), "x8, même en jeu (carte x16, emplacement ?)".to_string())
        );
        // Portable : câblé ainsi, une information avec ce que ça coûte.
        let laptop = Gpu { name: Some("GeForce RTX 3080 Laptop GPU".into()), ..desktop.clone() };
        assert_eq!(pcie(&laptop, Some((8, Some(16.0)))).0, Some(Level::Info));
        assert_eq!(pcie(&laptop, None).0, None);
        write(pc.sys.join("class/dmi/id/chassis_type"), "10\n");
        assert_eq!(
            pcie(&desktop, Some((8, Some(16.0)))),
            (Some(Level::Info), "x8 en PCIe 4.0, même en jeu (carte x16, emplacement ?)".to_string())
        );
    }

    #[test]
    fn integrated_amd_gpu_is_left_out() {
        // Portable Ryzen (Radeon Vega intégrée, 0000:05:00.0) + GTX 1650.
        let pc = pc();
        for slot in ["0000:01:00.0", "0000:05:00.0"] {
            let device = pc.sys.join("bus/pci/devices").join(slot);
            write(device.join("current_link_width"), "16\n");
            write(device.join("max_link_width"), "16\n");
            write(device.join("resource"), "0x00000000e0000000 0x00000000efffffff 0x000000000014220c\n");
        }
        // Nœud du processeur avec la puce graphique (APU) ; 5 << 8 = 1280.
        write(
            pc.sys.join("class/kfd/kfd/topology/nodes/0/properties"),
            "cpu_cores_count 8\nsimd_count 32\nlocation_id 1280\ndomain 0\n",
        );
        assert_eq!(amd_apus(&pc.sys), ["0000:05:00.0"]);
        let vega = Gpu {
            card: "card0".into(),
            pci_slot: "0000:05:00.0".into(),
            vendor: Vendor::Amd,
            vendor_id: 0x1002,
            device_id: 0x15d8,
            name: Some("Radeon Vega Series / Radeon Vega Mobile Series".into()),
            driver: Some("amdgpu".into()),
            driver_version: None,
            boot_vga: true,
            connectors: vec![],
        };
        let gtx = Gpu { name: Some("GeForce GTX 1650".into()), boot_vga: false, ..gpu("615.78.08") };
        let exec = FakeExec::new();
        let report = run(&Sources {
            sys: &pc.sys,
            dev: &pc.dev,
            proc_dir: &pc.proc_dir,
            exec: &exec,
            env: &|_| None,
            next_login_env: &|_| None,
            settings: &Settings::default(),
            gpus: &[vega, gtx],
            desktop: Desktop::Hyprland,
            default_proton: None,
            recent_games: Vec::new(),
            last_launch: None,
            version: "0.1.0.r61".into(),
            go_to_linux_version: None,
            pcie_in_game: Vec::new(),
            gamemode_allowed: true,
            precache: None,
            sessions: Vec::new(),
            mangohud: true,
            wrapped_games: Some((1, 1)),
            go_to_linux: false,
            can_zram: true,
            pacman: true,
            updates: Vec::new(),
        });
        // Une seule carte vérifiée : la GTX 1650, et son pilote.
        assert_eq!(report.checks.iter().filter(|c| c.id == "rebar").count(), 1);
        assert_eq!(report.facts.iter().filter(|(name, _)| name.starts_with("Liaison PCI Express")).count(), 1);
        assert!(report.checks.iter().any(|c| c.id == "driver-update" && c.detail.starts_with("nvidia 615.78.08")));
        // Aucun nom en double : l'interface les range par nom.
        let mut names: Vec<&String> = report.facts.iter().map(|(name, _)| name).collect();
        names.sort();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count, "{:?}", report.facts);
    }

    #[test]
    fn amd_link_reduced_at_rest_is_not_a_fault() {
        let dir = tempfile::tempdir().unwrap();
        let levels = dir.path().join("pp_dpm_pcie");
        write(levels.clone(), "0: 2.5GT/s, x8 97Mhz *\n1: 16.0GT/s, x16 619Mhz\n");
        assert!(amd_idle_link(dir.path()));
        write(levels.clone(), "0: 2.5GT/s, x8 97Mhz\n1: 16.0GT/s, x16 619Mhz *\n");
        assert!(!amd_idle_link(dir.path()));
        fs::remove_file(levels).unwrap();
        assert!(!amd_idle_link(dir.path()));
    }

    #[test]
    fn gamemode_group_members() {
        let dir = tempfile::tempdir().unwrap();
        let group = dir.path().join("group");
        write(group.clone(), "wheel:x:998:alice\ngamemode:x:961:bob,alice\n");
        assert!(gamemode_allowed(&group, "alice") && !gamemode_allowed(&group, "carol"));
        write(group.clone(), "wheel:x:998:alice\ngamemode:x:961:\n");
        assert!(!gamemode_allowed(&group, "alice"));
        // Pas de groupe gamemode : rien à dire.
        write(group.clone(), "wheel:x:998:alice\n");
        assert!(gamemode_allowed(&group, "alice"));
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
                next_login_env: &|_| None,
                settings: &Settings::default(),
                gpus: &[],
                desktop: Desktop::Plasma,
                default_proton: None,
                recent_games: Vec::new(),
                last_launch: None,
                version: "0.1.0.r56".into(),
                go_to_linux_version: None,
                pcie_in_game: Vec::new(),
                gamemode_allowed: true,
                precache: None,
                sessions: Vec::new(),
                mangohud: true,
                wrapped_games: Some((1, 1)),
                go_to_linux: true,
                can_zram: true,
                pacman: false,
                updates: Vec::new(),
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
        // Ou par Last1 Panel (game-performance), dans les jeux qui passent par lui.
        assert_eq!(power(&FakeExec::new().on("powerprofilesctl", "balanced\n")), Some(Level::Ok));
        let without_tools =
            FakeExec::new().on("powerprofilesctl", "balanced\n").without("game-performance").without("gamemoderun");
        assert_eq!(power(&without_tools), Some(Level::Warning));
        // Rien de lu (machine virtuelle) : pas d'avis.
        assert_eq!(power(&FakeExec::new().without("powerprofilesctl")), None);
    }

    #[test]
    fn reads_the_environment_games_get() {
        let pc = pc();
        write(pc.proc_dir.join("900/comm"), "Hyprland\n");
        write(pc.proc_dir.join("900/environ"), "HOME=/home/j\0__GL_SHADER_DISK_CACHE_SIZE=12000000000\0");
        // Steam lancé avant la reconnexion : sans la variable ; il passe
        // avant le bureau (les jeux héritent de lui).
        write(pc.proc_dir.join("1200/comm"), "steam\n");
        write(pc.proc_dir.join("1200/environ"), "HOME=/home/j\0");
        assert_eq!(game_env(&pc.proc_dir, "__GL_SHADER_DISK_CACHE_SIZE"), None);
        fs::remove_dir_all(pc.proc_dir.join("1200")).unwrap();
        assert_eq!(game_env(&pc.proc_dir, "__GL_SHADER_DISK_CACHE_SIZE").as_deref(), Some("12000000000"));
    }

    #[test]
    fn finds_what_blocks_ntsync_at_boot() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path().join("a/10-sons.conf"), "blacklist pcspkr\n");
        assert_eq!(ntsync_blocker(&[&dir.path().join("a")], "quiet splash"), None);
        write(dir.path().join("b/50-wine.conf"), "# vieux guide\nblacklist ntsync\n");
        let found = ntsync_blocker(&[&dir.path().join("a"), &dir.path().join("b")], "").unwrap();
        assert!(found.ends_with("b/50-wine.conf"), "{found}");
        assert!(ntsync_blocker(&[], "root=/dev/sda2 module_blacklist=nouveau,ntsync").is_some());
        assert!(ntsync_blocker(&[], "modprobe.blacklist=ntsync").is_some());
        assert_eq!(ntsync_blocker(&[], "module_blacklist=nouveau"), None);
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
        assert_eq!(largest_prefetchable_bar(&path), Some((1, 256 * 1024 * 1024)));
        assert_eq!(largest_prefetchable_bar(&dir.path().join("absent")), None);
    }
}
