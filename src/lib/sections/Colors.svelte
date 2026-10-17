<!-- Couleurs : luminosité, contraste et gamma par écran (profil ICC), vibrance numérique (NVIDIA),
     profils de couleurs (pour l'écran, ou pour un jeu dans sa seule fenêtre). -->
<script lang="ts">
  import { api, errorMessage } from "$lib/api/ipc";
  import type { Channel, ColorAdjust, ColorProfile, ColorState, Output } from "$lib/api/types";
  import { CHANNELS, clampChannel, meanChannel, neutralAdjust, sameChannels } from "$lib/color";
  import CurveGraph from "$lib/components/colors/CurveGraph.svelte";
  import Preview from "$lib/components/colors/Preview.svelte";
  import SectionHeader from "$lib/components/layout/SectionHeader.svelte";
  import Callout from "$lib/components/ui/Callout.svelte";
  import OutputTabs from "$lib/components/ui/OutputTabs.svelte";
  import Segmented from "$lib/components/ui/Segmented.svelte";
  import Slider from "$lib/components/ui/Slider.svelte";
  import Spinner from "$lib/components/ui/Spinner.svelte";
  import Switch from "$lib/components/ui/Switch.svelte";
  import { num, t } from "$lib/i18n.svelte";
  import { app } from "$lib/state/app.svelte";
  import { latest } from "$lib/util/latest";
  import { RotateCcw, Save, Trash2 } from "@lucide/svelte";

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
  let profiles = $state<ColorProfile[]>([]);
  let tray = $state(true);
  let profileName = $state("");
  let profileBusy = $state(false);
  let notice = $state<string | null>(null);
  /** Profil à supprimer au second clic. */
  let confirmDelete = $state<string | null>(null);

  const live = $derived(app.system?.features?.liveColor ?? false);
  const channelsDiffer = $derived(!sameChannels(adjust));
  /** « Tous » : la moyenne des trois canaux (leur valeur commune s'ils sont pareils). */
  const editing = $derived<Channel>(channel === "all" ? meanChannel(adjust) : adjust[channel]);

  /** Glissement d'un curseur de « Tous » : les canaux de départ, décalés d'autant. */
  let drag: { key: keyof Channel; from: number; base: ColorAdjust } | null = null;

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
      channel = "all";
      drag = null;
      dirty = false;
    } catch (e) {
      error = errorMessage(e);
    }
  }

  async function loadProfiles() {
    try {
      const fresh = await api.colorProfiles();
      profiles = fresh.profiles;
      tray = fresh.tray;
    } catch (e) {
      error = errorMessage(e);
    }
  }

  $effect(() => {
    load();
    loadProfiles();
  });

  $effect(() => {
    loadOutput(selected);
  });

  /** « Tous » décale les trois canaux d'autant : un réglage propre au bleu
   * (ou au rouge, au vert) reste. Un canal seul se règle tel quel. */
  function update(key: keyof Channel, value: number) {
    if (channel === "all") {
      if (drag?.key !== key) drag = { key, from: editing[key], base: $state.snapshot(adjust) };
      const delta = value - drag.from;
      for (const target of CHANNELS) adjust[target][key] = clampChannel(key, drag.base[target][key] + delta);
    } else {
      drag = null;
      adjust[channel][key] = value;
    }
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
    drag = null;
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

  const near = (a: number, b: number, margin: number) => Math.abs(a - b) < margin;
  const sameColors = (a: ColorAdjust, b: ColorAdjust) =>
    CHANNELS.every(
      (c) => near(a[c].brightness, b[c].brightness, 0.05) && near(a[c].contrast, b[c].contrast, 0.05) && near(a[c].gamma, b[c].gamma, 0.0005),
    );
  /** Profil aux réglages affichés (vibrance comprise, si elle se règle ici). */
  const isCurrent = (p: ColorProfile) =>
    sameColors(p.color, adjust) && (!!info?.vibranceUnavailable || near(p.vibrance, vibrance, 0.5));

  function summary(p: ColorProfile): string {
    const c = p.color.red;
    const curves = sameChannels(p.color)
      ? `${t("Luminosité", "Brightness")} ${num(c.brightness)} %, ${t("contraste", "contrast")} ${num(c.contrast)} %, gamma ${num(c.gamma, 2)}`
      : t("Réglé canal par canal", "Set channel by channel");
    return `${curves} · ${t("vibrance", "vibrance")} ${num(p.vibrance)} %`;
  }

  /** Réglages affichés gardés : dans un nouveau profil, ou à la place de `existing`. */
  async function saveProfile(existing: ColorProfile | null) {
    profileBusy = true;
    error = null;
    notice = null;
    try {
      const saved = await api.saveColorProfile(existing?.id ?? null, existing?.name ?? profileName, $state.snapshot(adjust), vibrance);
      await loadProfiles();
      if (!existing) profileName = "";
      notice = existing
        ? t(`« ${saved.name} » a maintenant ces réglages.`, `“${saved.name}” now has these settings.`)
        : t(`Profil « ${saved.name} » enregistré : liez-le à un jeu dans Jeux.`, `Profile “${saved.name}” saved: link it to a game in Games.`);
    } catch (e) {
      error = errorMessage(e);
    }
    profileBusy = false;
  }

  async function applyProfile(p: ColorProfile) {
    profileBusy = true;
    error = null;
    notice = null;
    try {
      await colorSender.flush();
      await vibranceSender.flush();
      await api.applyColorProfile(selected, p.id);
      await loadOutput(selected);
    } catch (e) {
      error = errorMessage(e);
    }
    profileBusy = false;
  }

  async function deleteProfile(p: ColorProfile) {
    if (confirmDelete !== p.id) {
      confirmDelete = p.id;
      setTimeout(() => {
        if (confirmDelete === p.id) confirmDelete = null;
      }, 3000);
      return;
    }
    confirmDelete = null;
    profileBusy = true;
    error = null;
    try {
      await api.deleteColorProfile(p.id);
      await loadProfiles();
      notice = t(`« ${p.name} » supprimé : ses jeux reprennent les couleurs de l'écran.`, `“${p.name}” deleted: its games go back to the screen's colors.`);
    } catch (e) {
      error = errorMessage(e);
    }
    profileBusy = false;
  }

  async function setTray(shown: boolean) {
    error = null;
    try {
      await api.setTray(shown);
    } catch (e) {
      tray = !shown;
      error = errorMessage(e);
    }
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
    {#if outputs.length > 0}
    <button class="btn" onclick={reset} disabled={!info || !!info.colorUnavailable || applying}>
      <RotateCcw size={16} />{t("Valeurs par défaut", "Defaults")}
    </button>
    {#if !live}
      <button class="btn btn--primary" onclick={apply} disabled={!dirty || applying || !!info?.colorUnavailable}>
        {#if applying}<Spinner />{/if}{t("Appliquer", "Apply")}
      </button>
    {/if}
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
  {#if notice && !error}<Callout kind="success">{notice}</Callout>{/if}

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
      {#if channel === "all" && channelsDiffer}
        <p class="field__hint">
          {t(
            "Rouge, vert et bleu sont réglés différemment : « Tous » les déplace ensemble, leurs écarts restent.",
            "Red, green and blue are set differently: “All” moves them together and keeps their differences.",
          )}
        </p>
      {:else if channel !== "all"}
        <p class="field__hint">
          {t(
            "Ce canal seul : sa couleur se renforce ou s'éteint dans toute l'image (un contraste fort sur le bleu seul jaunit les ombres). Les réglages des guides du panneau NVIDIA se mettent sur « Tous ».",
            "This channel only: its color gets stronger or fades across the whole picture (high contrast on blue alone turns shadows yellow). Settings from NVIDIA Control Panel guides go on “All”.",
          )}
        </p>
      {/if}
      <Slider label={t("Luminosité", "Brightness")} value={editing.brightness} min={0} max={100} defaultValue={50} format={percent} disabled={!!info?.colorUnavailable} oninput={(v) => update("brightness", v)} onchange={() => (drag = null)} />
      <Slider label={t("Contraste", "Contrast")} value={editing.contrast} min={0} max={100} defaultValue={50} format={percent} disabled={!!info?.colorUnavailable} oninput={(v) => update("contrast", v)} onchange={() => (drag = null)} />
      <Slider label="Gamma" value={editing.gamma} min={0.3} max={2.8} step={0.01} defaultValue={1} format={(v) => num(v, 2)} disabled={!!info?.colorUnavailable} oninput={(v) => update("gamma", v)} onchange={() => (drag = null)} />
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

    <section class="panel card">
      <div>
        <h2>{t("Profils de couleurs", "Color profiles")}</h2>
        <p class="field__hint">
          {t(
            "Ces réglages gardés sous un nom : pour les remettre sur l'écran, ou pour un jeu (page Jeux), dans sa seule fenêtre.",
            "These settings kept under a name: to put them back on the screen, or for a game (Games page), in its window only.",
          )}
        </p>
      </div>
      <form
        class="save"
        onsubmit={(e) => {
          e.preventDefault();
          saveProfile(null);
        }}
      >
        <input type="text" maxlength="60" placeholder={t("Nom (ex. Arc Raiders)", "Name (e.g. Arc Raiders)")} aria-label={t("Nom du profil", "Profile name")} bind:value={profileName} />
        <button class="btn btn--primary" type="submit" disabled={profileBusy || !info}>
          <Save size={16} />{t("Enregistrer ces réglages", "Save these settings")}
        </button>
      </form>
      {#if profiles.length > 0}
        <ul class="profiles">
          {#each profiles as profile (profile.id)}
            {@const current = isCurrent(profile)}
            <li class="profile" class:profile--current={current}>
              <div class="profile__text">
                <span class="profile__name">{profile.name}</span>
                <span class="field__hint">{summary(profile)}</span>
              </div>
              {#if current}<span class="badge badge--success">{t("Sur l'écran", "On screen")}</span>{/if}
              <div class="profile__actions">
                <button class="btn" onclick={() => applyProfile(profile)} disabled={profileBusy || current || !!info?.colorUnavailable}>
                  {t("Appliquer", "Apply")}
                </button>
                <button
                  class="btn btn--ghost"
                  onclick={() => saveProfile(profile)}
                  disabled={profileBusy || current}
                  title={t("Remplacer par les réglages affichés", "Replace with the settings shown")}
                  aria-label={t(`Remplacer ${profile.name} par les réglages affichés`, `Replace ${profile.name} with the settings shown`)}
                >
                  <Save size={16} />
                </button>
                <button
                  class="btn btn--ghost"
                  class:btn--danger={confirmDelete === profile.id}
                  onclick={() => deleteProfile(profile)}
                  disabled={profileBusy}
                  title={t("Supprimer", "Delete")}
                  aria-label={t(`Supprimer ${profile.name}`, `Delete ${profile.name}`)}
                >
                  <Trash2 size={16} />{#if confirmDelete === profile.id}{t("Confirmer", "Confirm")}{/if}
                </button>
              </div>
            </li>
          {/each}
        </ul>
      {/if}
      <Switch
        label={t("Profils dans l'icône près de l'horloge", "Profiles in the icon near the clock")}
        bind:checked={tray}
        hint={t(
          "Clic droit sur l'icône Last1 Panel : un profil sur l'écran en un clic. Clic gauche : cette fenêtre.",
          "Right-click the Last1 Panel icon: a profile on the screen in one click. Left-click: this window.",
        )}
        onchange={setTray}
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
    grid-template-rows: auto auto 1fr;
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
    grid-row: 1 / span 3;
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

  .save {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .save input {
    flex: 1;
    min-width: 12rem;
  }

  .profiles {
    display: grid;
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .profile {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
  }

  .profile--current {
    border-color: var(--color-panel-line);
    background: var(--color-panel-soft);
  }

  .profile__text {
    display: grid;
    flex: 1;
    gap: 2px;
    min-width: 12rem;
  }

  .profile__name {
    font-weight: 600;
    overflow-wrap: anywhere;
  }

  .profile__actions {
    display: flex;
    gap: var(--space-1);
  }
</style>
