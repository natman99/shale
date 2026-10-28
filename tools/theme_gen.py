"""Generate Shale's third-party built-in palettes.

This is the source of truth for `crates/core/src/theme/{tokyo_night,gruvbox,
nord,dracula,solarized,one,rose_pine}.rs`; edit the tables here and re-run
rather than editing those hexes by hand:

    python3 tools/theme_gen.py && cargo fmt --all

The modules it writes come out byte-identical to what is checked in (after
`cargo fmt`), so `git diff` on them is the check that this tool and the crate
agree.

Two things make it worth keeping around as well as the convenience:

* Every source colour here was read from the upstream palette file of that
  theme -- the URLs are in each module's rustdoc -- so a re-sync with upstream
  means editing one named colour, not hunting a hex through a Rust literal.
  Atom's One is defined in HSL and Solarized's tints are blends, so the tool
  carries the formulas as well as the values.
* Every role lands in `Theme.build`, which runs the same WCAG contrast checks
  as `theme::tests::the_built_in_palettes_are_readable` before emitting, and
  prints the ratios it measured. A palette that would fail that test is
  reported as a problem instead of being written silently, and roles given a
  candidate list take the first colour that clears the floor. One role is
  judged against the picker's backgrounds rather than the note's: `match_text`
  is the text a picker paints for the characters the query matched, so it has
  to read on `ui_bg` (an ordinary row) and on `ui_active_bg` (the highlighted
  one).

Shale's own `dark`/`light` and the Catppuccin flavours are hand-written in the
 crate (Catppuccin's hexes come from its own `palette.json`, and the role
 mapping follows its published style guide), so this tool does not write them.
"""

import os

FIELDS = [
    "bg", "fg", "cursor", "cursor_line_bg", "selection", "link",
    "link_unresolved", "dim", "heading", "quote", "accent", "tag", "spell",
    "code_bg", "code_block_bg", "code_text", "code_comment", "code_keyword",
    "code_string", "code_number", "code_function", "code_type", "code_const",
    "code_operator", "highlight_bg", "match_text", "table_line",
    "table_header_bg", "callout_bg", "ui_bg", "ui_fg", "ui_dim",
    "ui_status_bg", "ui_border", "ui_active_border", "ui_hover_bg",
    "ui_active_bg", "ui_field_bg", "ui_tab_bg", "ui_tab_active_bg",
    "popup_bg", "popup_border", "popup_hover_bg", "graph_node",
    "graph_node_active", "graph_edge", "graph_text", "callout_note",
    "callout_teal", "callout_question", "callout_warning", "callout_failure",
    "callout_danger",
]

# The floors in `theme::tests::the_built_in_palettes_are_readable`, as
# field -> (the role it is drawn over, floor). Most roles are judged against the
# note's background; the `code_*` tokens against the code block's, and the
# `ui_*` ones against the window's. A token colour only has to be
# distinguishable from the code around it -- `code_text` carries the body-text
# floor -- because every family's own syntax theme, not Shale, is the authority
# on what its tokens look like.
FLOORS = {
    "fg": ("bg", 4.5), "heading": ("bg", 4.5), "link": ("bg", 3.0),
    "tag": ("bg", 3.0), "quote": ("bg", 2.5), "dim": ("bg", 2.0),
    "link_unresolved": ("bg", 2.0), "spell": ("bg", 2.0),
    "code_text": ("code_block_bg", 4.0),
    "code_comment": ("code_block_bg", 2.0),
    "code_keyword": ("code_block_bg", 2.0),
    "code_string": ("code_block_bg", 2.0),
    "code_number": ("code_block_bg", 2.0),
    "code_function": ("code_block_bg", 2.0),
    "code_type": ("code_block_bg", 2.0),
    "code_const": ("code_block_bg", 2.0),
    "code_operator": ("code_block_bg", 2.0),
    "ui_fg": ("ui_bg", 4.5), "ui_dim": ("ui_bg", 2.0),
    "ui_active_border": ("ui_bg", 1.2), "ui_tab_bg": ("ui_bg", 1.1),
    "ui_tab_active_bg": ("ui_bg", 1.25),
    "popup_bg": ("bg", 1.1), "popup_border": ("popup_bg", 1.2),
    "graph_node": ("bg", 1.5), "graph_node_active": ("bg", 2.0),
    "graph_edge": ("bg", 1.1), "graph_text": ("bg", 2.5),
}

# A background role and the text role drawn on it, as (background, text, floor,
# what the reader is reading). A highlighted row gets a lower floor than body
# text: it is transient, its text is short, and it has to be *visible* against
# the surface it sits on, which for a palette whose text is a mid-tone is a pull
# in the other direction. `cursor_line_bg` is here as a pair against the note
# because what has to hold is that the band is visible at all; Solarized's own
# tone table rates body text on its secondary background as less than optimal,
# which is why that pair's floor is 4.0 rather than 4.5.
PAIRS = [
    ("ui_status_bg", "ui_fg", 4.5, "the status bar's text"),
    ("ui_field_bg", "ui_fg", 4.5, "what you type in the prompt field"),
    ("ui_tab_active_bg", "ui_fg", 3.0, "the open tab's label"),
    ("popup_bg", "ui_fg", 4.5, "the panel's text"),
    ("popup_hover_bg", "ui_fg", 3.5, "a panel row's text"),
    ("cursor_line_bg", "fg", 4.0, "text on the cursor's line"),
    ("bg", "cursor_line_bg", 1.05, "the cursor's line itself"),
]

# `match_text` is judged against the surfaces a picker draws its rows on, not
# against the note: an ordinary row sits on `ui_bg`, the highlighted row on
# `ui_active_bg`. The floors are the WCAG floor for short text over the plain
# row, and a lower one for the highlighted row, where the rest of the row is
# still body text at full contrast.
MATCH_BGS = ("ui_bg", "ui_active_bg")
MATCH_FLOORS = (3.0, 2.5)


def H(x):
    return int(x.lstrip("#"), 16)


def hx(v):
    return f"#{v:06x}"


def ch(v, sh):
    return (v >> sh) & 255


def over(base, colour, a):
    """`colour` drawn over `base` at alpha `a` (a `Palette` has no alpha)."""
    b, c = H(base) if isinstance(base, str) else base, (
        H(colour) if isinstance(colour, str) else colour)
    out = 0
    for sh in (16, 8, 0):
        out = (out << 8) | round(ch(b, sh) * (1 - a) + ch(c, sh) * a)
    return out


def hsl(h, s, l):
    """CSS HSL with s and l in percent, as 0xRRGGBB."""
    s, l = s / 100.0, l / 100.0
    c = (1 - abs(2 * l - 1)) * s
    x = c * (1 - abs((h / 60.0) % 2 - 1))
    m = l - c / 2
    r, g, b = {
        0: (c, x, 0), 1: (x, c, 0), 2: (0, c, x),
        3: (0, x, c), 4: (x, 0, c), 5: (c, 0, x),
    }[int(h // 60) % 6]
    return (round((r + m) * 255) << 16) | (round((g + m) * 255) << 8) | round((b + m) * 255)


def lift(h, s, l, amount):
    """LESS `lighten()`: lift the HSL lightness by whole percentage points."""
    return hsl(h, s, max(0.0, min(100.0, l + amount)))


def drop(h, s, l, amount):
    """LESS `darken()`."""
    return hsl(h, s, max(0.0, min(100.0, l - amount)))


def lum(rgb):
    def f(byte):
        c = byte / 255.0
        return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4
    return 0.2126 * f(ch(rgb, 16)) + 0.7152 * f(ch(rgb, 8)) + 0.0722 * f(rgb & 255)


def contrast(a, b):
    x, y = max(lum(a), lum(b)), min(lum(a), lum(b))
    return (x + 0.05) / (y + 0.05)


class Theme:
    """One palette: named upstream colours plus a role mapping over them."""

    def __init__(self, module, const, name, titles, colors, roles, notes=None):
        self.module, self.const, self.name = module, const, name
        self.titles = titles            # rustdoc lines for this const
        self.colors = {k: H(v) for k, v in colors.items()}
        self.roles = roles              # field -> spec
        self.notes = notes or {}        # field -> inline comment
        self.chosen = {}                # field -> (value, kind, label)

    def resolve(self, spec):
        """-> (value, kind, label). kind is "named" (an upstream colour, so no
        comment is needed) or "derived" (a blend or formula, which gets one).

        A spec is a colour name, ("lit", hex), ("val", value, label),
        ("blend", base, colour, alpha[, label]) or ("cand", [specs...])."""
        if isinstance(spec, str):
            return self.colors[spec], "named", spec
        kind = spec[0]
        if kind == "lit":
            return H(spec[1]), "derived", f"#{spec[1]}"
        if kind == "val":
            return spec[1], "derived", spec[2]
        if kind == "blend":
            _, base, colour, alpha = spec[:4]
            base_v = self.colors[base] if base in self.colors else H(base)
            colour_v = self.colors[colour] if colour in self.colors else H(colour)
            named = colour if colour in self.colors else hx(colour_v)
            label = spec[4] if len(spec) > 4 else (
                f"{named} {hx(colour_v)} at {round(alpha * 100)}% over "
                f"{hx(base_v)}")
            return over(base_v, colour_v, alpha), "derived", label
        raise SystemExit(f"unknown spec {spec!r}")

    def pick(self, field, spec):
        """Resolve a spec, trying candidates until one clears the role's floor.

        The background the role is judged against is the one `FLOORS` names for
        it, already settled by the time `build` reaches the role.
        """
        floor = None
        bg = 0
        if field in FLOORS:
            bg_field, floor = FLOORS[field]
            bg = self.chosen[bg_field][0]
        specs = spec[1] if isinstance(spec, tuple) and spec[0] == "cand" else [spec]
        tried = []
        for inner in specs:
            v, kind, label = self.resolve(inner)
            ratio = contrast(bg, v)
            tried.append(f"{label} ({ratio:.2f})")
            if floor is None or ratio > floor:
                return v, kind, label
        self.problems.append(
            f"{self.name}: {field} has no colour clearing {floor} on "
            f"{FLOORS[field][0]}: " + "; ".join(tried))
        v, kind, label = self.resolve(specs[0])
        return v, kind, label

    def pick_match(self, field, spec):
        """`match_text`, tried against both of the picker's backgrounds."""
        specs = spec[1] if isinstance(spec, tuple) and spec[0] == "cand" else [spec]
        tried = []
        for inner in specs:
            v, kind, label = self.resolve(inner)
            ratios = [contrast(self.chosen[f][0], v) for f in MATCH_BGS]
            tried.append(f"{label}({'/'.join(format(r, '.2f') for r in ratios)})")
            if all(r > floor for r, floor in zip(ratios, MATCH_FLOORS)):
                return v, kind, label
        self.problems.append(
            f"{self.name}: {field} has no colour clearing "
            f"{'/'.join(str(f) for f in MATCH_FLOORS)} on {MATCH_BGS}: "
            + "; ".join(tried))
        return self.resolve(specs[0])

    def build(self):
        self.problems = []
        self.chosen = {}
        self.chosen["bg"] = self.resolve(self.roles["bg"])
        # The picker's two surfaces are needed by `match_text`, which sits
        # earlier in FIELDS than they do, so they are settled up front. So is
        # `ui_bg`, which every `ui_*` role is judged against.
        for field in MATCH_BGS:
            self.chosen[field] = self.pick(field, self.roles[field])
        for field in FIELDS:
            if field in self.chosen:
                continue
            if field == "match_text":
                self.chosen[field] = self.pick_match(field, self.roles[field])
                continue
            self.chosen[field] = self.pick(field, self.roles[field])
        # The pairs are checked once everything is chosen, since either half may
        # sit anywhere in FIELDS.
        for bg_field, text_field, floor, what in PAIRS:
            ratio = contrast(self.chosen[bg_field][0], self.chosen[text_field][0])
            if ratio <= floor:
                self.problems.append(
                    f"{self.name}: {what} is {ratio:.2f} on "
                    f"{bg_field}, needs {floor}")
        return self.chosen

    def rust(self):
        self.build()
        out = ["/// " + line if line else "///" for line in self.titles]
        out.append(f"pub const {self.const}: Palette = Palette {{")
        for field in FIELDS:
            v, kind, label = self.chosen[field]
            note = self.notes.get(field)
            if note is None and kind == "derived":
                note = label
            tail = f"  // {note}" if note else ""
            out.append(f"    {field}: 0x{v:06x},{tail}")
        out.append("};")
        return "\n".join(out)

    def report(self):
        rows = [f"{field} {contrast(self.chosen[FLOORS[field][0]][0], self.chosen[field][0]):.2f}"
                for field in FLOORS]
        rows += [f"{b}/{t} " + format(
            contrast(self.chosen[b][0], self.chosen[t][0]), ".2f")
            for b, t, _, _ in PAIRS]
        return f"{self.name:<18} " + " ".join(rows)


def header(title, body, palettes):
    lines = [f"//! {title}"]
    lines += [f"//! {b}" if b else "//!" for b in body]
    lines += ["", "use super::Palette;", ""]
    return "\n".join(lines) + "\n" + "\n\n".join(palettes) + "\n"


# ---------------------------------------------------------------- Tokyo Night
# Named as in folke/tokyonight.nvim `lua/tokyonight/colors/{night,storm}.lua`.
TN = {
    "blue": "7aa2f7", "blue0": "3d59a1", "blue1": "2ac3de", "cyan": "7dcfff",
    "comment": "565f89", "dark3": "545c7e", "dark5": "737aa2", "fg": "c0caf5",
    "fg_dark": "a9b1d6", "fg_gutter": "3b4261", "green": "9ece6a",
    "green1": "73daca", "magenta": "bb9af7", "orange": "ff9e64",
    "purple": "9d7cd8", "red": "f7768e", "teal": "1abc9c", "yellow": "e0af68",
    "sel": "515c7e", "link": "6183bb",
}
# The day variant, from themes/tokyo-night-light-color-theme.json, named here by
# what the theme uses it for.
TNL = {
    "bg": "e6e7ed", "fg": "343b59", "fg_strong": "363c4d", "quote": "4e5579",
    "comment": "888b94", "muted": "707280", "link": "2959aa", "green": "385f0d",
    "deep_cyan": "0f4b6e", "purple": "65359d", "teal": "33635c",
    "yellow": "8f5e15", "orange": "965027", "red": "942f2f", "ln": "9da0ab",
    "line_hl": "dcdee3", "chrome": "d6d8df", "deep": "c1c2c7",
    "hover": "e1e2e8", "active": "e6e7ed", "sel": "acb0bf", "code_bg": "dadce3",
}


def tokyo_roles():
    return {
        "bg": "bg", "fg": "fg", "cursor": "cyan",
        "selection": ("blend", "bg", "sel", 0.30),
        "link": "link",
        "link_unresolved": ("cand", ["dark3", "dark5"]),
        "dim": "comment",
        "heading": "fg",
        "quote": "fg_dark",
        "accent": "magenta",
        "tag": ("cand", ["green", "green1"]),
        "code_bg": "bg_highlight",
        "code_block_bg": "bg_dark",
        "highlight_bg": ("blend", "bg", "yellow", 0.25),
        # Tokyo Night's orange: the palette's loudest accent, which is what a
        # query's matched characters have to be next to ordinary row text.
        "match_text": ("cand", ["orange", "yellow", "magenta"]),
        "table_line": "fg_gutter",
        "table_header_bg": "bg_highlight",
        "callout_bg": "bg_highlight",
        "ui_bg": "bg_dark",
        "ui_status_bg": "bg_dark1",
        "ui_border": "fg_gutter",
        "ui_hover_bg": "bg_highlight",
        "ui_active_bg": "fg_gutter",
        "callout_note": "blue",
        "callout_teal": "teal",
        "callout_question": "purple",
        "callout_warning": "yellow",
        "callout_failure": "orange",
        "callout_danger": "red",
        # Tokyo Night's own token colours, as its README names them: magenta
        # keywords, green strings, orange constants, blue functions, the brighter
        # blue for types, cyan for constants and markup.
        "cursor_line_bg": ("blend", "bg", "sel", 0.25,
                           "the selection shade again, more faintly: a band"),
        "spell": "red",
        "code_text": "fg",
        "code_comment": "comment",
        "code_keyword": "magenta",
        "code_string": "green",
        "code_number": "orange",
        "code_function": "blue",
        "code_type": "blue1",
        "code_const": "cyan",
        "code_operator": "fg_dark",
        "ui_fg": "fg",
        "ui_dim": "fg_dark",
        "ui_field_bg": "bg_highlight",
        "ui_active_border": "blue",
        "ui_tab_bg": "bg_highlight",
        "ui_tab_active_bg": "sel",
        # The raised pane reads against the default variant's background but not
        # against Storm's lighter one, which is a hair brighter than its own
        # highlight, so Storm falls back to the gutter shade.
        "popup_bg": ("cand", ["bg_highlight", "fg_gutter"]),
        # The gutter shade is the theme's rule colour; Storm's panel fell back
        # to that same shade, so its rule steps up to the brighter gutter grey.
        "popup_border": ("cand", ["fg_gutter", "dark5"]),
        "popup_hover_bg": ("blend", "bg_highlight", "fg", 0.12),
        "graph_node": "blue",
        "graph_node_active": "cyan",
        "graph_edge": "fg_gutter",
        "graph_text": "fg_dark",
    }


# --------------------------------------------------------------------- Gruvbox
# Gruvbox's own relative names, bound as upstream binds them in each mode.
GB_DARK = {
    "bg0": "282828", "bg1": "3c3836", "bg2": "504945", "bg3": "665c54",
    "bg4": "7c6f64", "gray": "928374",
    "fg0": "fbf1c7", "fg1": "ebdbb2", "fg2": "d5c4a1", "fg3": "bdae93",
    "fg4": "a89984",
    "red": "fb4934", "green": "b8bb26", "yellow": "fabd2f", "blue": "83a598",
    "purple": "d3869b", "aqua": "8ec07c", "orange": "fe8019",
}
GB_LIGHT = {
    "bg0": "fbf1c7", "bg1": "ebdbb2", "bg2": "d5c4a1", "bg3": "bdae93",
    "bg4": "a89984", "gray": "928374",
    "fg0": "282828", "fg1": "3c3836", "fg2": "504945", "fg3": "665c54",
    "fg4": "7c6f64",
    "red": "9d0006", "green": "79740e", "yellow": "b57614", "blue": "076678",
    "purple": "8f3f71", "aqua": "427b58", "orange": "af3a03",
}


def gruvbox_roles():
    return {
        "bg": "bg0",                # `Normal` upstream is fg1 on bg0
        "fg": "fg1",
        "cursor": "yellow",         # the colour gruvbox gives CursorLineNr
        "selection": "bg3",         # `Visual`'s background
        "link": "blue",
        "link_unresolved": "bg4",
        "dim": "bg4",               # `LineNr`
        "heading": "fg0",
        "quote": ("cand", ["gray", "fg2"]),
        "accent": "purple",
        "tag": "green",
        "code_bg": "bg2",
        "code_block_bg": "bg1",
        "highlight_bg": ("blend", "bg0", "yellow", 0.25),
        # Yellow in dark mode, orange in light: gruvbox re-binds these as its
        # `bright` and `faded` accents, and only the one its mode carries stays
        # legible on a highlighted row.
        "match_text": ("cand", ["yellow", "orange"]),
        "table_line": "bg3",
        "table_header_bg": "bg1",
        "callout_bg": "bg1",
        "ui_bg": "bg1",
        "ui_status_bg": "bg2",
        "ui_border": "bg3",
        "ui_hover_bg": "bg2",
        "ui_active_bg": "bg3",
        "callout_note": "blue",
        "callout_teal": "aqua",
        "callout_question": "purple",
        "callout_warning": "yellow",
        "callout_failure": "orange",
        "callout_danger": "red",
        # Gruvbox's own highlight groups: `Comment` gray, `String` and
        # `Function` green, `Number` and `Constant` purple, `Type` yellow,
        # `Identifier` blue, `Special` aqua, `CursorLine` bg1. It has seven
        # accents for eight token roles, so strings and functions share green
        # and constants and numbers share purple, exactly as upstream does.
        "cursor_line_bg": "bg1",
        "spell": "red",
        "code_text": "fg1",
        "code_comment": "gray",
        "code_keyword": "blue",
        "code_string": "green",
        "code_number": "purple",
        "code_function": "green",
        "code_type": "yellow",
        "code_const": "purple",
        "code_operator": "aqua",
        "ui_fg": "fg1",
        # fg4 is the faintest text in dark mode and too faint on the chrome's
        # own pane, so dark mode takes gray and light mode keeps fg4.
        "ui_dim": ("cand", ["fg4", "gray"]),
        "ui_field_bg": "bg2",
        "ui_active_border": "yellow",
        "ui_tab_bg": "bg2",
        "ui_tab_active_bg": "bg3",
        "popup_bg": "bg1",
        "popup_border": "bg3",
        "popup_hover_bg": ("blend", "bg1", "fg1", 0.12),
        "graph_node": "blue",
        "graph_node_active": "orange",
        "graph_edge": "bg3",
        "graph_text": "fg3",
    }


# ------------------------------------------------------------------------- Nord
NORD = {
    "nord0": "2e3440", "nord1": "3b4252", "nord2": "434c5e", "nord3": "4c566a",
    "nord4": "d8dee9", "nord5": "e5e9f0", "nord6": "eceff4",
    "nord7": "8fbcbb", "nord8": "88c0d0", "nord9": "81a1c1", "nord10": "5e81ac",
    "nord11": "bf616a", "nord12": "d08770", "nord13": "ebcb8b",
    "nord14": "a3be8c", "nord15": "b48ead",
}

# ---------------------------------------------------------------------- Dracula
DRACULA = {
    "bg": "282a36", "current_line": "44475a", "fg": "f8f8f2",
    "comment": "6272a4", "cyan": "8be9fd", "green": "50fa7b",
    "orange": "ffb86c", "pink": "ff79c6", "purple": "bd93f9", "red": "ff5555",
    "yellow": "f1fa8c",
}

# -------------------------------------------------------------------- Solarized
SOLARIZED = {
    "base03": "002b36", "base02": "073642", "base01": "586e75",
    "base00": "657b83", "base0": "839496", "base1": "93a1a1",
    "base2": "eee8d5", "base3": "fdf6e3",
    "yellow": "b58900", "orange": "cb4b16", "red": "dc322f",
    "magenta": "d33682", "violet": "6c71c4", "blue": "268bd2",
    "cyan": "2aa198", "green": "859900",
}

# -------------------------------------------------------------------- Rosé Pine
ROSE_PINE = {
    "base": "191724", "surface": "1f1d2e", "overlay": "26233a",
    "muted": "6e6a86", "subtle": "908caa", "text": "e0def4",
    "love": "eb6f92", "gold": "f6c177", "rose": "ebbcba", "pine": "31748f",
    "foam": "9ccfd8", "iris": "c4a7e7",
}
ROSE_PINE_MOON = dict(ROSE_PINE, base="232136", surface="2a273f",
                      overlay="393552", rose="ea9a97", pine="3e8fb0")
ROSE_PINE_DAWN = {
    "base": "faf4ed", "surface": "fffaf3", "overlay": "f2e9e1",
    "muted": "9893a5", "subtle": "797593", "text": "464261",
    "love": "b4637a", "gold": "ea9d34", "rose": "d7827e", "pine": "286983",
    "foam": "56949f", "iris": "907aa9",
}


def rose_pine_roles(dawn):
    # In dawn, `surface` is the lighter shade and `overlay` the deeper one,
    # while the other two variants put `overlay` deepest. Dawn's raised panes
    # are therefore surface and its recessed ones overlay.
    return {
        "bg": "base", "fg": "text", "cursor": "gold", "selection": "overlay",
        "link": ("cand", ["pine", "foam", "iris"]),
        "link_unresolved": "muted",
        "dim": "muted",
        "heading": "text",
        "quote": "subtle",
        "accent": "love",
        "tag": ("cand", ["iris", "foam", "rose"]),
        "code_bg": "overlay",
        "code_block_bg": ("overlay" if dawn else "surface"),
        "highlight_bg": ("blend", "base", "gold", 0.25),
        # Gold is Rosé Pine's highlight colour, and dawn's is too pale to read
        # as text, so dawn falls back to its pine.
        "match_text": ("cand", ["gold", "pine", "iris"]),
        "table_line": "overlay",
        "table_header_bg": ("surface" if dawn else "overlay"),
        "callout_bg": ("surface" if dawn else "overlay"),
        "ui_bg": "surface",
        "ui_status_bg": "overlay",
        "ui_border": "overlay",
        "ui_hover_bg": "overlay",
        "ui_active_bg": ("blend", "overlay", "text", 0.12),
        "callout_note": "pine",
        "callout_teal": "foam",
        "callout_question": "iris",
        "callout_warning": "gold",
        "callout_failure": "rose",
        "callout_danger": "love",
        # Rosé Pine names its colours for jobs, and the tokens follow them: iris
        # for variables and imports, pine for strings, gold for numbers, love
        # for functions, foam for types, rose for constants.
        "cursor_line_bg": ("overlay" if dawn else "surface"),
        "spell": "love",
        "code_text": "text",
        "code_comment": "muted",
        "code_keyword": "iris",
        "code_string": "pine",
        "code_number": ("cand", ["gold", "rose", "pine"]),
        "code_function": "love",
        "code_type": "foam",
        "code_const": "rose",
        "code_operator": "subtle",
        "ui_fg": "text",
        "ui_dim": "subtle",
        "ui_field_bg": ("overlay" if dawn else "surface"),
        "ui_active_border": "rose",
        # The three backgrounds are within a whisker of each other, so a tab and
        # a panel are the recessed pane with the text colour blended into it.
        "ui_tab_bg": ("cand", ["overlay", ("blend", "overlay", "text", 0.10)]),
        "ui_tab_active_bg": ("blend", "overlay", "text", 0.22),
        "popup_bg": ("cand", ["overlay", ("blend", "overlay", "text", 0.10),
                              ("blend", "overlay", "text", 0.20)]),
        "popup_border": "rose",
        "popup_hover_bg": ("blend", "overlay", "text", 0.12),
        "graph_node": "iris",
        "graph_node_active": "gold",
        "graph_edge": ("cand", ["overlay",
                                ("blend", "overlay", "text", 0.15)]),
        "graph_text": "subtle",
    }


# ----------------------------------------------------------------- Atom's One
def one(hue, sat, bright, mono, hues, light_variant):
    """The One themes: every colour is the upstream HSL definition evaluated."""
    h, s, l = hue, sat, bright
    m1, m3, m2 = mono                      # (saturation, lightness) at 1-3
    cyan, blue, purple, green, red, red2, orange, orange2 = hues
    accent = hsl(h, 100, 66)               # @syntax-accent
    # The UI chrome the *-ui packages derive from those same values.
    if light_variant:
        gutter = drop(h, s, l, 36)            # darken(@syntax-bg, 36%)
        sel, code_bg = drop(h, s, l, 8), drop(h, s, l, 8)
        panes, deep = drop(h, s, l, 4), drop(h, s, l, 6)   # levels 1 and 3
        hover, active = drop(h, s, l, 12), drop(h, s, l, 9)
        heading = drop(h, m1[0], m1[1], 12)
        guide = 0.20
        renamed, added = hsl(208, 100, 66), hsl(132, 60, 44)
        modified = hsl(40, 90, 50)
        # LESS cannot lighten a 98% background, so every raised surface here is
        # a lowered one; the tab and panel shades are steps down the same ramp.
        cursorline = drop(h, s, l, 5)
        tab, tab_active = drop(h, s, l, 12), drop(h, s, l, 20)
        panel = drop(h, s, l, 12)
    else:
        gutter = drop(h, m1[0], m1[1], 26)     # darken(@syntax-fg, 26%)
        sel, code_bg = lift(h, s, l, 10), lift(h, s, l, 8)
        panes, deep = lift(h, s, l, 4), drop(h, s, l, 3)
        hover, active = lift(h, s, l - 3, 6), lift(h, s, l - 3, 3)
        heading = lift(h, m1[0], m1[1], 20)
        guide = 0.15
        renamed, added = hsl(208, 100, 60), hsl(150, 60, 54)
        modified = hsl(40, 60, 70)
        cursorline = lift(h, s, l, 6)
        tab, tab_active = panes, lift(h, s, l, 10)
        panel = panes
    fg = hsl(h, m1[0], m1[1])
    guide_v = over(hsl(h, s, l), fg, guide)
    # A query's matched characters take the theme's warm end: `modified` (a soft
    # amber) reads best in dark mode, `orange` (its darkened twin) in light,
    # where the softer one cannot clear a raised row. Tried in that order.
    warm = [
        ("val", modified, "the theme's modified colour"),
        ("val", orange2, "orange's lighter twin"),
        ("val", orange, "orange"),
    ]
    if light_variant:
        warm.reverse()
    return {
        "bg": ("val", hsl(h, s, l), f"hsl({h}, {s}%, {l}%)"),
        "fg": ("val", fg, f"mono-1: hsl({h}, {m1[0]}%, {m1[1]}%)"),
        "cursor": ("val", accent, "@syntax-accent, the theme's caret colour"),
        "selection": ("val", sel, "the selection shade the theme derives"),
        "link": ("val", blue, "blue"),
        "link_unresolved": ("val", gutter, "@syntax-gutter"),
        "dim": ("val", hsl(h, m3[0], m3[1]), "mono-3, the comment colour"),
        "heading": ("val", heading, "@text-color-highlight"),
        "quote": ("val", hsl(h, m2[0], m2[1]), "mono-2"),
        "accent": ("val", accent, "the theme's accent, which it also uses as the caret"),
        "tag": ("val", green, "green"),
        "code_bg": ("val", code_bg, "one step off the note background"),
        "code_block_bg": ("val", deep, "the app-level pane shade"),
        "highlight_bg": ("val", over(hsl(h, s, l), orange2, 0.25),
                         f"{hx(orange2)} at 25% over the background"),
        # The theme's `modified` colour in dark mode (a soft amber) and its
        # `orange` in light, where the softer one cannot clear a raised row.
        "match_text": ("cand", warm),
        "table_line": ("val", guide_v,
                       f"the {round(guide * 100)}% foreground fade the theme uses for guides"),
        "table_header_bg": ("val", panes, "the raised panel shade"),
        "callout_bg": ("val", panes, "the raised panel shade"),
        "ui_bg": ("val", deep, "the app background"),
        "ui_status_bg": ("val", deep, "the app background"),
        "ui_border": ("val", guide_v, "a rule is what the theme draws as a guide"),
        "ui_hover_bg": ("val", hover, None),
        "ui_active_bg": ("val", active, None),
        "callout_note": ("val", renamed, "the theme's renamed colour"),
        "callout_teal": ("val", added, "the theme's added colour"),
        "callout_question": ("val", purple, "purple"),
        "callout_warning": ("val", modified, "the theme's modified colour"),
        "callout_failure": ("val", orange, "orange"),
        "callout_danger": ("val", red, "red"),
        # One Dark's eight hues are its syntax theme: purple keywords, green
        # strings, orange numbers and constants, blue functions, yellow types,
        # cyan operators, mono-3 comments. The UI shades come from the *-ui
        # packages: the app levels, the raised panel, the selection.
        "cursor_line_bg": ("val", cursorline,
                           "the note background one step off: a band"),
        "spell": ("val", red, "red"),
        "code_text": ("val", fg, "mono-1"),
        "code_comment": ("val", hsl(h, m3[0], m3[1]), "mono-3, the comment colour"),
        "code_keyword": ("val", purple, "purple"),
        "code_string": ("val", green, "green"),
        "code_number": ("val", orange, "orange"),
        "code_function": ("val", blue, "blue"),
        "code_type": ("cand", [("val", modified, "the theme's modified colour"),
                               ("val", orange, "orange"),
                               ("val", orange2, "orange's lighter twin")]),
        "code_const": ("cand", [("val", orange2, "orange's lighter twin"),
                                ("val", red2, "red's darker twin")]),
        "code_operator": ("val", cyan, "cyan"),
        "ui_fg": ("val", fg, "mono-1"),
        "ui_dim": ("val", hsl(h, m2[0], m2[1]), "mono-2"),
        "ui_field_bg": ("val", code_bg, "one step off the note background"),
        "ui_active_border": ("val", accent, "the theme's accent"),
        "ui_tab_bg": ("val", tab, None),
        "ui_tab_active_bg": ("val", tab_active, None),
        "popup_bg": ("val", panel, None),
        "popup_border": ("val", over(panel, fg, 0.35),
                        f"{hx(fg)} at 35% over the panel"),
        "popup_hover_bg": ("val", over(panel, fg, 0.12),
                           f"the panel with mono-1 at 12%"),
        "graph_node": ("val", purple, "purple"),
        "graph_node_active": ("cand", [("val", modified, "the theme's modified colour"),
                                       ("val", orange, "orange"),
                                       ("val", red2, "red's darker twin")]),
        "graph_edge": ("val", guide_v, "a rule is what the theme draws as a guide"),
        # mono-2 is 2.3:1 on the note, under the floor for a label, so the
        # graph's labels take the body colour.
        "graph_text": ("val", fg, "mono-1"),
    }


# ------------------------------------------------------------- the theme table
THEMES = [
    Theme("tokyo_night", "NIGHT", "tokyo-night", [
        "Tokyo Night: the default variant, on background `#1a1b26`.",
        "",
        "Named as in `night.lua`; the deeper panes (`bg_dark`, `bg_dark1`), the",
        "gutter shade and the link colour come from the official VS Code theme,",
        "which also supplies the 30% alpha the selection is drawn at. Tokyo Night",
        "has no cursor colour of its own, so the block takes the pale `cyan`.",
    ], dict(TN, bg="1a1b26", bg_dark="16161e", bg_dark1="0c0e14",
            bg_highlight="292e42"),
        dict(tokyo_roles(), bg="bg", selection=("blend", "bg", "sel", 0.30),
             ui_status_bg=("lit", "0c0e14")),
        notes={
            "link": "`textLink.foreground` of the official theme",
            "selection": "#515c7e at 30%, the alpha that theme applies",
            "ui_status_bg": "`bg_dark1`, the palette's deepest pane",
        }),
    Theme("tokyo_night", "STORM", "tokyo-night-storm", [
        "Tokyo Night Storm: the same palette on a lighter, less blue background,",
        "`#24283b`, for people who find the default too dark.",
        "",
        "Everything is from `storm.lua`; its token colours are the same as the",
        "default variant's, only the three background shades differ.",
    ], dict(TN, bg="24283b", bg_dark="1f2335", bg_dark1="1b1e2d",
            bg_highlight="292e42"),
        dict(tokyo_roles(), selection=("blend", "bg", "sel", 0.30),
             ui_status_bg=("lit", "1b1e2d")),
        notes={
            "link": "`textLink.foreground` of the official theme",
            "selection": "#515c7e at 30%, the alpha the official theme applies",
            "ui_status_bg": "`bg_dark1`, the palette's deepest pane",
        }),
    Theme("tokyo_night", "LIGHT", "tokyo-night-light", [
        "Tokyo Night Light: the day variant, on background `#e6e7ed`.",
        "",
        "tokyonight.nvim derives its day style by inverting the dark palette, so",
        "these are the literal values of the official VS Code theme",
        "`tokyo-night-light-color-theme.json`: `#d6d8df` chrome, `#c1c2c7`",
        "borders, and the theme's own link, comment and token shades.",
    ], TNL, {
        "bg": "bg", "fg": "fg", "cursor": "deep_cyan",
        "selection": ("blend", "bg", "sel", 0.25,
                      "the theme's selection shade at the 25% alpha it applies"),
        "link": "link",
        "link_unresolved": "muted",
        "dim": "comment",
        "heading": "fg_strong",
        "quote": "quote",
        "accent": "purple",
        "tag": ("cand", ["green", "teal"]),
        "code_bg": "code_bg",
        "code_block_bg": "chrome",
        "highlight_bg": ("blend", "bg", "yellow", 0.25),
        # The light theme's orange, the dark variants' orange darkened: its
        # yellow is only 3.9:1 on the theme's own chrome.
        "match_text": ("cand", ["orange", "yellow", "link"]),
        "table_line": "deep",
        "table_header_bg": "line_hl",
        "callout_bg": "line_hl",
        "ui_bg": "chrome",
        "ui_status_bg": "deep",
        "ui_border": "deep",
        "ui_hover_bg": "hover",
        "ui_active_bg": "active",
        "callout_note": "deep_cyan",
        "callout_teal": "teal",
        "callout_question": "purple",
        "callout_warning": "yellow",
        "callout_failure": "orange",
        "callout_danger": "red",
        # The light theme's own token shades, taken from what the VS Code file
        # paints each token with; `line_hl` is the theme's own line highlight.
        "cursor_line_bg": "line_hl",
        "spell": "red",
        "code_text": "fg",
        "code_comment": "comment",
        "code_keyword": "purple",
        "code_string": "green",
        "code_number": "orange",
        "code_function": "link",
        "code_type": "teal",
        "code_const": "deep_cyan",
        "code_operator": "quote",
        "ui_fg": "fg_strong",
        "ui_dim": "muted",
        "ui_field_bg": "code_bg",
        "ui_active_border": "link",
        # The theme's chrome shades are all within a hair of each other, so the
        # tabs are the chrome with its own foreground blended into it.
        "ui_tab_bg": ("blend", "chrome", "fg", 0.14),
        "ui_tab_active_bg": ("blend", "chrome", "fg", 0.26),
        "popup_bg": "deep",
        "popup_border": "ln",
        "popup_hover_bg": ("blend", "deep", "fg", 0.12),
        "graph_node": "link",
        "graph_node_active": "deep_cyan",
        "graph_edge": "ln",
        "graph_text": "quote",
    }, notes={
        "cursor": "the theme's dark cyan, since its cyan has no light twin",
        "tag": "the light theme's green: a tag is not a link here",
        "ui_status_bg": "the deepest shade of the theme's chrome",
    }),
    Theme("gruvbox", "DARK", "gruvbox-dark", [
        "Gruvbox dark: `Normal`'s fg1 `#ebdbb2` on bg0 `#282828`.",
        "",
        "The role mapping follows the upstream highlight groups: `Visual` is bg3,",
        "`LineNr` bg4, `Comment` gray, and `CursorLineNr` yellow. Dark mode binds",
        "the saturated `bright` accents; light mode the `faded` ones.",
    ], GB_DARK, gruvbox_roles()),
    Theme("gruvbox", "LIGHT", "gruvbox-light", [
        "Gruvbox light: the same relative names re-bound, as upstream re-binds",
        "them in light mode, onto light0 `#fbf1c7` with the `faded` accents.",
    ], GB_LIGHT, gruvbox_roles(), notes={"bg": "gruvbox's light0"}),
    Theme("nord", "PALETTE", "nord", [
        "Nord: arctic blue-grey, on Polar Night nord0 `#2e3440`.",
        "",
        "Nord has no colour lighter than nord4 for quiet text, and nord3 on",
        "nord0 measures 1.7:1, which is fine for a punctuation mark beside body",
        "text and not fine for a paragraph or a link. The three quiet text roles",
        "therefore use nord3 lifted toward nord4, by increasing amounts.",
    ], NORD, {
        "bg": "nord0", "fg": "nord4", "cursor": "nord8",
        "selection": "nord3",
        "link": ("cand", ["nord9", "nord10"]),
        "link_unresolved": ("cand", [("blend", "nord3", "nord4", a) for a in
                                     (0.30, 0.45, 0.60)]),
        "dim": ("cand", [("blend", "nord3", "nord4", a) for a in
                         (0.35, 0.50, 0.65)]),
        "heading": "nord6",
        "quote": ("blend", "nord3", "nord4", 0.45,
                  "nord3 lifted 45% toward nord4: too faint alone for a paragraph"),
        "accent": "nord8",
        "tag": "nord14",
        "code_bg": "nord2",
        "code_block_bg": "nord1",
        "highlight_bg": ("blend", "nord0", "nord13", 0.25),
        # nord8 is Nord's own emphasis colour: the frost the palette puts on the
        # things it wants read.
        "match_text": ("cand", ["nord8", "nord7", "nord13"]),
        "table_line": "nord2",
        "table_header_bg": "nord1",
        "callout_bg": "nord1",
        "ui_bg": "nord1",
        "ui_status_bg": "nord1",
        "ui_border": "nord3",
        "ui_hover_bg": "nord2",
        "ui_active_bg": "nord3",
        "callout_note": "nord9",
        "callout_teal": "nord7",
        "callout_question": "nord15",
        "callout_warning": "nord13",
        "callout_failure": "nord12",
        "callout_danger": "nord11",
        # Nord says what its four groups are for, and the tokens follow that:
        # Frost (nord7-10) for the parts of a program a reader navigates --
        # nord8 functions, nord7 types -- and Aurora (nord11-15) for the parts
        # it marks -- nord15 keywords, nord14 strings, nord13 numbers, nord12
        # constants. Comments take nord3 lifted toward nord4, as the quiet text
        # roles here already do: nord3 alone is 1.7:1 on its own pane.
        "cursor_line_bg": ("blend", "nord0", "nord4", 0.08,
                           "nord4 barely into nord0: a band, not a surface"),
        "spell": "nord11",
        "code_text": "nord4",
        "code_comment": ("cand", [("blend", "nord3", "nord4", a) for a in
                                  (0.35, 0.50, 0.65)]),
        "code_keyword": "nord15",
        "code_string": "nord14",
        "code_number": "nord13",
        "code_function": "nord8",
        "code_type": "nord7",
        "code_const": "nord12",
        "code_operator": ("cand", ["nord10", "nord9"]),
        "ui_fg": "nord4",
        "ui_dim": ("cand", [("blend", "nord3", "nord4", a) for a in
                            (0.45, 0.60)]),
        "ui_field_bg": "nord2",
        "ui_active_border": "nord8",
        "ui_tab_bg": "nord3",
        "ui_tab_active_bg": ("blend", "nord3", "nord4", 0.20),
        "popup_bg": "nord2",
        "popup_border": ("cand", ["nord3",
                                  ("blend", "nord2", "nord4", 0.35)]),
        "popup_hover_bg": ("blend", "nord2", "nord4", 0.12),
        "graph_node": "nord8",
        "graph_node_active": "nord13",
        "graph_edge": "nord2",
        "graph_text": ("cand", [("blend", "nord3", "nord4", a) for a in
                                (0.45, 0.60)]),
    }, notes={"ui_status_bg": "Nord has no pane deeper than nord1"}),
    Theme("dracula", "PALETTE", "dracula", [
        "Dracula: the dark pastel palette, on Background `#282a36`.",
        "",
        "Dracula's own CSS sets the caret to the foreground colour, which Shale",
        "would draw invisibly, so the cursor block takes Cyan. Comment does duty",
        "for punctuation, quotes and unresolved links, as it does upstream.",
    ], DRACULA, {
        "bg": "bg", "fg": "fg", "cursor": "cyan",
        "selection": "current_line",
        "link": ("cand", ["purple", "cyan"]),
        "link_unresolved": ("cand", ["comment", "current_line"]),
        "dim": "comment", "heading": "fg", "quote": "comment",
        "accent": "pink",
        "tag": ("cand", ["green", "cyan"]),
        "code_bg": "current_line",
        "code_block_bg": ("blend", "bg", "000000", 0.12,
                          "Background darkened 12%: Dracula has no deeper pane"),
        "highlight_bg": ("blend", "bg", "yellow", 0.25),
        "match_text": ("cand", ["orange", "yellow"]),
        "table_line": ("blend", "bg", "comment", 0.40),
        "table_header_bg": ("blend", "bg", "comment", 0.30),
        "callout_bg": ("blend", "bg", "comment", 0.30),
        "ui_bg": "bg",
        "ui_status_bg": "current_line",
        "ui_border": ("blend", "bg", "comment", 0.40),
        "ui_hover_bg": "current_line",
        "ui_active_bg": ("blend", "current_line", "comment", 0.30),
        "callout_note": "cyan",
        "callout_teal": ("cand", ["green", "cyan"]),
        "callout_question": "purple",
        "callout_warning": "yellow",
        "callout_failure": "orange",
        "callout_danger": "red",
        # Dracula publishes what each accent is for, and the tokens follow it:
        # Purple keywords, Green strings, Orange numbers, Cyan functions,
        # Yellow types, Pink constants, Red operators. Current Line is already
        # the cursor-line shade; everything between the eleven colours is a
        # blend of Comment into Background, commented as such.
        "cursor_line_bg": "current_line",
        "spell": "red",
        "code_text": "fg",
        "code_comment": "comment",
        "code_keyword": "purple",
        "code_string": "green",
        "code_number": "orange",
        "code_function": "cyan",
        "code_type": "yellow",
        "code_const": "pink",
        "code_operator": "red",
        "ui_fg": "fg",
        # Comment is the palette's only quiet colour and it is too faint on a
        # raised tab, so the hint colour is Comment lifted toward Foreground.
        "ui_dim": ("blend", "comment", "fg", 0.35,
                   "Comment lifted 35% toward Foreground"),
        "ui_field_bg": "current_line",
        "ui_active_border": "pink",
        "ui_tab_bg": "current_line",
        "ui_tab_active_bg": ("blend", "current_line", "comment", 0.45),
        "popup_bg": ("blend", "bg", "comment", 0.35),
        "popup_border": ("blend", "bg", "comment", 0.60),
        "popup_hover_bg": ("blend", "bg", "comment", 0.55),
        "graph_node": "purple",
        "graph_node_active": "orange",
        "graph_edge": ("blend", "bg", "comment", 0.40),
        "graph_text": "comment",
    }),
    Theme("solarized", "DARK", "solarized-dark", [
        "Solarized dark: base0 `#839496` on base03 `#002b36`.",
        "",
        "Eight neutrals do not cover as many surfaces as Shale has roles, so the",
        "code span, the table header and the status bar's rows are blends of one",
        "base shade into another. And the tone table rates body text on the",
        "secondary background as less than optimal reading, so the status bar --",
        "it draws body text -- stays on the note's own background.",
    ], SOLARIZED, {
        "bg": "base03", "fg": "base0", "cursor": "base2",
        "selection": "base02",
        "link": ("cand", ["blue", "cyan"]),
        "link_unresolved": "base01",
        "dim": "base01",
        "heading": "base1",
        "quote": ("cand", ["base00", "base01"]),
        "accent": "magenta",
        "tag": ("cand", ["green", "cyan"]),
        "code_bg": ("blend", "base02", "base01", 0.25,
                    "base02 with base01 lifted into it; base02 alone is the code block"),
        "code_block_bg": "base02",
        "highlight_bg": ("blend", "base03", "yellow", 0.25),
        # Solarized's yellow is the palette's highlight, and it just clears the
        # floor for a highlighted row; the lighter blends are what it falls back
        # to if that row's background is ever made brighter.
        "match_text": ("cand", ["yellow", ("blend", "yellow", "base2", 0.30),
                                ("blend", "yellow", "base1", 0.20)]),
        "table_line": "base01",
        "table_header_bg": ("blend", "base02", "base01", 0.25),
        "callout_bg": ("blend", "base02", "base01", 0.25),
        "ui_bg": "base02",
        "ui_status_bg": "base03",
        "ui_border": "base01",
        "ui_hover_bg": ("blend", "base02", "base01", 0.25),
        "ui_active_bg": ("blend", "base02", "base01", 0.50),
        "callout_note": "blue",
        "callout_teal": "cyan",
        "callout_question": "violet",
        "callout_warning": "yellow",
        "callout_failure": "orange",
        "callout_danger": "red",
        # Solarized's eight accents carry meanings, and the tokens follow them:
        # green statements, cyan literals, orange numbers, blue identifiers and
        # functions, yellow types, violet the reserved constant, red operators.
        # base02 is the palette's own "background line", and its text ladder
        # steps one rung brighter on a secondary background -- base1 rather than
        # base0 for the window -- which is what its tone table says to do.
        "cursor_line_bg": "base02",
        "spell": "red",
        "code_text": "base0",
        "code_comment": "base01",
        "code_keyword": "green",
        "code_string": ("cand", ["cyan", "blue"]),
        "code_number": "orange",
        "code_function": "blue",
        "code_type": "yellow",
        "code_const": ("cand", ["violet", "magenta"]),
        "code_operator": ("cand", ["red", "orange"]),
        "ui_fg": "base1",
        "ui_dim": "base01",
        "ui_field_bg": "base03",
        "ui_active_border": "blue",
        "ui_tab_bg": ("blend", "base02", "base01", 0.25),
        "ui_tab_active_bg": ("blend", "base02", "base01", 0.35),
        # base02 is the palette's raised pane; the panel has to read against the
        # note, and this is the shade Solarized itself puts text on. The field
        # you type in is cut from the note's own background.
        "popup_bg": "base02",
        "popup_border": "base01",
        "popup_hover_bg": ("blend", "base02", "base1", 0.10),
        "graph_node": "blue",
        "graph_node_active": "orange",
        "graph_edge": "base01",
        "graph_text": "base00",
    }),
    Theme("solarized", "LIGHT", "solarized-light", [
        "Solarized light: the ladder upside down, base01 `#586e75` on base3",
        "`#fdf6e3`.",
        "",
        "Its green and cyan are too faint against a cream background to carry a",
        "tag, so a tag takes blue -- the same conclusion Catppuccin's style guide",
        "reaches for its light flavour.",
    ], SOLARIZED, {
        "bg": "base3", "fg": "base01", "cursor": "base02",
        "selection": "base2",
        "link": ("cand", ["blue", "cyan"]),
        "link_unresolved": ("cand", ["base1", "base01"]),
        "dim": ("cand", ["base1", "base01"]),
        "heading": "base02",
        "quote": ("cand", ["base00", "base1"]),
        "accent": "magenta",
        "tag": ("cand", ["green", "cyan", "blue"]),
        "code_bg": "base2",
        "code_block_bg": "base2",
        "highlight_bg": ("blend", "base3", "yellow", 0.25),
        # Its yellow is not dark enough against the cream ladder to read as a
        # highlight, so a query's characters take orange; violet is the fallback
        # if the ladder's raised row is ever made fainter.
        "match_text": ("cand", ["orange", "yellow", "violet"]),
        "table_line": ("cand", ["base1", "base01"]),
        "table_header_bg": ("blend", "base2", "base1", 0.30),
        "callout_bg": ("blend", "base2", "base1", 0.30),
        "ui_bg": "base2",
        "ui_status_bg": "base3",
        "ui_border": ("cand", ["base1", "base01"]),
        "ui_hover_bg": "base2",
        "ui_active_bg": ("blend", "base2", "base1", 0.45),
        "callout_note": "blue",
        "callout_teal": "cyan",
        "callout_question": "violet",
        "callout_warning": "yellow",
        "callout_failure": "orange",
        "callout_danger": "red",
        # The same accent meanings as the dark mode, with the neutral ladder
        # upside down: base01 is the body text, base02 the tone for text on a
        # secondary background, and base1 the faintest rule.
        "cursor_line_bg": ("blend", "base3", "base01", 0.08,
                           "base01 barely into base3: a band, not a surface"),
        "spell": "red",
        "code_text": "base01",
        "code_comment": ("cand", ["base1", "base01"]),
        "code_keyword": "green",
        "code_string": ("cand", ["cyan", "blue"]),
        "code_number": "orange",
        "code_function": "blue",
        "code_type": "yellow",
        "code_const": ("cand", ["violet", "magenta"]),
        "code_operator": ("cand", ["red", "orange"]),
        "ui_fg": "base02",
        "ui_dim": ("cand", ["base1", "base01"]),
        "ui_field_bg": "base3",
        "ui_active_border": "blue",
        "ui_tab_bg": ("blend", "base2", "base01", 0.30),
        "ui_tab_active_bg": ("blend", "base2", "base01", 0.45),
        "popup_bg": ("blend", "base2", "base01", 0.12),
        "popup_border": "base1",
        "popup_hover_bg": ("blend", "base2", "base01", 0.35),
        "graph_node": "blue",
        "graph_node_active": "orange",
        "graph_edge": ("cand", ["base1", "base01"]),
        "graph_text": "base00",
    }),
    Theme("one", "DARK", "one-dark", [
        "One Dark: Atom's palette, on `hsl(220, 13%, 18%)` = `#282c34`.",
        "",
        "Atom defines One in HSL rather than in hex, so every value here is that",
        "definition evaluated: the mono-1..3 text ramp, the eight hues, and the",
        "shades LESS derives from them -- `@syntax-gutter`, the",
        "`lighten(@syntax-bg, 10%)` selection, the `@level-3-color` app",
        "background, `@text-color-highlight` headings, and the accent that",
        "`@syntax-cursor-color` names as the caret. The callout accents are the",
        "theme's own git colours: renamed, added, modified and removed.",
    ], {}, one(220, 13, 18,
               [(14, 71), (10, 40), (9, 55)],
               [hsl(187, 47, 55), hsl(207, 82, 66), hsl(286, 60, 67),
                hsl(95, 38, 62), hsl(355, 65, 65), hsl(5, 48, 51),
                hsl(29, 54, 61), hsl(39, 67, 69)],
               False)),
    Theme("one", "LIGHT", "one-light", [
        "One Light: the same definitions inverted, on `hsl(230, 1%, 98%)`.",
        "",
        "LESS cannot lighten a 98%-light background, so where the dark UI lifts a",
        "surface the light one lowers it; every other value is the formula.",
    ], {}, one(230, 1, 98,
               [(8, 24), (4, 64), (6, 44)],
               [hsl(198, 99, 37), hsl(221, 87, 60), hsl(301, 63, 40),
                hsl(119, 34, 47), hsl(5, 74, 59), hsl(344, 84, 43),
                hsl(41, 99, 30), hsl(41, 99, 38)],
               True)),
    Theme("rose_pine", "MAIN", "rose-pine", [
        "Rosé Pine, upstream codename `main`, on base `#191724`.",
        "",
        "The palette names its colours for roles, and the mapping follows them:",
        "base and surface are the two backgrounds, overlay the subtle elements,",
        "muted and subtle the quiet text, text the body, gold highlights, pine",
        "links, iris tags, foam code and notifications, love the interactive pink.",
        "",
        "One upstream advice is ignored: `rose` is meant for borders, which makes a",
        "table grid shout, so grid lines and window rules take `overlay`.",
    ], ROSE_PINE, rose_pine_roles(False)),
    Theme("rose_pine", "MOON", "rose-pine-moon", [
        "Rosé Pine Moon, upstream codename `moon`: the same palette on a warmer,",
        "lighter base, with a brighter pine and rose.",
    ], ROSE_PINE_MOON, rose_pine_roles(False)),
    Theme("rose_pine", "DAWN", "rose-pine-dawn", [
        "Rosé Pine Dawn, upstream codename `dawn`: the light variant, on base",
        "`#faf4ed`.",
        "",
        "Dawn inverts the shades as well as the hues -- its raised pane `surface`",
        "is lighter than its base and `overlay` is the recessed one -- so the",
        "panes swap roles relative to the other two variants.",
    ], ROSE_PINE_DAWN, rose_pine_roles(True)),
]

DOCS = {
    "tokyo_night": (
        "Tokyo Night, in its three official variants.",
        [
            "Tokyo Night is enkia's palette for working at night",
            "(https://github.com/tokyo-night/tokyo-night-vscode-theme): blue-purple sky,",
            "cyan and green for the things that matter, plus one daylight variant.",
            "Shale ships all three -- `tokyo-night`, `tokyo-night-storm` (a lighter,",
            "less blue background) and `tokyo-night-light`.",
            "",
            "Two upstream files describe this family. The VS Code themes above carry",
            "the workbench shades and the alpha the selection is drawn at;",
            "tokyonight.nvim (https://github.com/folke/tokyonight.nvim) names the dark",
            "palette's colours the way its users say them (`fg_dark`, `bg_highlight`,",
            "`fg_gutter`) and gives it the two deeper panes. Its day style is computed",
            "by inversion, so the light palette is taken from the VS Code file",
            "instead. Both upstreams are MIT-licensed, so the hexes may live in this",
            "Apache-2.0 crate.",
        ]),
    "gruvbox": (
        "Gruvbox, in its dark and light variants.",
        [
            "Gruvbox (https://github.com/morhetz/gruvbox, MIT) is a retro-groove palette:",
            "sixteen saturated accents over a warm grey-brown background ramp. Its",
            "source works through relative names -- bg0..bg4 from background to border,",
            "fg0..fg4 from brightest text to faintest, and the seven accents -- and then",
            "re-binds those names for light mode, where the ramp turns to cream and the",
            "accents become the darker `faded` family. The palettes here follow those",
            "bindings, including gruvbox's own choice of bg3 for a selection and bg4 for",
            "line numbers.",
        ]),
    "nord": (
        "Nord, the arctic palette.",
        [
            "Nord (https://nordtheme.com, https://github.com/nordtheme/nord, MIT) is 16",
            "colours in four groups, and its own source says what each group is for:",
            "Polar Night (nord0-3) is the interface and its shaded layers, Snow Storm",
            "(nord4-6) carries text, Frost (nord7-10) accents user-interface parts, and",
            "Aurora (nord11-15) marks things -- errors, warm warnings, attention,",
            "success, and the purple that means magic. Shale's roles line up with that.",
        ]),
    "dracula": (
        "Dracula, the dark palette.",
        [
            "Dracula (https://draculatheme.com,",
            "https://github.com/dracula/dracula-theme, MIT) publishes eleven colours: a",
            "background, a current-line shade that doubles as the selection, a",
            "foreground, a comment, and seven accents. There is no third surface in it,",
            "so the chrome reuses Background and Current Line and the handful of roles",
            "in between are blends of Comment into Background, commented as such. Every",
            "accent in the palette has a job here.",
        ]),
    "solarized": (
        "Solarized, in its dark and light variants.",
        [
            "Solarized (https://ethanschoonover.com/solarized/, the palette mirrored at",
            "https://github.com/altercation/solarized) is built as a ladder: base03 and",
            "base02 are backgrounds, base01 the comments and faintest text, base00 and",
            "base0 the secondary and primary body text on a dark background (base01 and",
            "base00 on a light one), and base1..base3 the same values upside down. Its",
            "accents carry meaning -- red errors, orange and yellow warnings, green and",
            "cyan emphasis, blue information, violet and magenta reserved -- which is how",
            "they land in Shale's callout roles.",
            "",
            "Eight neutrals do not cover as many surfaces as Shale has roles, so the code",
            "span and the table header are blends of one base shade into another rather",
            "than invented colours, and each one says so. The tone table rates body text",
            "on the secondary background as less than optimal reading, so the status bar",
            "stays on the note's own background.",
        ]),
    "one": (
        "Atom's One Dark and One Light.",
        [
            "One (https://github.com/atom/one-dark-syntax, one-light-syntax and the",
            "matching *-ui packages, MIT) is the palette Atom shipped for a decade and",
            "much of the editor world still defaults to. Unusually, it is defined in",
            "HSL: a hue, saturation and brightness, a three-step monochrome ramp, eight",
            "hues, and shades LESS derives from them -- the gutter, the guide, the",
            "selection, the app background levels, and an accent it also uses as the",
            "caret. This module evaluates those definitions, so the hexes here are the",
            "upstream ones and not a port's approximation.",
        ]),
    "rose_pine": (
        "Rosé Pine, in its three variants.",
        [
            "Rosé Pine (https://github.com/rose-pine/palette, MIT) publishes one palette",
            "file with three variants whose upstream codenames are `main`, `moon` and",
            "`dawn`; Shale calls them `rose-pine`, `rose-pine-moon` and `rose-pine-dawn`,",
            "which is the spelling that palette's README recommends for file names.",
            "",
            "Its colour names describe roles -- base and surface are backgrounds,",
            "overlay the subtle elements, muted and subtle the quiet text, text the body,",
            "love the interactive pink, gold highlights and emphasis, rose borders and",
            "muted accents, pine links, foam code and notifications, iris tags and",
            "underlines -- which map almost directly onto Shale's roles.",
        ]),
}

if __name__ == "__main__":
    here = os.path.dirname(os.path.abspath(__file__))
    outdir = os.path.join(os.path.dirname(here), "crates", "core", "src", "theme")
    by_module = {}
    for t in THEMES:
        by_module.setdefault(t.module, []).append(t.rust())
    problems = [p for t in THEMES for p in t.problems]
    for mod, blocks in by_module.items():
        title, body = DOCS[mod]
        path = os.path.join(outdir, mod + ".rs")
        open(path, "w").write(header(title, body, blocks))
        print(f"wrote {path} ({len(blocks)} palettes)")
    print()
    for t in THEMES:
        if t.problems:
            print("FAIL", t.report())
            for p in t.problems:
                print("   ", p)
        else:
            print("ok  ", t.report())
    print(f"\n{len(problems)} problem(s)")
