# Computed surfaces: the host, and the dice chip (P4.15 + P7.9)

Written 2026-11-12, before any of it is built, because the rulings exist (brainstorm
"Inline computed surfaces" 2026-10-29, "Dice probability" 2026-11-08, roadmap P4.15 / P7.9)
and the plan does not. This is the plan half.

## What this is, in one line

A number the app works out and paints **beside** the text it is about, without ever
touching the text: `!p 2d8+4 vs AC16` grows a readout of hit %, average and crit chance,
computed exactly, in the margin of the line that asked.

## The policy it lives under (already ruled, not re-litigated here)

- **Region-anchored or nothing.** The status bar owns whole-note and whole-vault facts. A
  computed surface must be a fact pinned to a line or a section — something the bar
  structurally cannot report (brainstorm, binding).
- **Margins only**: the gutter beside a line, a strip above the note, or the status area.
  **Never a phantom line inside prose.**
- **App-offered only.** No user-authored widgets (that is plugins, and plugins are out). A
  table the user wrote is a `.shq`.
- **Read at render time, cached per note, invalidated on the dirty beats that already
  exist.** No new storage. No animation (`design-language.md` reserves motion for toasts).
- **Writes nothing back.** A probability is a property of the expression, not an event: no
  result annotation, no `rolls` row. The `rolls` table and the `!r` roll chip are **deferred**
  (ruled 2026-11-12) — this milestone ships the math and the readout, not the log.

## Rulings taken for this milestone (2026-11-12)

| # | question | ruling |
| --- | --- | --- |
| A | what marks an expression | **`!p`**, as specced. A bare `2d8+4` in prose is not a request — it would fire on ordinary writing. `!p` is the first of the `!` family to ship; `!r` inherits the grammar later. |
| B | where the readout goes | **inline, after the expression on its own line**, render-only, buffer untouched. It is not a phantom line (no new row), and a number a column away from the dice it describes is meaningless. The gutter chip is the fallback if it fights the wrap model. |
| C | what `vs N` means | **the total must meet or beat N** (`≥`, not `>`) — "meeting an AC means you hit it". Modifiers are part of the total, so `d20+3 vs AC16` needs 13+ and answers 40%. |
| C2 | the d20 exceptions | When the expression is **one die of 20 sides, keeping one** (a plain roll, or advantage/disadvantage as `2d20kh1`/`2d20kl1`), a natural 20 always succeeds and a natural 1 always fails. Crit chance is shown only in that reading, and crit range is **20 only** — the grammar has no improved-crit syntax and none is invented here. |
| C3 | `AC` vs `DC` | **`DC` is an alias for `AC`**, and the logic is identical. The word only changes the chip's language: `AC` says "hits", `DC` says "succeeds", a bare number says "meets or beats". House rules that move nat-20/nat-1 around are not Shale's problem (ruled). |
| D | no target at all | **mean, min, max** — and the distribution named only when it is small enough to name (a single die, 2d6). Not a probability calculator with a menu: "there are websites for that" (ruled). |
| E | the cue widget (`> [!cue]`, P7.10) | **off for now.** The host is built for the dice chip; the cue rides it later. |

## The grammar (and one delta from the specced one)

`NdN` with optional `+m` / `-m` and optional `khN` / `klN`, then optionally
`vs (AC|DC|) N`. Examples that must parse: `d20`, `2d8+4`, `1d8-1`, `4d6kh1+3`,
`2d20kh1+3 vs AC16`, `d20+3 vs DC 12`, `1d20 vs 15`.

**Delta:** the brainstorm wrote the modifier before the keep clause
(`NdN[+/-m][khN|klN]`), but the form GMs actually write for an ability score is
`4d6kh1+3` — keep first, modifier last. Both spellings parse to the same thing; the
canonical form keeps the modifier last. This is the module being wider than its spec
because the spec's order is the one nobody types.

## Milestones

**M1 — the math, in `shale-core` (`dice/`).** No window, no buffer, no DB: pure Rust, no
crate (ruled). Three pieces, one module directory:

- `parse.rs` — the grammar above, into a `Dice` and an optional `Target`. Refusals name
  what broke and where (`3d`, `2d0+4`, `2d8+4 vs`, `kh0`).
- `dist.rs` — the **exact** distribution. No keep/drop: convolution over the dice, integer
  counts, no floating point until the very end. With keep/drop: enumerate the multisets of
  the roll and weight each by its multinomial coefficient — exact, and cheap for the rolls
  anyone actually makes (4d6kh1 is 126 multisets). A roll whose multiset count passes the
  budget **refuses out loud** rather than hanging: this is not a comprehensive calculator.
- `stats.rs` — mean, min, max, `P(meets or beats N)`, and the d20 reading (C2): needed
  value, success chance with the nat-20/nat-1 exceptions, crit chance.

The tests pin the parts the brainstorm named: **order statistics for `khN`/`klN`** —
advantage is the common case — against the closed forms a hand can check
(`d20 vs AC16` = 25%, `d20+3 vs AC16` = 40%, advantage `2d20kh1+3 vs AC16` = 64%, crit with
advantage = 9.75%), and the exception edges where arithmetic and 5e disagree
(`d20+7 vs AC8` = 95% not 100%, `d20-5 vs AC16` = 5% not 0%).

**M2 — the chip, in the app.** Find `!p` lines in the block map, compute through the core
module, cache per note keyed to the note's change id (the existing dirty beats), and paint
the readout after the expression. The readout is one string the frame builds, so a test can
assert what a user sees rather than what a struct holds.

**M3 — the host's contract, written down.** What a surface is (anchor, read, cache key,
paint), so the next widget — the cue, a counter, P4.7's counts — is a consumer and not a
second host. Only written after the second consumer exists would be too late to be a
contract; it is written after M2 so it describes built code.

## What is deliberately not here

- The `rolls` table and the `!r` roll chip (deferred by ruling).
- `> [!cue]` and `> [!counter]` (P7.10, off by ruling).
- Encounter budgeting (Hard vs Deadly) — the scope guard from the brainstorm stands.
- Improved-crit ranges, exploding dice, dice pools with rerolls, any roll the grammar does
  not spell.

## Where it stands (2026-11-12)

**M1 closed** (`372c93d`): `shale_core::dice` — grammar, exact distribution, the readout's
numbers. Eight falsifications, all red.

**M2 closed** (`b9bb95d`): the readout painted after its line. It is **not** spliced into the
line's runs — the piece map, caret and selection are measured against buffer text, so a
synthetic character in the run list would move the caret off its character. It is shaped on
its own, like a fold marker, and keyed to where the line **ends** so a wrapped line carries
it on the segment where its text stops. The fence decision is the render pass's, not the
cache's. Twelve tests read what a frame painted (`HitLine.chip`); five falsifications, all
red.

### What the reader's eye still has to decide

These are visual, not arithmetic, and they are the reason to look at a live window before
M3 writes the contract:

1. **Colour.** The chip is `palette.dim` today — quieter than the prose. Whether it should
   instead read as a computed thing (a colour of its own, like `tag` or `agent`) is a call
   about whether the chip belongs to the note's voice or to the app's.
2. **A long line.** The chip is painted after the row's shaped text and can run past the
   right edge; a wrapped line's chip lands on its last segment. The plan's gutter fallback
   is unbuilt. Whether to clip, wrap the chip onto its own display row, or move it to the
   gutter is the open question, and it is the one place this design could still fight the
   wrap model.
3. **The named distribution.** `Stats::shape` names the totals when there are few enough to
   read (ruling D) and is computed but not shown: a margin is not where a distribution
   belongs. It wants a surface — a hover card or `g`-style peek — and that is a consumer for
   the host contract, not a wider chip.
