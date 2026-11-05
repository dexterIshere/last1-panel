//! Ponctuation française de la DA v2 (docs/DA.md de go-to-linux) : espace fine
//! insécable (U+202F) avant « ; », « : », « ? » et « ! » dans les textes
//! français affichés, pour qu'aucune ligne ne commence par l'un d'eux. Les
//! textes français sont le premier argument de chaque `t(fr, en)`, dans
//! l'interface (Svelte, TypeScript) et dans le cœur (Rust) ; le code, les
//! clés et les textes anglais ne sont pas concernés.

use std::fs;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn sources() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "svelte" || e == "ts" || e == "rs") {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    for dir in ["src", "core/src", "src-tauri/src"] {
        walk(&root().join(dir), &mut out);
    }
    assert!(out.len() > 50, "sources introuvables");
    out
}

/// Morceaux de texte (début, fin, en octets) des chaînes du premier argument
/// de chaque appel `t(` : jusqu'à la virgule ou la parenthèse fermante de
/// même niveau. Le code d'un gabarit JavaScript (`${…}`) est sauté.
fn french_spans(src: &str) -> Vec<(usize, usize)> {
    let bytes = src.as_bytes();
    let mut spans = Vec::new();
    for (call, _) in src.match_indices("t(") {
        let before = src[..call].chars().next_back();
        if before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '$' || c == '.') {
            continue;
        }
        let (mut i, mut depth) = (call + 2, 0i32);
        while i < bytes.len() {
            match bytes[i] {
                quote @ (b'"' | b'\'' | b'`') => {
                    i += 1;
                    let mut start = i;
                    while i < bytes.len() && bytes[i] != quote {
                        if bytes[i] == b'\\' {
                            i += 2;
                        } else if quote == b'`' && bytes[i..].starts_with(b"${") {
                            spans.push((start, i));
                            let mut inner = 1;
                            i += 2;
                            while i < bytes.len() && inner > 0 {
                                inner += match bytes[i] {
                                    b'{' => 1,
                                    b'}' => -1,
                                    _ => 0,
                                };
                                i += 1;
                            }
                            start = i;
                        } else {
                            i += 1;
                        }
                    }
                    spans.push((start, i.min(bytes.len())));
                }
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => {
                    depth -= 1;
                    if depth < 0 {
                        break;
                    }
                }
                b',' if depth == 0 => break,
                _ => {}
            }
            i += 1;
        }
    }
    spans
}

/// Espaces ordinaires (ou insécables larges) avant ; : ? ! dans ces textes.
fn wide_spaces(src: &str) -> Vec<String> {
    let mut found = Vec::new();
    for (start, end) in french_spans(src) {
        let text = &src[start..end];
        let mut previous = None;
        for (at, c) in text.char_indices() {
            if matches!(c, ';' | ':' | '?' | '!') && matches!(previous, Some(' ' | '\u{a0}')) {
                let line = src[..start + at].matches('\n').count() + 1;
                found.push(format!("ligne {line} : « {} »", text.trim()));
            }
            previous = Some(c);
        }
    }
    found
}

#[test]
fn french_texts_use_a_narrow_space_before_punctuation() {
    let mut problems = Vec::new();
    for path in sources() {
        let src = fs::read_to_string(&path).unwrap();
        for problem in wide_spaces(&src) {
            problems.push(format!("{} {problem}", path.strip_prefix(root()).unwrap().display()));
        }
    }
    assert!(problems.is_empty(), "espace ordinaire avant ; : ? !, attendu U+202F :\n{}", problems.join("\n"));
}

#[test]
fn only_french_text_is_checked() {
    // Texte français, sur une ou plusieurs lignes, gabarit, format! de Rust.
    assert_eq!(wide_spaces(r#"t("Enregistré : appliqué.", "Saved: applied.")"#).len(), 1);
    assert_eq!(wide_spaces("{t(\n  \"Steam a fini ; bien.\",\n  \"Steam is done.\",\n)}").len(), 1);
    assert_eq!(wide_spaces("t(`« ${p.name} » supprimé : oui`, `deleted`)").len(), 1);
    assert_eq!(wide_spaces(r#"t(&format!("{x} : prêt"), &format!("{x}: ready"))"#).len(), 1);
    assert!(wide_spaces("t(\"Enregistré\u{202f}: appliqué.\", \"Saved : applied.\")").is_empty());
    // Le code n'est pas du texte : ternaire, gabarit, autre fonction, anglais.
    assert!(wide_spaces("t(app.amd ? \"AMD\" : \"NVIDIA\", \"x\")").is_empty());
    assert!(wide_spaces("t(`${n > 1 ? \"s\" : \"\"} cœurs`, `cores`)").is_empty());
    assert!(wide_spaces("format(\"a : b\"); set(\"c : d\")").is_empty());
    assert!(wide_spaces("t(\"Oui\", \"Yes : no\")").is_empty());
}
