-- Bureau « Signal » de go-to-linux, pour Hyprland 0.57 et plus.
-- Les réglages de go-to-linux (raccourcis, barre, clavier, écrans), mis à jour
-- avec lui, sont lus d'abord : ajoutez les vôtres en dessous, ils l'emportent.
-- Ce fichier est le vôtre : il n'est jamais remplacé.
local ok, err = pcall(dofile, "/usr/local/share/gtl/desktop/hypr/signal.lua")
if not ok then print("go-to-linux : " .. tostring(err)) end
