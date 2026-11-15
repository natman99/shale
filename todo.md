Like Obsidian, custom themes should live per vault in .shale/

For the note outline-- Already planned I think. Make sure the user can reorder headings from the file outline. It's a powerful feature for moving things around quickly without risking losing things by copy pasting.

Contrast is kind of low on labels in 3d, it gets really hard to read them with all of the links and nodes all around. Can we fix that somehow? Make them bigger or brighter or whatever? They also seem to be slightly transparent and blend into nodes really well. They look okay in 2d. Do they follow the config font size?

make `:e` take file paths

Docs mistake: says `space` is vim's move right motion when it is only used for chords.

Update docs about the DB being safely removable once it's storing more valuable information like timeline

Do a cargo deny pass later

Commands to insert callouts and tables at the cursor and props at the note head.

Test multiple search operators again manually

~~Update project to Rust 2024~~ — done in `8c00a9f`: `edition = "2024"` across the workspace, with the clippy fixer's let-chains swept in the same commit.

Add rendering for images in the vault as a note. We already have image preview inside of notes but we can't see them in the file tree and can't view them outside of an embed.

~~Make arrow keys move the cursor.~~ — done in `2b1ce63`: the arrows are the same motion as `hjkl` (display rows, extend a visual selection, scroll at the edge), and `shift`+arrow / `ctrl-d` / `ctrl-u` page through it.

What is `ctrl-t` even bound to? It duplicates the current note in a new tab.

Do a massive grammar and text overhaul on like everything for consistency and sense.

Mouse drag text select.

Right sidebar missing focus indicator on empty window (for example if there are now headings or links the sidebar is dead input and shows no indicator it's focused). The left sidebar always has at least one item to select.

~~Add a little clickable button in the top left that will close/open the file tree and show if it's open or not.~~ — done in `21a3812` and `64f73fa`: one mark per panel, in the card's corner while it is open and at the end of the tab bar while it is away, hollow closed / filled open, with a tooltip. The half still open is the "only the leftmost pane answers" bug below.

Merge "blank tab" and "new unnamed note"?

Important: way to move notes to folders
clicking focuses panes but doesn't move the focus border
When outline scrolls put the target in the middle or top of the screen instead of the bottom row

crash on windows. I believe it happened when waking from sleep. 
thread 'main' (12572) panicked at C:\Users\nsimm\.cargo\git\checkouts\zed-a70e2ad075855582\8dd255c\crates\gpui\src\app.rs:3325:14:
attempted to read a window that is already on the stack
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

The "show sidebar" buttons only work on the leftmost (or original/primary) pane. Buttons show up on all panes. Tooltips also broken on non-primary panes.

Insert table/callout commands

Soft wrap cutoff is cut off by the border/pane effects on the right side. It looks like about one char is covered up.

Wrap boundries aren't updated/redraw the editor when the sidebars open/close causing a visible reflow upon next input and redraw.
