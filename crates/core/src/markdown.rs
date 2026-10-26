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
                    out.push(WikiLink {
                        target,
                        alias,
                        embed,
                        start: line_start + full_start,
                        end: line_start + full_end,
                    });
                    i = full_end;
                    continue;
                }
            }
        }
        i += 1;
    }
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
}
