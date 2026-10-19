<!-- Demande d'accord, une fois, hors go-to-linux : partager les rapports de
     performances de ce PC avec last1.fun pour optimiser Last1 Panel. Rien
     n'est envoyé sans « Oui ». Révocable dans Performances. -->
<script lang="ts">
  import { t } from "$lib/i18n.svelte";
  import { Gauge } from "@lucide/svelte";

  let { onanswer }: { onanswer: (share: boolean) => void } = $props();
</script>

<div class="backdrop">
  <div class="dialog panel" role="dialog" aria-modal="true" aria-labelledby="sharing-title">
    <h2 id="sharing-title"><Gauge size={20} />{t("Aider à améliorer Last1 Panel ?", "Help improve Last1 Panel?")}</h2>
    <div class="body">
      <p>
        {t(
          "Avec votre accord, Last1 Panel envoie à last1.fun, au plus une fois par jour après vos parties, le rapport de la page Performances :",
          "With your consent, Last1 Panel sends last1.fun, at most once a day after your games, the report of the Performance page:",
        )}
      </p>
      <ul>
        <li>{t("les composants du PC : processeur, carte graphique et pilote, mémoire, noyau, distribution ;", "the PC's components: CPU, graphics card and driver, memory, kernel, distribution;")}</li>
        <li>{t("ce que le diagnostic a lu et trouvé ;", "what the diagnostic read and found;")}</li>
        <li>{t("les images par seconde des parties mesurées, avec le nom des jeux.", "the frame rate of measured games, with the games' names.")}</li>
      </ul>
      <p>
        {t(
          "Seulement pour optimiser le produit, rien d'autre : jamais revendu ni partagé. Ni nom, ni compte, ni fichiers : le nom du compte et celui du PC sont masqués, le PC n'est désigné que par un identifiant anonyme. Vous pouvez changer d'avis à tout moment dans Performances.",
          "Only to optimize the product, nothing else: never sold or shared. No name, no account, no files: the account and PC names are masked, the PC is only identified by an anonymous ID. You can change your mind at any time in Performance.",
        )}
      </p>
    </div>
    <div class="actions">
      <button class="btn" onclick={() => onanswer(false)}>{t("Non merci", "No thanks")}</button>
      <!-- Même poids pour les deux réponses : rien ne pousse au « oui ». -->
      <button class="btn" onclick={() => onanswer(true)}>{t("Oui, partager", "Yes, share")}</button>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 10;
    display: grid;
    place-items: center;
    background: rgb(5 6 8 / 0.7);
  }

  .dialog {
    display: grid;
    gap: var(--space-3);
    width: min(36rem, calc(100vw - 2rem));
    padding: var(--space-5);
  }

  h2 {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-lg);
  }

  .body {
    display: grid;
    gap: var(--space-2);
    color: var(--color-text-muted);
  }

  ul {
    margin: 0;
    padding-left: var(--space-5);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: var(--space-2);
    margin-top: var(--space-2);
  }
</style>
