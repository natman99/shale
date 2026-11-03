//! Shale's identity outside the window: the name a desktop files it under, and
//! the mark it is recognised by.
//!
//! A window that names nothing is filed under whatever the window manager can
//! guess from its title, which for Shale means a generic document icon and a
//! taskbar entry that says `shale` or `main` depending on the toolkit. One
//! string fixes all of it: `app_id` is `WM_CLASS` on X11 and the xdg `app_id` on
//! Wayland, and a `.desktop` entry's `StartupWMClass` has to match it for the
//! desktop to find the entry -- and through it the icon and the name -- for a
//! running window (`design-language.md` §13).
//!
//! The mark is defined once, here, as bars on a 32-unit grid. The window icon
//! rasterises them; `assets/shale.svg` is the same four rectangles in a file,
//! and a test keeps the two from drifting apart.
//!
//! Only [`APP_ID`] is used on every platform; the mark itself is handed to a
//! toolkit on Linux alone (see [`window_icon`]), so the four items behind it are
//! allowed to look dead elsewhere rather than being compiled out: the tests that
//! keep the raster and the `.svg` in step are the same tests on every platform,
//! and a mark that only exists on one platform is a mark that can drift.

use shale_core::theme::Palette;

/// The name the window manager, the taskbar and the `.desktop` entry all know
/// Shale by. Lowercase and unspaced, because that is the shape both `WM_CLASS`
/// and the xdg `app_id` expect and the shape `assets/shale.desktop` repeats.
pub(crate) const APP_ID: &str = "shale";

/// The grid the mark is drawn on. `assets/shale.svg` uses the same numbers as
/// its `viewBox`, which is what lets a test compare the two.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) const MARK_GRID: u32 = 32;

/// One bedding line of the mark, in grid units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) struct Bar {
    /// Left edge.
    pub(crate) x: u32,
    /// Top edge.
    pub(crate) y: u32,
    /// Width.
    pub(crate) width: u32,
    /// Thickness. The whole point of the mark: strata are told apart by how
    /// thick a bed is, so no two of these are the same.
    pub(crate) height: u32,
}

/// The strata mark: four bedding lines of uneven thickness and uneven inset.
///
/// Simple on purpose -- it has to survive at 16 px in a taskbar and at 512 px
/// in an icon theme -- and simple enough that the wordmark rule §13 parks can
/// reuse it rather than redraw it.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) const MARK_BARS: [Bar; 4] = [
    Bar {
        x: 4,
        y: 6,
        width: 24,
        height: 4,
    },
    Bar {
        x: 7,
        y: 13,
        width: 18,
        height: 3,
    },
    Bar {
        x: 4,
        y: 19,
        width: 22,
        height: 5,
    },
    Bar {
        x: 10,
        y: 26,
        width: 12,
        height: 2,
    },
];

/// The mark's colour: Shale's own accent, from the palette it ships with rather
/// than one more hex literal. One colour, because a two-colour mark at 16 px is
/// mud.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn mark_colour() -> (u8, u8, u8) {
    let rgb = Palette::DARK.accent;
    (
        ((rgb >> 16) & 0xff) as u8,
        ((rgb >> 8) & 0xff) as u8,
        (rgb & 0xff) as u8,
    )
}

/// The mark rasterised for the window manager: the bars scaled to `size`, on a
/// transparent ground.
///
/// X11 only, because that is as far as gpui carries a window icon
/// (`_NET_WM_ICON`). A Wayland desktop gets the same mark from
/// `assets/shale.svg` through the `.desktop` entry, matched by [`APP_ID`].
#[cfg(target_os = "linux")]
pub(crate) fn window_icon(size: u32) -> std::sync::Arc<image::RgbaImage> {
    let (r, g, b) = mark_colour();
    let pixel = image::Rgba([r, g, b, 255]);
    let mut image = image::RgbaImage::new(size, size);
    let scale = size as f32 / MARK_GRID as f32;
    for bar in MARK_BARS {
        // Floored at the top-left and ceiled at the bottom-right, so a bar never
        // rounds away to nothing at a small size; `the_mark_survives_being_small`
        // is the check that the rounding does not make two of them overlap.
        let x0 = (bar.x as f32 * scale).floor() as u32;
        let x1 = ((bar.x + bar.width) as f32 * scale).ceil().min(size as f32) as u32;
        let y0 = (bar.y as f32 * scale).floor() as u32;
        let y1 = ((bar.y + bar.height) as f32 * scale)
            .ceil()
            .min(size as f32) as u32;
        for y in y0..y1 {
            for x in x0..x1 {
                image.put_pixel(x, y, pixel);
            }
        }
    }
    std::sync::Arc::new(image)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `.desktop` entry and the window have to agree on the name, or the
    /// desktop cannot match a running window to its icon and its label.
    #[test]
    fn the_desktop_entry_names_the_same_app() {
        let text = asset("shale.desktop");
        assert!(
            text.contains(&format!("StartupWMClass={APP_ID}")),
            "the entry has to carry the window's class"
        );
        assert!(text.contains("Icon=shale"), "and point at the mark");
        assert!(
            text.contains("Exec=shale"),
            "and run the binary this crate builds"
        );
    }

    /// The SVG and the raster are the same four bars in the same colour, or the
    /// mark in a taskbar and the mark in an icon theme are two different marks.
    #[test]
    fn the_svg_is_the_same_mark() {
        let text = asset("shale.svg");
        assert!(
            text.contains(&format!("viewBox=\"0 0 {MARK_GRID} {MARK_GRID}\"")),
            "the svg is drawn on the same grid"
        );
        let rects = svg_rects(&text);
        let bars: Vec<(u32, u32, u32, u32)> = MARK_BARS
            .iter()
            .map(|bar| (bar.x, bar.y, bar.width, bar.height))
            .collect();
        assert_eq!(rects, bars, "the svg's rects are the mark's bars");

        let (r, g, b) = mark_colour();
        assert!(
            text.contains(&format!("#{r:02x}{g:02x}{b:02x}")),
            "and the same colour"
        );
    }

    /// The mark has to stay four separate beds at every size a desktop asks
    /// for, which the scaling could easily undo.
    #[test]
    fn the_mark_survives_being_small() {
        for size in [16u32, 24, 32, 48, 128] {
            let scale = size as f32 / MARK_GRID as f32;
            let rows: Vec<(u32, u32)> = MARK_BARS
                .iter()
                .map(|bar| {
                    (
                        (bar.y as f32 * scale).floor() as u32,
                        ((bar.y + bar.height) as f32 * scale).ceil() as u32,
                    )
                })
                .collect();
            for (bar, (y0, y1)) in MARK_BARS.iter().zip(&rows) {
                assert!(y1 > y0, "the bed at y={} vanishes at {size}px", bar.y);
            }
            for (i, (y0, y1)) in rows.iter().enumerate() {
                for (other0, other1) in &rows[i + 1..] {
                    assert!(
                        y1 <= other0 || other1 <= y0,
                        "two beds share a row at {size}px"
                    );
                }
            }
        }
    }

    /// One of the files in the repo's `assets` folder, read from the crate's
    /// own build directory.
    fn asset(name: &str) -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets")
            .join(name);
        std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("{}: {err}", path.display()))
    }

    /// Pulls `x`, `y`, `width` and `height` out of each `<rect>` in the SVG.
    /// Hand-rolled rather than an XML dependency: the file is four lines.
    fn svg_rects(text: &str) -> Vec<(u32, u32, u32, u32)> {
        text.lines()
            .filter(|line| line.trim_start().starts_with("<rect"))
            .map(|line| {
                let number = |key: &str| -> u32 {
                    let found = line
                        .find(&format!("{key}=\""))
                        .unwrap_or_else(|| panic!("no {key} in {line}"));
                    let start = found + key.len() + 2;
                    let end = line[start..]
                        .find('"')
                        .unwrap_or_else(|| panic!("unterminated {key} in {line}"));
                    line[start..start + end].parse().expect("a number")
                };
                (number("x"), number("y"), number("width"), number("height"))
            })
            .collect()
    }
}
