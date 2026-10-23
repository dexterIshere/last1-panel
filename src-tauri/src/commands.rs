//! Commandes appelées par l'interface (src/lib/api/ipc.ts). Chacune tourne
//! hors du fil de la fenêtre : un outil lent ne fige jamais l'interface.
//! Les erreurs sont des phrases dans la langue choisie, montrées telles quelles.

use last1_panel_core::color::curve::ColorAdjust;
use last1_panel_core::display::Features;
use last1_panel_core::display::model::{Output, OutputChange};
use last1_panel_core::error::CoreError;
use last1_panel_core::exec::{Exec, SystemExec, which};
use last1_panel_core::games::profile::GameProfile;
use last1_panel_core::games::steam::{self, Game};
use last1_panel_core::i18n::{self, t};
use last1_panel_core::panel::{ColorState, Panel};
use last1_panel_core::settings::{ColorProfile, GamesSettings, Settings};
use last1_panel_core::system::desktop::Desktop;
use last1_panel_core::system::diagnostic::{Fix, Report};
use last1_panel_core::system::gpu::{Gpu, Vendor};
use last1_panel_core::system::repair;
use last1_panel_core::system::sharing;
use last1_panel_core::tuning::lact::{self, ClockRanges, Device, Lact, OffsetRange, Stats, Tuning};
use last1_panel_core::update;
use serde::Serialize;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tauri::State;

type Result<T> = std::result::Result<T, String>;

/// Lance `work` hors du fil de la fenêtre.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> std::result::Result<T, CoreError> + Send + 'static,
) -> Result<T> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tools {
    nvibrant: bool,
    lact: bool,
    lact_running: bool,
    /// LACT installable par le bouton (pacman : Arch, CachyOS).
    pacman: bool,
    mangohud: bool,
    dlss_swapper: bool,
    steam: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    version: String,
    lang: &'static str,
    desktop: Desktop,
    features: Option<Features>,
    gpus: Vec<Gpu>,
    tools: Tools,
}

#[tauri::command]
pub async fn system_info(panel: State<'_, Arc<Panel>>) -> Result<SystemInfo> {
    let panel = panel.inner().clone();
    blocking(move || {
        Ok(SystemInfo {
            version: last1_panel_core::update::installed_version(panel.exec.as_ref())
                .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string()),
            lang: if i18n::is_english() { "en" } else { "fr" },
            desktop: panel.desktop,
            features: panel.features(),
            gpus: panel.gpus(),
            tools: Tools {
                nvibrant: panel.gpus().iter().any(|g| {
                    last1_panel_core::color::vibrance::program(panel.exec.as_ref(), g.driver_version.as_deref())
                        .is_some()
                }),
                lact: which("lact").is_some(),
                lact_running: lact::find_socket().is_some(),
                pacman: which("pacman").is_some(),
                mangohud: which("mangohud").is_some(),
                dlss_swapper: which("dlss-swapper").is_some(),
                steam: steam::find_root(&last1_panel_core::paths::home_dir()).is_some(),
            },
        })
    })
    .await
}

#[tauri::command]
pub fn set_language(lang: String) {
    i18n::set_english(lang == "en");
}

#[tauri::command]
pub async fn outputs(panel: State<'_, Arc<Panel>>) -> Result<Vec<Output>> {
    let panel = panel.inner().clone();
    blocking(move || panel.outputs()).await
}

#[tauri::command]
pub async fn apply_outputs(panel: State<'_, Arc<Panel>>, changes: Vec<OutputChange>) -> Result<Vec<Output>> {
    let panel = panel.inner().clone();
    blocking(move || {
        panel.apply_outputs(&changes)?;
        // Le bureau applique les écrans juste après avoir répondu.
        std::thread::sleep(Duration::from_millis(400));
        panel.outputs()
    })
    .await
}

#[tauri::command]
pub async fn color_state(panel: State<'_, Arc<Panel>>, output: String) -> Result<ColorState> {
    let panel = panel.inner().clone();
    blocking(move || panel.color_state(&output)).await
}

#[tauri::command]
pub async fn set_color(panel: State<'_, Arc<Panel>>, output: String, adjust: ColorAdjust) -> Result<()> {
    let panel = panel.inner().clone();
    blocking(move || panel.set_color(&output, &adjust)).await
}

#[tauri::command]
pub async fn set_vibrance(panel: State<'_, Arc<Panel>>, output: String, percent: f64) -> Result<()> {
    let panel = panel.inner().clone();
    blocking(move || panel.set_vibrance(&output, percent)).await
}

#[tauri::command]
pub async fn gpu_devices() -> Result<Vec<Device>> {
    blocking(|| Lact::connect()?.devices()).await
}

#[tauri::command]
pub async fn gpu_stats(id: String) -> Result<Stats> {
    blocking(move || Lact::connect()?.stats(&id)).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuTuning {
    tuning: Tuning,
    gpu_offset: Option<OffsetRange>,
    mem_offset: Option<OffsetRange>,
    stats: Stats,
}

#[tauri::command]
pub async fn gpu_tuning(id: String) -> Result<GpuTuning> {
    blocking(move || {
        let lact = Lact::connect()?;
        let ClockRanges { gpu_offset, mem_offset } = lact.clock_ranges(&id).unwrap_or_default();
        Ok(GpuTuning {
            tuning: lact::tuning_from_config(&lact.config(&id)?),
            gpu_offset,
            mem_offset,
            stats: lact.stats(&id)?,
        })
    })
    .await
}

#[tauri::command]
pub async fn gpu_try(panel: State<'_, Arc<Panel>>, id: String, tuning: Tuning) -> Result<u64> {
    let panel = panel.inner().clone();
    blocking(move || {
        let lact = Lact::connect()?;
        let slot = id.rsplit('-').next().unwrap_or_default().to_string();
        let nvidia = panel.gpus().iter().any(|g| g.pci_slot.eq_ignore_ascii_case(&slot) && g.vendor == Vendor::Nvidia);
        let config = lact::apply_tuning(&lact.config(&id)?, &tuning, nvidia);
        lact.try_config(&id, &config)
    })
    .await
}

#[tauri::command]
pub async fn gpu_confirm(keep: bool) -> Result<()> {
    blocking(move || Lact::connect()?.confirm(keep)).await
}

/// Version plus récente publiée sur last1.fun, pour Last1 Panel installé par
/// l'AppImage ou par go-to-linux (un paquet sans go-to-linux : pacman).
#[tauri::command]
pub async fn update_available() -> Result<Option<String>> {
    blocking(|| {
        if !update::enabled() && !update::by_go_to_linux() {
            return Ok(None);
        }
        let exec = SystemExec { timeout: Duration::from_secs(90), ..SystemExec::default() };
        Ok(update::check(&exec, update::now())?.map(|release| release.version))
    })
    .await
}

/// Installe la version plus récente, relance le service de session, puis
/// rouvre Last1 Panel sur la nouvelle version.
#[tauri::command]
pub async fn install_update(app: tauri::AppHandle) -> Result<()> {
    blocking(|| {
        let exec = SystemExec { timeout: Duration::from_secs(90), ..SystemExec::default() };
        let program = if update::by_go_to_linux() {
            // go-to-linux attend la fin d'une recherche en cours (15 min au plus).
            let exec = SystemExec { timeout: Duration::from_secs(3600), ..SystemExec::default() };
            update::update_by_go_to_linux(&exec)?;
            crate::restart_service();
            std::path::PathBuf::from(last1_panel_core::paths::SYSTEM_APP)
        } else {
            if let Some(release) = update::check(&exec, update::now())? {
                update::install(&release)?;
                crate::restart_service();
            }
            last1_panel_core::paths::user_appimage()
        };
        // Nouvelle fenêtre une fois celle-ci fermée (une seule à la fois).
        let mut command = std::process::Command::new("/bin/sh");
        command.args(["-c", "sleep 1; exec \"$0\""]).arg(program);
        last1_panel_core::exec::without_appimage_env(&mut command);
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        command.stdin(std::process::Stdio::null()).spawn()?;
        Ok(())
    })
    .await?;
    app.exit(0);
    Ok(())
}

/// Partage des rapports de performances (system/sharing.rs) : demandé une
/// fois, hors go-to-linux (qui envoie lui-même les siens).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sharing {
    asked: bool,
    enabled: bool,
    go_to_linux: bool,
}

#[tauri::command]
pub async fn sharing() -> Result<Sharing> {
    blocking(|| {
        let choice = Settings::load().share_reports;
        Ok(Sharing {
            asked: choice.is_some(),
            enabled: choice == Some(true),
            go_to_linux: Path::new("/var/lib/gtl").is_dir(),
        })
    })
    .await
}

/// Accord donné ou retiré. Retiré : last1.fun efface les rapports de ce PC
/// (en arrière-plan : sans Internet, l'accord est retiré quand même), et le
/// prochain « oui » renvoie un rapport sans attendre un jour.
#[tauri::command]
pub async fn set_sharing(enabled: bool) -> Result<()> {
    blocking(move || {
        let was = Settings::update(&Settings::path(), |s| s.share_reports.replace(enabled))?;
        if !enabled && was == Some(true) {
            let _ = std::fs::remove_file(last1_panel_core::paths::cache_dir().join("sharing.json"));
            std::thread::spawn(|| {
                let exec = SystemExec { timeout: Duration::from_secs(60), ..SystemExec::default() };
                if !sharing::withdraw(&exec, &sharing::this_pc()) {
                    eprintln!("Last1 Panel : rapports pas encore effacés sur last1.fun");
                }
            });
        }
        Ok(())
    })
    .await
}

/// Shaders que Steam compile en ce moment, et où il en est (bandeau de toutes
/// les pages).
#[tauri::command]
pub async fn shader_precache() -> Result<Option<last1_panel_core::games::shaders::Precache>> {
    blocking(|| {
        let steam = last1_panel_core::games::steam::find_root(&last1_panel_core::paths::home_dir());
        Ok(last1_panel_core::games::shaders::running(Path::new("/proc"), steam.as_deref()))
    })
    .await
}

/// Ferme Last1 Panel entièrement : l'icône près de l'horloge (service de
/// session), puis la fenêtre. Tout revient à la prochaine connexion.
#[tauri::command]
pub async fn quit(app: tauri::AppHandle) -> Result<()> {
    blocking(|| {
        crate::stop_service();
        Ok(())
    })
    .await?;
    app.exit(0);
    Ok(())
}

/// Installe LACT s'il manque (pacman ; avec une mise à jour du système si
/// la liste des paquets est trop ancienne), puis active et démarre son service.
/// Mot de passe demandé par pkexec.
const INSTALL_LACT: &str = "pacman -S --needed --noconfirm lact || pacman -Syu --needed --noconfirm lact";

#[tauri::command]
pub async fn enable_lact() -> Result<()> {
    blocking(|| {
        let enable = "systemctl enable --now lactd.service";
        let script =
            if which("lact").is_some() { enable.to_string() } else { format!("{{ {INSTALL_LACT}; }} && {enable}") };
        let exec = SystemExec { timeout: Duration::from_secs(1800), ..SystemExec::default() };
        exec.run_ok("pkexec", &["/bin/sh".into(), "-c".into(), script])?;
        // Le socket apparaît un instant après le démarrage du service.
        for _ in 0..30 {
            if lact::find_socket().is_some() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Err(CoreError::message(t("Le service LACT ne répond pas.", "The LACT service does not answer.")))
    })
    .await
}

/// Page Clips : Vice, installé, démarré ou non.
#[tauri::command]
pub async fn clips_state() -> Result<last1_panel_core::clips::ClipsState> {
    blocking(|| Ok(last1_panel_core::clips::state(&SystemExec::default(), &last1_panel_core::paths::home_dir()))).await
}

/// Ouvre Vice, son dossier de clips, go-to-linux (pour l'installer) ou sa
/// page, hors de la fenêtre et sans l'environnement de l'AppImage.
#[tauri::command]
pub async fn open_clips(target: String) -> Result<()> {
    blocking(move || {
        use last1_panel_core::clips;
        let home = last1_panel_core::paths::home_dir();
        let (program, args): (&str, Vec<String>) = match target.as_str() {
            "vice" if which("vice-app").is_some() => ("vice-app", vec![]),
            "vice" => ("gtk-launch", vec!["vice".into()]),
            "folder" => {
                let folder = clips::folder(&home);
                let _ = std::fs::create_dir_all(&folder);
                ("xdg-open", vec![folder.to_string_lossy().into_owned()])
            }
            "go-to-linux" => (clips::GO_TO_LINUX, vec![]),
            _ => ("xdg-open", vec![clips::VICE_URL.into()]),
        };
        let mut command = std::process::Command::new(program);
        command.args(args).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null());
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        last1_panel_core::exec::without_appimage_env(&mut command);
        command.spawn().map(|_| ()).map_err(|e| CoreError::message(e.to_string()))
    })
    .await
}

/// Démarre le service de Vice (ses touches), pour cette session et les
/// suivantes.
#[tauri::command]
pub async fn start_vice() -> Result<()> {
    blocking(|| {
        SystemExec::default()
            .run_ok("systemctl", &["--user".into(), "enable".into(), "--now".into(), "vice.service".into()])?;
        Ok(())
    })
    .await
}

/// Page Souris : pointeur du bureau et souris réglables (libratbag).
#[tauri::command]
pub async fn mouse_state(panel: State<'_, Arc<Panel>>) -> Result<last1_panel_core::panel::MouseState> {
    let panel = panel.inner().clone();
    blocking(move || Ok(panel.mouse_state())).await
}

#[tauri::command]
pub async fn set_pointer(panel: State<'_, Arc<Panel>>, acceleration: bool, speed: f64) -> Result<()> {
    let panel = panel.inner().clone();
    blocking(move || panel.set_pointer(acceleration, speed)).await
}

/// Paliers de DPI, palier actif et fréquence, écrits dans la souris.
#[tauri::command]
pub async fn set_mouse_hardware(
    panel: State<'_, Arc<Panel>>,
    id: String,
    dpis: Vec<u32>,
    active: usize,
    rate: Option<u32>,
) -> Result<()> {
    let panel = panel.inner().clone();
    blocking(move || panel.set_mouse_hardware(&id, &dpis, active, rate)).await
}

/// libratbag (ratbagd, démarré à la demande par D-Bus) installé, mot de
/// passe demandé par pkexec.
#[tauri::command]
pub async fn install_ratbag() -> Result<()> {
    blocking(|| {
        if which("pacman").is_none() {
            return Err(CoreError::message(t(
                "Installez libratbag avec le gestionnaire de paquets du système, puis rouvrez cette page.",
                "Install libratbag with the system's package manager, then reopen this page.",
            )));
        }
        let exec = SystemExec { timeout: Duration::from_secs(1800), ..SystemExec::default() };
        let script = "pacman -S --needed --noconfirm libratbag || pacman -Syu --needed --noconfirm libratbag";
        exec.run_ok("pkexec", &["/bin/sh".into(), "-c".into(), script.into()])?;
        Ok(())
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GamesInfo {
    steam: bool,
    steam_running: bool,
    games: Vec<Game>,
    settings: GamesSettings,
    launch_option: String,
    color_profiles: Vec<ColorProfile>,
    /// Couche Vulkan des couleurs des jeux installée (vkBasalt livré avec Last1 Panel).
    color_layer: bool,
}

#[tauri::command]
pub async fn games() -> Result<GamesInfo> {
    blocking(|| {
        let root = steam::find_root(&last1_panel_core::paths::home_dir());
        let settings = Settings::load();
        Ok(GamesInfo {
            steam: root.is_some(),
            steam_running: steam::is_running(Path::new("/proc")),
            games: root.as_deref().map(steam::installed_games).unwrap_or_default(),
            settings: settings.games,
            launch_option: format!("{} %command%", steam::wrapper()),
            color_profiles: settings.color_profiles,
            color_layer: last1_panel_core::paths::color_layer_installed(),
        })
    })
    .await
}

#[tauri::command]
pub async fn save_game_settings(panel: State<'_, Arc<Panel>>, settings: GamesSettings) -> Result<()> {
    let panel = panel.inner().clone();
    blocking(move || panel.save_games(settings)).await
}

/// Profils de couleurs, et l'icône près de l'horloge qui les met sur l'écran.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColorProfiles {
    profiles: Vec<ColorProfile>,
    tray: bool,
}

#[tauri::command]
pub async fn color_profiles() -> Result<ColorProfiles> {
    blocking(|| {
        let settings = Settings::load();
        Ok(ColorProfiles { profiles: settings.color_profiles, tray: settings.tray })
    })
    .await
}

/// Couleurs d'un profil sur l'écran (`id` absent : valeurs par défaut).
#[tauri::command]
pub async fn apply_color_profile(panel: State<'_, Arc<Panel>>, output: String, id: Option<String>) -> Result<()> {
    let panel = panel.inner().clone();
    blocking(move || {
        let (color, vibrance) = match id {
            None => (ColorAdjust::default(), 50.0),
            Some(id) => {
                let profile = panel
                    .settings()
                    .color_profile(&id)
                    .cloned()
                    .ok_or_else(|| CoreError::message(t("Profil introuvable.", "Profile not found.")))?;
                (profile.color, profile.vibrance)
            }
        };
        panel.apply_colors(&output, &color, vibrance)
    })
    .await
}

/// Diagnostic des performances en jeu.
#[tauri::command]
pub async fn performance_report(panel: State<'_, Arc<Panel>>) -> Result<Report> {
    let panel = panel.inner().clone();
    blocking(move || Ok(panel.performance_report())).await
}

/// Corrections proposées par le diagnostic : une, ou toutes (« Tout
/// régler »). Celles du système passent par un seul script d'administrateur
/// (mot de passe demandé une fois) ; puis les réglages de Last1 Panel, puis
/// les jeux (Steam fermé le temps de l'écrire). Rend ce qu'il reste à faire
/// au joueur (se reconnecter).
#[tauri::command]
pub async fn performance_fix(panel: State<'_, Arc<Panel>>, fixes: Vec<Fix>) -> Result<Vec<String>> {
    let panel = panel.inner().clone();
    blocking(move || {
        let power_profiles = which("powerprofilesctl").is_some();
        let vulkan = last1_panel_core::system::packages::vulkan_packages(&panel.gpus());
        let script =
            repair::root_script(&fixes, Path::new("/etc"), power_profiles, which("pacman").is_some(), &vulkan)?;
        if let Some(script) = script {
            // Une mise à jour du système peut être longue.
            let exec = SystemExec { timeout: Duration::from_secs(3 * 3600), ..SystemExec::default() };
            let output = exec.run("pkexec", &["/bin/sh".into(), "-c".into(), script])?;
            if repair::refused(output.status) {
                return Err(CoreError::message(if output.stderr.contains("authentication agent") {
                    t(
                        "Ce bureau n'a pas de fenêtre de mot de passe (agent polkit, hyprpolkitagent sous Hyprland) : rien n'a changé.",
                        "This desktop has no password window (polkit agent, hyprpolkitagent on Hyprland): nothing changed.",
                    )
                } else {
                    t("Mot de passe non donné : rien n'a changé.", "No password given: nothing changed.")
                }));
            }
            if !output.success() {
                return Err(CoreError::ToolFailed {
                    program: "pkexec".into(),
                    detail: output.stderr.trim().to_string(),
                });
            }
        }
        if fixes.contains(&Fix::GpuPowerMax) {
            gpu_power_max(&panel)?;
        }
        if fixes.contains(&Fix::Performance) && power_profiles {
            SystemExec::default().run_ok("powerprofilesctl", &["set".into(), "performance".into()])?;
        }
        if fixes.contains(&Fix::LowLatency) {
            panel.reset_low_latency()?;
        }
        if fixes.contains(&Fix::WrapGames) {
            let root = steam::find_root(&last1_panel_core::paths::home_dir())
                .ok_or_else(|| CoreError::message(t("Steam n'est pas installé.", "Steam is not installed.")))?;
            let ids: Vec<u32> = steam::installed_games(&root).iter().map(|game| game.app_id).collect();
            set_wrapped_closing_steam(&ids, true)?;
        }
        // En dernier : logind redémarre le PC de la session active, sans mot
        // de passe.
        if fixes.contains(&Fix::Restart) {
            SystemExec::default().run_ok("systemctl", &["reboot".into()])?;
        }
        let mut notices = Vec::new();
        if fixes.contains(&Fix::SystemUpdate) || fixes.contains(&Fix::VulkanDriver) {
            // Listes de paquets refaites : le diagnostic relit pacman.
            let _ = std::fs::remove_file(last1_panel_core::paths::cache_dir().join("updates.txt"));
        }
        if fixes.contains(&Fix::SystemUpdate) {
            notices.push(t(
                "Système à jour : un redémarrage met en service le nouveau pilote et le nouveau noyau.",
                "System up to date: a restart puts the new driver and kernel into service.",
            ));
        }
        if fixes.contains(&Fix::Zram) {
            notices.push(t(
                "Mémoire compressée activée : les programmes en arrière-plan restent en mémoire, même après un redémarrage.",
                "Compressed memory on: background programs stay in memory, even after a restart.",
            ));
        }
        if fixes.contains(&Fix::GpuPowerMax) {
            notices.push(t(
                "Carte graphique à sa puissance maximale, gardée à chaque démarrage (page Carte graphique pour la changer).",
                "Graphics card at its maximum power, kept at every boot (Graphics card page to change it).",
            ));
        }
        if fixes.contains(&Fix::CpuPowerLimit) {
            notices.push(t(
                "Puissance du processeur libérée pour les jeux : elle vaut dès la prochaine partie, chargeur branché.",
                "CPU power freed for games: it applies from the next game, with the charger plugged in.",
            ));
        }
        if fixes.contains(&Fix::PanelSelfRefresh) {
            notices.push(t(
                "Panel Self Refresh coupé : il vaut après un redémarrage du PC.",
                "Panel Self Refresh off: it applies after restarting the PC.",
            ));
        }
        if fixes.contains(&Fix::ShaderCache) {
            notices.push(t(
                "Cache des shaders agrandi : il vaudra après une déconnexion-reconnexion (ou un redémarrage).",
                "Shader cache enlarged: it applies after logging out and back in (or a restart).",
            ));
        }
        Ok(notices)
    })
    .await
}

/// Carte des jeux à sa puissance maximale, gardée par LACT (service démarré
/// juste avant par le script d'administrateur).
fn gpu_power_max(panel: &Panel) -> last1_panel_core::error::CoreResult<()> {
    for _ in 0..50 {
        if lact::find_socket().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let lact = Lact::connect()?;
    let gpus = panel.gpus();
    let games = last1_panel_core::system::gpu::games_gpu(&gpus)
        .ok_or_else(|| CoreError::message(t("Aucune carte graphique pour les jeux.", "No graphics card for games.")))?;
    let device =
        lact.devices()?.into_iter().find(|device| device.pci_slot.eq_ignore_ascii_case(&games.pci_slot)).ok_or_else(
            || CoreError::message(t("LACT ne voit pas la carte des jeux.", "LACT doesn't see the games card.")),
        )?;
    let max = lact.stats(&device.id)?.power_cap_max.ok_or_else(|| {
        CoreError::message(t(
            "Cette carte ne donne pas sa puissance maximale.",
            "This card doesn't report its maximum power.",
        ))
    })?;
    let config = lact.config(&device.id)?;
    let tuning = Tuning { power_cap: Some(max), ..lact::tuning_from_config(&config) };
    let config = lact::apply_tuning(&config, &tuning, games.vendor == Vendor::Nvidia);
    lact.try_config(&device.id, &config)?;
    lact.confirm(true)
}

/// Ce que devient le lien entre un jeu et un profil de couleurs.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameLink {
    /// L'option de lancement de Last1 Panel manque et Steam est ouvert : elle
    /// attend qu'il se ferme (`close_steam_and_wrap`).
    needs_steam_closed: bool,
}

/// Profil de couleurs d'un jeu Steam (`profile` absent : plus de profil), et
/// l'option de lancement de Last1 Panel mise si elle manque : le profil
/// s'active dès le lancement du jeu.
#[tauri::command]
pub async fn link_game_color(app_id: u32, profile: Option<String>) -> Result<GameLink> {
    blocking(move || {
        let linked = profile.is_some();
        Settings::update(&Settings::path(), |s| {
            let game = s.games.per_game.entry(app_id).or_default();
            game.color_profile = profile;
            if *game == GameProfile::default() {
                s.games.per_game.remove(&app_id);
            }
        })?;
        let root = steam::find_root(&last1_panel_core::paths::home_dir());
        let wrapped = root
            .as_deref()
            .is_some_and(|root| steam::installed_games(root).iter().any(|game| game.app_id == app_id && game.wrapped));
        let Some(root) = root.filter(|_| linked && !wrapped) else { return Ok(GameLink { needs_steam_closed: false }) };
        if steam::is_running(Path::new("/proc")) {
            return Ok(GameLink { needs_steam_closed: true });
        }
        steam::set_wrapped(&root, Path::new("/proc"), &[app_id], true)?;
        Ok(GameLink { needs_steam_closed: false })
    })
    .await
}

/// Steam fermé proprement, l'option de lancement mise pour ces jeux, puis
/// Steam rouvert, comme avant.
#[tauri::command]
pub async fn close_steam_and_wrap(app_ids: Vec<u32>) -> Result<usize> {
    blocking(move || set_wrapped_closing_steam(&app_ids, true)).await
}

/// Met (ou retire) l'option de lancement de Last1 Panel. Steam lancé : il
/// est fermé le temps de l'écrire (il la remplacerait en se fermant), puis
/// rouvert, même en cas d'échec. Jamais pendant un jeu : fermer Steam le
/// fermerait.
fn set_wrapped_closing_steam(app_ids: &[u32], wrapped: bool) -> last1_panel_core::error::CoreResult<usize> {
    let root = steam::find_root(&last1_panel_core::paths::home_dir())
        .ok_or_else(|| CoreError::message(t("Steam n'est pas installé.", "Steam is not installed.")))?;
    steam::check_writable(&root, wrapped)?;
    with_steam_closed(|| steam::set_wrapped(&root, Path::new("/proc"), app_ids, wrapped))
}

/// `work` fait Steam fermé proprement (s'il était lancé), puis Steam rouvert,
/// même en cas d'échec. Jamais pendant un jeu : fermer Steam le fermerait.
fn with_steam_closed<T>(
    work: impl FnOnce() -> last1_panel_core::error::CoreResult<T>,
) -> last1_panel_core::error::CoreResult<T> {
    let proc_dir = Path::new("/proc");
    let was_running = steam::is_running(proc_dir);
    if was_running {
        if steam::game_running(proc_dir) {
            return Err(CoreError::message(t(
                "Un jeu est lancé : quittez-le, puis réessayez (fermer Steam le fermerait aussi).",
                "A game is running: quit it, then try again (closing Steam would close it too).",
            )));
        }
        let _ = steam_command(&["-shutdown"]).spawn().map(|mut child| std::thread::spawn(move || child.wait()));
        if !steam::wait_closed(proc_dir, Duration::from_secs(60)) {
            return Err(CoreError::message(t(
                "Steam ne s'est pas fermé : quittez-le (menu Steam > Quitter), puis réessayez.",
                "Steam did not close: quit it (Steam menu > Exit), then try again.",
            )));
        }
    }
    let result = work();
    if was_running && let Ok(mut child) = steam_command(&[]).spawn() {
        std::thread::spawn(move || child.wait());
    }
    result
}

/// Steam relancé avec tous les cœurs pour compiler les shaders
/// (steam_dev.cfg, lu à son démarrage) : il reprend la compilation aussitôt.
#[tauri::command]
pub async fn speed_up_shaders() -> Result<()> {
    blocking(|| {
        let root = steam::find_root(&last1_panel_core::paths::home_dir())
            .ok_or_else(|| CoreError::message(t("Steam n'est pas installé.", "Steam is not installed.")))?;
        with_steam_closed(|| {
            last1_panel_core::games::shaders::give_cores(&root, last1_panel_core::games::shaders::cores())?;
            Ok(())
        })
    })
    .await
}

/// Steam lancé hors de la fenêtre (son propre groupe de processus) et sans
/// l'environnement de l'AppImage.
fn steam_command(args: &[&str]) -> std::process::Command {
    use std::os::unix::process::CommandExt;
    let mut command = std::process::Command::new("steam");
    command
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0);
    last1_panel_core::exec::without_appimage_env(&mut command);
    command
}

/// Icône près de l'horloge affichée ou masquée (le service de la session
/// relit le réglage).
#[tauri::command]
pub async fn set_tray(shown: bool) -> Result<()> {
    blocking(move || Settings::update(&Settings::path(), |s| s.tray = shown)).await
}

/// Profil créé (sans `id`) ou remplacé.
#[tauri::command]
pub async fn save_color_profile(
    id: Option<String>,
    name: String,
    color: ColorAdjust,
    vibrance: f64,
) -> Result<ColorProfile> {
    blocking(move || {
        Settings::update(&Settings::path(), |s| s.put_color_profile(id.as_deref(), &name, color, vibrance))
    })
    .await
}

#[tauri::command]
pub async fn delete_color_profile(id: String) -> Result<()> {
    blocking(move || Settings::update(&Settings::path(), |s| s.remove_color_profile(&id))).await
}

/// Option de lancement mise ou retirée ; Steam ouvert : fermé puis rouvert.
#[tauri::command]
pub async fn set_wrapped(app_ids: Vec<u32>, wrapped: bool) -> Result<usize> {
    blocking(move || set_wrapped_closing_steam(&app_ids, wrapped)).await
}
