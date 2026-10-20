<!-- Affichage : définition, fréquence, échelle, orientation, G-SYNC/FreeSync, HDR, profondeur de couleur. -->
<script lang="ts">
  import { api, errorMessage } from "$lib/api/ipc";
  import type { Mode, Output, OutputChange, Rotation, Vrr } from "$lib/api/types";
  import SectionHeader from "$lib/components/layout/SectionHeader.svelte";
  import Callout from "$lib/components/ui/Callout.svelte";
  import ConfirmDialog from "$lib/components/ui/ConfirmDialog.svelte";
  import OutputTabs from "$lib/components/ui/OutputTabs.svelte";
  import Spinner from "$lib/components/ui/Spinner.svelte";
  import Switch from "$lib/components/ui/Switch.svelte";
  import { num, t } from "$lib/i18n.svelte";
  import { app } from "$lib/state/app.svelte";

  let outputs = $state<Output[]>([]);
  let selected = $state("");
  let draft = $state<Output | null>(null);
  let loading = $state(true);
  let applying = $state(false);
  let error = $state<string | null>(null);
  /** Écrans d'avant le changement en attente de confirmation. */
  let pending = $state<Output[] | null>(null);

  const features = $derived(app.system?.features ?? null);
  const current = $derived(outputs.find((o) => o.name === selected) ?? null);

  const sizeKey = (m: Mode) => `${m.width}x${m.height}`;
  const resolutions = $derived(
    draft ? [...new Map(draft.modes.map((m) => [sizeKey(m), m])).values()] : [],
  );
  const rates = $derived(
    draft?.currentMode ? draft.modes.filter((m) => sizeKey(m) === sizeKey(draft!.currentMode!)) : [],
  );
  const scales = $derived([...new Set([1, 1.25, 1.5, 1.75, 2, draft?.scale ?? 1])].sort((a, b) => a - b));

  async function load() {
    loading = true;
    try {
      outputs = await api.outputs();
      if (!outputs.some((o) => o.name === selected)) selected = outputs.find((o) => o.enabled)?.name ?? outputs[0]?.name ?? "";
    } catch (e) {
      error = errorMessage(e);
    }
    loading = false;
  }

  $effect(() => {
    load();
  });

  $effect(() => {
    draft = current ? structuredClone($state.snapshot(current)) : null;
  });

  function setResolution(key: string) {
    if (!draft) return;
    const sameSize = draft.modes.filter((m) => sizeKey(m) === key);
    // Même fréquence si elle existe dans cette définition, sinon la plus haute.
    const rate = draft.currentMode?.refreshMhz ?? 0;
    draft.currentMode = sameSize.reduce((best, m) => (Math.abs(m.refreshMhz - rate) < Math.abs(best.refreshMhz - rate) ? m : best), sameSize[0]);
  }

  function setRate(refresh: number) {
    if (!draft?.currentMode) return;
    draft.currentMode = draft.modes.find((m) => sizeKey(m) === sizeKey(draft!.currentMode!) && m.refreshMhz === refresh) ?? draft.currentMode;
  }

  /** Les champs changés par rapport à l'écran actuel. */
  function change(): OutputChange | null {
    if (!draft || !current) return null;
    const c: OutputChange = { name: draft.name };
    if (draft.enabled !== current.enabled) c.enabled = draft.enabled;
    if (JSON.stringify(draft.currentMode) !== JSON.stringify(current.currentMode) && draft.currentMode) c.mode = draft.currentMode;
    if (draft.scale !== current.scale) c.scale = draft.scale;
    if (draft.rotation !== current.rotation) c.rotation = draft.rotation;
    if (draft.vrr !== current.vrr && draft.vrr) c.vrr = draft.vrr;
    if (draft.hdr !== current.hdr && draft.hdr !== null) c.hdr = draft.hdr;
    if (draft.bitDepth !== current.bitDepth && draft.bitDepth !== null) c.bitDepth = draft.bitDepth;
    if (draft.primary && !current.primary) c.primary = true;
    return Object.keys(c).length > 1 ? c : null;
  }

  const changed = $derived(draft ? change() !== null : false);

  /** L'état complet d'un écran, pour revenir en arrière. */
  function restore(o: Output): OutputChange {
    return {
      name: o.name,
      enabled: o.enabled,
      ...(o.currentMode ? { mode: o.currentMode } : {}),
      position: o.position,
      scale: o.scale,
      rotation: o.rotation,
      ...(o.vrr ? { vrr: o.vrr } : {}),
      ...(o.hdr !== null ? { hdr: o.hdr } : {}),
      ...(o.bitDepth !== null ? { bitDepth: o.bitDepth } : {}),
      ...(o.primary ? { primary: true } : {}),
    };
  }

  async function apply() {
    const c = change();
    if (!c) return;
    applying = true;
    error = null;
    const before = $state.snapshot(outputs);
    try {
      outputs = await api.applyOutputs([c]);
      pending = before;
    } catch (e) {
      error = errorMessage(e);
      await load();
    }
    applying = false;
  }

  async function revert() {
    const before = pending;
    pending = null;
    if (!before) return;
    try {
      outputs = await api.applyOutputs(before.map(restore));
    } catch (e) {
      error = errorMessage(e);
    }
  }

  const rotationLabel = (r: Rotation) =>
    ({
      normal: t("Paysage", "Landscape"),
      left: t("Portrait", "Portrait"),
      inverted: t("Paysage retourné", "Landscape (flipped)"),
      right: t("Portrait retourné", "Portrait (flipped)"),
    })[r];

  // Les deux choix du panneau NVIDIA : « plein écran » et « fenêtré et plein
  // écran ». Sous Wayland, il n'y a plus de plein écran exclusif : un jeu en
  // plein écran fenêtré (sans bordure) est un plein écran pour KWin et Hyprland.
  const vrrLabel = (v: Vrr) =>
    ({
      never: t("Désactivé", "Off"),
      fullscreen: t("Plein écran et sans bordure", "Fullscreen and borderless"),
      always: t("Fenêtré et plein écran", "Windowed and fullscreen"),
    })[v];
</script>

<SectionHeader
  title={t("Écran", "Screen")}
  lead={t(
    "Définition, fréquence, G-SYNC ou FreeSync, HDR et profondeur de couleur, écran par écran. Un changement se confirme : sans réponse, l'écran revient à ses réglages d'avant.",
    "Resolution, refresh rate, G-SYNC or FreeSync, HDR and color depth, screen by screen. Each change must be confirmed: without an answer, the screen goes back to its previous settings.",
  )}
>
  {#snippet actions()}
    {#if outputs.length > 0}
      <button class="btn btn--primary" onclick={apply} disabled={!changed || applying || !!features?.readOnly}>
        {#if applying}<Spinner />{/if}{t("Appliquer", "Apply")}
      </button>
    {/if}
  {/snippet}
</SectionHeader>

{#if features?.readOnly}
  <Callout kind="warning" title={t("Réglages en lecture seule", "Read-only settings")}>{features.readOnly}</Callout>
{/if}
{#if error}<Callout kind="danger" title={t("Réglage refusé", "Setting refused")}>{error}</Callout>{/if}

{#if loading}
  <p class="muted"><Spinner /> {t("Lecture des écrans…", "Reading screens…")}</p>
{:else if outputs.length === 0}
  <Callout kind="warning" title={t("Aucun écran", "No screen")}>{t("Le bureau n'a donné aucun écran.", "The desktop reported no screen.")}</Callout>
{:else}
  <OutputTabs {outputs} bind:selected />

  {#if draft}
    <section class="panel card">
      <Switch
        label={t("Écran allumé", "Screen on")}
        bind:checked={draft.enabled}
        hint={t("Un écran éteint ici reste branché, mais le bureau ne l'utilise plus.", "A screen turned off here stays plugged in, but the desktop stops using it.")}
      />

      <div class="grid" class:grid--off={!draft.enabled}>
        <label class="field">
          <span class="field__label">{t("Définition", "Resolution")}</span>
          <select value={draft.currentMode ? sizeKey(draft.currentMode) : ""} onchange={(e) => setResolution(e.currentTarget.value)} disabled={!draft.enabled}>
            {#each resolutions as mode (sizeKey(mode))}
              <option value={sizeKey(mode)}>
                {mode.width} × {mode.height}{draft.preferredMode && sizeKey(draft.preferredMode) === sizeKey(mode) ? ` (${t("native", "native")})` : ""}
              </option>
            {/each}
          </select>
        </label>

        <label class="field">
          <span class="field__label">{t("Fréquence", "Refresh rate")}</span>
          <select value={draft.currentMode?.refreshMhz ?? 0} onchange={(e) => setRate(Number(e.currentTarget.value))} disabled={!draft.enabled}>
            {#each rates as mode (mode.refreshMhz)}
              <option value={mode.refreshMhz}>{num(mode.refreshMhz / 1000, 2)} Hz</option>
            {/each}
          </select>
        </label>

        <label class="field">
          <span class="field__label">{t("Échelle", "Scale")}</span>
          <select bind:value={draft.scale} disabled={!draft.enabled}>
            {#each scales as scale (scale)}<option value={scale}>{num(scale * 100)} %</option>{/each}
          </select>
        </label>

        <label class="field">
          <span class="field__label">{t("Orientation", "Orientation")}</span>
          <select bind:value={draft.rotation} disabled={!draft.enabled}>
            {#each ["normal", "left", "inverted", "right"] as Rotation[] as rotation (rotation)}<option value={rotation}>{rotationLabel(rotation)}</option>{/each}
          </select>
        </label>

        {#if draft.vrr}
          <label class="field">
            <span class="field__label">G-SYNC / FreeSync</span>
            <select bind:value={draft.vrr} disabled={!draft.enabled}>
              {#each ["never", "fullscreen", "always"] as Vrr[] as vrr (vrr)}<option value={vrr}>{vrrLabel(vrr)}</option>{/each}
            </select>
            <span class="field__hint">
              {draft.vrr === "always"
                ? t("Aussi pour les jeux en fenêtre. Sur certains écrans, le bureau peut scintiller.", "Also for games in a window. On some screens, the desktop may flicker.")
                : t("Jeux en plein écran ou en plein écran fenêtré (sans bordure) : plus de déchirure d'image sans la latence de la V-Sync.", "Games in fullscreen or borderless windowed: no tearing without V-Sync's latency.")}
            </span>
          </label>
        {/if}

        {#if draft.bitDepth !== null && features}
          <label class="field">
            <span class="field__label">{t("Profondeur de couleur", "Color depth")}</span>
            <select bind:value={draft.bitDepth} disabled={!draft.enabled}>
              {#each features.bitDepths as bits (bits)}<option value={bits}>{bits === 0 ? t("Automatique", "Automatic") : `${bits} bits`}</option>{/each}
            </select>
          </label>
        {/if}
      </div>

      {#if draft.hdr !== null}
        <Switch
          label="HDR"
          checked={draft.hdr ?? false}
          onchange={(on) => draft && (draft.hdr = on)}
          disabled={!draft.enabled}
          hint={t("Les couleurs de l'écran (gamma, contraste) ne s'appliquent pas en HDR.", "Screen colors (gamma, contrast) don't apply in HDR.")}
        />
      {/if}
      {#if features?.primary}
        <Switch
          label={t("Écran principal", "Main display")}
          checked={draft.primary}
          onchange={(on) => draft && (draft.primary = on)}
          disabled={!draft.enabled || current?.primary}
          hint={t("Celui où s'ouvrent les jeux et la barre des tâches.", "Where games and the taskbar open.")}
        />
      {/if}
    </section>
  {/if}
{/if}

{#if pending}
  <ConfirmDialog title={t("Garder ces réglages d'affichage ?", "Keep these display settings?")} seconds={15} onkeep={() => (pending = null)} onrevert={revert} />
{/if}

<style>
  .card {
    display: grid;
    gap: var(--space-5);
    padding: var(--space-5);
  }

  .grid {
    display: grid;
    align-items: start;
    grid-template-columns: repeat(auto-fill, minmax(14rem, 1fr));
    gap: var(--space-4) var(--space-5);
  }

  .grid--off {
    opacity: 0.5;
  }
</style>
