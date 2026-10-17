<!-- Courbes rouge, verte et bleue (entrée → sortie), comme le graphe du panneau NVIDIA. -->
<script lang="ts">
  import type { ColorAdjust } from "$lib/api/types";
  import { applyChannel } from "$lib/color";
  import { t } from "$lib/i18n.svelte";

  let { adjust }: { adjust: ColorAdjust } = $props();

  const SIZE = 200;
  const STEPS = 64;

  function path(channel: ColorAdjust["red"]): string {
    const points: string[] = [];
    for (let i = 0; i <= STEPS; i++) {
      const x = i / STEPS;
      const y = applyChannel(channel, x);
      points.push(`${(x * SIZE).toFixed(1)},${((1 - y) * SIZE).toFixed(1)}`);
    }
    return `M${points.join("L")}`;
  }

  const curves = $derived([
    { key: "red", d: path(adjust.red) },
    { key: "green", d: path(adjust.green) },
    { key: "blue", d: path(adjust.blue) },
  ]);
</script>

<figure class="graph">
  <svg viewBox="-1 -1 {SIZE + 2} {SIZE + 2}" role="img" aria-label={t("Courbes de couleur", "Color curves")}>
    {#each [0.25, 0.5, 0.75] as line (line)}
      <path class="grid" d="M{line * SIZE},0V{SIZE}M0,{line * SIZE}H{SIZE}" />
    {/each}
    <rect class="frame" x="0" y="0" width={SIZE} height={SIZE} />
    <path class="identity" d="M0,{SIZE}L{SIZE},0" />
    {#each curves as curve (curve.key)}
      <path class="curve curve--{curve.key}" d={curve.d} />
    {/each}
  </svg>
  <figcaption class="label">{t("Entrée → sortie", "Input → output")}</figcaption>
</figure>

<style>
  .graph {
    display: grid;
    gap: var(--space-2);
    margin: 0;
  }

  svg {
    width: 100%;
    aspect-ratio: 1;
  }

  .frame {
    fill: var(--color-bg);
    stroke: var(--color-border-strong);
  }

  .grid {
    stroke: var(--color-border);
  }

  .identity {
    stroke: var(--color-text-faint);
    stroke-dasharray: 3 4;
  }

  .curve {
    fill: none;
    stroke-width: 2;
    mix-blend-mode: screen;
  }

  .curve--red {
    stroke: var(--color-red);
  }

  .curve--green {
    stroke: var(--color-green);
  }

  .curve--blue {
    stroke: var(--color-blue);
  }
</style>
