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

/// `base` may name another theme file, not only a built-in (§15), so a vault can
/// layer a tweak on a tweak.
#[test]
fn a_theme_file_may_build_on_another_theme_file() {
    let dir = tempfile::tempdir().unwrap();
    let themes = dir.path().join(THEME_DIR);
    std::fs::create_dir_all(&themes).unwrap();
    std::fs::write(
        themes.join("warm.toml"),
        "base = \"mocha\"\nbg = \"#1e1e1e\"\nfg = \"#eeeeee\"\n",
    )
    .unwrap();
    std::fs::write(
        themes.join("warmer.toml"),
        "base = \"warm\"\nbg = \"#202020\"\nlink = \"#ff0000\"\n",
    )
    .unwrap();
    let out = palette_for(dir.path(), "warmer").unwrap();
    assert_eq!(out.bg, 0x202020, "the outer file wins");
    assert_eq!(out.fg, 0xeeeeee, "the middle file still carries");
    assert_eq!(out.link, 0xff0000);
    assert_eq!(out.tag, catppuccin::MOCHA.tag, "down to the built-in");
}

/// An inheritance ring is an error that names the ring, rather than a stack
/// overflow or a bare "circular base".
#[test]
fn an_inheritance_ring_names_the_ring() {
    let dir = tempfile::tempdir().unwrap();
    let themes = dir.path().join(THEME_DIR);
    std::fs::create_dir_all(&themes).unwrap();
    std::fs::write(themes.join("a.toml"), "base = \"b\"\n").unwrap();
    std::fs::write(themes.join("b.toml"), "base = \"c\"\n").unwrap();
    std::fs::write(themes.join("c.toml"), "base = \"a\"\n").unwrap();
    let err = format!("{:#}", palette_for(dir.path(), "a").unwrap_err());
    assert!(
        err.contains("a.toml -> b.toml -> c.toml -> a.toml"),
        "the message spells out the ring, got {err:?}"
    );
    // A file that inherits from itself is the one-file version of the same bug.
    std::fs::write(themes.join("d.toml"), "base = \"d\"\n").unwrap();
    let err = format!("{:#}", palette_for(dir.path(), "d").unwrap_err());
    assert!(
        err.contains("d.toml -> d.toml"),
        "a one-file ring is named too, got {err:?}"
    );
}

/// A palette says whether it is dark or light, which is what follow-the-system
/// picks a built-in with. The threshold is where black and white text are
/// equally readable, so a colour on either side of it is on the side its own
/// text would choose.
#[test]
fn a_palette_is_dark_or_light_by_its_background() {
    assert_eq!(Palette::DEFAULT.mode(), Mode::Dark);
    assert_eq!(Palette::built_in("latte").unwrap().mode(), Mode::Light);
    assert_eq!(Palette::built_in("light").unwrap().mode(), Mode::Light);
    assert_eq!(
        Palette::built_in("tokyo-night-light").unwrap().mode(),
        Mode::Light
    );
    // Every built-in agrees with its own name, which is the check that the
    // threshold is not off by a family.
    for (name, palette) in Palette::BUILT_INS {
        let name = *name;
        let says_dark = name.contains("dark")
            || ["mocha", "frappe", "macchiato", "nord", "dracula"].contains(&name);
        let says_light = name.contains("light") || name == "latte" || name == "rose-pine-dawn";
        if says_dark {
            assert_eq!(palette.mode(), Mode::Dark, "{name} should read dark");
        } else if says_light {
            assert_eq!(palette.mode(), Mode::Light, "{name} should read light");
        }
    }
    // A theme file may say otherwise, and the spelling is checked.
    let theme = ThemeFile::from_toml("mode = \"light\"\n").unwrap();
    assert_eq!(theme.mode, Some(Mode::Light));
    assert!(ThemeFile::from_toml("mode = \"pink\"\n").is_err());
}

/// `:theme export` writes the palette that is on screen as a file the loader
/// reads back, so tweaking a theme starts from real colours (§15).
#[test]
fn an_exported_palette_reads_back_as_itself() {
    let palette = Palette::built_in("gruvbox-dark").unwrap();
    let text = palette.to_theme_toml().unwrap();
    // Colours are hex strings, not the integers `Palette` holds, or the file
    // would not parse.
    assert!(
        text.contains("bg = \"#"),
        "exported colours are hex strings: {text}"
    );
    assert!(!text.contains("bg = 0x"), "no bare integers: {text}");
    let file = ThemeFile::from_toml(&text).unwrap();
    assert_eq!(file.mode, Some(Mode::Dark));
    // No `base`, so applying it to anything starts from the file's own colours:
    // the export is a whole palette, not a diff.
    let out = Palette::DARK.apply(&file);
    assert_eq!(out, palette, "the export round-trips");
    // And it round-trips through the vault, which is how `:theme export` is used.
    let dir = tempfile::tempdir().unwrap();
    let path = ThemeFile::path(dir.path(), "on-screen");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, &text).unwrap();
    assert_eq!(palette_for(dir.path(), "on-screen").unwrap(), palette);
}

/// The built-in palettes have to be usable in their own right: a theme whose
/// text matches its background is not a theme.
///
/// The rules, the surfaces each role is judged against and the floors it has
/// to clear live in [`contrast::audit`] rather than here, because the same
/// judgement runs over a vault's own theme file when it loads -- see
/// [`the_auditor_flags_a_theme_file_that_reads_badly`]. One table, judged
/// twice.
#[test]
fn the_built_in_palettes_are_readable() {
    for (name, palette) in Palette::BUILT_INS {
        let flags = contrast::audit(palette);
        assert!(
            flags.is_empty(),
            "{name}: {}",
            flags
                .iter()
                .map(|flag| flag.to_string())
                .collect::<Vec<_>>()
                .join("; ")
        );
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

/// The auditor is what the app runs against a vault's own theme file, so it
/// has to answer for a palette a user wrote and not only for the built-ins: a
/// colour copied out of a dark palette into a light one, or two graph node
/// kinds given one hue, is reported rather than silently painted.
#[test]
fn the_auditor_flags_a_theme_file_that_reads_badly() {
    let dir = tempfile::tempdir().unwrap();
    let path = ThemeFile::path(dir.path(), "mine");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        "base = \"dark\"\nfg = \"#1e1e1e\"\ngraph_tag = \"#7aa6f0\"\n",
    )
    .unwrap();
    let palette = palette_for(dir.path(), "mine").unwrap();
    let said = contrast::audit(&palette)
        .iter()
        .map(|flag| flag.to_string())
        .collect::<Vec<_>>();
    assert!(
        said.iter().any(|s| s.starts_with("note text is too faint")),
        "the note's text went unreported: {said:?}"
    );
    assert!(
        said.iter()
            .any(|s| s.starts_with("a note node and a tag node")),
        "the two node kinds went unreported: {said:?}"
    );
    // And a palette that reads is reported as reading.
    assert!(contrast::audit(&Palette::DARK).is_empty());
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
