#!/usr/bin/env bash
# Icônes de Last1 Panel, depuis une seule source :
# src-tauri/icons/source/icon.svg (comme go-to-linux, tools/make-app-icons.sh).
#
#   tools/make-app-icons.sh          dessin (icon.svg, carré, formes seulement)
#                                    → les tailles de src-tauri/icons/ (fenêtre,
#                                    paquet, AppImage), core/assets/tray.png
#                                    (zone de notification) et source/icon.png
#                                    (1024 px, aperçu)
#   tools/make-app-icons.sh --check  vérifie que les icônes livrées sont bien
#                                    celles que donne icon.svg (rien n'est écrit)
#
# Tout passe par « tauri icon » (rendu SVG resvg), dont la version est fixée
# par bun.lock : le même dessin donne les mêmes octets. Chaque taille est
# rendue depuis le SVG, pas réduite depuis un grand PNG : le dessin, calé sur
# des cases de 64 unités, reste net en 16 et 32 px. Ses couleurs sont des
# jetons de src/app.css (src-tauri/tests/design.rs). Outils : bun, et
# « bun install » fait.
set -euo pipefail

root=$(git rev-parse --show-toplevel)
icons="$root/src-tauri/icons"
mode=${1:-make}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

tauri_icon() { (cd "$root" && bun x tauri icon "$@" >/dev/null 2>&1); }

# Tailles livrées : 32, 64 (zone de notification), 128, 256 (128@2x), 512
# (icon.png : fenêtre, menu des applications) et 1024 (aperçu).
render() {
  tauri_icon "$icons/source/icon.svg" -o "$work/out" --png 32,64,128,256,512,1024
  mkdir -p "$work/final/source"
  cp "$work/out/32x32.png" "$work/final/32x32.png"
  cp "$work/out/128x128.png" "$work/final/128x128.png"
  cp "$work/out/256x256.png" "$work/final/128x128@2x.png"
  cp "$work/out/512x512.png" "$work/final/icon.png"
  cp "$work/out/1024x1024.png" "$work/final/source/icon.png"
  cp "$work/out/64x64.png" "$work/final/tray.png"
}

# Carré, sans texte ni image externe (rendus avec les polices et fichiers de
# la machine, donc différents d'un PC à l'autre).
check_drawing() {
  python3 - "$icons/source/icon.svg" <<'PY'
import sys, xml.etree.ElementTree as ET
root = ET.parse(sys.argv[1]).getroot()
box = root.get("viewBox", "").replace(",", " ").split()
problems = []
if len(box) != 4 or box[2] != box[3]:
    problems.append("viewBox carré attendu (par exemple « 0 0 1024 1024 »)")
for element in root.iter():
    tag = element.tag.rsplit("}", 1)[-1]
    if tag in ("text", "tspan", "image", "foreignObject", "style", "script"):
        problems.append(f"<{tag}> refusé : formes seulement (texte en contours)")
    for name, value in element.attrib.items():
        if name.rsplit("}", 1)[-1] == "href" and not value.startswith("#"):
            problems.append(f"lien externe refusé : {value}")
for problem in problems:
    print(f"icon.svg : {problem}", file=sys.stderr)
sys.exit(1 if problems else 0)
PY
}

# Fichier rendu → fichier livré.
targets() {
  for name in 32x32.png 128x128.png 128x128@2x.png icon.png; do echo "$name $icons/$name"; done
  echo "source/icon.png $icons/source/icon.png"
  echo "tray.png $root/core/assets/tray.png"
}

case "$mode" in
  make)
    check_drawing
    render
    targets | while read -r name target; do cp "$work/final/$name" "$target"; done
    echo "icônes refaites depuis $icons/source/icon.svg"
    ;;
  --check)
    check_drawing
    render
    status=0
    while read -r name target; do
      if [ ! -f "$target" ] || ! cmp -s "$work/final/$name" "$target"; then
        echo "icône à refaire : ${target#"$root/"} (tools/make-app-icons.sh)" >&2
        status=1
      fi
    done < <(targets)
    [ "$status" -eq 0 ] && echo "icônes à jour (rendues depuis icon.svg)"
    exit "$status"
    ;;
  *) echo "usage : $0 [--check]" >&2; exit 2 ;;
esac
