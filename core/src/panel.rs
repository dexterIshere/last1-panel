//! Last1 Panel vu de l'interface et de l'outil en ligne de commande : le PC,
//! son bureau, et les réglages qui touchent plusieurs outils à la fois.

use crate::color::curve::ColorAdjust;
use crate::color::icc;
use crate::color::vibrance;
use crate::display::hyprland::Hyprland;
use crate::display::kde::Kde;
use crate::display::model::{Output, OutputChange};
use crate::display::{DisplayBackend, Features, ProfileState};
use crate::error::{CoreError, CoreResult};
use crate::exec::{Exec, SystemExec};
use crate::i18n::t;
use crate::settings::{GamesSettings, OutputSettings, Settings};
use crate::system::desktop::Desktop;
use crate::system::gpu::{self, Gpu, SysPaths, Vendor};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const ICC_PREFIX: &str = "last1-panel-";

pub struct Panel {
    pub exec: Arc<dyn Exec>,
    pub desktop: Desktop,
    pub sys: SysPaths,
    pub settings_path: PathBuf,
    pub icc_dir: PathBuf,
    display: Option<Box<dyn DisplayBackend>>,
    /// Un réglage à la fois (couleurs et vibrance arrivent par deux files).
    busy: Mutex<()>,
}

/// Variables de la session graphique utiles aux outils des bureaux.
const SESSION_VARS: [&str; 6] = [
    "WAYLAND_DISPLAY",
    "XDG_CURRENT_DESKTOP",
    "XDG_SESSION_TYPE",
    "HYPRLAND_INSTANCE_SIGNATURE",
    "DISPLAY",
    "XDG_RUNTIME_DIR",
];

/// État des couleurs d'un écran, pour la page « Couleurs ».
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColorState {
    pub output: String,
    pub adjust: ColorAdjust,
    /// Vibrance en % (50 : inchangée).
    pub vibrance: f64,
    /// Pourquoi la vibrance ne se règle pas sur cet écran (absent : elle se règle).
    pub vibrance_unavailable: Option<String>,
    /// Pourquoi luminosité, contraste et gamma ne se règlent pas (HDR…).
    pub color_unavailable: Option<String>,
    /// Profil ICC de l'utilisateur remplacé par nos réglages (remis à la fin).
    pub replaced_profile: Option<String>,
}

impl Panel {
    /// Le vrai système : bureau de la session, outils installés.
    pub fn system() -> Self {
        Self::for_session(BTreeMap::new())
    }

    /// Le vrai système, vu du service de la session.
    pub fn session() -> Self {
        Self::for_session(crate::system::desktop::session_env())
    }

    /// Le vrai système, vu d'un service lancé avant le bureau : les variables
    /// de la session graphique sont celles que le bureau a données à systemd
    /// (`systemctl --user show-environment`), sinon celles du processus.
    pub fn for_session(session: BTreeMap<String, String>) -> Self {
        let lookup = |name: &str| session.get(name).cloned().or_else(|| std::env::var(name).ok());
        let desktop = Desktop::from_env(lookup);
        let env = SESSION_VARS.iter().filter_map(|name| Some((name.to_string(), lookup(name)?))).collect();
        let exec: Arc<dyn Exec> = Arc::new(SystemExec { env, ..SystemExec::default() });
        let sys = SysPaths::default();
        let display: Option<Box<dyn DisplayBackend>> = match desktop {
            Desktop::Plasma => Some(Box::new(Kde::new(exec.clone(), sys.sys.clone()))),
            Desktop::Hyprland => Some(Box::new(Hyprland::new(
                exec.clone(),
                sys.sys.clone(),
                crate::paths::user_config_dir().join("hypr"),
            ))),
            Desktop::Other => None,
        };
        Self {
            exec,
            desktop,
            sys,
            settings_path: Settings::path(),
            icc_dir: crate::paths::icc_dir(),
            display,
            busy: Mutex::new(()),
        }
    }

    /// Pour les tests : système et bureau donnés.
    pub fn with(
        exec: Arc<dyn Exec>,
        desktop: Desktop,
        sys: SysPaths,
        settings_path: PathBuf,
        icc_dir: PathBuf,
        display: Option<Box<dyn DisplayBackend>>,
    ) -> Self {
        Self { exec, desktop, sys, settings_path, icc_dir, display, busy: Mutex::new(()) }
    }

    pub fn settings(&self) -> Settings {
        Settings::load_from(&self.settings_path)
    }

    fn update<T>(&self, change: impl FnOnce(&mut Settings) -> T) -> CoreResult<T> {
        Settings::update(&self.settings_path, change)
    }

    /// Réglages des jeux (page « Jeux »), le reste du fichier gardé.
    pub fn save_games(&self, games: GamesSettings) -> CoreResult<()> {
        self.update(|settings| settings.games = games)
    }

    pub fn gpus(&self) -> Vec<Gpu> {
        gpu::detect(&self.sys)
    }

    pub fn display(&self) -> CoreResult<&dyn DisplayBackend> {
        self.display.as_deref().ok_or_else(|| {
            CoreError::message(t(
                "Réglages d'écran disponibles sous Hyprland et KDE Plasma (session Wayland) seulement.",
                "Screen settings are only available on Hyprland and KDE Plasma (Wayland session).",
            ))
        })
    }

    pub fn features(&self) -> Option<Features> {
        self.display.as_ref().map(|display| display.features())
    }

    pub fn outputs(&self) -> CoreResult<Vec<Output>> {
        self.display()?.outputs()
    }

    /// Applique ces changements ; les écrans voisins sont recalés côte à côte
    /// si une taille change (définition, échelle, orientation).
    pub fn apply_outputs(&self, changes: &[OutputChange]) -> CoreResult<()> {
        let display = self.display()?;
        let before = display.outputs()?;
        let mut after: Vec<Output> = before
            .iter()
            .map(|output| match changes.iter().find(|c| c.name == output.name) {
                Some(change) => {
                    let mut next = change.applied_to(output);
                    next.scale = display.normalize_scale(next.current_mode.as_ref(), next.scale);
                    next
                }
                None => output.clone(),
            })
            .collect();
        crate::display::model::relayout(&before, &mut after);
        let all = crate::display::model::diff(&before, &after);
        if all.is_empty() { Ok(()) } else { display.apply(&all) }
    }

    fn is_ours(&self, path: &str) -> bool {
        Path::new(path).parent() == Some(self.icc_dir.as_path())
            && Path::new(path).file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(ICC_PREFIX))
    }

    /// Profils ICC de Last1 Panel pour cette sortie, sauf `keep`.
    fn remove_old_profiles(&self, output: &str, keep: Option<&Path>) {
        let prefix = format!("{ICC_PREFIX}{}-", safe_name(output));
        let Ok(entries) = fs::read_dir(&self.icc_dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            let ours = entry.file_name().to_str().is_some_and(|n| n.starts_with(&prefix) && n.ends_with(".icc"));
            if ours && Some(path.as_path()) != keep {
                let _ = fs::remove_file(path);
            }
        }
    }

    pub fn color_state(&self, output: &str) -> CoreResult<ColorState> {
        let settings = self.settings();
        let saved = settings.outputs.get(output).cloned().unwrap_or_default();
        let screen = self.outputs()?.into_iter().find(|o| o.name == output);
        let color_unavailable = match &screen {
            None => Some(t("écran introuvable", "screen not found")),
            Some(screen) if screen.hdr == Some(true) => Some(t(
                "Écran en HDR : luminosité, contraste et gamma ne s'appliquent qu'en SDR (désactivez le HDR dans Affichage).",
                "Screen in HDR mode: brightness, contrast and gamma only apply in SDR (turn HDR off in Display).",
            )),
            Some(screen) if screen.icc_profile.is_none() => {
                Some(t("Ce bureau ne gère pas les profils de couleur.", "This desktop does not handle color profiles."))
            }
            _ => self.features().and_then(|f| f.read_only),
        };
        let current = self.display()?.profile(output).unwrap_or_default();
        let replaced = saved
            .previous_profile
            .as_ref()
            .and_then(|p| p.icc.clone())
            .or_else(|| current.icc.clone().filter(|path| !path.is_empty() && !self.is_ours(path)));
        Ok(ColorState {
            output: output.to_string(),
            adjust: saved.color,
            vibrance: saved.vibrance.unwrap_or(50.0),
            vibrance_unavailable: self.vibrance_unavailable(output),
            color_unavailable,
            replaced_profile: replaced,
        })
    }

    fn vibrance_unavailable(&self, output: &str) -> Option<String> {
        let gpus = self.gpus();
        match gpu::gpu_for_connector(&gpus, output) {
            Some(gpu) if gpu.vendor == Vendor::Nvidia => {
                vibrance::program(self.exec.as_ref(), gpu.driver_version.as_deref()).is_none().then(|| {
                    t(
                        "Vibrance indisponible pour cette version du pilote NVIDIA : mettez à jour Last1 Panel.",
                        "Vibrance unavailable for this NVIDIA driver version: update Last1 Panel.",
                    )
                })
            }
            Some(_) if gpus.iter().any(|g| g.vendor == Vendor::Nvidia) => Some(t(
                "Cet écran est branché sur la puce graphique intégrée : la vibrance NVIDIA ne s'y applique pas.",
                "This screen is driven by the integrated graphics: NVIDIA vibrance does not apply to it.",
            )),
            _ => Some(t(
                "Vibrance numérique : cartes NVIDIA seulement pour l'instant.",
                "Digital vibrance: NVIDIA cards only for now.",
            )),
        }
    }

    /// Luminosité, contraste et gamma d'un écran : profil ICC écrit puis posé
    /// (un nouveau fichier à chaque réglage : KWin et Hyprland ne relisent pas
    /// un profil dont le chemin n'a pas changé). Valeurs par défaut : le profil
    /// d'avant Last1 Panel est remis.
    pub fn set_color(&self, output: &str, adjust: &ColorAdjust) -> CoreResult<()> {
        let _busy = self.busy.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let display = self.display()?;
        // Profil illisible : on s'arrête, sinon celui de l'utilisateur serait
        // noté « aucun » et jamais remis.
        let current = display.profile(output)?;
        let previous = self.settings().outputs.get(output).and_then(|o| o.previous_profile.clone());
        if adjust.is_neutral() {
            if let Some(previous) = &previous {
                display.set_profile(output, previous)?;
            } else if current.icc.as_deref().is_some_and(|path| self.is_ours(path)) {
                display.set_profile(output, &ProfileState { icc: None, source: current_source_default(&current) })?;
            }
            self.update(|settings| {
                let entry = settings.outputs.entry(output.to_string()).or_default();
                entry.color = *adjust;
                entry.previous_profile = None;
            })?;
            self.remove_old_profiles(output, None);
            return Ok(());
        }
        let transfer = display.features().icc_transfer;
        let bytes = icc::build_profile(&format!("Last1 Panel {output}"), adjust, transfer)?;
        fs::create_dir_all(&self.icc_dir)?;
        let path = self.icc_dir.join(format!("{ICC_PREFIX}{}-{:016x}.icc", safe_name(output), fnv1a(&bytes)));
        if !path.exists() {
            let temporary = path.with_extension("icc.new");
            fs::write(&temporary, &bytes)?;
            fs::rename(&temporary, &path)?;
        }
        let record =
            (previous.is_none() && !current.icc.as_deref().is_some_and(|p| self.is_ours(p))).then(|| current.clone());
        display.set_profile(
            output,
            &ProfileState {
                icc: Some(path.to_string_lossy().into_owned()),
                source: current.source.as_ref().map(|_| "ICC".into()),
            },
        )?;
        self.update(|settings| {
            let entry = settings.outputs.entry(output.to_string()).or_default();
            entry.color = *adjust;
            if entry.previous_profile.is_none() {
                entry.previous_profile = record;
            }
        })?;
        self.remove_old_profiles(output, Some(&path));
        Ok(())
    }

    /// Vibrance d'un écran, avec celles des autres écrans de la même carte
    /// (nvibrant les règle toutes d'un coup). Seul un refus sur cet écran est
    /// une erreur : un autre écran éteint ne bloque rien.
    pub fn set_vibrance(&self, output: &str, percent: f64) -> CoreResult<()> {
        let _busy = self.busy.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let gpus = self.gpus();
        let gpu = gpu::gpu_for_connector(&gpus, output)
            .ok_or_else(|| CoreError::message(format!("{output} : {}", t("écran introuvable", "screen not found"))))?;
        let mut values = vibrance_values(&self.settings(), gpu);
        values.insert(output.to_string(), percent);
        vibrance::apply(self.exec.as_ref(), gpu, nvidia_index(&gpus, gpu), &values, &[output.to_string()])?;
        self.update(|settings| settings.outputs.entry(output.to_string()).or_default().vibrance = Some(percent))
    }

    /// Diagnostic des performances en jeu (system/diagnostic.rs), avec les
    /// variables de la session dont Steam et ses jeux héritent.
    pub fn performance_report(&self) -> crate::system::diagnostic::Report {
        use crate::games::steam;
        let steam = steam::find_root(&crate::paths::home_dir());
        let default_proton = steam.as_deref().and_then(steam::default_compat_tool);
        // Les trois derniers jeux lancés, avec le Proton de chacun.
        let recent_games = steam
            .as_deref()
            .map(|root| {
                steam::recently_played(root)
                    .into_iter()
                    .take(3)
                    .map(|game| {
                        let proton = steam::compat_tool(root, &game.app_id.to_string())
                            .or_else(|| default_proton.clone())
                            .unwrap_or_else(|| t("Proton de Steam", "Steam's Proton"));
                        format!("{} ({proton})", game.name)
                    })
                    .collect()
            })
            .unwrap_or_default();
        crate::system::diagnostic::run(&crate::system::diagnostic::Sources {
            sys: &self.sys.sys,
            dev: Path::new("/dev"),
            proc_dir: Path::new("/proc"),
            exec: self.exec.as_ref(),
            env: &|name| std::env::var(name).ok().or_else(|| crate::system::diagnostic::etc_environment(name)),
            settings: &self.settings(),
            gpus: &self.gpus(),
            desktop: self.desktop,
            default_proton,
            recent_games,
            sessions: crate::system::sessions::recent(&crate::paths::game_logs_dir(), 10),
            mangohud: crate::exec::which("mangohud").is_some(),
            wrapped_games: steam
                .as_deref()
                .map(steam::installed_games)
                .filter(|games| !games.is_empty())
                .map(|games| (games.iter().filter(|game| game.wrapped).count(), games.len())),
            go_to_linux: Path::new("/var/lib/gtl").is_dir(),
        })
    }

    /// Faible latence rendue aux jeux, partout (correction du diagnostic).
    pub fn reset_low_latency(&self) -> CoreResult<()> {
        self.update(|settings| {
            settings.games.global.low_latency = crate::games::profile::LowLatency::App;
            for game in settings.games.per_game.values_mut() {
                game.low_latency = crate::games::profile::LowLatency::App;
            }
            settings.games.per_game.retain(|_, game| *game != crate::games::profile::GameProfile::default());
        })
    }

    /// Couleurs d'un profil (ou valeurs par défaut) sur un écran : courbes,
    /// puis vibrance sur un écran d'une carte NVIDIA (ailleurs, sans elle).
    /// Ce qui est déjà en place n'est pas refait : l'écran ne clignote pas.
    pub fn apply_colors(&self, output: &str, color: &ColorAdjust, vibrance: f64) -> CoreResult<()> {
        let state = self.color_state(output)?;
        if !state.adjust.approx_eq(color) {
            if let Some(reason) = state.color_unavailable {
                return Err(CoreError::message(format!("{output} : {reason}")));
            }
            self.set_color(output, color)?;
        }
        if state.vibrance_unavailable.is_none() && (state.vibrance - vibrance).abs() >= 0.5 {
            self.set_vibrance(output, vibrance)?;
        }
        Ok(())
    }

    /// À la connexion : vibrance réappliquée (perdue au redémarrage), profils
    /// ICC remis s'ils ont disparu (monitors.conf réécrit par nwg-displays…).
    pub fn apply_saved(&self) -> Vec<String> {
        let mut problems = Vec::new();
        let settings = self.settings();
        let gpus = self.gpus();
        for gpu in gpus.iter().filter(|g| g.vendor == Vendor::Nvidia) {
            let values = vibrance_values(&settings, gpu);
            let targets: Vec<String> =
                values.iter().filter(|(_, v)| vibrance::to_nvkms(**v) != 0).map(|(name, _)| name.clone()).collect();
            if !targets.is_empty()
                && let Err(error) =
                    vibrance::apply(self.exec.as_ref(), gpu, nvidia_index(&gpus, gpu), &values, &targets)
            {
                problems.push(error.to_string());
            }
        }
        if let Some(display) = &self.display {
            let connected: Vec<String> = display.outputs().unwrap_or_default().into_iter().map(|o| o.name).collect();
            for (name, saved) in &settings.outputs {
                if saved.color.is_neutral() || !connected.contains(name) {
                    continue;
                }
                let Ok(current) = display.profile(name) else { continue };
                let ok = current.icc.as_deref().is_some_and(|p| self.is_ours(p) && Path::new(p).exists());
                if !ok && let Err(error) = self.set_color(name, &saved.color) {
                    problems.push(format!("{name} : {error}"));
                }
            }
        }
        problems
    }
}

/// Source de couleur à remettre quand on retire notre profil sans connaître
/// la précédente : sRGB, le réglage d'usine de Plasma.
fn current_source_default(current: &ProfileState) -> Option<String> {
    current.source.as_ref().map(|_| "sRGB".to_string())
}

fn vibrance_values(settings: &Settings, gpu: &Gpu) -> BTreeMap<String, f64> {
    gpu.connectors
        .iter()
        .filter_map(|c| {
            let saved: &OutputSettings = settings.outputs.get(&c.name)?;
            Some((c.name.clone(), saved.vibrance?))
        })
        .collect()
}

/// Rang de la carte parmi les cartes NVIDIA (variable NVIDIA_GPU de nvibrant).
fn nvidia_index(gpus: &[Gpu], gpu: &Gpu) -> usize {
    let mut nvidia: Vec<&Gpu> = gpus.iter().filter(|g| g.vendor == Vendor::Nvidia).collect();
    nvidia.sort_by(|a, b| a.pci_slot.cmp(&b.pci_slot));
    nvidia.iter().position(|g| g.card == gpu.card).unwrap_or(0)
}

fn safe_name(output: &str) -> String {
    output.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect()
}

/// Empreinte FNV-1a 64 bits du profil (lcms2 y écrit l'heure : un nouveau
/// fichier à chaque réglage).
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::curve::Channel;
    use crate::display::model::tests::output;
    use std::sync::Mutex;

    /// Bureau simulé : garde le profil posé sur chaque sortie.
    struct FakeDisplay {
        profiles: Mutex<BTreeMap<String, ProfileState>>,
        hdr: bool,
    }

    impl DisplayBackend for FakeDisplay {
        fn features(&self) -> Features {
            Features {
                primary: true,
                bit_depths: vec![0, 8, 10],
                live_color: true,
                icc_transfer: icc::Transfer::Gamma22,
                read_only: None,
            }
        }
        fn outputs(&self) -> CoreResult<Vec<Output>> {
            let mut screen = output("DP-1", 0, 2560, 1440, 1.0);
            screen.hdr = Some(self.hdr);
            screen.icc_profile =
                Some(self.profiles.lock().unwrap().get("DP-1").and_then(|p| p.icc.clone()).unwrap_or_default());
            Ok(vec![screen])
        }
        fn apply(&self, _: &[OutputChange]) -> CoreResult<()> {
            Ok(())
        }
        fn profile(&self, output: &str) -> CoreResult<ProfileState> {
            Ok(self.profiles.lock().unwrap().get(output).cloned().unwrap_or_default())
        }
        fn set_profile(&self, output: &str, state: &ProfileState) -> CoreResult<()> {
            self.profiles.lock().unwrap().insert(output.to_string(), state.clone());
            Ok(())
        }
    }

    fn panel(dir: &Path, start: ProfileState, hdr: bool) -> Panel {
        let display = FakeDisplay { profiles: Mutex::new(BTreeMap::from([("DP-1".to_string(), start)])), hdr };
        Panel::with(
            Arc::new(crate::exec::FakeExec::new().without("nvibrant")),
            Desktop::Plasma,
            SysPaths { sys: dir.join("sys"), pci_ids: vec![] },
            dir.join("settings.json"),
            dir.join("icc"),
            Some(Box::new(display)),
        )
    }

    fn icc_files(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir.join("icc"))
            .map(|entries| entries.flatten().filter_map(|e| e.file_name().into_string().ok()).collect())
            .unwrap_or_default();
        names.sort();
        names
    }

    #[test]
    fn color_profile_is_set_then_the_original_comes_back() {
        let dir = tempfile::tempdir().unwrap();
        let original = ProfileState { icc: Some("/home/j/calibration.icc".into()), source: Some("ICC".into()) };
        let panel = panel(dir.path(), original.clone(), false);

        let warm = ColorAdjust::all(Channel { brightness: 55.0, contrast: 50.0, gamma: 1.1 });
        panel.set_color("DP-1", &warm).unwrap();
        let first = panel.display().unwrap().profile("DP-1").unwrap();
        assert!(panel.is_ours(first.icc.as_deref().unwrap()));
        assert_eq!(first.source.as_deref(), Some("ICC"));
        assert_eq!(icc_files(dir.path()).len(), 1);

        let state = panel.color_state("DP-1").unwrap();
        assert_eq!(state.adjust, warm);
        assert_eq!(state.replaced_profile.as_deref(), Some("/home/j/calibration.icc"));
        assert!(state.vibrance_unavailable.is_some());

        // Nouveau réglage : nouveau fichier, l'ancien disparaît.
        let brighter = ColorAdjust::all(Channel { brightness: 60.0, contrast: 50.0, gamma: 1.1 });
        panel.set_color("DP-1", &brighter).unwrap();
        let second = panel.display().unwrap().profile("DP-1").unwrap();
        assert_ne!(first.icc, second.icc);
        assert_eq!(icc_files(dir.path()).len(), 1);

        // Valeurs par défaut : profil de calibration remis, nos fichiers effacés.
        panel.set_color("DP-1", &ColorAdjust::default()).unwrap();
        assert_eq!(panel.display().unwrap().profile("DP-1").unwrap(), original);
        assert!(icc_files(dir.path()).is_empty());
        assert!(panel.settings().outputs["DP-1"].previous_profile.is_none());
    }

    #[test]
    fn profile_colors_go_on_the_screen_once() {
        let dir = tempfile::tempdir().unwrap();
        let panel = panel(dir.path(), ProfileState::default(), false);
        let guide = ColorAdjust::all(Channel { brightness: 52.0, contrast: 47.0, gamma: 1.47 });
        panel.apply_colors("DP-1", &guide, 62.0).unwrap();
        let first = panel.display().unwrap().profile("DP-1").unwrap();
        assert_eq!(panel.settings().outputs["DP-1"].color, guide);
        // Vibrance indisponible (pas de carte NVIDIA) : le reste s'applique.
        assert_eq!(panel.settings().outputs["DP-1"].vibrance, None);
        // Déjà en place : rien n'est refait.
        panel.apply_colors("DP-1", &guide, 62.0).unwrap();
        assert_eq!(panel.display().unwrap().profile("DP-1").unwrap(), first);
        panel.apply_colors("DP-1", &ColorAdjust::default(), 50.0).unwrap();
        assert!(panel.settings().outputs["DP-1"].color.is_neutral());
    }

    #[test]
    fn hdr_blocks_color_curves() {
        let dir = tempfile::tempdir().unwrap();
        let panel = panel(dir.path(), ProfileState::default(), true);
        assert!(panel.color_state("DP-1").unwrap().color_unavailable.unwrap().contains("HDR"));
    }

    #[test]
    fn login_restores_a_lost_profile() {
        let dir = tempfile::tempdir().unwrap();
        let panel = panel(dir.path(), ProfileState { icc: None, source: Some("sRGB".into()) }, false);
        let adjust = ColorAdjust::all(Channel { brightness: 50.0, contrast: 58.0, gamma: 1.0 });
        panel.set_color("DP-1", &adjust).unwrap();
        // Profil retiré par un autre outil.
        panel.display().unwrap().set_profile("DP-1", &ProfileState::default()).unwrap();
        assert!(panel.apply_saved().is_empty());
        assert!(panel.display().unwrap().profile("DP-1").unwrap().icc.is_some_and(|p| panel.is_ours(&p)));
    }

    #[test]
    fn same_profile_same_file_name() {
        assert_eq!(fnv1a(b"abc"), fnv1a(b"abc"));
        assert_ne!(fnv1a(b"abc"), fnv1a(b"abd"));
        assert_eq!(safe_name("DP-1"), "DP-1");
        assert_eq!(safe_name("Virtual 1"), "Virtual_1");
    }
}
