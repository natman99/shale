//! The readability rules a palette has to satisfy, and the measuring that
//! checks them.
//!
//! This lives in `core` rather than in the theme tests because the same
//! judgement has to run twice: once over the built-ins, where a failure is a
//! bug and fails the build, and once over a vault's own theme file, where a
//! failure is the user's colour choice and only earns a line in the status bar
//! (`design-language.md` §10). Two copies of the table would drift, and the
//! copy that drifted would be the one nobody tests.
//!
//! The measure is the WCAG contrast ratio: the two colours' relative
//! luminances, offset and divided. It catches a theme whose text matches its
//! background and a colour so close to its surface that the difference is
//! invisible. It does not certify a palette, and it is not asked to: a theme
//! author who likes a low-contrast look is free to keep it, which is why the
//! loader warns rather than refuses.

use super::Palette;

/// WCAG relative luminance: the sRGB channels linearized and combined with
/// the Rec. 709 weights. The channels have to be linearized first; weighting
/// the raw 0-1 values (the obvious shortcut) under-reports mid-tone contrast
/// badly enough to fail a palette that is perfectly readable -- Catppuccin
/// Latte's text on its own background is 7:1, which the shortcut calls 2.7:1.
pub fn luminance(rgb: u32) -> f32 {
    let channel = |byte: u32| {
        let c = byte as f32 / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel((rgb >> 16) & 0xff)
        + 0.7152 * channel((rgb >> 8) & 0xff)
        + 0.0722 * channel(rgb & 0xff)
}

/// The WCAG contrast ratio of two colours: 1 is identical, 21 is black on
/// white, and 4.5 is the usual floor for body text.
pub fn contrast(a: u32, b: u32) -> f32 {
    let x = luminance(a).max(luminance(b));
    let y = luminance(a).min(luminance(b));
    (x + 0.05) / (y + 0.05)
}

/// One way a palette fails a rule. Says enough for a status bar line and for a
/// test failure: what the colour is for, what it is drawn against, and by how
/// much it missed.
#[derive(Debug, Clone, PartialEq)]
pub enum Flag {
    /// A colour too close to the surface it is drawn on to read.
    TooFaint {
        /// What the colour is for, in the words a theme author would recognise.
        what: &'static str,
        /// The surface it is drawn on.
        on: &'static str,
        /// The ratio it measured.
        ratio: f32,
        /// The ratio it had to clear.
        floor: f32,
    },
    /// Two things a reader has to tell apart were given one colour.
    SameColour {
        /// The first role.
        a: &'static str,
        /// The second role.
        b: &'static str,
    },
    /// A control that cannot be used reads as more important than a label.
    DisabledTooLoud {
        /// The disabled control's ratio against the window.
        ratio: f32,
        /// The quiet label's ratio against the same window.
        label: f32,
    },
}

impl std::fmt::Display for Flag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Flag::TooFaint {
                what,
                on,
                ratio,
                floor,
            } => write!(
                f,
                "{what} is too faint on {on} ({ratio:.1}:1, needs {floor:.1}:1)"
            ),
            Flag::SameColour { a, b } => write!(f, "{a} and {b} are the same colour"),
            Flag::DisabledTooLoud { ratio, label } => write!(
                f,
                "a disabled control is louder than a label ({ratio:.1}:1 against {label:.1}:1)"
            ),
        }
    }
}

/// A colour role, the surface it is drawn on, and the ratio it has to clear.
///
/// The floors are the WCAG body-text floor for the roles a reader has to parse
/// (note text, headings, links, tags, code) and a lower bar for the roles that
/// are meant to recede (syntax markers, unresolved links, quoted text,
/// comments) or only to be *distinguishable* rather than read (a rule, a tab's
/// edge, a graph's edges). Roles nothing paints yet are still judged, so a
/// reserved colour cannot quietly be a useless one.
fn roles(p: &Palette) -> Vec<(&'static str, u32, &'static str, u32, f32)> {
    vec![
        ("note text", p.fg, "the note background", p.bg, 4.5),
        ("heading text", p.heading, "the note background", p.bg, 4.5),
        ("a resolved link", p.link, "the note background", p.bg, 3.0),
        ("a tag", p.tag, "the note background", p.bg, 3.0),
        ("quoted text", p.quote, "the note background", p.bg, 2.5),
        ("syntax markers", p.dim, "the note background", p.bg, 2.0),
        (
            "an unresolved link",
            p.link_unresolved,
            "the note background",
            p.bg,
            2.0,
        ),
        (
            "the checker's squiggle",
            p.spell,
            "the note background",
            p.bg,
            2.0,
        ),
        (
            "ordinary code",
            p.code_text,
            "the code block",
            p.code_block_bg,
            4.0,
        ),
        (
            "a code comment",
            p.code_comment,
            "the code block",
            p.code_block_bg,
            2.0,
        ),
        // A token colour only has to be distinguishable from the code around
        // it: `code_text` carries the body-text floor, and every family's own
        // syntax theme is the authority on its tokens.
        (
            "a keyword",
            p.code_keyword,
            "the code block",
            p.code_block_bg,
            2.0,
        ),
        (
            "a string",
            p.code_string,
            "the code block",
            p.code_block_bg,
            2.0,
        ),
        (
            "a number",
            p.code_number,
            "the code block",
            p.code_block_bg,
            2.0,
        ),
        (
            "a function name",
            p.code_function,
            "the code block",
            p.code_block_bg,
            2.0,
        ),
        (
            "a type name",
            p.code_type,
            "the code block",
            p.code_block_bg,
            2.0,
        ),
        (
            "a constant",
            p.code_const,
            "the code block",
            p.code_block_bg,
            2.0,
        ),
        (
            "an operator",
            p.code_operator,
            "the code block",
            p.code_block_bg,
            2.0,
        ),
        ("window text", p.ui_fg, "the window", p.ui_bg, 4.5),
        ("a hint line", p.ui_dim, "the window", p.ui_bg, 2.0),
        (
            "a focused rule",
            p.ui_active_border,
            "the window",
            p.ui_bg,
            1.2,
        ),
        ("a tab", p.ui_tab_bg, "the window", p.ui_bg, 1.1),
        (
            "the open tab",
            p.ui_tab_active_bg,
            "the window",
            p.ui_bg,
            1.25,
        ),
        ("the panel", p.popup_bg, "the note", p.bg, 1.1),
        (
            "the panel's rule",
            p.popup_border,
            "the panel",
            p.popup_bg,
            1.2,
        ),
        ("a graph node", p.graph_node, "the note", p.bg, 1.5),
        (
            "the focused node",
            p.graph_node_active,
            "the note",
            p.bg,
            2.0,
        ),
        ("a graph edge", p.graph_edge, "the note", p.bg, 1.1),
        ("a node label", p.graph_text, "the note", p.bg, 2.5),
        ("a ghost node", p.graph_ghost, "the note", p.bg, 1.5),
        ("a tag node", p.graph_tag, "the note", p.bg, 1.5),
        (
            "an attachment node",
            p.graph_attachment,
            "the note",
            p.bg,
            1.5,
        ),
        ("the agent's mark", p.agent, "the window", p.ui_bg, 2.0),
        (
            "a disabled control",
            p.ui_disabled,
            "the window",
            p.ui_bg,
            1.5,
        ),
        // The review strip is chrome, so its three line colours are judged on
        // the window rather than on the note they describe.
        (
            "a proposed addition",
            p.diff_add,
            "the window",
            p.ui_bg,
            2.0,
        ),
        (
            "a proposed deletion",
            p.diff_delete,
            "the window",
            p.ui_bg,
            2.0,
        ),
        (
            "a proposed change",
            p.diff_changed,
            "the window",
            p.ui_bg,
            2.0,
        ),
    ]
}

/// A text role, the background role it is drawn on, and the ratio that pair
/// has to clear.
///
/// A highlighted row gets a lower floor than body text: it is transient, its
/// text is short, and it has to be *visible* against the surface it sits on,
/// which for a palette whose text is a mid-tone pulls the other way.
/// Solarized's own tone table rates body text on its secondary background as
/// less than optimal reading, which is why the cursor's line is held to 4.0
/// rather than 4.5.
/// The callout colours, judged on both surfaces they are drawn on.
///
/// A callout's type is carried by colour and a small mark, and the palette answers
/// fourteen kinds with ten colours — so for pairs like `warning`/`failure` these roles are
/// close to the whole signal. Nothing audited them at all until now, which is how that
/// stayed unnoticed.
///
/// Each colour is judged twice: the 2 px bar against the box it is painted on, and the
/// kind mark against the note, because the mark hangs in the gutter outside the box.
///
/// Walked from [`Palette::callout_accent`] rather than written out by hand, so a new
/// callout kind is audited the moment it exists and a palette edit that re-points a kind
/// is judged on the colour it actually uses.
fn callouts(p: &Palette) -> Vec<(&'static str, u32, &'static str, u32, f32)> {
    use crate::mdstyle::CalloutKind;
    const KINDS: &[(CalloutKind, &str)] = &[
        (CalloutKind::Note, "a note callout"),
        (CalloutKind::Info, "an info callout"),
        (CalloutKind::Todo, "a todo callout"),
        (CalloutKind::Abstract, "an abstract callout"),
        (CalloutKind::Example, "an example callout"),
        (CalloutKind::Tip, "a tip callout"),
        (CalloutKind::Success, "a success callout"),
        (CalloutKind::Question, "a question callout"),
        (CalloutKind::Warning, "a warning callout"),
        (CalloutKind::Failure, "a failure callout"),
        (CalloutKind::Danger, "a danger callout"),
        (CalloutKind::Bug, "a bug callout"),
        (CalloutKind::Quote, "a quote callout"),
        (CalloutKind::Other, "an unknown-type callout"),
    ];
    // A 2 px rule and an 8 px glyph are both thin marks, so they are held to the same bar
    // a graph node's kind is: visible without shouting over the text beside them.
    let surfaces = [
        ("a callout's background", p.callout_bg, 1.5),
        ("the note", p.bg, 1.5),
    ];
    let mut out: Vec<(&'static str, u32, &'static str, u32, f32)> = Vec::new();
    for (kind, name) in KINDS {
        let colour = p.callout_accent(*kind);
        for (on, against, floor) in surfaces {
            // Several kinds share one colour on purpose; the flag should name one of them
            // rather than repeat itself four times on the same pair.
            if out
                .iter()
                .any(|(_, seen, seen_on, _, _)| *seen == colour && *seen_on == on)
            {
                continue;
            }
            out.push((name, colour, on, against, floor));
        }
    }
    out
}

fn pairs(p: &Palette) -> Vec<(&'static str, u32, &'static str, u32, f32)> {
    vec![
        (
            "the status bar's text",
            p.ui_fg,
            "the status bar",
            p.ui_status_bg,
            4.5,
        ),
        (
            "what you type",
            p.ui_fg,
            "the prompt field",
            p.ui_field_bg,
            4.5,
        ),
        (
            "the open tab's label",
            p.ui_fg,
            "the open tab",
            p.ui_tab_active_bg,
            3.0,
        ),
        ("the panel's text", p.ui_fg, "the panel", p.popup_bg, 4.5),
        (
            "a panel row's text",
            p.ui_fg,
            "the highlighted row",
            p.popup_hover_bg,
            3.5,
        ),
        // A picker's rows are drawn on the panel, and the chosen one on the
        // panel's hover role. Matched characters are short, so they get the floor
        // for short text on an ordinary row and a lower one on the highlighted
        // row, where the rest of the row is still body text at full contrast.
        (
            "a query's matched characters",
            p.match_text,
            "a picker row",
            p.popup_bg,
            3.0,
        ),
        (
            "a query's matched characters",
            p.match_text,
            "the chosen row",
            p.popup_hover_bg,
            2.5,
        ),
        // The row's path, the panel's label and its footer are all the quiet
        // colour, now drawn on the raised panel rather than on the window.
        ("a panel row's path", p.ui_dim, "the panel", p.popup_bg, 2.0),
        (
            "a tab's label",
            p.ui_dim,
            "an inactive tab",
            p.ui_tab_bg,
            2.0,
        ),
        (
            "text on the cursor's line",
            p.fg,
            "the cursor's line",
            p.cursor_line_bg,
            4.0,
        ),
    ]
}

/// Every way `palette` fails the readability rules. Empty means it reads.
///
/// Cheap enough to run every time a palette is resolved: a few dozen ratios,
/// once per theme load, not per frame.
pub fn audit(palette: &Palette) -> Vec<Flag> {
    let mut flags = Vec::new();
    for (what, colour, on, against, floor) in roles(palette)
        .into_iter()
        .chain(callouts(palette))
        .chain(pairs(palette))
    {
        let ratio = contrast(against, colour);
        if ratio <= floor {
            flags.push(Flag::TooFaint {
                what,
                on,
                ratio,
                floor,
            });
        }
    }
    // The cursor's line is a band, not a highlight: it has to be visible
    // without competing with the text on it.
    let band = contrast(palette.bg, palette.cursor_line_bg);
    if band <= 1.05 {
        flags.push(Flag::TooFaint {
            what: "the cursor's line",
            on: "the note background",
            ratio: band,
            floor: 1.05,
        });
    }
    // A node's kind is told by its colour, so the four kinds have to be four
    // colours: a tag that is the colour of every note node is not a tag.
    let kinds = [
        ("a note node", palette.graph_node),
        ("a ghost node", palette.graph_ghost),
        ("a tag node", palette.graph_tag),
        ("an attachment node", palette.graph_attachment),
    ];
    for (i, (what, colour)) in kinds.iter().enumerate() {
        for (other, other_colour) in &kinds[i + 1..] {
            if colour == other_colour {
                flags.push(Flag::SameColour { a: what, b: other });
            }
        }
    }
    // "You cannot have this" must never read as more important than a label.
    let disabled = contrast(palette.ui_bg, palette.ui_disabled);
    let label = contrast(palette.ui_bg, palette.ui_dim);
    if disabled > label + 0.001 {
        flags.push(Flag::DisabledTooLoud {
            ratio: disabled,
            label,
        });
    }
    flags
}
