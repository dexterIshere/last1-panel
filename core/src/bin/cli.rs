//! last1-panel-cli : ce qui tourne sans la fenêtre.
//!
//!   last1-panel-cli run <commande du jeu…>   lanceur des jeux (option Steam
//!                                            « last1-panel-cli run %command% »)
//!   last1-panel-cli apply [--delay S]        réglages remis maintenant
//!                                            (vibrance, profils de couleur)
//!   last1-panel-cli watch                    service de la session : réglages
//!                                            remis à la connexion, puis à
//!                                            chaque écran branché ou rallumé ;
//!                                            AppImage tenue à jour
//!   last1-panel-cli update                   AppImage mise à jour maintenant

use last1_panel_core::error::CoreResult;
use last1_panel_core::exec::SystemExec;
use last1_panel_core::games::launch::{self, Context};
use last1_panel_core::i18n::{self, t};
use last1_panel_core::panel::Panel;
use last1_panel_core::settings::Settings;
use last1_panel_core::system::gpu::{self, SysPaths, Vendor};
use last1_panel_core::update;
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
        Some("update") => update_now(),
        _ => {
            eprintln!(
                "{}",
                t(
                    "Utilisation : last1-panel-cli run <commande du jeu…> | apply [--delay secondes] | watch | update",
                    "Usage: last1-panel-cli run <game command…> | apply [--delay seconds] | watch | update",
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
    if apply_until_ready(10) { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

/// Variables que le bureau a données à systemd (Hyprland par
/// dbus-update-activation-environment, Plasma de lui-même) : le service,
/// lancé avant le bureau, les relit à chaque passage.
fn session_env() -> Vec<(String, String)> {
    let text = Command::new("systemctl")
        .args(["--user", "show-environment"])
        .stderr(Stdio::null())
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
        .unwrap_or_default();
    text.lines().filter_map(|line| line.split_once('=')).map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

fn session_panel() -> Panel {
    Panel::for_session(session_env().into_iter().collect())
}

/// Réglages remis ; un écran pas encore allumé par le bureau (juste après la
/// connexion, ou au réveil) est réessayé toutes les 3 s.
fn apply_until_ready(tries: u32) -> bool {
    for attempt in 1..=tries {
        let problems = session_panel().apply_saved();
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
    if update::enabled() {
        std::thread::spawn(keep_up_to_date);
    }
    std::thread::sleep(Duration::from_secs(3));
    apply_until_ready(20);
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
        apply_until_ready(5);
    }
    let _ = child.kill();
    ExitCode::SUCCESS
}

/// Installation par l'AppImage tenue à jour (update.rs) : cinq minutes après
/// la connexion, puis chaque jour ; une heure plus tard en cas d'échec (pas
/// d'Internet). Une fois la mise à jour faite, le service s'arrête en erreur
/// et systemd le relance sur le nouvel outil.
fn keep_up_to_date() {
    std::thread::sleep(Duration::from_secs(300));
    loop {
        let wait = match update_appimage() {
            Ok(Some(version)) => {
                notify(&t(
                    &format!("Last1 Panel est à jour (version {version})."),
                    &format!("Last1 Panel is up to date (version {version})."),
                ));
                std::process::exit(75);
            }
            Ok(None) => Duration::from_secs(24 * 3600),
            Err(error) => {
                eprintln!("last1-panel-cli : mise à jour : {error}");
                Duration::from_secs(3600)
            }
        };
        std::thread::sleep(wait);
    }
}

/// `last1-panel-cli update` : la même chose, tout de suite.
fn update_now() -> ExitCode {
    if !update::enabled() {
        eprintln!(
            "{}",
            t(
                "Rien à faire : seule l'installation par l'AppImage se met à jour ainsi (le paquet Arch, par go-to-linux ou pacman).",
                "Nothing to do: only the AppImage install updates this way (the Arch package, through go-to-linux or pacman).",
            )
        );
        return ExitCode::SUCCESS;
    }
    match update_appimage() {
        Ok(Some(version)) => {
            println!("{}", t(&format!("Version {version} installée."), &format!("Version {version} installed.")));
            ExitCode::SUCCESS
        }
        Ok(None) => {
            println!("{}", t("Déjà à jour.", "Already up to date."));
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("last1-panel-cli : {error}");
            ExitCode::FAILURE
        }
    }
}

/// Version installée, s'il y en avait une plus récente.
fn update_appimage() -> CoreResult<Option<String>> {
    let exec = SystemExec { timeout: Duration::from_secs(90), ..SystemExec::default() };
    let Some(release) = update::check(&exec, update::now())? else { return Ok(None) };
    update::install(&release)?;
    Ok(Some(release.version))
}

fn notify(message: &str) {
    let _ = Command::new("notify-send")
        .args(["--app-name=Last1 Panel", "--icon=last1-panel", "Last1 Panel", message])
        .envs(session_env())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}
