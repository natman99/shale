//! The headless UI tests: a real `MarkdownEditor` in a gpui test window, driven
//! through the same keymap and input paths the live app uses.
//! `simulate_keystrokes` exercises the keys we bind to actions and
//! `simulate_input` the characters that reach the buffer through the input
//! handler, which is how an input-path regression gets caught without a live
//! window.

use super::*;
use gpui::{TestAppContext, VisualTestContext};

mod keys;
mod paint;
