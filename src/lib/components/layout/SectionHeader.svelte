<!-- En-tête d'une page : titre, phrase d'explication, actions. -->
<script lang="ts">
  import type { Snippet } from "svelte";

  let { title, lead, actions }: { title: string; lead: string; actions?: Snippet } = $props();
</script>

<header class="head">
  <div>
    <h1>{title}</h1>
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
    margin-bottom: var(--space-1);
    font-size: var(--text-2xl);
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
