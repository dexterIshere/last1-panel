//! Shaders compilés par Steam en arrière-plan (« Traitement des shaders
//! Vulkan », fossilize_replay, après une mise à jour d'un jeu ou du pilote) :
//! pour quel jeu, et où il en est. L'avancement est lu comme Steam le lit,
//! dans le bloc de contrôle qu'il partage avec fossilize_replay
//! (fossilize_external_replayer_control_block.hpp de Fossilize).
//!
//! Steam crée ce bloc (/dev/shm/fossilize-external-<pid de Steam>-<n>) et
//! l'efface aussitôt du dossier ; fossilize_replay le garde en mémoire mais
//! ferme son descripteur. Seul Steam le garde ouvert : le nom est lu dans la
//! mémoire de fossilize_replay (/proc/<pid>/maps), le bloc dans les
//! descripteurs de Steam (/proc/<pid>/fd).

use super::{steam, vdf};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Precache {
    /// Jeu dont les shaders sont compilés (nom Steam, sinon « AppId N »).
    pub game: Option<String>,
    /// Travail fait, sur `total` : le pourcentage de Steam. Absents : pas
    /// encore commencé, ou bloc illisible.
    pub done: Option<u32>,
    pub total: Option<u32>,
    /// Bloc de contrôle trouvé et lu (faux : Steam Flatpak, autre version).
    pub readable: bool,
    /// Processus fossilize_replay (le maître et ceux qui compilent pour lui).
    pub processes: u32,
    /// Temps de processeur qu'ils ont pris, en centièmes de seconde :
    /// l'interface en tire le nombre de cœurs au travail (la preuve que ça
    /// avance, même sans pourcentage).
    pub cpu_ticks: u64,
    /// Depuis combien de secondes la compilation tourne.
    pub running_secs: Option<u64>,
}

/// Temps de processeur d'un processus (utime + stime, en tops d'horloge :
/// 100 par seconde sous Linux) et son heure de départ, en tops depuis le
/// démarrage du PC (/proc/<pid>/stat, champs 14, 15 et 22).
fn cpu_and_start(process: &Path) -> Option<(u64, u64)> {
    let stat = fs::read_to_string(process.join("stat")).ok()?;
    let fields: Vec<&str> = stat.rsplit_once(')')?.1.split_whitespace().collect();
    let field = |n: usize| fields.get(n - 3)?.parse::<u64>().ok();
    Some((field(14)? + field(15)?, field(22)?))
}

/// Compilation en cours, s'il y en a une.
pub fn running(proc_dir: &Path, steam_root: Option<&Path>) -> Option<Precache> {
    let mut found: Option<(PathBuf, Vec<String>)> = None;
    let (mut processes, mut cpu_ticks) = (0, 0);
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
        processes += 1;
        cpu_ticks += cpu_and_start(&entry.path()).map_or(0, |(ticks, _)| ticks);
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
    }
    let (process, args) = found?;
    let uptime = fs::read_to_string(proc_dir.join("uptime"))
        .ok()
        .and_then(|text| text.split_whitespace().next()?.parse::<f64>().ok());
    let running_secs =
        cpu_and_start(&process).zip(uptime).map(|((_, start), uptime)| (uptime as u64).saturating_sub(start / 100));
    // « …/steamapps/shadercache/<appid>/… » : le jeu, et sa bibliothèque.
    let cache = args.iter().find_map(|arg| {
        let (library, rest) = arg.split_once("/steamapps/shadercache/")?;
        Some((PathBuf::from(library), rest.split('/').next()?.parse::<u32>().ok()?))
    });
    let game = cache.as_ref().map(|(library, id)| {
        manifest_name(library, *id)
            .or_else(|| steam_root.and_then(|root| steam::libraries(root).iter().find_map(|l| manifest_name(l, *id))))
            .unwrap_or_else(|| format!("AppId {id}"))
    });
    let block = control_block(proc_dir, &process);
    let progress = block.as_deref().and_then(condensed);
    Some(Precache {
        game,
        done: progress.and_then(|p| p.map(|p| p.0)),
        total: progress.and_then(|p| p.map(|p| p.1)),
        readable: progress.is_some(),
        processes,
        cpu_ticks,
        running_secs,
    })
}

/// Nom d'un jeu dans sa bibliothèque Steam (son appmanifest).
fn manifest_name(library: &Path, app_id: u32) -> Option<String> {
    let text = fs::read_to_string(library.join(format!("steamapps/appmanifest_{app_id}.acf"))).ok()?;
    vdf::parse(&text).ok()?.block("AppState")?.text("name").map(str::to_string)
}

/// Le bloc de contrôle de cette compilation : son nom dans la mémoire de
/// fossilize_replay, son contenu par le descripteur que Steam garde ouvert
/// (Steam, d'après le nom, sinon le parent de fossilize_replay).
fn control_block(proc_dir: &Path, process: &Path) -> Option<Vec<u8>> {
    let maps = fs::read_to_string(process.join("maps")).ok()?;
    let path = maps.lines().find_map(|line| {
        let start = line.find("/fossilize-external-")?;
        let begin = line[..start].rfind(' ').map_or(0, |i| i + 1);
        Some(line[begin..].trim_end_matches(" (deleted)").to_string())
    })?;
    // Version de Fossilize qui laisse le bloc dans /dev/shm.
    if let Ok(block) = fs::read(&path) {
        return Some(block);
    }
    let name = Path::new(&path).file_name()?.to_string_lossy().into_owned();
    let creator = name.strip_prefix("fossilize-external-")?.split('-').next()?.to_string();
    let parent = fs::read_to_string(process.join("status")).ok().and_then(|status| {
        status.lines().find_map(|line| line.strip_prefix("PPid:").map(|pid| pid.trim().to_string()))
    });
    [Some(creator), parent].into_iter().flatten().find_map(|pid| {
        fs::read_dir(proc_dir.join(pid).join("fd")).ok()?.flatten().find_map(|fd| {
            let target = fs::read_link(fd.path()).ok()?;
            let target = target.file_name()?.to_string_lossy();
            (target.trim_end_matches(" (deleted)") == name).then(|| fs::read(fd.path()).ok()).flatten()
        })
    })
}

/// Dispositions du bloc selon la version de Fossilize (décalages de ses
/// champs u32, calculés depuis ses en-têtes) : la bonne est celle où les
/// champs de l'anneau sont valides, comme fossilize_replay les vérifie.
struct Layout {
    size: u64,
    ring_offset: usize,
    ring_size: usize,
    progress_started: usize,
    successful_modules: usize,
    parsed_module_failures: usize,
    total_modules: usize,
    banned_modules: usize,
    module_validation_failures: usize,
    /// Graphiques, calcul, lancer de rayons (absent avant septembre 2021).
    successful: [Option<usize>; 3],
    skipped: [Option<usize>; 3],
    cached: [Option<usize>; 3],
    static_total: [Option<usize>; 3],
}

/// Depuis septembre 2021 (lancer de rayons) ; l'anneau a bougé en octobre
/// 2023 (battements de cœur des processus).
const fn with_raytracing(size: u64, ring_offset: usize) -> Layout {
    Layout {
        size,
        ring_offset,
        ring_size: ring_offset + 4,
        progress_started: 108,
        successful_modules: 8,
        parsed_module_failures: 80,
        total_modules: 96,
        banned_modules: 100,
        module_validation_failures: 104,
        successful: [Some(12), Some(16), Some(20)],
        skipped: [Some(24), Some(28), Some(32)],
        cached: [Some(36), Some(40), Some(44)],
        static_total: [Some(116), Some(120), Some(124)],
    }
}

/// De septembre 2020 à septembre 2021.
const fn without_raytracing(size: u64, ring_offset: usize) -> Layout {
    Layout {
        size,
        ring_offset,
        ring_size: ring_offset + 4,
        progress_started: 84,
        successful_modules: 8,
        parsed_module_failures: 60,
        total_modules: 72,
        banned_modules: 76,
        module_validation_failures: 80,
        successful: [Some(12), Some(16), None],
        skipped: [Some(20), Some(24), None],
        cached: [Some(28), Some(32), None],
        static_total: [Some(92), Some(96), None],
    }
}

const LAYOUTS: [Layout; 5] = [
    with_raytracing(3244, 3236),
    with_raytracing(2220, 2212),
    without_raytracing(2192, 2184),
    without_raytracing(2180, 2172),
    without_raytracing(124, 116),
];

const MAGIC: u32 = 0x19bc_de1d;

/// Avancement (fait, total), calculé comme Steam
/// (ExternalReplayer::compute_condensed_progress) : graphiques, calcul et
/// lancer de rayons ; les modules comptent pour un dixième, ce qui était
/// déjà prêt ne compte presque pas. None : bloc illisible ; Some(None) :
/// pas encore commencé.
fn condensed(block: &[u8]) -> Option<Option<(u32, u32)>> {
    let at = |offset: usize| -> Option<u64> {
        Some(u64::from(u32::from_le_bytes(block.get(offset..offset + 4)?.try_into().ok()?)))
    };
    if at(0)? != u64::from(MAGIC) {
        return None;
    }
    // Mêmes contrôles que fossilize_replay.
    let layout = LAYOUTS.iter().find(|layout| {
        at(layout.ring_offset).zip(at(layout.ring_size)).is_some_and(|(offset, size)| {
            offset >= layout.size && size.is_power_of_two() && offset + size <= block.len() as u64
        })
    })?;
    if at(layout.progress_started)? == 0 {
        return Some(None);
    }
    let field = |offset: Option<usize>| offset.map_or(Some(0), at);
    let (mut done, mut total) = (0, 0);
    for kind in 0..3 {
        let skipped = field(layout.cached[kind])? + field(layout.skipped[kind])?;
        let static_total = field(layout.static_total[kind])?;
        let work = static_total - static_total.min(skipped);
        let weighted_skipped = skipped.div_ceil(100);
        done += field(layout.successful[kind])?.min(work) + weighted_skipped;
        total += work + weighted_skipped;
    }
    let modules = at(layout.total_modules)?;
    let decompressed = (at(layout.successful_modules)?
        + at(layout.module_validation_failures)?
        + at(layout.banned_modules)?
        + at(layout.parsed_module_failures)?)
    .min(modules);
    done += decompressed.div_ceil(10);
    total += modules.div_ceil(10);
    Some(Some((u32::try_from(done).ok()?, u32::try_from(total).ok()?)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bloc de contrôle comme Steam le crée (64 Kio + 4 Kio, anneau de 64 Kio
    /// après 4 Kio), dans la disposition d'une version de Fossilize.
    fn block(layout: &Layout, values: &[(Option<usize>, u32)]) -> Vec<u8> {
        let mut block = vec![0u8; 64 * 1024 + 4 * 1024];
        let mut set = |offset: usize, value: u32| block[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        set(0, MAGIC);
        set(layout.ring_offset, 4096);
        set(layout.ring_size, 64 * 1024);
        set(layout.progress_started, 1);
        for &(offset, value) in values {
            set(offset.unwrap(), value);
        }
        block
    }

    #[test]
    fn progress_is_read_like_steam_in_every_version() {
        // 10 000 pipelines graphiques, 2 500 compilés, 1 000 déjà prêts ;
        // 3 000 modules sur 3 000.
        for layout in &LAYOUTS {
            let block = block(
                layout,
                &[
                    (layout.static_total[0], 10_000),
                    (layout.successful[0], 2_500),
                    (layout.cached[0], 1_000),
                    (Some(layout.total_modules), 3_000),
                    (Some(layout.successful_modules), 3_000),
                ],
            );
            assert_eq!(condensed(&block), Some(Some((2_500 + 10 + 300, 9_000 + 10 + 300))), "{}", layout.size);
            // Pas encore commencé.
            let mut waiting = block.clone();
            waiting[layout.progress_started..layout.progress_started + 4].copy_from_slice(&0u32.to_le_bytes());
            assert_eq!(condensed(&waiting), Some(None));
        }
        // Disposition inconnue, autre chose, bloc tronqué : illisible.
        let mut other = block(&LAYOUTS[0], &[]);
        other[LAYOUTS[0].ring_size..LAYOUTS[0].ring_size + 4].copy_from_slice(&1000u32.to_le_bytes());
        assert_eq!(condensed(&other), None);
        assert_eq!(condensed(&[0u8; 4096]), None);
        assert_eq!(condensed(&block(&LAYOUTS[0], &[])[..100]), None);
    }

    #[test]
    fn finds_the_game_and_reads_the_block_steam_keeps_open() {
        let dir = tempfile::tempdir().unwrap();
        let library = dir.path().join("SteamLibrary");
        fs::create_dir_all(library.join("steamapps")).unwrap();
        fs::write(
            library.join("steamapps/appmanifest_1962700.acf"),
            "\"AppState\"\n{\n\t\"appid\"\t\t\"1962700\"\n\t\"name\"\t\t\"Subnautica 2\"\n}\n",
        )
        .unwrap();
        let proc_dir = dir.path().join("proc");
        let process = |pid: &str, comm: &str, args: &[&str]| {
            let path = proc_dir.join(pid);
            fs::create_dir_all(path.join("fd")).unwrap();
            fs::write(path.join("comm"), format!("{comm}\n")).unwrap();
            fs::write(path.join("cmdline"), args.join("\0") + "\0").unwrap();
            path
        };
        assert_eq!(running(&proc_dir, None), None);
        // Steam (4100) a créé le bloc et l'a effacé de /dev/shm ; il garde son
        // descripteur. fossilize_replay a fermé le sien, le bloc reste dans sa
        // mémoire.
        let steam = process("4100", "steam", &["steam"]);
        let shm = dir.path().join("shm");
        fs::create_dir_all(&shm).unwrap();
        let layout = &LAYOUTS[1];
        fs::write(
            shm.join("fossilize-external-4100-3"),
            block(layout, &[(layout.static_total[0], 100), (layout.successful[0], 25)]),
        )
        .unwrap();
        std::os::unix::fs::symlink(dir.path().join("ailleurs"), steam.join("fd/12")).unwrap();
        std::os::unix::fs::symlink(shm.join("fossilize-external-4100-3"), steam.join("fd/45")).unwrap();
        let cache =
            format!("{}/steamapps/shadercache/1962700/fozpipelinesv6/steamapp_pipeline_cache.foz", library.display());
        process("4242", "fossilize_repla", &["/proc/self/exe", "--slave-process", "--shmem-fd", "7"]);
        let master =
            process("4200", "fossilize_repla", &["fossilize_replay", &cache, "--master-process", "--shmem-fd", "98"]);
        fs::write(
            master.join("maps"),
            "5555-6666 r-xp 00000000 08:02 1 /usr/bin/fossilize_replay\n\
             7f12a0000000-7f12a0011000 rw-s 00000000 00:19 123456                     /dev/shm/fossilize-external-4100-3 (deleted)\n",
        )
        .unwrap();
        let precache = running(&proc_dir, None).unwrap();
        assert_eq!(
            precache,
            Precache {
                game: Some("Subnautica 2".into()),
                done: Some(25),
                total: Some(100),
                readable: true,
                processes: 2,
                cpu_ticks: 0,
                running_secs: None
            }
        );
        // Temps de processeur des deux processus, et depuis quand ça tourne
        // (parti 600 s après le démarrage, il y a 3000 s).
        let stat = |ticks: u64, start: u64| {
            format!("4200 (fossilize_repla) S 4100 4100 0 0 -1 0 0 0 0 0 {ticks} 50 0 0 39 19 1 0 {start} 0 0\n")
        };
        fs::write(master.join("stat"), stat(1000, 60_000)).unwrap();
        fs::write(proc_dir.join("4242/stat"), stat(250, 61_000)).unwrap();
        fs::write(proc_dir.join("uptime"), "3600.42 1000.00\n").unwrap();
        let alive = running(&proc_dir, None).unwrap();
        assert_eq!((alive.processes, alive.cpu_ticks, alive.running_secs), (2, 1350, Some(3000)));
        // Autre espace de numéros (le pid du nom n'est pas celui vu ici) : le
        // parent de fossilize_replay.
        fs::rename(&steam, proc_dir.join("4101")).unwrap();
        fs::write(master.join("status"), "Name:\tfossilize_repla\nPPid:\t4101\n").unwrap();
        assert_eq!(running(&proc_dir, None).unwrap().done, Some(25));
        // Bloc introuvable : la compilation reste dite, sans avancement.
        fs::remove_file(proc_dir.join("4101/fd/45")).unwrap();
        assert_eq!(
            running(&proc_dir, None),
            Some(Precache {
                game: Some("Subnautica 2".into()),
                done: None,
                total: None,
                readable: false,
                processes: 2,
                cpu_ticks: 1350,
                running_secs: Some(3000)
            })
        );
    }
}
