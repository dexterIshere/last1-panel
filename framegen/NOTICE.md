# Génération d'images de Last1 Panel

Couche Vulkan **explicite** `VK_LAYER_LAST1_framegen`, allumée seulement quand
Last1 Panel lance un jeu avec `VK_LOADER_LAYERS_ENABLE=VK_LAYER_LAST1_framegen` :
le chargeur la place alors sous les couches implicites (MangoHud, vkBasalt,
overlay Steam), qui voient les vraies images ; un nom inconnu n'empêche pas le
jeu de démarrer.

## Origine

- **afmf-linux** 1.3.1, commit `bc56e5bd0e3a6c328aedef664d0d36363fb58b36`
  (tag `v1.3.1` signé + 2 commits de documentation), © 2026 Lain, licence MIT
  (`LICENSE`). <https://github.com/serialexperimentslainnnn/afmf-linux>
- **AMD FidelityFX SDK** (flux optique, SPD), © 2024 Advanced Micro Devices,
  licence MIT (`shaders/fidelityfx/LICENSE.txt`). Six fichiers y sont **modifiés**
  par afmf-linux (liste dans `shaders/fidelityfx/NOTICE.md`), les autres sont
  identiques à l'original.

Repris : `src/`, `layer/`, `shaders/`, `cmake/`, `CMakeLists.txt`, `LICENSE`.
Pas repris : tests, empaquetage, documentation, CI.

## Changements de Last1 Panel

Vérifiés par une revue du code (sécurité, robustesse, NVIDIA), 2026-11-04.

- Nom de couche, variables et bibliothèque propres (`VK_LAYER_LAST1_framegen`,
  `LAST1_FG_*`, `libVkLayer_last1_framegen.so`), couche explicite : aucun conflit
  avec un afmf-linux installé par le joueur, et ses variables `AFMF_*` sont
  ignorées.
- Écriture d'images sur le disque (`AFMF_DUMP_DIR`) retirée.
- Journal dans un fichier seulement par `LAST1_FG_LOG_FILE` (ajout, sans lien
  symbolique, 0600).
- Mode de présentation du jeu gardé (la synchro verticale n'est plus coupée).
- Sous 40 images réelles par seconde, pas de génération.
- Interpolation impossible : vrai passage direct (plus d'image répétée).
- Swapchain refusée avec les ajouts : celle du jeu, sans génération.
- Erreur de la couche : génération arrêtée, rien renvoyé au jeu (sauf la perte
  du périphérique).
- `oldSwapchain` : génération arrêtée sur l'ancienne avant la recréation.
- 8 images au plus.
- SAD 16 bits empaquetée coupée sur NVIDIA.
- Options de durcissement de la compilation.
