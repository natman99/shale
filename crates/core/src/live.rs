//! The live channel between a running Shale and an MCP client (roadmap P2.3).
//!
//! Shale speaks MCP in **two modes**, and this module is the part they agree on:
//!
//! - The **app** serves a small HTTP API on loopback while it runs. It holds the
//!   only copy of what the user is looking at — the open note, the cursor, the
//!   selection, the jump chain — so it is the only thing that can answer for it.
//! - **`shale-mcp`** speaks stdio MCP and owns the vault and the index. It proxies
//!   the live questions to the app over HTTP, and answers everything else itself.
//!
//! The app leaves a [`Lock`] in `<vault>/.shale/mcp.lock` so `shale-mcp` can find the
//! port. **The lockfile is a hint, never a fact** (the binding rule from the
//! 2026-10-29 review): a pid can be reused, and an app that was `kill -9`ed leaves a
//! lock behind. The authority is the probe — [`probe`] actually asks the endpoint —
//! and a live tool is only ever offered when that probe answers.
//!
//! ## What the HTTP surface is, and is not
//!
//! It binds **127.0.0.1** and nothing else, and every request must carry the token
//! the lockfile names. That token keeps *other local processes* from driving the
//! editor; it is not a secret from anyone who can read `<vault>/.shale/`, who could
//! read the vault itself anyway. The API is **read-only** for now: an agent changes
//! the vault through the vault tools, which the app notices as a disk change, and it
//! changes the *window* only once the pending-edit review path (roadmap P2.5) exists
//! to show the user what is about to happen. A live tool that can move the user's
//! screen without that review is the one thing the operational rules forbid.
//!
//! ## Why the HTTP is hand-rolled
//!
//! The project's dependency rule keeps bindings out, and this needs one request and
//! one response per connection: `GET`, a token header, a JSON body, `Connection:
//! close`. That is a few dozen lines of parsing, against an HTTP crate that would
//! pull a runtime into the app binary. [`parse_response`] is deliberately strict —
//! it is the function that decides whether a port is Shale or some other program
//! that grabbed it first.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context as _, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::atomic;
use crate::config::Config;

/// The file a running app leaves in `<vault>/.shale/`.
pub const LOCK_FILE: &str = "mcp.lock";

/// The header every request must carry, named by the lockfile.
pub const TOKEN_HEADER: &str = "x-shale-token";

/// The endpoint that answers for the app being there at all. It is also the shape of
/// the `shale://status` MCP resource.
pub const PATH_STATUS: &str = "/status";
/// The open note: path, dirty flag, view, cursor.
pub const PATH_NOTE: &str = "/note";
/// The selection, in byte offsets, and whether it is visual or mouse.
pub const PATH_SELECTION: &str = "/selection";
/// The jump chain, nearest first, both directions.
pub const PATH_JUMPS: &str = "/jumps";

/// The live tools the app can answer for, in the order it lists them. `shale-mcp`
/// offers exactly this list when the probe succeeds and none of it when it does not,
/// so an agent never sees a tool that would fail.
pub const LIVE_TOOLS: &[&str] = &["live_status", "live_note", "live_selection", "live_jumps"];

/// How long to wait for a process that is on the same machine. Generous for a
/// scheduler hiccup, short enough that a dead app is reported rather than waited on.
const CONNECT_TIMEOUT: Duration = Duration::from_millis(400);
/// The same for the reply. The app answers from a snapshot it keeps ready, so a slow
/// reply means a stuck process.
const READ_TIMEOUT: Duration = Duration::from_millis(1500);

/// What a running app leaves in `<vault>/.shale/mcp.lock`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lock {
    /// The app's process id. **Informational only** — pid reuse is real, and an app
    /// that was killed leaves this file behind, so liveness is decided by [`probe`].
    pub pid: u32,
    /// The loopback port the app bound. Port 0 asked the OS for one, so this is what
    /// the OS gave.
    pub port: u16,
    /// The token every request must carry. See [`token`].
    pub token: String,
    /// The app's version, so a mismatch can be reported instead of guessed at.
    pub version: String,
    /// When the app started, in unix epoch seconds — the same stamp the index uses.
    pub started_at: i64,
}

impl Lock {
    /// Where the lock lives for a given vault.
    pub fn path(vault_root: &Path) -> PathBuf {
        Config::shale_dir(vault_root).join(LOCK_FILE)
    }

    /// Write it. Atomic like every file Shale owns, so a reader never sees half.
    pub fn write(&self, vault_root: &Path) -> Result<()> {
        std::fs::create_dir_all(Config::shale_dir(vault_root))
            .with_context(|| format!("creating {}", Config::shale_dir(vault_root).display()))?;
        atomic::write_file(
            &Self::path(vault_root),
            &serde_json::to_string_pretty(self).context("serializing the MCP lock")?,
        )
    }

    /// Read it. `None` when there is no lock at all; an error when one is there but
    /// cannot be read — a lock written by a newer Shale, or truncated by a crash, is
    /// worth saying so about, and every caller still treats it as "nothing safe to
    /// connect to".
    pub fn read(vault_root: &Path) -> Result<Option<Lock>> {
        let path = Self::path(vault_root);
        match std::fs::read_to_string(&path) {
            Ok(text) => Ok(Some(serde_json::from_str::<Lock>(&text).with_context(
                || format!("the MCP lock at {} is unreadable", path.display()),
            )?)),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err).with_context(|| format!("reading {}", path.display())),
        }
    }

    /// Take the lock away, as a graceful exit does. A missing file is not an error:
    /// the app may have been told to leave before it finished starting.
    pub fn remove(vault_root: &Path) -> Result<()> {
        match std::fs::remove_file(Self::path(vault_root)) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err).context("removing the MCP lock"),
        }
    }
}

/// A per-process token. Generated from what only this process knows at the moment it
/// starts — its pid, the clock's sub-second part, and a counter — rather than from
/// anything downloaded. It is a door-lock against other local processes, not a
/// defense against a reader of `<vault>/.shale/`.
pub fn token() -> String {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    let mut hasher = DefaultHasher::new();
    std::process::id().hash(&mut hasher);
    nanos.hash(&mut hasher);
    COUNTER
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        .hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// What `shale://status` answers, and what `shale-mcp` serves from it: the pollable
/// contract that lets an agent adapt when the app opens or closes mid-session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    /// Whether a Shale window answered the probe. `false` is what `shale-mcp` reports
    /// when it did not, with `live_tools` empty.
    pub app_running: bool,
    /// The live tools that can be called right now. Empty when the app is not there.
    pub live_tools: Vec<String>,
    /// The app's version, empty when it is not running.
    pub version: String,
    /// The port the app answered on, 0 when it is not running.
    pub port: u16,
    /// What has the keyboard in the app — `none`, `picker`, `prompt` or `popup`. A
    /// live mutation would have to queue behind anything but `none`; the field is
    /// reported now so the rule has something to read when mutations arrive.
    pub overlay: String,
}

impl Status {
    /// The status of an app that is not there, which is a value rather than an error:
    /// `shale-mcp` still has a vault to serve.
    pub fn absent() -> Status {
        Status {
            app_running: false,
            live_tools: Vec::new(),
            version: String::new(),
            port: 0,
            overlay: "none".to_string(),
        }
    }
}

/// The address the app binds. Loopback only, so nothing off this machine can reach
/// the editor.
pub fn local_addr() -> SocketAddr {
    (Ipv4Addr::LOCALHOST, 0).into()
}

/// Ask the app one question. The reply's JSON body, when the answer is 200.
pub fn get(port: u16, token: &str, path: &str) -> Result<Value> {
    let addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), port);
    let mut stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)
        .with_context(|| format!("connecting to Shale on port {port}"))?;
    stream
        .set_read_timeout(Some(READ_TIMEOUT))
        .context("setting the read timeout")?;
    stream
        .set_write_timeout(Some(READ_TIMEOUT))
        .context("setting the write timeout")?;
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {addr}\r\n{TOKEN_HEADER}: {token}\r\n\
         Accept: application/json\r\nConnection: close\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .context("sending the request")?;
    stream.flush().context("flushing the request")?;
    // `Connection: close` is what the server is told to answer with, so EOF is the
    // end of the body and there is no framing to keep track of.
    let mut bytes = Vec::new();
    stream
        .read_to_end(&mut bytes)
        .context("reading the reply")?;
    let (code, body) = parse_response(&bytes).with_context(|| format!("{path} on port {port}"))?;
    if code != 200 {
        bail!("{path} answered {code}: {body}");
    }
    Ok(body)
}

/// Is the app there? The one check the rules allow for deciding whether to offer a
/// live tool: ask it, and believe the answer rather than the lockfile.
pub fn probe(port: u16, token: &str) -> Option<Status> {
    let body = get(port, token, PATH_STATUS).ok()?;
    serde_json::from_value::<Status>(body).ok()
}

/// Read a status line, headers and a JSON body off a reply. Strict on purpose: this
/// is what tells a Shale answer from some other program's port.
fn parse_response(bytes: &[u8]) -> Result<(u16, Value)> {
    let split = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .context("the reply had no header terminator")?;
    let head = std::str::from_utf8(&bytes[..split]).context("the reply's headers are not utf-8")?;
    let status_line = head.lines().next().context("the reply was empty")?;
    let code = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .with_context(|| format!("no status code in {status_line:?}"))?;
    let body = &bytes[split + 4..];
    if body.is_empty() {
        return Ok((code, Value::Null));
    }
    Ok((
        code,
        serde_json::from_slice(body).context("the reply's body is not JSON")?,
    ))
}

/// The JSON body of a live endpoint, rendered the way MCP wants it: text an agent can
/// read, with the JSON kept as structured content beside it.
pub fn mcp_json(body: &Value) -> Value {
    json!({
        "content": [{ "type": "text", "text": serde_json::to_string_pretty(body).unwrap_or_default() }],
        "structuredContent": body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lock_round_trips_and_lives_in_the_shale_dir() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let lock = Lock {
            pid: 4242,
            port: 4321,
            token: "deadbeef".into(),
            version: "0.1.0".into(),
            started_at: 1_700_000_000,
        };
        assert!(
            Lock::read(dir.path())?.is_none(),
            "no lock before the app starts"
        );
        lock.write(dir.path())?;
        assert_eq!(Lock::read(dir.path())?.as_ref(), Some(&lock));
        assert!(Lock::path(dir.path()).ends_with(".shale/mcp.lock"));
        Lock::remove(dir.path())?;
        assert!(Lock::read(dir.path())?.is_none());
        // Removing a lock that is already gone is not an error: the exit path runs it
        // whether or not the server ever got as far as writing one.
        Lock::remove(dir.path())?;
        Ok(())
    }

    #[test]
    fn an_unreadable_lock_is_reported_rather_than_guessed_at() -> Result<()> {
        let dir = tempfile::tempdir()?;
        std::fs::create_dir_all(Config::shale_dir(dir.path()))?;
        std::fs::write(Lock::path(dir.path()), "{ not json")?;
        let err = Lock::read(dir.path()).unwrap_err();
        assert!(
            format!("{err:#}").contains("unreadable"),
            "the caller gets to say what it found: {err:#}"
        );
        Ok(())
    }

    #[test]
    fn a_response_is_read_strictly() {
        let ok = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"a\":1}";
        assert_eq!(parse_response(ok).unwrap().0, 200);
        assert_eq!(parse_response(ok).unwrap().1, json!({ "a": 1 }));

        let not_found = b"HTTP/1.1 404 Not Found\r\n\r\n{\"error\":\"no such path\"}";
        assert_eq!(parse_response(not_found).unwrap().0, 404);

        // An empty body is an answer with nothing to say, not a parse failure.
        let empty = b"HTTP/1.1 204 No Content\r\n\r\n";
        assert_eq!(parse_response(empty).unwrap(), (204, Value::Null));

        // Not HTTP at all, and no status code: both are "that is not Shale".
        assert!(parse_response(b"SSH-2.0-OpenSSH").is_err());
        assert!(parse_response(b"HTTP/1.1 \r\n\r\n").is_err());
        assert!(parse_response(b"HTTP/1.1 200 OK\r\n\r\nnot json").is_err());
    }

    #[test]
    fn a_token_is_hex_and_two_calls_differ() {
        let a = token();
        let b = token();
        assert_eq!(a.len(), 16, "a fixed-width hex token");
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(
            a, b,
            "the counter separates two calls in the same nanosecond"
        );
    }

    #[test]
    fn the_absent_status_is_a_value_not_an_error() {
        let status = Status::absent();
        assert!(!status.app_running);
        assert!(status.live_tools.is_empty());
        assert_eq!(status.port, 0);
        // It round-trips through the same shape the app answers with, so `shale-mcp`
        // can serve either without a second type.
        let back: Status = serde_json::from_value(serde_json::to_value(&status).unwrap()).unwrap();
        assert_eq!(back, status);
    }

    #[test]
    fn the_app_only_listens_on_loopback() {
        assert_eq!(local_addr().ip().to_string(), "127.0.0.1");
    }

    #[test]
    fn a_probe_of_a_port_nobody_listens_on_is_none() {
        // Port 1 is privileged and nothing of ours is there: the probe has to fail
        // closed, because that failure is what keeps an agent from being offered a
        // live tool that cannot work.
        assert!(probe(1, "whatever").is_none());
    }
}
