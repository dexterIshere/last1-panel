#!/usr/bin/env bash
# Notes de mise à jour, depuis git : une ligne par commit de main (500 au
# plus, du plus récent au plus ancien), « <commit 7 car.>\t<fr>\t<en> », le
# commit seul s'il ne change rien pour les joueurs. Les notes sont les lignes
# « Joueurs: » et « Players: » du bloc de fin des messages (comme
# Co-Authored-By) ; plusieurs par commit, séparées par \x1e. Même format que
# go-to-linux. Écrit sur la sortie : la construction le met dans
# release-notes.tsv, que core/build.rs embarque.
set -euo pipefail
git rev-list --first-parent -n 500 HEAD | while read -r commit; do
  fr=$(git log -1 --format='%(trailers:key=Joueurs,valueonly,separator=%x1e)' "$commit" | tr -d '\n')
  en=$(git log -1 --format='%(trailers:key=Players,valueonly,separator=%x1e)' "$commit" | tr -d '\n')
  if [ -n "$fr" ] || [ -n "$en" ]; then
    printf '%s\t%s\t%s\n' "${commit:0:7}" "$fr" "$en"
  else
    printf '%s\n' "${commit:0:7}"
  fi
done
