//! Direction artistique « Signal » v2 (docs/DA.md du dépôt go-to-linux), sur
//! le modèle de web/design_test.go de go-to-linux : les couleurs ne sont
//! définies qu'une fois, dans les jetons de `:root` de src/app.css, avec les
//! mêmes noms et les mêmes valeurs que last1.fun et l'application
//! go-to-linux ; tout le reste s'y réfère. Aplats francs (ni dégradé, ni
//! ombre, ni lueur, ni flou), Pulse retiré, police de titre embarquée, aucune
//! ressource d'un tiers.

use std::fs;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn read(path: &str) -> String {
    fs::read_to_string(root().join(path)).unwrap_or_else(|e| panic!("{path} : {e}"))
}

/// Fichiers de l'interface : styles, composants et la page.
fn interface_files() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "svelte" || e == "css" || e == "html") {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(&root().join("src"), &mut out);
    assert!(out.len() > 20, "fichiers de l'interface introuvables");
    out
}

/// Bloc `:root { … }` de la feuille de style, et le reste.
fn split_root(css: &str) -> (&str, String) {
    let start = css.find(":root {").expect("bloc :root introuvable");
    let end = start + css[start..].find("\n}").expect("fin du bloc :root") + 2;
    (&css[start..end], format!("{}{}", &css[..start], &css[end..]))
}

/// Première couleur écrite en dur : `#abc`, `#aabbcc`, `%23…` (SVG en URL),
/// `rgb(`, `rgba(`, `hsl(`, `hsla(`.
fn raw_color(text: &str) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    for function in ["rgb(", "rgba(", "hsl(", "hsla("] {
        if let Some(at) = lower.find(function)
            && !lower[..at].ends_with(|c: char| c.is_ascii_alphanumeric() || c == '-')
        {
            return Some(text[at..].chars().take(24).collect());
        }
    }
    for marker in ["#", "%23"] {
        for (at, _) in lower.match_indices(marker) {
            let hex: String = lower[at + marker.len()..].chars().take_while(char::is_ascii_hexdigit).collect();
            let next = lower[at + marker.len() + hex.len()..].chars().next();
            if (3..=8).contains(&hex.len()) && !next.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_') {
                return Some(text[at..at + marker.len() + hex.len()].to_string());
            }
        }
    }
    None
}

/// Jetons de :root (nom → valeur, en majuscules).
fn tokens(root: &str) -> Vec<(String, String)> {
    root.lines()
        .filter_map(|line| {
            let line = line.trim();
            let (name, value) = line.strip_prefix("--")?.split_once(':')?;
            Some((format!("--{name}"), value.trim().trim_end_matches(';').to_ascii_uppercase()))
        })
        .collect()
}

/// Les jetons de la DA v2 partagés avec last1.fun et go-to-linux.
const SHARED: [(&str, &str); 37] = [
    ("--color-bg", "#0E0E0E"),
    ("--color-surface", "#171717"),
    ("--color-surface-raised", "#212121"),
    ("--color-border", "#333333"),
    ("--color-border-input", "#777777"),
    ("--color-heading", "#FFFFFF"),
    ("--color-text", "#D9D9D9"),
    ("--color-text-muted", "#A3A3A3"),
    ("--color-volt", "#D9F56F"),
    ("--color-volt-hover", "#E6FA94"),
    ("--color-on-volt", "#0E0E0E"),
    ("--color-link", "#FFFFFF"),
    ("--color-focus", "#FFFFFF"),
    ("--color-pill", "#FFFFFF"),
    ("--color-warning", "#FFB547"),
    ("--color-danger", "#FF6B6B"),
    ("--color-volt-soft", "#222518"),
    ("--color-volt-line", "#5F6A35"),
    ("--color-warning-soft", "#261F14"),
    ("--color-warning-line", "#6E5125"),
    ("--color-danger-soft", "#261717"),
    ("--color-danger-line", "#6E3333"),
    ("--color-gtl", "#B7A8FF"),
    ("--color-sf", "#FFC145"),
    ("--color-panel", "#5CE6D6"),
    ("--color-panel-soft", "#172826"),
    ("--color-panel-line", "#2D645E"),
    ("--color-sidebar", "#121212"),
    ("--color-surface-hover", "#2A2A2A"),
    ("--color-border-strong", "#444444"),
    ("--color-text-faint", "#8A8A8A"),
    ("--font-display", "\"ARCHIVO\", SYSTEM-UI, \"SEGOE UI\", SANS-SERIF"),
    ("--weight-display", "900"),
    ("--radius-sm", "0"),
    ("--radius-md", "0"),
    ("--radius-lg", "0"),
    ("--radius-pill", "999PX"),
];

#[test]
fn app_css_holds_the_v2_tokens_and_nothing_else_has_colors() {
    let css = read("src/app.css");
    let (root_block, outside) = split_root(&css);
    let defined = tokens(root_block);
    for (name, value) in SHARED {
        let found = defined.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str());
        assert_eq!(found, Some(value), "jeton {name} : attendu {value} (DA v2)");
    }
    for (i, line) in outside.lines().enumerate() {
        if let Some(color) = raw_color(line) {
            panic!("app.css : couleur en dur hors de :root ({color}) : {} (ligne {})", line.trim(), i + 1);
        }
    }
}

#[test]
fn components_use_only_tokens() {
    let css = read("src/app.css");
    let defined: Vec<String> = tokens(split_root(&css).0).into_iter().map(|(n, _)| n).collect();
    // Couleurs calculées depuis les réglages du joueur (aperçu de l'écran).
    let computed = ["Preview.svelte"];
    let mut problems = Vec::new();
    for path in interface_files() {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let text = fs::read_to_string(&path).unwrap();
        let text = if name == "app.css" { split_root(&text).1 } else { text };
        for (i, line) in text.lines().enumerate() {
            if let Some(color) = raw_color(line)
                && !(computed.contains(&name.as_str()) && line.contains("${"))
            {
                problems.push(format!("{name}:{} : couleur en dur ({color})", i + 1));
            }
            for banned in ["gradient(", "--color-pulse", "glow", "drop-shadow", "blur(", "color-mix("] {
                if line.contains(banned) {
                    problems.push(format!("{name}:{} : « {banned} » interdit par la DA v2", i + 1));
                }
            }
            if let Some(shadow) = line.trim().strip_prefix("box-shadow:")
                && !shadow.contains("focus-ring")
                && shadow.trim() != "none;"
            {
                problems.push(format!("{name}:{} : ombre interdite (seul le focus) : {}", i + 1, line.trim()));
            }
            // Jetons de couleur utilisés : tous définis dans :root.
            for (at, _) in line.match_indices("var(--color-") {
                let token: String =
                    line[at + 4..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
                if !defined.contains(&token) {
                    problems.push(format!("{name}:{} : jeton jamais défini : {token}", i + 1));
                }
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn title_font_is_ours_and_nothing_comes_from_a_third_party() {
    let css = read("src/app.css");
    assert!(css.contains("url(\"/archivo-900.woff2\")"), "Archivo 900 embarquée (/archivo-900.woff2)");
    let font = fs::metadata(root().join("static/archivo-900.woff2")).expect("static/archivo-900.woff2");
    assert!(font.len() < 60 * 1024, "sous-ensemble de 60 Ko au plus");
    assert!(read("static/Archivo-OFL.txt").contains("SIL OPEN FONT LICENSE"), "licence OFL jointe");
    for path in interface_files() {
        let text = fs::read_to_string(&path).unwrap();
        for scheme in ["http://", "https://"] {
            for (at, _) in text.match_indices(scheme) {
                let url: String = text[at..].chars().take_while(|c| !c.is_whitespace() && *c != '"').collect();
                // Espace de noms XML des SVG : jamais chargé.
                assert!(
                    url == "http://www.w3.org/2000/svg" || url.starts_with("http://www.w3.org/2000/svg'"),
                    "{} : ressource d'un tiers : {url}",
                    path.display()
                );
            }
        }
    }
}

#[test]
fn icon_is_shapes_in_token_colors() {
    let svg = read("src-tauri/icons/source/icon.svg");
    assert!(svg.contains("viewBox=\"0 0 1024 1024\""), "viewBox carré de 1024");
    for banned in ["<text", "<image", "<style", "<script", "href="] {
        assert!(!svg.contains(banned), "icon.svg : {banned} refusé (formes seulement)");
    }
    for (at, _) in svg.match_indices("fill=\"") {
        let color: String = svg[at + 6..].chars().take_while(|c| *c != '"').collect();
        assert!(
            ["#0E0E0E", "#FFFFFF", "#5CE6D6"].contains(&color.as_str()),
            "icon.svg : couleur {color} hors des jetons (bg, heading, panel)"
        );
    }
}

#[test]
fn raw_colors_are_found() {
    assert_eq!(raw_color("  color: #fff;").as_deref(), Some("#fff"));
    assert_eq!(raw_color("background: rgb(5 6 8 / 0.7);").as_deref(), Some("rgb(5 6 8 / 0.7);"));
    assert_eq!(raw_color("stroke='%230E0E0E'").as_deref(), Some("%230E0E0E"));
    assert_eq!(raw_color("{#each items as item}"), None);
    assert_eq!(raw_color("{#if ok}<a href=\"#main\">"), None);
    assert_eq!(raw_color("color: var(--color-red);"), None);
}
