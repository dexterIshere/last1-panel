<!-- Mise en garde avant de mettre un profil de couleurs dans un jeu (filtre
     d'image, interdit par les règles de certains jeux en ligne). -->
<script lang="ts">
  import WarningDialog from "$lib/components/ui/WarningDialog.svelte";
  import { t } from "$lib/i18n.svelte";

  /** `game` absent : tous les jeux lancés avec Last1 Panel. */
  let { game, onconfirm, oncancel }: { game: string | null; onconfirm: () => void; oncancel: () => void } = $props();
</script>

<WarningDialog
  title={t("Couleurs dans un jeu : règles des jeux en ligne", "Colors in a game: online games' rules")}
  confirmLabel={t("J'accepte le risque, activer", "I accept the risk, turn on")}
  {onconfirm}
  {oncancel}
>
  <p>
    {game
      ? t(`${game} montrera le profil dans sa fenêtre,`, `${game} will show the profile in its window,`)
      : t("Tous les jeux lancés avec Last1 Panel, jeux en ligne compris, montreront le profil dans leur fenêtre,", "Every game launched with Last1 Panel, online games included, will show the profile in its window,")}
    {t(
      "par une couche Vulkan (vkBasalt), comme MangoHud : aucun fichier du jeu n'est modifié.",
      "through a Vulkan layer (vkBasalt), like MangoHud: no game file is modified.",
    )}
  </p>
  <p>
    {t(
      "Certains jeux en ligne interdisent pourtant les filtres d'image (comme ReShade) dans leurs règles, ou peuvent les détecter, et suspendre ou bannir le compte. Vérifiez les règles du jeu avant de l'activer pour un jeu en ligne.",
      "Some online games still forbid image filters (like ReShade) in their rules, or may detect them, and suspend or ban the account. Check the game's rules before turning this on for an online game.",
    )}
  </p>
  <p>
    <strong>
      {t(
        "Vous l'activez sous votre seule responsabilité : Last1 Panel et ses auteurs déclinent toute responsabilité en cas de sanction, de perte de compte ou de problème dans le jeu.",
        "You turn it on at your own risk: Last1 Panel and its authors accept no liability for any sanction, account loss or in-game problem.",
      )}
    </strong>
  </p>
</WarningDialog>
