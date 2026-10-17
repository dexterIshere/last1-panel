//! Steam : jeux installés et options de lancement.
//!
//! Nos réglages passent par l'option de lancement
//! `last1-panel-cli run %command%` (chemin complet de l'outil quand il vient
//! de l'AppImage, hors du PATH), écrite dans localconfig.vdf de chaque compte
//! Steam du PC. Steam réécrit ce fichier en quittant : on n'y touche que
//! Steam fermé.

use super::vdf;
use crate::error::{CoreError, CoreResult};
use crate::i18n::t;
use crate::paths;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const CLI: &str = "last1-panel-cli";
const COMMAND: &str = "%command%";

/// Ce que notre option de lancement ajoute devant la commande du jeu :
/// `last1-panel-cli run` avec le paquet, le chemin complet de l'outil posé
/// par l'AppImage sinon.
pub fn wrapper() -> String {
    wrapper_for(&paths::cli())
}

fn wrapper_for(cli: &Path) -> String {
    if cli == Path::new(paths::SYSTEM_CLI) {
        return format!("{CLI} run");
    }
    let text = cli.to_string_lossy();
    if text.chars().all(|c| c.is_ascii_alphanumeric() || "/._-+".contains(c)) {
        format!("{text} run")
    } else {
        // Options exécutées par le shell : chemin entre apostrophes.
        format!("'{}' run", text.replace('\'', r"'\''"))
    }
}

/// Position de notre outil dans les options (début et fin, « run » compris),
/// quel que soit son chemin : `last1-panel-cli run`, `/chemin/last1-panel-cli
/// run` ou `'/chemin avec espace/last1-panel-cli' run`.
fn wrapper_span(options: &str) -> Option<(usize, usize)> {
    for (index, _) in options.match_indices(CLI) {
        let after = &options[index + CLI.len()..];
        let quote = after.starts_with('\'');
        let rest = if quote { &after[1..] } else { after };
        let Some(tail) = rest.strip_prefix(" run") else { continue };
        if !(tail.is_empty() || tail.starts_with(char::is_whitespace)) {
            continue;
        }
        let before = &options[..index];
        let start = if quote {
            match before.rfind('\'') {
                Some(start) => start,
                None => continue,
            }
        } else {
            before.rfind(char::is_whitespace).map_or(0, |i| i + 1)
        };
        // Le mot entier est l'outil : seul ou au bout d'un chemin.
        let prefix = options[start..index].trim_start_matches('\'');
        if !(prefix.is_empty() || prefix.ends_with('/')) {
            continue;
        }
        return Some((start, options.len() - tail.len()));
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    pub app_id: u32,
    pub name: String,
    /// Dossier d'installation du jeu.
    pub install_dir: PathBuf,
    /// Option de lancement actuelle (celle du premier compte qui en a une).
    pub launch_options: String,
    /// Notre option est en place pour tous les comptes.
    pub wrapped: bool,
}

/// Dossier de Steam : celui du paquet de CachyOS, ou celui de Flatpak.
pub fn find_root(home: &Path) -> Option<PathBuf> {
    [".local/share/Steam", ".steam/steam", ".var/app/com.valvesoftware.Steam/data/Steam"]
        .iter()
        .map(|relative| home.join(relative))
        .find(|path| path.join("steamapps").is_dir())
}

/// Bibliothèques déclarées (dont celles des disques de Windows ajoutées par
/// go-to-linux), Steam lui-même compris.
pub fn libraries(root: &Path) -> Vec<PathBuf> {
    let mut found = vec![root.to_path_buf()];
    if let Ok(text) = fs::read_to_string(root.join("steamapps/libraryfolders.vdf"))
        && let Ok(file) = vdf::parse(&text)
        && let Some(folders) = file.block("libraryfolders")
    {
        for (_, entry) in &folders.entries {
            if let vdf::Value::Block(folder) = entry
                && let Some(path) = folder.text("path")
            {
                let path = PathBuf::from(path);
                if !found.iter().any(|known| same_dir(known, &path)) {
                    found.push(path);
                }
            }
        }
    }
    found
}

fn same_dir(a: &Path, b: &Path) -> bool {
    a == b || matches!((fs::canonicalize(a), fs::canonicalize(b)), (Ok(x), Ok(y)) if x == y)
}

/// Outils de Steam installés comme des jeux, jamais montrés.
fn is_tool(name: &str) -> bool {
    ["Proton", "Steam Linux Runtime", "Steamworks Common Redistributables", "SteamVR"]
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

/// Jeux installés, triés par nom.
pub fn installed_games(root: &Path) -> Vec<Game> {
    let options = launch_options(root);
    let mut games: BTreeMap<u32, Game> = BTreeMap::new();
    for library in libraries(root) {
        let Ok(entries) = fs::read_dir(library.join("steamapps")) else { continue };
        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let Some(name) = file_name.to_str() else { continue };
            if !(name.starts_with("appmanifest_") && name.ends_with(".acf")) {
                continue;
            }
            let Ok(text) = fs::read_to_string(entry.path()) else { continue };
            let Ok(file) = vdf::parse(&text) else { continue };
            let Some(state) = file.block("AppState") else { continue };
            let (Some(app_id), Some(title)) = (state.text("appid").and_then(|id| id.parse().ok()), state.text("name"))
            else {
                continue;
            };
            if is_tool(title) {
                continue;
            }
            let per_user = options.get(&app_id).cloned().unwrap_or_default();
            let launch = per_user.iter().find(|value| !value.is_empty()).cloned().unwrap_or_default();
            let wrapped = !per_user.is_empty() && per_user.iter().all(|value| is_wrapped(value));
            games.insert(
                app_id,
                Game {
                    app_id,
                    name: title.to_string(),
                    install_dir: library.join("steamapps/common").join(state.text("installdir").unwrap_or(title)),
                    launch_options: launch,
                    wrapped,
                },
            );
        }
    }
    let mut list: Vec<Game> = games.into_values().collect();
    list.sort_by_key(|game| game.name.to_lowercase());
    list
}

/// Fichiers localconfig.vdf des comptes Steam du PC.
pub fn local_configs(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(root.join("userdata")) else { return Vec::new() };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| entry.file_name().to_str().is_some_and(|name| name.parse::<u64>().is_ok_and(|id| id > 0)))
        .map(|entry| entry.path().join("config/localconfig.vdf"))
        .filter(|path| path.is_file())
        .collect();
    files.sort();
    files
}

const APPS_PATH: [&str; 5] = ["UserLocalConfigStore", "Software", "Valve", "Steam", "apps"];

/// Options de lancement de chaque jeu, pour chaque compte (même ordre que
/// [`local_configs`] ; texte vide si le compte n'en a pas).
fn launch_options(root: &Path) -> BTreeMap<u32, Vec<String>> {
    let mut found: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    let configs = local_configs(root);
    for (index, path) in configs.iter().enumerate() {
        let Ok(text) = fs::read_to_string(path) else { continue };
        let Ok(file) = vdf::parse(&text) else { continue };
        let Some(apps) = file.path(&APPS_PATH) else { continue };
        for (key, value) in &apps.entries {
            let (Ok(app_id), vdf::Value::Block(app)) = (key.parse::<u32>(), value) else { continue };
            let slot = found.entry(app_id).or_insert_with(|| vec![String::new(); configs.len()]);
            slot[index] = app.text("LaunchOptions").unwrap_or_default().to_string();
        }
    }
    found
}

pub fn is_wrapped(options: &str) -> bool {
    wrapper_span(options).is_some()
}

/// Options de lancement avec notre outil devant le jeu. Les variables et
/// arguments déjà présents restent (`FOO=1 %command% -dx12` devient
/// `FOO=1 last1-panel-cli run %command% -dx12`) ; un outil déjà là, à un
/// autre chemin (AppImage puis paquet), est remplacé.
pub fn wrap(options: &str) -> String {
    wrap_with(options, &wrapper())
}

fn wrap_with(options: &str, wrapper: &str) -> String {
    let current = options.trim();
    if wrapper_span(current).is_some_and(|(start, end)| &current[start..end] == wrapper) {
        return current.to_string();
    }
    let options = unwrap(current);
    if options.contains(COMMAND) {
        options.replacen(COMMAND, &format!("{wrapper} {COMMAND}"), 1)
    } else if options.is_empty() {
        format!("{wrapper} {COMMAND}")
    } else {
        // Sans %command%, Steam ajoute les options après la commande du jeu.
        format!("{wrapper} {COMMAND} {options}")
    }
}

/// Options de lancement sans notre outil, le reste à l'identique ; vide
/// s'il ne reste que %command%.
pub fn unwrap(options: &str) -> String {
    let rest = match wrapper_span(options) {
        Some((start, end)) => {
            let after = &options[end..];
            format!("{}{}", &options[..start], after.strip_prefix(' ').unwrap_or(after))
        }
        None => options.to_string(),
    };
    if rest.trim() == COMMAND { String::new() } else { rest.trim().to_string() }
}

/// Steam en Flatpak : les options de lancement s'y exécutent dans le bac à
/// sable, où /usr/bin du système (donc last1-panel-cli) n'existe pas.
pub fn is_flatpak(root: &Path) -> bool {
    root.components().any(|part| part.as_os_str() == "com.valvesoftware.Steam")
}

/// Steam est-il lancé ? (il réécrirait localconfig.vdf en quittant).
pub fn is_running(proc_dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(proc_dir) else { return false };
    entries.flatten().any(|entry| {
        entry.file_name().to_str().is_some_and(|name| name.chars().all(|c| c.is_ascii_digit()))
            && fs::read_to_string(entry.path().join("comm")).is_ok_and(|comm| comm.trim() == "steam")
    })
}

/// Met (ou retire) notre option de lancement pour ces jeux, dans tous les
/// comptes Steam. Refusé si Steam est lancé.
pub fn set_wrapped(root: &Path, proc_dir: &Path, app_ids: &[u32], wrapped: bool) -> CoreResult<usize> {
    if wrapped && is_flatpak(root) {
        return Err(CoreError::message(t(
            "Steam est installé en Flatpak : un jeu ne pourrait pas lancer Last1 Panel depuis son bac à sable. Installez Steam par le paquet de CachyOS (steam).",
            "Steam is installed as a Flatpak: a game could not start Last1 Panel from its sandbox. Install Steam from the CachyOS package (steam).",
        )));
    }
    if is_running(proc_dir) {
        return Err(CoreError::message(t(
            "Fermez Steam d'abord (menu Steam > Quitter) : il remplacerait ce réglage en se fermant.",
            "Quit Steam first (Steam menu > Exit): it would overwrite this setting when closing.",
        )));
    }
    let configs = local_configs(root);
    if configs.is_empty() {
        return Err(CoreError::message(t(
            "Aucun compte Steam sur ce PC : lancez Steam et connectez-vous une première fois.",
            "No Steam account on this PC: start Steam and sign in once.",
        )));
    }
    let mut changed = 0;
    for path in configs {
        let text = fs::read_to_string(&path)?;
        let mut file =
            vdf::parse(&text).map_err(|error| CoreError::Unreadable("localconfig.vdf".into(), error.to_string()))?;
        let apps = file.path_mut(&APPS_PATH);
        for app_id in app_ids {
            let app = apps.block_mut(&app_id.to_string());
            let current = app.text("LaunchOptions").unwrap_or_default().to_string();
            let next = if wrapped { wrap(&current) } else { unwrap(&current) };
            if next != current {
                app.set_text("LaunchOptions", &next);
                changed += 1;
            }
        }
        write_atomic(&path, &vdf::serialize(&file))?;
    }
    Ok(changed)
}

/// Écrit à côté puis remplace : jamais de fichier à moitié écrit, même en cas
/// de coupure (une copie de l'original reste en .last1-panel.bak).
fn write_atomic(path: &Path, text: &str) -> CoreResult<()> {
    let backup = path.with_extension("vdf.last1-panel.bak");
    if !backup.exists() {
        fs::copy(path, &backup)?;
    }
    let temporary = path.with_extension("vdf.last1-panel.new");
    fs::write(&temporary, text)?;
    fs::rename(&temporary, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steam() -> tempfile::TempDir {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join(".local/share/Steam");
        let other = home.path().join("windows-d/SteamLibrary");
        fs::create_dir_all(root.join("steamapps")).unwrap();
        fs::create_dir_all(other.join("steamapps")).unwrap();
        fs::write(
            root.join("steamapps/libraryfolders.vdf"),
            format!(
                "\"libraryfolders\"\n{{\n\t\"0\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n\t\"1\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n}}\n",
                root.display(),
                other.display()
            ),
        )
        .unwrap();
        let manifest = |dir: &Path, id: u32, name: &str| {
            fs::write(
                dir.join(format!("steamapps/appmanifest_{id}.acf")),
                format!("\"AppState\"\n{{\n\t\"appid\"\t\t\"{id}\"\n\t\"name\"\t\t\"{name}\"\n\t\"installdir\"\t\t\"{name}\"\n}}\n"),
            )
            .unwrap();
        };
        manifest(&root, 1091500, "Cyberpunk 2077");
        manifest(&root, 2805730, "Proton 9.0");
        manifest(&root, 1628350, "Steam Linux Runtime 3.0 (sniper)");
        manifest(&other, 570, "Dota 2");
        manifest(&other, 1245620, "ELDEN RING");
        for user in ["12345678", "87654321"] {
            let dir = root.join("userdata").join(user).join("config");
            fs::create_dir_all(&dir).unwrap();
            let options = if user == "12345678" { "PROTON_LOG=1 %command% -dx12" } else { "" };
            fs::write(
                dir.join("localconfig.vdf"),
                format!(
                    "\"UserLocalConfigStore\"\n{{\n\t\"Software\"\n\t{{\n\t\t\"Valve\"\n\t\t{{\n\t\t\t\"Steam\"\n\t\t\t{{\n\t\t\t\t\"apps\"\n\t\t\t\t{{\n\t\t\t\t\t\"1091500\"\n\t\t\t\t\t{{\n\t\t\t\t\t\t\"LastPlayed\"\t\t\"1760000000\"\n\t\t\t\t\t\t\"LaunchOptions\"\t\t\"{options}\"\n\t\t\t\t\t}}\n\t\t\t\t}}\n\t\t\t}}\n\t\t}}\n\t}}\n\t\"friends\"\n\t{{\n\t\t\"PersonaName\"\t\t\"joueur\"\n\t}}\n}}\n"
                ),
            )
            .unwrap();
        }
        // Dossier « 0 » des connexions anonymes : jamais un compte.
        fs::create_dir_all(root.join("userdata/0/config")).unwrap();
        home
    }

    fn proc_without_steam() -> tempfile::TempDir {
        let proc_dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(proc_dir.path().join("1")).unwrap();
        fs::write(proc_dir.path().join("1/comm"), "systemd\n").unwrap();
        proc_dir
    }

    #[test]
    fn lists_games_from_every_library_without_tools() {
        let home = steam();
        let root = find_root(home.path()).unwrap();
        let games = installed_games(&root);
        let names: Vec<&str> = games.iter().map(|game| game.name.as_str()).collect();
        assert_eq!(names, ["Cyberpunk 2077", "Dota 2", "ELDEN RING"]);
        let cyberpunk = &games[0];
        assert_eq!(cyberpunk.app_id, 1091500);
        assert_eq!(cyberpunk.launch_options, "PROTON_LOG=1 %command% -dx12");
        assert!(!cyberpunk.wrapped);
        assert!(games[1].install_dir.ends_with("windows-d/SteamLibrary/steamapps/common/Dota 2"));
    }

    #[test]
    fn wrap_keeps_existing_options() {
        let package = wrapper_for(Path::new(paths::SYSTEM_CLI));
        assert_eq!(package, "last1-panel-cli run");
        let wrap = |options: &str| wrap_with(options, &package);
        assert_eq!(wrap(""), "last1-panel-cli run %command%");
        assert_eq!(wrap("PROTON_LOG=1 %command% -dx12"), "PROTON_LOG=1 last1-panel-cli run %command% -dx12");
        assert_eq!(wrap("-novid -high"), "last1-panel-cli run %command% -novid -high");
        let once = wrap("gamemoderun %command%");
        assert_eq!(wrap(&once), once);
        assert_eq!(unwrap(&wrap("PROTON_LOG=1 %command% -dx12")), "PROTON_LOG=1 %command% -dx12");
        assert_eq!(unwrap(&wrap("")), "");
        assert_eq!(unwrap(&wrap("-novid")), "%command% -novid");
        // Les espaces de l'utilisateur ne bougent pas.
        assert_eq!(unwrap(&wrap(r#"%command% -name "a  b""#)), r#"%command% -name "a  b""#);
        // Un autre outil qui finit pareil n'est pas le nôtre.
        assert!(!is_wrapped("my-last1-panel-cli run %command%"));
    }

    #[test]
    fn appimage_tool_is_called_by_its_full_path() {
        let appimage = wrapper_for(Path::new("/home/nelly/.local/lib/last1-panel/last1-panel-cli"));
        assert_eq!(appimage, "/home/nelly/.local/lib/last1-panel/last1-panel-cli run");
        let options = wrap_with("PROTON_LOG=1 %command% -dx12", &appimage);
        assert_eq!(options, "PROTON_LOG=1 /home/nelly/.local/lib/last1-panel/last1-panel-cli run %command% -dx12");
        assert!(is_wrapped(&options));
        assert_eq!(unwrap(&options), "PROTON_LOG=1 %command% -dx12");
        // Passage au paquet : le chemin de l'AppImage est remplacé.
        assert_eq!(wrap_with(&options, "last1-panel-cli run"), "PROTON_LOG=1 last1-panel-cli run %command% -dx12");

        let spaced = wrapper_for(Path::new("/home/N l/.local/lib/last1-panel/last1-panel-cli"));
        assert_eq!(spaced, "'/home/N l/.local/lib/last1-panel/last1-panel-cli' run");
        let options = wrap_with("%command% -novid", &spaced);
        assert!(is_wrapped(&options));
        assert_eq!(unwrap(&options), "%command% -novid");
    }

    #[test]
    fn flatpak_steam_is_refused() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join(".var/app/com.valvesoftware.Steam/data/Steam");
        fs::create_dir_all(root.join("steamapps")).unwrap();
        assert_eq!(find_root(home.path()).as_deref(), Some(root.as_path()));
        assert!(is_flatpak(&root));
        let error = set_wrapped(&root, home.path(), &[570], true).unwrap_err().to_string();
        assert!(error.contains("Flatpak"), "{error}");
        assert!(!is_flatpak(&home.path().join(".local/share/Steam")));
    }

    #[test]
    fn writes_launch_options_for_every_account() {
        let home = steam();
        let root = find_root(home.path()).unwrap();
        let proc_dir = proc_without_steam();
        let changed = set_wrapped(&root, proc_dir.path(), &[1091500, 570], true).unwrap();
        assert_eq!(changed, 4);
        let games = installed_games(&root);
        assert!(games.iter().find(|game| game.app_id == 1091500).unwrap().wrapped);
        assert!(games.iter().find(|game| game.app_id == 570).unwrap().wrapped);
        assert!(!games.iter().find(|game| game.app_id == 1245620).unwrap().wrapped);

        // Le reste du fichier ne bouge pas, et l'original est gardé.
        let path = root.join("userdata/12345678/config/localconfig.vdf");
        let file = vdf::parse(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(file.path(&["UserLocalConfigStore", "friends"]).unwrap().text("PersonaName"), Some("joueur"));
        let app = file.path(&["UserLocalConfigStore", "Software", "Valve", "Steam", "apps", "1091500"]).unwrap();
        assert_eq!(app.text("LastPlayed"), Some("1760000000"));
        let expected = format!("PROTON_LOG=1 {} %command% -dx12", wrapper());
        assert_eq!(app.text("LaunchOptions"), Some(expected.as_str()));
        assert!(path.with_extension("vdf.last1-panel.bak").is_file());

        set_wrapped(&root, proc_dir.path(), &[1091500, 570], false).unwrap();
        let games = installed_games(&root);
        let cyberpunk = games.iter().find(|game| game.app_id == 1091500).unwrap();
        assert_eq!(cyberpunk.launch_options, "PROTON_LOG=1 %command% -dx12");
        assert!(!cyberpunk.wrapped);
    }

    #[test]
    fn refuses_while_steam_runs() {
        let home = steam();
        let root = find_root(home.path()).unwrap();
        let proc_dir = proc_without_steam();
        fs::create_dir_all(proc_dir.path().join("4242")).unwrap();
        fs::write(proc_dir.path().join("4242/comm"), "steam\n").unwrap();
        assert!(is_running(proc_dir.path()));
        assert!(set_wrapped(&root, proc_dir.path(), &[570], true).is_err());
        assert!(!installed_games(&root).iter().any(|game| game.wrapped));
    }

    #[test]
    fn no_steam_no_games() {
        let home = tempfile::tempdir().unwrap();
        assert!(find_root(home.path()).is_none());
        assert!(installed_games(home.path()).is_empty());
    }
}
