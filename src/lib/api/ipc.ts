// Commandes du cœur Rust (src-tauri/src/commands.rs), typées. Les erreurs
// arrivent en texte dans la langue de l'interface, à montrer tel quel.

import { invoke } from "@tauri-apps/api/core";
import { t, type Lang } from "$lib/i18n.svelte";
import type {
  ClipsState,
  ColorAdjust,
  ColorProfile,
  ColorProfiles,
  GameLink,
  PerfFix,
  Precache,
  Sharing,
  PerfReport,
  ColorState,
  GamesInfo,
  GamesSettings,
  GpuDevice,
  GpuStats,
  GpuTuning,
  MouseState,
  News,
  Output,
  OutputChange,
  SystemInfo,
  Tuning,
} from "./types";

export function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return t("erreur inconnue", "unknown error");
}

export const api = {
  systemInfo: () => invoke<SystemInfo>("system_info"),
  setLanguage: (lang: Lang) => invoke<void>("set_language", { lang }),

  outputs: () => invoke<Output[]>("outputs"),
  /** Applique les changements et relit les écrans. */
  applyOutputs: (changes: OutputChange[]) => invoke<Output[]>("apply_outputs", { changes }),

  colorState: (output: string) => invoke<ColorState>("color_state", { output }),
  setColor: (output: string, adjust: ColorAdjust) => invoke<void>("set_color", { output, adjust }),
  setVibrance: (output: string, percent: number) => invoke<void>("set_vibrance", { output, percent }),

  gpuDevices: () => invoke<GpuDevice[]>("gpu_devices"),
  gpuStats: (id: string) => invoke<GpuStats>("gpu_stats", { id }),
  gpuTuning: (id: string) => invoke<GpuTuning>("gpu_tuning", { id }),
  /** Essaie les réglages : LACT les annule s'ils ne sont pas confirmés à temps (secondes renvoyées). */
  gpuTry: (id: string, tuning: Tuning) => invoke<number>("gpu_try", { id, tuning }),
  gpuConfirm: (keep: boolean) => invoke<void>("gpu_confirm", { keep }),
  /** Active le service LACT (pkexec, mot de passe demandé). */
  enableLact: () => invoke<void>("enable_lact"),
  colorProfiles: () => invoke<ColorProfiles>("color_profiles"),
  /** Couleurs du profil sur l'écran (`id` null : valeurs par défaut). */
  applyColorProfile: (output: string, id: string | null) => invoke<void>("apply_color_profile", { output, id }),
  setTray: (shown: boolean) => invoke<void>("set_tray", { shown }),
  /** Profil de couleurs d'un jeu (null : plus de profil), option de lancement mise si elle manque. */
  linkGameColor: (appId: number, profile: string | null) => invoke<GameLink>("link_game_color", { appId, profile }),
  performanceReport: () => invoke<PerfReport>("performance_report"),
  /** Rend ce qu'il reste à faire au joueur (se reconnecter…). */
  performanceFix: (fixes: PerfFix[]) => invoke<string[]>("performance_fix", { fixes }),
  /** Steam fermé, option de lancement mise pour ces jeux, Steam rouvert. */
  closeSteamAndWrap: (appIds: number[]) => invoke<number>("close_steam_and_wrap", { appIds }),
  saveColorProfile: (id: string | null, name: string, color: ColorAdjust, vibrance: number) =>
    invoke<ColorProfile>("save_color_profile", { id, name, color, vibrance }),
  deleteColorProfile: (id: string) => invoke<void>("delete_color_profile", { id }),
  updateAvailable: () => invoke<string | null>("update_available"),
  installUpdate: () => invoke<void>("install_update"),
  quit: () => invoke<void>("quit"),
  sharing: () => invoke<Sharing>("sharing"),
  shaderPrecache: () => invoke<Precache | null>("shader_precache"),
  /** Steam relancé avec tous les cœurs pour compiler. */
  speedUpShaders: () => invoke<void>("speed_up_shaders"),
  setSharing: (enabled: boolean) => invoke<void>("set_sharing", { enabled }),

  games: () => invoke<GamesInfo>("games"),
  saveGameSettings: (settings: GamesSettings) => invoke<void>("save_game_settings", { settings }),
  /** Met (ou retire) l'option de lancement dans Steam, Steam fermé. */
  setWrapped: (appIds: number[], wrapped: boolean) => invoke<number>("set_wrapped", { appIds, wrapped }),

  mouseState: () => invoke<MouseState>("mouse_state"),
  /** Accélération et sensibilité (vitesse libinput, -1 à 1), gardées et remises à chaque connexion. */
  setPointer: (acceleration: boolean, speed: number) => invoke<void>("set_pointer", { acceleration, speed }),
  /** Écrit dans la souris : paliers de DPI, palier actif (index), fréquence. */
  setMouseHardware: (id: string, dpis: number[], active: number, rate: number | null) =>
    invoke<void>("set_mouse_hardware", { id, dpis, active, rate }),
  /** Installe libratbag (pkexec, mot de passe demandé). */
  installRatbag: () => invoke<void>("install_ratbag"),

  /** Nouveautés de la version installée, pas encore fermées. */
  news: () => invoke<News | null>("news"),
  dismissNews: () => invoke<void>("dismiss_news"),

  clipsState: () => invoke<ClipsState>("clips_state"),
  setClips: (enabled: boolean) => invoke<void>("set_clips", { enabled }),
  /** gpu-screen-recorder et les touches F8/F9 (pkexec, mot de passe demandé). */
  installClips: () => invoke<void>("install_clips"),
  /** Vidéo ou dossier ouvert par l'application du bureau. */
  openClip: (path: string) => invoke<void>("open_clip", { path }),
  deleteClip: (path: string) => invoke<void>("delete_clip", { path }),
};
