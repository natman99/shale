use super::*;

#[test]
fn hex_colours_parse_with_or_without_the_hash() {
    assert_eq!(parse_color("#aabbcc").unwrap(), 0xaabbcc);
    assert_eq!(parse_color("aabbcc").unwrap(), 0xaabbcc);
    assert_eq!(parse_color(" #FFF000 ").unwrap(), 0xfff000);
    assert!(parse_color("red").is_err());
    assert!(parse_color("#abc").is_err());
    assert!(parse_color("#gggggg").is_err());
}

#[test]
fn a_theme_file_overrides_only_what_it_names() {
    let theme = ThemeFile::from_toml(
        "base = \"light\"\nbg = \"#282828\"\nfg = \"#ebdbb2\"\nlink = \"#83a598\"\n",
    )
    .unwrap();
    let base = Palette::built_in("light").unwrap();
    let out = base.apply(&theme);
    assert_eq!(out.bg, 0x282828);
    assert_eq!(out.fg, 0xebdbb2);
    assert_eq!(out.link, 0x83a598);
    assert_eq!(out.tag, base.tag, "everything else is inherited");
}

#[test]
fn a_theme_file_rejects_a_bad_key_or_colour() {
    assert!(ThemeFile::from_toml("base = \"light\"\nbak = \"#282828\"").is_err());
    assert!(ThemeFile::from_toml("bg = \"pink\"").is_err());
}

#[test]
fn theme_names_lists_the_vaults_own_files() {
    let dir = tempfile::tempdir().unwrap();
    // No theme folder at all is the common case, and not an error.
    assert_eq!(theme_names(dir.path()), Vec::<String>::new());
    let themes = dir.path().join(THEME_DIR);
    std::fs::create_dir_all(&themes).unwrap();
    std::fs::write(themes.join("mine.toml"), "base = \"dark\"\n").unwrap();
    std::fs::write(themes.join("also.toml"), "base = \"light\"\n").unwrap();
    // A file that is not a theme is not a theme name.
    std::fs::write(themes.join("notes.md"), "not a theme\n").unwrap();
    assert_eq!(theme_names(dir.path()), ["also", "mine"]);
}

#[test]
fn palette_for_resolves_built_ins_and_files() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(palette_for(dir.path(), "dark").unwrap(), Palette::DARK);
    assert_eq!(palette_for(dir.path(), "Light").unwrap(), Palette::LIGHT);
    // No name at all is the default palette, which is a flavour rather than
    // Shale's own dark value.
    assert_eq!(palette_for(dir.path(), "").unwrap(), Palette::DEFAULT);
    assert_eq!(Palette::DEFAULT, catppuccin::MOCHA);
    assert_eq!(
        Palette::built_in(Palette::DEFAULT_NAME),
        Some(Palette::DEFAULT)
    );
    // An unknown name with no file is an error the caller can report.
    assert!(palette_for(dir.path(), "gruvbox").is_err());

    let path = ThemeFile::path(dir.path(), "gruvbox");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "base = \"dark\"\nbg = \"#282828\"\n").unwrap();
    let out = palette_for(dir.path(), "gruvbox").unwrap();
    assert_eq!(out.bg, 0x282828);
    assert_eq!(out.fg, Palette::DARK.fg);

    std::fs::write(&path, "base = \"nonsense\"\n").unwrap();
    assert!(palette_for(dir.path(), "gruvbox").is_err());
}

/// WCAG relative luminance: the sRGB channels linearized and combined with
/// the Rec. 709 weights. The channels have to be linearized first; weighting
/// the raw 0-1 values (the obvious shortcut) under-reports mid-tone contrast
/// badly enough to fail a palette that is perfectly readable — Catppuccin
/// Latte's text on its own background is 7:1, which the shortcut calls 2.7:1.
/// This is still only here to catch a theme whose text matches its
/// background, not to certify a palette.
fn luminance(rgb: u32) -> f32 {
    let channel = |byte: u32| {
        let c = byte as f32 / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel((rgb >> 16) & 0xff)
        + 0.7152 * channel((rgb >> 8) & 0xff)
        + 0.0722 * channel(rgb & 0xff)
}

/// The WCAG contrast ratio of two colours: 1 is identical, 21 is black on
/// white, and 4.5 is the usual floor for body text.
fn contrast(a: u32, b: u32) -> f32 {
    let x = luminance(a).max(luminance(b));
    let y = luminance(a).min(luminance(b));
    (x + 0.05) / (y + 0.05)
}

/// The built-in palettes have to be usable in their own right: a theme whose
/// text matches its background is not a theme.
///
/// Every role is judged against the surface it is actually drawn on, which
/// is the note's background for most of them, the code block's for the
/// `code_*` tokens, and the window's for the `ui_*` ones. The thresholds are
/// the WCAG body-text floor for the roles a reader has to parse (note text,
/// headings, links, tags, code) and a lower bar for the roles that are meant
/// to recede (syntax markers, unresolved links, quoted text, comments) or
/// only to be *distinguishable* rather than read (a rule, a tab's edge, a
/// graph's edges). Roles that are not painted yet are still checked, so a
/// reserved colour cannot quietly be a useless one.
#[test]
fn the_built_in_palettes_are_readable() {
    // (role, the role it is drawn over, floor)
    let roles = |p: &Palette| -> Vec<(&str, u32, &str, u32, f32)> {
        vec![
            ("note text", p.fg, "the note background", p.bg, 4.5),
            ("heading text", p.heading, "the note background", p.bg, 4.5),
            ("a resolved link", p.link, "the note background", p.bg, 3.0),
            ("a tag", p.tag, "the note background", p.bg, 3.0),
            ("quoted text", p.quote, "the note background", p.bg, 2.5),
            ("syntax markers", p.dim, "the note background", p.bg, 2.0),
            (
                "an unresolved link",
                p.link_unresolved,
                "the note background",
                p.bg,
                2.0,
            ),
            (
                "the checker's squiggle",
                p.spell,
                "the note background",
                p.bg,
                2.0,
            ),
            (
                "ordinary code",
                p.code_text,
                "the code block",
                p.code_block_bg,
                4.0,
            ),
            (
                "a code comment",
                p.code_comment,
                "the code block",
                p.code_block_bg,
                2.0,
            ),
            // A token colour only has to be distinguishable from the code
            // around it: `code_text` carries the body-text floor, and every
            // family's own syntax theme is the authority on its tokens.
            (
                "a keyword",
                p.code_keyword,
                "the code block",
                p.code_block_bg,
                2.0,
            ),
            (
                "a string",
                p.code_string,
                "the code block",
                p.code_block_bg,
                2.0,
            ),
            (
                "a number",
                p.code_number,
                "the code block",
                p.code_block_bg,
                2.0,
            ),
            (
                "a function name",
                p.code_function,
                "the code block",
                p.code_block_bg,
                2.0,
            ),
            (
                "a type name",
                p.code_type,
                "the code block",
                p.code_block_bg,
                2.0,
            ),
            (
                "a constant",
                p.code_const,
                "the code block",
                p.code_block_bg,
                2.0,
            ),
            (
                "an operator",
                p.code_operator,
                "the code block",
                p.code_block_bg,
                2.0,
            ),
            ("window text", p.ui_fg, "the window", p.ui_bg, 4.5),
            ("a hint line", p.ui_dim, "the window", p.ui_bg, 2.0),
            (
                "a focused rule",
                p.ui_active_border,
                "the window",
                p.ui_bg,
                1.2,
            ),
            ("a tab", p.ui_tab_bg, "the window", p.ui_bg, 1.1),
            (
                "the open tab",
                p.ui_tab_active_bg,
                "the window",
                p.ui_bg,
                1.25,
            ),
            ("the panel", p.popup_bg, "the note", p.bg, 1.1),
            (
                "the panel's rule",
                p.popup_border,
                "the panel",
                p.popup_bg,
                1.2,
            ),
            ("a graph node", p.graph_node, "the note", p.bg, 1.5),
            (
                "the focused node",
                p.graph_node_active,
                "the note",
                p.bg,
                2.0,
            ),
            ("a graph edge", p.graph_edge, "the note", p.bg, 1.1),
            ("a node label", p.graph_text, "the note", p.bg, 2.5),
        ]
    };
    // A background role and the text role drawn on it, with the floor that
    // pair has to clear. A highlighted row gets a lower floor than body
    // text: it is transient, its text is short, and it has to be *visible*
    // against the surface it sits on, which for a palette whose text is a
    // mid-tone pulls the other way. Solarized's own tone table rates body
    // text on its secondary background as less than optimal reading, which
    // is why the cursor's line is held to 4.0 rather than 4.5.
    let pairs = |p: &Palette| -> Vec<(&str, u32, &str, u32, f32)> {
        vec![
            (
                "the status bar's text",
                p.ui_fg,
                "the status bar",
                p.ui_status_bg,
                4.5,
            ),
            (
                "what you type",
                p.ui_fg,
                "the prompt field",
                p.ui_field_bg,
                4.5,
            ),
            (
                "the open tab's label",
                p.ui_fg,
                "the open tab",
                p.ui_tab_active_bg,
                3.0,
            ),
            ("the panel's text", p.ui_fg, "the panel", p.popup_bg, 4.5),
            (
                "a panel row's text",
                p.ui_fg,
                "the highlighted row",
                p.popup_hover_bg,
                3.5,
            ),
            (
                "text on the cursor's line",
                p.fg,
                "the cursor's line",
                p.cursor_line_bg,
                4.0,
            ),
        ]
    };
    for (name, palette) in Palette::BUILT_INS {
        for (role, colour, on, against, floor) in roles(palette) {
            assert!(
                contrast(against, colour) > floor,
                "{name}: {role} is too faint on {on}",
            );
        }
        // The cursor's line is a band, not a highlight: it has to be
        // visible without competing with the text on it.
        assert!(
            contrast(palette.bg, palette.cursor_line_bg) > 1.05,
            "{name}: the cursor's line is indistinguishable from the note",
        );
        // A picker's rows are drawn on the window's background, and the
        // highlighted one on `ui_active_bg`. Matched characters are short,
        // so they get the floor for short text on an ordinary row and a
        // lower one on the highlighted row, where the rest of the row is
        // still body text at full contrast.
        assert!(
            contrast(palette.ui_bg, palette.match_text) > 3.0,
            "{name}: a query's matched characters are too faint on a picker row",
        );
        assert!(
            contrast(palette.ui_active_bg, palette.match_text) > 2.5,
            "{name}: a query's matched characters are too faint on the chosen row",
        );
        for (what, colour, on, against, floor) in pairs(palette) {
            assert!(
                contrast(against, colour) > floor,
                "{name}: {what} is too faint on {on}",
            );
        }
    }
    assert_ne!(Palette::DARK.bg, Palette::LIGHT.bg);
    // Every callout role reads off the palette it was given, so a theme can
    // retune one type without touching the others.
    let dark = Palette::DARK;
    assert_eq!(
        dark.callout_accent(CalloutKind::Warning),
        dark.callout_warning
    );
    assert_eq!(dark.callout_accent(CalloutKind::Quote), dark.quote);
    assert_eq!(dark.callout_accent(CalloutKind::Other), dark.accent);
}

/// The Catppuccin flavours are selectable by name, by their `catppuccin-`
/// prefixed alias, and as the `base` of a theme file, so a Catppuccin user
/// can pick a flavour or tune one without leaving the palette they know.
#[test]
fn the_catppuccin_flavours_are_built_ins() {
    let dir = tempfile::tempdir().unwrap();
    for (name, palette) in catppuccin::FLAVORS {
        assert_eq!(Palette::built_in(name), Some(*palette));
        assert_eq!(
            Palette::built_in(&format!("catppuccin-{name}")),
            Some(*palette)
        );
        // Case does not matter, and the alias works through the loader too.
        let upper = name.to_uppercase();
        assert_eq!(palette_for(dir.path(), &upper).unwrap(), *palette);
        assert_eq!(
            palette_for(dir.path(), &format!("Catppuccin-{upper}")).unwrap(),
            *palette
        );
    }
    // The flavours are the four upstream ones, lightest first, and they are
    // distinct palettes rather than one palette copied four times.
    assert_eq!(catppuccin::FLAVORS.len(), 4);
    assert_eq!(catppuccin::FLAVORS[0].0, "latte");
    assert_eq!(catppuccin::FLAVORS[3].0, "mocha");
    assert_eq!(catppuccin::MOCHA.bg, 0x1e1e2e);
    assert_eq!(catppuccin::LATTE.bg, 0xeff1f5);

    // A flavour is a legal `base`, so a theme file can be a small edit to it.
    let path = ThemeFile::path(dir.path(), "mocha-warm");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "base = \"mocha\"\nbg = \"#1e1e28\"\n").unwrap();
    let out = palette_for(dir.path(), "mocha-warm").unwrap();
    assert_eq!(out.bg, 0x1e1e28);
    assert_eq!(out.link, catppuccin::MOCHA.link, "the rest is Mocha");
}

/// The other theme families are selectable by name and as the `base` of a
/// theme file, in the spellings their own users type: mixed case, and for a
/// family whose dark variant carries the bare name, a `-dark` suffix.
#[test]
fn the_other_families_are_built_ins() {
    let named: &[(&str, Palette)] = &[
        ("tokyo-night", tokyo_night::NIGHT),
        ("tokyo-night-storm", tokyo_night::STORM),
        ("tokyo-night-light", tokyo_night::LIGHT),
        ("gruvbox-dark", gruvbox::DARK),
        ("gruvbox-light", gruvbox::LIGHT),
        ("nord", nord::PALETTE),
        ("dracula", dracula::PALETTE),
        ("solarized-dark", solarized::DARK),
        ("solarized-light", solarized::LIGHT),
        ("one-dark", one::DARK),
        ("one-light", one::LIGHT),
        ("rose-pine", rose_pine::MAIN),
        ("rose-pine-moon", rose_pine::MOON),
        ("rose-pine-dawn", rose_pine::DAWN),
    ];
    let dir = tempfile::tempdir().unwrap();
    let path = ThemeFile::path(dir.path(), "mine");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    for (name, palette) in named {
        assert_eq!(Palette::built_in(name), Some(*palette), "{name}");
        assert_eq!(
            palette_for(dir.path(), &name.to_uppercase()).unwrap(),
            *palette,
            "{name} is found whatever case it is written in"
        );
        // Any of them can be the base a vault's own theme file edits.
        std::fs::write(&path, format!("base = \"{name}\"\nbg = \"#010203\"\n")).unwrap();
        let out = palette_for(dir.path(), "mine").unwrap();
        assert_eq!(out.bg, 0x010203, "{name} as a theme file's base");
        assert_eq!(out.accent, palette.accent, "the rest of {name} is kept");
    }

    // The two spellings people reach for that are not themselves names.
    assert_eq!(
        Palette::built_in("Tokyo-Night-Dark"),
        Some(tokyo_night::NIGHT)
    );
    assert_eq!(Palette::built_in("rose-pine-dark"), Some(rose_pine::MAIN));
    assert_eq!(
        Palette::built_in("solarized-dark"),
        Some(solarized::DARK),
        "a family that spells its own variants -dark keeps that name"
    );
    // A family name on its own would have to guess between its variants.
    assert_eq!(Palette::built_in("gruvbox"), None);
    assert_eq!(Palette::built_in("one"), None);
}

/// The registry the picker will list has to be a set: no two entries sharing
/// a name, and no two entries that are the same colours under different names.
#[test]
fn the_built_in_list_is_a_set() {
    for (i, (name, palette)) in Palette::BUILT_INS.iter().enumerate() {
        for (other, other_palette) in &Palette::BUILT_INS[i + 1..] {
            assert!(!name.eq_ignore_ascii_case(other), "{name} is listed twice");
            assert_ne!(*palette, *other_palette, "{name} and {other} are identical");
        }
    }
    // A name is a file stem, so nothing in the list may need quoting.
    for (name, _) in Palette::BUILT_INS {
        assert!(
            name.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
            "{name} is not a plain lowercase file stem"
        );
    }
}
