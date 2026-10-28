//! Spelling and grammar, from a language server.
//!
//! Shale is the LSP **client**: it starts the server (`harper-ls --stdio` by
//! default), speaks JSON-RPC over its standard input and output, and draws what
//! comes back as wavy underlines. The arrangement is deliberately modest — one
//! server per window, one document open at a time, whole-document sync — because
//! that is all a single-note editor needs, and `harper-ls` advertises exactly
//! that (`textDocumentSync.change = 1`, full).
//!
//! Two things about this server are load-bearing and easy to get wrong:
//!
//! - It **asks the client questions** (`workspace/configuration`,
//!   `client/registerCapability`) and does not process `didOpen` until they are
//!   answered. A client that ignores them sees no diagnostics, ever.
//! - Grammar rules are off unless `initialize` carries
//!   `initializationOptions.grammarOpacity = "all"`. Spell checking alone is what
//!   the defaults give, which is half of what the user asked for.

mod mapping;
mod wire;

use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::Duration;

use anyhow::{Context as _, Result};
use futures::StreamExt;
use gpui::{App, AppContext as _, Context, Entity};
use serde_json::{json, Value};
use shale_core::config::SpellcheckConfig;

pub(crate) use mapping::Diagnostic;

/// How long to wait after the last keystroke before asking again. Long enough
/// that typing a word is one request rather than one per character, short enough
/// that the underline appears before the user looks away.
const DEBOUNCE: Duration = Duration::from_millis(400);

/// The document kinds the checker is asked about. Markdown is what a vault holds;
/// the server strips the markup itself.
const LANGUAGE: &str = "markdown";

/// A running checker, and what it currently has to say about the open note.
pub(crate) struct Spell {
    /// The server process. Held so that dropping the editor can stop it: a
    /// `Child` that is simply dropped keeps running.
    server: Child,
    /// Where requests go. Written from the main thread; the only large writes are
    /// whole notes, which the server drains as it checks them.
    stdin: ChildStdin,
    /// The id the next request will use.
    next_id: i64,
    /// The id of our `initialize` request, whose answer means the server is up.
    init_id: Option<i64>,
    /// The document the server holds, and the version it holds it at.
    open: Option<(String, i64)>,
    /// The text we last sent. The ranges that come back are positions in it, so
    /// they can only be placed against it.
    sent_text: String,
    /// The current document's diagnostics, in buffer byte offsets.
    diagnostics: Vec<Diagnostic>,
    /// Bumped by every edit, so a debounce that has been overtaken can see it.
    revision: usize,
}

/// What an incoming message means for us. Kept apart from applying it so the
/// decision can be tested without a server process.
#[derive(Debug, PartialEq)]
enum Action {
    /// The server is waiting on this; write these bytes and it continues.
    Answer(Vec<u8>),
    /// A fresh set of notes about the document we sent.
    Diagnostics(Vec<Diagnostic>),
    /// The `initialize` answer: the server is ready for documents.
    Ready,
    /// Something we do not act on (log messages, progress, our own answers).
    Ignore,
}

impl Spell {
    /// Start the server and shake hands with it. An error here means the command
    /// is not installed or would not start; the caller turns that into "no
    /// spellcheck", which is the right answer rather than a broken editor.
    pub(crate) fn start(config: &SpellcheckConfig, cx: &mut App) -> Result<Entity<Spell>> {
        let mut server = Command::new(&config.command)
            .args(&config.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // Harper's own chatter goes nowhere: it is `window/logMessage` over
            // the protocol when it matters, and this pipe when it does not.
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("starting the checker `{}`", config.command))?;
        let stdin = server.stdin.take().context("the checker has no stdin")?;
        let stdout = server.stdout.take().context("the checker has no stdout")?;

        let spell = cx.new(|_cx| Spell {
            server,
            stdin,
            next_id: 1,
            init_id: None,
            open: None,
            sent_text: String::new(),
            diagnostics: Vec::new(),
            revision: 0,
        });

        // One thread reads frames; the messages arrive on the main thread through
        // a stream, so nothing in the editor has to think about threads.
        let (sender, receiver) = futures::channel::mpsc::unbounded::<wire::Message>();
        std::thread::spawn(move || {
            let mut framer = wire::Framer::new(BufReader::new(stdout));
            while let Some(message) = framer.next_message() {
                if sender.unbounded_send(message).is_err() {
                    break;
                }
            }
        });

        let watched = spell.downgrade();
        let mut receiver = receiver;
        cx.spawn(async move |cx| {
            while let Some(message) = receiver.next().await {
                if watched
                    .update(cx, |spell, cx| spell.receive(message, cx))
                    .is_err()
                {
                    // The editor is gone, so the checker has nobody to answer.
                    break;
                }
            }
        })
        .detach();

        spell.update(cx, |spell, _cx| spell.initialize());
        Ok(spell)
    }

    /// The handshake. `initializationOptions` is where grammar is switched on.
    fn initialize(&mut self) {
        let id = self.next_id;
        self.next_id += 1;
        self.init_id = Some(id);
        self.send(&wire::request(
            id,
            "initialize",
            json!({
                "processId": std::process::id(),
                "clientInfo": { "name": "shale" },
                "rootUri": null,
                "capabilities": {
                    "workspace": { "configuration": true },
                    "textDocument": { "publishDiagnostics": {} }
                },
                "initializationOptions": {
                    "dialect": "American",
                    "grammarOpacity": "all",
                    "isolateEnglish": false
                }
            }),
        ));
    }

    /// Tell the server about a document it has not seen.
    pub(crate) fn open(&mut self, path: &Path, text: &str, cx: &mut Context<Self>) {
        let uri = file_uri(path);
        self.send(&wire::notification(
            "textDocument/didOpen",
            json!({
                "textDocument": { "uri": uri, "languageId": LANGUAGE, "version": 1, "text": text }
            }),
        ));
        self.open = Some((uri, 1));
        self.sent_text = text.to_string();
        self.diagnostics.clear();
        cx.notify();
    }

    /// The open document's text changed. Debounced, so a typed word is one
    /// request rather than one per character.
    pub(crate) fn changed(&mut self, text: &str, cx: &mut Context<Self>) {
        self.revision += 1;
        let revision = self.revision;
        let text = text.to_string();
        cx.spawn(async move |me, cx| {
            cx.background_executor().timer(DEBOUNCE).await;
            _ = me.update(cx, |spell, cx| {
                // Overtaken by a later edit, so this request is already stale.
                if spell.revision == revision {
                    spell.send_change(&text, cx);
                }
            });
        })
        .detach();
    }

    /// Whole-document sync: the server advertised `change = 1`, so the new text
    /// is sent in full rather than as a patch.
    fn send_change(&mut self, text: &str, cx: &mut Context<Self>) {
        let Some((uri, version)) = self.open.clone() else {
            return;
        };
        let version = version + 1;
        self.open = Some((uri.clone(), version));
        self.send(&wire::notification(
            "textDocument/didChange",
            json!({
                "textDocument": { "uri": uri, "version": version },
                "contentChanges": [{ "text": text }]
            }),
        ));
        self.sent_text = text.to_string();
        // The old notes are about text that no longer exists.
        self.diagnostics.clear();
        cx.notify();
    }

    /// Tell the server the document it holds is gone: the note was closed or
    /// deleted, so there is nothing left to check.
    pub(crate) fn close(&mut self, cx: &mut Context<Self>) {
        let Some((uri, _version)) = self.open.take() else {
            return;
        };
        self.send(&wire::notification(
            "textDocument/didClose",
            json!({ "textDocument": { "uri": uri } }),
        ));
        self.sent_text.clear();
        self.diagnostics.clear();
        cx.notify();
    }

    /// The checker's notes on the open document, in buffer byte offsets.
    pub(crate) fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    fn send(&mut self, bytes: &[u8]) {
        if let Err(err) = self.stdin.write_all(bytes) {
            tracing::warn!("the checker's stdin closed: {err}");
        }
    }

    fn receive(&mut self, message: wire::Message, cx: &mut Context<Self>) {
        match decide(&message, self.init_id, &self.sent_text) {
            Action::Answer(bytes) => self.send(&bytes),
            Action::Diagnostics(found) => {
                tracing::debug!(count = found.len(), "spellcheck diagnostics");
                self.diagnostics = found;
                cx.notify();
            }
            Action::Ready => {
                tracing::debug!("spellcheck server ready");
                self.init_id = None;
            }
            Action::Ignore => {}
        }
    }
}

/// What to do with one message. Every request from the server has to be answered,
/// even with `null`, or it never gets around to checking anything.
fn decide(message: &wire::Message, init_id: Option<i64>, sent_text: &str) -> Action {
    match message {
        wire::Message::Request { id, method, params } => {
            let result = match method.as_str() {
                // One answer per item asked for; `null` means "the defaults", and
                // the interesting defaults were already sent in initialize.
                "workspace/configuration" => {
                    let items = params
                        .get("items")
                        .and_then(Value::as_array)
                        .map(Vec::len)
                        .unwrap_or(1);
                    Value::Array(vec![Value::Null; items.max(1)])
                }
                "workspace/workspaceFolders"
                | "client/registerCapability"
                | "window/workDoneProgress/create" => Value::Null,
                other => {
                    tracing::debug!(method = other, "answering an unfamiliar server request");
                    Value::Null
                }
            };
            Action::Answer(wire::reply(id, result))
        }
        wire::Message::Notification { method, params } => match method.as_str() {
            "textDocument/publishDiagnostics" => {
                Action::Diagnostics(mapping::diagnostics(params, sent_text))
            }
            "window/logMessage" | "window/showMessage" => {
                tracing::debug!(
                    message = params
                        .get("message")
                        .and_then(|message| message.as_str())
                        .unwrap_or_default(),
                    "checker"
                );
                Action::Ignore
            }
            _ => Action::Ignore,
        },
        wire::Message::Response { id, error, .. } => {
            let ours = match (id, init_id) {
                (wire::Id::Number(id), Some(want)) => *id == want,
                _ => false,
            };
            if ours {
                if error.is_some() {
                    tracing::warn!("the checker refused to initialize");
                }
                Action::Ready
            } else {
                Action::Ignore
            }
        }
    }
}

impl Drop for Spell {
    fn drop(&mut self) {
        // A `Child` that is only dropped keeps running, which would leave a
        // checker per window after the window is gone.
        self.send(&wire::notification("exit", Value::Null));
        _ = self.stdin.flush();
        _ = self.server.kill();
        _ = self.server.wait();
    }
}

/// The `file:` URI for a vault note. Spaces and other punctuation have to be
/// escaped — a vault with `Another Note.md` in it is the normal case, and an
/// unescaped URI is one the server will not match its answers back to.
pub(crate) fn file_uri(path: &Path) -> String {
    let text = path.to_string_lossy();
    let mut out = String::with_capacity(text.len() + 8);
    out.push_str("file://");
    for ch in text.chars() {
        match ch {
            '/' | 'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '.' | '_' | '~' => out.push(ch),
            other => {
                for byte in other.encode_utf8(&mut [0u8; 4]).as_bytes() {
                    out.push_str(&format!("%{byte:02X}"));
                }
            }
        }
    }
    out
}

/// The absolute path of a vault-relative note, which is what a URI needs.
pub(crate) fn absolute(root: &Path, rel: &Path) -> PathBuf {
    if rel.is_absolute() {
        rel.to_path_buf()
    } else {
        root.join(rel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_uri_escapes_what_a_uri_cannot_carry() {
        assert_eq!(
            file_uri(Path::new("/vault/notes/Another Note.md")),
            "file:///vault/notes/Another%20Note.md"
        );
        assert_eq!(
            file_uri(Path::new("/vault/café.md")),
            "file:///vault/caf%C3%A9.md",
            "the bytes of a non-ASCII name, percent-escaped"
        );
    }

    #[test]
    fn a_relative_note_is_made_absolute_for_the_uri() {
        assert_eq!(
            absolute(Path::new("/vault"), Path::new("a/b.md")),
            PathBuf::from("/vault/a/b.md")
        );
    }

    #[test]
    fn a_server_request_is_answered_rather_than_ignored() {
        let message = wire::Message::Request {
            id: wire::Id::Number(0),
            method: "workspace/configuration".into(),
            params: json!({ "items": [{}, {}] }),
        };
        let Action::Answer(bytes) = decide(&message, Some(1), "") else {
            panic!("a server request has to be answered");
        };
        let decoded = wire::Framer::new(std::io::BufReader::new(&bytes[..]))
            .next_message()
            .unwrap();
        match decoded {
            wire::Message::Response { id, result, .. } => {
                assert_eq!(id, wire::Id::Number(0));
                assert_eq!(result.unwrap().as_array().unwrap().len(), 2);
            }
            other => panic!("expected a response, got {other:?}"),
        }
    }

    #[test]
    fn diagnostics_are_placed_against_the_text_we_sent() {
        let message = wire::Message::Notification {
            method: "textDocument/publishDiagnostics".into(),
            params: json!({
                "uri": "file:///a.md",
                "diagnostics": [{
                    "message": "Did you mean to spell `thier` this way?",
                    "code": "SpellCheck",
                    "range": { "start": { "line": 0, "character": 0 },
                               "end": { "line": 0, "character": 5 } }
                }]
            }),
        };
        let Action::Diagnostics(found) = decide(&message, Some(1), "thier spelling") else {
            panic!("expected diagnostics");
        };
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].code, "SpellCheck");
    }

    #[test]
    fn the_initialize_answer_is_what_turns_the_checker_on() {
        let ours = wire::Message::Response {
            id: wire::Id::Number(1),
            result: Some(json!({ "capabilities": {} })),
            error: None,
        };
        assert_eq!(decide(&ours, Some(1), ""), Action::Ready);
        // Some other answer (a code action, a shutdown) is not the handshake.
        let theirs = wire::Message::Response {
            id: wire::Id::Number(9),
            result: Some(Value::Null),
            error: None,
        };
        assert_eq!(decide(&theirs, Some(1), ""), Action::Ignore);
    }
}
