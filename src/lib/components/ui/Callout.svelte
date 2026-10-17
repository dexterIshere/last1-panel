<!-- Encadré : information, réussite, avertissement ou erreur. -->
<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    kind = "info",
    title,
    children,
  }: { kind?: "info" | "success" | "warning" | "danger"; title?: string; children?: Snippet } = $props();
</script>

<div class="callout callout--{kind}" role={kind === "danger" ? "alert" : undefined}>
  {#if title}<strong class="callout__title">{title}</strong>{/if}
  {#if children}<div class="callout__body">{@render children()}</div>{/if}
</div>

<style>
  .callout {
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
    background: var(--soft);
  }

  .callout--info {
    --line: var(--color-pulse-line);
    --soft: var(--color-pulse-soft);
    --accent: var(--color-pulse);
  }

  .callout--success {
    --line: var(--color-volt-line);
    --soft: var(--color-volt-soft);
    --accent: var(--color-volt);
  }

  .callout--warning {
    --line: var(--color-warning-line);
    --soft: var(--color-warning-soft);
    --accent: var(--color-warning);
  }

  .callout--danger {
    --line: var(--color-danger-line);
    --soft: var(--color-danger-soft);
    --accent: var(--color-danger);
  }

  .callout__title {
    display: block;
    margin-bottom: var(--space-1);
    color: var(--accent);
    font-weight: 600;
  }

  .callout__body {
    color: var(--color-text-muted);
  }
</style>
