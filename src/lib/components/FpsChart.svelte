<!-- Courbe d'une partie : images par seconde et 1 % les plus lentes au fil
     des minutes, avec la charge du processeur et de la carte au survol. -->
<script lang="ts">
  import type { Point } from "$lib/api/types";
  import { num, t } from "$lib/i18n.svelte";

  let { points }: { points: Point[] } = $props();

  const W = 640;
  const H = 220;
  const pad = { top: 12, right: 12, bottom: 26, left: 40 };
  const plotW = W - pad.left - pad.right;
  const plotH = H - pad.top - pad.bottom;

  /** Haut de l'échelle : un multiple de 30 au-dessus du plus haut. */
  const top = $derived(Math.max(30, Math.ceil(Math.max(...points.map((p) => p.fps), 1) / 30) * 30));
  const end = $derived(Math.max(points[points.length - 1]?.minute ?? 1, 0.1));
  const x = (minute: number) => pad.left + (minute / end) * plotW;
  const y = (fps: number) => pad.top + plotH - (Math.min(fps, top) / top) * plotH;
  const path = (key: "fps" | "fpsLow") =>
    points.map((p, i) => `${i === 0 ? "M" : "L"}${x(p.minute).toFixed(1)},${y(p[key]).toFixed(1)}`).join(" ");
  const yTicks = $derived([0, 0.25, 0.5, 0.75, 1].map((f) => Math.round(top * f)));
  const xTicks = $derived.by(() => {
    const step = [1, 2, 5, 10, 15, 30, 60].find((s) => end / s <= 8) ?? 120;
    return Array.from({ length: Math.floor(end / step) + 1 }, (_, i) => i * step);
  });
  const markers = $derived(points.length <= 24);

  let hover = $state<number | null>(null);
  const hovered = $derived(hover === null ? null : points[hover]);

  function move(event: PointerEvent) {
    const svg = event.currentTarget as SVGSVGElement;
    const box = svg.getBoundingClientRect();
    const minute = (((event.clientX - box.left) / box.width) * W - pad.left) / plotW * end;
    let best = 0;
    points.forEach((p, i) => {
      if (Math.abs(p.minute - minute) < Math.abs(points[best].minute - minute)) best = i;
    });
    hover = best;
  }
</script>

<figure class="chart">
  <div class="legend">
    <span class="legend__item"><span class="legend__swatch legend__swatch--fps"></span>{t("Images/s", "FPS")}</span>
    <span class="legend__item"><span class="legend__swatch legend__swatch--low"></span>{t("1 % les plus lentes", "1 % lows")}</span>
  </div>
  <div class="plot">
    <svg
      viewBox="0 0 {W} {H}"
      role="img"
      aria-label={t("Images par seconde au fil de la partie", "Frames per second over the game")}
      onpointermove={move}
      onpointerleave={() => (hover = null)}
    >
      {#each yTicks as tick (tick)}
        <line class="grid" x1={pad.left} x2={W - pad.right} y1={y(tick)} y2={y(tick)} />
        <text class="axis" x={pad.left - 6} y={y(tick)} text-anchor="end" dominant-baseline="middle">{tick}</text>
      {/each}
      {#each xTicks as tick (tick)}
        <text class="axis" x={x(tick)} y={H - 8} text-anchor="middle">{tick} min</text>
      {/each}
      <path class="line line--low" d={path("fpsLow")} />
      <path class="line line--fps" d={path("fps")} />
      {#if markers}
        {#each points as p (p.minute)}
          <circle class="dot dot--low" cx={x(p.minute)} cy={y(p.fpsLow)} r="4" />
          <circle class="dot dot--fps" cx={x(p.minute)} cy={y(p.fps)} r="4" />
        {/each}
      {/if}
      {#if hovered}
        <line class="cross" x1={x(hovered.minute)} x2={x(hovered.minute)} y1={pad.top} y2={pad.top + plotH} />
        <circle class="dot dot--low" cx={x(hovered.minute)} cy={y(hovered.fpsLow)} r="5" />
        <circle class="dot dot--fps" cx={x(hovered.minute)} cy={y(hovered.fps)} r="5" />
      {/if}
    </svg>
    {#if hovered}
      <div class="tip" style:left="{(x(hovered.minute) / W) * 100}%" class:tip--left={x(hovered.minute) > W * 0.6}>
        <strong>{t(`À ${num(hovered.minute, 1)} min`, `At ${num(hovered.minute, 1)} min`)}</strong>
        <span><span class="legend__swatch legend__swatch--fps"></span>{t("Images/s", "FPS")} <b class="mono">{num(hovered.fps)}</b></span>
        <span><span class="legend__swatch legend__swatch--low"></span>1 % <b class="mono">{num(hovered.fpsLow)}</b></span>
        <span class="tip__muted">{t("Processeur", "CPU")} {num(hovered.cpuLoad)} % · {t("Carte", "GPU")} {num(hovered.gpuLoad)} %</span>
      </div>
    {/if}
  </div>
</figure>

<style>
  .chart {
    --series-fps: #2aa89b;
    --series-low: #7f6df0;
    display: grid;
    gap: var(--space-2);
    margin: var(--space-3) 0 0;
  }

  .legend {
    display: flex;
    gap: var(--space-4);
    color: var(--color-text-muted);
    font-size: var(--text-xs);
  }

  .legend__item,
  .tip span {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .legend__swatch {
    display: inline-block;
    width: 10px;
    height: 10px;
    border-radius: 50%;
  }

  .legend__swatch--fps {
    background: var(--series-fps);
  }

  .legend__swatch--low {
    background: var(--series-low);
  }

  .plot {
    position: relative;
  }

  svg {
    display: block;
    width: 100%;
    height: auto;
    touch-action: none;
  }

  .grid {
    stroke: var(--color-border);
    stroke-width: 1;
  }

  .axis {
    fill: var(--color-text-faint);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }

  .line {
    fill: none;
    stroke-width: 2;
    stroke-linejoin: round;
    stroke-linecap: round;
  }

  .line--fps {
    stroke: var(--series-fps);
  }

  .line--low {
    stroke: var(--series-low);
  }

  .dot {
    stroke: var(--color-surface);
    stroke-width: 2;
  }

  .dot--fps {
    fill: var(--series-fps);
  }

  .dot--low {
    fill: var(--series-low);
  }

  .cross {
    stroke: var(--color-border-strong);
    stroke-width: 1;
  }

  .tip {
    position: absolute;
    top: 0;
    display: grid;
    gap: 2px;
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-surface-raised);
    color: var(--color-text);
    font-size: var(--text-xs);
    pointer-events: none;
    transform: translateX(12px);
    white-space: nowrap;
  }

  .tip--left {
    transform: translateX(calc(-100% - 12px));
  }

  .tip__muted {
    color: var(--color-text-muted);
  }
</style>
