#!/usr/bin/env python3
"""Cross-check Shale's vim engine against real vim.

Reads the engine's case table (the same one `cargo test -p shale-core vim_table
-- --ignored` prints) and runs each case through `vim -es`, then diffs the two
results. Anything the engine does differently is printed as a row, with the
byte offsets of the first difference, so a deviation is either fixed or
deliberate and written down — never assumed.

    python3 scripts/vimcrosscheck.py

Requires vim on PATH. Exits non-zero when the two disagree.
"""

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WORK = Path.home() / ".hermes/cache/shale-vimcrosscheck"


def engine_rows():
    """Run the engine's own table and read back its answers."""
    out = subprocess.run(
        ["cargo", "test", "-p", "shale-core", "vim_table", "--", "--ignored", "--nocapture"],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    if out.returncode != 0:
        sys.exit(out.stderr[-2000:])
    rows = []
    for line in out.stdout.splitlines():
        parts = line.split("\t")
        if len(parts) != 4:
            continue
        rows.append((parts[0], int(parts[1]), parts[2], parts[3]))
    return rows


def decode(escaped):
    """The table prints Rust's `escape_debug` output."""
    return bytes(escaped, "utf-8").decode("unicode_escape").encode("latin-1").decode("utf-8")


def vim_answer(text, cursor, keys):
    """The same edit, done by vim: open the file, position the cursor, `normal!`."""
    WORK.mkdir(parents=True, exist_ok=True)
    src, dst = WORK / "in.txt", WORK / "out.txt"
    src.write_text(text)
    row = text[:cursor].count("\n") + 1
    col = cursor - text[:cursor].rfind("\n")  # 1-based column of the byte offset
    script = ["normal! gg", f"call cursor({row},{col})", f"normal! {keys}"]
    # -N: not compatible mode (compatible mode leaves numbered registers 1-9
    #     unused, so it is a different vim from the one users run).
    # -u NONE / -i NONE: no vimrc, no viminfo. Without these the user's own
    #     settings and previous session leak into the answer, which is how a
    #     cross-check starts lying without saying so.
    args = ["vim", "-es", "-N", "-u", "NONE", "-i", "NONE", str(src)]
    for line in script:
        args += ["-c", line]
    args += ["-c", f"w! {dst}", "-c", "q!"]
    subprocess.run(args, capture_output=True, text=True, timeout=30)
    got = dst.read_text()
    # vim adds the trailing newline a file was missing; the engine does not.
    if not text.endswith("\n") and got.endswith("\n"):
        got = got[:-1]
    return got


def first_diff(a, b):
    for i, (x, y) in enumerate(zip(a, b)):
        if x != y:
            return i
    return min(len(a), len(b))


def main():
    rows = engine_rows()
    bad = 0
    for text, cursor, keys, mine in rows:
        mine = decode(mine)
        theirs = vim_answer(decode(text), cursor, keys)
        if mine != theirs:
            bad += 1
            at = first_diff(mine, theirs)
            print(f"DIFF  {keys!r} on {text!r} @{cursor}")
            print(f"      vim    {theirs!r}")
            print(f"      shale  {mine!r}")
            print(f"      first difference at byte {at}")
    print(f"\n{len(rows) - bad}/{len(rows)} cases agree with vim")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
