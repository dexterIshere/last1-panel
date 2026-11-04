//! Les scripts et barres du joueur qui pilotent Hyprland par `hyprctl`
//! (Waybar, scripts de ~/.config/hypr, hypridle…) : Hyprland 0.57 refuse
//! `hyprctl keyword` et les dispatchers d'avant (`dispatch workspace 1`).
//! Les appels écrits en clair sont réécrits (`hyprctl dispatch 'hl.dsp…'`,
//! `hyprctl eval 'hl.config(…)'`) ; ceux qui dépendent du script
//! (`$variable`, calcul) sont listés, jamais devinés.

use super::hypr_convert::value;
use super::hypr_options::OPTIONS;
use std::fs;
use std::path::{Path, PathBuf};

/// Comment le texte est écrit autour de la commande.
#[derive(Clone, Copy, PartialEq)]
pub enum Context {
    /// Script ou fichier lu par sh (hypridle, hyprlock…).
    Shell,
    /// Dans une chaîne JSON (configuration de Waybar) : `"` y devient `\"`.
    Json,
}

/// Fin de la commande hyprctl : séparateur de shell, ou guillemet qui
/// ferme la chaîne qui la contient (`bash -c "hyprctl …"`, chaîne JSON).
/// `None` : guillemets au milieu des arguments, la commande dépend de son
/// écriture (laissée telle quelle et signalée).
fn command_end(rest: &str, context: Context) -> Option<usize> {
    let mut end = rest.len();
    for pat in [";", "&&", "||", "|", "\n", "`", ")"] {
        if let Some(i) = rest.find(pat) {
            end = end.min(i);
        }
    }
    let segment = &rest[..end];
    let quotes: &[char] = if context == Context::Json { &['"', '\\'] } else { &['"', '\''] };
    match segment.find(quotes) {
        None => Some(end),
        // Le guillemet ferme la chaîne autour : il finit le segment.
        Some(q) if segment[q + 1..].trim().is_empty() || context == Context::Json && segment[q..].starts_with('"') => {
            Some(q)
        }
        Some(_) => None,
    }
}

/// `hyprctl keyword a:b valeur` → `hl.config(…)`.
fn keyword(rest: &str) -> Option<String> {
    let (key, val) = rest.trim().split_once(char::is_whitespace)?;
    if key == "monitor" {
        return super::hyprland::Rule::parse(&format!("monitor = {val}")).map(|r| r.lua());
    }
    let kind = OPTIONS.iter().find(|(name, _)| *name == key).map(|(_, k)| *k)?;
    let v = value(kind, val)?;
    let key = key.replace('-', "_");
    let path: Vec<&str> = key.split([':', '.']).collect();
    let mut lua = v;
    for part in path.iter().rev() {
        lua = format!("{{ {part} = {lua} }}");
    }
    Some(format!("hl.config({lua})"))
}

/// Réécrit un texte ; rend le nouveau texte et ce qui n'a pas pu l'être.
pub fn rewrite(text: &str, context: Context) -> (String, Vec<String>) {
    let mut out = String::new();
    let mut left = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find("hyprctl ") {
        out.push_str(&rest[..i]);
        let after = &rest[i + "hyprctl ".len()..];
        let Some(end) = command_end(after, context) else {
            // Arguments entre guillemets : vérifiés à la main.
            let line_end = after.find('\n').unwrap_or(after.len());
            let shown = after[..line_end].trim_end();
            if shown.starts_with("dispatch") && !shown.contains("hl.")
                || shown.starts_with("keyword")
                || shown.starts_with("--batch")
            {
                left.push(format!("hyprctl {shown}"));
            }
            out.push_str(&format!("hyprctl {}", &after[..line_end]));
            rest = &after[line_end..];
            continue;
        };
        let command = &after[..end];
        let trimmed = command.trim_end();
        let (verb, args) = trimmed.trim_start().split_once(char::is_whitespace).unwrap_or((trimmed.trim_start(), ""));
        let args = args.trim();
        let lua = match verb {
            // Déjà en Lua, ou rien à dire.
            "dispatch" if args.starts_with("hl.") || args.starts_with('\'') || args.starts_with('"') => None,
            "dispatch" | "keyword" if args.contains('$') || args.contains('`') => {
                left.push(format!("hyprctl {trimmed}"));
                None
            }
            "dispatch" => {
                let (name, dargs) = args.split_once(char::is_whitespace).unwrap_or((args, ""));
                let lua = super::hypr_dispatch::dispatcher(name, dargs).map(|l| ("dispatch", l));
                if lua.is_none() {
                    left.push(format!("hyprctl {trimmed}"));
                }
                lua
            }
            "keyword" => {
                let lua = keyword(args).map(|l| ("eval", l));
                if lua.is_none() {
                    left.push(format!("hyprctl {trimmed}"));
                }
                lua
            }
            "--batch" => {
                left.push(format!("hyprctl {trimmed}"));
                None
            }
            _ => None,
        };
        match lua {
            Some((new_verb, code)) => {
                let code = if context == Context::Json { code.replace('"', "\\\"") } else { code };
                out.push_str(&format!("hyprctl {new_verb} '{code}'"));
                out.push_str(&command[trimmed.len()..]);
            }
            None => out.push_str(&format!("hyprctl {command}")),
        }
        rest = &after[end..];
    }
    out.push_str(rest);
    (out, left)
}

/// Fichiers à regarder : configurations des outils du bureau et scripts
/// du joueur.
pub fn candidates(home: &Path) -> Vec<(PathBuf, Context)> {
    let dirs = [
        ".config/hypr",
        ".config/waybar",
        ".config/swaync",
        ".config/wlogout",
        ".config/rofi",
        ".config/wofi",
        ".config/fuzzel",
        ".config/mako",
        ".config/dunst",
        ".local/bin",
    ];
    let mut found = Vec::new();
    for dir in dirs {
        let mut stack = vec![home.join(dir)];
        while let Some(d) = stack.pop() {
            let Ok(entries) = fs::read_dir(&d) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                let Ok(meta) = entry.metadata() else { continue };
                if meta.is_dir() {
                    stack.push(path);
                } else if meta.len() < 1 << 20 {
                    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    // hyprland.conf est converti à part ; les .lua sont déjà au nouveau format.
                    if name.ends_with(".lua") || name == "hyprland.conf" || name.ends_with(".last1-panel.bak") {
                        continue;
                    }
                    let json = name.ends_with(".json")
                        || name.ends_with(".jsonc")
                        || (dir == ".config/waybar" && !name.contains('.'));
                    found.push((path, if json { Context::Json } else { Context::Shell }));
                }
            }
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}

/// Ce que la réécriture changerait : fichier, nouveau texte, ce qui reste.
pub struct Change {
    pub path: PathBuf,
    pub text: String,
    pub left: Vec<String>,
}

pub fn plan(home: &Path) -> Vec<Change> {
    candidates(home)
        .into_iter()
        .filter_map(|(path, context)| {
            let text = fs::read_to_string(&path).ok()?;
            if !text.contains("hyprctl ") {
                return None;
            }
            let (new, left) = rewrite(&text, context);
            (new != text || !left.is_empty()).then_some(Change { path, text: new, left })
        })
        .collect()
}

/// Écrit les changements, l'original gardé à côté (`.last1-panel.bak`).
pub fn apply(changes: &[Change]) -> std::io::Result<()> {
    for change in changes {
        let backup = PathBuf::from(format!("{}.last1-panel.bak", change.path.display()));
        if !backup.exists() {
            fs::copy(&change.path, &backup)?;
        }
        let permissions = fs::metadata(&change.path)?.permissions();
        fs::write(&change.path, &change.text)?;
        fs::set_permissions(&change.path, permissions)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_calls_are_rewritten_dynamic_ones_listed() {
        let script = "#!/bin/sh\nhyprctl dispatch workspace 3 && notify-send ok\nhyprctl keyword general:gaps_out 5\nhyprctl dispatch workspace \"$1\"\nhyprctl dispatch 'hl.dsp.exit()'\nhyprctl clients -j\n";
        let (new, left) = rewrite(script, Context::Shell);
        assert_eq!(
            new,
            "#!/bin/sh\nhyprctl dispatch 'hl.dsp.focus({ workspace = \"3\" })' && notify-send ok\nhyprctl eval 'hl.config({ general = { gaps_out = 5 } })'\nhyprctl dispatch workspace \"$1\"\nhyprctl dispatch 'hl.dsp.exit()'\nhyprctl clients -j\n"
        );
        assert_eq!(left, ["hyprctl dispatch workspace \"$1\""]);
        // Dans une chaîne de bash -c : réécrit.
        let (new, left) = rewrite("bash -c \"hyprctl dispatch exit\"\n", Context::Shell);
        assert_eq!((new.as_str(), left.len()), ("bash -c \"hyprctl dispatch 'hl.dsp.exit()'\"\n", 0));
        // Waybar : dans une chaîne JSON.
        let waybar = r#"{ "on-scroll-up": "hyprctl dispatch workspace e+1", "on-click": "hyprctl dispatch dpms off" }"#;
        let (new, left) = rewrite(waybar, Context::Json);
        assert!(left.is_empty());
        assert_eq!(
            new,
            r#"{ "on-scroll-up": "hyprctl dispatch 'hl.dsp.focus({ workspace = \"e+1\" })'", "on-click": "hyprctl dispatch 'hl.dsp.dpms({ action = \"off\" })'" }"#
        );
        // hypridle.conf : lu par sh.
        let (new, _) = rewrite("    on-timeout = hyprctl dispatch dpms off\n", Context::Shell);
        assert_eq!(new, "    on-timeout = hyprctl dispatch 'hl.dsp.dpms({ action = \"off\" })'\n");
    }
}
