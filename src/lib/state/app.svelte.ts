// État de la fenêtre : le PC (system_info), la page affichée, la langue.

import { api, errorMessage } from "$lib/api/ipc";
import type { Gpu, Sharing, SystemInfo } from "$lib/api/types";
import { i18n, saveLang, savedLang, t, type Lang } from "$lib/i18n.svelte";

export type Section = "colors" | "display" | "mouse" | "gpu" | "games" | "clips" | "perf";

class App {
  system = $state<SystemInfo | null>(null);
  section = $state<Section>("colors");
  /** Erreur au démarrage (le reste de l'interface reste utilisable). */
  error = $state<string | null>(null);
  /** Version plus récente sur last1.fun (installation par l'AppImage). */
  update = $state<string | null>(null);
  /** Partage des rapports de performances (accord du joueur). */
  sharing = $state<Sharing | null>(null);

  async init(): Promise<void> {
    try {
      this.system = await api.systemInfo();
      i18n.lang = savedLang() ?? this.system.lang;
      await api.setLanguage(i18n.lang);
      api.updateAvailable().then((version) => (this.update = version), () => undefined);
      this.sharing = await api.sharing().catch(() => null);
    } catch (error) {
      this.error = errorMessage(error);
    }
  }

  async setLanguage(lang: Lang): Promise<void> {
    i18n.lang = lang;
    saveLang(lang);
    await api.setLanguage(lang).catch(() => undefined);
  }

  /** Réponse du joueur au partage des rapports (gardée, révocable). */
  async setSharing(enabled: boolean): Promise<void> {
    if (this.sharing) this.sharing = { ...this.sharing, asked: true, enabled };
    await api.setSharing(enabled);
  }

  /** La carte des jeux : celle dont parlent les réglages (Reflex, G-SYNC…),
   * quel que soit le nombre de cartes du PC. */
  get gamesGpu(): Gpu | null {
    return this.system?.gpus.find((gpu) => gpu.games) ?? this.system?.gpus[0] ?? null;
  }

  /** La carte des jeux est une NVIDIA. */
  get nvidia(): boolean {
    return this.gamesGpu?.vendor === "nvidia";
  }

  /** La carte des jeux est une AMD. */
  get amd(): boolean {
    return this.gamesGpu?.vendor === "amd";
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
