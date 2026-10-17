<!-- « Garder ces réglages ? » avec retour automatique, comme sous Windows. -->
<script lang="ts">
  import { t } from "$lib/i18n.svelte";
  import { onDestroy, onMount } from "svelte";

  let {
    title,
    seconds,
    onkeep,
    onrevert,
  }: { title: string; seconds: number; onkeep: () => void; onrevert: () => void } = $props();

  let left = $state(0);
  let timer: ReturnType<typeof setInterval> | undefined;
  let done = false;

  function finish(keep: boolean) {
    if (done) return;
    done = true;
    clearInterval(timer);
    (keep ? onkeep : onrevert)();
  }

  onMount(() => {
    left = seconds;
    timer = setInterval(() => {
      left -= 1;
      if (left <= 0) finish(false);
    }, 1000);
  });

  onDestroy(() => clearInterval(timer));
</script>

<div class="backdrop">
  <div class="dialog panel" role="alertdialog" aria-modal="true" aria-labelledby="confirm-title">
    <h2 id="confirm-title">{title}</h2>
    <p class="muted">
      {t(`Retour aux réglages précédents dans ${left} s.`, `Going back to the previous settings in ${left} s.`)}
    </p>
    <div class="actions">
      <button class="btn" onclick={() => finish(false)}>{t("Revenir en arrière", "Revert")}</button>
      <!-- svelte-ignore a11y_autofocus -->
      <button class="btn btn--primary" autofocus onclick={() => finish(true)}>{t("Garder", "Keep")}</button>
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
    width: min(28rem, calc(100vw - 2rem));
    padding: var(--space-5);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
    margin-top: var(--space-2);
  }
</style>
