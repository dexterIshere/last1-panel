<!-- Couleurs : luminosité, contraste et gamma par écran (profil ICC), vibrance numérique (NVIDIA). -->
<script lang="ts">
  import { api, errorMessage } from "$lib/api/ipc";
  import type { Channel, ColorAdjust, ColorState, Output } from "$lib/api/types";
  import { neutralAdjust, sameChannels } from "$lib/color";
  import CurveGraph from "$lib/components/colors/CurveGraph.svelte";
  import Preview from "$lib/components/colors/Preview.svelte";
  import SectionHeader from "$lib/components/layout/SectionHeader.svelte";
  import Callout from "$lib/components/ui/Callout.svelte";
  import OutputTabs from "$lib/components/ui/OutputTabs.svelte";
  import Segmented from "$lib/components/ui/Segmented.svelte";
  import Slider from "$lib/components/ui/Slider.svelte";
  import Spinner from "$lib/components/ui/Spinner.svelte";
  import { num, t } from "$lib/i18n.svelte";
  import { app } from "$lib/state/app.svelte";
  import { latest } from "$lib/util/latest";
  import { RotateCcw } from "@lucide/svelte";

  type ChannelKey = "all" | "red" | "green" | "blue";

  let outputs = $state<Output[]>([]);
  let selected = $state("");
  let info = $state<ColorState | null>(null);
  let adjust = $state<ColorAdjust>(neutralAdjust());
  let vibrance = $state(50);
  let channel = $state<ChannelKey>("all");
  let loading = $state(true);
  let error = $state<string | null>(null);
  /** Hyprland : réglages pas encore appliqués à l'écran. */
  let dirty = $state(false);
  let applying = $state(false);

  const live = $derived(app.system?.features?.liveColor ?? false);
  const editing = $derived<Channel>(channel === "all" ? adjust.red : adjust[channel]);

  const colorSender = latest<{ output: string; adjust: ColorAdjust }>(
    (value) => api.setColor(value.output, value.adjust),
    150,
    (e) => (error = errorMessage(e)),
  );
  const vibranceSender = latest<{ output: string; percent: number }>(
    (value) => api.setVibrance(value.output, value.percent),
    250,
    (e) => (error = errorMessage(e)),
  );

  async function load() {
    loading = true;
    error = null;
    try {
      outputs = (await api.outputs()).filter((o) => o.enabled);
      if (!outputs.some((o) => o.name === selected)) selected = outputs[0]?.name ?? "";
    } catch (e) {
      error = errorMessage(e);
    }
    loading = false;
  }

  async function loadOutput(name: string) {
    if (!name) return;
    try {
      const fresh = await api.colorState(name);
      info = fresh;
      adjust = structuredClone(fresh.adjust);
      vibrance = fresh.vibrance;
      channel = sameChannels(adjust) ? "all" : "red";
      dirty = false;
    } catch (e) {
      error = errorMessage(e);
    }
  }

  $effect(() => {
    load();
  });

  $effect(() => {
    loadOutput(selected);
  });

  function update(key: keyof Channel, value: number) {
    const targets: ("red" | "green" | "blue")[] = channel === "all" ? ["red", "green", "blue"] : [channel];
    for (const target of targets) adjust[target][key] = value;
    changed();
  }

  function changed() {
    error = null;
    if (live) colorSender.push({ output: selected, adjust: $state.snapshot(adjust) });
    else dirty = true;
  }

  async function apply() {
    applying = true;
    error = null;
    try {
      await api.setColor(selected, $state.snapshot(adjust));
      dirty = false;
      info = await api.colorState(selected);
    } catch (e) {
      error = errorMessage(e);
    }
    applying = false;
  }

  async function reset() {
    adjust = neutralAdjust();
    channel = "all";
    if (live) {
      colorSender.push({ output: selected, adjust: $state.snapshot(adjust) });
      await colorSender.flush();
    } else {
      await apply();
    }
  }

  function setVibrance(value: number) {
    error = null;
    vibranceSender.push({ output: selected, percent: value });
  }

  const percent = (v: number) => `${num(v)} %`;
</script>

<SectionHeader
  title={t("Couleurs de l'écran", "Screen colors")}
  lead={t(
    "Les réglages de « Régler les paramètres de couleur du bureau » du panneau NVIDIA, avec les mêmes valeurs : luminosité et contraste à 50 %, gamma à 1,00.",
    "The “Adjust desktop color settings” page of the NVIDIA Control Panel, with the same values: brightness and contrast at 50%, gamma at 1.00.",
  )}
>
  {#snippet actions()}
    <button class="btn" onclick={reset} disabled={!info || !!info.colorUnavailable || applying}>
      <RotateCcw size={16} />{t("Valeurs par défaut", "Defaults")}
    </button>
    {#if !live}
      <button class="btn btn--primary" onclick={apply} disabled={!dirty || applying || !!info?.colorUnavailable}>
        {#if applying}<Spinner />{/if}{t("Appliquer", "Apply")}
      </button>
    {/if}
  {/snippet}
</SectionHeader>

{#if loading}
  <p class="muted"><Spinner /> {t("Lecture des écrans…", "Reading screens…")}</p>
{:else if outputs.length === 0}
  <Callout kind="warning" title={t("Aucun écran à régler", "No screen to set")}>{error ?? ""}</Callout>
{:else}
  <OutputTabs {outputs} bind:selected />

  {#if error}<Callout kind="danger" title={t("Réglage non appliqué", "Setting not applied")}>{error}</Callout>{/if}

  <div class="layout">
    <section class="panel card">
      <div class="card__head">
        <h2>{t("Luminosité, contraste, gamma", "Brightness, contrast, gamma")}</h2>
        <Segmented
          label={t("Canal", "Channel")}
          bind:value={channel}
          disabled={!!info?.colorUnavailable}
          options={[
            { value: "all", label: t("Tous", "All") },
            { value: "red", label: t("Rouge", "Red") },
            { value: "green", label: t("Vert", "Green") },
            { value: "blue", label: t("Bleu", "Blue") },
          ]}
        />
      </div>
      {#if info?.colorUnavailable}
        <Callout kind="warning">{info.colorUnavailable}</Callout>
      {/if}
      <Slider label={t("Luminosité", "Brightness")} value={editing.brightness} min={0} max={100} defaultValue={50} format={percent} disabled={!!info?.colorUnavailable} oninput={(v) => update("brightness", v)} />
      <Slider label={t("Contraste", "Contrast")} value={editing.contrast} min={0} max={100} defaultValue={50} format={percent} disabled={!!info?.colorUnavailable} oninput={(v) => update("contrast", v)} />
      <Slider label="Gamma" value={editing.gamma} min={0.3} max={2.8} step={0.01} defaultValue={1} format={(v) => num(v, 2)} disabled={!!info?.colorUnavailable} oninput={(v) => update("gamma", v)} />
      {#if !live}
        <p class="field__hint">
          {t(
            "Sous Hyprland, la courbe est chargée dans la carte graphique au clic sur « Appliquer » : l'écran peut clignoter un instant.",
            "On Hyprland, the curve is loaded into the graphics card when you click “Apply”: the screen may flicker briefly.",
          )}
        </p>
      {/if}
      {#if info?.replacedProfile}
        <Callout kind="info" title={t("Votre profil de couleur est mis de côté", "Your color profile is set aside")}>
          {t(
            `${info.replacedProfile} est remplacé par ces réglages ; il revient avec « Valeurs par défaut ».`,
            `${info.replacedProfile} is replaced by these settings; it comes back with “Defaults”.`,
          )}
        </Callout>
      {/if}
    </section>

    <section class="panel card">
      <h2>{t("Vibrance numérique", "Digital vibrance")}</h2>
      {#if info?.vibranceUnavailable}
        <Callout kind="info">{info.vibranceUnavailable}</Callout>
      {/if}
      <Slider
        label={t("Vibrance", "Vibrance")}
        bind:value={vibrance}
        min={0}
        max={100}
        defaultValue={50}
        format={percent}
        disabled={!!info?.vibranceUnavailable}
        hint={t("Couleurs plus vives sur tout l'écran, jeux compris, sans coût en performances.", "More vivid colors on the whole screen, games included, at no performance cost.")}
        oninput={setVibrance}
      />
    </section>

    <aside class="panel card card--side">
      <CurveGraph {adjust} />
      <Preview {adjust} {vibrance} />
    </aside>
  </div>
{/if}

<style>
  .layout {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 18rem;
    grid-template-rows: auto 1fr;
    gap: var(--space-4);
  }

  .card {
    display: grid;
    align-content: start;
    gap: var(--space-5);
    padding: var(--space-5);
  }

  .card--side {
    grid-column: 2;
    grid-row: 1 / span 2;
  }

  .card__head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
  }

  h2 {
    font-size: var(--text-lg);
  }
</style>
