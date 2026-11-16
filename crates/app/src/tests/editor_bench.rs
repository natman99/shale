//! Timing the editor's frame and keystroke paths — the unbenched half of
//! typing latency. The core benches (`bench_paths.rs`) cover the whole-text
//! passes in isolation; this measures what a frame of the real view costs
//! and what one keystroke adds on top: buffer edit, restyle, re-layout,
//! scene build. The draw call's GPU half is not in here, same as the graph
//! paint bench.
//!
//! ```text
//! cargo test -p shale-app --release -- --ignored --nocapture editor_bench
//! ```
//!
//! **Release, always** — the graph paint bench's warning applies verbatim:
//! a debug build measures 5–10× the work and points at the wrong things.
//! Like that bench, the number printed is the **fastest round** (the
//! machine moves the same code by 20%), and the primitive count rides
//! along because it does not move with the machine.

use std::time::{Duration, Instant};

use gpui::{Focusable, TestAppContext, VisualTestContext, WindowHandle};

use crate::editor::MarkdownEditor;

/// Frames per measurement, rounds of which the fastest is printed, and
/// warmup frames — the same conventions as the graph paint bench.
const REPS: usize = 30;
const ROUNDS: usize = 3;
const WARMUP: usize = 5;

/// A deterministic markdown note with every construct the styler and the
/// layout walk: headings, emphasis, wikilinks, tags, highlights, code,
/// lists, and long paragraphs that force soft-wrap decisions.
fn synthetic_note(words: usize) -> String {
    let block = "## A section heading with some words\n\n\
                 The quick brown [[Some Note]] jumps over the #lazydog, and ==highlights== \
                 sit beside `inline code` and **bold words** in a paragraph that keeps going \
                 long enough to wrap at least once at a normal window width, which is the \
                 point of making it this long.\n\n\
                 - a list item with [[Another Note]] inside it\n\
                 - [ ] a task with *emphasis* and a link [to a page](elsewhere.md)\n\n\
                 ```rust\nlet x = 1 + 2;\nprintln!(\"{x}\");\n```\n\n\
                 A paragraph of ordinary prose to give the styler plain runs to walk \
                 through, with numbers 1234 and symbols &*() mixed in for good measure.\n\n";
    let mut out = String::new();
    while out.split_whitespace().count() < words {
        out.push_str(block);
    }
    out
}

/// The editor in a test window, holding `text`, with the keyboard on it.
fn editor_with(
    cx: &mut TestAppContext,
    text: &str,
) -> (WindowHandle<MarkdownEditor>, VisualTestContext) {
    let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
    let mut vcx = VisualTestContext::from_window(editor.into(), cx);
    editor
        .update(&mut vcx, |editor, window, cx| {
            editor.set_text(text.to_string(), cx);
            let focus = editor.focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();
    // The first frame has no measured width yet — the paint tests' rule.
    for _ in 0..2 {
        vcx.update(|window, cx| window.draw(cx).clear(cx));
    }
    (editor, vcx)
}

/// One forced frame: notify (dirty the view, so `draw` re-renders rather
/// than skipping a clean one) then draw. This is the full render path —
/// render, layout, paint, scene build — with the GPU half left out
/// because there is no GPU in a test window.
fn frame(editor: &WindowHandle<MarkdownEditor>, vcx: &mut VisualTestContext) {
    editor
        .update(vcx, |_editor, _window, cx| cx.notify())
        .expect("the bench window is open");
    vcx.update(|window, cx| window.draw(cx).clear(cx));
}

/// Time `body` over REPS frames, ROUNDS times, printing the fastest round.
fn timed(
    editor: &WindowHandle<MarkdownEditor>,
    vcx: &mut VisualTestContext,
    body: impl Fn(&WindowHandle<MarkdownEditor>, &mut VisualTestContext),
) -> Duration {
    for _ in 0..WARMUP {
        body(editor, vcx);
    }
    let mut best = Duration::MAX;
    for _ in 0..ROUNDS {
        let started = Instant::now();
        for _ in 0..REPS {
            body(editor, vcx);
        }
        best = best.min(started.elapsed() / REPS as u32);
    }
    best
}

/// The three numbers a typist pays: a forced frame of a clean buffer, a
/// keystroke (edit + the frame that shows it), and a scrolling motion.
#[gpui::test]
#[ignore]
fn editor_frame_and_keystroke_bench(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(crate::workspace::keybindings()));
    for words in [500usize, 2000, 8000] {
        let text = synthetic_note(words);
        let (_editor, mut vcx) = editor_with(cx, &text);

        let clean = timed(&_editor, &mut vcx, frame);
        // A character through the input path, plus the frame that shows it.
        let keystroke = timed(&_editor, &mut vcx, |editor, vcx| {
            vcx.simulate_input("x");
            frame(editor, vcx);
        });
        // `999j` in normal mode: the cursor crosses the buffer and the
        // view scrolls to follow it.
        let motion = timed(&_editor, &mut vcx, |editor, vcx| {
            vcx.simulate_keystrokes("escape 9 9 9 j");
            frame(editor, vcx);
        });
        let quads = vcx.update(|window, _cx| window.painted_quads().len());
        println!(
            "editor words={words:>5}  frame {clean:>8.2?}  keystroke {keystroke:>8.2?}  999j {motion:>8.2?}  quads={quads}"
        );
    }
}
