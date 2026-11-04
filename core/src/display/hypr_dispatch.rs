//! Raccourcis et dispatchers de hyprland.conf en Lua (Hyprland 0.57+),
//! d'après docs/hyprland-lua.md (§1 raccourcis, §2 dispatchers). Ce qui n'a
//! pas d'équivalent sûr rend `None` : la ligne reste non reprise.

use super::hypr_lua::string as s;

/// Sélecteur de fenêtre : préfixe obligatoire en Lua (une regex nue était
/// une classe en hyprlang).
fn window(sel: &str) -> String {
    let sel = sel.trim();
    let known = [
        "class:",
        "initialclass:",
        "title:",
        "initialtitle:",
        "tag:",
        "stableid:",
        "address:",
        "pid:",
        "active",
        "floating",
        "tiled",
    ];
    if known.iter().any(|p| sel.starts_with(p)) { s(sel) } else { s(&format!("class:{sel}")) }
}

/// Direction hyprlang (première lettre) → Lua.
fn direction(arg: &str) -> Option<&'static str> {
    match arg.trim().chars().next()?.to_ascii_lowercase() {
        'l' => Some("left"),
        'r' => Some("right"),
        'u' | 't' => Some("up"),
        'd' | 'b' => Some("down"),
        _ => None,
    }
}

fn num(text: &str) -> Option<String> {
    let t = text.trim();
    t.parse::<f64>().ok().filter(|n| n.is_finite())?;
    Some(t.trim_start_matches('+').to_string())
}

/// `x y` (entiers ou décimaux, sans %) pour resize/move.
fn xy(args: &str) -> Option<(String, String)> {
    let mut parts = args.split_whitespace();
    let (x, y) = (parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    Some((num(x)?, num(y)?))
}

/// Toggle/on/off des dispatchers à action.
fn action(arg: &str, empty: &str) -> &'static str {
    match arg.trim() {
        "" => match empty {
            "on" => "on",
            "off" => "off",
            _ => "toggle",
        },
        "lock" | "on" | "enable" => "on",
        "unlock" | "off" | "disable" => "off",
        _ => "toggle",
    }
}

/// Un dispatcher hyprlang (`nom`, `arguments`) en expression Lua.
pub fn dispatcher(name: &str, args: &str) -> Option<String> {
    let name = name.trim().to_ascii_lowercase();
    let args = args.trim();
    let opt_window =
        |sel: &str| if sel.trim().is_empty() { String::new() } else { format!(", window = {}", window(sel)) };
    Some(match name.as_str() {
        "exec" => format!("hl.dsp.exec_cmd({})", s(args)),
        "execr" => format!("hl.dsp.exec_raw({})", s(args)),
        "pass" => format!("hl.dsp.pass({{ window = {} }})", window(args)),
        "killactive" => "hl.dsp.window.close()".into(),
        "forcekillactive" => "hl.dsp.window.kill()".into(),
        "closewindow" => format!("hl.dsp.window.close({{ window = {} }})", window(args)),
        "killwindow" => format!("hl.dsp.window.kill({{ window = {} }})", window(args)),
        "signal" => format!("hl.dsp.window.signal({{ signal = {} }})", num(args)?),
        "signalwindow" => {
            let (sel, sig) = args.rsplit_once(',')?;
            format!("hl.dsp.window.signal({{ signal = {}, window = {} }})", num(sig)?, window(sel))
        }
        "togglefloating" | "setfloating" | "settiled" => {
            let act = match name.as_str() {
                "togglefloating" => "toggle",
                "setfloating" => "on",
                _ => "off",
            };
            format!("hl.dsp.window.float({{ action = \"{act}\"{} }})", opt_window(args))
        }
        "pseudo" if args.is_empty() => "hl.dsp.window.pseudo()".into(),
        "pseudo" => format!("hl.dsp.window.pseudo({{ window = {} }})", window(args)),
        "pin" if args.is_empty() => "hl.dsp.window.pin()".into(),
        "pin" => format!("hl.dsp.window.pin({{ window = {} }})", window(args)),
        "fullscreen" => {
            let mut parts = args.split_whitespace();
            let mode = if parts.next() == Some("1") { "maximized" } else { "fullscreen" };
            let act = match parts.next() {
                Some("set") => "set",
                Some("unset") => "unset",
                _ => "toggle",
            };
            format!("hl.dsp.window.fullscreen({{ mode = \"{mode}\", action = \"{act}\" }})")
        }
        "fullscreenstate" => {
            let mut parts = args.split_whitespace();
            let (i, c) = (parts.next()?, parts.next()?);
            if i.starts_with('-') || c.starts_with('-') {
                return None; // « -1 » (garder l'état) : pas d'équivalent.
            }
            format!("hl.dsp.window.fullscreen_state({{ internal = {}, client = {} }})", num(i)?, num(c)?)
        }
        "workspace" => format!("hl.dsp.focus({{ workspace = {} }})", s(args)),
        "movetoworkspace" | "movetoworkspacesilent" => {
            let (ws, sel) = match args.rsplit_once(',') {
                Some((ws, sel)) => (ws, sel),
                None => (args, ""),
            };
            let follow = if name == "movetoworkspacesilent" { ", follow = false" } else { "" };
            format!("hl.dsp.window.move({{ workspace = {}{follow}{} }})", s(ws.trim()), opt_window(sel))
        }
        "movefocus" => format!("hl.dsp.focus({{ direction = \"{}\" }})", direction(args)?),
        "movewindow" => {
            if let Some(mon) = args.strip_prefix("mon:") {
                let (mon, silent) = match mon.trim().strip_suffix(" silent") {
                    Some(m) => (m, ", follow = false"),
                    None => (mon.trim(), ""),
                };
                format!("hl.dsp.window.move({{ monitor = {}{silent} }})", s(mon))
            } else if args.is_empty() {
                // bindm sans argument : déplacer à la souris.
                "hl.dsp.window.drag()".into()
            } else {
                format!("hl.dsp.window.move({{ direction = \"{}\" }})", direction(args)?)
            }
        }
        "resizewindow" => match args {
            "" | "0" => "hl.dsp.window.resize()".into(),
            "1" => "hl.dsp.window.resize({ keep_aspect_ratio = true })".into(),
            "2" => "hl.dsp.window.resize({ keep_aspect_ratio = false })".into(),
            _ => return None,
        },
        "swapwindow" => match direction(args).filter(|_| args.len() == 1) {
            Some(dir) => format!("hl.dsp.window.swap({{ direction = \"{dir}\" }})"),
            None => format!("hl.dsp.window.swap({{ target = {} }})", window(args)),
        },
        "centerwindow" => "hl.dsp.window.center()".into(),
        "resizeactive" | "moveactive" => {
            let verb = if name == "resizeactive" { "resize" } else { "move" };
            match args.strip_prefix("exact") {
                Some(rest) => {
                    let (x, y) = xy(rest)?;
                    format!("hl.dsp.window.{verb}({{ x = {x}, y = {y} }})")
                }
                None => {
                    let (x, y) = xy(args)?;
                    format!("hl.dsp.window.{verb}({{ x = {x}, y = {y}, relative = true }})")
                }
            }
        }
        "resizewindowpixel" | "movewindowpixel" => {
            let verb = if name == "resizewindowpixel" { "resize" } else { "move" };
            let (values, sel) = args.split_once(',')?;
            let w = window(sel);
            match values.trim().strip_prefix("exact") {
                Some(rest) => {
                    let (x, y) = xy(rest)?;
                    format!("hl.dsp.window.{verb}({{ x = {x}, y = {y}, window = {w} }})")
                }
                None => {
                    let (x, y) = xy(values)?;
                    format!("hl.dsp.window.{verb}({{ x = {x}, y = {y}, relative = true, window = {w} }})")
                }
            }
        }
        "cyclenext" => {
            let words: Vec<&str> = args.split_whitespace().collect();
            let prev = words.iter().any(|w| ["prev", "p", "last", "l"].contains(w));
            let mut fields = vec![format!("next = {}", !prev)];
            if words.iter().any(|w| ["tiled", "tile"].contains(w)) {
                fields.push("tiled = true".into());
            }
            if words.iter().any(|w| ["floating", "float"].contains(w)) {
                fields.push("floating = true".into());
            }
            format!("hl.dsp.window.cycle_next({{ {} }})", fields.join(", "))
        }
        "swapnext" => {
            if ["prev", "l", "last", "b", "back"].contains(&args) {
                "hl.dsp.window.swap({ prev = true })".into()
            } else {
                "hl.dsp.window.swap({ next = true })".into()
            }
        }
        "focuswindow" => format!("hl.dsp.focus({{ window = {} }})", window(args)),
        "focuswindowbyclass" => format!("hl.dsp.focus({{ window = {} }})", s(&format!("class:{args}"))),
        "focusmonitor" => format!("hl.dsp.focus({{ monitor = {} }})", s(args)),
        "focusurgentorlast" => "hl.dsp.focus({ urgent_or_last = true })".into(),
        "focuscurrentorlast" => "hl.dsp.focus({ last = true })".into(),
        "focusworkspaceoncurrentmonitor" => {
            format!("hl.dsp.focus({{ workspace = {}, on_current_monitor = true }})", s(args))
        }
        "tagwindow" => {
            let mut parts = args.splitn(2, char::is_whitespace);
            let tag = parts.next()?;
            format!("hl.dsp.window.tag({{ tag = {}{} }})", s(tag), opt_window(parts.next().unwrap_or("")))
        }
        "toggleswallow" => "hl.dsp.window.toggle_swallow()".into(),
        "submap" => format!("hl.dsp.submap({})", s(args)),
        "movecursortocorner" => format!("hl.dsp.cursor.move_to_corner({{ corner = {} }})", num(args)?),
        "movecursor" => {
            let (x, y) = xy(args)?;
            format!("hl.dsp.cursor.move({{ x = {x}, y = {y} }})")
        }
        "renameworkspace" => {
            let mut parts = args.splitn(2, char::is_whitespace);
            let id = parts.next()?;
            match parts.next().map(str::trim).filter(|n| !n.is_empty()) {
                Some(n) => format!("hl.dsp.workspace.rename({{ workspace = {}, name = {} }})", s(id), s(n)),
                None => format!("hl.dsp.workspace.rename({{ workspace = {} }})", s(id)),
            }
        }
        "exit" => "hl.dsp.exit()".into(),
        "forcerendererreload" => "hl.dsp.force_renderer_reload()".into(),
        "movecurrentworkspacetomonitor" => format!("hl.dsp.workspace.move({{ monitor = {} }})", s(args)),
        "moveworkspacetomonitor" => {
            let mut parts = args.split_whitespace();
            let (ws, mon) = (parts.next()?, parts.next()?);
            format!("hl.dsp.workspace.move({{ workspace = {}, monitor = {} }})", s(ws), s(mon))
        }
        "swapactiveworkspaces" => {
            let mut parts = args.split_whitespace();
            let (a, b) = (parts.next()?, parts.next()?);
            format!("hl.dsp.workspace.swap_monitors({{ monitor1 = {}, monitor2 = {} }})", s(a), s(b))
        }
        "togglespecialworkspace" if args.is_empty() => "hl.dsp.workspace.toggle_special()".into(),
        "togglespecialworkspace" => format!("hl.dsp.workspace.toggle_special({})", s(args)),
        "togglegroup" => "hl.dsp.group.toggle()".into(),
        "changegroupactive" => match args {
            "" | "f" | "forward" => "hl.dsp.group.next()".into(),
            "b" | "prev" => "hl.dsp.group.prev()".into(),
            n => format!("hl.dsp.group.active({{ index = {} }})", num(n)?),
        },
        "movegroupwindow" => {
            let forward = !matches!(args, "b" | "prev");
            format!("hl.dsp.group.move_window({{ forward = {forward} }})")
        }
        "lockgroups" => format!("hl.dsp.group.lock({{ all = true, action = \"{}\" }})", action(args, "on")),
        "lockactivegroup" => format!("hl.dsp.group.lock({{ action = \"{}\" }})", action(args, "off")),
        "moveintogroup" => format!("hl.dsp.window.move({{ into_group = {} }})", s(&direction(args)?[..1])),
        "moveintoorcreategroup" => {
            format!("hl.dsp.window.move({{ into_or_create_group = {} }})", s(&direction(args)?[..1]))
        }
        "moveoutofgroup" if args.is_empty() || args == "active" => "hl.dsp.window.move({ out_of_group = true })".into(),
        "moveoutofgroup" => format!("hl.dsp.window.move({{ out_of_group = true, window = {} }})", window(args)),
        "movewindoworgroup" => {
            format!("hl.dsp.window.move({{ direction = \"{}\", group_aware = true }})", direction(args)?)
        }
        "denywindowfromgroup" => {
            format!("hl.dsp.window.deny_from_group({{ action = \"{}\" }})", action(args, "off"))
        }
        "layoutmsg" => format!("hl.dsp.layout({})", s(args)),
        "togglesplit" => "hl.dsp.layout(\"togglesplit\")".into(),
        "swapsplit" => "hl.dsp.layout(\"swapsplit\")".into(),
        "splitratio" => match args.strip_prefix("exact") {
            Some(v) => format!("hl.dsp.layout({})", s(&format!("splitratio {} exact", v.trim()))),
            None => format!("hl.dsp.layout({})", s(&format!("splitratio {args}"))),
        },
        "alterzorder" => {
            let (mode, sel) = args.split_once(',').unwrap_or((args, ""));
            format!("hl.dsp.window.alter_zorder({{ mode = {}{} }})", s(mode.trim()), opt_window(sel))
        }
        "bringactivetotop" => "hl.dsp.window.bring_to_top()".into(),
        "global" => format!("hl.dsp.global({})", s(args)),
        "event" => format!("hl.dsp.event({})", s(args)),
        "setprop" => {
            let mut parts = args.splitn(3, char::is_whitespace);
            let (sel, prop, value) = (parts.next()?, parts.next()?, parts.next()?);
            format!(
                "hl.dsp.window.set_prop({{ prop = {}, value = {}, window = {} }})",
                s(prop),
                s(value.trim()),
                window(sel)
            )
        }
        "dpms" => {
            let mut parts = args.split_whitespace();
            let act = match parts.next() {
                None | Some("off") => "off",
                Some("on") => "on",
                Some("toggle") => "toggle",
                Some(_) => return None,
            };
            match parts.next() {
                Some(mon) => format!("hl.dsp.dpms({{ action = \"{act}\", monitor = {} }})", s(mon)),
                None => format!("hl.dsp.dpms({{ action = \"{act}\" }})"),
            }
        }
        "forceidle" => format!("hl.dsp.force_idle({})", num(args)?),
        "releaseinputcapture" => "hl.dsp.release_input_capture()".into(),
        "sendshortcut" => {
            let parts: Vec<&str> = args.splitn(3, ',').map(str::trim).collect();
            let (mods, key) = (parts.first()?, parts.get(1)?);
            let w =
                parts.get(2).filter(|w| !w.is_empty()).map(|w| format!(", window = {}", window(w))).unwrap_or_default();
            format!("hl.dsp.send_shortcut({{ mods = {}, key = {}{w} }})", s(mods), s(key))
        }
        _ => return None,
    })
}

/// Modificateurs d'un raccourci, comme hyprlang les reconnaît (recherche
/// dans le texte : « SUPER_SHIFT », « $mainMod SHIFT »), dans un ordre
/// unique (unbind compare le texte).
pub fn mods(text: &str) -> Option<Vec<&'static str>> {
    let up = text.to_ascii_uppercase();
    if up.trim().is_empty() {
        return Some(Vec::new());
    }
    let has = |names: &[&str]| names.iter().any(|n| up.contains(n));
    let mut out = Vec::new();
    for (names, canon) in [
        (&["SUPER", "WIN", "LOGO", "MOD4", "META"][..], "SUPER"),
        (&["CTRL", "CONTROL"][..], "CTRL"),
        (&["ALT", "MOD1"][..], "ALT"),
        (&["SHIFT"][..], "SHIFT"),
        (&["CAPS"][..], "CAPS"),
        (&["MOD2"][..], "MOD2"),
        (&["MOD3"][..], "MOD3"),
        (&["MOD5"][..], "MOD5"),
    ] {
        if has(names) {
            out.push(canon);
        }
    }
    (!out.is_empty()).then_some(out)
}

/// Touche : un nombre au-delà de 9 est un code de touche.
pub fn key(text: &str) -> String {
    let t = text.trim();
    match t.parse::<u32>() {
        Ok(n) if n > 9 => format!("code:{n}"),
        _ => t.to_string(),
    }
}

/// « SUPER + SHIFT + Q ».
pub fn keys(mods_text: &str, key_text: &str) -> Option<String> {
    let mut parts: Vec<String> = mods(mods_text)?.into_iter().map(str::to_string).collect();
    let k = key(key_text);
    if k.is_empty() {
        return None;
    }
    parts.push(k);
    Some(parts.join(" + "))
}

/// Un `bind<lettres> = …` en `hl.bind(…)`.
pub fn bind(flags: &str, value: &str) -> Option<String> {
    if flags.contains('s') {
        return None; // plusieurs touches liées par « & » : équivalent pas vérifié.
    }
    let device = flags.contains('k');
    let described = flags.contains('d');
    let fixed = 2 + usize::from(device) + usize::from(described);
    let parts: Vec<&str> = value.splitn(fixed + 2, ',').map(str::trim).collect();
    let (mods_text, key_text) = (parts.first()?, parts.get(1)?);
    let mut opts: Vec<String> = Vec::new();
    let mut i = 2;
    if device {
        let list = parts.get(i)?;
        let (inclusive, list) = match list.strip_prefix('!') {
            Some(rest) => (false, rest),
            None => (true, *list),
        };
        let names: Vec<String> = list.split_whitespace().map(s).collect();
        opts.push(format!("device = {{ inclusive = {inclusive}, list = {{ {} }} }}", names.join(", ")));
        i += 1;
    }
    if described {
        opts.push(format!("description = {}", s(parts.get(i)?)));
        i += 1;
    }
    let disp_name = parts.get(i)?;
    let args = parts.get(i + 1).copied().unwrap_or("");
    let mouse = flags.contains('m');
    let dsp = if mouse {
        match disp_name.to_ascii_lowercase().as_str() {
            "movewindow" => "hl.dsp.window.drag()".to_string(),
            "resizewindow" => dispatcher("resizewindow", args)?,
            _ => return None,
        }
    } else {
        dispatcher(disp_name, args)?
    };
    for (letter, field) in [
        ('l', "locked"),
        ('r', "release"),
        ('e', "repeating"),
        ('m', "mouse"),
        ('n', "non_consuming"),
        ('a', "auto_consuming"),
        ('t', "transparent"),
        ('i', "ignore_mods"),
        ('o', "long_press"),
        ('p', "dont_inhibit"),
        ('c', "click"),
        ('g', "drag"),
        ('u', "submap_universal"),
        ('x', "allow_input_capture"),
    ] {
        if flags.contains(letter) {
            opts.push(format!("{field} = true"));
        }
    }
    let k = keys(mods_text, key_text)?;
    Some(if opts.is_empty() {
        format!("hl.bind({}, {dsp})", s(&k))
    } else {
        format!("hl.bind({}, {dsp}, {{ {} }})", s(&k), opts.join(", "))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binds_like_the_signal_desktop() {
        assert_eq!(
            bind("", "SUPER, Return, exec, alacritty").unwrap(),
            r#"hl.bind("SUPER + Return", hl.dsp.exec_cmd("alacritty"))"#
        );
        assert_eq!(bind("", "ALT, F4, killactive,").unwrap(), r#"hl.bind("ALT + F4", hl.dsp.window.close())"#);
        assert_eq!(
            bind("", "SUPER, Up, fullscreen, 1").unwrap(),
            r#"hl.bind("SUPER + Up", hl.dsp.window.fullscreen({ mode = "maximized", action = "toggle" }))"#
        );
        assert_eq!(
            bind("", "SUPER SHIFT, Up, movewindow, u").unwrap(),
            r#"hl.bind("SUPER + SHIFT + Up", hl.dsp.window.move({ direction = "up" }))"#
        );
        assert_eq!(
            bind("m", "SUPER, mouse:272, movewindow").unwrap(),
            r#"hl.bind("SUPER + mouse:272", hl.dsp.window.drag(), { mouse = true })"#
        );
        assert_eq!(
            bind("", "SUPER, code:10, focusworkspaceoncurrentmonitor, 1").unwrap(),
            r#"hl.bind("SUPER + code:10", hl.dsp.focus({ workspace = "1", on_current_monitor = true }))"#
        );
        assert_eq!(
            bind("el", ", XF86AudioRaiseVolume, exec, wpctl set-volume -l 1 @DEFAULT_AUDIO_SINK@ 5%+").unwrap(),
            r#"hl.bind("XF86AudioRaiseVolume", hl.dsp.exec_cmd("wpctl set-volume -l 1 @DEFAULT_AUDIO_SINK@ 5%+"), { locked = true, repeating = true })"#
        );
        assert_eq!(
            bind("", "ALT SHIFT, Tab, cyclenext, prev").unwrap(),
            r#"hl.bind("ALT + SHIFT + Tab", hl.dsp.window.cycle_next({ next = false }))"#
        );
        assert_eq!(
            bind("d", "SUPER_SHIFT, 24, Quitter, exec, a, b").unwrap(),
            r#"hl.bind("SUPER + SHIFT + code:24", hl.dsp.exec_cmd("a, b"), { description = "Quitter" })"#
        );
        assert_eq!(
            bind("", "SUPER, R, resizeactive, 10 -10").unwrap(),
            r#"hl.bind("SUPER + R", hl.dsp.window.resize({ x = 10, y = -10, relative = true }))"#
        );
        assert!(bind("", "SUPER, R, resizeactive, 10% 0").is_none());
        assert!(bind("", "SUPER, R, nosuchdispatcher, x").is_none());
        assert!(bind("", "HYPER, R, exec, x").is_none(), "modificateur inconnu");
        assert_eq!(
            dispatcher("movetoworkspacesilent", "special:magic").unwrap(),
            r#"hl.dsp.window.move({ workspace = "special:magic", follow = false })"#
        );
        assert_eq!(dispatcher("dpms", "").unwrap(), r#"hl.dsp.dpms({ action = "off" })"#);
        assert_eq!(dispatcher("focuswindow", "firefox").unwrap(), r#"hl.dsp.focus({ window = "class:firefox" })"#);
    }
}
