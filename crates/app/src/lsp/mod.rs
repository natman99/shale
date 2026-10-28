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

pub(crate) use mapping::{Diagnostic, Fix};

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
    /// Our requests that are still waiting for an answer, and what each answer is
    /// for. The id alone is not enough: the same response shape carries the
    /// handshake and the code actions, and telling them apart is what decides
    /// whether to start checking or to fill a popup.
    pending: Vec<(wire::Id, Pending)>,
    /// The document the server holds, and the version it holds it at.
    open: Option<(String, i64)>,
    /// The text we last sent. The ranges that come back are positions in it, so
    /// they can only be placed against it.
    sent_text: String,
    /// The current document's diagnostics, in buffer byte offsets.
    diagnostics: Vec<Diagnostic>,
    /// The fixes the server offered last, and the buffer range they were asked
    /// for. Empty until a `codeAction` answer arrives.
    fixes: Vec<Fix>,
    /// Bumped by every edit, so a debounce that has been overtaken can see it.
    revision: usize,
}

/// What one of our outstanding requests is waiting for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pending {
    /// The `initialize` answer, which is when the server starts checking.
    Initialize,
    /// A `textDocument/codeAction` answer: what can be done about a note.
    Fixes,
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
    /// The answer to a `codeAction` request: what can be done about a note.
    Fixes(Vec<Fix>),
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
            pending: Vec::new(),
            open: None,
            sent_text: String::new(),
            diagnostics: Vec::new(),
            fixes: Vec::new(),
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
        self.pending
            .push((wire::Id::Number(id), Pending::Initialize));
        self.send(&wire::request(
            id,
            "initialize",
            json!({
                "processId": std::process::id(),
                "clientInfo": { "name": "shale" },
                "rootUri": null,
                // `codeActionProvider` and `executeCommandProvider` are asked for
                // because the client can act on both: the popup's rows are the
                // actions, and applying one may run the server's own command.
                "capabilities": {
                    "workspace": { "configuration": true, "executeCommand": {} },
                    "textDocument": {
                        "publishDiagnostics": {},
                        "codeAction": {
                            "codeActionLiteralSupport": {
                                "codeActionKind": { "valueSet": ["quickfix"] }
                            }
                        }
                    }
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
        self.fixes.clear();
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
        // The old notes are about text that no longer exists, and so is any
        // `codeAction` answer still on its way: a popup filled from text the user
        // has since edited would offer fixes for words that are not there.
        self.diagnostics.clear();
        self.fixes.clear();
        self.pending.retain(|(_, kind)| *kind != Pending::Fixes);
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
        self.fixes.clear();
        self.pending.retain(|(_, kind)| *kind != Pending::Fixes);
        cx.notify();
    }

    /// The checker's notes on the open document, in buffer byte offsets.
    pub(crate) fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Ask the server what can be done about the buffer range `start..end` — the
    /// note the user is looking at, from the popup. The answer arrives as a
    /// notification on this entity, which is what fills the popup's rows.
    ///
    /// The context is sent empty on purpose. Harper re-lints the range itself and
    /// ignores what the client echoes; a server that keys its actions off the
    /// diagnostics we pass will need the payload kept, which is the follow-up
    /// noted in the plan.
    pub(crate) fn fixes_at(&mut self, start: usize, end: usize, cx: &mut Context<Self>) {
        let Some((uri, _)) = self.open.clone() else {
            return;
        };
        let id = self.next_id;
        self.next_id += 1;
        self.pending.push((wire::Id::Number(id), Pending::Fixes));
        self.fixes.clear();
        self.send(&wire::request(
            id,
            "textDocument/codeAction",
            json!({
                "textDocument": { "uri": uri },
                "range": mapping::lsp_range(&self.sent_text, start, end),
                "context": { "diagnostics": [] }
            }),
        ));
        cx.notify();
    }

    /// The fixes the server offered for the range we last asked about.
    pub(crate) fn fixes(&self) -> &[Fix] {
        &self.fixes
    }

    /// Run one of the server's own commands — adding a word to a dictionary,
    /// ignoring a lint — with the arguments the server handed us. Harper answers a
    /// `HarperAddTo*` or `HarperIgnoreLint` by re-publishing the diagnostics for
    /// the document, so the underline goes away on its own.
    pub(crate) fn run(&mut self, command: &str, args: &[Value], cx: &mut Context<Self>) {
        let id = self.next_id;
        self.next_id += 1;
        // Not tracked in `pending`: the answer is a value we have no use for, and
        // an unexpected one is simply ignored by `decide`.
        self.send(&wire::request(
            id,
            "workspace/executeCommand",
            json!({ "command": command, "arguments": args }),
        ));
        cx.notify();
    }

    fn send(&mut self, bytes: &[u8]) {
        if let Err(err) = self.stdin.write_all(bytes) {
            tracing::warn!("the checker's stdin closed: {err}");
        }
    }

    fn receive(&mut self, message: wire::Message, cx: &mut Context<Self>) {
        let session = Session {
            sent_text: &self.sent_text,
            uri: self.open.as_ref().map(|(uri, _)| uri.as_str()),
            pending: &self.pending,
        };
        // Read the answer *before* settling it: `decide` works out what a response
        // is for by looking the id up in `pending`, so dropping the id first would
        // make every answer look like a request we never sent.
        let action = decide(&message, &session);
        // An answer settles the request it answers, whatever we make of it: an id
        // left in `pending` would misattribute a later answer.
        if let wire::Message::Response { id, .. } = &message {
            self.pending.retain(|(pending, _)| pending != id);
        }
        match action {
            Action::Answer(bytes) => self.send(&bytes),
            Action::Diagnostics(found) => {
                tracing::debug!(count = found.len(), "spellcheck diagnostics");
                self.diagnostics = found;
                cx.notify();
            }
            Action::Fixes(found) => {
                tracing::debug!(count = found.len(), "spellcheck fixes");
                self.fixes = found;
                cx.notify();
            }
            Action::Ready => {
                tracing::debug!("spellcheck server ready");
            }
            Action::Ignore => {}
        }
    }
}

/// What an incoming answer has to be read against: the document the server is
/// looking at, and the requests of ours it might be answering.
struct Session<'a> {
    /// The text the server holds, which is what every range it sends back is in.
    sent_text: &'a str,
    /// The URI of that document, which is what a fix's edits are keyed by.
    uri: Option<&'a str>,
    /// Our outstanding requests, oldest first.
    pending: &'a [(wire::Id, Pending)],
}

/// What to do with one message. Every request from the server has to be answered,
/// even with `null`, or it never gets around to checking anything.
fn decide(message: &wire::Message, session: &Session) -> Action {
    match message {
        wire::Message::Request { id, method, params } => {
            let result = match method.as_str() {
                // One answer per item asked for. An empty object means "the
                // defaults"; Harper wants an object here rather than `null`, and
                // says so on stderr if it does not get one.
                "workspace/configuration" => {
                    let items = params
                        .get("items")
                        .and_then(Value::as_array)
                        .map(Vec::len)
                        .unwrap_or(1);
                    Value::Array(vec![json!({}); items.max(1)])
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
                Action::Diagnostics(mapping::diagnostics(params, session.sent_text))
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
        wire::Message::Response { id, error, result } => {
            let ours = session
                .pending
                .iter()
                .find(|(want, _)| want == id)
                .map(|(_, kind)| *kind);
            match ours {
                Some(Pending::Initialize) => {
                    if error.is_some() {
                        tracing::warn!("the checker refused to initialize");
                    }
                    Action::Ready
                }
                Some(Pending::Fixes) => {
                    if let Some(err) = error {
                        // A server with no code actions is normal, not an error
                        // worth shouting about: it just means the popup has rows
                        // of nothing to offer.
                        tracing::debug!(error = %err, "the checker offered no fixes");
                        return Action::Ignore;
                    }
                    let response = json!({ "result": result.clone().unwrap_or(Value::Null) });
                    let uri = session.uri.unwrap_or_default();
                    Action::Fixes(mapping::fixes(&response, session.sent_text, uri))
                }
                // An answer to something we no longer care about (a command whose
                // result we have no use for, or one that arrived after an edit).
                None => Action::Ignore,
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
        let Action::Answer(bytes) = decide(&message, &session("", None, &[])) else {
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
        let Action::Diagnostics(found) = decide(&message, &session("thier spelling", None, &[]))
        else {
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
        let pending = [(wire::Id::Number(1), Pending::Initialize)];
        assert_eq!(decide(&ours, &session("", None, &pending)), Action::Ready);
        // An answer to something we never asked (or no longer care about) is not
        // the handshake.
        let theirs = wire::Message::Response {
            id: wire::Id::Number(9),
            result: Some(Value::Null),
            error: None,
        };
        assert_eq!(
            decide(&theirs, &session("", None, &pending)),
            Action::Ignore
        );
    }

    /// The `codeAction` answer is the other kind of response, and the only thing
    /// that tells the two apart is which request the id belongs to.
    #[test]
    fn a_code_action_answer_becomes_the_offered_fixes() {
        let message = wire::Message::Response {
            id: wire::Id::Number(4),
            result: Some(json!([{
                "title": "Replace with: “their”",
                "edit": { "changes": { "file:///a.md": [{
                    "range": { "start": { "line": 0, "character": 0 },
                               "end": { "line": 0, "character": 5 } },
                    "newText": "their" }] } }
            }])),
            error: None,
        };
        let pending = [(wire::Id::Number(4), Pending::Fixes)];
        let Action::Fixes(found) = decide(
            &message,
            &session("thier spelling", Some("file:///a.md"), &pending),
        ) else {
            panic!("expected fixes");
        };
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].edits, vec![(0, 5, "their".to_string())]);
    }

    /// The order `receive` works in, which is easy to get backwards: an answer is
    /// read against the request it settles, and only then is that request
    /// forgotten. Read the other way round, every answer looks like a request we
    /// never sent — and a popup that quietly loses its rows leaves nothing in the
    /// logs to explain it.
    #[test]
    fn an_answer_is_read_before_its_request_is_settled() {
        let answer = wire::Message::Response {
            id: wire::Id::Number(4),
            result: Some(json!([])),
            error: None,
        };
        let pending = [(wire::Id::Number(4), Pending::Fixes)];
        assert!(
            matches!(
                decide(
                    &answer,
                    &session("thier spelling", Some("file:///a.md"), &pending)
                ),
                Action::Fixes(_)
            ),
            "while the request is outstanding, its answer is fixes"
        );
        assert_eq!(
            decide(
                &answer,
                &session("thier spelling", Some("file:///a.md"), &[])
            ),
            Action::Ignore,
            "once it is settled, the same answer is noise"
        );
    }

    /// A server that has no code actions answers with an error, which is a normal
    /// answer to a question, not a fault: the popup simply has no rows.
    #[test]
    fn a_server_without_code_actions_is_not_an_error() {
        let message = wire::Message::Response {
            id: wire::Id::Number(4),
            result: None,
            error: Some(json!({ "code": -32601, "message": "Method not found" })),
        };
        let pending = [(wire::Id::Number(4), Pending::Fixes)];
        assert_eq!(
            decide(&message, &session("", Some("file:///a.md"), &pending)),
            Action::Ignore
        );
    }

    /// A session for `decide`, with the two things an answer is read against.
    fn session<'a>(
        text: &'a str,
        uri: Option<&'a str>,
        pending: &'a [(wire::Id, Pending)],
    ) -> Session<'a> {
        Session {
            sent_text: text,
            uri,
            pending,
        }
    }
}
