//! The queue side of agent edit review, from the agent's end (roadmap P2.5a).
//!
//! A profile with `review = true` does not write: its `write_note` becomes a
//! **proposal** that lands in `.shale/pending-edits/` as a file, and the user accepts
//! or rejects each hunk in Shale. The server writes that file itself, which is what
//! makes two ruled behaviours free — edits queue while Shale is closed, and they
//! survive `kill -TERM` because they were never in the app's memory.
//!
//! Two tools let an agent see its own queue:
//!
//! - `pending_edits` lists what it proposed and what is still waiting, with each
//!   hunk's status (`pending` or `stale`). A broken patch is visible to the agent
//!   that made it, which is the whole point: its "redo" is a fresh write call
//!   re-anchored against the note as it stands now.
//! - `retract_pending` takes one of its own proposals out of the queue. That is the
//!   one queue mutation an agent keeps; **accept is user-only**, always.
//!
//! An agent only ever sees the edits its own profile proposed, and only for notes its
//! scope may read — the queue is not a way to look at a note the profile cannot open.

use anyhow::Result;
use serde_json::{json, Value};
use shale_core::pending::{self, PendingEdit};

use crate::refusal;
use crate::Server;

/// The queue tools, in the order `tools/list` offers them.
pub(crate) fn definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "pending_edits",
            "description": "List this profile's proposed edits that are waiting for the user, with each hunk's status: pending, or stale because the note changed under it.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "note": { "type": "string", "description": "Optional: only proposals for this vault-relative note" }
                }
            }
        }),
        json!({
            "name": "retract_pending",
            "description": "Take one of this profile's own proposals out of the queue. Accepting is the user's alone.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "The edit id the write_note result gave back" }
                },
                "required": ["id"]
            }
        }),
    ]
}

/// Turn a `write_note` call from a profile that asked for review into a queued
/// proposal, and tell the agent what happened in a form it can act on.
pub(crate) fn propose(server: &Server, note: &str, content: &str) -> Result<Value> {
    // The note as it is on disk right now, which is what the diff is taken against.
    // A note that does not exist yet diffs from empty, which is a new note.
    let current = server.vault.read(note).unwrap_or_default();
    let hunks = shale_core::diff::hunks(&current, content);
    if hunks.is_empty() {
        return Ok(json!({
            "content": [{ "type": "text", "text": format!("{note} already says that: nothing to queue") }],
        }));
    }

    let profile_name = server
        .profile_name
        .clone()
        .unwrap_or_else(|| "unprofiled".into());
    let edit = PendingEdit {
        id: PendingEdit::new_id(&profile_name, "write_note", shale_core::now_epoch()),
        note: std::path::PathBuf::from(note),
        profile: profile_name,
        tool: "write_note".to_string(),
        created_at: shale_core::now_epoch(),
        hunks,
    };
    let removed: usize = edit.hunks.iter().map(|h| h.removed()).sum();
    let added: usize = edit.hunks.iter().map(|h| h.added()).sum();
    let path = pending::write(server.vault.root(), &edit)?;

    Ok(json!({
        "content": [{
            "type": "text",
            "text": format!(
                "queued for review as {}: {} hunk(s) in {note} (-{removed} +{added}). \
                 The user accepts or rejects it in Shale's review strip; nothing was \
                 written. The file is {}.",
                edit.id,
                edit.hunks.len(),
                path.display()
            )
        }],
        "structuredContent": { "id": edit.id, "note": note, "hunks": edit.hunks.len() },
    }))
}

/// `pending_edits`: this profile's proposals, oldest first, each hunk marked pending
/// or stale against the note as it stands on disk.
pub(crate) fn list(server: &Server, arguments: &Value) -> Result<Value> {
    let only_note = arguments.get("note").and_then(Value::as_str);
    let queue = pending::list(server.vault.root());

    let mut lines = Vec::new();
    let mut listed = Vec::new();
    for edit in queue.edits {
        // An agent sees its own queue: the profile that proposed it is this one.
        if Some(edit.profile.as_str()) != server.profile_name.as_deref() {
            continue;
        }
        if let Some(note) = only_note {
            if edit.note != std::path::Path::new(note) {
                continue;
            }
        }
        // And a note the scope may not read is not in the queue at all, even though
        // the file is on disk.
        if !server.profile.can_read(&edit.note) {
            continue;
        }

        let current = server.vault.read(&edit.note).unwrap_or_default();
        let stale = pending::stale_hunks(&edit, &current);
        let statuses: Vec<&str> = edit
            .hunks
            .iter()
            .enumerate()
            .map(|(index, _)| {
                if stale.contains(&index) {
                    "stale"
                } else {
                    "pending"
                }
            })
            .collect();
        for (index, status) in statuses.iter().enumerate() {
            lines.push(format!(
                "{} hunk {} {status} ({}): {}",
                edit.id,
                index + 1,
                edit.note.display(),
                edit.hunks[index].text.lines().next().unwrap_or_default()
            ));
        }
        listed.push(json!({
            "id": edit.id,
            "note": edit.note.display().to_string(),
            "tool": edit.tool,
            "created_at": edit.created_at,
            "hunks": edit
                .hunks
                .iter()
                .zip(statuses.iter())
                .map(|(hunk, status)| json!({
                    "status": status,
                    "removed": hunk.removed(),
                    "added": hunk.added(),
                    "diff": hunk.text,
                }))
                .collect::<Vec<_>>(),
        }));
    }

    if listed.is_empty() {
        return Ok(json!({
            "content": [{
                "type": "text",
                "text": match server.profile_name.as_deref() {
                    Some(name) => format!("no pending edits for profile \"{name}\""),
                    None => "no pending edits: this connection has no profile, so its \
                            writes are not queued for review"
                        .to_string(),
                }
            }],
            "structuredContent": { "edits": [] },
        }));
    }
    Ok(json!({
        "content": [{ "type": "text", "text": lines.join("\n") }],
        "structuredContent": { "edits": listed },
    }))
}

/// `retract_pending`: take one of this profile's own proposals out of the queue.
pub(crate) fn retract(server: &Server, arguments: &Value) -> Result<Value> {
    let Some(id) = arguments.get("id").and_then(Value::as_str) else {
        anyhow::bail!("missing string argument: id");
    };
    // Whose edit is it? The queue file says, and the answer decides whether this
    // connection may remove it.
    let path = pending::path_for(server.vault.root(), id)?;
    let edit = match pending::read(&path) {
        Ok(edit) => edit,
        Err(_) => {
            return Ok(refusal(&format!(
                "no pending edit {id}. `pending_edits` lists what this profile has queued."
            )))
        }
    };
    if Some(edit.profile.as_str()) != server.profile_name.as_deref() {
        return Ok(refusal(&format!(
            "{id} was proposed by \"{}\", not by {}; an agent may only retract its own.",
            edit.profile,
            server.who()
        )));
    }
    pending::remove(server.vault.root(), id)?;
    Ok(json!({
        "content": [{
            "type": "text",
            "text": format!("retracted {id} ({}); it will not be offered to the user", edit.note.display())
        }],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use shale_core::config::McpProfile;
    use shale_core::Vault;
    use std::path::Path;

    /// A server whose profile asks for review, which is the whole feature's switch,
    /// over a fresh vault holding one note.
    fn review_server(profile_name: Option<&str>) -> Result<(tempfile::TempDir, Server)> {
        let dir = tempfile::tempdir()?;
        Vault::open(dir.path())?.write("idea.md", "one\ntwo\nthree\n")?;
        let server = review_server_in(dir.path(), profile_name)?;
        Ok((dir, server))
    }

    /// A second profile over the same vault, which is what makes "not its own edit"
    /// mean what it says.
    fn review_server_in(root: &Path, profile_name: Option<&str>) -> Result<Server> {
        let profile = McpProfile {
            review: profile_name.is_some(),
            ..McpProfile::default()
        };
        Ok(Server {
            vault: Vault::open(root)?,
            docs: None,
            profile,
            profile_name: profile_name.map(str::to_string),
        })
    }

    fn text(result: &Value) -> String {
        result["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    }

    /// The P2.4 fail-closed flip: a review profile's write is queued, not refused,
    /// and not written.
    #[test]
    fn a_review_write_is_queued_and_writes_nothing() -> Result<()> {
        let (dir, server) = review_server(Some("writing-help"))?;
        let result = propose(&server, "idea.md", "one\ntwo\nTHREE\n")?;
        let body = text(&result);
        assert!(body.starts_with("queued for review as "), "{body}");
        assert!(body.contains("(-1 +1)"), "the counts come back: {body}");

        let id = result["structuredContent"]["id"].as_str().unwrap();
        assert!(id.contains("writing-help"));
        assert_eq!(
            server.vault.read("idea.md")?,
            "one\ntwo\nthree\n",
            "nothing reached the note"
        );

        let file = shale_core::pending::dir(dir.path()).join(format!("{id}.toml"));
        assert!(file.is_file(), "the queue file is where the plan said");
        Ok(())
    }

    /// A proposal that would change nothing is not queued: an empty hunk in the
    /// strip is noise the user has to switch off.
    #[test]
    fn proposing_what_the_note_already_says_queues_nothing() -> Result<()> {
        let (dir, server) = review_server(Some("writing-help"))?;
        let result = propose(&server, "idea.md", "one\ntwo\nthree\n")?;
        assert!(text(&result).contains("nothing to queue"));
        assert!(shale_core::pending::list(dir.path()).edits.is_empty());
        Ok(())
    }

    #[test]
    fn an_agent_sees_its_own_queue_and_marks_what_has_gone_stale() -> Result<()> {
        let (_dir, server) = review_server(Some("writing-help"))?;
        propose(&server, "idea.md", "one\ntwo\nTHREE\n")?;

        let listed = list(&server, &json!({}))?;
        let edits = listed["structuredContent"]["edits"].as_array().unwrap();
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0]["hunks"][0]["status"], "pending");
        assert!(text(&listed).contains("hunk 1 pending"));

        // The user edits the very line the proposal wanted, and the same hunk is
        // stale to the agent that made it.
        server.vault.write("idea.md", "one\ntwo\nthree, mine\n")?;
        let listed = list(&server, &json!({}))?;
        assert_eq!(
            listed["structuredContent"]["edits"][0]["hunks"][0]["status"],
            "stale"
        );
        Ok(())
    }

    /// One profile's queue is not another's, and an unprofiled connection has none.
    #[test]
    fn the_queue_is_per_profile() -> Result<()> {
        let (dir, mine) = review_server(Some("writing-help"))?;
        propose(&mine, "idea.md", "one\ntwo\nTHREE\n")?;

        let other = review_server_in(dir.path(), Some("someone-else"))?;
        assert!(text(&list(&other, &json!({}))?).contains("no pending edits"));

        let unprofiled = review_server_in(dir.path(), None)?;
        assert!(text(&list(&unprofiled, &json!({}))?).contains("no profile"));
        Ok(())
    }

    /// A note the scope may not read is not in the queue an agent can see, even
    /// though its file is on disk.
    #[test]
    fn the_queue_respects_the_scopes_reads() -> Result<()> {
        let (_dir, mut server) = review_server(Some("writing-help"))?;
        server.vault.write("private/february.md", "hush\n")?;
        propose(&server, "private/february.md", "hush, edited\n")?;
        server.profile.read = vec!["idea.md".into()];
        server.profile.deny = vec!["private/".into()];

        let listed = list(&server, &json!({}))?;
        assert!(text(&listed).contains("no pending edits"), "{listed}");
        Ok(())
    }

    #[test]
    fn an_agent_retracts_its_own_proposal_and_no_one_elses() -> Result<()> {
        let (dir, server) = review_server(Some("writing-help"))?;
        let queued = propose(&server, "idea.md", "one\ntwo\nTHREE\n")?;
        let id = queued["structuredContent"]["id"].as_str().unwrap();

        let other = review_server_in(dir.path(), Some("someone-else"))?;
        let refused = retract(&other, &json!({ "id": id }))?;
        assert_eq!(refused["isError"], true);
        assert!(text(&refused).contains("only retract its own"), "{refused}");

        let done = retract(&server, &json!({ "id": id }))?;
        assert!(text(&done).starts_with("retracted "), "{done}");
        assert!(shale_core::pending::list(server.vault.root())
            .edits
            .is_empty());

        // A second retract says so rather than pretending.
        let gone = retract(&server, &json!({ "id": id }))?;
        assert_eq!(gone["isError"], true);
        Ok(())
    }

    /// An id from a client is confined like every other path it could send.
    #[test]
    fn a_hostile_id_is_refused_before_it_touches_the_disk() {
        let (_dir, server) = review_server(Some("p")).unwrap();
        for hostile in ["../config", "a/b", "..", ""] {
            let result = retract(&server, &json!({ "id": hostile }));
            assert!(result.is_err(), "{hostile:?} must not reach the filesystem");
        }
    }
}
