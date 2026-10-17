//! Last1 Panel : la fenêtre. L'interface (Svelte) appelle les commandes de
//! `commands.rs`, qui passent au cœur (crate last1-panel-core).

mod commands;

use last1_panel_core::panel::Panel;
use last1_panel_core::system::gpu::{self, SysPaths, Vendor};
use std::sync::Arc;
use tauri::Manager;

pub fn run() {
    last1_panel_core::i18n::init_from_env();
    // WebKitGTK affiche une fenêtre vide avec le pilote NVIDIA sous Wayland
    // quand il partage ses images par DMA-BUF : rendu classique dans ce cas.
    let nvidia = gpu::detect(&SysPaths::default()).iter().any(|g| g.vendor == Vendor::Nvidia);
    if nvidia && std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        // SAFETY : avant tout autre fil d'exécution (début de main).
        unsafe { std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1") };
    }

    let result = tauri::Builder::default()
        // Une seule fenêtre : une seconde ouverture ramène la première.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .setup(|app| {
            app.manage(Arc::new(Panel::system()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::system_info,
            commands::set_language,
            commands::outputs,
            commands::apply_outputs,
            commands::color_state,
            commands::set_color,
            commands::set_vibrance,
            commands::gpu_devices,
            commands::gpu_stats,
            commands::gpu_tuning,
            commands::gpu_try,
            commands::gpu_confirm,
            commands::enable_lact,
            commands::games,
            commands::save_game_settings,
            commands::set_wrapped,
        ])
        .run(tauri::generate_context!());
    if let Err(error) = result {
        eprintln!("Last1 Panel : démarrage impossible : {error}");
        std::process::exit(1);
    }
}
