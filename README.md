# Last1 Panel

Les réglages du **panneau de configuration NVIDIA** (et d'AMD Adrenalin) pour
jouer sous Linux, sur un bureau Wayland : couleurs de l'écran, affichage, carte
graphique et réglages 3D par jeu. Libre (GPL-3.0), gratuit, installé d'office
par [go-to-linux](https://last1.fun) sur CachyOS.

*English: the NVIDIA Control Panel (and AMD Adrenalin) settings for Linux
gaming on Wayland — screen colors and digital vibrance, display modes, GPU
tuning, per-game 3D settings. Free software (GPL-3.0), part of the Last1 suite.*

## Ce que l'on retrouve

| Panneau NVIDIA | Last1 Panel | Outil utilisé |
|---|---|---|
| Couleurs du bureau : luminosité, contraste, gamma (par canal) | **Couleurs** | profil ICC (courbe VCGT) appliqué par KWin ou Hyprland |
| Vibrance numérique | **Couleurs** (cartes NVIDIA) | [nvibrant](https://github.com/Tremeschin/nvibrant) (MIT, inclus) |
| Résolution, fréquence, G-SYNC, HDR, profondeur de couleur, orientation, échelle | **Affichage** | `kscreen-doctor` (Plasma), `hyprctl` (Hyprland) |
| Limite de puissance, fréquences, ventilateurs (MSI Afterburner) | **Carte graphique** | [LACT](https://github.com/ilya-zlobintsev/LACT) |
| Paramètres 3D par jeu : images/s max, V-Sync, faible latence, filtrage, DLSS | **Jeux** | Proton, DXVK, VKD3D-Proton, dxvk-nvapi, MangoHud |
| Couleurs propres à un jeu (profils de NVIDIA Profile Inspector, ReShade) | **Couleurs** (profils) et **Jeux** | [vkBasalt](https://github.com/DadSchoorse/vkBasalt) (zlib, inclus), dans la seule fenêtre du jeu |

Chaque réglage passe par un outil existant et maintenu : Last1 Panel les
réunit dans une seule fenêtre, en français et en anglais, et ne réécrit pas ce
qu'ils font déjà. Le détail des choix est dans [docs/DESIGN.md](docs/DESIGN.md).

**Impossible aujourd'hui sous Wayland avec une carte NVIDIA**, quel que soit
l'outil : choisir RVB ou YCbCr, plage complète ou limitée, l'overscan.

## Bureaux pris en charge

- **Hyprland** (bureau « Signal » de go-to-linux) ;
- **KDE Plasma 6** en session Wayland.

## Installer

go-to-linux l'installe d'office sous CachyOS et le tient à jour. Sinon,
téléchargez l'AppImage sur [last1.fun/last1-panel](https://last1.fun/last1-panel)
(gratuit, sans compte) et ouvrez-la, comme SoundFence : elle s'installe
d'elle-même dans votre dossier (`~/.local/bin/last1-panel`, menu des
applications, outil des jeux, nvibrant et vkBasalt dans
`~/.local/lib/last1-panel`, service de session qui remet les réglages à
chaque connexion et tient l'icône près de l'horloge). Ses mises à
jour sont proposées, jamais installées sans vous : chaque jour, le service
vérifie la liste signée de last1.fun et, s'il y a plus récent, une
notification propose « Mettre à jour ». Sinon : le bouton « Mettre à jour »
en bas de la barre latérale, ou
`~/.local/bin/last1-panel-cli update` dans un terminal. Pour la carte graphique, un bouton
installe LACT (service du système, avec votre mot de passe). Si le paquet Arch
est installé (`/usr/bin/last1-panel`), l'AppImage lui laisse la place.

## Réglages 3D des jeux

Chaque jeu Steam réglé dans Last1 Panel est lancé par
`last1-panel-cli run %command%` (option de lancement mise par Last1 Panel,
Steam fermé). Le détail des variables posées est dans
[docs/DESIGN.md](docs/DESIGN.md#réglages-3d--ce-que-pose-le-lanceur).

« DLSS : dernière version » fait utiliser au jeu une autre version du DLSS que
la sienne : certains jeux en ligne (anti-triche, conditions d'utilisation)
peuvent le sanctionner. Last1 Panel le dit et demande d'accepter ce risque
avant de l'activer ; le joueur l'active sous sa seule responsabilité (voir
aussi l'absence de garantie de la licence GPL).

## Profils de couleurs

Dans **Couleurs**, « Enregistrer ces réglages » garde luminosité, contraste,
gamma et vibrance sous un nom, pour l'écran ou pour un jeu Steam choisi dans la
liste. Un profil se remet sur l'écran d'un clic, ou se lie à un ou plusieurs
jeux (« Lier à un jeu », ou « Couleurs dans le jeu » dans **Jeux**) : il
s'active dès le lancement du jeu et ne colore que sa fenêtre, le bureau garde
ses couleurs. L'option de lancement Steam de Last1 Panel est mise d'office ;
Steam ouvert, un bouton le ferme, la met et le rouvre. Jeux par Proton et jeux
Vulkan ; la touche Arrêt défil coupe ou remet le profil en jeu. Aucun fichier
du jeu n'est modifié, mais certains jeux en ligne interdisent les filtres
d'image : une mise en garde est à accepter avant de lier un profil à un jeu,
que le joueur active sous sa seule responsabilité.

Comme dans le panneau NVIDIA, « Tous », rouge, vert et bleu ont chacun leurs
curseurs, et aucun ne déplace les autres. Un curseur de canal laissé à sa
valeur par défaut (50 %, gamma 1,00) suit « Tous » ; ailleurs, il le remplace
pour ce canal (le bleu d'un guide, par exemple).

L'icône Last1 Panel près de l'horloge (Plasma, Waybar sous Hyprland) met un
profil sur l'écran au clic droit et ouvre la fenêtre au clic gauche ; elle se
masque depuis son menu et revient par **Couleurs**.

## Construire

Il faut Rust (1.90 ou plus récent), [Bun](https://bun.sh) et les bibliothèques
de Tauri (`webkit2gtk-4.1`, `gtk3`) et `lcms2`. vkBasalt, livré dans les
paquets, se compile à part (meson, glslang : voir `packaging/arch/PKGBUILD`).

```sh
bun install
bun run tauri build --no-bundle   # target/release/last1-panel et last1-panel-cli
cargo test --workspace            # tests
bun run dev                       # interface dans un navigateur, PC simulé
```

`bun run dev` hors de Tauri simule un PC (Hyprland, RTX 4070, deux écrans) ;
paramètres d'adresse : `?bureau=plasma`, `?carte=amd`, `?lact=arrete`,
`?steam=ouvert`, `?page=display|gpu|games`.

Paquet Arch : `packaging/arch/build.sh <version> <dossier>` (PKGBUILD dans
`packaging/arch/`). AppImage : conteneur Debian 12 de `packaging/appimage/`
(commandes en tête de son `build.sh`). À chaque commit sur `main`, la CI
construit les deux et les publie ensemble sur last1.fun
(`tools/publish-release.sh`, jeton OIDC de GitHub, aucun secret), comme
SoundFence. Tant que le site ne
connaît pas Last1 Panel, l'étape est sautée avec un avertissement : relancez
alors la CI à la main (*Actions → CI → Run workflow*).

Le cœur (`core/`) ne dépend ni de GTK ni de WebKit : `cargo test -p last1-panel-core`
suffit pour travailler sur les réglages.

## Licence

GPL-3.0-or-later, voir [LICENSE](LICENSE). Space Grotesk : SIL Open Font
License ([static/OFL.txt](static/OFL.txt)).
