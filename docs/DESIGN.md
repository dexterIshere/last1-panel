# Last1 Panel : choix techniques

Objectif : rendre au joueur passé de Windows à Linux les réglages qu'il
faisait dans le panneau de configuration NVIDIA (ou AMD Adrenalin), sur les
deux bureaux de go-to-linux : Hyprland (« Signal », par défaut) et KDE Plasma,
tous deux en Wayland.

## Principes

1. **Ne pas réinventer.** Chaque réglage passe par un outil existant et
   maintenu, que Last1 Panel pilote (ligne de commande, socket, fichier de
   configuration). On n'écrit que ce qui manque : l'interface, la traduction
   des réglages de Windows, et le lien entre les outils.
2. **Les valeurs de Windows.** Luminosité et contraste à 50 %, gamma à 1,00,
   vibrance à 50 % : les mêmes échelles que le panneau NVIDIA, pour que chacun
   retrouve ses valeurs.
3. **Rien de cassé en cas de doute.** Un mode d'affichage changé se confirme
   (retour automatique sinon) ; un jeu se lance toujours, même si nos réglages
   ne peuvent pas être lus ; le profil de couleur d'origine est remis quand on
   revient aux valeurs par défaut.
4. **Dire les limites** : ce qui est impossible sous Wayland est affiché comme
   tel, avec la raison.

## Décisions

| Sujet | Choix | Pourquoi |
|---|---|---|
| Licence | **GPL-3.0-or-later**, gratuit | Logiciel libre de la suite Last1, installé d'office par go-to-linux. Une version modifiée redistribuée reste libre. |
| Interface | **Tauri 2 + SvelteKit**, DA « Signal » de last1.fun | Comme l'application go-to-linux : mêmes jetons de couleur et composants. |
| Cœur | **Crate `core` sans GTK ni WebKit** | Testable partout ; l'outil des jeux (`last1-panel-cli`) n'en dépend pas : un jeu se lance même si la fenêtre ne s'ouvre pas. |
| Luminosité, contraste, gamma | **Profil ICC par écran, avec la courbe de calibration (VCGT)**, posé par `kscreen-doctor` (Plasma) ou la règle `monitor` (Hyprland) | Aucun des deux bureaux n'a ces réglages, mais tous deux appliquent la VCGT d'un profil ICC, sur la table gamma de la carte quand c'est possible. Même méthode que les outils de calibration. |
| Courbes | **Calcul de nvidia-settings** (contraste autour du gris moyen, luminosité en décalage d'un tiers de la plage au maximum, puis gamma en puissance) | La courbe que les utilisateurs NVIDIA connaissent. Réécrite (nvidia-settings est en GPL-2.0 seule, incompatible avec la GPL-3.0). |
| Profil ICC | **Primaires sRGB, blanc D65 ; TRC en puissance 2,2 sous Plasma, sRGB sous Hyprland** (lcms2, VCGT en table 3 × 256 × 16 bits) | Chaque compositeur suppose sa fonction de transfert pour un écran SDR (KWin : 2,2 ; Hyprland avec un profil ICC : sRGB) : le profil seul ne change rien, seule notre courbe agit. Hyprland n'accepte que la VCGT en table. |
| Application des couleurs | **Plasma : en direct**, à chaque mouvement de curseur. **Hyprland : au clic sur « Appliquer »**, par `hyprctl keyword monitor` puis `hyprctl dispatch forcerendererreload` | KWin applique un nouveau profil aussitôt. Hyprland ne recharge la courbe dans la carte (table gamma) qu'en réappliquant tout l'écran (`applyMonitorRule`) : un bref clignotement possible, donc pas à chaque mouvement. |
| Vibrance numérique | **nvibrant** (NVIDIA), dans le matériel d'affichage | Seul outil qui règle la vibrance sous Wayland (interface NVKMS du pilote) ; aucun coût en jeu. Non gardé au redémarrage : réappliqué à la connexion (`last1-panel-cli apply`). |
| Écrans | **`kscreen-doctor`** (Plasma), **`hyprctl` + `monitors.conf`** (Hyprland) | Les outils des bureaux eux-mêmes ; KWin garde ses réglages, Hyprland relit `monitors.conf`. |
| Carte graphique | **Service LACT** (`lactd`, socket JSON) | Maintenu, NVIDIA (NVML), AMD et Intel ; courbes de ventilateurs tenues par le service ; changement confirmé sous 5 s, sinon annulé. |
| Réglages 3D par jeu | **Option de lancement Steam `last1-panel-cli run %command%`**, écrite par Last1 Panel (Steam fermé) | Le lanceur lit nos réglages au lancement et pose les variables de Proton, DXVK, VKD3D-Proton, dxvk-nvapi, du pilote et de MangoHud. `dlss-swapper` et `game-performance` de CachyOS sont repris tels quels. |

## Limites connues (Wayland, octobre 2026)

| Réglage du panneau NVIDIA | État |
|---|---|
| RVB / YCbCr, plage complète / limitée | Impossible : ni KWin ni Hyprland ne l'exposent pour NVIDIA. |
| Overscan | Impossible sur NVIDIA (propriété absente du pilote). |
| Vibrance sur un portable dont l'écran passe par la puce intégrée | Impossible (nvibrant ne règle que les sorties de la carte NVIDIA). |
| DLSS sur GTX 10xx et plus anciennes ; Smooth Motion avant RTX 40 | Matériel. |
| Couleurs de l'écran en HDR | Le profil ICC ne sert qu'en SDR (Plasma et Hyprland). |

## Questions ouvertes

1. Correspondance entre les prises de nvibrant (numérotées par le pilote) et les
   sorties DRM (DP-1, HDMI-A-1) : ordre par type supposé, à vérifier sur
   plusieurs cartes, et sur les écrans en DisplayPort MST.
2. Hyprland en configuration Lua (`hyprland.lua`, par défaut depuis 0.55) :
   `hyprctl keyword` y est refusé. Bureau Signal de go-to-linux en hyprlang :
   réglages d'écran désactivés en Lua pour l'instant, avec la raison.
3. Hyprland : hyprsunset et les autres outils de gamma (wlr-gamma-control)
   sont refusés tant qu'une courbe VCGT est posée ; à signaler.
