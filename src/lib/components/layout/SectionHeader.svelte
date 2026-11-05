<!-- En-tête d'une page : grand titre (capitales par le CSS), phrase
     d'explication, actions. `pill` : la pastille à trois cœurs, à la fin de
     la première ligne du titre (premier écran seulement). -->
<script lang="ts">
  import type { Snippet } from "svelte";
  import PixelPill from "$lib/components/ui/PixelPill.svelte";

  let { title, lead, actions, pill = false }: { title: string; lead: string; actions?: Snippet; pill?: boolean } = $props();

  /** Le dernier mot reste avec la pastille. */
  const split = $derived.by(() => {
    const at = title.lastIndexOf(" ");
    return at < 0 ? ["", title] : [title.slice(0, at + 1), title.slice(at + 1)];
  });
</script>

<header class="head">
  <div>
    <h1 class="display-title">
      {#if pill}{split[0]}<span class="nowrap">{split[1]} <PixelPill /></span>{:else}{title}{/if}
    </h1>
    <p class="muted">{lead}</p>
  </div>
  {#if actions}<div class="head__actions">{@render actions()}</div>{/if}
</header>

<style>
  .head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: var(--space-5);
  }

  h1 {
    margin-bottom: var(--space-2);
  }

  p {
    max-width: 44rem;
  }

  .head__actions {
    display: flex;
    flex: none;
    gap: var(--space-2);
  }
</style>
