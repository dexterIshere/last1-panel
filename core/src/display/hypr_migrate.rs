//! Passer le bureau Hyprland du joueur en Lua, une fois et sans perte :
//! hyprland.conf (et ses `source`) converti en hyprland.lua, ses scripts et
//! sa Waybar réécrits ; tout est vérifié avant d'être posé, et ce qui était
//! là est gardé (`.last1-panel.bak`). Rien n'est posé si une seule ligne
//! n'a pas d'équivalent sûr : le joueur garde son bureau tel quel, la raison
//! part dans les rapports.

use super::{hypr_conf, hypr_convert, hypr_lua, hypr_scripts};
use crate::exec::Exec;
use std::fs;
use std::path::{Path, PathBuf};

/// Ce qui a été fait.
#[derive(Debug, Default, PartialEq)]
pub struct Done {
    pub lua: PathBuf,
    pub scripts: Vec<PathBuf>,
    pub notes: Vec<String>,
    /// Le Hyprland installé lit déjà la conversion (sinon seul le futur :
    /// à défaire si la mise à jour n'a pas lieu).
    pub read_by_installed: bool,
}

/// Remet tout comme avant (mise à jour annulée après la conversion).
pub fn rollback(done: &Done) {
    let restore = |path: &Path| {
        let backup = PathBuf::from(format!("{}.last1-panel.bak", path.display()));
        if backup.exists() {
            let _ = fs::rename(&backup, path);
        }
    };
    let lua_backup = done.lua.with_file_name("hyprland.lua.last1-panel.bak");
    if lua_backup.exists() {
        let _ = fs::rename(&lua_backup, &done.lua);
    } else {
        let _ = fs::remove_file(&done.lua);
    }
    for script in &done.scripts {
        restore(script);
    }
}

/// La configuration à convertir : hyprland.conf, s'il n'y a pas déjà un
/// hyprland.lua à soi (l'exemple généré par Hyprland ne compte pas).
pub fn needed(hypr_dir: &Path) -> bool {
    hypr_dir.join("hyprland.conf").exists()
        && (!hypr_dir.join("hyprland.lua").exists() || hypr_lua::config_lost(hypr_dir))
}

/// `Hyprland --verify-config` d'un fichier, avec le Hyprland installé (ou
/// celui d'un dossier d'essai : `root`).
pub fn verify(exec: &dyn Exec, root: Option<&Path>, file: &Path) -> Result<(), String> {
    // Sans écran ni Hyprland joignables : ce qu'une configuration lancerait
    // pendant l'essai (hl.exec_cmd hors évènement) ne peut rien ouvrir.
    let mut args: Vec<String> =
        ["-u", "WAYLAND_DISPLAY", "-u", "DISPLAY", "-u", "HYPRLAND_INSTANCE_SIGNATURE"].map(String::from).to_vec();
    match root {
        Some(root) => {
            args.push(format!("LD_LIBRARY_PATH={}", root.join("usr/lib").display()));
            args.push(root.join("usr/bin/Hyprland").to_string_lossy().into_owned());
        }
        None => args.push("Hyprland".into()),
    }
    args.extend(["--verify-config".into(), "-c".into(), file.to_string_lossy().into_owned()]);
    let output = exec.run("env", &args).map_err(|e| e.to_string())?;
    crate::system::update_check::verify_result(output.status, &output.stdout)
}

/// Raccourcis sur des touches qui n'existent pas (« xf86AudioPlayPause ») :
/// l'ancien Hyprland les ignorait, 0.57 les refuse. Si ce sont les seules
/// erreurs, ces lignes passent en commentaire (rien ne change pour le
/// joueur) ; sinon `None`.
pub fn drop_unknown_keys(lua: &str, errors: &str) -> Option<(String, Vec<String>)> {
    let mut numbers = Vec::new();
    for line in errors.lines().filter(|l| !l.trim().is_empty()) {
        let (place, message) = line.split_once(": hl.bind: failed to create bind: Unknown key: ")?;
        let number: usize = place.rsplit(':').next()?.trim().parse().ok()?;
        numbers.push((number, message.trim().to_string()));
    }
    if numbers.is_empty() {
        return None;
    }
    let mut notes = Vec::new();
    let text: Vec<String> = lua
        .lines()
        .enumerate()
        .map(|(i, l)| match numbers.iter().find(|(n, _)| *n == i + 1) {
            Some((_, key)) => {
                notes.push(format!("raccourci sur « {key} » retiré : touche inconnue, sans effet avant aussi"));
                format!("-- touche inconnue, sans effet avant aussi : {l}")
            }
            None => l.to_string(),
        })
        .collect();
    Some((text.join("\n") + "\n", notes))
}

/// Écrit `lua` dans `file` et le fait lire par Hyprland ; retire au besoin
/// les raccourcis sur des touches inexistantes, puis relit. Rend le texte
/// posé et ses notes.
pub fn write_verified(
    exec: &dyn Exec,
    root: Option<&Path>,
    file: &Path,
    lua: &str,
) -> Result<(String, Vec<String>), String> {
    fs::write(file, lua).map_err(|e| e.to_string())?;
    match verify(exec, root, file) {
        Ok(()) => Ok((lua.to_string(), Vec::new())),
        Err(errors) => {
            let (fixed, notes) = drop_unknown_keys(lua, &errors).ok_or(errors)?;
            fs::write(file, &fixed).map_err(|e| e.to_string())?;
            verify(exec, root, file).map(|()| (fixed, notes))
        }
    }
}

/// Prépare la conversion sans rien poser : le texte Lua, les scripts à
/// réécrire ; `Err` avec ce qui n'a pas d'équivalent sûr.
pub fn prepare(home: &Path) -> Result<(String, Vec<hypr_scripts::Change>, Vec<String>), Vec<String>> {
    let hypr_dir = home.join(".config/hypr");
    let config = hypr_conf::Config::read(&hypr_dir.join("hyprland.conf"), home);
    let conversion = hypr_convert::convert(&config);
    let changes = hypr_scripts::plan(home);
    let mut missing: Vec<String> = config.problems.clone();
    missing.extend(conversion.skipped.iter().cloned());
    for change in &changes {
        for left in &change.left {
            missing.push(format!("{} : {left}", change.path.display()));
        }
    }
    if missing.is_empty() { Ok((conversion.lua, changes, conversion.notes)) } else { Err(missing) }
}

/// Convertit, vérifie (Hyprland installé s'il lit le Lua, sinon `root`, le
/// futur Hyprland d'un essai), puis pose. `Err` : rien n'a changé.
pub fn migrate(exec: &dyn Exec, home: &Path, root: Option<&Path>) -> Result<Done, Vec<String>> {
    let hypr_dir = home.join(".config/hypr");
    let (lua, changes, notes) = prepare(home)?;
    let staged = hypr_dir.join("hyprland.lua.last1-panel.new");
    let mut notes = notes;
    // Le Hyprland qui lira le fichier ; à défaut celui de l'essai.
    let installed = write_verified(exec, None, &staged, &lua);
    let read_by_installed = installed.is_ok();
    let checked = installed.or_else(|local| match root {
        Some(root) => write_verified(exec, Some(root), &staged, &lua),
        None => Err(local),
    });
    match checked {
        Ok((_, more)) => notes.extend(more),
        Err(error) => {
            let _ = fs::remove_file(&staged);
            return Err(vec![format!("Hyprland refuse la conversion : {error}")]);
        }
    }
    let main = hypr_dir.join("hyprland.lua");
    if main.exists() {
        let backup = hypr_dir.join("hyprland.lua.last1-panel.bak");
        fs::rename(&main, &backup).map_err(|e| vec![e.to_string()])?;
    }
    fs::rename(&staged, &main).map_err(|e| vec![e.to_string()])?;
    hypr_scripts::apply(&changes).map_err(|e| vec![e.to_string()])?;
    Ok(Done { lua: main, scripts: changes.into_iter().map(|c| c.path).collect(), notes, read_by_installed })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::{FakeExec, Output};

    #[test]
    fn verified_then_placed_with_backups() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let hypr = home.join(".config/hypr");
        let waybar = home.join(".config/waybar");
        fs::create_dir_all(&hypr).unwrap();
        fs::create_dir_all(&waybar).unwrap();
        fs::write(hypr.join("hyprland.conf"), "bind = SUPER, Q, killactive,\nexec-once = waybar\n").unwrap();
        fs::write(hypr.join("hyprland.lua"), "-- AUTOGENERATED HYPRLAND CONFIG.\n").unwrap();
        let bar = r#"{ "on-scroll-up": "hyprctl dispatch workspace e+1" }"#;
        fs::write(waybar.join("config.jsonc"), bar).unwrap();
        assert!(needed(&hypr));
        let ok = |status| Output {
            status,
            stdout: "\n======== Config parsing result:\n\nconfig ok\n".into(),
            stderr: String::new(),
        };
        let exec = FakeExec::new().on_output("env", ok(0));
        let done = migrate(&exec, home, None).unwrap();
        let lua = fs::read_to_string(hypr.join("hyprland.lua")).unwrap();
        assert!(lua.contains(r#"hl.bind("SUPER + Q", hl.dsp.window.close())"#), "{lua}");
        assert!(fs::read_to_string(hypr.join("hyprland.lua.last1-panel.bak")).unwrap().contains("AUTOGENERATED"));
        assert!(fs::read_to_string(waybar.join("config.jsonc")).unwrap().contains("hl.dsp.focus"));
        assert_eq!(fs::read_to_string(waybar.join("config.jsonc.last1-panel.bak")).unwrap(), bar);
        assert_eq!(done.scripts, [waybar.join("config.jsonc")]);
        assert!(!needed(&hypr));
        assert!(hypr.join("hyprland.conf").exists(), "l'ancienne gardée");
        // Défaite : tout revient.
        rollback(&done);
        assert!(fs::read_to_string(hypr.join("hyprland.lua")).unwrap().contains("AUTOGENERATED"));
        assert_eq!(fs::read_to_string(waybar.join("config.jsonc")).unwrap(), bar);
    }

    #[test]
    fn unknown_keys_only_are_dropped() {
        let lua = "a\nhl.bind(\"xf86AudioPlayPause\", hl.dsp.exec_cmd(\"x\"))\nb\n";
        let errors = "/h/hyprland.lua:2: hl.bind: failed to create bind: Unknown key: xf86AudioPlayPause\n";
        let (fixed, notes) = drop_unknown_keys(lua, errors).unwrap();
        assert_eq!(
            fixed,
            "a\n-- touche inconnue, sans effet avant aussi : hl.bind(\"xf86AudioPlayPause\", hl.dsp.exec_cmd(\"x\"))\nb\n"
        );
        assert_eq!(notes.len(), 1);
        let other = format!("{errors}/h/hyprland.lua:28: error setting 'x': bad\n");
        assert!(drop_unknown_keys(lua, &other).is_none(), "une autre erreur : rien de retiré");
    }

    #[test]
    fn nothing_placed_when_something_is_unsure_or_refused() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let hypr = home.join(".config/hypr");
        fs::create_dir_all(&hypr).unwrap();
        fs::write(hypr.join("hyprland.conf"), "bind = SUPER, R, resizeactive, 10% 0\n").unwrap();
        let exec = FakeExec::new();
        assert!(migrate(&exec, home, None).unwrap_err()[0].contains("resizeactive"));
        assert!(!hypr.join("hyprland.lua").exists());
        // Refusée par Hyprland : rien de posé non plus.
        fs::write(hypr.join("hyprland.conf"), "bind = SUPER, Q, killactive,\n").unwrap();
        let refused = Output { status: 1, stdout: "Config parsing result:\n\nboom\n".into(), stderr: String::new() };
        let exec = FakeExec::new().on_output("env", refused);
        assert!(migrate(&exec, home, None).unwrap_err()[0].contains("boom"));
        assert!(!hypr.join("hyprland.lua").exists() && !hypr.join("hyprland.lua.last1-panel.new").exists());
    }
}
