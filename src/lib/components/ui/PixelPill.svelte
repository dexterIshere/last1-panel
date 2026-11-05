<!-- Pastille pixel (docs/DA.md de go-to-linux) : capsule blanche, trois cœurs
     noirs sur une grille de 9 cases, une barre de vie pleine. Une seule, sur
     le titre du premier écran ; toujours décorative (aria-hidden). -->
<script lang="ts">
  const HEART = "M1 1h2v1h-2zM6 1h2v1h-2zM0 2h4v1h-4zM5 2h4v1h-4zM0 3h9v1h-9zM0 4h9v1h-9zM1 5h7v1h-7zM2 6h5v1h-5zM3 7h3v1h-3zM4 8h1v1h-1z";
</script>

<span class="pixel-pill" aria-hidden="true">
  {#each [0, 1, 2] as index (index)}
    <svg class="pixel" viewBox="0 0 9 9"><path d={HEART} /></svg>
  {/each}
</span>
