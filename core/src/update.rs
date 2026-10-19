//! Mise à jour de Last1 Panel installé par l'AppImage
//! (`~/.local/bin/last1-panel`), comme le fait go-to-linux pour le paquet :
//! le service de session (`last1-panel-cli watch`) la sait dès sa
//! publication (attente longue sur last1.fun, wait_for_release) et la
//! propose ; installée seulement au clic de l'utilisateur (notification ou
//! bouton « Mettre à jour » de la fenêtre).
//!
//! Liste publique de last1.fun, signée (Ed25519, même clé que go-to-linux) :
//! seulement une version plus récente que celle installée, AppImage
//! téléchargée, taille et empreinte vérifiées, puis remplacée d'un coup.
//!
//! Paquet Arch posé par go-to-linux : même liste, comparée à la version du
//! paquet ; installé par go-to-linux (`update.sh --last1-panel`, sans mot de
//! passe pour la session active). Paquet sans go-to-linux : pacman.

use crate::error::{CoreError, CoreResult};
use crate::exec::{Exec, SystemExec};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const SITE: &str = "https://last1.fun";
const PRODUCT: &str = "last1-panel";
/// Clé publique de last1.fun (linux/last1-public-key du dépôt go-to-linux).
const PUBLIC_KEY: &str = "LDh9ERH5mg2iTfgydusH9g8mi662SPRDGJRg9aVgl0s=";

/// Version publiée de cette construction (« 0.1.0.r42 »), posée par la CI de
/// l'AppImage ; absente (construction locale, paquet Arch) : pas de mise à
/// jour proposée.
pub const RELEASE: Option<&str> = option_env!("LAST1_PANEL_RELEASE");

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Download {
    pub name: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub appimage: Download,
}

#[derive(Deserialize)]
struct Manifest {
    typ: String,
    prd: String,
    rel: String,
    exp: i64,
    #[serde(default)]
    downloads: Vec<Download>,
}

#[derive(Deserialize)]
struct Response {
    manifest: String,
}

fn refused(detail: &str) -> CoreError {
    CoreError::message(format!(
        "{} {detail}",
        crate::i18n::t("Liste de last1.fun refusée :", "last1.fun list refused:")
    ))
}

/// La mise à jour proposée concerne-t-elle cette installation ?
pub fn enabled() -> bool {
    RELEASE.is_some() && !Path::new(crate::paths::SYSTEM_APP).exists() && crate::paths::user_appimage().is_file()
}

/// Mise à jour de go-to-linux, pour Last1 Panel seul (`--last1-panel`).
pub const GTL_UPDATE: &str = "/usr/local/lib/gtl/update.sh";

/// Cette fenêtre est-elle celle du paquet (/usr/bin) ?
pub fn is_package() -> bool {
    std::env::current_exe().is_ok_and(|exe| exe.starts_with("/usr/bin"))
}

/// Paquet posé par go-to-linux, qui sait le mettre à jour.
pub fn by_go_to_linux() -> bool {
    is_package() && Path::new("/var/lib/gtl").is_dir() && Path::new(GTL_UPDATE).exists()
}

/// Version installée : celle du paquet (« 0.1.0.r52-1 » → « 0.1.0.r52 »),
/// sinon celle de l'AppImage (posée par sa CI).
pub fn installed_version(exec: &dyn Exec) -> Option<String> {
    if !is_package() {
        return RELEASE.map(str::to_string);
    }
    // « /usr/bin/last1-panel is owned by last1-panel-bin 0.1.0.r52-1 »
    let output = exec.run("pacman", &["-Qo".into(), crate::paths::SYSTEM_APP.into()]).ok().filter(|o| o.success())?;
    package_version(output.stdout.split_whitespace().last()?)
}

/// Version d'un paquet sans son époque ni sa révision.
fn package_version(version: &str) -> Option<String> {
    let version = version.rsplit_once('-').map_or(version, |(v, _)| v);
    let version = version.split_once(':').map_or(version, |(_, v)| v);
    (!version.is_empty()).then(|| version.to_string())
}

/// Last1 Panel mis à jour par go-to-linux. Sa réponse : « updated X »,
/// « installed X », « up-to-date », « unpublished », « absent » ; vide : un
/// go-to-linux plus ancien a tout mis à jour (lui, Last1 Panel, le système).
pub fn update_by_go_to_linux(exec: &dyn Exec) -> CoreResult<()> {
    let output = exec.run("pkexec", &[GTL_UPDATE.into(), "--last1-panel".into()])?;
    if output.status == 126 || output.status == 127 {
        return Err(CoreError::message(crate::i18n::t(
            "Mise à jour refusée : rien n'a changé.",
            "Update refused: nothing changed.",
        )));
    }
    let answer = output.stdout.lines().last().unwrap_or_default().trim();
    if !output.success() || answer == "unpublished" || answer == "absent" {
        return Err(CoreError::message(crate::i18n::t(
            "go-to-linux n'a pas pu mettre Last1 Panel à jour (Internet ?). Le détail est dans /var/log/gtl-update.log.",
            "go-to-linux could not update Last1 Panel (Internet?). Details are in /var/log/gtl-update.log.",
        )));
    }
    Ok(())
}

/// Heure actuelle, en secondes (expiration de la liste).
pub fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// Installe `release` : l'AppImage est téléchargée (sauf si elle est déjà là,
/// installation interrompue), puis lancée en `--integrate` : elle repose
/// elle-même l'outil des jeux et nvibrant, sans fenêtre ; extraite le temps
/// de l'installation, sans besoin de FUSE.
pub fn install(release: &Release) -> CoreResult<()> {
    let appimage = crate::paths::user_appimage();
    if sha256(&appimage).ok().as_deref() != Some(release.appimage.sha256.as_str()) {
        let exec = SystemExec { timeout: Duration::from_secs(3700), ..SystemExec::default() };
        download(&exec, release, &appimage)?;
    }
    let mut command = Command::new(&appimage);
    command.arg("--integrate").stdin(Stdio::null());
    crate::exec::without_appimage_env(&mut command);
    if !command.env("APPIMAGE_EXTRACT_AND_RUN", "1").status()?.success() {
        return Err(CoreError::message(crate::i18n::t(
            "la nouvelle version ne s'est pas installée",
            "the new version did not install",
        )));
    }
    Ok(())
}

/// Charge d'un jeton « charge.signature » (base64url) signé par `key`
/// (32 octets en base64).
pub fn verify(token: &str, key: &str) -> CoreResult<Vec<u8>> {
    let (payload, signature) = token.split_once('.').ok_or_else(|| refused("format"))?;
    let payload = URL_SAFE_NO_PAD.decode(payload).map_err(|_| refused("format"))?;
    let signature = URL_SAFE_NO_PAD.decode(signature).map_err(|_| refused("format"))?;
    let signature = Signature::from_slice(&signature).map_err(|_| refused("signature"))?;
    let key: [u8; 32] =
        STANDARD.decode(key).ok().and_then(|bytes| bytes.try_into().ok()).ok_or_else(|| refused("clé"))?;
    let key = VerifyingKey::from_bytes(&key).map_err(|_| refused("clé"))?;
    key.verify_strict(&payload, &signature).map_err(|_| refused("signature"))?;
    Ok(payload)
}

/// Version publiée et son AppImage, d'après la charge vérifiée.
pub fn parse_release(payload: &[u8], now: i64) -> CoreResult<Release> {
    let manifest: Manifest = serde_json::from_slice(payload).map_err(|_| refused("contenu"))?;
    if manifest.typ != "manifest" || manifest.prd != PRODUCT {
        return Err(refused("pas celle de Last1 Panel"));
    }
    if manifest.exp <= now {
        return Err(refused(&crate::i18n::t("expirée (heure du PC ?)", "expired (PC clock?)")));
    }
    let appimage = manifest
        .downloads
        .into_iter()
        .find(|d| d.name.ends_with(".AppImage") && !d.name.contains('/') && d.sha256.len() == 64)
        .ok_or_else(|| refused("AppImage absente"))?;
    Ok(Release { version: manifest.rel, appimage })
}

/// `candidate` est-elle plus récente que `current` ? Versions « 0.1.0.r42 » :
/// nombres comparés comme des nombres, lettres comme du texte.
pub fn newer(candidate: &str, current: &str) -> bool {
    fn parts(version: &str) -> Vec<Result<u64, String>> {
        let mut parts = Vec::new();
        let mut chars = version.chars().peekable();
        while let Some(&c) = chars.peek() {
            if c.is_ascii_digit() {
                let mut digits = String::new();
                while let Some(&d) = chars.peek().filter(|d| d.is_ascii_digit()) {
                    digits.push(d);
                    chars.next();
                }
                parts.push(Ok(digits.parse().unwrap_or(u64::MAX)));
            } else if c.is_ascii_alphabetic() {
                let mut letters = String::new();
                while let Some(&l) = chars.peek().filter(|l| l.is_ascii_alphabetic()) {
                    letters.push(l);
                    chars.next();
                }
                parts.push(Err(letters));
            } else {
                chars.next();
            }
        }
        parts
    }
    let (a, b) = (parts(candidate), parts(current));
    for (x, y) in a.iter().zip(&b) {
        let order = match (x, y) {
            (Ok(x), Ok(y)) => x.cmp(y),
            (Err(x), Err(y)) => x.cmp(y),
            // Un nombre l'emporte sur des lettres (1.0.1 > 1.0.rc).
            (Ok(_), Err(_)) => Ordering::Greater,
            (Err(_), Ok(_)) => Ordering::Less,
        };
        if order != Ordering::Equal {
            return order == Ordering::Greater;
        }
    }
    a.len() > b.len()
}

/// La version en ligne, si elle est plus récente que celle installée.
pub fn check(exec: &dyn Exec, now: i64) -> CoreResult<Option<Release>> {
    let Some(current) = installed_version(exec) else { return Ok(None) };
    let current = current.as_str();
    let output = exec.run_ok(
        "curl",
        &["-fsS", "--proto", "=https", "--max-time", "60", &format!("{SITE}/api/public/{PRODUCT}/manifest")]
            .map(String::from),
    )?;
    let response: Response = serde_json::from_str(&output.stdout).map_err(|_| refused("réponse"))?;
    let release = parse_release(&verify(&response.manifest, PUBLIC_KEY)?, now)?;
    Ok(newer(&release.version, current).then_some(release))
}

/// Attend qu'une version de Last1 Panel autre que `seen` (la dernière en
/// ligne déjà vue, vide au départ) soit en ligne : last1.fun répond tout de
/// suite si c'est déjà le cas, sinon dès la publication (attente longue,
/// 80 s au plus). Some : la version en ligne ; None : rien de nouveau,
/// redemander aussitôt ; erreur (429 : trop d'attentes ; pas d'Internet) :
/// redemander dans une minute.
pub fn wait_for_release(exec: &dyn Exec, seen: &str) -> CoreResult<Option<String>> {
    let url = format!("{SITE}/api/public/releases/wait?{PRODUCT}={seen}");
    let output = exec.run(
        "curl",
        &["-sS", "--proto", "=https", "--max-time", "100", "-w", "\n%{http_code}", &url].map(String::from),
    )?;
    let (body, status) = output.stdout.trim_end().rsplit_once('\n').unwrap_or(("", output.stdout.trim()));
    match status.trim() {
        "204" => Ok(None),
        "200" => {
            let value: serde_json::Value = serde_json::from_str(body).map_err(|_| refused("réponse"))?;
            value["versions"][PRODUCT]
                .as_str()
                .map(|version| Some(version.to_string()))
                .ok_or_else(|| refused("réponse"))
        }
        other => Err(CoreError::message(format!("last1.fun : {other}"))),
    }
}

/// Télécharge l'AppImage de `release` et remplace `target` d'un coup ; rien
/// n'est remplacé au moindre écart de taille ou d'empreinte.
pub fn download(exec: &dyn Exec, release: &Release, target: &Path) -> CoreResult<()> {
    let temporary = target.with_extension("download");
    // ask=1 : installation demandée par l'utilisateur (clic). last1.fun ne
    // sert plus l'AppImage aux versions qui l'installaient sans demander.
    let url = format!("{SITE}/api/public/{PRODUCT}/files/{}?ask=1", release.appimage.name);
    let fetched = exec
        .run_ok(
            "curl",
            &["-fsS", "--proto", "=https", "--max-time", "3600", "-o", &temporary.to_string_lossy(), &url]
                .map(String::from),
        )
        .map_err(|error| {
            CoreError::message(format!(
                "{} ({error})",
                crate::i18n::t(
                    "Téléchargement de la mise à jour impossible. Si ça continue, retéléchargez Last1 Panel sur last1.fun/last1-panel et ouvrez-le une fois",
                    "The update could not be downloaded. If this keeps happening, download Last1 Panel again from last1.fun/last1-panel and open it once",
                )
            ))
        });
    let result = fetched.and_then(|_| install_download(&temporary, target, &release.appimage));
    let _ = fs::remove_file(&temporary);
    result
}

/// Fichier téléchargé vérifié, rendu exécutable, puis mis à la place de `target`.
pub fn install_download(temporary: &Path, target: &Path, expected: &Download) -> CoreResult<()> {
    let size = fs::metadata(temporary)?.len();
    if size != expected.size || sha256(temporary)? != expected.sha256 {
        return Err(CoreError::message(crate::i18n::t(
            "AppImage téléchargée différente de celle publiée : pas installée.",
            "Downloaded AppImage differs from the published one: not installed.",
        )));
    }
    fs::set_permissions(temporary, fs::Permissions::from_mode(0o755))?;
    fs::rename(temporary, target)?;
    Ok(())
}

/// Empreinte SHA-256 d'un fichier, en hexadécimal.
pub fn sha256(path: &Path) -> CoreResult<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn sign(key: &SigningKey, payload: &str) -> String {
        let signature = key.sign(payload.as_bytes());
        format!("{}.{}", URL_SAFE_NO_PAD.encode(payload), URL_SAFE_NO_PAD.encode(signature.to_bytes()))
    }

    fn manifest(product: &str, exp: i64) -> String {
        format!(
            r#"{{"typ":"manifest","v":1,"prd":"{product}","rel":"0.1.0.r12","mid":"","iat":1,"exp":{exp},"files":[],"downloads":[{{"name":"Last1Panel_0.1.0.r12_amd64.AppImage","size":11,"sha256":"{}"}},{{"name":"last1-panel-0.1.0.r12-1-x86_64.pkg.tar.zst","size":5,"sha256":"{}"}}]}}"#,
            "a".repeat(64),
            "b".repeat(64)
        )
    }

    #[test]
    fn server_tokens_are_verified() {
        // Jeton signé par le serveur Go (même vecteur que les tests de go-to-linux).
        let key = "7y8ObZhLppBDpF+ONDpTyYKSyhUfmEbdRN3SXEsZJ1o=";
        let proof = "eyJ0eXAiOiJhY2Nlc3MiLCJ2IjoxLCJwcmQiOiJnby10by1saW51eCIsInVpZCI6NywiaWF0IjoxODAwMDAwMDAwLCJleHAiOjE4MDA2MDQ4MDB9.h8_Gokv2Hhs2KT-FFspsDXLSoDUmP3bfQcMkDhf_pSaF6nAwF0LQlBDntUGdtTkqznouJMpL1nJVBt6_SNMbDA";
        let payload = verify(proof, key).unwrap();
        assert!(String::from_utf8(payload).unwrap().contains(r#""prd":"go-to-linux""#));
        // Charge modifiée, autre clé, signature tronquée : refusé.
        let forged =
            format!("{}.{}", URL_SAFE_NO_PAD.encode(br#"{"typ":"manifest"}"#), proof.split_once('.').unwrap().1);
        assert!(verify(&forged, key).is_err());
        assert!(verify(proof, PUBLIC_KEY).is_err());
        assert!(verify(&proof[..proof.len() - 6], key).is_err());
        assert!(verify("pas un jeton", key).is_err());
    }

    #[test]
    fn release_comes_from_a_signed_current_list() {
        let signing = SigningKey::from_bytes(&[7; 32]);
        let key = STANDARD.encode(signing.verifying_key().to_bytes());
        let payload = verify(&sign(&signing, &manifest("last1-panel", 2000)), &key).unwrap();
        let release = parse_release(&payload, 1000).unwrap();
        assert_eq!(release.version, "0.1.0.r12");
        assert_eq!(release.appimage.name, "Last1Panel_0.1.0.r12_amd64.AppImage");
        // Expirée, ou liste d'un autre logiciel : refusée.
        assert!(parse_release(&payload, 2000).is_err());
        let other = verify(&sign(&signing, &manifest("soundfence", 2000)), &key).unwrap();
        assert!(parse_release(&other, 1000).is_err());
    }

    #[test]
    fn only_newer_versions_are_installed() {
        assert!(newer("0.1.0.r10", "0.1.0.r9"));
        assert!(newer("0.2.0.r1", "0.1.0.r50"));
        assert!(newer("0.1.0.r9", "0.1.0"));
        assert!(!newer("0.1.0.r9", "0.1.0.r9"));
        assert!(!newer("0.1.0.r8", "0.1.0.r9"));
        assert!(!newer("0.1.0", "0.1.0.r9"));
    }

    #[test]
    fn refused_download_says_what_to_do() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("last1-panel");
        fs::write(&target, b"ancienne").unwrap();
        let name = "Last1Panel_0.1.0.r30_amd64.AppImage";
        let temporary = target.with_extension("download").to_string_lossy().into_owned();
        let url = format!("{SITE}/api/public/{PRODUCT}/files/{name}?ask=1");
        let refused = crate::exec::Output {
            status: 22,
            stdout: String::new(),
            stderr: "curl: (22) The requested URL returned error: 404".into(),
        };
        let exec = crate::exec::FakeExec::new().on_args(
            "curl",
            &["-fsS", "--proto", "=https", "--max-time", "3600", "-o", &temporary, &url],
            refused,
        );
        let release = Release {
            version: "0.1.0.r30".into(),
            appimage: Download { name: name.into(), size: 1, sha256: "0".repeat(64) },
        };
        let error = download(&exec, &release, &target).unwrap_err().to_string();
        assert!(error.contains("last1.fun/last1-panel") && error.contains("404"), "{error}");
        assert_eq!(fs::read(&target).unwrap(), b"ancienne");
    }

    #[test]
    fn downloads_are_checked_before_replacing() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("last1-panel");
        fs::write(&target, b"ancienne").unwrap();
        let temporary = dir.path().join("last1-panel.download");
        fs::write(&temporary, b"appimage v2").unwrap();
        let good = Download {
            name: "Last1Panel_0.1.0.r12_amd64.AppImage".into(),
            size: 11,
            sha256: sha256(&temporary).unwrap(),
        };

        let wrong = Download { sha256: "0".repeat(64), ..good.clone() };
        assert!(install_download(&temporary, &target, &wrong).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"ancienne");
        let short = Download { size: 10, ..good.clone() };
        assert!(install_download(&temporary, &target, &short).is_err());

        install_download(&temporary, &target, &good).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"appimage v2");
        assert_eq!(fs::metadata(&target).unwrap().permissions().mode() & 0o777, 0o755);
        assert!(!temporary.exists());
    }

    #[test]
    fn package_version_without_epoch_or_release() {
        assert_eq!(package_version("0.1.0.r52-1").as_deref(), Some("0.1.0.r52"));
        assert_eq!(package_version("1:0.1.0.r52-3").as_deref(), Some("0.1.0.r52"));
        assert_eq!(package_version("-1"), None);
    }

    #[test]
    fn waits_for_a_new_release() {
        use crate::exec::{FakeExec, Output};
        let answer = |stdout: &str| {
            FakeExec::new().on_args(
                "curl",
                &[
                    "-sS",
                    "--proto",
                    "=https",
                    "--max-time",
                    "100",
                    "-w",
                    "\n%{http_code}",
                    "https://last1.fun/api/public/releases/wait?last1-panel=0.1.0.r58",
                ],
                Output { status: 0, stdout: stdout.into(), ..Default::default() },
            )
        };
        let published = answer("{\"versions\":{\"last1-panel\":\"0.1.0.r59\"}}\n200");
        assert_eq!(wait_for_release(&published, "0.1.0.r58").unwrap().as_deref(), Some("0.1.0.r59"));
        assert_eq!(wait_for_release(&answer("\n204"), "0.1.0.r58").unwrap(), None);
        assert!(wait_for_release(&answer("trop\n429"), "0.1.0.r58").is_err());
    }
}
