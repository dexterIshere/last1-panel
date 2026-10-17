// PC simulé, pour `vite dev` hors de Tauri : l'interface se conçoit et se
// teste dans un navigateur. Jamais inclus dans les versions compilées.
//
// Paramètres d'adresse :
//   ?bureau=plasma        KDE Plasma (couleurs en direct) ; Hyprland sinon
//   ?carte=amd            carte AMD (pas de vibrance ni de DLSS)
//   ?lact=arrete          service LACT installé mais arrêté
//   ?lact=absent          LACT pas installé
//   ?steam=ouvert         Steam lancé (options de lancement bloquées)
//   ?page=display|gpu|games   page affichée au démarrage
//   ?lang=en              système en anglais

import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type {
  ColorAdjust,
  GameProfile,
  GamesSettings,
  Output,
  OutputChange,
  SystemInfo,
  Tuning,
} from "$lib/api/types";
import { app, type Section } from "$lib/state/app.svelte";

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

export function installMockBackend(): void {
  const params = new URLSearchParams(window.location.search);
  const plasma = params.get("bureau") === "plasma";
  const amd = params.get("carte") === "amd";
  let lactRunning = !["arrete", "absent"].includes(params.get("lact") ?? "");
  const steamRunning = params.get("steam") === "ouvert";
  const page = params.get("page");
  if (page && ["colors", "display", "gpu", "games"].includes(page)) app.section = page as Section;

  const system: SystemInfo = {
    version: "0.1.0",
    lang: params.get("lang") === "en" ? "en" : "fr",
    desktop: plasma ? "plasma" : "hyprland",
    features: plasma
      ? { primary: true, bitDepths: [0, 8, 10], liveColor: true, readOnly: null }
      : { primary: false, bitDepths: [8, 10], liveColor: false, readOnly: null },
    gpus: [
      amd
        ? { card: "card1", pciSlot: "0000:03:00.0", vendor: "amd", name: "Radeon RX 7800 XT", driver: "amdgpu", driverVersion: null, bootVga: true }
        : { card: "card1", pciSlot: "0000:01:00.0", vendor: "nvidia", name: "GeForce RTX 4070", driver: "nvidia", driverVersion: "595.58.03", bootVga: true },
    ],
    tools: { nvibrant: !amd, lact: params.get("lact") !== "absent", lactRunning, pacman: true, mangohud: true, dlssSwapper: true, gamePerformance: true, steam: true },
  };

  const mode = (width: number, height: number, hz: number) => ({ width, height, refreshMhz: Math.round(hz * 1000) });
  let outputs: Output[] = [
    {
      name: "DP-1",
      label: "AW2723DF",
      enabled: true,
      primary: true,
      modes: [mode(2560, 1440, 280), mode(2560, 1440, 240), mode(2560, 1440, 165), mode(2560, 1440, 144), mode(2560, 1440, 59.951), mode(1920, 1080, 240), mode(1920, 1080, 60), mode(1280, 720, 60)],
      currentMode: mode(2560, 1440, 165),
      preferredMode: mode(2560, 1440, 280),
      position: { x: 0, y: 0 },
      scale: 1,
      rotation: "normal",
      vrr: "fullscreen",
      hdr: false,
      bitDepth: plasma ? 0 : 8,
      iccProfile: "",
    },
    {
      name: "HDMI-A-1",
      label: "LG TV SSCR2",
      enabled: true,
      primary: false,
      modes: [mode(3840, 2160, 120), mode(3840, 2160, 60), mode(3840, 2160, 59.94), mode(1920, 1080, 120), mode(1920, 1080, 60)],
      currentMode: mode(3840, 2160, 60),
      preferredMode: mode(3840, 2160, 60),
      position: { x: 2560, y: 0 },
      scale: 2,
      rotation: "normal",
      vrr: "never",
      hdr: true,
      bitDepth: plasma ? 10 : 10,
      iccProfile: "",
    },
  ];

  const neutral = { brightness: 50, contrast: 50, gamma: 1 };
  const colors: Record<string, { adjust: ColorAdjust; vibrance: number }> = {
    "DP-1": { adjust: { red: { ...neutral }, green: { ...neutral }, blue: { ...neutral } }, vibrance: 50 },
    "HDMI-A-1": { adjust: { red: { ...neutral }, green: { ...neutral }, blue: { ...neutral } }, vibrance: 50 },
  };

  let tuning: Tuning = { powerCap: null, fans: { mode: "auto" }, gpuOffset: null, memOffset: null };
  let pending: Tuning | null = null;

  const profile = (p: Partial<GameProfile>): GameProfile => ({
    fpsLimit: null,
    vsync: "app",
    lowLatency: "app",
    anisotropic: null,
    dlssLatest: null,
    dlssIndicator: null,
    smoothMotion: null,
    hdr: null,
    overlay: null,
    performanceMode: null,
    ...p,
  });
  let gameSettings: GamesSettings = {
    global: profile({ lowLatency: "on", performanceMode: true }),
    perGame: { "1091500": profile({ dlssLatest: true, fpsLimit: 117 }) },
  };
  const games = [
    { appId: 1091500, name: "Cyberpunk 2077", installDir: "/mnt/windows-d/SteamLibrary/steamapps/common/Cyberpunk 2077", launchOptions: "last1-panel-cli run %command%", wrapped: true },
    { appId: 570, name: "Dota 2", installDir: "~/.local/share/Steam/steamapps/common/dota 2 beta", launchOptions: "", wrapped: false },
    { appId: 1245620, name: "ELDEN RING", installDir: "/mnt/windows-d/SteamLibrary/steamapps/common/ELDEN RING", launchOptions: "PROTON_LOG=1 %command%", wrapped: false },
    { appId: 2694490, name: "Path of Exile 2", installDir: "", launchOptions: "", wrapped: false },
    { appId: 553850, name: "HELLDIVERS 2", installDir: "", launchOptions: "", wrapped: false },
    { appId: 1172470, name: "Apex Legends", installDir: "", launchOptions: "", wrapped: false },
  ];

  const stats = () => {
    const t = Date.now() / 1000;
    const load = 80 + Math.sin(t / 3) * 15;
    return amd
      ? {
          temperatures: { edge: 61 + Math.sin(t / 5) * 2, junction: 78 + Math.sin(t / 4) * 3, mem: 70 },
          fanPercent: 41,
          fanRpm: 1380,
          fanAuto: tuning.fans.mode === "auto",
          power: 210 + Math.sin(t) * 20,
          powerCap: tuning.powerCap ?? 263,
          powerCapMin: 223,
          powerCapMax: 280,
          powerCapDefault: 263,
          gpuClock: 2410,
          vramClock: 2425,
          vramUsed: 7.1 * 1024 ** 3,
          vramTotal: 16 * 1024 ** 3,
          busyPercent: Math.round(load),
        }
      : {
          temperatures: { GPU: 63 + Math.sin(t / 5) * 2, "GPU Hotspot": 75 + Math.sin(t / 4) * 3 },
          fanPercent: tuning.fans.mode === "fixed" ? tuning.fans.percent : 45,
          fanRpm: 1450,
          fanAuto: tuning.fans.mode === "auto",
          power: 165 + Math.sin(t) * 18,
          powerCap: tuning.powerCap ?? 200,
          powerCapMin: 100,
          powerCapMax: 220,
          powerCapDefault: 200,
          gpuClock: 2715,
          vramClock: 10501,
          vramUsed: 5.4 * 1024 ** 3,
          vramTotal: 12 * 1024 ** 3,
          busyPercent: Math.round(load),
        };
  };

  const applyChange = (c: OutputChange) => {
    outputs = outputs.map((o) =>
      o.name !== c.name
        ? o
        : {
            ...o,
            enabled: c.enabled ?? o.enabled,
            primary: c.primary ?? o.primary,
            currentMode: c.mode ?? o.currentMode,
            position: c.position ?? o.position,
            scale: c.scale ?? o.scale,
            rotation: c.rotation ?? o.rotation,
            vrr: c.vrr ?? o.vrr,
            hdr: c.hdr ?? o.hdr,
            bitDepth: c.bitDepth ?? o.bitDepth,
          },
    );
  };

  const commands: Record<string, (args: Record<string, unknown>) => unknown> = {
    system_info: () => system,
    set_language: () => undefined,
    outputs: () => outputs,
    apply_outputs: async (args) => {
      await wait(500);
      (args.changes as OutputChange[]).forEach(applyChange);
      return outputs;
    },
    color_state: (args) => {
      const name = args.output as string;
      const screen = outputs.find((o) => o.name === name);
      return {
        output: name,
        adjust: colors[name].adjust,
        vibrance: colors[name].vibrance,
        vibranceUnavailable: amd ? "Vibrance numérique : cartes NVIDIA seulement pour l'instant." : null,
        colorUnavailable: screen?.hdr
          ? "Écran en HDR : luminosité, contraste et gamma ne s'appliquent qu'en SDR (désactivez le HDR dans Affichage)."
          : null,
        replacedProfile: null,
      };
    },
    set_color: async (args) => {
      await wait(plasma ? 60 : 700);
      colors[args.output as string].adjust = args.adjust as ColorAdjust;
    },
    set_vibrance: async (args) => {
      await wait(300);
      colors[args.output as string].vibrance = args.percent as number;
    },
    gpu_devices: () => {
      if (!lactRunning) throw "Le service LACT ne tourne pas (paquet lact, service lactd).";
      return [
        amd
          ? { id: "1002:747E-1DA2:E453-0000:03:00.0", name: "Radeon RX 7800 XT", integrated: false, pciSlot: "0000:03:00.0" }
          : { id: "10DE:2786-1458:4111-0000:01:00.0", name: "GeForce RTX 4070", integrated: false, pciSlot: "0000:01:00.0" },
      ];
    },
    gpu_stats: () => stats(),
    gpu_tuning: () => ({
      tuning,
      gpuOffset: amd ? null : { current: tuning.gpuOffset ?? 0, min: -1000, max: 1000 },
      memOffset: amd ? null : { current: tuning.memOffset ?? 0, min: -2000, max: 3000 },
      stats: stats(),
    }),
    gpu_try: async (args) => {
      await wait(300);
      pending = args.tuning as Tuning;
      return 5;
    },
    gpu_confirm: (args) => {
      if (args.keep && pending) tuning = pending;
      pending = null;
    },
    enable_lact: async () => {
      await wait(1200);
      lactRunning = true;
      system.tools.lact = true;
      system.tools.lactRunning = true;
    },
    games: () => ({
      steam: true,
      steamRunning,
      games,
      settings: structuredClone(gameSettings),
      launchOption: "last1-panel-cli run %command%",
    }),
    save_game_settings: async (args) => {
      await wait(200);
      gameSettings = args.settings as GamesSettings;
    },
    set_wrapped: async (args) => {
      await wait(300);
      if (steamRunning) throw "Fermez Steam d'abord (menu Steam > Quitter) : il remplacerait ce réglage en se fermant.";
      const ids = args.appIds as number[];
      let count = 0;
      for (const game of games) {
        if (!ids.includes(game.appId) || game.wrapped === args.wrapped) continue;
        game.wrapped = args.wrapped as boolean;
        game.launchOptions = game.wrapped ? `last1-panel-cli run ${game.launchOptions || "%command%"}` : game.launchOptions.replace("last1-panel-cli run ", "");
        count++;
      }
      return count;
    },
  };

  mockWindows("main");
  mockIPC(async (cmd, args) => {
    const handler = commands[cmd];
    if (!handler) throw `commande simulée inconnue : ${cmd}`;
    return handler((args ?? {}) as Record<string, unknown>);
  });
}
