<!-- Souris : accélération et sensibilité pour toutes les souris (bureau) ;
     paliers de DPI et fréquence enregistrés dans les souris de jeu, comme G HUB. -->
<script lang="ts">
  import { api, errorMessage } from "$lib/api/ipc";
  import type { GamingMouse, MouseState } from "$lib/api/types";
  import SectionHeader from "$lib/components/layout/SectionHeader.svelte";
  import Callout from "$lib/components/ui/Callout.svelte";
  import Segmented from "$lib/components/ui/Segmented.svelte";
  import Slider from "$lib/components/ui/Slider.svelte";
  import Spinner from "$lib/components/ui/Spinner.svelte";
  import Switch from "$lib/components/ui/Switch.svelte";
  import { num, t } from "$lib/i18n.svelte";
  import { app } from "$lib/state/app.svelte";
  import { onMount } from "svelte";

  /** Ce qui s'écrit dans une souris de jeu. */
  interface Draft {
    dpis: number[];
    active: number;
    rate: number | null;
  }

  let info = $state<MouseState | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let acceleration = $state(true);
  /** Multiplicateur du curseur (vitesse libinput + 1). */
  let factor = $state(1);
  let drafts = $state<Record<string, Draft>>({});
  let saving = $state<string | null>(null);
  let installing = $state(false);

  const pointer = $derived(info?.pointer ?? null);
  const ratbag = $derived(info?.ratbag ?? null);

  const draftOf = (mouse: GamingMouse): Draft => ({
    dpis: mouse.stages.map((s) => s.dpi),
    active: Math.max(0, mouse.stages.findIndex((s) => s.active)),
    rate: mouse.rate,
  });

  async function load() {
    try {
      info = await api.mouseState();
      acceleration = info.pointer.acceleration;
      factor = Math.min(2, Math.max(0.1, info.pointer.speed + 1));
      drafts = Object.fromEntries(info.ratbag.mice.map((m) => [m.id, draftOf(m)]));
    } catch (e) {
      error = errorMessage(e);
    } finally {
      loading = false;
    }
  }

  async function savePointer() {
    error = null;
    try {
      await api.setPointer(acceleration, Math.round((factor - 1) * 1000) / 1000);
    } catch (e) {
      error = errorMessage(e);
    }
  }

  const changed = (mouse: GamingMouse) => JSON.stringify(drafts[mouse.id]) !== JSON.stringify(draftOf(mouse));

  async function saveMouse(mouse: GamingMouse) {
    const draft = drafts[mouse.id];
    saving = mouse.id;
    error = null;
    try {
      await api.setMouseHardware(mouse.id, draft.dpis, draft.active, draft.rate);
      await load();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      saving = null;
    }
  }

  async function installRatbag() {
    installing = true;
    error = null;
    try {
      await api.installRatbag();
      await load();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      installing = false;
    }
  }

  onMount(load);
</script>

<SectionHeader
  title={t("Souris", "Mouse")}
  lead={t(
    "Accélération et sensibilité pour toutes les souris, gardées et remises à chaque connexion. Sur une souris de jeu, les paliers de DPI et la fréquence s'enregistrent dans la souris elle-même, comme avec G HUB.",
    "Acceleration and sensitivity for every mouse, kept and set again at each login. On a gaming mouse, DPI stages and polling rate are saved in the mouse itself, like with G HUB.",
  )}
/>

{#if error}<Callout kind="danger" title={t("Réglage refusé", "Setting refused")}>{error}</Callout>{/if}

{#if loading}
  <p class="muted"><Spinner /> {t("Lecture des souris…", "Reading mice…")}</p>
{:else if pointer && ratbag}
  <section class="panel card">
    <h2>{t("Pointeur", "Pointer")}</h2>
    {#if pointer.unavailable}
      <Callout kind="warning" title={t("Réglage du bureau indisponible", "Desktop setting unavailable")}>{pointer.unavailable}</Callout>
    {:else}
      <Switch
        label={t("Accélération de la souris", "Mouse acceleration")}
        bind:checked={acceleration}
        onchange={savePointer}
        hint={acceleration
          ? t(
              "Un geste rapide envoie le curseur plus loin qu'un geste lent sur la même distance : la visée change avec la vitesse du geste. Désactivée, c'est mieux pour jouer.",
              "A fast move sends the cursor further than a slow one over the same distance: aim changes with the speed of the move. Off is better for gaming.",
            )
          : t(
              "Désactivée : le curseur avance toujours de la même distance que la souris, quelle que soit la vitesse du geste. La visée reste la même d'un mouvement à l'autre.",
              "Off: the cursor always moves the same distance as the mouse, whatever the speed of the move. Aim stays the same from one move to the next.",
            )}
      />
      <Slider
        label={t("Sensibilité", "Sensitivity")}
        bind:value={factor}
        min={0.1}
        max={2}
        step={0.05}
        defaultValue={1}
        format={(v) => `× ${num(v, 2)}`}
        onchange={savePointer}
        hint={t(
          "Vitesse du curseur, × 1 : celle de la souris. Pour viser dans un jeu, réglez plutôt les DPI de la souris ou la sensibilité du jeu.",
          "Cursor speed, × 1: the mouse's own. For aiming in a game, rather set the mouse's DPI or the game's sensitivity.",
        )}
      />
    {/if}
    {#if pointer.mice.length > 0}
      <p class="field__hint">{t("Souris branchées", "Plugged-in mice")} : {pointer.mice.join(", ")}</p>
    {/if}
  </section>

  {#if !ratbag.installed}
    <section class="panel card">
      <h2>{t("Souris de jeu", "Gaming mice")}</h2>
      <p class="muted">
        {t(
          "Pour régler les DPI et la fréquence des souris de jeu (Logitech G, SteelSeries, Roccat, ASUS ROG, Glorious et d'autres), Last1 Panel installe libratbag, le moteur de Piper. Le mot de passe est demandé.",
          "To set the DPI and polling rate of gaming mice (Logitech G, SteelSeries, Roccat, ASUS ROG, Glorious and others), Last1 Panel installs libratbag, the engine behind Piper. The password is asked.",
        )}
      </p>
      {#if app.system?.tools.pacman}
        <div>
          <button class="btn btn--primary" onclick={installRatbag} disabled={installing}>
            {#if installing}<Spinner />{t("Installation…", "Installing…")}{:else}{t("Installer", "Install")}{/if}
          </button>
        </div>
      {:else}
        <Callout kind="info">{t("Installez libratbag avec le gestionnaire de paquets du système, puis rouvrez cette page.", "Install libratbag with the system's package manager, then reopen this page.")}</Callout>
      {/if}
    </section>
  {:else if ratbag.mice.length === 0}
    <Callout kind="info" title={t("Aucune souris de jeu reconnue", "No gaming mouse recognized")}>
      {t(
        "libratbag ne connaît pas les souris branchées : leurs DPI se règlent avec leurs propres boutons. L'accélération et la sensibilité au-dessus marchent pour toutes.",
        "libratbag doesn't know the plugged-in mice: their DPI is set with their own buttons. Acceleration and sensitivity above work for all of them.",
      )}
    </Callout>
  {:else}
    {#each ratbag.mice as mouse (mouse.id)}
      {@const draft = drafts[mouse.id]}
      <section class="panel card">
        <div class="mouse__head">
          <h2>{mouse.name}</h2>
          <button class="btn btn--primary" onclick={() => saveMouse(mouse)} disabled={!changed(mouse) || saving !== null}>
            {#if saving === mouse.id}<Spinner />{/if}{t("Enregistrer dans la souris", "Save to the mouse")}
          </button>
        </div>

        {#if draft.dpis.length > 0}
          <div class="field">
            <span class="field__label">{t("Paliers de DPI", "DPI stages")}</span>
            <span class="field__hint">
              {t(
                "Le bouton DPI de la souris passe d'un palier à l'autre ; le palier coché est celui de départ.",
                "The mouse's DPI button switches between stages; the checked one is the starting stage.",
              )}
            </span>
            <div class="stages">
              {#each draft.dpis as dpi, index (index)}
                <label class="stage" class:stage--active={draft.active === index}>
                  <input type="radio" name="active-{mouse.id}" checked={draft.active === index} onchange={() => (draft.active = index)} />
                  <span class="stage__name">{t(`Palier ${index + 1}`, `Stage ${index + 1}`)}</span>
                  <select value={dpi} onchange={(e) => (draft.dpis[index] = Number(e.currentTarget.value))}>
                    {#each mouse.dpiChoices.includes(dpi) ? mouse.dpiChoices : [dpi, ...mouse.dpiChoices] as choice (choice)}
                      <option value={choice}>{choice} DPI</option>
                    {/each}
                  </select>
                </label>
              {/each}
            </div>
          </div>
        {/if}

        {#if mouse.rateChoices.length > 1 && draft.rate !== null}
          <div class="field">
            <span class="field__label">{t("Fréquence d'interrogation", "Polling rate")}</span>
            <Segmented
              label={t("Fréquence d'interrogation", "Polling rate")}
              options={mouse.rateChoices.map((hz) => ({ value: hz, label: `${hz} Hz` }))}
              value={draft.rate}
              onchange={(hz) => (draft.rate = hz)}
            />
            <span class="field__hint">
              {t(
                `La souris donne sa position ${draft.rate} fois par seconde. ${Math.max(...mouse.rateChoices)} Hz, le plus haut, est le plus réactif pour jouer.`,
                `The mouse reports its position ${draft.rate} times per second. ${Math.max(...mouse.rateChoices)} Hz, the highest, is the most responsive for gaming.`,
              )}
            </span>
          </div>
        {/if}
      </section>
    {/each}
  {/if}
{/if}

<style>
  .card {
    display: grid;
    gap: var(--space-5);
    padding: var(--space-5);
  }

  h2 {
    margin: 0;
    font-size: var(--text-lg);
  }

  .mouse__head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
  }

  .stages {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(12rem, 1fr));
    gap: var(--space-3);
  }

  .stage {
    display: grid;
    grid-template-columns: auto 1fr;
    align-items: center;
    gap: var(--space-2) var(--space-3);
    padding: var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    cursor: pointer;
  }

  .stage--active {
    border-color: var(--color-panel-line);
    background: var(--color-panel-soft);
  }

  .stage__name {
    font-weight: 600;
  }

  .stage select {
    grid-column: 1 / -1;
  }
</style>
