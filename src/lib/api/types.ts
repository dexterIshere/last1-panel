// Types échangés avec le cœur Rust (core/src, sérialisés en camelCase).

import type { Lang } from "$lib/i18n.svelte";

export type Vendor = "nvidia" | "amd" | "intel" | "other";
export type Desktop = "hyprland" | "plasma" | "other";

export interface Gpu {
  card: string;
  pciSlot: string;
  vendor: Vendor;
  name: string | null;
  driver: string | null;
  driverVersion: string | null;
  bootVga: boolean;
  /** Puce intégrée au processeur. */
  integrated: boolean;
  /** La carte des jeux (core/src/system/gpu.rs, choose_games). */
  games: boolean;
}

export interface Features {
  primary: boolean;
  bitDepths: number[];
  liveColor: boolean;
  readOnly: string | null;
}

/** Outils installés dont dépendent les réglages. */
export interface Tools {
  nvibrant: boolean;
  lact: boolean;
  lactRunning: boolean;
  /** LACT installable par le bouton (Arch, CachyOS). */
  pacman: boolean;
  mangohud: boolean;
  dlssSwapper: boolean;
  steam: boolean;
}

export interface SystemInfo {
  version: string;
  lang: Lang;
  desktop: Desktop;
  features: Features | null;
  gpus: Gpu[];
  tools: Tools;
}

// Couleurs

export interface Channel {
  brightness: number;
  contrast: number;
  gamma: number;
}

/** Courbes rouge, verte et bleue appliquées à l'écran. */
export interface ColorAdjust {
  red: Channel;
  green: Channel;
  blue: Channel;
  /** Curseurs de « Tous » (interface seulement). */
  all?: Channel;
  /** Curseurs propres à chaque canal : à leur valeur par défaut, « Tous » s'applique. */
  own?: OwnChannels;
}

export interface OwnChannels {
  red: Channel;
  green: Channel;
  blue: Channel;
}

export interface ColorState {
  output: string;
  adjust: ColorAdjust;
  vibrance: number;
  vibranceUnavailable: string | null;
  colorUnavailable: string | null;
  replacedProfile: string | null;
}

// Affichage

export interface Mode {
  width: number;
  height: number;
  refreshMhz: number;
}

export type Rotation = "normal" | "left" | "inverted" | "right";
export type Vrr = "never" | "always" | "fullscreen";

export interface Position {
  x: number;
  y: number;
}

export interface Output {
  name: string;
  label: string;
  enabled: boolean;
  primary: boolean;
  modes: Mode[];
  currentMode: Mode | null;
  preferredMode: Mode | null;
  position: Position;
  scale: number;
  rotation: Rotation;
  vrr: Vrr | null;
  hdr: boolean | null;
  bitDepth: number | null;
  iccProfile: string | null;
}

export interface OutputChange {
  name: string;
  enabled?: boolean;
  primary?: boolean;
  mode?: Mode;
  position?: Position;
  scale?: number;
  rotation?: Rotation;
  vrr?: Vrr;
  hdr?: boolean;
  bitDepth?: number;
}

// Carte graphique (LACT)

export interface GpuDevice {
  id: string;
  name: string;
  integrated: boolean;
  pciSlot: string;
}

export interface GpuStats {
  temperatures: Record<string, number>;
  fanPercent: number | null;
  fanRpm: number | null;
  fanAuto: boolean;
  power: number | null;
  powerCap: number | null;
  powerCapMin: number | null;
  powerCapMax: number | null;
  powerCapDefault: number | null;
  gpuClock: number | null;
  vramClock: number | null;
  vramUsed: number | null;
  vramTotal: number | null;
  busyPercent: number | null;
}

export interface OffsetRange {
  current: number;
  min: number;
  max: number;
}

export type Fans = { mode: "auto" } | { mode: "fixed"; percent: number } | { mode: "curve"; points: [number, number][] };

export interface Tuning {
  powerCap: number | null;
  fans: Fans;
  gpuOffset: number | null;
  memOffset: number | null;
}

export interface GpuTuning {
  tuning: Tuning;
  gpuOffset: OffsetRange | null;
  memOffset: OffsetRange | null;
  stats: GpuStats;
}

// Jeux

export type Toggle = "app" | "on" | "off";
export type LowLatency = "app" | "on" | "ultra";

export interface GameProfile {
  fpsLimit: number | null;
  vsync: Toggle;
  lowLatency: LowLatency;
  anisotropic: number | null;
  dlssLatest: boolean | null;
  dlssIndicator: boolean | null;
  smoothMotion: boolean | null;
  hdr: boolean | null;
  overlay: boolean | null;
  /** Profil de couleurs (identifiant), dans la seule fenêtre du jeu. */
  colorProfile: string | null;
}

/** Profil de couleurs gardé sous un nom. */
export interface ColorProfile {
  id: string;
  name: string;
  color: ColorAdjust;
  vibrance: number;
}

/** Diagnostic des performances en jeu (core/src/system/diagnostic.rs). */
export type PerfLevel = "problem" | "warning" | "info" | "ok";
export type PerfFix =
  | "low-latency"
  | "install-mango-hud"
  | "wrap-games"
  | "performance"
  | "ntsync"
  | "shader-cache"
  | "split-lock"
  | "x3d-cache"
  | "scheduler"
  | "zram"
  | "restart"
  | "vulkan-driver"
  | "system-update"
  | "game-mode"
  | "panel-self-refresh"
  | "cpu-power-limit"
  | "gpu-power-max"
  | "razer-cooling";

export interface PerfCheck {
  id: string;
  level: PerfLevel;
  title: string;
  detail: string;
  /** Correction que Last1 Panel fait lui-même (ses réglages). */
  fix: PerfFix | null;
}

/** Partie mesurée par MangoHud (core/src/system/sessions.rs). */
export interface GameSession {
  program: string;
  started: string;
  minutes: number;
  fpsAverage: number;
  /** Images par seconde du 1 % des images les plus lentes. */
  fpsOnePercentLow: number;
  cpuLoad: number;
  gpuLoad: number;
  gpuClock: number;
  limit: "gpu" | "cpu" | "cap" | "loading" | "?";
  /** Ce qui entourait la partie : carte, puissance, températures, mode du processeur, freins… */
  details: string[];
  /** La partie au fil du temps (60 points au plus). */
  timeline: Point[];
}

/** Un moment d'une partie (moyenne d'une tranche du journal de MangoHud). */
export interface Point {
  minute: number;
  fps: number;
  fpsLow: number;
  cpuLoad: number;
  gpuLoad: number;
}

export interface PerfReport {
  checks: PerfCheck[];
  facts: [string, string][];
  sessions: GameSession[];
  /** Rapport à copier. */
  text: string;
  /** PC installé par go-to-linux (il pose les optimisations) ; sinon chaque point dit comment le corriger. */
  goToLinux: boolean;
}

/** Partage des rapports de performances avec last1.fun (avec l'accord du joueur). */
/** Shaders que Steam compile en ce moment (games/shaders.rs). */
export interface Precache {
  /** Nom du jeu dans Steam (sinon « AppId N »). */
  game: string | null;
  /** Avancement comme Steam le compte ; absents : pas encore commencé. */
  done: number | null;
  total: number | null;
  /** Avancement lisible (faux : Steam Flatpak ou version inconnue). */
  readable: boolean;
  /** Processus fossilize_replay. */
  processes: number;
  /** Temps de processeur pris, en centièmes de seconde. */
  cpuTicks: number;
  /** Depuis combien de secondes la compilation tourne. */
  runningSecs: number | null;
  /** Cœurs que Steam lui donne (connu si Steam le dit), et ceux du processeur. */
  threads: number | null;
  cores: number;
  /** Steam lancé avant d'avoir tous les cœurs : relancé, il compile plus vite. */
  restartToSpeedUp: boolean;
}

export interface Sharing {
  /** Le joueur a répondu. */
  asked: boolean;
  enabled: boolean;
  /** PC installé par go-to-linux : il envoie lui-même ses rapports. */
  goToLinux: boolean;
}

/** Lien d'un jeu à un profil de couleurs. */
export interface GameLink {
  /** L'option de lancement de Last1 Panel manque et Steam est ouvert. */
  needsSteamClosed: boolean;
}

export interface ColorProfiles {
  profiles: ColorProfile[];
  /** Icône près de l'horloge affichée. */
  tray: boolean;
}

export interface GamesSettings {
  global: GameProfile;
  perGame: Record<string, GameProfile>;
}

export interface Game {
  appId: number;
  name: string;
  installDir: string;
  launchOptions: string;
  wrapped: boolean;
}

export interface GamesInfo {
  steam: boolean;
  steamRunning: boolean;
  games: Game[];
  settings: GamesSettings;
  /** Option de lancement à mettre dans Steam. */
  launchOption: string;
  colorProfiles: ColorProfile[];
  /** Couche Vulkan des couleurs des jeux installée. */
  colorLayer: boolean;
}

/** Pointeur réglé par le bureau (libinput) : accélération, sensibilité. */
export interface PointerState {
  /** Souris branchées (pavés tactiles exclus). */
  mice: string[];
  acceleration: boolean;
  /** Vitesse libinput, de -1 à 1 (0 : celle de la souris). */
  speed: number;
  /** Pourquoi le bureau ne se règle pas d'ici. */
  unavailable: string | null;
}

export interface DpiStage {
  id: string;
  dpi: number;
  active: boolean;
}

/** Souris de jeu réglée dans sa mémoire (libratbag). */
export interface GamingMouse {
  id: string;
  name: string;
  profile: string;
  stages: DpiStage[];
  dpiChoices: number[];
  /** Fréquence d'interrogation (Hz). */
  rate: number | null;
  rateChoices: number[];
}

export interface Ratbag {
  /** libratbag installé. */
  installed: boolean;
  mice: GamingMouse[];
}

export interface MouseState {
  pointer: PointerState;
  ratbag: Ratbag;
}

/** Une vidéo des clips (core/src/clips.rs). */
export interface Clip {
  name: string;
  path: string;
  /** Octets. */
  size: number;
  /** Secondes depuis 1970. */
  modified: number;
}

export interface ClipsState {
  /** gpu-screen-recorder installé. */
  installed: boolean;
  /** Claviers lisibles : F8 et F9 marchent. */
  hotkeys: boolean;
  enabled: boolean;
  folder: string;
  clips: Clip[];
}

/** Nouveautés de la version installée (core/src/release_notes.rs). */
export interface News {
  version: string;
  notes: string[];
}
