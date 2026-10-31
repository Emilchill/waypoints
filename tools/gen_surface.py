#!/usr/bin/env python3
"""Generates the Waypoints menu: a custom JSON-UI surface (src/surface.json) and the list of
backing controls whose indices its buttons press (src/surface_controls.rs).

Screens: List, Form (new or edit), Settings. Everything dynamic comes from `#bindings` the mod
updates; the document itself never changes."""
import json, pathlib, sys
sys.path.insert(0, str(pathlib.Path(__file__).parent))
import kofi

ROOT = pathlib.Path(__file__).resolve().parents[1] / "mods/waypoints/src"
W, H = 420, 262
FONT = "mod_panel"

BG = [0.055, 0.063, 0.082, 0.96]
CARD = [0.105, 0.117, 0.145, 1.0]
RAISED = [0.16, 0.18, 0.22, 1.0]
BORDER = [0.24, 0.27, 0.33, 1.0]
TEXT = [0.95, 0.96, 0.98, 1.0]
MUTED = [0.6, 0.65, 0.72, 1.0]
ACCENT = [0.29, 0.62, 1.0, 1.0]
DANGER = [0.93, 0.33, 0.33, 1.0]
WHITE = [1, 1, 1, 1]

controls = []  # (kind, id)
def control(cid, kind="button"):
    for i, (k, c) in enumerate(controls):
        if c == cid:
            return i
    controls.append((kind, cid))
    return len(controls) - 1

def place(node, size, offset, anchor="top_left"):
    node.update({"size": size, "offset": offset, "anchor_from": anchor, "anchor_to": anchor})
    return node

def rect(size, offset, color, radius=4, binding=None, alpha=None):
    n = place({"type": "custom", "renderer": "cinnabar_rounded_rectangle", "radius": radius, "color": color}, size, offset)
    b = []
    if binding:
        b.append({"binding_name": binding, "binding_name_override": "#color"})
    if alpha:
        b.append({"binding_name": alpha, "binding_name_override": "#alpha"})
    if b:
        n["bindings"] = b
    return n

def text(value, size, offset, color=TEXT, scale=0.85, align="left", bound=False, color_binding=None, font=FONT):
    n = place({"type": "label", "text": value, "color": color, "localize": False, "font_type": font,
               "font_scale_factor": scale}, size, offset)
    if align != "left":
        n["text_alignment"] = align
    b = []
    if bound:
        b.append({"binding_name": value})
    if color_binding:
        b.append({"binding_name": color_binding, "binding_name_override": "#color"})
    if b:
        n["bindings"] = b
    return n

def panel(size, offset, children, visible=None):
    n = place({"type": "panel", "controls": children}, size, offset)
    if visible:
        n["bindings"] = [{"binding_name": visible, "binding_name_override": "#visible"}]
    return n

def named(name, node):
    return {name: node}

def button(cid, size, offset, children, kind="button", visible=None, hover=True):
    idx = control(cid, kind)
    # Hover: the host eases #row_N_hover from 0 to 1 under the pointer; drawn over the button.
    glow = [named("hover", rect(size, [0, 0], [1, 1, 1, 0.1], radius=4, alpha=f"#row_{idx}_hover"))] if hover else []
    n = place({"type": "button", "controls": children + glow,
               "button_mappings": [{"from_button_id": "button.menu_select", "to_button_id": f"mod.control:{idx}", "mapping_type": "pressed"}]},
              size, offset)
    if visible:
        n["bindings"] = [{"binding_name": visible, "binding_name_override": "#visible"}]
    return n

def text_button(cid, label, size, offset, style="raised", bound=False, visible=None, kind="button", scale=0.85):
    fill = {"raised": RAISED, "accent": ACCENT, "danger": [0.35, 0.12, 0.13, 1], "ghost": [0.135, 0.15, 0.185, 1]}[style]
    color = {"danger": [1, 0.62, 0.62, 1]}.get(style, TEXT)
    kids = [named("bg", rect(size, [0, 0], fill, 5))]
    kids.append(named("text", text(label, [size[0] - 8, 10], [4, (size[1] - 10) / 2 + 0.5], color, scale, "center", bound)))
    return button(cid, size, offset, kids, kind, visible)

def pill(cid, color_binding, offset):
    """A toggle switch drawn from a colour and a knob offset binding."""
    track = rect([22, 12], [0, 0], RAISED, 6, binding=color_binding + "_col")
    knob = rect([8, 8], [2, 2], WHITE, 4)
    knob["bindings"] = [{"binding_name": color_binding + "_knob", "binding_name_override": "#offset"}]
    return button(cid, [22, 12], offset, [named("track", track), named("knob", knob)])

# ---------------------------------------------------------------- header
header = [
    named("title", text("Waypoints", [100, 14], [12, 8], TEXT, 1.05)),
    named("dim_chip", rect([78, 14], [92, 7], RAISED, 7)),
    named("dim", text("#dim", [74, 10], [94, 9.5], MUTED, 0.75, "center", True)),
    named("settings", text_button("open_settings", "Settings", [56, 16], [W - 86, 6], "ghost")),
    # The close button routes straight to the host's own close action.
    named("close", place({"type": "button", "controls": [named("x", text("X", [18, 10], [0, 3.5], MUTED, 0.85, "center"))],
        "button_mappings": [{"from_button_id": "button.menu_select", "to_button_id": "mod.close", "mapping_type": "pressed"}]},
        [18, 16], [W - 26, 6])),
    named("rule", rect([W, 1], [0, 27], BORDER, 0)),
    named("kofi", button("kofi", [kofi.WIDTH, kofi.HEIGHT], [W - 92 - kofi.WIDTH, 6.5],
                         [named(n, k) for n, k in kofi.nodes(rect, text, RAISED)])),
]



# ---------------------------------------------------------------- list screen
ROWS, ROW_H = 7, 23
rows = []
for r in range(ROWS):
    y = 58 + r * (ROW_H + 1)
    tile = [named("tile", rect([17, 17], [6, 3], WHITE, 4, binding=f"#r{r}_col")),
            named("glyph", text(f"#r{r}_glyph", [17, 10], [6, 7], [1, 1, 1, 1], 0.8, "center", True, f"#r{r}_gc")),
            named("name", text(f"#r{r}_name", [190, 10], [30, 7], TEXT, 0.9, "left", True)),
            named("info", text(f"#r{r}_info", [180, 10], [W - 24 - 186, 7.5], MUTED, 0.75, "right", True))]
    row = button(f"row_{r}", [W - 24, ROW_H], [12, y], [named("bg", rect([W - 24, ROW_H], [0, 0], CARD, 5))] + tile,
                 visible=f"#r{r}_on")
    rows.append(named(f"row{r}", row))

list_view = panel([W, H], [0, 0], [
    named("new", text_button("open_new", "+  New Waypoint", [110, 18], [12, 34], "accent")),
    named("sort", text_button("sort_next", "#sort", [92, 18], [W - 12 - 92 - 96, 34], "ghost", bound=True)),
    named("filter", text_button("filter_next", "#filter", [92, 18], [W - 12 - 92, 34], "ghost", bound=True)),
    *rows,
    named("empty", panel([W, 40], [0, 110], [
        named("t1", text("No waypoints in this dimension yet", [W, 12], [0, 0], TEXT, 0.95, "center")),
        named("t2", text("#empty_hint", [W, 10], [0, 16], MUTED, 0.8, "center", True))], visible="#empty")),
    named("prev", text_button("prev_page", "<", [24, 18], [12, H - 26], "ghost")),
    named("page", text("#page", [80, 10], [40, H - 21], MUTED, 0.8, "left", True)),
    named("next", text_button("next_page", ">", [24, 18], [120, H - 26], "ghost")),
    named("hint", text("Click a waypoint to edit it", [200, 10], [W - 212, H - 21], MUTED, 0.75, "right")),
], visible="#v_list")

# ---------------------------------------------------------------- form screen (new / edit)
PALETTE = [0xFF000000, 0xFF0000AA, 0xFF00AA00, 0xFF00AAAA, 0xFFAA0000, 0xFFAA00AA, 0xFFFFAA00, 0xFFAAAAAA,
         0xFF555555, 0xFF5555FF, 0xFF55FF55, 0xFF55FFFF, 0xFFFF0000, 0xFFFF55FF, 0xFFFFFF55, 0xFFFFFFFF]
def argb(c):
    return [round(((c >> 16) & 255) / 255, 3), round(((c >> 8) & 255) / 255, 3), round((c & 255) / 255, 3), 1]
GLYPHS = ["Ab", "★", "♥", "⌂", "⚑", "⚔", "☠", "✿", "☀", "☽", "♦", "♣", "♠", "♪", "✚", "▲", "●", "⚓", "☺", "⚡", "✖"]
SW, SH = 22, 16
swatches = []
for i in range(16):
    x = 220 + (i % 8) * (SW + 2)
    y = 84 + (i // 8) * (SH + 2)
    swatches.append(named(f"c{i}", button(f"color_{i}", [SW, SH], [x, y], [named("fill", rect([SW - 2, SH - 2], [1, 1], argb(PALETTE[i]), 3))], hover=False)))
IW, IH = 24, 18
icons = []
for i in range(21):
    x = 220 + (i % 7) * (IW + 2)
    y = 138 + (i // 7) * (IH + 2)
    icons.append(named(f"i{i}", button(f"icon_{i}", [IW, IH], [x, y], [
        named("bg", rect([IW, IH], [0, 0], RAISED, 4)),
        named("g", text(GLYPHS[i], [IW, 10], [0, 4.5], TEXT, 0.85, "center"))], hover=False)))

def stepper(label, minus, plus, value, x, y, w=190):
    return [
        named(f"{minus}_l", text(label, [70, 10], [x, y + 4], MUTED, 0.8)),
        named(minus, text_button(minus, "<", [18, 16], [x + 70, y], "ghost")),
        named(f"{minus}_v", text(value, [w - 70 - 40, 10], [x + 90, y + 4], TEXT, 0.85, "center", True)),
        named(plus, text_button(plus, ">", [18, 16], [x + w - 20, y], "ghost")),
    ]

form_view = panel([W, H], [0, 0], [
    named("back", text_button("back", "<  Back", [56, 18], [12, 34], "ghost")),
    named("title", text("#form_title", [200, 12], [76, 37], TEXT, 1.0, "left", True)),
    # Preview of the waypoint as it looks in the world.
    named("card", rect([196, 46], [12, 58], CARD, 6)),
    named("ptile", rect([34, 34], [18, 64], WHITE, 6, binding="#f_col")),
    named("pglyph", text("#f_glyph", [34, 12], [18, 75], WHITE, 1.15, "center", True, "#f_gc")),
    named("pname", text("#f_name", [150, 12], [58, 67], TEXT, 1.0, "left", True)),
    named("ppos", text("#f_pos", [150, 10], [58, 84], MUTED, 0.75, "left", True)),
    # The name box: click it, type, Enter.
    named("name_l", text("Name", [60, 10], [12, 114], MUTED, 0.8)),
    named("name_box", button("name_box", [126, 18], [82, 110], [
        named("bg", rect([126, 18], [0, 0], [0.135, 0.15, 0.185, 1], 5, binding="#f_name_box")),
        named("inner", rect([124, 16], [1, 1], [0.08, 0.09, 0.11, 1], 4)),
        named("t", text("#f_name", [118, 10], [5, 4.5], TEXT, 0.85, "left", True))])),
    named("name_tip", text("#f_name_tip", [196, 10], [12, 131], MUTED, 0.7, "left", True)),
    *stepper("Set", "set_prev", "set_next", "#f_set", 12, 146),
    named("hid_l", text("Hidden", [60, 10], [12, 170], MUTED, 0.8)),
    named("hid", pill("toggle_hidden", "#f_hidden", [82, 169])),
    # Edit only: nudge the position.
    named("nudge", panel([200, 40], [12, 188], [
        named("step", text_button("step_next", "#f_step", [60, 16], [0, 0], "ghost", bound=True)),
        named("xm", text_button("x_minus", "X-", [30, 16], [64, 0])), named("xp", text_button("x_plus", "X+", [30, 16], [96, 0])),
        named("zm", text_button("z_minus", "Z-", [30, 16], [128, 0])), named("zp", text_button("z_plus", "Z+", [30, 16], [160, 0])),
        named("ym", text_button("y_minus", "Down", [44, 16], [64, 19])), named("yp", text_button("y_plus", "Up", [44, 16], [112, 19])),
    ], visible="#f_edit")),
    named("color_l", text("#f_colname", [190, 10], [220, 62], MUTED, 0.8, "left", True)),
    named("color_ring", rect([SW + 2, SH + 2], [219, 83], WHITE, 4)),
    *swatches,
    named("icon_l", text("#f_iconname", [190, 10], [220, 124], MUTED, 0.8, "left", True)),
    named("icon_ring", rect([IW + 2, IH + 2], [219, 137], ACCENT, 5)),
    *icons,
    # Bottom bar.
    named("new_bar", panel([W, 22], [0, H - 28], [
        named("create", text_button("create", "Create here", [110, 20], [W - 12 - 110, 0], "accent")),
        named("cancel", text_button("back_cancel", "Cancel", [60, 20], [W - 12 - 110 - 66, 0], "ghost")),
    ], visible="#f_new")),
    named("edit_bar", panel([W, 22], [0, H - 28], [
        named("move", text_button("move_here", "Move here", [72, 20], [12, 0], "ghost")),
        named("copy", text_button("copy_dim", "#f_copy", [104, 20], [88, 0], "ghost", bound=True)),
        named("keep", text_button("keep", "Keep it", [56, 20], [196, 0], "ghost", visible="#f_keep")),
        named("delete", text_button("delete", "#f_delete", [72, 20], [W - 12 - 72 - 60, 0], "danger", bound=True)),
        named("done", text_button("done", "Done", [54, 20], [W - 12 - 54, 0], "accent")),
    ], visible="#f_edit")),
], visible="#v_form")
# The swatch and icon selection rings move with the selection.
form_view["controls"][[list(c)[0] for c in form_view["controls"]].index("color_ring")]["color_ring"]["bindings"] = [{"binding_name": "#f_color_ring", "binding_name_override": "#offset"}]
form_view["controls"][[list(c)[0] for c in form_view["controls"]].index("icon_ring")]["icon_ring"]["bindings"] = [{"binding_name": "#f_icon_ring", "binding_name_override": "#offset"}]

# ---------------------------------------------------------------- settings screen
def toggle_row(label, cid, binding, x, y):
    return [named(f"{cid}_l", text(label, [150, 10], [x, y + 3], TEXT, 0.85)), named(cid, pill(cid, binding, [x + 164, y + 1]))]

def key_row(label, cid, x, y):
    idx = control(cid, "keybind")
    return [named(f"{cid}_l", text(label, [110, 10], [x, y + 4], TEXT, 0.85)),
            named(cid, button(cid, [70, 16], [x + 116, y], [named("bg", rect([70, 16], [0, 0], RAISED, 5)),
                  named("k", text(f"#row_{idx}_value", [66, 10], [2, 3.5], TEXT, 0.85, "center", True))], kind="keybind"))]

L, R = 16, 218
settings_view = panel([W, H], [0, 0], [
    named("back", text_button("back_settings", "<  Back", [56, 18], [12, 34], "ghost")),
    named("title", text("Settings", [200, 12], [76, 37], TEXT, 1.0)),
    named("lcard", rect([196, 186], [12, 58], CARD, 6)),
    named("rcard", rect([194, 186], [214, 58], CARD, 6)),
    named("lh", text("In the world", [150, 10], [L, 63], ACCENT, 0.8)),
    *toggle_row("Show waypoints", "t_show", "#s_show", L, 76),
    *toggle_row("Show names", "t_names", "#s_names", L, 96),
    *stepper("Label size", "icon_minus", "icon_plus", "#s_icon", L, 116, 184),
    *toggle_row("Beams", "t_beams", "#s_beams", L, 140),
    *toggle_row("Beams through walls", "t_walls", "#s_walls", L, 160),
    *stepper("Beam size", "beam_minus", "beam_plus", "#s_beam", L, 180, 184),
    named("rh", text("Death point", [150, 10], [R, 63], ACCENT, 0.8)),
    *toggle_row("Mark where I die", "t_death", "#s_death", R, 76),
    *toggle_row("Remove when reached", "t_dclear", "#s_dclear", R, 96),
    named("kh", text("Keys", [150, 10], [R, 124], ACCENT, 0.8)),
    *key_row("Open menu", "menu_key", R, 136),
    *key_row("New waypoint here", "add_key", R, 156),
    named("credit", text("By Emil Chill & Claude AI", [W, 10], [0, H - 13], MUTED, 0.7, "center")),
], visible="#v_settings")

screen = {"type": "screen", "size": ["100%", "100%"], "controls": [
    named("window", place({"type": "panel", "controls": [
        named("shadow", rect([W + 6, H + 6], [-3, 0], [0, 0, 0, 0.35], 12)),
        named("edge", rect([W, H], [0, 0], BORDER, 10)),
        named("bg", rect([W - 2, H - 2], [1, 1], BG, 9)),
        *header,
        named("list", list_view),
        named("form", form_view),
        named("settings_view", settings_view),
        named("kofi_note", kofi.note(rect, text, W, H, lambda size, off, kids, vis: panel(
            size, off, [named(n, k) for n, k in kids], visible=vis))),
    ]}, [W, H], [0, 0], "center")),
]}
document = {"namespace": "waypoints", "menu": screen}
def tidy(v):
    if isinstance(v, float):
        return round(v, 3) if v != int(v) else int(v)
    if isinstance(v, list):
        return [tidy(x) for x in v]
    if isinstance(v, dict):
        return {k: tidy(x) for k, x in v.items()}
    return v
text_doc = json.dumps(tidy(document), separators=(",", ":"), ensure_ascii=False)
(ROOT / "surface.json").write_text(text_doc)
# The same document as the inside of a JSON string, so the mod never escapes 90 KB at runtime.
(ROOT / "surface.escaped").write_text(json.dumps(text_doc, ensure_ascii=False)[1:-1])

kinds = {"button": "Kind::Button", "keybind": "Kind::Keybind"}
lines = ["//! Generated by tools/gen_surface.py: the menu's backing controls, in the order the",
         "//! surface's buttons press them (`mod.control:<index>`).", "",
         "#[derive(Clone, Copy, Debug, PartialEq, Eq)]", "pub enum Kind {", "    Button,", "    Keybind,", "}", "",
         f"pub const CONTROLS: [(Kind, &str); {len(controls)}] = ["]
for kind, cid in controls:
    lines.append(f'    ({kinds[kind]}, "{cid}"),')
lines += ["];", "", "/// The document, already escaped to sit inside a JSON string.",
          'pub const DOCUMENT_ESCAPED: &str = include_str!("surface.escaped");', "",
          f"pub const WIDTH: u32 = {W};", f"pub const HEIGHT: u32 = {H};", ""]
(ROOT / "surface_controls.rs").write_text("\n".join(lines))
def count(n):
    c = 1
    for ch in n.get("controls", []):
        c += count(list(ch.values())[0])
    return c
print(len(controls), "controls,", len(text_doc), "bytes,", count(screen), "nodes")
