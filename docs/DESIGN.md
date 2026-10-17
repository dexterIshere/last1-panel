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
| Vibrance numérique | **nvibrant** (NVIDIA, MIT), dans le matériel d'affichage ; **ses binaires livrés dans notre paquet** | Seul outil qui règle la vibrance sous Wayland (interface NVKMS du pilote) ; aucun coût en jeu. Un binaire par version du pilote (ceux publiés par son auteur sur PyPI, empreinte vérifiée) ; Last1 Panel prend le plus récent qui ne dépasse pas le pilote installé, comme le lanceur Python de nvibrant, sans Python ni AUR. Non gardé au redémarrage : réappliqué à la connexion. |
| Écrans | **`kscreen-doctor`** (Plasma), **`hyprctl` + `monitors.conf`** (Hyprland) | Les outils des bureaux eux-mêmes ; KWin garde ses réglages, Hyprland relit `monitors.conf`. |
| Carte graphique | **Service LACT** (`lactd`, socket JSON) | Maintenu, NVIDIA (NVML), AMD et Intel ; courbes de ventilateurs tenues par le service ; changement confirmé sous 5 s, sinon annulé. |
| Réglages 3D par jeu | **Option de lancement Steam `last1-panel-cli run %command%`**, écrite par Last1 Panel (Steam fermé) | Le lanceur lit nos réglages au lancement et pose les variables de Proton, DXVK, VKD3D-Proton, dxvk-nvapi, du pilote et de MangoHud. `dlss-swapper` et `game-performance` de CachyOS sont repris tels quels. |
| Réglages remis à la connexion | **Service utilisateur `last1-panel-apply.service`** (`last1-panel-cli watch`), actif pour tous les comptes | La vibrance est perdue au redémarrage et au rebranchement d'un écran : remise à la connexion, puis à chaque événement DRM (`udevadm monitor`), réessayée tant que le bureau n'a pas allumé l'écran. Les profils ICC, eux, sont gardés par KWin et par monitors.conf. Le service ne dépend pas du bureau (Hyprland n'active pas graphical-session.target). |
| Distribution | **AUR `last1-panel-bin`** (binaires compilés une fois par la CI) et **releases GitHub** ; page sur last1.fun | Compilé dans un conteneur Arch à chaque nouvelle version de `Cargo.toml` : la release garde le paquet (`pacman -U`) et l'archive des binaires que télécharge le PKGBUILD de l'AUR, poussé par la CI. Installé en quelques secondes, sans compilation chez le joueur ; dépendances des dépôts officiels (WebKitGTK, lcms2, LACT). go-to-linux l'installe d'office depuis l'AUR (ligne `si-publié` de son `apps.list`). |

## Réglages 3D : ce que pose le lanceur

Vérifié dans les sources de chaque projet (octobre 2026). Une variable déjà
posée par l'utilisateur dans l'option de lancement n'est jamais remplacée.

| Réglage | Jeu par Proton | Jeu natif | Remarques |
|---|---|---|---|
| Images par seconde max. | `DXVK_CONFIG` : `dxgi.maxFrameRate`, `d3d9.maxFrameRate` ; `VKD3D_FRAME_RATE` | MangoHud caché (`fps_limit`, `no_display`) | `DXVK_FRAME_RATE` n'existe plus depuis DXVK 3.0. La DXGI de DXVK limite aussi le D3D12. |
| V-Sync forcée | `dxgi.syncInterval`, `d3d9.presentInterval` ; `VKD3D_SWAPCHAIN_PRESENT_MODE` | NVIDIA : `__GL_SYNC_TO_VBLANK` ; Mesa : `vblank_mode`, `MESA_VK_WSI_PRESENT_MODE` | |
| Faible latence | `dxgi.maxFrameLatency=1`, `d3d9.maxFrameLatency=1`, `VKD3D_SWAPCHAIN_LATENCY_FRAMES=1` | — | `__GL_MaxFramesAllowed` n'est documentée nulle part : pas utilisée. |
| Ultra (Reflex) | + `DXVK_NVAPI_VKREFLEX=1` (jeux Vulkan) | idem | Les jeux D3D11 avec Reflex l'ont par dxvk-nvapi, actif d'office dans Proton. |
| Filtrage anisotrope | `d3d11.samplerAnisotropy`, `d3d9.samplerAnisotropy` | NVIDIA : `__GL_LOG_MAX_ANISO` ; AMD : `RADV_TEX_ANISO` | Pas d'équivalent dans VKD3D-Proton (sauf RADV). |
| DLSS dernière version | `dlss-swapper` de CachyOS (sinon ses variables : `PROTON_ENABLE_NGX_UPDATER`, `DXVK_NVAPI_DRS_NGX_DLSS_*`) | — | |
| Indicateur DLSS | `PROTON_DLSS_INDICATOR=1` | — | Proton CachyOS. |
| Smooth Motion | `NVPRESENT_ENABLE_SMOOTH_MOTION=1` | idem (Vulkan) | RTX 40 et plus récentes. |
| HDR | `PROTON_ENABLE_WAYLAND=1`, `DXVK_HDR=1` ; désactivé : `DXVK_NO_HDR=1` | — | Proton CachyOS active le HDR seul quand c'est possible. |
| Performances à l'écran | `mangohud` (configuration de l'utilisateur gardée : `read_cfg`) | idem | |
| Mode performance | `game-performance` de CachyOS, sinon `gamemoderun` | idem | |

Le dernier lancement est noté dans `~/.cache/last1-panel/last-launch.txt`
(variables posées, commande), pour vérifier ce qu'un jeu a reçu.

## Intégration à go-to-linux

1. **last1.fun** : page `/last1-panel` (français et anglais), téléchargement
   gratuit depuis la dernière release GitHub, sans compte.
2. **Installation de CachyOS** : `last1-panel-bin` et `lact` sont une ligne
   `si-publié` de `linux/apps.list` de go-to-linux : dès que le paquet est sur
   l'AUR, le premier démarrage l'installe (paru) et active le service LACT
   (`lactd`) ; les PC déjà installés le voient dans « Nouveautés pour ce PC ».
   Rien à publier côté go-to-linux.
3. **Mises à jour** : celles des paquets de l'AUR (paru), comme Vesktop ou
   Spotify.
4. `gtl-ecrans` ne touche plus aux écrans dès que monitors.conf ne commence
   plus par « # gtl:auto » : Last1 Panel remplace cet en-tête à son premier
   réglage, le choix de l'utilisateur gagne.

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
