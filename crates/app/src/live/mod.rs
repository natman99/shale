//! The app's half of the live channel: a loopback HTTP server that answers what the
//! window is showing (roadmap P2.3).
//!
//! It exists so an agent can ask *"what is the user looking at?"* — a question only a
//! running window can answer, since the note on disk does not know where the cursor
//! is. [`shale_core::live`] holds the contract both sides agree on; this module is
//! the serving end.
//!
//! Three rules shape it:
//!
//! - **Loopback only.** [`shale_core::live::local_addr`] is 127.0.0.1, so nothing off
//!   this machine can reach the editor.
//! - **A token, from the lockfile.** Every request must carry it. That keeps other
//!   local processes from driving the window; it is not a secret from a reader of
//!   `<vault>/.shale/`, who could read the vault itself.
//! - **Reads, never writes.** An agent changes the vault with the vault tools, which
//!   the app notices as a disk change. Changing the *window* waits for the edit
//!   review path (roadmap P2.5), because a tool that moves the user's screen without
//!   showing them why is what the operational rules forbid.
//!
//! The thread answers from the [`Snapshot`] the window publishes, so it never touches
//! gpui state, which belongs to the main thread.

mod state;

pub(crate) use state::Snapshot;

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{Context as _, Result};
use serde_json::{json, Value};
use shale_core::live::{Lock, PATH_JUMPS, PATH_NOTE, PATH_SELECTION, PATH_STATUS, TOKEN_HEADER};

/// The most request head the server will read before deciding it is not a request.
/// A real one is a request line, a token header and a couple more, well under this.
const MAX_HEAD: usize = 8 * 1024;

/// How long an accepted connection gets to send its head. Longer than a localhost
/// round trip by orders of magnitude, short enough that a stalled client cannot hold
/// the server.
const REQUEST_TIMEOUT: Duration = Duration::from_millis(2000);

/// How often the accept loop looks at the stop flag, since `accept` would otherwise
/// block forever. One poll per request would be enough; this is the same shape as the
/// termination-signal poll, which is the pattern in this crate for "a thread that
/// must be able to be told to stop".
const ACCEPT_POLL: Duration = Duration::from_millis(50);

/// What the server answers with for its own version, so `shale://status` can name the
/// build the user is looking at.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The running server. The window holds one so a graceful exit can take the lockfile
/// and the port with it.
pub(crate) struct LiveServer {
    port: u16,
    /// The token every request must carry, also written to the lockfile.
    token: String,
    /// Which vault's `.shale/` the lock belongs to.
    vault_root: std::path::PathBuf,
    stop: Arc<AtomicBool>,
    /// The accept loop, joined by [`LiveServer::stop`] so stopping really stops.
    thread: Option<JoinHandle<()>>,
}

impl LiveServer {
    /// Bind a loopback port, start answering, and leave the lockfile that names it.
    ///
    /// Port 0 asks the OS, which matters: two vaults open at once, or a Shale and
    /// something else that had the port, and each server still gets one.
    pub(crate) fn start(vault_root: &Path, shared: Arc<Mutex<Snapshot>>) -> Result<LiveServer> {
        let listener = TcpListener::bind(shale_core::live::local_addr())
            .context("binding the live MCP channel to loopback")?;
        let port = listener
            .local_addr()
            .context("asking which port the live channel was given")?
            .port();
        listener
            .set_nonblocking(true)
            .context("putting the live channel's listener in nonblocking mode")?;

        let token = shale_core::live::token();
        let stop = Arc::new(AtomicBool::new(false));
        let serving = Serving {
            port,
            token: token.clone(),
            shared,
        };
        let thread_stop = Arc::clone(&stop);
        let thread = std::thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _addr)) => serving.serve(stream),
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(ACCEPT_POLL)
                    }
                    Err(err) => {
                        tracing::warn!("the live channel stopped accepting: {err}");
                        break;
                    }
                }
            }
        });

        let server = LiveServer {
            port,
            token,
            vault_root: vault_root.to_path_buf(),
            stop,
            thread: Some(thread),
        };
        Lock {
            pid: std::process::id(),
            port: server.port,
            token: server.token.clone(),
            version: VERSION.to_string(),
            started_at: shale_core::now_epoch(),
        }
        .write(&server.vault_root)
        .context("writing the MCP lock")?;
        Ok(server)
    }

    /// The port the app answered on, for the window's own reporting.
    pub(crate) fn port(&self) -> u16 {
        self.port
    }

    /// Stop listening and take the lock away, so the next `shale-mcp` to look finds
    /// nothing rather than a stale hint. A graceful exit runs this; a `kill -9` does
    /// not, which is exactly why the client probes instead of trusting the file.
    pub(crate) fn stop(mut self) -> Result<()> {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        Lock::remove(&self.vault_root)
    }
}

/// What one connection needs in order to be answered: the token, the port to name in
/// `/status`, and the snapshot the window keeps published.
struct Serving {
    port: u16,
    token: String,
    shared: Arc<Mutex<Snapshot>>,
}

impl Serving {
    /// Answer one connection and close it. Every failure is quiet: a client that
    /// sends rubbish is not worth a log line each, and the app has a note to keep.
    fn serve(&self, mut stream: TcpStream) {
        let _ = stream.set_read_timeout(Some(REQUEST_TIMEOUT));
        let head = match read_head(&mut stream) {
            Ok(head) => head,
            Err(_) => return,
        };
        let Some(request) = parse_request(&head) else {
            respond(
                &mut stream,
                400,
                json!({ "error": "that is not a request" }),
            );
            return;
        };
        if request.token.as_deref() != Some(self.token.as_str()) {
            respond(
                &mut stream,
                401,
                json!({ "error": "the token in .shale/mcp.lock was not offered" }),
            );
            return;
        }
        if request.method != "GET" {
            respond(
                &mut stream,
                405,
                json!({ "error": format!("{} is not allowed; ask with GET", request.method) }),
            );
            return;
        }
        // The lock is only ever held for the few microseconds it takes to copy the
        // snapshot out, so a window publishing while an agent asks cannot block it.
        let snapshot = self
            .shared
            .lock()
            .map(|snapshot| snapshot.clone())
            .unwrap_or_default();
        match snapshot.answer(&request.path, VERSION, self.port) {
            Some(body) => respond(&mut stream, 200, body),
            None => respond(
                &mut stream,
                404,
                json!({
                    "error": format!("no such path: {}", request.path),
                    "paths": [PATH_STATUS, PATH_NOTE, PATH_SELECTION, PATH_JUMPS],
                }),
            ),
        }
    }
}

/// A parsed request head: the method, the path, and the token it offered (if any).
struct Request {
    method: String,
    path: String,
    token: Option<String>,
}

/// Read up to the header terminator, and no further than [`MAX_HEAD`].
fn read_head(stream: &mut TcpStream) -> Result<String> {
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        let read = stream.read(&mut chunk).context("reading the request")?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..read]);
        if bytes.windows(4).any(|window| window == b"\r\n\r\n") || bytes.len() >= MAX_HEAD {
            break;
        }
    }
    anyhow::ensure!(!bytes.is_empty(), "the connection sent nothing");
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// The request line and the token header. `None` when the first line is not a
/// request line at all, which is what keeps a port-scanner from getting an answer.
fn parse_request(head: &str) -> Option<Request> {
    let mut lines = head.lines();
    let mut request_line = lines.next()?.split_whitespace();
    let method = request_line.next()?.to_string();
    let path = request_line.next()?.to_string();
    let token = lines.find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.trim()
            .eq_ignore_ascii_case(TOKEN_HEADER)
            .then(|| value.trim().to_string())
    });
    Some(Request {
        method,
        path,
        token,
    })
}

/// One JSON response, closed. `Connection: close` is what lets the client read to EOF
/// instead of framing, so both sides know the reply ended.
fn respond(stream: &mut TcpStream, code: u16, body: Value) {
    let reason = match code {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Error",
    };
    let text = body.to_string();
    let response = format!(
        "HTTP/1.1 {code} {reason}\r\nContent-Type: application/json\r\nContent-Length: \
         {}\r\nConnection: close\r\n\r\n{text}",
        text.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

#[cfg(test)]
mod tests {
    use super::*;
    use shale_core::live::{get, probe, Status};

    /// A snapshot with something in it, so an answer can be told from a default.
    fn snapshot() -> Arc<Mutex<Snapshot>> {
        Arc::new(Mutex::new(Snapshot {
            note: Some("notes/Idea.md".into()),
            dirty: true,
            view: "raw".into(),
            mode: "normal".into(),
            row: 1,
            col: 3,
            byte: 12,
            selection: None,
            back: vec!["notes/Before.md".into()],
            forward: Vec::new(),
            overlay: "picker".into(),
            updated_at: 1_700_000_000,
        }))
    }

    #[test]
    fn the_server_answers_the_window_and_leaves_a_lock_that_names_it() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let server = LiveServer::start(dir.path(), snapshot())?;

        let lock = Lock::read(dir.path())?.expect("a running server leaves a lock");
        assert_eq!(lock.port, server.port());
        assert_eq!(lock.pid, std::process::id());

        let status: Status =
            serde_json::from_value(get(server.port(), &lock.token, PATH_STATUS)?).unwrap();
        assert!(status.app_running);
        assert_eq!(
            status.overlay, "picker",
            "what has the keyboard is part of the answer"
        );

        let note = get(server.port(), &lock.token, PATH_NOTE)?;
        assert_eq!(note["path"], "notes/Idea.md");
        assert_eq!(note["view"], "raw");
        assert_eq!(note["cursor"]["byte"], 12);

        let jumps = get(server.port(), &lock.token, PATH_JUMPS)?;
        assert_eq!(jumps["back"][0], "notes/Before.md");

        server.stop()?;
        assert!(
            Lock::read(dir.path())?.is_none(),
            "a graceful exit takes the lock"
        );
        Ok(())
    }

    #[test]
    fn the_wrong_token_and_the_unknown_path_are_refused_by_name() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let server = LiveServer::start(dir.path(), snapshot())?;
        let token = "0123456789abcdef";

        let err = format!("{:#}", get(server.port(), token, PATH_NOTE).unwrap_err());
        assert!(err.contains("401"), "a wrong token is refused: {err}");

        let err = format!(
            "{:#}",
            get(server.port(), &server.token, "/not-a-path").unwrap_err()
        );
        assert!(err.contains("404"), "an unknown path is refused: {err}");
        assert!(
            err.contains("/note"),
            "and the 404 says what can be asked: {err}"
        );
        server.stop()?;
        Ok(())
    }

    #[test]
    fn stopping_the_server_stops_the_answer() -> Result<()> {
        // The whole point of the client probing rather than reading the lockfile:
        // once the app is gone, the port must be silent, not merely unlisted.
        let dir = tempfile::tempdir()?;
        let server = LiveServer::start(dir.path(), snapshot())?;
        let port = server.port();
        let token = server.token.clone();
        assert!(probe(port, &token).is_some(), "while it runs, it answers");
        server.stop()?;
        assert!(
            probe(port, &token).is_none(),
            "after it stops, nothing answers"
        );
        Ok(())
    }

    #[test]
    fn rubbish_on_the_port_gets_nothing_it_can_use() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let server = LiveServer::start(dir.path(), snapshot())?;
        let mut stream = std::net::TcpStream::connect(("127.0.0.1", server.port()))?;
        stream.write_all(b"GET /note HTTP/1.1\r\n\r\n")?;
        let mut reply = String::new();
        stream.read_to_string(&mut reply)?;
        assert!(reply.contains("401"), "no token, no answer: {reply}");
        server.stop()?;
        Ok(())
    }
}
