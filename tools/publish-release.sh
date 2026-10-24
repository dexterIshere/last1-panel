#!/usr/bin/env bash
# Publie une version d'un produit sur Last1.fun, depuis une pipeline GitHub
# Actions (permission « id-token: write »). Aucun secret : le site vérifie le
# jeton OIDC signé par GitHub (dépôt et branche du produit, voir web/oidc.go).
#
#   tools/publish-release.sh <produit> <version> download:<fichier>... content:<fichier>...
#
#   download : installeur ou paquet, téléchargé depuis « Mon compte » ;
#   content  : contenu protégé, téléchargé par l'application avec sa licence.
#
# Hors GitHub Actions (CI locale, tools/ci-local.sh) : variable PUBLISH_TOKEN, jeton
# de publication dont le site ne connaît que l'empreinte (PUBLISH_TOKEN_SHA256).
#
# Variable SITE_URL : adresse du site (https://last1.fun par défaut). Au-delà de
# PUBLISH_CHUNK_MIB Mio (90), un fichier est envoyé par morceaux : le proxy de
# Cloudflare refuse les envois de plus de 100 Mo. Le site redémarre à chaque
# mise à jour (30 à 90 s sans réponse : 502, 503) : chaque envoi est retenté
# 6 fois, à 15 s d'écart (la publication d'un même fichier est idempotente).
# La copie de ce script dans les autres dépôts doit rester identique.
set -euo pipefail

product=${1:?produit manquant}
version=${2:?version manquante}
shift 2
[ $# -gt 0 ] || { echo "aucun fichier à publier" >&2; exit 2; }
site=${SITE_URL:-https://last1.fun}
site=${site%/}

if [ -z "${PUBLISH_TOKEN:-}" ] && [ -z "${ACTIONS_ID_TOKEN_REQUEST_URL:-}" ]; then
  echo "::error::Jeton OIDC indisponible : ajoutez « permissions: id-token: write » au job (ou PUBLISH_TOKEN hors GitHub Actions)." >&2
  exit 1
fi
# Un jeton neuf par requête : ils ne vivent que quelques minutes.
oidc() {
  if [ -n "${PUBLISH_TOKEN:-}" ]; then
    printf '%s' "$PUBLISH_TOKEN"
    return
  fi
  local token
  token=$(curl -fsS --retry 3 -H "Authorization: bearer $ACTIONS_ID_TOKEN_REQUEST_TOKEN" \
    "$ACTIONS_ID_TOKEN_REQUEST_URL&audience=$(jq -rn --arg a "$site" '$a|@uri')" | jq -r .value)
  echo "::add-mask::$token" >&2
  printf '%s' "$token"
}

# Commit publié, noté avec la version (le jeton OIDC le porte déjà).
commit=${GITHUB_SHA:-$(git rev-parse HEAD 2>/dev/null || true)}
chunk_mib=${PUBLISH_CHUNK_MIB:-90}
chunk=$((chunk_mib * 1024 * 1024))
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# put <url> <fichier> <empreinte> : envoie un fichier ou un morceau, renvoie le
# code HTTP (réponse dans $work/reponse).
put() {
  curl -sS --http1.1 --retry 6 --retry-delay 15 --retry-all-errors --max-time 900 -o "$work/reponse" -w '%{http_code}' -X PUT \
    -H "Authorization: Bearer $(oidc)" -H "X-Commit-SHA: $commit" -H "X-Content-SHA256: $3" -H 'Content-Type: application/octet-stream' \
    --data-binary "@$2" "$1"
}

for entry in "$@"; do
  kind=${entry%%:*}
  file=${entry#*:}
  name=$(basename "$file")
  size=$(stat -c %s "$file")
  sum=$(sha256sum "$file" | cut -d' ' -f1)
  url="$site/api/publish/$product/$version/files/$name?kind=$kind"
  echo "envoi de $name ($kind, $(du -h "$file" | cut -f1), sha256 $sum)"
  if [ "$size" -le "$chunk" ]; then
    code=$(put "$url" "$file" "$sum")
  else
    offset=0
    while [ "$offset" -lt "$size" ]; do
      dd if="$file" of="$work/morceau" bs=1M iflag=skip_bytes,count_bytes skip="$offset" count="$chunk" status=none
      code=$(put "$url&offset=$offset&total=$size" "$work/morceau" "$sum")
      case $code in
        201 | 202) offset=$((offset + $(stat -c %s "$work/morceau"))) ;;
        409) offset=$(jq -r .received "$work/reponse") ;; # morceau déjà reçu (réponse perdue) : reprise
        *) break ;;
      esac
      echo "  $((offset * 100 / size)) %"
    done
  fi
  cat "$work/reponse"
  echo
  [ "$code" = 201 ] || { echo "::error::$name refusé par $site (HTTP $code)" >&2; exit 1; }
done

curl --fail-with-body -sS --retry 6 --retry-delay 15 --retry-all-errors --max-time 60 -X POST \
  -H "Authorization: Bearer $(oidc)" -H "X-Commit-SHA: $commit" "$site/api/publish/$product/$version/release" | jq .
echo "$product $version en ligne sur $site"
