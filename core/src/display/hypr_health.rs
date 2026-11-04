//! Ce que Hyprland reproche à la configuration du joueur : réglages
//! dépréciés (« You are using 1 deprecated config option », à chaque
//! ouverture de session) et erreurs de lecture (barre rouge). Joint aux
//! rapports de performances pour voir sur tous les PC ce qui casse après
//! une mise à jour de Hyprland.

use crate::exec::Exec;
use serde::Serialize;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Health {
    /// `hyprctl version` : « 0.57.1 ».
    pub version: String,
    /// `hyprctl deprecated-config` : « option: explication ».
    pub deprecated: Vec<String>,
    /// `hyprctl configerrors` : « fichier:ligne: erreur ».
    #[serde(rename = "configErrors")]
    pub config_errors: Vec<String>,
}

/// Liste JSON de `hyprctl -j …` ; lignes vides retirées.
fn list(json: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(json)
        .unwrap_or_default()
        .into_iter()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect()
}

/// « Hyprland 0.57.1 built from branch … » → « 0.57.1 ».
fn version(text: &str) -> String {
    let json: serde_json::Value = serde_json::from_str(text).unwrap_or_default();
    let tag = json["tag"].as_str().or_else(|| json["version"].as_str()).unwrap_or_default();
    tag.trim_start_matches('v').split('-').next().unwrap_or_default().to_string()
}

/// L'état de Hyprland en cours ; `None` hors Hyprland.
pub fn read(exec: &dyn Exec) -> Option<Health> {
    let run = |what: &str| {
        exec.run("hyprctl", &["-j".to_string(), what.to_string()])
            .ok()
            .filter(|out| out.success())
            .map(|out| out.stdout)
    };
    let version = version(&run("version")?);
    Some(Health {
        version,
        deprecated: list(&run("deprecated-config").unwrap_or_default()),
        config_errors: list(&run("configerrors").unwrap_or_default()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::{FakeExec, Output};

    fn ok(stdout: &str) -> Output {
        Output { status: 0, stdout: stdout.into(), stderr: String::new() }
    }

    #[test]
    fn deprecated_options_and_errors_are_read() {
        let exec = FakeExec::new()
            .on_args(
                "hyprctl",
                &["-j", "version"],
                ok(r#"{"branch": "v0.57.1", "tag": "v0.57.1", "version": "0.57.1"}"#),
            )
            .on_args(
                "hyprctl",
                &["-j", "deprecated-config"],
                ok(r#"["dwindle:special_scale_factor: Use workspace rules to achieve the same effect"]"#),
            )
            .on_args("hyprctl", &["-j", "configerrors"], ok("[\n\t\"\"\n]\n"));
        assert_eq!(
            read(&exec).unwrap(),
            Health {
                version: "0.57.1".into(),
                deprecated: vec!["dwindle:special_scale_factor: Use workspace rules to achieve the same effect".into()],
                config_errors: Vec::new(),
            }
        );
        assert!(read(&FakeExec::new().without("hyprctl")).is_none());
    }
}
