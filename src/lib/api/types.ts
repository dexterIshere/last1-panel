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
  gamePerformance: boolean;
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
  performanceMode: boolean | null;
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
