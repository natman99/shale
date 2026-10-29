//! The document half of a checker session: what the server is meant to be
//! holding, and the frames that keep it there.
//!
//! Kept apart from the server process so the **order** can be tested without one,
//! which is the part that is easy to get wrong. A `didOpen` the server reads
//! before it has answered `initialize` is dropped, and it is then left holding no
//! document at all: every later `didChange` is answered with notes about nothing,
//! which is the `count=0` in the log and the note that stays ununderlined until it
//! is closed and opened again. So the open is *staged* until the handshake is
//! answered, and the newest text rides along with it.

use serde_json::json;

use super::{wire, LANGUAGE};

/// The document the checker is looking at, as far as we know.
pub(crate) struct Document {
    /// The URI the server holds — or will hold once it is ready — and the version
    /// it holds it at.
    open: Option<(String, i64)>,
    /// The `initialize` answer has arrived and `initialized` has been sent, so the
    /// server will take documents.
    ready: bool,
    /// A `didOpen` the server could not have taken yet: the document it is about,
    /// and the newest text it should hold.
    staged_open: Option<(String, String)>,
    /// The text we last sent. The ranges that come back are positions in it, so
    /// they can only be placed against it.
    sent_text: String,
}

impl Document {
    pub(crate) fn new() -> Self {
        Self {
            open: None,
            ready: false,
            staged_open: None,
            sent_text: String::new(),
        }
    }

    /// The text the server holds, which is what every range it sends back is a
    /// position in.
    pub(crate) fn text(&self) -> &str {
        &self.sent_text
    }

    /// The URI of that document, which is what a fix's edits are keyed by.
    pub(crate) fn uri(&self) -> Option<&str> {
        self.open.as_ref().map(|(uri, _)| uri.as_str())
    }

    /// Whether the server has finished its handshake and will take documents, and
    /// therefore whether a request about one is worth writing.
    pub(crate) fn is_ready(&self) -> bool {
        self.ready
    }

    /// A note was opened: the `didOpen` for it, or `None` while the server is
    /// still starting, in which case the text is staged for the one that will go
    /// out when it finishes.
    pub(crate) fn opened(&mut self, uri: &str, text: &str) -> Option<Vec<u8>> {
        self.open = Some((uri.to_string(), 1));
        self.sent_text = text.to_string();
        if self.ready {
            Some(did_open(uri, 1, text))
        } else {
            self.staged_open = Some((uri.to_string(), text.to_string()));
            None
        }
    }

    /// The open note's text changed. Whole-document sync: the server advertised
    /// `change = 1`, so the new text goes in full rather than as a patch.
    ///
    /// `None` when there is nothing to write — either no note is open, or the
    /// server has never seen this one, in which case a change to a document it
    /// does not hold means nothing to it and the text goes into the staged
    /// `didOpen` that will carry it instead.
    pub(crate) fn changed(&mut self, text: &str) -> Option<Vec<u8>> {
        let (uri, version) = self.open.clone()?;
        let version = version + 1;
        self.open = Some((uri.clone(), version));
        self.sent_text = text.to_string();
        if self.ready {
            Some(wire::notification(
                "textDocument/didChange",
                json!({
                    "textDocument": { "uri": uri, "version": version },
                    "contentChanges": [{ "text": text }]
                }),
            ))
        } else {
            self.staged_open = Some((uri, text.to_string()));
            None
        }
    }

    /// The note was closed or deleted. `None` when the server was never told about
    /// it, in which case the staged `didOpen` is simply forgotten: a `didClose`
    /// for a document the server has never seen is a story it would have to make
    /// up an answer to.
    pub(crate) fn closed(&mut self) -> Option<Vec<u8>> {
        let (uri, _version) = self.open.take()?;
        self.sent_text.clear();
        if self.staged_open.take().is_some() {
            None
        } else {
            Some(wire::notification(
                "textDocument/didClose",
                json!({ "textDocument": { "uri": uri } }),
            ))
        }
    }

    /// The handshake was answered. Everything the server could not be told before:
    /// `initialized` — the protocol's other half, and what harper-ls does its
    /// follow-up questions on — then the note that has been on screen the whole
    /// time, at the version it has reached by now.
    pub(crate) fn handshake(&mut self) -> Vec<Vec<u8>> {
        self.ready = true;
        let mut frames = vec![wire::notification("initialized", json!({}))];
        if let Some((uri, text)) = self.staged_open.take() {
            let version = self.open.as_ref().map(|(_, version)| *version).unwrap_or(1);
            frames.push(did_open(&uri, version, &text));
        }
        frames
    }
}

/// `textDocument/didOpen` for a whole document.
fn did_open(uri: &str, version: i64, text: &str) -> Vec<u8> {
    wire::notification(
        "textDocument/didOpen",
        json!({
            "textDocument": { "uri": uri, "languageId": LANGUAGE, "version": version, "text": text }
        }),
    )
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;

    /// The methods of the frames a step wrote, in the order they went out.
    fn methods(frames: &[Vec<u8>]) -> Vec<String> {
        decode(frames)
            .into_iter()
            .map(|message| match message {
                wire::Message::Notification { method, .. } => method,
                other => format!("{other:?}"),
            })
            .collect()
    }

    /// The params of the one `didOpen` among these frames.
    fn opened_params(frames: &[Vec<u8>]) -> Value {
        for message in decode(frames) {
            if let wire::Message::Notification { method, params } = message {
                if method == "textDocument/didOpen" {
                    return params;
                }
            }
        }
        panic!("no `didOpen` among {} frames", frames.len());
    }

    /// Frames are written one at a time in production; a test looks at a batch, so
    /// they are concatenated and read back as the stream they become.
    fn decode(frames: &[Vec<u8>]) -> Vec<wire::Message> {
        let bytes: Vec<u8> = frames.iter().flatten().copied().collect();
        let mut framer = wire::Framer::new(std::io::BufReader::new(&bytes[..]));
        let mut out = Vec::new();
        while let Some(message) = framer.next_message() {
            out.push(message);
        }
        out
    }

    /// The bug this ordering fixes. `:harper on` starts a server and hands it the
    /// note on screen in the same breath, and a `didOpen` the server reads while
    /// it is still initializing is thrown away: it is left holding no document, so
    /// every change after it is answered with notes about nothing — `count=0` in
    /// the log forever, and no underline until the note is reopened.
    #[test]
    fn a_note_opened_before_the_handshake_waits_for_it() {
        let mut doc = Document::new();
        assert!(
            doc.opened("file:///a.md", "thier spelling").is_none(),
            "the note waits for the handshake instead of being thrown away"
        );
        // Typed into while the server was still starting.
        assert!(doc.changed("thier spelling is wrogn").is_none());

        let frames = doc.handshake();
        assert_eq!(
            methods(&frames),
            ["initialized", "textDocument/didOpen"],
            "one `didOpen`, once the server can take it"
        );
        let opened = opened_params(&frames);
        assert_eq!(
            opened["textDocument"]["text"], "thier spelling is wrogn",
            "it carries the newest text, not the text open when the server started"
        );
        assert_eq!(
            opened["textDocument"]["version"], 2,
            "and the version the note has reached by now"
        );

        // Past the handshake, the ordinary path is the ordinary path.
        let changed = doc
            .changed("thier spelling is wrong")
            .expect("a change goes out at once");
        assert_eq!(methods(&[changed]), ["textDocument/didChange"]);
    }

    /// The other order, which is what a normal start-up does: the handshake is
    /// answered first, so the note goes out as soon as it is opened.
    #[test]
    fn a_note_opened_after_the_handshake_goes_at_once() {
        let mut doc = Document::new();
        assert_eq!(methods(&doc.handshake()), ["initialized"]);
        let opened = doc
            .opened("file:///a.md", "thier")
            .expect("the server can take it now");
        assert_eq!(methods(&[opened]), ["textDocument/didOpen"]);
    }

    /// A note the server was never told about is not closed, it is forgotten — and
    /// the staged open goes with it, so the server is not handed a document the
    /// user has already moved away from.
    #[test]
    fn a_note_the_server_never_saw_is_not_closed() {
        let mut doc = Document::new();
        doc.opened("file:///a.md", "thier");
        assert!(
            doc.closed().is_none(),
            "no `didClose` for a document it never held"
        );
        assert_eq!(methods(&doc.handshake()), ["initialized"]);
    }

    /// Closing a note the server *has* seen is the ordinary case, and it is the
    /// only way the server stops checking a note that is no longer on screen.
    #[test]
    fn a_note_the_server_holds_is_closed() {
        let mut doc = Document::new();
        doc.handshake();
        doc.opened("file:///a.md", "thier");
        let closed = doc.closed().expect("the server holds it");
        assert_eq!(methods(&[closed]), ["textDocument/didClose"]);
        assert!(doc.text().is_empty(), "and we hold nothing either");
    }
}
