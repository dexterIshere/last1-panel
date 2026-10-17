//! Commandes appelées par l'interface (src/lib/api/ipc.ts). Chacune tourne
//! hors du fil de la fenêtre : un outil lent ne fige jamais l'interface.
//! Les erreurs sont des phrases dans la langue choisie, montrées telles quelles.

use last1_panel_core::color::curve::ColorAdjust;
use last1_panel_core::display::Features;
use last1_panel_core::display::model::{Output, OutputChange};
use last1_panel_core::error::CoreError;
use last1_panel_core::exec::{Exec, SystemExec, which};
use last1_panel_core::games::steam::{self, Game};
use last1_panel_core::i18n::{self, t};
use last1_panel_core::panel::{ColorState, Panel};
use last1_panel_core::settings::{GamesSettings, Settings};
use last1_panel_core::system::desktop::Desktop;
use last1_panel_core::system::gpu::{Gpu, Vendor};
use last1_panel_core::tuning::lact::{self, ClockRanges, Device, Lact, OffsetRange, Stats, Tuning};
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
    game_performance: bool,
    steam: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    version: &'static str,
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
            version: env!("CARGO_PKG_VERSION"),
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
                game_performance: which("game-performance").is_some(),
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GamesInfo {
    steam: bool,
    steam_running: bool,
    games: Vec<Game>,
    settings: GamesSettings,
    launch_option: String,
}

#[tauri::command]
pub async fn games() -> Result<GamesInfo> {
    blocking(|| {
        let root = steam::find_root(&last1_panel_core::paths::home_dir());
        Ok(GamesInfo {
            steam: root.is_some(),
            steam_running: steam::is_running(Path::new("/proc")),
            games: root.as_deref().map(steam::installed_games).unwrap_or_default(),
            settings: Settings::load().games,
            launch_option: format!("{} %command%", steam::wrapper()),
        })
    })
    .await
}

#[tauri::command]
pub async fn save_game_settings(panel: State<'_, Arc<Panel>>, settings: GamesSettings) -> Result<()> {
    let panel = panel.inner().clone();
    blocking(move || panel.save_games(settings)).await
}

#[tauri::command]
pub async fn set_wrapped(app_ids: Vec<u32>, wrapped: bool) -> Result<usize> {
    blocking(move || {
        let root = steam::find_root(&last1_panel_core::paths::home_dir())
            .ok_or_else(|| CoreError::message(t("Steam n'est pas installé.", "Steam is not installed.")))?;
        steam::set_wrapped(&root, Path::new("/proc"), &app_ids, wrapped)
    })
    .await
}
