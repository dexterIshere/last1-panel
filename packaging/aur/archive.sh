#!/usr/bin/env bash
# Archive des binaires de last1-panel-bin (attachée à la release GitHub) : le
# dossier usr/ du paquet Arch construit par packaging/arch/build.sh, puis le
# PKGBUILD de l'AUR essayé sur cette archive avant toute publication. Sous un
# compte ordinaire (makepkg refuse root).
#   packaging/aur/archive.sh <paquet .pkg.tar.zst> <version> <dossier de sortie>
set -euo pipefail
package=${1:?paquet}
version=${2:?version}
out=${3:?dossier de sortie}
[[ $version =~ ^[0-9][0-9A-Za-z.]*$ ]] || { echo "version invalide : $version" >&2; exit 2; }
root=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
archive=$out/last1-panel-$version-x86_64.tar.zst
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

mkdir "$work/tree" "$work/check"
bsdtar -xf "$package" -C "$work/tree" usr
bsdtar --zstd --uid 0 --gid 0 --uname root --gname root -cf "$archive" -C "$work/tree" usr

# PKGBUILD de l'AUR sur l'archive locale : même paquet que celui de l'AUR.
# shellcheck disable=SC2016 # $_pkgname, $pkgver : variables du PKGBUILD
sed -e "s/^pkgver=.*/pkgver=$version/" -e 's|^source=.*|source=("$_pkgname-$pkgver-x86_64.tar.zst")|' \
  "$root/packaging/aur/PKGBUILD" >"$work/check/PKGBUILD"
cp "$archive" "$work/check/"
(cd "$work/check" && makepkg --noconfirm --nodeps >/dev/null)
for file in usr/bin/last1-panel usr/bin/last1-panel-cli usr/lib/systemd/user/last1-panel-apply.service \
  usr/share/licenses/last1-panel-bin/LICENSE usr/lib/last1-panel/nvibrant/nvibrant-; do
  bsdtar -tf "$work/check/last1-panel-bin-$version-1-x86_64.pkg.tar.zst" | grep -q "^$file" ||
    { echo "last1-panel-bin : $file absent" >&2; exit 1; }
done
echo "$archive"
