#!/usr/bin/env python3
"""Capture the shale window on Xvfb to a PNG."""
import time, os, sys
from Xlib import X, display
from Xlib.ext.xtest import fake_input
from PIL import Image

def find_target(d, root):
    def walk(w, out):
        for c in w.query_tree().children:
            try:
                out.append((c, c.get_wm_name(), c.get_geometry()))
            except Exception:
                pass
            walk(c, out)
        return out
    wins = walk(root, [])
    for w, name, geom in wins:
        if name and "hale" in name:
            return w, geom
    for w, name, geom in wins:
        if geom.width * geom.height > 100000:
            return w, geom
    return None, None

def main():
    d = display.Display(os.environ.get("DISPLAY", ":99"))
    root = d.screen().root
    w, geom = find_target(d, root)
    if w is None:
        print("no window"); return 1
    print("window", geom.width, geom.height, geom.x, geom.y)
    img = w.get_image(0, 0, geom.width, geom.height, X.ZPixmap, 0xffffffff)
    im = Image.frombytes("RGBX", (geom.width, geom.height), img.data, "raw", "BGRX").convert("RGB")
    out = sys.argv[1] if len(sys.argv) > 1 else "shot.png"
    im.save(out)
    print("saved", out)
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
