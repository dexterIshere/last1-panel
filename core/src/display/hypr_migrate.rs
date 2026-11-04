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
    /// Lancements de l'ouverture de session (`hl.exec_cmd(…)`).
    pub start: Vec<String>,
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

/// Crochet de pacman de go-to-linux : il pose lui-même le hyprland.lua du
/// bureau Signal pendant la mise à jour de Hyprland.
pub const GTL_HOOK: &str = "/etc/pacman.d/hooks/gtl-hyprland.hook";

/// Bureau Signal de go-to-linux, que go-to-linux passe en Lua lui-même.
pub fn by_go_to_linux(hypr_dir: &Path, hook: &Path) -> bool {
    hypr_dir.join("gtl-machine.conf").exists() && hook.exists()
}

/// La configuration à convertir : hyprland.conf, s'il n'y a pas déjà un
/// hyprland.lua à soi (l'exemple généré par Hyprland ne compte pas).
pub fn needed(hypr_dir: &Path) -> bool {
    // L'exemple de Hyprland à la place : quoi qu'il arrive, la configuration
    // du joueur revient (même sur le bureau Signal, si go-to-linux n'a pas pu).
    hypr_lua::config_lost(hypr_dir)
        || !by_go_to_linux(hypr_dir, Path::new(GTL_HOOK))
            && hypr_dir.join("hyprland.conf").exists()
            && !hypr_dir.join("hyprland.lua").exists()
}

/// Hyprland 0.57 installé ne trouvera pas la configuration du joueur :
/// hyprland.conf sans hyprland.lua, ou l'exemple de Hyprland à sa place.
pub fn lost(hypr_dir: &Path) -> bool {
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

/// Début des fichiers convertis par Last1 Panel.
const CONVERTED: &str = "-- Configuration convertie depuis hyprland.conf par Last1 Panel";

/// Réglages dépréciés par Hyprland 0.57 (« deprecated config option » à
/// chaque démarrage) dans un fichier converti par une version précédente :
/// retirés (sans effet) ou déplacés. `None` : rien à changer.
pub fn clean_deprecated(lua: &str) -> Option<(String, Vec<String>)> {
    let mut path: Vec<String> = Vec::new();
    let mut out = Vec::new();
    let mut moved = Vec::new();
    let mut notes = Vec::new();
    for line in lua.lines() {
        let trimmed = line.trim();
        if let Some(name) =
            trimmed.strip_suffix(" = {").filter(|n| n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
        {
            path.push(name.to_string());
        } else if trimmed == "}," || trimmed == "}" {
            path.pop();
        } else if let Some((key, value)) = trimmed.split_once(" = ").filter(|_| !path.is_empty()) {
            let full = format!("{}:{key}", path.join(":"));
            if hypr_convert::REMOVED.contains(&full.as_str()) {
                notes.push(format!("{full} retiré (sans effet depuis Hyprland 0.57)"));
                continue;
            }
            if let Some((_, new)) = hypr_convert::MOVED.iter().find(|(old, _)| *old == full) {
                let (section, field) = new.split_once(':')?;
                moved.push(format!("hl.config({{ {section} = {{ {field} = {} }} }})", value.trim_end_matches(',')));
                notes.push(format!("{full} déplacé en {new}"));
                continue;
            }
        }
        out.push(line.replace("hl.dsp.group.lock_active(", "hl.dsp.group.lock("));
    }
    let lock = lua.contains("hl.dsp.group.lock_active(");
    if lock {
        notes.push("hl.dsp.group.lock_active remplacé par hl.dsp.group.lock".into());
    }
    if notes.is_empty() {
        return None;
    }
    let mut text = out.join("\n") + "\n";
    if !moved.is_empty() {
        text.push_str(&format!("\n-- Réglages déplacés par Hyprland 0.57\n{}\n", moved.join("\n")));
    }
    Some((text, notes))
}

/// hyprland.lua converti par Last1 Panel débarrassé des réglages dépréciés,
/// vérifié par le Hyprland installé avant d'être posé (l'ancien gardé en
/// `.last1-panel.bak`). Rend ce qui a changé ; vide si rien.
pub fn tidy(exec: &dyn Exec, hypr_dir: &Path) -> Vec<String> {
    let main = hypr_dir.join("hyprland.lua");
    let Ok(text) = fs::read_to_string(&main) else { return Vec::new() };
    if !text.starts_with(CONVERTED) {
        return Vec::new();
    }
    let Some((clean, notes)) = clean_deprecated(&text) else { return Vec::new() };
    let staged = hypr_dir.join("hyprland.lua.last1-panel.new");
    if fs::write(&staged, &clean).is_err() || verify(exec, None, &staged).is_err() {
        let _ = fs::remove_file(&staged);
        return Vec::new();
    }
    let backup = hypr_dir.join("hyprland.lua.last1-panel.bak");
    if !backup.exists() && fs::copy(&main, &backup).is_err() {
        let _ = fs::remove_file(&staged);
        return Vec::new();
    }
    if fs::rename(&staged, &main).is_err() {
        return Vec::new();
    }
    notes
}

/// Bureau Signal posé par go-to-linux sous Hyprland 0.57 : les changements
/// du joueur dans son ancien hyprland.conf (fond d'écran, raccourcis,
/// réglages) ajoutés par-dessus (hypr_overlay.rs), après lecture par le
/// Hyprland installé. `None` : rien à faire ; sinon les lancements à faire
/// dans la session en cours, et ce qui n'a pas pu être repris.
/// Lancements à faire tout de suite, et lignes non reprises.
pub type Overlay = (Vec<String>, Vec<String>);

pub fn personal_overlay(exec: &dyn Exec, home: &Path) -> Option<Result<Overlay, String>> {
    use super::hypr_overlay;
    let hypr_dir = home.join(".config/hypr");
    if !hypr_dir.join("gtl-machine.conf").exists()
        || !hypr_overlay::signal_lua_in_place(&hypr_dir, Path::new(hypr_overlay::MODEL))
    {
        return None;
    }
    let overlay = hypr_overlay::personal(&hypr_dir.join("hyprland.conf"), home, &[])?;
    let conversion = hypr_convert::convert(&overlay);
    let text = format!(
        "-- Vos changements du bureau Signal (ancien hyprland.conf), repris par Last1 Panel.\n{}",
        conversion.lua
    );
    let target = hypr_dir.join(hypr_overlay::FILE);
    if fs::read_to_string(&target).is_ok_and(|old| old == text) {
        return None;
    }
    let main = fs::read_to_string(hypr_dir.join("hyprland.lua")).ok()?;
    let check = hypr_dir.join("hyprland.lua.last1-panel.check");
    let checked = fs::write(&check, format!("{main}\n{text}"))
        .map_err(|e| e.to_string())
        .and_then(|()| verify(exec, None, &check));
    let _ = fs::remove_file(&check);
    if let Err(errors) = checked {
        return Some(Err(errors));
    }
    let placed = fs::write(&target, &text)
        .map_err(|e| e.to_string())
        .and_then(|()| hypr_lua::ensure_required(&hypr_dir, hypr_overlay::FILE).map_err(|e| e.to_string()));
    Some(placed.map(|()| (conversion.start, conversion.skipped)))
}

/// Prépare la conversion sans rien poser : le texte Lua, les scripts à
/// réécrire ; `Err` avec ce qui n'a pas d'équivalent sûr.
pub fn prepare(home: &Path) -> Result<(String, Vec<hypr_scripts::Change>, Vec<String>), Vec<String>> {
    let (conversion, changes, missing) = convert_all(home);
    if missing.is_empty() { Ok((conversion.lua, changes, conversion.notes)) } else { Err(missing) }
}

/// Conversion de hyprland.conf et des scripts, et ce qui n'a pas
/// d'équivalent sûr.
fn convert_all(home: &Path) -> (hypr_convert::Conversion, Vec<hypr_scripts::Change>, Vec<String>) {
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
    (conversion, changes, missing)
}

/// Convertit, vérifie (Hyprland installé s'il lit le Lua, sinon `root`, le
/// futur Hyprland d'un essai), puis pose. `Err` : rien n'a changé.
pub fn migrate(exec: &dyn Exec, home: &Path, root: Option<&Path>) -> Result<Done, Vec<String>> {
    let (conversion, changes, missing) = convert_all(home);
    if !missing.is_empty() {
        return Err(missing);
    }
    place(exec, home, root, conversion, changes, Vec::new())
}

/// Hyprland 0.57 est déjà là (mis à jour par n'importe quel chemin) : la
/// configuration du joueur est reprise même si quelques lignes n'ont pas
/// d'équivalent sûr (listées dans les notes, et en commentaire du fichier) —
/// bien mieux que l'exemple de Hyprland. Le fichier doit être lu par le
/// Hyprland installé.
pub fn restore(exec: &dyn Exec, home: &Path) -> Result<Done, Vec<String>> {
    let (conversion, changes, missing) = convert_all(home);
    let left = missing.into_iter().map(|m| format!("non repris : {m}")).collect();
    place(exec, home, None, conversion, changes, left)
}

fn place(
    exec: &dyn Exec,
    home: &Path,
    root: Option<&Path>,
    conversion: hypr_convert::Conversion,
    changes: Vec<hypr_scripts::Change>,
    left: Vec<String>,
) -> Result<Done, Vec<String>> {
    let hypr_dir = home.join(".config/hypr");
    let start = conversion.start.clone();
    let (lua, notes) = (conversion.lua, [conversion.notes, left].concat());
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
    Ok(Done { lua: main, scripts: changes.into_iter().map(|c| c.path).collect(), notes, read_by_installed, start })
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
    fn signal_desktop_is_left_to_go_to_linux() {
        let dir = tempfile::tempdir().unwrap();
        let hook = dir.path().join("gtl-hyprland.hook");
        fs::write(dir.path().join("gtl-machine.conf"), "").unwrap();
        assert!(!by_go_to_linux(dir.path(), &hook));
        fs::write(&hook, "").unwrap();
        assert!(by_go_to_linux(dir.path(), &hook));
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

    #[test]
    fn deprecated_options_of_an_older_conversion_are_cleaned() {
        let old = "-- Configuration convertie depuis hyprland.conf par Last1 Panel (Hyprland 0.57 ne lit plus les .conf).\n\nhl.config({\n    dwindle = {\n        preserve_split = true,\n        special_scale_factor = 0.8,\n    },\n    misc = {\n        layers_hog_keyboard_focus = false,\n        vrr = 2,\n    },\n})\nhl.bind(\"SUPER + G\", hl.dsp.group.lock_active({ action = \"toggle\" }))\n";
        let (new, notes) = clean_deprecated(old).unwrap();
        assert_eq!(notes.len(), 3);
        assert!(!new.contains("special_scale_factor") && !new.contains("lock_active"));
        assert!(new.contains(
            "    dwindle = {\n        preserve_split = true,\n    },\n    misc = {\n        vrr = 2,\n    },"
        ));
        assert!(new.ends_with("hl.config({ layers = { layers_hog_keyboard_focus = false } })\n"));
        assert!(clean_deprecated(&new).is_none());
    }

    #[test]
    fn example_in_place_is_always_restored_even_partly() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let hypr = home.join(".config/hypr");
        fs::create_dir_all(&hypr).unwrap();
        fs::write(
            hypr.join("hyprland.conf"),
            "exec-once = waybar\nbind = SUPER, R, resizeactive, 10% 0\nbind = SUPER, Q, killactive,\n",
        )
        .unwrap();
        // Hyprland 0.57 a posé son exemple : à reprendre, bureau Signal compris.
        fs::write(hypr.join("hyprland.lua"), "-- AUTOGENERATED HYPRLAND CONFIG.\n").unwrap();
        fs::write(hypr.join("gtl-machine.conf"), "").unwrap();
        assert!(needed(&hypr) && lost(&hypr));
        let ok = Output { status: 0, stdout: "Config parsing result:\n\nconfig ok\n".into(), stderr: String::new() };
        let done = restore(&FakeExec::new().on_output("env", ok), home).unwrap();
        let text = fs::read_to_string(hypr.join("hyprland.lua")).unwrap();
        assert!(text.contains("hl.dsp.window.close()") || text.contains("SUPER + Q"), "{text}");
        assert!(done.notes.iter().any(|n| n.contains("resizeactive")), "{:?}", done.notes);
        assert_eq!(done.start, ["hl.exec_cmd(\"waybar\")"]);
        assert!(!lost(&hypr) && !needed(&hypr));
        // L'exemple gardé à côté.
        assert!(fs::read_to_string(hypr.join("hyprland.lua.last1-panel.bak")).unwrap().contains("AUTOGENERATED"));
    }
}
