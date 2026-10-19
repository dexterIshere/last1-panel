// PC simulé, pour `vite dev` hors de Tauri : l'interface se conçoit et se
// teste dans un navigateur. Jamais inclus dans les versions compilées.
//
// Paramètres d'adresse :
//   ?bureau=plasma        KDE Plasma (couleurs en direct) ; Hyprland sinon
//   ?carte=amd            carte AMD (pas de vibrance ni de DLSS)
//   ?lact=arrete          service LACT installé mais arrêté
//   ?lact=absent          LACT pas installé
//   ?steam=ouvert         Steam lancé (options de lancement bloquées)
//   ?page=display|gpu|games|perf   page affichée au démarrage
//   ?lang=en              système en anglais
//   ?maj=1                version plus récente sur last1.fun

import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type {
  ColorAdjust,
  ColorProfile,
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
  if (page && ["colors", "display", "gpu", "games", "perf"].includes(page)) app.section = page as Section;

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
    colorProfile: null,
    ...p,
  });
  const guide = (b: number, c: number, g: number) => ({ brightness: b, contrast: c, gamma: g });
  let tray = true;
  let lowLatencyForced = true;
  const perfFixed = new Set<string>();
  let colorProfiles: ColorProfile[] = [
    { id: "p1", name: "Arc Raiders", color: { red: guide(52, 47, 1.47), green: guide(52, 47, 1.47), blue: guide(62, 72, 1.65) }, vibrance: 50 },
  ];
  let gameSettings: GamesSettings = {
    global: profile({ lowLatency: "on", performanceMode: true }),
    perGame: { "1091500": profile({ dlssLatest: true, fpsLimit: 117 }) },
  };
  const games = [
    { appId: 1091500, name: "Cyberpunk 2077", installDir: "/mnt/windows-d/SteamLibrary/steamapps/common/Cyberpunk 2077", launchOptions: "$(command -v last1-panel-cli && echo run) %command%", wrapped: true },
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
        vibranceUnavailable: amd ? "Vibrance du bureau : cartes NVIDIA seulement (Wayland ne la permet pas aux autres). Dans les jeux, elle passe par un profil de couleurs (Couleurs dans le jeu), sur toutes les cartes." : null,
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
    update_available: () => (params.get("maj") ? "0.1.0.r12" : null),
    install_update: async () => {
      await wait(1500);
      window.location.reload();
    },
    quit: async () => {
      window.location.reload();
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
      launchOption: "$(command -v last1-panel-cli && echo run) %command%",
      colorProfiles: structuredClone(colorProfiles),
      colorLayer: params.get("vkbasalt") !== "absent",
    }),
    color_profiles: () => ({ profiles: structuredClone(colorProfiles), tray }),
    save_color_profile: async (args) => {
      await wait(150);
      const next = Math.max(0, ...colorProfiles.map((p) => Number(p.id.slice(1)) || 0)) + 1;
      const id = (args.id as string | null) ?? `p${next}`;
      const profile: ColorProfile = { id, name: (args.name as string).trim() || "Sans nom", color: args.color as ColorAdjust, vibrance: args.vibrance as number };
      colorProfiles = colorProfiles.some((p) => p.id === id) ? colorProfiles.map((p) => (p.id === id ? profile : p)) : [...colorProfiles, profile];
      return structuredClone(profile);
    },
    apply_color_profile: async (args) => {
      await wait(plasma ? 100 : 700);
      const profile = colorProfiles.find((p) => p.id === args.id);
      const name = args.output as string;
      colors[name].adjust = structuredClone(profile?.color ?? { red: { ...neutral }, green: { ...neutral }, blue: { ...neutral } });
      if (!amd) colors[name].vibrance = profile?.vibrance ?? 50;
    },
    set_tray: (args) => {
      tray = args.shown as boolean;
    },
    delete_color_profile: async (args) => {
      await wait(150);
      colorProfiles = colorProfiles.filter((p) => p.id !== args.id);
    },
    save_game_settings: async (args) => {
      await wait(200);
      gameSettings = args.settings as GamesSettings;
    },
    performance_report: async () => {
      await wait(400);
      const open = (fix: string) => !perfFixed.has(fix);
      const checks = [
        ...(open("x3d-cache")
          ? [{ id: "x3d", level: "problem", title: "Les jeux tournent sans la 3D V-Cache", detail: "Processeur X3D en mode « frequency » : les jeux partent sur le bloc sans 3D V-Cache, beaucoup d'images en moins quand le processeur limite.", fix: "x3d-cache" }]
          : [{ id: "x3d", level: "ok", title: "Jeux sur la 3D V-Cache", detail: "Mode « cache ».", fix: null }]),
        ...(lowLatencyForced
          ? [{ id: "low-latency", level: "warning", title: "Mode de faible latence forcé (Last1 Panel)", detail: "Une seule image préparée d'avance : quand le processeur et la carte prennent chacun autant de temps (jeux Unreal Engine 5), ils s'attendent l'un l'autre et les images par seconde chutent.", fix: "low-latency" }]
          : []),
        ...(open("ntsync")
          ? [{ id: "ntsync", level: "warning", title: "NTSync absent", detail: "Proton synchronise alors les fils des jeux sans l'aide du noyau : plus de travail pour le processeur.", fix: "ntsync" }]
          : [{ id: "ntsync", level: "ok", title: "NTSync", detail: "Disponible pour Proton.", fix: null }]),
        ...(open("shader-cache")
          ? [{ id: "shader-cache", level: "warning", title: "Cache des shaders limité à 1 Go", detail: "Les shaders d'un gros jeu (Unreal Engine 5) n'y tiennent pas et se recompilent à chaque partie : saccades, chutes d'images par seconde.", fix: "shader-cache" }]
          : [{ id: "shader-cache", level: "info", title: "Cache des shaders agrandi à la prochaine connexion", detail: "C'est fait : Steam et ses jeux le prendront après un redémarrage du PC (ou une déconnexion-reconnexion).", fix: "restart" }]),
        ...(open("zram")
          ? [{ id: "swap", level: "info", title: "Conseil : mémoire bien remplie pendant les parties", detail: "3 Go de programmes en arrière-plan sont sur le disque pour laisser la mémoire au jeu. La mémoire compressée (zram, comme CachyOS) les garderait en mémoire, bien plus vite à reprendre.", fix: "zram" }]
          : []),
        { id: "rebar", level: "warning", title: "Resizable BAR désactivé", detail: "Le processeur ne voit la mémoire de la carte que par une fenêtre de 256 Mo, ce qui freine les jeux DirectX 12 (VKD3D-Proton). À activer dans le BIOS : Resizable BAR et Above 4G Decoding.", fix: null },
        ...(open("scheduler")
          ? [{ id: "scheduler", level: "info", title: "Ordonnanceur lavd actif", detail: "Conçu pour le Steam Deck : aucun gain d'images par seconde prouvé, et des chutes rapportées. go-to-linux garde celui du noyau.", fix: "scheduler" }]
          : []),
        { id: "power", level: "ok", title: "Mode performance", detail: "Processeur à pleine vitesse.", fix: null },
      ];
      const facts: [string, string][] = [
        ["Processeur", "AMD Ryzen 9 7950X3D 16-Core Processor"],
        ["Fréquence du processeur", "amd-pstate-epp / powersave / performance"],
        ["Profil d'énergie", "performance"],
        ["Ordonnanceur", open("scheduler") ? "lavd" : "celui du noyau"],
        ["Noyau", "6.17.2-2-cachyos"],
        ["Carte card1", "GeForce RTX 4070 (nvidia 595.58.03)"],
        ["Proton par défaut (Steam)", "proton-cachyos-slr"],
        ["Derniers jeux lancés", "ARC Raiders (proton-cachyos-slr), Cyberpunk 2077 (proton-cachyos-slr)"],
        ["Mémoire", "32 Go"],
      ];
      const sessions = [
        { program: "PioneerGame.exe", started: "2026-10-18 21:04", minutes: 32, fpsAverage: 58, fpsOnePercentLow: 31, cpuLoad: 45, gpuLoad: 60, gpuClock: 2610, limit: "cpu" },
        { program: "Cyberpunk2077.exe", started: "2026-10-17 20:12", minutes: 75, fpsAverage: 96, fpsOnePercentLow: 71, cpuLoad: 38, gpuLoad: 98, gpuClock: 2745, limit: "gpu" },
      ];
      const text = ["Last1 Panel — diagnostic des performances", "", ...facts.map(([n, v]) => `${n} : ${v}`), "", ...checks.map((c) => `[${c.level}] ${c.title} — ${c.detail}`)].join("\n");
      return { checks, facts, sessions, text, goToLinux: !params.get("arch") };
    },
    performance_fix: async (args) => {
      await wait(600);
      const fixes = args.fixes as string[];
      if (fixes.includes("restart")) {
        window.location.reload();
        return [];
      }
      for (const fix of fixes) perfFixed.add(fix);
      if (fixes.includes("low-latency")) lowLatencyForced = false;
      return fixes.includes("shader-cache") ? ["Cache des shaders agrandi : il vaudra après une déconnexion-reconnexion (ou un redémarrage)."] : [];
    },
    link_game_color: async (args) => {
      await wait(200);
      const id = String(args.appId);
      const next = profile({ ...gameSettings.perGame[id], colorProfile: (args.profile as string | null) ?? null });
      gameSettings.perGame[id] = next;
      if (JSON.stringify(next) === JSON.stringify(profile({}))) delete gameSettings.perGame[id];
      const game = games.find((g) => g.appId === args.appId);
      if (!args.profile || !game || game.wrapped) return { needsSteamClosed: false };
      if (steamRunning) return { needsSteamClosed: true };
      game.wrapped = true;
      game.launchOptions = `$(command -v last1-panel-cli && echo run) ${game.launchOptions || "%command%"}`;
      return { needsSteamClosed: false };
    },
    close_steam_and_wrap: async (args) => {
      await wait(1500);
      const ids = args.appIds as number[];
      for (const game of games.filter((g) => ids.includes(g.appId) && !g.wrapped)) {
        game.wrapped = true;
        game.launchOptions = `$(command -v last1-panel-cli && echo run) ${game.launchOptions || "%command%"}`;
      }
      return ids.length;
    },
    set_wrapped: async (args) => {
      await wait(steamRunning ? 1500 : 300);
      const ids = args.appIds as number[];
      let count = 0;
      for (const game of games) {
        if (!ids.includes(game.appId) || game.wrapped === args.wrapped) continue;
        game.wrapped = args.wrapped as boolean;
        game.launchOptions = game.wrapped ? `$(command -v last1-panel-cli && echo run) ${game.launchOptions || "%command%"}` : game.launchOptions.replace("$(command -v last1-panel-cli && echo run) ", "");
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
