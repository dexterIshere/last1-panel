<!-- Couleurs : luminosité, contraste et gamma par écran (profil ICC), vibrance numérique (NVIDIA),
     profils de couleurs (pour l'écran, ou pour un jeu dans sa seule fenêtre). -->
<script lang="ts">
  import { api, errorMessage } from "$lib/api/ipc";
  import type { Channel, ColorAdjust, ColorProfile, ColorState, Game, GamesInfo, Output } from "$lib/api/types";
  import { CHANNELS, allChannel, neutralAdjust, ownChannels, sameChannel, sameChannels } from "$lib/color";
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
  import { Link2, RotateCcw, Save, Trash2, X } from "@lucide/svelte";

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
  /** Jeux Steam, pour lier un profil à un jeu. */
  let games = $state<GamesInfo | null>(null);
  /** Jeu choisi en enregistrant un nouveau profil. */
  let profileGame = $state("");
  let steamBusy = $state(false);

  const live = $derived(app.system?.features?.liveColor ?? false);
  /** Valeurs de « Tous », et les canaux réglés à part, qu'il ne change pas. */
  const all = $derived(allChannel(adjust));
  const own = $derived(ownChannels(adjust));
  const editing = $derived<Channel>(channel === "all" ? all : adjust[channel]);
  const CHANNEL_NAMES = $derived({
    red: t("le rouge", "red"),
    green: t("le vert", "green"),
    blue: t("le bleu", "blue"),
  });
  const names = (list: readonly (typeof CHANNELS)[number][]) => {
    const words = list.map((c) => CHANNEL_NAMES[c]);
    const text = words.length > 1 ? `${words.slice(0, -1).join(", ")} ${t("et", "and")} ${words[words.length - 1]}` : (words[0] ?? "");
    return text.charAt(0).toUpperCase() + text.slice(1);
  };

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

  async function loadGames() {
    try {
      games = await api.games();
    } catch {
      games = null;
    }
  }

  $effect(() => {
    load();
    loadProfiles();
    loadGames();
  });

  const profileOf = (game: Game) => games?.settings.perGame[String(game.appId)]?.colorProfile ?? null;
  const gamesOf = (id: string) => games?.games.filter((g) => profileOf(g) === id) ?? [];
  /** Jeux liés à un profil dont l'option de lancement manque encore. */
  const unwrapped = $derived(games?.games.filter((g) => !!profileOf(g) && !g.wrapped) ?? []);
  const canLink = $derived(!!games?.colorLayer && (games?.games.length ?? 0) > 0);

  /** `profile` à chaque lancement de ce jeu, dans sa seule fenêtre (null : plus de profil). */
  async function link(appId: number, profile: ColorProfile | null) {
    profileBusy = true;
    error = null;
    notice = null;
    try {
      const result = await api.linkGameColor(appId, profile?.id ?? null);
      await loadGames();
      const name = games?.games.find((g) => g.appId === appId)?.name ?? "";
      if (!result.needsSteamClosed) {
        notice = profile
          ? t(`${name} prendra « ${profile.name} » dès son lancement, dans sa seule fenêtre.`, `${name} will get “${profile.name}” as soon as it starts, in its window only.`)
          : t(`${name} garde les couleurs de l'écran.`, `${name} keeps the screen's colors.`);
      }
    } catch (e) {
      error = errorMessage(e);
    }
    profileBusy = false;
  }

  async function wrapPending() {
    steamBusy = true;
    error = null;
    notice = null;
    try {
      const names = unwrapped.map((g) => g.name).join(", ");
      await api.closeSteamAndWrap(unwrapped.map((g) => g.appId));
      await loadGames();
      notice = t(`Option de lancement mise (${names}) : le profil s'active dès le lancement.`, `Launch option set (${names}): the profile turns on as soon as the game starts.`);
    } catch (e) {
      error = errorMessage(e);
    }
    steamBusy = false;
  }

  $effect(() => {
    loadOutput(selected);
  });

  /** Comme le panneau NVIDIA : « Tous » règle les canaux qui le suivent ;
   * un canal réglé à part garde ses valeurs, et « Tous » les siennes. */
  function update(key: keyof Channel, value: number) {
    const current = allChannel(adjust);
    if (channel === "all") {
      const followers = CHANNELS.filter((c) => sameChannel(adjust[c], current));
      const next = { ...current, [key]: value };
      adjust.all = next;
      for (const target of followers) adjust[target] = { ...next };
    } else {
      adjust.all = current;
      adjust[channel][key] = value;
    }
    changed();
  }

  /** Ces canaux reprennent les valeurs de « Tous », et les suivent. */
  function follow(targets: readonly (typeof CHANNELS)[number][]) {
    const current = allChannel(adjust);
    adjust.all = current;
    for (const target of targets) adjust[target] = { ...current };
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
      const game = existing ? undefined : games?.games.find((g) => String(g.appId) === profileGame);
      const name = existing?.name ?? (profileName.trim() || game?.name || "");
      const saved = await api.saveColorProfile(existing?.id ?? null, name, $state.snapshot(adjust), vibrance);
      await loadProfiles();
      notice = existing
        ? t(`« ${saved.name} » a maintenant ces réglages.`, `“${saved.name}” now has these settings.`)
        : t(`Profil « ${saved.name} » enregistré.`, `Profile “${saved.name}” saved.`);
      if (!existing) {
        profileName = "";
        profileGame = "";
      }
      profileBusy = false;
      if (game) await link(game.appId, saved);
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
      await loadGames();
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
      {#if channel === "all" && own.length > 0}
        <div class="linked">
          <p class="field__hint">
            {#if own.length === 3}
              {t("Rouge, vert et bleu ont chacun leur réglage : « Tous » ne les change pas.", "Red, green and blue each have their own setting: “All” does not change them.")}
            {:else}
              {t(
                `${names(own)} ${own.length > 1 ? "ont leur" : "a son"} propre réglage, que « Tous » ne change pas.`,
                `${names(own)} ${own.length > 1 ? "have their" : "has its"} own setting, which “All” does not change.`,
              )}
            {/if}
          </p>
          <button class="btn btn--ghost" onclick={() => follow(own)} disabled={!!info?.colorUnavailable}>
            <Link2 size={16} />{t("Remettre sur « Tous »", "Back to “All”")}
          </button>
        </div>
      {:else if channel !== "all" && own.includes(channel)}
        <div class="linked">
          <p class="field__hint">
            {t(
              `Réglage propre au ${CHANNEL_NAMES[channel].replace(/^le /, "")} : « Tous » ne le change plus.`,
              `${names([channel])} has its own setting: “All” no longer changes it.`,
            )}
          </p>
          <button class="btn btn--ghost" onclick={() => follow([channel as (typeof CHANNELS)[number]])} disabled={!!info?.colorUnavailable}>
            <Link2 size={16} />{t("Suivre « Tous »", "Follow “All”")}
          </button>
        </div>
      {:else if channel !== "all"}
        <p class="field__hint">
          {t(
            "Ce canal suit « Tous » tant que vous ne le réglez pas ici. Seul, il renforce ou éteint sa couleur dans toute l'image (un contraste fort sur le bleu seul jaunit les ombres).",
            "This channel follows “All” until you set it here. On its own, it strengthens or fades its color across the whole picture (high contrast on blue alone turns shadows yellow).",
          )}
        </p>
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

    <section class="panel card">
      <div>
        <h2>{t("Profils de couleurs", "Color profiles")}</h2>
        <p class="field__hint">
          {t(
            "Ces réglages gardés sous un nom : pour les remettre sur l'écran, ou pour un jeu, dans sa seule fenêtre.",
            "These settings kept under a name: to put them back on the screen, or for a game, in its window only.",
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
        {#if canLink}
          <select class="save__game" bind:value={profileGame} aria-label={t("Jeu du profil", "Profile's game")}>
            <option value="">{t("Pour l'écran seulement", "For the screen only")}</option>
            {#each games?.games ?? [] as game (game.appId)}<option value={String(game.appId)}>{t(`Pour ${game.name}`, `For ${game.name}`)}</option>{/each}
          </select>
        {/if}
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
                {#if canLink}
                  <div class="profile__games">
                    {#each gamesOf(profile.id) as game (game.appId)}
                      <span class="chip" class:chip--warning={!game.wrapped} title={game.wrapped ? t("S'active dès le lancement du jeu", "Turns on as soon as the game starts") : t("Option de lancement à mettre (Steam ouvert)", "Launch option still to set (Steam open)")}>
                        {game.name}
                        <button class="chip__remove" onclick={() => link(game.appId, null)} disabled={profileBusy} aria-label={t(`Délier ${game.name}`, `Unlink ${game.name}`)}><X size={12} /></button>
                      </span>
                    {/each}
                    <select
                      class="chip-select"
                      value=""
                      disabled={profileBusy}
                      aria-label={t(`Lier ${profile.name} à un jeu`, `Link ${profile.name} to a game`)}
                      onchange={(e) => {
                        const appId = Number(e.currentTarget.value);
                        e.currentTarget.value = "";
                        if (appId) link(appId, profile);
                      }}
                    >
                      <option value="">{t("+ Lier à un jeu", "+ Link to a game")}</option>
                      {#each games?.games.filter((g) => profileOf(g) !== profile.id) ?? [] as game (game.appId)}
                        <option value={String(game.appId)}>{game.name}{profileOf(game) ? ` (${profiles.find((p) => p.id === profileOf(game))?.name ?? ""})` : ""}</option>
                      {/each}
                    </select>
                  </div>
                {/if}
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
      {#if unwrapped.length > 0}
        <Callout kind="warning" title={t("Encore une étape pour ces jeux", "One more step for these games")}>
          <p>
            {t(
              `${unwrapped.map((g) => g.name).join(", ")} : l'option de lancement de Last1 Panel doit être mise dans Steam pour que le profil s'active au lancement, et Steam la remplacerait s'il restait ouvert.`,
              `${unwrapped.map((g) => g.name).join(", ")}: Last1 Panel's launch option must be set in Steam for the profile to turn on at launch, and Steam would overwrite it if left open.`,
            )}
          </p>
          <button class="btn" onclick={wrapPending} disabled={steamBusy}>
            {#if steamBusy}<Spinner />{/if}{games?.steamRunning ? t("Fermer Steam, mettre l'option et le rouvrir", "Close Steam, set the option and reopen it") : t("Mettre l'option de lancement", "Set the launch option")}
          </button>
        </Callout>
      {/if}
      {#if games && !games.colorLayer}
        <p class="field__hint">{t("Couche Vulkan vkBasalt de Last1 Panel absente : les profils ne peuvent pas aller dans les jeux (réinstallez Last1 Panel).", "Last1 Panel's vkBasalt Vulkan layer is missing: profiles cannot go into games (reinstall Last1 Panel).")}</p>
      {:else if games && games.games.length === 0}
        <p class="field__hint">{t("Aucun jeu Steam installé : les profils se lient aux jeux Steam.", "No Steam game installed: profiles link to Steam games.")}</p>
      {:else}
        <p class="field__hint">{t("Lié à un jeu, un profil s'active dès son lancement, dans sa seule fenêtre (jeux Proton et Vulkan). Touche Arrêt défil : profil coupé ou remis.", "Linked to a game, a profile turns on as soon as it starts, in its window only (Proton and Vulkan games). Scroll Lock key: profile off or back on.")}</p>
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

  .save__game {
    flex: 0 1 16rem;
    min-width: 10rem;
  }

  .linked {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }

  .profile__games {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1);
    margin-top: var(--space-1);
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    padding: 2px 4px 2px var(--space-2);
    border: 1px solid var(--color-panel-line);
    border-radius: 999px;
    background: var(--color-panel-soft);
    font-size: var(--text-sm);
  }

  .chip--warning {
    border-color: var(--color-warning-line);
    background: var(--color-warning-soft);
  }

  .chip__remove {
    display: inline-grid;
    place-items: center;
    width: 1.25rem;
    height: 1.25rem;
    padding: 0;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: inherit;
    cursor: pointer;
  }

  .chip__remove:hover:not(:disabled) {
    background: var(--color-surface-raised);
  }

  .chip-select {
    width: auto;
    min-width: 0;
    padding-block: 2px;
    font-size: var(--text-sm);
  }

  .profile__actions {
    display: flex;
    gap: var(--space-1);
  }
</style>
