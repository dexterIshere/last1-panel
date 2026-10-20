//! Limite de puissance du processeur le temps des jeux, sur secteur : la
//! limite longue durée (PL1, RAPL) relevée au niveau de la courte (PL2),
//! comme le mode Performance des applis de fabricant sous Windows, puis
//! rendue à la fin de la partie. Le processeur se protège seul de la chaleur
//! (il se bride à sa température limite).
//!
//! Les fichiers de RAPL sont à root : le bouton du diagnostic
//! (Fix::CpuPowerLimit) les donne au compte du joueur une fois pour toutes
//! (tmpfiles, à chaque démarrage) ; le service de la session fait le reste.

use std::fs;
use std::path::{Path, PathBuf};

/// Zones RAPL du processeur : par registre (MSR) et par mémoire (MMIO) ; la
/// plus basse des deux s'applique.
pub const ZONES: [&str; 2] = ["intel-rapl:0", "intel-rapl-mmio:0"];

fn read(path: &Path) -> Option<u64> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// Fichier d'une contrainte par son nom (long_term, short_term).
fn constraint(zone: &Path, name: &str) -> Option<PathBuf> {
    (0..4).find_map(|i| {
        (fs::read_to_string(zone.join(format!("constraint_{i}_name"))).ok()?.trim() == name)
            .then(|| zone.join(format!("constraint_{i}_power_limit_uw")))
    })
}

/// Une limite relevée : son fichier et sa valeur d'avant (µW).
#[derive(Debug, Clone, PartialEq)]
pub struct Lifted {
    pub file: PathBuf,
    pub before: u64,
}

/// Limites longue durée en dessous de la courte, que ce compte peut écrire.
fn liftable(sys: &Path) -> Vec<(PathBuf, u64, u64)> {
    ZONES
        .iter()
        .filter_map(|zone| {
            let zone = sys.join("class/powercap").join(zone);
            let long = constraint(&zone, "long_term")?;
            let short = read(&constraint(&zone, "short_term")?)?;
            let before = read(&long)?;
            let writable = fs::OpenOptions::new().write(true).open(&long).is_ok();
            (writable && before > 0 && short > before).then_some((long, before, short))
        })
        .collect()
}

/// Une limite longue durée sous la courte : à relever pour les jeux.
pub fn held_back(sys: &Path) -> bool {
    ZONES.iter().any(|zone| {
        let zone = sys.join("class/powercap").join(zone);
        let long = constraint(&zone, "long_term").and_then(|f| read(&f));
        let short = constraint(&zone, "short_term").and_then(|f| read(&f));
        matches!((long, short), (Some(long), Some(short)) if long > 0 && short > long)
    })
}

/// Le compte peut-il les écrire (bouton déjà passé) ?
pub fn writable(sys: &Path) -> bool {
    ZONES.iter().any(|zone| {
        constraint(&sys.join("class/powercap").join(zone), "long_term")
            .is_some_and(|file| fs::OpenOptions::new().write(true).open(file).is_ok())
    })
}

/// Relève les limites ; rend ce qu'il faudra remettre, gardé aussi dans
/// `state` (remis au démarrage suivant du service s'il s'est arrêté avant).
pub fn lift(sys: &Path, state: &Path) -> Vec<Lifted> {
    let mut lifted = Vec::new();
    for (file, before, short) in liftable(sys) {
        if fs::write(&file, short.to_string()).is_ok() {
            lifted.push(Lifted { file, before });
        }
    }
    if !lifted.is_empty() {
        let text: String = lifted.iter().map(|l| format!("{} {}\n", l.file.display(), l.before)).collect();
        if let Some(dir) = state.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(state, text);
    }
    lifted
}

/// Remet les limites d'avant (celles de `state`, après un arrêt du service).
pub fn restore(state: &Path) {
    let Ok(text) = fs::read_to_string(state) else { return };
    for line in text.lines() {
        if let Some((file, before)) = line.rsplit_once(' ') {
            let _ = fs::write(file, before);
        }
    }
    let _ = fs::remove_file(state);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zone(sys: &Path, name: &str, long: u64, short: u64) -> PathBuf {
        let zone = sys.join("class/powercap").join(name);
        fs::create_dir_all(&zone).unwrap();
        fs::write(zone.join("constraint_0_name"), "long_term\n").unwrap();
        fs::write(zone.join("constraint_0_power_limit_uw"), format!("{long}\n")).unwrap();
        fs::write(zone.join("constraint_1_name"), "short_term\n").unwrap();
        fs::write(zone.join("constraint_1_power_limit_uw"), format!("{short}\n")).unwrap();
        zone
    }

    #[test]
    fn long_term_limit_rises_to_the_short_one_then_comes_back() {
        let dir = tempfile::tempdir().unwrap();
        let sys = dir.path();
        let state = dir.path().join("state/power-limit.txt");
        assert!(!held_back(sys));
        // Portable bridé : 45 W longue durée, 90 W en pointe ; MMIO déjà haut.
        let msr = zone(sys, "intel-rapl:0", 45_000_000, 90_000_000);
        zone(sys, "intel-rapl-mmio:0", 90_000_000, 90_000_000);
        assert!(held_back(sys) && writable(sys));
        let lifted = lift(sys, &state);
        assert_eq!(lifted, [Lifted { file: msr.join("constraint_0_power_limit_uw"), before: 45_000_000 }]);
        assert_eq!(read(&msr.join("constraint_0_power_limit_uw")), Some(90_000_000));
        assert!(!held_back(sys));
        // Fin de la partie (ou service relancé) : la limite d'avant revient.
        restore(&state);
        assert_eq!(read(&msr.join("constraint_0_power_limit_uw")), Some(45_000_000));
        assert!(!state.exists());
        restore(&state);
    }
}
