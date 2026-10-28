//! The Language Server Protocol's wire format: JSON objects behind a
//! `Content-Length` header, one per message, over a pipe.
//!
//! The framing is trivial and the mistakes it hides are not: `Content-Length`
//! counts **bytes**, so a client that counts characters desynchronises the stream
//! the first time a note contains anything non-ASCII — which, in a markdown vault,
//! is not an edge case.

use std::io::BufRead;

use serde_json::Value;

/// A request id. The LSP allows either; `harper-ls` uses integers, but a string
/// id is not a reason to lose track of a message.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Id {
    /// The id as a number, which is what servers send us.
    Number(i64),
    /// The id as a string, kept verbatim so the answer can repeat it.
    Text(String),
}

impl Id {
    /// The JSON value this id answers with.
    fn value(&self) -> Value {
        match self {
            Id::Number(n) => Value::from(*n),
            Id::Text(s) => Value::from(s.clone()),
        }
    }
}

/// One message off the wire, in whichever shape it arrived. Telling these apart
/// matters: a **request from the server** has to be answered or the server sits
/// there waiting, which is exactly how `harper-ls` behaves if the client ignores
/// its `workspace/configuration` question.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Message {
    /// Something happened; nothing to answer.
    Notification { method: String, params: Value },
    /// The server wants something back before it continues.
    Request {
        id: Id,
        method: String,
        params: Value,
    },
    /// The answer to a request we sent.
    Response {
        id: Id,
        result: Option<Value>,
        error: Option<Value>,
    },
}

impl Message {
    /// The body of a message, classified. A body that is not one of the three
    /// shapes is dropped rather than ending the session.
    fn classify(body: Value) -> Option<Message> {
        if let Some(method) = body.get("method").and_then(Value::as_str) {
            let params = body.get("params").cloned().unwrap_or(Value::Null);
            let method = method.to_string();
            return match body.get("id").and_then(classify_id) {
                Some(id) => Some(Message::Request { id, method, params }),
                None => Some(Message::Notification { method, params }),
            };
        }
        let id = body.get("id").and_then(classify_id)?;
        Some(Message::Response {
            id,
            result: body.get("result").cloned(),
            error: body.get("error").cloned(),
        })
    }
}

/// The id field, if there is one that means anything.
fn classify_id(value: &Value) -> Option<Id> {
    match value {
        Value::Number(n) => n.as_i64().map(Id::Number),
        Value::String(s) => Some(Id::Text(s.clone())),
        _ => None,
    }
}

/// The bytes of one message: header, blank line, body.
pub(crate) fn encode(body: &Value) -> Vec<u8> {
    let payload = serde_json::to_vec(body).unwrap_or_else(|_| b"{}".to_vec());
    let mut out = Vec::with_capacity(payload.len() + 32);
    out.extend_from_slice(format!("Content-Length: {}\r\n\r\n", payload.len()).as_bytes());
    out.extend_from_slice(&payload);
    out
}

/// A request we are sending, with the id the answer will come back with.
pub(crate) fn request(id: i64, method: &str, params: Value) -> Vec<u8> {
    encode(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    }))
}

/// A notification we are sending: something happened, no answer expected.
pub(crate) fn notification(method: &str, params: Value) -> Vec<u8> {
    encode(&serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
    }))
}

/// Answering one of the server's requests. Every request left unanswered is a
/// server that never gets to the next message.
pub(crate) fn reply(id: &Id, result: Value) -> Vec<u8> {
    encode(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": id.value(),
        "result": result,
    }))
}

/// Turns a byte stream back into messages.
pub(crate) struct Framer<R: BufRead> {
    source: R,
}

impl<R: BufRead> Framer<R> {
    pub(crate) fn new(source: R) -> Self {
        Self { source }
    }

    /// The next message, or `None` once the pipe is closed. A frame that cannot
    /// be understood is skipped: one malformed message is not a reason to stop
    /// checking the user's notes.
    pub(crate) fn next_message(&mut self) -> Option<Message> {
        loop {
            let body = self.next_body()?;
            if let Some(message) = Message::classify(body) {
                return Some(message);
            }
        }
    }

    /// One framed body, headers read and discarded.
    fn next_body(&mut self) -> Option<Value> {
        let mut length = None;
        loop {
            let mut line = Vec::new();
            if self.source.read_until(b'\n', &mut line).ok()? == 0 {
                return None;
            }
            let line = String::from_utf8_lossy(&line)
                .trim_end_matches(['\r', '\n'])
                .to_string();
            if line.is_empty() {
                break;
            }
            if let Some((name, value)) = line.split_once(':') {
                if name.trim().eq_ignore_ascii_case("content-length") {
                    length = value.trim().parse::<usize>().ok();
                }
            }
        }
        let mut payload = vec![0u8; length?];
        self.source.read_exact(&mut payload).ok()?;
        serde_json::from_slice(&payload).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, BufReader, Read};

    /// A reader that hands over one byte at a time, so a frame that arrives in
    /// pieces is exercised rather than assumed.
    struct Dribble<'a>(&'a [u8]);

    impl Read for Dribble<'_> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.0.is_empty() || buf.is_empty() {
                return Ok(0);
            }
            buf[0] = self.0[0];
            self.0 = &self.0[1..];
            Ok(1)
        }
    }

    fn decode(bytes: &[u8]) -> Vec<Message> {
        let mut framer = Framer::new(BufReader::new(Dribble(bytes)));
        let mut out = Vec::new();
        while let Some(message) = framer.next_message() {
            out.push(message);
        }
        out
    }

    #[test]
    fn a_frame_round_trips() {
        let bytes = notification(
            "textDocument/didOpen",
            serde_json::json!({ "textDocument": { "uri": "file:///a.md" } }),
        );
        let messages = decode(&bytes);
        assert_eq!(messages.len(), 1);
        match &messages[0] {
            Message::Notification { method, params } => {
                assert_eq!(method, "textDocument/didOpen");
                assert_eq!(params["textDocument"]["uri"], "file:///a.md");
            }
            other => panic!("expected a notification, got {other:?}"),
        }
    }

    /// The header counts bytes. A note with an accented character in it is the
    /// ordinary case, not the exotic one.
    #[test]
    fn the_length_counts_bytes_not_characters() {
        let bytes = notification("x", serde_json::json!({ "text": "café naïve — 🎩" }));
        let text = String::from_utf8(bytes.clone()).unwrap();
        let header = text.split("\r\n").next().unwrap();
        let declared: usize = header.split_once(':').unwrap().1.trim().parse().unwrap();
        let body = &bytes[header.len() + 4..];
        assert_eq!(
            declared,
            body.len(),
            "the header must match the body's bytes"
        );
        assert!(
            declared > std::str::from_utf8(body).unwrap().chars().count(),
            "this body is deliberately wider in bytes than in characters"
        );
        assert_eq!(decode(&bytes).len(), 1, "and it decodes");
    }

    #[test]
    fn a_server_request_is_recognised_as_one() {
        let bytes = request(
            7,
            "workspace/configuration",
            serde_json::json!({ "items": [{}] }),
        );
        let messages = decode(&bytes);
        assert!(
            matches!(
                &messages[0],
                Message::Request { id: Id::Number(7), method, .. }
                    if method == "workspace/configuration"
            ),
            "{:?}",
            messages[0]
        );
    }

    #[test]
    fn a_response_is_told_apart_from_a_notification() {
        let bytes =
            encode(&serde_json::json!({ "jsonrpc": "2.0", "id": 1, "result": { "ok": true } }));
        let messages = decode(&bytes);
        assert!(
            matches!(
                &messages[0],
                Message::Response {
                    id: Id::Number(1),
                    result: Some(_),
                    error: None
                }
            ),
            "{:?}",
            messages[0]
        );
    }

    /// One unreadable frame must not end the session: the next one still arrives.
    #[test]
    fn a_bad_frame_is_skipped_not_fatal() {
        let mut bytes = encode(&serde_json::json!({ "this is not": "json-rpc" }));
        bytes.extend_from_slice(&notification(
            "textDocument/publishDiagnostics",
            serde_json::json!({}),
        ));
        let messages = decode(&bytes);
        assert_eq!(messages.len(), 1);
        assert!(
            matches!(
                &messages[0],
                Message::Notification { method, .. }
                    if method == "textDocument/publishDiagnostics"
            ),
            "{:?}",
            messages[0]
        );
    }

    /// A reply has to carry the id it was given, in the shape it arrived.
    #[test]
    fn a_reply_echoes_its_id() {
        let bytes = reply(&Id::Text("abc".into()), Value::Null);
        let messages = decode(&bytes);
        assert!(
            matches!(&messages[0], Message::Response { id: Id::Text(s), .. } if s == "abc"),
            "{:?}",
            messages[0]
        );
    }
}
