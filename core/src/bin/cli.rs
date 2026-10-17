//! last1-panel-cli : ce qui tourne sans la fenêtre.
//!
//!   last1-panel-cli run <commande du jeu…>   lanceur des jeux (option Steam
//!                                            « last1-panel-cli run %command% »)
//!   last1-panel-cli apply [--delay S]        réglages remis maintenant
//!                                            (vibrance, profils de couleur)
//!   last1-panel-cli watch                    service de la session : réglages
//!                                            remis à la connexion, puis à
//!                                            chaque écran branché ou rallumé

use last1_panel_core::games::launch::{self, Context};
use last1_panel_core::i18n::{self, t};
use last1_panel_core::panel::Panel;
use last1_panel_core::settings::Settings;
use last1_panel_core::system::gpu::{self, SysPaths, Vendor};
use std::io::{BufRead, BufReader};
use std::os::unix::process::CommandExt;
use std::process::{Command, ExitCode, Stdio};
use std::sync::mpsc;
use std::time::Duration;

fn main() -> ExitCode {
    i18n::init_from_env();
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("run") => run(&args[1..]),
        Some("apply") => apply(&args[1..]),
        Some("watch") => watch(),
        _ => {
            eprintln!(
                "{}",
                t(
                    "Utilisation : last1-panel-cli run <commande du jeu…> | apply [--delay secondes] | watch",
                    "Usage: last1-panel-cli run <game command…> | apply [--delay seconds] | watch",
                )
            );
            ExitCode::from(2)
        }
    }
}

/// Lance le jeu avec nos réglages. Quoi qu'il arrive, le jeu est lancé :
/// sans réglages lisibles, il part tel quel.
fn run(command: &[String]) -> ExitCode {
    let Some((program, rest)) = command.split_first() else {
        eprintln!("last1-panel-cli run : {}", t("commande du jeu manquante", "missing game command"));
        return ExitCode::from(2);
    };
    let settings = Settings::load();
    let profile = match launch::steam_app_id(|name| std::env::var(name).ok()) {
        Some(id) => settings.games.per_game.get(&id).cloned().unwrap_or_default().merged_over(&settings.games.global),
        None => settings.games.global.clone(),
    };
    let gpus = gpu::detect(&SysPaths::default());
    let nvidia = gpus.iter().any(|g| g.vendor == Vendor::Nvidia);
    let context = Context {
        proton: launch::is_proton(command),
        nvidia,
        amd: !nvidia && gpus.iter().any(|g| g.vendor == Vendor::Amd),
        tools: ["game-performance", "gamemoderun", "dlss-swapper", "mangohud"]
            .iter()
            .filter(|tool| last1_panel_core::exec::which(tool).is_some())
            .map(|tool| tool.to_string())
            .collect(),
    };
    let plan = launch::plan(&profile, &context);
    let env = plan.environment(|name| std::env::var(name).ok());
    log_launch(command, &env, &plan.wrappers);

    let mut full: Vec<&String> = plan.wrappers.iter().collect();
    full.push(program);
    full.extend(rest);
    let error = Command::new(full[0]).args(&full[1..]).envs(env.iter().map(|(k, v)| (k, v))).exec();
    // Outil chaîné introuvable : le jeu seul, avec nos variables.
    eprintln!("last1-panel-cli : {error}");
    let error = Command::new(program).args(rest).envs(env.iter().map(|(k, v)| (k, v))).exec();
    eprintln!("last1-panel-cli : {program} : {error}");
    ExitCode::from(127)
}

/// Dernier lancement noté dans ~/.cache/last1-panel/last-launch.txt (pour
/// vérifier ce qui a été appliqué à un jeu).
fn log_launch(command: &[String], env: &[(String, String)], wrappers: &[String]) {
    let home = last1_panel_core::paths::home_dir();
    let dir = std::env::var_os("XDG_CACHE_HOME").map(std::path::PathBuf::from).unwrap_or_else(|| home.join(".cache"));
    let dir = dir.join("last1-panel");
    let mut text = format!("SteamAppId={}\n", std::env::var("SteamAppId").unwrap_or_default());
    for (name, value) in env {
        text.push_str(&format!("{name}={value}\n"));
    }
    text.push_str(&format!("{}\n", wrappers.iter().chain(command).cloned().collect::<Vec<_>>().join(" ")));
    let _ = std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(dir.join("last-launch.txt"), text));
}

fn apply(args: &[String]) -> ExitCode {
    if let Some(index) = args.iter().position(|a| a == "--delay")
        && let Some(seconds) = args.get(index + 1).and_then(|s| s.parse::<u64>().ok())
    {
        std::thread::sleep(Duration::from_secs(seconds.min(60)));
    }
    if apply_until_ready(&Panel::system(), 10) { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

/// Réglages remis ; un écran pas encore allumé par le bureau (juste après la
/// connexion, ou au réveil) est réessayé toutes les 3 s.
fn apply_until_ready(panel: &Panel, tries: u32) -> bool {
    for attempt in 1..=tries {
        let problems = panel.apply_saved();
        if problems.is_empty() {
            return true;
        }
        if attempt == tries {
            for problem in &problems {
                eprintln!("last1-panel-cli : {problem}");
            }
        }
        std::thread::sleep(Duration::from_secs(3));
    }
    false
}

/// Service de la session (last1-panel-apply.service) : la vibrance NVIDIA
/// est perdue au redémarrage et quand un écran est rebranché ; elle est
/// remise à la connexion puis à chaque événement des cartes graphiques
/// (`udevadm monitor`, sous-système drm).
fn watch() -> ExitCode {
    let panel = Panel::system();
    std::thread::sleep(Duration::from_secs(3));
    apply_until_ready(&panel, 20);
    let child = Command::new("udevadm")
        .args(["monitor", "--kernel", "--subsystem-match=drm"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = child else {
        eprintln!("last1-panel-cli watch : udevadm introuvable, réglages remis une seule fois");
        return ExitCode::SUCCESS;
    };
    let (events, received) = mpsc::channel();
    let stdout = child.stdout.take().expect("stdout demandé");
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if line.contains(" change ") && events.send(()).is_err() {
                break;
            }
        }
    });
    // Un branchement donne plusieurs événements : on attend qu'ils cessent.
    while received.recv().is_ok() {
        while received.recv_timeout(Duration::from_secs(2)).is_ok() {}
        apply_until_ready(&panel, 5);
    }
    let _ = child.kill();
    ExitCode::SUCCESS
}
