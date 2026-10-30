//! Chaque fois que Steam recompile les shaders d'un jeu (« Traitement des
//! shaders Vulkan ») : quand, et ce qui a changé depuis la fois d'avant pour
//! ce jeu — le pilote graphique, la version de Proton, le jeu (mise à jour),
//! sinon Steam lui-même (nouveaux shaders téléchargés). Gardé dans
//! `shader-runs.tsv` (cache) : le diagnostic dit combien de fois et pourquoi,
//! la session d'analyse s'en sert pour régler la cause.

use std::fs;
use std::io::Write;
use std::path::Path;

/// Une recompilation : date (s), jeu, pilote, Proton, version du jeu.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub at: u64,
    pub game: String,
    pub driver: String,
    pub proton: String,
    pub build: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Cause {
    Driver,
    Proton,
    Game,
    Steam,
}

impl Cause {
    pub fn label(self) -> String {
        crate::i18n::t(
            match self {
                Cause::Driver => "pilote graphique",
                Cause::Proton => "Proton",
                Cause::Game => "mise à jour du jeu",
                Cause::Steam => "shaders téléchargés par Steam",
            },
            match self {
                Cause::Driver => "graphics driver",
                Cause::Proton => "Proton",
                Cause::Game => "game update",
                Cause::Steam => "shaders downloaded by Steam",
            },
        )
    }
}

fn clean(text: &str) -> String {
    text.replace(['\t', '\n'], " ").trim().to_string()
}

pub fn parse(text: &str) -> Vec<Run> {
    text.lines()
        .filter_map(|line| {
            let mut f = line.split('\t');
            Some(Run {
                at: f.next()?.parse().ok()?,
                game: f.next()?.to_string(),
                driver: f.next()?.to_string(),
                proton: f.next()?.to_string(),
                build: f.next()?.to_string(),
            })
        })
        .collect()
}

/// Ajoute une recompilation (200 gardées).
pub fn record(file: &Path, run: &Run) {
    let mut runs = parse(&fs::read_to_string(file).unwrap_or_default());
    runs.push(run.clone());
    let keep = runs.len().saturating_sub(200);
    if let Some(dir) = file.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Ok(mut out) = fs::File::create(file) {
        for r in &runs[keep..] {
            let _ = writeln!(
                out,
                "{}\t{}\t{}\t{}\t{}",
                r.at,
                clean(&r.game),
                clean(&r.driver),
                clean(&r.proton),
                clean(&r.build)
            );
        }
    }
}

/// Ce qui a changé depuis la recompilation d'avant du même jeu.
pub fn cause(before: Option<&Run>, now: &Run) -> Cause {
    let Some(before) = before else { return Cause::Steam };
    let changed = |a: &str, b: &str| !a.is_empty() && !b.is_empty() && a != b;
    if changed(&before.driver, &now.driver) {
        Cause::Driver
    } else if changed(&before.proton, &now.proton) {
        Cause::Proton
    } else if changed(&before.build, &now.build) {
        Cause::Game
    } else {
        Cause::Steam
    }
}

/// Recompilations des `days` derniers jours, par cause (la première fois
/// d'un jeu n'en est pas une).
pub fn recent(runs: &[Run], now: u64, days: u64) -> Vec<(Cause, u32)> {
    let mut counts: std::collections::BTreeMap<Cause, u32> = std::collections::BTreeMap::new();
    for (i, run) in runs.iter().enumerate() {
        let before = runs[..i].iter().rev().find(|r| r.game == run.game);
        if before.is_none() || run.at + days * 86_400 < now {
            continue;
        }
        *counts.entry(cause(before, run)).or_default() += 1;
    }
    let mut list: Vec<(Cause, u32)> = counts.into_iter().collect();
    list.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    list
}

/// Version de Proton avec laquelle le jeu a tourné en dernier (écrite par
/// Proton dans le préfixe), et version du jeu (appmanifest).
pub fn game_versions(library: &Path, app_id: u32) -> (String, String) {
    let prefix = library.join(format!("steamapps/compatdata/{app_id}"));
    let proton = fs::read_to_string(prefix.join("config_info"))
        .or_else(|_| fs::read_to_string(prefix.join("version")))
        .ok()
        .and_then(|text| text.lines().next().map(str::to_string))
        .unwrap_or_default();
    let build = fs::read_to_string(library.join(format!("steamapps/appmanifest_{app_id}.acf")))
        .ok()
        .and_then(|text| {
            text.lines().find_map(|line| {
                let mut parts = line.split('"').filter(|p| !p.trim().is_empty());
                (parts.next()? == "buildid").then(|| parts.next().map(str::to_string))?
            })
        })
        .unwrap_or_default();
    (proton, build)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_rebuild_gets_its_cause() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("shader-runs.tsv");
        let run = |at: u64, driver: &str, proton: &str, build: &str| Run {
            at,
            game: "ARC Raiders".into(),
            driver: driver.into(),
            proton: proton.into(),
            build: build.into(),
        };
        record(&file, &run(100, "580.95", "proton-10.0-3", "1"));
        record(&file, &run(200, "580.105", "proton-10.0-3", "1"));
        record(&file, &run(300, "580.105", "proton-10.0-4", "1"));
        record(&file, &run(400, "580.105", "proton-10.0-4", "2"));
        record(&file, &run(500, "580.105", "proton-10.0-4", "2"));
        let runs = parse(&fs::read_to_string(&file).unwrap());
        assert_eq!(runs.len(), 5);
        assert_eq!(
            recent(&runs, 500, 14),
            [(Cause::Driver, 1), (Cause::Proton, 1), (Cause::Game, 1), (Cause::Steam, 1)]
        );
        // Trop anciennes : rien.
        assert!(recent(&runs, 500 + 30 * 86_400, 14).is_empty());

        let library = dir.path();
        fs::create_dir_all(library.join("steamapps/compatdata/1808500")).unwrap();
        fs::write(
            library.join("steamapps/compatdata/1808500/config_info"),
            "proton-cachyos-10.0-20251020\n/usr/share\n",
        )
        .unwrap();
        fs::write(
            library.join("steamapps/appmanifest_1808500.acf"),
            "\"AppState\"\n{\n\t\"appid\"\t\t\"1808500\"\n\t\"buildid\"\t\t\"20412345\"\n}\n",
        )
        .unwrap();
        assert_eq!(game_versions(library, 1808500), ("proton-cachyos-10.0-20251020".into(), "20412345".into()));
    }
}
