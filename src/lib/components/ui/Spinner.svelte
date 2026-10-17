<script lang="ts">
  let { size = 16 }: { size?: number } = $props();
</script>

<span class="spinner" style:width="{size}px" style:height="{size}px" aria-hidden="true"></span>

<style>
  .spinner {
    display: inline-block;
    flex: none;
    border: 2px solid currentColor;
    border-right-color: transparent;
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .spinner {
      animation-duration: 2.4s;
    }
  }
</style>
