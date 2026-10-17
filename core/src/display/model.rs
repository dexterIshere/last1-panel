//! Écrans, vus de la même façon sous Plasma et sous Hyprland.

use serde::{Deserialize, Serialize};

/// Mode d'affichage : définition et fréquence.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mode {
    pub width: u32,
    pub height: u32,
    /// Fréquence en millièmes de hertz (143 981 pour 143,981 Hz).
    pub refresh_mhz: u32,
}

impl Mode {
    pub fn new(width: u32, height: u32, refresh_hz: f64) -> Self {
        Self { width, height, refresh_mhz: (refresh_hz * 1000.0).round() as u32 }
    }

    pub fn refresh_hz(&self) -> f64 {
        f64::from(self.refresh_mhz) / 1000.0
    }

    /// Même mode à l'arrondi près : une fréquence relue avec deux décimales
    /// (Hyprland) est reconnue.
    pub fn matches(&self, other: &Mode) -> bool {
        self.width == other.width && self.height == other.height && self.refresh_mhz.abs_diff(other.refresh_mhz) <= 5
    }
}

/// L'élément dont le mode est le plus proche de `target` (59,997 Hz et 60 Hz
/// restent distincts : le plus proche gagne).
pub fn closest<'a, T>(items: &'a [T], mode: impl Fn(&T) -> &Mode, target: &Mode) -> Option<&'a T> {
    items
        .iter()
        .filter(|item| mode(item).matches(target))
        .min_by_key(|item| mode(item).refresh_mhz.abs_diff(target.refresh_mhz))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Rotation {
    #[default]
    Normal,
    /// Portrait : quart de tour vers la gauche.
    Left,
    Inverted,
    /// Portrait retourné.
    Right,
}

impl Rotation {
    pub fn is_portrait(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }
}

/// Fréquence variable (G-SYNC, FreeSync).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Vrr {
    Never,
    Always,
    /// Seulement pour les applications en plein écran (les jeux).
    Fullscreen,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Output {
    /// Nom de la sortie : « DP-2 ».
    pub name: String,
    /// Nom de l'écran (EDID) : « AW2723DF ».
    pub label: String,
    pub enabled: bool,
    pub primary: bool,
    /// Modes disponibles, du plus grand au plus petit, fréquences décroissantes.
    pub modes: Vec<Mode>,
    pub current_mode: Option<Mode>,
    pub preferred_mode: Option<Mode>,
    pub position: Position,
    pub scale: f64,
    pub rotation: Rotation,
    /// Absent : l'écran (ou le bureau) ne gère pas la fréquence variable.
    pub vrr: Option<Vrr>,
    /// Absent : pas de HDR possible.
    pub hdr: Option<bool>,
    /// Bits par couleur (0 : automatique). Absent : non réglable.
    pub bit_depth: Option<u32>,
    /// Profil ICC actuel (vide : aucun). Absent : profils ICC non gérés.
    pub icc_profile: Option<String>,
}

impl Output {
    /// Taille occupée sur le bureau (pixels divisés par l'échelle, tournés).
    pub fn logical_size(&self) -> (i32, i32) {
        let (w, h) = self.current_mode.as_ref().map(|m| (m.width, m.height)).unwrap_or((0, 0));
        let (w, h) = if self.rotation.is_portrait() { (h, w) } else { (w, h) };
        let scale = if self.scale > 0.0 { self.scale } else { 1.0 };
        ((f64::from(w) / scale).round() as i32, (f64::from(h) / scale).round() as i32)
    }
}

/// Changement demandé pour une sortie (absent : inchangé).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OutputChange {
    pub name: String,
    pub enabled: Option<bool>,
    pub primary: Option<bool>,
    pub mode: Option<Mode>,
    pub position: Option<Position>,
    pub scale: Option<f64>,
    pub rotation: Option<Rotation>,
    pub vrr: Option<Vrr>,
    pub hdr: Option<bool>,
    pub bit_depth: Option<u32>,
}

impl OutputChange {
    /// L'état complet d'une sortie, pour revenir en arrière.
    pub fn restore(output: &Output) -> Self {
        Self {
            name: output.name.clone(),
            enabled: Some(output.enabled),
            primary: output.primary.then_some(true),
            mode: output.current_mode.clone(),
            position: Some(output.position),
            scale: Some(output.scale),
            rotation: Some(output.rotation),
            vrr: output.vrr,
            hdr: output.hdr,
            bit_depth: output.bit_depth,
        }
    }

    /// La sortie telle qu'elle sera après ce changement.
    pub fn applied_to(&self, output: &Output) -> Output {
        let mut next = output.clone();
        if let Some(enabled) = self.enabled {
            next.enabled = enabled;
        }
        if let Some(primary) = self.primary {
            next.primary = primary;
        }
        if let Some(mode) = &self.mode {
            next.current_mode = Some(mode.clone());
        }
        if let Some(position) = self.position {
            next.position = position;
        }
        if let Some(scale) = self.scale {
            next.scale = scale;
        }
        if let Some(rotation) = self.rotation {
            next.rotation = rotation;
        }
        if self.vrr.is_some() {
            next.vrr = self.vrr;
        }
        if self.hdr.is_some() {
            next.hdr = self.hdr;
        }
        if self.bit_depth.is_some() {
            next.bit_depth = self.bit_depth;
        }
        next
    }
}

/// Modes triés et sans doublon (plus grande définition, puis plus haute fréquence).
pub fn sort_modes(modes: &mut Vec<Mode>) {
    modes.sort_by(|a, b| {
        (u64::from(b.width) * u64::from(b.height), b.width, b.refresh_mhz).cmp(&(
            u64::from(a.width) * u64::from(a.height),
            a.width,
            a.refresh_mhz,
        ))
    });
    modes.dedup();
}

/// Positions après un changement de taille : un écran qui touchait le bord
/// droit (ou bas) d'un voisin, en face de lui, le touche encore, sans
/// chevauchement ni trou. Un écran déplacé à la main garde sa position.
pub fn relayout(before: &[Output], after: &mut [Output]) {
    fn overlap(a: (i32, i32), b: (i32, i32)) -> bool {
        a.0 < b.0 + b.1 && b.0 < a.0 + a.1
    }
    for horizontal in [true, false] {
        let mut order: Vec<usize> = (0..after.len()).filter(|&i| after[i].enabled).collect();
        order.sort_by_key(|&i| if horizontal { after[i].position.x } else { after[i].position.y });
        for i in order {
            let Some(old) = before.iter().find(|o| o.name == after[i].name) else { continue };
            let moved =
                if horizontal { after[i].position.x != old.position.x } else { after[i].position.y != old.position.y };
            if moved {
                continue;
            }
            let (ow, oh) = old.logical_size();
            let neighbour = before.iter().filter(|n| n.enabled && n.name != old.name).find(|n| {
                let (nw, nh) = n.logical_size();
                if horizontal {
                    n.position.x + nw == old.position.x && overlap((n.position.y, nh), (old.position.y, oh))
                } else {
                    n.position.y + nh == old.position.y && overlap((n.position.x, nw), (old.position.x, ow))
                }
            });
            let Some(next) = neighbour.and_then(|n| after.iter().find(|o| o.name == n.name)).cloned() else { continue };
            let (nw, nh) = next.logical_size();
            if horizontal {
                after[i].position.x = next.position.x + nw;
            } else {
                after[i].position.y = next.position.y + nh;
            }
        }
    }
}

/// Les changements à envoyer pour passer de `before` à `after` (sorties
/// modifiées seulement, positions comprises).
pub fn diff(before: &[Output], after: &[Output]) -> Vec<OutputChange> {
    after
        .iter()
        .filter_map(|next| {
            let old = before.iter().find(|o| o.name == next.name)?;
            let mut change = OutputChange { name: next.name.clone(), ..Default::default() };
            let mut any = false;
            macro_rules! field {
                ($field:ident, $value:expr) => {
                    if old.$field != next.$field {
                        change.$field = $value;
                        any = true;
                    }
                };
            }
            field!(enabled, Some(next.enabled));
            field!(primary, Some(next.primary));
            if old.current_mode != next.current_mode {
                change.mode = next.current_mode.clone();
                any = true;
            }
            field!(position, Some(next.position));
            field!(scale, Some(next.scale));
            field!(rotation, Some(next.rotation));
            field!(vrr, next.vrr);
            field!(hdr, next.hdr);
            field!(bit_depth, next.bit_depth);
            any.then_some(change)
        })
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn output(name: &str, x: i32, width: u32, height: u32, scale: f64) -> Output {
        let mode = Mode::new(width, height, 60.0);
        Output {
            name: name.into(),
            label: name.into(),
            enabled: true,
            primary: false,
            modes: vec![mode.clone()],
            current_mode: Some(mode.clone()),
            preferred_mode: Some(mode),
            position: Position { x, y: 0 },
            scale,
            rotation: Rotation::Normal,
            vrr: Some(Vrr::Fullscreen),
            hdr: None,
            bit_depth: Some(0),
            icc_profile: Some(String::new()),
        }
    }

    #[test]
    fn modes_are_sorted_biggest_and_fastest_first() {
        let mut modes = vec![
            Mode::new(1920, 1080, 60.0),
            Mode::new(2560, 1440, 59.951),
            Mode::new(2560, 1440, 165.0),
            Mode::new(1920, 1080, 60.0),
            Mode::new(1280, 1024, 75.0),
        ];
        sort_modes(&mut modes);
        assert_eq!(
            modes,
            [
                Mode::new(2560, 1440, 165.0),
                Mode::new(2560, 1440, 59.951),
                Mode::new(1920, 1080, 60.0),
                Mode::new(1280, 1024, 75.0)
            ]
        );
        assert!(Mode::new(2560, 1440, 143.981).matches(&Mode::new(2560, 1440, 143.98)));
        let both = [Mode::new(3840, 2160, 59.997), Mode::new(3840, 2160, 60.0)];
        assert_eq!(closest(&both, |m| m, &Mode::new(3840, 2160, 60.0)), Some(&both[1]));
        assert_eq!(closest(&both, |m| m, &Mode::new(3840, 2160, 59.997)), Some(&both[0]));
        assert_eq!(closest(&both, |m| m, &Mode::new(3840, 2160, 120.0)), None);
    }

    #[test]
    fn logical_size_follows_scale_and_rotation() {
        let mut screen = output("DP-1", 0, 3840, 2160, 1.5);
        assert_eq!(screen.logical_size(), (2560, 1440));
        screen.rotation = Rotation::Left;
        assert_eq!(screen.logical_size(), (1440, 2560));
    }

    #[test]
    fn neighbours_stay_side_by_side() {
        let before = vec![output("DP-1", 0, 2560, 1440, 1.0), output("HDMI-A-1", 2560, 1920, 1080, 1.0)];
        let mut after = before.clone();
        after[0].current_mode = Some(Mode::new(1920, 1080, 144.0));
        relayout(&before, &mut after);
        assert_eq!(after[1].position, Position { x: 1920, y: 0 });

        let changes = diff(&before, &after);
        assert_eq!(changes.len(), 2);
        assert_eq!(changes[0].mode, Some(Mode::new(1920, 1080, 144.0)));
        assert_eq!(changes[1].position, Some(Position { x: 1920, y: 0 }));
        assert_eq!(changes[1].mode, None);
    }

    /// Écran DP-1 (0,0), portable eDP-1 dessous (0,1440), HDMI-A-1 à droite de DP-1.
    fn three_screens() -> Vec<Output> {
        let mut laptop = output("eDP-1", 0, 2560, 1600, 1.0);
        laptop.position.y = 1440;
        vec![output("DP-1", 0, 2560, 1440, 1.0), laptop, output("HDMI-A-1", 2560, 1920, 1080, 1.0)]
    }

    #[test]
    fn only_facing_neighbours_follow() {
        // Portable plus étroit : HDMI-A-1 n'est pas en face de lui, il ne bouge pas.
        let before = three_screens();
        let mut after = before.clone();
        after[1].current_mode = Some(Mode::new(1920, 1200, 60.0));
        relayout(&before, &mut after);
        assert_eq!(after[2].position, Position { x: 2560, y: 0 });

        // DP-1 plus étroit : HDMI-A-1 le suit, le portable reste dessous et
        // DP-1, moins haut, laisse le portable remonter contre lui.
        let mut after = before.clone();
        after[0].current_mode = Some(Mode::new(1920, 1080, 60.0));
        relayout(&before, &mut after);
        assert_eq!(after[2].position, Position { x: 1920, y: 0 });
        assert_eq!(after[1].position, Position { x: 0, y: 1080 });
    }

    #[test]
    fn moved_screens_keep_their_position() {
        let before = three_screens();
        let mut after = before.clone();
        after[0].current_mode = Some(Mode::new(1920, 1080, 60.0));
        after[2].position = Position { x: 4000, y: 0 };
        relayout(&before, &mut after);
        assert_eq!(after[2].position, Position { x: 4000, y: 0 });
    }

    #[test]
    fn restore_brings_back_the_previous_state() {
        let before = output("DP-1", 0, 2560, 1440, 1.0);
        let mut changed = before.clone();
        changed.scale = 1.25;
        changed.vrr = Some(Vrr::Never);
        assert_eq!(OutputChange::restore(&before).applied_to(&changed), before);
        assert!(diff(std::slice::from_ref(&before), std::slice::from_ref(&before)).is_empty());
    }
}
