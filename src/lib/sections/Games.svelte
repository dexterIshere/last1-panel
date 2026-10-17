<!-- Jeux : « Paramètres 3D » globaux et par jeu, appliqués au lancement par last1-panel-cli. -->
<script lang="ts">
  import { api, errorMessage } from "$lib/api/ipc";
  import type { GameProfile, GamesInfo, LowLatency, Toggle } from "$lib/api/types";
  import SectionHeader from "$lib/components/layout/SectionHeader.svelte";
  import Callout from "$lib/components/ui/Callout.svelte";
  import Spinner from "$lib/components/ui/Spinner.svelte";
  import { t } from "$lib/i18n.svelte";
  import { app } from "$lib/state/app.svelte";
  import { Check, Copy, Search } from "@lucide/svelte";

  const EMPTY: GameProfile = {
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
  };

  let info = $state<GamesInfo | null>(null);
  let saved = $state("");
  /** null : tous les jeux. */
  let selected = $state<number | null>(null);
  let query = $state("");
  let loading = $state(true);
  let saving = $state(false);
  let wrapping = $state(false);
  let error = $state<string | null>(null);
  let notice = $state<string | null>(null);
  let copied = $state(false);

  const tools = $derived(app.system?.tools);
  const nvidia = $derived(app.nvidia);
  const games = $derived(info?.games.filter((g) => g.name.toLowerCase().includes(query.trim().toLowerCase())) ?? []);
  const game = $derived(info?.games.find((g) => g.appId === selected) ?? null);
  const dirty = $derived(info ? JSON.stringify(info.settings) !== saved : false);

  /** Profil affiché : global, ou celui du jeu (créé à la première modification). */
  const profile = $derived.by((): GameProfile => {
    if (!info) return EMPTY;
    if (selected === null) return info.settings.global;
    return info.settings.perGame[String(selected)] ?? { ...EMPTY };
  });

  async function load() {
    loading = true;
    try {
      info = await api.games();
      info.settings.global = { ...EMPTY, ...info.settings.global };
      saved = JSON.stringify(info.settings);
    } catch (e) {
      error = errorMessage(e);
    }
    loading = false;
  }

  $effect(() => {
    load();
  });

  function edit<K extends keyof GameProfile>(key: K, value: GameProfile[K]) {
    if (!info) return;
    if (selected === null) {
      info.settings.global[key] = value;
    } else {
      const id = String(selected);
      const next = { ...EMPTY, ...info.settings.perGame[id], [key]: value };
      info.settings.perGame[id] = next;
      // Jeu revenu entièrement aux réglages globaux : rien à garder.
      if (JSON.stringify(next) === JSON.stringify(EMPTY)) delete info.settings.perGame[id];
    }
  }

  async function save() {
    if (!info) return;
    saving = true;
    error = null;
    try {
      await api.saveGameSettings($state.snapshot(info.settings));
      saved = JSON.stringify(info.settings);
      notice = t("Enregistré : appliqué au prochain lancement du jeu.", "Saved: applied the next time the game starts.");
    } catch (e) {
      error = errorMessage(e);
    }
    saving = false;
  }

  async function setWrapped(ids: number[], wrapped: boolean) {
    wrapping = true;
    error = null;
    notice = null;
    try {
      const count = await api.setWrapped(ids, wrapped);
      const fresh = await api.games();
      if (info) {
        info.games = fresh.games;
        info.steamRunning = fresh.steamRunning;
      }
      notice = wrapped
        ? t(`Option de lancement mise (${count} changement${count > 1 ? "s" : ""}).`, `Launch option set (${count} change${count > 1 ? "s" : ""}).`)
        : t("Option de lancement retirée.", "Launch option removed.");
    } catch (e) {
      error = errorMessage(e);
    }
    wrapping = false;
  }

  async function copyOption() {
    if (!info) return;
    await navigator.clipboard.writeText(info.launchOption).catch(() => undefined);
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }

  // Choix à trois états : « comme tous les jeux » (jeu) ou « réglage du jeu » (global).
  const inherit = $derived(selected === null ? t("Réglage du jeu", "Game setting") : t("Comme tous les jeux", "Same as all games"));
  const boolValue = (v: boolean | null) => (v === null ? "inherit" : v ? "on" : "off");
  const fromBool = (v: string) => (v === "inherit" ? null : v === "on");
</script>

<SectionHeader
  title={t("Réglages 3D des jeux", "Game 3D settings")}
  lead={t(
    "Les « Paramètres 3D » du panneau NVIDIA : pour tous les jeux, ou jeu par jeu. Ils s'appliquent au lancement, par l'option Steam de Last1 Panel.",
    "The NVIDIA Control Panel “3D Settings”: for all games, or game by game. They apply at launch, through Last1 Panel's Steam launch option.",
  )}
>
  {#snippet actions()}
    <button class="btn btn--primary" onclick={save} disabled={!dirty || saving}>
      {#if saving}<Spinner />{/if}{t("Enregistrer", "Save")}
    </button>
  {/snippet}
</SectionHeader>

{#if error}<Callout kind="danger" title={t("Action impossible", "Action failed")}>{error}</Callout>{/if}
{#if notice && !error}<Callout kind="success">{notice}</Callout>{/if}

<!-- Choix oui/non : pour tous les jeux, « non » est la valeur par défaut ;
     pour un jeu, on peut aussi suivre « Tous les jeux ». -->
{#snippet boolOptions(on: string, off: string)}
  {#if selected === null}
    <option value="inherit">{off}</option>
    <option value="on">{on}</option>
  {:else}
    <option value="inherit">{inherit}</option>
    <option value="on">{on}</option>
    <option value="off">{off}</option>
  {/if}
{/snippet}

{#if loading}
  <p class="muted"><Spinner /> {t("Lecture de la bibliothèque Steam…", "Reading the Steam library…")}</p>
{:else if info}
  {#if info.steamRunning}
    <Callout kind="warning" title={t("Steam est ouvert", "Steam is open")}>
      {t(
        "Fermez Steam (menu Steam > Quitter) pour que Last1 Panel puisse mettre son option de lancement : Steam la remplacerait en se fermant. Les réglages, eux, s'enregistrent Steam ouvert.",
        "Quit Steam (Steam menu > Exit) so Last1 Panel can set its launch option: Steam would overwrite it when closing. Settings can be saved with Steam open.",
      )}
    </Callout>
  {/if}

  <div class="layout">
    <aside class="panel list">
      <label class="search">
        <Search size={16} />
        <input type="search" placeholder={t("Rechercher un jeu", "Search a game")} bind:value={query} />
      </label>
      <ul>
        <li>
          <button class="item" aria-current={selected === null ? "true" : undefined} onclick={() => (selected = null)}>
            <span class="item__name">{t("Tous les jeux", "All games")}</span>
            <span class="item__meta">{t("Réglages globaux", "Global settings")}</span>
          </button>
        </li>
        {#each games as entry (entry.appId)}
          <li>
            <button class="item" aria-current={selected === entry.appId ? "true" : undefined} onclick={() => (selected = entry.appId)}>
              <span class="item__name">{entry.name}</span>
              <span class="item__meta">
                {#if entry.wrapped}<span class="badge badge--success">{t("Actif", "Active")}</span>{/if}
                {#if info.settings.perGame[String(entry.appId)]}<span class="badge badge--info">{t("Réglé", "Custom")}</span>{/if}
              </span>
            </button>
          </li>
        {:else}
          <li class="empty muted">{info.steam ? t("Aucun jeu Steam installé.", "No Steam game installed.") : t("Steam n'est pas installé.", "Steam is not installed.")}</li>
        {/each}
      </ul>
    </aside>

    <section class="panel card">
      <div class="card__head">
        <div>
          <h2>{game ? game.name : t("Tous les jeux", "All games")}</h2>
          <p class="field__hint">
            {game
              ? t("Ce que vous ne réglez pas ici suit « Tous les jeux ».", "What you don't set here follows “All games”.")
              : t("Pour chaque jeu lancé avec Last1 Panel, sauf réglage propre au jeu.", "For every game launched with Last1 Panel, unless the game has its own setting.")}
          </p>
        </div>
        {#if game}
          <button class="btn" disabled={wrapping || info.steamRunning} onclick={() => setWrapped([game.appId], !game.wrapped)}>
            {#if wrapping}<Spinner />{/if}{game.wrapped ? t("Désactiver pour ce jeu", "Turn off for this game") : t("Activer pour ce jeu", "Turn on for this game")}
          </button>
        {:else}
          <button class="btn" disabled={wrapping || info.steamRunning || info.games.length === 0} onclick={() => setWrapped(info!.games.map((g) => g.appId), true)}>
            {#if wrapping}<Spinner />{/if}{t("Activer pour tous les jeux", "Turn on for all games")}
          </button>
        {/if}
      </div>

      <div class="grid">
        <label class="field">
          <span class="field__label">{t("Images par seconde max.", "Max frame rate")}</span>
          <select
            value={profile.fpsLimit === null ? "inherit" : String(profile.fpsLimit)}
            onchange={(e) => edit("fpsLimit", e.currentTarget.value === "inherit" ? null : Number(e.currentTarget.value))}
          >
            <option value="inherit">{selected === null ? t("Sans limite", "Unlimited") : inherit}</option>
            {#if selected !== null}<option value="0">{t("Sans limite", "Unlimited")}</option>{/if}
            {#each [30, 60, 72, 90, 117, 120, 141, 144, 162, 165, 180, 237, 240] as fps (fps)}<option value={String(fps)}>{fps}</option>{/each}
          </select>
          <span class="field__hint">{t("Avec G-SYNC, 3 de moins que la fréquence de l'écran.", "With G-SYNC, 3 below the screen's refresh rate.")}</span>
        </label>

        <label class="field">
          <span class="field__label">{t("Synchronisation verticale", "Vertical sync")}</span>
          <select value={profile.vsync} onchange={(e) => edit("vsync", e.currentTarget.value as Toggle)}>
            <option value="app">{inherit}</option>
            <option value="on">{t("Activée", "On")}</option>
            <option value="off">{t("Désactivée", "Off")}</option>
          </select>
        </label>

        <label class="field">
          <span class="field__label">{t("Mode de faible latence", "Low latency mode")}</span>
          <select value={profile.lowLatency} onchange={(e) => edit("lowLatency", e.currentTarget.value as LowLatency)}>
            <option value="app">{inherit}</option>
            <option value="on">{t("Activé", "On")}</option>
            {#if nvidia}<option value="ultra">{t("Ultra (avec NVIDIA Reflex)", "Ultra (with NVIDIA Reflex)")}</option>{/if}
          </select>
          <span class="field__hint">{t("Une seule image préparée d'avance.", "Only one frame prepared ahead.")}</span>
        </label>

        <label class="field">
          <span class="field__label">{t("Filtrage anisotrope", "Anisotropic filtering")}</span>
          <select
            value={profile.anisotropic === null ? "inherit" : String(profile.anisotropic)}
            onchange={(e) => edit("anisotropic", e.currentTarget.value === "inherit" ? null : Number(e.currentTarget.value))}
          >
            <option value="inherit">{inherit}</option>
            {#if selected !== null}<option value="0">{t("Réglage du jeu", "Game setting")}</option>{/if}
            {#each [2, 4, 8, 16] as level (level)}<option value={String(level)}>{level}×</option>{/each}
          </select>
        </label>

        {#if nvidia}
          <label class="field">
            <span class="field__label">{t("DLSS : dernière version", "DLSS: latest version")}</span>
            <select value={boolValue(profile.dlssLatest)} onchange={(e) => edit("dlssLatest", fromBool(e.currentTarget.value))}>
              {@render boolOptions(t("Activée", "On"), t("Version du jeu", "Game's version"))}
            </select>
            <span class="field__hint">{t("Comme « Remplacement DLSS » de l'application NVIDIA (RTX).", "Like the NVIDIA app's “DLSS Override” (RTX).")}</span>
          </label>

          <label class="field">
            <span class="field__label">{t("Indicateur DLSS", "DLSS indicator")}</span>
            <select value={boolValue(profile.dlssIndicator)} onchange={(e) => edit("dlssIndicator", fromBool(e.currentTarget.value))}>
              {@render boolOptions(t("Affiché", "Shown"), t("Caché", "Hidden"))}
            </select>
            <span class="field__hint">{t("Version du DLSS en jeu (Proton CachyOS).", "DLSS version in game (Proton CachyOS).")}</span>
          </label>

          <label class="field">
            <span class="field__label">Smooth Motion</span>
            <select value={boolValue(profile.smoothMotion)} onchange={(e) => edit("smoothMotion", fromBool(e.currentTarget.value))}>
              {@render boolOptions(t("Activé", "On"), t("Désactivé", "Off"))}
            </select>
            <span class="field__hint">{t("Images générées par le pilote (RTX 40 et plus récentes).", "Driver frame generation (RTX 40 and newer).")}</span>
          </label>
        {/if}

        <label class="field">
          <span class="field__label">HDR</span>
          <select value={boolValue(profile.hdr)} onchange={(e) => edit("hdr", fromBool(e.currentTarget.value))}>
            {@render boolOptions(t("Activé", "On"), t("Automatique", "Automatic"))}
          </select>
          <span class="field__hint">{t("Écran en HDR (Affichage) et Proton CachyOS.", "Screen in HDR (Display) and Proton CachyOS.")}</span>
        </label>

        <label class="field">
          <span class="field__label">{t("Couleurs dans le jeu", "In-game colors")}</span>
          <select
            value={profile.colorProfile === null ? "inherit" : profile.colorProfile === "" ? "screen" : profile.colorProfile}
            disabled={!info.colorLayer || (info.colorProfiles.length === 0 && profile.colorProfile === null)}
            onchange={(e) => {
              const value = e.currentTarget.value;
              edit("colorProfile", value === "inherit" ? null : value === "screen" ? "" : value);
            }}
          >
            <option value="inherit">{selected === null ? t("Celles de l'écran", "The screen's") : inherit}</option>
            {#if selected !== null}<option value="screen">{t("Celles de l'écran", "The screen's")}</option>{/if}
            {#each info.colorProfiles as color (color.id)}<option value={color.id}>{color.name}</option>{/each}
          </select>
          <span class="field__hint">
            {#if !info.colorLayer}
              {t("Couche Vulkan vkBasalt de Last1 Panel absente : réinstallez Last1 Panel.", "Last1 Panel's vkBasalt Vulkan layer is missing: reinstall Last1 Panel.")}
            {:else if info.colorProfiles.length === 0}
              {t("Enregistrez d'abord un profil dans Couleurs.", "Save a profile in Colors first.")}
            {:else}
              {t("Dans la seule fenêtre du jeu (Proton et jeux Vulkan). Touche Arrêt défil : profil coupé ou remis.", "In the game window only (Proton and Vulkan games). Scroll Lock key: profile off or back on.")}
            {/if}
          </span>
        </label>

        <label class="field">
          <span class="field__label">{t("Performances à l'écran", "On-screen performance")}</span>
          <select value={boolValue(profile.overlay)} disabled={!tools?.mangohud} onchange={(e) => edit("overlay", fromBool(e.currentTarget.value))}>
            {@render boolOptions(t("Affiché", "Shown"), t("Caché", "Hidden"))}
          </select>
          <span class="field__hint">{tools?.mangohud ? t("Images par seconde, températures (MangoHud).", "Frame rate, temperatures (MangoHud).") : t("MangoHud n'est pas installé.", "MangoHud is not installed.")}</span>
        </label>

        <label class="field">
          <span class="field__label">{t("Mode performance", "Performance mode")}</span>
          <select value={boolValue(profile.performanceMode)} disabled={!tools?.gamePerformance} onchange={(e) => edit("performanceMode", fromBool(e.currentTarget.value))}>
            {@render boolOptions(t("Activé", "On"), t("Désactivé", "Off"))}
          </select>
          <span class="field__hint">{t("Processeur à pleine vitesse pendant le jeu.", "CPU at full speed while playing.")}</span>
        </label>
      </div>

      <div class="option">
        <span class="label">{t("Option de lancement Steam", "Steam launch option")}</span>
        <div class="option__row">
          <code class="mono">{info.launchOption}</code>
          <button class="btn btn--ghost" onclick={copyOption} aria-label={t("Copier", "Copy")}>
            {#if copied}<Check size={16} />{:else}<Copy size={16} />{/if}
          </button>
        </div>
        <span class="field__hint">
          {game?.launchOptions
            ? t(`Option actuelle de ce jeu : ${game.launchOptions}`, `This game's current option: ${game.launchOptions}`)
            : t("Mise par « Activer », ou à coller dans Propriétés > Options de lancement.", "Set by “Turn on”, or paste it in Properties > Launch options.")}
        </span>
      </div>
    </section>
  </div>
{/if}

<style>
  .layout {
    display: grid;
    grid-template-columns: 17rem minmax(0, 1fr);
    gap: var(--space-4);
    align-items: start;
  }

  .list {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: var(--space-2);
    padding: var(--space-3);
  }

  .search {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding-left: var(--space-2);
    color: var(--color-text-faint);
  }

  .search input {
    flex: 1;
    min-width: 0;
  }

  ul {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 2px;
    max-height: 32rem;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    list-style: none;
  }

  .item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    width: 100%;
    padding: var(--space-2) var(--space-3);
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: transparent;
    text-align: left;
    cursor: pointer;
  }

  .item:hover {
    background: var(--color-surface-raised);
  }

  .item[aria-current="true"] {
    border-color: var(--color-panel-line);
    background: var(--color-panel-soft);
  }

  .item__name {
    min-width: 0;
    overflow: hidden;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .item__meta {
    display: flex;
    flex: none;
    gap: var(--space-1);
    color: var(--color-text-faint);
    font-size: var(--text-xs);
  }

  .empty {
    padding: var(--space-3);
  }

  .card {
    display: grid;
    gap: var(--space-5);
    padding: var(--space-5);
  }

  .card__head {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--space-3);
  }

  h2 {
    font-size: var(--text-lg);
  }

  .grid {
    display: grid;
    align-items: start;
    grid-template-columns: repeat(auto-fill, minmax(15rem, 1fr));
    gap: var(--space-4) var(--space-5);
  }

  .option {
    display: grid;
    gap: var(--space-2);
    padding-top: var(--space-4);
    border-top: 1px solid var(--color-border);
  }

  .option__row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  code {
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-bg);
    color: var(--color-volt);
    user-select: all;
  }
</style>
