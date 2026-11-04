# Hyprland : de hyprland.conf (≤ 0.56) à Lua (0.57+) — table de conversion

Référence du convertisseur (core/src/display/hypr_convert.rs), relevée dans les sources de Hyprland (fichier:ligne). En anglais, comme les sources citées.

Sources read (all claims quoted from these trees):

- **M** = hyprwm/Hyprland `main` @ a8a2c24 (2026-11-03). `src/config/lua/bindings/*.cpp`, `LuaBindingsInternal.*` and `src/config/values/ConfigValues.cpp` are **byte-identical to tag v0.57.0** (diffed), so everything below holds for 0.57.0.
- **L** = tag v0.56.2 (last release that reads `.conf`). 0.55/0.56 already route every legacy dispatcher through `src/config/legacy/DispatcherTranslator.cpp` (one translator → `Config::Actions::*`, the same action layer Lua uses). The legacy keyword parser is `src/config/legacy/ConfigManager.cpp`.
- **L52** = v0.52.2 (pre-0.53 `windowrule`/`windowrulev2` effect names), **L54** = v0.54.3 (dispatcher list before `togglesplit`/`swapsplit`/`splitratio` were removed).

Conventions: `file:line`. "UNVERIFIED" = not confirmed from source.

---

## 0. Error model (it shapes how the converter should emit code)

| Situation | Effect | Source |
|---|---|---|
| Lua syntax error in the main file | `luaL_loadfile` fails, the error is recorded, **nothing** from the file is applied (on reload the old state is kept: phase 1 runs before any state is cleared) | M `config/lua/ConfigManager.cpp:657-672` |
| Runtime Lua error (`error()`, calling nil, …) in the main chunk | **Aborts the rest of the file**. Everything before it stays applied. The error and a traceback are added and verification fails | M `ConfigManager.cpp:711-722` |
| Runtime error inside a `require`d module | Caught by the safe `require`: error added, `{}` returned, **the caller continues** | M `ConfigManager.cpp:315-380` |
| `require` of a missing module | Re-raised as a real Lua error, so the rest of the caller is aborted unless you wrap it in `pcall` | M `ConfigManager.cpp:343-347` |
| Any `hl.*` validation error (bad field, unknown key, bad dispatcher args, unknown keysym…) | **Not a Lua error.** `Internal::configError` logs it, adds it to the error list (level ERROR → `addError`) and returns 0 values. Execution continues | M `LuaBindingsInternal.cpp:356-365, 493-498` |
| `hl.config` / `hl.window_rule` / `hl.layer_rule` / `hl.workspace_rule` / `hl.device` / `hl.monitor`: bad or unknown field | `self->addError(...)`. The bad field is skipped and the other fields still apply | M `LuaBindingsConfigRules.cpp:943-992, 1141-1249…` |
| Dispatcher factory called with bad args (e.g. `hl.dsp.focus({})`) | Records an error and returns **nothing**, so the enclosing `hl.bind` then also errors ("expected a dispatcher…") | M `LuaBindingsDispatcherUtils.cpp:61-78, 109-128` |
| Errors and zero binds registered | "Emergency mode": built-in SUPER+Q / SUPER+R / SUPER+M binds | M `ConfigManager.cpp:744-756` |
| Whole config evaluation timeout | 1500 ms (`LUA_TIMEOUT_CONFIG_RELOAD_MS`). Event callbacks get 50 ms, keybind callbacks 100 ms | M `config/lua/LuaCaller.hpp:36-42` |

Converter advice: emit only `hl.*` calls with literal tables (never let a nil reach a Lua operator). Each line then fails on its own instead of truncating the file.

---

## 1. Binds

### 1.1 Legacy grammar (L `legacy/ConfigManager.cpp:1492-1655`)

`bind<flags> = MODS, KEY[, DEVICES][, DESCRIPTION], DISPATCHER, ARGS`

- `DEVICES` is present only with flag `k`. It comes **before** `DESCRIPTION` (flag `d`): `DEVICEARGS = ARGS[2]`, `DESCRIPTION = ARGS[2+DEVICE_OFFSET]` (L:1580-1586).
- `bindm` (flag `m`): `MODS, KEY, ACTION` with no args field. `ACTION` is `movewindow` or `resizewindow[ N]` (L:1586-1589, more than 3 fields is an error, L:1561).
- ARGS is split with `CVarList(value, numbArgs)`, so the last field keeps its commas (L:1554-1555).
- MODS → `stringToModMask`: **substring** search, upper-cased, for `SHIFT, CAPS, CTRL|CONTROL, ALT|MOD1, MOD2, MOD3, SUPER|WIN|LOGO|MOD4|META, MOD5` (L `managers/KeybindManager.cpp:221-243`). Any separator works (`SUPER_SHIFT`, `SUPER SHIFT`, `$mainMod SHIFT`). A non-empty mod string with no match is an error (L:1601-1604).
- KEY → `parseKey` (L:1481-1490): a pure number > 9 is a **keycode**, `code:NN` is a keycode, `catchall` is the catch-all, anything else is a keysym name (case-insensitive). An empty KEY silently adds no bind (L:1616).
- `catchall` is only allowed inside a submap (L:1619-1622).

### 1.2 Lua: `hl.bind(keys, dispatcher, opts?)` → returns an `HL.Keybind` (M `LuaBindingsToplevel.cpp:64-232`)

- `keys`: one string, **`+`-separated**, each token trimmed (`CVarList2(value, 0, '+', true)`, :34-62). Empty token → error. Duplicate tokens (case-insensitive) → error.
- Token classification (M `keybinds/Bind.cpp:102-160`):
  - Modifier: an **exact** whole token (case-insensitive) from `SHIFT, CAPS, CTRL, CONTROL, ALT, MOD1, MOD2, MOD3, SUPER, WIN, LOGO, MOD4, META, MOD5` (M `keybinds/Resolver.cpp:17-21`). `SUPER_SHIFT` as a single token is **not** a modifier and fails with "Unknown key". The converter must split legacy MODS into separate tokens.
  - Sided modifier (also a modifier): `SHIFT_L SHIFT_R CONTROL_L CTRL_L CONTROL_R CTRL_R ALT_L ALT_R SUPER_L SUPER_R` (Bind.cpp:20-31).
  - Modifiers must come before keys ("Modifiers cannot appear after keys").
  - Key (M `keybinds/Key.cpp:8-43`): `catchall`, `mouse:NNN`, `switch:…`, `mouse_down|mouse_up|mouse_left|mouse_right` (event keys), `code:NN`, otherwise `xkb_keysym_from_name(…, CASE_INSENSITIVE)`. An unknown keysym → error "Unknown key: X".
  - At most one event key, and it cannot be combined with ordinary keys. Several ordinary keys form an ordered chord.
- Formats to emit:
  - empty mods: `"Return"`
  - mods: `"SUPER + SHIFT + Q"`
  - mouse: `"SUPER + mouse:272"`
  - wheel: `"SUPER + mouse_down"`
  - keycode: legacy `bind = SUPER, 24, …` or `code:24` → `"SUPER + code:24"`
  - catch-all: `"catchall"` (only inside `hl.define_submap`, else error :82-83)
- `dispatcher` is an `hl.dsp.*(...)` result **or** a plain Lua function. Passing the factory itself (`hl.dsp.exit` without `()`) is an explicit error (DispatcherUtils.cpp:109-128). A function callback should `return hl.dispatch(...)` when pass_event matters (bindResultFromTable, Internal.cpp:389-424; default `success=true`, `keybinds/Bind.hpp:58-63`).

### 1.3 Flag letters → `opts` (legacy L:1516-1543; Lua M Toplevel.cpp:130-203)

| Legacy | Meaning | Lua opts field |
|---|---|---|
| `l` | locked | `locked = true` |
| `r` | release | `release = true` |
| `e` | repeat | `repeating = true` |
| `m` | mouse (bindm) | `mouse = true` |
| `n` | non-consuming | `non_consuming = true` |
| `a` | auto-consuming | `auto_consuming = true` |
| `t` | transparent | `transparent = true` |
| `i` | ignore mods | `ignore_mods = true` |
| `s` | multi-key (`&`-joined mods/keys) | no flag. Put all keys in the `+` string (chord). UNVERIFIED that chord semantics equal legacy `s` |
| `o` | long press | `long_press = true` |
| `d` | has description | `description = "…"` (alias `desc`) |
| `p` | don't inhibit | `dont_inhibit = true` |
| `c` | click (implies release) | `click = true` (implies release, :163-166) |
| `g` | drag (implies release) | `drag = true` (:168-171) |
| `u` | submap universal | `submap_universal = true` |
| `k` | per-device | `device = { inclusive = <bool, default true>, list = { "kb1", "kb2" } }`. Legacy `!kb1 kb2` → `inclusive = false` (legacy L:1607-1614, Lua :185-202) |
| `x` | allow input capture | `allow_input_capture = true` |

Lua keeps the same exclusivity checks: click+drag; (long_press or release)+repeating; mouse with repeating/release/locked → error (:173-180). Unknown opts keys are silently ignored.

### 1.4 bindm

`bindm = SUPER, mouse:272, movewindow` → `hl.bind("SUPER + mouse:272", hl.dsp.window.drag(), { mouse = true })`

`bindm = SUPER, mouse:273, resizewindow[ N]`, where the Lua call depends on N (CA::mouse, M `config/shared/actions/ConfigActions.cpp:1695-1723`; Lua M Dispatchers.cpp:1050-1080):

| N | Lua dispatcher |
|---|---|
| none or 0 | `hl.dsp.window.resize()` |
| `1` (force ratio) | `hl.dsp.window.resize({ keep_aspect_ratio = true })` |
| `2` (block ratio) | `hl.dsp.window.resize({ keep_aspect_ratio = false })` |

### 1.5 unbind

- `unbind = all` → `hl.unbind("all")` (Toplevel.cpp:370-374).
- `unbind = MODS, KEY` → `hl.unbind("MODS + KEY")`.

**Caveat:** Lua matching is textual. Both the bind's display string and the argument are lower-cased with whitespace removed, then compared (M `keybinds/Registry.cpp:18-31, 88-100`). Legacy matched semantically (modmask + key, L:1667-1671). The converter must emit **one canonical spelling and mod order** for every bind and unbind (e.g. `SUPER + CTRL + ALT + SHIFT + key`, using `SUPER` for WIN/LOGO/MOD4/META).

### 1.6 Submaps

Legacy (L:1795-1800): `submap = NAME[, RESETTO]` … binds … `submap = reset`. `RESETTO` means: after any non-`submap` bind in this submap fires, switch to that submap (`reset` = default map) (L `KeybindManager.cpp:818-822`).

Lua (M Toplevel.cpp:234-263; examples in `hyprtester/test.lua:218-236`):

```lua
hl.bind("SUPER + R", hl.dsp.submap("resize"))
hl.define_submap("resize", function()            -- or hl.define_submap(name, "reset"|"other", fn)
  hl.bind("right", hl.dsp.window.resize({ x = 10, y = 0, relative = true }), { repeating = true })
  hl.bind("escape", hl.dsp.submap("reset"))
end)
```

- The 3-argument form `hl.define_submap(name, reset, fn)` equals legacy `submap = name, reset`.
- `hl.dsp.submap("reset")` or `""` returns to the global map. Any other name must be registered by dispatch time, else a runtime error (M ConfigActions.cpp:1740-1756), so definition order does not matter.
- Errors inside `fn` are caught and recorded. The rest of the file continues (:255-258).

---

## 2. Dispatchers (legacy name → Lua)

Legacy name list = L `managers/KeybindManager.cpp:41-112`. Arg handling = L `legacy/DispatcherTranslator.cpp` (function line in **T:n**). Lua registrations are at M `LuaBindingsDispatchers.cpp:1320-1391` (**D:n** = implementation line).

Common helpers:

- `window = <selector>`: optional on nearly every `hl.dsp.window.*`. Selector grammar: `class:RE`, `initialclass:RE`, `title:RE`, `initialtitle:RE`, `tag:RE`, `stableid:`, `address:0x…`, `pid:N`, `active…`, `floating`, `tiled` (M `desktop/state/ViewQuery.cpp:~80-160`). A bare string without a prefix compiles to an empty class regex, so always emit a prefix.
- `action` (toggle family): `"toggle"` or empty → toggle, `"enable"|"on"` → on, `"disable"|"off"` → off. Unknown values silently toggle (Internal.cpp:317-325, 477-485). The default when omitted is **toggle**.
- `direction`: `"left"|"l"`, `"right"|"r"`, `"up"|"u"|"t"`, `"down"|"d"|"b"` (Internal.cpp:305-315). Legacy used only the first char (`Math::fromChar(args[0])`), so map by first letter.
- Workspace/monitor selectors are passed through as strings with the legacy grammar (`e+1`, `r-1`, `m+1`, `name:x`, `special:x`, `previous`, `empty`, `+1`, `l`, `DP-1`, `desc:…`). Numbers are also accepted (Internal.cpp:21-28, 190-211).

| Legacy (args) | Lua | Notes |
|---|---|---|
| `exec CMD` | `hl.dsp.exec_cmd("CMD")` | `[rules] CMD` prefix is still parsed by the executor (M `supplementary/executor/Executor.cpp:101-120`), but prefer `hl.dsp.exec_cmd("CMD", { workspace="3 silent", float=true })` (see §4.1) |
| `execr CMD` | `hl.dsp.exec_raw("CMD")` | D:286 |
| `pass SEL` | `hl.dsp.pass({ window = SEL })` | `window` required (D:316-324) |
| `sendshortcut MODS, KEY, SEL` | `hl.dsp.send_shortcut({ mods="MODS", key="KEY", window=SEL? })` | Empty legacy SEL → omit `window` (T:531, D:429-441) |
| `sendkeystate MODS, KEY, STATE, SEL` | `hl.dsp.send_key_state({ mods, key, state="down"\|"up"\|"repeat", window? })` | D:443-465 |
| `killactive` | `hl.dsp.window.close()` | |
| `forcekillactive` | `hl.dsp.window.kill()` | |
| `closewindow SEL` | `hl.dsp.window.close({ window = SEL })` | |
| `killwindow SEL` | `hl.dsp.window.kill({ window = SEL })` | |
| `signal N` | `hl.dsp.window.signal({ signal = N })` | |
| `signalwindow SEL,N` | `hl.dsp.window.signal({ signal = N, window = SEL })` | T:101 |
| `togglefloating [SEL]` | `hl.dsp.window.float({ action = "toggle", window? })` | T:117 |
| `setfloating [SEL]` | `hl.dsp.window.float({ action = "on", window? })` | |
| `settiled [SEL]` | `hl.dsp.window.float({ action = "off", window? })` | |
| `pseudo [SEL]` | `hl.dsp.window.pseudo({ window? })` | Toggle |
| `pin [SEL]` | `hl.dsp.window.pin({ window? })` | Toggle (T:670) |
| `fullscreen [MODE [toggle\|set\|unset]]` | `hl.dsp.window.fullscreen({ mode = "fullscreen"\|"maximized", action = "toggle"\|"set"\|"unset" })` | Legacy: only `"1"` means maximized, anything else is fullscreen (T:160-186). Lua also accepts `mode="0"/"1"` (D:708-758) |
| `fullscreenstate I C` | `hl.dsp.window.fullscreen_state({ internal = I, client = C })` | Lua default action `"set"`. Both fields required. Legacy `-1` = "keep current" (T:188-207) has **no Lua equivalent**: resolve to explicit values or emit a function using `hl.get_active_window().fullscreen` / `.fullscreen_client` (UNVERIFIED field semantics) |
| `workspace W` | `hl.dsp.focus({ workspace = "W" })` | D:1137-1149 |
| `movetoworkspace W[,SEL]` | `hl.dsp.window.move({ workspace = "W", window? })` | Last comma splits SEL (T:209-223) |
| `movetoworkspacesilent W[,SEL]` | `hl.dsp.window.move({ workspace = "W", follow = false, window? })` | silent ⇔ `follow == false` (D:838-847) |
| `movefocus D` | `hl.dsp.focus({ direction = "left"… })` | |
| `movewindow D` | `hl.dsp.window.move({ direction = "left"… })` | T:248-268 |
| `movewindow mon:M [silent]` | `hl.dsp.window.move({ monitor = "M", follow = false? })` | D:849-858 |
| `swapwindow D` | `hl.dsp.window.swap({ direction = … })` | |
| `swapwindow SEL` | `hl.dsp.window.swap({ target = SEL })` | Aliases `with`, `other` (D:917-928) |
| `centerwindow [x]` | `hl.dsp.window.center()` | Legacy arg ignored in 0.56 (T:292) |
| `resizeactive DX DY` | `hl.dsp.window.resize({ x = DX, y = DY, relative = true })` | **Lua default is `relative = false`** (D:1063). Always emit `relative=true` for non-exact |
| `resizeactive exact W H` | `hl.dsp.window.resize({ x = W, y = H })` | Absolute must be ≥ 1 (ConfigActions.cpp:688) |
| `moveactive DX DY` / `exact X Y` | `hl.dsp.window.move({ x, y, relative = true })` / `({ x, y })` | D:826-836 |
| `resizewindowpixel ARGS,SEL` / `movewindowpixel ARGS,SEL` | same as above + `window = SEL` | First comma splits (T:448-473) |
| …with `%` values (`50%`, `exact 50% 50%`) | **no direct form** (x/y must be numbers, `tableOptNum`) | Legacy: exact% = % of monitor size, relative% = % of window size (L `Compositor.cpp:827-874`). Emit a Lua function that computes from `hl.get_active_window().size.x/.y` or `hl.get_active_monitor().width/.height` (fields exist: M `objects/LuaWindow.cpp:90`, `LuaMonitor.cpp:110-112`; UNVERIFIED end-to-end) |
| `cyclenext [prev\|p\|last\|l] [tiled\|tile] [floating\|float]` | `hl.dsp.window.cycle_next({ next = <bool>, tiled = true?, floating = true? })` | `next = false` only if a prev token is present (T:475-493). `hist` and `visible` were already ignored in 0.56 |
| `swapnext [prev\|l\|last\|b\|back]` | `hl.dsp.window.swap({ next = true })` / `({ prev = true })` | T:653 |
| `focuswindow SEL` / `focuswindowbyclass X` | `hl.dsp.focus({ window = SEL })` / `({ window = "class:X" })` | Both used the same selector in 0.56 (T:495, map :826-827) |
| `focusmonitor M` | `hl.dsp.focus({ monitor = "M" })` | |
| `focusurgentorlast` | `hl.dsp.focus({ urgent_or_last = true })` | |
| `focuscurrentorlast` | `hl.dsp.focus({ last = true })` | |
| `focusworkspaceoncurrentmonitor W` | `hl.dsp.focus({ workspace = "W", on_current_monitor = true })` | |
| `tagwindow [+\|-]TAG [SEL]` | `hl.dsp.window.tag({ tag = "+TAG", window? })` | Whitespace splits SEL (T:502-514) |
| `toggleswallow` | `hl.dsp.window.toggle_swallow()` | |
| `submap NAME` | `hl.dsp.submap("NAME")` | |
| `movecursortocorner N` | `hl.dsp.cursor.move_to_corner({ corner = N })` | Optional `window` |
| `movecursor X Y` | `hl.dsp.cursor.move({ x = X, y = Y })` | |
| `renameworkspace ID [NAME]` | `hl.dsp.workspace.rename({ workspace = ID, name = "NAME"? })` | |
| `exit` | `hl.dsp.exit()` | |
| `forcerendererreload` | `hl.dsp.force_renderer_reload()` | |
| `movecurrentworkspacetomonitor M` | `hl.dsp.workspace.move({ monitor = "M" })` | D:1289-1306 |
| `moveworkspacetomonitor W M` | `hl.dsp.workspace.move({ workspace = "W", monitor = "M" })` | |
| `swapactiveworkspaces M1 M2` | `hl.dsp.workspace.swap_monitors({ monitor1 = "M1", monitor2 = "M2" })` | |
| `togglespecialworkspace [NAME]` | `hl.dsp.workspace.toggle_special("NAME")` or `toggle_special()` | **Positional string**, not a table (D:1251-1255). Empty → `special:special` (M `workspace/WorkspaceUtils.hpp:8-10`) |
| `togglegroup` | `hl.dsp.group.toggle()` | |
| `changegroupactive f\|forward` (default) | `hl.dsp.group.next()` | |
| `changegroupactive b\|prev` | `hl.dsp.group.prev()` | |
| `changegroupactive N` | `hl.dsp.group.active({ index = N })` | 1-based, ≤0 = last, same as legacy (T:300-323; ConfigActions.cpp:1005-1024) |
| `movegroupwindow [b\|prev]` | `hl.dsp.group.move_window({ forward = false })` | Default forward |
| `lockgroups lock\|unlock\|toggle` | `hl.dsp.group.lock({ all = true, action = "on"\|"off"\|"toggle" })` | **Legacy empty = lock**, Lua omitted = toggle (T:702-711, D:142-167). Always emit `action` |
| `lockactivegroup lock\|unlock\|toggle` | `hl.dsp.group.lock({ action = … })` | Legacy empty/other = **unlock** (T:713-722). `group.lock_active` exists but is deprecated (D:169-179) |
| `moveintogroup D` | `hl.dsp.window.move({ into_group = "l" })` | |
| `moveintoorcreategroup D` | `hl.dsp.window.move({ into_or_create_group = "l" })` | |
| `moveoutofgroup [SEL\|active]` | `hl.dsp.window.move({ out_of_group = true, window? })` | D:882-897 |
| `movewindoworgroup D` | `hl.dsp.window.move({ direction = …, group_aware = true })` | D:812-818 |
| `denywindowfromgroup on\|off\|toggle` | `hl.dsp.window.deny_from_group({ action = … })` | Legacy empty/other = **off** (T:753-762) |
| `setignoregrouplock` | **none** | Already a no-op in 0.56 (T:861). Drop it |
| `workspaceopt …` | **none** | Already an error in 0.56 (T:356-358) |
| `layoutmsg MSG` | `hl.dsp.layout("MSG")` | |
| `togglesplit` (≤0.53) | `hl.dsp.layout("togglesplit")` | Dwindle (M `layout/algorithm/tiled/dwindle/DwindleAlgorithm.cpp:681`). Removed as a dispatcher in L54 (`KeybindManager.cpp:1712-1722`) |
| `swapsplit` (≤0.53) | `hl.dsp.layout("swapsplit")` | Dwindle :686 |
| `splitratio [exact ]V` (≤0.53) | `hl.dsp.layout("splitratio V[ exact]")` | **Order flips**: legacy `exact` first (L52 `KeybindManager.cpp:1808-1816`), dwindle wants value then `exact` (Dwindle :749-751) |
| `alterzorder top\|bottom[,SEL]` | `hl.dsp.window.alter_zorder({ mode = "top", window? })` | |
| `bringactivetotop` | `hl.dsp.window.bring_to_top()` | |
| `global NAME` | `hl.dsp.global("NAME")` | |
| `event DATA` | `hl.dsp.event("DATA")` | |
| `setprop SEL PROP VALUE…` | `hl.dsp.window.set_prop({ prop = "PROP", value = "VALUE", window = SEL })` | Legacy order is window, prop, value (T:772-783). Same `CA::setProp` both sides |
| `dpms on\|off\|toggle [M]` | `hl.dsp.dpms({ action = "on"\|"off"\|"toggle", monitor = "M"? })` | **Legacy empty arg = OFF**, Lua omitted = toggle (T:633-651). Always emit `action` |
| `forceidle SECS` | `hl.dsp.force_idle(SECS)` | Positional number |
| `releaseinputcapture` | `hl.dsp.release_input_capture()` | |
| `mouse` (bindm only) | see §1.4 | |
| `fakefullscreen` | **none** | Not present in L54/L |

Lua-only: `hl.dsp.reload_config()`, `hl.dsp.window.clear_tags()`, `hl.dsp.workspace.change_id({workspace,id})`, `hl.dsp.no_op()`.

Run a dispatcher outside a bind with `hl.dispatch(hl.dsp.…)` (Toplevel.cpp:328-345).

---

## 3. Rules

### 3.1 `hl.window_rule(spec)` (M `LuaBindingsConfigRules.cpp:1141-1249`)

- `name` is optional. Without a name, a new unnamed rule is created on each call. **With a name that already exists**, the call adds matches and effects to the existing rule. Never reuse a name for two different legacy rules.
- `enabled` (bool, default true).
- `match = { prop = value }` with prop names from M `desktop/rule/Rule.cpp:27-44`:
  - regex props: `class`, `title`, `initial_class`, `initial_title`, `content`, `xdg_tag`, `namespace`
  - bool props: `float`, `xwayland`, `fullscreen`, `pin`, `focus`, `group`, `modal`
  - int props: `fullscreen_state_internal`, `fullscreen_state_client`
  - other: `tag`, `workspace`
  - Values may be string, bool or number. Numbers go through `lua_tointeger`. Regex `negative:` prefix is supported (`RegexMatchEngine.cpp:6-13`). Bool match uses `truthy` (`true`/`yes`/`on`/`1`).
- Every other key is an effect. Types come from M `LuaBindingsInternal.hpp:49-110`:

| Lua type | Effects |
|---|---|
| **bool** (Lua `true`/`false` or 0/1 only; strings are rejected, `LuaConfigBool.cpp:10-27`) | `float tile fullscreen maximize center pseudo no_initial_focus pin persistent_size allows_input dim_around decorate focus_on_activate keep_aspect_ratio nearest_neighbor no_anim no_blur no_dim no_focus no_follow_mouse no_max_size no_shadow no_glow no_wobble no_shortcuts_inhibit opaque force_rgbx sync_fullscreen immediate xray render_unfocused no_screen_share no_vrr no_auto_hdr stay_focused confine_pointer no_xdg_drags` |
| **int** | `rounding` (0-100), `border_size`, `no_close_for` |
| **float** | `scrolling_width`, `rounding_power` (1-10), `scroll_mouse` / `scroll_touchpad` (0.01-10) |
| **expression vec2** (string `"EXPR EXPR"`, or `{a, b}` of numbers or strings; `LuaConfigExpressionVec2.cpp:26-64`) | `move`, `size`, `min_size`, `max_size`. Variables: `monitor_w monitor_h window_w window_h window_x window_y cursor_x cursor_y` |
| **string** | `fullscreen_state` ("I C"), `monitor`, `workspace` ("3 silent"), `group`, `suppress_event`, `content`, `animation`, `idle_inhibit`, `opacity` ("0.9 0.8"), `tag` ("+x"), `tonemap` |
| **gradient** | `border_color`: color string or `{colors={…}, angle=N}`. A legacy multi-color string is also accepted for this effect only (Internal.cpp:528-534) |

Plugin-registered effects are taken as string/bool/number (:1216-1236).

### 3.2 Legacy syntax A — 0.53…0.56 inline: `windowrule = match:class ^(x)$, match:title y, float on, size 800 600`

(L:2016-2055) Comma-separated fields, each `KEY VALUE` split at the first space. `match:PROP` is a match, anything else is an effect. Since 0.53 the names are **identical** to Lua (L `WindowRuleEffectContainer.cpp` vs M; M only adds `no_glow no_wobble no_xdg_drags`).

Translation: `match:P V` → `match.P = "V"`, and `E V` → `E = <typed V>`.

- bool effects: `truthy(V)` → `true`/`false` (`truthy` = "1" or prefix true/yes/on, L `helpers/MiscFunctions.cpp:829-844`)
- int and float effects: tonumber
- all others: keep the string verbatim

Same for the **block form**:

```
windowrule {
  name = n
  enable = 0|1
  match:class = …
  float = on
}
```

(special category keyed by `name`, L:591-596, 893-917) → `hl.window_rule({ name = "n", enabled = <enable≠0>, … })`.

`windowrulev2` in 0.56 is a hard error ("deprecated", L:431-435).

### 3.3 Legacy syntax B — ≤0.52 `windowrule`/`windowrulev2 = EFFECT[ VAL], [EFFECT2…,] key:val, key:val`

In L52 both keywords use the same parser (L52 `config/ConfigManager.cpp:2685-2810`). Tokens before the first `key:val` are effects. A token is a param when it contains `:`, except when it starts with `workspace `/`monitor ` or ends with `plugin`.

Param → match key:

| Old param | Lua match |
|---|---|
| `class` | `class` |
| `title` | `title` |
| `initialClass` | `initial_class` |
| `initialTitle` | `initial_title` |
| `tag` | `tag` |
| `xwayland:1` | `xwayland = true` |
| `floating` | `float` |
| `fullscreen` | `fullscreen` |
| `pinned` | `pin` |
| `focus` | `focus` |
| `group` | `group` |
| `modal` | `modal` |
| `fullscreenstate:I C` | `fullscreen_state_internal = I, fullscreen_state_client = C` (drop `*`) |
| `workspace:` / `onworkspace:` | `workspace` (approximate; UNVERIFIED that the old `workspace:` id/name form behaves identically) |
| `content` | `content` |
| `xdgTag` | `xdg_tag` |

Old bool params: `"1"` = true, anything else = false.

Old effect → new effect (from the L52 `desktop/WindowRule.cpp:7-14` lists and `Window.hpp:479-512` props):

| Old effect | New effect |
|---|---|
| `float` | `float=true` |
| `tile` | `tile` |
| `fullscreen` | `fullscreen` |
| `maximize` | `maximize` |
| `noinitialfocus` | `no_initial_focus` |
| `pin` | `pin` |
| `stayfocused` | `stay_focused` |
| `renderunfocused` | `render_unfocused` |
| `persistentsize` | `persistent_size` |
| `center [1]` | `center=true` |
| `pseudo` | `pseudo=true` |
| `animation X` | `animation="X"` |
| `bordercolor X` | `border_color="X"` |
| `bordersize N` | `border_size=N` |
| `rounding N` | `rounding=N` |
| `roundingpower F` | `rounding_power=F` |
| `content X` | `content` |
| `fullscreenstate I C` | `fullscreen_state="I C"` |
| `group X` | `group="X"` |
| `idleinhibit X` | `idle_inhibit` |
| `maxsize` | `max_size` |
| `minsize` | `min_size` |
| `monitor` | `monitor` |
| `move` | `move` |
| `size` | `size` |
| `noclosefor N` | `no_close_for` |
| `opacity` | `opacity` |
| `scrollmouse` | `scroll_mouse` |
| `scrolltouchpad` | `scroll_touchpad` |
| `suppressevent` | `suppress_event` |
| `tag` | `tag` |
| `workspace` | `workspace` |
| `xray [0/1]` | `xray` |
| `novrr` | `no_vrr` |
| `allowsinput` | `allows_input` |
| `dimaround` | `dim_around` |
| `decorate` | `decorate` |
| `focusonactivate` | `focus_on_activate` |
| `keepaspectratio` | `keep_aspect_ratio` |
| `nearestneighbor` | `nearest_neighbor` |
| `noanim` | `no_anim` |
| `noblur` | `no_blur` |
| `nodim` | `no_dim` |
| `nofocus` | `no_focus` |
| `nofollowmouse` | `no_follow_mouse` |
| `nomaxsize` | `no_max_size` |
| `noshadow` | `no_shadow` |
| `noshortcutsinhibit` | `no_shortcuts_inhibit` |
| `opaque` | `opaque` |
| `forcergbx` | `force_rgbx` |
| `syncfullscreen` | `sync_fullscreen` |
| `immediate` | `immediate` |
| `noscreenshare` | `no_screen_share` |
| `noborder` | **no effect** → `border_size = 0` |
| `norounding` | **no effect** → `rounding = 0` |
| `prop X V` | the same mapping applied to X |
| `plugin:…` | drop or warn |
| `unset …` | no equivalent (Lua rules are rebuilt every reload; drop the rule) |

- Old `move`/`size` values like `50% 50%` → expression `monitor_w*0.5 monitor_h*0.5`. Also `cursor …` and `onscreen …` prefixes: no verified mapping (UNVERIFIED).
- **Very old v1 (`windowrule = EFFECT, REGEX`)**: the 2nd field is a class regex → `match = { class = REGEX }` (behavior predates L52; in L52 a field without `:` is treated as an effect and errors).
- Multiple effects in one old line → one `hl.window_rule` with all effects.

### 3.4 Layer rules → `hl.layer_rule(spec)` (M ConfigRules.cpp:1251-1364)

`name`/`enabled`/`match` work as for window rules. Match values are string or bool. Normally `match = { namespace = "^(waybar)$" }`.

Effects (ConfigRules.cpp:182-193):

| Effect | Type |
|---|---|
| `no_anim`, `blur`, `blur_popups`, `dim_around`, `xray`, `no_screen_share` | bool |
| `ignore_alpha` | float 0-1 |
| `animation` | string |
| `order` | int |
| `above_lock` | int 0-2 |

- 0.53-0.56 inline `layerrule = match:namespace x, blur on` and the block form: names identical, translate as in §3.2 (L:2057-2094).
  - Note: in L the block form skips rules whose `enable` is explicitly ≠ 0 (an inverted check, L:921-923). Translate `enable` faithfully anyway.
- ≤0.52 `layerrule = RULE, NAMESPACE_REGEX` (L52 `ConfigManager.cpp:2812-2840`; names from `LayerRule.cpp:7-8`):

| Old rule | New effect |
|---|---|
| `noanim` | `no_anim` |
| `blur` | `blur` |
| `blurpopups` | `blur_popups` |
| `dimaround` | `dim_around` |
| `noscreenshare` | `no_screen_share` |
| `ignorealpha [A]` | `ignore_alpha = A` (default 0) |
| `ignorezero` | `ignore_alpha = 0` |
| `xray B` | `xray` |
| `animation X` | `animation` |
| `order N` | `order` |
| `abovelock` | `above_lock = 1` |
| `abovelock true` (interactable) | `above_lock = 2` (1/2 meaning inferred from the int range 0-2, UNVERIFIED) |
| `unset` | drop |

### 3.5 Workspace rules → `hl.workspace_rule(spec)` (M ConfigRules.cpp:197-233, 634-741)

`workspace = ID, k:v, k:v…` (L:1676-1793) → `hl.workspace_rule({ workspace = "ID", … })`. `workspace` is required and must be a string. There is no `name` field. Unknown fields → error. `enabled` is an optional bool.

| Legacy | Lua |
|---|---|
| `monitor:M` | `monitor = "M"` |
| `default:B` | `default = bool` |
| `persistent:B` | `persistent = bool` |
| `gapsin:CSS` | `gaps_in = <cssgap>` (§4.7) |
| `gapsout:CSS` | `gaps_out = <cssgap>` |
| — | `float_gaps` (Lua-only) |
| `bordersize:N` | `border_size = N` |
| `border:B` | `no_border = not B` |
| `shadow:B` | `no_shadow = not B` |
| `rounding:B` | `no_rounding = not B` |
| `decorate:B` | `decorate = B` |
| `defaultName:S` | `default_name = "S"` |
| `on-created-empty:CMD` | `on_created_empty = "CMD"` (**deprecated**, warns. Better: `hl.on("workspace.created", fn)` and check `.is_empty`, :220-226) |
| `layoutopt:K:V` | `layout_opts = { K = "V" }` |
| `layout:L` | `layout = "L"` |
| `animation:S` | `animation = "S"` |

Legacy B values use `ParserUtils::parseInt` (`true/on/yes/1`).

---

## 4. Misc

### 4.1 exec family and `hl.on`

- `hl.exec_cmd(cmd, rules?)` (Toplevel.cpp:297-311): **spawns immediately when the line is evaluated**.
- `hl.dsp.exec_cmd(cmd, rules?)` is the bind version.
- `rules` = window-rule effects table (Internal.cpp:542-606; `floating` is an alias of `float`). An unknown effect → error, and the command is **not** spawned.
- Legacy `[a b; c; d e] cmd` → `{ a = "b", c = true, d = "e" }`. Inside brackets, legacy 0.53+ uses the new effect names. Old (≤0.52) names inside brackets are **silently ignored** by the current executor (`WindowRule.cpp:489-523` `continue; // invalid`), so rename them (§3.3).

Events (M `LuaEventHandler.cpp:253-288`): `hyprland.start` (once, at compositor start, :157), `hyprland.shutdown` (:158-161), `config.reloaded` (**every** load including the first and `--verify-config`, emitted in `postConfigReload` M `ConfigManager.cpp:824`), plus window.*, workspace.*, monitor.*, etc. Signature: `hl.on(name, fn)`.

| Legacy | Lua | Why |
|---|---|---|
| `exec-once = CMD` | `hl.on("hyprland.start", function() hl.exec_cmd("CMD") end)` | Legacy queued to start (L:1269-1274). This is the pattern in `example/hyprland.lua:45-49` |
| `execr-once = CMD` | `hl.on("hyprland.start", function() hl.dispatch(hl.dsp.exec_raw("CMD")) end)` | |
| `exec = CMD` (legacy: first launch → deferred to start; each reload → run, L:1259-1267) | Recommended: same as exec-once. Re-running on reload has no clean equivalent | Top-level `hl.exec_cmd` runs at **parse time**. The config is loaded at `STAGE_PRIORITY` before the Wayland socket exists (M `Compositor.cpp:304` vs socket at :391-420), so children get an empty `WAYLAND_DISPLAY` (`Executor.cpp:194`). It also runs under `--verify-config`. `config.reloaded` also fires on the first load and in verify mode. UNVERIFIED workaround: inside a `config.reloaded` handler, guard on `#hl.get_monitors() > 0` |
| `execr = CMD` | as `exec`, with `hl.dsp.exec_raw` | |
| `exec-shutdown = CMD` | `hl.on("hyprland.shutdown", function() hl.exec_cmd("CMD") end)` | 50 ms callback budget; fork is fine |

### 4.2 env

| Legacy | Lua |
|---|---|
| `env = NAME,VALUE` | `hl.env("NAME", "VALUE")` |
| `envd = NAME,VALUE` | `hl.env("NAME", "VALUE", true)` |

The value is everything after the first comma (L:1857-1885; Lua ConfigRules.cpp:488-545). Same skip-if-unchanged logic on reload.

### 4.3 Curves and animations

- `bezier = NAME, x1, y1, x2, y2` → `hl.curve("NAME", { type = "bezier", points = { {x1, y1}, {x2, y2} } })`.
  - Each coordinate must be within **[-1, 2]** in Lua (ConfigRules.cpp:327). Legacy had no limit (L:1389-1424).
  - Springs (Lua-only): `{ type="spring", mass, stiffness, damping }` (each > 0.5).
- `animation = NAME, 0[, …]` → `hl.animation({ leaf = "NAME", enabled = false })`.
- `animation = NAME, 1, SPEED, CURVE[, STYLE]` → `hl.animation({ leaf = "NAME", enabled = true, speed = SPEED, bezier = "CURVE", style = "STYLE" })` (ConfigRules.cpp:400-486; legacy L:1426-1479).
  - Legacy "enabled" is parsed via `parseInt` (`on`/`yes`/`true` → 1).
  - `speed` must be > 0.
  - The curve must already exist, so emit `hl.curve` first. `"default"` is built in (used in `example/hyprland.lua:145`).

### 4.4 Gestures

`gesture[p] = FINGERS, DIR, [mod:MODS,] [scale:S,] ACTION[, args]` (L:1931-2014) → `hl.gesture({...})` (ConfigRules.cpp:743-941):

| Legacy | Lua |
|---|---|
| `p` flag | `disable_inhibit = true` |
| `FINGERS` | `fingers` (2-9) |
| `DIR` | `direction` (same strings, `dirForString`) |
| `mod:X` | `mods = "X"` |
| `scale:S` | `scale = S` (Lua: ±0.1…10, legacy clamped to 0.1…10) |
| `workspace` / `resize` / `move` / `close` | `action = "workspace"` / `"resize"` / `"move"` / `"close"` |
| `special, NAME` | `action="special", workspace_name="NAME"` |
| `float, MODE` | `action="float", mode="MODE"` |
| `fullscreen, MODE` | `action="fullscreen", mode="MODE"` |
| `cursorZoom, LVL, MODE` | `action="cursor_zoom", zoom_level="LVL", mode="MODE"` |
| `scrollMove` | `action="scroll_move"` |
| `unset` | `action="unset"` |
| `dispatcher, D, ARGS` | `action = function() hl.dispatch(<§2 translation of D ARGS>) end` |

A Lua function action fires once at gesture end, like the legacy dispatcher gesture (M `LuaFunctionGesture.cpp:152-162` vs L `DispatcherGesture.cpp:17-22`). A dispatcher object is **not** accepted as `action`: only a function, a `{start,update,finish}` table, or a string.

### 4.5 permission, plugin

- `permission = BIN_REGEX, TYPE, MODE` → `hl.permission("BIN_REGEX", "TYPE", "MODE")` or `hl.permission({ binary=…, type=…, mode=… })` (ConfigRules.cpp:566-632).
  - TYPE: `screencopy`, `cursorpos`, `plugin`, `keyboard`/`keeb`, `input-capture`.
  - MODE: `ask`, `allow`, `deny`.
  - Applied only on first launch (same as legacy).
- `plugin = PATH` → `hl.plugin.load("PATH")` (:547-564).

### 4.6 Devices and monitors

- `device { name = X … }` → `hl.device({ name = "X", … })` (:1020-1075). Spaces in the name become `-`.
  - Allowed fields (:235-279): `sensitivity accel_profile rotation kb_file kb_layout kb_variant kb_options kb_rules kb_model repeat_rate repeat_delay natural_scroll tap_button_map numlock_by_default resolve_binds_by_sym disable_while_typing clickfinger_behavior middle_button_emulation tap_to_click tap_and_drag drag_lock left_handed scroll_method scroll_button scroll_button_lock scroll_points scroll_factor transform output enabled region_position absolute_region_position region_size relative_input active_area_position active_area_size flip_x flip_y drag_3fg keybinds share_states release_pressed_on_close tags`.
  - Legacy `tap-to-click` / `tap-and-drag` → `_`.
  - Legacy device keys `eraser_button_mode eraser_button_override pressure_range_min pressure_range_max` (L:515-566) are **not** in the Lua list: "unknown field" error. Unknown whether they moved; the globals `input.tablettool.*` exist (M `ConfigValues.cpp:468-479`).
- `monitor = NAME, MODE, POS, SCALE[, mirror X][, bitdepth N][, cm X][, sdrbrightness F][, sdrsaturation F][, transform N][, vrr N][, icc P]` (L:1293-1387) → `hl.monitor({ output = "NAME", mode, position, scale, mirror, bitdepth, cm, sdrbrightness, sdrsaturation, transform, vrr, icc })` (M :81-178, 1077-1139).
  - `NAME, disable` → `disabled = true`.
  - `NAME, transform, N` → `transform = N` (merges with an existing rule, :1098-1100).
  - `NAME, addreserved, T, B, L, R` (note the legacy order T,B,L,R, L:1323) → `reserved = { top=T, right=R, bottom=B, left=L }`.
  - `monitorv2 { output=… }` keys map 1:1, except `addreserved` → `reserved`. `sdr_eotf` is an int in legacy and a string in Lua (UNVERIFIED value names).

### 4.7 Options (`hl.config`) and value formats

- Key mapping: `luaConfigValueName` replaces `:` → `.` and `-` → `_` (M `ConfigManager.cpp:1114-1119`).
- `hl.config` walks nested tables and joins keys with `.`, so `decoration:blur:size = 3` and the block `decoration { blur { size = 3 } }` both → `hl.config({ decoration = { blur = { size = 3 } } })`.
- Dotted subkeys like `col.active_border` nest as well: `general = { col = { active_border = … } }` (example :93-96). A literal key `["decoration.blur.size"]` also works (lookup by the full key first, :970-976).
- Unknown key → error, the rest continues.

Value conversions:

| Type | Lua accepts | Legacy → Lua |
|---|---|---|
| Bool | `true`/`false`, 0/1 **only** (`LuaConfigBool.cpp:10-27`) | `yes/on/true/1` → `true`, `no/off/false/0` → `false` |
| Int | integer, bool, or a string **only if the option has a name map** (`LuaConfigInt.cpp:11-63`) | A float literal like `2.0` is rejected: emit an integer |
| Float | number | |
| Color | string: `rgba(RRGGBBAA)`, `rgba(r,g,b,a)`, `rgb(…)`, `#rgb[a]`, `0xAARRGGBB`, decimal (`ParserUtils.cpp:23-130`; `LuaConfigColor.cpp:25-40`) | Keep the legacy text as a quoted string. A Lua number like `0xee1a1a1a` also works (example :119) |
| Gradient | single color string, or `{ colors = {"c1","c2",…}, angle = DEG }` (`LuaConfigGradient.cpp:21-84`) | `rgba(a) rgba(b) 45deg` → `{ colors = {"rgba(a)","rgba(b)"}, angle = 45 }`. A multi-color **string is rejected** here (except the `border_color` window-rule effect). Legacy: tokens split on space, `deg` token = angle, max 10 colors (L:103-161) |
| Vec2 | `"x y"`, `"x,y"`, or `{x, y}` (`LuaConfigVec2.cpp:14-62`) | Pass through |
| CssGap | number, or `{ top, right, bottom, left }` (missing fields = **0**, no CSS shorthand; `LuaConfigCssGap.cpp:18-66`) | Expand the legacy shorthand (L `shared/complex/ComplexDataTypes.hpp:81-120`): 1 value → all four; 2 values `v,h` → top/bottom = v, left/right = h; 3 values `t,h,b`; 4 values `t,r,b,l` |
| FontWeight | number, or name (`thin ultralight light semilight book normal medium semibold bold ultrabold heavy ultraheavy`) (`LuaConfigFontWeight.cpp:16-45`) | |
| String | any string (some have validators) | |

Option changes 0.56.2 → 0.57.0 (diff of `ConfigValues.cpp`):

- Removed: `debug:fifo_pending_workaround`.
- Bool → Int (Lua Int accepts bools): `binds:workspace_back_and_forth`, `misc:exit_window_retains_fullscreen`, `scrolling:follow_focus`.
- Deprecated but still present: `misc:layers_hog_keyboard_focus` (→ `layers:`), `debug:invalidate_fp16`, `dwindle:special_scale_factor`, `master:special_scale_factor`.

Plugin options `plugin:NAME:opt` → `plugin.NAME.opt` (`registerPluginValue`, :1125-1142). Plugins load in `postConfigReload`, after evaluation (`handlePluginLoads`, :820), so plugin keys may be "unknown" on the first evaluation (UNVERIFIED).

### 4.8 `source`, variables and other hyprlang features

- `source = PATH` (glob; relative to the **current** file, L:1802-1855) → `require(...)` (M `ConfigManager.cpp:80-380, 508-572`):
  - Module form: `package.path` is prefixed with `<configdir>/?.lua;<configdir>/?/init.lua`. `require("conf.colors")` → `<configdir>/conf/colors.lua`.
  - Explicit path form (starts with `/`, `./`, `../` or `~/`): `require("./conf/colors.lua")`. It tries `P`, `P.lua`, `P/init.lua`. **Relative to the main config file**, not the requiring file (`resolveRequirePath` → `absolutePath(raw, mainConfigPath)`). Rebase legacy relative paths.
  - Globs (`*?[`) work only with explicit paths: `require("./conf.d/*.lua")`. Matches are sorted; no match → Lua error.
  - Modules run once per load (`package.loaded`, cleared on each reload, :631-655). Legacy could source the same file twice.
- `$VAR = value` / `$VAR` → Lua `local VAR = "value"` and string concatenation. Note Lua's limit of 200 locals per function; use a table for large configs.
- hyprlang `# hyprlang if/endif/noerror` directives and `##` escapes have no Lua counterpart: resolve them at conversion time.

---

## 5. Verifying without starting the compositor

Command: `Hyprland --verify-config -c /path/hyprland.lua` (M `src/main.cpp:32-39, 118-160, 291-295`).

Preconditions, checked before verification:

- `-c` must be an existing regular file (canonicalized), else exit 1.
- `XDG_RUNTIME_DIR` must be set and non-empty, else exit 1 (main.cpp:199-202).
- It refuses to run as root or setuid (`getuid()!=geteuid() || !geteuid()`, `init/*.cpp:20-22`), else exit 1. Bypass with `--i-am-really-stupid`.
- Any extension is loaded as Lua (Jeremy explicit path → `Lua::CConfigManager`, M `config/ConfigManager.cpp:15-35`). A `.conf` file just fails as a Lua syntax error.

What happens:

- `CCompositor(true)` returns early: no instance dir, no signature (`Compositor.cpp:193-195`).
- `initServer` creates only `Keybinds::mgr()`, `Animation::mgr()` and the config manager, runs `init()` (a full reload), then prints `\n\n======== Config parsing result:\n\n` followed by `config ok` or the newline-joined error list. It returns **before** `wl_display_create`: no Wayland socket, no backend, no GPU, no display needed (`Compositor.cpp:279-287`).

Exit code: `return !configVerifPassed()` (main.cpp:291-295):

- `0` = no errors.
- `1` = any collected error: syntax error, runtime Lua error, any `hl.*` ERROR-level config error, or a `require` error (`m_lastConfigVerificationWasSuccessful = m_errors.empty()`, ConfigManager.cpp:717-722).
- Precondition failures also exit 1.

What it checks:

- Lua syntax and runtime.
- Unknown or ill-typed `hl.config` keys.
- Rule fields and effect values (parsed at call time).
- Bind key strings (keysym lookup via xkb, no keyboard needed).
- Dispatcher **factory** arguments (evaluated when `hl.bind` is called).
- Curve and animation names, gesture fields, monitor and device fields.

What it does NOT check: dispatch-time failures (bad workspace/monitor selector strings, unknown `layoutmsg` text, a submap name that does not exist, window selector strings).

Side effects to beware of in verify mode:

- Top-level `hl.exec_cmd` really forks processes (`Executor.cpp:173-215`).
- `config.reloaded` handlers run.
- `hl.env` calls `setenv` in the verifier process.
- `hyprland.start` handlers do **not** run (start is emitted by the renderer, `render/Renderer.cpp:2294`).
- UNVERIFIED: that no `hl.*` binding dereferences an uninitialized manager in verify mode (`hl.permission` guards `g_pDynamicPermissionManager`; `g_pTrackpadGestures` is statically constructed, `TrackpadGestures.hpp:48`).
