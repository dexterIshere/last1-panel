<!-- Repère de Last1 Panel : trois curseurs sur une tuile sombre. -->
<script lang="ts">
  let { size = 32 }: { size?: number } = $props();
</script>

<svg class="mark" width={size} height={size} viewBox="0 0 32 32" aria-hidden="true">
  <rect class="mark__tile" x="0.5" y="0.5" width="31" height="31" rx="8" />
  <path class="mark__track" d="M9.5 8v16M16 8v16M22.5 8v16" />
  <path class="mark__knob" d="M7 19h5M13.5 11h5M20 16h5" />
</svg>

<style>
  .mark {
    flex: none;
  }

  .mark__tile {
    fill: var(--color-panel-soft);
    stroke: var(--color-panel-line);
  }

  .mark__track,
  .mark__knob {
    fill: none;
    stroke-linecap: round;
    stroke-width: 2.2;
  }

  .mark__track {
    stroke: var(--color-panel-line);
  }

  .mark__knob {
    stroke: var(--color-panel);
  }
</style>
