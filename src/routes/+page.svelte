<script lang="ts">
  import Sidebar from "$lib/components/layout/Sidebar.svelte";
  import Callout from "$lib/components/ui/Callout.svelte";
  import Clips from "$lib/sections/Clips.svelte";
  import NewsBanner from "$lib/components/NewsBanner.svelte";
  import Colors from "$lib/sections/Colors.svelte";
  import Display from "$lib/sections/Display.svelte";
  import Games from "$lib/sections/Games.svelte";
  import Mouse from "$lib/sections/Mouse.svelte";
  import GpuTuning from "$lib/sections/Gpu.svelte";
  import Performance from "$lib/sections/Performance.svelte";
  import ShaderBanner from "$lib/components/ShaderBanner.svelte";
  import SharingDialog from "$lib/components/SharingDialog.svelte";
  import { t } from "$lib/i18n.svelte";
  import { app } from "$lib/state/app.svelte";
  import { isTauri } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  let ready = $state(false);
  /** Accord de partage des rapports à demander (une fois, hors go-to-linux). */
  let askSharing = $state(false);

  onMount(async () => {
    // Hors de Tauri, `vite dev` tourne avec un PC simulé : l'interface se
    // travaille dans un navigateur. Retiré des versions compilées.
    if (import.meta.env.DEV && !isTauri()) {
      const { installMockBackend } = await import("$lib/dev/mock-backend");
      installMockBackend();
    }
    await app.init();
    ready = true;
    askSharing = !!app.sharing && !app.sharing.asked && !app.sharing.goToLinux;
  });
</script>

<div class="app">
  <Sidebar />
  <main class="main">
    {#if app.error}
      <Callout kind="danger" title={t("Last1 Panel n'a pas pu lire ce PC", "Last1 Panel couldn't read this PC")}>{app.error}</Callout>
    {/if}
    {#if ready}
      <NewsBanner />
      <ShaderBanner />
    {/if}
    <svelte:boundary>
      {#if ready}
        {#if app.section === "colors"}
          <Colors />
        {:else if app.section === "display"}
          <Display />
        {:else if app.section === "mouse"}
          <Mouse />
        {:else if app.section === "gpu"}
          <GpuTuning />
        {:else if app.section === "clips"}
          <Clips />
        {:else if app.section === "perf"}
          <Performance />
        {:else}
          <Games />
        {/if}
      {/if}
      {#snippet failed(error, reset)}
        <section class="crash" role="alert">
          <h2>{t("Cette page n'a pas pu s'afficher", "This page couldn't be displayed")}</h2>
          <p class="muted">{error instanceof Error ? error.message : String(error)}</p>
          <button class="btn btn--primary" onclick={reset}>{t("Réessayer", "Try again")}</button>
        </section>
      {/snippet}
    </svelte:boundary>
  </main>
</div>

{#if askSharing}
  <SharingDialog
    onanswer={(share) => {
      askSharing = false;
      app.setSharing(share).catch(() => undefined);
    }}
  />
{/if}

<style>
  .app {
    display: flex;
    height: 100vh;
  }

  .main {
    display: grid;
    flex: 1;
    align-content: start;
    gap: var(--space-5);
    min-width: 0;
    padding: var(--space-6) var(--space-6) var(--space-7);
    overflow-y: auto;
  }

  .crash {
    display: grid;
    justify-items: start;
    gap: var(--space-3);
  }
</style>
