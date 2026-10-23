<!-- Clips : c'est Vice qui les fait (installé d'office par go-to-linux) ; la
     page l'ouvre, le démarre s'il est arrêté, ou dit où l'installer. -->
<script lang="ts">
  import { api, errorMessage } from "$lib/api/ipc";
  import type { ClipsState } from "$lib/api/types";
  import SectionHeader from "$lib/components/layout/SectionHeader.svelte";
  import Callout from "$lib/components/ui/Callout.svelte";
  import Spinner from "$lib/components/ui/Spinner.svelte";
  import { t } from "$lib/i18n.svelte";
  import { ExternalLink, FolderOpen } from "@lucide/svelte";
  import { onMount } from "svelte";

  let info = $state<ClipsState | null>(null);
  let loading = $state(true);
  let starting = $state(false);
  let error = $state<string | null>(null);

  async function load() {
    try {
      info = await api.clipsState();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      loading = false;
    }
  }

  async function open(target: "vice" | "folder" | "go-to-linux" | "web") {
    error = null;
    try {
      await api.openClips(target);
    } catch (e) {
      error = errorMessage(e);
    }
  }

  async function start() {
    starting = true;
    error = null;
    try {
      await api.startVice();
      await load();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      starting = false;
    }
  }

  onMount(load);
</script>

<SectionHeader
  title={t("Clips", "Clips")}
  lead={t(
    "Vice garde les derniers instants de vos parties, comme Medal ou ShadowPlay : une touche, et le moment est enregistré. La carte graphique encode elle-même : presque rien en images par seconde.",
    "Vice keeps the last moments of your games, like Medal or ShadowPlay: one key, and the moment is saved. The graphics card encodes it itself: next to nothing in frames per second.",
  )}
/>

{#if error}<Callout kind="danger" title={t("Refusé", "Refused")}>{error}</Callout>{/if}

{#if loading}
  <p class="muted"><Spinner /> {t("Recherche de Vice…", "Looking for Vice…")}</p>
{:else if info}
  {#if !info.installed}
    <section class="panel card row">
      <div>
        <h2>{t("Installer Vice", "Install Vice")}</h2>
        <p class="muted">
          {info.goToLinux
            ? t("Vice s'installe d'un clic dans go-to-linux, rubrique Applications.", "Vice installs in one click in go-to-linux, Applications section.")
            : t("Vice est un logiciel libre pour Linux ; sa page explique comment l'installer.", "Vice is free software for Linux; its page explains how to install it.")}
        </p>
      </div>
      {#if info.goToLinux}
        <button class="btn btn--primary" onclick={() => open("go-to-linux")}>{t("Ouvrir go-to-linux", "Open go-to-linux")}</button>
      {:else}
        <button class="btn btn--primary" onclick={() => open("web")}><ExternalLink size={16} />{t("Page de Vice", "Vice's page")}</button>
      {/if}
    </section>
  {:else}
    {#if !info.running}
      <section class="panel card row">
        <div>
          <h2>{t("Vice est arrêté", "Vice is stopped")}</h2>
          <p class="muted">{t("Ses touches ne marchent pas tant qu'il ne tourne pas.", "Its keys don't work while it isn't running.")}</p>
        </div>
        <button class="btn btn--primary" onclick={start} disabled={starting}>
          {#if starting}<Spinner />{/if}{t("Démarrer Vice", "Start Vice")}
        </button>
      </section>
    {/if}
    <section class="panel card">
      <div class="keys">
        <span class="key mono">F9</span><span>{t("les 20 dernières secondes", "the last 20 seconds")}</span>
        <span class="key mono">F9 ×2</span><span>{t("toute la partie", "the whole game")}</span>
      </div>
      <p class="field__hint">{t("Les touches et la durée se changent dans Vice.", "Keys and length are changed in Vice.")}</p>
      <div class="actions">
        <button class="btn btn--primary" onclick={() => open("vice")}>{t("Ouvrir Vice", "Open Vice")}</button>
        <button class="btn" onclick={() => open("folder")}><FolderOpen size={16} />{t("Dossier des clips", "Clips folder")}</button>
      </div>
      <p class="field__hint mono">{info.folder}</p>
    </section>
  {/if}
{/if}

<style>
  .card {
    display: grid;
    gap: var(--space-4);
    padding: var(--space-5);
  }

  .row {
    grid-template-columns: 1fr auto;
    align-items: center;
  }

  h2 {
    margin: 0 0 var(--space-1);
    font-size: var(--text-lg);
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

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
</style>
