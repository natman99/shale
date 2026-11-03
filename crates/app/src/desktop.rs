//! What the desktop says about dark and light.
//!
//! A theme is normally a name in `.shale/config.toml`. When the vault names
//! none, the desktop's own preference decides which built-in to open in
//! (`design-language.md` §15), and this module is the only place that asks.
//!
//! The answer is `Option` on purpose. "The desktop has no opinion" and "the
//! desktop prefers light" are different facts, and §15 wants the first one to
//! land on dark while the second lands on a light palette. A platform that
//! cannot tell the two apart -- gpui's Linux backend, whose window appearance
//! starts at `Light` and is only corrected once the portal answers -- is not
//! usable as the answer, which is why Linux asks the portal directly.

use gpui::WindowAppearance;
use shale_core::theme::Mode;

/// The built-in that matches the desktop's preference, or `None` when the
/// desktop has no answer to give.
///
/// `appearance` is what gpui already believes about the window. It is the whole
/// answer on a platform that reads its setting synchronously when the window is
/// built (Windows: `AppsUseLightTheme`), and no answer at all on Linux, where
/// the value is a placeholder until the portal replies.
pub(crate) async fn theme_name(appearance: WindowAppearance) -> Option<&'static str> {
    #[cfg(target_os = "linux")]
    {
        // gpui's own value is not usable here: it is `Light` until the portal
        // answers, so trusting it would paint a light palette on a dark desktop
        // for the first moment of the window's life. Ask the portal, and if it
        // is not there, say nothing and let the caller keep its default.
        let _ = appearance;
        portal_mode().await.map(built_in_for)
    }
    #[cfg(not(target_os = "linux"))]
    {
        Some(built_in_for(match appearance {
            WindowAppearance::Light | WindowAppearance::VibrantLight => Mode::Light,
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Mode::Dark,
        }))
    }
}

/// The pair §15 maps a desktop preference onto: today's default for dark, and
/// the same family's light flavour for light. The doc author's extension, and
/// the one line to change if the pair is ever wrong.
fn built_in_for(mode: Mode) -> &'static str {
    match mode {
        Mode::Dark => "mocha",
        Mode::Light => "latte",
    }
}

/// The XDG Desktop Portal's `color-scheme`: `org.freedesktop.portal.Settings`
/// reading the `org.freedesktop.appearance` namespace.
///
/// `ashpd` is the portal client gpui's Linux backend already links, so this
/// names a crate the build already has rather than adding one. `NoPreference`
/// is the desktop answering that it has no preference, which is the same answer
/// as not answering: `None`, and the caller keeps dark.
#[cfg(target_os = "linux")]
async fn portal_mode() -> Option<Mode> {
    use ashpd::desktop::settings::{ColorScheme, Settings};

    let settings = Settings::new().await.ok()?;
    match settings.color_scheme().await.ok()? {
        ColorScheme::PreferDark => Some(Mode::Dark),
        ColorScheme::PreferLight => Some(Mode::Light),
        ColorScheme::NoPreference => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shale_core::theme::Palette;

    /// The pair §15 settles on: the default dark palette, and the same family's
    /// light flavour. Both names have to resolve, or the follow silently does
    /// nothing on a light desktop.
    #[test]
    fn the_desktop_answer_names_two_built_ins() {
        assert_eq!(built_in_for(Mode::Dark), "mocha");
        assert_eq!(built_in_for(Mode::Light), "latte");
        for name in [built_in_for(Mode::Dark), built_in_for(Mode::Light)] {
            assert!(
                Palette::built_in(name).is_some(),
                "{name} is not a built-in palette"
            );
        }
        assert_eq!(Palette::built_in("mocha").unwrap().mode(), Mode::Dark);
        assert_eq!(Palette::built_in("latte").unwrap().mode(), Mode::Light);
    }
}
