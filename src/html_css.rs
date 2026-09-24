//! Minimal CSS → terminal-style extractor.
//!
//! Not a real CSS engine. It scans `selector { decls }` rules and keeps only
//! the handful of properties a terminal can honor: fg/bg color, bold, italic,
//! underline, line-through, `display:none`/`block`, and `::before` content.
//!
//! Method A: only single selectors are indexed — `tag`, `.class`, `tag.class`.
//! Descendant/complex selectors (`div.level-1 p.normaltext-1`, `a:hover`, …)
//! are ignored. ponytail: descendant selectors dropped; add last-segment
//! matching if sense-level indentation proves illegible.

use std::collections::HashMap;

use ratatui::style::Color;

/// Terminal-representable style fragment contributed by one CSS rule.
#[derive(Clone, Default, Debug, PartialEq)]
pub struct TermStyle {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub hidden: bool,
    pub block: bool,
}

impl TermStyle {
    /// Merge `other` on top of `self` (later/more-specific wins for colors;
    /// flags are additive since we never model resets).
    pub fn merge(&mut self, other: &TermStyle) {
        if other.fg.is_some() {
            self.fg = other.fg;
        }
        if other.bg.is_some() {
            self.bg = other.bg;
        }
        self.bold |= other.bold;
        self.italic |= other.italic;
        self.underline |= other.underline;
        self.strike |= other.strike;
        self.hidden |= other.hidden;
        self.block |= other.block;
    }
}

#[derive(Clone, Default, Debug)]
struct Rule {
    style: TermStyle,
    prefix: Option<String>,
}

/// Indexed CSS rules keyed by `(tag, class)`; empty string means "any".
#[derive(Default, Debug)]
pub struct StyleTable {
    rules: HashMap<(String, String), Rule>,
}

/// Style resolved for one element, plus any `::before` text to emit.
#[derive(Default, Debug, PartialEq)]
pub struct Resolved {
    pub style: TermStyle,
    pub prefix: Option<String>,
    pub matched: bool,
}

impl StyleTable {
    pub fn parse(css: &str) -> Self {
        let mut table = StyleTable::default();
        let stripped = strip_comments(css);
        // Naive rule split on '}'. @media/@font-face bodies get mangled into
        // junk selectors that parse_selector rejects — acceptable for method A.
        for chunk in stripped.split('}') {
            let Some((sels, body)) = chunk.split_once('{') else {
                continue;
            };
            let (style, prefix) = parse_body(body);
            for sel in sels.split(',') {
                let Some((tag, class, is_before)) = parse_selector(sel) else {
                    continue;
                };
                let entry = table.rules.entry((tag, class)).or_default();
                if is_before {
                    if let Some(p) = &prefix {
                        entry.prefix = Some(p.clone());
                    }
                } else {
                    entry.style.merge(&style);
                }
            }
        }
        table
    }

    /// Resolve the merged style for an element. Applies `tag`, then each class
    /// (class beats tag; later class beats earlier).
    pub fn resolve(&self, tag: &str, classes: &[&str]) -> Resolved {
        let mut out = Resolved::default();
        if let Some(r) = self.rules.get(&(tag.to_string(), String::new())) {
            out.style.merge(&r.style);
            if r.prefix.is_some() {
                out.prefix = r.prefix.clone();
            }
            out.matched = true;
        }
        for class in classes {
            for key in [
                (String::new(), class.to_string()),
                (tag.to_string(), class.to_string()),
            ] {
                if let Some(r) = self.rules.get(&key) {
                    out.style.merge(&r.style);
                    if r.prefix.is_some() {
                        out.prefix = r.prefix.clone();
                    }
                    out.matched = true;
                }
            }
        }
        out
    }
}

fn strip_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        match rest[start + 2..].find("*/") {
            Some(end) => rest = &rest[start + 2 + end + 2..],
            None => return out, // unterminated comment: drop the tail
        }
    }
    out.push_str(rest);
    out
}

/// Parse a single selector into `(tag, class, is_before)`, or None if it is a
/// descendant/complex/pseudo selector we don't support.
fn parse_selector(sel: &str) -> Option<(String, String, bool)> {
    let sel = sel.trim();
    let (sel, is_before) = match sel
        .strip_suffix("::before")
        .or_else(|| sel.strip_suffix(":before"))
    {
        Some(s) => (s.trim(), true),
        None => (sel, false),
    };
    if sel.is_empty()
        || sel.contains([
            ' ', '\t', '\n', '>', '+', '~', '(', '[', ']', '*', '#', ':', '@',
        ])
    {
        return None;
    }
    let (tag, class) = match sel.find('.') {
        Some(0) => ("", sel[1..].split('.').next().unwrap_or("")),
        Some(i) => (&sel[..i], sel[i + 1..].split('.').next().unwrap_or("")),
        None => (sel, ""),
    };
    if class.is_empty() && tag.is_empty() {
        return None;
    }
    Some((tag.to_lowercase(), class.to_string(), is_before))
}

fn parse_body(body: &str) -> (TermStyle, Option<String>) {
    let mut ts = TermStyle::default();
    let mut prefix = None;
    for decl in body.split(';') {
        let Some((prop, val)) = decl.split_once(':') else {
            continue;
        };
        let prop = prop.trim().to_ascii_lowercase();
        let val = val.trim();
        match prop.as_str() {
            "color" => {
                if let Some(c) = parse_color(val) {
                    ts.fg = Some(c);
                }
            }
            "background" | "background-color" => {
                if let Some((r, g, b)) = parse_color_raw(val) {
                    // Dictionary CSS is authored for a light-page design;
                    // high-luma backgrounds wash out on a dark terminal.
                    // Drop them (transparent = terminal default), keep only
                    // dark/medium backgrounds like blue headers.
                    if luma((r, g, b)) < 190 {
                        ts.bg = Some(Color::Rgb(r, g, b));
                    }
                }
            }
            "font-weight" => {
                if matches!(val, "bold" | "bolder" | "700" | "800" | "900") {
                    ts.bold = true;
                }
            }
            "font-style" if val.eq_ignore_ascii_case("italic") => {
                ts.italic = true;
            }
            "text-decoration" | "text-decoration-line" => {
                if val.contains("underline") {
                    ts.underline = true;
                }
                if val.contains("line-through") {
                    ts.strike = true;
                }
            }
            "display" => match val {
                "none" => ts.hidden = true,
                "block" => ts.block = true,
                _ => {}
            },
            "content" => prefix = parse_content(val),
            _ => {}
        }
    }
    (ts, prefix)
}

/// Parse a color value into a terminal-readable RGB, or None when the author
/// meant "default text" (black / transparent / keywords we can't honor).
///
/// Dictionary CSS is authored for a light page background (`body{background:#fcfcfc}`).
/// On a dark terminal its dark example/gloss colors (`rgb(19,18,18)`, `dimgray`,
/// `rgb(115,115,125)`) collapse into the background. Every parsed color passes
/// through `readable`, which lifts too-dark colors toward white while keeping
/// their hue, so example/source/translation stay visually distinct yet legible.
fn parse_color(val: &str) -> Option<Color> {
    readable(parse_color_raw(val)?)
}

/// Parse a color value into an RGB tuple, without the dark-terminal lift.
/// Used directly for backgrounds (lifting a background would wash it out) and
/// as the front half of `parse_color` for foregrounds.
fn parse_color_raw(val: &str) -> Option<(u8, u8, u8)> {
    let v = val.trim();
    let rgb = if let Some(hex) = v.strip_prefix('#') {
        parse_hex(hex)?
    } else if v.starts_with("rgb") {
        parse_rgb(v)?
    } else {
        match v.to_ascii_lowercase().as_str() {
            "black" | "transparent" | "inherit" | "currentcolor" | "none" | "initial" => {
                return None;
            }
            "white" => (255, 255, 255),
            "red" => (220, 40, 40),
            "brown" => (165, 42, 42),
            "grey" | "gray" => (128, 128, 128),
            "dimgray" | "dimgrey" => (105, 105, 105),
            "blue" => (0, 90, 200),
            "green" => (0, 128, 0),
            "orange" => (211, 84, 0),
            _ => {
                // A gradient/complex value: scan for the first hex color.
                let i = v.find('#')?;
                parse_hex(&v[i + 1..])?
            }
        }
    };
    // Pure black is the author's "default text"; drop it either way.
    if rgb == (0, 0, 0) { None } else { Some(rgb) }
}

/// Perceived luminance (Rec. 601), 0..=255.
fn luma((r, g, b): (u8, u8, u8)) -> u8 {
    ((r as u32 * 299 + g as u32 * 587 + b as u32 * 114) / 1000) as u8
}

/// Lift a parsed foreground color to be readable on a dark terminal.
///
/// Dictionary CSS assumes a light page, so it uses dark greys for secondary
/// text (example `rgb(19,18,18)` luma 18, gloss `dimgray` luma 105,
/// source `rgb(115,115,125)` luma 116). On black these collapse into the
/// background AND into each other — but they must stay *visually distinct*
/// to serve as style cues. Strategy:
///
/// - Grey (spread ≤ 24): **linear remap** luma [0, FLOOR) → [BFLOOR, FLOOR),
///   keeping relative order. example→69, gloss→115, source→121 — all grey,
///   all distinct, all readable. Output is perfectly grey (all channels equal).
/// - Hued (spread > 24): **additive white lift** (k = FLOOR - luma, +k per
///   channel). Preserves hue ordering; clamped at 255 for very saturated darks.
/// - Already bright (luma ≥ FLOOR): pass through unchanged.
/// - Pure black: dropped upstream in `parse_color_raw`.
const FLOOR: u8 = 128;
/// Minimum acceptable luma for grey text on a dark terminal.
const GREY_FLOOR: u8 = 110;
fn readable((r, g, b): (u8, u8, u8)) -> Option<Color> {
    let l = luma((r, g, b));
    if l >= FLOOR {
        return Some(Color::Rgb(r, g, b));
    }
    let spread = r.max(g).max(b) - r.min(g).min(b);
    if spread <= 24 {
        // Grey: remap luma linearly into [GREY_FLOOR, FLOOR). All channels
        // set to the same target to stay perfectly grey.
        let target =
            GREY_FLOOR as u32 + l as u32 * (FLOOR as u32 - GREY_FLOOR as u32) / FLOOR as u32;
        let c = target.min(255) as u8;
        Some(Color::Rgb(c, c, c))
    } else {
        // Hued: additive white lift to reach FLOOR (preserves hue).
        let k = FLOOR - l;
        let lift = |c: u8| c.saturating_add(k);
        Some(Color::Rgb(lift(r), lift(g), lift(b)))
    }
}

fn parse_hex(hex: &str) -> Option<(u8, u8, u8)> {
    let h: String = hex.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
    match h.len() {
        3 => {
            let d = |c: u8| (c as char).to_digit(16).unwrap_or(0) as u8;
            let bytes = h.as_bytes();
            Some((d(bytes[0]) * 17, d(bytes[1]) * 17, d(bytes[2]) * 17))
        }
        6 | 8 => Some((
            u8::from_str_radix(&h[0..2], 16).ok()?,
            u8::from_str_radix(&h[2..4], 16).ok()?,
            u8::from_str_radix(&h[4..6], 16).ok()?,
        )),
        _ => None,
    }
}

fn parse_rgb(val: &str) -> Option<(u8, u8, u8)> {
    let inner = val.split_once('(')?.1.trim_end_matches([')', ' ']);
    let mut it = inner.split(',').map(|s| s.trim());
    let r: u8 = it.next()?.parse().ok()?;
    let g: u8 = it.next()?.parse().ok()?;
    let b: u8 = it.next()?.parse().ok()?;
    Some((r, g, b))
}

/// Decode a `content` value: strip quotes, decode `\XXXX` unicode escapes.
fn parse_content(val: &str) -> Option<String> {
    let v = val.trim().trim_matches(['"', '\'']);
    if v.is_empty() {
        return None;
    }
    let mut out = String::new();
    let mut chars = v.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            let mut hex = String::new();
            while let Some(&h) = chars.peek() {
                if h.is_ascii_hexdigit() && hex.len() < 6 {
                    hex.push(h);
                    chars.next();
                } else {
                    if h == ' ' {
                        chars.next(); // CSS escape terminator space
                    }
                    break;
                }
            }
            if let Some(ch) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                out.push(ch);
            }
        } else {
            out.push(c);
        }
    }
    (!out.is_empty()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_color() {
        // #1685C0 (luma 106) is hued but below the floor → lifted, hue kept.
        let t = StyleTable::parse(".src { color: #1685C0; font-style: italic; }");
        let r = t.resolve("span", &["src"]);
        let Some(Color::Rgb(rr, gg, bb)) = r.style.fg else {
            panic!("expected lifted rgb")
        };
        assert!(bb > gg && gg > rr, "hue not preserved: {rr},{gg},{bb}");
        assert!(luma((rr, gg, bb)) >= FLOOR, "not lifted to floor");
        assert!(r.style.italic);
        assert!(r.matched);
    }

    #[test]
    fn tag_class_selector() {
        let t = StyleTable::parse("span.error { color: brown; text-decoration: line-through; }");
        let hit = t.resolve("span", &["error"]);
        // brown (luma 78) is hued below the floor → lifted, red stays dominant.
        let Some(Color::Rgb(rr, gg, bb)) = hit.style.fg else {
            panic!("expected lifted rgb")
        };
        assert!(rr > gg && rr > bb, "hue not preserved: {rr},{gg},{bb}");
        assert!(luma((rr, gg, bb)) >= FLOOR, "not lifted to floor");
        assert!(hit.style.strike);
        // Same class on a different tag must NOT match a tag-qualified rule.
        assert!(!t.resolve("div", &["error"]).matched);
    }

    #[test]
    fn dim_grey_remapped_preserves_order() {
        // dimgray (105) → remap luma 105→115 in [GREY_FLOOR, FLOOR). Grey stays
        // perfectly grey, distinct from the lighter source and from body cyan.
        let t = StyleTable::parse("span.ch{color:dimgray}");
        let ch = t.resolve("span", &["ch"]);
        let Some(Color::Rgb(r, g, b)) = ch.style.fg else {
            panic!("expected grey")
        };
        assert!(r == g && g == b, "grey not preserved: {r},{g},{b}");
        let l = luma((r, g, b));
        assert!(l >= GREY_FLOOR, "too dim: {l}");
        assert!(l < FLOOR, "should be below FLOOR (remap target): {l}");
    }

    #[test]
    fn dim_source_grey_remapped() {
        // Source rgb(115,115,125): luma 116 → remap to ~121 grey. Sits between
        // gloss (115) and bright threshold, kept distinct by remap spread.
        let t = StyleTable::parse("span.source{color:rgb(115,115,125)}");
        let Some(Color::Rgb(r, g, b)) = t.resolve("span", &["source"]).style.fg else {
            panic!("expected grey");
        };
        assert!(r == g && g == b, "grey not preserved: {r},{g},{b}");
        let l = luma((r, g, b));
        assert!(l >= GREY_FLOOR, "too dim: {l}");
    }

    #[test]
    fn grey_remap_spread() {
        // The three key greys must stay spread apart (not collapsed to one point).
        let t = StyleTable::parse(
            ".a{color:rgb(19,18,18)} .b{color:dimgray} .c{color:rgb(115,115,125)}",
        );
        let extract = |cls| {
            let r = t.resolve("span", &[cls]);
            match r.style.fg {
                Some(Color::Rgb(c, c2, c3)) if c == c2 && c2 == c3 => c as i16,
                _ => panic!("{cls} not grey"),
            }
        };
        let (a, b, _c) = (extract("a"), extract("b"), extract("c"));
        assert!(b - a >= 10, "example vs gloss too close: {a} vs {b}");
        // gloss vs source gap narrows as GREY_FLOOR rises; source has italic
        // as an additional visual distinction, so color gap alone is not required.
    }

    #[test]
    fn bright_grey_kept() {
        // A light grey (above the floor) stays as-is; only dim greys drop.
        let t = StyleTable::parse(".g{color:grey}");
        assert_eq!(
            t.resolve("span", &["g"]).style.fg,
            Some(Color::Rgb(128, 128, 128))
        );
    }

    #[test]
    fn dark_hued_colour_lifted_keeping_hue() {
        // A dark example colour with hue gets brightened, not dropped: the blue
        // channel must stay dominant after lifting.
        let t = StyleTable::parse(".b{color:#102040}");
        let Some(Color::Rgb(r, g, b)) = t.resolve("span", &["b"]).style.fg else {
            panic!("expected lifted rgb");
        };
        assert!(b > r && b > g, "hue not preserved: {r},{g},{b}");
        assert!(luma((r, g, b)) >= FLOOR, "not lifted to floor");
    }

    #[test]
    fn before_content_symbol_and_escape() {
        let t = StyleTable::parse(r#"li.ex::before{content:"◇"} q::before{content:"\2014\00A0"}"#);
        assert_eq!(t.resolve("li", &["ex"]).prefix.as_deref(), Some("◇"));
        assert_eq!(t.resolve("q", &[]).prefix.as_deref(), Some("—\u{A0}"));
    }

    #[test]
    fn black_is_default_not_a_color() {
        let t = StyleTable::parse("a{color:black} b{color:#000}");
        assert_eq!(t.resolve("a", &[]).style.fg, None);
        assert_eq!(t.resolve("b", &[]).style.fg, None);
    }

    #[test]
    fn rgb_and_background() {
        let t = StyleTable::parse(".hdr{background-color:rgb(0,114,198);color:#fff}");
        let r = t.resolve("div", &["hdr"]);
        assert_eq!(r.style.bg, Some(Color::Rgb(0, 114, 198)));
        assert_eq!(r.style.fg, Some(Color::Rgb(255, 255, 255)));
    }

    #[test]
    fn descendant_and_pseudo_ignored() {
        let t = StyleTable::parse(
            "div.level-1 p.normaltext-1{color:red} a:hover{color:blue} .x{color:brown}",
        );
        assert!(!t.resolve("p", &["normaltext-1"]).matched);
        assert!(!t.resolve("a", &[]).matched);
        assert!(t.resolve("span", &["x"]).matched);
    }

    #[test]
    fn comments_and_media_do_not_break_parsing() {
        let css =
            "/* hi */ .a{color:brown} @media (min-width:576px){.b{color:red}} .c{font-weight:bold}";
        let t = StyleTable::parse(css);
        assert!(t.resolve("span", &["a"]).matched);
        assert!(t.resolve("span", &["c"]).style.bold);
    }
}
