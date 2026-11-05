//! Génération d'images (couche Vulkan de framegen/, d'après afmf-linux) :
//! pour l'instant chez les testeurs seulement, sur tous leurs jeux, avec un
//! journal par partie résumé dans le diagnostic (donc dans leurs rapports).
//!
//! La couche est explicite : `VK_LOADER_LAYERS_ENABLE` la place sous les
//! couches implicites (MangoHud, vkBasalt, overlay Steam), et un nom inconnu
//! n'empêche pas le jeu de démarrer.

use crate::paths;
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

/// lsfg-vk (Lossless Scaling sous Linux) installé : sa couche implicite
/// génère déjà des images, la nôtre par-dessus ferait deux générations qui
/// se gênent (afmf-linux le signale : blocage de la présentation).
pub fn lsfg_installed(data_home: &Path) -> bool {
    let has_lsfg = |dir: &Path| {
        fs::read_dir(dir).ok().is_some_and(|entries| {
            entries.flatten().any(|e| e.file_name().to_string_lossy().to_lowercase().contains("lsfg"))
        })
    };
    ["/usr/share/vulkan/implicit_layer.d", "/etc/vulkan/implicit_layer.d"].iter().any(|d| has_lsfg(Path::new(d)))
        || has_lsfg(&data_home.join("vulkan/implicit_layer.d"))
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
    let safe: String = game.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).take(40).collect();
    Some(dir.join(format!("{now:012}-{safe}.log")))
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

/// Réglages des outils que le joueur s'est posés lui-même (souvent par
/// GOverlay) : lsfg-vk (Lossless Scaling), MangoHud, vkBasalt. Pour les
/// testeurs : comparer nos réglages aux leurs. Valeurs seules, sans
/// commentaires ni chemins personnels ; 400 caractères par outil.
pub fn player_tools(config_home: &Path) -> Vec<(String, String)> {
    let files = [
        ("lsfg-vk", "lsfg-vk/conf.toml"),
        ("MangoHud", "MangoHud/MangoHud.conf"),
        ("vkBasalt", "vkBasalt/vkBasalt.conf"),
        ("GOverlay", "goverlay/goverlay.conf"),
    ];
    let home = paths::home_dir().display().to_string();
    files
        .iter()
        .filter_map(|(name, rel)| {
            let text = fs::read_to_string(config_home.join(rel)).ok()?;
            let lines: Vec<String> = text
                .lines()
                .map(|l| l.split('#').next().unwrap_or_default().trim())
                .filter(|l| !l.is_empty())
                .map(|l| l.replace(&home, "~").split_whitespace().collect::<Vec<_>>().join(" "))
                .collect();
            if lines.is_empty() {
                return None;
            }
            let mut value = lines.join(" ; ");
            if value.chars().count() > 400 {
                value = value.chars().take(399).collect::<String>() + "…";
            }
            Some((format!("Réglages du joueur : {name}"), value))
        })
        .collect()
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

    #[test]
    fn players_own_tool_settings_are_summed_up() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("lsfg-vk")).unwrap();
        fs::write(
            dir.path().join("lsfg-vk/conf.toml"),
            "version = 2\n# commentaire\n[[profile]]\nactive_in = \"PioneerGame.exe\"\nmultiplier = 2\nflow_scale = 0.8\n",
        )
        .unwrap();
        let tools = player_tools(dir.path());
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].0, "Réglages du joueur : lsfg-vk");
        assert_eq!(
            tools[0].1,
            "version = 2 ; [[profile]] ; active_in = \"PioneerGame.exe\" ; multiplier = 2 ; flow_scale = 0.8"
        );
    }
}
