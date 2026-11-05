<!-- En-tête d'une page : grand titre (capitales par le CSS ; « / » dans le
     titre devient un retour à la ligne), phrase d'explication, actions.
     `pill` : la pastille à trois cœurs, à la fin de la première ligne du
     titre (premier écran seulement). -->
<script lang="ts">
  import type { Snippet } from "svelte";
  import PixelPill from "$lib/components/ui/PixelPill.svelte";

  let { title, lead, actions, pill = false }: { title: string; lead: string; actions?: Snippet; pill?: boolean } = $props();

  /** Lignes du titre, coupées aux « / » (docs/refonte/textes-panel.md). */
  const lines = $derived(title.split(" / "));
  /** Le dernier mot de la première ligne reste avec la pastille. */
  const first = $derived.by(() => {
    const at = lines[0].lastIndexOf(" ");
    return at < 0 ? ["", lines[0]] : [lines[0].slice(0, at + 1), lines[0].slice(at + 1)];
  });
</script>

<header class="head">
  <div class="head__text">
    <h1 class="display-title">
      {#each lines as line, index (index)}
        {#if index > 0}<br />{/if}
        {#if index === 0 && pill}{first[0]}<span class="nowrap">{first[1]} <PixelPill /></span>{:else}{line}{/if}
      {/each}
    </h1>
    <p class="muted">{lead}</p>
  </div>
  {#if actions}<div class="head__actions">{@render actions()}</div>{/if}
</header>

<style>
  /* Fenêtre étroite ou textes anglais plus longs : les actions passent sous
     la phrase plutôt que de sortir de la fenêtre. */
  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    justify-content: space-between;
    gap: var(--space-4) var(--space-5);
  }

  .head__text {
    flex: 1 1 26rem;
    min-width: 0;
  }

  h1 {
    margin-bottom: var(--space-2);
  }

  p {
    max-width: 44rem;
  }

  .head__actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }
</style>
