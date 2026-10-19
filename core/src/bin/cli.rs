//! last1-panel-cli : ce qui tourne sans la fenêtre.
//!
//!   last1-panel-cli run <commande du jeu…>   lanceur des jeux (option Steam
//!                                            « $(command -v last1-panel-cli
//!                                            && echo run) %command% »)
//!   last1-panel-cli apply [--delay S]        réglages remis maintenant
//!                                            (vibrance, profils de couleur)
//!   last1-panel-cli watch                    service de la session : réglages
//!                                            remis à la connexion, puis à
//!                                            chaque écran branché ou rallumé ;
//!                                            icône près de l'horloge ;
//!                                            AppImage tenue à jour
//!   last1-panel-cli update                   AppImage mise à jour maintenant
//!   last1-panel-cli diagnostic [--json]      performances en jeu : ce qui
//!                                            freine ce PC (rapport à copier,
//!                                            ou JSON pour go-to-linux)
//!   last1-panel-cli fix wrap-games           option de lancement sur tous
//!                                            les jeux Steam, Steam fermé
//!                                            (go-to-linux)

use last1_panel_core::error::CoreResult;
use last1_panel_core::exec::SystemExec;
use last1_panel_core::games::launch::{self, Context};
use last1_panel_core::i18n::{self, t};
use last1_panel_core::panel::Panel;
use last1_panel_core::settings::Settings;
use last1_panel_core::system::desktop;
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
        Some("diagnostic") => {
            let report = Panel::system().performance_report();
            // --json : pour go-to-linux, qui l'envoie à last1.fun.
            if args.get(1).is_some_and(|a| a == "--json") {
                println!("{}", serde_json::to_string(&report).expect("rapport sérialisable"));
            } else {
                print!("{}", report.text);
            }
            ExitCode::SUCCESS
        }
        Some("fix") if args.get(1).is_some_and(|a| a == "wrap-games") => wrap_games(),
        _ => {
            eprintln!(
                "{}",
                t(
                    "Utilisation : last1-panel-cli run <commande du jeu…> | apply [--delay secondes] | watch | update | diagnostic [--json] | fix wrap-games",
                    "Usage: last1-panel-cli run <game command…> | apply [--delay seconds] | watch | update | diagnostic [--json] | fix wrap-games",
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
        logs: game_logs(),
        // Le HDR « Automatique » suit l'écran : lu par le bureau (hyprctl,
        // kscreen-doctor), dans la session de Steam.
        hdr_screen: Panel::session()
            .outputs()
            .is_ok_and(|outputs| outputs.iter().any(|output| output.enabled && output.hdr == Some(true))),
    };
    let mut plan = launch::plan(&profile, &context);
    // Profil de couleurs du jeu, dans sa seule fenêtre (vkBasalt) ; en cas
    // d'échec, le jeu part avec les couleurs de l'écran.
    if let Some(color) = profile.color_profile.as_deref().and_then(|id| settings.color_profile(id)) {
        match last1_panel_core::color::lut::game_environment(&settings, color) {
            Ok(vars) => plan.env.extend(vars),
            Err(error) => eprintln!("last1-panel-cli : profil de couleurs : {error}"),
        }
    }
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

/// Notre option de lancement sur tous les jeux Steam installés, pour tous les
/// comptes, sans jamais fermer Steam : go-to-linux l'appelle au démarrage du
/// PC puis tant que Steam est fermé. Sortie : le nombre d'options changées.
/// Code 0 : fait, ou rien à faire ; 3 : Steam est lancé, rien n'est écrit ;
/// 1 : erreur.
fn wrap_games() -> ExitCode {
    use last1_panel_core::games::steam;
    let proc_dir = std::path::Path::new("/proc");
    let nothing = |why: String| {
        eprintln!("{why}");
        println!("0");
        ExitCode::SUCCESS
    };
    let Some(root) = steam::find_root(&last1_panel_core::paths::home_dir()) else {
        return nothing(t("Steam n'est pas installé : rien à faire.", "Steam is not installed: nothing to do."));
    };
    if steam::is_flatpak(&root) {
        return nothing(t(
            "Steam est en Flatpak : un jeu ne pourrait pas lancer Last1 Panel depuis son bac à sable.",
            "Steam is a Flatpak: a game could not start Last1 Panel from its sandbox.",
        ));
    }
    if steam::local_configs(&root).is_empty() {
        return nothing(t(
            "Aucun compte Steam sur ce PC : rien à faire.",
            "No Steam account on this PC: nothing to do.",
        ));
    }
    if steam::is_running(proc_dir) {
        eprintln!("{}", t("Steam est lancé : rien n'est écrit.", "Steam is running: nothing is written."));
        return ExitCode::from(3);
    }
    let ids: Vec<u32> = steam::installed_games(&root).iter().map(|game| game.app_id).collect();
    match steam::set_wrapped(&root, proc_dir, &ids, true) {
        Ok(changed) => {
            println!("{changed}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("last1-panel-cli fix wrap-games : {error}");
            ExitCode::FAILURE
        }
    }
}

/// Dossier des mesures des parties, créé, les 30 derniers journaux gardés.
fn game_logs() -> Option<String> {
    let dir = last1_panel_core::paths::game_logs_dir();
    std::fs::create_dir_all(&dir).ok()?;
    last1_panel_core::system::sessions::prune(&dir, 30);
    dir.to_str().map(str::to_string)
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

/// Réglages remis ; un écran pas encore allumé par le bureau (juste après la
/// connexion, ou au réveil) est réessayé toutes les 3 s.
fn apply_until_ready(tries: u32) -> bool {
    for attempt in 1..=tries {
        let problems = Panel::session().apply_saved();
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
/// (`udevadm monitor`, sous-système drm). Il tient aussi l'icône près de
/// l'horloge (tray.rs).
fn watch() -> ExitCode {
    if update::enabled() {
        std::thread::spawn(propose_updates);
    }
    std::thread::spawn(share_reports);
    std::thread::spawn(watch_updates);
    let tray = last1_panel_core::tray::spawn();
    std::thread::sleep(Duration::from_secs(3));
    apply_until_ready(20);
    let child = Command::new("udevadm")
        .args(["monitor", "--kernel", "--subsystem-match=drm"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = child else {
        eprintln!("last1-panel-cli watch : udevadm introuvable, réglages remis une seule fois");
        // L'icône reste.
        loop {
            std::thread::park();
        }
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
        tray.screens_changed();
    }
    let _ = child.kill();
    ExitCode::SUCCESS
}

/// Mises à jour des paquets (system/packages.rs) : `checkupdates` cinq
/// minutes après la connexion, puis toutes les 6 h ; le diagnostic dit alors
/// tout de suite si le pilote graphique a une version plus récente.
fn watch_updates() {
    if last1_panel_core::exec::which("checkupdates").is_none() {
        return;
    }
    std::thread::sleep(Duration::from_secs(300));
    let cache = last1_panel_core::paths::cache_dir().join("updates.txt");
    loop {
        let exec = SystemExec { timeout: Duration::from_secs(300), ..SystemExec::default() };
        last1_panel_core::system::packages::refresh(&exec, &cache);
        std::thread::sleep(Duration::from_secs(6 * 3600));
    }
}

/// Rapports de performances partagés avec last1.fun (system/sharing.rs) :
/// seulement avec l'accord du joueur, et hors go-to-linux (qui envoie les
/// siens). Regardé dix minutes après la connexion, puis toutes les 30 minutes ;
/// envoyé au plus une fois par jour, après des parties, s'il a changé.
fn share_reports() {
    use last1_panel_core::system::sharing;
    std::thread::sleep(Duration::from_secs(600));
    let state_path = last1_panel_core::paths::cache_dir().join("sharing.json");
    let read = |path: &str| std::fs::read_to_string(path).unwrap_or_default().trim().to_string();
    loop {
        if Settings::load().share_reports == Some(true) && !std::path::Path::new("/var/lib/gtl").exists() {
            let report = Panel::system().performance_report();
            let mut state = sharing::State::load(&state_path);
            let now = update::now();
            if let Some((sum, occasion)) = sharing::due(&report, &state, now) {
                let gpus = gpu::detect(&SysPaths::default());
                let body = sharing::body(
                    &report,
                    occasion,
                    update::RELEASE.unwrap_or(env!("CARGO_PKG_VERSION")),
                    &sharing::this_pc(),
                    &sharing::system(std::path::Path::new("/"), &gpus),
                    &std::env::var("USER").unwrap_or_default(),
                    &read("/proc/sys/kernel/hostname"),
                );
                let exec = SystemExec { timeout: Duration::from_secs(60), ..SystemExec::default() };
                if sharing::post(&exec, &body, &state_path.with_extension("pending")) {
                    state = sharing::State { sent_at: now, sum, retry_at: 0 };
                } else {
                    state.retry_at = now + sharing::RETRY;
                }
                state.save(&state_path);
            }
        }
        std::thread::sleep(Duration::from_secs(1800));
    }
}

/// Installation par l'AppImage (update.rs) : cinq minutes après la connexion,
/// puis chaque jour, une version plus récente est cherchée et proposée
/// (notification avec « Mettre à jour », comme le bouton de la fenêtre) ;
/// rien ne s'installe sans ce clic. Une heure plus tard en cas d'échec (pas
/// d'Internet). Une fois la mise à jour faite, le service s'arrête en erreur
/// et systemd le relance sur le nouvel outil.
fn propose_updates() {
    std::thread::sleep(Duration::from_secs(300));
    loop {
        let exec = SystemExec { timeout: Duration::from_secs(90), ..SystemExec::default() };
        let wait = match update::check(&exec, update::now()) {
            Ok(Some(_)) => {
                let message = t(
                    "Une mise à jour de Last1 Panel est disponible. « Mettre à jour » installe la dernière version.",
                    "A Last1 Panel update is available. \"Update\" installs the latest version.",
                );
                if !desktop::ask(&message, &t("Mettre à jour", "Update")) {
                    Duration::from_secs(24 * 3600)
                } else {
                    // Clic parfois des jours après la recherche : la version en
                    // ligne est redemandée, jamais celle notée alors (dépassée,
                    // voire plus ancienne que celle installée depuis par la
                    // fenêtre ou go-to-linux).
                    match update_appimage() {
                        Ok(version) => {
                            let version = version.unwrap_or_else(|| update::RELEASE.unwrap_or_default().to_string());
                            desktop::notify(&t(
                                &format!("Last1 Panel est à jour (version {version})."),
                                &format!("Last1 Panel is up to date (version {version})."),
                            ));
                            // Le service repart sur l'AppImage installée.
                            std::process::exit(75);
                        }
                        Err(error) => {
                            eprintln!("last1-panel-cli : mise à jour : {error}");
                            desktop::notify(&t(
                                "La mise à jour n'a pas abouti : réessayez avec « Mettre à jour » dans la fenêtre.",
                                "The update did not finish: try again with \"Update\" in the window.",
                            ));
                            Duration::from_secs(3600)
                        }
                    }
                }
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
