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
| Courbes | **Calcul de la rampe gamma de Windows** des outils NVIDIA (WindowsDisplayAPI, `DisplayGammaRamp`, LGPL-3.0, réécrit) : contraste qui rogne ou relève le noir d'au plus 10 % / 12,5 %, luminosité en décalage d'un cinquième au plus, puis gamma en puissance | Les réglages copiés du panneau NVIDIA sous Windows donnent la même image. Celui de nvidia-settings (X11), bien plus fort, écrasait les ombres : 72 % de contraste sur le seul bleu jaunissait l'image. |
| Profil ICC | **Primaires sRGB, blanc D65 ; TRC en puissance 2,2 sous Plasma, sRGB sous Hyprland** (lcms2, VCGT en table 3 × 256 × 16 bits) | Chaque compositeur suppose sa fonction de transfert pour un écran SDR (KWin : 2,2 ; Hyprland avec un profil ICC : sRGB) : le profil seul ne change rien, seule notre courbe agit. Hyprland n'accepte que la VCGT en table. |
| « Tous » et canaux | **Curseurs de « Tous » et de chaque canal gardés avec le réglage** (`all`, `own`), les courbes de l'écran calculées d'après eux : chaque curseur de canal hors de sa valeur par défaut remplace « Tous » pour ce canal | Comme le panneau NVIDIA : un guide règle « Tous » puis le bleu, dans n'importe quel ordre, et aucun curseur ne déplace les autres. « Suivre « Tous » » remet les curseurs d'un canal par défaut. |
| Application des couleurs | **Plasma : en direct**, à chaque mouvement de curseur. **Hyprland : au clic sur « Appliquer »**, par `hyprctl keyword monitor` puis `hyprctl dispatch forcerendererreload` | KWin applique un nouveau profil aussitôt. Hyprland ne recharge la courbe dans la carte (table gamma) qu'en réappliquant tout l'écran (`applyMonitorRule`) : un bref clignotement possible, donc pas à chaque mouvement. |
| Vibrance numérique | **nvibrant** (NVIDIA, MIT), dans le matériel d'affichage ; **ses binaires livrés dans notre paquet** | Seul outil qui règle la vibrance sous Wayland (interface NVKMS du pilote) ; aucun coût en jeu. Un binaire par version du pilote (ceux publiés par son auteur sur PyPI, empreinte vérifiée) ; Last1 Panel prend le plus récent qui ne dépasse pas le pilote installé, comme le lanceur Python de nvibrant, sans Python ni AUR. Non gardé au redémarrage : réappliqué à la connexion. |
| Écrans | **`kscreen-doctor`** (Plasma), **`hyprctl` + `monitors.conf`** (Hyprland) | Les outils des bureaux eux-mêmes ; KWin garde ses réglages, Hyprland relit `monitors.conf`. |
| Souris | **Pointeur réglé par le bureau** (KWin : propriétés D-Bus de `org.kde.KWin.InputDevice`, gardées par KWin ; Hyprland : un bloc `device` par souris dans `~/.config/hypr/last1-panel-mouse.conf`, sourcé depuis `hyprland.conf`, et `hyprctl keyword device[…]` tout de suite) ; **DPI et fréquence par libratbag** (`ratbagd`, D-Bus système, installé par un bouton : pkexec, pacman) | Accélération (profil libinput plat ou adaptatif) et vitesse libinput valent pour toutes les souris, pavés tactiles exclus ; gardées dans nos réglages, remises à la connexion et à chaque souris branchée (événement `input` de `udevadm monitor`, déjà lu par le service). Paliers de DPI, palier de départ et fréquence d'interrogation s'écrivent dans la mémoire de la souris (Logitech G, SteelSeries, Roccat, ASUS ROG, Glorious…), comme G HUB : ils restent vrais sur tout PC. |
| Carte graphique | **Service LACT** (`lactd`, socket JSON) | Maintenu, NVIDIA (NVML), AMD et Intel ; courbes de ventilateurs tenues par le service ; changement confirmé sous 5 s, sinon annulé. |
| Cartes graphiques du PC | **Une seule description pour tout PC** (`core/src/system/gpu.rs`) : chaque carte est intégrée (Intel sur le bus du processeur, AMD décrite par KFD comme processeur et puce graphique) ou à part ; **la carte des jeux** (`choose_games`) : une carte à part plutôt qu'une puce intégrée, puis celle qui a le plus d'écrans, puis NVIDIA, AMD, Intel, puis la principale. | Tout s'en sert : le lanceur (marque de la carte des jeux ; si elle ne dessine pas le bureau, le jeu lui est envoyé : PRIME NVIDIA ou `DRI_PRIME=pci-…` ; MangoHud `pci_dev` quand il y a plusieurs cartes), le diagnostic (vérifications des seules cartes à part, chaque ligne nommée quand il y en a plusieurs, pilote de la carte des jeux), l'interface (Reflex, G-SYNC/FreeSync, carte proposée au réglage). Testé pour : carte seule NVIDIA/AMD/Intel, portables Intel ou Ryzen + NVIDIA ou Radeon, MUX, deux et trois cartes, Intel Arc, machine virtuelle. |
| Réglages 3D par jeu | **Option de lancement Steam `$(command -v last1-panel-cli && echo run) %command%`**, écrite par Last1 Panel (Steam fermé) ; `last1-panel-cli fix wrap-games` la pose sur tous les jeux pour go-to-linux | Steam passe l'option à `/bin/sh` : Last1 Panel désinstallé, il ne reste que `%command%`, et aucun jeu ne cesse de démarrer. Le lanceur lit nos réglages au lancement et pose les variables de Proton, DXVK, VKD3D-Proton, dxvk-nvapi, du pilote et de MangoHud. `dlss-swapper` de CachyOS est repris tel quel ; le mode performance est tenu comme `game-performance` (`powerprofilesctl launch`), mais avec un verrou de mise en veille de l'écran seulement : l'arrêt du PC n'est jamais bloqué par un jeu. |
| Profils de couleurs d'un jeu | **Table de couleurs 3D (.cube, 33³) appliquée par vkBasalt** (zlib, compilé depuis sa version publiée et livré dans nos paquets), sous son propre nom de couche Vulkan (`VK_LAYER_LAST1_color`, manifeste `last1-panel-color.json`), active seulement si le lanceur pose `LAST1_PANEL_COLOR=1` | Les courbes de l'écran (VCGT) valent pour tout le bureau : Windows ne fait pas mieux sans ReShade. vkBasalt colore chaque image du jeu avant l'affichage : jeux Proton (DXVK, VKD3D-Proton) et Vulkan, 64 bits. La table (`core/src/color/lut.rs`) applique la vibrance du profil (saturation), puis ses courbes, puis l'inverse des courbes et de la vibrance de l'écran, que le pilote et le compositeur appliquent encore par-dessus : le jeu montre exactement son profil. Refaite à chaque lancement dans `~/.cache/last1-panel/color`. Touche de comparaison : Arrêt défil (Début, celle de vkBasalt, sert dans des jeux). |
| Performances en jeu | **Diagnostic** (`core/src/system/diagnostic.rs`, page Performances, `last1-panel-cli diagnostic`) **et un bouton par point réglable** (`core/src/system/repair.rs`) | Les optimisations du système (mode performance, X3D, split locks, NTSync, cache des shaders, ordonnanceur) sont posées par go-to-linux dès l'installation ; ailleurs (Arch, AppImage), ou s'il en manque une, le bouton pose la même chose, sans commande à taper. Les corrections d'administrateur passent par un seul script pkexec (« Tout régler » : un mot de passe), dans nos propres fichiers : `/etc/modules-load.d/last1-panel-ntsync.conf`, `/etc/udev/rules.d/99-last1-panel-{ntsync,x3d}.rules`, `/etc/sysctl.d/99-last1-panel-split-lock.conf`, `/etc/tmpfiles.d/last1-panel-performance.conf` (sans power-profiles-daemon, jamais sur un portable), lignes de `/etc/environment`, limites de puissance du processeur (RAPL) données au compte du joueur (`/etc/tmpfiles.d/last1-panel-power-limit.conf`) : le service de la session relève la limite longue durée au niveau de la courte le temps des jeux, sur secteur, et la rend à la fin (`core/src/system/power_limit.rs`), portables Razer Blade : contrôleur (EC) ouvert au compte du joueur (`/etc/udev/rules.d/70-last1-panel-razer.rules`, hidraw des modèles connus de razer-laptop-control) ; pendant les jeux, sur secteur, le service le met en mode Gaming, ventilateurs à fond, comme Synapse sous Windows, et rend le mode d'avant à la fin (`core/src/system/razer.rs`, rapports HID « feature » d'openrazer), Panel Self Refresh coupé pour l'écran d'un portable sur une puce Intel ou AMD (`/etc/modprobe.d/99-last1-panel-psr.conf`, image de démarrage refaite : fin quadrillage ou image figée quand rien ne bouge, retard à la reprise), zram comme CachyOS (`zram-generator`, `/etc/systemd/zram-generator.conf` s'il manque, `/etc/sysctl.d/99-last1-panel-zram.conf`) ; `scx_loader`/`scx` désactivés. La mémoire d'échange se lit dans `/proc/swaps` : zram (en RAM) n'est jamais un problème, le disque est un conseil. « Redémarrer le PC » : `systemctl reboot` (logind), après confirmation. Le lanceur des jeux n'ajoute rien d'office. |
| Shaders compilés par Steam | **Bandeau de toutes les pages avec jauge** (`ShaderBanner.svelte`, `core/src/games/shaders.rs`) et notifications du service (début après 30 s, puis fin ou interruption) | L'avancement est lu comme Steam le lit : bloc de contrôle partagé de Fossilize (`/proc/<pid>/fd/<--shmem-fd>` du `fossilize_replay` maître, mêmes contrôles de disposition, même calcul que `compute_condensed_progress`) ; le jeu vient de `shadercache/<appid>`. Temps restant : vitesse des deux dernières minutes. Steam ne donne que peu de cœurs à la compilation : le service lui donne tous ceux du processeur (`unShaderBackgroundProcessingThreads` dans `steam_dev.cfg`, lu au démarrage de Steam ; il arrête de compiler quand un jeu se lance) ; Steam lancé avant, le bandeau propose « Accélérer » (Steam fermé et rouvert, jamais pendant un jeu). |
| Clips des parties | **Vice** (splitshotz/Vice, GPL-3.0), installé d'office par go-to-linux ; la page Clips l'ouvre, démarre son service (`vice.service`) s'il est arrêté, ou mène à son installation (go-to-linux › Applications, sinon sa page) | Un seul enregistreur sur le PC : Last1 Panel n'enregistre rien lui-même, il ne doublerait pas l'encodeur de Vice. Touches de Vice : F9 (20 dernières secondes), F9 deux fois (toute la partie), dossier `~/Videos/Vice`. |
| Icône près de l'horloge | **StatusNotifierItem tenu par le service de session** (`core/src/tray.rs`, crate ksni, sans GTK) | Le service tourne déjà en permanence : l'icône ne coûte pas une fenêtre WebKit ouverte. Menu : profils de couleurs de chaque écran (cochés d'après les réglages enregistrés), « Tous les écrans », ouvrir la fenêtre (`systemd-run`, hors du groupe du service). Réglages relus toutes les 2 s s'ils ont changé ; masquable (réglage `tray`). « Quitter » (menu, ou bas de la fenêtre) ferme les fenêtres (`pkill -x last1-panel`, ce compte) et arrête le service, sans le relancer : sortie normale avec `Restart=on-failure`, ou `systemctl --user stop` depuis la fenêtre ; il revient à la prochaine connexion. |
| Réglages remis à la connexion | **Service utilisateur `last1-panel-apply.service`** (`last1-panel-cli watch`), actif pour tous les comptes | La vibrance est perdue au redémarrage et au rebranchement d'un écran : remise à la connexion, puis à chaque événement DRM (`udevadm monitor`), réessayée tant que le bureau n'a pas allumé l'écran. Une souris branchée (événement `input`) reçoit l'accélération et la sensibilité gardées. Les profils ICC, eux, sont gardés par KWin et par monitors.conf. Le service ne dépend pas du bureau (Hyprland n'active pas graphical-session.target). |
| Distribution | **AppImage et paquet Arch publiés ensemble sur last1.fun** à chaque commit sur `main` (CI, jeton OIDC) ; **gratuit et sans compte** ; **aucun paquet de l'AUR** | Comme SoundFence : le joueur télécharge l'AppImage (construite dans Debian 12, elle tourne partout) et l'ouvre, elle s'installe dans son dossier sans mot de passe (`src-tauri/src/install.rs`) : copie dans `~/.local/bin`, entrée du menu, outil des jeux et nvibrant dans `~/.local/lib/last1-panel`, service de session. L'option de lancement Steam appelle alors l'outil par son chemin complet. Mise à jour automatique (`core/src/update.rs`) : chaque jour, le service de session lit la liste publique, vérifie sa signature avec la clé de last1.fun embarquée, et si la version publiée est plus récente que la sienne (posée à la construction), télécharge l'AppImage, vérifie taille et empreinte, la remplace d'un coup, la lance en `--integrate` (outil des jeux et nvibrant reposés, sans fenêtre) puis se fait relancer par systemd sur le nouvel outil. LACT, service du système, s'installe par un bouton (pkexec, pacman). go-to-linux installe d'office le paquet Arch sous CachyOS et le tient à jour (liste signée, empreinte, `pacman -U`) ; l'AppImage passe alors la main à `/usr/bin/last1-panel` et retire ce qu'elle avait posé. Les outils lancés depuis l'AppImage reçoivent un environnement sans le sien (`exec::appimage_env`). |

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
| Mode performance | Automatique dans chaque jeu, sans réglage : `systemd-inhibit --what=idle powerprofilesctl launch -p performance` (comme `game-performance` de CachyOS, sans bloquer l'arrêt du PC), sinon `gamemoderun` | idem | GameMode ne change le gouverneur que pour un membre du groupe `gamemode` (sa règle polkit, `allow_active` à `no`) : le diagnostic le vérifie dans `/etc/group`, « Activer GameMode » l'installe s'il manque et y ajoute le compte (`PKEXEC_UID`). |

Le dernier lancement est noté dans `~/.cache/last1-panel/last-launch.txt`
(variables posées, commande), pour vérifier ce qu'un jeu a reçu.

## Intégration à go-to-linux

1. **last1.fun** : Last1 Panel est un produit gratuit du catalogue
   (`"free": true`) ; sa liste signée (`/api/public/last1-panel/manifest`),
   son AppImage et son paquet sont publics, et sa page propose l'AppImage.
2. **Installation de CachyOS** : au premier démarrage, après les mises à jour
   et les pilotes, go-to-linux télécharge le paquet depuis last1.fun, vérifie
   la signature de la liste et l'empreinte du paquet, l'installe avec pacman et
   active le service LACT (`lactd`).
3. **Mises à jour** : la mise à jour quotidienne de go-to-linux fait de même
   quand last1.fun publie une version plus récente ; les PC déjà installés le
   reçoivent ainsi à leur prochaine mise à jour.
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
| Souris Razer, souris sans mémoire | libratbag ne les règle pas : leurs DPI se changent par leurs boutons (accélération et sensibilité marchent pour toutes). Hyprland en configuration Lua (`hyprland.lua`) : pointeur non réglable d'ici. |
| Profil de couleurs d'un jeu OpenGL ou 32 bits | vkBasalt ne voit que les jeux Vulkan 64 bits (Proton les y met presque tous) ; ailleurs, le jeu a les couleurs de l'écran. |

## Questions ouvertes

1. Correspondance entre les prises de nvibrant (numérotées par le pilote) et les
   sorties DRM (DP-1, HDMI-A-1) : ordre par type supposé, à vérifier sur
   plusieurs cartes, et sur les écrans en DisplayPort MST.
2. Hyprland en configuration Lua (`hyprland.lua`, par défaut depuis 0.55) :
   `hyprctl keyword` y est refusé. Bureau Signal de go-to-linux en hyprlang :
   réglages d'écran désactivés en Lua pour l'instant, avec la raison.
3. Hyprland : hyprsunset et les autres outils de gamma (wlr-gamma-control)
   sont refusés tant qu'une courbe VCGT est posée ; à signaler.
