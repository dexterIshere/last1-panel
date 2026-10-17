//! Hyprland : écrans lus par `hyprctl -j monitors all`, réglés par des règles
//! `monitor = …` écrites dans ~/.config/hypr/monitors.conf (que le bureau
//! Signal de go-to-linux et nwg-displays utilisent déjà) et appliquées aussitôt
//! par `hyprctl keyword monitor`.
//!
//! Tant que monitors.conf commence par « # gtl:auto », go-to-linux y remet la
//! définition native et la fréquence maximale à chaque connexion : notre
//! première écriture remplace cet en-tête, le réglage de l'utilisateur gagne.

use super::model::{self, Mode, Output, OutputChange, Position, Rotation, Vrr};
use super::{DisplayBackend, Features, ProfileState};
use crate::color::icc::Transfer;
use crate::error::{CoreError, CoreResult};
use crate::exec::Exec;
use crate::i18n::t;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const PROGRAM: &str = "hyprctl";
const GTL_AUTO: &str = "# gtl:auto";
const HEADER: [&str; 3] = [
    "# Écrans réglés avec Last1 Panel (définition, fréquence, échelle, couleurs).",
    "# Vous pouvez modifier ce fichier : Last1 Panel ne change que les lignes des écrans",
    "# que vous réglez dans sa fenêtre.",
];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Monitor {
    name: String,
    #[serde(default)]
    model: String,
    width: u32,
    height: u32,
    refresh_rate: f64,
    x: i32,
    y: i32,
    scale: f64,
    #[serde(default)]
    transform: u32,
    #[serde(default)]
    disabled: bool,
    #[serde(default)]
    current_format: String,
    #[serde(default)]
    available_modes: Vec<String>,
}

/// « 2560x1440@165.00Hz » → mode.
fn parse_mode(text: &str) -> Option<Mode> {
    let (size, rate) = text.trim().trim_end_matches("Hz").split_once('@')?;
    let (width, height) = size.split_once('x')?;
    Some(Mode::new(width.parse().ok()?, height.parse().ok()?, rate.parse().ok()?))
}

/// Règle `monitor = nom, mode, position, échelle, clé, valeur…` de hyprlang.
#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub name: String,
    pub mode: String,
    pub position: String,
    pub scale: String,
    /// Arguments suivants, par paires (transform, bitdepth, cm, vrr, icc…).
    pub extras: Vec<(String, String)>,
    pub disabled: bool,
}

impl Rule {
    /// Lit une ligne `monitor = …`. Les formes spéciales (`addreserved`,
    /// `transform` seul) ne sont pas des règles complètes : laissées telles quelles.
    pub fn parse(line: &str) -> Option<Rule> {
        let line = line.split('#').next()?.trim();
        let (key, value) = line.split_once('=')?;
        if key.trim() != "monitor" {
            return None;
        }
        let args: Vec<String> = value.split(',').map(|arg| arg.trim().to_string()).collect();
        let name = args.first()?.clone();
        match args.get(1).map(String::as_str) {
            Some("disable" | "disabled") => {
                return Some(Rule {
                    name,
                    mode: String::new(),
                    position: String::new(),
                    scale: String::new(),
                    extras: Vec::new(),
                    disabled: true,
                });
            }
            Some("addreserved" | "transform") | None => return None,
            _ => {}
        }
        if args.len() < 4 {
            return None;
        }
        let extras =
            args[4..].chunks(2).filter(|pair| pair.len() == 2).map(|pair| (pair[0].clone(), pair[1].clone())).collect();
        Some(Rule {
            name,
            mode: args[1].clone(),
            position: args[2].clone(),
            scale: args[3].clone(),
            extras,
            disabled: false,
        })
    }

    pub fn extra(&self, key: &str) -> Option<&str> {
        self.extras.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    pub fn set_extra(&mut self, key: &str, value: Option<String>) {
        match value {
            Some(value) => match self.extras.iter_mut().find(|(k, _)| k == key) {
                Some(slot) => slot.1 = value,
                None => self.extras.push((key.to_string(), value)),
            },
            None => self.extras.retain(|(k, _)| k != key),
        }
    }

    /// Arguments de la règle, sans « monitor = » (pour `hyprctl keyword monitor`).
    pub fn arguments(&self) -> String {
        if self.disabled {
            return format!("{}, disable", self.name);
        }
        let mut parts = vec![self.name.clone(), self.mode.clone(), self.position.clone(), self.scale.clone()];
        for (key, value) in &self.extras {
            parts.push(key.clone());
            parts.push(value.clone());
        }
        parts.join(", ")
    }

    pub fn line(&self) -> String {
        format!("monitor = {}", self.arguments())
    }
}

/// monitors.conf : les règles, et les autres lignes gardées à leur place.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MonitorsFile {
    lines: Vec<String>,
}

impl MonitorsFile {
    pub fn parse(text: &str) -> Self {
        Self { lines: text.lines().map(str::to_string).collect() }
    }

    pub fn rule(&self, name: &str) -> Option<Rule> {
        // Hyprland garde la dernière règle d'un même nom.
        self.lines.iter().filter_map(|line| Rule::parse(line)).rfind(|rule| rule.name == name)
    }

    pub fn is_gtl_auto(&self) -> bool {
        self.lines.first().is_some_and(|line| line.trim() == GTL_AUTO)
    }

    /// Remplace la règle de cet écran (ou l'ajoute à la fin).
    pub fn set_rule(&mut self, rule: &Rule) {
        if self.is_gtl_auto() {
            // En-tête de go-to-linux (« # gtl:auto » et ses deux lignes
            // d'explication) remplacé par le nôtre.
            let explanation = self.lines.iter().skip(1).take_while(|line| line.starts_with('#')).count();
            self.lines.splice(0..1 + explanation, HEADER.iter().map(|line| line.to_string()));
        } else if self.lines.is_empty() {
            self.lines.extend(HEADER.iter().map(|line| line.to_string()));
        }
        let mut replaced = false;
        self.lines.retain_mut(|line| match Rule::parse(line) {
            Some(existing) if existing.name == rule.name => {
                if replaced {
                    false
                } else {
                    *line = rule.line();
                    replaced = true;
                    true
                }
            }
            _ => true,
        });
        if !replaced {
            self.lines.push(rule.line());
        }
    }

    pub fn text(&self) -> String {
        let mut text = self.lines.join("\n");
        text.push('\n');
        text
    }
}

fn format_number(value: f64, decimals: usize) -> String {
    let text = format!("{value:.decimals$}");
    if text.contains('.') { text.trim_end_matches('0').trim_end_matches('.').to_string() } else { text }
}

pub struct Hyprland {
    exec: Arc<dyn Exec>,
    sys: PathBuf,
    /// ~/.config/hypr
    config_dir: PathBuf,
}

impl Hyprland {
    pub fn new(exec: Arc<dyn Exec>, sys: PathBuf, config_dir: PathBuf) -> Self {
        Self { exec, sys, config_dir }
    }

    fn monitors_conf(&self) -> PathBuf {
        self.config_dir.join("monitors.conf")
    }

    fn file(&self) -> MonitorsFile {
        MonitorsFile::parse(&fs::read_to_string(self.monitors_conf()).unwrap_or_default())
    }

    /// Configuration Lua (par défaut depuis Hyprland 0.55) : hyprland.conf est
    /// alors ignoré, et `hyprctl keyword` refusé.
    fn lua(&self) -> bool {
        self.config_dir.join("hyprland.lua").exists()
    }

    fn writable(&self) -> CoreResult<()> {
        if self.lua() {
            return Err(CoreError::message(t(
                "Votre Hyprland utilise une configuration Lua (hyprland.lua) : Last1 Panel ne sait régler les écrans qu'avec hyprland.conf pour l'instant.",
                "Your Hyprland uses a Lua configuration (hyprland.lua): Last1 Panel can only set screens with hyprland.conf for now.",
            )));
        }
        Ok(())
    }

    fn monitors(&self) -> CoreResult<Vec<Monitor>> {
        let output = self.exec.run_ok(PROGRAM, &["-j".into(), "monitors".into(), "all".into()])?;
        serde_json::from_str(&output.stdout).map_err(|error| CoreError::Unreadable(PROGRAM.into(), error.to_string()))
    }

    /// Fréquence variable par défaut (`misc:vrr`), pour les écrans sans règle `vrr`.
    fn default_vrr(&self) -> u32 {
        let Ok(output) = self.exec.run(PROGRAM, &["-j".into(), "getoption".into(), "misc:vrr".into()]) else {
            return 0;
        };
        serde_json::from_str::<serde_json::Value>(&output.stdout)
            .ok()
            .and_then(|value| value.get("int")?.as_u64())
            .unwrap_or(0) as u32
    }

    fn to_output(&self, monitor: &Monitor, file: &MonitorsFile, default_vrr: u32) -> Output {
        let rule = file.rule(&monitor.name);
        let extra = |key: &str| rule.as_ref().and_then(|rule| rule.extra(key).map(str::to_string));
        let edid = crate::system::edid::for_connector(&self.sys, &monitor.name);
        let mut modes: Vec<Mode> = monitor.available_modes.iter().filter_map(|mode| parse_mode(mode)).collect();
        model::sort_modes(&mut modes);
        let current = Mode::new(monitor.width, monitor.height, monitor.refresh_rate);
        let current_mode = model::closest(&modes, |m| m, &current).cloned().or(Some(current));
        let vrr = extra("vrr").and_then(|v| v.parse().ok()).unwrap_or(default_vrr);
        let cm = extra("cm").unwrap_or_default();
        Output {
            name: monitor.name.clone(),
            label: edid
                .as_ref()
                .map(|m| m.label())
                .unwrap_or_else(|| if monitor.model.is_empty() { monitor.name.clone() } else { monitor.model.clone() }),
            enabled: !monitor.disabled,
            primary: false,
            preferred_mode: modes.first().cloned(),
            modes,
            current_mode,
            position: Position { x: monitor.x, y: monitor.y },
            scale: monitor.scale,
            rotation: match monitor.transform {
                1 => Rotation::Left,
                2 => Rotation::Inverted,
                3 => Rotation::Right,
                _ => Rotation::Normal,
            },
            vrr: Some(match vrr {
                0 => Vrr::Never,
                1 => Vrr::Always,
                _ => Vrr::Fullscreen,
            }),
            hdr: edid.filter(|m| m.hdr).map(|_| cm == "hdr" || cm == "hdredid"),
            bit_depth: Some(
                if extra("bitdepth").as_deref() == Some("10") || monitor.current_format.contains("2101010") {
                    10
                } else {
                    8
                },
            ),
            icc_profile: Some(extra("icc").unwrap_or_default()),
        }
    }

    /// Règle complète d'un écran après changement : l'état actuel, les
    /// arguments déjà écrits par l'utilisateur, puis nos changements.
    fn rule_for(&self, output: &Output, existing: Option<Rule>) -> Rule {
        let mut rule = existing.filter(|rule| !rule.disabled).unwrap_or(Rule {
            name: output.name.clone(),
            mode: String::new(),
            position: String::new(),
            scale: String::new(),
            extras: Vec::new(),
            disabled: false,
        });
        if !output.enabled {
            return Rule { disabled: true, ..rule };
        }
        if let Some(mode) = &output.current_mode {
            rule.mode = format!("{}x{}@{}", mode.width, mode.height, format_number(mode.refresh_hz(), 3));
        }
        rule.position = format!("{}x{}", output.position.x, output.position.y);
        rule.scale = format_number(output.scale, 4);
        let transform = match output.rotation {
            Rotation::Normal => None,
            Rotation::Left => Some("1"),
            Rotation::Inverted => Some("2"),
            Rotation::Right => Some("3"),
        };
        rule.set_extra("transform", transform.map(str::to_string));
        if let Some(vrr) = output.vrr {
            let value = match vrr {
                Vrr::Never => "0",
                Vrr::Always => "1",
                Vrr::Fullscreen => "2",
            };
            rule.set_extra("vrr", Some(value.into()));
        }
        if let Some(bits) = output.bit_depth {
            rule.set_extra("bitdepth", (bits == 10).then(|| "10".into()));
        }
        match output.hdr {
            Some(true) => {
                // Le HDR ne va pas avec un profil ICC (Hyprland l'ignore alors).
                rule.set_extra("cm", Some("hdr".into()));
                rule.set_extra("icc", None);
            }
            Some(false) if matches!(rule.extra("cm"), Some("hdr" | "hdredid")) => rule.set_extra("cm", None),
            _ => {}
        }
        rule
    }

    fn keyword(&self, rule: &Rule) -> CoreResult<()> {
        let output = self.exec.run_ok(PROGRAM, &["keyword".into(), "monitor".into(), rule.arguments()])?;
        let answer = output.stdout.trim();
        if answer == "ok" || answer.is_empty() {
            Ok(())
        } else {
            Err(CoreError::ToolFailed { program: PROGRAM.into(), detail: answer.to_string() })
        }
    }

    fn save(&self, file: &MonitorsFile) -> CoreResult<()> {
        write_atomic(&self.monitors_conf(), &file.text())
    }
}

fn write_atomic(path: &Path, text: &str) -> CoreResult<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let temporary = path.with_extension("conf.last1-panel.new");
    fs::write(&temporary, text)?;
    fs::rename(&temporary, path)?;
    Ok(())
}

impl DisplayBackend for Hyprland {
    fn features(&self) -> Features {
        Features {
            primary: false,
            bit_depths: vec![8, 10],
            live_color: false,
            icc_transfer: Transfer::Srgb,
            read_only: self.lua().then(|| t("configuration Lua (hyprland.lua)", "Lua configuration (hyprland.lua)")),
        }
    }

    fn outputs(&self) -> CoreResult<Vec<Output>> {
        let file = self.file();
        let default_vrr = self.default_vrr();
        Ok(self.monitors()?.iter().map(|monitor| self.to_output(monitor, &file, default_vrr)).collect())
    }

    fn apply(&self, changes: &[OutputChange]) -> CoreResult<()> {
        self.writable()?;
        let current = self.outputs()?;
        let mut file = self.file();
        let mut rules = Vec::new();
        for change in changes {
            let Some(output) = current.iter().find(|o| o.name == change.name) else { continue };
            let rule = self.rule_for(&change.applied_to(output), file.rule(&change.name));
            file.set_rule(&rule);
            rules.push(rule);
        }
        self.save(&file)?;
        rules.iter().try_for_each(|rule| self.keyword(rule))
    }

    fn profile(&self, output: &str) -> CoreResult<ProfileState> {
        Ok(ProfileState {
            icc: self.file().rule(output).and_then(|rule| rule.extra("icc").map(str::to_string)),
            source: None,
        })
    }

    fn set_profile(&self, output: &str, state: &ProfileState) -> CoreResult<()> {
        self.writable()?;
        let current = self.outputs()?;
        let screen = current
            .iter()
            .find(|o| o.name == output)
            .ok_or_else(|| CoreError::message(format!("{output} : {}", t("écran introuvable", "screen not found"))))?;
        let mut file = self.file();
        let mut rule = self.rule_for(screen, file.rule(output));
        rule.set_extra("icc", state.icc.clone().filter(|path| !path.is_empty()));
        file.set_rule(&rule);
        self.save(&file)?;
        self.keyword(&rule)?;
        // La courbe VCGT n'atteint la table gamma de la carte qu'en réappliquant
        // l'écran entier : un changement de profil seul ne le fait pas.
        self.exec.run_ok(PROGRAM, &["dispatch".into(), "forcerendererreload".into()])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::{FakeExec, Output as Out};

    const MONITORS: &str = include_str!("../../tests/fixtures/hyprctl-monitors.json");

    fn setup(conf: Option<&str>) -> (tempfile::TempDir, Arc<FakeExec>, Hyprland) {
        let dir = tempfile::tempdir().unwrap();
        if let Some(conf) = conf {
            fs::write(dir.path().join("monitors.conf"), conf).unwrap();
        }
        let fake = Arc::new(
            FakeExec::new()
                .on_args(
                    PROGRAM,
                    &["-j", "monitors", "all"],
                    Out { status: 0, stdout: MONITORS.into(), stderr: String::new() },
                )
                .on_args(
                    PROGRAM,
                    &["-j", "getoption", "misc:vrr"],
                    Out {
                        status: 0,
                        stdout: r#"{"option":"misc:vrr","int":2,"set":true}"#.into(),
                        stderr: String::new(),
                    },
                )
                .on(PROGRAM, "ok"),
        );
        let hypr = Hyprland::new(fake.clone(), PathBuf::from("/nonexistent"), dir.path().to_path_buf());
        (dir, fake, hypr)
    }

    const GTL_AUTO_FILE: &str = "# gtl:auto\n# Écrans réglés par go-to-linux à chaque connexion (définition native, fréquence\n# maximale). Pour garder vos propres réglages : nwg-displays, ou supprimez la 1re ligne.\nmonitor = DP-1, 2560x1440@165.0, auto, auto\nmonitor = HDMI-A-1, 3840x2160@143.99, auto, auto\n";

    #[test]
    fn rules_round_trip() {
        let rule = Rule::parse("monitor = DP-1, 2560x1440@165, 0x0, 1, bitdepth, 10, vrr, 2 # commentaire").unwrap();
        assert_eq!(rule.name, "DP-1");
        assert_eq!(rule.mode, "2560x1440@165");
        assert_eq!(rule.extra("bitdepth"), Some("10"));
        assert_eq!(rule.extra("vrr"), Some("2"));
        assert_eq!(rule.line(), "monitor = DP-1, 2560x1440@165, 0x0, 1, bitdepth, 10, vrr, 2");
        assert!(Rule::parse("monitor=HDMI-A-1,disable").unwrap().disabled);
        assert!(Rule::parse("monitor = DP-1, addreserved, 10, 0, 0, 0").is_none());
        assert!(Rule::parse("monitor = DP-1, transform, 1").is_none());
        assert!(Rule::parse("source = ~/.config/hypr/monitors.conf").is_none());
        assert_eq!(Rule::parse("monitor = , preferred, auto, auto").unwrap().name, "");
    }

    #[test]
    fn reads_monitors_with_their_rules() {
        let conf = "monitor = HDMI-A-1, 3840x2160@60, 2560x0, 1.5, cm, hdr, vrr, 1\n";
        let (_dir, _fake, hypr) = setup(Some(conf));
        let outputs = hypr.outputs().unwrap();
        assert_eq!(outputs.len(), 2);
        let dp = &outputs[0];
        assert_eq!(dp.name, "DP-1");
        assert_eq!(dp.label, "Q27G2G3R3B");
        assert_eq!(dp.current_mode, Some(Mode::new(2560, 1440, 165.0)));
        assert_eq!(dp.modes.len(), 4);
        assert_eq!(dp.vrr, Some(Vrr::Fullscreen), "misc:vrr = 2 sans règle");
        assert_eq!(dp.bit_depth, Some(8));
        assert_eq!(dp.hdr, None, "pas d'EDID HDR");
        let tv = &outputs[1];
        assert_eq!(tv.current_mode, Some(Mode::new(3840, 2160, 60.0)), "59,997 Hz affiché 60.00Hz");
        assert_eq!(tv.vrr, Some(Vrr::Always));
        assert_eq!(tv.bit_depth, Some(10), "format XRGB2101010");
        assert_eq!(tv.scale, 1.5);
    }

    #[test]
    fn first_change_replaces_the_gtl_auto_header() {
        let (dir, fake, hypr) = setup(Some(GTL_AUTO_FILE));
        let change = OutputChange {
            name: "DP-1".into(),
            mode: Some(Mode::new(2560, 1440, 143.91)),
            vrr: Some(Vrr::Fullscreen),
            bit_depth: Some(10),
            ..Default::default()
        };
        hypr.apply(&[change]).unwrap();
        let text = fs::read_to_string(dir.path().join("monitors.conf")).unwrap();
        assert!(!text.starts_with(GTL_AUTO), "go-to-linux ne remet plus ses réglages");
        assert!(text.starts_with(HEADER[0]));
        assert!(text.contains("monitor = DP-1, 2560x1440@143.91, 0x0, 1, vrr, 2, bitdepth, 10\n"), "{text}");
        assert!(
            text.contains("monitor = HDMI-A-1, 3840x2160@143.99, auto, auto\n"),
            "les autres écrans ne bougent pas"
        );
        assert_eq!(text.matches("monitor =").count(), 2);
        let keyword = fake.calls().into_iter().find(|(_, args)| args[0] == "keyword").unwrap();
        assert_eq!(keyword.1, ["keyword", "monitor", "DP-1, 2560x1440@143.91, 0x0, 1, vrr, 2, bitdepth, 10"]);
    }

    #[test]
    fn user_arguments_are_kept() {
        let conf = "# mes écrans\nmonitor = DP-1, 2560x1440@165, 0x0, 1, sdrbrightness, 1.2, mirror, eDP-1\n";
        let (dir, _fake, hypr) = setup(Some(conf));
        hypr.apply(&[OutputChange { name: "DP-1".into(), rotation: Some(Rotation::Left), ..Default::default() }])
            .unwrap();
        let text = fs::read_to_string(dir.path().join("monitors.conf")).unwrap();
        assert!(text.starts_with("# mes écrans\n"));
        assert!(
            text.contains(
                "monitor = DP-1, 2560x1440@165, 0x0, 1, sdrbrightness, 1.2, mirror, eDP-1, transform, 1, vrr, 2\n"
            ),
            "{text}"
        );
    }

    #[test]
    fn icc_profile_is_written_and_reloaded() {
        let (dir, fake, hypr) = setup(None);
        let icc = "/home/j/.local/share/icc/last1-panel-DP-1.icc";
        hypr.set_profile("DP-1", &ProfileState { icc: Some(icc.into()), source: None }).unwrap();
        let text = fs::read_to_string(dir.path().join("monitors.conf")).unwrap();
        assert!(text.contains(&format!("monitor = DP-1, 2560x1440@165, 0x0, 1, vrr, 2, icc, {icc}\n")), "{text}");
        assert_eq!(hypr.profile("DP-1").unwrap().icc.as_deref(), Some(icc));
        let calls = fake.calls();
        assert_eq!(calls.last().unwrap().1, ["dispatch", "forcerendererreload"]);

        hypr.set_profile("DP-1", &ProfileState::default()).unwrap();
        let text = fs::read_to_string(dir.path().join("monitors.conf")).unwrap();
        assert!(!text.contains("icc"), "{text}");
    }

    #[test]
    fn hdr_removes_the_icc_profile() {
        let conf = "monitor = DP-1, 2560x1440@165, 0x0, 1, icc, /p.icc\n";
        let (dir, _fake, hypr) = setup(Some(conf));
        hypr.apply(&[OutputChange { name: "DP-1".into(), hdr: Some(true), ..Default::default() }]).unwrap();
        let text = fs::read_to_string(dir.path().join("monitors.conf")).unwrap();
        assert!(text.contains("cm, hdr") && !text.contains("icc"), "{text}");
    }

    #[test]
    fn lua_configuration_is_read_only() {
        let (dir, _fake, hypr) = setup(None);
        fs::write(dir.path().join("hyprland.lua"), "hl.monitor({})").unwrap();
        assert!(hypr.features().read_only.is_some());
        assert!(hypr.outputs().is_ok());
        assert!(hypr.apply(&[OutputChange { name: "DP-1".into(), scale: Some(1.25), ..Default::default() }]).is_err());
        assert!(!dir.path().join("monitors.conf").exists());
    }

    #[test]
    fn hyprctl_errors_are_reported() {
        let dir = tempfile::tempdir().unwrap();
        let fake = Arc::new(
            FakeExec::new()
                .on_args(
                    PROGRAM,
                    &["-j", "monitors", "all"],
                    Out { status: 0, stdout: MONITORS.into(), stderr: String::new() },
                )
                .on(PROGRAM, "invalid syntax at \"bitdepht\""),
        );
        let hypr = Hyprland::new(fake, PathBuf::from("/nonexistent"), dir.path().to_path_buf());
        let error =
            hypr.apply(&[OutputChange { name: "DP-1".into(), scale: Some(1.25), ..Default::default() }]).unwrap_err();
        assert!(error.to_string().contains("invalid syntax"));
    }
}
