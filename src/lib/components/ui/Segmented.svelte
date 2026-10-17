<!-- Choix exclusif en segments. -->
<script lang="ts" generics="T extends string | number">
  let {
    options,
    value = $bindable(),
    label,
    disabled = false,
    onchange,
  }: {
    options: { value: T; label: string }[];
    value: T;
    label: string;
    disabled?: boolean;
    onchange?: (value: T) => void;
  } = $props();
</script>

<div class="segmented" role="group" aria-label={label}>
  {#each options as option (option.value)}
    <button
      aria-pressed={value === option.value}
      {disabled}
      onclick={() => {
        value = option.value;
        onchange?.(option.value);
      }}>{option.label}</button
    >
  {/each}
</div>
