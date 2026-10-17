<!-- Courbe des ventilateurs : points température → vitesse, modifiables. -->
<script lang="ts">
  import { t } from "$lib/i18n.svelte";
  import { Plus, Trash2 } from "@lucide/svelte";

  let { points = $bindable(), current }: { points: [number, number][]; current?: number | null } = $props();

  const W = 320;
  const H = 160;
  const x = (temp: number) => ((temp - 20) / 80) * W;
  const y = (speed: number) => H - (speed / 100) * H;
  const sorted = $derived([...points].sort((a, b) => a[0] - b[0]));
  const line = $derived(sorted.map(([temp, speed], i) => `${i ? "L" : "M"}${x(temp).toFixed(1)},${y(speed).toFixed(1)}`).join(""));

  function add() {
    const last = sorted.at(-1) ?? [60, 50];
    points = [...points, [Math.min(100, last[0] + 5), Math.min(100, last[1] + 10)]];
  }
</script>

<div class="curve">
  <svg viewBox="-6 -6 {W + 12} {H + 12}" role="img" aria-label={t("Courbe des ventilateurs", "Fan curve")}>
    <rect class="frame" x="0" y="0" width={W} height={H} />
    {#each [40, 60, 80] as temp (temp)}<path class="grid" d="M{x(temp)},0V{H}" />{/each}
    {#each [25, 50, 75] as speed (speed)}<path class="grid" d="M0,{y(speed)}H{W}" />{/each}
    {#if current != null}<path class="now" d="M{x(current)},0V{H}" />{/if}
    <path class="line" d={line} />
    {#each sorted as [temp, speed], index (index)}<circle cx={x(temp)} cy={y(speed)} r="4" />{/each}
  </svg>
  <div class="points">
    {#each points as point, index (index)}
      <div class="point">
        <label>
          <span class="label">°C</span>
          <input type="number" min="20" max="100" bind:value={point[0]} />
        </label>
        <label>
          <span class="label">%</span>
          <input type="number" min="0" max="100" bind:value={point[1]} />
        </label>
        <button class="btn btn--ghost" aria-label={t("Retirer ce point", "Remove this point")} disabled={points.length <= 2} onclick={() => (points = points.filter((_, i) => i !== index))}>
          <Trash2 size={14} />
        </button>
      </div>
    {/each}
    <button class="btn" onclick={add} disabled={points.length >= 10}><Plus size={14} />{t("Ajouter un point", "Add a point")}</button>
  </div>
</div>

<style>
  .curve {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: var(--space-5);
    align-items: start;
  }

  svg {
    width: 100%;
  }

  .frame {
    fill: var(--color-bg);
    stroke: var(--color-border-strong);
  }

  .grid {
    stroke: var(--color-border);
  }

  .now {
    stroke: var(--color-warning);
    stroke-dasharray: 3 3;
  }

  .line {
    fill: none;
    stroke: var(--color-pulse);
    stroke-width: 2;
  }

  circle {
    fill: var(--color-bg);
    stroke: var(--color-pulse);
    stroke-width: 2;
  }

  .points {
    display: grid;
    gap: var(--space-2);
  }

  .point {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .point label {
    display: flex;
    align-items: center;
    gap: var(--space-1);
  }

  .point input {
    width: 4.5rem;
    min-height: 2rem;
  }
</style>
