//! Clips des parties, comme Medal : pendant un jeu, gpu-screen-recorder garde
//! la dernière minute en mémoire (encodeur de la carte graphique : presque
//! rien en images par seconde) ; F8 l'enregistre, F9 lance ou arrête un
//! enregistrement complet. Les vidéos vont dans « Vidéos/Last1 Panel ».
//!
//! Les touches sont lues sur les claviers eux-mêmes (/dev/input/event*),
//! comme Vice (splitshotz/Vice, GPL-3.0) : elles marchent sur tout bureau et
//! même quand le jeu garde le clavier pour lui. Il faut pour cela la règle
//! udev « uaccess » posée par l'installation (Fix des clips).

use crate::exec::which;
use serde::Serialize;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::{Duration, UNIX_EPOCH};

/// Secondes gardées en mémoire, enregistrées par F8.
pub const REPLAY_SECS: u32 = 60;
/// Règle udev qui ouvre les claviers au compte connecté.
pub const UDEV_RULE: &str = "/etc/udev/rules.d/70-last1-panel-input.rules";

/// Une touche des clips.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// F8 : les 60 dernières secondes.
    Clip,
    /// F9 : lancer ou arrêter un enregistrement.
    Record,
}

/// Code des touches (linux/input-event-codes.h).
const KEY_F8: u16 = 66;
const KEY_F9: u16 = 67;
const EV_KEY: u16 = 1;

/// Le dossier des vidéos de ce compte (xdg-user-dirs), « Last1 Panel » dedans.
pub fn dir(home: &Path) -> PathBuf {
    let configured = fs::read_to_string(home.join(".config/user-dirs.dirs")).ok().and_then(|text| {
        text.lines().find_map(|line| {
            let value = line.strip_prefix("XDG_VIDEOS_DIR=")?.trim().trim_matches('"');
            Some(PathBuf::from(value.replace("$HOME", &home.to_string_lossy())))
        })
    });
    configured.filter(|path| path != home).unwrap_or_else(|| home.join("Videos")).join("Last1 Panel")
}

/// Une vidéo enregistrée.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Clip {
    pub name: String,
    pub path: String,
    /// Octets.
    pub size: u64,
    /// Secondes depuis 1970.
    pub modified: u64,
}

/// Les vidéos du dossier, de la plus récente à la plus ancienne.
pub fn list(dir: &Path) -> Vec<Clip> {
    let mut clips: Vec<Clip> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| {
            entry.path().extension().is_some_and(|ext| ["mp4", "mkv", "webm"].contains(&ext.to_string_lossy().as_ref()))
        })
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;
            Some(Clip {
                name: entry.path().file_stem()?.to_string_lossy().into_owned(),
                path: entry.path().to_string_lossy().into_owned(),
                size: meta.len(),
                modified: meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs(),
            })
        })
        .collect();
    clips.sort_by_key(|clip| std::cmp::Reverse(clip.modified));
    clips
}

/// État de la page Clips.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipsState {
    /// gpu-screen-recorder installé.
    pub installed: bool,
    /// Les claviers sont lisibles : F8 et F9 marchent.
    pub hotkeys: bool,
    pub enabled: bool,
    /// Vice (splitshotz/Vice), installé par le joueur, fait les clips :
    /// Last1 Panel s'efface (deux enregistreurs, et F9 pris par les deux).
    pub vice: bool,
    pub folder: String,
    pub clips: Vec<Clip>,
}

pub fn installed() -> bool {
    which("gpu-screen-recorder").is_some()
}

/// Vice démarre avec la session (ou tourne) ?
pub fn vice_in_use() -> bool {
    ["is-enabled", "is-active"].iter().any(|query| {
        std::process::Command::new("systemctl")
            .args(["--user", query, "--quiet", "vice.service"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    })
}

/// Les claviers sont-ils lisibles par ce compte, par notre règle udev ? Celle
/// de Vice, quand go-to-linux le retire, part avec lui : sans la nôtre, F8 et
/// F9 cesseraient de marcher au prochain redémarrage.
pub fn hotkeys_ready(rule: &Path, dev: &Path) -> bool {
    rule.exists() && keyboards(dev).next().is_some()
}

/// Les périphériques d'entrée que ce compte peut lire.
fn keyboards(dev: &Path) -> impl Iterator<Item = PathBuf> {
    fs::read_dir(dev.join("input"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.file_name().is_some_and(|n| n.to_string_lossy().starts_with("event")))
        .filter(|path| fs::File::open(path).is_ok())
}

pub fn state(home: &Path, dev: &Path, enabled: bool) -> ClipsState {
    let folder = dir(home);
    ClipsState {
        installed: installed(),
        hotkeys: hotkeys_ready(Path::new(UDEV_RULE), dev),
        enabled,
        vice: vice_in_use(),
        clips: list(&folder),
        folder: folder.to_string_lossy().into_owned(),
    }
}

/// La commande de gpu-screen-recorder : l'écran (capture directe, sans
/// demande), 60 i/s, la dernière minute en mémoire, le son du PC, sans le
/// curseur ; `script` est appelé avec chaque vidéo enregistrée.
pub fn command(dir: &Path, script: &Path) -> Vec<String> {
    [
        "gpu-screen-recorder",
        "-w",
        "screen",
        "-f",
        "60",
        "-r",
        &REPLAY_SECS.to_string(),
        "-c",
        "mp4",
        "-a",
        "default_output",
        "-cursor",
        "no",
        "-sc",
        &script.to_string_lossy(),
        "-o",
        &dir.to_string_lossy(),
    ]
    .iter()
    .map(|arg| arg.to_string())
    .collect()
}

/// Nom d'une vidéo : le jeu et l'heure (« ARC Raiders 2026-10-21 21-30-05 »),
/// sans caractère qui gênerait un système de fichiers.
pub fn saved_name(game: &str, stamp: &str) -> String {
    let game: String =
        game.chars().map(|c| if c.is_alphanumeric() || " -_'.()".contains(c) { c } else { ' ' }).collect();
    let game = game.split_whitespace().collect::<Vec<_>>().join(" ");
    if game.is_empty() { stamp.to_string() } else { format!("{game} {stamp}") }
}

/// Renomme la vidéo que gpu-screen-recorder vient d'écrire (même dossier,
/// même extension) ; rend son nouveau chemin.
pub fn rename_saved(path: &Path, game: &str, stamp: &str) -> PathBuf {
    let ext = path.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_else(|| "mp4".into());
    let dir = path.parent().unwrap_or(Path::new("."));
    let base = saved_name(game, stamp);
    let mut target = dir.join(format!("{base}.{ext}"));
    let mut n = 2;
    while target.exists() {
        target = dir.join(format!("{base} ({n}).{ext}"));
        n += 1;
    }
    if fs::rename(path, &target).is_ok() { target } else { path.to_path_buf() }
}

/// Touche pressée dans un événement d'entrée (struct input_event : temps sur
/// 16 octets, type, code, valeur), F8 ou F9 enfoncée.
fn key_of(event: &[u8; 24]) -> Option<Key> {
    let kind = u16::from_ne_bytes([event[16], event[17]]);
    let code = u16::from_ne_bytes([event[18], event[19]]);
    let value = i32::from_ne_bytes([event[20], event[21], event[22], event[23]]);
    match (kind, code, value) {
        (EV_KEY, KEY_F8, 1) => Some(Key::Clip),
        (EV_KEY, KEY_F9, 1) => Some(Key::Record),
        _ => None,
    }
}

/// Lit F8 et F9 sur tous les claviers, branchés après coup compris (revus
/// toutes les 3 s) ; ne rend jamais la main.
pub fn watch_keys(dev: &Path, keys: Sender<Key>) {
    let mut watched: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
    let (gone, ended) = std::sync::mpsc::channel::<PathBuf>();
    loop {
        while let Ok(path) = ended.try_recv() {
            watched.remove(&path);
        }
        for path in keyboards(dev) {
            if !watched.insert(path.clone()) {
                continue;
            }
            let (keys, gone) = (keys.clone(), gone.clone());
            std::thread::spawn(move || {
                if let Ok(mut file) = fs::File::open(&path) {
                    let mut event = [0u8; 24];
                    while file.read_exact(&mut event).is_ok() {
                        if let Some(key) = key_of(&event) {
                            let _ = keys.send(key);
                        }
                    }
                }
                // Débranché : revu au prochain passage.
                let _ = gone.send(path);
            });
        }
        std::thread::sleep(Duration::from_secs(3));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn videos_go_to_the_videos_folder() {
        let home = tempfile::tempdir().unwrap();
        assert_eq!(dir(home.path()), home.path().join("Videos/Last1 Panel"));
        fs::create_dir_all(home.path().join(".config")).unwrap();
        fs::write(home.path().join(".config/user-dirs.dirs"), "XDG_VIDEOS_DIR=\"$HOME/Vidéos\"\n").unwrap();
        assert_eq!(dir(home.path()), home.path().join("Vidéos/Last1 Panel"));
    }

    #[test]
    fn clips_are_named_after_the_game_and_listed_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let saved = dir.path().join("Replay_2026-10-21_21-30-05.mp4");
        fs::write(&saved, b"video").unwrap();
        let renamed = rename_saved(&saved, "ARC Raiders: Ultimate/Edition", "2026-10-21 21-30-05");
        assert_eq!(renamed, dir.path().join("ARC Raiders Ultimate Edition 2026-10-21 21-30-05.mp4"));
        fs::write(&saved, b"video 2").unwrap();
        let second = rename_saved(&saved, "ARC Raiders: Ultimate/Edition", "2026-10-21 21-30-05");
        assert_eq!(second, dir.path().join("ARC Raiders Ultimate Edition 2026-10-21 21-30-05 (2).mp4"));
        fs::write(dir.path().join("notes.txt"), b"x").unwrap();
        let clips = list(dir.path());
        assert_eq!(clips.len(), 2);
        assert!(clips.iter().all(|clip| clip.name.starts_with("ARC Raiders")));
    }

    #[test]
    fn f8_and_f9_are_read_from_keyboard_events() {
        let event = |kind: u16, code: u16, value: i32| {
            let mut e = [0u8; 24];
            e[16..18].copy_from_slice(&kind.to_ne_bytes());
            e[18..20].copy_from_slice(&code.to_ne_bytes());
            e[20..24].copy_from_slice(&value.to_ne_bytes());
            e
        };
        assert_eq!(key_of(&event(EV_KEY, KEY_F8, 1)), Some(Key::Clip));
        assert_eq!(key_of(&event(EV_KEY, KEY_F9, 1)), Some(Key::Record));
        // Relâchée, répétée, autre touche : rien.
        assert_eq!(key_of(&event(EV_KEY, KEY_F8, 0)), None);
        assert_eq!(key_of(&event(EV_KEY, KEY_F8, 2)), None);
        assert_eq!(key_of(&event(EV_KEY, 30, 1)), None);
    }

    #[test]
    fn recorder_keeps_the_last_minute_of_the_screen() {
        let args = command(Path::new("/home/j/Vidéos/Last1 Panel"), Path::new("/tmp/clip-saved.sh"));
        let joined = args.join(" ");
        assert!(joined.starts_with("gpu-screen-recorder -w screen -f 60 -r 60"), "{joined}");
        assert!(joined.ends_with("-sc /tmp/clip-saved.sh -o /home/j/Vidéos/Last1 Panel"), "{joined}");
    }
}
