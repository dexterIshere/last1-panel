//! Ce qui prend le processeur pendant une partie : le temps de calcul de
//! chaque programme (/proc/<pid>/stat), entre deux relevés du service. Dit
//! dans le relevé de la partie (« fossilize_replay 310 % ») : le jeu, mais
//! aussi ce qui lui vole du processeur (shaders de Steam, échange de la
//! mémoire, navigateur…).
//!
//! La compilation des shaders de Steam (fossilize_replay) peut tourner
//! pendant le jeu, sur tous les cœurs : elle passe en SCHED_IDLE, le jeu
//! passe toujours avant elle.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Nom, temps de calcul et démarrage (tops d'horloge) de chaque processus,
/// par pid.
pub type Snapshot = BTreeMap<u32, (String, u64, u64)>;

/// Nom (comm), temps de calcul (utime + stime) et démarrage (starttime) d'un
/// « /proc/<pid>/stat ».
fn parse_stat(text: &str) -> Option<(String, u64, u64)> {
    let (open, close) = (text.find('(')?, text.rfind(')')?);
    let name = text.get(open + 1..close)?.to_string();
    // Après « ) » : état (champ 3), … utime (14), stime (15).
    let fields: Vec<&str> = text.get(close + 2..)?.split_whitespace().collect();
    let utime: u64 = fields.get(11)?.parse().ok()?;
    let stime: u64 = fields.get(12)?.parse().ok()?;
    let start: u64 = fields.get(19)?.parse().ok()?;
    Some((name, utime + stime, start))
}

pub fn snapshot(proc_dir: &Path) -> Snapshot {
    fs::read_dir(proc_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let pid: u32 = entry.file_name().to_str()?.parse().ok()?;
            Some((pid, parse_stat(&fs::read_to_string(entry.path().join("stat")).ok()?)?))
        })
        .collect()
}

/// Processeur pris par nom de programme entre deux relevés (en % d'un
/// cœur : 300 % = trois cœurs), du plus gourmand au moins.
pub fn usage(before: &Snapshot, after: &Snapshot, seconds: f64) -> Vec<(String, f64)> {
    // SAFETY : sysconf sans effet de bord.
    let ticks = unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as f64;
    // Au plus tous les cœurs à fond.
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get()) as f64;
    let mut by_name: BTreeMap<String, f64> = BTreeMap::new();
    for (pid, (name, time, started)) in after {
        // Le même processus aux deux relevés seulement (même nom, même
        // démarrage) : un pid repris par un autre programme, ou un processus
        // né entre les deux, apportait tout son temps depuis son démarrage
        // (« tracker-miner-fs 13069 % » sur un 12 fils).
        let Some((_, was, _)) = before.get(pid).filter(|(n, _, s)| n == name && s == started) else { continue };
        if time > was {
            *by_name.entry(name.clone()).or_default() += (time - was) as f64 / ticks / seconds * 100.0;
        }
    }
    for percent in by_name.values_mut() {
        *percent = percent.min(cores * 100.0);
    }
    let mut list: Vec<(String, f64)> = by_name.into_iter().filter(|(_, percent)| *percent >= 5.0).collect();
    list.sort_by(|a, b| b.1.total_cmp(&a.1));
    list
}

/// La compilation des shaders de Steam (« fossilize_replay », coupé à 15
/// lettres par le noyau) en tâche de fond : SCHED_IDLE pour chacun de ses
/// fils. Rend vrai si elle tournait.
pub fn shaders_to_background(proc_dir: &Path, snapshot: &Snapshot) -> bool {
    let mut found = false;
    for (pid, (name, _, _)) in snapshot {
        if !name.starts_with("fossilize_repl") {
            continue;
        }
        found = true;
        let tasks = fs::read_dir(proc_dir.join(pid.to_string()).join("task")).into_iter().flatten().flatten();
        for task in tasks {
            if let Some(tid) = task.file_name().to_str().and_then(|t| t.parse::<i32>().ok()) {
                let param = libc::sched_param { sched_priority: 0 };
                // SAFETY : un processus du compte passe en SCHED_IDLE, permis
                // sans droits particuliers ; échoue sans effet sinon.
                unsafe { libc::sched_setscheduler(tid, libc::SCHED_IDLE, &param) };
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_time_is_read_per_program() {
        let stat = "4242 (fossilize_repla) S 1 4242 4242 0 -1 4194560 100 0 0 0 1500 300 0 0 39 19 8 0 100 0 0";
        assert_eq!(parse_stat(stat), Some(("fossilize_repla".into(), 1800, 100)));
        // Un nom avec espaces et parenthèses.
        let game = "77 (Main (game) x) R 1 77 77 0 -1 0 0 0 0 0 10 5 0 0 20 0 30 0 1 0 0";
        assert_eq!(parse_stat(game), Some(("Main (game) x".into(), 15, 1)));

        let ticks = unsafe { libc::sysconf(libc::_SC_CLK_TCK) } as u64;
        let before: Snapshot = [
            (1, ("game".to_string(), 0, 10)),
            (2, ("fossilize_repla".to_string(), 0, 10)),
            (4, ("old".to_string(), 0, 10)),
        ]
        .into();
        let after: Snapshot = [
            (1, ("game".to_string(), ticks * 20, 10)),
            (2, ("fossilize_repla".to_string(), ticks * 10, 10)),
            (3, ("new".to_string(), 999_999, 500)),
            // pid 4 repris par un autre processus (même nom, autre démarrage).
            (4, ("old".to_string(), 999_999, 600)),
        ]
        .into();
        let list = usage(&before, &after, 20.0);
        assert_eq!(list, [("game".to_string(), 100.0), ("fossilize_repla".to_string(), 50.0)]);
        // Jamais plus que tous les cœurs.
        let cores = std::thread::available_parallelism().unwrap().get() as f64;
        let after: Snapshot = [(1, ("game".to_string(), 1_000_000 * ticks, 10))].into();
        assert_eq!(usage(&before, &after, 20.0)[0].1, cores * 100.0);
    }
}
