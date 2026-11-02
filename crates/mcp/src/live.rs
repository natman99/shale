//! The live tools: what `shale-mcp` can only answer by asking a running Shale
//! window (roadmap P2.3).
//!
//! The window publishes its state over a loopback port and leaves
//! `.shale/mcp.lock` naming it; this module asks that port. The binding rule is that
//! the lockfile is a **hint, never an authority**: a pid can be reused and a port
//! taken over, so every decision here — whether to offer the tools at all, and
//! whether a call worked — comes from what the port actually answers.
//!
//! When nothing answers, the answer is a plain "live tools unavailable" rather than
//! a failure: the vault and its index are still there to serve, which is the other
//! half of the two-mode design.
//!
//! Diagnostics go to stderr. Stdout is the protocol.

use std::path::Path;

use anyhow::Result;
use serde_json::{json, Value};
use shale_core::live::{
    get, mcp_json, probe, Lock, Status, PATH_JUMPS, PATH_NOTE, PATH_SELECTION, PATH_STATUS,
};

/// The live tools, in the order `tools/list` offers them. Each is one endpoint of
/// the window's answer, and each takes no arguments: the question is always "what is
/// the user looking at now".
pub(crate) fn definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "live_status",
            "description": "Is a Shale window up, which version, and what has the keyboard (none, picker, prompt, popup).",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "live_note",
            "description": "The note the window is showing: path, dirty, view, vim mode, and the cursor as row/col/byte.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "live_selection",
            "description": "The window's selection as byte offsets, however it was made. Null when nothing is selected.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "live_jumps",
            "description": "The window's jump chain: back, forward, and the note it is standing on.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
    ]
}

/// Is a window answering? The one check allowed before offering a live tool, and the
/// same check `shale://status` reports.
pub(crate) fn status(vault_root: &Path) -> Status {
    match Lock::read(vault_root) {
        Ok(Some(lock)) => probe(lock.port, &lock.token).unwrap_or_else(|| {
            // A lock that names a port nobody answers is what a `kill -9` leaves
            // behind. Say so, then believe the probe.
            eprintln!(
                "shale-mcp: .shale/mcp.lock names port {}, which did not answer",
                lock.port
            );
            Status::absent()
        }),
        Ok(None) => Status::absent(),
        Err(err) => {
            eprintln!("shale-mcp: .shale/mcp.lock could not be read: {err:#}");
            Status::absent()
        }
    }
}

/// Whether the live tools should be offered right now.
pub(crate) fn available(vault_root: &Path) -> bool {
    status(vault_root).app_running
}

/// Ask the window one of the four questions. A window that is not there is an
/// `isError` result the agent can read, not a protocol failure: the vault tools are
/// still good, and the reason the live half is not is worth more than an error code.
pub(crate) fn call(vault_root: &Path, name: &str) -> Result<Value> {
    let Some(path) = endpoint(name) else {
        anyhow::bail!("unknown tool: {name}");
    };
    let lock = match Lock::read(vault_root) {
        Ok(Some(lock)) => lock,
        Ok(None) => return unavailable("no .shale/mcp.lock: no Shale window has opened one"),
        Err(err) => return unavailable(&format!(".shale/mcp.lock could not be read: {err:#}")),
    };
    match get(lock.port, &lock.token, path) {
        Ok(body) => Ok(mcp_json(&body)),
        Err(err) => unavailable(&format!(
            "the port in .shale/mcp.lock ({}) is not answering: {err:#}",
            lock.port
        )),
    }
}

/// The endpoint a live tool asks, or `None` for a name that is not one.
fn endpoint(name: &str) -> Option<&'static str> {
    match name {
        "live_status" => Some(PATH_STATUS),
        "live_note" => Some(PATH_NOTE),
        "live_selection" => Some(PATH_SELECTION),
        "live_jumps" => Some(PATH_JUMPS),
        _ => None,
    }
}

/// The "live tools unavailable" answer, spelled the same wherever it comes from so
/// an agent can act on the phrase rather than on the reason beside it.
fn unavailable(why: &str) -> Result<Value> {
    Ok(json!({
        "content": [{
            "type": "text",
            "text": format!(
                "live tools unavailable: {why}. The vault tools still work; ask those, or \
                 start Shale and try again."
            )
        }],
        "isError": true,
    }))
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    use super::*;

    /// A stand-in for a Shale window: answers the four paths over loopback, token
    /// required, one reply per connection. The responder thread lives as long the
    /// test process does, which is fine for a test that only ever asks it questions.
    struct FakeWindow {
        port: u16,
        token: String,
    }

    impl FakeWindow {
        fn start() -> Result<FakeWindow> {
            let listener = TcpListener::bind(shale_core::live::local_addr())?;
            let port = listener.local_addr()?.port();
            let token = shale_core::live::token();

            let serving = listener.try_clone()?;
            let answer_token = token.clone();
            std::thread::spawn(move || {
                for accepted in serving.incoming() {
                    let Ok(mut stream) = accepted else { break };
                    // Read the head, not to EOF: the client waits for the reply
                    // before it closes, so reading to EOF here would deadlock.
                    let mut head = Vec::new();
                    let mut chunk = [0u8; 512];
                    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                        match stream.read(&mut chunk) {
                            Ok(0) | Err(_) => break,
                            Ok(read) => head.extend_from_slice(&chunk[..read]),
                        }
                    }
                    let head = String::from_utf8_lossy(&head).into_owned();
                    let asked = head
                        .lines()
                        .next()
                        .and_then(|line| line.split_whitespace().nth(1))
                        .unwrap_or("")
                        .to_string();
                    let body = if !head
                        .lines()
                        .any(|line| line.to_ascii_lowercase().contains(&answer_token))
                    {
                        "HTTP/1.1 401 Nope\r\nContent-Length: 0\r\n\r\n".to_string()
                    } else if asked == PATH_STATUS {
                        let status = Status {
                            app_running: true,
                            live_tools: shale_core::live::LIVE_TOOLS
                                .iter()
                                .map(|name| (*name).to_string())
                                .collect(),
                            version: "9.9.9".into(),
                            port,
                            overlay: "none".into(),
                        };
                        let body = serde_json::to_string(&status).unwrap();
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                            body.len()
                        )
                    } else {
                        let body = json!({ "path": "Now Here.md", "view": "raw" }).to_string();
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                            body.len()
                        )
                    };
                    let _ = stream.write_all(body.as_bytes());
                    let _ = stream.flush();
                }
            });
            Ok(FakeWindow { port, token })
        }

        /// The lock a running window would have left.
        fn lock(&self) -> Lock {
            Lock {
                pid: std::process::id(),
                port: self.port,
                token: self.token.clone(),
                version: "9.9.9".into(),
                started_at: shale_core::now_epoch(),
            }
        }
    }

    #[test]
    fn nothing_is_offered_without_a_window() -> Result<()> {
        let dir = tempfile::tempdir()?;
        assert!(!available(dir.path()), "no lock, no live tools");
        assert_eq!(status(dir.path()), Status::absent());

        let answer = call(dir.path(), "live_note")?;
        assert_eq!(answer["isError"], true);
        assert!(answer["content"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("live tools unavailable"));
        Ok(())
    }

    #[test]
    fn a_window_that_answers_is_believed() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let window = FakeWindow::start()?;
        window.lock().write(dir.path())?;

        assert!(available(dir.path()));
        let status = status(dir.path());
        assert_eq!(status.version, "9.9.9", "the version comes from the answer");
        assert_eq!(status.port, window.port);
        assert_eq!(status.live_tools, shale_core::live::LIVE_TOOLS);

        let answer = call(dir.path(), "live_note")?;
        assert_eq!(answer["isError"], Value::Null);
        assert_eq!(answer["structuredContent"]["path"], "Now Here.md");
        Ok(())
    }

    /// The rule the whole design rests on: a lock that names a port which does not
    /// answer — a `kill -9`, a taken-over port, a token that is not the one the
    /// window gave — is not believed, however new the file looks.
    #[test]
    fn a_lock_that_names_a_port_that_is_not_shale_is_not_believed() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let window = FakeWindow::start()?;
        let mut lock = window.lock();
        lock.token = "0123456789abcdef".into();
        lock.write(dir.path())?;

        assert!(
            !available(dir.path()),
            "the token is refused, so no live tools"
        );
        let answer = call(dir.path(), "live_status")?;
        assert_eq!(answer["isError"], true);
        assert!(answer["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("not answering"));
        Ok(())
    }

    /// A lock the window left but that no longer names a live port: the classic
    /// `kill -9` remains. A port that was bound and then closed is what that looks
    /// like from the other side, so the test makes one rather than pretending.
    #[test]
    fn a_stale_lock_is_reported_and_ignored() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let listener = TcpListener::bind(shale_core::live::local_addr())?;
        let port = listener.local_addr()?.port();
        drop(listener);

        Lock {
            pid: std::process::id(),
            port,
            token: shale_core::live::token(),
            version: "9.9.9".into(),
            started_at: shale_core::now_epoch(),
        }
        .write(dir.path())?;

        assert!(!available(dir.path()));
        let answer = call(dir.path(), "live_jumps")?;
        assert_eq!(answer["isError"], true);
        assert!(
            answer["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains(&port.to_string()),
            "and the answer names the port it tried"
        );
        Ok(())
    }

    /// A tool name that is not one of the four is a protocol error, not a polite
    /// "unavailable": the agent asked for something that does not exist.
    #[test]
    fn a_live_tool_that_does_not_exist_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(call(dir.path(), "live_dance").is_err());
    }
}
