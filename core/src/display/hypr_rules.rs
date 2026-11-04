//! Règles de fenêtres, de couches et d'espaces de travail, et gestes, de
//! hyprland.conf en Lua (Hyprland 0.57+), d'après docs/hyprland-lua.md §3
//! et §4.4. Trois écritures des règles existent : celle de 0.53 à 0.56
//! (`match:class x, float on`, et en bloc), et celle d'avant
//! (`windowrulev2 = float, class:x`). Ce qui n'a pas d'équivalent sûr rend
//! `None`.

use super::hypr_lua::string as s;

#[derive(Clone, Copy, PartialEq)]
enum T {
    Bool,
    Int,
    Float,
    Str,
}

/// Effets des règles de fenêtres et leur type (§3.1).
fn window_effect(name: &str) -> Option<T> {
    const BOOL: &[&str] = &[
        "float",
        "tile",
        "fullscreen",
        "maximize",
        "center",
        "pseudo",
        "no_initial_focus",
        "pin",
        "persistent_size",
        "allows_input",
        "dim_around",
        "decorate",
        "focus_on_activate",
        "keep_aspect_ratio",
        "nearest_neighbor",
        "no_anim",
        "no_blur",
        "no_dim",
        "no_focus",
        "no_follow_mouse",
        "no_max_size",
        "no_shadow",
        "no_glow",
        "no_wobble",
        "no_shortcuts_inhibit",
        "opaque",
        "force_rgbx",
        "sync_fullscreen",
        "immediate",
        "xray",
        "render_unfocused",
        "no_screen_share",
        "no_vrr",
        "no_auto_hdr",
        "stay_focused",
        "confine_pointer",
        "no_xdg_drags",
    ];
    match name {
        n if BOOL.contains(&n) => Some(T::Bool),
        "rounding" | "border_size" | "no_close_for" => Some(T::Int),
        "scrolling_width" | "rounding_power" | "scroll_mouse" | "scroll_touchpad" => Some(T::Float),
        "move" | "size" | "min_size" | "max_size" | "fullscreen_state" | "monitor" | "workspace" | "group"
        | "suppress_event" | "content" | "animation" | "idle_inhibit" | "opacity" | "tag" | "tonemap"
        | "border_color" => Some(T::Str),
        _ => None,
    }
}

fn layer_effect(name: &str) -> Option<T> {
    match name {
        "no_anim" | "blur" | "blur_popups" | "dim_around" | "xray" | "no_screen_share" => Some(T::Bool),
        "ignore_alpha" => Some(T::Float),
        "order" | "above_lock" => Some(T::Int),
        "animation" => Some(T::Str),
        _ => None,
    }
}

fn truthy(value: &str) -> bool {
    let v = value.trim().to_ascii_lowercase();
    v == "1" || v.starts_with("true") || v.starts_with("yes") || v.starts_with("on")
}

fn typed(t: T, value: &str) -> Option<String> {
    let v = value.trim();
    match t {
        T::Bool => Some(if v.is_empty() { "true".into() } else { truthy(v).to_string() }),
        T::Int => v.parse::<i64>().ok().map(|n| n.to_string()),
        T::Float => v.parse::<f64>().ok().filter(|n| n.is_finite()).map(|_| v.to_string()),
        T::Str => Some(s(v)),
    }
}

/// Champs de correspondance : regex, booléens, entiers (§3.1).
fn match_value(key: &str, value: &str) -> Option<String> {
    match key {
        "class" | "title" | "initial_class" | "initial_title" | "content" | "xdg_tag" | "namespace" | "tag"
        | "workspace" => Some(s(value.trim())),
        "float" | "xwayland" | "fullscreen" | "pin" | "focus" | "group" | "modal" => Some(truthy(value).to_string()),
        "fullscreen_state_internal" | "fullscreen_state_client" => {
            value.trim().parse::<i64>().ok().map(|n| n.to_string())
        }
        _ => None,
    }
}

/// Une règle Lua à partir de ses correspondances et effets déjà écrits.
fn rule(function: &str, name: Option<&str>, enabled: Option<bool>, matches: &[String], effects: &[String]) -> String {
    let mut fields = Vec::new();
    if let Some(n) = name {
        fields.push(format!("name = {}", s(n)));
    }
    if let Some(e) = enabled {
        fields.push(format!("enabled = {e}"));
    }
    fields.push(format!("match = {{ {} }}", matches.join(", ")));
    fields.extend(effects.iter().cloned());
    format!("{function}({{ {} }})", fields.join(", "))
}

/// Écriture 0.53-0.56 : `match:P V` et `effet V`, en ligne ou en bloc.
fn modern(
    function: &str,
    effect_type: fn(&str) -> Option<T>,
    fields: &[(String, String)],
    name: Option<&str>,
    enabled: Option<bool>,
) -> Option<String> {
    let (mut matches, mut effects) = (Vec::new(), Vec::new());
    for (key, value) in fields {
        if let Some(prop) = key.strip_prefix("match:") {
            matches.push(format!("{prop} = {}", match_value(prop, value)?));
        } else {
            effects.push(format!("{key} = {}", typed(effect_type(key)?, value)?));
        }
    }
    (!matches.is_empty()).then(|| rule(function, name, enabled, &matches, &effects))
}

/// Champs « clé valeur » d'une règle en ligne.
fn inline_fields(value: &str) -> Vec<(String, String)> {
    value
        .split(',')
        .map(str::trim)
        .filter(|f| !f.is_empty())
        .map(|f| match f.split_once(char::is_whitespace) {
            Some((k, v)) => (k.trim().to_string(), v.trim().to_string()),
            None => (f.to_string(), String::new()),
        })
        .collect()
}

/// `N%` d'une valeur de position ou taille d'avant 0.53 → expression.
fn percent_expression(value: &str) -> String {
    let tokens: Vec<&str> = value.split_whitespace().collect();
    if tokens.len() != 2 {
        return value.to_string();
    }
    let axis = |t: &str, dim: &str| match t.strip_suffix('%').and_then(|p| p.parse::<f64>().ok()) {
        Some(p) => format!("{dim}*{}", p / 100.0),
        None => t.to_string(),
    };
    format!("{} {}", axis(tokens[0], "monitor_w"), axis(tokens[1], "monitor_h"))
}

/// Nom d'effet d'avant 0.53 → actuel (§3.3) ; `noborder`/`norounding`
/// deviennent des valeurs.
fn old_window_effect(name: &str, value: &str) -> Option<(String, String)> {
    let v = value.trim();
    let new = match name {
        "float" | "tile" | "fullscreen" | "maximize" | "pin" | "pseudo" | "center" | "opaque" | "immediate"
        | "decorate" | "xray" => name,
        "noinitialfocus" => "no_initial_focus",
        "stayfocused" => "stay_focused",
        "renderunfocused" => "render_unfocused",
        "persistentsize" => "persistent_size",
        "animation" | "rounding" | "content" | "group" | "monitor" | "move" | "size" | "opacity" | "tag"
        | "workspace" => name,
        "bordercolor" => "border_color",
        "bordersize" => "border_size",
        "roundingpower" => "rounding_power",
        "fullscreenstate" => "fullscreen_state",
        "idleinhibit" => "idle_inhibit",
        "maxsize" => "max_size",
        "minsize" => "min_size",
        "noclosefor" => "no_close_for",
        "scrollmouse" => "scroll_mouse",
        "scrolltouchpad" => "scroll_touchpad",
        "suppressevent" => "suppress_event",
        "novrr" => "no_vrr",
        "allowsinput" => "allows_input",
        "dimaround" => "dim_around",
        "focusonactivate" => "focus_on_activate",
        "keepaspectratio" => "keep_aspect_ratio",
        "nearestneighbor" => "nearest_neighbor",
        "noanim" => "no_anim",
        "noblur" => "no_blur",
        "nodim" => "no_dim",
        "nofocus" => "no_focus",
        "nofollowmouse" => "no_follow_mouse",
        "nomaxsize" => "no_max_size",
        "noshadow" => "no_shadow",
        "noshortcutsinhibit" => "no_shortcuts_inhibit",
        "forcergbx" => "force_rgbx",
        "syncfullscreen" => "sync_fullscreen",
        "noscreenshare" => "no_screen_share",
        "noborder" => return Some(("border_size".into(), "0".into())),
        "norounding" => return Some(("rounding".into(), "0".into())),
        _ => return None,
    };
    let v =
        if matches!(new, "move" | "size" | "max_size" | "min_size") { percent_expression(v) } else { v.to_string() };
    Some((new.to_string(), v))
}

fn old_window_match(key: &str, value: &str) -> Option<String> {
    let bool_of = |v: &str| (v.trim() == "1").to_string();
    Some(match key {
        "class" | "title" | "tag" | "content" => format!("{key} = {}", s(value.trim())),
        "initialClass" | "initialclass" => format!("initial_class = {}", s(value.trim())),
        "initialTitle" | "initialtitle" => format!("initial_title = {}", s(value.trim())),
        "xdgTag" | "xdgtag" => format!("xdg_tag = {}", s(value.trim())),
        "xwayland" => format!("xwayland = {}", bool_of(value)),
        "floating" => format!("float = {}", bool_of(value)),
        "fullscreen" | "focus" | "group" | "modal" => format!("{key} = {}", bool_of(value)),
        "pinned" => format!("pin = {}", bool_of(value)),
        "fullscreenstate" => {
            let mut parts = value.split_whitespace();
            let (i, c) = (parts.next()?, parts.next()?);
            let mut out = Vec::new();
            if i != "*" {
                out.push(format!("fullscreen_state_internal = {}", i.parse::<i64>().ok()?));
            }
            if c != "*" {
                out.push(format!("fullscreen_state_client = {}", c.parse::<i64>().ok()?));
            }
            out.join(", ")
        }
        "workspace" | "onworkspace" => format!("workspace = {}", s(value.trim())),
        _ => return None,
    })
}

/// Écriture d'avant 0.53 : `windowrulev2 = effet [valeur], clé:valeur, …`
/// (et la toute première, `windowrule = effet, regex de classe`).
fn old_window_rule(value: &str) -> Option<String> {
    let fields: Vec<&str> = value.split(',').map(str::trim).filter(|f| !f.is_empty()).collect();
    let is_param = |f: &str| f.contains(':') && !f.starts_with("workspace ") && !f.starts_with("monitor ");
    let (mut matches, mut effects) = (Vec::new(), Vec::new());
    let params: Vec<&str> = fields.iter().copied().filter(|f| is_param(f)).collect();
    if params.is_empty() && fields.len() == 2 {
        matches.push(format!("class = {}", s(fields[1])));
    }
    for field in &fields {
        if is_param(field) {
            let (k, v) = field.split_once(':')?;
            matches.push(old_window_match(k.trim(), v)?);
        } else if !(params.is_empty() && fields.len() == 2 && field == &fields[1]) {
            let (name, val) = field.split_once(char::is_whitespace).unwrap_or((field, ""));
            let (name, val) = old_window_effect(name, val)?;
            effects.push(format!("{name} = {}", typed(window_effect(&name)?, &val)?));
        }
    }
    (!matches.is_empty() && !effects.is_empty()).then(|| rule("hl.window_rule", None, None, &matches, &effects))
}

/// `windowrule = …` (ou `windowrulev2`) en ligne.
pub fn window_rule(value: &str) -> Option<String> {
    if value.contains("match:") {
        modern("hl.window_rule", window_effect, &inline_fields(value), None, None)
    } else {
        old_window_rule(value)
    }
}

/// `layerrule = …` en ligne.
pub fn layer_rule(value: &str) -> Option<String> {
    if value.contains("match:") {
        return modern("hl.layer_rule", layer_effect, &inline_fields(value), None, None);
    }
    // Avant 0.53 : `layerrule = règle [valeur], regex de l'espace de nom`.
    let (effect, namespace) = value.rsplit_once(',')?;
    let (name, val) = effect.trim().split_once(char::is_whitespace).unwrap_or((effect.trim(), ""));
    let field = match name {
        "noanim" => "no_anim = true".to_string(),
        "blur" => "blur = true".to_string(),
        "blurpopups" => "blur_popups = true".to_string(),
        "dimaround" => "dim_around = true".to_string(),
        "noscreenshare" => "no_screen_share = true".to_string(),
        "ignorezero" => "ignore_alpha = 0".to_string(),
        "ignorealpha" => format!("ignore_alpha = {}", if val.is_empty() { "0".into() } else { typed(T::Float, val)? }),
        "xray" => format!("xray = {}", truthy(val)),
        "animation" => format!("animation = {}", s(val)),
        "order" => format!("order = {}", typed(T::Int, val)?),
        _ => return None,
    };
    Some(rule("hl.layer_rule", None, None, &[format!("namespace = {}", s(namespace.trim()))], &[field]))
}

/// Bloc `windowrule { … }` ou `layerrule { … }` (0.53-0.56).
pub fn rule_block(kind: &str, fields: &[(String, String)]) -> Option<String> {
    let name = fields.iter().find(|(k, _)| k == "name").map(|(_, v)| v.as_str());
    let enabled = fields.iter().find(|(k, _)| k == "enable").map(|(_, v)| v.trim() != "0");
    let rest: Vec<(String, String)> = fields.iter().filter(|(k, _)| k != "name" && k != "enable").cloned().collect();
    match kind {
        "windowrule" => modern("hl.window_rule", window_effect, &rest, name, enabled),
        "layerrule" => modern("hl.layer_rule", layer_effect, &rest, name, enabled),
        _ => None,
    }
}

/// `workspace = ID, clé:valeur, …` (§3.5).
pub fn workspace_rule(value: &str) -> Option<String> {
    let mut parts = value.split(',').map(str::trim);
    let id = parts.next().filter(|i| !i.is_empty())?;
    let mut fields = vec![format!("workspace = {}", s(id))];
    let int_bool = |v: &str| truthy(v) || v.trim().parse::<i64>().is_ok_and(|n| n != 0);
    for part in parts.filter(|p| !p.is_empty()) {
        let (k, v) = part.split_once(':')?;
        let v = v.trim();
        fields.push(match k.trim() {
            "monitor" => format!("monitor = {}", s(v)),
            "default" => format!("default = {}", int_bool(v)),
            "persistent" => format!("persistent = {}", int_bool(v)),
            "decorate" => format!("decorate = {}", int_bool(v)),
            "border" => format!("no_border = {}", !int_bool(v)),
            "shadow" => format!("no_shadow = {}", !int_bool(v)),
            "rounding" => format!("no_rounding = {}", !int_bool(v)),
            "bordersize" => format!("border_size = {}", v.parse::<i64>().ok()?),
            "gapsin" => format!("gaps_in = {}", super::hypr_convert::value(super::hypr_options::Kind::CssGap, v)?),
            "gapsout" => format!("gaps_out = {}", super::hypr_convert::value(super::hypr_options::Kind::CssGap, v)?),
            "defaultName" => format!("default_name = {}", s(v)),
            "on-created-empty" => format!("on_created_empty = {}", s(v)),
            "layout" => format!("layout = {}", s(v)),
            "animation" => format!("animation = {}", s(v)),
            "layoutopt" => {
                let (ok, ov) = v.split_once(':')?;
                format!("layout_opts = {{ {} = {} }}", ok.trim(), s(ov.trim()))
            }
            _ => return None,
        });
    }
    Some(format!("hl.workspace_rule({{ {} }})", fields.join(", ")))
}

/// `gesture[p] = doigts, direction, [mod:X,] [scale:S,] action[, args]`.
pub fn gesture(flags: &str, value: &str) -> Option<String> {
    let parts: Vec<&str> = value.split(',').map(str::trim).collect();
    let fingers: u32 = parts.first()?.parse().ok()?;
    let mut fields = vec![format!("fingers = {fingers}"), format!("direction = {}", s(parts.get(1)?))];
    if flags.contains('p') {
        fields.push("disable_inhibit = true".into());
    }
    let mut i = 2;
    while let Some(p) = parts.get(i) {
        if let Some(m) = p.strip_prefix("mod:") {
            fields.push(format!("mods = {}", s(m.trim())));
        } else if let Some(sc) = p.strip_prefix("scale:") {
            fields.push(format!("scale = {}", typed(T::Float, sc)?));
        } else {
            break;
        }
        i += 1;
    }
    let action = parts.get(i)?.to_string();
    let rest: Vec<&str> = parts[i + 1..].to_vec();
    match action.as_str() {
        "workspace" | "resize" | "move" | "close" | "unset" => fields.push(format!("action = {}", s(&action))),
        "scrollMove" => fields.push("action = \"scroll_move\"".into()),
        "special" => {
            fields.push("action = \"special\"".into());
            fields.push(format!("workspace_name = {}", s(rest.first()?)));
        }
        "float" | "fullscreen" => {
            fields.push(format!("action = {}", s(&action)));
            if let Some(mode) = rest.first().filter(|m| !m.is_empty()) {
                fields.push(format!("mode = {}", s(mode)));
            }
        }
        "cursorZoom" => {
            fields.push("action = \"cursor_zoom\"".into());
            fields.push(format!("zoom_level = {}", s(rest.first()?)));
            if let Some(mode) = rest.get(1) {
                fields.push(format!("mode = {}", s(mode)));
            }
        }
        "dispatcher" => {
            let dsp = super::hypr_dispatch::dispatcher(rest.first()?, &rest[1..].join(","))?;
            fields.push(format!("action = function() hl.dispatch({dsp}) end"));
        }
        _ => return None,
    }
    Some(format!("hl.gesture({{ {} }})", fields.join(", ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_in_their_three_spellings() {
        assert_eq!(
            window_rule("match:class ^(kitty)$, float on, size 800 600").unwrap(),
            r#"hl.window_rule({ match = { class = "^(kitty)$" }, float = true, size = "800 600" })"#
        );
        assert_eq!(
            window_rule("float, class:^(pavucontrol)$, title:Volume").unwrap(),
            r#"hl.window_rule({ match = { class = "^(pavucontrol)$", title = "Volume" }, float = true })"#
        );
        assert_eq!(
            window_rule("size 50% 60%, class:^(x)$").unwrap(),
            r#"hl.window_rule({ match = { class = "^(x)$" }, size = "monitor_w*0.5 monitor_h*0.6" })"#
        );
        assert_eq!(
            window_rule("noborder, floating:1").unwrap(),
            r#"hl.window_rule({ match = { float = true }, border_size = 0 })"#
        );
        assert_eq!(
            window_rule("float, ^(kitty)$").unwrap(),
            r#"hl.window_rule({ match = { class = "^(kitty)$" }, float = true })"#
        );
        assert!(window_rule("unknowneffect, class:x").is_none());
        let block = [
            ("name".to_string(), "steam".to_string()),
            ("match:class".to_string(), "^(steam)$".to_string()),
            ("workspace".to_string(), "5 silent".to_string()),
            ("immediate".to_string(), "yes".to_string()),
        ];
        assert_eq!(
            rule_block("windowrule", &block).unwrap(),
            r#"hl.window_rule({ name = "steam", match = { class = "^(steam)$" }, workspace = "5 silent", immediate = true })"#
        );
        assert_eq!(
            layer_rule("blur, waybar").unwrap(),
            r#"hl.layer_rule({ match = { namespace = "waybar" }, blur = true })"#
        );
        assert_eq!(
            layer_rule("match:namespace ^(rofi)$, blur on, ignore_alpha 0.5").unwrap(),
            r#"hl.layer_rule({ match = { namespace = "^(rofi)$" }, blur = true, ignore_alpha = 0.5 })"#
        );
        assert_eq!(
            workspace_rule("1, monitor:DP-1, default:true, gapsout:10").unwrap(),
            r#"hl.workspace_rule({ workspace = "1", monitor = "DP-1", default = true, gaps_out = 10 })"#
        );
        assert_eq!(
            gesture("", "3, horizontal, workspace").unwrap(),
            r#"hl.gesture({ fingers = 3, direction = "horizontal", action = "workspace" })"#
        );
    }
}
