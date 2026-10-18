//! Profil de couleurs d'un jeu, dans sa seule fenêtre : table de couleurs 3D
//! (.cube) appliquée à chaque image du jeu par vkBasalt, couche Vulkan livrée
//! avec Last1 Panel (tous les jeux Windows par Proton, et les jeux Vulkan).
//!
//! Les couleurs de l'écran (vibrance du pilote, puis courbes de la carte
//! graphique) s'appliquent encore par-dessus l'image du jeu : la table les
//! compense, pour que le jeu montre exactement son profil et le bureau le
//! sien.

use crate::color::curve::{Channel, ColorAdjust};
use crate::error::CoreResult;
use crate::settings::{ColorProfile, Settings};
use std::fmt::Write;
use std::fs;

/// Points par côté de la table (33³, la taille habituelle des .cube).
pub const SIZE: usize = 33;
/// Précision de l'inversion des courbes de l'écran.
const INVERSE_POINTS: usize = 4096;

/// Variable qui active notre couche Vulkan (manifeste last1-panel-color.json).
pub const LAYER_ENV: &str = "LAST1_PANEL_COLOR";

/// Courbe d'un canal de l'écran, à inverser.
struct Inverse(Vec<f64>);

impl Inverse {
    fn new(channel: &Channel) -> Self {
        Self((0..INVERSE_POINTS).map(|i| channel.apply(i as f64 / (INVERSE_POINTS - 1) as f64)).collect())
    }

    /// L'entrée qui donne `y` à l'écran (la plus petite ; 0 ou 1 hors de portée).
    fn apply(&self, y: f64) -> f64 {
        let index = self.0.partition_point(|&value| value < y);
        (index.min(INVERSE_POINTS - 1)) as f64 / (INVERSE_POINTS - 1) as f64
    }
}

/// Saturation de la vibrance : 50 % inchangée, 0 noir et blanc, 100 % doublée.
fn saturation(vibrance: f64) -> f64 {
    (vibrance.clamp(0.0, 100.0) / 50.0).max(0.0)
}

/// vkBasalt garde la table sur 8 bits en tronquant (255 × valeur) : une
/// demi-marche ajoutée, pour que la valeur gardée soit l'arrondi.
fn stored(value: f64) -> f64 {
    (value + 0.5 / 255.0).min(1.0)
}

fn saturate([r, g, b]: [f64; 3], factor: f64) -> [f64; 3] {
    let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    [r, g, b].map(|c| (luma + (c - luma) * factor).clamp(0.0, 1.0))
}

/// Contenu du fichier .cube : `profile` dans la fenêtre du jeu, sur un écran
/// réglé avec les courbes `screen` et la vibrance `screen_vibrance`.
pub fn cube(profile: &ColorAdjust, vibrance: f64, screen: &ColorAdjust, screen_vibrance: f64) -> String {
    let inverse = [Inverse::new(&screen.red), Inverse::new(&screen.green), Inverse::new(&screen.blue)];
    let curves = [profile.red, profile.green, profile.blue];
    let game_saturation = saturation(vibrance);
    let screen_saturation = saturation(screen_vibrance);

    let mut text = String::with_capacity(SIZE * SIZE * SIZE * 26 + 64);
    text.push_str("TITLE \"Last1 Panel\"\n");
    let _ = writeln!(text, "LUT_3D_SIZE {SIZE}");
    // Rouge d'abord, puis vert, puis bleu (ordre des fichiers .cube).
    for b in 0..SIZE {
        for g in 0..SIZE {
            for r in 0..SIZE {
                let input = [r, g, b].map(|v| v as f64 / (SIZE - 1) as f64);
                // Le profil : vibrance, puis courbes…
                let mut color = saturate(input, game_saturation);
                for (c, curve) in color.iter_mut().zip(&curves) {
                    *c = curve.apply(*c);
                }
                // … puis l'inverse de l'écran : ses courbes, puis sa vibrance.
                for (c, inverse) in color.iter_mut().zip(&inverse) {
                    *c = inverse.apply(*c);
                }
                if screen_saturation > 0.0 {
                    color = saturate(color, 1.0 / screen_saturation);
                }
                let [r, g, b] = color.map(stored);
                let _ = writeln!(text, "{r:.6} {g:.6} {b:.6}");
            }
        }
    }
    text
}

/// Variables à poser pour que le jeu montre `profile` : table et réglages de
/// vkBasalt écrits dans ~/.cache/last1-panel/color. Rien si la couche Vulkan
/// n'est pas installée.
pub fn game_environment(settings: &Settings, profile: &ColorProfile) -> CoreResult<Vec<(String, String)>> {
    if !crate::paths::color_layer_installed() {
        return Ok(Vec::new());
    }
    let dir = crate::paths::cache_dir().join("color");
    fs::create_dir_all(&dir)?;
    let name: String = profile.id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
    let lut = dir.join(format!("{name}.cube"));
    let config = dir.join(format!("{name}.conf"));
    let (screen, screen_vibrance) = settings.screen_colors();
    write_atomic(&lut, &cube(&profile.color, profile.vibrance, &screen, screen_vibrance))?;
    write_atomic(
        &config,
        &format!(
            "# Last1 Panel : profil de couleurs « {} » (refait à chaque lancement).\n\
             effects = lut\n\
             lutFile = \"{}\"\n\
             # Touche Arrêt défil : profil coupé / remis, pour comparer (rare\n\
             # dans les jeux, contrairement à Début, la touche par défaut).\n\
             toggleKey = Scroll_Lock\n\
             enableOnLaunch = True\n",
            profile.name.replace('\n', " "),
            lut.display()
        ),
    )?;
    Ok(vec![
        (LAYER_ENV.to_string(), "1".to_string()),
        ("VKBASALT_CONFIG_FILE".to_string(), config.display().to_string()),
    ])
}

/// Deux jeux lancés ensemble lisent un fichier entier, jamais à moitié écrit.
fn write_atomic(path: &std::path::Path, text: &str) -> CoreResult<()> {
    let temporary = path.with_extension(format!("{}.new", std::process::id()));
    fs::write(&temporary, text)?;
    fs::rename(&temporary, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La table telle que vkBasalt la garde (lut_cube.cpp : 8 bits tronqués).
    fn entries(text: &str) -> Vec<[f64; 3]> {
        text.lines()
            .filter(|line| line.starts_with(|c: char| c.is_ascii_digit()))
            .map(|line| {
                let v: Vec<f64> =
                    line.split(' ').map(|n| f64::from((255.0 * n.parse::<f32>().unwrap()) as u8) / 255.0).collect();
                [v[0], v[1], v[2]]
            })
            .collect()
    }

    fn at(table: &[[f64; 3]], r: usize, g: usize, b: usize) -> [f64; 3] {
        table[(b * SIZE + g) * SIZE + r]
    }

    #[test]
    fn neutral_profile_on_neutral_screen_changes_nothing() {
        let text = cube(&ColorAdjust::default(), 50.0, &ColorAdjust::default(), 50.0);
        assert!(text.contains("LUT_3D_SIZE 33"));
        let table = entries(&text);
        assert_eq!(table.len(), SIZE * SIZE * SIZE);
        for (r, g, b) in [(0, 0, 0), (32, 32, 32), (10, 20, 30), (32, 0, 0)] {
            let want = [r, g, b].map(|v| v as f64 / 32.0);
            let got = at(&table, r, g, b);
            for c in 0..3 {
                assert_eq!((got[c] * 255.0).round(), (want[c] * 255.0).round(), "{r} {g} {b} : {got:?}");
            }
        }
    }

    #[test]
    fn game_shows_its_profile_whatever_the_screen() {
        let guide = ColorAdjust {
            red: Channel { brightness: 52.0, contrast: 47.0, gamma: 1.47 },
            green: Channel { brightness: 52.0, contrast: 47.0, gamma: 1.47 },
            blue: Channel { brightness: 62.0, contrast: 72.0, gamma: 1.65 },
            ..Default::default()
        };
        // Écran neutre : la table est le profil.
        let table = entries(&cube(&guide, 50.0, &ColorAdjust::default(), 50.0));
        let grey = at(&table, 8, 8, 8)[2];
        assert!((grey - guide.blue.apply(0.25)).abs() <= 0.5 / 255.0 + 1e-9);

        // Écran déjà réglé (le guide sur le bureau) : la table compense ses
        // courbes, l'écran affiche le profil du jeu, ici neutre.
        let table = entries(&cube(&ColorAdjust::default(), 50.0, &guide, 50.0));
        for i in [4, 8, 16, 24] {
            let x = i as f64 / 32.0;
            let shown = at(&table, i, i, i);
            let on_screen = [guide.red.apply(shown[0]), guide.green.apply(shown[1]), guide.blue.apply(shown[2])];
            for c in on_screen {
                assert!((c - x).abs() < 0.01, "gris {x} affiché {on_screen:?}");
            }
        }
    }

    #[test]
    fn vibrance_saturates_colors_but_not_greys() {
        let table = entries(&cube(&ColorAdjust::default(), 75.0, &ColorAdjust::default(), 50.0));
        let grey = at(&table, 16, 16, 16);
        assert!(grey.iter().all(|c| (c - 0.5).abs() <= 0.5 / 255.0 + 1e-9));
        let red = at(&table, 24, 8, 8);
        assert!(red[0] > 0.75 && red[1] < 0.25, "{red:?}");
    }
}
