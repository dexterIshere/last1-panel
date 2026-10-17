<!-- Carte graphique : mesures en direct, limite de puissance, ventilateurs, fréquences (service LACT). -->
<script lang="ts">
  import { api, errorMessage } from "$lib/api/ipc";
  import type { GpuDevice, GpuStats, OffsetRange, Tuning } from "$lib/api/types";
  import FanCurve from "$lib/components/FanCurve.svelte";
  import SectionHeader from "$lib/components/layout/SectionHeader.svelte";
  import Callout from "$lib/components/ui/Callout.svelte";
  import ConfirmDialog from "$lib/components/ui/ConfirmDialog.svelte";
  import Segmented from "$lib/components/ui/Segmented.svelte";
  import Slider from "$lib/components/ui/Slider.svelte";
  import Spinner from "$lib/components/ui/Spinner.svelte";
  import { num, t } from "$lib/i18n.svelte";
  import { app } from "$lib/state/app.svelte";
  import { onDestroy } from "svelte";

  const DEFAULT_CURVE: [number, number][] = [
    [30, 25],
    [50, 35],
    [65, 55],
    [75, 75],
    [85, 100],
  ];

  let devices = $state<GpuDevice[]>([]);
  let selected = $state("");
  let stats = $state<GpuStats | null>(null);
  let tuning = $state<Tuning | null>(null);
  let saved = $state<string>("");
  let gpuRange = $state<OffsetRange | null>(null);
  let memRange = $state<OffsetRange | null>(null);
  let unavailable = $state<string | null>(null);
  let error = $state<string | null>(null);
  let loading = $state(true);
  let applying = $state(false);
  let enabling = $state(false);
  let confirmSeconds = $state<number | null>(null);
  let poll: ReturnType<typeof setInterval> | undefined;

  type FanMode = "auto" | "fixed" | "curve";
  let fanMode = $state<FanMode>("auto");
  let fanFixed = $state(50);
  let fanCurve = $state<[number, number][]>(DEFAULT_CURVE);

  const device = $derived(devices.find((d) => d.id === selected) ?? null);
  const temperature = $derived(stats ? (stats.temperatures["GPU"] ?? stats.temperatures["edge"] ?? Object.values(stats.temperatures)[0] ?? null) : null);

  async function load() {
    loading = true;
    unavailable = null;
    try {
      devices = (await api.gpuDevices()).filter((d) => !d.integrated);
      selected = devices[0]?.id ?? "";
      if (selected) await loadDevice(selected);
    } catch (e) {
      unavailable = errorMessage(e);
    }
    loading = false;
  }

  async function loadDevice(id: string) {
    const info = await api.gpuTuning(id);
    tuning = info.tuning;
    saved = JSON.stringify(info.tuning);
    stats = info.stats;
    gpuRange = info.gpuOffset;
    memRange = info.memOffset;
    fanMode = info.tuning.fans.mode;
    if (info.tuning.fans.mode === "fixed") fanFixed = info.tuning.fans.percent;
    if (info.tuning.fans.mode === "curve" && info.tuning.fans.points.length >= 2) fanCurve = info.tuning.fans.points;
    clearInterval(poll);
    poll = setInterval(async () => {
      try {
        stats = await api.gpuStats(id);
      } catch {
        // Mesure manquée : la suivante arrive dans une seconde.
      }
    }, 1000);
  }

  $effect(() => {
    load();
  });

  onDestroy(() => clearInterval(poll));

  /** Les réglages tels qu'ils seront envoyés à LACT. */
  function next(): Tuning | null {
    if (!tuning) return null;
    const fans =
      fanMode === "auto" ? { mode: "auto" as const } : fanMode === "fixed" ? { mode: "fixed" as const, percent: fanFixed } : { mode: "curve" as const, points: fanCurve.map(([a, b]) => [Number(a), Number(b)] as [number, number]) };
    return { ...$state.snapshot(tuning), fans };
  }

  const changed = $derived.by(() => {
    // Dépendances : chaque réglage modifiable.
    void [fanMode, fanFixed, JSON.stringify(fanCurve), tuning?.powerCap, tuning?.gpuOffset, tuning?.memOffset];
    const value = next();
    return value !== null && JSON.stringify(value) !== saved;
  });

  async function apply() {
    const value = next();
    if (!value || !selected) return;
    applying = true;
    error = null;
    try {
      confirmSeconds = await api.gpuTry(selected, value);
    } catch (e) {
      error = errorMessage(e);
    }
    applying = false;
  }

  async function keep() {
    confirmSeconds = null;
    try {
      await api.gpuConfirm(true);
      await loadDevice(selected);
    } catch (e) {
      error = errorMessage(e);
    }
  }

  async function revert() {
    confirmSeconds = null;
    // LACT annule seul un réglage non confirmé : une erreur ici veut dire que c'est déjà fait.
    await api.gpuConfirm(false).catch(() => undefined);
    await loadDevice(selected).catch(() => undefined);
  }

  async function enableLact() {
    enabling = true;
    error = null;
    try {
      await api.enableLact();
      app.system = await api.systemInfo().catch(() => app.system);
      await load();
    } catch (e) {
      error = errorMessage(e);
    }
    enabling = false;
  }

  const watts = (v: number) => `${num(v)} W`;
  const mhz = (v: number) => `${v > 0 ? "+" : ""}${num(v)} MHz`;
  const gib = (bytes: number) => num(bytes / 1024 ** 3, 1);
</script>

<SectionHeader
  title={t("Carte graphique", "Graphics card")}
  lead={t(
    "Limite de puissance, ventilateurs et fréquences, par le service LACT. Un réglage s'essaie : sans confirmation, la carte revient seule à ses réglages d'avant.",
    "Power limit, fans and clocks, through the LACT service. A setting is tried first: without confirmation, the card goes back to its previous settings on its own.",
  )}
>
  {#snippet actions()}
    {#if tuning}
      <button class="btn btn--primary" onclick={apply} disabled={!changed || applying}>
        {#if applying}<Spinner />{/if}{t("Essayer", "Try")}
      </button>
    {/if}
  {/snippet}
</SectionHeader>

{#if error}<Callout kind="danger" title={t("Réglage refusé", "Setting refused")}>{error}</Callout>{/if}

{#if loading}
  <p class="muted"><Spinner /> {t("Connexion au service LACT…", "Connecting to the LACT service…")}</p>
{:else if unavailable}
  <Callout kind="warning" title={t("Carte graphique non disponible", "Graphics card unavailable")}>
    <p>{unavailable}</p>
    {#if app.system?.tools.lact && !app.system.tools.lactRunning}
      <p class="action">
        <button class="btn btn--primary" onclick={enableLact} disabled={enabling}>
          {#if enabling}<Spinner />{/if}{t("Activer le service LACT", "Enable the LACT service")}
        </button>
      </p>
    {:else if !app.system?.tools.lact && app.system?.tools.pacman}
      <p>{t("Last1 Panel règle la carte graphique par LACT, un service du système : il s'installe une fois, avec votre mot de passe.", "Last1 Panel tunes the graphics card through LACT, a system service: it's installed once, with your password.")}</p>
      <p class="action">
        <button class="btn btn--primary" onclick={enableLact} disabled={enabling}>
          {#if enabling}<Spinner />{t("Installation de LACT…", "Installing LACT…")}{:else}{t("Installer LACT", "Install LACT")}{/if}
        </button>
      </p>
    {:else if !app.system?.tools.lact}
      <p>{t("Installez le paquet lact avec le gestionnaire de paquets de votre système.", "Install the lact package with your system's package manager.")}</p>
    {/if}
  </Callout>
{:else if !device || !tuning}
  <Callout kind="info" title={t("Aucune carte graphique dédiée", "No dedicated graphics card")}>{t("LACT ne voit que la puce graphique intégrée.", "LACT only sees the integrated graphics.")}</Callout>
{:else}
  {#if devices.length > 1}
    <Segmented label={t("Carte", "Card")} bind:value={selected} options={devices.map((d) => ({ value: d.id, label: d.name }))} onchange={(id) => loadDevice(id)} />
  {/if}

  {#if stats}
    <div class="tiles">
      <div class="tile panel">
        <span class="label">{t("Température", "Temperature")}</span>
        <strong class="tile__value">{temperature !== null ? `${num(temperature)} °C` : "—"}</strong>
        {#if stats.temperatures["GPU Hotspot"] ?? stats.temperatures["junction"]}<span class="tile__detail mono">{t("point chaud", "hotspot")} {num(stats.temperatures["GPU Hotspot"] ?? stats.temperatures["junction"])} °C</span>{/if}
      </div>
      <div class="tile panel">
        <span class="label">{t("Ventilateurs", "Fans")}</span>
        <strong class="tile__value">{stats.fanPercent !== null ? `${num(stats.fanPercent)} %` : "—"}</strong>
        <span class="tile__detail mono">{stats.fanRpm !== null ? `${num(stats.fanRpm)} tr/min` : ""}{stats.fanAuto ? ` · ${t("auto", "auto")}` : ""}</span>
      </div>
      <div class="tile panel">
        <span class="label">{t("Puissance", "Power")}</span>
        <strong class="tile__value">{stats.power !== null ? watts(Math.round(stats.power)) : "—"}</strong>
        <span class="tile__detail mono">{stats.powerCap !== null ? `${t("limite", "limit")} ${watts(Math.round(stats.powerCap))}` : ""}</span>
      </div>
      <div class="tile panel">
        <span class="label">{t("Fréquence", "Clock")}</span>
        <strong class="tile__value">{stats.gpuClock !== null ? `${num(stats.gpuClock)} MHz` : "—"}</strong>
        <span class="tile__detail mono">{stats.vramClock !== null ? `${t("mémoire", "memory")} ${num(stats.vramClock)} MHz` : ""}</span>
      </div>
      <div class="tile panel">
        <span class="label">{t("Charge", "Load")}</span>
        <strong class="tile__value">{stats.busyPercent !== null ? `${num(stats.busyPercent)} %` : "—"}</strong>
        <span class="tile__detail mono">{stats.vramUsed !== null && stats.vramTotal ? `${gib(stats.vramUsed)} / ${gib(stats.vramTotal)} Go` : ""}</span>
      </div>
    </div>
  {/if}

  <section class="panel card">
    <h2>{t("Limite de puissance", "Power limit")}</h2>
    {#if stats?.powerCapMin != null && stats?.powerCapMax != null}
      <Slider
        label={t("Limite", "Limit")}
        value={tuning.powerCap ?? stats.powerCapDefault ?? stats.powerCapMax}
        min={Math.round(stats.powerCapMin)}
        max={Math.round(stats.powerCapMax)}
        defaultValue={stats.powerCapDefault ?? undefined}
        format={watts}
        hint={t("Plus bas : carte plus fraîche et plus silencieuse, un peu moins rapide.", "Lower: cooler, quieter card, slightly slower.")}
        oninput={(v) => tuning && (tuning.powerCap = v === stats?.powerCapDefault ? null : v)}
      />
    {:else}
      <p class="muted">{t("Cette carte ne donne pas de limite réglable.", "This card has no adjustable limit.")}</p>
    {/if}
  </section>

  <section class="panel card">
    <div class="card__head">
      <h2>{t("Ventilateurs", "Fans")}</h2>
      <Segmented
        label={t("Mode des ventilateurs", "Fan mode")}
        bind:value={fanMode}
        options={[
          { value: "auto", label: t("Automatique", "Automatic") },
          { value: "fixed", label: t("Vitesse fixe", "Fixed speed") },
          { value: "curve", label: t("Courbe", "Curve") },
        ]}
      />
    </div>
    {#if fanMode === "auto"}
      <p class="muted">{t("La carte règle ses ventilateurs elle-même, comme à la sortie d'usine.", "The card controls its own fans, as from the factory.")}</p>
    {:else if fanMode === "fixed"}
      <Slider label={t("Vitesse", "Speed")} bind:value={fanFixed} min={0} max={100} format={(v) => `${num(v)} %`} />
    {:else}
      <FanCurve bind:points={fanCurve} current={temperature} />
    {/if}
  </section>

  {#if gpuRange || memRange}
    <section class="panel card">
      <h2>{t("Fréquences", "Clocks")}</h2>
      <Callout kind="warning">
        {t(
          "Monter les fréquences peut rendre la carte instable : augmentez par petits pas et essayez un jeu à chaque fois.",
          "Raising clocks can make the card unstable: go up in small steps and try a game each time.",
        )}
      </Callout>
      {#if gpuRange}
        <Slider label={t("Décalage du processeur graphique", "GPU clock offset")} value={tuning.gpuOffset ?? 0} min={gpuRange.min} max={gpuRange.max} step={5} defaultValue={0} format={mhz} oninput={(v) => tuning && (tuning.gpuOffset = v || null)} />
      {/if}
      {#if memRange}
        <Slider label={t("Décalage de la mémoire", "Memory clock offset")} value={tuning.memOffset ?? 0} min={memRange.min} max={memRange.max} step={10} defaultValue={0} format={mhz} oninput={(v) => tuning && (tuning.memOffset = v || null)} />
      {/if}
    </section>
  {:else if app.system?.gpus.some((g) => g.vendor === "amd")}
    <Callout kind="info">{t("Fréquences des cartes AMD : réglables dans LACT pour l'instant.", "AMD card clocks: adjustable in LACT for now.")}</Callout>
  {/if}
{/if}

{#if confirmSeconds !== null}
  <ConfirmDialog title={t("Garder ces réglages de la carte ?", "Keep these card settings?")} seconds={confirmSeconds} onkeep={keep} onrevert={revert} />
{/if}

<style>
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(10rem, 1fr));
    gap: var(--space-3);
  }

  .tile {
    display: grid;
    gap: var(--space-1);
    padding: var(--space-4);
  }

  .tile__value {
    font: 600 var(--text-2xl) / 1.1 var(--font-display);
    font-variant-numeric: tabular-nums;
  }

  .tile__detail {
    min-height: 1.2em;
    color: var(--color-text-faint);
    font-size: var(--text-2xs);
  }

  .card {
    display: grid;
    gap: var(--space-4);
    padding: var(--space-5);
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

  .action {
    margin-top: var(--space-3);
  }
</style>
