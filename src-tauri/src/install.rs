//! L'AppImage s'installe elle-même, comme SoundFence : le joueur télécharge un
//! fichier et l'ouvre une fois. Last1 Panel se copie dans
//! `~/.local/bin/last1-panel`, pose son icône et son entrée du menu, l'outil
//! des jeux et nvibrant dans `~/.local/lib/last1-panel`, et active le service
//! de session qui remet les réglages à chaque connexion. Une AppImage plus
//! récente ouverte plus tard met tout à jour. Rien ne demande de mot de passe.
//!
//! Installé par le paquet Arch (go-to-linux le pose d'office), Last1 Panel est
//! dans /usr/bin : l'AppImage lui passe alors la main sans rien installer.

use std::fs;
use std::io::{self, Read};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// Last1 Panel du paquet Arch.
pub const SYSTEM_BINARY: &str = "/usr/bin/last1-panel";
const SERVICE: &str = "last1-panel-apply.service";
const ICON: &[u8] = include_bytes!("../icons/icon.png");
/// Mêmes textes que l'entrée du menu et le service du paquet.
const DESKTOP_ENTRY: &str = include_str!("../../packaging/last1-panel.desktop");
const SERVICE_UNIT: &str = include_str!("../../packaging/last1-panel-apply.service");

/// Où vont les fichiers. Les noms sont ceux du paquet : l'entrée du menu et
/// l'icône de l'utilisateur prennent le pas sur celles du système.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub binary: PathBuf,
    /// `last1-panel-cli` à côté, pour le terminal (lien vers l'outil des jeux).
    pub command: PathBuf,
    pub desktop_entry: PathBuf,
    pub icon: PathBuf,
    /// Outil des jeux et nvibrant.
    pub lib: PathBuf,
    /// Services systemd de l'utilisateur.
    pub units: PathBuf,
}

impl Layout {
    pub fn for_user() -> Option<Self> {
        let home = std::env::var_os("HOME").map(PathBuf::from).filter(|p| p.is_absolute())?;
        let xdg = |var: &str, fallback: &str| {
            std::env::var_os(var).map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(|| home.join(fallback))
        };
        Some(Self::under(&home, &xdg("XDG_DATA_HOME", ".local/share"), &xdg("XDG_CONFIG_HOME", ".config")))
    }

    fn under(home: &Path, data: &Path, config: &Path) -> Self {
        Self {
            binary: home.join(".local/bin/last1-panel"),
            command: home.join(".local/bin/last1-panel-cli"),
            desktop_entry: data.join("applications/last1-panel.desktop"),
            icon: data.join("icons/hicolor/512x512/apps/last1-panel.png"),
            lib: home.join(".local/lib/last1-panel"),
            units: config.join("systemd/user"),
        }
    }

    pub fn cli(&self) -> PathBuf {
        self.lib.join("last1-panel-cli")
    }

    fn unit(&self) -> PathBuf {
        self.units.join(SERVICE)
    }

    fn unit_link(&self) -> PathBuf {
        self.units.join("default.target.wants").join(SERVICE)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Première ouverture de l'AppImage : Last1 Panel est dans le menu.
    Installed,
    /// Une AppImage plus récente a remplacé celle installée.
    Updated,
    /// Déjà installé, rien n'a changé.
    UpToDate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Report {
    pub outcome: Outcome,
    /// Outil ou service changé : le service est à relancer.
    pub restart_service: bool,
}

/// L'AppImage lancée et son dossier monté, posés par son lanceur.
pub fn running_appimage() -> Option<(PathBuf, PathBuf)> {
    let file = std::env::var_os("APPIMAGE").map(PathBuf::from).filter(|p| p.is_absolute() && p.is_file())?;
    let dir = std::env::var_os("APPDIR").map(PathBuf::from).filter(|p| p.is_absolute() && p.is_dir())?;
    Some((file, dir))
}

/// Installe ou met à jour Last1 Panel depuis l'AppImage `appimage`, dont le
/// contenu est monté dans `appdir`.
pub fn integrate(appimage: &Path, appdir: &Path, layout: &Layout) -> io::Result<Report> {
    let existed = layout.binary.is_file();
    let outcome = if sync_file(appimage, &layout.binary, 0o755)? {
        if existed { Outcome::Updated } else { Outcome::Installed }
    } else {
        Outcome::UpToDate
    };

    let mut restart_service = sync_file(&appdir.join("usr/bin/last1-panel-cli"), &layout.cli(), 0o755)?;
    sync_dir(&appdir.join("usr/lib/last1-panel/nvibrant"), &layout.lib.join("nvibrant"))?;
    if fs::read_link(&layout.command).ok() != Some(layout.cli()) {
        let _ = fs::remove_file(&layout.command);
        std::os::unix::fs::symlink(layout.cli(), &layout.command)?;
    }

    write_if_changed(&layout.icon, ICON)?;
    write_if_changed(&layout.desktop_entry, desktop_entry(&layout.binary).as_bytes())?;
    restart_service |= write_if_changed(&layout.unit(), service_unit(&layout.cli()).as_bytes())?;
    let link = layout.unit_link();
    if fs::symlink_metadata(&link).is_err() {
        fs::create_dir_all(link.parent().expect("dossier des services"))?;
        std::os::unix::fs::symlink(format!("../{SERVICE}"), &link)?;
        restart_service = true;
    }
    Ok(Report { outcome, restart_service })
}

/// Retire ce que l'AppImage avait posé et qui cacherait Last1 Panel du
/// paquet (entrée du menu, icône, service, copie de l'AppImage). L'outil des
/// jeux reste : des options de lancement Steam peuvent encore l'appeler.
pub fn yield_to_package(layout: &Layout) -> io::Result<bool> {
    let mut removed = false;
    for path in
        [&layout.desktop_entry, &layout.icon, &layout.unit_link(), &layout.unit(), &layout.command, &layout.binary]
    {
        match fs::remove_file(path) {
            Ok(()) => removed = true,
            Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
            Err(_) => {}
        }
    }
    Ok(removed)
}

fn desktop_entry(binary: &Path) -> String {
    // Entre guillemets, selon la spécification (espaces, guillemets, `$`, `\`).
    let exec: String = binary
        .display()
        .to_string()
        .chars()
        .flat_map(|c| match c {
            '"' | '`' | '$' | '\\' => vec!['\\', c],
            c => vec![c],
        })
        .collect();
    DESKTOP_ENTRY.replace("\nExec=last1-panel\n", &format!("\nExec=\"{exec}\"\n"))
}

fn service_unit(cli: &Path) -> String {
    // Chemin entre guillemets ; `%` est un spécificateur pour systemd.
    let path = cli.display().to_string().replace('%', "%%").replace('\\', "\\\\").replace('"', "\\\"");
    SERVICE_UNIT
        .replace("Activé pour tous les comptes par le paquet", "Activé pour ce compte par l'AppImage")
        .replace("ExecStart=/usr/bin/last1-panel-cli watch", &format!("ExecStart=\"{path}\" watch"))
}

/// Copie `source` sur `target` s'ils diffèrent ; vrai si copié.
fn sync_file(source: &Path, target: &Path, mode: u32) -> io::Result<bool> {
    let same_file = fs::canonicalize(source).ok().is_some_and(|s| fs::canonicalize(target).ok() == Some(s));
    if same_file || (target.is_file() && same_content(source, target)?) {
        return Ok(false);
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    // Copie à côté puis renommage : une copie en cours d'exécution continue de
    // tourner, et une copie ratée ne laisse jamais de fichier cassé.
    let temporary = target.with_extension("download");
    let _ = fs::remove_file(&temporary);
    let result = fs::copy(source, &temporary)
        .and_then(|_| fs::set_permissions(&temporary, fs::Permissions::from_mode(mode)))
        .and_then(|()| fs::rename(&temporary, target));
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map(|()| true)
}

/// Dossier `target` identique à `source` (fichiers exécutables).
fn sync_dir(source: &Path, target: &Path) -> io::Result<()> {
    if !source.is_dir() {
        return Ok(());
    }
    fs::create_dir_all(target)?;
    let mut names = Vec::new();
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            sync_file(&entry.path(), &target.join(entry.file_name()), 0o755)?;
            names.push(entry.file_name());
        }
    }
    for entry in fs::read_dir(target)? {
        let entry = entry?;
        if !names.contains(&entry.file_name()) && entry.file_type()?.is_file() {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

/// Écrit `content` s'il diffère ; vrai si écrit.
fn write_if_changed(path: &Path, content: &[u8]) -> io::Result<bool> {
    if fs::read(path).is_ok_and(|current| current == content) {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, content)?;
    fs::rename(&temporary, path)?;
    Ok(true)
}

fn same_content(a: &Path, b: &Path) -> io::Result<bool> {
    if fs::metadata(a)?.len() != fs::metadata(b)?.len() {
        return Ok(false);
    }
    let (mut a, mut b) = (fs::File::open(a)?, fs::File::open(b)?);
    let (mut left, mut right) = (vec![0u8; 1 << 20], vec![0u8; 1 << 20]);
    loop {
        let read = a.read(&mut left)?;
        if read == 0 {
            return Ok(true);
        }
        b.read_exact(&mut right[..read])?;
        if left[..read] != right[..read] {
            return Ok(false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// AppImage factice : le fichier et son contenu monté.
    fn setup() -> (tempfile::TempDir, Layout, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let layout = Layout::under(&home, &home.join(".local/share"), &home.join(".config"));
        let appimage = dir.path().join("Téléchargements/Last1Panel_0.1.0.r9_amd64.AppImage");
        fs::create_dir_all(appimage.parent().unwrap()).unwrap();
        fs::write(&appimage, b"appimage v1").unwrap();
        let appdir = dir.path().join("mount");
        fs::create_dir_all(appdir.join("usr/bin")).unwrap();
        fs::create_dir_all(appdir.join("usr/lib/last1-panel/nvibrant")).unwrap();
        fs::write(appdir.join("usr/bin/last1-panel-cli"), b"cli v1").unwrap();
        fs::write(appdir.join("usr/lib/last1-panel/nvibrant/nvibrant-570.86.16"), b"nv570").unwrap();
        (dir, layout, appimage, appdir)
    }

    #[test]
    fn installs_then_updates() {
        let (_dir, layout, appimage, appdir) = setup();
        let report = integrate(&appimage, &appdir, &layout).unwrap();
        assert_eq!(report, Report { outcome: Outcome::Installed, restart_service: true });
        assert_eq!(fs::read(&layout.binary).unwrap(), b"appimage v1");
        assert_eq!(fs::metadata(&layout.binary).unwrap().permissions().mode() & 0o777, 0o755);
        assert_eq!(fs::read(layout.cli()).unwrap(), b"cli v1");
        assert_eq!(fs::metadata(layout.cli()).unwrap().permissions().mode() & 0o777, 0o755);
        assert_eq!(fs::read(layout.lib.join("nvibrant/nvibrant-570.86.16")).unwrap(), b"nv570");
        assert_eq!(fs::read(&layout.icon).unwrap(), ICON);

        let entry = fs::read_to_string(&layout.desktop_entry).unwrap();
        assert!(entry.contains(&format!("\nExec=\"{}\"\n", layout.binary.display())));
        assert!(entry.contains("Icon=last1-panel") && entry.contains("Name=Last1 Panel"));
        let unit = fs::read_to_string(layout.unit()).unwrap();
        assert!(unit.contains(&format!("ExecStart=\"{}\" watch", layout.cli().display())));
        assert!(!unit.contains("/usr/bin"));
        assert_eq!(fs::read_link(layout.unit_link()).unwrap(), PathBuf::from("../last1-panel-apply.service"));
        assert_eq!(fs::read(&layout.command).unwrap(), b"cli v1", "commande du terminal");

        // Rouverte, ou lancée depuis le menu (la copie installée est sa source).
        assert_eq!(
            integrate(&appimage, &appdir, &layout).unwrap(),
            Report { outcome: Outcome::UpToDate, restart_service: false }
        );
        assert_eq!(integrate(&layout.binary, &appdir, &layout).unwrap().outcome, Outcome::UpToDate);

        // Nouvelle version : AppImage, outil et nvibrant remplacés.
        fs::write(&appimage, b"appimage v2").unwrap();
        fs::write(appdir.join("usr/bin/last1-panel-cli"), b"cli v2").unwrap();
        fs::remove_file(appdir.join("usr/lib/last1-panel/nvibrant/nvibrant-570.86.16")).unwrap();
        fs::write(appdir.join("usr/lib/last1-panel/nvibrant/nvibrant-580.76.05"), b"nv580").unwrap();
        assert_eq!(
            integrate(&appimage, &appdir, &layout).unwrap(),
            Report { outcome: Outcome::Updated, restart_service: true }
        );
        assert_eq!(fs::read(&layout.binary).unwrap(), b"appimage v2");
        assert_eq!(fs::read(layout.cli()).unwrap(), b"cli v2");
        assert!(!layout.lib.join("nvibrant/nvibrant-570.86.16").exists());
        assert!(layout.lib.join("nvibrant/nvibrant-580.76.05").is_file());
    }

    #[test]
    fn package_takes_over() {
        let (_dir, layout, appimage, appdir) = setup();
        integrate(&appimage, &appdir, &layout).unwrap();
        assert!(yield_to_package(&layout).unwrap());
        for path in
            [&layout.binary, &layout.command, &layout.desktop_entry, &layout.icon, &layout.unit(), &layout.unit_link()]
        {
            assert!(fs::symlink_metadata(path).is_err(), "{} reste", path.display());
        }
        // L'outil reste pour les options de lancement Steam déjà écrites.
        assert!(layout.cli().is_file());
        assert!(appimage.is_file(), "le fichier téléchargé n'est pas touché");
        assert!(!yield_to_package(&layout).unwrap());
    }

    #[test]
    fn paths_are_quoted() {
        let entry = desktop_entry(Path::new("/home/a b/$x/last1\"panel"));
        assert!(entry.contains(r#"Exec="/home/a b/\$x/last1\"panel""#));
        let unit = service_unit(Path::new("/home/a b/100%/last1-panel-cli"));
        assert!(unit.contains(r#"ExecStart="/home/a b/100%%/last1-panel-cli" watch"#));
    }
}
