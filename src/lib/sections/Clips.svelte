<!-- Clips : comme Medal, F8 garde la dernière minute du jeu en cours, F9
     lance ou arrête un enregistrement ; les vidéos et leur dossier. -->
<script lang="ts">
  import { api, errorMessage } from "$lib/api/ipc";
  import type { Clip, ClipsState } from "$lib/api/types";
  import SectionHeader from "$lib/components/layout/SectionHeader.svelte";
  import Callout from "$lib/components/ui/Callout.svelte";
  import Spinner from "$lib/components/ui/Spinner.svelte";
  import Switch from "$lib/components/ui/Switch.svelte";
  import { i18n, num, t } from "$lib/i18n.svelte";
  import { app } from "$lib/state/app.svelte";
  import { FolderOpen, Play, Trash2 } from "@lucide/svelte";
  import { onDestroy, onMount } from "svelte";

  let info = $state<ClipsState | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let installing = $state(false);
  let enabled = $state(true);
  let timer: ReturnType<typeof setInterval> | undefined;

  const ready = $derived(!!info && info.installed && info.hotkeys);

  async function load() {
    try {
      info = await api.clipsState();
      enabled = info.enabled;
    } catch (e) {
      error = errorMessage(e);
    } finally {
      loading = false;
    }
  }

  async function install() {
    installing = true;
    error = null;
    try {
      await api.installClips();
      await load();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      installing = false;
    }
  }

  async function act(work: () => Promise<void>) {
    error = null;
    try {
      await work();
    } catch (e) {
      error = errorMessage(e);
    }
  }

  async function remove(clip: Clip) {
    await act(() => api.deleteClip(clip.path));
    await load();
  }

  const size = (bytes: number) =>
    bytes >= 1e9 ? `${num(bytes / 1e9, 1)} Go` : `${num(Math.max(1, Math.round(bytes / 1e6)))} Mo`;
  const date = (seconds: number) =>
    new Date(seconds * 1000).toLocaleString(i18n.lang === "fr" ? "fr-FR" : "en-US", { dateStyle: "medium", timeStyle: "short" });

  onMount(() => {
    load();
    // Les clips enregistrés pendant que la page est ouverte apparaissent.
    timer = setInterval(() => api.clipsState().then((next) => info && (info = { ...next, enabled })).catch(() => undefined), 5_000);
  });
  onDestroy(() => clearInterval(timer));
</script>

<SectionHeader
  title={t("Clips", "Clips")}
  lead={t(
    "Comme Medal : pendant un jeu, F8 enregistre la dernière minute, F9 lance ou arrête une vidéo de la partie. La carte graphique encode elle-même : presque rien en images par seconde.",
    "Like Medal: during a game, F8 saves the last minute, F9 starts or stops a video of the game. The graphics card encodes it itself: next to nothing in frames per second.",
  )}
>
  {#snippet actions()}
    {#if info}
      <button class="btn" onclick={() => act(() => api.openClip(info!.folder))}><FolderOpen size={16} />{t("Ouvrir le dossier", "Open the folder")}</button>
    {/if}
  {/snippet}
</SectionHeader>

{#if error}<Callout kind="danger" title={t("Refusé", "Refused")}>{error}</Callout>{/if}

{#if loading}
  <p class="muted"><Spinner /> {t("Lecture des clips…", "Reading clips…")}</p>
{:else if info}
  {#if info.vice}
    <section class="panel card setup">
      <div>
        <h2>{t("Vice fait vos clips", "Vice makes your clips")}</h2>
        <p class="muted">
          {t(
            "Vous avez installé Vice : il garde ses touches et ses réglages, Last1 Panel n'enregistre rien pour ne pas le gêner.",
            "You installed Vice: it keeps its keys and settings, Last1 Panel records nothing so as not to get in its way.",
          )}
        </p>
      </div>
      <div class="actions">
        {#if !info.hotkeys && app.system?.tools.pacman !== false}
          <!-- Sa règle udev part si son paquet est retiré : la nôtre ouvre les mêmes claviers. -->
          <button class="btn btn--primary" onclick={install} disabled={installing}>
            {#if installing}<Spinner />{/if}{t("Rendre ses touches à Vice", "Give Vice its keys back")}
          </button>
        {/if}
        <button class={info.hotkeys ? "btn btn--primary" : "btn"} onclick={() => act(() => api.openVice())}>{t("Ouvrir Vice", "Open Vice")}</button>
      </div>
    </section>
  {:else if !ready}
    <section class="panel card setup">
      <div>
        <h2>{t("Activer les clips", "Turn on clips")}</h2>
        <p class="muted">
          {t(
            "Last1 Panel installe l'enregistreur (gpu-screen-recorder) et lui permet d'entendre F8 et F9, même quand le jeu garde le clavier. Le mot de passe est demandé une fois.",
            "Last1 Panel installs the recorder (gpu-screen-recorder) and lets it hear F8 and F9, even when the game keeps the keyboard. The password is asked once.",
          )}
        </p>
      </div>
      {#if app.system?.tools.pacman !== false}
        <button class="btn btn--primary" onclick={install} disabled={installing}>
          {#if installing}<Spinner />{t("Installation…", "Installing…")}{:else}{t("Activer", "Turn on")}{/if}
        </button>
      {/if}
    </section>
  {:else}
    <section class="panel card">
      <Switch
        label={t("Clips pendant les jeux", "Clips during games")}
        bind:checked={enabled}
        onchange={(on) => act(() => api.setClips(on))}
        hint={enabled
          ? t(
              "F8 : la dernière minute. F9 : lancer ou arrêter une vidéo. Une notification confirme chaque vidéo.",
              "F8: the last minute. F9: start or stop a video. A notification confirms each video.",
            )
          : t("Coupés : rien n'est gardé pendant les jeux.", "Off: nothing is kept during games.")}
      />
      <div class="keys">
        <span class="key mono">F8</span><span>{t("les 60 dernières secondes", "the last 60 seconds")}</span>
        <span class="key mono">F9</span><span>{t("lancer / arrêter une vidéo", "start / stop a video")}</span>
      </div>
    </section>
  {/if}

  <section class="panel card">
    <h2>{t("Vidéos", "Videos")}</h2>
    {#if info.clips.length === 0}
      <p class="muted">{t("Aucune vidéo pour l'instant. Pendant un jeu, appuyez sur F8.", "No video yet. During a game, press F8.")}</p>
    {:else}
      <ul class="clips">
        {#each info.clips as clip (clip.path)}
          <li class="clip">
            <button class="clip__play" onclick={() => act(() => api.openClip(clip.path))} aria-label={t(`Voir ${clip.name}`, `Watch ${clip.name}`)}>
              <Play size={18} />
            </button>
            <div class="clip__text">
              <strong>{clip.name}</strong>
              <span class="field__hint">{date(clip.modified)} · {size(clip.size)}</span>
            </div>
            <button class="clip__delete" onclick={() => remove(clip)} aria-label={t(`Supprimer ${clip.name}`, `Delete ${clip.name}`)} title={t("Supprimer", "Delete")}>
              <Trash2 size={16} />
            </button>
          </li>
        {/each}
      </ul>
    {/if}
    <p class="field__hint mono">{info.folder}</p>
  </section>
{/if}

<style>
  .card {
    display: grid;
    gap: var(--space-4);
    padding: var(--space-5);
  }

  h2 {
    margin: 0;
    font-size: var(--text-lg);
  }

  .setup {
    grid-template-columns: 1fr auto;
    align-items: center;
  }

  .keys {
    display: grid;
    grid-template-columns: auto 1fr;
    align-items: center;
    gap: var(--space-2) var(--space-3);
    color: var(--color-text-muted);
  }

  .key {
    padding: 2px var(--space-2);
    border: 1px solid var(--color-border-strong);
    border-bottom-width: 3px;
    border-radius: var(--radius-sm);
    color: var(--color-text);
    font-weight: 600;
    text-align: center;
  }

  .clips {
    display: grid;
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .clip {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
  }

  .clip__play {
    display: grid;
    place-items: center;
    width: 2.25rem;
    height: 2.25rem;
    flex: none;
    border: 1px solid var(--color-panel-line);
    border-radius: 50%;
    background: var(--color-panel-soft);
    color: var(--color-panel);
    cursor: pointer;
  }

  .clip__text {
    display: grid;
    flex: 1;
    min-width: 0;
  }

  .clip__text strong {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .clip__delete {
    padding: var(--space-2);
    border: 0;
    background: transparent;
    color: var(--color-text-faint);
    cursor: pointer;
  }

  .clip__delete:hover {
    color: var(--color-danger);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
</style>
