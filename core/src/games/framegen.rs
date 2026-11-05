//! Génération d'images (couche Vulkan de framegen/, d'après afmf-linux) :
//! pour l'instant chez les testeurs seulement, sur tous leurs jeux, avec un
//! journal par partie résumé dans le diagnostic (donc dans leurs rapports).
//!
//! La couche est explicite : `VK_LOADER_LAYERS_ENABLE` la place sous les
//! couches implicites (MangoHud, vkBasalt, overlay Steam), et un nom inconnu
//! n'empêche pas le jeu de démarrer.

use super::profile::GameProfile;
use crate::paths;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

/// Variable du chargeur Vulkan qui allume des couches par leur nom.
pub const LOADER_ENABLE: &str = "VK_LOADER_LAYERS_ENABLE";

/// PC de testeur : marqué par go-to-linux (compte testeur sur last1.fun), ou à
/// la main hors go-to-linux (`~/.config/last1-panel/testeur`).
pub fn tester(gtl_state: &Path, config_dir: &Path) -> bool {
    gtl_state.join("testeur").exists() || config_dir.join("testeur").exists()
}

/// Manifeste de la couche, du paquet ou de l'AppImage.
pub fn installed(data_home: &Path) -> bool {
    Path::new("/usr/share/vulkan/explicit_layer.d").join(paths::FRAMEGEN_LAYER).is_file()
        || data_home.join("vulkan/explicit_layer.d").join(paths::FRAMEGEN_LAYER).is_file()
}

/// Journaux des parties d'essai (30 gardés).
pub fn log_dir(data_home: &Path) -> PathBuf {
    data_home.join("last1-panel/framegen")
}

/// Jeu lancé dans gamescope : la couche ne génère que dans le jeu, pas dans
/// le compositeur, qui reçoit la même variable.
pub fn under_gamescope(command: &[String]) -> bool {
    command.iter().any(|arg| Path::new(arg).file_name().is_some_and(|name| name == "gamescope"))
}

/// Variable du pilote NVIDIA qui allume Smooth Motion (sa génération d'images).
pub const SMOOTH_MOTION: &str = "NVPRESENT_ENABLE_SMOOTH_MOTION";

/// Pourquoi la couche ne s'applique pas à un jeu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Off {
    /// Hors essai : rien ne change.
    NotTester,
    /// Couche absente (paquet ou AppImage sans elle).
    NotInstalled,
    /// Désactivée pour ce jeu (ou pour tous les jeux).
    Disabled,
    /// Smooth Motion génère déjà des images : jamais deux générateurs.
    SmoothMotion,
}

/// Smooth Motion allumé pour ce lancement : par nos variables (`env`) ou
/// par celles du joueur.
pub fn smooth_motion(env: &[(String, String)], existing: impl Fn(&str) -> Option<String>) -> bool {
    let on = |v: &str| !v.trim().is_empty() && v.trim() != "0";
    env.iter().any(|(name, value)| name == SMOOTH_MOTION && on(value))
        || existing(SMOOTH_MOTION).is_some_and(|v| on(&v))
}

/// La couche pour ce jeu : activée chez les testeurs quand elle est
/// installée, sauf réglage contraire ou Smooth Motion.
pub fn decide(profile: &GameProfile, tester: bool, installed: bool, smooth_motion: bool) -> Result<(), Off> {
    if !tester {
        Err(Off::NotTester)
    } else if !installed {
        Err(Off::NotInstalled)
    } else if profile.frame_generation == Some(false) {
        Err(Off::Disabled)
    } else if smooth_motion {
        Err(Off::SmoothMotion)
    } else {
        Ok(())
    }
}

/// Les variables du lancement selon la décision : celles de la couche
/// (`log` : journal de la partie) quand elle s'applique ; sinon aucune, et
/// désactivée par le réglage ou par Smooth Motion, son nom retiré d'un
/// `VK_LOADER_LAYERS_ENABLE` du joueur. Hors essai, rien ne change.
pub fn launch_environment(
    env: &mut Vec<(String, String)>,
    existing: impl Fn(&str) -> Option<String>,
    decision: Result<(), Off>,
    log: Option<&Path>,
    gamescope: bool,
) {
    if decision == Err(Off::NotTester) {
        return;
    }
    env.retain(|(name, _)| !name.starts_with("LAST1_FG_") && name != LOADER_ENABLE);
    match decision {
        Ok(()) => {
            if let Some(log) = log {
                env.extend(environment(&existing, log, gamescope));
            }
        }
        Err(Off::Disabled | Off::SmoothMotion) => {
            if let Some(list) = existing(LOADER_ENABLE)
                && list.split(',').any(|layer| layer.trim() == paths::FRAMEGEN_LAYER_NAME)
            {
                let kept: Vec<&str> =
                    list.split(',').filter(|layer| layer.trim() != paths::FRAMEGEN_LAYER_NAME).collect();
                env.push((LOADER_ENABLE.into(), kept.join(",")));
            }
        }
        Err(_) => {}
    }
}

/// Variables qui allument la couche pour ce jeu, journal dans `log_file`.
/// Celles de la couche sont toujours les nôtres (jamais héritées).
pub fn environment(
    existing: impl Fn(&str) -> Option<String>,
    log_file: &Path,
    gamescope: bool,
) -> Vec<(String, String)> {
    let enable = match existing(LOADER_ENABLE).filter(|v| !v.trim().is_empty()) {
        Some(list) if list.split(',').any(|layer| layer.trim() == paths::FRAMEGEN_LAYER_NAME) => list,
        Some(list) => format!("{list},{}", paths::FRAMEGEN_LAYER_NAME),
        None => paths::FRAMEGEN_LAYER_NAME.to_string(),
    };
    vec![
        (LOADER_ENABLE.into(), enable),
        ("LAST1_FG_LOG".into(), "2".into()),
        ("LAST1_FG_LOG_FILE".into(), log_file.display().to_string()),
        ("LAST1_FG_GAMESCOPE".into(), if gamescope { "1" } else { "0" }.into()),
    ]
}

/// Journal de la partie qui commence : `<jeu>-<heure>.log`, les plus vieux
/// retirés au-delà de 30.
pub fn new_log(dir: &Path, game: &str, now: i64) -> Option<PathBuf> {
    fs::create_dir_all(dir).ok()?;
    let mut logs: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "log"))
        .collect();
    logs.sort();
    while logs.len() >= 30 {
        let _ = fs::remove_file(logs.remove(0));
    }
    Some(dir.join(format!("{now:012}-{}.log", log_name(game))))
}

/// Le jeu tel qu'il est écrit dans le nom de ses journaux.
fn log_name(game: &str) -> String {
    game.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).take(40).collect()
}

/// Journaux des parties, du plus vieux au plus récent : (heure, jeu, chemin).
fn logs(dir: &Path) -> Vec<(i64, String, PathBuf)> {
    let mut logs: Vec<(i64, String, PathBuf)> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "log"))
        .filter_map(|p| {
            let stem = p.file_stem()?.to_str()?.to_string();
            let (when, game) = stem.split_once('-')?;
            Some((when.parse().ok()?, game.to_string(), p))
        })
        .collect();
    logs.sort();
    logs
}

/// Ce que la couche a fait pendant une partie, pour l'application.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LastGame {
    /// Jeu (numéro Steam, ou nom du programme).
    pub game: String,
    /// Début de la partie (secondes depuis 1970).
    pub when: i64,
    /// La couche s'est chargée (journal non vide).
    pub loaded: bool,
    /// Partie terminée : images présentées en tout, dont générées.
    pub presents: Option<u64>,
    pub generated: Option<u64>,
    /// Pourquoi elle est restée en retrait (texte de la couche).
    pub held_back: Option<String>,
    /// Sa première alerte (texte de la couche).
    pub alert: Option<String>,
}

fn read_game(when: i64, game: String, path: &Path) -> Option<LastGame> {
    let text = fs::read_to_string(path).ok()?;
    let body = |line: &str| line.split_once("] ").map(|(_, b)| b.trim().to_string()).unwrap_or_default();
    let number = |text: &str, after: &str| -> Option<u64> {
        text.split_once(after)?.1.split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()
    };
    // « swapchain 0x… destroyed after 9000 presents: 4400 generated, … » :
    // la dernière swapchain détruite, celle de la fin de la partie.
    let destroyed = text.lines().rfind(|l| l.contains("destroyed after"));
    Some(LastGame {
        game,
        when,
        loaded: !text.trim().is_empty(),
        presents: destroyed.and_then(|l| number(l, "destroyed after ")),
        generated: destroyed.and_then(|l| number(l, "presents: ")),
        held_back: text.lines().find(|l| l.contains("pass-through") || l.contains("unavailable")).map(body),
        alert: text
            .lines()
            .find(|l| l.contains("[last1-framegen warn]") || l.contains("[last1-framegen error]"))
            .map(body),
    })
}

/// Dernière partie d'essai de chaque jeu.
pub fn last_games(dir: &Path) -> Vec<LastGame> {
    let mut out: Vec<LastGame> = Vec::new();
    for (when, game, path) in logs(dir).into_iter().rev() {
        if !out.iter().any(|g| g.game == game)
            && let Some(last) = read_game(when, game, &path)
        {
            out.push(last);
        }
    }
    out
}

/// Dernière partie d'essai de ce jeu.
pub fn last_game(dir: &Path, game: &str) -> Option<LastGame> {
    let name = log_name(game);
    let (when, game, path) = logs(dir).into_iter().rev().find(|(_, g, _)| *g == name)?;
    read_game(when, game, &path)
}

/// Résumé de la dernière partie d'essai, pour le diagnostic : ce que la couche
/// a fait (générées / présentées), ou pourquoi elle est restée en retrait, et
/// sa première alerte. `None` : aucune partie d'essai.
pub fn summary(dir: &Path) -> Option<String> {
    let mut logs: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "log"))
        .collect();
    logs.sort();
    let last = logs.pop()?;
    let text = fs::read_to_string(&last).ok()?;
    let game = last
        .file_stem()
        .and_then(|s| s.to_str())
        .and_then(|s| s.split_once('-'))
        .map(|(_, g)| g.to_string())
        .unwrap_or_default();
    let body = |line: &str| line.split_once("] ").map(|(_, b)| b.trim().to_string()).unwrap_or_default();
    // « swapchain 0x… destroyed after 9000 presents: 4400 generated, 100 skipped (…) »
    let destroyed: Vec<String> = text
        .lines()
        .filter(|l| l.contains("destroyed after"))
        .map(|l| body(l).split_once(" destroyed ").map(|(_, r)| r.to_string()).unwrap_or_default())
        .collect();
    let off = text.lines().find(|l| l.contains("pass-through") || l.contains("unavailable")).map(body);
    let alert =
        text.lines().find(|l| l.contains("[last1-framegen warn]") || l.contains("[last1-framegen error]")).map(body);
    let mut parts = vec![game];
    match destroyed.last() {
        Some(last) => parts.push(last.clone()),
        None if text.trim().is_empty() => parts.push("couche pas chargée (journal vide)".into()),
        None => parts.push("partie en cours ou arrêtée net".into()),
    }
    if let Some(off) = off {
        parts.push(off);
    }
    if let Some(alert) = alert.filter(|a| Some(a) != parts.last()) {
        parts.push(alert);
    }
    let mut out = parts.join(" — ");
    if out.chars().count() > 400 {
        out = out.chars().take(399).collect::<String>() + "…";
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_enabled_once_after_the_users_layers() {
        let log = Path::new("/home/a/.local/share/last1-panel/framegen/1-g.log");
        let none = environment(|_| None, log, false);
        assert_eq!(none[0], (LOADER_ENABLE.to_string(), "VK_LAYER_LAST1_framegen".to_string()));
        assert_eq!(none[3].1, "0");
        let mine = environment(|n| (n == LOADER_ENABLE).then(|| "VK_LAYER_foo".to_string()), log, true);
        assert_eq!(mine[0].1, "VK_LAYER_foo,VK_LAYER_LAST1_framegen");
        assert_eq!(mine[3].1, "1");
        let again = environment(|n| (n == LOADER_ENABLE).then(|| "VK_LAYER_LAST1_framegen".to_string()), log, false);
        assert_eq!(again[0].1, "VK_LAYER_LAST1_framegen");
        assert!(under_gamescope(&["/usr/bin/gamescope".into(), "-W".into(), "--".into(), "game".into()]));
        assert!(!under_gamescope(&["/games/proton".into(), "waitforexitandrun".into()]));
    }

    #[test]
    fn last_test_game_summed_up() {
        let dir = tempfile::tempdir().unwrap();
        assert!(summary(dir.path()).is_none());
        let first = new_log(dir.path(), "ARC Raiders", 100).unwrap();
        fs::write(&first, "").unwrap();
        assert!(summary(dir.path()).unwrap().contains("couche pas chargée"));
        let second = new_log(dir.path(), "ARC Raiders", 200).unwrap();
        fs::write(
            &second,
            "[last1-framegen info] device 0x1: layer queue on family 2 (normal priority)\n\
             [last1-framegen warn] swapchain 0x2: replaced by a new swapchain; pass-through from now on\n\
             [last1-framegen info] swapchain 0x3 destroyed after 9000 presents: 4400 generated, 100 skipped (100 no free image, 0 no history, 0 held back by the governor)\n",
        )
        .unwrap();
        let text = summary(dir.path()).unwrap();
        assert!(text.starts_with("ARC_Raiders — after 9000 presents: 4400 generated"), "{text}");
        assert!(text.contains("replaced by a new swapchain"), "{text}");
        for i in 0..40 {
            fs::write(new_log(dir.path(), "g", 300 + i).unwrap(), "").unwrap();
        }
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 30);
    }

    fn var(vars: &[(String, String)], name: &str) -> Option<String> {
        vars.iter().find(|(n, _)| n == name).map(|(_, v)| v.clone())
    }

    #[test]
    fn layer_follows_the_game_setting() {
        let on = GameProfile::default();
        let off = GameProfile { frame_generation: Some(false), ..Default::default() };
        // Par défaut chez les testeurs : activée ; le réglage du jeu gagne.
        assert_eq!(decide(&on, true, true, false), Ok(()));
        assert_eq!(decide(&off, true, true, false), Err(Off::Disabled));
        // Hors essai, ou couche absente (paquet, AppImage) : jamais.
        assert_eq!(decide(&on, false, true, false), Err(Off::NotTester));
        assert_eq!(decide(&on, true, false, false), Err(Off::NotInstalled));
        // Smooth Motion : jamais deux générateurs.
        assert_eq!(decide(&on, true, true, true), Err(Off::SmoothMotion));

        let log = Path::new("/home/a/.local/share/last1-panel/framegen/1-g.log");
        let mut env = vec![("DXVK_HDR".to_string(), "1".to_string())];
        launch_environment(&mut env, |_| None, Ok(()), Some(log), false);
        assert_eq!(var(&env, LOADER_ENABLE).as_deref(), Some("VK_LAYER_LAST1_framegen"));
        assert_eq!(var(&env, "LAST1_FG_LOG_FILE").as_deref(), Some(log.to_str().unwrap()));
        assert_eq!(var(&env, "DXVK_HDR").as_deref(), Some("1"));

        // Désactivée : aucune variable de la couche.
        for decision in [Err(Off::Disabled), Err(Off::SmoothMotion), Err(Off::NotInstalled)] {
            let mut env = vec![("DXVK_HDR".to_string(), "1".to_string())];
            launch_environment(&mut env, |_| None, decision, Some(log), false);
            assert_eq!(env, [("DXVK_HDR".to_string(), "1".to_string())], "{decision:?}");
        }
        // VK_LOADER_LAYERS_ENABLE du joueur : ses couches gardées, la nôtre
        // retirée quand elle est désactivée.
        let user = |n: &str| (n == LOADER_ENABLE).then(|| "VK_LAYER_foo,VK_LAYER_LAST1_framegen".to_string());
        let mut env = Vec::new();
        launch_environment(&mut env, user, Err(Off::Disabled), None, false);
        assert_eq!(env, [(LOADER_ENABLE.to_string(), "VK_LAYER_foo".to_string())]);
        let mut env = Vec::new();
        launch_environment(
            &mut env,
            |n| (n == LOADER_ENABLE).then(|| "VK_LAYER_foo".to_string()),
            Err(Off::Disabled),
            None,
            false,
        );
        assert!(env.is_empty(), "liste du joueur sans la nôtre : laissée telle quelle");
        // Hors essai : rien ne change, même sa liste.
        let mut env = Vec::new();
        launch_environment(&mut env, user, Err(Off::NotTester), None, false);
        assert!(env.is_empty());
        // gamescope : la couche le sait.
        let mut env = Vec::new();
        launch_environment(&mut env, |_| None, Ok(()), Some(log), true);
        assert_eq!(var(&env, "LAST1_FG_GAMESCOPE").as_deref(), Some("1"));
    }

    #[test]
    fn smooth_motion_seen_from_ours_or_the_players() {
        let ours = vec![(SMOOTH_MOTION.to_string(), "1".to_string())];
        assert!(smooth_motion(&ours, |_| None));
        assert!(smooth_motion(&[], |n| (n == SMOOTH_MOTION).then(|| "1".to_string())));
        assert!(!smooth_motion(&[], |n| (n == SMOOTH_MOTION).then(|| "0".to_string())));
        assert!(!smooth_motion(&[], |_| None));
    }

    #[test]
    fn last_game_of_each_game() {
        let dir = tempfile::tempdir().unwrap();
        assert!(last_game(dir.path(), "1808500").is_none());
        fs::write(new_log(dir.path(), "1808500", 100).unwrap(), "").unwrap();
        fs::write(
            new_log(dir.path(), "1808500", 200).unwrap(),
            "[last1-framegen info] swapchain 0x3 destroyed after 9000 presents: 4400 generated, 100 skipped (100 no free image, 0 no history, 0 held back by the governor)\n",
        )
        .unwrap();
        fs::write(
            new_log(dir.path(), "730", 300).unwrap(),
            "[last1-framegen info] swapchain 0x1: pass-through (surface capabilities unavailable)\n",
        )
        .unwrap();
        let arc = last_game(dir.path(), "1808500").unwrap();
        assert_eq!((arc.when, arc.loaded, arc.presents, arc.generated), (200, true, Some(9000), Some(4400)));
        assert_eq!(arc.held_back, None);
        let cs = last_game(dir.path(), "730").unwrap();
        assert_eq!(cs.held_back.as_deref(), Some("swapchain 0x1: pass-through (surface capabilities unavailable)"));
        assert_eq!(cs.presents, None);
        let all = last_games(dir.path());
        assert_eq!(all.iter().map(|g| g.game.as_str()).collect::<Vec<_>>(), ["730", "1808500"]);
        // Couche pas chargée : journal vide.
        fs::write(new_log(dir.path(), "730", 400).unwrap(), "").unwrap();
        assert!(!last_game(dir.path(), "730").unwrap().loaded);
    }
}
