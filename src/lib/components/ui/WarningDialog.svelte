<!-- Mise en garde à accepter avant un réglage risqué (Annuler par défaut). -->
<script lang="ts">
  import { t } from "$lib/i18n.svelte";
  import { TriangleAlert } from "@lucide/svelte";
  import type { Snippet } from "svelte";

  let {
    title,
    confirmLabel,
    children,
    onconfirm,
    oncancel,
  }: { title: string; confirmLabel: string; children: Snippet; onconfirm: () => void; oncancel: () => void } = $props();
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && oncancel()} />

<div class="backdrop">
  <div class="dialog panel" role="alertdialog" aria-modal="true" aria-labelledby="warning-title" aria-describedby="warning-body">
    <h2 id="warning-title"><TriangleAlert size={20} />{title}</h2>
    <div id="warning-body" class="body">{@render children()}</div>
    <div class="actions">
      <!-- svelte-ignore a11y_autofocus -->
      <button class="btn" autofocus onclick={oncancel}>{t("Annuler", "Cancel")}</button>
      <button class="btn btn--danger" onclick={onconfirm}>{confirmLabel}</button>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 10;
    display: grid;
    place-items: center;
    background: rgb(5 6 8 / 0.7);
  }

  .dialog {
    display: grid;
    gap: var(--space-3);
    width: min(34rem, calc(100vw - 2rem));
    padding: var(--space-5);
  }

  h2 {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--color-warning);
    font-size: var(--text-lg);
  }

  .body {
    display: grid;
    gap: var(--space-2);
    color: var(--color-text-muted);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: var(--space-2);
    margin-top: var(--space-2);
  }
</style>
