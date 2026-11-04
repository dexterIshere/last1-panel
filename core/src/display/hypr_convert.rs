//! Conversion d'une configuration hyprlang (Hyprland 0.56 et avant) en Lua
//! (Hyprland 0.57 et suivants, qui ne lit plus les `.conf`). Ce qui ne se
//! convertit pas avec certitude n'est pas deviné : gardé en commentaire dans
//! le fichier Lua et listé dans `skipped`, pour que la conversion ne soit
//! appliquée que vérifiée (Hyprland --verify-config) et complète.
//!
//! Noms et formes d'après les sources de Hyprland 0.57
//! (src/config/lua/bindings/*, example/hyprland.lua) ; types des options
//! d'après src/config/values/ConfigValues.cpp (hypr_options.rs).

use super::hypr_conf::{Config, Item, Line};
use super::hypr_lua::string;
use super::hypr_options::{Kind, OPTIONS};

/// Résultat : le texte Lua et ce qui n'a pas été repris.
#[derive(Debug, Default)]
pub struct Conversion {
    pub lua: String,
    pub skipped: Vec<String>,
    /// Changements faits pour que Hyprland 0.57 accepte (valeur bornée…).
    pub notes: Vec<String>,
    /// Ce qui se lance à l'ouverture de session (`exec-once`…), en Lua :
    /// relancé à la main quand la session a démarré sur l'exemple.
    pub start: Vec<String>,
}

/// Arbre des options (`general = { gaps_in = 5 }`), dans l'ordre d'écriture.
#[derive(Debug, Default)]
struct Tree {
    entries: Vec<(String, Node)>,
}

#[derive(Debug)]
enum Node {
    Value(String),
    Tree(Tree),
}

impl Tree {
    fn set(&mut self, path: &[&str], value: String) {
        let (first, rest) = (path[0], &path[1..]);
        let position = self.entries.iter().position(|(k, _)| k == first);
        if rest.is_empty() {
            match position {
                Some(i) => self.entries[i].1 = Node::Value(value),
                None => self.entries.push((first.to_string(), Node::Value(value))),
            }
            return;
        }
        let i = match position {
            Some(i) if matches!(self.entries[i].1, Node::Tree(_)) => i,
            Some(i) => {
                self.entries[i].1 = Node::Tree(Tree::default());
                i
            }
            None => {
                self.entries.push((first.to_string(), Node::Tree(Tree::default())));
                self.entries.len() - 1
            }
        };
        if let Node::Tree(tree) = &mut self.entries[i].1 {
            tree.set(rest, value);
        }
    }

    fn write(&self, out: &mut String, indent: usize) {
        let pad = "    ".repeat(indent);
        for (key, node) in &self.entries {
            match node {
                Node::Value(value) => out.push_str(&format!("{pad}{} = {value},\n", lua_key(key))),
                Node::Tree(tree) => {
                    out.push_str(&format!("{pad}{} = {{\n", lua_key(key)));
                    tree.write(out, indent + 1);
                    out.push_str(&format!("{pad}}},\n"));
                }
            }
        }
    }
}

/// Clé de table Lua : nom simple, sinon `["…"]`.
fn lua_key(key: &str) -> String {
    let simple = key.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if simple { key.to_string() } else { format!("[{}]", string(key)) }
}

/// Champs acceptés par hl.device (docs/hyprland-lua.md §4.6).
const DEVICE_FIELDS: &[&str] = &[
    "sensitivity",
    "accel_profile",
    "rotation",
    "kb_file",
    "kb_layout",
    "kb_variant",
    "kb_options",
    "kb_rules",
    "kb_model",
    "repeat_rate",
    "repeat_delay",
    "natural_scroll",
    "tap_button_map",
    "numlock_by_default",
    "resolve_binds_by_sym",
    "disable_while_typing",
    "clickfinger_behavior",
    "middle_button_emulation",
    "tap_to_click",
    "tap_and_drag",
    "drag_lock",
    "left_handed",
    "scroll_method",
    "scroll_button",
    "scroll_button_lock",
    "scroll_points",
    "scroll_factor",
    "transform",
    "output",
    "enabled",
    "region_position",
    "absolute_region_position",
    "region_size",
    "relative_input",
    "active_area_position",
    "active_area_size",
    "flip_x",
    "flip_y",
    "drag_3fg",
    "keybinds",
    "share_states",
    "release_pressed_on_close",
    "tags",
];

fn kind_of(path: &str) -> Option<Kind> {
    OPTIONS.iter().find(|(name, _)| *name == path).map(|(_, kind)| *kind)
}

/// Options de hyprland.conf (0.54 à 0.56) supprimées par Hyprland 0.57 :
/// sans effet, retirées avec une note (comparaison des listes d'options des
/// sources de Hyprland).
pub(crate) const REMOVED: [&str; 8] = [
    "debug:fifo_pending_workaround",
    "decoration:shadow:ignore_window",
    "dwindle:pseudotile",
    "misc:vfr",
    "render:cm_fs_passthrough",
    // Encore acceptées par 0.57, sans effet, et signalées à chaque démarrage
    // (« deprecated config option »).
    "debug:invalidate_fp16",
    "dwindle:special_scale_factor",
    "master:special_scale_factor",
];

/// Options dépréciées par Hyprland 0.57 qui ont changé de place.
pub(crate) const MOVED: [(&str, &str); 1] = [("misc:layers_hog_keyboard_focus", "layers:layers_hog_keyboard_focus")];

fn bool_value(value: &str) -> Option<bool> {
    // hyprlang lit le début : « yes, please :) » de son propre exemple.
    let word: String = value.trim().chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
    match word.to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Some(true),
        "false" | "no" | "off" | "0" => Some(false),
        _ => None,
    }
}

/// Nombre Lua (entier, décimal ou 0x…), tel quel s'il est valide.
fn number(value: &str) -> Option<String> {
    let v = value.trim();
    if let Some(hex) = v.strip_prefix("0x").or_else(|| v.strip_prefix("0X")) {
        return i64::from_str_radix(hex, 16).ok().map(|n| n.to_string());
    }
    v.parse::<f64>().ok().filter(|n| n.is_finite()).map(|_| v.trim_start_matches('+').to_string())
}

/// Une valeur hyprlang écrite en Lua selon le type de l'option.
pub fn value(kind: Kind, raw: &str) -> Option<String> {
    let raw = raw.trim();
    match kind {
        Kind::Bool => bool_value(raw).map(|b| b.to_string()),
        // Un oui/non en booléen : les entiers l'acceptent, pas en texte
        // (options passées de booléen à liste de choix en 0.57).
        Kind::Int => number(raw)
            .filter(|n| !n.contains('.'))
            .or_else(|| bool_value(raw).filter(|_| raw.split_whitespace().count() == 1).map(|b| b.to_string()))
            .or_else(|| Some(string(raw))),
        Kind::Float => number(raw),
        Kind::String | Kind::Color | Kind::Vec2 => Some(string(raw)),
        Kind::FontWeight => number(raw).or_else(|| Some(string(raw))),
        Kind::CssGap => {
            let parts: Vec<&str> = raw.split([',', ' ']).filter(|p| !p.is_empty()).collect();
            let nums: Option<Vec<i64>> = parts.iter().map(|p| p.parse::<i64>().ok()).collect();
            match nums?.as_slice() {
                [all] => Some(all.to_string()),
                [v, h] => Some(format!("{{ top = {v}, right = {h}, bottom = {v}, left = {h} }}")),
                [t, h, b] => Some(format!("{{ top = {t}, right = {h}, bottom = {b}, left = {h} }}")),
                [t, r, b, l] => Some(format!("{{ top = {t}, right = {r}, bottom = {b}, left = {l} }}")),
                _ => None,
            }
        }
        Kind::Gradient => {
            let mut tokens: Vec<&str> = raw.split_whitespace().collect();
            let angle = tokens.last().and_then(|t| t.strip_suffix("deg")).and_then(|a| a.parse::<f64>().ok());
            if angle.is_some() {
                tokens.pop();
            }
            match (tokens.as_slice(), angle) {
                ([one], None) => Some(string(one)),
                ([], _) => None,
                (colors, angle) => {
                    let colors: Vec<String> = colors.iter().map(|c| string(c)).collect();
                    let angle = angle.map(|a| format!(", angle = {a}")).unwrap_or_default();
                    Some(format!("{{ colors = {{ {} }}{angle} }}", colors.join(", ")))
                }
            }
        }
    }
}

fn split_args(value: &str, count: usize) -> Vec<String> {
    value.splitn(count, ',').map(|s| s.trim().to_string()).collect()
}

fn skip(conversion: &mut Conversion, line: &Line, reason: &str) {
    conversion.skipped.push(format!(
        "{}:{} {} = {} ({reason})",
        line.file.display(),
        line.number,
        line.key,
        line.value
    ));
}

/// La configuration entière en Lua.
pub fn convert(config: &Config) -> Conversion {
    let mut conversion = Conversion::default();
    let mut options = Tree::default();
    let (mut env, mut monitors, mut curves, mut animations, mut devices, mut start, mut shutdown) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let (mut rules, mut workspaces, mut gestures, mut binds, mut extras) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    // Sous-menu en cours (`submap = nom[, retour]` … `submap = reset`).
    let mut submap: Option<(String, Option<String>, Vec<String>)> = None;
    let mut submaps: Vec<String> = Vec::new();
    for item in &config.items {
        let line = match item {
            Item::Line(line) => line,
            Item::Block { kind, fields, file, number } if kind == "device" => {
                let name = fields.iter().find(|(k, _)| k == "name").map(|(_, v)| v.replace(' ', "-"));
                let mut parts = vec![format!("name = {}", string(name.as_deref().unwrap_or("")))];
                for (key, raw) in fields.iter().filter(|(k, _)| k != "name") {
                    let field = key.replace('-', "_");
                    let typed = DEVICE_FIELDS.contains(&field.as_str()).then(|| {
                        ["input:", "input:touchpad:", "input:tablet:"]
                            .iter()
                            .find_map(|prefix| kind_of(&format!("{prefix}{key}")))
                            .and_then(|kind| value(kind, raw))
                    });
                    match typed.flatten() {
                        Some(v) => parts.push(format!("{field} = {v}")),
                        None => conversion.skipped.push(format!(
                            "{}:{number} device {{ {key} = {raw} }} (réglage de périphérique inconnu)",
                            file.display()
                        )),
                    }
                }
                if name.is_some() {
                    devices.push(format!("hl.device({{ {} }})", parts.join(", ")));
                }
                continue;
            }
            Item::Block { kind, fields, file, number } => {
                match super::hypr_rules::rule_block(kind, fields) {
                    Some(lua) => rules.push(lua),
                    None => conversion.skipped.push(format!("{}:{number} bloc {kind} (non repris)", file.display())),
                }
                continue;
            }
        };
        // Le mot-clé : dernier segment (`animations:bezier` est `bezier`).
        let keyword = line.key.rsplit(':').next().unwrap_or(&line.key);
        match keyword {
            "env" | "envd" => match line.value.split_once(',') {
                Some((name, val)) => env.push(format!(
                    "hl.env({}, {}{})",
                    string(name.trim()),
                    string(val.trim()),
                    if keyword == "envd" { ", true" } else { "" }
                )),
                None => skip(&mut conversion, line, "variable d'environnement illisible"),
            },
            "submap" => {
                if let Some((name, reset, inner)) = submap.take() {
                    let reset = reset.map(|r| format!("{}, ", string(&r))).unwrap_or_default();
                    submaps.push(format!(
                        "hl.define_submap({}, {reset}function()\n{}\nend)",
                        string(&name),
                        inner.iter().map(|b| format!("    {b}")).collect::<Vec<_>>().join("\n")
                    ));
                }
                let (name, reset) = match line.value.split_once(',') {
                    Some((n, r)) => (n.trim().to_string(), Some(r.trim().to_string())),
                    None => (line.value.trim().to_string(), None),
                };
                if name != "reset" {
                    submap = Some((name, reset, Vec::new()));
                }
            }
            "unbind" => {
                let target = if line.value.trim() == "all" {
                    Some("all".to_string())
                } else {
                    line.value.split_once(',').and_then(|(m, k)| super::hypr_dispatch::keys(m, k))
                };
                match target {
                    Some(t) => binds.push(format!("hl.unbind({})", string(&t))),
                    None => skip(&mut conversion, line, "unbind illisible"),
                }
            }
            k if k.starts_with("bind") => match super::hypr_dispatch::bind(&k[4..], &line.value) {
                Some(lua) => match &mut submap {
                    Some((_, _, inner)) => inner.push(lua),
                    None => binds.push(lua),
                },
                None => skip(&mut conversion, line, "raccourci sans équivalent sûr"),
            },
            "windowrule" | "windowrulev2" => match super::hypr_rules::window_rule(&line.value) {
                Some(lua) => rules.push(lua),
                None => skip(&mut conversion, line, "règle de fenêtre sans équivalent sûr"),
            },
            "layerrule" => match super::hypr_rules::layer_rule(&line.value) {
                Some(lua) => rules.push(lua),
                None => skip(&mut conversion, line, "règle de couche sans équivalent sûr"),
            },
            "workspace" => match super::hypr_rules::workspace_rule(&line.value) {
                Some(lua) => workspaces.push(lua),
                None => skip(&mut conversion, line, "règle d'espace de travail sans équivalent sûr"),
            },
            k if k.starts_with("gesture") => match super::hypr_rules::gesture(&k[7..], &line.value) {
                Some(lua) => gestures.push(lua),
                None => skip(&mut conversion, line, "geste sans équivalent sûr"),
            },
            "permission" => {
                let parts: Vec<&str> = line.value.split(',').map(str::trim).collect();
                match parts.as_slice() {
                    [bin, kind, mode] => {
                        extras.push(format!("hl.permission({}, {}, {})", string(bin), string(kind), string(mode)))
                    }
                    _ => skip(&mut conversion, line, "permission illisible"),
                }
            }
            "monitor" => match super::hyprland::Rule::parse(&format!("monitor = {}", line.value)) {
                Some(rule) => monitors.push(rule.lua()),
                None => skip(&mut conversion, line, "règle d'écran non reprise"),
            },
            "bezier" => {
                let args = split_args(&line.value, 5);
                let points: Option<Vec<String>> = args
                    .get(1..5)
                    .map(|p| p.to_vec())
                    .and_then(|p| p.iter().map(|n| number(n)).collect::<Option<Vec<String>>>());
                // Hyprland 0.57 borne chaque coordonnée à [-1, 2].
                let points = points.map(|p| {
                    p.iter()
                        .map(|n| {
                            let v: f64 = n.parse().unwrap_or(0.0);
                            if (-1.0..=2.0).contains(&v) { n.clone() } else { v.clamp(-1.0, 2.0).to_string() }
                        })
                        .collect::<Vec<String>>()
                });
                if line
                    .value
                    .split(',')
                    .skip(1)
                    .filter_map(|n| n.trim().parse::<f64>().ok())
                    .any(|v| !(-1.0..=2.0).contains(&v))
                {
                    conversion.notes.push(format!("{}:{} courbe bornée à [-1, 2]", line.file.display(), line.number));
                }
                match (args.first(), points) {
                    (Some(name), Some(p)) if p.len() == 4 => curves.push(format!(
                        "hl.curve({}, {{ type = \"bezier\", points = {{ {{{}, {}}}, {{{}, {}}} }} }})",
                        string(name),
                        p[0],
                        p[1],
                        p[2],
                        p[3]
                    )),
                    _ => skip(&mut conversion, line, "courbe illisible"),
                }
            }
            "animation" => {
                let args = split_args(&line.value, 5);
                let enabled = args.get(1).and_then(|v| bool_value(v));
                match (args.first(), enabled) {
                    (Some(leaf), Some(enabled)) => {
                        let mut fields = vec![format!("leaf = {}", string(leaf)), format!("enabled = {enabled}")];
                        if let Some(speed) =
                            args.get(2).and_then(|s| number(s)).filter(|s| s.parse::<f64>().is_ok_and(|v| v > 0.0))
                        {
                            // Hyprland 0.57 borne la vitesse à 100.
                            let speed = if speed.parse::<f64>().is_ok_and(|v| v > 100.0) {
                                conversion.notes.push(format!(
                                    "{}:{} vitesse d'animation {speed} ramenée à 100 (maximum de Hyprland 0.57)",
                                    line.file.display(),
                                    line.number
                                ));
                                "100".to_string()
                            } else {
                                speed
                            };
                            fields.push(format!("speed = {speed}"));
                        }
                        if let Some(curve) = args.get(3).filter(|c| !c.is_empty()) {
                            fields.push(format!("bezier = {}", string(curve)));
                        }
                        if let Some(style) = args.get(4).filter(|s| !s.is_empty()) {
                            fields.push(format!("style = {}", string(style)));
                        }
                        animations.push(format!("hl.animation({{ {} }})", fields.join(", ")));
                    }
                    _ => skip(&mut conversion, line, "animation illisible"),
                }
            }
            // `exec` aussi au démarrage : en Lua, hl.exec_cmd hors d'un
            // évènement part avant que le bureau existe (et à chaque essai
            // --verify-config).
            "exec-once" | "exec" => start.push(format!("    hl.exec_cmd({})", string(&line.value))),
            "execr-once" | "execr" => start.push(format!("    hl.dispatch(hl.dsp.exec_raw({}))", string(&line.value))),
            "exec-shutdown" => shutdown.push(format!("    hl.exec_cmd({})", string(&line.value))),
            _ if line.key.starts_with("plugin:") || keyword == "plugin" => {
                skip(&mut conversion, line, "extension (plugin) : à reprendre à la main")
            }
            _ if REMOVED.contains(&line.key.as_str()) => conversion.notes.push(format!(
                "{}:{} {} = {} (option supprimée par Hyprland 0.57, sans effet : retirée)",
                line.file.display(),
                line.number,
                line.key,
                line.value
            )),
            _ => match kind_of(&line.key) {
                Some(kind) => match value(kind, &line.value) {
                    Some(v) => {
                        let moved = MOVED.iter().find(|(old, _)| *old == line.key).map(|(_, new)| *new);
                        let key = moved.unwrap_or(&line.key).replace('-', "_");
                        let path: Vec<&str> = key.split([':', '.']).collect();
                        options.set(&path, v);
                    }
                    None => skip(&mut conversion, line, "valeur illisible pour ce réglage"),
                },
                None => skip(&mut conversion, line, "réglage inconnu de Hyprland 0.57"),
            },
        }
    }
    let mut lua = String::from(
        "-- Configuration convertie depuis hyprland.conf par Last1 Panel (Hyprland 0.57 ne lit plus les .conf).\n\
         -- L'ancienne configuration est gardée telle quelle à côté.\n\n",
    );
    let mut section = |title: &str, lines: &[String]| {
        if !lines.is_empty() {
            lua.push_str(&format!("-- {title}\n{}\n\n", lines.join("\n")));
        }
    };
    section("Variables d'environnement", &env);
    section("Écrans", &monitors);
    let mut config_text = String::new();
    options.write(&mut config_text, 1);
    if !config_text.is_empty() {
        section("Réglages", &[format!("hl.config({{\n{config_text}}})")]);
    }
    section("Courbes", &curves);
    section("Animations", &animations);
    section("Périphériques", &devices);
    section("Espaces de travail", &workspaces);
    section("Règles de fenêtres et de couches", &rules);
    section("Gestes", &gestures);
    section("Permissions", &extras);
    if let Some((name, reset, inner)) = submap.take() {
        let reset = reset.map(|r| format!("{}, ", string(&r))).unwrap_or_default();
        submaps.push(format!(
            "hl.define_submap({}, {reset}function()\n{}\nend)",
            string(&name),
            inner.iter().map(|b| format!("    {b}")).collect::<Vec<_>>().join("\n")
        ));
    }
    section("Raccourcis", &binds);
    section("Sous-menus de raccourcis", &submaps);
    conversion.start = start.iter().map(|l| l.trim().to_string()).collect();
    if !start.is_empty() {
        section("Au démarrage", &[format!("hl.on(\"hyprland.start\", function()\n{}\nend)", start.join("\n"))]);
    }
    if !shutdown.is_empty() {
        section("À l'arrêt", &[format!("hl.on(\"hyprland.shutdown\", function()\n{}\nend)", shutdown.join("\n"))]);
    }
    if !conversion.notes.is_empty() {
        lua.push_str("-- Changements faits pour Hyprland 0.57 :\n");
        for n in &conversion.notes {
            lua.push_str(&format!("-- {n}\n"));
        }
    }
    if !conversion.skipped.is_empty() {
        lua.push_str("-- Non repris automatiquement :\n");
        for s in &conversion.skipped {
            lua.push_str(&format!("-- {s}\n"));
        }
    }
    conversion.lua = lua;
    conversion
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn values_are_typed() {
        assert_eq!(value(Kind::Bool, "yes").as_deref(), Some("true"));
        assert_eq!(value(Kind::Bool, "0").as_deref(), Some("false"));
        assert_eq!(value(Kind::Int, "0xff").as_deref(), Some("255"));
        assert_eq!(value(Kind::Int, "true").as_deref(), Some("true"));
        assert_eq!(value(Kind::Int, "per_monitor").as_deref(), Some("\"per_monitor\""));
        assert_eq!(value(Kind::Float, "0.1696").as_deref(), Some("0.1696"));
        assert_eq!(value(Kind::Color, "rgba(595959aa)").as_deref(), Some("\"rgba(595959aa)\""));
        assert_eq!(value(Kind::CssGap, "5").as_deref(), Some("5"));
        assert_eq!(value(Kind::CssGap, "5,10,5,10").as_deref(), Some("{ top = 5, right = 10, bottom = 5, left = 10 }"));
        assert_eq!(
            value(Kind::Gradient, "rgba(33ccffee) rgba(00ff99ee) 45deg").as_deref(),
            Some("{ colors = { \"rgba(33ccffee)\", \"rgba(00ff99ee)\" }, angle = 45 }")
        );
        assert_eq!(value(Kind::Gradient, "rgba(595959aa)").as_deref(), Some("\"rgba(595959aa)\""));
    }

    #[test]
    fn options_env_monitors_animations_and_start() {
        let text = "\
monitor = DP-1, 2560x1440@144, 0x0, 1
env = XCURSOR_SIZE,24
exec-once = waybar & hyprpaper
general {
    gaps_in = 5
    gaps_out = 10
    col.active_border = rgba(33ccffee) rgba(00ff99ee) 45deg
    layout = dwindle
}
decoration:blur:enabled = yes
animations {
    enabled = true
    bezier = myBezier, 0.05, 0.9, 0.1, 1.05
    animation = windows, 1, 7, myBezier, popin 80%
}
device {
    name = epic-mouse
    sensitivity = -0.5
}
misc:not_an_option = 3
";
        let config = Config::parse(text, Path::new("/h/hyprland.conf"), Path::new("/h"));
        let c = convert(&config);
        assert!(c.lua.contains("hl.env(\"XCURSOR_SIZE\", \"24\")"), "{}", c.lua);
        assert!(c.lua.contains("hl.monitor({ output = \"DP-1\", mode = \"2560x1440@144\""), "{}", c.lua);
        assert!(c.lua.contains(
            "hl.config({\n    general = {\n        gaps_in = 5,\n        gaps_out = 10,\n        col = {\n            active_border = { colors = { \"rgba(33ccffee)\", \"rgba(00ff99ee)\" }, angle = 45 },\n        },\n        layout = \"dwindle\",\n    },\n    decoration = {\n        blur = {\n            enabled = true,\n        },\n    },\n    animations = {\n        enabled = true,\n    },\n})"
        ), "{}", c.lua);
        assert!(c.lua.contains("hl.curve(\"myBezier\", { type = \"bezier\", points = { {0.05, 0.9}, {0.1, 1.05} } })"));
        assert!(c.lua.contains(
            "hl.animation({ leaf = \"windows\", enabled = true, speed = 7, bezier = \"myBezier\", style = \"popin 80%\" })"
        ));
        assert!(c.lua.contains("hl.device({ name = \"epic-mouse\", sensitivity = -0.5 })"), "{}", c.lua);
        assert!(c.lua.contains("hl.on(\"hyprland.start\", function()\n    hl.exec_cmd(\"waybar & hyprpaper\")\nend)"));
        assert_eq!(c.skipped.len(), 1, "{:?}", c.skipped);
        assert!(c.skipped[0].contains("misc:not_an_option"));
    }
}
