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
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-$work/run}"
mkdir -p "$XDG_RUNTIME_DIR" && chmod 700 "$XDG_RUNTIME_DIR"
Hyprland --version | head -n 1

# Configurations : nom et hyprland.conf.
cp -r tests/hyprland/signal "$work/signal"
mkdir -p "$work/hypr056"
curl -fsSL https://raw.githubusercontent.com/hyprwm/Hyprland/v0.56.0/example/hyprland.conf -o "$work/hypr056/hyprland.conf" \
  || curl -fsSL https://raw.githubusercontent.com/hyprwm/Hyprland/v0.55.0/example/hyprland.conf -o "$work/hypr056/hyprland.conf"
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
  HOME="$HOME" "$cli" hypr-convert "$conf" "$out" 2> "$work/$name.skipped"
  echo "== $name : $(grep -c . "$work/$name.skipped") lignes non reprises"
  sed 's/^/   /' "$work/$name.skipped" | head -n 40
  result=$(HOME="$HOME" Hyprland --verify-config -c "$out" 2>&1)
  status=$?
  echo "$result" | sed -n '/Config parsing result/,$p'
  if [ $status -ne 0 ] || ! echo "$result" | grep -q "config ok"; then
    echo "::error::$name : Hyprland refuse la conversion"
    failed=1
  fi
done
exit $failed
