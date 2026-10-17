//! Courbes de couleur d'un écran : luminosité, contraste et gamma, comme dans
//! « Régler les paramètres de couleur du bureau » du panneau NVIDIA.
//!
//! Les valeurs se règlent comme sous Windows (luminosité et contraste à 50 %,
//! gamma à 1,00 par défaut), pour que chacun retrouve ses réglages. Le calcul
//! suit celui de nvidia-settings sous X11 : contraste autour du gris moyen,
//! luminosité en décalage, puis gamma en puissance. Luminosité et contraste
//! extrêmes déplacent d'un tiers de la plage, comme lui.

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
        // -1 à 1, puis la part de la plage déplacée (un tiers au maximum).
        let contrast = (c.contrast - PERCENT_DEFAULT) / PERCENT_DEFAULT / 3.0;
        let brightness = (c.brightness - PERCENT_DEFAULT) / PERCENT_DEFAULT / 3.0;

        // Contraste : pente autour du gris moyen. Plus fort, la pente monte
        // jusqu'à 3 (0,5 / (0,5 − 1/3)) ; plus faible, elle descend à 1/3.
        let slope = if contrast > 0.0 { 0.5 / (0.5 - contrast) } else { (0.5 + contrast) / 0.5 };
        let y = ((x - 0.5) * slope + 0.5 + brightness).clamp(0.0, 1.0);
        y.powf(1.0 / c.gamma)
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
    fn brightness_shifts_by_up_to_a_third() {
        assert!((channel(100.0, 50.0, 1.0).apply(0.0) - 1.0 / 3.0).abs() < 1e-12);
        assert!((channel(0.0, 50.0, 1.0).apply(1.0) - 2.0 / 3.0).abs() < 1e-12);
        assert!((channel(75.0, 50.0, 1.0).apply(0.5) - (0.5 + 1.0 / 6.0)).abs() < 1e-12);
    }

    #[test]
    fn contrast_turns_around_mid_grey() {
        let strong = channel(50.0, 100.0, 1.0);
        let weak = channel(50.0, 0.0, 1.0);
        assert!((strong.apply(0.5) - 0.5).abs() < 1e-12);
        assert!((weak.apply(0.5) - 0.5).abs() < 1e-12);
        // Pente 3 au maximum, 1/3 au minimum.
        assert!((strong.apply(0.6) - 0.8).abs() < 1e-9);
        assert!((weak.apply(1.0) - (0.5 + 0.5 / 3.0)).abs() < 1e-9);
        assert_eq!(strong.apply(0.9), 1.0);
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
