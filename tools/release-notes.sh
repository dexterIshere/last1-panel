#!/usr/bin/env bash
# Notes de mise à jour, depuis git : une ligne par commit de main (500 au
# plus, du plus récent au plus ancien), « <commit 7 car.>\t<fr>\t<en> », le
# commit seul s'il ne change rien pour les joueurs. Les notes sont les lignes
# « Joueurs: » et « Players: » de la fin des messages (avec
# Co-Authored-By) ; plusieurs par commit, séparées par \x1e. Même format que
# go-to-linux. Écrit sur la sortie : la construction le met dans
# release-notes.tsv, que core/build.rs embarque.
set -euo pipefail
git rev-list --first-parent -n 500 HEAD | while read -r commit; do
  # Lignes du message, pas seulement le dernier bloc : une ligne vide avant
  # Co-Authored-By les sortait des « trailers » de git (23 commits manqués).
  body=$(git log -1 --format=%B "$commit")
  fr=$(printf '%s\n' "$body" | sed -n 's/^Joueurs: *//p' | paste -sd $'\x1e' -)
  en=$(printf '%s\n' "$body" | sed -n 's/^Players: *//p' | paste -sd $'\x1e' -)
  if [ -n "$fr" ] || [ -n "$en" ]; then
    printf '%s\t%s\t%s\n' "${commit:0:7}" "$fr" "$en"
  else
    printf '%s\n' "${commit:0:7}"
  fi
done
