//! Tests for what a frame paints: soft wrap and its break points, live-preview
//! hiding, markdown styling and the table grid.

use super::*;

/// Soft wrap: a long row must break into several display lines, and `j`
/// has to step across them (neovim-style) even though it is one buffer row.
#[gpui::test]
fn soft_wrap_steps_across_display_lines(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));

    let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);
    editor
        .update(cx, |editor, window, cx| {
            editor.set_text("word ".repeat(200), cx);
            let focus = editor.focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();

    // The first frame has no measured width yet, so paint twice.
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    let (rows, displays) = editor
        .read_with(cx, |editor, _cx| {
            (editor.line_count(), editor.display_line_count())
        })
        .unwrap();
    assert_eq!(rows, 1, "the note is a single buffer row");
    // 1000 characters at ~150 chars per line in a maximized test window.
    assert!(
        (4..=12).contains(&displays),
        "a long row should wrap into a sensible number of display lines, got {displays}"
    );

    cx.simulate_keystrokes("j");
    let (row, col) = editor
        .read_with(cx, |editor, _cx| editor.cursor_position())
        .unwrap();
    assert_eq!(row, 0, "still the same buffer row");
    assert!(
        (40..1000).contains(&col),
        "j should land on the next display line, got col {col}"
    );
}

/// Diagnostic: compare the wrap model against what shaping actually
/// produces, to see whether painted segments overflow the wrap width.
#[gpui::test]
fn wrap_model_versus_shaped_widths(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));

    let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);
    editor
        .update(cx, |editor, window, cx| {
            editor.set_text(
                "The quick brown fox jumps over the lazy dog. ".repeat(20),
                cx,
            );
            let focus = editor.focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    let (ranges, width, text) = editor
        .read_with(cx, |editor, _cx| {
            (editor.display_ranges(), editor.wrap_width(), editor.text())
        })
        .unwrap();
    let painted = editor
        .read_with(cx, |editor, _cx| editor.painted_lines())
        .unwrap();

    cx.update(|window, _cx| {
        let ts = window.text_system();
        let shaped = |s: &str| {
            let run = gpui::TextRun {
                len: s.len(),
                font: gpui::font("monospace"),
                color: gpui::black().into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            ts.shape_line(
                s.to_string().into(),
                px(editor::DEFAULT_FONT_SIZE),
                &[run],
                None,
            )
            .width()
        };
        println!("wrap width = {width:?}; display lines = {}", ranges.len());
        println!("painted lines from the last paint ({}):", painted.len());
        let mut overflow = 0;
        for (s, e, w) in &painted {
            let expected = shaped(&text[*s..*e]);
            if *w > width {
                overflow += 1;
            }
            let flag = if *w == expected { "ok" } else { "MISMATCH" };
            println!("  bytes {s}..{e} painted {w:?} vs monospace {expected:?} {flag}");
        }
        assert_eq!(overflow, 0, "no painted segment may exceed the wrap width");
    });
}

/// The wrap model must stay accurate for the text a markdown note actually
/// contains: tabs, accents, CJK (which wrap without spaces) and emoji.
#[gpui::test]
fn wrap_stays_accurate_for_tabs_and_unicode(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));

    let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);
    editor
        .update(cx, |editor, window, cx| {
            editor.set_text(
                "indented\tcode\tand héllo wörld 你好世界 🎉 emoji ".repeat(30),
                cx,
            );
            let focus = editor.focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    let (ranges, width, text) = editor
        .read_with(cx, |editor, _cx| {
            (editor.display_ranges(), editor.wrap_width(), editor.text())
        })
        .unwrap();
    let painted = editor
        .read_with(cx, |editor, _cx| editor.painted_lines())
        .unwrap();
    assert_eq!(ranges.len(), painted.len(), "model and paint agree");
    // The buffer's display-line index space must be the painted one, or the
    // cursor and `j`/`k` land on a different row than the user sees.
    for (model, paint) in ranges.iter().zip(&painted) {
        assert_eq!(
            (model.0, model.1),
            (paint.0, paint.1),
            "the buffer's display lines must be exactly the painted ones"
        );
    }

    cx.update(|window, _cx| {
        let ts = window.text_system();
        let shaped = |s: &str| {
            let run = gpui::TextRun {
                len: s.len(),
                font: gpui::font("monospace"),
                color: gpui::black().into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            ts.shape_line(
                s.to_string().into(),
                px(editor::DEFAULT_FONT_SIZE),
                &[run],
                None,
            )
            .width()
        };
        let mut widest = px(0.);
        for (i, (s, e, w)) in painted.iter().enumerate() {
            widest = widest.max(*w);
            assert!(
                *w <= width,
                "display line {i} is {w:?} wide, over the {width:?} wrap width"
            );
            // The painted line must measure exactly what the wrapper measured
            // for the same bytes, or the break points drift away from what
            // the user sees.
            assert_eq!(
                *w,
                shaped(&text[*s..*e]),
                "display line {i} was painted at a different width than the wrap model measured"
            );
        }
        println!(
            "{} display lines, widest {widest:?} of {width:?}",
            painted.len()
        );
        // Every character must be reachable: the segments have to tile the row.
        let ranges2 = ranges.clone();
        assert_eq!(ranges2.first().unwrap().0, 0);
        assert_eq!(ranges2.last().unwrap().1, text.len());
        for pair in ranges2.windows(2) {
            assert_eq!(pair[0].1, pair[1].0, "segments must tile the row");
        }
    });
}

/// A table is painted as a grid: one display line per row (never soft-wrapped,
/// however wide it is) and one painted piece per cell, so the buffer's display
/// model and the paint stay in step. The cursor is on the header row, which
/// brings the `---` delimiter row back as a line of its own.
#[gpui::test]
fn a_table_is_painted_cell_by_cell(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));

    let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);
    let wide = "x".repeat(200);
    editor
        .update(cx, |editor, window, cx| {
            editor.set_text(
                format!("| a | {wide} |\n| --- | --- |\n| 1 | 2 |\nplain line\n"),
                cx,
            );
            let focus = editor.focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    editor
        .update(cx, |editor, _window, _cx| {
            // The 200-character cell is far wider than the window, but a
            // table row is one grid row: wrapping it would put j/k out of
            // step with what is painted.
            assert_eq!(
                editor.display_line_count(),
                5,
                "the header, the rule row (the cursor is on the header, so it is drawn), the body row, the plain line and the trailing empty row"
            );
            assert_eq!(
                editor.painted_segments(),
                vec![2, 2, 2, 1, 1],
                "each table row paints one piece per column"
            );
        })
        .unwrap();
}

/// A table's `---` delimiter row is drawn as a line of its own while the cursor
/// is on the header above it (which is how its `:---:` markers get edited), and
/// collapses to nothing once the cursor moves into the body, where the table
/// reads as a grid again.
#[gpui::test]
fn a_table_delimiter_row_is_drawn_near_the_cursor(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));

    let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);
    editor
        .update(cx, |editor, window, cx| {
            editor.set_text(
                "| Note | Links to |\n| :--- | ---: |\n| [[Welcome]] | **bold** |\n".to_string(),
                cx,
            );
            let focus = editor.focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    // The cursor starts on the header row, so the rule row is drawn below it.
    let lines = editor
        .read_with(cx, |editor, _cx| editor.display_line_count())
        .unwrap();
    assert_eq!(
        lines, 4,
        "header, rule row, body row and the trailing empty row"
    );
    let pieces = editor
        .read_with(cx, |editor, _cx| editor.painted_pieces())
        .unwrap();
    assert_eq!(
        pieces[1],
        vec![":---", "---:"],
        "the rule row is painted as its own cells, alignment markers and all"
    );

    // shift-J is the cell-to-cell motion, so it steps over the rule row; the
    // cursor is now below it, and the row is gone.
    cx.simulate_keystrokes("J");
    let lines = editor
        .read_with(cx, |editor, _cx| editor.display_line_count())
        .unwrap();
    assert_eq!(
        lines, 3,
        "the rule row takes no space once the cursor leaves it"
    );
}

/// A fenced block's ``` lines are not drawn while the cursor is outside the
/// block, so the code reads as a block of text, and come back the moment the
/// cursor moves into it.
#[gpui::test]
fn a_fenced_block_collapses_its_fence_lines(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));

    let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);
    editor
        .update(cx, |editor, window, cx| {
            editor.set_text("before\n```\nlet x = 1;\n```\nafter\n".to_string(), cx);
            let focus = editor.focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    // The cursor is on `before`, outside the block: the two fence lines are
    // gone, and j steps straight from the prose into the code.
    let (lines, painted) = editor
        .read_with(cx, |editor, _cx| {
            (editor.display_line_count(), editor.painted_text())
        })
        .unwrap();
    assert_eq!(
        lines, 4,
        "before, the code line, after, and the trailing empty row"
    );
    assert_eq!(painted[1], "let x = 1;", "the code itself is untouched");

    cx.simulate_keystrokes("j");
    let (lines, painted) = editor
        .read_with(cx, |editor, _cx| {
            (editor.display_line_count(), editor.painted_text())
        })
        .unwrap();
    assert_eq!(
        lines, 6,
        "inside the block both fence lines are drawn again"
    );
    assert_eq!(painted[1], "```", "and stay editable");
}

/// A table cell is painted by the grid rather than as markdown, but a row the
/// cursor is not on still hides its markers: the user sees `Welcome`, not
/// `[[Welcome]]`. The delimiter row, drawn because the cursor is on the header,
/// keeps its alignment markers.
#[gpui::test]
fn a_table_cell_hides_its_markers(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));

    let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);
    editor
        .update(cx, |editor, window, cx| {
            editor.set_text(
                "| Note | Links to |\n| ---- | ---- |\n| [[Welcome]] | **bold** |\n".to_string(),
                cx,
            );
            let focus = editor.focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    editor
        .update(cx, |editor, _window, _cx| {
            let pieces = editor.painted_pieces();
            assert_eq!(
                pieces[0],
                vec!["Note", "Links to"],
                "the cursor's own row keeps its syntax"
            );
            assert_eq!(
                pieces[1],
                vec!["----", "----"],
                "the rule row is drawn as its own text while the cursor is on the header"
            );
            assert_eq!(
                pieces[2],
                vec!["Welcome", "bold"],
                "the grid hides the pipes and live preview hides the markers"
            );
        })
        .unwrap();
}

/// Live preview: a row the cursor is not on is painted without its markdown
/// markers, the cursor's row keeps them, and the buffer never changes.
#[gpui::test]
fn live_preview_hides_markers_off_the_cursor_row(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));

    let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);
    editor
        .update(cx, |editor, window, cx| {
            editor.set_text("# Heading\n\nSome **bold** and [[Note]].\n".to_string(), cx);
            let focus = editor.focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    // The cursor sits at the start of row 0, so that row still shows its
    // syntax and the rows around it read as their reader will see them.
    let painted = editor
        .read_with(cx, |editor, _cx| editor.painted_text())
        .unwrap();
    assert_eq!(
        painted[0], "# Heading",
        "the cursor's own row keeps its markers"
    );
    assert_eq!(
        painted[2], "Some bold and Note.",
        "an inactive row is painted without its markers"
    );

    // Moving the cursor flips which row shows its syntax.
    cx.simulate_keystrokes("j j");
    let (painted, text) = editor
        .read_with(cx, |editor, _cx| (editor.painted_text(), editor.text()))
        .unwrap();
    assert_eq!(
        painted[0], "Heading",
        "the row the cursor left has its markers hidden now"
    );
    assert_eq!(
        painted[2], "Some **bold** and [[Note]].",
        "the row the cursor is on shows its markers"
    );
    assert_eq!(
        text, "# Heading\n\nSome **bold** and [[Note]].\n",
        "hiding markers is a paint decision: the note keeps every byte"
    );
}

/// A thematic break is drawn as a rule across the editor rather than as the
/// dashes the file holds, and the dashes come back on the row the cursor is on.
#[gpui::test]
fn a_thematic_break_is_painted_as_a_rule(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));

    let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);
    editor
        .update(cx, |editor, window, cx| {
            editor.set_text("above\n\n---\n\nbelow\n".to_string(), cx);
            let focus = editor.focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    let (painted, rules) = editor
        .read_with(cx, |editor, _cx| {
            (editor.painted_text(), editor.painted_rules())
        })
        .unwrap();
    assert_eq!(painted[2], "", "the dashes themselves are not painted");
    assert_eq!(rules, vec![2], "the row is painted as a rule instead");

    // The cursor's own row shows its syntax again, so it can be edited.
    cx.simulate_keystrokes("j j");
    let (painted, rules) = editor
        .read_with(cx, |editor, _cx| {
            (editor.painted_text(), editor.painted_rules())
        })
        .unwrap();
    assert_eq!(painted[2], "---", "the cursor's row shows its dashes");
    assert!(rules.is_empty(), "no rule while the dashes are visible");
}

/// A callout is painted as a box that covers every row of the block, with its
/// `>` markers and `[!type]` header hidden on the rows the cursor is not on.
#[gpui::test]
fn a_callout_is_painted_as_a_box(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));

    let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);
    editor
        .update(cx, |editor, window, cx| {
            editor.set_text(
                "> [!warning] Watch out\n> mind the gap\n\nplain\n".to_string(),
                cx,
            );
            let focus = editor.focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    // The cursor starts on the title row, whose syntax stays visible so it can
    // be edited, while the body row is already painted as its reader sees it.
    let (painted, boxes) = editor
        .read_with(cx, |editor, _cx| {
            (editor.painted_text(), editor.painted_callouts())
        })
        .unwrap();
    assert_eq!(painted[0], "> [!warning] Watch out");
    assert_eq!(painted[1], " mind the gap");
    assert_eq!(
        boxes,
        vec![0, 1],
        "both rows of the block get the box, and no row outside it"
    );

    // With the cursor below the block, the header's brackets are hidden but the
    // type name is kept: it is the box's title.
    cx.simulate_keystrokes("j j");
    let (painted, boxes) = editor
        .read_with(cx, |editor, _cx| {
            (editor.painted_text(), editor.painted_callouts())
        })
        .unwrap();
    assert_eq!(
        painted[0], " warning Watch out",
        "the quote markers and the header's brackets are hidden"
    );
    assert_eq!(boxes, vec![0, 1]);
}

/// A click on a row whose markers are hidden has to land on the visible
/// character rather than on syntax that is not drawn, and the cursor and
/// selection quads have to be drawn where the text actually is.
#[gpui::test]
fn a_click_lands_on_visible_text(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));

    let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);
    editor
        .update(cx, |editor, window, cx| {
            editor.set_text("first line\n\na **b** c\n".to_string(), cx);
            let focus = editor.focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    // Row 2 is `a **b** c` painted as `a b c`: the `b` is at buffer offset 16
    // and the hidden `**` before it at 14.
    editor
        .update(cx, |editor, _window, _cx| {
            assert_eq!(editor.painted_text()[2], "a b c");
            let at_marker = editor.x_for_offset(2, 14).unwrap();
            assert_eq!(
                editor.offset_at_x(2, at_marker),
                Some(16),
                "clicking the gap where the hidden markers were lands on the `b`"
            );
            // A visible character round-trips exactly.
            let at_b = editor.x_for_offset(2, 16).unwrap();
            assert_eq!(editor.offset_at_x(2, at_b), Some(16));
            // The hidden markers occupy no room: both ends of the gap are the
            // same x, which is where the `b` is drawn.
            assert_eq!(at_marker, editor.x_for_offset(2, 15).unwrap());
        })
        .unwrap();
}

/// A folded heading carries a marker saying how many rows it hides, and clicking
/// the marker opens the section again instead of moving the cursor.
#[gpui::test]
fn a_folded_heading_is_marked_and_clickable(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));

    let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);
    editor
        .update(cx, |editor, window, cx| {
            editor.set_text("# Top\none\ntwo\n# Other".to_string(), cx);
            let focus = editor.focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    cx.simulate_keystrokes("z a");
    let (markers, painted) = editor
        .read_with(cx, |editor, _cx| {
            (editor.painted_fold_markers(), editor.painted_text())
        })
        .unwrap();
    assert_eq!(
        markers,
        vec![(0, " ⋯ 2 more lines".to_string())],
        "the heading row carries the count"
    );
    // The marker is not buffer text, so it is painted beside the row rather than
    // spliced into it: the painted line is still exactly the row.
    assert_eq!(painted[0], "# Top");

    let (index, x0, x1) = editor
        .read_with(cx, |editor, _cx| editor.fold_marker_bounds())
        .unwrap()
        .remove(0);
    assert_eq!(index, 0);
    assert!(x1 > x0, "the marker is wide enough to click");
    let (top, bottom) = editor
        .read_with(cx, |editor, _cx| editor.hit_line_edges(0))
        .unwrap()
        .unwrap();

    cx.simulate_click(
        gpui::point((x0 + x1) / 2., (top + bottom) / 2.),
        gpui::Modifiers::none(),
    );
    let lines = editor
        .read_with(cx, |editor, _cx| editor.display_line_count())
        .unwrap();
    assert_eq!(lines, 4, "clicking the marker opened the section");
}

/// Live markdown styling must not disturb the wrap model. Rows painted with
/// a wide face (headings, strong, quotes) have to be *measured* with that
/// face too, or a painted segment can overflow the wrap width and the break
/// points stop matching what the user sees.
#[gpui::test]
fn styled_markdown_keeps_the_wrap_accurate(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));

    let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);
    let mut note = String::new();
    for _ in 0..4 {
        note.push_str("# A heading with **bold** in it, long enough to wrap\n");
        note.push_str("> a quoted line **with bold** in it, long enough to wrap too\n");
        note.push_str("- an item with `inline code` and [[Another Note]] and **strong**\n");
    }
    editor
        .update(cx, |editor, window, cx| {
            editor.set_text(note, cx);
            let focus = editor.focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    let (ranges, width) = editor
        .read_with(cx, |editor, _cx| {
            (editor.display_ranges(), editor.wrap_width())
        })
        .unwrap();
    let painted = editor
        .read_with(cx, |editor, _cx| editor.painted_lines())
        .unwrap();
    assert!(
        ranges.len() >= 12,
        "the styled note should wrap into many display lines, got {}",
        ranges.len()
    );
    assert_eq!(ranges.len(), painted.len(), "model and paint agree");
    for (model, paint) in ranges.iter().zip(&painted) {
        assert_eq!(
            (model.0, model.1),
            (paint.0, paint.1),
            "the buffer's display lines must be exactly the painted ones"
        );
    }
    for (i, (_, _, w)) in painted.iter().enumerate() {
        assert!(
            *w <= width,
            "styled display line {i} is {w:?} wide, over the {width:?} wrap width"
        );
    }
}

/// Headings are painted at a larger size, and their rows have to be
/// *wrapped* at that size too, or a long heading's break points would not
/// match what is on screen.
#[gpui::test]
fn headings_are_painted_larger(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));

    let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);
    editor
        .update(cx, |editor, window, cx| {
            editor.set_text("# Heading text here\nBody text here xx\n".to_string(), cx);
            let focus = editor.focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    let painted = editor
        .read_with(cx, |editor, _cx| editor.painted_lines())
        .unwrap();
    let heights = editor
        .read_with(cx, |editor, _cx| editor.painted_heights())
        .unwrap();
    assert_eq!(
        painted.len(),
        3,
        "three display lines (the trailing newline leaves an empty row)"
    );
    assert!(
        painted[0].2 > painted[1].2,
        "the heading line ({:?}) should be painted wider than the body line ({:?})",
        painted[0].2,
        painted[1].2
    );
    assert!(
        heights[0] > heights[1],
        "the heading row ({:?}) should be taller than the body row ({:?})",
        heights[0],
        heights[1]
    );
}

/// Styling must not disturb editing: the same vim keys work on a styled
/// note, and the style cache has to follow every edit.
#[gpui::test]
fn editing_a_styled_note(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));

    let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);
    editor
        .update(cx, |editor, window, cx| {
            editor.set_text("## Title **bold**\n- [ ] task `code`\n".to_string(), cx);
            let focus = editor.focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    cx.simulate_keystrokes("j d d");
    let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
    assert_eq!(
        text, "## Title **bold**\n",
        "`dd` should delete the task line"
    );

    // `k` first: after deleting the last line the cursor sits on the empty
    // row the trailing newline leaves behind (vim steps up to the previous
    // line instead; noted as a cursor-model nit in PROJECT.md).
    cx.simulate_keystrokes("k 0 i x");
    let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
    assert_eq!(text, "x## Title **bold**\n", "`i` should still insert");

    // Painting the edited note must not panic even though the style cache
    // was rebuilt after the change.
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let painted = editor
        .read_with(cx, |editor, _cx| editor.painted_lines())
        .unwrap();
    assert_eq!(painted.len(), 2, "two display lines after the edit");
}

/// The note is not flush against the window's chrome, and a picker floats *over*
/// it: opening one used to shrink the text area, which re-wrapped and shifted
/// every line on screen.
#[gpui::test]
fn the_note_has_margins_and_the_picker_floats_over_it(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    cx.simulate_keystrokes("i");
    cx.simulate_input("hello");
    cx.simulate_keystrokes("escape");

    let origin = |window: &WindowHandle<Workspace>, cx: &mut VisualTestContext| {
        window
            .update(cx, |ws, _window, cx| ws.note_origin(cx))
            .unwrap()
    };
    let before = origin(&window, cx);
    let (top, left) = before.expect("the note is painted");
    // The sidebar is 260px wide, so anything at its edge would report 260; the
    // note's padding has to put the text clearly to the right of that.
    assert!(
        left > px(260.) + px(8.),
        "the text is inset from the sidebar, got {left:?}"
    );
    assert!(top > px(8.), "the text is inset from the top, got {top:?}");

    cx.simulate_keystrokes("ctrl-o");
    assert_eq!(
        origin(&window, cx),
        before,
        "an open picker must not move a single line of the note"
    );
}
