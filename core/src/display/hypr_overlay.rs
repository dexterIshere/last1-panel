//! Bureau Signal de go-to-linux sous Hyprland 0.57 : go-to-linux pose son
//! propre hyprland.lua (qui relit monitors.conf et gtl-machine.conf à chaque
//! chargement), sans ce que le joueur avait changé dans son hyprland.conf
//! (fond d'écran, raccourcis, réglages). Ces changements seuls — ce qui
//! diffère du hyprland.conf livré par go-to-linux, toutes versions — sont
//! convertis dans `last1-panel-perso.lua`, chargé après le bureau Signal
//! (go-to-linux garde les lignes « last1-panel » de hyprland.lua).

use super::hypr_conf::{Config, Item, Line};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

/// hyprland.conf du bureau Signal livré par go-to-linux, de la première
/// version à la dernière d'avant Hyprland 0.57 (dépôt go-to-linux,
/// linux/desktop/hypr/hyprland.conf).
const STOCK: [&str; 9] = [
    include_str!("signal_stock/v1.conf"),
    include_str!("signal_stock/v2.conf"),
    include_str!("signal_stock/v3.conf"),
    include_str!("signal_stock/v4.conf"),
    include_str!("signal_stock/v5.conf"),
    include_str!("signal_stock/v6.conf"),
    include_str!("signal_stock/v7.conf"),
    include_str!("signal_stock/v8.conf"),
    include_str!("signal_stock/v9.conf"),
];
/// Celui que la version installée de go-to-linux livre.
pub const INSTALLED_STOCK: &str = "/usr/local/share/gtl/desktop/hypr/hyprland.conf";
/// hyprland.lua du bureau Signal livré par go-to-linux.
pub const MODEL: &str = "/usr/local/share/gtl/desktop/hypr/hyprland.lua";
/// Les changements du joueur, en Lua.
pub const FILE: &str = "last1-panel-perso.lua";

/// Ce qui identifie un élément : clé et valeur, espaces ramenés à un seul.
fn identity(item: &Item) -> String {
    let squash = |s: &str| {
        let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
        s.replace(" ,", ",").replace(", ", ",")
    };
    match item {
        Item::Line(line) => format!("{}={}", line.key, squash(&line.value)),
        Item::Block { kind, fields, .. } => {
            let mut fields: Vec<String> = fields.iter().map(|(k, v)| format!("{k}={}", squash(v))).collect();
            fields.sort();
            format!("{kind}{{{}}}", fields.join(";"))
        }
    }
}

/// Éléments écrits dans `main` lui-même (pas dans ses `source`, que le
/// bureau Signal relit lui-même).
fn own_items(config: &Config, main: &Path) -> Vec<Item> {
    config
        .items
        .iter()
        .filter(|item| match item {
            Item::Line(line) => line.file == main,
            Item::Block { file, .. } => file == main,
        })
        .cloned()
        .collect()
}

/// Premier mot d'une commande (`swaybg -i …` → `swaybg`).
fn program(command: &str) -> Option<&str> {
    command.split_whitespace().next().map(|p| p.rsplit('/').next().unwrap_or(p))
}

/// Les changements du joueur dans `user` (son hyprland.conf) par rapport au
/// hyprland.conf livré (`stocks` : textes ; vide → ceux connus et
/// l'installé). `None` : pas un hyprland.conf de Signal, ou rien de changé.
pub fn personal(user: &Path, home: &Path, stocks: &[String]) -> Option<Config> {
    let mine = own_items(&Config::read(user, home), user);
    let mut texts: Vec<String> = stocks.to_vec();
    if texts.is_empty() {
        texts = STOCK.iter().map(|s| s.to_string()).collect();
        if let Ok(installed) = fs::read_to_string(INSTALLED_STOCK) {
            texts.push(installed);
        }
    }
    // Lus comme des fichiers (variables, catégories), puis effacés.
    let dir = std::env::temp_dir().join(format!("last1-panel-signal-{}", std::process::id()));
    fs::create_dir_all(&dir).ok()?;
    let result = compare(&mine, &texts, &dir, home);
    let _ = fs::remove_dir_all(&dir);
    result
}

fn compare(mine: &[Item], texts: &[String], dir: &Path, home: &Path) -> Option<Config> {
    let mut union = HashSet::new();
    let mut best: Option<(usize, Vec<Item>)> = None;
    let my_ids: HashSet<String> = mine.iter().map(identity).collect();
    for (i, text) in texts.iter().enumerate() {
        let path = dir.join(format!("stock{i}.conf"));
        fs::write(&path, text).ok()?;
        let items = own_items(&Config::read(&path, home), &path);
        let common = items.iter().filter(|item| my_ids.contains(&identity(item))).count();
        union.extend(items.iter().map(identity));
        if best.as_ref().is_none_or(|(n, _)| common > *n) {
            best = Some((common, items));
        }
    }
    let (common, stock) = best?;
    // Moins de la moitié en commun : pas le bureau Signal, rien à faire ici.
    if common * 2 < stock.len() {
        return None;
    }
    let removed: Vec<&Item> = stock.iter().filter(|item| !my_ids.contains(&identity(item))).collect();
    let mut out = Vec::new();
    // Raccourcis du bureau Signal retirés ou changés par le joueur.
    for item in &removed {
        if let Item::Line(line) = item
            && line.key.starts_with("bind")
            && let Some((mods, rest)) = line.value.split_once(',')
        {
            let key = rest.split(',').next().unwrap_or_default().trim();
            out.push(Item::Line(Line {
                key: "unbind".into(),
                value: format!("{}, {key}", mods.trim()),
                ..line.clone()
            }));
        }
    }
    for item in mine.iter().filter(|item| !union.contains(&identity(item))) {
        let mut item = item.clone();
        // Lancement remplacé (fond d'écran…) : celui du bureau Signal arrêté
        // d'abord.
        if let Item::Line(line) = &mut item
            && line.key.starts_with("exec")
            && let Some(prog) = program(&line.value)
        {
            let replaced = removed
                .iter()
                .any(|r| matches!(r, Item::Line(l) if l.key.starts_with("exec") && program(&l.value) == Some(prog)));
            if replaced {
                line.value = format!("sleep 1; pkill -x {prog}; {}", line.value);
            }
        }
        out.push(item);
    }
    (!out.is_empty()).then(|| Config::with_items(out))
}

/// Sans les lignes de Last1 Panel ni les lignes vides.
fn core_text(text: &str) -> String {
    text.lines().filter(|l| !l.contains("last1-panel") && !l.trim().is_empty()).collect::<Vec<_>>().join("\n")
}

/// hyprland.lua est celui du bureau Signal livré par go-to-linux (`model`),
/// plus éventuellement les lignes de Last1 Panel.
pub fn signal_lua_in_place(hypr_dir: &Path, model: &Path) -> bool {
    let (Ok(lua), Ok(model)) = (fs::read_to_string(hypr_dir.join("hyprland.lua")), fs::read_to_string(model)) else {
        return false;
    };
    core_text(&lua) == core_text(&model)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_players_changes_are_kept() {
        let dir = tempfile::tempdir().unwrap();
        let stock = "$gtl = /usr/local/lib/gtl/desktop\nsource = ~/.config/hypr/monitors.conf\nexec-once = swaybg -m fill -i /usr/local/share/gtl/desktop/signal.png\nexec-once = $gtl/gtl-barre\ngeneral {\n    gaps_out = 12\n    border_size = 2\n}\nbind = SUPER, Return, exec, alacritty\nbind = SUPER, Q, killactive,\nbind = SUPER, F, fullscreen, 0\n";
        let user = dir.path().join("hyprland.conf");
        fs::write(
            &user,
            "$gtl = /usr/local/lib/gtl/desktop\nsource = ~/.config/hypr/monitors.conf\nexec-once = swaybg -m fill -i ~/Images/chat.png\nexec-once = $gtl/gtl-barre\ngeneral {\n    gaps_out = 5\n    border_size = 2\n}\nbind = SUPER, Return, exec, kitty\nbind = SUPER, Q, killactive,\nbind = SUPER, F, fullscreen, 0\nbind = SUPER, B, exec, firefox\n",
        )
        .unwrap();
        fs::write(dir.path().join("monitors.conf"), "monitor = , preferred, auto, 1\n").unwrap();
        let home = dir.path().parent().unwrap();
        let overlay = personal(&user, home, &[stock.to_string()]).unwrap();
        let lua = super::super::hypr_convert::convert(&overlay);
        assert!(lua.lua.contains("hl.unbind(\"SUPER + Return\")"), "{}", lua.lua);
        assert!(lua.lua.contains("kitty") && lua.lua.contains("firefox"), "{}", lua.lua);
        assert!(lua.lua.contains("gaps_out = 5"), "{}", lua.lua);
        assert!(!lua.lua.contains("border_size") && !lua.lua.contains("killactive") && !lua.lua.contains("monitor"));
        assert!(lua.start.iter().any(|l| l.contains("pkill -x swaybg; swaybg -m fill -i")), "{:?}", lua.start);
        assert!(!lua.start.iter().any(|l| l.contains("gtl-barre")));
        // Rien de changé : rien à ajouter.
        fs::write(&user, stock).unwrap();
        assert!(personal(&user, home, &[stock.to_string()]).is_none());
    }

    #[test]
    fn known_stock_versions_are_signal_desktops() {
        let dir = tempfile::tempdir().unwrap();
        let user = dir.path().join("hyprland.conf");
        fs::write(&user, STOCK[8]).unwrap();
        assert!(personal(&user, dir.path(), &[]).is_none(), "fichier livré tel quel");
        let model = dir.path().join("model.lua");
        fs::write(&model, "require(\"./signal.lua\")\n").unwrap();
        fs::write(
            dir.path().join("hyprland.lua"),
            "require(\"./signal.lua\")\n\npcall(require, \"./last1-panel-mouse.lua\")\n",
        )
        .unwrap();
        assert!(signal_lua_in_place(dir.path(), &model));
    }
}
