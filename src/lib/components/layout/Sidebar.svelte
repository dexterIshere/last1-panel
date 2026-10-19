<!-- Barre latérale : marque, pages, carte graphique et bureau, langue. -->
<script lang="ts">
  import Mark from "$lib/components/ui/Mark.svelte";
  import Spinner from "$lib/components/ui/Spinner.svelte";
  import { api, errorMessage } from "$lib/api/ipc";
  import { i18n, t, type Lang } from "$lib/i18n.svelte";
  import { app, type Section } from "$lib/state/app.svelte";
  import { Cpu, Gamepad2, Gauge, Monitor, Palette, Power } from "@lucide/svelte";

  const LANGS: Lang[] = ["fr", "en"];

  const sections = $derived<{ id: Section; title: string; detail: string; icon: typeof Palette }[]>([
    { id: "colors", title: t("Couleurs", "Colors"), detail: t("Gamma, contraste, vibrance", "Gamma, contrast, vibrance"), icon: Palette },
    { id: "display", title: t("Affichage", "Display"), detail: t(`Définition, fréquence, ${app.vrrName}`, `Resolution, refresh, ${app.vrrName}`), icon: Monitor },
    { id: "gpu", title: t("Carte graphique", "Graphics card"), detail: t("Puissance, ventilateurs", "Power, fans"), icon: Cpu },
    { id: "games", title: t("Jeux", "Games"), detail: t("Réglages 3D par jeu", "Per-game 3D settings"), icon: Gamepad2 },
    { id: "perf", title: t("Performances", "Performance"), detail: t("Diagnostic en jeu", "In-game diagnostic"), icon: Gauge },
  ]);

  let updating = $state(false);
  let updateError = $state<string | null>(null);

  /** Installe la nouvelle version ; Last1 Panel se rouvre dessus. */
  async function installUpdate() {
    updating = true;
    updateError = null;
    try {
      await api.installUpdate();
    } catch (e) {
      updateError = errorMessage(e);
      updating = false;
    }
  }

  const desktop = $derived(
    app.system?.desktop === "hyprland" ? "Hyprland" : app.system?.desktop === "plasma" ? "KDE Plasma" : t("Bureau non pris en charge", "Unsupported desktop"),
  );
</script>

<aside class="sidebar">
  <div class="brand">
    <Mark size={34} />
    <div>
      <strong class="brand__name">Last1 Panel</strong>
      <span class="brand__by">{t("par Last1", "by Last1")}</span>
    </div>
    <div class="lang" role="group" aria-label={t("Langue", "Language")}>
      {#each LANGS as lang (lang)}
        <button class="lang__option mono" aria-pressed={i18n.lang === lang} onclick={() => app.setLanguage(lang)}>{lang.toUpperCase()}</button>
      {/each}
    </div>
  </div>

  <nav aria-label={t("Réglages", "Settings")}>
    <ul class="nav">
      {#each sections as section (section.id)}
        {@const Icon = section.icon}
        <li>
          <button class="nav__item" aria-current={app.section === section.id ? "page" : undefined} onclick={() => (app.section = section.id)}>
            <span class="nav__icon"><Icon size={18} /></span>
            <span class="nav__text">
              <span class="nav__title">{section.title}</span>
              <span class="nav__detail">{section.detail}</span>
            </span>
          </button>
        </li>
      {/each}
    </ul>
  </nav>

  <div class="pc">
    {#each app.system?.gpus ?? [] as gpu (gpu.card)}
      <div class="pc__gpu">
        <span class="badge" class:badge--info={gpu.vendor === "nvidia"}>{gpu.vendor === "other" ? "GPU" : gpu.vendor.toUpperCase()}</span>
        <span class="pc__name">{gpu.name ?? gpu.pciSlot}</span>
        {#if (app.system?.gpus.length ?? 0) > 1}
          <span class="pc__role">{gpu.games ? t("jeux", "games") : gpu.integrated ? t("intégrée", "integrated") : ""}</span>
        {/if}
        {#if gpu.driver}<span class="pc__driver mono">{gpu.driver}{gpu.driverVersion ? ` ${gpu.driverVersion}` : ""}</span>{/if}
      </div>
    {/each}
    {#if app.update}
      <button class="btn btn--primary update" onclick={installUpdate} disabled={updating}>
        {#if updating}<Spinner />{t("Mise à jour…", "Updating…")}{:else}{t(`Mettre à jour (${app.update})`, `Update (${app.update})`)}{/if}
      </button>
      {#if updateError}<p class="pc__meta update__error">{updateError}</p>{/if}
    {/if}
    <div class="pc__foot">
      <p class="pc__meta mono">{desktop} · v{app.system?.version ?? "0.1.0"}</p>
      <button
        class="quit"
        onclick={() => api.quit()}
        title={t(
          "Ferme la fenêtre et l'icône près de l'horloge. L'icône revient à la prochaine connexion.",
          "Closes the window and the icon by the clock. The icon comes back at the next login.",
        )}
      >
        <Power size={14} />{t("Quitter", "Quit")}
      </button>
    </div>
  </div>
</aside>

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    width: 17.5rem;
    flex: none;
    padding: var(--space-5) var(--space-4);
    border-right: 1px solid var(--color-border);
    background: var(--color-sidebar);
  }

  .brand {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: 0 var(--space-2) var(--space-5);
  }

  .brand__name {
    display: block;
    font: 600 var(--text-lg) / 1.2 var(--font-display);
    letter-spacing: -0.01em;
  }

  .brand__by {
    color: var(--color-text-faint);
    font: var(--text-2xs) / 1.4 var(--font-mono);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .lang {
    display: flex;
    flex: none;
    margin-left: auto;
    overflow: hidden;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
  }

  .lang__option {
    padding: 2px 6px;
    border: 0;
    background: transparent;
    color: var(--color-text-faint);
    font-size: var(--text-2xs);
    cursor: pointer;
  }

  .lang__option[aria-pressed="true"] {
    background: var(--color-surface-raised);
    color: var(--color-text);
  }

  .nav {
    display: grid;
    gap: var(--space-1);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .nav__item {
    position: relative;
    display: flex;
    align-items: center;
    gap: var(--space-3);
    width: 100%;
    padding: var(--space-3);
    border: 1px solid transparent;
    border-radius: var(--radius-md);
    background: transparent;
    text-align: left;
    cursor: pointer;
    transition: background var(--duration-fast) var(--ease-out);
  }

  .nav__item:hover {
    background: var(--color-surface);
  }

  .nav__item[aria-current="page"] {
    border-color: var(--color-border);
    background: var(--color-surface);
  }

  .nav__item[aria-current="page"]::before {
    content: "";
    position: absolute;
    top: 25%;
    bottom: 25%;
    left: -1px;
    width: 3px;
    border-radius: 3px;
    background: var(--color-panel);
  }

  .nav__icon {
    display: grid;
    place-items: center;
    width: 1.875rem;
    height: 1.875rem;
    flex: none;
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-sm);
    color: var(--color-text-muted);
  }

  .nav__item[aria-current="page"] .nav__icon {
    border-color: var(--color-panel-line);
    background: var(--color-panel-soft);
    color: var(--color-panel);
  }

  .nav__text {
    display: grid;
    min-width: 0;
  }

  .nav__title {
    font-weight: 600;
  }

  .nav__detail {
    overflow: hidden;
    color: var(--color-text-faint);
    font-size: var(--text-xs);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .pc {
    display: grid;
    gap: var(--space-2);
    margin-top: auto;
  }

  .pc__gpu {
    display: grid;
    grid-template-columns: auto 1fr;
    align-items: center;
    gap: 2px var(--space-2);
    padding: var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface);
  }

  .pc__name {
    overflow: hidden;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .pc__role {
    color: var(--color-text-muted);
    font-size: 0.75rem;
  }

  .pc__driver {
    grid-column: 2;
    color: var(--color-text-faint);
    font-size: var(--text-2xs);
  }

  .pc__meta {
    padding: 0 var(--space-2);
    color: var(--color-text-faint);
    font-size: var(--text-2xs);
  }

  .pc__foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }

  .quit {
    display: inline-flex;
    flex: none;
    align-items: center;
    gap: 4px;
    padding: 2px var(--space-2);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-text-faint);
    font-size: var(--text-2xs);
    cursor: pointer;
  }

  .quit:hover {
    border-color: var(--color-danger-line);
    color: var(--color-danger);
  }

  .update {
    justify-content: center;
  }

  .update__error {
    color: var(--color-danger);
  }
</style>
