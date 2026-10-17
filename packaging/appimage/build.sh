#!/usr/bin/env bash
# Dans le conteneur de construction (Dockerfile à côté), sur les sources
# montées en lecture seule dans /src : AppImage de la version $VERSION (celle
# du paquet Arch, « 0.1.0.r42 »), écrite dans
# /out/Last1Panel_$VERSION_amd64.AppImage. Sur un PC :
#
#   docker build --tag last1-panel-appimage packaging/appimage
#   docker run --rm --env VERSION=0.1.0 --volume "$PWD:/src:ro" \
#     --volume "$PWD/dist:/out" last1-panel-appimage
#
# L'AppImage contient, aux mêmes endroits que le paquet Arch, l'outil des jeux
# (usr/bin/last1-panel-cli) et nvibrant (usr/lib/last1-panel/nvibrant), que
# Last1 Panel installe pour l'utilisateur à sa première ouverture
# (src-tauri/src/install.rs).
set -euo pipefail
: "${VERSION:?version manquante}"
[[ $VERSION =~ ^[0-9][0-9A-Za-z.]*$ ]] || { echo "version invalide : $VERSION" >&2; exit 2; }

# Copie de travail : les sources restent intactes, et ni node_modules ni les
# caches de l'hôte ne s'y mêlent.
rsync --archive --delete \
    --exclude node_modules --exclude /build --exclude .svelte-kit \
    --exclude /dist --exclude /target \
    /src/ /work/
cd /work
bun install --frozen-lockfile

# lcms2 compilé dans les exécutables : l'outil des jeux tourne hors de
# l'AppImage, sans dépendre des bibliothèques du système. Aucun chemin local
# (nom d'utilisateur) dans les exécutables.
export LCMS2_STATIC=1 RUSTFLAGS="--remap-path-prefix=/work=."
# Version de cette AppImage, pour sa mise à jour automatique (core/src/update.rs).
export LAST1_PANEL_RELEASE=$VERSION
cargo build --locked --release -p last1-panel-core --bin last1-panel-cli

# nvibrant (MIT) : mêmes binaires et empreintes que packaging/arch/PKGBUILD.
nvibrant=1.3.0
mkdir -p /tmp/nvibrant
curl -fsSL -o /tmp/nvibrant.whl \
    "https://files.pythonhosted.org/packages/5e/fc/453cf625d659ffaac14ef5a89edd5ac1d9e5ff3708c120efd4a3ac145b22/nvibrant-$nvibrant-py3-none-manylinux_2_17_x86_64.whl"
curl -fsSL -o /tmp/nvibrant-LICENSE "https://raw.githubusercontent.com/Tremeschin/nvibrant/v$nvibrant/license.txt"
sha256sum --check --quiet - <<SUMS
d60a080b52e4a16e16d16a7d4b6405a32a3fe1b8ee0df65dc3685213ef0393e8  /tmp/nvibrant.whl
015b42dfc301724c0f8ac8de7658e36801a7a86463d91024bf173ee9ceec495f  /tmp/nvibrant-LICENSE
SUMS
unzip -q -o /tmp/nvibrant.whl 'nvibrant/resources/nvibrant-*' -d /tmp/nvibrant

# Fichiers ajoutés à l'AppImage (bundle > linux > appimage > files).
files="\"/usr/bin/last1-panel-cli\": \"/work/target/release/last1-panel-cli\""
files+=", \"/usr/share/licenses/last1-panel/LICENSE\": \"/work/LICENSE\""
files+=", \"/usr/share/licenses/last1-panel/nvibrant-LICENSE\": \"/tmp/nvibrant-LICENSE\""
for binary in /tmp/nvibrant/nvibrant/resources/nvibrant-*; do
    files+=", \"/usr/lib/last1-panel/nvibrant/${binary##*/}\": \"$binary\""
done
cat >/tmp/appimage.json <<JSON
{"bundle": {"active": true, "targets": ["appimage"], "linux": {"appimage": {"files": {$files}}}}}
JSON
bun run tauri build --config /tmp/appimage.json

appimage=(target/release/bundle/appimage/*.AppImage)
out="/out/Last1Panel_${VERSION}_amd64.AppImage"
install -D --mode 0755 "${appimage[0]}" "$out"
chown "${HOST_UID:-0}:${HOST_GID:-0}" "$out"
echo "AppImage $VERSION : ${out##*/} ($(du -h "$out" | cut -f1)), SHA-256 $(sha256sum "$out" | cut -d' ' -f1)"
