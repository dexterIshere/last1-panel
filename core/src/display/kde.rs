//! Plasma : écrans lus et réglés par kscreen-doctor (paquet libkscreen).
//!
//! `kscreen-doctor -j` donne l'état en JSON ; les réglages passent en une
//! seule commande (`output.DP-2.mode.4 output.DP-2.scale.1.5 …`), que KWin
//! applique d'un bloc et garde (~/.config/kwinoutputconfig.json).
//! kscreen-doctor sort toujours avec le code 0 : un refus de KWin se lit sur
//! sa sortie (« applying config failed! »).

use super::model::{self, Mode, Output, OutputChange, Position, Rotation, Vrr};
use super::{DisplayBackend, ProfileState};
use crate::error::{CoreError, CoreResult};
use crate::exec::Exec;
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Arc;

const PROGRAM: &str = "kscreen-doctor";

#[derive(Deserialize)]
struct Config {
    outputs: Vec<JsonOutput>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonOutput {
    name: String,
    #[serde(default = "yes")]
    enabled: bool,
    #[serde(default = "yes")]
    connected: bool,
    #[serde(default)]
    priority: u32,
    #[serde(default)]
    current_mode_id: String,
    #[serde(default)]
    preferred_modes: Vec<String>,
    #[serde(default)]
    modes: Vec<JsonMode>,
    #[serde(default)]
    pos: JsonPoint,
    #[serde(default = "one")]
    scale: f64,
    #[serde(default = "one_u")]
    rotation: u32,
    vrr_policy: Option<u32>,
    hdr: Option<bool>,
    max_bpc: Option<u32>,
    icc_profile_path: Option<String>,
}

fn yes() -> bool {
    true
}
fn one() -> f64 {
    1.0
}
fn one_u() -> u32 {
    1
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonMode {
    id: String,
    refresh_rate: f64,
    size: JsonSize,
}

#[derive(Deserialize, Default)]
struct JsonSize {
    width: u32,
    height: u32,
}

#[derive(Deserialize, Default)]
struct JsonPoint {
    x: i32,
    y: i32,
}

/// Identifiant kscreen-doctor de chaque mode d'une sortie (valable pour la
/// commande suivante seulement).
type ModeIds = Vec<(Mode, String)>;

/// Sorties décrites par `kscreen-doctor -j`, avec les identifiants des modes.
fn parse(json: &str) -> CoreResult<Vec<(Output, ModeIds)>> {
    let config: Config =
        serde_json::from_str(json).map_err(|error| CoreError::Unreadable(PROGRAM.into(), error.to_string()))?;
    Ok(config
        .outputs
        .into_iter()
        .filter(|output| output.connected)
        .map(|json| {
            let ids: ModeIds = json
                .modes
                .iter()
                .map(|mode| (Mode::new(mode.size.width, mode.size.height, mode.refresh_rate), mode.id.clone()))
                .collect();
            let find = |id: &str| ids.iter().find(|(_, mode_id)| mode_id == id).map(|(mode, _)| mode.clone());
            let mut modes: Vec<Mode> = ids.iter().map(|(mode, _)| mode.clone()).collect();
            model::sort_modes(&mut modes);
            let output = Output {
                label: json.name.clone(),
                enabled: json.enabled,
                primary: json.priority == 1,
                current_mode: find(&json.current_mode_id),
                preferred_mode: json.preferred_modes.first().and_then(|id| find(id)),
                modes,
                position: Position { x: json.pos.x, y: json.pos.y },
                scale: json.scale,
                rotation: match json.rotation {
                    2 => Rotation::Left,
                    4 => Rotation::Inverted,
                    8 => Rotation::Right,
                    _ => Rotation::Normal,
                },
                vrr: json.vrr_policy.map(|policy| match policy {
                    0 => Vrr::Never,
                    1 => Vrr::Always,
                    _ => Vrr::Fullscreen,
                }),
                hdr: json.hdr,
                bit_depth: json.max_bpc,
                icc_profile: json.icc_profile_path,
                name: json.name,
            };
            (output, ids)
        })
        .collect())
}

/// Arguments de kscreen-doctor pour ces changements. `ids` : identifiants des
/// modes lus juste avant (un mode inconnu est demandé par son nom).
fn arguments(changes: &[OutputChange], ids: &[(String, ModeIds)]) -> Vec<String> {
    let mut args = Vec::new();
    for change in changes {
        let prefix = format!("output.{}", change.name);
        let mut push = |what: String| args.push(format!("{prefix}.{what}"));
        match change.enabled {
            Some(true) => push("enable".into()),
            Some(false) => {
                push("disable".into());
                continue;
            }
            None => {}
        }
        if let Some(mode) = &change.mode {
            let known = ids
                .iter()
                .find(|(name, _)| *name == change.name)
                .and_then(|(_, modes)| model::closest(modes, |(m, _)| m, mode))
                .map(|(_, id)| id.clone());
            push(format!(
                "mode.{}",
                known.unwrap_or_else(|| format!("{}x{}@{}", mode.width, mode.height, mode.refresh_hz().round()))
            ));
        }
        if let Some(position) = change.position {
            push(format!("position.{},{}", position.x, position.y));
        }
        if let Some(scale) = change.scale {
            push(format!("scale.{}", format_scale(scale)));
        }
        if let Some(rotation) = change.rotation {
            push(format!(
                "rotation.{}",
                match rotation {
                    Rotation::Normal => "normal",
                    Rotation::Left => "left",
                    Rotation::Inverted => "inverted",
                    Rotation::Right => "right",
                }
            ));
        }
        if let Some(vrr) = change.vrr {
            push(format!(
                "vrrpolicy.{}",
                match vrr {
                    Vrr::Never => "never",
                    Vrr::Always => "always",
                    Vrr::Fullscreen => "automatic",
                }
            ));
        }
        if let Some(hdr) = change.hdr {
            push(format!("hdr.{}", if hdr { "enable" } else { "disable" }));
        }
        if let Some(bits) = change.bit_depth {
            push(if bits == 0 { "maxbpc.automatic".into() } else { format!("maxbpc.{bits}") });
        }
        if change.primary == Some(true) {
            push("primary".into());
        }
    }
    args
}

/// Échelle sans zéros inutiles (1.25, 1.5, 2).
fn format_scale(scale: f64) -> String {
    let text = format!("{:.4}", (scale * 120.0).round() / 120.0);
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Source du profil de couleur de chaque sortie, lue dans `kscreen-doctor -o`
/// (absente du JSON). La sortie porte des codes couleur ANSI.
fn parse_profile_sources(text: &str) -> Vec<(String, String)> {
    let plain = strip_ansi(text);
    let mut found = Vec::new();
    let mut current: Option<String> = None;
    for line in plain.lines() {
        if let Some(rest) = line.strip_prefix("Output: ") {
            current = rest.split_whitespace().nth(1).map(str::to_string);
        } else if let Some(source) = line.trim().strip_prefix("Color profile source: ")
            && let Some(name) = &current
        {
            found.push((name.clone(), source.trim().to_string()));
        }
    }
    found
}

fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Refus de KWin ou argument rejeté : kscreen-doctor ne le dit que par écrit.
fn check(output: &crate::exec::Output) -> CoreResult<()> {
    let refused = output.stdout.lines().find(|line| line.contains("applying config failed!"));
    let wrong = output.stderr.lines().find(|line| line.contains("Wrong input") || line.contains("not found"));
    match refused.or(wrong) {
        Some(line) => Err(CoreError::ToolFailed {
            program: PROGRAM.into(),
            detail: line.replace("applying config failed!", "").trim().to_string(),
        }),
        None if !output.success() => {
            Err(CoreError::ToolFailed { program: PROGRAM.into(), detail: output.stderr.clone() })
        }
        None => Ok(()),
    }
}

pub struct Kde {
    exec: Arc<dyn Exec>,
    sys: PathBuf,
}

impl Kde {
    pub fn new(exec: Arc<dyn Exec>, sys: PathBuf) -> Self {
        Self { exec, sys }
    }

    fn read(&self) -> CoreResult<Vec<(Output, ModeIds)>> {
        let output = self.exec.run_ok(PROGRAM, &["-j".into()])?;
        let mut outputs = parse(&output.stdout)?;
        for (output, _) in &mut outputs {
            if let Some(monitor) = crate::system::edid::for_connector(&self.sys, &output.name) {
                output.label = monitor.label();
            }
        }
        Ok(outputs)
    }

    fn run(&self, args: Vec<String>) -> CoreResult<()> {
        if args.is_empty() {
            return Ok(());
        }
        check(&self.exec.run(PROGRAM, &args)?)
    }
}

impl DisplayBackend for Kde {
    fn outputs(&self) -> CoreResult<Vec<Output>> {
        Ok(self.read()?.into_iter().map(|(output, _)| output).collect())
    }

    fn apply(&self, changes: &[OutputChange]) -> CoreResult<()> {
        let ids: Vec<(String, ModeIds)> = self.read()?.into_iter().map(|(output, ids)| (output.name, ids)).collect();
        self.run(arguments(changes, &ids))
    }

    fn profile(&self, output: &str) -> CoreResult<ProfileState> {
        let icc = self.outputs()?.into_iter().find(|o| o.name == output).and_then(|o| o.icc_profile);
        let text = self.exec.run_ok(PROGRAM, &["-o".into()])?.stdout;
        let source = parse_profile_sources(&text).into_iter().find(|(name, _)| name == output).map(|(_, s)| s);
        Ok(ProfileState { icc: icc.filter(|path| !path.is_empty()), source })
    }

    fn set_profile(&self, output: &str, state: &ProfileState) -> CoreResult<()> {
        let mut args = vec![format!("output.{output}.iccprofile.{}", state.icc.as_deref().unwrap_or(""))];
        if let Some(source) = &state.source {
            args.push(format!("output.{output}.colorProfileSource.{source}"));
        }
        self.run(args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::{FakeExec, Output as Out};

    const FIXTURE: &str = include_str!("../../tests/fixtures/kscreen-doctor.json");

    #[test]
    fn reads_outputs_from_json() {
        let outputs = parse(FIXTURE).unwrap();
        assert_eq!(outputs.len(), 2);
        let (laptop, _) = &outputs[0];
        assert_eq!(laptop.name, "eDP-1");
        assert!(laptop.primary);
        assert_eq!(laptop.current_mode, Some(Mode::new(2560, 1600, 165.0)));
        assert_eq!(laptop.scale, 1.25);
        assert_eq!(laptop.vrr, Some(Vrr::Fullscreen));
        assert_eq!(laptop.hdr, None, "pas de HDR sur cet écran");
        assert_eq!(laptop.icc_profile.as_deref(), Some(""));

        let (screen, ids) = &outputs[1];
        assert_eq!(screen.name, "DP-2");
        assert!(!screen.primary);
        assert_eq!(screen.current_mode, Some(Mode::new(3840, 2160, 143.981)));
        assert_eq!(screen.preferred_mode, Some(Mode::new(3840, 2160, 60.0)));
        assert_eq!(screen.position, Position { x: 2048, y: 0 });
        assert_eq!(screen.hdr, Some(false));
        assert_eq!(screen.bit_depth, Some(10));
        // 59,997 Hz et 60 Hz restent deux modes distincts.
        assert_eq!(screen.modes.iter().filter(|m| m.width == 3840 && m.refresh_hz().round() == 60.0).count(), 2);
        assert_eq!(screen.modes[0], Mode::new(3840, 2160, 143.981));
        assert_eq!(ids.len(), 4);
    }

    #[test]
    fn builds_one_command_for_all_changes() {
        let outputs = parse(FIXTURE).unwrap();
        let ids: Vec<_> = outputs.into_iter().map(|(o, ids)| (o.name, ids)).collect();
        let changes = vec![
            OutputChange {
                name: "DP-2".into(),
                mode: Some(Mode::new(3840, 2160, 60.0)),
                scale: Some(1.25),
                vrr: Some(Vrr::Always),
                hdr: Some(true),
                bit_depth: Some(10),
                primary: Some(true),
                ..Default::default()
            },
            OutputChange {
                name: "eDP-1".into(),
                position: Some(Position { x: 0, y: 360 }),
                rotation: Some(Rotation::Left),
                ..Default::default()
            },
        ];
        assert_eq!(
            arguments(&changes, &ids),
            [
                "output.DP-2.mode.7",
                "output.DP-2.scale.1.25",
                "output.DP-2.vrrpolicy.always",
                "output.DP-2.hdr.enable",
                "output.DP-2.maxbpc.10",
                "output.DP-2.primary",
                "output.eDP-1.position.0,360",
                "output.eDP-1.rotation.left",
            ]
        );
        let off =
            vec![OutputChange { name: "eDP-1".into(), enabled: Some(false), scale: Some(2.0), ..Default::default() }];
        assert_eq!(arguments(&off, &ids), ["output.eDP-1.disable"]);
    }

    #[test]
    fn scale_is_written_without_trailing_zeros() {
        assert_eq!(format_scale(1.0), "1");
        assert_eq!(format_scale(1.5), "1.5");
        assert_eq!(format_scale(1.25), "1.25");
        assert_eq!(format_scale(1.175), "1.175");
    }

    #[test]
    fn reads_profile_source_through_colour_codes() {
        let text = "\u{1b}[01;32mOutput: \u{1b}[0;0m1 eDP-1 a1b2\n\t\u{1b}[01;33mColor profile source: \u{1b}[0;0mEDID\n\
                    \u{1b}[01;32mOutput: \u{1b}[0;0m2 DP-2 c3d4\n\t\u{1b}[01;33mICC profile: \u{1b}[0;0mnone\n\t\u{1b}[01;33mColor profile source: \u{1b}[0;0msRGB\n";
        assert_eq!(
            parse_profile_sources(text),
            [("eDP-1".to_string(), "EDID".to_string()), ("DP-2".to_string(), "sRGB".to_string())]
        );
    }

    #[test]
    fn kwin_refusal_becomes_an_error() {
        let refused = Out {
            status: 0,
            stdout: "applying config failed! Failed to load the ICC profile\n".into(),
            stderr: String::new(),
        };
        let error = check(&refused).unwrap_err().to_string();
        assert!(error.contains("Failed to load the ICC profile"), "{error}");
        assert!(check(&Out::default()).is_ok());
        let wrong = Out {
            status: 0,
            stdout: String::new(),
            stderr: "kscreen.doctor: Wrong input: only allowed values…\n".into(),
        };
        assert!(check(&wrong).is_err());
    }

    #[test]
    fn sets_and_restores_profiles() {
        let fake = Arc::new(FakeExec::new().on("kscreen-doctor", FIXTURE));
        let kde = Kde::new(fake.clone(), PathBuf::from("/nonexistent"));
        kde.set_profile(
            "DP-2",
            &ProfileState {
                icc: Some("/home/j/.local/share/icc/last1-panel-DP-2.icc".into()),
                source: Some("ICC".into()),
            },
        )
        .unwrap();
        kde.set_profile("DP-2", &ProfileState { icc: None, source: Some("sRGB".into()) }).unwrap();
        let calls = fake.calls();
        assert_eq!(
            calls[0].1,
            [
                "output.DP-2.iccprofile./home/j/.local/share/icc/last1-panel-DP-2.icc",
                "output.DP-2.colorProfileSource.ICC"
            ]
        );
        assert_eq!(calls[1].1, ["output.DP-2.iccprofile.", "output.DP-2.colorProfileSource.sRGB"]);
    }
}
