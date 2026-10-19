//! Shaders compilés par Steam en arrière-plan (« Traitement des shaders
//! Vulkan », fossilize_replay, après une mise à jour d'un jeu ou du pilote) :
//! pour quel jeu, et où il en est. L'avancement est lu comme Steam le lit,
//! dans le bloc de contrôle qu'il partage avec fossilize_replay (`--shmem-fd`,
//! fossilize_external_replayer_control_block.hpp de Fossilize).

use super::{steam, vdf};
use serde::Serialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Precache {
    /// Jeu dont les shaders sont compilés (nom Steam, sinon « AppId N »).
    pub game: Option<String>,
    /// Travail fait, sur `total` : le pourcentage de Steam. Absents : pas
    /// encore commencé, ou bloc illisible (autre version de Fossilize).
    pub done: Option<u32>,
    pub total: Option<u32>,
}

/// Compilation en cours, s'il y en a une.
pub fn running(proc_dir: &Path, steam_root: Option<&Path>) -> Option<Precache> {
    let mut found: Option<(std::path::PathBuf, Vec<String>)> = None;
    for entry in fs::read_dir(proc_dir).ok()?.flatten() {
        if !entry.file_name().to_str().is_some_and(|name| name.chars().all(|c| c.is_ascii_digit())) {
            continue;
        }
        // Le nom du programme, coupé à 15 caractères par le noyau.
        if fs::read_to_string(entry.path().join("comm")).map(|comm| comm.trim().to_string()).ok().as_deref()
            != Some("fossilize_repla")
        {
            continue;
        }
        let args: Vec<String> = fs::read(entry.path().join("cmdline"))
            .unwrap_or_default()
            .split(|&b| b == 0)
            .filter(|arg| !arg.is_empty())
            .map(|arg| String::from_utf8_lossy(arg).into_owned())
            .collect();
        // Le processus maître (celui que Steam lance) a tout ; les autres
        // compilent pour lui.
        let master = args.iter().any(|arg| arg == "--master-process");
        if master || found.is_none() {
            found = Some((entry.path(), args));
        }
        if master {
            break;
        }
    }
    let (process, args) = found?;
    let app_id = args.iter().find_map(|arg| {
        let (_, rest) = arg.split_once("shadercache/")?;
        rest.split('/').next()?.parse::<u32>().ok()
    });
    let game = app_id.map(|id| steam_root.and_then(|root| app_name(root, id)).unwrap_or_else(|| format!("AppId {id}")));
    let progress = args
        .iter()
        .position(|arg| arg == "--shmem-fd")
        .and_then(|i| args.get(i + 1))
        .and_then(|fd| fs::read(process.join("fd").join(fd)).ok())
        .and_then(|block| condensed(&block));
    Some(Precache { game, done: progress.map(|p| p.0), total: progress.map(|p| p.1) })
}

/// Nom d'un jeu installé (son appmanifest).
fn app_name(root: &Path, app_id: u32) -> Option<String> {
    steam::libraries(root).iter().find_map(|library| {
        let text = fs::read_to_string(library.join(format!("steamapps/appmanifest_{app_id}.acf"))).ok()?;
        vdf::parse(&text).ok()?.block("AppState")?.text("name").map(str::to_string)
    })
}

const MAGIC: u32 = 0x19bc_de1d;
/// sizeof(SharedControlBlock) ; décalages de ses champs (u32) ci-dessous.
const SIZE: usize = 3244;
const SUCCESSFUL: [usize; 3] = [12, 16, 20];
const SKIPPED: [usize; 3] = [24, 28, 32];
const CACHED: [usize; 3] = [36, 40, 44];
const STATIC_TOTAL: [usize; 3] = [116, 120, 124];
const SUCCESSFUL_MODULES: usize = 8;
const PARSED_MODULE_FAILURES: usize = 80;
const TOTAL_MODULES: usize = 96;
const BANNED_MODULES: usize = 100;
const MODULE_VALIDATION_FAILURES: usize = 104;
const PROGRESS_STARTED: usize = 108;
const RING_BUFFER_OFFSET: usize = 3236;
const RING_BUFFER_SIZE: usize = 3240;

/// Avancement (fait, total), calculé comme Steam
/// (ExternalReplayer::compute_condensed_progress) : graphiques, calcul et
/// lancer de rayons ; les modules comptent pour un dixième, ce qui était
/// déjà prêt ne compte presque pas.
fn condensed(block: &[u8]) -> Option<(u32, u32)> {
    let at = |offset: usize| -> Option<u64> {
        Some(u64::from(u32::from_le_bytes(block.get(offset..offset + 4)?.try_into().ok()?)))
    };
    // Mêmes contrôles que fossilize_replay : une autre disposition du bloc
    // ne donne pas de chiffres faux.
    let (ring_offset, ring_size) = (at(RING_BUFFER_OFFSET)?, at(RING_BUFFER_SIZE)?);
    if at(0)? != u64::from(MAGIC)
        || ring_offset < SIZE as u64
        || ring_size == 0
        || !ring_size.is_power_of_two()
        || ring_offset + ring_size > block.len() as u64
        || at(PROGRESS_STARTED)? == 0
    {
        return None;
    }
    let (mut done, mut total) = (0, 0);
    for kind in 0..3 {
        let skipped = at(CACHED[kind])? + at(SKIPPED[kind])?;
        let static_total = at(STATIC_TOTAL[kind])?;
        let work = static_total - static_total.min(skipped);
        let weighted_skipped = skipped.div_ceil(100);
        done += at(SUCCESSFUL[kind])?.min(work) + weighted_skipped;
        total += work + weighted_skipped;
    }
    let modules = at(TOTAL_MODULES)?;
    let decompressed =
        (at(SUCCESSFUL_MODULES)? + at(MODULE_VALIDATION_FAILURES)? + at(BANNED_MODULES)? + at(PARSED_MODULE_FAILURES)?)
            .min(modules);
    done += decompressed.div_ceil(10);
    total += modules.div_ceil(10);
    Some((u32::try_from(done).ok()?, u32::try_from(total).ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bloc de contrôle comme Steam le crée (64 Kio + 4 Kio, anneau de 64 Kio).
    fn block(values: &[(usize, u32)]) -> Vec<u8> {
        let mut block = vec![0u8; 64 * 1024 + 4 * 1024];
        let mut set = |offset: usize, value: u32| block[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        set(0, MAGIC);
        set(RING_BUFFER_OFFSET, 4096);
        set(RING_BUFFER_SIZE, 64 * 1024);
        set(PROGRESS_STARTED, 1);
        for &(offset, value) in values {
            set(offset, value);
        }
        block
    }

    #[test]
    fn progress_is_read_like_steam() {
        // 10 000 pipelines graphiques, 2 500 compilés, 1 000 déjà prêts ;
        // 3 000 modules sur 3 000.
        let block = block(&[
            (STATIC_TOTAL[0], 10_000),
            (SUCCESSFUL[0], 2_500),
            (CACHED[0], 1_000),
            (TOTAL_MODULES, 3_000),
            (SUCCESSFUL_MODULES, 3_000),
        ]);
        assert_eq!(condensed(&block), Some((2_500 + 10 + 300, 9_000 + 10 + 300)));
        // Pas commencé, ou bloc d'une autre disposition : rien.
        let mut waiting = block.clone();
        waiting[PROGRESS_STARTED..PROGRESS_STARTED + 4].copy_from_slice(&0u32.to_le_bytes());
        assert_eq!(condensed(&waiting), None);
        let mut other = block.clone();
        other[RING_BUFFER_SIZE..RING_BUFFER_SIZE + 4].copy_from_slice(&1000u32.to_le_bytes());
        assert_eq!(condensed(&other), None);
        assert_eq!(condensed(&block[..100]), None);
    }

    #[test]
    fn finds_the_game_and_its_progress() {
        let dir = tempfile::tempdir().unwrap();
        let steam = dir.path().join("Steam");
        fs::create_dir_all(steam.join("steamapps")).unwrap();
        fs::write(
            steam.join("steamapps/appmanifest_1091500.acf"),
            "\"AppState\"\n{\n\t\"appid\"\t\t\"1091500\"\n\t\"name\"\t\t\"Cyberpunk 2077\"\n}\n",
        )
        .unwrap();
        let proc_dir = dir.path().join("proc");
        let process = |pid: &str, args: &[&str]| {
            let path = proc_dir.join(pid);
            fs::create_dir_all(path.join("fd")).unwrap();
            fs::write(path.join("comm"), "fossilize_repla\n").unwrap();
            fs::write(path.join("cmdline"), args.join("\0") + "\0").unwrap();
            path
        };
        assert_eq!(running(&proc_dir, Some(&steam)), None);
        process("4242", &["/proc/self/exe", "--slave-process", "--shmem-fd", "7"]);
        let master = process(
            "4200",
            &[
                "/home/j/.local/share/Steam/ubuntu12_64/fossilize_replay",
                "/home/j/.local/share/Steam/steamapps/shadercache/1091500/fozpipelinesv6/steamapp_pipeline_cache.foz",
                "--master-process",
                "--shmem-fd",
                "98",
            ],
        );
        fs::write(master.join("fd/98"), block(&[(STATIC_TOTAL[0], 100), (SUCCESSFUL[0], 25)])).unwrap();
        let precache = running(&proc_dir, Some(&steam)).unwrap();
        assert_eq!(precache, Precache { game: Some("Cyberpunk 2077".into()), done: Some(25), total: Some(100) });
        // Jeu introuvable dans Steam, bloc illisible : la compilation reste dite.
        fs::remove_file(master.join("fd/98")).unwrap();
        assert_eq!(
            running(&proc_dir, None),
            Some(Precache { game: Some("AppId 1091500".into()), done: None, total: None })
        );
    }
}
