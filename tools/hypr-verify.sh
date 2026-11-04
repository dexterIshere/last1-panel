#!/bin/sh
# Conversion des configurations Hyprland en Lua (last1-panel-cli hypr-convert),
# lue par le vrai Hyprland (--verify-config) : des configurations réelles,
# celle du bureau Signal de go-to-linux, l'exemple de Hyprland 0.56 et les
# dots de KooL. Échoue si Hyprland refuse une conversion.
#   tools/hypr-verify.sh <last1-panel-cli> <dossier de travail>
set -u
cli=$1
work=$2
mkdir -p "$work"
# Hyprland refuse de tourner en root : un compte à part si besoin.
if [ "$(id -u)" = 0 ]; then
  id hyprverify >/dev/null 2>&1 || useradd -m hyprverify
  run_as() { runuser -u hyprverify -- env XDG_RUNTIME_DIR="$work/run" "$@"; }
else
  run_as() { env XDG_RUNTIME_DIR="$work/run" "$@"; }
fi
mkdir -p "$work/run"
Hyprland --version | head -n 1

# Configurations : nom et hyprland.conf.
cp -r tests/hyprland/signal "$work/signal"
mkdir -p "$work/hypr056"
# Le dernier exemple en hyprlang : 0.54 (0.55 et 0.56 le donnent en Lua).
curl -fsSL https://raw.githubusercontent.com/hyprwm/Hyprland/v0.54.3/example/hyprland.conf -o "$work/hypr056/hyprland.conf"
# Dots de KooL (JaKooLit/Hyprland-Dots), dernière version en hyprlang.
git clone --quiet --depth 1 https://github.com/JaKooLit/Hyprland-Dots.git "$work/kool-src" \
  && cp -r "$work/kool-src/config/hypr" "$work/kool"

failed=0
for name in signal hypr056 kool; do
  conf="$work/$name/hyprland.conf"
  [ -f "$conf" ] || { echo "::warning::$name : pas de hyprland.conf"; continue; }
  out="$work/$name/hyprland.lua"
  # Les « source = ~/… » des dots pointent vers ~/.config/hypr.
  HOME="$work/home-$name" && mkdir -p "$HOME/.config" && ln -sfn "$work/$name" "$HOME/.config/hypr"
  [ "$(id -u)" = 0 ] && chown -R hyprverify "$work/run" && chmod 700 "$work/run" && chmod -R a+rwX "$work"
  # Comme sur le PC du joueur : conversion lue par Hyprland, touches
  # inexistantes retirées (last1-panel-cli hypr-convert --verify).
  run_as HOME="$HOME" "$cli" hypr-convert --verify "$conf" "$out" 2> "$work/$name.log"
  echo "== $name : $(grep -c '^non repris' "$work/$name.log") lignes non reprises, $(grep -c '^changé' "$work/$name.log") changements"
  sed 's/^/   /' "$work/$name.log" | head -n 40
  result=$(run_as HOME="$HOME" Hyprland --verify-config -c "$out" 2>&1)
  status=$?
  echo "$result" | tail -n 25 | sed 's/^/   | /'
  if [ $status -ne 0 ] || ! echo "$result" | grep -q "config ok"; then
    echo "::error::$name : Hyprland refuse la conversion"
    failed=1
  fi
done
exit $failed
