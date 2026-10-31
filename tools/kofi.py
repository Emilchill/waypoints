"""The Ko-fi button shared by the menus: a normal button with a small coffee cup and the words
in rainbow letters, and a note that shows after a press ("look at the game chat")."""

RAINBOW = [[1, 0.36, 0.36, 1], [1, 0.62, 0.25, 1], [1, 0.9, 0.3, 1], [0.45, 0.95, 0.4, 1],
           [0.3, 0.85, 1, 1], [0.45, 0.55, 1, 1], [0.8, 0.5, 1, 1]]
LABEL = "Support me on Ko-fi"
CELL = 4.3   # each letter gets the same cell, centred in it
SPACE = 2.4
WIDTH = 20 + sum(SPACE if c == " " else CELL for c in LABEL) + 6
HEIGHT = 15


def cup(rect, x, y):
    """A small cup with a heart, from rounded rectangles."""
    white, dark, red = [1, 1, 1, 1], [0.16, 0.18, 0.22, 1], [1, 0.36, 0.42, 1]
    return [
        ("cup_handle", rect([5, 5], [x + 6.5, y + 2], white, 2.5)),
        ("cup_hole", rect([2.4, 2.4], [x + 7.8, y + 3.3], dark, 1.2)),
        ("cup_body", rect([8, 7.5], [x, y + 1], white, 2)),
        ("cup_heart", rect([3.2, 3], [x + 2.4, y + 3.2], red, 1.5)),
    ]


def nodes(rect, text, bg):
    """The button's children: background, cup and one label per letter."""
    out = [("bg", rect([WIDTH, HEIGHT], [0, 0], bg, 5))]
    out += cup(rect, 6, 2.5)
    x, n = 18.0, 0
    for i, ch in enumerate(LABEL):
        if ch == " ":
            x += SPACE
            continue
        out.append((f"c{i}", text(ch, [CELL + 2, 10], [x - 1, 3], RAINBOW[n % len(RAINBOW)], 0.78, "center")))
        x += CELL
        n += 1
    return out


def note(rect, text, w, h, panel_visible):
    """The note under the button's press, centred at the bottom of the window."""
    nw = 220
    return panel_visible([nw, 18], [(w - nw) / 2, h - 40], [
        ("edge", rect([nw, 18], [0, 0], [0.29, 0.62, 1, 1], 6)),
        ("bg", rect([nw - 2, 16], [1, 1], [0.08, 0.1, 0.14, 1], 5)),
        ("t", text("Link sent! Look at the game chat (T).", [nw - 8, 10], [4, 4.5], [1, 1, 1, 1], 0.75, "center")),
    ], "#kofi_sent")
