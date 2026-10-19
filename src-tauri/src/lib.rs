//! Last1 Panel : la fenêtre. L'interface (Svelte) appelle les commandes de
//! `commands.rs`, qui passent au cœur (crate last1-panel-core).

mod commands;
mod install;

use install::{Layout, Outcome};
use last1_panel_core::exec::{Exec, SystemExec};
use last1_panel_core::i18n::t;
use last1_panel_core::panel::Panel;
use last1_panel_core::system::gpu::{self, SysPaths, Vendor};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::sync::Arc;
use tauri::Manager;

pub fn run() {
    last1_panel_core::i18n::init_from_env();
    // Mise à jour par le service de session : la nouvelle AppImage s'installe
    // (outil des jeux, nvibrant), sans fenêtre.
    if std::env::args().nth(1).as_deref() == Some("--integrate") {
        std::process::exit(integrate_only());
    }
    if install::running_appimage().is_some() && Path::new(install::SYSTEM_BINARY).is_file() {
        hand_over_to_package();
    }
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
            integrate_appimage();
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
            commands::color_profiles,
            commands::save_color_profile,
            commands::delete_color_profile,
            commands::apply_color_profile,
            commands::set_tray,
            commands::link_game_color,
            commands::close_steam_and_wrap,
            commands::performance_report,
            commands::performance_fix,
            commands::update_available,
            commands::install_update,
            commands::quit,
            commands::sharing,
            commands::set_sharing,
        ])
        .run(tauri::generate_context!());
    if let Err(error) = result {
        eprintln!("Last1 Panel : démarrage impossible : {error}");
        std::process::exit(1);
    }
}

/// Ouvert depuis l'AppImage alors que le paquet est installé (go-to-linux) :
/// ce qu'avait posé l'AppImage est retiré, puis Last1 Panel du paquet prend
/// la place, avec un environnement sans celui de l'AppImage.
fn hand_over_to_package() -> ! {
    if let Some(layout) = Layout::for_user() {
        match install::yield_to_package(&layout) {
            Ok(true) => restart_service(),
            Ok(false) => {}
            Err(error) => eprintln!("Last1 Panel : fichiers de l'AppImage non retirés : {error}"),
        }
    }
    let mut command = std::process::Command::new(install::SYSTEM_BINARY);
    command.args(std::env::args_os().skip(1));
    last1_panel_core::exec::without_appimage_env(&mut command);
    let error = command.exec();
    eprintln!("Last1 Panel : {} introuvable : {error}", install::SYSTEM_BINARY);
    std::process::exit(1);
}

/// AppImage : installée ou mise à jour pour l'utilisateur (menu, outil des
/// jeux, service de session), sans retarder l'ouverture de la fenêtre.
fn integrate_appimage() {
    let (Some((appimage, appdir)), Some(layout)) = (install::running_appimage(), Layout::for_user()) else {
        return;
    };
    let spawned = std::thread::Builder::new().name("last1-panel-install".into()).spawn(move || {
        match install::integrate(&appimage, &appdir, &layout) {
            Ok(report) => {
                if report.restart_service {
                    restart_service();
                }
                if report.outcome == Outcome::Installed {
                    notify(&t(
                        "Last1 Panel est installé : il est dans le menu des applications, et vos réglages reviennent à chaque connexion.",
                        "Last1 Panel is installed: it's in the applications menu, and your settings come back at every login.",
                    ));
                }
            }
            Err(error) => eprintln!("Last1 Panel : installation impossible : {error}"),
        }
    });
    if let Err(error) = spawned {
        eprintln!("Last1 Panel : installation non lancée : {error}");
    }
}

/// `last1-panel --integrate` : installation seule, code de sortie 0 si faite.
/// Le service qui l'a lancée se relance ensuite lui-même.
fn integrate_only() -> i32 {
    let (Some((appimage, appdir)), Some(layout)) = (install::running_appimage(), Layout::for_user()) else {
        eprintln!("Last1 Panel : --integrate ne sert que depuis l'AppImage");
        return 2;
    };
    match install::integrate(&appimage, &appdir, &layout) {
        Ok(report) => {
            if report.restart_service {
                systemctl(&["daemon-reload"]);
            }
            0
        }
        Err(error) => {
            eprintln!("Last1 Panel : installation impossible : {error}");
            1
        }
    }
}

/// Service de session relu et relancé (nouvel outil, ou celui du paquet).
pub(crate) fn restart_service() {
    systemctl(&["daemon-reload"]);
    systemctl(&["restart", "last1-panel-apply.service"]);
}

/// Service de session arrêté (l'icône près de l'horloge disparaît) ; il
/// revient à la prochaine connexion.
pub(crate) fn stop_service() {
    systemctl(&["--no-block", "stop", "last1-panel-apply.service"]);
}

fn systemctl(args: &[&str]) {
    let args: Vec<String> = ["--user"].iter().chain(args).map(|arg| arg.to_string()).collect();
    if let Err(error) = SystemExec::default().run_ok("systemctl", &args) {
        eprintln!("Last1 Panel : systemctl {} : {error}", args.join(" "));
    }
}

fn notify(message: &str) {
    let args = ["--app-name=Last1 Panel", "--icon=last1-panel", "Last1 Panel", message].map(String::from);
    let _ = SystemExec::default().run("notify-send", &args);
}
