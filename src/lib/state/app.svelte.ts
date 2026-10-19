// État de la fenêtre : le PC (system_info), la page affichée, la langue.

import { api, errorMessage } from "$lib/api/ipc";
import type { SystemInfo } from "$lib/api/types";
import { i18n, saveLang, savedLang, t, type Lang } from "$lib/i18n.svelte";

export type Section = "colors" | "display" | "gpu" | "games" | "perf";

class App {
  system = $state<SystemInfo | null>(null);
  section = $state<Section>("colors");
  /** Erreur au démarrage (le reste de l'interface reste utilisable). */
  error = $state<string | null>(null);
  /** Version plus récente sur last1.fun (installation par l'AppImage). */
  update = $state<string | null>(null);

  async init(): Promise<void> {
    try {
      this.system = await api.systemInfo();
      i18n.lang = savedLang() ?? this.system.lang;
      await api.setLanguage(i18n.lang);
      api.updateAvailable().then((version) => (this.update = version), () => undefined);
    } catch (error) {
      this.error = errorMessage(error);
    }
  }

  async setLanguage(lang: Lang): Promise<void> {
    i18n.lang = lang;
    saveLang(lang);
    await api.setLanguage(lang).catch(() => undefined);
  }

  /** La carte NVIDIA du PC, s'il en a une. */
  get nvidia(): boolean {
    return this.system?.gpus.some((gpu) => gpu.vendor === "nvidia") ?? false;
  }

  /** Carte AMD, sans NVIDIA. */
  get amd(): boolean {
    return !this.nvidia && (this.system?.gpus.some((gpu) => gpu.vendor === "amd") ?? false);
  }

  /** Nom de la fréquence variable chez le fabricant de la carte. */
  get vrrName(): string {
    return this.nvidia ? "G-SYNC" : this.amd ? "FreeSync" : "VRR";
  }

  /** Le logiciel du fabricant que Last1 Panel remplace. */
  get vendorPanel(): string {
    return this.amd ? t("le logiciel AMD (Adrenalin)", "the AMD Software (Adrenalin)") : t("le panneau NVIDIA", "the NVIDIA Control Panel");
  }
}

export const app = new App();
