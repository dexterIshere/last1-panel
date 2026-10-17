//! Courbes de couleur d'un écran : luminosité, contraste et gamma, comme dans
//! « Régler les paramètres de couleur du bureau » du panneau NVIDIA.
//!
//! Les valeurs se règlent comme sous Windows (luminosité et contraste à 50 %,
//! gamma à 1,00 par défaut), et donnent la même courbe que là-bas, pour que
//! chacun retrouve ses réglages : le calcul est celui de la rampe gamma de
//! Windows tel que l'emploient les outils NVIDIA sous Windows (bibliothèque
//! WindowsDisplayAPI, `DisplayGammaRamp`, LGPL-3.0, réécrit ici). Le contraste
//! étire ou resserre la courbe autour du milieu (noir rogné d'au plus 10 %,
//! ou relevé d'au plus 12,5 %), la luminosité la décale d'au plus un
//! cinquième, puis le gamma en puissance. Celui de nvidia-settings sous X11,
//! bien plus fort (contraste jusqu'à 3 fois la pente), écrasait les ombres :
//! un contraste à 72 % sur le seul bleu, réglage courant sous Windows, y
//! jaunissait toute l'image sombre.

use serde::{Deserialize, Serialize};

/// Bornes du panneau NVIDIA.
pub const PERCENT_MIN: f64 = 0.0;
pub const PERCENT_MAX: f64 = 100.0;
pub const PERCENT_DEFAULT: f64 = 50.0;
pub const GAMMA_MIN: f64 = 0.30;
pub const GAMMA_MAX: f64 = 2.80;
pub const GAMMA_DEFAULT: f64 = 1.0;

/// Nombre de points des courbes écrites dans le profil (celui de la balise
/// VCGT des profils ICC, que lcms2 écrit de toute façon sur 256 points).
pub const RAMP_SIZE: usize = 256;

/// Réglage d'un canal (ou des trois à la fois).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    /// Luminosité, 0 à 100 % (50 : inchangée).
    pub brightness: f64,
    /// Contraste, 0 à 100 % (50 : inchangé).
    pub contrast: f64,
    /// Gamma, 0,30 à 2,80 (1,00 : inchangé).
    pub gamma: f64,
}

impl Default for Channel {
    fn default() -> Self {
        Self { brightness: PERCENT_DEFAULT, contrast: PERCENT_DEFAULT, gamma: GAMMA_DEFAULT }
    }
}

impl Channel {
    /// Le même réglage, ramené dans les bornes du panneau NVIDIA.
    pub fn clamped(self) -> Self {
        let clamp = |value: f64, min: f64, max: f64, default: f64| {
            if value.is_finite() { value.clamp(min, max) } else { default }
        };
        Self {
            brightness: clamp(self.brightness, PERCENT_MIN, PERCENT_MAX, PERCENT_DEFAULT),
            contrast: clamp(self.contrast, PERCENT_MIN, PERCENT_MAX, PERCENT_DEFAULT),
            gamma: clamp(self.gamma, GAMMA_MIN, GAMMA_MAX, GAMMA_DEFAULT),
        }
    }

    pub fn is_neutral(&self) -> bool {
        let c = self.clamped();
        (c.brightness - PERCENT_DEFAULT).abs() < 1e-6
            && (c.contrast - PERCENT_DEFAULT).abs() < 1e-6
            && (c.gamma - GAMMA_DEFAULT).abs() < 1e-6
    }

    /// Valeur de sortie (0 à 1) pour une entrée `x` (0 à 1).
    pub fn apply(&self, x: f64) -> f64 {
        let c = self.clamped();
        // -1 à 1.
        let contrast = (c.contrast - PERCENT_DEFAULT) / PERCENT_DEFAULT;
        let brightness = (c.brightness - PERCENT_DEFAULT) / PERCENT_DEFAULT;

        // Contraste : début de la courbe déplacé (25,4 ou 32 points sur 255 au
        // maximum), sa hauteur réduite ou agrandie d'autant aux deux bouts.
        let mut offset = if contrast > 0.0 { -contrast * 25.4 / 255.0 } else { -contrast * 32.0 / 255.0 };
        let range = 1.0 + 2.0 * offset;
        // Luminosité : un cinquième de cette hauteur au maximum.
        offset += brightness * range / 5.0;
        let y = (x + offset) / range;
        if y <= 0.0 { 0.0 } else { y.powf(1.0 / c.gamma).min(1.0) }
    }
}

/// Réglage complet d'un écran : les trois canaux, liés ou séparés.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColorAdjust {
    pub red: Channel,
    pub green: Channel,
    pub blue: Channel,
}

impl ColorAdjust {
    /// Les trois canaux réglés pareil (curseur « Tous les canaux »).
    pub fn all(channel: Channel) -> Self {
        Self { red: channel, green: channel, blue: channel }
    }

    pub fn is_neutral(&self) -> bool {
        self.red.is_neutral() && self.green.is_neutral() && self.blue.is_neutral()
    }

    /// Courbes rouge, verte et bleue sur [`RAMP_SIZE`] points de 16 bits.
    pub fn ramps(&self) -> [Vec<u16>; 3] {
        [ramp(&self.red), ramp(&self.green), ramp(&self.blue)]
    }
}

fn ramp(channel: &Channel) -> Vec<u16> {
    (0..RAMP_SIZE)
        .map(|i| {
            let x = i as f64 / (RAMP_SIZE - 1) as f64;
            (channel.apply(x) * 65535.0).round() as u16
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel(brightness: f64, contrast: f64, gamma: f64) -> Channel {
        Channel { brightness, contrast, gamma }
    }

    #[test]
    fn default_is_identity() {
        let ramps = ColorAdjust::default().ramps();
        for curve in &ramps {
            assert_eq!(curve[0], 0);
            assert_eq!(curve[RAMP_SIZE - 1], 65535);
            for (i, value) in curve.iter().enumerate() {
                let expected = (i as f64 / 255.0 * 65535.0).round() as u16;
                assert_eq!(*value, expected, "point {i}");
            }
        }
        assert!(ColorAdjust::default().is_neutral());
    }

    #[test]
    fn gamma_above_one_brightens_midtones() {
        let brighter = channel(50.0, 50.0, 1.5);
        let darker = channel(50.0, 50.0, 0.7);
        assert!(brighter.apply(0.5) > 0.5);
        assert!(darker.apply(0.5) < 0.5);
        // Noir et blanc ne bougent pas.
        assert_eq!(brighter.apply(0.0), 0.0);
        assert_eq!(brighter.apply(1.0), 1.0);
        assert!((brighter.apply(0.5) - 0.5f64.powf(1.0 / 1.5)).abs() < 1e-12);
    }

    #[test]
    fn brightness_shifts_by_up_to_a_fifth() {
        assert!((channel(100.0, 50.0, 1.0).apply(0.0) - 0.2).abs() < 1e-12);
        assert!((channel(0.0, 50.0, 1.0).apply(1.0) - 0.8).abs() < 1e-12);
        assert!((channel(75.0, 50.0, 1.0).apply(0.5) - 0.6).abs() < 1e-12);
    }

    #[test]
    fn contrast_turns_around_mid_grey() {
        let strong = channel(50.0, 100.0, 1.0);
        let weak = channel(50.0, 0.0, 1.0);
        assert!((strong.apply(0.5) - 0.5).abs() < 1e-12);
        assert!((weak.apply(0.5) - 0.5).abs() < 1e-12);
        // Au maximum, le noir rogné jusqu'à 25,4/255 ; au minimum, relevé à 32/319.
        assert_eq!(strong.apply(25.0 / 255.0), 0.0);
        assert!(strong.apply(26.0 / 255.0) > 0.0);
        assert!((weak.apply(0.0) - 32.0 / 319.0).abs() < 1e-12);
        assert!((weak.apply(1.0) - 287.0 / 319.0).abs() < 1e-12);
    }

    /// Réglages d'un guide Arc Raiders du panneau NVIDIA : tous les canaux à
    /// 52 % / 47 % / 1,47, puis le bleu seul à 62 % / 72 % / 1,65. Sous
    /// Windows, l'image s'éclaircit sans teinte : les ombres gardent leur bleu.
    #[test]
    fn guide_settings_brighten_without_a_yellow_cast() {
        let all = channel(52.0, 47.0, 1.47);
        let blue = channel(62.0, 72.0, 1.65);
        // Valeurs de la rampe de Windows (DisplayGammaRamp), à 0,001 près.
        for (x, want_all, want_blue) in [(0.1, 0.228, 0.262), (0.2, 0.349, 0.399), (0.5, 0.631, 0.695)] {
            assert!((all.apply(x) - want_all).abs() < 1e-3, "tous {x}");
            assert!((blue.apply(x) - want_blue).abs() < 1e-3, "bleu {x}");
        }
        // Ombres plus claires, et le bleu n'y tombe jamais loin sous les autres
        // (dès 2 % : sous le noir quasi pur, l'écart ne se voit pas).
        for i in 2..=20 {
            let x = i as f64 / 100.0;
            assert!(all.apply(x) > x);
            assert!(blue.apply(x) > 0.9 * all.apply(x), "ombre {x}");
        }
    }

    #[test]
    fn curves_never_go_down() {
        for c in [channel(0.0, 100.0, 0.3), channel(100.0, 0.0, 2.8), channel(37.0, 81.0, 1.2)] {
            let curve = ramp(&c);
            assert!(curve.windows(2).all(|pair| pair[0] <= pair[1]), "{c:?}");
        }
    }

    #[test]
    fn out_of_range_values_are_clamped() {
        let wild = channel(500.0, -20.0, f64::NAN).clamped();
        assert_eq!(wild, channel(100.0, 0.0, 1.0));
        assert_eq!(channel(50.0, 50.0, 9.0).clamped().gamma, GAMMA_MAX);
    }

    #[test]
    fn channels_are_independent() {
        let adjust = ColorAdjust { blue: channel(40.0, 50.0, 1.0), ..Default::default() };
        let [red, green, blue] = adjust.ramps();
        assert_eq!(red, green);
        assert!(blue[255] < red[255]);
        assert!(!adjust.is_neutral());
    }
}
