-- Bureau « Signal » de go-to-linux pour Hyprland 0.57 et plus (configuration
-- en Lua ; les versions d'avant lisent hyprland.conf, son équivalent).
-- Chargé par ~/.config/hypr/hyprland.lua ; remplacé à chaque mise à jour de
-- go-to-linux : vos propres réglages vont dans hyprland.lua, après lui.
-- Palette : docs/DA.md.
--   fond #0A0C10 · surface #12151B · contour #262B35 · texte #ECEFF4
--   Pulse #9B8CFF (focus, information) · Volt #C5F82A (actif, rare)
-- Raccourcis repris de Windows ; liste complète : Super + F1.

local gtl = "/usr/local/lib/gtl/desktop"
local terminal = "alacritty"
local files = "dolphin"
local config = os.getenv("XDG_CONFIG_HOME")
if not config or config == "" then config = (os.getenv("HOME") or "") .. "/.config" end
local hypr = config .. "/hypr"

-- Une partie en erreur n'empêche pas les autres (une erreur Lua arrête le
-- reste du fichier) : chacune est faite à part, son erreur affichée.
local function part(name, fn)
    local ok, err = pcall(fn)
    if not ok then
        print("go-to-linux, " .. name .. " : " .. tostring(err))
    end
end

local function trim(s)
    return (s:gsub("^%s+", ""):gsub("%s+$", ""))
end

local function split(s)
    local fields = {}
    for field in (s .. ","):gmatch("([^,]*),") do
        fields[#fields + 1] = trim(field)
    end
    return fields
end

local function value(s)
    if s == "true" or s == "on" or s == "yes" then return true end
    if s == "false" or s == "off" or s == "no" then return false end
    return tonumber(s) or s
end

-- Écran au format de hyprland.conf (« nom, mode, position, échelle[, clé,
-- valeur…] », celui de nwg-displays et de go-to-linux) → hl.monitor.
local monitorNumbers = { transform = true, bitdepth = true, vrr = true, sdrbrightness = true, sdrsaturation = true }
local monitorStrings = { mirror = true, cm = true, sdr_eotf = true }

local function monitor(line)
    local f = split(line)
    if #f < 2 then return end
    if f[2] == "disable" or f[2] == "disabled" then
        hl.monitor({ output = f[1], disabled = true })
        return
    end
    local rule = { output = f[1], mode = f[2], position = f[3] or "auto", scale = f[4] or "auto" }
    local i = 5
    while f[i] and f[i + 1] do
        local key = f[i]
        if monitorNumbers[key] then
            rule[key] = tonumber(f[i + 1])
        elseif monitorStrings[key] then
            rule[key] = f[i + 1]
        end
        i = i + 2
    end
    hl.monitor(rule)
end

-- Fichiers au format de hyprland.conf écrits par go-to-linux (gtl-machine.conf :
-- clavier, Verr. Num, carte graphique) et par nwg-displays (monitors.conf) :
-- écrans, variables d'environnement et sections (input { kb_layout = fr }).
local function hyprlang(path)
    local file = io.open(path, "r")
    if not file then return end
    local sections, settings = {}, {}
    for raw in file:lines() do
        local line = trim((raw:gsub("#.*$", "")))
        local name = line:match("^([%w_%-%.]+)%s*{$")
        if name then
            sections[#sections + 1] = name
        elseif line == "}" then
            sections[#sections] = nil
        else
            local key, v = line:match("^([%w_%-%.]+)%s*=%s*(.-)$")
            if key and #sections == 0 and key == "monitor" then
                monitor(v)
            elseif key and #sections == 0 and key == "env" then
                local k, val = v:match("^([^,]+),%s*(.*)$")
                if k then hl.env(trim(k), val) end
            elseif key and #sections > 0 then
                local t = settings
                for _, s in ipairs(sections) do
                    t[s] = t[s] or {}
                    t = t[s]
                end
                t[key] = value(v)
            end
        end
    end
    file:close()
    if next(settings) then hl.config(settings) end
end

-- Écrans : réglés à chaque connexion (définition native, fréquence maximale,
-- gtl-ecrans) tant que monitors.conf garde la ligne « # gtl:auto » ;
-- nwg-displays le remplace.
part("écrans", function()
    hl.monitor({ output = "", mode = "preferred", position = "auto", scale = "auto" })
    hyprlang(hypr .. "/monitors.conf")
end)
part("clavier", function() hyprlang(hypr .. "/gtl-machine.conf") end)

part("environnement", function()
    hl.env("XCURSOR_THEME", "breeze_cursors")
    hl.env("XCURSOR_SIZE", "24")
    hl.env("QT_QPA_PLATFORMTHEME", "kde")
    hl.env("QT_QPA_PLATFORM", "wayland;xcb")
    hl.env("GDK_BACKEND", "wayland,x11,*")
end)

hl.on("hyprland.start", function()
    for _, cmd in ipairs({
        "dbus-update-activation-environment --systemd WAYLAND_DISPLAY XDG_CURRENT_DESKTOP HYPRLAND_INSTANCE_SIGNATURE QT_QPA_PLATFORMTHEME",
        "hyprctl setcursor breeze_cursors 24",
        gtl .. "/gtl-ecrans",
        "swaybg -m fill -c 0a0c10 -i /usr/local/share/gtl/desktop/signal.png",
        gtl .. "/gtl-barre",
        "mako",
        "hypridle",
        "/usr/lib/polkit-kde-authentication-agent-1",
        "nm-applet --indicator",
        "blueman-applet",
        "wl-paste --watch cliphist store",
        gtl .. "/gtl-premiere-fois",
    }) do
        hl.exec_cmd(cmd)
    end
end)

part("apparence", function()
    hl.config({
        general = {
            gaps_in = 4,
            gaps_out = 12,
            border_size = 2,
            col = {
                active_border = "rgb(9b8cff)",
                inactive_border = "rgb(262b35)",
            },
            layout = "dwindle",
            resize_on_border = true,
        },
        decoration = {
            rounding = 10,
            shadow = { enabled = false },
            blur = { enabled = false },
        },
        animations = { enabled = true },
        dwindle = { preserve_split = true },
        misc = {
            disable_hyprland_logo = true,
            disable_splash_rendering = true,
            force_default_wallpaper = 0,
            background_color = "rgb(0a0c10)",
            -- Fréquence variable (FreeSync, G-Sync) en plein écran seulement : jeux.
            vrr = 2,
            middle_click_paste = false,
            mouse_move_enables_dpms = true,
            key_press_enables_dpms = true,
        },
        input = {
            follow_mouse = 1,
            touchpad = {
                ["tap-to-click"] = true,
                natural_scroll = false,
            },
        },
    })
end)

-- Mouvements courts et doux (150 ms), sans rebond.
part("animations", function()
    hl.curve("signal", { type = "bezier", points = { { 0.2, 0.8 }, { 0.2, 1.0 } } })
    hl.animation({ leaf = "windows", enabled = true, speed = 1.5, bezier = "signal", style = "popin 96%" })
    hl.animation({ leaf = "windowsOut", enabled = true, speed = 1.5, bezier = "signal", style = "popin 96%" })
    hl.animation({ leaf = "border", enabled = true, speed = 1.5, bezier = "signal" })
    hl.animation({ leaf = "fade", enabled = true, speed = 1.5, bezier = "signal" })
    hl.animation({ leaf = "layers", enabled = true, speed = 1.5, bezier = "signal", style = "fade" })
    hl.animation({ leaf = "workspaces", enabled = true, speed = 2, bezier = "signal", style = "slidefade 8%" })
end)

-- --- Raccourcis --------------------------------------------------------------

local function run(cmd) return hl.dsp.exec_cmd(cmd) end
local menu = "pkill -x fuzzel || fuzzel"

part("raccourcis", function()
    -- Touche Windows seule : applications (comme le menu Démarrer).
    hl.bind("SUPER + SUPER_L", run(menu), { release = true })
    hl.bind("SUPER + S", run(menu))
    hl.bind("SUPER + R", run(menu))

    hl.bind("SUPER + Return", run(terminal))
    hl.bind("SUPER + E", run(files))
    hl.bind("SUPER + I", run("systemsettings"))
    -- Écrans (comme « Projeter » de Windows) : la page Écrans des Paramètres de
    -- KDE ne marche que sous Plasma.
    hl.bind("SUPER + P", run("nwg-displays"))
    hl.bind("SUPER + L", run("loginctl lock-session"))
    hl.bind("SUPER + V", run(gtl .. "/gtl-presse-papiers"))
    hl.bind("SUPER + N", run("makoctl restore"))
    hl.bind("SUPER + SHIFT + N", run("makoctl dismiss --all"))
    hl.bind("SUPER + X", run(gtl .. "/gtl-alimentation"))
    hl.bind("CTRL + ALT + Delete", run(gtl .. "/gtl-alimentation"))
    hl.bind("CTRL + SHIFT + Escape", run(terminal .. " --class gtl-btop -e btop"))
    hl.bind("SUPER + F1", run(gtl .. "/gtl-raccourcis"))
    -- Capture : glisser pour une zone, cliquer sur une fenêtre ou sur l'écran.
    hl.bind("Print", run(gtl .. "/gtl-capture"))
    hl.bind("SUPER + SHIFT + S", run(gtl .. "/gtl-capture"))
end)

part("fenêtres", function()
    hl.bind("ALT + F4", hl.dsp.window.close())
    hl.bind("SUPER + Q", hl.dsp.window.close())
    hl.bind("SUPER + F", hl.dsp.window.fullscreen({ mode = "fullscreen" }))
    hl.bind("SUPER + Up", hl.dsp.window.fullscreen({ mode = "maximized" }))
    hl.bind("SUPER + Down", hl.dsp.window.float({ action = "toggle" }))
    hl.bind("SUPER + Left", hl.dsp.window.move({ direction = "left" }))
    hl.bind("SUPER + Right", hl.dsp.window.move({ direction = "right" }))
    hl.bind("SUPER + SHIFT + Up", hl.dsp.window.move({ direction = "up" }))
    hl.bind("SUPER + SHIFT + Down", hl.dsp.window.move({ direction = "down" }))
    hl.bind("ALT + Tab", function()
        hl.dispatch(hl.dsp.window.cycle_next())
        hl.dispatch(hl.dsp.window.bring_to_top())
    end)
    hl.bind("ALT + SHIFT + Tab", function()
        hl.dispatch(hl.dsp.window.cycle_next({ next = false }))
        hl.dispatch(hl.dsp.window.bring_to_top())
    end)
    hl.bind("SUPER + mouse:272", hl.dsp.window.drag(), { mouse = true })
    hl.bind("SUPER + mouse:273", hl.dsp.window.resize(), { mouse = true })
end)

-- Bureaux (comme les bureaux virtuels de Windows : Ctrl + Windows + flèches).
-- Windows + chiffre : la touche du chiffre, quelle que soit la disposition (en
-- AZERTY, elle donne « & », « é »…) ; le bureau vient sur l'écran en cours.
part("bureaux", function()
    hl.bind("SUPER + CTRL + Left", hl.dsp.focus({ workspace = "r-1" }))
    hl.bind("SUPER + CTRL + Right", hl.dsp.focus({ workspace = "r+1" }))
    hl.bind("SUPER + CTRL + D", hl.dsp.focus({ workspace = "empty" }))
    for n = 1, 9 do
        local key = "code:" .. (n + 9)
        hl.bind("SUPER + " .. key, hl.dsp.focus({ workspace = tostring(n), on_current_monitor = true }))
        hl.bind("SUPER + SHIFT + " .. key, hl.dsp.window.move({ workspace = tostring(n) }))
    end
end)

-- Son, luminosité et lecture.
part("son", function()
    local held = { locked = true, repeating = true }
    local locked = { locked = true }
    hl.bind("XF86AudioRaiseVolume", run("wpctl set-volume -l 1 @DEFAULT_AUDIO_SINK@ 5%+"), held)
    hl.bind("XF86AudioLowerVolume", run("wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%-"), held)
    hl.bind("XF86AudioMute", run("wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle"), locked)
    hl.bind("XF86AudioMicMute", run("wpctl set-mute @DEFAULT_AUDIO_SOURCE@ toggle"), locked)
    hl.bind("XF86MonBrightnessUp", run("brightnessctl -e4 -n2 set 5%+"), held)
    hl.bind("XF86MonBrightnessDown", run("brightnessctl -e4 -n2 set 5%-"), held)
    hl.bind("XF86AudioPlay", run("playerctl play-pause"), locked)
    hl.bind("XF86AudioPause", run("playerctl play-pause"), locked)
    hl.bind("XF86AudioNext", run("playerctl next"), locked)
    hl.bind("XF86AudioPrev", run("playerctl previous"), locked)
end)

-- --- Fenêtres particulières ----------------------------------------------------

part("règles des fenêtres", function()
    -- Réglages et petites fenêtres : flottantes, au centre.
    hl.window_rule({
        name = "gtl-reglages",
        match = { class = "^(org.pulseaudio.pavucontrol|blueman-manager|nm-connection-editor|nwg-displays|gtl-btop)$" },
        float = true,
        center = true,
    })
    hl.window_rule({ name = "gtl-btop", match = { class = "^(gtl-btop)$" }, size = "960 640" })
    hl.window_rule({ name = "gtl-qml", match = { class = "^(org.qt-project.qml6?|qml6?)$" }, float = true, center = true })
    hl.window_rule({
        name = "gtl-steam",
        match = { title = "^(Liste d'amis|Friends List|Steam - News|Steam Settings|Paramètres Steam)$" },
        float = true,
    })
    -- Écran jamais éteint ni verrouillé pendant un jeu ou une vidéo en plein écran.
    hl.window_rule({ name = "gtl-plein-ecran", match = { class = ".*" }, idle_inhibit = "fullscreen" })
end)
