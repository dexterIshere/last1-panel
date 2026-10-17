<!-- Curseur avec son libellé, sa valeur et un repère de la valeur par défaut. -->
<script lang="ts">
  let {
    label,
    value = $bindable(),
    min,
    max,
    step = 1,
    defaultValue,
    format = (v: number) => String(v),
    disabled = false,
    hint,
    oninput,
    onchange,
  }: {
    label: string;
    value: number;
    min: number;
    max: number;
    step?: number;
    defaultValue?: number;
    format?: (value: number) => string;
    disabled?: boolean;
    hint?: string;
    oninput?: (value: number) => void;
    onchange?: (value: number) => void;
  } = $props();

  const id = `slider-${Math.random().toString(36).slice(2, 9)}`;
  const fill = $derived(`${((value - min) / (max - min)) * 100}%`);
  const mark = $derived(defaultValue === undefined ? null : `${((defaultValue - min) / (max - min)) * 100}%`);
</script>

<div class="field">
  <div class="field__head">
    <label class="field__label" for={id}>{label}</label>
    <span class="field__value">{format(value)}</span>
  </div>
  <div class="track">
    {#if mark}<span class="track__default" style:left={mark} aria-hidden="true"></span>{/if}
    <input
      {id}
      type="range"
      {min}
      {max}
      {step}
      {disabled}
      bind:value
      style:--fill={fill}
      oninput={() => oninput?.(value)}
      onchange={() => onchange?.(value)}
    />
  </div>
  {#if hint}<span class="field__hint">{hint}</span>{/if}
</div>

<style>
  .track {
    position: relative;
  }

  .track input {
    position: relative;
    z-index: 1;
  }

  .track__default {
    position: absolute;
    top: 50%;
    width: 2px;
    height: 0.75rem;
    margin-left: -1px;
    border-radius: 1px;
    background: var(--color-text-faint);
    transform: translateY(-50%);
    pointer-events: none;
  }
</style>
