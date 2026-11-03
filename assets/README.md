# Desktop files

Shale names its window (`app_id = "shale"`), and draws its own mark into the
window icon on X11. What it does **not** do is install anything: a `.desktop`
entry and an icon belong to the desktop, and installing them is a packaging
step, not something an editor should do to your home directory on first run.

These two files are the ones a package installs.

| File | Where it goes |
| --- | --- |
| `shale.desktop` | `~/.local/share/applications/shale.desktop` |
| `shale.svg` | `~/.local/share/icons/hicolor/scalable/apps/shale.svg` |

```sh
# Edit the Exec line first: Shale takes the vault to open, and only you know
# which one that is.
sed -i "s#/path/to/your/vault#$HOME/notes#" assets/shale.desktop

install -Dm644 assets/shale.desktop ~/.local/share/applications/shale.desktop
install -Dm644 assets/shale.svg ~/.local/share/icons/hicolor/scalable/apps/shale.svg
update-desktop-database ~/.local/share/applications  # if your desktop needs it
```

The name has to match on both sides. `shale.desktop`'s `StartupWMClass` is
`shale`, which is `identity::APP_ID` — the same string Shale passes to the
window manager as `WM_CLASS` (X11) and `app_id` (Wayland). Change one and the
desktop stops finding the icon for a running window; `the_desktop_entry_names_the_same_app`
fails when that happens.

The mark is defined twice on purpose and checked against itself once: the four
bars in `assets/shale.svg`, and the four bars `crates/app/src/identity.rs`
rasterises for the X11 window icon. `the_svg_is_the_same_mark` compares them.
