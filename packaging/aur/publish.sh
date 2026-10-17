#!/usr/bin/env bash
# Pousse last1-panel-bin <version> sur l'AUR, avec l'empreinte de l'archive de
# la release GitHub v<version>. Clé SSH du compte AUR dans AUR_SSH_PRIVATE_KEY.
# Sans effet si l'AUR a déjà ce PKGBUILD. Sous un compte ordinaire (makepkg
# refuse root pour écrire .SRCINFO).
#   packaging/aur/publish.sh <version> <sha256 de l'archive>
set -euo pipefail
version=${1:?version}
sha=${2:?empreinte}
[[ $version =~ ^[0-9][0-9A-Za-z.]*$ ]] || { echo "version invalide : $version" >&2; exit 2; }
[[ $sha =~ ^[0-9a-f]{64}$ ]] || { echo "empreinte invalide : $sha" >&2; exit 2; }
: "${AUR_SSH_PRIVATE_KEY:?clé SSH du compte AUR}"
root=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# Hôte de l'AUR vérifié : empreinte Ed25519 publiée sur aur.archlinux.org.
ssh-keyscan -t ed25519 aur.archlinux.org >"$work/known_hosts" 2>/dev/null
if [ "$(ssh-keygen -lf "$work/known_hosts" | awk '{print $2}')" != SHA256:RFzBCUItH9LZS0cKB5UE6ceAYhBD5C8GeOBip8Z11+4 ]; then
  echo "clé de l'hôte aur.archlinux.org inattendue" >&2
  exit 1
fi
install -m 600 /dev/null "$work/key"
printf '%s\n' "$AUR_SSH_PRIVATE_KEY" >"$work/key"
export GIT_SSH_COMMAND="ssh -i $work/key -o IdentitiesOnly=yes -o UserKnownHostsFile=$work/known_hosts -o StrictHostKeyChecking=yes"

# Dépôt du paquet (vide la première fois : le premier push crée le paquet).
git clone -q ssh://aur@aur.archlinux.org/last1-panel-bin.git "$work/aur"
sed -e "s/^pkgver=.*/pkgver=$version/" -e "s/^sha256sums=.*/sha256sums=('$sha')/" \
  "$root/packaging/aur/PKGBUILD" >"$work/aur/PKGBUILD"
cd "$work/aur"
makepkg --printsrcinfo >.SRCINFO
git add PKGBUILD .SRCINFO
if git diff --cached --quiet; then
  echo "AUR : last1-panel-bin $version déjà publié"
  exit 0
fi
git -c user.name=Last1 -c user.email=contact@last1.fun commit -q -m "Last1 Panel $version"
git push -q origin HEAD:master
echo "AUR : last1-panel-bin $version publié"
