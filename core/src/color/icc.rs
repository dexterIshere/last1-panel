//! Profils ICC d'écran portant nos courbes de couleur.
//!
//! Ni KWin ni Hyprland ne règlent luminosité, contraste et gamma par écran,
//! mais tous deux appliquent la courbe de calibration (balise VCGT) du profil
//! ICC choisi pour un écran. Le profil décrit un écran sRGB ordinaire (pas de
//! correction de gamut) et ne porte que nos courbes : c'est la méthode des
//! outils de calibration (DisplayCAL), sans sonde.

use crate::color::curve::ColorAdjust;
use crate::error::{CoreError, CoreResult};
use lcms2::{CIExyY, CIExyYTRIPLE, Locale, MLU, Profile, ProfileClassSignature, Tag, TagSignature, ToneCurve};

/// Fonction de transfert décrite par le profil : celle que le compositeur
/// suppose déjà pour un écran SDR, pour que le profil seul ne change rien.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transfer {
    /// sRGB par morceaux (IEC 61966-2-1).
    Srgb,
    /// Puissance 2,2.
    Gamma22,
}

/// Profil ICC (octets du fichier) : écran sRGB décrit par `transfer`, et nos
/// courbes `adjust` dans la balise VCGT.
pub fn build_profile(description: &str, adjust: &ColorAdjust, transfer: Transfer) -> CoreResult<Vec<u8>> {
    // Blanc D65 et primaires de sRGB (Rec. 709).
    let white = CIExyY { x: 0.3127, y: 0.3290, Y: 1.0 };
    let primaries = CIExyYTRIPLE {
        Red: CIExyY { x: 0.64, y: 0.33, Y: 1.0 },
        Green: CIExyY { x: 0.30, y: 0.60, Y: 1.0 },
        Blue: CIExyY { x: 0.15, y: 0.06, Y: 1.0 },
    };
    let trc = match transfer {
        // Courbe paramétrique de type 4 : Y = (aX + b)^g si X ≥ d, sinon cX.
        Transfer::Srgb => ToneCurve::new_parametric(4, &[2.4, 1.0 / 1.055, 0.055 / 1.055, 1.0 / 12.92, 0.04045]),
        Transfer::Gamma22 => Ok(ToneCurve::new(2.2)),
    }
    .map_err(|error| CoreError::message(format!("lcms2 : {error}")))?;
    let mut profile = Profile::new_rgb(&white, &primaries, &[&trc, &trc, &trc])
        .map_err(|error| CoreError::message(format!("lcms2 : {error}")))?;
    profile.set_device_class(ProfileClassSignature::DisplayClass);

    let mut text = MLU::new(1);
    text.set_text(description, Locale::none());
    profile.write_tag(TagSignature::ProfileDescriptionTag, Tag::MLU(&text));
    let mut copyright = MLU::new(1);
    copyright.set_text("Last1 Panel (GPL-3.0-or-later)", Locale::none());
    profile.write_tag(TagSignature::CopyrightTag, Tag::MLU(&copyright));

    let [red, green, blue] = adjust.ramps();
    let curves = [ToneCurve::new_tabulated(&red), ToneCurve::new_tabulated(&green), ToneCurve::new_tabulated(&blue)];
    if !profile.write_tag(TagSignature::VcgtTag, Tag::VcgtCurves([&curves[0], &curves[1], &curves[2]])) {
        return Err(CoreError::message("lcms2 : VCGT"));
    }
    profile.icc().map_err(|error| CoreError::message(format!("lcms2 : {error}")))
}

/// Courbes VCGT d'un profil, sur `points` points de 16 bits (tests, et
/// vérification d'un profil déjà posé).
pub fn read_vcgt(bytes: &[u8], points: usize) -> CoreResult<Option<[Vec<u16>; 3]>> {
    let profile = Profile::new_icc(bytes).map_err(|error| CoreError::message(format!("lcms2 : {error}")))?;
    let sample = |curve: &lcms2::ToneCurveRef| -> Vec<u16> {
        (0..points)
            .map(|i| {
                let x = i as f32 / (points - 1) as f32;
                (curve.eval(x).clamp(0.0, 1.0) * 65535.0).round() as u16
            })
            .collect()
    };
    Ok(match profile.read_tag(TagSignature::VcgtTag) {
        Tag::VcgtCurves([red, green, blue]) => Some([sample(red), sample(green), sample(blue)]),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::curve::Channel;

    fn close(a: &[u16], b: &[u16], tolerance: u16) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.abs_diff(*y) <= tolerance)
    }

    #[test]
    fn profile_is_a_display_profile_with_our_curves() {
        let adjust = ColorAdjust {
            red: Channel { brightness: 55.0, contrast: 60.0, gamma: 1.1 },
            green: Channel::default(),
            blue: Channel { brightness: 45.0, contrast: 50.0, gamma: 0.9 },
        };
        let bytes = build_profile("Last1 Panel DP-1", &adjust, Transfer::Gamma22).unwrap();
        assert_eq!(&bytes[36..40], b"acsp", "signature ICC");
        assert_eq!(&bytes[12..16], b"mntr", "profil d'écran");
        assert_eq!(&bytes[16..20], b"RGB ", "espace RVB");

        let [red, green, blue] = read_vcgt(&bytes, 256).unwrap().expect("balise VCGT");
        let [want_red, want_green, want_blue] = adjust.ramps();
        // Écart d'arrondi seulement (lcms2 réécrit la table sur 16 bits).
        assert!(close(&red, &want_red, 2));
        assert!(close(&green, &want_green, 2));
        assert!(close(&blue, &want_blue, 2));
    }

    #[test]
    fn profile_describes_srgb_primaries() {
        let bytes = build_profile("sRGB", &ColorAdjust::default(), Transfer::Srgb).unwrap();
        let profile = Profile::new_icc(&bytes).unwrap();
        match profile.read_tag(TagSignature::RedColorantTag) {
            // Primaire rouge de sRGB adaptée à D50 (valeurs du profil sRGB de référence).
            Tag::CIEXYZ(xyz) => {
                assert!((xyz.X - 0.4361).abs() < 0.002, "{}", xyz.X);
                assert!((xyz.Y - 0.2225).abs() < 0.002, "{}", xyz.Y);
            }
            _ => panic!("pas de primaire rouge"),
        }
    }

    #[test]
    fn neutral_profile_has_identity_curves() {
        let bytes = build_profile("neutre", &ColorAdjust::default(), Transfer::Srgb).unwrap();
        let [red, ..] = read_vcgt(&bytes, 256).unwrap().unwrap();
        let identity: Vec<u16> = (0..256).map(|i| (i as f64 / 255.0 * 65535.0).round() as u16).collect();
        assert!(close(&red, &identity, 2));
    }
}
