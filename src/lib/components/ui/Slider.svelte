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
  /** Part remplie (0 à 1), sous le centre du bouton. */
  const fill = $derived(Math.min(1, Math.max(0, (value - min) / (max - min))));
  const mark = $derived(defaultValue === undefined ? null : `calc(0.5rem + (100% - 1rem) * ${(defaultValue - min) / (max - min)})`);
</script>

<div class="field">
  <div class="field__head">
    <label class="field__label" for={id}>{label}</label>
    <span class="field__value">{format(value)}</span>
  </div>
  <div class="track" class:track--disabled={disabled}>
    <span class="track__rail" aria-hidden="true"><span class="track__fill" style:--fill={fill}></span></span>
    {#if mark}<span class="track__default" style:left={mark} aria-hidden="true"></span>{/if}
    <input
      {id}
      type="range"
      {min}
      {max}
      {step}
      {disabled}
      bind:value
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

  /* Piste : rail gris, partie remplie en blanc jusqu'au centre du bouton (1 rem). */
  .track__rail {
    position: absolute;
    top: 50%;
    right: 0;
    left: 0;
    height: 4px;
    background: var(--color-border-strong);
    transform: translateY(-50%);
    pointer-events: none;
  }

  .track__fill {
    display: block;
    width: calc(0.5rem + (100% - 1rem) * var(--fill));
    height: 100%;
    background: var(--color-heading);
  }

  .track--disabled .track__rail {
    opacity: 0.45;
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
    background: var(--color-text-faint);
    transform: translateY(-50%);
    pointer-events: none;
  }
</style>
