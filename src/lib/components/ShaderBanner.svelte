<!-- Bandeau de toutes les pages : Steam compile des shaders (les jeux
     saccadent pendant ce temps), pour quel jeu, où il en est et quand il aura
     fini ; puis « prêts » quelques secondes. -->
<script lang="ts">
  import { api } from "$lib/api/ipc";
  import type { Precache } from "$lib/api/types";
  import { t } from "$lib/i18n.svelte";
  import { onDestroy, onMount } from "svelte";

  let precache = $state<Precache | null>(null);
  /** Compilation qui vient de s'arrêter (affichée 15 s) : finie, ou
   * interrompue par Steam (il la reprend plus tard). */
  let stopped = $state<{ game: string | null; ratio: number | null } | null>(null);
  /** Avancement relevé (heure, fraction) pour le temps restant. */
  let samples: { at: number; ratio: number }[] = [];
  let remaining = $state<number | null>(null);
  /** Temps de processeur relevé (heure, centièmes de seconde) : les cœurs au
   * travail prouvent que la compilation avance, même sans pourcentage. */
  let cpuSamples: { at: number; ticks: number }[] = [];
  let cores = $state<number | null>(null);
  /** Plus rien ne bouge depuis deux minutes. */
  let stalled = $state(false);
  let timer: ReturnType<typeof setInterval> | undefined;
  let stoppedTimer: ReturnType<typeof setTimeout> | undefined;

  const ratio = $derived(
    precache?.done != null && precache.total ? Math.min(1, precache.done / precache.total) : null,
  );

  async function poll() {
    let next: Precache | null;
    try {
      next = await api.shaderPrecache();
    } catch {
      return;
    }
    if (!next) {
      if (precache) {
        stopped = { game: precache.game, ratio };
        clearTimeout(stoppedTimer);
        stoppedTimer = setTimeout(() => (stopped = null), 15_000);
      }
      precache = null;
      samples = [];
      cpuSamples = [];
      cores = null;
      stalled = false;
      remaining = null;
      return;
    }
    if (precache?.game !== next.game) {
      samples = [];
      cpuSamples = [];
    }
    precache = next;
    stopped = null;
    const now = Date.now();
    cpuSamples.push({ at: now, ticks: next.cpuTicks });
    cpuSamples = cpuSamples.filter((s) => now - s.at <= 150_000);
    const busy = (since: number) => {
      const window = cpuSamples.filter((s) => now - s.at <= since);
      const first = window[0];
      const last = window[window.length - 1];
      // Un processus parti fait baisser le total : pas de chiffre faux.
      return last.at - first.at >= 6_000 && last.ticks >= first.ticks
        ? (last.ticks - first.ticks) / 100 / ((last.at - first.at) / 1000)
        : null;
    };
    cores = busy(30_000);
    const quiet = busy(120_000);
    stalled = quiet !== null && now - cpuSamples[0].at >= 115_000 && quiet < 0.05;
    if (next.done != null && next.total) {
      samples.push({ at: now, ratio: Math.min(1, next.done / next.total) });
      // Vitesse des deux dernières minutes : elle change d'un jeu à l'autre.
      samples = samples.filter((s) => now - s.at <= 120_000);
      const first = samples[0];
      const last = samples[samples.length - 1];
      const speed = (last.ratio - first.ratio) / (last.at - first.at);
      remaining = last.at - first.at >= 15_000 && speed > 0 ? (1 - last.ratio) / speed : null;
    }
  }

  /** « 52 min », « 1 h 05 ». */
  function since(seconds: number): string {
    const minutes = Math.floor(seconds / 60);
    if (minutes < 1) return t("moins d'une minute", "less than a minute");
    if (minutes < 60) return `${minutes} min`;
    return `${Math.floor(minutes / 60)} h ${(minutes % 60).toString().padStart(2, "0")}`;
  }

  function duration(ms: number): string {
    const minutes = Math.ceil(ms / 60_000);
    if (minutes <= 1) return t("moins d'une minute", "less than a minute");
    if (minutes < 60) return t(`environ ${minutes} min`, `about ${minutes} min`);
    const hours = Math.floor(minutes / 60);
    const rest = minutes % 60;
    return t(`environ ${hours} h ${rest.toString().padStart(2, "0")}`, `about ${hours} h ${rest.toString().padStart(2, "0")}`);
  }

  onMount(() => {
    poll();
    timer = setInterval(poll, 3_000);
  });
  onDestroy(() => {
    clearInterval(timer);
    clearTimeout(stoppedTimer);
  });
</script>

{#if precache}
  <section class="banner" role="status" aria-live="polite">
    <div class="banner__head">
      <strong class="banner__title">
        {precache.game
          ? t(`Steam compile les shaders de ${precache.game}`, `Steam is compiling the shaders of ${precache.game}`)
          : t("Steam compile des shaders", "Steam is compiling shaders")}
      </strong>
      {#if ratio !== null}
        <span class="banner__percent">{Math.floor(ratio * 100)} %</span>
      {/if}
    </div>
    <div
      class="gauge"
      class:gauge--waiting={ratio === null}
      role="progressbar"
      aria-valuemin="0"
      aria-valuemax="100"
      aria-valuenow={ratio === null ? undefined : Math.floor(ratio * 100)}
    >
      <div class="gauge__fill" style:width={ratio === null ? undefined : `${ratio * 100}%`}></div>
    </div>
    <p class="banner__text">
      {#if ratio === null && !precache.readable}
        {t("Steam ne donne pas son avancement à Last1 Panel : la jauge attend la fin.", "Steam doesn't share its progress with Last1 Panel: the gauge waits for the end.")}
      {:else if ratio === null}
        {t("Steam prépare la compilation.", "Steam is getting the compilation ready.")}
      {:else if remaining !== null}
        {t(`Fini dans ${duration(remaining)}.`, `Done in ${duration(remaining)}.`)}
      {:else}
        {t("Calcul du temps restant…", "Working out the time left…")}
      {/if}
      {t(
        "C'est Steam (« Traitement des shaders Vulkan »), après une mise à jour d'un jeu ou du pilote : pendant ce temps, les jeux peuvent saccader. Mieux vaut attendre la fin pour jouer ; ensuite, ils tournent mieux.",
        "It's Steam (“Vulkan shader processing”), after a game or driver update: meanwhile, games may stutter. Better wait for the end to play; afterwards, they run better.",
      )}
    </p>
    {#if stalled}
      <p class="banner__alive banner__alive--stalled">
        {t(
          "Plus rien ne bouge depuis 2 minutes : Steam a peut-être mis la compilation en pause (un jeu lancé ?). Elle reprend d'elle-même ; si elle reste figée, fermez Steam et rouvrez-le.",
          "Nothing has moved for 2 minutes: Steam may have paused the compilation (a game running?). It resumes on its own; if it stays frozen, close Steam and open it again.",
        )}
      </p>
    {:else if precache.runningSecs !== null || cores !== null}
      <p class="banner__alive">
        {#if precache.runningSecs !== null}
          {t(`En cours depuis ${since(precache.runningSecs)}`, `Running for ${since(precache.runningSecs)}`)}
        {/if}
        {#if cores !== null}
          · {t(
            `${cores.toLocaleString("fr-FR", { maximumFractionDigits: 1 })} cœur${cores >= 2 ? "s" : ""} du processeur au travail : ça avance.`,
            `${cores.toLocaleString("en-US", { maximumFractionDigits: 1 })} CPU core${cores >= 2 ? "s" : ""} at work: it's moving.`,
          )}
        {/if}
      </p>
    {/if}
  </section>
{:else if stopped && stopped.ratio !== null && stopped.ratio < 0.95}
  <section class="banner" role="status" aria-live="polite">
    <strong class="banner__title">
      {t(`Compilation interrompue à ${Math.floor(stopped.ratio * 100)} %`, `Compilation paused at ${Math.floor(stopped.ratio * 100)} %`)}
    </strong>
    <p class="banner__text">
      {stopped.game
        ? t(`Steam a arrêté de compiler les shaders de ${stopped.game} : il reprendra plus tard, la jauge reviendra ici.`, `Steam stopped compiling the shaders of ${stopped.game}: it will resume later, the gauge will come back here.`)
        : t("Steam a arrêté de compiler : il reprendra plus tard, la jauge reviendra ici.", "Steam stopped compiling: it will resume later, the gauge will come back here.")}
    </p>
  </section>
{:else if stopped}
  <section class="banner banner--done" role="status" aria-live="polite">
    <strong class="banner__title">
      {stopped.game ? t(`Shaders de ${stopped.game} prêts`, `${stopped.game} shaders ready`) : t("Shaders prêts", "Shaders ready")}
    </strong>
    <p class="banner__text">{t("Steam a fini : les jeux ne saccadent plus à cause de lui.", "Steam is done: games no longer stutter because of it.")}</p>
  </section>
{/if}

<style>
  .banner {
    display: grid;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--color-warning-line);
    border-radius: var(--radius-md);
    background: var(--color-warning-soft);
  }

  .banner--done {
    border-color: var(--color-volt-line);
    background: var(--color-volt-soft);
  }

  .banner__head {
    display: flex;
    gap: var(--space-3);
    align-items: baseline;
    justify-content: space-between;
  }

  .banner__title {
    color: var(--color-warning);
    font-weight: 600;
  }

  .banner--done .banner__title {
    color: var(--color-volt);
  }

  .banner__percent {
    color: var(--color-warning);
    font-size: 1.25rem;
    font-variant-numeric: tabular-nums;
    font-weight: 700;
  }

  .banner__text {
    margin: 0;
    color: var(--color-text-muted);
  }

  .banner__alive {
    margin: 0;
    color: var(--color-text);
    font-variant-numeric: tabular-nums;
  }

  .banner__alive--stalled {
    color: var(--color-warning);
    font-weight: 600;
  }

  .gauge {
    overflow: hidden;
    height: 10px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--color-warning) 18%, transparent);
  }

  .gauge__fill {
    height: 100%;
    border-radius: inherit;
    background: var(--color-warning);
    transition: width 0.6s ease;
  }

  .gauge--waiting .gauge__fill {
    width: 30%;
    animation: waiting 1.6s ease-in-out infinite;
  }

  @keyframes waiting {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(340%);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .gauge--waiting .gauge__fill {
      animation: none;
      width: 100%;
      opacity: 0.4;
    }
  }
</style>
