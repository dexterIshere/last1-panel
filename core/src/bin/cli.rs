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
use last1_panel_core::system::gpu::{self, SysPaths};
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
        // Appelé par gpu-screen-recorder pour chaque vidéo enregistrée.
        Some("clip-saved") => clip_saved(&args[1..]),
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
    let context = Context {
        proton: launch::is_proton(command),
        tools: ["powerprofilesctl", "systemd-inhibit", "gamemoderun", "dlss-swapper", "mangohud"]
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
        ..Context::default()
    }
    .with_gpus(&gpus);
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
    let file = last1_panel_core::paths::last_launch_file();
    let mut text = format!("SteamAppId={}\n", std::env::var("SteamAppId").unwrap_or_default());
    for (name, value) in env {
        text.push_str(&format!("{name}={value}\n"));
    }
    text.push_str(&format!("{}\n", wrappers.iter().chain(command).cloned().collect::<Vec<_>>().join(" ")));
    if let Some(dir) = file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(file, text);
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

#[derive(PartialEq)]
enum Plugged {
    Screen,
    Input,
}

/// Service de la session (last1-panel-apply.service) : la vibrance NVIDIA
/// est perdue au redémarrage et quand un écran est rebranché ; elle est
/// remise à la connexion puis à chaque événement des cartes graphiques
/// (`udevadm monitor`, sous-système drm). Une souris branchée reçoit
/// l'accélération et la sensibilité gardées (sous-système input). Il tient
/// aussi l'icône près de l'horloge (tray.rs).
fn watch() -> ExitCode {
    if update::enabled() {
        std::thread::spawn(propose_updates);
    }
    // Mis à jour depuis la dernière fois : ce qui change, en trois lignes.
    if let Some(news) = last1_panel_core::release_notes::on_start(
        &last1_panel_core::paths::cache_dir(),
        last1_panel_core::release_notes::EMBEDDED,
        update::RELEASE.unwrap_or(env!("CARGO_PKG_VERSION")),
    ) {
        let lines: Vec<String> = news.notes.iter().take(3).map(|note| format!("• {}", note.text())).collect();
        let more = if news.notes.len() > 3 {
            t("\nLa suite dans Last1 Panel.", "\nMore in Last1 Panel.")
        } else {
            String::new()
        };
        desktop::notify(&format!(
            "{}\n{}{more}",
            t(
                &format!("Last1 Panel est à jour ({}) :", news.version),
                &format!("Last1 Panel is up to date ({}):", news.version)
            ),
            lines.join("\n")
        ));
    }
    std::thread::spawn(share_reports);
    std::thread::spawn(watch_updates);
    std::thread::spawn(watch_shaders);
    std::thread::spawn(watch_own_update);
    std::thread::spawn(watch_game);
    std::thread::spawn(clips);
    let tray = last1_panel_core::tray::spawn();
    std::thread::sleep(Duration::from_secs(3));
    apply_until_ready(20);
    let child = Command::new("udevadm")
        .args(["monitor", "--kernel", "--subsystem-match=drm", "--subsystem-match=input"])
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
            // « KERNEL[…] change /devices/…/drm/card1 (drm) »
            let event = if line.ends_with("(drm)") && line.contains(" change ") {
                Plugged::Screen
            } else if line.ends_with("(input)") && line.contains(" add ") {
                Plugged::Input
            } else {
                continue;
            };
            if events.send(event).is_err() {
                break;
            }
        }
    });
    // Un branchement donne plusieurs événements : on attend qu'ils cessent.
    while let Ok(first) = received.recv() {
        let mut screens = first == Plugged::Screen;
        while let Ok(event) = received.recv_timeout(Duration::from_secs(2)) {
            screens |= event == Plugged::Screen;
        }
        if screens {
            // Le pointeur est remis avec le reste.
            apply_until_ready(5);
            tray.screens_changed();
        } else if let Err(error) = Panel::session().reapply_pointer() {
            eprintln!("last1-panel-cli watch : {error}");
        }
    }
    let _ = child.kill();
    ExitCode::SUCCESS
}

/// Clips des parties (clips.rs) : gpu-screen-recorder garde la dernière
/// minute tant qu'un jeu tourne ; F8 l'enregistre, F9 lance ou arrête un
/// enregistrement. Arrêté hors des jeux (rien n'est gardé du bureau).
fn clips() {
    use last1_panel_core::clips::{self, Key};
    let home = last1_panel_core::paths::home_dir();
    let (keys, pressed) = mpsc::channel();
    std::thread::spawn(move || clips::watch_keys(std::path::Path::new("/dev"), keys));
    let script = last1_panel_core::paths::cache_dir().join("clip-saved.sh");
    let mut recorder: Option<std::process::Child> = None;
    let mut recording = false;
    let mut retry_at = std::time::Instant::now();
    let signal = |child: &std::process::Child, signal: i32| {
        // SAFETY : kill(2) sur le processus que nous avons lancé.
        unsafe { libc::kill(child.id() as i32, signal) };
    };
    loop {
        match pressed.recv_timeout(Duration::from_secs(2)) {
            Ok(Key::Clip) => {
                if let Some(child) = &recorder {
                    signal(child, libc::SIGUSR1);
                }
            }
            Ok(Key::Record) => {
                if let Some(child) = &recorder {
                    signal(child, libc::SIGRTMIN());
                    recording = !recording;
                    desktop::notify(&if recording {
                        t("Enregistrement lancé (F9 pour l'arrêter).", "Recording started (F9 to stop it).")
                    } else {
                        t(
                            "Enregistrement arrêté : la vidéo arrive dans Clips.",
                            "Recording stopped: the video is coming to Clips.",
                        )
                    });
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
        if let Some(child) = recorder.as_mut()
            && child.try_wait().ok().flatten().is_some()
        {
            recorder = None;
            recording = false;
            retry_at = std::time::Instant::now() + Duration::from_secs(60);
        }
        let wanted = Settings::load().clips != Some(false)
            && clips::installed()
            && last1_panel_core::games::steam::game_running(std::path::Path::new("/proc"));
        // Vice fait les clips : il garde ses touches, Last1 Panel ne lance rien.
        if wanted && recorder.is_none() && std::time::Instant::now() >= retry_at && clips::vice_in_use() {
            retry_at = std::time::Instant::now() + Duration::from_secs(60);
        } else if wanted && recorder.is_none() && std::time::Instant::now() >= retry_at {
            let dir = clips::dir(&home);
            let _ = std::fs::create_dir_all(&dir);
            let cli = std::env::current_exe().unwrap_or_else(|_| "last1-panel-cli".into());
            let _ = std::fs::write(&script, format!("#!/bin/sh\nexec '{}' clip-saved \"$@\"\n", cli.display()));
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755));
            }
            let args = clips::command(&dir, &script);
            let mut command = Command::new(&args[0]);
            command
                .args(&args[1..])
                .envs(last1_panel_core::system::desktop::session_env())
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            last1_panel_core::exec::without_appimage_env(&mut command);
            recorder = command.spawn().ok();
            if recorder.is_none() {
                retry_at = std::time::Instant::now() + Duration::from_secs(60);
            }
        } else if !wanted && let Some(mut child) = recorder.take() {
            // SIGINT : un enregistrement en cours est fini et gardé.
            signal(&child, libc::SIGINT);
            let _ = child.wait();
            recording = false;
        }
    }
}

/// Une vidéo vient d'être enregistrée : nommée d'après le jeu, et dite.
fn clip_saved(args: &[String]) -> ExitCode {
    let Some(path) = args.first() else { return ExitCode::from(2) };
    let game = last1_panel_core::panel::last_game_name().unwrap_or_default();
    let stamp = Command::new("date")
        .arg("+%Y-%m-%d %H-%M-%S")
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .unwrap_or_default();
    let saved = last1_panel_core::clips::rename_saved(std::path::Path::new(path), &game, &stamp);
    let name = saved.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    desktop::notify(&if args.get(1).is_some_and(|kind| kind == "replay") {
        t(&format!("Clip enregistré : {name}"), &format!("Clip saved: {name}"))
    } else {
        t(&format!("Vidéo enregistrée : {name}"), &format!("Video saved: {name}"))
    });
    ExitCode::SUCCESS
}

/// Mises à jour des paquets (system/packages.rs) : `checkupdates` cinq
/// minutes après la connexion, puis toutes les 6 h ; le diagnostic dit alors
/// tout de suite si le pilote graphique a une version plus récente.
/// Attend la fin de la partie en cours : les tâches de fond qui lisent le
/// disque ou le réseau ne tombent jamais pendant un jeu.
fn wait_for_no_game() {
    while last1_panel_core::games::steam::game_running(std::path::Path::new("/proc")) {
        std::thread::sleep(Duration::from_secs(60));
    }
}

fn watch_updates() {
    if last1_panel_core::exec::which("checkupdates").is_none() {
        return;
    }
    std::thread::sleep(Duration::from_secs(300));
    let cache = last1_panel_core::paths::cache_dir().join("updates.txt");
    loop {
        wait_for_no_game();
        let exec = SystemExec { timeout: Duration::from_secs(300), ..SystemExec::default() };
        last1_panel_core::system::packages::refresh(&exec, &cache);
        std::thread::sleep(Duration::from_secs(6 * 3600));
    }
}

/// Last1 Panel mis à jour, par n'importe quel chemin (notification, bouton
/// de la fenêtre, go-to-linux, pacman) : l'outil de ce service a été
/// remplacé. Regardé toutes les 30 s.
fn watch_own_update() {
    use std::os::unix::fs::MetadataExt;
    let Ok(path) = std::fs::read_link("/proc/self/exe") else { return };
    let inode = || std::fs::metadata(&path).map(|meta| meta.ino()).ok();
    let started = inode();
    loop {
        std::thread::sleep(Duration::from_secs(30));
        if inode().is_some() && inode() != started {
            // Le temps que la mise à jour finisse d'écrire ses fichiers.
            std::thread::sleep(Duration::from_secs(5));
            restart_after_update();
        }
    }
}

/// Après une mise à jour : la fenêtre ouverte rouverte sur la nouvelle
/// version, puis le service s'arrête en erreur pour que systemd le relance
/// sur le nouvel outil.
fn restart_after_update() -> ! {
    let window = if update::is_package() {
        std::path::PathBuf::from(last1_panel_core::paths::SYSTEM_APP)
    } else {
        last1_panel_core::paths::user_appimage()
    };
    desktop::reopen_window(&window);
    std::process::exit(75);
}

/// Pendant les parties Steam : la liaison PCI Express des cartes (toutes
/// les 20 s ; au repos, le pilote peut la réduire, le diagnostic juge sur
/// celle mesurée en jeu, la plus large gardée), et une fois par minute ce qui
/// entoure la partie et que MangoHud ne voit pas (system/game_context.rs),
/// gardé à côté de son journal.
fn watch_game() {
    use last1_panel_core::exec::SystemExec;
    use last1_panel_core::games::steam;
    use last1_panel_core::system::diagnostic::link_now;
    use last1_panel_core::system::game_context;
    let file = last1_panel_core::paths::pcie_in_game_file();
    let logs = last1_panel_core::paths::game_logs_dir();
    let sys = std::path::Path::new("/sys");
    let exec = SystemExec { timeout: Duration::from_secs(10), ..SystemExec::default() };
    use last1_panel_core::system::power_limit;
    let power_state = last1_panel_core::paths::power_limit_file();
    // Service arrêté pendant une partie : les limites d'avant reviennent.
    power_limit::restore(&power_state);
    use last1_panel_core::system::razer;
    let razer_state = last1_panel_core::paths::razer_mode_file();
    let laptop = razer::find(sys, std::path::Path::new("/dev"));
    // Le mode d'avant, si le service s'est arrêté pendant une partie.
    let razer_restore = |laptop: &Option<razer::Laptop>| {
        if let (Some(laptop), Ok(text)) = (laptop, std::fs::read_to_string(&razer_state)) {
            let mode = text.trim().parse().unwrap_or(razer::BALANCED);
            if razer::restore(laptop, mode).is_ok() {
                let _ = std::fs::remove_file(&razer_state);
            }
        }
    };
    razer_restore(&laptop);
    use last1_panel_core::system::input_devices;
    let proc_dir = std::path::Path::new("/proc");
    let mut devices = input_devices::detect(proc_dir);
    let mut started = std::time::Instant::now();
    let mut playing = false;
    let mut lifted = false;
    let mut tick: u32 = 0;
    let mut razer_error: Option<String> = None;
    // Par carte : la plus large, la plus rapide.
    let mut best: std::collections::BTreeMap<String, (u32, Option<f64>)> = std::collections::BTreeMap::new();
    loop {
        std::thread::sleep(Duration::from_secs(20));
        if !steam::game_running(std::path::Path::new("/proc")) {
            playing = false;
            razer_error = None;
            if lifted {
                power_limit::restore(&power_state);
                razer_restore(&laptop);
                lifted = false;
            }
            continue;
        }
        // Processeur à pleine puissance le temps du jeu, sur secteur
        // (system/power_limit.rs) ; rendue s'il passe sur batterie.
        // Portable Razer : processeur Boost, carte Haut, ventilateurs à fond,
        // comme Synapse.
        let battery = game_context::on_battery(sys);
        if !lifted && !battery {
            // Pas plus de puissance à un processeur qui chauffait déjà à la
            // partie d'avant : il se briderait encore plus.
            let hot = last1_panel_core::system::sessions::recent(&logs, 1)
                .first()
                .and_then(|s| s.context.as_ref())
                .is_some_and(|c| c.cpu_thermal_throttles > 0);
            let cpu = !hot && !power_limit::lift(sys, &power_state).is_empty();
            let fans = laptop.as_ref().filter(|l| razer::writable(l)).is_some_and(|l| {
                let before = razer::mode(l).unwrap_or(razer::BALANCED);
                let _ = std::fs::write(&razer_state, before.to_string());
                match razer::boost(l) {
                    Ok(()) => true,
                    Err(e) => {
                        razer_error = Some(e.to_string());
                        false
                    }
                }
            });
            lifted = cpu || fans;
        } else if lifted && battery {
            power_limit::restore(&power_state);
            razer_restore(&laptop);
            lifted = false;
        }
        if !playing {
            playing = true;
            best.clear();
            tick = 0;
            started = std::time::Instant::now();
            devices = input_devices::detect(proc_dir);
        }
        // Souris, clavier ou manette branchés ou débranchés : dit dans le
        // relevé de la partie, avec la minute.
        let now_devices = input_devices::detect(proc_dir);
        let changes = input_devices::changes(&devices, &now_devices);
        devices = now_devices;
        if !changes.is_empty()
            && let Some(log) = game_context::current_log(&logs)
        {
            let minute = (started.elapsed().as_secs_f64() / 6.0).round() / 10.0;
            let mut context = game_context::load(&log).unwrap_or_default();
            for change in changes {
                if context.input_changes.len() < 20 {
                    context.input_changes.push(format!("{minute} {change}"));
                }
            }
            game_context::save(&log, &context);
        }
        tick += 1;
        let gpus = gpu::detect(&SysPaths::default());
        for gpu in gpus.iter().filter(|g| g.discrete()) {
            if let Some((width, speed)) = link_now(sys, gpu) {
                let entry = best.entry(gpu.pci_slot.clone()).or_insert((width, speed));
                entry.0 = entry.0.max(width);
                entry.1 = match (entry.1, speed) {
                    (Some(a), Some(b)) => Some(a.max(b)),
                    (a, b) => a.or(b),
                };
            }
        }
        let text: String = best
            .iter()
            .map(|(slot, (width, speed))| match speed {
                Some(speed) => format!("{slot} {width} {speed}\n"),
                None => format!("{slot} {width}\n"),
            })
            .collect();
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&file, text);
        // Une fois par minute : le relevé de la partie, à côté de son journal.
        if tick % 3 == 1
            && let Some(log) = game_context::current_log(&logs)
        {
            let mut context = game_context::load(&log).unwrap_or_default();
            context.add(&game_context::sample(sys, &exec, &gpus));
            if let Some(l) = laptop.as_ref().filter(|_| !battery) {
                context.razer = Some(match (&razer_error, razer::state(l)) {
                    (Some(e), _) => format!("refusé : {e}"),
                    (None, Ok(state)) => state,
                    (None, Err(e)) => format!("illisible : {e}"),
                });
            }
            game_context::save(&log, &context);
        }
    }
}

/// Shaders compilés par Steam (games/shaders.rs) : tous les cœurs lui sont
/// donnés (steam_dev.cfg) ; prévenu quand une
/// compilation dure (les jeux saccadent pendant ce temps), puis quand elle
/// s'arrête, finie ou interrompue. Regardé toutes les 10 s ; Steam enchaîne
/// parfois deux processus pour un jeu : arrêtée après 20 s sans compilation.
fn watch_shaders() {
    use last1_panel_core::games::{shaders, steam};
    use last1_panel_core::system::desktop::notify;
    struct Tracked {
        game: Option<String>,
        since: std::time::Instant,
        notified: bool,
        /// Dernier avancement lu (0 à 1).
        ratio: Option<f64>,
    }
    let name = |game: &Option<String>| game.clone().unwrap_or_else(|| t("un jeu", "a game"));
    let percent = |ratio: f64| (ratio * 100.0).floor() as u32;
    let mut current: Option<Tracked> = None;
    let mut absent = 0;
    let mut given = false;
    loop {
        let root = steam::find_root(&last1_panel_core::paths::home_dir());
        // Tous les cœurs pour compiler, dès le prochain démarrage de Steam.
        if !given && let Some(root) = &root {
            given = true;
            if let Err(error) = shaders::give_cores(root, shaders::cores()) {
                eprintln!("last1-panel-cli watch : steam_dev.cfg : {error}");
            }
        }
        std::thread::sleep(Duration::from_secs(10));
        let precache = shaders::running(std::path::Path::new("/proc"), root.as_deref());
        let stopped = match (&precache, &current) {
            (Some(now), Some(tracked)) if tracked.game == now.game => None,
            (Some(_), Some(_)) => current.take(),
            (None, Some(_)) => {
                absent += 1;
                if absent >= 2 { current.take() } else { None }
            }
            _ => None,
        };
        if let Some(tracked) = stopped.filter(|tracked| tracked.notified) {
            let game = name(&tracked.game);
            notify(&match tracked.ratio {
                Some(ratio) if ratio < 0.95 => t(
                    &format!(
                        "Steam a interrompu la compilation des shaders de {game} ({} %) : il la reprendra plus tard.",
                        percent(ratio)
                    ),
                    &format!(
                        "Steam paused compiling the shaders of {game} ({} %): it will resume later.",
                        percent(ratio)
                    ),
                ),
                Some(_) => t(
                    &format!("Shaders de {game} prêts : Steam a fini, les jeux ne saccadent plus à cause de lui."),
                    &format!("{game} shaders ready: Steam is done, games no longer stutter because of it."),
                ),
                None => t(
                    &format!("Steam ne compile plus les shaders de {game}."),
                    &format!("Steam is no longer compiling the shaders of {game}."),
                ),
            });
        }
        let Some(now) = precache else { continue };
        absent = 0;
        let tracked = current.get_or_insert_with(|| Tracked {
            game: now.game.clone(),
            since: std::time::Instant::now(),
            notified: false,
            ratio: None,
        });
        if let Some((done, total)) = now.done.zip(now.total).filter(|(_, total)| *total > 0) {
            tracked.ratio = Some((f64::from(done) / f64::from(total)).min(1.0));
        }
        if !tracked.notified && tracked.since.elapsed() >= Duration::from_secs(30) {
            tracked.notified = true;
            let progress = tracked.ratio.map(|ratio| format!(" ({} %)", percent(ratio))).unwrap_or_default();
            let game = name(&tracked.game);
            notify(&t(
                &format!(
                    "Steam compile les shaders de {game}{progress} : les jeux peuvent saccader jusqu'à la fin. L'avancement est dans Last1 Panel."
                ),
                &format!(
                    "Steam is compiling the shaders of {game}{progress}: games may stutter until it ends. Progress is in Last1 Panel."
                ),
            ));
        }
    }
}

/// Rapports de performances partagés avec last1.fun (system/sharing.rs) :
/// seulement avec l'accord du joueur, et hors go-to-linux (qui envoie les
/// siens). Regardé dix minutes après la connexion, puis toutes les 5 minutes
/// hors des jeux : envoyé après chaque nouvelle partie.
fn share_reports() {
    use last1_panel_core::system::sharing;
    std::thread::sleep(Duration::from_secs(600));
    let state_path = last1_panel_core::paths::cache_dir().join("sharing.json");
    let read = |path: &str| std::fs::read_to_string(path).unwrap_or_default().trim().to_string();
    loop {
        if Settings::load().share_reports == Some(true) && !std::path::Path::new("/var/lib/gtl").exists() {
            wait_for_no_game();
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
        std::thread::sleep(Duration::from_secs(300));
    }
}

/// Installation par l'AppImage (update.rs) : une version plus récente est
/// cherchée cinq minutes après la connexion, dès qu'une version est publiée
/// (attente longue sur last1.fun, watch_releases), et en secours toutes les
/// 3 h, comme go-to-linux ; elle est proposée (notification avec « Mettre à
/// jour », comme le bouton de la fenêtre) ; rien ne s'installe sans ce clic.
/// Jamais pendant une partie : elle attend la fin du jeu.
/// Notification fermée sans clic : reproposée à la prochaine occasion ; tant
/// qu'elle est à l'écran, pas de double (desktop::ask attend la réponse).
/// Une heure plus tard en cas d'échec (pas d'Internet). Une fois la mise à
/// jour faite, le service s'arrête en erreur et systemd le relance sur le
/// nouvel outil.
fn propose_updates() {
    let (published, received) = mpsc::channel();
    std::thread::spawn(move || watch_releases(&published));
    std::thread::sleep(Duration::from_secs(300));
    const BACKUP: Duration = Duration::from_secs(3 * 3600);
    // Réveillé par une publication : la liste signée peut avoir un temps de
    // retard sur l'avis, revue deux minutes plus tard s'il ne propose rien.
    let mut woken = false;
    loop {
        let exec = SystemExec { timeout: Duration::from_secs(90), ..SystemExec::default() };
        let wait = match update::check(&exec, update::now()) {
            Ok(Some(_)) => {
                // Jamais par-dessus un jeu : proposée à la fin de la partie.
                wait_for_no_game();
                let message = t(
                    "Une mise à jour de Last1 Panel est disponible. « Mettre à jour » installe la dernière version.",
                    "A Last1 Panel update is available. \"Update\" installs the latest version.",
                );
                if !desktop::ask(&message, &t("Mettre à jour", "Update")) {
                    BACKUP
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
                            // Fenêtre et service repartent sur l'AppImage installée.
                            restart_after_update();
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
            Ok(None) if woken => Duration::from_secs(120),
            Ok(None) => BACKUP,
            Err(error) => {
                eprintln!("last1-panel-cli : mise à jour : {error}");
                Duration::from_secs(3600)
            }
        };
        // Une publication réveille tout de suite ; celles arrivées pendant
        // la notification ne comptent qu'une fois.
        woken = received.recv_timeout(wait).is_ok();
        while received.try_recv().is_ok() {}
    }
}

/// Publications de Last1 Panel, sues dès qu'elles arrivent : le flux de
/// last1.fun (une connexion gardée ouverte, update::stream_releases), sinon
/// l'attente longue (ancien serveur ; le flux est retenté chaque heure). La
/// première version reçue est seulement notée, chaque changement ensuite
/// réveille propose_updates.
fn watch_releases(published: &mpsc::Sender<()>) {
    let mut seen: Option<String> = None;
    let mut stop = false;
    let note = |version: String, seen: &mut Option<String>, stop: &mut bool| {
        if seen.as_deref() != Some(version.as_str()) {
            *stop = seen.is_some() && published.send(()).is_err();
            *seen = Some(version);
        }
    };
    loop {
        let started = std::time::Instant::now();
        let end = update::stream_releases(&mut |version| note(version, &mut seen, &mut stop));
        if stop {
            return;
        }
        match end {
            // Coupure : on se reconnecte, sans marteler un serveur qui ferme
            // aussitôt.
            update::StreamEnd::Closed if started.elapsed() > Duration::from_secs(60) => {
                std::thread::sleep(Duration::from_secs(5))
            }
            update::StreamEnd::Closed => std::thread::sleep(Duration::from_secs(60)),
            update::StreamEnd::Unavailable => {
                let exec = SystemExec { timeout: Duration::from_secs(110), ..SystemExec::default() };
                let until = std::time::Instant::now() + Duration::from_secs(3600);
                while std::time::Instant::now() < until {
                    match update::wait_for_release(&exec, seen.clone().unwrap_or_default().as_str()) {
                        Ok(Some(version)) if seen.as_deref() != Some(version.as_str()) => {
                            note(version, &mut seen, &mut stop);
                            if stop {
                                return;
                            }
                        }
                        Ok(None) => {}
                        _ => std::thread::sleep(Duration::from_secs(60)),
                    }
                }
            }
        }
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
