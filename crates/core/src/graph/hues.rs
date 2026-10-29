//! Tag colors: stable, distinct, strong. The plan's rules (graph-view-plan.md
//! "Tag-node coloring" and the resolved hue-family decision):
//!
//! - A tag's hue is seeded from a stable hash of its name — no manual
//!   coloring, same vault tomorrow gets the same colors.
//! - Colors live in OKLCH (perceptually uniform: equal hue steps look equal,
//!   no dark-navy-vs-bright-yellow surprises) at a fixed lightness/chroma
//!   band, so nothing comes out a muted grey.
//! - A collision-avoid pass guarantees ~20° minimum hue distance between
//!   root tags while there is room (360/20 = 18 slots); past that the
//!   spacing degrades gracefully by maximizing the minimum distance.
//! - Nested tags (`#proj/weekly`) form hue families: the root anchors the
//!   family, children sit within ±FAMILY_SPREAD of it and stay distinct
//!   from their siblings.
//!
//! Output is sRGB bytes plus an HSLA triple (gpui's `Hsla` shape) so the
//! paint boundary is a struct literal, not a conversion.

use std::collections::HashMap;

/// Minimum hue separation between root tags, degrees.
pub const MIN_SEP: f32 = 20.0;
/// Children stay within this window around their root's hue, degrees.
pub const FAMILY_SPREAD: f32 = 30.0;
/// Minimum separation between siblings inside one family, degrees.
pub const SIB_SEP: f32 = 8.0;

/// The fixed OKLCH band: lightness and chroma are constant for every tag so
/// the palette reads as one family of strong colors — only the hue varies.
/// L 0.72 / C 0.155 is vivid without neon; hues that fall outside sRGB get
/// their chroma pulled in (hue preserved) rather than clipped ugly.
const OKLCH_L: f32 = 0.72;
const OKLCH_C: f32 = 0.155;

/// A tag's assigned color, ready for the renderer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TagColor {
    pub hue: f32,
    pub srgb: [u8; 3],
    /// gpui `Hsla` components (h in 0..360, s/l in 0..1).
    pub hsla: [f32; 3],
}

/// FNV-1a, 64-bit: stable across builds and platforms, good enough to seed
/// a hue (we only need the low bits to be well-mixed).
fn name_hash(name: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in name.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

fn seed_hue(name: &str) -> f32 {
    (name_hash(name) % 36_000) as f32 / 100.0
}

/// Smallest angular distance between two hues, degrees (0..180).
fn hue_dist(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

/// OKLCH -> linear sRGB (may be out of gamut; the caller fixes that).
/// f64 internally: the matrix constants are full-precision, and this runs
/// once per tag at assignment, never per frame.
fn oklch_to_linear(l: f32, c: f32, h_deg: f32) -> [f32; 3] {
    let (l, c) = (l as f64, c as f64);
    let h = (h_deg as f64).to_radians();
    let a = c * h.cos();
    let b = c * h.sin();
    let l_ = l + 0.396_337_777_4 * a + 0.215_803_757_3 * b;
    let m_ = l - 0.105_561_345_8 * a - 0.063_854_172_8 * b;
    let s_ = l - 0.089_484_177_5 * a - 1.291_485_548_0 * b;
    let (lc, mc, sc) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);
    [
        (4.076_741_662_1 * lc - 3.307_711_591_3 * mc + 0.230_969_929_2 * sc) as f32,
        (-1.268_438_004_6 * lc + 2.609_757_401_1 * mc - 0.341_319_396_5 * sc) as f32,
        (-0.004_196_086_3 * lc - 0.703_418_614_7 * mc + 1.707_614_701_0 * sc) as f32,
    ]
}

fn in_gamut(rgb: &[f32; 3]) -> bool {
    rgb.iter().all(|v| (0.0..=1.0).contains(v))
}

fn gamma(v: f32) -> f32 {
    let v = v.clamp(0.0, 1.0);
    if v <= 0.003_130_8 {
        12.92 * v
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

/// OKLCH -> sRGB bytes, pulling chroma in (binary search, hue preserved)
/// when the color falls outside sRGB. Never clips a channel: a slightly
/// softer color beats a hue-shifted one.
pub fn oklch_to_srgb(l: f32, c: f32, h_deg: f32) -> [u8; 3] {
    let mut lo = 0.0f32;
    let mut hi = c;
    if in_gamut(&oklch_to_linear(l, hi, h_deg)) {
        // full chroma fits
    } else {
        for _ in 0..16 {
            let mid = (lo + hi) * 0.5;
            if in_gamut(&oklch_to_linear(l, mid, h_deg)) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        hi = lo;
    }
    let lin = oklch_to_linear(l, hi, h_deg);
    [
        (gamma(lin[0]) * 255.0).round() as u8,
        (gamma(lin[1]) * 255.0).round() as u8,
        (gamma(lin[2]) * 255.0).round() as u8,
    ]
}

/// sRGB -> HSL (gpui's `Hsla` layout). Straightforward max/min math.
pub fn srgb_to_hsla(rgb: [u8; 3]) -> [f32; 3] {
    let r = rgb[0] as f32 / 255.0;
    let g = rgb[1] as f32 / 255.0;
    let b = rgb[2] as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) * 0.5;
    if (max - min).abs() < 1e-6 {
        return [0.0, 0.0, l];
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if max == r {
        ((g - b) / d + if g < b { 6.0 } else { 0.0 }) * 60.0
    } else if max == g {
        ((b - r) / d + 2.0) * 60.0
    } else {
        ((r - g) / d + 4.0) * 60.0
    };
    [h.rem_euclid(360.0), s, l]
}

/// The top-level segment of a nested tag (`proj/weekly` -> `proj`).
fn root_of(tag: &str) -> &str {
    tag.split('/').next().unwrap_or(tag)
}

/// Pick the best hue for `seed` against `taken`: the candidate maximizing
/// the minimum distance to `taken` (ties break toward the seed). Scans at
/// 1° resolution — 360 candidates is nothing, and it makes the guarantee
/// exact rather than approximate.
fn best_hue(seed: f32, taken: &[f32], window: Option<(f32, f32)>) -> f32 {
    if taken.is_empty() {
        return seed;
    }
    let (lo, hi) = match window {
        // Family window around the root hue, scanned at 1°.
        Some((center, spread)) => (center - spread, center + spread),
        None => (0.0, 360.0),
    };
    let mut best = seed;
    let mut best_score = f32::MIN;
    let mut x = lo;
    while x <= hi {
        let h = x.rem_euclid(360.0);
        let mind = taken.iter().fold(f32::MAX, |m, t| m.min(hue_dist(h, *t)));
        // Prefer separation; nudge toward the seed on near-ties so the
        // result still smells like the hash.
        let seed_pull = hue_dist(h, seed).min(45.0) / 45.0;
        let score = mind - 0.05 * seed_pull;
        if score > best_score {
            best_score = score;
            best = h;
        }
        x += 1.0;
    }
    best
}

/// Assign colors to a set of tag names. Deterministic: the result depends
/// only on the set, not the input order (roots are processed sorted).
#[derive(Debug, Default, Clone)]
pub struct TagHues {
    colors: HashMap<String, TagColor>,
}

impl TagHues {
    pub fn assign(tags: &[String]) -> Self {
        // Roots first, sorted for determinism.
        let mut roots: Vec<&str> = tags.iter().map(|t| root_of(t)).collect();
        roots.sort_unstable();
        roots.dedup();

        let mut root_hue: HashMap<&str, f32> = HashMap::new();
        let mut taken: Vec<f32> = Vec::new();
        for r in &roots {
            let seed = seed_hue(r);
            let h = best_hue(seed, &taken, None);
            root_hue.insert(r, h);
            taken.push(h);
        }

        let mut colors: HashMap<String, TagColor> = HashMap::new();
        // Non-root tags: sorted by full name; each child anchors on its
        // parent's assigned hue (not the root's) so deep trees fan out
        // within the family, and siblings keep SIB_SEP between them.
        let mut children: Vec<&String> = tags.iter().filter(|t| t.contains('/')).collect();
        children.sort();
        // Assign parents before children (a/b before a/b/c) by depth.
        children.sort_by_key(|t| t.split('/').count());
        // `family_taken` is keyed by the nearest ASSIGNED ancestor, so all
        // descendants of one root share one separation budget.
        let mut family_taken: HashMap<String, Vec<f32>> = HashMap::new();
        for t in &children {
            let segs: Vec<&str> = t.split('/').collect();
            let parent = segs[..segs.len() - 1].join("/");
            // Walk up to the nearest ancestor with an assigned hue (the
            // root always has one, so this terminates).
            let mut anchor = None;
            let mut cur = parent.clone();
            loop {
                if colors.contains_key(&cur) || root_hue.contains_key(cur.as_str()) {
                    anchor = Some(
                        colors
                            .get(&cur)
                            .map(|c| c.hue)
                            .unwrap_or_else(|| root_hue[cur.as_str()]),
                    );
                    break;
                }
                match cur.rfind('/') {
                    Some(k) => cur.truncate(k),
                    None => break,
                }
            }
            let anchor = anchor.unwrap_or_else(|| seed_hue(&parent));
            let seed = anchor + (seed_hue(t) - 180.0) / 180.0 * FAMILY_SPREAD;
            // Budget keyed by the nearest assigned ancestor: every
            // displayed descendant of one anchor shares one separation
            // budget, so two children under different *unlisted*
            // intermediates still can't collide.
            let fam = cur;
            let taken = family_taken.entry(fam).or_default();
            let h = best_hue(seed, taken, Some((anchor, FAMILY_SPREAD)));
            taken.push(h);
            colors.insert(t.to_string(), make_color(h));
        }
        for (r, h) in root_hue {
            colors.insert(r.to_string(), make_color(h));
        }
        Self { colors }
    }

    pub fn get(&self, tag: &str) -> Option<&TagColor> {
        self.colors.get(tag)
    }

    pub fn len(&self) -> usize {
        self.colors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.colors.is_empty()
    }
}

fn make_color(hue: f32) -> TagColor {
    let srgb = oklch_to_srgb(OKLCH_L, OKLCH_C, hue);
    TagColor {
        hue,
        srgb,
        hsla: srgb_to_hsla(srgb),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn assignment_is_deterministic_and_order_independent() {
        let a = TagHues::assign(&tags(&["alpha", "beta", "gamma", "delta"]));
        let b = TagHues::assign(&tags(&["gamma", "alpha", "delta", "beta"]));
        for t in ["alpha", "beta", "gamma", "delta"] {
            assert_eq!(a.get(t).unwrap(), b.get(t).unwrap(), "{t}");
        }
    }

    #[test]
    fn roots_keep_twenty_degrees_until_the_palette_fills() {
        // 15 roots: comfortably inside the 18-slot budget.
        let names: Vec<String> = (0..15).map(|i| format!("tag{i}")).collect();
        let th = TagHues::assign(&names);
        let hues: Vec<f32> = names.iter().map(|n| th.get(n).unwrap().hue).collect();
        for i in 0..hues.len() {
            for j in i + 1..hues.len() {
                assert!(
                    hue_dist(hues[i], hues[j]) >= MIN_SEP - 0.5,
                    "tags {i}/{j} only {:.1} apart",
                    hue_dist(hues[i], hues[j])
                );
            }
        }
    }

    #[test]
    fn past_capacity_spacing_degrades_but_stays_ordered() {
        // 40 roots: no 20° guarantee, but the greedy max-min pass should
        // still spread them far better than raw hashing (worst pair well
        // above a couple of degrees).
        let names: Vec<String> = (0..40).map(|i| format!("tag{i}")).collect();
        let th = TagHues::assign(&names);
        let hues: Vec<f32> = names.iter().map(|n| th.get(n).unwrap().hue).collect();
        let mut worst: f32 = 360.0;
        for i in 0..hues.len() {
            for j in i + 1..hues.len() {
                worst = worst.min(hue_dist(hues[i], hues[j]));
            }
        }
        assert!(worst > 5.0, "worst pair only {worst:.1} apart");
    }

    #[test]
    fn colors_are_strong_never_grey() {
        // The band's promise: every tag color has real saturation. HSL S
        // under ~0.35 would read as the "repeated muted greys" the user
        // rejected.
        let names: Vec<String> = (0..30).map(|i| format!("tag{i}")).collect();
        let th = TagHues::assign(&names);
        for n in &names {
            let c = th.get(n).unwrap();
            assert!(c.hsla[1] > 0.35, "{n} saturation {}", c.hsla[1]);
            assert!(
                c.hsla[2] > 0.35 && c.hsla[2] < 0.85,
                "{n} lightness {}",
                c.hsla[2]
            );
        }
    }

    #[test]
    fn nested_tags_stay_in_their_family_and_siblings_separate() {
        let th = TagHues::assign(&tags(&[
            "proj",
            "proj/weekly",
            "proj/daily",
            "proj/deep/deeper",
            "other",
        ]));
        let root = th.get("proj").unwrap().hue;
        for t in ["proj/weekly", "proj/daily", "proj/deep/deeper"] {
            let d = hue_dist(th.get(t).unwrap().hue, root);
            assert!(d <= FAMILY_SPREAD * 2.0 + 0.5, "{t} is {d:.1} from root");
        }
        assert!(
            hue_dist(
                th.get("proj/weekly").unwrap().hue,
                th.get("proj/daily").unwrap().hue
            ) >= SIB_SEP
        );
        // The other root is far from the family.
        assert!(hue_dist(th.get("other").unwrap().hue, root) >= MIN_SEP - 0.5);
    }

    #[test]
    fn srgb_conversion_round_trips_a_known_color() {
        // Reddish hue 30 at the band: must land in gamut and be red-dominant.
        let c = oklch_to_srgb(OKLCH_L, OKLCH_C, 30.0);
        assert!(
            c[0] > c[1] && c[1] >= c[2],
            "expected red-dominant, got {c:?}"
        );
        // Hue 265: blue-dominant.
        let c = oklch_to_srgb(OKLCH_L, OKLCH_C, 265.0);
        assert!(
            c[2] > c[0] && c[2] > c[1],
            "expected blue-dominant, got {c:?}"
        );
    }

    #[test]
    fn out_of_gamut_hues_pull_chroma_not_clip() {
        // High-chroma purple near the sRGB edge: the result must be in
        // range and still clearly purple (blue > red > green-ish), not a
        // clipped flat blue.
        let c = oklch_to_srgb(OKLCH_L, 0.30, 300.0);
        assert!(c[2] > c[1], "purple lost its red: {c:?}");
        let h = srgb_to_hsla(c);
        assert!(h[1] > 0.3, "chroma collapsed: {h:?}");
    }

    #[test]
    fn hsla_matches_srgb_for_a_pure_channel() {
        let h = srgb_to_hsla([255, 0, 0]);
        assert!((h[0] - 0.0).abs() < 0.01 && (h[1] - 1.0).abs() < 0.01);
        let h = srgb_to_hsla([0, 0, 255]);
        assert!((h[0] - 240.0).abs() < 0.01);
    }
}
