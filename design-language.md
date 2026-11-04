# Shale's design language

The grammar every surface draws from: color, spacing, shape, state, elevation,
type, motion, glyphs, and phrasing. Written from the ruled design round of
**2026-11-03** (all items ✅ unless marked otherwise). When you add or restyle a
surface — a tab, a popup, the review strip, a `.shq` table, the graph — this is
the file it must obey; when this file and a surface disagree, the surface is the
bug.

Status legend: ✅ ruled · ❄️ parked · ⏳ implementation owed.

> Writeback note: these rounds were ruled while the app-track agent was
> mid-flight, so they live only here for now. The brainstorm.md/roadmap.md
> hooks (P6.9 fold-in, the new palette roles feeding P2.5b, the glyph policy)
> landed 2026-11-03 — see §16. The §11 sweep itself landed the same day.

## 1. Color — the Palette is the only source

Already true, kept: every paint site reads a named role from
`shale_core::theme::Palette` (0xRRGGBB roles; 20 built-in palettes;
`.shale/themes/*.toml` overrides). No surface hardcodes a color; a new role is
added to the Palette, never inlined at the paint site.

**✅ New roles (landed 2026-11-03, `96a6224`):**

| Role | Carries |
|---|---|
| `diff_add` / `diff_delete` / `diff_changed` | the review strip's proposed hunks, and P2.5b's inline tints (they land before both) |
| `agent` | "the agent is / has been here": review badges, edit provenance, graph node rings — one color for the whole presence cluster |
| `ui_disabled` | unavailable controls: dimmed keybinds, inactive chips |
| `graph_ghost` / `graph_tag` / `graph_attachment` | node kinds named, not derived from neighbors' colors |

**✅ Rule: roles are additive.** A new role is *optional* in TOML themes, so the 20 built-ins
and every user theme keep loading untouched, and **the built-ins name them explicitly in the
same pass**.

**Amended as built (2026-11-03).** The first draft said a new role gets a *computed* fallback
(`agent` → `accent`, `diff_add` → `callout_note`). What landed instead names every role in
every built-in and lets a user theme inherit from its `base`: a theme that names none of the
new roles gets sensible values for free, which is the whole point of the rule. A derivation
chain was rejected because it makes `theme export` ambiguous (which `agent` do you write out —
the inherited one or the derived one?) and makes the §10 audit judge colours nobody chose. The
one case that differs: a theme that overrides `accent` but not `agent` keeps the base's
`agent`. Naming `agent` is the way to opt in.

## 2. Spacing — one scale, in `style.rs`

**✅ (landed 2026-11-03, `413dda6`)** `SPACE_1..4 = 4 / 8 / 12 / 16` px, siblings of
`CHIP_RADIUS` in `crates/app/src/style.rs`. Chrome divs use them via gpui padding/margin;
custom-painted surfaces take them as arguments to `paint_quad` the way `CHIP_RADIUS` is
already passed. A surface that needs a gap picks the nearest step; an off-scale number in a
diff is a review comment. They are gpui's own quarter-rems (`p_1` is `SPACE_1`), so a div
written with a helper already obeys the scale — the names are for the constants, and for
saying which step a surface means. Two constants that were off the scale came onto it.

## 3. Shape — one radius

Already true: `CHIP_RADIUS = 6px` is the single rounding decision for
gpui-styled chrome. **✅ (landed 2026-11-03, `413dda6`)** the custom-painted surfaces take the
same radius — this is roadmap **P6.9**, folded into this doc's first sweep (§11) rather than a
standalone pass.

P6.9 is **one module deciding each surface**, not one radius everywhere. The checker's panel
was the miss: it used the raised triple's colours and none of its shape, so it sat square next
to a rounded picker; its rows, and the review strip's, are chips like a file-tree row and a
picker row. Deliberately square, and written down in `style.rs` so it is not re-litigated: the
caret and the selection (a vim block cursor sits on a cell), a 1px rule or the 2px callout bar
(a 6px radius is bigger than the thing it would round), and the note's full-width bands — a
fenced block is one quad per row, so rounding each row is a stack of pills, and rounding the
block's ends needs the layout pass to say which row of the block is first, which it does not
say today.

## 4. State grammar — three states, three roles, everywhere

**✅ (sweep landed 2026-11-03, `a1b744e`)** Every interactive row/pane obeys exactly:

| State | Role |
|---|---|
| hover | `ui_hover_bg` |
| selected | `ui_active_bg` |
| keyboard focus | `ui_active_border` (the rule; `ui_border` when unfocused) |

Applies to: file tree, picker rows, tabs, status chips, graph nodes
(background→node tint equivalents), the review strip's rows, future `.shq`
tables. A surface inventing its own hover color is the drift this table kills.

**Resolved against §5:** a *raised* panel's row takes `popup_hover_bg`, not `ui_hover_bg`.
§5's raised triple is `popup_bg` + `popup_border` + its own dim step, and a panel over the
note is judged against the panel, not the window. §4's `ui_hover_bg` is for rows on chrome.

What the sweep changed: the file tree's keyboard cursor was borrowing the **hover background**,
so the row a vault edit would act on looked like the row the pointer was over — it carries the
focus rule now, on every row (transparent unless selected), because a border that appeared on
one row pushes every row below it down. `ui_active_border` was painted by nothing at all; it is
the focused field's edge and the graph canvas's rule. The graph's filter chips were drawing an
off chip in `ui_dim` (a label's role) and an on chip in `ui_field_bg` (a field's role).

## 5. Elevation — border + bg-shift + dim, no shadows

**✅** gpui gives us no shadows; we don't fake them. Depth is expressed by
exactly three tools, in this combination:

- **raised** (popups, picker panel, review strip, checker panel): `popup_bg` +
  `popup_border` + one dim level below it (the dimming system's `fade`);
- **surface** (chrome, tree, tabs): `ui_bg` + `ui_border`;
- **note** (the buffer): `bg`, no border.

Every floating thing uses the raised triple identically. No new elevation
level without amending this section.

**Closed 2026-11-03.** Both surfaces that were listed here as disobeying now obey. The
picker's panel is `popup_bg` + `popup_border` (`b7fd9da`), and the review strip is too: it was
`ui_status_bg` with no border of its own, which made it read as a second status bar rather than
as the one thing on screen asking for a decision. Its rows moved to `popup_hover_bg` with it, the
same ruling the picker's rows follow — a raised panel's row is judged against the panel, not the
window. The §10 floors already covered the pair (`a panel row's text` and `a panel row's path`
against `popup_bg`), so no contrast table moved. The strip is in-flow rather than anchored, so it
takes the border and the bg-shift; the dim step of the triple is about content behind a floating
panel and does not apply to a bar that pushes the note up. A paint test pins the pair, because
nothing else in the suite looked at the strip's colours and §5 is a rule rather than a preference.

**A raised panel that can hold the keyboard says so on its rule.** The strip's rule is
`popup_border` while the note has the keys and `accent` while the strip has them. It has no cursor
and no selection of its own to show, and its marked row is drawn in both states, so there was
nothing to tell the user whether `y` was about to accept a hunk or yank a character — the two-layer
keymap makes that the single most consequential thing on screen. Hue only, never a second border
weight: the panel keeps its line and §5's three levels stay three. Accent is already how this app
says "the keyboard is here" (the tree's selected row, the status bar's save state), so this adds no
role. A paint test pins both states.

## 6. Type — the note's scale exists; the chrome's is now shared

Already true: editor base size + proportional heading scale + one `LINE_RATIO`;
mono-family resolution with real bold/italic faces. **✅ (landed 2026-11-03, `9260c4d`)** chrome
text sizes (the status bar's `text_xs`, picker rows, tab labels) join `style.rs` as named
constants — the values in use today become the set; no per-surface sizes. They are gpui's
`text_xs`/`text_sm`/`text_lg` spelled as rems, so the values did not move and one module now
holds them. Mode words (`NORMAL/INSERT/…`) stay text: vim users read the word, and color
already carries it.

## 7. Motion — tokens, and a contract

**✅ (landed 2026-11-03, `4432a76`)** durations and easings as constants in `style.rs` (one fast
~120ms, one slow ~240ms to start). Contract: **chrome does not animate except toasts** — no
fades on tab switches, no sliding panels. The graph keeps its own ruled idle/animation contract
(M5a, PROJECT.md); this doc does not touch it. A new animation needs a token from this file and
a reason.

The audit found nothing to remove: no chrome surface animates at all today. The tokens exist
for the exception, and they are `#[allow(dead_code)]` until a toast uses one.

## 8. Glyphs and icons — ruled policy

**✅** Shale is a text-first app and stays one, with one named exception class.

- **Formalized unicode set (landed 2026-11-03, `4432a76`)** — the marks already in use move into
  a named set in `style.rs` so they stop being per-site characters: `▸`/`▾` (tree disclosure),
  `●` (dirty, drawn in `accent`), `×` (close). New chrome marks come from this set or propose
  an addition.
- **Drawn-glyph exception class (⏳ callout marks landed 2026-11-03; graph node kinds and the
  agent mark still owed)** — custom-painted surfaces may draw simple vector marks
  where unicode is weak or color-only information needs a second channel: **callout types** (a
  mark on the left bar), **graph node kinds** (shape/glyph per kind so ghost/tag/attachment read
  without color), and the **agent presence mark** (one glyph carrying `agent` color across
  review badges, provenance, and graph rings). The set landed; these stay owed, and §11 puts the
  set first so each mark arrives with the surface that needs it. The graph's four node kinds are
  at least four *colours* now, which `contrast::audit` enforces.

  **Callout marks (landed 2026-11-03).** The second channel turned out to be load-bearing rather
  than nice: the palette maps fourteen callout kinds onto ten colours, and four pairs share one
  outright (`info`/`todo`, `abstract`/`example`, `tip`/`success`, `danger`/`bug`) — with live
  preview hiding the `[!type]` header, a reader who cannot separate those two colours has no way
  at all to tell a `warning` from a `failure`. So the invariant is stated in code, not just here:
  **kinds that share a palette colour never share a mark**, tested against both built-ins because
  the two palettes do not collide in the same places. One pair is exempt by design — `other` is
  drawn as a `note`, title text and all.

  The mark is **hung in the note's gutter**, not on the bar: the bar is 2 px and carries
  nothing, and widening it would cost the callout's text 12 px and reach into the wrap model.
  The gutter is `style::GUTTER` — a named column, applied as the editor scroller's left
  padding, because `overflow_scroll` clips to the scroller's own bounds and padding on an
  ancestor is outside the mask. **It is a shared column, not the callout mark's slot:** fold
  markers, line numbers, a changed-line rule and provenance marks all want this place, and
  each would otherwise rediscover the clipping rule and re-pick a width. A mark keeps
  `MARK_GUT` clear of the text; the size against the column is a test, not a hope. It is drawn
  once, on the box's header row.

  Shapes are Lucide's vocabulary (`triangle-alert`, `octagon`, `quote`) reduced to what survives
  at 8 px: Lucide's 24 px line art with a 2 px stroke is under a pixel at that size and gpui's
  stroker has no round caps, so these are filled where Lucide outlines, and outlined only where
  the outline *is* the distinction.

  **Palette follow-ups from the same finding.** `CalloutKind::Other` is now coloured
  `callout_note`, which is what its own doc always claimed; it had been taking `accent`, which in
  both built-ins is `link`'s colour, so a vault's `> [!shiplog]` wore the blue of an `info`. And
  `contrast::audit` now judges every callout accent twice — the bar against the box, the mark
  against the note — walked from `callout_accent` so a new kind is audited the moment it exists.
  Nothing audited callout colours before, which is how ten colours for fourteen kinds sat
  unnoticed.
- **Permanent exclusion** — buffer content is never icon-ified: task boxes,
  heading markers, list bullets, table pipes, link brackets stay visible text.
  That is the Raw-mode posture, not an oversight; iconing them means WYSIWYG,
  which is a different product.

## 9. Microcopy — the bar already has a voice; everything else joins it

**✅** The status line's existing voice is the convention: lowercase, terse, no
trailing period, plain declarative (`graph: building…`, `nothing is waiting for
review`, `the vault holds no notes yet`). Empty states and picker hints use the
same register. Keymap legends (`j/k hop · y accept · …`) render as boxed
**key-chips** (a `style.rs` treatment, not prose) wherever they appear.

**Landed 2026-11-03.** `style.rs` gained `key_chip` and `key_legend`: the key in a small
boxed rule, the meaning beside it in `ui_dim`, [`SPACE_1`] inside a pair and [`SPACE_2`]
between pairs. The review strip's legend was the only legend in the app and was prose
(`j/k hop · y accept · x reject · o note · za collapse`), which made `j/k` read as the start
of a label rather than as a key. A chip is a rule and a corner, **not a fill**: it delimits
text rather than claiming to sit above the surface, so it takes none of §5's elevation and
needs no contrast floor of its own — the key is judged against whatever the chip sits on,
exactly as the word beside it already is. The colours are arguments to the helper rather than
a `Palette` import, because this module owns the treatment and never decides which role a
surface takes; the strip is raised, so its chips take `popup_border` and follow the surface
they are on. The rule is `CHIP_RULE` (1.5 px) rather than the window's hairline: a 1 px box
around two characters at `TEXT_XS` reads as a smudge rather than as a key. Still a rule, so the
elevation levels stay three.

## 10. Accessibility — color-only information needs a second channel

**✅** Any meaning carried by color alone must gain a mark or shape: node kinds
and callout types via §8's drawn glyphs; dirty state already pairs `●` +
`accent`. **✅ (landed 2026-11-03, `6958252`)** a **contrast guard** asserts minimum contrast
per role-pair across all built-in palettes (e.g. `match_text` legible on both `ui_bg` and
`ui_active_bg`); it runs against user TOML themes at load and warns rather than rejects.

It is one table in `shale_core::theme::contrast`, used twice: the built-in test asserts `audit()`
comes back empty, and the app calls it when a palette is committed (startup, `:theme`, the
picker's enter) and puts the first finding on the status bar. Three kinds of finding, not one: a
colour too faint for its surface, two roles a reader must tell apart given one colour (the
graph's node kinds), and a disabled control reading louder than a label.

## 11. Implementation sweep, in order (⏳)

**Status 2026-11-03: steps 1–6 landed** (`96a6224`, `6958252`, `a1b744e`, `413dda6`,
`4432a76`, plus §6's type constants in `9260c4d`). PROJECT.md thread 44 records what each step
found, and the three things the audit turned up that are **not** fixed: the picker and review
strip's elevation (§5), the unpainted `ui_tab_*` roles (§4 lists tabs; the palette cut roles for
them and the tabs obey §4 instead), and §8's drawn marks.

**Follow-up 2026-11-03:** two of those three are closed. The picker is raised now --
`popup_bg`, `popup_border`, `popup_hover_bg` for its chosen row -- and the tab bar paints
`ui_tab_bg` / `ui_tab_active_bg` instead of leaving the roles reserved (`b7fd9da`). Retuning
Solarized to clear the two new floors turned into a correction: a light Solarized's `ui_dim`
was `#93a1a1`, which in that variant is the family's *background* grey, not its quiet-content
one. **The review strip's elevation is closed too** (§5); §8's drawn marks and §9's key-chips
are what is left open.

1. **Palette roles** (§1) + built-in values + TOML fallbacks — unblocks
   P2.5a's colors; small, core-side.
2. **Contrast guard test** (§10) — protects the roles as they land.
3. **State grammar sweep** (§4) — audit every surface against the table.
4. **Spacing scale + P6.9 rounding** (§2–3) — one pass over the custom painters.
5. **Motion tokens + chrome contract** (§7).
6. **Glyph formalization** (§8 set) — trivial; then the drawn marks
   (callouts, graph kinds, agent) as each surface needs them.

## 13. Identity beyond pixels — ruled 2026-11-03 (second round)

- **✅ Actor vocabulary.** One word for the agent everywhere: UI strings, MCP
  tool names, filters (`you / agent / both`), provenance copy (`edited by
  agent · 3m ago`), the review strip. Never "AI", "assistant", "bot". The
  agent-native identity is linguistic before it is visual; this is the parent
  rule for every future string.
- **✅ Keybinding taxonomy.** The mnemonic system is written down as grammar:
  `space <letter>` leaders for views/modes (`space g` graph), `g`-prefixed
  motion pairs (`gx` follow link), vim-native verbs kept verbatim (`za` fold).
  New features pick keys by this rule so they are predictable before they are
  learned. (Full enumeration lives with the keymap; this section rules that
  the taxonomy exists and new keys must obey it.)
- **✅ Units and relative time.** One format everywhere: `3m ago`, `N words`,
  `2 hunks`; dates short and unambiguous. Any new number in the UI takes the
  existing format, not a new one.
- **✅ Empty states.** Every empty surface gets exactly one line in the
  status bar's voice (`the vault holds no notes yet` is the register). No
  illustrations, no tip-splatter, no second line.
- **✅ Linux window identity** — **landed 2026-11-03** (`692800a`). `app_id = "shale"` on the
  window: `WM_CLASS` on X11, xdg `app_id` on Wayland, and `assets/shale.desktop`'s
  `StartupWMClass` matches it so the desktop can find the entry. The strata mark (four bedding
  lines of uneven thickness and inset) is defined twice and checked once: `identity.rs`
  rasterises it for the X11 window icon, `assets/shale.svg` is the same four rects, and a test
  compares them. Installing the entry stays a packaging step (`assets/README.md`); the app does
  not write into `~/.local/share`.
- **✅ Cursor shape per vim mode** — **spiked 2026-11-03**. gpui exposes only the *pointer*
  (`CursorStyle`, `Window::set_cursor_style(style, hitbox)`) — arrow/beam/hand/resize, no caret
  shapes, and nothing to gate. The caret is already Shale's own painted quad
  (`line/mod.rs`: a 1.5 px bar in insert, a block elsewhere), so per-mode shapes are entirely
  ours: swap `LineElement::insert: bool` for the mode and paint an underline band for visual.
  One field, two paint sites, no upstream work. Not built — it is mode-legibility, not identity,
  and it wants a look decided rather than guessed.
- **✅ Command-name grammar** — ruled before the registry grows: bare nouns for views (`:graph`), verbs for actions, dotted ids in the registry (`note.save`, `theme.change`). New commands follow it at P2.7/P6.3. `theme.export <name>` (`:theme export <name>`) is the first one built to the rule.
- **🔭 Strata-named palettes** (`shale`, `slate`, `silt`…) — the idea where
  the name finally pays rent; flavor, not scheduled. Door open: §1's additive
  roles make a new built-in family cheap.
- **❄️ Wordmark system** — one mark, three places (window icon, `:about`,
  README). Drawn once, after the window icon (§13) gives it a first home.
- **❄️ First-run experience** — empty-vault moment + discoverability path;
  waits for tabs/graph to settle.
- **❌ Scope guards (parent rules, do not re-pitch):** no sounds, no splash
  screen, no mascot, no startup animation. The chrome-doesn't-animate
  contract (§7) already implies the last one; these settle the rest.

## 15. Theme system — ruled 2026-11-03 (third round)

The spine is kept as-is: data palette, `<vault>/.shale/themes/<name>.toml`
partial overrides, `deny_unknown_fields` loud validation, picker preview that
never writes config.

- **✅ Deeper inheritance** — **landed 2026-11-03** (`32db0bb`). `base` may name another user
  theme, resolved by the same rule as `appearance.theme`; the chain of files already open catches
  a ring and the error spells it out (`a.toml -> b.toml -> c.toml -> a.toml`).
- **✅ `mode` metadata + follow-the-system** — **landed 2026-11-03**. Theme files may say
  `mode = "dark" | "light"`; absent, `Palette::mode()` reads it off the note background at the
  crossover where black and white text are equally readable (luminance 0.179). An **empty**
  `appearance.theme` (`theme = ""`) names no palette and follows the desktop: the XDG Desktop
  Portal's `color-scheme` on Linux, gpui's own reading of the platform setting elsewhere, dark
  when neither answers. The window opens dark and settles a moment later rather than holding the
  first frame for a D-Bus round trip, and choosing a theme by hand stops the following. Live
  following (reacting when the desktop changes mid-session) is not built. *(The auto pair —
  dark → `mocha`, light → `latte` — is the doc author's extension, in one function:
  `desktop::built_in_for`.)*
- **✅ Role vocabulary grows with §1.** The new roles (`diff_*`, `agent`, `ui_disabled`, `graph_ghost/tag/attachment`) land as `Option` fields in `ThemeFile` in the same commit as the Palette roles — 53 → 61 overridable keys, same optional-with-fallback rule.
- **✅ `theme export`** — **landed 2026-11-03**. `:theme export <name>` writes the palette *on
  screen* — not the name in the config — as 61 `#rrggbb` keys plus `mode`, and the file reads back
  as the same palette. A built-in's name is refused (the loader answers built-ins first, so the
  file would be dead) and an existing file is refused (burying an author's edits is the one thing
  an export must not do). `export` belongs to the command, so a vault theme literally named
  `export` is a picker row and a valid `appearance.theme` but not a `:` name.
- **✅ Layering policy (written rule, no code):** **theme = color;
  appearance = fonts/size; geometry (spacing, radius, motion) is not
  themeable.** Fonts stay in `config.toml` because they are machine-dependent
  (installed families; the mono-fallback resolver) and ergonomic, not mood.
  If one-click "full looks" are ever wanted, the shape is an *appearance
  preset* referencing theme + font + size — never font fields inside the
  theme file.
- **❄️ Computed colors** (`lighten(base.bg, 5%)`, alpha in the palette) —
  frozen; `base` + `theme export` cover the authoring burden. Door open if
  theme authors actually hit it.
- **✅ Contrast warnings on user themes.** The §10 guard's logic runs against
  a user theme at load and **warns** (status bar + `tracing::warn!`), never
  rejects — the logic already exists for built-ins; this is one call site.

## 16. ❄️ Parked, and the hooks owed elsewhere

- **Density/scale setting** (chrome-wide rem scaling): parked — real slog
  across custom painters, no demand yet. Door open: `style.rs` constants make
  it a multiplier someday.
- **Hooks owed when brainstorm.md/roadmap.md are free to touch** (other agent
  in flight at ruling time) — **landed 2026-11-03**: roadmap P6.9 points at §11.4 and is
  marked done for everything it can be; the §1 palette roles are a dependency line under P2.5a;
  this file is listed beside `graph-view-plan.md` in the roadmap's plan-file index. The
  brainstorm agent-presence cluster still owes its citation of §8's `agent` role and mark —
  that file is the other track's, and was left alone.
