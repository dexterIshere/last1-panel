#!/bin/sh
# Table des options de Hyprland et de leur type (core/src/display/hypr_options.rs),
# tirée de src/config/values/ConfigValues.cpp des sources de Hyprland :
#   tools/hypr-options.sh <dossier des sources de Hyprland> <commit>
set -eu
src=$1/src/config/values/ConfigValues.cpp
commit=$2
out=$(dirname "$0")/../core/src/display/hypr_options.rs
{
  echo "//! Options de Hyprland et leur type, pour écrire leurs valeurs en Lua"
  echo "//! (hypr_convert.rs). Généré par tools/hypr-options.sh depuis"
  echo "//! src/config/values/ConfigValues.cpp de Hyprland ($commit) : ne pas modifier"
  echo "//! à la main."
  echo
  echo "#[derive(Debug, Clone, Copy, PartialEq, Eq)]"
  echo "pub enum Kind {"
  echo "    Bool,"
  echo "    Int,"
  echo "    Float,"
  echo "    String,"
  echo "    Color,"
  echo "    Gradient,"
  echo "    CssGap,"
  echo "    Vec2,"
  echo "    FontWeight,"
  echo "}"
  echo
  echo "pub const OPTIONS: &[(&str, Kind)] = &["
  grep -o 'MS<[A-Za-z0-9]*>("[^"]*"' "$src" | sed 's/MS<\([A-Za-z0-9]*\)>("\(.*\)"/    ("\2", Kind::\1),/'
  echo "];"
} > "$out"
echo "$(grep -c 'Kind::' "$out") options -> $out"
