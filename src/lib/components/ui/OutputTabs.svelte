<!-- Choix de l'écran : nom de l'écran et sortie (DP-1…). -->
<script lang="ts">
  import type { Output } from "$lib/api/types";
  import { t } from "$lib/i18n.svelte";
  import { Monitor } from "@lucide/svelte";

  let { outputs, selected = $bindable() }: { outputs: Output[]; selected: string } = $props();
</script>

<div class="tabs" role="tablist" aria-label={t("Écrans", "Screens")}>
  {#each outputs as output (output.name)}
    <button class="tab" role="tab" aria-selected={selected === output.name} onclick={() => (selected = output.name)}>
      <Monitor size={16} />
      <span class="tab__text">
        <span class="tab__label">{output.label}</span>
        <span class="tab__name mono">{output.name}{output.enabled ? "" : ` · ${t("éteint", "off")}`}</span>
      </span>
    </button>
  {/each}
</div>

<style>
  .tabs {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .tab {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-4) var(--space-2) var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text-muted);
    text-align: left;
    cursor: pointer;
  }

  .tab:hover {
    border-color: var(--color-border-strong);
  }

  .tab[aria-selected="true"] {
    border-color: var(--color-panel-line);
    background: var(--color-panel-soft);
    color: var(--color-panel);
  }

  .tab__text {
    display: grid;
  }

  .tab__label {
    color: var(--color-text);
    font-weight: 600;
  }

  .tab__name {
    color: var(--color-text-faint);
    font-size: var(--text-2xs);
  }
</style>
