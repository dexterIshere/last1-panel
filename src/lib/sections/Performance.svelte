<!-- Performances : diagnostic de ce qui freine les jeux sur ce PC (lecture seule ;
     les optimisations du système sont posées par go-to-linux). -->
<script lang="ts">
  import { api, errorMessage } from "$lib/api/ipc";
  import type { PerfCheck, PerfFix, PerfReport } from "$lib/api/types";
  import SectionHeader from "$lib/components/layout/SectionHeader.svelte";
  import Callout from "$lib/components/ui/Callout.svelte";
  import Spinner from "$lib/components/ui/Spinner.svelte";
  import { t } from "$lib/i18n.svelte";
  import { Check, CircleAlert, Copy, Info, RefreshCw, TriangleAlert } from "@lucide/svelte";

  let report = $state<PerfReport | null>(null);
  let loading = $state(true);
  let fixing = $state<PerfFix | null>(null);
  let error = $state<string | null>(null);
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

  async function fix(kind: PerfFix) {
    fixing = kind;
    error = null;
    try {
      await api.performanceFix(kind);
      await load();
    } catch (e) {
      error = errorMessage(e);
    }
    fixing = null;
  }

  async function copy() {
    if (!report) return;
    await navigator.clipboard.writeText(report.text).catch(() => undefined);
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }

  const toFix = $derived(report?.checks.filter((c) => c.level === "problem" || c.level === "warning").length ?? 0);
  const fixLabel = (kind: PerfFix) =>
    ({
      "low-latency": t("Rendre la latence aux jeux", "Give latency back to games"),
      "install-mango-hud": t("Installer MangoHud", "Install MangoHud"),
      "wrap-games": t("Activer pour tous les jeux", "Turn on for all games"),
    })[kind];
  const icon = (check: PerfCheck) => ({ problem: CircleAlert, warning: TriangleAlert, info: Info, ok: Check })[check.level];
</script>

<SectionHeader
  title={t("Performances en jeu", "In-game performance")}
  lead={t(
    "Ce qui fait perdre des images par seconde sur ce PC par rapport à Windows. Rien n'est changé ici : go-to-linux pose les optimisations du système dès l'installation (« Jeux et performances »).",
    "What costs frames per second on this PC compared to Windows. Nothing is changed here: go-to-linux sets the system optimizations at install (“Games and performance”).",
  )}
>
  {#snippet actions()}
    <button class="btn" onclick={copy} disabled={!report}>
      {#if copied}<Check size={16} />{:else}<Copy size={16} />{/if}{t("Copier le rapport", "Copy the report")}
    </button>
    <button class="btn btn--primary" onclick={load} disabled={loading}>
      {#if loading}<Spinner />{:else}<RefreshCw size={16} />{/if}{t("Relancer", "Run again")}
    </button>
  {/snippet}
</SectionHeader>

{#if error}<Callout kind="danger" title={t("Diagnostic impossible", "Diagnostic failed")}>{error}</Callout>{/if}

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
              <button class="btn" onclick={() => fix(check.fix!)} disabled={fixing !== null}>
                {#if fixing === check.fix}<Spinner />{/if}{fixLabel(check.fix)}
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
    </aside>
  </div>
{:else if loading}
  <p class="muted"><Spinner /> {t("Lecture du PC…", "Reading the PC…")}</p>
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
