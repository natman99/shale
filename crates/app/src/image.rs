//! Embedding images in a note: resolving an embed target to a file in the vault
//! and sizing the picture for the room the editor has.
//!
//! The buffer keeps the source line (`![[diagram.png|300]]`) exactly as the file
//! holds it, so the picture is only a paint decision — the same rule live
//! preview follows for every other piece of markdown syntax. A row the cursor is
//! on is drawn as its source so it stays editable, and a row that is nothing but
//! an embed is drawn as the image.

use std::path::PathBuf;
use std::sync::Arc;

use gpui::{px, Pixels, RenderImage};
use shale_core::vault::VaultEntry;
use shale_core::Vault;

/// The tallest an embedded image may be drawn, however tall the file is. A
/// screenshot taller than the window would push the rest of the note off screen.
pub(crate) const MAX_HEIGHT: f32 = 600.;

/// An image row's paint plan: which file to draw and what size the note asked
/// for. Resolved once per frame per row, before the picture itself is loaded.
#[derive(Clone, Debug)]
pub(crate) struct ImagePlan {
    /// Absolute path of the attachment file.
    pub(crate) path: PathBuf,
    /// A width in pixels from the embed's size suffix, if it had one.
    pub(crate) width: Option<f32>,
    /// A height in pixels, from the `WxH` forms only.
    pub(crate) height: Option<f32>,
}

/// An embedded image as it is painted: the decoded file and the rectangle it
/// fits in, which is also the height of the display line that carries it.
#[derive(Clone)]
pub(crate) struct ImagePaint {
    /// The decoded image, ready for gpui's painter.
    pub(crate) image: Arc<RenderImage>,
    /// Width in pixels.
    pub(crate) width: Pixels,
    /// Height in pixels.
    pub(crate) height: Pixels,
}

/// The image each buffer row embeds, indexed by row: `Some` for a row that is
/// nothing but an embed whose target resolves to a file in the vault.
///
/// Whether the row is *drawn* as an image is the caller's decision — live
/// preview keeps the source visible on the row the cursor is editing.
pub(crate) fn plans_by_row(
    text: &str,
    vault: &Vault,
    attachments: &[VaultEntry],
) -> Vec<Option<ImagePlan>> {
    text.split('\n')
        .map(|line| {
            shale_core::markdown::line_image_embed(line).and_then(|embed| {
                let entry = vault.resolve_attachment(&embed.target, attachments)?;
                Some(ImagePlan {
                    path: vault.root().join(&entry.path),
                    width: embed.width,
                    height: embed.height,
                })
            })
        })
        .collect()
}

/// The size to draw an image at: the one the note asked for, or the file's own,
/// scaled down to fit `available` pixels of width and [`MAX_HEIGHT`] of height.
/// An image is never enlarged, because a 4000-pixel screenshot would otherwise
/// fill the window on its own.
pub(crate) fn fit_size(
    natural_w: f32,
    natural_h: f32,
    plan: &ImagePlan,
    available: f32,
) -> (f32, f32) {
    // One dimension asked for means the other follows the file's ratio; both
    // means the note knows best.
    let (w, h) = match (plan.width, plan.height) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) if natural_w > 0. => (w, natural_h * (w / natural_w)),
        (None, Some(h)) if natural_h > 0. => (natural_w * (h / natural_h), h),
        _ => (natural_w, natural_h),
    };
    if w <= 0. || h <= 0. {
        return (0., 0.);
    }
    // A frame that has not been laid out yet has no width to fit into, so the
    // picture is drawn at its own size until the next one.
    let fit = if available > 0. {
        (available / w).min(1.)
    } else {
        1.
    }
    .min((MAX_HEIGHT / h).min(1.));
    (w * fit, h * fit)
}

/// [`fit_size`] for a decoded file: its pixel size divided by the window's scale
/// factor, which is what turns the file's own pixels into the pixels we draw it
/// with.
pub(crate) fn draw_size(
    image: &Arc<RenderImage>,
    scale: f32,
    available: Pixels,
    plan: &ImagePlan,
) -> ImagePaint {
    let size = image.size(0);
    let scale = scale.max(f32::EPSILON);
    let (w, h) = fit_size(
        size.width.0 as f32 / scale,
        size.height.0 as f32 / scale,
        plan,
        f32::from(available),
    );
    ImagePaint {
        image: Arc::clone(image),
        width: px(w),
        height: px(h),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plan with only the size suffix filled in, which is all the sizing cares
    /// about.
    fn plan(width: Option<f32>, height: Option<f32>) -> ImagePlan {
        ImagePlan {
            path: PathBuf::from("diagram.png"),
            width,
            height,
        }
    }

    #[test]
    fn an_image_fits_the_editor_without_growing() {
        // A wide screenshot is scaled down to the room there is, ratio kept.
        assert_eq!(
            fit_size(2000., 1000., &plan(None, None), 800.),
            (800., 400.)
        );
        // A small one is drawn at its own size: upscaling would only blur it.
        assert_eq!(fit_size(100., 80., &plan(None, None), 800.), (100., 80.));
        // A very tall one is capped by height instead of by width.
        let (w, h) = fit_size(400., 4000., &plan(None, None), 2000.);
        assert!(
            (w - 60.).abs() < 0.01 && (h - MAX_HEIGHT).abs() < 0.01,
            "width follows the height cap: {w} x {h}"
        );
        // Before the first layout there is no width to fit into, so only the
        // height cap applies.
        assert_eq!(fit_size(2000., 1000., &plan(None, None), 0.), (1200., 600.));
        // A file that reports no size draws nothing rather than dividing by zero.
        assert_eq!(fit_size(0., 0., &plan(None, None), 800.), (0., 0.));
    }

    #[test]
    fn a_size_suffix_sets_the_box_and_keeps_the_ratio() {
        assert_eq!(
            fit_size(640., 480., &plan(Some(320.), None), 2000.),
            (320., 240.)
        );
        assert_eq!(
            fit_size(640., 480., &plan(Some(100.), Some(50.)), 2000.),
            (100., 50.)
        );
        // The note may ask for more room than there is; the room still wins.
        assert_eq!(
            fit_size(640., 480., &plan(Some(1600.), None), 800.),
            (800., 600.)
        );
    }

    #[test]
    fn an_embed_resolves_to_a_file_in_the_vault() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        std::fs::create_dir_all(dir.path().join("assets"))?;
        std::fs::write(dir.path().join("assets/Diagram.png"), "not really a png")?;
        let vault = Vault::open(dir.path())?;
        let attachments = vault.list_attachments()?;

        let text = "intro\n![[diagram.png]]\n![[missing.png]]\ntext ![[diagram.png]]\n";
        let plans = plans_by_row(text, &vault, &attachments);
        assert_eq!(plans.len(), 5, "one entry per buffer row");
        assert!(plans[0].is_none(), "ordinary text");
        assert_eq!(
            plans[1].as_ref().unwrap().path,
            vault.root().join("assets/Diagram.png"),
            "a bare file name resolves anywhere in the vault"
        );
        assert!(plans[2].is_none(), "an embed that names no file stays text");
        assert!(
            plans[3].is_none(),
            "an embed with other text on its row stays text too"
        );
        Ok(())
    }
}
