<!-- Interrupteur avec libellé et explication. -->
<script lang="ts">
  let {
    label,
    checked = $bindable(),
    hint,
    disabled = false,
    onchange,
  }: { label: string; checked: boolean; hint?: string; disabled?: boolean; onchange?: (checked: boolean) => void } =
    $props();
</script>

<div class="row">
  <div class="row__text">
    <span class="field__label">{label}</span>
    {#if hint}<span class="field__hint">{hint}</span>{/if}
  </div>
  <button
    class="switch"
    role="switch"
    aria-checked={checked}
    aria-label={label}
    {disabled}
    onclick={() => {
      checked = !checked;
      onchange?.(checked);
    }}
  ></button>
</div>

<style>
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
  }

  .row__text {
    display: grid;
    gap: 2px;
  }
</style>
