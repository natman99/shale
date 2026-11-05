//! The pane tree (roadmap P3.2). The window's editing area is a tiling tree:
//! a leaf is one pane (a tab stack, in `tabs.rs`), a split is a row or a
//! column of children with a relative fraction each. This is the neovim shape,
//! and the shape the user asked for: `ctrl-w s` splits the pane in hand,
//! `ctrl-w h/j/k/l` steps between panes geometrically, `ctrl-w q` closes one,
//! `ctrl-w o` keeps only it.
//!
//! The tree lives beside the flat `Vec<Pane>` rather than inside it because
//! panes hold entities and the tree is geometry: the flat list keeps pane
//! identity stable across tree edits, and the tree's leaves are indices into
//! it. Removing a pane renumbers the leaves above it, which is fine — nothing
//! outside this module names a pane by index except `active_pane`, which every
//! removal here fixes up.
//!
//! Fractions are relative, not normalized: `[2, 1]` lays out the same as
//! `[4, 2]`. Equalizing sets them all to 1.

use gpui::{Context, Window};
use shale_core::session::LayoutSeat;

use crate::workspace::Workspace;

/// How much one press of `ctrl-w >` moves a divider, in the tree's relative fractions.
/// A quarter of an equal share: visible in one press, small enough to fine-tune with a
/// few.
const RESIZE_STEP: f32 = 0.25;

/// How small a pane may be squeezed, in the tree's relative fractions. Below this the
/// resize refuses: a pane that has been dragged to a sliver is still a pane the user
/// has to be able to see and select.
const MIN_FRAC: f32 = 0.2;

/// A split's orientation. `Row` lays children side by side (vim's `:vsplit`,
/// `ctrl-w v`), `Column` stacks them (vim's `:split`, `ctrl-w s`) — named for
/// the flex direction, so the render code reads without a translation step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SplitDir {
    Row,
    Column,
}

/// One node of the pane tree.
#[derive(Clone, Debug)]
pub(crate) enum Node {
    /// A pane, by index into `Workspace::panes`.
    Leaf(usize),
    Split {
        dir: SplitDir,
        kids: Vec<Node>,
        /// Relative sizes, one per kid. The sum does not have to be 1.
        frac: Vec<f32>,
    },
}

impl Node {
    /// The first leaf under this node — the pane `ctrl-w l` from the sidebar
    /// lands in.
    pub(crate) fn first_leaf(&self) -> Option<usize> {
        match self {
            Node::Leaf(ix) => Some(*ix),
            Node::Split { kids, .. } => kids.first()?.first_leaf(),
        }
    }

    /// The tree as the session file keeps it: the same shape, the same fractions, and
    /// the leaves as leaves (roadmap P3.2). The leaf *indices* are not written — the
    /// file's `panes` list is the leaves, in this tree's leaf order.
    pub(crate) fn seat(&self) -> LayoutSeat {
        match self {
            Node::Leaf(_) => LayoutSeat::Leaf,
            Node::Split { dir, kids, frac } => LayoutSeat::Split {
                dir: match dir {
                    SplitDir::Row => "row".into(),
                    SplitDir::Column => "column".into(),
                },
                frac: frac.clone(),
                kids: kids.iter().map(|kid| kid.seat()).collect(),
            },
        }
    }

    /// The tree a session file describes, for a window that is to have `leaves` panes.
    ///
    /// `None` when the shape does not describe exactly that many panes, names a
    /// direction that is not one of the two, or gives a kid no space. A session file is
    /// plain text a person can edit, and a window that cannot make sense of the shape
    /// keeps the single pane it opened with rather than painting nonsense.
    pub(crate) fn from_seat(seat: &LayoutSeat, leaves: usize) -> Option<Node> {
        let mut next = 0;
        let node = Node::from_seat_at(seat, leaves, &mut next)?;
        (next == leaves).then_some(node)
    }

    fn from_seat_at(seat: &LayoutSeat, leaves: usize, next: &mut usize) -> Option<Node> {
        match seat {
            LayoutSeat::Leaf => {
                if *next >= leaves {
                    return None;
                }
                let ix = *next;
                *next += 1;
                Some(Node::Leaf(ix))
            }
            LayoutSeat::Split { dir, frac, kids } => {
                let dir = match dir.as_str() {
                    "row" => SplitDir::Row,
                    "column" => SplitDir::Column,
                    _ => return None,
                };
                if kids.is_empty()
                    || kids.len() != frac.len()
                    || frac.iter().any(|f| !f.is_finite() || *f <= 0.)
                {
                    return None;
                }
                let kids = kids
                    .iter()
                    .map(|kid| Node::from_seat_at(kid, leaves, next))
                    .collect::<Option<Vec<_>>>()?;
                Some(Node::Split {
                    dir,
                    kids,
                    frac: frac.clone(),
                })
            }
        }
    }

    /// Whether this node's subtree contains the pane `leaf_ix`.
    fn holds(&self, leaf_ix: usize) -> bool {
        match self {
            Node::Leaf(ix) => *ix == leaf_ix,
            Node::Split { kids, .. } => kids.iter().any(|kid| kid.holds(leaf_ix)),
        }
    }

    /// Give the pane in hand a bigger or smaller slice of the split it sits directly
    /// in (roadmap P3.2's resize half). `delta` is in the tree's relative units, where
    /// 1.0 is an equal share, so `[1, 1]` plus `0.25` is a 5:4 pair — the other kids
    /// keep their fractions and take proportionally less.
    ///
    /// `false` when there is no split to resize within (a window with one pane), and
    /// when the move would take the pane below [`MIN_FRAC`]: a pane squeezed to nothing
    /// is a pane the user cannot get back.
    pub(crate) fn resize(&mut self, leaf_ix: usize, delta: f32) -> bool {
        let Node::Split { kids, frac, .. } = self else {
            return false;
        };
        let Some(slot) = kids
            .iter()
            .position(|kid| matches!(kid, Node::Leaf(ix) if *ix == leaf_ix))
        else {
            // Deeper down: the nearest split the pane sits in is the one that resizes.
            return kids
                .iter_mut()
                .find(|kid| kid.holds(leaf_ix))
                .is_some_and(|kid| kid.resize(leaf_ix, delta));
        };
        let next = frac[slot] + delta;
        if next < MIN_FRAC {
            return false;
        }
        frac[slot] = next;
        true
    }

    /// Normalized rectangles (0..1 in both axes) for every leaf. Geometry for
    /// the directional focus move: one walk of the tree lays the whole tiling
    /// out, fractions and all.
    pub(crate) fn rects(&self) -> Vec<(usize, f32, f32, f32, f32)> {
        let mut out = Vec::new();
        self.rects_at(0., 0., 1., 1., &mut out);
        out
    }

    fn rects_at(&self, x: f32, y: f32, w: f32, h: f32, out: &mut Vec<(usize, f32, f32, f32, f32)>) {
        match self {
            Node::Leaf(ix) => out.push((*ix, x, y, w, h)),
            Node::Split { dir, kids, frac } => {
                let total: f32 = frac.iter().sum();
                let total = if total > 0. { total } else { kids.len() as f32 };
                let mut off = 0.;
                for (kid, f) in kids.iter().zip(frac.iter()) {
                    let share = f / total;
                    match dir {
                        SplitDir::Row => kid.rects_at(x + off * w, y, w * share, h, out),
                        SplitDir::Column => kid.rects_at(x, y + off * h, w, h * share, out),
                    }
                    off += share;
                }
            }
        }
    }

    /// Replace the leaf holding `ix` with `new`, in place.
    pub(crate) fn replace_leaf(&mut self, ix: usize, new: Node) {
        match self {
            Node::Leaf(cur) if *cur == ix => {
                *self = new;
            }
            Node::Leaf(_) => {}
            Node::Split { kids, .. } => {
                for kid in kids.iter_mut() {
                    kid.replace_leaf(ix, new.clone());
                }
            }
        }
    }

    /// Remove the leaf holding `ix` and its fraction slot, recursing through
    /// the whole tree. A split left with one kid collapses into that kid, so
    /// closing a pane in a three-pane tree never leaves a one-child split
    /// rendering a pointless wrapper. The root never collapses to nothing
    /// because the caller refuses to close the last pane.
    pub(crate) fn remove_leaf(&mut self, ix: usize) {
        let Node::Split { dir, kids, frac } = self else {
            return;
        };
        for kid in kids.iter_mut() {
            if matches!(kid, Node::Leaf(cur) if *cur == ix) {
                continue;
            }
            kid.remove_leaf(ix);
        }
        let mut kept_kids = Vec::with_capacity(kids.len());
        let mut kept_frac = Vec::with_capacity(kids.len());
        for (kid, f) in std::mem::take(kids).into_iter().zip(std::mem::take(frac)) {
            if matches!(kid, Node::Leaf(cur) if cur == ix) {
                continue;
            }
            kept_kids.push(kid);
            kept_frac.push(f);
        }
        *kids = kept_kids;
        *frac = kept_frac;
        if kids.len() == 1 {
            let kid = kids.remove(0);
            let _ = dir;
            *self = kid;
        }
    }

    /// Shift every leaf above `ix` down by one. `remove_leaf` drops the closed
    /// pane's slot but the panes *above* it in the flat list renumbered when it
    /// was removed, and the tree has to follow or a leaf names the wrong pane.
    pub(crate) fn renumber_after(&mut self, ix: usize) {
        match self {
            Node::Leaf(p) => {
                if *p > ix {
                    *p -= 1;
                }
            }
            Node::Split { kids, .. } => {
                for kid in kids.iter_mut() {
                    kid.renumber_after(ix);
                }
            }
        }
    }

    /// Set every fraction in the tree to equal.
    pub(crate) fn equalize(&mut self) {
        if let Node::Split { kids, frac, .. } = self {
            *frac = vec![1.; kids.len()];
            for kid in kids.iter_mut() {
                kid.equalize();
            }
        }
    }
}

impl Workspace {
    /// Split the active pane. `dir` is the *container* direction the pair
    /// gets laid out in: `Row` puts the panes side by side (`ctrl-w v`),
    /// `Column` stacks them (`ctrl-w s`). With `carry`, the new pane opens on
    /// the same note the active tab shows (vim's `S`/`V`); without, it opens
    /// an empty tab.
    ///
    /// The new pane becomes active — the keyboard follows the split, as it
    /// does in vim.
    pub(crate) fn split_pane(
        &mut self,
        dir: SplitDir,
        carry: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let old_ix = self.active_pane;
        // The note to carry has to be read before the tree moves: `add_tab`
        // and the open below both act on whichever pane is active, and by
        // then the active pane is the new one.
        let carry_path = carry.then(|| self.pane().active_tab().path().map(|p| p.to_path_buf()));
        let new_ix = self.panes.len();
        self.panes
            .push(crate::workspace::tabs::Pane::new(Vec::new()));
        let pair = Node::Split {
            dir,
            kids: vec![Node::Leaf(old_ix), Node::Leaf(new_ix)],
            frac: vec![1., 1.],
        };
        if matches!(self.layout, Node::Leaf(_)) {
            self.layout = pair;
        } else {
            self.layout.replace_leaf(old_ix, pair);
        }
        self.active_pane = new_ix;
        self.hover_stale = true;
        // The new pane starts the way every pane starts: one tab. `add_tab`
        // works on the active pane, which is now the new one.
        self.add_tab(cx);
        if let Some(Some(path)) = carry_path {
            // `load_note`, not `open_note`: the find-don't-duplicate rule would
            // notice the note already open in the *old* pane and focus that
            // one, which is exactly backwards for a split-with-buffer. The
            // whole point is a second view, so the note loads into the new
            // pane's tab directly.
            self.load_note(&path, cx);
        }
        self.focus_active_tab(window, cx);
        cx.notify();
    }

    /// Close the active pane, writing its notes first — the same rule as
    /// closing a tab, one level up: a failed write keeps the pane. The last
    /// pane cannot be closed; the status bar says so rather than the window
    /// vanishing under the user.
    pub(crate) fn close_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.panes.len() <= 1 {
            self.say("this is the only pane", cx);
            return;
        }
        // Write every dirty tab in the pane being closed. A failure aborts the
        // whole close, pane included: half the pane's notes on disk and half
        // not is the one state a close must never leave.
        let ix = self.active_pane;
        let n = self.panes[ix].tabs.len();
        for tab_ix in 0..n {
            let dirty = self.panes[ix].tabs[tab_ix]
                .editor()
                .is_some_and(|editor| editor.read_with(cx, |editor, _cx| editor.is_dirty()));
            if dirty {
                self.write_tab(tab_ix, cx);
                let still = self.panes[ix].tabs[tab_ix]
                    .editor()
                    .is_some_and(|editor| editor.read_with(cx, |editor, _cx| editor.is_dirty()));
                if still {
                    self.say("the pane stayed open: a write failed", cx);
                    return;
                }
            }
        }
        // The pane's tabs go with it: each tab's editor subscription and
        // autosave task live *in* the tab, so dropping the pane drops them.
        self.panes.remove(ix);
        self.layout.remove_leaf(ix);
        self.layout.renumber_after(ix);
        // The removal renumbered every pane above the closed one; follow.
        self.active_pane = self.active_pane.min(self.panes.len() - 1);
        self.hover_stale = true;
        self.focus_active_tab(window, cx);
        cx.notify();
    }

    /// `ctrl-w o` / `:only`: close every pane but the active one.
    pub(crate) fn only_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Track the kept pane by index as the others close: each close
        // renumbers the panes above the victim, and the kept pane drifts down
        // whenever the victim sat to its left.
        let mut kept = self.active_pane;
        while self.panes.len() > 1 {
            // Close the others one at a time so each one's write rules run;
            // if a write refuses a close, stop rather than spin or half-close.
            let victim = (0..self.panes.len()).find(|ix| *ix != kept);
            let Some(victim) = victim else { break };
            let before = self.panes.len();
            self.active_pane = victim;
            self.close_pane(window, cx);
            if self.panes.len() == before {
                self.active_pane = kept.min(self.panes.len() - 1);
                return;
            }
            if kept > victim {
                kept -= 1;
            }
        }
        self.active_pane = kept;
        self.layout.equalize();
        self.hover_stale = true;
        self.focus_active_tab(window, cx);
        cx.notify();
    }

    /// Step focus to the neighbouring pane in a direction, geometrically like
    /// vim: the tree lays the panes out, and the neighbour is the leaf on the
    /// far side in that direction that overlaps on the cross axis, nearest
    /// first. `dx`/`dy` are -1/0/1.
    ///
    /// The sidebar participates at the edges — stepping left out of the
    /// leftmost pane lands in the tree (when visible), stepping right from
    /// the tree lands in the leftmost pane — so `ctrl-w h/l` stays one
    /// continuous habit from P1.8's two-panel window. That edge handling
    /// lives in `window_left`/`window_right`; this is the pane-to-pane half.
    /// Returns whether a pane took the move.
    pub(crate) fn focus_pane_relative(
        &mut self,
        dx: i32,
        dy: i32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let rects = self.layout.rects();
        let Some((_, ax, ay, aw, ah)) = rects.iter().find(|(ix, ..)| *ix == self.active_pane)
        else {
            return false;
        };
        let (ax, ay, aw, ah) = (*ax, *ay, *aw, *ah);
        let mut best: Option<(usize, f32)> = None;
        for (ix, x, y, w, h) in &rects {
            if *ix == self.active_pane {
                continue;
            }
            let ok = match (dx, dy) {
                (1, _) => *x >= ax + aw - 0.001 && overlap(*y, *h, ay, ah),
                (-1, _) => *x + *w <= ax + 0.001 && overlap(*y, *h, ay, ah),
                (0, 1) => *y >= ay + ah - 0.001 && overlap(*x, *w, ax, aw),
                (0, -1) => *y + *h <= ay + 0.001 && overlap(*x, *w, ax, aw),
                _ => false,
            };
            if !ok {
                continue;
            }
            let dist = if dx != 0 {
                (*x - ax).abs().max((*x + *w - (ax + aw)).abs())
            } else {
                (*y - ay).abs().max((*y + *h - (ay + ah)).abs())
            };
            if best.map_or(true, |(_, bd)| dist < bd) {
                best = Some((*ix, dist));
            }
        }
        let Some((ix, _)) = best else { return false };
        self.active_pane = ix;
        self.hover_stale = true;
        self.focus_active_tab(window, cx);
        cx.notify();
        true
    }

    /// The leftmost pane, for `window_right` from the sidebar.
    pub(crate) fn first_pane(&self) -> Option<usize> {
        self.layout.first_leaf()
    }

    /// `ctrl-w =`: every pane gets its share back.
    pub(crate) fn equalize_panes(&mut self, cx: &mut Context<Self>) {
        self.layout.equalize();
        cx.notify();
    }

    /// Give the pane with the keyboard more or less of the space its split offers
    /// (roadmap P3.2). Answers whether anything moved, so a test — and a caller that
    /// wants to say something — can tell a window with one pane from a window that just
    /// moved a divider.
    pub(crate) fn resize_pane(&mut self, delta: f32, cx: &mut Context<Self>) -> bool {
        let moved = self.layout.resize(self.active_pane, delta);
        if moved {
            cx.notify();
        }
        moved
    }

    /// Find a tab holding `path` in *any* pane. Opening a note that is
    /// already open focuses the pane showing it rather than opening a third
    /// copy — which is what a split makes newly important.
    pub(crate) fn tab_for_path_anywhere(&self, path: &std::path::Path) -> Option<(usize, usize)> {
        self.panes.iter().enumerate().find_map(|(pix, pane)| {
            pane.tabs
                .iter()
                .position(|t| t.path().is_some_and(|p| p == path))
                .map(|tix| (pix, tix))
        })
    }

    // The `ctrl-w` alphabet (roadmap P3.2). Every handler asks
    // `overlay_owns_keys` first for the same reason `window_left` does: these
    // bindings are context-free, and a prompt or picker under the user owns
    // the keyboard until it is dismissed.

    pub(crate) fn window_split_down(
        &mut self,
        _: &crate::acts::WindowSplitDown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.overlay_owns_keys() {
            return;
        }
        self.split_pane(SplitDir::Column, false, window, cx);
    }

    pub(crate) fn window_split_right(
        &mut self,
        _: &crate::acts::WindowSplitRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.overlay_owns_keys() {
            return;
        }
        self.split_pane(SplitDir::Row, false, window, cx);
    }

    pub(crate) fn window_split_down_here(
        &mut self,
        _: &crate::acts::WindowSplitDownHere,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.overlay_owns_keys() {
            return;
        }
        self.split_pane(SplitDir::Column, true, window, cx);
    }

    pub(crate) fn window_split_right_here(
        &mut self,
        _: &crate::acts::WindowSplitRightHere,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.overlay_owns_keys() {
            return;
        }
        self.split_pane(SplitDir::Row, true, window, cx);
    }

    pub(crate) fn window_close(
        &mut self,
        _: &crate::acts::WindowClose,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.overlay_owns_keys() {
            return;
        }
        self.close_pane(window, cx);
    }

    pub(crate) fn window_only(
        &mut self,
        _: &crate::acts::WindowOnly,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.overlay_owns_keys() {
            return;
        }
        self.only_pane(window, cx);
    }

    pub(crate) fn window_up(
        &mut self,
        _: &crate::acts::WindowUp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.overlay_owns_keys() {
            return;
        }
        self.focus_pane_relative(0, -1, window, cx);
    }

    pub(crate) fn window_down(
        &mut self,
        _: &crate::acts::WindowDown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.overlay_owns_keys() {
            return;
        }
        self.focus_pane_relative(0, 1, window, cx);
    }

    pub(crate) fn window_equalize(
        &mut self,
        _: &crate::acts::WindowEqualize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.overlay_owns_keys() {
            return;
        }
        self.equalize_panes(cx);
    }

    pub(crate) fn window_grow(
        &mut self,
        _: &crate::acts::WindowGrow,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.overlay_owns_keys() {
            return;
        }
        self.resize_pane(RESIZE_STEP, cx);
    }

    pub(crate) fn window_shrink(
        &mut self,
        _: &crate::acts::WindowShrink,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.overlay_owns_keys() {
            return;
        }
        self.resize_pane(-RESIZE_STEP, cx);
    }
}

fn overlap(a0: f32, alen: f32, b0: f32, blen: f32) -> bool {
    a0 < b0 + blen - 0.001 && b0 < a0 + alen - 0.001
}
