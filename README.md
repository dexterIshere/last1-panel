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
| Vibrance numérique | **Couleurs** (cartes NVIDIA) | [nvibrant](https://github.com/Tremeschin/nvibrant) |
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

## Construire

Il faut Rust (1.90 ou plus récent), [Bun](https://bun.sh) et les bibliothèques
de Tauri (`webkit2gtk-4.1`, `gtk3`) et `lcms2`.

```sh
bun install
bun run tauri build      # fenêtre : target/release/last1-panel
cargo test --workspace   # tests
```

Le cœur (`core/`) ne dépend ni de GTK ni de WebKit : `cargo test -p last1-panel-core`
suffit pour travailler sur les réglages.

## Licence

GPL-3.0-or-later, voir [LICENSE](LICENSE). Space Grotesk : SIL Open Font
License ([static/OFL.txt](static/OFL.txt)).
