<!-- Nouveautés de la version qui vient d'être installée, jusqu'à ce que le
     joueur les ferme. -->
<script lang="ts">
  import { api } from "$lib/api/ipc";
  import type { News } from "$lib/api/types";
  import { t } from "$lib/i18n.svelte";
  import { Sparkles, X } from "@lucide/svelte";
  import { onMount } from "svelte";

  let news = $state<News | null>(null);

  onMount(() => {
    api.news().then((next) => (news = next), () => undefined);
  });

  function close() {
    news = null;
    api.dismissNews().catch(() => undefined);
  }
</script>

{#if news}
  <section class="news" aria-label={t("Nouveautés", "What's new")}>
    <div class="news__head">
      <strong><Sparkles size={16} />{t(`Nouveautés de la version ${news.version}`, `What's new in version ${news.version}`)}</strong>
      <button class="news__close" onclick={close} aria-label={t("Fermer", "Close")}><X size={16} /></button>
    </div>
    <ul>
      {#each news.notes as note, i (i)}<li>{note}</li>{/each}
    </ul>
  </section>
{/if}

<style>
  .news {
    display: grid;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--color-panel-line);
    border-radius: var(--radius-md);
    background: var(--color-panel-soft);
  }

  .news__head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
  }

  .news__head strong {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--color-panel);
  }

  .news__close {
    padding: var(--space-1);
    border: 0;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
  }

  ul {
    display: grid;
    gap: var(--space-1);
    margin: 0;
    padding-left: var(--space-5);
    color: var(--color-text);
  }
</style>
