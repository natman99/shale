#!/usr/bin/env python3
"""Headless repro: with shale already running on $DISPLAY, click a tree row."""
import time, os
from Xlib import X, display
from Xlib.ext.xtest import fake_input

def find_windows(root, out=None):
    if out is None: out = []
    tree = root.query_tree()
    for w in tree.children:
        try:
            name = w.get_wm_name()
            geom = w.get_geometry()
            out.append((w, name, geom))
        except Exception:
            pass
        find_windows(w, out)
    return out

def main():
    d = display.Display(os.environ.get("DISPLAY", ":99"))
    root = d.screen().root
    wins = find_windows(root)
    target = None
    for w, name, geom in wins:
        print(f"window: name={name!r} geom={geom.width}x{geom.height}+{geom.x}+{geom.y}")
        if name and "hale" in name:
            target = (w, geom)
    if target is None:
        for w, name, geom in wins:
            if geom.width * geom.height > 10000:
                target = (w, geom)
                break
    if target is None:
        print("no window found"); return 1
    w, geom = target
    abs_x, abs_y = geom.x, geom.y
    print(f"target window at {abs_x},{abs_y} size {geom.width}x{geom.height}")

    # Hover over the Welcome.md tree row (~x=100, y=60 inside the window),
    # then click. Two motion events: gpui needs the pointer position before
    # the button event for hit-testing.
    cx, cy = abs_x + 100, abs_y + 60
    fake_input(d, X.MotionNotify, x=cx, y=cy); d.sync()
    time.sleep(0.4)
    fake_input(d, X.MotionNotify, x=cx, y=cy); d.sync()
    time.sleep(0.4)
    fake_input(d, X.ButtonPress, 1, x=cx, y=cy); d.sync()
    time.sleep(0.15)
    fake_input(d, X.ButtonRelease, 1, x=cx, y=cy); d.sync()
    time.sleep(0.8)
    print("clicked at", cx, cy)
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
