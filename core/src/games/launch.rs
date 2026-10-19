//! Lanceur des jeux : `$(command -v last1-panel-cli && echo run) %command%`
//! dans les options de lancement Steam (games/steam.rs). Il lit nos réglages, pose les variables que lisent
//! Proton, DXVK, VKD3D-Proton, dxvk-nvapi, les pilotes et MangoHud, chaîne
//! les outils de CachyOS (game-performance, dlss-swapper) puis lance le jeu.
//!
//! Sources des variables (vérifiées dans le code de chaque projet,
//! octobre 2026) : dxvk.conf et README de DXVK 3.1, README et swapchain.c de
//! vkd3d-proton 3.1, README de dxvk-nvapi, README du pilote NVIDIA 615,
//! README de MangoHud, scripts de CachyOS-Settings.
//!
//! Une variable déjà posée par l'utilisateur (`FOO=1 last1-panel-cli run
//! %command%`) n'est jamais remplacée ; ses options DXVK passent après les
//! nôtres, donc gagnent.

use super::profile::{GameProfile, LowLatency, Toggle};

/// Le jeu envoyé à la carte des jeux quand elle ne dessine pas le bureau.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Offload {
    /// PRIME de NVIDIA (comme `prime-run`).
    Nvidia,
    /// Mesa (AMD, Intel) : DRI_PRIME=pci-0000_03_00_0.
    Mesa(String),
}

/// Ce que le lanceur sait du jeu et du PC.
#[derive(Debug, Clone, Default)]
pub struct Context {
    /// Jeu lancé par Proton (un argument se termine par « /proton »).
    pub proton: bool,
    /// Marque de la carte des jeux (system/gpu.rs, choose_games).
    pub nvidia: bool,
    pub amd: bool,
    /// Carte des jeux à part de celle du bureau (portable à deux puces).
    pub offload: Option<Offload>,
    /// Carte des jeux à mesurer (MangoHud pci_dev), quand il y en a plusieurs.
    pub measured_gpu: Option<String>,
    /// Outils installés : game-performance, powerprofilesctl, gamemoderun,
    /// dlss-swapper, mangohud.
    pub tools: Vec<String>,
    /// Dossier où MangoHud enregistre chaque partie (images par seconde,
    /// charges), lu par le diagnostic des performances. Absent : pas de mesure.
    pub logs: Option<String>,
    /// Un écran est en HDR (le HDR « Automatique » des jeux le suit).
    pub hdr_screen: bool,
}

impl Context {
    pub fn has(&self, tool: &str) -> bool {
        self.tools.iter().any(|t| t == tool)
    }

    /// Ce que le lanceur tire des cartes du PC, quel qu'il soit : la marque
    /// de la carte des jeux, le jeu envoyé vers elle si elle ne dessine pas
    /// le bureau, et elle seule mesurée.
    pub fn with_gpus(mut self, gpus: &[crate::system::gpu::Gpu]) -> Self {
        use crate::system::gpu::{self, Vendor};
        let games = gpu::games_gpu(gpus);
        self.nvidia = games.is_some_and(|g| g.vendor == Vendor::Nvidia);
        self.amd = games.is_some_and(|g| g.vendor == Vendor::Amd);
        self.offload = gpu::offload(gpus).map(|g| match g.vendor {
            Vendor::Nvidia => Offload::Nvidia,
            _ => Offload::Mesa(format!("pci-{}", g.pci_slot.replace([':', '.'], "_"))),
        });
        self.measured_gpu = games.filter(|_| gpus.len() > 1).map(|g| g.pci_slot.clone());
        self
    }
}

/// Variables à poser et commandes à mettre devant le jeu.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    pub env: Vec<(String, String)>,
    /// Options DXVK (variable DXVK_CONFIG, séparées par « ; »).
    pub dxvk: Vec<String>,
    /// Options MangoHud (variable MANGOHUD_CONFIG).
    pub mangohud: Vec<String>,
    pub wrappers: Vec<String>,
}

impl Plan {
    fn set(&mut self, name: &str, value: impl Into<String>) {
        self.env.push((name.to_string(), value.into()));
    }

    /// Variables finales, celles de l'utilisateur gardées. `existing` donne
    /// la valeur actuelle d'une variable.
    pub fn environment(&self, existing: impl Fn(&str) -> Option<String>) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> =
            self.env.iter().filter(|(name, _)| existing(name).is_none()).cloned().collect();
        if !self.dxvk.is_empty() {
            let mut value = self.dxvk.join("; ");
            if let Some(user) = existing("DXVK_CONFIG").filter(|v| !v.trim().is_empty()) {
                value = format!("{value}; {user}");
            }
            out.push(("DXVK_CONFIG".into(), value));
        }
        if !self.mangohud.is_empty() {
            // Configuration de l'utilisateur d'abord, nos clés ensuite (sauf
            // « cacher » : s'il a sa configuration, il veut voir MangoHud).
            let user = existing("MANGOHUD_CONFIG")
                .filter(|v| !v.trim().is_empty())
                // MangoHud déjà demandé (MANGOHUD=1 hérité : compteur d'images
                // de go-to-linux, GOverlay…) : son fichier de configuration,
                // visible.
                .or_else(|| (existing("MANGOHUD").as_deref() == Some("1")).then(|| "read_cfg".to_string()));
            let value = match user {
                Some(user) => {
                    let ours: Vec<&str> = self
                        .mangohud
                        .iter()
                        .map(String::as_str)
                        .filter(|o| !HIDDEN.contains(o) && *o != "read_cfg")
                        .collect();
                    if ours.is_empty() { user } else { format!("{user},{}", ours.join(",")) }
                }
                None => self.mangohud.join(","),
            };
            out.push(("MANGOHUD_CONFIG".into(), value));
        }
        out
    }
}

/// Le plan de lancement pour ce profil (déjà fusionné avec les réglages globaux).
pub fn plan(profile: &GameProfile, context: &Context) -> Plan {
    let mut plan = Plan::default();
    let overlay = profile.overlay == Some(true) && context.has("mangohud");

    // Images par seconde : limiteur de DXVK (D3D9 à 11, et D3D12 par sa DXGI)
    // et de VKD3D-Proton pour Proton ; MangoHud pour un jeu natif.
    if let Some(fps) = profile.fps_limit.filter(|fps| *fps > 0) {
        if context.proton {
            plan.dxvk.push(format!("dxgi.maxFrameRate = {fps}"));
            plan.dxvk.push(format!("d3d9.maxFrameRate = {fps}"));
            plan.set("VKD3D_FRAME_RATE", fps.to_string());
        } else if context.has("mangohud") {
            plan.mangohud.push(format!("fps_limit={fps}"));
        }
    }

    match profile.vsync {
        Toggle::App => {}
        Toggle::On | Toggle::Off => {
            let on = profile.vsync == Toggle::On;
            plan.dxvk.push(format!("dxgi.syncInterval = {}", u8::from(on)));
            plan.dxvk.push(format!("d3d9.presentInterval = {}", u8::from(on)));
            plan.set("VKD3D_SWAPCHAIN_PRESENT_MODE", if on { "FIFO" } else { "IMMEDIATE" });
            if context.nvidia {
                plan.set("__GL_SYNC_TO_VBLANK", if on { "1" } else { "0" });
            } else {
                plan.set("vblank_mode", if on { "3" } else { "0" });
                plan.set("MESA_VK_WSI_PRESENT_MODE", if on { "fifo" } else { "immediate" });
            }
        }
    }

    // Faible latence : une seule image préparée d'avance (D3D9, D3D11, D3D12).
    if profile.low_latency != LowLatency::App {
        plan.dxvk.push("dxgi.maxFrameLatency = 1".into());
        plan.dxvk.push("d3d9.maxFrameLatency = 1".into());
        plan.set("VKD3D_SWAPCHAIN_LATENCY_FRAMES", "1");
        // Ultra : NVIDIA Reflex aussi pour les jeux Vulkan natifs (couche de
        // dxvk-nvapi, désactivée par défaut). Les jeux D3D11 avec Reflex l'ont
        // déjà par dxvk-nvapi, actif d'office dans Proton.
        if profile.low_latency == LowLatency::Ultra && context.nvidia {
            plan.set("DXVK_NVAPI_VKREFLEX", "1");
        }
    }

    if let Some(level) = profile.anisotropic.filter(|level| *level > 0) {
        let level = level.min(16);
        plan.dxvk.push(format!("d3d11.samplerAnisotropy = {level}"));
        plan.dxvk.push(format!("d3d9.samplerAnisotropy = {level}"));
        if context.nvidia {
            // OpenGL NVIDIA : puissance de deux (4 = 16×).
            plan.set("__GL_LOG_MAX_ANISO", (31 - level.leading_zeros()).to_string());
        } else if context.amd {
            // RADV : tous les jeux Vulkan, D3D12 compris.
            plan.set("RADV_TEX_ANISO", level.to_string());
        }
    }

    if context.nvidia {
        if profile.dlss_latest == Some(true) {
            if context.has("dlss-swapper") {
                plan.wrappers.push("dlss-swapper".into());
            } else {
                // Les variables du script dlss-swapper de CachyOS.
                plan.set("PROTON_ENABLE_NGX_UPDATER", "1");
                for kind in ["RR", "SR", "FG"] {
                    plan.set(&format!("DXVK_NVAPI_DRS_NGX_DLSS_{kind}_OVERRIDE"), "on");
                }
                for kind in ["RR", "SR"] {
                    plan.set(
                        &format!("DXVK_NVAPI_DRS_NGX_DLSS_{kind}_OVERRIDE_RENDER_PRESET_SELECTION"),
                        "render_preset_latest",
                    );
                }
            }
        }
        // Indicateur DLSS (Proton CachyOS) : retiré par Proton quand il est absent.
        if profile.dlss_indicator == Some(true) {
            plan.set("PROTON_DLSS_INDICATOR", "1");
        }
        if profile.smooth_motion == Some(true) {
            plan.set("NVPRESENT_ENABLE_SMOOTH_MOTION", "1");
        }
    }

    // HDR : Proton (10 et plus récents, Proton CachyOS, GE-Proton) le transmet
    // au bureau par le pilote Wayland de Wine : PROTON_ENABLE_HDR, avec
    // PROTON_ENABLE_WAYLAND (sans lui, le jeu part par XWayland, sans HDR) ;
    // DXVK_HDR fait voir l'écran HDR aux jeux DirectX. « Automatique » : en
    // HDR si un écran l'est.
    match profile.hdr.unwrap_or(context.hdr_screen) {
        true => {
            plan.set("PROTON_ENABLE_WAYLAND", "1");
            plan.set("PROTON_ENABLE_HDR", "1");
            plan.set("DXVK_HDR", "1");
        }
        false if profile.hdr == Some(false) => plan.set("DXVK_NO_HDR", "1"),
        false => {}
    }

    // Carte des jeux qui ne dessine pas le bureau : le jeu lui est envoyé
    // (Vulkan et OpenGL), comme `prime-run`.
    match &context.offload {
        Some(Offload::Nvidia) => {
            plan.set("__NV_PRIME_RENDER_OFFLOAD", "1");
            plan.set("__GLX_VENDOR_LIBRARY_NAME", "nvidia");
            plan.set("__VK_LAYER_NV_optimus", "NVIDIA_only");
        }
        Some(Offload::Mesa(device)) => plan.set("DRI_PRIME", device.clone()),
        None => {}
    }

    // Processeur à pleine vitesse le temps de chaque jeu, sans réglage : le
    // profil « performance » de power-profiles-daemon (game-performance de
    // CachyOS, qui échoue sans powerprofilesctl), sinon GameMode.
    if context.has("game-performance") && context.has("powerprofilesctl") {
        plan.wrappers.insert(0, "game-performance".into());
    } else if context.has("gamemoderun") {
        plan.wrappers.insert(0, "gamemoderun".into());
    }

    // Mesure de la partie par MangoHud, caché s'il n'est pas affiché : un
    // fichier par partie dans `logs` (system/sessions.rs). Un chemin avec une
    // virgule casserait MANGOHUD_CONFIG : pas de mesure alors.
    if let Some(dir) = context.logs.as_deref().filter(|dir| context.has("mangohud") && !dir.contains(',')) {
        plan.mangohud.extend([
            "autostart_log=1".to_string(),
            "log_interval=100".to_string(),
            format!("output_folder={dir}"),
        ]);
        // Plusieurs cartes : les charges et fréquences de celle des jeux.
        if let Some(slot) = &context.measured_gpu {
            plan.mangohud.push(format!("pci_dev={slot}"));
        }
    }

    // MangoHud : affiché, ou caché mais limiteur d'images d'un jeu natif, ou
    // mesure de la partie.
    let measured = plan.mangohud.iter().any(|key| key == "autostart_log=1");
    if overlay || !plan.mangohud.is_empty() {
        plan.wrappers.push("mangohud".into());
        if overlay && !plan.mangohud.is_empty() {
            // Garder la configuration de l'utilisateur (GOverlay…).
            plan.mangohud.insert(0, "read_cfg".into());
        } else if !overlay && measured {
            // Caché mais actif : avec no_display, MangoHud ne met plus ses
            // mesures à jour et autostart_log ne démarre jamais (overlay.cpp,
            // update_hud_info). Il tourne donc normalement, transparent.
            plan.mangohud.extend(["alpha=0".to_string(), "background_alpha=0".to_string()]);
        } else if !overlay {
            plan.mangohud.push("no_display".into());
        }
    }
    plan
}

/// Clés qui cachent MangoHud : jamais ajoutées à la configuration de
/// l'utilisateur (s'il en a une, il veut le voir).
const HIDDEN: [&str; 3] = ["no_display", "alpha=0", "background_alpha=0"];

/// Jeu lancé par Proton : un argument de la commande Steam est le script
/// `…/proton`.
pub fn is_proton(args: &[String]) -> bool {
    args.iter().any(|arg| arg.ends_with("/proton"))
}

/// Numéro Steam du jeu lancé (variables posées par Steam).
pub fn steam_app_id(var: impl Fn(&str) -> Option<String>) -> Option<u32> {
    ["SteamAppId", "STEAM_COMPAT_APP_ID", "SteamGameId"]
        .iter()
        .find_map(|name| var(name).and_then(|value| value.parse().ok()).filter(|id| *id > 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(proton: bool, nvidia: bool, tools: &[&str]) -> Context {
        Context {
            proton,
            nvidia,
            amd: !nvidia,
            tools: tools.iter().map(|t| t.to_string()).collect(),
            ..Context::default()
        }
    }

    fn env(plan: &Plan) -> Vec<(String, String)> {
        plan.environment(|_| None)
    }

    fn get(vars: &[(String, String)], name: &str) -> Option<String> {
        vars.iter().find(|(n, _)| n == name).map(|(_, v)| v.clone())
    }

    #[test]
    fn default_profile_changes_nothing() {
        // game-performance sans powerprofilesctl échouerait : pas utilisé.
        let plan = plan(&GameProfile::default(), &ctx(true, true, &["mangohud", "dlss-swapper", "game-performance"]));
        assert_eq!(plan, Plan::default());
    }

    #[test]
    fn games_go_to_the_games_gpu() {
        use crate::system::gpu::{Gpu, Vendor, choose_games};
        let card = |vendor, slot: &str, boot_vga, integrated| Gpu {
            card: "card".into(),
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
            connectors: vec![],
        };
        // Portable Ryzen + GTX 1650 : le jeu sur la NVIDIA, mesurée elle.
        let mut gpus =
            vec![card(Vendor::Amd, "0000:05:00.0", true, true), card(Vendor::Nvidia, "0000:01:00.0", false, false)];
        choose_games(&mut gpus);
        let mut context = ctx(true, false, &["mangohud"]).with_gpus(&gpus);
        assert!(context.nvidia && !context.amd);
        context.logs = Some("/home/j/.local/share/last1-panel/parties".into());
        let vars = env(&plan(&GameProfile::default(), &context));
        assert_eq!(get(&vars, "__NV_PRIME_RENDER_OFFLOAD").as_deref(), Some("1"));
        assert_eq!(get(&vars, "__VK_LAYER_NV_optimus").as_deref(), Some("NVIDIA_only"));
        assert!(get(&vars, "MANGOHUD_CONFIG").unwrap().contains("pci_dev=0000:01:00.0"));
        // Portable Ryzen + Radeon : Mesa, par l'adresse de la carte.
        let mut gpus =
            vec![card(Vendor::Amd, "0000:06:00.0", true, true), card(Vendor::Amd, "0000:03:00.0", false, false)];
        choose_games(&mut gpus);
        let context = ctx(true, false, &[]).with_gpus(&gpus);
        assert!(context.amd);
        assert_eq!(
            get(&env(&plan(&GameProfile::default(), &context)), "DRI_PRIME").as_deref(),
            Some("pci-0000_03_00_0")
        );
        // Une seule carte : rien de plus.
        let mut gpus = vec![card(Vendor::Nvidia, "0000:01:00.0", true, false)];
        choose_games(&mut gpus);
        let context = ctx(true, false, &[]).with_gpus(&gpus);
        assert_eq!((context.offload, context.measured_gpu), (None, None));
    }

    #[test]
    fn every_game_gets_the_cpu_at_full_speed() {
        let both = ["game-performance", "powerprofilesctl", "gamemoderun"];
        assert_eq!(plan(&GameProfile::default(), &ctx(true, true, &both)).wrappers, ["game-performance"]);
        assert_eq!(plan(&GameProfile::default(), &ctx(true, true, &both[1..])).wrappers, ["gamemoderun"]);
    }

    #[test]
    fn proton_game_with_nvidia_like_settings() {
        let profile = GameProfile {
            fps_limit: Some(141),
            vsync: Toggle::Off,
            low_latency: LowLatency::Ultra,
            anisotropic: Some(16),
            dlss_latest: Some(true),
            smooth_motion: Some(true),
            ..Default::default()
        };
        let plan =
            plan(&profile, &ctx(true, true, &["dlss-swapper", "game-performance", "powerprofilesctl", "mangohud"]));
        let vars = env(&plan);
        assert_eq!(
            get(&vars, "DXVK_CONFIG").unwrap(),
            "dxgi.maxFrameRate = 141; d3d9.maxFrameRate = 141; dxgi.syncInterval = 0; d3d9.presentInterval = 0; \
             dxgi.maxFrameLatency = 1; d3d9.maxFrameLatency = 1; d3d11.samplerAnisotropy = 16; d3d9.samplerAnisotropy = 16"
        );
        assert_eq!(get(&vars, "VKD3D_FRAME_RATE").as_deref(), Some("141"));
        assert_eq!(get(&vars, "VKD3D_SWAPCHAIN_PRESENT_MODE").as_deref(), Some("IMMEDIATE"));
        assert_eq!(get(&vars, "VKD3D_SWAPCHAIN_LATENCY_FRAMES").as_deref(), Some("1"));
        assert_eq!(get(&vars, "DXVK_NVAPI_VKREFLEX").as_deref(), Some("1"));
        assert_eq!(get(&vars, "__GL_SYNC_TO_VBLANK").as_deref(), Some("0"));
        assert_eq!(get(&vars, "__GL_LOG_MAX_ANISO").as_deref(), Some("4"));
        assert_eq!(get(&vars, "NVPRESENT_ENABLE_SMOOTH_MOTION").as_deref(), Some("1"));
        // Proton : limiteur de DXVK, pas MangoHud.
        assert_eq!(plan.wrappers, ["game-performance", "dlss-swapper"]);
        assert!(get(&vars, "MANGOHUD_CONFIG").is_none());
    }

    #[test]
    fn dlss_without_cachyos_script_sets_its_variables() {
        let profile = GameProfile { dlss_latest: Some(true), ..Default::default() };
        let vars = env(&plan(&profile, &ctx(true, true, &[])));
        assert_eq!(get(&vars, "PROTON_ENABLE_NGX_UPDATER").as_deref(), Some("1"));
        assert_eq!(get(&vars, "DXVK_NVAPI_DRS_NGX_DLSS_SR_OVERRIDE").as_deref(), Some("on"));
        assert_eq!(
            get(&vars, "DXVK_NVAPI_DRS_NGX_DLSS_SR_OVERRIDE_RENDER_PRESET_SELECTION").as_deref(),
            Some("render_preset_latest")
        );
        // Rien de NVIDIA sur une carte AMD.
        assert!(env(&plan(&profile, &ctx(true, false, &[]))).is_empty());
    }

    #[test]
    fn native_game_uses_mangohud_hidden_as_limiter() {
        let profile =
            GameProfile { fps_limit: Some(60), vsync: Toggle::On, anisotropic: Some(8), ..Default::default() };
        let plan = plan(&profile, &ctx(false, false, &["mangohud", "gamemoderun"]));
        let vars = env(&plan);
        assert_eq!(get(&vars, "MANGOHUD_CONFIG").as_deref(), Some("fps_limit=60,no_display"));
        // Sans power-profiles-daemon : GameMode pour la pleine vitesse.
        assert_eq!(plan.wrappers, ["gamemoderun", "mangohud"]);
        assert_eq!(get(&vars, "vblank_mode").as_deref(), Some("3"));
        assert_eq!(get(&vars, "MESA_VK_WSI_PRESENT_MODE").as_deref(), Some("fifo"));
        assert_eq!(get(&vars, "RADV_TEX_ANISO").as_deref(), Some("8"));
    }

    #[test]
    fn overlay_keeps_the_user_mangohud_config() {
        let shown = GameProfile { overlay: Some(true), ..Default::default() };
        let plan_shown = plan(&shown, &ctx(true, true, &["mangohud"]));
        assert_eq!(plan_shown.wrappers, ["mangohud"]);
        assert!(get(&env(&plan_shown), "MANGOHUD_CONFIG").is_none(), "fichier de MangoHud utilisé tel quel");

        let limited = GameProfile { overlay: Some(true), fps_limit: Some(90), ..Default::default() };
        let vars = env(&plan(&limited, &ctx(false, true, &["mangohud"])));
        assert_eq!(get(&vars, "MANGOHUD_CONFIG").as_deref(), Some("read_cfg,fps_limit=90"));

        // MangoHud absent : pas d'affichage, et rien ne casse.
        assert!(plan(&shown, &ctx(true, true, &[])).wrappers.is_empty());
    }

    #[test]
    fn every_game_is_measured_by_hidden_mangohud() {
        let mut context = ctx(true, true, &["mangohud"]);
        context.logs = Some("/home/j/.local/share/last1-panel/parties".into());
        let plan = plan(&GameProfile::default(), &context);
        assert_eq!(plan.wrappers, ["mangohud"]);
        assert_eq!(
            get(&env(&plan), "MANGOHUD_CONFIG").as_deref(),
            Some(
                "autostart_log=1,log_interval=100,output_folder=/home/j/.local/share/last1-panel/parties,alpha=0,background_alpha=0"
            )
        );
        // Jamais no_display avec la mesure : MangoHud ne démarrerait pas
        // l'enregistrement.
        assert!(!get(&env(&plan), "MANGOHUD_CONFIG").unwrap().contains("no_display"));
        // MANGOHUD_CONFIG de l'utilisateur : il veut le voir, rien de caché.
        let vars = plan.environment(|name| (name == "MANGOHUD_CONFIG").then(|| "fps,gpu_stats".to_string()));
        let config = get(&vars, "MANGOHUD_CONFIG").unwrap();
        assert!(config.starts_with("fps,gpu_stats,autostart_log=1") && !config.contains("alpha=0"), "{config}");
        // MANGOHUD=1 hérité (compteur d'images de go-to-linux, GOverlay) : son
        // fichier de configuration, visible, et la partie mesurée.
        let vars = plan.environment(|name| (name == "MANGOHUD").then(|| "1".to_string()));
        let config = get(&vars, "MANGOHUD_CONFIG").unwrap();
        assert!(config.starts_with("read_cfg,autostart_log=1"), "{config}");
        assert!(!config.contains("alpha=0") && !config.contains("no_display"), "{config}");
        // Affiché : la configuration de l'utilisateur, plus la mesure.
        let shown = GameProfile { overlay: Some(true), ..Default::default() };
        let vars = env(&super::plan(&shown, &context));
        assert!(get(&vars, "MANGOHUD_CONFIG").unwrap().starts_with("read_cfg,autostart_log=1"));
        assert!(!get(&vars, "MANGOHUD_CONFIG").unwrap().contains("alpha=0"));
        // Sans MangoHud : rien.
        context.tools.clear();
        assert_eq!(super::plan(&GameProfile::default(), &context), Plan::default());
    }

    #[test]
    fn user_variables_win() {
        let profile = GameProfile { vsync: Toggle::Off, hdr: Some(true), ..Default::default() };
        let plan = plan(&profile, &ctx(true, true, &[]));
        let vars = plan.environment(|name| match name {
            "DXVK_CONFIG" => Some("dxgi.syncInterval = 1".into()),
            "PROTON_ENABLE_WAYLAND" => Some("0".into()),
            _ => None,
        });
        assert_eq!(
            get(&vars, "DXVK_CONFIG").as_deref(),
            Some("dxgi.syncInterval = 0; d3d9.presentInterval = 0; dxgi.syncInterval = 1")
        );
        assert!(get(&vars, "PROTON_ENABLE_WAYLAND").is_none(), "déjà posée par l'utilisateur");
        assert_eq!(get(&vars, "DXVK_HDR").as_deref(), Some("1"));
        assert_eq!(get(&vars, "PROTON_ENABLE_HDR").as_deref(), Some("1"));
    }

    #[test]
    fn automatic_hdr_follows_the_screen() {
        let auto = GameProfile::default();
        let mut context = ctx(true, false, &[]);
        assert!(get(&env(&plan(&auto, &context)), "PROTON_ENABLE_HDR").is_none());
        // Écran en HDR : le jeu aussi, par le pilote Wayland de Wine.
        context.hdr_screen = true;
        let vars = env(&plan(&auto, &context));
        for name in ["PROTON_ENABLE_WAYLAND", "PROTON_ENABLE_HDR", "DXVK_HDR"] {
            assert_eq!(get(&vars, name).as_deref(), Some("1"), "{name}");
        }
        // Désactivé pour ce jeu : jamais.
        let off = GameProfile { hdr: Some(false), ..Default::default() };
        let vars = env(&plan(&off, &context));
        assert!(get(&vars, "PROTON_ENABLE_HDR").is_none());
        assert_eq!(get(&vars, "DXVK_NO_HDR").as_deref(), Some("1"));
    }

    #[test]
    fn user_mangohud_config_keeps_our_limiter() {
        let limited = GameProfile { fps_limit: Some(60), ..Default::default() };
        let plan = plan(&limited, &ctx(false, true, &["mangohud"]));
        let vars = plan.environment(|name| (name == "MANGOHUD_CONFIG").then(|| "fps,frametime".to_string()));
        assert_eq!(get(&vars, "MANGOHUD_CONFIG").as_deref(), Some("fps,frametime,fps_limit=60"));
    }

    #[test]
    fn recognises_proton_and_app_ids() {
        let args: Vec<String> = [
            "/home/j/.local/share/Steam/ubuntu12_32/reaper",
            "SteamLaunch",
            "AppId=1091500",
            "--",
            "/usr/share/steam/compatibilitytools.d/proton-cachyos/proton",
            "waitforexitandrun",
            "/games/Cyberpunk2077.exe",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert!(is_proton(&args));
        assert!(!is_proton(&["/games/factorio/bin/x64/factorio".to_string()]));
        assert_eq!(steam_app_id(|name| (name == "SteamAppId").then(|| "1091500".to_string())), Some(1091500));
        assert_eq!(steam_app_id(|name| (name == "SteamAppId").then(|| "0".to_string())), None);
        assert_eq!(steam_app_id(|_| None), None);
    }
}
