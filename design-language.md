# Shale's design language

The grammar every surface draws from: color, spacing, shape, state, elevation,
type, motion, glyphs, and phrasing. Written from the ruled design round of
**2026-11-03** (all items ✅ unless marked otherwise). When you add or restyle a
surface — a tab, a popup, the review strip, a `.shq` table, the graph — this is
the file it must obey; when this file and a surface disagree, the surface is the
bug.

Status legend: ✅ ruled · ❄️ parked · ⏳ implementation owed.

> Writeback note: this round was ruled while the app-track agent was mid-flight,
> so it lives only here for now. The brainstorm.md/roadmap.md hooks (P6.9
> fold-in, the new palette roles feeding P2.5a, the glyph policy) are listed in
> §14 and owed once those files are free to touch.

## 1. Color — the Palette is the only source

Already true, kept: every paint site reads a named role from
`shale_core::theme::Palette` (0xRRGGBB roles; 20 built-in palettes;
`.shale/themes/*.toml` overrides). No surface hardcodes a color; a new role is
added to the Palette, never inlined at the paint site.

**✅ New roles (⏳, and the one that bites first):**

| Role | Carries |
|---|---|
| `diff_add` / `diff_delete` / `diff_changed` | the review strip's proposed hunks (P2.5a renders against these — land before it) |
| `agent` | "the agent is / has been here": review badges, edit provenance, graph node rings — one color for the whole presence cluster |
| `ui_disabled` | unavailable controls: dimmed keybinds, inactive chips |
| `graph_ghost` / `graph_tag` / `graph_attachment` | node kinds named, not derived from neighbors' colors |

**✅ Rule: roles are additive.** A new role is *optional* in TOML themes with a
computed fallback (e.g. `diff_add` falls back to `callout_note`, `agent` to
`accent`), so the 20 built-ins and every user theme keep loading untouched.
Built-ins gain explicit values for the new roles in the same pass.

## 2. Spacing — one scale, in `style.rs`

**✅ (⏳)** `SPACE_1..4 = 4 / 8 / 12 / 16` px, siblings of `CHIP_RADIUS` in
`crates/app/src/style.rs`. Chrome divs use them via gpui padding/margin;
custom-painted surfaces take them as arguments to `paint_quad` the way
`CHIP_RADIUS` is already passed. A surface that needs a gap picks the nearest
step; an off-scale number in a diff is a review comment.

## 3. Shape — one radius

Already true: `CHIP_RADIUS = 6px` is the single rounding decision for
gpui-styled chrome. **✅ (⏳)** the custom-painted surfaces (line-element
backgrounds, table grid, review badges, fold markers) take the same radius —
this is roadmap **P6.9**, folded into this doc's first sweep (§11) rather than
a standalone pass.

## 4. State grammar — three states, three roles, everywhere

**✅ (⏳ sweep)** Every interactive row/pane obeys exactly:

| State | Role |
|---|---|
| hover | `ui_hover_bg` |
| selected | `ui_active_bg` |
| keyboard focus | `ui_active_border` (the rule; `ui_border` when unfocused) |

Applies to: file tree, picker rows, tabs, status chips, graph nodes
(background→node tint equivalents), the review strip's rows, future `.shq`
tables. A surface inventing its own hover color is the drift this table kills.

## 5. Elevation — border + bg-shift + dim, no shadows

**✅** gpui gives us no shadows; we don't fake them. Depth is expressed by
exactly three tools, in this combination:

- **raised** (popups, picker panel, review strip, checker panel): `popup_bg` +
  `popup_border` + one dim level below it (the dimming system's `fade`);
- **surface** (chrome, tree, tabs): `ui_bg` + `ui_border`;
- **note** (the buffer): `bg`, no border.

Every floating thing uses the raised triple identically. No new elevation
level without amending this section.

## 6. Type — the note's scale exists; the chrome's is now shared

Already true: editor base size + proportional heading scale + one `LINE_RATIO`;
mono-family resolution with real bold/italic faces. **✅** chrome text sizes
(the status bar's `text_xs`, picker rows, tab labels) join `style.rs` as named
constants — the values in use today become the set; no per-surface sizes.
Mode words (`NORMAL/INSERT/…`) stay text: vim users read the word, and color
already carries it.

## 7. Motion — tokens, and a contract

**✅ (⏳)** durations and easings as constants in `style.rs` (one fast ~120ms,
one slow ~240ms to start). Contract: **chrome does not animate except toasts**
— no fades on tab switches, no sliding panels. The graph keeps its own ruled
idle/animation contract (M5a, PROJECT.md); this doc does not touch it. A new
animation needs a token from this file and a reason.

## 8. Glyphs and icons — ruled policy

**✅** Shale is a text-first app and stays one, with one named exception class.

- **Formalized unicode set (⏳)** — the marks already in use move into a named
  set in `style.rs` so they stop being per-site characters: `▸`/`▾` (tree
  disclosure), `●` (dirty, drawn in `accent`), `×` (close). New chrome marks
  come from this set or propose an addition.
- **Drawn-glyph exception class (⏳)** — custom-painted surfaces may draw
  simple vector marks where unicode is weak or color-only information needs a
  second channel: **callout types** (a mark on the left bar), **graph node
  kinds** (shape/glyph per kind so ghost/tag/attachment read without color),
  and the **agent presence mark** (one glyph carrying `agent` color across
  review badges, provenance, and graph rings).
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

## 10. Accessibility — color-only information needs a second channel

**✅** Any meaning carried by color alone must gain a mark or shape: node kinds
and callout types via §8's drawn glyphs; dirty state already pairs `●` +
`accent`. **✅ (⏳)** a **contrast guard test** asserts minimum contrast per
role-pair across all built-in palettes (e.g. `match_text` legible on both
`ui_bg` and `ui_active_bg`); it runs against user TOML themes at load and
warns rather than rejects.

## 11. Implementation sweep, in order (⏳)

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
- **✅ Linux window identity.** WM_CLASS, `.desktop` entry, and a window icon
  — a simple strata mark (stacked bedding lines) drawn once, reused by the
  wordmark rule below. (⏳)
- **✅ Cursor shape per vim mode** — spike what gpui exposes (block/line/
  underline); half mode-legibility, half identity. (⏳ spike)
- **✅ Command-name grammar** — ruled before the registry grows: bare nouns
  for views (`:graph`), verbs for actions, dotted ids in the registry
  (`note.save`, `theme.change`). New commands follow it at P2.7/P6.3.
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

## 14. ❄️ Parked, and the hooks owed elsewhere

- **Density/scale setting** (chrome-wide rem scaling): parked — real slog
  across custom painters, no demand yet. Door open: `style.rs` constants make
  it a multiplier someday.
- **Hooks owed when brainstorm.md/roadmap.md are free to touch** (other agent
  in flight at ruling time): roadmap P6.9 gets a pointer to §11.4; the §1
  palette roles get a line under P2.5a's dependencies; the brainstorm
  agent-presence cluster cites §8's `agent` role + mark; this file gets listed
  beside `graph-view-plan.md` in the roadmap's plan-file index.
