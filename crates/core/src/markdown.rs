/// A `[[wiki-link]]` occurrence in markdown source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikiLink {
    /// The link target, without alias or heading anchor.
    pub target: String,
    /// Display text when the link uses the `[[target|alias]]` form.
    pub alias: Option<String>,
    /// Whether the link is an embed (`![[...]]`).
    pub embed: bool,
    /// Byte offset of the start of the whole construct (including `!` for
    /// embeds).
    pub start: usize,
    /// Byte offset just past the end of the whole construct.
    pub end: usize,
    /// Byte range of the note name inside the construct: the last path component
    /// of the target, with no folder prefix, no `#heading` anchor, no `|alias`
    /// and no padding spaces. That is the only part a rename may rewrite — the
    /// rest of the link is how the author chose to write it. Empty when the link
    /// has no name at all, as `[[#a heading in this note]]` does not.
    pub stem_start: usize,
    /// Byte offset just past [`Self::stem_start`]'s name.
    pub stem_end: usize,
}

/// Extract wiki-links from markdown text.
///
/// Links inside fenced code blocks and inline code spans are ignored,
/// matching Obsidian's behavior. The scanner is line-based: a wiki-link
/// never spans lines.
pub fn extract_wikilinks(text: &str) -> Vec<WikiLink> {
    let mut links = Vec::new();
    // Active fence: (fence character, fence run length).
    let mut fence: Option<(u8, usize)> = None;

    let mut line_start = 0usize;
    for line in text.split_inclusive('\n') {
        let content = line.trim_end_matches(['\n', '\r']);
        let trimmed = content.trim_start();

        if let Some((fc, flen)) = fence {
            if closes_fence(trimmed.as_bytes(), fc, flen) {
                fence = None;
            }
            line_start += line.len();
            continue;
        }
        if let Some(opened) = opens_fence(trimmed.as_bytes()) {
            fence = Some(opened);
            line_start += line.len();
            continue;
        }

        scan_line(line_start, content, &mut links);
        line_start += line.len();
    }
    links
}

/// A fence opens a code block when the line starts with 3+ backticks or
/// tildes and (for backticks) the info string contains no backticks.
fn opens_fence(trimmed: &[u8]) -> Option<(u8, usize)> {
    let fc = *trimmed.first()?;
    if fc != b'`' && fc != b'~' {
        return None;
    }
    let run = trimmed.iter().take_while(|&&b| b == fc).count();
    if run < 3 {
        return None;
    }
    if fc == b'`' && trimmed[run..].contains(&b'`') {
        return None;
    }
    Some((fc, run))
}

fn closes_fence(trimmed: &[u8], fc: u8, flen: usize) -> bool {
    let run = trimmed.iter().take_while(|&&b| b == fc).count();
    run >= flen && trimmed[run..].iter().all(|b| b.is_ascii_whitespace())
}

/// Scan one line of non-code text for `[[...]]` constructs.
fn scan_line(line_start: usize, line: &str, out: &mut Vec<WikiLink>) {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Skip inline code spans: a run of N backticks ends at the next
        // run of exactly N backticks.
        if bytes[i] == b'`' {
            let run = bytes[i..].iter().take_while(|&&b| b == b'`').count();
            let mut j = i + run;
            let mut closed = false;
            while j < bytes.len() {
                if bytes[j] == b'`' {
                    let r2 = bytes[j..].iter().take_while(|&&b| b == b'`').count();
                    if r2 == run {
                        closed = true;
                        break;
                    }
                    j += r2;
                } else {
                    j += 1;
                }
            }
            i = if closed { j + run } else { i + run };
            continue;
        }

        // Wiki-link: `[[...]]`, optionally preceded by `!`.
        if bytes[i] == b'[' && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            if let Some(close) = find_close(bytes, i + 2) {
                let inner = &line[i + 2..close];
                if !inner.contains("[[") && !inner.contains('[') {
                    let embed = i > 0 && bytes[i - 1] == b'!';
                    let full_start = if embed { i - 1 } else { i };
                    let full_end = close + 2;
                    let (target, alias) = match inner.split_once('|') {
                        Some((t, a)) => (t.trim().to_string(), Some(a.trim().to_string())),
                        None => (inner.trim().to_string(), None),
                    };
                    // Strip heading/block anchors from the target.
                    let target = target.split('#').next().unwrap_or(&target).to_string();
                    let (stem_start, stem_end) = stem_range(inner);
                    out.push(WikiLink {
                        target,
                        alias,
                        embed,
                        start: line_start + full_start,
                        end: line_start + full_end,
                        stem_start: line_start + i + 2 + stem_start,
                        stem_end: line_start + i + 2 + stem_end,
                    });
                    i = full_end;
                    continue;
                }
            }
        }
        i += 1;
    }
}

/// The byte range of the note name inside a wiki-link's `inner` text (everything
/// between the brackets), relative to `inner` itself.
///
/// Cut in order: stop at the `|` that starts an alias, then at the `#` that starts
/// a heading or block anchor, then keep only what follows the last `/`. Finally
/// trim the padding a writer may have left inside the brackets, so `[[ Note ]]`
/// names `Note` and nothing else.
fn stem_range(inner: &str) -> (usize, usize) {
    let head_end = inner.find('|').unwrap_or(inner.len());
    let path_end = inner[..head_end].find('#').unwrap_or(head_end);
    let name_start = inner[..path_end].rfind('/').map(|at| at + 1).unwrap_or(0);
    let written = inner[name_start..path_end].trim_start();
    let name = written.trim_end();
    if name.is_empty() {
        return (0, 0);
    }
    let start = name_start + (inner[name_start..path_end].len() - written.len());
    (start, start + name.len())
}

/// Find the index of the `]]` closing a wiki-link opened at `from - 2`.
fn find_close(bytes: &[u8], from: usize) -> Option<usize> {
    let mut i = from;
    while i + 1 < bytes.len() {
        if bytes[i] == b']' && bytes[i + 1] == b']' {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// An image that a whole line embeds, which the editor can draw in place of the
/// source text.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageEmbed {
    /// The attachment target: a vault-relative path, a file name or a stem.
    pub target: String,
    /// A width in pixels from Obsidian's size suffix (`![[img.png|300]]`,
    /// `![[img.png|300x200]]`) or markdown's (`![alt](img.png =300x200)`).
    pub width: Option<f32>,
    /// A height in pixels, from the `WxH` forms only.
    pub height: Option<f32>,
}

/// The image a whole line embeds, if the line is nothing but one: `![[att.png]]`
/// (Obsidian's embed) or `![alt](path)` (markdown image syntax).
///
/// A line with any other text on it is not an image line. That is deliberate:
/// the editor hands the entire display line over to the picture, so a row that
/// mixes text and image would put the paint and the wrap model out of step.
pub fn line_image_embed(line: &str) -> Option<ImageEmbed> {
    let trimmed = line.trim();
    let (inner, size) = wiki_embed_target(trimmed).or_else(|| markdown_image_target(trimmed))?;
    let (width, height) = size.map(parse_size).unwrap_or((None, None));
    // A heading anchor names a section of a note, not part of an image.
    let target = inner.split('#').next().unwrap_or(inner).trim().to_string();
    if target.is_empty() {
        return None;
    }
    Some(ImageEmbed {
        target,
        width,
        height,
    })
}

/// The Obsidian embed form, `![[att.png]]` or `![[att.png|300]]`: the target and
/// its size suffix, which may be an alias instead of a size.
fn wiki_embed_target(trimmed: &str) -> Option<(&str, Option<&str>)> {
    let rest = trimmed.strip_prefix("![[")?;
    let inner = rest.strip_suffix("]]")?;
    // An aliased link is not an image, and a nested bracket is a mistake.
    if inner.contains('[') || inner.contains(']') {
        return None;
    }
    match inner.split_once('|') {
        Some((target, size)) => Some((target, Some(size))),
        None => Some((inner, None)),
    }
}

/// The markdown image form, `![alt](path)`: the path and whatever follows it,
/// which may be `=300x200`, a quoted title, or nothing.
fn markdown_image_target(trimmed: &str) -> Option<(&str, Option<&str>)> {
    let rest = trimmed.strip_prefix("![")?;
    let inner = rest.strip_suffix(')')?;
    let (alt, dest) = inner.split_once("](")?;
    // An alt text may not contain brackets, and a second `)` means the form was
    // not a plain image after all.
    if alt.contains('[') || alt.contains(']') || dest.contains(')') {
        return None;
    }
    let mut parts = dest.split_whitespace();
    let path = parts.next()?;
    Some((path, parts.next()))
}

/// Obsidian's image size suffix: `300` (width) or `300x200`.
fn parse_size(size: &str) -> (Option<f32>, Option<f32>) {
    let size = size.trim().trim_start_matches('=');
    if let Some((w, h)) = size.split_once('x') {
        let (w, h) = (w.parse::<f32>().ok(), h.parse::<f32>().ok());
        return (w.filter(|v| *v > 0.), h.filter(|v| *v > 0.));
    }
    let width = size.parse::<f32>().ok();
    (width.filter(|v| *v > 0.), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn targets(text: &str) -> Vec<String> {
        extract_wikilinks(text)
            .into_iter()
            .map(|l| l.target)
            .collect()
    }

    #[test]
    fn simple_link() {
        assert_eq!(targets("See [[Some Note]] here."), vec!["Some Note"]);
    }

    #[test]
    fn alias_link() {
        let links = extract_wikilinks("See [[Some Note|the note]].");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "Some Note");
        assert_eq!(links[0].alias.as_deref(), Some("the note"));
    }

    #[test]
    fn embed_link() {
        let links = extract_wikilinks("![[image.png]]");
        assert_eq!(links.len(), 1);
        assert!(links[0].embed);
        assert_eq!(links[0].target, "image.png");
        assert_eq!(links[0].start, 0);
        assert_eq!(
            &"![[image.png]]"[links[0].start..links[0].end],
            "![[image.png]]"
        );
    }

    #[test]
    fn heading_anchor_stripped() {
        assert_eq!(targets("See [[Note#Section]]."), vec!["Note"]);
    }

    #[test]
    fn code_blocks_ignored() {
        let text = "```rust\n[[NotALink]]\n```\n\nReal [[Link]].";
        assert_eq!(targets(text), vec!["Link"]);
    }

    #[test]
    fn tilde_fences_ignored() {
        let text = "~~~\n[[Nope]]\n~~~\n[[Yes]]";
        assert_eq!(targets(text), vec!["Yes"]);
    }

    #[test]
    fn inline_code_ignored() {
        assert_eq!(targets("Use `[[x]]` but [[y]]."), vec!["y"]);
    }

    #[test]
    fn multiple_links_offsets_ordered() {
        let text = "[[a]] then [[b]]";
        let links = extract_wikilinks(text);
        assert_eq!(links.len(), 2);
        assert!(links[0].start < links[1].start);
        assert_eq!(&text[links[1].start..links[1].end], "[[b]]");
    }

    #[test]
    fn offsets_across_lines() {
        let text = "first line\nsecond [[here]]";
        let links = extract_wikilinks(text);
        assert_eq!(links.len(), 1);
        assert_eq!(&text[links[0].start..links[0].end], "[[here]]");
    }

    #[test]
    fn unterminated_link_ignored() {
        assert_eq!(targets("broken [[link"), Vec::<String>::new());
    }

    #[test]
    fn multiline_link_ignored() {
        assert_eq!(targets("[[spans\nlines]]"), Vec::<String>::new());
    }

    #[test]
    fn an_image_line_is_only_an_embed() {
        let embed = line_image_embed("![[diagram.png]]").unwrap();
        assert_eq!(embed.target, "diagram.png");
        assert_eq!((embed.width, embed.height), (None, None));

        // Indentation and trailing spaces do not stop it being an image line.
        assert_eq!(
            line_image_embed("  ![[assets/diagram.png]]  ")
                .unwrap()
                .target,
            "assets/diagram.png"
        );

        // Markdown image syntax, with alt text and a title.
        let md = line_image_embed(r#"![a diagram](assets/diagram.png "the title")"#).unwrap();
        assert_eq!(md.target, "assets/diagram.png");

        // Anything else on the row means it is not an image line.
        assert!(line_image_embed("see ![[diagram.png]] below").is_none());
        assert!(line_image_embed("![[unclosed").is_none());
        assert!(
            line_image_embed("[[note]]").is_none(),
            "a link is not an embed"
        );
        assert!(line_image_embed("![[ ]]").is_none(), "an empty target");
    }

    #[test]
    fn an_image_line_can_carry_a_size() {
        let embed = line_image_embed("![[diagram.png|300]]").unwrap();
        assert_eq!((embed.width, embed.height), (Some(300.), None));

        let embed = line_image_embed("![[diagram.png|300x200]]").unwrap();
        assert_eq!((embed.width, embed.height), (Some(300.), Some(200.)));

        let embed = line_image_embed("![alt](diagram.png =300x200)").unwrap();
        assert_eq!(embed.target, "diagram.png");
        assert_eq!((embed.width, embed.height), (Some(300.), Some(200.)));

        // A non-numeric suffix is an alias, not a size: Obsidian's `|alias` on
        // an embed is rare, and drawing at the natural size is the safe answer.
        let embed = line_image_embed("![[diagram.png|the diagram]]").unwrap();
        assert_eq!(embed.target, "diagram.png");
        assert_eq!((embed.width, embed.height), (None, None));
    }

    /// The name a rename is allowed to change: the last path component of the
    /// target, and nothing around it.
    #[test]
    fn a_links_stem_is_its_name_and_nothing_else() {
        for (text, name) in [
            ("[[Note]]", "Note"),
            ("[[ Note ]]", "Note"),
            ("![[Note]]", "Note"),
            ("[[folder/Note]]", "Note"),
            ("[[Note#A heading]]", "Note"),
            ("[[folder/Note#A heading|read this]]", "Note"),
            ("[[Note|read this]]", "Note"),
            ("[[note]]", "note"),
        ] {
            let links = extract_wikilinks(text);
            assert_eq!(links.len(), 1, "{text} should hold one link");
            assert_eq!(
                &text[links[0].stem_start..links[0].stem_end],
                name,
                "{text} names {name}"
            );
        }
    }

    /// A link to a heading in the same note has no name to rewrite, and the stem
    /// range is empty rather than pointing at something it would overwrite.
    #[test]
    fn a_link_with_no_name_has_an_empty_stem() {
        let text = "[[#A heading in this note]]";
        let links = extract_wikilinks(text);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].stem_start, links[0].stem_end);
    }
}
