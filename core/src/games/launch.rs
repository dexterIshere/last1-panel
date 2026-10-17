//! Lanceur des jeux : `last1-panel-cli run %command%` dans les options de
//! lancement Steam. Il lit nos réglages, pose les variables que lisent
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

/// Ce que le lanceur sait du jeu et du PC.
#[derive(Debug, Clone, Default)]
pub struct Context {
    /// Jeu lancé par Proton (un argument se termine par « /proton »).
    pub proton: bool,
    pub nvidia: bool,
    pub amd: bool,
    /// Outils installés : game-performance, gamemoderun, dlss-swapper, mangohud.
    pub tools: Vec<String>,
}

impl Context {
    pub fn has(&self, tool: &str) -> bool {
        self.tools.iter().any(|t| t == tool)
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
            let value = match existing("MANGOHUD_CONFIG").filter(|v| !v.trim().is_empty()) {
                Some(user) => {
                    let ours: Vec<&str> = self
                        .mangohud
                        .iter()
                        .map(String::as_str)
                        .filter(|o| *o != "no_display" && *o != "read_cfg")
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

    // HDR : pilote Wayland de Wine (Proton CachyOS, GE-Proton), qui transmet le
    // HDR au bureau, et HDR de DXVK.
    match profile.hdr {
        Some(true) => {
            plan.set("PROTON_ENABLE_WAYLAND", "1");
            plan.set("DXVK_HDR", "1");
        }
        Some(false) => plan.set("DXVK_NO_HDR", "1"),
        None => {}
    }

    if profile.performance_mode == Some(true) {
        if context.has("game-performance") {
            plan.wrappers.insert(0, "game-performance".into());
        } else if context.has("gamemoderun") {
            plan.wrappers.insert(0, "gamemoderun".into());
        }
    }

    // MangoHud : affiché, ou caché mais limiteur d'images d'un jeu natif.
    if overlay || !plan.mangohud.is_empty() {
        plan.wrappers.push("mangohud".into());
        if overlay && !plan.mangohud.is_empty() {
            // Garder la configuration de l'utilisateur (GOverlay…).
            plan.mangohud.insert(0, "read_cfg".into());
        } else if !overlay {
            plan.mangohud.push("no_display".into());
        }
    }
    plan
}

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
        Context { proton, nvidia, amd: !nvidia, tools: tools.iter().map(|t| t.to_string()).collect() }
    }

    fn env(plan: &Plan) -> Vec<(String, String)> {
        plan.environment(|_| None)
    }

    fn get(vars: &[(String, String)], name: &str) -> Option<String> {
        vars.iter().find(|(n, _)| n == name).map(|(_, v)| v.clone())
    }

    #[test]
    fn default_profile_changes_nothing() {
        let plan = plan(&GameProfile::default(), &ctx(true, true, &["mangohud", "dlss-swapper", "game-performance"]));
        assert_eq!(plan, Plan::default());
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
            performance_mode: Some(true),
            ..Default::default()
        };
        let plan = plan(&profile, &ctx(true, true, &["dlss-swapper", "game-performance", "mangohud"]));
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
        assert_eq!(plan.wrappers, ["mangohud"]);
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
