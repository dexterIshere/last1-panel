//! Clips des parties : c'est Vice (splitshotz/Vice, GPL-3.0) qui les fait,
//! installé d'office par go-to-linux. La page Clips y mène : l'ouvrir, le
//! démarrer s'il est arrêté, ou dire où l'installer. Last1 Panel n'enregistre
//! rien lui-même (un seul enregistreur, un seul encodeur).

use crate::exec::{Exec, which};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Programme de go-to-linux (sa rubrique Applications installe Vice).
pub const GO_TO_LINUX: &str = "/usr/local/lib/gtl/session/go-to-linux";
/// Page de Vice.
pub const VICE_URL: &str = "https://github.com/splitshotz/Vice";

/// Dossier des clips de Vice (sa configuration par défaut).
pub fn folder(home: &Path) -> PathBuf {
    home.join("Videos/Vice")
}

/// État de la page Clips.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipsState {
    /// Vice installé.
    pub installed: bool,
    /// Son service (vice.service) tourne : les touches marchent.
    pub running: bool,
    /// PC installé par go-to-linux : Vice s'installe par sa rubrique
    /// Applications.
    pub go_to_linux: bool,
    pub folder: String,
}

pub fn installed() -> bool {
    which("vice").is_some() || which("vice-app").is_some() || Path::new("/usr/share/applications/vice.desktop").exists()
}

pub fn state(exec: &dyn Exec, home: &Path) -> ClipsState {
    let running = exec
        .run("systemctl", &["--user".into(), "is-active".into(), "vice.service".into()])
        .is_ok_and(|output| output.stdout.trim() == "active");
    ClipsState {
        installed: installed(),
        running,
        go_to_linux: Path::new(GO_TO_LINUX).exists(),
        folder: folder(home).to_string_lossy().into_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::FakeExec;

    #[test]
    fn vice_service_state_is_read() {
        let home = Path::new("/home/j");
        let running = FakeExec::new().on("systemctl", "active\n");
        assert!(state(&running, home).running);
        let stopped = FakeExec::new().on("systemctl", "inactive\n");
        assert!(!state(&stopped, home).running);
        assert_eq!(state(&stopped, home).folder, "/home/j/Videos/Vice");
    }
}
