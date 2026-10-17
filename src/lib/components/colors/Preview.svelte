<!-- Aperçu : dégradés de gris et de couleurs passés par les courbes. -->
<script lang="ts">
  import type { ColorAdjust } from "$lib/api/types";
  import { applyChannel } from "$lib/color";
  import { t } from "$lib/i18n.svelte";

  let { adjust, vibrance = 50 }: { adjust: ColorAdjust; vibrance?: number } = $props();

  const STEPS = 16;
  const byte = (value: number) => Math.round(Math.min(1, Math.max(0, value)) * 255);

  /** Couleur après vibrance (saturation autour de la luminance) puis courbes. */
  function color(r: number, g: number, b: number): string {
    const amount = vibrance / 50;
    const luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    const [vr, vg, vb] = [r, g, b].map((c) => luma + (c - luma) * amount);
    return `rgb(${byte(applyChannel(adjust.red, vr))} ${byte(applyChannel(adjust.green, vg))} ${byte(applyChannel(adjust.blue, vb))})`;
  }

  const rows = $derived([
    { key: "grey", cells: Array.from({ length: STEPS }, (_, i) => color(i / (STEPS - 1), i / (STEPS - 1), i / (STEPS - 1))) },
    { key: "red", cells: Array.from({ length: STEPS }, (_, i) => color(i / (STEPS - 1), 0.1, 0.1)) },
    { key: "green", cells: Array.from({ length: STEPS }, (_, i) => color(0.1, i / (STEPS - 1), 0.15)) },
    { key: "blue", cells: Array.from({ length: STEPS }, (_, i) => color(0.1, 0.15, i / (STEPS - 1))) },
    { key: "skin", cells: Array.from({ length: STEPS }, (_, i) => color(0.2 + (0.75 * i) / (STEPS - 1), 0.12 + (0.55 * i) / (STEPS - 1), 0.08 + (0.42 * i) / (STEPS - 1))) },
  ]);
</script>

<figure class="preview">
  <div class="rows">
    {#each rows as row (row.key)}
      <div class="row">
        {#each row.cells as cell, index (index)}<span style:background={cell}></span>{/each}
      </div>
    {/each}
  </div>
  <figcaption class="label">{t("Aperçu des réglages", "Settings preview")}</figcaption>
</figure>

<style>
  .preview {
    display: grid;
    gap: var(--space-2);
    margin: 0;
  }

  .rows {
    display: grid;
    gap: 3px;
    overflow: hidden;
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-sm);
  }

  .row {
    display: grid;
    grid-template-columns: repeat(16, 1fr);
    height: 1.5rem;
  }
</style>
