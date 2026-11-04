//! Lecture d'une configuration hyprlang (Hyprland 0.56 et avant :
//! hyprland.conf et ses `source = …`), pour la convertir en Lua
//! (hypr_convert.rs). Ce que fait hyprlang : commentaires `#` (`##` pour un
//! `#`), variables `$nom = valeur`, fichiers inclus, catégories
//! `nom { … }` imbriquées (`decoration { blur { size = 3 } }` vaut
//! `decoration:blur:size = 3`), blocs spéciaux à plusieurs valeurs
//! (`device { name = … }`, `windowrule { … }`).

use std::fs;
use std::path::{Path, PathBuf};

/// Catégories dont chaque bloc est un élément à part (ses clés vont ensemble).
const BLOCKS: [&str; 5] = ["device", "windowrule", "layerrule", "monitorv2", "permission"];

/// Une ligne `clé = valeur`, clé complète (`decoration:blur:size`, `bind`…).
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub key: String,
    pub value: String,
    /// Fichier et numéro de ligne, pour dire ce qui n'a pas été repris.
    pub file: PathBuf,
    pub number: usize,
}

/// Un élément de la configuration, dans l'ordre du fichier.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Line(Line),
    /// Bloc spécial : son nom (`device`…) et ses clés.
    Block {
        kind: String,
        fields: Vec<(String, String)>,
        file: PathBuf,
        number: usize,
    },
}

#[derive(Debug, Default)]
pub struct Config {
    pub items: Vec<Item>,
    /// Fichiers lus (le principal et ses `source`).
    pub files: Vec<PathBuf>,
    /// Ce qui n'a pas pu être lu (fichier introuvable, accolade en trop…).
    pub problems: Vec<String>,
    variables: Vec<(String, String)>,
    home: PathBuf,
}

type OpenBlock = (String, Vec<(String, String)>, usize, usize);

/// Retire le commentaire d'une ligne (`##` est un `#`).
fn strip_comment(line: &str) -> String {
    // Ligne qui commence par « # » : commentaire, même « ####### ».
    if line.trim_start().starts_with('#') {
        return String::new();
    }
    let mut out = String::new();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '#' {
            if chars.peek() == Some(&'#') {
                chars.next();
                out.push('#');
                continue;
            }
            break;
        }
        out.push(c);
    }
    out.trim().to_string()
}

impl Config {
    /// Configuration faite de ces éléments seuls (déjà lus ailleurs).
    pub fn with_items(items: Vec<Item>) -> Config {
        Config { items, ..Config::default() }
    }

    /// Lit `main` et tout ce qu'il inclut.
    pub fn read(main: &Path, home: &Path) -> Config {
        let mut config = Config { home: home.to_path_buf(), ..Config::default() };
        config.read_file(main, home, &mut Vec::new(), 0);
        config
    }

    /// Lit un texte comme s'il était le fichier `path` (tests).
    pub fn parse(text: &str, path: &Path, home: &Path) -> Config {
        let mut config = Config { home: home.to_path_buf(), ..Config::default() };
        config.read_text(text, path, home, &mut Vec::new(), 0);
        config
    }

    fn expand(&self, value: &str) -> String {
        let mut out = value.to_string();
        // Les plus longs d'abord : $mainModShift avant $mainMod.
        let mut names: Vec<&(String, String)> = self.variables.iter().collect();
        names.sort_by_key(|(name, _)| std::cmp::Reverse(name.len()));
        for (name, val) in names {
            out = out.replace(&format!("${name}"), val);
        }
        // Puis les variables d'environnement, comme hyprlang ($HOME…).
        while let Some(start) = out.find('$') {
            let name: String =
                out[start + 1..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
            let value =
                if name == "HOME" { Some(self.home.to_string_lossy().into_owned()) } else { std::env::var(&name).ok() };
            match value.filter(|_| !name.is_empty()) {
                Some(value) => out.replace_range(start..start + 1 + name.len(), &value),
                None => break,
            }
        }
        out
    }

    fn read_file(&mut self, path: &Path, home: &Path, stack: &mut Vec<String>, depth: usize) {
        if depth > 16 || self.files.iter().any(|f| f == path) {
            return;
        }
        match fs::read_to_string(path) {
            Ok(text) => {
                self.files.push(path.to_path_buf());
                self.read_text(&text, path, home, stack, depth);
            }
            Err(error) => self.problems.push(format!("{} : {error}", path.display())),
        }
    }

    fn read_text(&mut self, text: &str, path: &Path, home: &Path, stack: &mut Vec<String>, depth: usize) {
        // Bloc spécial en cours : son nom, ses clés, sa ligne, sa profondeur.
        let mut block: Option<OpenBlock> = None;
        for (index, raw) in text.lines().enumerate() {
            let number = index + 1;
            let line = strip_comment(raw);
            if line.is_empty() {
                continue;
            }
            if line == "}" {
                match &block {
                    Some((_, _, _, level)) if *level == stack.len() => {
                        let (kind, fields, start, _) = block.take().unwrap();
                        self.items.push(Item::Block { kind, fields, file: path.to_path_buf(), number: start });
                    }
                    _ => {}
                }
                if stack.pop().is_none() {
                    self.problems.push(format!("{}:{number} : « }} » en trop", path.display()));
                }
                continue;
            }
            if let Some(name) = line.strip_suffix('{').map(str::trim) {
                let name = self.expand(name);
                stack.push(name.clone());
                if block.is_none() && BLOCKS.contains(&name.as_str()) {
                    block = Some((name, Vec::new(), number, stack.len()));
                }
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                self.problems.push(format!("{}:{number} : ligne illisible : {line}", path.display()));
                continue;
            };
            let (key, value) = (key.trim(), value.trim());
            if let Some(name) = key.strip_prefix('$') {
                let value = self.expand(value);
                self.variables.retain(|(n, _)| n != name);
                self.variables.push((name.to_string(), value));
                continue;
            }
            let value = self.expand(value);
            if let Some((_, fields, _, level)) = &mut block {
                // Clés du bloc, relatives à lui (sous-catégories comprises).
                let inner = stack[*level..].join(":");
                let key = if inner.is_empty() { key.to_string() } else { format!("{inner}:{key}") };
                fields.push((key, value));
                continue;
            }
            let full = if stack.is_empty() { key.to_string() } else { format!("{}:{key}", stack.join(":")) };
            if key == "source" {
                for file in sources(&value, path, home) {
                    let mut inner = Vec::new();
                    self.read_file(&file, home, &mut inner, depth + 1);
                }
                continue;
            }
            self.items.push(Item::Line(Line { key: full, value, file: path.to_path_buf(), number }));
        }
        if block.is_some() || !stack.is_empty() {
            self.problems.push(format!("{} : accolade non fermée", path.display()));
        }
    }
}

/// Les fichiers d'un `source = …` : `~`, chemin relatif au fichier, `*`.
fn sources(value: &str, from: &Path, home: &Path) -> Vec<PathBuf> {
    let value = value.trim();
    let path = if let Some(rest) = value.strip_prefix("~/") {
        home.join(rest)
    } else if value.starts_with('/') {
        PathBuf::from(value)
    } else {
        from.parent().unwrap_or(Path::new(".")).join(value)
    };
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if !name.contains('*') {
        return vec![path];
    }
    let (prefix, suffix) = name.split_once('*').unwrap_or((&name, ""));
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut found: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.file_name().is_some_and(|n| {
                    let n = n.to_string_lossy();
                    n.starts_with(prefix) && n.ends_with(suffix)
                })
        })
        .collect();
    found.sort();
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(config: &Config) -> Vec<(String, String)> {
        config
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Line(l) => Some((l.key.clone(), l.value.clone())),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn variables_categories_sources_and_blocks() {
        let dir = tempfile::tempdir().unwrap();
        let hypr = dir.path().join(".config/hypr");
        fs::create_dir_all(hypr.join("conf")).unwrap();
        fs::write(hypr.join("conf/a.conf"), "env = A,1\n").unwrap();
        fs::write(hypr.join("conf/b.conf"), "env = B,2\n").unwrap();
        fs::write(hypr.join("keys.conf"), "bind = $mod, Q, exec, $term\n").unwrap();
        let text = "\
$mod = SUPER
$term = kitty # le terminal
$modShift = SUPER SHIFT
##########
source = $HOME/.config/hypr/keys.conf
source = ./conf/*.conf
general {
    gaps_in = 5
    col.active_border = rgba(33ccffee) rgba(00ff99ee) 45deg
}
decoration {
    blur {
        size = 3
    }
}
animations {
    bezier = quick, 0.15, 0, 0.1, 1
}
device {
    name = epic-mouse
    sensitivity = -0.5
}
bind = $modShift, C, exec, echo ##1
";
        let config = Config::parse(text, &hypr.join("hyprland.conf"), dir.path());
        assert!(config.problems.is_empty(), "{:?}", config.problems);
        assert_eq!(
            lines(&config),
            [
                ("bind".to_string(), "SUPER, Q, exec, kitty".to_string()),
                ("env".to_string(), "A,1".to_string()),
                ("env".to_string(), "B,2".to_string()),
                ("general:gaps_in".to_string(), "5".to_string()),
                ("general:col.active_border".to_string(), "rgba(33ccffee) rgba(00ff99ee) 45deg".to_string()),
                ("decoration:blur:size".to_string(), "3".to_string()),
                ("animations:bezier".to_string(), "quick, 0.15, 0, 0.1, 1".to_string()),
                ("bind".to_string(), "SUPER SHIFT, C, exec, echo #1".to_string()),
            ]
        );
        assert!(config.items.iter().any(|item| matches!(item,
            Item::Block { kind, fields, .. } if kind == "device"
                && fields == &[("name".to_string(), "epic-mouse".to_string()), ("sensitivity".to_string(), "-0.5".to_string())])));
        assert_eq!(config.files.len(), 3, "keys.conf, a.conf, b.conf (le texte n'est pas un fichier lu)");
    }

    #[test]
    fn missing_source_and_braces_are_reported() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::parse("source = ~/nope.conf\ngeneral {\n", &dir.path().join("h.conf"), dir.path());
        assert_eq!(config.problems.len(), 2, "{:?}", config.problems);
    }
}
