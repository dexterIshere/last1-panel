//! Parties jouées, d'après les journaux de MangoHud (un fichier CSV par
//! partie : `<programme>_<AAAA-MM-JJ_HH-MM-SS>.csv`, une ligne par mesure).
//! Résumé par partie : images par seconde (moyenne, 1 % les plus lentes),
//! charge du processeur et de la carte, et ce qui limite le jeu. C'est ce
//! qui dit, sur les PC des joueurs, pourquoi un jeu perd des images.

use crate::system::game_context::{self, GameContext};
use serde::Serialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameSession {
    /// Programme mesuré (exécutable Windows sous Proton : « PioneerGame.exe »).
    pub program: String,
    /// Début de la partie, « AAAA-MM-JJ HH:MM ».
    pub started: String,
    pub minutes: f64,
    pub fps_average: f64,
    /// Images par seconde du 1 % des images les plus lentes (saccades).
    pub fps_one_percent_low: f64,
    pub cpu_load: f64,
    pub gpu_load: f64,
    /// Fréquence moyenne de la carte graphique (MHz).
    pub gpu_clock: f64,
    /// Ce qui limite : « gpu » (carte à fond), « cpu » (carte qui attend),
    /// « cap » (images tenues à un plafond : synchro, limite d'images),
    /// « loading » (presque que du chargement : écran de compilation des
    /// shaders, menus figés), « ? ».
    pub limit: &'static str,
    /// Carte qui a dessiné le jeu, d'après MangoHud (en tête de son journal).
    pub gpu_name: Option<String>,
    /// Puissance de la carte (W) : moyenne et la plus haute ; 0 : inconnue.
    pub gpu_power: f64,
    pub gpu_power_max: f64,
    /// Températures les plus hautes (°C) ; 0 : inconnues.
    pub gpu_temp_max: f64,
    pub cpu_temp_max: f64,
    /// Fréquence moyenne du processeur (MHz) et sa puissance moyenne (W) ;
    /// 0 : inconnues.
    pub cpu_mhz: f64,
    pub cpu_power: f64,
    /// Ce qui entourait la partie (secteur, modes de puissance, freins de
    /// la carte…), relevé par le service de la session (game_context.rs).
    pub context: Option<GameContext>,
    /// Le tout en phrases courtes (diagnostic::session_details), pour
    /// l'interface.
    pub details: Vec<String>,
    /// La partie au fil du temps (60 points au plus), pour la courbe.
    pub timeline: Vec<Point>,
}

/// Un moment de la partie : moyenne d'une tranche du journal.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Point {
    /// Minutes depuis le début.
    pub minute: f64,
    pub fps: f64,
    /// 1 % des images les plus lentes de la tranche.
    pub fps_low: f64,
    pub cpu_load: f64,
    pub gpu_load: f64,
}

/// Le journal découpé en tranches égales (10 s au moins).
fn timeline(rows: &[Row], elapsed_ns: f64) -> Vec<Point> {
    if elapsed_ns <= 0.0 || rows.iter().all(|row| row.at.is_none()) {
        return Vec::new();
    }
    let width = (elapsed_ns / 60.0).max(10e9);
    let count = (elapsed_ns / width).ceil().max(1.0) as usize;
    let mut slices: Vec<Vec<&Row>> = vec![Vec::new(); count];
    for row in rows {
        if let Some(at) = row.at {
            slices[((at / width) as usize).min(count - 1)].push(row);
        }
    }
    let round = |v: f64| (v * 10.0).round() / 10.0;
    let mean = |v: &[f64]| if v.is_empty() { 0.0 } else { v.iter().sum::<f64>() / v.len() as f64 };
    slices
        .iter()
        .enumerate()
        .filter(|(_, slice)| !slice.is_empty())
        .map(|(i, slice)| {
            let mut frametimes: Vec<f64> = slice.iter().map(|row| row.frametime).collect();
            frametimes.sort_by(f64::total_cmp);
            let p99 = frametimes[((frametimes.len() as f64 * 0.99) as usize).min(frametimes.len() - 1)];
            Point {
                minute: round((i as f64 + 0.5) * width / 60e9),
                fps: round(mean(&slice.iter().map(|row| row.fps).collect::<Vec<_>>())),
                fps_low: round(1000.0 / p99),
                cpu_load: round(mean(&slice.iter().filter_map(|row| row.cpu).collect::<Vec<_>>())),
                gpu_load: round(mean(&slice.iter().filter_map(|row| row.gpu).collect::<Vec<_>>())),
            }
        })
        .collect()
}

/// Les `count` dernières parties d'au moins une minute, de la plus récente à
/// la plus ancienne.
pub fn recent(dir: &Path, count: usize) -> Vec<GameSession> {
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    let mut files: Vec<_> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "csv")
                && !path.file_name().is_some_and(|n| n.to_string_lossy().ends_with("_summary.csv"))
        })
        .collect();
    // Le nom finit par la date : l'ordre des noms (à date égale) suffit.
    files.sort_by_key(|path| std::cmp::Reverse(date_of(path).unwrap_or_default()));
    files
        .iter()
        .filter_map(|path| {
            let name = path.file_stem()?.to_string_lossy().into_owned();
            let mut session = summarize(&name, &fs::read_to_string(path).ok()?)?;
            session.context = game_context::load(path);
            Some(session)
        })
        .filter(|session| session.minutes >= 1.0)
        .take(count)
        .collect()
}

/// Garde les `keep` derniers journaux (et leurs résumés) : MangoHud en écrit
/// un par partie, quelques Mo par heure de jeu.
pub fn prune(dir: &Path, keep: usize) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    let mut logs: Vec<_> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "csv")
                && !path.file_name().is_some_and(|n| n.to_string_lossy().ends_with("_summary.csv"))
        })
        .collect();
    logs.sort_by_key(|path| std::cmp::Reverse(date_of(path).unwrap_or_default()));
    for old in logs.iter().skip(keep) {
        let _ = fs::remove_file(old);
        let _ = fs::remove_file(game_context::file_for(old));
        let _ = fs::remove_file(old.with_file_name(format!(
            "{}_summary.csv",
            old.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
        )));
    }
}

fn date_of(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_string_lossy().into_owned();
    // « Jeu_2026-10-18_21-04-33 » : les 19 derniers caractères.
    stem.len().checked_sub(19).map(|start| stem[start..].to_string())
}

/// Une ligne du journal de MangoHud.
struct Row {
    /// Depuis le début du journal (ns).
    at: Option<f64>,
    fps: f64,
    frametime: f64,
    cpu: Option<f64>,
    gpu: Option<f64>,
    clock: Option<f64>,
    gpu_power: Option<f64>,
    cpu_power: Option<f64>,
    gpu_temp: Option<f64>,
    cpu_temp: Option<f64>,
    cpu_mhz: Option<f64>,
}

impl Row {
    /// Pas du jeu : moins de 15 images par seconde (chargement, écran de
    /// compilation des shaders, jeu en pause ou en arrière-plan).
    fn loading(&self) -> bool {
        self.fps < 15.0
    }
}

/// Résumé d'un journal ; `name` : son nom sans « .csv ».
pub fn summarize(name: &str, text: &str) -> Option<GameSession> {
    // En tête : « os,cpu,gpu,… » puis ses valeurs : la carte qui a dessiné.
    let mut lines = text.lines();
    let mut gpu_name = None;
    let mut previous: Option<&str> = None;
    let header: Vec<&str> = loop {
        let line = lines.next()?;
        if line.starts_with("fps,") {
            break line.split(',').collect();
        }
        if let Some(names) = previous.filter(|p| p.starts_with("os,")) {
            let position = names.split(',').position(|name| name.trim() == "gpu");
            gpu_name = position
                .and_then(|i| line.split(',').nth(i))
                .map(|name| name.trim().to_string())
                .filter(|name| !name.is_empty());
        }
        previous = Some(line);
    };
    let column = |key: &str| header.iter().position(|h| h.trim() == key);
    let (fps_col, frametime_col) = (column("fps")?, column("frametime")?);
    let (cpu_col, gpu_col, clock_col, elapsed_col) =
        (column("cpu_load"), column("gpu_load"), column("gpu_core_clock"), column("elapsed"));

    let mut rows: Vec<Row> = Vec::new();
    let mut elapsed_ns = 0.0_f64;
    for line in lines {
        let values: Vec<f64> = line.split(',').map(|v| v.trim().parse().unwrap_or(f64::NAN)).collect();
        let at = |col: Option<usize>| col.and_then(|c| values.get(c).copied()).filter(|v| v.is_finite());
        let (Some(fps), Some(frametime)) = (at(Some(fps_col)), at(Some(frametime_col))) else { continue };
        if fps <= 0.0 || frametime <= 0.0 {
            continue;
        }
        // 0 : valeur que MangoHud n'a pas pu lire.
        let known = |col: Option<usize>| at(col).filter(|v| *v > 0.0);
        rows.push(Row {
            at: at(elapsed_col),
            fps,
            frametime,
            cpu: at(cpu_col),
            gpu: at(gpu_col),
            clock: at(clock_col),
            gpu_power: known(column("gpu_power")),
            cpu_power: known(column("cpu_power")),
            gpu_temp: known(column("gpu_temp")),
            cpu_temp: known(column("cpu_temp")),
            cpu_mhz: known(column("cpu_mhz")),
        });
        if let Some(e) = at(elapsed_col) {
            elapsed_ns = elapsed_ns.max(e);
        }
    }
    if rows.is_empty() {
        return None;
    }
    // Le chargement est compté à part : la partie, c'est le reste ; s'il n'y
    // a presque que ça, la partie n'en est pas une.
    let playing = rows.iter().filter(|row| !row.loading()).count();
    let mostly_loading = playing * 10 < rows.len();
    let used: Vec<&Row> = rows.iter().filter(|row| mostly_loading || !row.loading()).collect();
    let fps: Vec<f64> = used.iter().map(|row| row.fps).collect();
    let mut frametimes: Vec<f64> = used.iter().map(|row| row.frametime).collect();
    let cpu: Vec<f64> = used.iter().filter_map(|row| row.cpu).collect();
    let gpu: Vec<f64> = used.iter().filter_map(|row| row.gpu).collect();
    let clock: Vec<f64> = used.iter().filter_map(|row| row.clock).collect();
    let gpu_power: Vec<f64> = used.iter().filter_map(|row| row.gpu_power).collect();
    let cpu_mhz: Vec<f64> = used.iter().filter_map(|row| row.cpu_mhz).collect();
    let highest = |values: Vec<f64>| values.into_iter().fold(0.0_f64, f64::max);
    let mean = |v: &[f64]| if v.is_empty() { 0.0 } else { v.iter().sum::<f64>() / v.len() as f64 };
    // 1 % les plus lentes : le temps d'image au 99e centile.
    frametimes.sort_by(f64::total_cmp);
    let p99 = frametimes[((frametimes.len() as f64 * 0.99) as usize).min(frametimes.len() - 1)];
    let gpu_load = mean(&gpu);
    // Carte à fond (85 % et plus : NVIDIA ne dépasse guère 95 %) la moitié
    // du temps de jeu au moins : c'est elle qui limite. Une moyenne serait
    // tirée vers le bas par les menus et les chargements.
    let gpu_bound = gpu.iter().filter(|load| **load >= 85.0).count();
    // Images tenues à un plafond (synchro verticale, limite d'images, VRR à
    // la fréquence de l'écran) : presque toutes à la même cadence, la carte
    // n'étant pas à fond. Ni le processeur ni la carte ne limitent alors.
    let mut sorted = fps.clone();
    sorted.sort_by(f64::total_cmp);
    let at = |q: f64| sorted[((sorted.len() as f64 * q) as usize).min(sorted.len() - 1)];
    let capped = sorted.len() >= 10 && at(0.9) > 0.0 && at(0.1) / at(0.9) >= 0.92;
    let limit = if gpu.is_empty() {
        "?"
    } else if mostly_loading {
        "loading"
    } else if gpu_bound * 2 >= gpu.len() {
        "gpu"
    } else if capped {
        "cap"
    } else {
        "cpu"
    };
    let (program, started) = match name.len().checked_sub(20).filter(|i| name.as_bytes().get(*i) == Some(&b'_')) {
        Some(i) => {
            let date = &name[i + 1..];
            (name[..i].to_string(), format!("{} {}", &date[..10], date[11..16].replace('-', ":")))
        }
        None => (name.to_string(), String::new()),
    };
    let round = |v: f64| (v * 10.0).round() / 10.0;
    Some(GameSession {
        program,
        started,
        minutes: round(elapsed_ns / 60e9),
        fps_average: round(mean(&fps)),
        fps_one_percent_low: round(1000.0 / p99),
        cpu_load: round(mean(&cpu)),
        gpu_load: round(gpu_load),
        gpu_clock: mean(&clock).round(),
        limit,
        gpu_name,
        gpu_power: round(mean(&gpu_power)),
        gpu_power_max: round(highest(gpu_power.clone())),
        gpu_temp_max: highest(used.iter().filter_map(|row| row.gpu_temp).collect()),
        cpu_temp_max: highest(used.iter().filter_map(|row| row.cpu_temp).collect()),
        cpu_mhz: mean(&cpu_mhz).round(),
        cpu_power: round(mean(&used.iter().filter_map(|row| row.cpu_power).collect::<Vec<f64>>())),
        context: None,
        details: Vec::new(),
        timeline: timeline(&rows, elapsed_ns),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log(rows: &[(f64, f64, f64, f64, u64)]) -> String {
        let mut text = String::from(
            "os,cpu,gpu,ram,kernel,driver,cpuscheduler\nArch Linux,AMD Ryzen 7 7800X3D,GeForce RTX 4070,32 GB,6.17,595.58,\n\
             fps,frametime,cpu_load,cpu_power,gpu_load,cpu_temp,gpu_temp,gpu_core_clock,gpu_mem_clock,gpu_vram_used,gpu_power,ram_used,swap_used,process_rss,cpu_mhz,elapsed\n",
        );
        for (fps, frametime, cpu, gpu, elapsed) in rows {
            text.push_str(&format!(
                "{fps},{frametime},{cpu},65,{gpu},70,66,2610,10501,7.1,180,14.2,0,6.3,5100,{elapsed}\n"
            ));
        }
        text
    }

    #[test]
    fn cpu_bound_game_is_recognised() {
        // Deux minutes : la carte n'est qu'à 60 %, le processeur limite.
        // Cadence qui varie (la carte attend un processeur plus ou moins
        // chargé) : pas un plafond.
        let mut rows: Vec<(f64, f64, f64, f64, u64)> = (0..99)
            .map(|i| {
                let fps = if i % 2 == 0 { 50.0 } else { 70.0 };
                (fps, 1000.0 / fps, 45.0, 60.0, (i as u64 + 1) * 1_200_000_000)
            })
            .collect();
        rows.push((20.0, 50.0, 45.0, 60.0, 120_000_000_000));
        let session = summarize("PioneerGame.exe_2026-10-18_21-04-33", &log(&rows)).unwrap();
        assert_eq!(session.program, "PioneerGame.exe");
        assert_eq!(session.started, "2026-10-18 21:04");
        assert_eq!(session.minutes, 2.0);
        assert_eq!(session.fps_average, 59.5);
        assert_eq!(session.fps_one_percent_low, 20.0);
        assert_eq!((session.cpu_load, session.gpu_load, session.gpu_clock), (45.0, 60.0, 2610.0));
        assert_eq!(session.limit, "cpu");
        // Ce que MangoHud mesure aussi : la carte qui a dessiné, sa puissance,
        // les températures, la fréquence du processeur.
        assert_eq!(session.gpu_name.as_deref(), Some("GeForce RTX 4070"));
        assert_eq!((session.gpu_power, session.gpu_power_max), (180.0, 180.0));
        assert_eq!((session.gpu_temp_max, session.cpu_temp_max, session.cpu_mhz), (66.0, 70.0, 5100.0));
        // Au fil de la partie : une tranche de 10 s par point, la saccade de
        // la fin dans la dernière.
        assert_eq!(session.timeline.len(), 12);
        assert_eq!(
            session.timeline[0],
            Point { minute: 0.1, fps: 60.0, fps_low: 50.0, cpu_load: 45.0, gpu_load: 60.0 }
        );
        assert_eq!(session.timeline[11].fps_low, 20.0);
    }

    #[test]
    fn card_at_full_load_most_of_the_game_is_the_limit() {
        // ARC Raiders sur RTX 2060 : menus à 250 i/s, chargements à 1 i/s,
        // et le jeu à 60-80 i/s, carte entre 86 et 94 % : la moyenne de la
        // carte (sous 90 %) ne doit pas faire dire « processeur ».
        let rows: Vec<(f64, f64, f64, f64, u64)> = (0..200)
            .map(|i| {
                let (fps, gpu) = match i {
                    0..20 => (250.0, 50.0),
                    20..40 => (1.0, 12.0),
                    _ => (60.0 + (i % 20) as f64, 86.0 + (i % 9) as f64),
                };
                (fps, 1000.0 / fps, 60.0, gpu, (i as u64 + 1) * 1_000_000_000)
            })
            .collect();
        let session = summarize("PioneerGame_2026-10-21_10-54-00", &log(&rows)).unwrap();
        assert_eq!(session.limit, "gpu");
        // Les chargements à 1 i/s ne font pas le 1 % bas.
        assert!(session.fps_one_percent_low > 50.0, "{}", session.fps_one_percent_low);
    }

    #[test]
    fn frame_cap_is_not_a_cpu_limit() {
        // 144 images/s tenues (synchro de l'écran) avec la carte à 70 %.
        let rows: Vec<(f64, f64, f64, f64, u64)> =
            (0..100).map(|i| (143.0 + (i % 3) as f64, 6.9, 40.0, 70.0, (i as u64 + 1) * 1_200_000_000)).collect();
        assert_eq!(summarize("Game.exe_2026-10-18_21-04-33", &log(&rows)).unwrap().limit, "cap");
    }

    #[test]
    fn loading_screens_are_set_apart() {
        // Six minutes d'écran « compilation des shaders » : 6 i/s, carte à 12 %.
        let compiling: Vec<(f64, f64, f64, f64, u64)> =
            (1..=360).map(|i| (6.0, 166.0, 47.0, 12.0, i * 1_000_000_000)).collect();
        let session = summarize("PioneerGame_2026-10-19_15-38-00", &log(&compiling)).unwrap();
        assert_eq!((session.limit, session.fps_average), ("loading", 6.0));
        // Puis vingt minutes de jeu : la partie, sans le chargement.
        let mut game = compiling.clone();
        game.extend((1..=1200).map(|i| (90.0, 11.1, 40.0, 98.0, (360 + i) * 1_000_000_000)));
        let session = summarize("PioneerGame_2026-10-19_15-38-00", &log(&game)).unwrap();
        assert_eq!((session.limit, session.fps_average, session.gpu_load), ("gpu", 90.0, 98.0));
        assert_eq!(session.minutes, 26.0);
    }

    #[test]
    fn recent_sessions_newest_first_short_ones_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let rows = |gpu: f64, minutes: u64| -> Vec<(f64, f64, f64, f64, u64)> {
            (1..=10).map(|i| (140.0, 7.1, 30.0, gpu, i * minutes * 6_000_000_000)).collect()
        };
        fs::write(dir.path().join("Game.exe_2026-10-17_20-00-00.csv"), log(&rows(97.0, 5))).unwrap();
        fs::write(dir.path().join("Game.exe_2026-10-18_20-00-00.csv"), log(&rows(97.0, 3))).unwrap();
        // Moins d'une minute : écarté ; résumé de MangoHud : ignoré.
        fs::write(dir.path().join("Menu.exe_2026-10-18_21-00-00.csv"), log(&rows(50.0, 0))).unwrap();
        fs::write(dir.path().join("Game.exe_2026-10-18_20-00-00_summary.csv"), "fps\n1\n").unwrap();
        let sessions = recent(dir.path(), 5);
        let dates: Vec<&str> = sessions.iter().map(|s| s.started.as_str()).collect();
        assert_eq!(dates, ["2026-10-18 20:00", "2026-10-17 20:00"]);
        assert!(sessions.iter().all(|s| s.limit == "gpu"));
        assert!(recent(&dir.path().join("absent"), 5).is_empty());

        // Les plus anciens journaux partent, avec leur résumé.
        prune(dir.path(), 2);
        assert!(!dir.path().join("Game.exe_2026-10-17_20-00-00.csv").exists());
        assert!(dir.path().join("Game.exe_2026-10-18_20-00-00.csv").exists());
        assert!(dir.path().join("Game.exe_2026-10-18_20-00-00_summary.csv").exists());
    }

    #[test]
    fn unreadable_logs_are_skipped() {
        assert_eq!(summarize("x", "pas un journal"), None);
        assert_eq!(summarize("x", "fps,frametime\n0,0\n"), None);
    }
}
