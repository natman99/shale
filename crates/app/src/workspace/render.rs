//! The frame: sidebar rows, the prompt, the note, the status bar, and the
//! window's own title.

use std::path::{Path, PathBuf};

use gpui::{anchored, div, prelude::*, px, AnchoredPositionMode, Context, MouseButton, Window};

use crate::acts::*;
use crate::editor::View;
use crate::prompt;
use crate::style::CHIP_RADIUS;
use crate::tree;

use super::bar::{mode_label, save_indicator, word_badge};
use super::tabs::TabContent;
use super::tree::{push_node, TreeState};
use super::Workspace;

/// The height of the tab bar's row, and of one tab within it. A little taller than
/// the status bar because it is something the pointer aims at, not something it
/// reads.
const TAB_BAR: gpui::Pixels = gpui::px(28.);
/// One tab's height inside the bar. The four pixels of bar colour above and below it
/// are the point: with one tab open, a chip that fills the bar reads as the bar, and
/// a chip the bar frames reads as a tab.
const TAB_ROW: gpui::Pixels = gpui::px(20.);
/// The bar's own colour left at its ends and between two tabs: the spacing scale's
/// first step (`design-language.md` §2), which is also the bar colour above and below
/// a tab, so the frame the bar draws around its tabs is the same width on every side.
const TAB_GAP: gpui::Pixels = crate::style::SPACE_1;

/// The gap between the vault's name and the note's in the window's title. An em
/// dash with spaces, which is how a title's two parts are set off in prose; a
/// plain `-` would make `my-vault - Welcome` read as one long hyphenated name,
/// and vault names are full of hyphens.
const TITLE_GAP: &str = " — ";

/// The vault's name: the last component of its path, which is what a vault is
/// called by the person who made it. A path with no last component (`/`) falls
/// back to the whole path, because an empty name is worse than a long one.
///
/// Also what the window is *opened* with, before the first frame has a note to
/// name, so the window manager never has a moment where the window is called
/// something else.
pub(crate) fn vault_title(root: &Path) -> String {
    root.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.display().to_string())
}

impl Workspace {
    /// One pane's tab bar (roadmap P3.2). Built here rather than in a widget of
    /// its own because a tab is not a thing that can be drawn without the window:
    /// clicking one moves the keyboard *to that pane* and closing one writes its
    /// note. Each pane draws its own bar; the bar's clicks name their pane.
    pub(crate) fn tab_bar(&self, pix: usize, cx: &mut Context<Self>) -> gpui::AnyElement {
        let tabs: Vec<gpui::Stateful<gpui::Div>> = self.panes[pix]
            .tab_state(cx)
            .into_iter()
            .enumerate()
            .map(|(index, tab)| {
                let label = if tab.graph {
                    // The graph tab has no file name to show, and "graph" is what the
                    // tab is: the same word the status bar's badge uses.
                    "graph".to_string()
                } else {
                    tab.path
                        .as_ref()
                        .map(|path| {
                            path.file_stem()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string()
                        })
                        .unwrap_or_else(|| "no note".to_string())
                };
                let entity = cx.entity().clone();
                let close = cx.entity().clone();
                let id = tab.id;
                let active = index == self.panes[pix].active;
                div()
                    .id(("tab", id))
                    .flex_1()
                    .min_w_0()
                    .h(TAB_ROW)
                    .px_2()
                    .rounded(CHIP_RADIUS)
                    .flex()
                    .items_center()
                    .gap_1()
                    // A tab is a thing to act on, so the pointer says so rather than
                    // offering the caret it inherits from the note.
                    .cursor(gpui::CursorStyle::PointingHand)
                    .text_color(gpui::rgb(if active {
                        self.palette.ui_fg
                    } else {
                        self.palette.ui_dim
                    }))
                    .child(
                        // The same dot the status bar uses, on the tab it belongs to:
                        // with tabs open, a note ahead of its file is not necessarily
                        // the one on screen.
                        div()
                            .text_color(gpui::rgb(self.palette.accent))
                            .child(save_indicator(tab.dirty)),
                    )
                    .child(div().flex_1().min_w_0().text_ellipsis().child(label))
                    .child(
                        div()
                            .id(("tab-close", id))
                            .text_color(gpui::rgb(self.palette.ui_dim))
                            .cursor(gpui::CursorStyle::PointingHand)
                            .child(crate::style::GLYPH_CLOSE)
                            // The close mark is the tab's own click. Without stopping
                            // the propagation the row behind it would take the same
                            // click and switch to the tab it is closing.
                            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            .on_click(move |_, window, cx| {
                                close.update(cx, |ws, cx| {
                                    // The bar belongs to pane `pix`; the click makes
                                    // it the active pane first, so the close lands on
                                    // this bar's tab and not the same index in
                                    // whichever pane had the keyboard.
                                    ws.active_pane = pix;
                                    ws.hover_stale = true;
                                    ws.click_tab_close(id, window, cx);
                                });
                            }),
                    )
                    // The tab bar's own pair: an inactive tab is a raised chip on
                    // the bar, the open one is the chip that is connected to the
                    // surface below it. They were unpainted before, which left the
                    // roles reserved but unused (`design-language.md` §4).
                    .bg(gpui::rgb(self.palette.ui_tab_bg))
                    .when(active, |d| d.bg(gpui::rgb(self.palette.ui_tab_active_bg)))
                    .when(!active, |d| {
                        d.hover(|s| s.bg(gpui::rgb(self.palette.ui_hover_bg)))
                    })
                    .on_click(move |_, window, cx| {
                        entity.update(cx, |ws, cx| {
                            ws.active_pane = pix;
                            ws.hover_stale = true;
                            ws.click_tab(id, window, cx);
                        });
                    })
            })
            .collect();

        div()
            .flex_none()
            .w_full()
            .h(TAB_BAR)
            .px(TAB_GAP)
            .flex()
            .items_center()
            .gap(TAB_GAP)
            .text_size(crate::style::TEXT_XS)
            .bg(gpui::rgb(self.palette.ui_status_bg))
            .text_color(gpui::rgb(self.palette.ui_dim))
            // The card's top corners: the bar's own background carries the
            // round (the chips are inset by TAB_GAP and never reach them).
            .rounded_t(crate::style::PANE_RADIUS)
            .children(tabs)
            .into_any_element()
    }

    /// One pane: its tab bar over its active tab's view. The padding and the
    /// background belong to the pane, so two panes read as two notes with air
    /// between their chrome, not one note with a gutter in the middle.
    pub(crate) fn pane_element(&self, pix: usize, cx: &mut Context<Self>) -> gpui::AnyElement {
        // A pane without the keyboard paints its cursor faded: the caret stays
        // visible — switching panes should not make a note look uneditable —
        // but the solid block marks the pane the keys are in. Set at paint time
        // so every active_pane change lands, whichever handler caused it; the
        // value converges after one frame and the editor repaints this frame
        // with the flag already correct.
        if let TabContent::Note { editor, .. } = &self.panes[pix].active_tab().content {
            editor.update(cx, |editor, _cx| {
                editor.cursor_dim = pix != self.active_pane
            });
        }
        // The slot belongs to the tab on screen in *this* pane: a note tab's
        // editor or the graph view, never both, and never the active pane's.
        let slot: Option<gpui::AnyElement> = match &self.panes[pix].active_tab().content {
            TabContent::Note { editor, .. } => Some(editor.clone().into_any_element()),
            TabContent::Graph(view) => Some(view.clone().into_any_element()),
        };
        div()
            .flex()
            .flex_col()
            .min_w_0()
            .min_h_0()
            // The pane fills its slot in whatever lays it out: the pane area for
            // a root leaf, or the split's flex row/column for a kid. Grow plus a
            // zero basis is the proportional share; without it the pane would
            // size to its content and the note's viewport would collapse.
            .flex_grow(1.)
            .flex_basis(gpui::relative(0.))
            // gpui does not stretch flex items the way CSS does: the cross
            // axis must be spelled out, or a pane inside a split sizes to its
            // content and the editor under the tab bar collapses to zero
            // height. The main axis still comes from grow/basis, which win
            // over these on whichever axis the parent splits along.
            .size_full()
            // Which pane has the keyboard: the palette reserves
            // `ui_active_border` for exactly this — "the rule around whatever
            // the window currently has the keyboard" — with `ui_border` on the
            // rest. Only worth drawing when the window is actually tiled; a
            // lone pane would just gain a frame around the whole editor.
            .rounded(crate::style::PANE_RADIUS)
            .when(self.panes.len() > 1, |d| {
                d.border_1()
                    .border_color(gpui::rgb(if pix == self.active_pane {
                        self.palette.ui_active_border
                    } else {
                        self.palette.ui_border
                    }))
            })
            .child(self.tab_bar(pix, cx))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    // The note's background ends in the same round corners the
                    // pane's rule draws; gpui clips children to rectangles, so
                    // each painted surface rounds itself.
                    .rounded_b(crate::style::PANE_RADIUS)
                    .h_full()
                    .flex()
                    .flex_col()
                    // The note gets some air: text hard against the pane's edges
                    // is cramped. The padding is on this column and its background
                    // is the note's, so the air belongs to the note.
                    .px_4()
                    .pt_3()
                    .pb_2()
                    .bg(gpui::rgb(self.palette.bg))
                    // The column, not just the text, has to report the pointer:
                    // the space below the last line belongs to no line, and a
                    // panel the pointer walked away downwards would otherwise
                    // never be told to go.
                    .on_mouse_move(
                        cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                            this.pointer_moved(event.position, window, cx);
                        }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .when_some(slot, |d, e| d.child(e))
                            // A blank tab is not a void: the start block sits on
                            // the note's own surface, at the note's text indent,
                            // and disappears the moment the tab becomes a note
                            // (`workspace::start`). `occlude` plus the stopped
                            // mouse-down are what make its rows clickable rather
                            // than clicks that fall through to the editor's caret
                            // underneath (the picker overlays use the same pair).
                            .when(self.show_start_block(pix, cx), |d| {
                                d.child(
                                    div()
                                        .absolute()
                                        .top_8()
                                        .left(crate::style::GUTTER)
                                        .occlude()
                                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                            cx.stop_propagation();
                                        })
                                        .child(self.start_block(pix, cx)),
                                )
                            }),
                    ),
            )
            .into_any_element()
    }

    /// The pane tree, laid out. A split is a flex row or column whose children
    /// carry `flex_grow` fractions — gpui has no `flex_ratio`, but grow plus a
    /// zero basis is exactly proportional distribution.
    pub(crate) fn render_node(
        &self,
        node: &super::panes::Node,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        use super::panes::{Node, SplitDir};
        match node {
            Node::Leaf(pix) => self.pane_element(*pix, cx),
            Node::Split { dir, kids, frac } => {
                let el = div().size_full().min_w_0().min_h_0().flex();
                let el = match dir {
                    SplitDir::Row => el.flex_row(),
                    SplitDir::Column => el.flex_col(),
                };
                el.children(kids.iter().zip(frac.iter()).map(|(kid, f)| {
                    div()
                        .flex_grow(*f)
                        .flex_basis(gpui::relative(0.))
                        .min_w_0()
                        .min_h_0()
                        // A plain div is Block layout: its child would ignore
                        // grow and size to content. Make the wrapper a flex
                        // container and give it its full box explicitly.
                        .size_full()
                        .flex()
                        .child(self.render_node(kid, cx))
                }))
                .into_any_element()
            }
        }
    }
    /// The window's title: the vault, and the note on screen when there is one --
    /// `my-vault — Welcome`.
    ///
    /// The vault's name rather than its path, because the window list is a list of
    /// vaults and the path is what the picker already shows. The note's name
    /// without its `.md`, which is how the tab bar names the same note. A tab with
    /// no note -- the graph, or the empty tab that closing the last tab leaves --
    /// leaves the vault name alone: the title says *which* window this is, not
    /// which view of it happens to be on screen.
    fn window_title(&self) -> String {
        let vault = vault_title(self.vault.root());
        match self.note() {
            Some(path) => {
                let note = path.file_stem().unwrap_or_default().to_string_lossy();
                format!("{vault}{TITLE_GAP}{note}")
            }
            None => vault,
        }
    }

    /// Give the window its title, if this frame changed it.
    ///
    /// Called from `render` rather than from every place a tab opens, closes or
    /// moves: the title describes the frame, and the frame is the only thing that
    /// knows what it is showing. The guard is not tidiness -- `set_window_title`
    /// reaches the window manager, so an unguarded call would be a conversation
    /// with the desktop every frame.
    fn sync_window_title(&mut self, window: &mut Window) {
        let title = self.window_title();
        if self.last_window_title.as_deref() == Some(title.as_str()) {
            return;
        }
        window.set_window_title(&title);
        self.last_window_title = Some(title);
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The frame the window sits in, kept current: render is the one place
        // a `Window` is within reach of the session's save paths, and copying
        // five floats a frame is cheaper than any resize subscription.
        self.last_bounds = Some(Workspace::geom_of(window));
        // A tab change that had no window to rehook the hover subscription with gets
        // it here, one frame later, rather than never: the pointer can only hover over
        // the note that is drawn, so the subscription has to be the drawn tab's. The
        // keyboard follows the same rule, for the same reason.
        if self.hover_stale {
            cx.on_next_frame(window, |ws, window, cx| ws.watch_hover(window, cx));
        }
        if self.focus_stale {
            cx.on_next_frame(window, |ws, window, cx| ws.focus_active_tab(window, cx));
        }
        // The same one-frame deferral for a neighbourhood that moved with the note: the
        // picture is re-made outside whatever borrow the tab change arrived in.
        if self.graph_follow_pending {
            cx.on_next_frame(window, |ws, _window, cx| {
                ws.graph_follow_pending = false;
                ws.refresh_graph(cx);
            });
        }
        // The live channel's snapshot is kept current by drawing: a window publishes
        // itself on the frame in which whatever changed became visible, and an idle
        // window — which draws nothing, by the idle-park contract — publishes nothing.
        // The old 250 ms beat cost four wakeups a second at rest to learn that the
        // user had not moved. Only the window the app considers active publishes, so
        // the answer stays the one the user is looking at; when no window is active
        // (an unfocused app, a test context) the window drawing is the answer.
        let active = cx.active_window().map(|w| w.window_id());
        if active.is_none() || active == Some(window.window_handle().window_id()) {
            self.publish_live(cx);
        }
        self.sync_window_title(window);
        let tree = tree::build_tree(&self.entries, &self.dirs);
        let active = self.note();
        let selected = self.tree_selection.clone();
        let expanded = &self.expanded;
        // The keyboard selection is only drawn while the sidebar has the
        // keyboard, so a stale selection does not look like the cursor.
        let tree_focused = self.tree_focus.is_focused(window);
        let selected_row = if tree_focused {
            selected.as_ref()
        } else {
            None
        };

        // Build sidebar rows.
        let mut rows: Vec<gpui::AnyElement> = Vec::new();
        // The paths of the rows in the order they are drawn, which is what the
        // keyboard motions step through.
        let mut paths: Vec<PathBuf> = Vec::new();
        let mut idx = 0usize;
        let state = TreeState {
            active: &active,
            selected: selected_row,
            expanded,
            palette: &self.palette,
        };
        for node in &tree {
            push_node(&mut rows, &mut paths, node, 0, &state, &mut idx, cx);
        }
        self.tree_rows = paths;
        let prompt = self
            .prompt
            .as_ref()
            .filter(|(kind, _)| {
                !matches!(
                    kind,
                    prompt::PromptKind::Command | prompt::PromptKind::Search
                )
            })
            .map(|(_, prompt)| prompt.clone());
        // vim's command line and its `/` search both ask for something to be typed
        // at the bottom of the note rather than a name for a file, so both sit where
        // vim puts them — over the bottom of the note, not in the sidebar with the
        // prompts that name a note.
        let bottom_field = self
            .prompt
            .as_ref()
            .filter(|(kind, _)| {
                matches!(
                    kind,
                    prompt::PromptKind::Command | prompt::PromptKind::Search
                )
            })
            .map(|(_, prompt)| prompt.clone());
        let picker = self.picker.clone();
        // The popup is drawn last, over the note, where its line was painted.
        // Asking each frame is what lets it follow the note as it scrolls; a line
        // that is no longer on screen means no panel, which is the right answer
        // rather than a panel floating over the wrong word. The hover path asks the
        // same question rather than reading what this frame drew, because a pointer
        // move can arrive between frames — and near the bottom of the window the
        // panel has moved to above its line by then.
        let placed = self.popup_place(window, cx).map(|placed| placed.bounds);
        // A panel with nowhere to be drawn is a panel nothing renders — and the
        // keyboard is still pointed at it, because the element that would give it
        // keys, and the note's keys with it, is not in the tree. So it goes here,
        // and the `pending_focus` below catches the refocus in this same frame.
        if self.popup.is_some() && placed.is_none() {
            self.drop_popup(cx);
        }
        let popup = self.popup_entity().zip(placed);
        // The card follows the same rule one step at a time: asked each frame, dropped
        // when its line is off screen. It has no keyboard to hand back, so dropping it is
        // only about the picture.
        let card_placed = self.card_place(window, cx).map(|placed| placed.bounds);
        if self.card.is_some() && card_placed.is_none() {
            self.close_card(cx);
        }
        let card = self.card.clone().zip(card_placed);
        // Scrolling moves text under a pointer that never moved, and gpui reports no
        // mouse move for it — so a surface parked there is describing a word the pointer
        // is no longer on. Notice it here and act at the start of the next frame — the
        // frame that drew the scrolled text, which is the frame whose records are the
        // scrolled ones. Both halves need those records: the collision, and the surface's
        // place, which is read from the note's *last painted* rows and so is one scroll
        // behind in this frame. A panel asked for with `K` is the user asking, not the
        // pointer, so it is not re-asked.
        if self.card.is_some() || (self.popup.is_some() && self.popup_from_pointer) {
            let scrolled = self.editor().is_some_and(|editor| {
                editor.read_with(cx, |editor, _cx| editor.scroll_y()) != self.surface_scroll
            });
            if scrolled {
                if let Some(editor) = self.editor() {
                    self.surface_scroll = editor.read_with(cx, |editor, _cx| editor.scroll_y());
                }
                self.surface_reask = true;
            }
            if self.surface_reask {
                self.surface_reask = false;
                cx.on_next_frame(window, |ws, window, cx| ws.surface_scroll_frame(window, cx));
            }
        }
        // The one path that had to close a popup without a window to hand the
        // keyboard back with (the file watcher) left the refocus for here, which
        // is the first place there is a window. `defer` keeps the change out of
        // the draw itself.
        if let Some(focus) = self.pending_focus.take() {
            window.defer(cx, move |window, cx| focus.focus(window, cx));
        }

        // The status bar's note segments. With the graph tab on screen there is no
        // buffer to report a mode, a save dot, a cursor or a word count for, so each is
        // empty rather than a stale number from a note that is not drawn. The view
        // badge is the one segment that changes, and it says what *is* on screen.
        let editor = self.editor().cloned();
        let mode = editor
            .as_ref()
            .map(|editor| mode_label(editor.read(cx).mode()))
            .unwrap_or("");
        let dirty = editor
            .as_ref()
            .map(|editor| save_indicator(editor.read(cx).is_dirty()))
            .unwrap_or("");
        let pending = editor
            .as_ref()
            .map(|editor| editor.read(cx).pending_hint())
            .unwrap_or_default();
        let (line, col) = editor
            .as_ref()
            .map(|editor| editor.read(cx).display_position())
            .unwrap_or((0, 0));
        // Which of the three views the note is in, or `None` for the graph tab. All
        // three are named, so the slot does not appear and disappear as the user moves
        // between them; the default one is drawn quiet, because a badge for the
        // state the note is nearly always in should not compete with the two that
        // mean something is different.
        let view = editor.as_ref().map(|editor| editor.read(cx).view());
        // Focus mode gets a badge only while it is on, and names its scope with
        // it, because "dimmed" and "dimmed to the whole section" are different
        // things to see. Unlike the view badge this slot comes and goes: a mode
        // that is nearly always off would only add a word to every status line.
        let focus = match &editor {
            Some(editor) if editor.read(cx).focus_mode() => {
                format!("focus {}", editor.read(cx).focus_scope().word())
            }
            _ => String::new(),
        };
        // The checker's count, so "no underlines" can be told apart from "the
        // checker has not answered yet". Preview draws no underlines at all, so it
        // has nothing to count either, and the graph tab has no note to check.
        let spell = match &editor {
            Some(editor) if editor.read(cx).view() == View::Preview => String::new(),
            Some(_) => match self.checker_notes.len() {
                0 => String::new(),
                1 => "1 spellcheck note".into(),
                n => format!("{n} spellcheck notes"),
            },
            None => String::new(),
        };
        // Which match of how many the cursor is on, while a `/` search is up.
        let search = self.search_label();
        // The open note's own word count, live from the buffer. It is state, not a
        // message: it is true for exactly as long as it is shown, and it moves with
        // the typing rather than a save later.
        // Which embed the caret is inside, if any. A state segment, not a message:
        // it is true exactly as long as it is true, and it is the fact the user needs
        // *before* the keystroke they are about to type lands in another note.
        let embed = match &editor {
            Some(editor) => editor
                .read(cx)
                .entered_name()
                .map(|stem| format!("in ![[{stem}]]"))
                .unwrap_or_default(),
            None => String::new(),
        };
        let words = word_badge(
            self.note().is_some(),
            editor
                .as_ref()
                .map(|editor| editor.read(cx).word_count())
                .unwrap_or(0),
        );

        div()
            .track_focus(&self.focus_handle)
            .size_full()
            .flex()
            .flex_col()
            // A gutter between Shale's chrome and the window edge: the focused
            // pane's rule and the status bar would otherwise sit under the
            // compositor's own border and get clipped by its rounded corners.
            .p_1()
            // And air between the cards — sidebar, tiling, status bar — so the
            // rounded corners read as separate cards, not one shape with nicks.
            .gap_1()
            .bg(gpui::rgb(self.palette.ui_bg))
            .text_color(gpui::rgb(self.palette.ui_fg))
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::quit))
            .on_action(cx.listener(Self::quit_forced))
            // Into the embed under the caret (`ctrl-]`); `ctrl-o` walks back out, and
            // that handler already exists as the jump chain (roadmap P4.0b).
            .on_action(cx.listener(Self::enter_embed_key))
            // The vault edits are available from anywhere: the tree's selection
            // says what they act on, and the open note is the fallback.
            .on_action(cx.listener(Self::new_note))
            .on_action(cx.listener(Self::new_folder))
            .on_action(cx.listener(Self::rename_note))
            .on_action(cx.listener(Self::delete_note))
            // A frontmatter block for the note under the caret. On the window's
            // chain rather than the tree's, because it writes a note and not a
            // selection: with the tree focused it still means the note the
            // keyboard is on.
            .on_action(cx.listener(Self::add_properties))
            .on_action(cx.listener(Self::focus_tree))
            .on_action(cx.listener(Self::focus_editor))
            .on_action(cx.listener(Self::window_left))
            .on_action(cx.listener(Self::window_right))
            .on_action(cx.listener(Self::window_split_down))
            .on_action(cx.listener(Self::window_split_right))
            .on_action(cx.listener(Self::window_split_down_here))
            .on_action(cx.listener(Self::window_split_right_here))
            .on_action(cx.listener(Self::window_close))
            .on_action(cx.listener(Self::window_only))
            .on_action(cx.listener(Self::window_up))
            .on_action(cx.listener(Self::window_down))
            .on_action(cx.listener(Self::window_equalize))
            .on_action(cx.listener(Self::window_grow))
            .on_action(cx.listener(Self::window_shrink))
            .on_action(cx.listener(Self::tab_next))
            .on_action(cx.listener(Self::tab_prev))
            .on_action(cx.listener(Self::tab_new))
            .on_action(cx.listener(Self::tab_close))
            .on_action(cx.listener(Self::tab_only))
            .on_action(cx.listener(Self::quick_open))
            .on_action(cx.listener(Self::jump_back))
            .on_action(cx.listener(Self::jump_forward))
            .on_action(cx.listener(Self::search_vault))
            .on_action(cx.listener(Self::search_buffer))
            .on_action(cx.listener(Self::search_next))
            .on_action(cx.listener(Self::search_prev))
            .on_action(cx.listener(Self::show_note))
            .on_action(cx.listener(Self::show_raw))
            .on_action(cx.listener(Self::show_live))
            .on_action(cx.listener(Self::show_preview))
            .on_action(cx.listener(Self::toggle_focus))
            .on_action(cx.listener(Self::toggle_typewriter))
            // The sidebar is handled on the window for the same reason the graph is:
            // `space e` starts in the note, and the palette and the command line reach
            // it too.
            .on_action(cx.listener(Self::toggle_tree))
            .on_action(cx.listener(Self::command_palette))
            .on_action(cx.listener(Self::command_line))
            .on_action(cx.listener(Self::change_theme))
            .on_action(cx.listener(Self::change_vault))
            .on_action(cx.listener(Self::new_window))
            .on_action(cx.listener(|ws, _: &CheckerOn, _window, cx| ws.set_checker(true, cx)))
            .on_action(cx.listener(|ws, _: &CheckerOff, _window, cx| ws.set_checker(false, cx)))
            .on_action(cx.listener(Self::copy_text))
            .on_action(cx.listener(Self::show_wordcount))
            .on_action(cx.listener(Self::show_props))
            .on_action(cx.listener(Self::show_property_keys))
            .on_action(cx.listener(Self::show_rename_key))
            // Agent edit review (roadmap P2.5a). Handled here rather than on the strip
            // itself because these commands are reachable from three doors at once: the
            // strip's own keys, `]a`/`[a` in the note, and the palette.
            .on_action(cx.listener(Self::review_focus))
            .on_action(cx.listener(Self::review_next))
            .on_action(cx.listener(Self::review_prev))
            .on_action(cx.listener(Self::review_accept))
            .on_action(cx.listener(Self::review_reject))
            .on_action(cx.listener(Self::review_open_note))
            .on_action(cx.listener(Self::review_collapse))
            // The graph tab (roadmap P3.4). Handled on the window rather than on the
            // view because the command is reachable from the note's `space g` chord,
            // the palette and the command line as well as from the graph itself.
            .on_action(cx.listener(Self::show_graph))
            // Clicking off the picker or the command line closes it. Both float over
            // the note, and a click under them used to belong to nothing at all, so
            // they simply stayed up until `escape`. A click inside a panel never
            // reaches here: the panel occludes everything behind it, this handler
            // among them, and stops the click on top of that.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event: &gpui::MouseDownEvent, window, cx| {
                    this.dismiss_overlays(window, cx);
                }),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .gap_1()
                    .when(self.tree_visible, |d| {
                        d.child(
                            div()
                                .flex_none()
                                .w(px(260.))
                                .h_full()
                                .flex()
                                .flex_col()
                                // The tree is a card too: the chrome surface
                                // (same family as the tab bars and the status
                                // bar), rounded like the panes. Not `bg` — in
                                // some themes that equals the window's own
                                // colour and the card vanishes. The old
                                // right-edge divider goes: the gap between
                                // cards is the separation now.
                                .bg(gpui::rgb(self.palette.ui_status_bg))
                                .rounded(crate::style::PANE_RADIUS)
                                .child(
                                    div()
                                        .p_2()
                                        .pb_1()
                                        .font_weight(gpui::FontWeight::BOLD)
                                        .child(
                                            self.vault
                                                .root()
                                                .file_name()
                                                .unwrap_or_default()
                                                .to_string_lossy()
                                                .to_string(),
                                        ),
                                )
                                // The prompt is a sibling of the tree, not a child of
                                // it: gpui enables a context's bindings whenever that
                                // context is in the focused element's stack, so a prompt
                                // *inside* the tree would still take the tree's `j`/`k`.
                                .when_some(prompt, |d, prompt| {
                                    d.child(div().px_2().pb_2().child(prompt))
                                })
                                .child(
                                    div()
                                        .id("tree-scroll")
                                        .flex_1()
                                        .min_h_0()
                                        .overflow_scroll()
                                        .key_context(tree::CONTEXT)
                                        .track_focus(&self.tree_focus)
                                        .on_action(cx.listener(
                                            |this, _: &TreeDown, _window, cx| {
                                                this.move_tree_selection(1, cx)
                                            },
                                        ))
                                        .on_action(cx.listener(|this, _: &TreeUp, _window, cx| {
                                            this.move_tree_selection(-1, cx)
                                        }))
                                        .on_action(cx.listener(|this, _: &TreeOpen, window, cx| {
                                            this.open_tree_row(window, cx)
                                        }))
                                        .on_action(cx.listener(
                                            |this, _: &TreeCollapse, _window, cx| {
                                                this.tree_collapse(cx)
                                            },
                                        ))
                                        .on_action(cx.listener(
                                            |this, _: &TreeExpand, _window, cx| {
                                                this.tree_expand(cx)
                                            },
                                        ))
                                        .on_action(cx.listener(Self::focus_editor))
                                        .on_action(cx.listener(Self::new_note))
                                        .on_action(cx.listener(Self::new_folder))
                                        .on_action(cx.listener(Self::rename_note))
                                        .on_action(cx.listener(Self::delete_note))
                                        .p_2()
                                        .pt_1()
                                        .text_size(crate::style::TEXT_SM)
                                        .children(rows),
                                ),
                        )
                    })
                    // The panes (roadmap P3.2): the tiling tree, each leaf drawing
                    // its own tab bar over its own note. The overlays float over the
                    // whole tiling rather than one pane: a picker is one thing the
                    // window owns, and so is the command line.
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .relative()
                            .flex()
                            .child(self.render_node(&self.layout, cx))
                            // A picker *overlays* the note rather than pushing it
                            // down: it is painted after the text, so opening one does
                            // not re-wrap every line underneath. Like the prompt it is
                            // a sibling of the editor, so the vim contexts stay out of
                            // its way.
                            //
                            // Both overlays occlude what is behind them. gpui's hit
                            // test walks the hitboxes under the pointer from the
                            // topmost down and stops at one that blocks the mouse, so
                            // this is what keeps the note *under* the panel from
                            // answering the pointer: a harper panel used to open
                            // itself underneath the picker — two panels at once, and
                            // neither answering for the other — and the note's IBeam
                            // showed through the panel.
                            //
                            // A click inside a panel stops there too. The window's own
                            // mouse-down handler (below) dismisses an overlay, and it
                            // sits *behind* this one, so the occlusion already keeps it
                            // from seeing a click inside the panel; the explicit stop is
                            // here so that rule does not depend on how the occlusion is
                            // spelled. Either one alone is enough — the test for it
                            // fails only when both are off.
                            .when_some(picker, |d, picker| {
                                d.child(
                                    div()
                                        .absolute()
                                        .top_2()
                                        .left_4()
                                        .right_4()
                                        .occlude()
                                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                            cx.stop_propagation();
                                        })
                                        .child(picker),
                                )
                            })
                            .when_some(bottom_field, |d, field| {
                                d.child(
                                    div()
                                        .absolute()
                                        .bottom_2()
                                        .left_4()
                                        .right_4()
                                        .occlude()
                                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                            cx.stop_propagation();
                                        })
                                        .child(field),
                                )
                            }),
                    ),
            )
            // The review strip: the last thing before the status bar, so it sits
            // **above** it, and only when something is waiting (roadmap P2.5a). The
            // status bar owns facts about the whole note and is left alone, and the
            // note never shifts once the strip is up: the strip is `flex_none`, and the
            // note column above it takes what is left.
            //
            // It carries no border of its own here. The strip is raised
            // (`design-language.md` §5) and draws its own `popup_border` on all four
            // sides, so a rule here would be a second line one pixel below the first.
            .when(self.review_active(cx), |d| {
                d.child(div().flex_none().w_full().child(self.review.clone()))
            })
            .child(
                div()
                    .h(px(24.))
                    .px_2()
                    .flex()
                    .items_center()
                    .gap_4()
                    .text_size(crate::style::TEXT_XS)
                    .bg(gpui::rgb(self.palette.ui_status_bg))
                    .rounded(crate::style::PANE_RADIUS)
                    .text_color(gpui::rgb(self.palette.ui_fg))
                    .child(format!("{mode}{pending}"))
                    // The view gets its own badge, and it is the only place the view
                    // is named: the command line deliberately leaves the status line
                    // alone, so `:preview` is one accent label rather than the same
                    // word twice. It matters most for preview, which looks almost
                    // exactly like live preview and simply refuses to type — `space l`
                    // or `:live` is how the user gets back. Live is the resting state,
                    // so it is drawn in the chrome's quiet colour rather than the
                    // accent, which is reserved for the two views that change what the
                    // note will do. The graph tab has no view, so the same slot names
                    // what is on screen instead — in the quiet colour, because the
                    // graph is the resting state of the tab it is on.
                    .child(
                        div()
                            .text_color(gpui::rgb(match view {
                                Some(View::Raw) | Some(View::Preview) => self.palette.accent,
                                // Live is the resting state, and the graph tab has no
                                // view at all: neither is news, so neither is accented.
                                _ => self.palette.ui_dim,
                            }))
                            .child(match view {
                                Some(view) => view.word().to_string(),
                                None => "graph".to_string(),
                            }),
                    )
                    .when(!focus.is_empty(), |d| {
                        d.child(
                            div()
                                .text_color(gpui::rgb(self.palette.accent))
                                .child(focus),
                        )
                    })
                    // The save state gets its own element and the palette's
                    // accent rather than riding along in the mode string: it is
                    // the one part of the bar the user has to read without
                    // looking for it, and it is what tells them an autosave
                    // happened.
                    .when(!dirty.is_empty(), |d| {
                        d.child(
                            div()
                                .text_color(gpui::rgb(self.palette.accent))
                                .child(dirty),
                        )
                    })
                    .when(!self.status.is_empty(), |d| {
                        // A message only takes a slot when there is one. An empty
                        // child is still a flex item, so leaving it in put a gap on
                        // both sides of nothing and split the bar with a wider space
                        // than anywhere else in it.
                        d.child(self.status.clone())
                    })
                    .when(!embed.is_empty(), |d| {
                        // Accent, not quiet: this is the one segment that says the
                        // caret is not where the tab says it is.
                        d.child(
                            div()
                                .text_color(gpui::rgb(self.palette.accent))
                                .child(embed),
                        )
                    })
                    .when(!spell.is_empty(), |d| d.child(spell))
                    // The search's own readout, so the count does not overwrite the
                    // status line the way a transient message would.
                    .when(!search.is_empty(), |d| {
                        d.child(
                            div()
                                .text_color(gpui::rgb(self.palette.accent))
                                .child(search),
                        )
                    })
                    .when(!words.is_empty(), |d| {
                        // The count sits next to the cursor position at the right end
                        // of the bar, and in the chrome's quiet colour: it is a number
                        // to glance at, not news, and the accent is reserved for the
                        // segments that mean something is different.
                        d.child(
                            div()
                                .text_color(gpui::rgb(self.palette.ui_dim))
                                .child(words),
                        )
                    })
                    .child(format!("{}:{}", line + 1, col + 1)),
            )
            .when_some(card, |d, (card, bounds)| {
                d.child(
                    anchored()
                        .position(bounds.origin)
                        .position_mode(AnchoredPositionMode::Window)
                        .snap_to_window()
                        .child(card),
                )
            })
            .when_some(popup, |d, (popup, bounds)| {
                // `anchored` paints at a window coordinate rather than in the
                // layout, which is what lets the panel sit over the note without
                // pushing the note down; snapping to the window's edge keeps a
                // popup near the bottom or the right edge on screen.
                d.child(
                    anchored()
                        .position(bounds.origin)
                        .position_mode(AnchoredPositionMode::Window)
                        .snap_to_window()
                        .child(popup),
                )
            })
    }
}
