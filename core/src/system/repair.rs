//! Corrections du système proposées par le diagnostic, d'un clic : les
//! réglages que go-to-linux pose à l'installation, dans des fichiers à nous
//! (`…last1-panel…`). Tout ce qui demande les droits d'administrateur passe
//! par un seul script, lancé par pkexec : le mot de passe est demandé une
//! fois, même pour « Tout régler ».

use crate::error::{CoreError, CoreResult};
use crate::i18n::t;
use crate::system::diagnostic::Fix;
use std::path::Path;

const ZRAM_GENERATOR: &str = "/usr/lib/systemd/system-generators/zram-generator";

/// Script d'administrateur pour `fixes` (rien s'il n'y a rien à faire en
/// administrateur). `etc` : /etc, ou un dossier de test. `power_profiles` :
/// power-profiles-daemon présent (le mode performance passe alors par lui,
/// sans mot de passe) ; `pacman` : MangoHud s'installe par lui.
pub fn root_script(fixes: &[Fix], etc: &Path, power_profiles: bool, pacman: bool) -> CoreResult<Option<String>> {
    let etc = etc.to_string_lossy();
    let mut parts: Vec<String> = Vec::new();
    for fix in fixes {
        match fix {
            // Chargé à chaque démarrage, ouvert à tous les comptes (Proton
            // l'ouvre sous le compte du joueur).
            Fix::Ntsync => parts.push(format!(
                r#"mkdir -p {etc}/modules-load.d {etc}/udev/rules.d
echo ntsync > {etc}/modules-load.d/last1-panel-ntsync.conf
echo 'KERNEL=="ntsync", MODE="0666"' > {etc}/udev/rules.d/99-last1-panel-ntsync.rules
modprobe ntsync
chmod 0666 /dev/ntsync"#
            )),
            // 12 Go pour NVIDIA et pour Mesa (AMD, Intel), comme go-to-linux ;
            // les valeurs d'avant sont remplacées. Lu à la connexion : par
            // pam_env (/etc/environment) et par le gestionnaire systemd de la
            // session (environment.d : uwsm, Plasma, programmes lancés par
            // systemd), quel que soit le chemin par lequel Steam démarre.
            Fix::ShaderCache => parts.push(format!(
                r#"touch {etc}/environment
if [ -s {etc}/environment ] && [ -n "$(tail -c 1 {etc}/environment)" ]; then echo >> {etc}/environment; fi
sed -i -e '/^# Last1 Panel : cache des shaders/d' -e '/^[[:space:]]*\(export[[:space:]]\{{1,\}}\)\{{0,1\}}__GL_SHADER_DISK_CACHE_SIZE=/d' -e '/^[[:space:]]*\(export[[:space:]]\{{1,\}}\)\{{0,1\}}MESA_SHADER_CACHE_MAX_SIZE=/d' {etc}/environment
printf '%s\n' '# Last1 Panel : cache des shaders de 12 Go' '__GL_SHADER_DISK_CACHE_SIZE=12000000000' 'MESA_SHADER_CACHE_MAX_SIZE=12G' >> {etc}/environment
mkdir -p {etc}/environment.d
printf '%s\n' '__GL_SHADER_DISK_CACHE_SIZE=12000000000' 'MESA_SHADER_CACHE_MAX_SIZE=12G' > {etc}/environment.d/90-last1-panel-shader-cache.conf"#
            )),
            // Comme SteamOS.
            Fix::SplitLock => parts.push(format!(
                r#"mkdir -p {etc}/sysctl.d
echo 'kernel.split_lock_mitigate = 0' > {etc}/sysctl.d/99-last1-panel-split-lock.conf
sysctl -q -w kernel.split_lock_mitigate=0"#
            )),
            // À chaque démarrage (règle udev, dès que le pilote est là), et
            // tout de suite.
            Fix::X3dCache => parts.push(format!(
                r#"mkdir -p {etc}/udev/rules.d
echo 'ACTION=="add|bind", SUBSYSTEM=="platform", DRIVER=="amd_x3d_vcache", ATTR{{amd_x3d_mode}}="cache"' > {etc}/udev/rules.d/99-last1-panel-x3d.rules
for f in /sys/bus/platform/drivers/amd_x3d_vcache/*/amd_x3d_mode; do if [ -e "$f" ]; then echo cache > "$f"; fi; done"#
            )),
            // L'ordonnanceur du noyau : plus de sched_ext (scx_loader de
            // CachyOS, ou scx.service d'Arch), ni au démarrage ni maintenant.
            Fix::Scheduler => parts.push(
                r#"for unit in scx_loader.service scx.service; do systemctl disable --now "$unit" 2>/dev/null || true; done
pkill -x 'scx_.+' || true"#
                    .to_string(),
            ),
            // Sans power-profiles-daemon : le gouverneur, à chaque démarrage
            // (tmpfiles) et tout de suite. Le diagnostic ne le propose pas
            // sur un portable.
            Fix::Performance if !power_profiles => parts.push(format!(
                r#"mkdir -p {etc}/tmpfiles.d
echo 'w /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor - - - - performance' > {etc}/tmpfiles.d/last1-panel-performance.conf
for f in /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor; do if [ -e "$f" ]; then echo performance > "$f"; fi; done"#
            )),
            // lib32-mangohud : jeux 32 bits ; sans le dépôt multilib, MangoHud
            // seul ; liste des paquets trop ancienne : avec une mise à jour du
            // système (comme pour LACT).
            Fix::InstallMangoHud if pacman => parts.push(
                "pacman -S --needed --noconfirm mangohud lib32-mangohud || pacman -S --needed --noconfirm mangohud || pacman -Syu --needed --noconfirm mangohud"
                    .to_string(),
            ),
            Fix::InstallMangoHud => {
                return Err(CoreError::message(t(
                    "Installez MangoHud avec le gestionnaire de paquets du système.",
                    "Install MangoHud with the system's package manager.",
                )));
            }
            // Mémoire compressée, comme CachyOS (zram de la taille de la RAM,
            // zstd, avant l'échange sur disque) ; la configuration de
            // zram-generator déjà là est gardée. Le service de préparation ne
            // fait que créer le périphérique : c'est dev-zram0.swap qui le met
            // en service (swapon). Vérifié à la fin : sinon, une erreur.
            Fix::Zram => {
                if !pacman && !Path::new(ZRAM_GENERATOR).exists() {
                    return Err(CoreError::message(t(
                        "Installez zram-generator avec le gestionnaire de paquets du système.",
                        "Install zram-generator with the system's package manager.",
                    )));
                }
                parts.push(format!(
                    r#"if [ ! -e {ZRAM_GENERATOR} ]; then pacman -S --needed --noconfirm zram-generator || pacman -Syu --needed --noconfirm zram-generator; fi
if [ ! -e {etc}/systemd/zram-generator.conf ]; then mkdir -p {etc}/systemd; printf '%s\n' '[zram0]' 'zram-size = ram' 'compression-algorithm = zstd' 'swap-priority = 100' > {etc}/systemd/zram-generator.conf; fi
mkdir -p {etc}/sysctl.d
printf '%s\n' 'vm.swappiness = 150' 'vm.page-cluster = 0' > {etc}/sysctl.d/99-last1-panel-zram.conf
sysctl -q -p {etc}/sysctl.d/99-last1-panel-zram.conf
systemctl daemon-reload
systemctl start dev-zram0.swap || systemctl start swap.target
if ! grep -q '^/dev/zram' /proc/swaps; then echo "zram ne s'est pas activé (zramctl, journalctl -b -u systemd-zram-setup@zram0)" >&2; exit 1; fi"#
                ));
            }
            Fix::Performance | Fix::LowLatency | Fix::WrapGames | Fix::Restart => {}
        }
    }
    Ok((!parts.is_empty()).then(|| format!("set -e\n{}\n", parts.join("\n"))))
}

/// pkexec refusé (fenêtre de mot de passe fermée) : rien n'a changé.
pub fn refused(status: i32) -> bool {
    status == 126 || status == 127
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;

    const ALL: [Fix; 11] = [
        Fix::Performance,
        Fix::Ntsync,
        Fix::ShaderCache,
        Fix::SplitLock,
        Fix::X3dCache,
        Fix::Scheduler,
        Fix::InstallMangoHud,
        Fix::LowLatency,
        Fix::WrapGames,
        Fix::Zram,
        Fix::Restart,
    ];

    #[test]
    fn one_script_for_every_system_fix() {
        let script = root_script(&ALL, Path::new("/etc"), false, true).unwrap().unwrap();
        for expected in [
            "/etc/modules-load.d/last1-panel-ntsync.conf",
            "/etc/udev/rules.d/99-last1-panel-ntsync.rules",
            "__GL_SHADER_DISK_CACHE_SIZE=12000000000",
            "/etc/environment.d/90-last1-panel-shader-cache.conf",
            "/etc/sysctl.d/99-last1-panel-split-lock.conf",
            "/etc/udev/rules.d/99-last1-panel-x3d.rules",
            "systemctl disable --now",
            "/etc/tmpfiles.d/last1-panel-performance.conf",
            "pacman -S --needed --noconfirm mangohud",
            "/etc/systemd/zram-generator.conf",
            "systemctl start dev-zram0.swap",
        ] {
            assert!(script.contains(expected), "{expected}\n{script}");
        }
        // Le shell le lit sans erreur.
        let check = Command::new("/bin/sh").args(["-n", "-c", &script]).status().unwrap();
        assert!(check.success());
        // power-profiles-daemon : le mode performance passe par lui.
        let script = root_script(&[Fix::Performance], Path::new("/etc"), true, true).unwrap();
        assert_eq!(script, None);
        // Réglages de Last1 Panel seuls : pas de mot de passe.
        assert_eq!(root_script(&[Fix::LowLatency, Fix::WrapGames], Path::new("/etc"), false, true).unwrap(), None);
        // Sans pacman, MangoHud ne s'installe pas d'ici.
        assert!(root_script(&[Fix::InstallMangoHud], Path::new("/etc"), false, false).is_err());
    }

    #[test]
    fn shader_cache_replaces_old_values_once() {
        let dir = tempfile::tempdir().unwrap();
        let environment = dir.path().join("environment");
        // Sans retour à la ligne final, avec une ancienne valeur.
        fs::write(&environment, "PATH=/usr/bin\nexport __GL_SHADER_DISK_CACHE_SIZE=1000\nEDITOR=nano").unwrap();
        let script = root_script(&[Fix::ShaderCache], dir.path(), false, true).unwrap().unwrap();
        for _ in 0..2 {
            assert!(Command::new("/bin/sh").args(["-c", &script]).status().unwrap().success());
        }
        let text = fs::read_to_string(&environment).unwrap();
        assert_eq!(text.matches("# Last1 Panel").count(), 1);
        assert_eq!(
            text.lines().filter(|l| !l.starts_with('#')).collect::<Vec<_>>(),
            [
                "PATH=/usr/bin",
                "EDITOR=nano",
                "__GL_SHADER_DISK_CACHE_SIZE=12000000000",
                "MESA_SHADER_CACHE_MAX_SIZE=12G"
            ]
        );
        assert_eq!(
            crate::system::diagnostic::environment_value(&text, "MESA_SHADER_CACHE_MAX_SIZE").as_deref(),
            Some("12G")
        );
        // Et pour le gestionnaire systemd de la session.
        let session = fs::read_to_string(dir.path().join("environment.d/90-last1-panel-shader-cache.conf")).unwrap();
        assert_eq!(session, "__GL_SHADER_DISK_CACHE_SIZE=12000000000\nMESA_SHADER_CACHE_MAX_SIZE=12G\n");
    }
}
