#!/usr/bin/env python3
"""Queue an agent proposal against the note Shale has open, so the review strip has something to show.

Usage:
  scripts/review_proposal.py --vault PATH [--note PATH] [--text TEXT]
                             [--profile NAME] [--retract ID] [--clean] [--binary PATH]

Why this exists: the review path needs three things lined up at once -- a profile with
`review = true`, a proposal file in `.shale/pending-edits/`, and a Shale window looking at
that queue. Rebuilding that by hand every time the strip needs looking at cost about as
long as the change being tested.

What it does:

1. **Finds the note.** `--note` wins. Otherwise it asks the running window through
   `live_note`, which is the point of the whole thing: the proposal lands against whatever
   note you are actually looking at, and the strip appears without touching the keyboard.
2. **Writes a profile scoped to exactly that note** into `<vault>/.shale/config.toml`, with
   `review = true`. The block sits between two sentinel comment lines and is rewritten in
   place, so it follows the note from run to run and never widens past the note it was
   written for. `--clean` deletes it.
3. **Reads the note, appends one line, calls `write_note`.** Because the profile reviews,
   that lands as a proposal in the queue rather than as a write to the file.
4. **Prints the id** and the command that takes the proposal back.

Nothing here edits the note. If the note on disk changes after this runs, that is the
review path failing, not a side effect to shrug at: accepting is the user's, in the window.
"""

import argparse
import json
import os
import subprocess
from pathlib import Path
from typing import NoReturn

# Delimiters for the block this script owns inside the vault's config. The comment is the
# marker: `shale` rewrites config lines it owns and leaves the rest of the file alone, so a
# block that can be found by its own comment can be replaced without parsing TOML.
SENTINEL_IN = "# >>> scripts/review_proposal.py (agent scratch profile; delete freely)"
SENTINEL_OUT = "# <<< scripts/review_proposal.py"

REPO = Path(__file__).resolve().parent.parent


def die(message: str) -> NoReturn:
    """Stop with the reason on stderr, since a half-done review is worse than none."""
    raise SystemExit(f"review_proposal: {message}")


def find_binary(explicit: str | None) -> str:
    """The `shale-mcp` to drive.

    Checked in the order a developer would expect: what they said, what they exported,
    this repo's debug build, this repo's release build, then whatever is on PATH.
    """
    if explicit:
        return explicit
    if os.environ.get("SHALE_MCP"):
        return os.environ["SHALE_MCP"]
    for candidate in ("target/debug/shale-mcp", "target/release/shale-mcp"):
        path = REPO / candidate
        if path.exists():
            return str(path)
    return "shale-mcp"


class Mcp:
    """One `shale-mcp` child process, spoken to over stdio.

    A connection is cheap and its scope is decided at startup from `--profile`, so the
    script opens one per scope rather than reusing a single connection across both.
    """

    def __init__(self, binary: str, vault: Path, profile: str | None) -> None:
        args = [binary, "--vault", str(vault)]
        if profile:
            args += ["--profile", profile]
        self.proc = subprocess.Popen(
            args,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        self.counter = 0
        self._handshake()

    def _handshake(self) -> None:
        self._send(
            {
                "jsonrpc": "2.0",
                "id": 0,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": {"name": "review_proposal", "version": "0"},
                },
            }
        )
        self._reply()
        self._send({"jsonrpc": "2.0", "method": "notifications/initialized"})

    def _send(self, message: dict) -> None:
        assert self.proc.stdin is not None
        self.proc.stdin.write(json.dumps(message) + "\n")
        self.proc.stdin.flush()

    def _reply(self) -> dict:
        assert self.proc.stdout is not None
        line = self.proc.stdout.readline()
        if not line:
            stderr = self.proc.stderr.read() if self.proc.stderr else ""
            die(f"the server closed before answering.\n{stderr.strip()[:600]}")
        return json.loads(line)

    def call(self, tool: str, arguments: dict) -> dict:
        """Call a tool and return its answer, raising on a tool-level error.

        The server answers with `structuredContent` when it has shape to give and prose
        when it does not; both come back here as a dict so callers do not care which.
        """
        self.counter += 1
        self._send(
            {
                "jsonrpc": "2.0",
                "id": self.counter,
                "method": "tools/call",
                "params": {"name": tool, "arguments": arguments},
            }
        )
        answer = self._reply()
        if "error" in answer:
            die(f"{tool}: {answer['error']}")
        result = answer.get("result", {})
        if result.get("isError"):
            text = result.get("content", [{}])[0].get("text", "")
            die(f"{tool}: {text}")
        return result.get("structuredContent") or {
            "text": result.get("content", [{}])[0].get("text", "")
        }

    def close(self) -> None:
        assert self.proc.stdin is not None
        self.proc.stdin.close()
        self.proc.wait(timeout=5)


def live_note(binary: str, vault: Path) -> tuple[str, bool]:
    """The note the window has open, and whether it holds unsaved edits.

    Asked over an unprofiled connection on purpose: the review profile is scoped to the
    note, and the note is not known until this answers. The live endpoints are read-only
    either way.
    """
    mcp = Mcp(binary, vault, None)
    try:
        answer = mcp.call("live_note", {})
    finally:
        mcp.close()
    path = answer.get("path")
    if not path:
        die(
            "the window answered with no note open. Open one, or pass --note; "
            "a proposal needs a note to be about."
        )
    return str(path), bool(answer.get("dirty"))


def write_profile(vault: Path, profile: str, note: str) -> Path:
    """Put the scratch profile in the vault's config, scoped to one note.

    Rewritten whole between the sentinels on every run, so the scope always matches the
    note being proposed against and never accumulates a wildcard from an earlier run.
    """
    config = vault / ".shale" / "config.toml"
    config.parent.mkdir(parents=True, exist_ok=True)
    block = (
        f"{SENTINEL_IN}\n"
        f"# Scoped to one note on purpose: a scratch profile that can write the whole\n"
        f"# vault would make this script the thing that bypasses the review path.\n"
        f'[mcp.profile."{profile}"]\n'
        f"read = [{json.dumps(note)}]\n"
        f"write = [{json.dumps(note)}]\n"
        f"review = true\n"
        f"{SENTINEL_OUT}"
    )
    text = config.read_text() if config.exists() else ""
    if SENTINEL_IN in text and SENTINEL_OUT in text:
        start = text.index(SENTINEL_IN)
        end = text.index(SENTINEL_OUT) + len(SENTINEL_OUT)
        text = text[:start] + block + text[end:]
    else:
        text = text.rstrip("\n") + ("\n\n" if text.strip() else "") + block + "\n"
    config.write_text(text)
    return config


def clean_profile(vault: Path) -> bool:
    """Delete the block this script owns. Returns whether there was one to delete."""
    config = vault / ".shale" / "config.toml"
    if not config.exists():
        return False
    text = config.read_text()
    if SENTINEL_IN not in text or SENTINEL_OUT not in text:
        return False
    start = text.index(SENTINEL_IN)
    end = text.index(SENTINEL_OUT) + len(SENTINEL_OUT)
    # Take the blank line that introduced the block with it, so cleaning leaves the file
    # the shape it had before rather than a run of empty lines.
    while start > 0 and text[start - 1] == "\n":
        start -= 1
    config.write_text(text[:start].rstrip("\n") + "\n" + text[end:].lstrip("\n"))
    return True


def note_of_pending(vault: Path, edit_id: str) -> str | None:
    """Which note a queued proposal is about, read from the queue file itself.

    Only needed to re-scope the scratch profile for a retract: the server lets a
    connection retract only what its own profile proposed, so the profile has to exist and
    cover the note before the call can succeed.
    """
    path = vault / ".shale" / "pending-edits" / f"{edit_id}.toml"
    if not path.exists():
        return None
    for line in path.read_text().splitlines():
        if line.startswith("note = "):
            return json.loads(line.split("=", 1)[1].strip())
    return None


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Queue an MCP review proposal against the note Shale has open."
    )
    parser.add_argument("--vault", type=Path, help="The vault. The one Shale has open.")
    parser.add_argument(
        "--note", help="Vault-relative note to propose against. Default: the open one."
    )
    parser.add_argument(
        "--text",
        default="Proposed by scripts/review_proposal.py so the review strip has something to show.",
        help="The sentence to propose. One line, appended at the end.",
    )
    parser.add_argument(
        "--profile",
        default="review-strip",
        help="The scratch profile to write and connect as.",
    )
    parser.add_argument(
        "--retract",
        metavar="ID",
        help="Take a queued proposal back instead of making one.",
    )
    parser.add_argument(
        "--clean",
        action="store_true",
        help="Delete the scratch profile from the config.",
    )
    parser.add_argument(
        "--binary", help="Path to shale-mcp. Default: this repo's build."
    )
    args = parser.parse_args()

    if not args.vault:
        die(
            "--vault is required: the lockfile that finds a running window lives inside "
            "the vault, so there is nothing to discover it from."
        )
    vault = args.vault.resolve()
    if not vault.is_dir():
        die(f"no such vault: {vault}")

    binary = find_binary(args.binary)
    if args.clean:
        removed = clean_profile(vault)
        print(
            f"{'removed' if removed else 'no'} scratch profile in {vault / '.shale' / 'config.toml'}"
        )
        if not args.retract:
            return

    if args.retract:
        # The server lets a connection retract only what its own profile proposed, so the
        # scratch profile has to be in the config and cover the note. It may already have
        # been cleaned away, so it is written back from the queue file — which is where the
        # note is named — rather than assumed.
        note = note_of_pending(vault, args.retract)
        if note is None:
            die(
                f"nothing is queued as {args.retract!r} in "
                f"{vault / '.shale' / 'pending-edits'}, so there is nothing to retract"
            )
        write_profile(vault, args.profile, note)
        mcp = Mcp(binary, vault, args.profile)
        try:
            print(
                json.dumps(mcp.call("retract_pending", {"id": args.retract}), indent=2)
            )
        finally:
            mcp.close()
        return

    if args.note:
        note, dirty = args.note, False
    else:
        note, dirty = live_note(binary, vault)
        print(f"the window has {note} open")
    if dirty:
        print(
            "note: that note is dirty in the window. The proposal is computed against the "
            "disk, so the hunk may read as stale once the buffer saves."
        )

    if not (vault / note).exists():
        die(f"{vault / note} is not on disk, so there is nothing to propose against.")

    config = write_profile(vault, args.profile, note)
    print(f"scratch profile in {config}: read and write {note!r}, review = true")

    mcp = Mcp(binary, vault, args.profile)
    try:
        current = mcp.call("read_note", {"path": note})
        body = current.get("text") or current.get("content") or ""
        if not isinstance(body, str):
            die(f"unexpected read_note shape: {current}")
        proposed = body.rstrip("\n") + f"\n\n{args.text}\n"
        queued = mcp.call("write_note", {"path": note, "content": proposed})
        print(json.dumps(queued, indent=2))
    finally:
        mcp.close()

    on_disk = (vault / note).read_text()
    if args.text in on_disk:
        die(
            f"the sentence is in {note} on disk, which means it was written rather than "
            "queued. Check that the profile really says review = true."
        )

    print(
        f"\nQueued against {note}. The strip should be up in the window; "
        f"`:review` gives it the keyboard."
    )
    print(
        f"Take it back with:  scripts/review_proposal.py --vault {args.vault} "
        f"--profile {args.profile} --retract {queued.get('id')}"
    )


if __name__ == "__main__":
    main()
