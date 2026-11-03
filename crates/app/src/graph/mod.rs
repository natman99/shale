//! The graph tab (roadmap P3.4, `graph-view-plan.md` M1b Task 5).
//!
//! **A view, not a graph.** The window owns the one [`shale_core::graph::GraphIndex`]
//! and (from Task 6 onward) the one force simulation; this entity is what the user is
//! looking at — the counts for now, and the camera, hover, selection and filter
//! bitmask as the canvas lands. That split is the integration contract's, and it is
//! what lets M5j's minimap and a local-graph view appear later without a second
//! simulation of the same vault.
//!
//! What is here is the *tab kind* made real: a focusable panel with its own key
//! context, so the graph can take the keyboard without borrowing vim's normal-mode
//! keys, and hand it back on `escape`. The canvas painting (Task 6), the interactions
//! (Task 7) and the filter panel (Task 8) come next; the panel below is deliberately
//! the graph's chrome rather than a placeholder, because the header and the counts are
//! what will sit around the canvas, and the canvas will go in the one slot below them.

use std::rc::Rc;

use gpui::{div, prelude::*, App, Context, FocusHandle, Focusable, IntoElement, Render, Window};
use shale_core::theme::Palette;

/// The key context the graph's bindings live in. It is a context of its own for the
/// same reason the picker and the prompt are: gpui enables a context's bindings
/// whenever it is in the focused element's stack, so the graph's keys cannot collide
/// with vim normal mode — and every key the graph wants (`hjkl`, `f`, `Enter`) is
/// already spoken for in the note.
pub(crate) const CONTEXT: &str = "graph";

/// What the window tells the view about the graph it is a view over.
///
/// A snapshot rather than a borrow, because the graph lives on the window and the view
/// is an entity that renders on its own: the window pushes this in whenever the graph
/// is built or a note changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GraphSummary {
    /// Notes in the graph, including the ones that link nothing.
    pub(crate) notes: usize,
    /// Link targets that resolve to no file — visible structure rather than vanished.
    pub(crate) ghosts: usize,
    /// `#tag` nodes.
    pub(crate) tags: usize,
    /// Links between them, with A→B and B→A already one edge.
    pub(crate) links: usize,
}

/// The graph tab.
pub(crate) struct GraphView {
    /// The view's own keyboard, so the `graph` context can be the active one and
    /// `escape` can hand the keyboard back to the note.
    pub(crate) focus_handle: FocusHandle,
    /// The colours the panel is drawn with, from the window. Shared with every other
    /// surface so a theme change is one assignment rather than a rebuild, and chosen
    /// from the `graph_*` roles for the parts that will be the canvas.
    palette: Rc<Palette>,
    /// What the graph holds, once the window has built it. `None` is the moment before
    /// the first build, and the panel says so rather than drawing a blank box.
    summary: Option<GraphSummary>,
}

impl GraphView {
    pub(crate) fn new(palette: Rc<Palette>, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            palette,
            summary: None,
        }
    }

    /// The palette the panel is drawn with.
    pub(crate) fn set_palette(&mut self, palette: Rc<Palette>, cx: &mut Context<Self>) {
        self.palette = palette;
        cx.notify();
    }

    /// What the graph looks like now. Called by the window whenever the index is built
    /// or a note moves under it.
    pub(crate) fn set_summary(&mut self, summary: GraphSummary, cx: &mut Context<Self>) {
        self.summary = Some(summary);
        cx.notify();
    }

    /// What the panel is showing. Only the tests ask: the window is the one that
    /// pushes the counts in.
    #[cfg(test)]
    pub(crate) fn summary(&self) -> Option<GraphSummary> {
        self.summary
    }
}

impl Focusable for GraphView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// One count in the summary row: the number in the node colour, its label in the
/// chrome's quiet colour, so the row reads as data with captions rather than prose.
fn stat(palette: &Palette, value: usize, label: &'static str) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap_1()
        .child(
            div()
                .text_color(gpui::rgb(palette.graph_node))
                .child(value.to_string()),
        )
        .child(div().text_color(gpui::rgb(palette.ui_dim)).child(label))
}

impl Render for GraphView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let palette = Rc::clone(&self.palette);
        // The summary, or the reason there is not one. A graph that has been built but
        // is empty says "0 notes", which is a different fact from "nothing has been
        // built", and the user should be able to tell them apart.
        let body = match self.summary {
            None => div()
                .text_color(gpui::rgb(palette.ui_dim))
                .child("The graph has not been built yet."),
            Some(counts) => div()
                .flex()
                .flex_wrap()
                .gap_4()
                .child(stat(&palette, counts.notes, "notes"))
                .child(stat(&palette, counts.links, "links"))
                .child(stat(&palette, counts.tags, "tags"))
                .child(stat(&palette, counts.ghosts, "missing")),
        };
        div()
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .size_full()
            .flex()
            .flex_col()
            .gap_2()
            .text_color(gpui::rgb(palette.ui_fg))
            .child(
                div()
                    .flex_none()
                    .font_weight(gpui::FontWeight::BOLD)
                    // The label is drawn in the colour the graph's own text will use,
                    // so the one thing on screen that is already the graph is painted
                    // with the graph's palette.
                    .text_color(gpui::rgb(palette.graph_text))
                    .child("Graph"),
            )
            .child(body)
            // The canvas slot. Empty until Task 6, but sized and bordered now so the
            // tab is recognizably a view rather than a caption, and so the frame the
            // nodes will be drawn in is the one that exists.
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .border_1()
                    .border_color(gpui::rgb(palette.ui_border))
                    .rounded(crate::style::CHIP_RADIUS)
                    .occlude(),
            )
    }
}
