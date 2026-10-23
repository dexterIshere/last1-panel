#!/usr/bin/env bash
# Construit le paquet Arch de Last1 Panel depuis le commit en cours.
#   packaging/arch/build.sh <version> <dossier de sortie>
# Version sans tiret (pacman) : « 0.1.0.r42 ». À lancer sous un compte
# ordinaire (makepkg refuse root), sur Arch ou CachyOS.
set -euo pipefail
version=${1:?version}
out=${2:?dossier de sortie}
[[ $version =~ ^[0-9][0-9A-Za-z.]*$ ]] || { echo "version invalide : $version" >&2; exit 2; }
root=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
mkdir -p "$out"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cp "$root/packaging/arch/PKGBUILD" "$work/"
sed -i "s/^pkgver=.*/pkgver=$version/" "$work/PKGBUILD"
# Notes de mise à jour (tools/release-notes.sh), posées par la CI à la racine :
# ajoutées aux sources, hors git.
notes=()
[ -f "$root/release-notes.tsv" ] && notes=(--add-file="$root/release-notes.tsv")
git -C "$root" archive --format=tar.gz --prefix="last1-panel-$version/" "${notes[@]}" -o "$work/last1-panel-$version.tar.gz" HEAD
(cd "$work" && PKGDEST="$(realpath "$out")" makepkg --noconfirm --syncdeps --cleanbuild)
ls "$out"/last1-panel-*.pkg.tar.zst
