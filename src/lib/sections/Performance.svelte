<!-- Performances : diagnostic de ce qui freine les jeux sur ce PC, et le bouton
     qui règle chaque point réglable (« Tout régler » : un seul mot de passe). -->
<script lang="ts">
  import { api, errorMessage } from "$lib/api/ipc";
  import type { PerfCheck, PerfFix, PerfReport } from "$lib/api/types";
  import SectionHeader from "$lib/components/layout/SectionHeader.svelte";
  import Callout from "$lib/components/ui/Callout.svelte";
  import WarningDialog from "$lib/components/ui/WarningDialog.svelte";
  import Spinner from "$lib/components/ui/Spinner.svelte";
  import Switch from "$lib/components/ui/Switch.svelte";
  import { app } from "$lib/state/app.svelte";
  import { t } from "$lib/i18n.svelte";
  import { Check, CircleAlert, Copy, Info, RefreshCw, TriangleAlert, Wrench } from "@lucide/svelte";

  let report = $state<PerfReport | null>(null);
  let loading = $state(true);
  /** Corrections en cours (une, ou toutes). */
  let fixing = $state<PerfFix[]>([]);
  let error = $state<string | null>(null);
  let notices = $state<string[]>([]);
  let copied = $state(false);

  async function load() {
    loading = true;
    error = null;
    try {
      report = await api.performanceReport();
    } catch (e) {
      error = errorMessage(e);
    }
    loading = false;
  }

  $effect(() => {
    load();
  });

  const sharing = $derived(app.sharing);
  const setSharing = (enabled: boolean) => app.setSharing(enabled).catch((e) => (error = errorMessage(e)));

  async function fix(kinds: PerfFix[]) {
    fixing = kinds;
    error = null;
    notices = [];
    try {
      notices = await api.performanceFix(kinds);
      await load();
      // Relu après coup : un réglage qui n'a pas pris le dit, jamais rien
      // de muet.
      const stuck = kinds.filter((kind) => kind !== "restart" && report?.checks.some((c) => c.fix === kind));
      if (stuck.length > 0) {
        notices = [];
        error = t(
          `Pas pris effet : ${stuck.map(fixLabel).join(", ")}. Le rapport de ce PC (« Copier le rapport ») dit pourquoi.`,
          `Did not take effect: ${stuck.map(fixLabel).join(", ")}. This PC's report (“Copy the report”) says why.`,
        );
      }
    } catch (e) {
      error = errorMessage(e);
    }
    fixing = [];
  }

  async function copy() {
    if (!report) return;
    await navigator.clipboard.writeText(report.text).catch(() => undefined);
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }

  const toFix = $derived(report?.checks.filter((c) => c.level === "problem" || c.level === "warning").length ?? 0);
  /** Tout ce qui se règle d'un clic (le redémarrage reste à part). */
  const fixable = $derived(report?.checks.flatMap((c) => (c.fix && c.fix !== "restart" ? [c.fix] : [])) ?? []);
  let confirmRestart = $state(false);
  const fixLabel = (kind: PerfFix) =>
    ({
      "low-latency": t("Rendre la latence aux jeux", "Give latency back to games"),
      "install-mango-hud": t("Installer MangoHud", "Install MangoHud"),
      "wrap-games": t("Activer pour tous les jeux", "Turn on for all games"),
      performance: t("Passer en mode performance", "Switch to performance mode"),
      ntsync: t("Activer NTSync", "Turn on NTSync"),
      "shader-cache": t("Agrandir à 12 Go", "Enlarge to 12 GB"),
      "split-lock": t("Arrêter ce frein", "Stop this slowdown"),
      "x3d-cache": t("Mettre les jeux sur la 3D V-Cache", "Put games on the 3D V-Cache"),
      scheduler: t("Revenir à l'ordonnanceur du noyau", "Go back to the kernel's scheduler"),
      zram: t("Activer la mémoire compressée", "Turn on compressed memory"),
      restart: t("Redémarrer le PC", "Restart the PC"),
    })[kind];
  const icon = (check: PerfCheck) => ({ problem: CircleAlert, warning: TriangleAlert, info: Info, ok: Check })[check.level];
</script>

<SectionHeader
  title={t("Performances en jeu", "In-game performance")}
  lead={t(
    "Ce qui fait perdre des images par seconde sur ce PC par rapport à Windows, et le bouton qui le règle. Le mot de passe du PC est demandé une fois.",
    "What costs frames per second on this PC compared to Windows, and the button that fixes it. The PC's password is asked once.",
  )}
>
  {#snippet actions()}
    {#if fixable.length > 1}
      <button class="btn btn--primary" onclick={() => fix(fixable)} disabled={fixing.length > 0}>
        {#if fixing.length > 1}<Spinner />{:else}<Wrench size={16} />{/if}{t(`Tout régler (${fixable.length})`, `Fix everything (${fixable.length})`)}
      </button>
    {/if}
    <button class="btn" onclick={copy} disabled={!report}>
      {#if copied}<Check size={16} />{:else}<Copy size={16} />{/if}{t("Copier le rapport", "Copy the report")}
    </button>
    <button class="btn" onclick={load} disabled={loading}>
      {#if loading}<Spinner />{:else}<RefreshCw size={16} />{/if}{t("Relancer", "Run again")}
    </button>
  {/snippet}
</SectionHeader>

{#if error}<Callout kind="danger">{error}</Callout>{/if}
{#each notices as notice (notice)}<Callout kind="info">{notice}</Callout>{/each}

{#if report}
  {#if toFix === 0}
    <Callout kind="success">{t("Rien ne freine les jeux sur ce PC.", "Nothing holds games back on this PC.")}</Callout>
  {/if}
  <div class="layout">
    <section class="panel card">
      <ul class="checks">
        {#each report.checks as check (check.id)}
          {@const Icon = icon(check)}
          <li class="check check--{check.level}">
            <span class="check__icon"><Icon size={18} /></span>
            <div class="check__text">
              <strong>{check.title}</strong>
              <span class="field__hint">{check.detail}</span>
            </div>
            {#if check.fix}
              <button
                class="btn"
                onclick={() => (check.fix === "restart" ? (confirmRestart = true) : fix([check.fix!]))}
                disabled={fixing.length > 0}
              >
                {#if fixing.includes(check.fix)}<Spinner />{/if}{fixLabel(check.fix)}
              </button>
            {/if}
          </li>
        {/each}
      </ul>
    </section>

    {#if report.sessions.length > 0}
      <section class="panel card sessions">
        <h2>{t("Dernières parties", "Last games")}</h2>
        <table>
          <thead>
            <tr>
              <th>{t("Jeu", "Game")}</th>
              <th>{t("Images/s", "FPS")}</th>
              <th>1 %</th>
              <th>{t("Processeur", "CPU")}</th>
              <th>{t("Carte", "GPU")}</th>
              <th>{t("Limite", "Limit")}</th>
            </tr>
          </thead>
          <tbody>
            {#each report.sessions as session (session.program + session.started)}
              <tr>
                <td><strong>{session.program}</strong><span class="field__hint">{session.started} · {session.minutes} min</span></td>
                <td class="mono">{session.fpsAverage}</td>
                <td class="mono">{session.fpsOnePercentLow}</td>
                <td class="mono">{session.cpuLoad} %</td>
                <td class="mono">{session.gpuLoad} %</td>
                <td>{session.limit === "gpu" ? t("Carte graphique", "Graphics card") : session.limit === "cpu" ? t("Processeur", "CPU") : "?"}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </section>
    {/if}

    <aside class="panel card facts">
      <h2>{t("Ce PC", "This PC")}</h2>
      <dl>
        {#each report.facts as [name, value] (name)}
          <dt>{name}</dt>
          <dd class="mono">{value}</dd>
        {/each}
      </dl>
      <p class="field__hint">
        {t(
          "Pour de l'aide : « Copier le rapport », avec le nom du jeu et les images par seconde sous Windows et sous Linux.",
          "For help: “Copy the report”, with the game's name and the frame rate on Windows and on Linux.",
        )}
      </p>
      {#if sharing && !sharing.goToLinux}
        <Switch
          label={t("Partager ces rapports", "Share these reports")}
          hint={t(
            "Avec last1.fun, au plus une fois par jour après vos parties, pour optimiser Last1 Panel : composants, diagnostic, images par seconde. Ni nom ni compte.",
            "With last1.fun, at most once a day after your games, to optimize Last1 Panel: components, diagnostic, frame rate. No name, no account.",
          )}
          checked={sharing.enabled}
          onchange={setSharing}
        />
      {/if}
    </aside>
  </div>
{:else if loading}
  <p class="muted"><Spinner /> {t("Lecture du PC…", "Reading the PC…")}</p>
{/if}

{#if confirmRestart}
  <WarningDialog
    title={t("Redémarrer le PC ?", "Restart the PC?")}
    confirmLabel={t("Redémarrer", "Restart")}
    onconfirm={() => {
      confirmRestart = false;
      fix(["restart"]);
    }}
    oncancel={() => (confirmRestart = false)}
  >
    <p>{t("Les programmes ouverts seront fermés : enregistrez votre travail avant.", "Open programs will be closed: save your work first.")}</p>
  </WarningDialog>
{/if}

<style>
  .layout {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 20rem;
    gap: var(--space-4);
    align-items: start;
  }

  .card {
    padding: var(--space-4);
  }

  .checks {
    display: grid;
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .check {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
  }

  .check__icon {
    display: grid;
    place-items: center;
    flex: none;
  }

  .check--problem {
    border-color: var(--color-danger-line);
  }

  .check--problem .check__icon {
    color: var(--color-danger);
  }

  .check--warning {
    border-color: var(--color-warning-line);
  }

  .check--warning .check__icon {
    color: var(--color-warning);
  }

  .check--info .check__icon {
    color: var(--color-text-muted);
  }

  .check--ok .check__icon {
    color: var(--color-panel);
  }

  .check__text {
    display: grid;
    flex: 1;
    gap: 2px;
    min-width: 0;
    /* Les commandes à lancer se sélectionnent et se copient. */
    user-select: text;
  }

  .sessions {
    grid-column: 1;
    overflow-x: auto;
  }

  table {
    width: 100%;
    margin-top: var(--space-3);
    border-collapse: collapse;
    font-size: var(--text-sm);
  }

  th,
  td {
    padding: var(--space-2);
    border-bottom: 1px solid var(--color-border);
    text-align: left;
    vertical-align: top;
  }

  td strong {
    display: block;
  }

  th {
    color: var(--color-text-muted);
    font-weight: 600;
  }

  .facts {
    grid-row: 1 / span 2;
    grid-column: 2;
    display: grid;
    gap: var(--space-3);
  }

  h2 {
    font-size: var(--text-lg);
  }

  dl {
    display: grid;
    gap: var(--space-1);
    margin: 0;
  }

  dt {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  dd {
    margin: 0 0 var(--space-2);
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }

  @media (max-width: 70rem) {
    .layout {
      grid-template-columns: minmax(0, 1fr);
    }

    .facts {
      grid-row: auto;
      grid-column: auto;
    }
  }
</style>
