//! Avant de proposer une mise à jour du système : l'essayer sur ce PC, sans
//! rien installer. Les paquets du bureau qui vont changer (Hyprland et ses
//! bibliothèques, Waybar…) sont téléchargés depuis les miroirs du système,
//! leur signature vérifiée avec les clés de pacman, puis dépaquetés dans un
//! dossier à part ; le futur Hyprland lit alors la configuration actuelle du
//! joueur (`Hyprland --verify-config`, sans ouvrir d'écran).
//!
//! - Elle passe : la mise à jour peut être proposée.
//! - Elle ne passe qu'une fois convertie en Lua (Hyprland 0.57 ne lit plus
//!   les `.conf`) : la conversion (hypr_convert.rs) est essayée aussi ; la
//!   mise à jour sera proposée avec elle.
//! - Sinon : retenue, avec la raison (envoyée dans les rapports d'erreur
//!   pour que Last1 Panel apprenne à la régler).
//!
//! Waybar : avec Hyprland 0.57, ses modules hyprland/* doivent parler Lua
//! (`hl.dsp…`) ; vérifié dans le binaire qui sera installé.

use crate::exec::Exec;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Paquets du bureau Hyprland : un changement de l'un d'eux est essayé.
pub const DESKTOP: [&str; 12] = [
    "hyprland",
    "aquamarine",
    "hyprutils",
    "hyprlang",
    "hyprgraphics",
    "hyprcursor",
    "hyprwire",
    "hyprland-qt-support",
    "hyprtoolkit",
    "xdg-desktop-portal-hyprland",
    "glaze",
    "waybar",
];

/// Une mise à jour en attente : « nom ancienne -> nouvelle » (checkupdates).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pending {
    pub name: String,
    pub from: String,
    pub to: String,
}

pub fn parse_pending(text: &str) -> Vec<Pending> {
    text.lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let (name, from, arrow, to) = (words.next()?, words.next()?, words.next()?, words.next()?);
            (arrow == "->").then(|| Pending { name: name.into(), from: from.into(), to: to.into() })
        })
        .collect()
}

/// Verdict de l'essai.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "verdict")]
pub enum Verdict {
    /// Rien du bureau ne change, ou tout passe tel quel.
    Ok,
    /// Passe une fois la configuration convertie en Lua.
    NeedsConversion,
    /// Retenue : pourquoi (pour le joueur et pour les rapports).
    Blocked { reasons: Vec<String> },
}

/// Résultat gardé (cache) : pour quelle liste de mises à jour, et le verdict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub pending: Vec<Pending>,
    pub verdict: Verdict,
    /// Version de Hyprland essayée, s'il y en avait une.
    pub hyprland: Option<String>,
}

/// Ce qui, dans les mises à jour, touche au bureau.
pub fn desktop_changes(pending: &[Pending]) -> Vec<&Pending> {
    pending.iter().filter(|p| DESKTOP.contains(&p.name.as_str())).collect()
}

/// `hl.dsp` dans le binaire de Waybar : ses modules Hyprland parlent Lua.
pub fn waybar_speaks_lua(binary: &[u8]) -> bool {
    binary.windows(6).any(|w| w == b"hl.dsp")
}

/// La configuration de Waybar se sert des modules Hyprland.
pub fn waybar_uses_hyprland(config_dir: &Path) -> bool {
    ["config", "config.jsonc", "config.json"]
        .iter()
        .any(|name| fs::read_to_string(config_dir.join(name)).is_ok_and(|text| text.contains("\"hyprland/")))
}

/// Dossier de travail : paquets téléchargés et dépaquetés.
pub struct Sandbox {
    pub dir: PathBuf,
}

impl Sandbox {
    pub fn root(&self) -> PathBuf {
        self.dir.join("root")
    }

    /// Adresses des paquets (et de ceux qu'ils tirent) d'après les listes de
    /// `dbpath` (celles de checkupdates), sans rien installer.
    pub fn urls(exec: &dyn Exec, dbpath: &Path, names: &[&str]) -> Vec<String> {
        let mut args: Vec<String> = vec![
            "-Sp".into(),
            "--dbpath".into(),
            dbpath.to_string_lossy().into_owned(),
            "--print-format".into(),
            "%l".into(),
        ];
        args.extend(names.iter().map(|n| n.to_string()));
        exec.run("pacman", &args)
            .map(|o| o.stdout.lines().map(str::trim).filter(|l| l.contains("://")).map(str::to_string).collect())
            .unwrap_or_default()
    }

    /// Télécharge, vérifie la signature (clés de pacman) et dépaquette.
    /// Rend l'erreur lisible au premier paquet qui ne passe pas.
    pub fn fetch(&self, exec: &dyn Exec, urls: &[String]) -> Result<(), String> {
        let packages = self.dir.join("packages");
        fs::create_dir_all(&packages).map_err(|e| e.to_string())?;
        fs::create_dir_all(self.root()).map_err(|e| e.to_string())?;
        for url in urls {
            let name = url.rsplit('/').next().unwrap_or("paquet").to_string();
            let file = packages.join(&name);
            let sig = packages.join(format!("{name}.sig"));
            for (from, to) in [(url.clone(), &file), (format!("{url}.sig"), &sig)] {
                if let Some(local) = from.strip_prefix("file://") {
                    fs::copy(local, to).map_err(|e| format!("{local} : {e}"))?;
                } else {
                    exec.run_ok(
                        "curl",
                        &["-fsSL".into(), "--retry".into(), "3".into(), "-o".into(), to.to_string_lossy().into(), from],
                    )
                    .map_err(|e| e.to_string())?;
                }
            }
            exec.run_ok(
                "gpgv",
                &[
                    "--keyring".into(),
                    "/etc/pacman.d/gnupg/pubring.gpg".into(),
                    sig.to_string_lossy().into(),
                    file.to_string_lossy().into(),
                ],
            )
            .map_err(|e| format!("signature de {name} : {e}"))?;
            exec.run_ok(
                "bsdtar",
                &[
                    "-xf".into(),
                    file.to_string_lossy().into(),
                    "-C".into(),
                    self.root().to_string_lossy().into(),
                    "--exclude".into(),
                    ".*".into(),
                ],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Le futur Hyprland lit `config` : `Ok` si « config ok », sinon ses
    /// erreurs.
    pub fn verify_hyprland(&self, exec: &dyn Exec, config: &Path) -> Result<(), String> {
        let root = self.root();
        let binary = root.join("usr/bin/Hyprland");
        let output = exec
            .run(
                "env",
                &[
                    format!("LD_LIBRARY_PATH={}", root.join("usr/lib").display()),
                    binary.to_string_lossy().into(),
                    "--verify-config".into(),
                    "-c".into(),
                    config.to_string_lossy().into(),
                ],
            )
            .map_err(|e| e.to_string())?;
        verify_result(output.status, &output.stdout)
    }
}

/// Sortie de `Hyprland --verify-config` : « config ok » et code 0, sinon les
/// erreurs après « Config parsing result ».
pub fn verify_result(status: i32, stdout: &str) -> Result<(), String> {
    let result = stdout.split("Config parsing result:").nth(1).map(str::trim).unwrap_or(stdout.trim());
    if status == 0 && result.contains("config ok") {
        Ok(())
    } else {
        Err(result.lines().take(15).collect::<Vec<_>>().join("\n"))
    }
}

/// Version de Hyprland qui passera à Lua seulement (0.57 et suivantes).
pub fn needs_lua(version: &str) -> bool {
    let mut parts = version.split(['.', '-', ':']).filter_map(|p| p.parse::<u32>().ok());
    matches!((parts.next(), parts.next()), (Some(major), Some(minor)) if major > 0 || minor >= 57)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_desktop_and_verdicts() {
        let pending = parse_pending(
            "hyprland 0.56.2-4 -> 0.57.1-1\nfirefox 140.0-1 -> 141.0-1\nlinux-firmware-nvidia 1-1 -> 2-1\nwaybar 0.15.0-3 -> 0.15.0-4\n",
        );
        assert_eq!(pending.len(), 4);
        let names: Vec<&str> = desktop_changes(&pending).iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["hyprland", "waybar"]);
        assert!(needs_lua("0.57.1-1") && needs_lua("1.0.0-1") && !needs_lua("0.56.2-4"));
        assert!(waybar_speaks_lua(b"\0buildLuaDispatch hl.dsp.focus({ workspace = \0"));
        assert!(!waybar_speaks_lua(b"dispatch workspace "));
        let ok = "Welcome\n\n======== Config parsing result:\n\nconfig ok\n";
        assert_eq!(verify_result(0, ok), Ok(()));
        let bad = "\n\n======== Config parsing result:\n\nhyprland.lua:2: unexpected symbol near 'monitor'\n";
        assert_eq!(verify_result(1, bad), Err("hyprland.lua:2: unexpected symbol near 'monitor'".to_string()));
        let check = Check {
            pending,
            verdict: Verdict::Blocked { reasons: vec!["Waybar".into()] },
            hyprland: Some("0.57.1-1".into()),
        };
        let json = serde_json::to_string(&check).unwrap();
        assert!(json.contains("\"verdict\":\"blocked\""), "{json}");
    }
}
