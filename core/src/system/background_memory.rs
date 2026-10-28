//! Discord, le navigateur et les autres applications ouvertes à côté du jeu :
//! quand la mémoire se remplit pendant une partie, le service de la session
//! range d'avance leur mémoire dans la mémoire compressée (zram), au lieu
//! d'attendre que le noyau la prenne au milieu d'un combat. Elles restent
//! ouvertes et répondent toujours (la mémoire compressée se relit en
//! quelques microsecondes) ; le jeu garde la vraie mémoire.
//!
//! Par les groupes de systemd du compte (user@<uid>.service, délégués au
//! compte) : « memory.reclaim » de chaque groupe d'une application connue,
//! jamais celui de Steam et des jeux.

use std::fs;
use std::path::{Path, PathBuf};

/// Applications d'à côté, reconnues au nom de leur groupe ou de leurs
/// processus (en minuscules).
pub const APPS: [&str; 20] = [
    "discord",
    "vesktop",
    "webcord",
    "firefox",
    "librewolf",
    "zen-browser",
    "chrome",
    "chromium",
    "brave",
    "vivaldi",
    "opera",
    "microsoft-edge",
    "spotify",
    "telegram",
    "signal",
    "slack",
    "teams",
    "thunderbird",
    "obsidian",
    "element",
];

/// Jamais touchés : Steam et ce qu'il lance (les jeux), les lanceurs.
const KEEP: [&str; 9] =
    ["steam", "reaper", "pressure-vessel", "wine", "proton", "gamescope", "lutris", "heroic", "last1-panel"];

/// Le groupe du compte (« …/user@1000.service »), d'après celui du service.
pub fn user_root(self_cgroup: &str, mount: &Path) -> Option<PathBuf> {
    let path = self_cgroup.lines().find_map(|line| line.strip_prefix("0::"))?;
    let at = path.find("/user@")?;
    let end = at + path[at..].find(".service")? + ".service".len();
    Some(mount.join(path[..end].trim_start_matches('/')))
}

fn comms(group: &Path, proc_dir: &Path) -> Vec<String> {
    fs::read_to_string(group.join("cgroup.procs"))
        .unwrap_or_default()
        .lines()
        .filter_map(|pid| fs::read_to_string(proc_dir.join(pid.trim()).join("comm")).ok())
        .map(|comm| comm.trim().to_lowercase())
        .collect()
}

/// Les groupes des applications d'à côté, avec leur nom (« discord »).
pub fn groups(root: &Path, proc_dir: &Path) -> Vec<(PathBuf, &'static str)> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                stack.push(entry.path());
            }
        }
        let name = dir.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        let comms = comms(&dir, proc_dir);
        if comms.is_empty() || !dir.join("memory.reclaim").exists() {
            continue;
        }
        let kept = |text: &str| KEEP.iter().any(|keep| text.contains(keep)) || text.ends_with(".exe");
        if kept(&name) || comms.iter().any(|comm| kept(comm)) {
            continue;
        }
        if let Some(app) = APPS.iter().find(|app| name.contains(*app) || comms.iter().any(|comm| comm.contains(*app))) {
            found.push((dir, *app));
        }
    }
    found
}

fn current(group: &Path) -> u64 {
    fs::read_to_string(group.join("memory.current")).ok().and_then(|v| v.trim().parse().ok()).unwrap_or(0)
}

/// Range la moitié de la mémoire de chaque application d'à côté ; rend ce
/// qui a été libéré, par application (octets).
pub fn compress(groups: &[(PathBuf, &'static str)]) -> Vec<(&'static str, u64)> {
    let mut freed: Vec<(&'static str, u64)> = Vec::new();
    for (group, app) in groups {
        let before = current(group);
        if before < 64 << 20 {
            continue;
        }
        // EAGAIN si tout n'a pas pu être rangé : ce qui l'a été compte.
        let _ = fs::write(group.join("memory.reclaim"), (before / 2).to_string());
        let gained = before.saturating_sub(current(group));
        match freed.iter_mut().find(|(name, _)| name == app) {
            Some((_, total)) => *total += gained,
            None => freed.push((app, gained)),
        }
    }
    freed.retain(|(_, bytes)| *bytes >= 16 << 20);
    freed
}

/// La mémoire se remplit : moins d'un tiers disponible.
pub fn memory_tight(meminfo: &str) -> bool {
    let kib =
        |key: &str| meminfo.lines().find_map(|l| l.strip_prefix(key)?.split_whitespace().next()?.parse::<u64>().ok());
    matches!((kib("MemTotal:"), kib("MemAvailable:")), (Some(total), Some(avail)) if avail * 3 < total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_apps_are_found_but_never_steam() {
        let dir = tempfile::tempdir().unwrap();
        let (cg, proc_dir) = (dir.path().join("cg"), dir.path().join("proc"));
        let root = cg.join("user.slice/user-1000.slice/user@1000.service");
        assert_eq!(
            user_root("0::/user.slice/user-1000.slice/user@1000.service/app.slice/last1-panel.service\n", &cg),
            Some(root.clone())
        );
        assert_eq!(user_root("0::/system.slice/sshd.service\n", &cg), None);
        let group = |name: &str, pid: u32, comm: &str| {
            let path = root.join("app.slice").join(name);
            fs::create_dir_all(&path).unwrap();
            fs::write(path.join("cgroup.procs"), format!("{pid}\n")).unwrap();
            fs::write(path.join("memory.reclaim"), "").unwrap();
            fs::create_dir_all(proc_dir.join(pid.to_string())).unwrap();
            fs::write(proc_dir.join(pid.to_string()).join("comm"), format!("{comm}\n")).unwrap();
        };
        group("app-discord@autostart.service", 10, "Discord");
        group("app-Hyprland-firefox-22.scope", 11, "firefox");
        group("app-steam@autostart.service", 12, "steam");
        group("app-Hyprland-kitty-5.scope", 13, "kitty");
        let mut found: Vec<&str> = groups(&root, &proc_dir).into_iter().map(|(_, app)| app).collect();
        found.sort();
        assert_eq!(found, ["discord", "firefox"]);

        assert!(memory_tight("MemTotal: 16000000 kB\nMemAvailable: 4000000 kB\n"));
        assert!(!memory_tight("MemTotal: 32000000 kB\nMemAvailable: 16000000 kB\n"));
    }
}
