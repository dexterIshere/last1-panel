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

Chaque réglage passe par un outil existant et maintenu : Last1 Panel les
réunit dans une seule fenêtre, en français et en anglais, et ne réécrit pas ce
qu'ils font déjà. Le détail des choix est dans [docs/DESIGN.md](docs/DESIGN.md).

**Impossible aujourd'hui sous Wayland avec une carte NVIDIA**, quel que soit
l'outil : choisir RVB ou YCbCr, plage complète ou limitée, l'overscan.

## Bureaux pris en charge

- **Hyprland** (bureau « Signal » de go-to-linux) ;
- **KDE Plasma 6** en session Wayland.

## Installer

go-to-linux l'installe d'office sous CachyOS. Sinon, sur Arch ou CachyOS :

```sh
paru -S last1-panel-bin   # AUR, binaires déjà compilés
```

ou le paquet de la [dernière release](https://github.com/dexterIshere/last1-panel/releases/latest) :
`sudo pacman -U last1-panel-*.pkg.tar.zst`. LACT et les bibliothèques viennent
des dépôts officiels ; nvibrant est inclus. Page : [last1.fun/last1-panel](https://last1.fun/last1-panel).

## Réglages 3D des jeux

Chaque jeu Steam réglé dans Last1 Panel est lancé par
`last1-panel-cli run %command%` (option de lancement mise par Last1 Panel,
Steam fermé). Le détail des variables posées est dans
[docs/DESIGN.md](docs/DESIGN.md#réglages-3d--ce-que-pose-le-lanceur).

## Construire

Il faut Rust (1.90 ou plus récent), [Bun](https://bun.sh) et les bibliothèques
de Tauri (`webkit2gtk-4.1`, `gtk3`) et `lcms2`.

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
`packaging/arch/`). À chaque commit sur `main`, la CI construit le paquet dans
un conteneur Arch. Pour publier une version, changez `version` dans
`Cargo.toml` : la CI crée la release GitHub `v<version>` (paquet et archive
des binaires, `packaging/aur/archive.sh`), puis pousse `last1-panel-bin` sur
l'AUR (`packaging/aur/publish.sh`, clé SSH du compte AUR dans le secret
`AUR_SSH_PRIVATE_KEY` ; sans lui, l'étape est sautée avec un avertissement).
Une fois la clé ajoutée, relancez la CI à la main (« Run workflow ») pour
publier la version déjà sortie.

Le cœur (`core/`) ne dépend ni de GTK ni de WebKit : `cargo test -p last1-panel-core`
suffit pour travailler sur les réglages.

## Licence

GPL-3.0-or-later, voir [LICENSE](LICENSE). Space Grotesk : SIL Open Font
License ([static/OFL.txt](static/OFL.txt)).
