<!-- Icône de Last1 Panel, la même que dans la barre des tâches
     (src-tauri/icons/source/icon.svg) : trois curseurs en pixels blancs sur
     noir, une marche de bande à 45° du repère turquoise dans le coin. -->
<script lang="ts">
  let { size = 32 }: { size?: number } = $props();
</script>

<svg class="mark" width={size} height={size} viewBox="0 0 1024 1024" aria-hidden="true">
  <rect class="mark__tile" width="1024" height="1024" />
  <path class="mark__sliders" d="M192 128h64v768h-64zM448 128h64v768h-64zM704 128h64v768h-64zM128 512h192v128h-192zM384 256h192v128h-192zM640 384h192v128h-192z" />
  <path class="mark__step" d="M960 704h64v64h-64zM896 768h128v64h-128zM832 832h192v64h-192zM768 896h256v64h-256zM704 960h320v64h-320z" />
</svg>

<style>
  .mark {
    flex: none;
    shape-rendering: crispEdges;
    outline: 1px solid var(--color-border);
  }

  .mark__tile {
    fill: var(--color-bg);
  }

  .mark__sliders {
    fill: var(--color-heading);
  }

  .mark__step {
    fill: var(--color-panel);
  }
</style>
