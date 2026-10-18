//! Parties jouées, d'après les journaux de MangoHud (un fichier CSV par
//! partie : `<programme>_<AAAA-MM-JJ_HH-MM-SS>.csv`, une ligne par mesure).
//! Résumé par partie : images par seconde (moyenne, 1 % les plus lentes),
//! charge du processeur et de la carte, et ce qui limite le jeu. C'est ce
//! qui dit, sur les PC des joueurs, pourquoi un jeu perd des images.

use serde::Serialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize)]
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
    /// Ce qui limite : « gpu » (carte à fond), « cpu » (carte qui attend), « ? ».
    pub limit: &'static str,
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
            summarize(&name, &fs::read_to_string(path).ok()?)
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

/// Résumé d'un journal ; `name` : son nom sans « .csv ».
pub fn summarize(name: &str, text: &str) -> Option<GameSession> {
    let mut lines = text.lines();
    let header: Vec<&str> = lines.by_ref().find(|line| line.starts_with("fps,"))?.split(',').collect();
    let column = |key: &str| header.iter().position(|h| h.trim() == key);
    let (fps_col, frametime_col) = (column("fps")?, column("frametime")?);
    let (cpu_col, gpu_col, clock_col, elapsed_col) =
        (column("cpu_load"), column("gpu_load"), column("gpu_core_clock"), column("elapsed"));

    let mut fps = Vec::new();
    let mut frametimes = Vec::new();
    let (mut cpu, mut gpu, mut clock) = (Vec::new(), Vec::new(), Vec::new());
    let mut elapsed_ns = 0.0_f64;
    for line in lines {
        let values: Vec<f64> = line.split(',').map(|v| v.trim().parse().unwrap_or(f64::NAN)).collect();
        let at = |col: Option<usize>| col.and_then(|c| values.get(c).copied()).filter(|v| v.is_finite());
        let (Some(f), Some(ft)) = (at(Some(fps_col)), at(Some(frametime_col))) else { continue };
        if f <= 0.0 || ft <= 0.0 {
            continue;
        }
        fps.push(f);
        frametimes.push(ft);
        cpu.extend(at(cpu_col));
        gpu.extend(at(gpu_col));
        clock.extend(at(clock_col));
        if let Some(e) = at(elapsed_col) {
            elapsed_ns = elapsed_ns.max(e);
        }
    }
    if fps.is_empty() {
        return None;
    }
    let mean = |v: &[f64]| if v.is_empty() { 0.0 } else { v.iter().sum::<f64>() / v.len() as f64 };
    // 1 % les plus lentes : le temps d'image au 99e centile.
    frametimes.sort_by(f64::total_cmp);
    let p99 = frametimes[((frametimes.len() as f64 * 0.99) as usize).min(frametimes.len() - 1)];
    let gpu_load = mean(&gpu);
    let limit = if gpu.is_empty() {
        "?"
    } else if gpu_load >= 90.0 {
        "gpu"
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
        let mut rows: Vec<(f64, f64, f64, f64, u64)> =
            (0..99).map(|i| (60.0, 16.6, 45.0, 60.0, (i as u64 + 1) * 1_200_000_000)).collect();
        rows.push((20.0, 50.0, 45.0, 60.0, 120_000_000_000));
        let session = summarize("PioneerGame.exe_2026-10-18_21-04-33", &log(&rows)).unwrap();
        assert_eq!(session.program, "PioneerGame.exe");
        assert_eq!(session.started, "2026-10-18 21:04");
        assert_eq!(session.minutes, 2.0);
        assert_eq!(session.fps_average, 59.6);
        assert_eq!(session.fps_one_percent_low, 20.0);
        assert_eq!((session.cpu_load, session.gpu_load, session.gpu_clock), (45.0, 60.0, 2610.0));
        assert_eq!(session.limit, "cpu");
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
