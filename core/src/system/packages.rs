//! Paquets du système (pacman : Arch, CachyOS) : ceux qui ont une version plus
//! récente dans les dépôts, et les pilotes graphiques dont les jeux ont besoin.
//! Le service de la session demande les mises à jour à `checkupdates`
//! (pacman-contrib, sans mot de passe, sur une copie des listes de paquets)
//! toutes les 6 h et les garde ici ; le diagnostic les relit aussitôt, sinon
//! demande à `pacman -Qu` (listes locales, à jour après un `pacman -Syu`).

use crate::exec::Exec;
use crate::system::gpu::{Gpu, Vendor};
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

/// Au-delà, la liste gardée est trop vieille : `pacman -Qu`.
const FRESH: Duration = Duration::from_secs(24 * 3600);

/// Noms des paquets d'une sortie « nom ancienne -> nouvelle ».
fn names(output: &str) -> Vec<String> {
    output.lines().filter_map(|line| line.split_whitespace().next()).map(str::to_string).collect()
}

/// Paquets qui ont une version plus récente : la liste gardée si elle est
/// récente ; sinon refaite tout de suite par `checkupdates` (quelques
/// secondes : le service ne l'a pas encore faite) ; sans lui, `pacman -Qu`.
pub fn upgradable(exec: &dyn Exec, cache: &Path) -> Vec<String> {
    let fresh = |cache: &Path| {
        fs::metadata(cache)
            .and_then(|m| m.modified())
            .is_ok_and(|at| SystemTime::now().duration_since(at).unwrap_or_default() < FRESH)
    };
    if !fresh(cache) && exec.which("checkupdates").is_some() {
        refresh(exec, cache);
    }
    if fresh(cache)
        && let Ok(text) = fs::read_to_string(cache)
    {
        return names(&text);
    }
    exec.run("pacman", &["-Qu".into()]).map(|output| names(&output.stdout)).unwrap_or_default()
}

/// Liste gardée refaite par `checkupdates` (code 0 : des mises à jour ;
/// 2 : aucune ; autre : échec, rien de gardé).
pub fn refresh(exec: &dyn Exec, cache: &Path) {
    let Ok(output) = exec.run("checkupdates", &[]) else { return };
    if output.status == 0 || output.status == 2 {
        if let Some(dir) = cache.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(cache, if output.status == 0 { output.stdout.as_str() } else { "" });
    }
}

/// Paquet du pilote graphique (NVIDIA, ou Mesa pour AMD et Intel).
pub fn is_driver(name: &str) -> bool {
    name.starts_with("nvidia")
        || name.starts_with("lib32-nvidia")
        || name.contains("-nvidia")
        || ["mesa", "lib32-mesa", "vulkan-radeon", "lib32-vulkan-radeon", "vulkan-intel", "lib32-vulkan-intel"]
            .contains(&name)
}

/// Pilotes Vulkan dont les jeux ont besoin (DXVK, VKD3D-Proton), en 64 et
/// 32 bits (Steam, jeux 32 bits), pour les cartes du PC.
pub fn vulkan_packages(gpus: &[Gpu]) -> Vec<&'static str> {
    let mut packages = Vec::new();
    if gpus.iter().any(|g| g.vendor == Vendor::Nvidia) {
        packages.extend(["nvidia-utils", "lib32-nvidia-utils"]);
    }
    if gpus.iter().any(|g| g.vendor == Vendor::Amd) {
        packages.extend(["vulkan-radeon", "lib32-vulkan-radeon"]);
    }
    packages
}

/// Ceux de `packages` qui manquent : `pacman -T` (un paquet qui les fournit,
/// version -git ou branche 580 de NVIDIA, compte).
pub fn missing(exec: &dyn Exec, packages: &[&str]) -> Vec<String> {
    if packages.is_empty() {
        return Vec::new();
    }
    let args: Vec<String> = std::iter::once("-T".to_string()).chain(packages.iter().map(|p| p.to_string())).collect();
    exec.run("pacman", &args).map(|output| names(&output.stdout)).unwrap_or_default()
}

/// Version installée d'un paquet (« 595.58.03 », sans la révision « -1 »).
pub fn installed_version(exec: &dyn Exec, package: &str) -> Option<String> {
    let output = exec.run("pacman", &["-Q".into(), package.into()]).ok().filter(|o| o.success())?;
    let version = output.stdout.split_whitespace().nth(1)?;
    let version = version.rsplit_once('-').map_or(version, |(v, _)| v);
    // Époque éventuelle : « 1:25.2.4 ».
    Some(version.rsplit_once(':').map_or(version, |(_, v)| v).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::{FakeExec, Output};

    #[test]
    fn upgradable_from_the_kept_list_or_pacman() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("updates.txt");
        let pacman = FakeExec::new().without("checkupdates").on("pacman", "mesa 1:25.2.3-1 -> 1:25.2.4-1\n");
        assert_eq!(upgradable(&pacman, &cache), ["mesa"]);
        // checkupdates : gardé, puis relu sans pacman.
        let check = FakeExec::new().on("checkupdates", "nvidia-utils 595.58.03-1 -> 600.10-1\nfirefox 1 -> 2\n");
        refresh(&check, &cache);
        assert_eq!(upgradable(&FakeExec::new().without("checkupdates"), &cache), ["nvidia-utils", "firefox"]);
        // Pas de liste gardée : checkupdates tout de suite.
        let fresh = dir.path().join("neuve.txt");
        let check = FakeExec::new().on("checkupdates", "mesa 1 -> 2\n");
        assert_eq!(upgradable(&check, &fresh), ["mesa"]);
        // Aucune mise à jour (code 2) : liste vide gardée.
        let none = FakeExec::new().on_args("checkupdates", &[], Output { status: 2, ..Default::default() });
        refresh(&none, &cache);
        assert!(upgradable(&FakeExec::new(), &cache).is_empty());
        // Échec (pas d'Internet) : l'ancienne liste reste.
        let failed = FakeExec::new().on_args("checkupdates", &[], Output { status: 1, ..Default::default() });
        refresh(&failed, &cache);
        assert!(cache.exists());
    }

    #[test]
    fn driver_packages_and_versions() {
        assert!(is_driver("nvidia-utils") && is_driver("lib32-nvidia-utils") && is_driver("linux-cachyos-nvidia-open"));
        assert!(is_driver("mesa") && is_driver("lib32-vulkan-radeon"));
        assert!(!is_driver("firefox") && !is_driver("mesa-demos"));
        let exec = FakeExec::new()
            .on_args(
                "pacman",
                &["-Q", "nvidia-utils"],
                Output { status: 0, stdout: "nvidia-utils 595.58.03-2\n".into(), ..Default::default() },
            )
            .on_args(
                "pacman",
                &["-Q", "mesa"],
                Output { status: 0, stdout: "mesa 1:25.2.4-1\n".into(), ..Default::default() },
            )
            .on_args(
                "pacman",
                &["-T", "vulkan-radeon", "lib32-vulkan-radeon"],
                Output { status: 127, stdout: "lib32-vulkan-radeon\n".into(), ..Default::default() },
            );
        assert_eq!(installed_version(&exec, "nvidia-utils").as_deref(), Some("595.58.03"));
        assert_eq!(installed_version(&exec, "mesa").as_deref(), Some("25.2.4"));
        assert_eq!(missing(&exec, &["vulkan-radeon", "lib32-vulkan-radeon"]), ["lib32-vulkan-radeon"]);
    }
}
