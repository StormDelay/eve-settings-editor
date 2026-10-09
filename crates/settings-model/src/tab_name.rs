//! Overview tab names, read the way EVE's client draws them.
//!
//! A tab name is a markup-bearing string. The client (`overviewWindow.
//! _ConstructTabs`) strips it, wraps it in `<color=…>` when the tab's own
//! `color` key is set, and hands it to an ordinary `Label`, whose markup is
//! tokenised natively (`trinity.ParseLabelText`) and applied by
//! `carbonui/control/label.py`. So a tab name accepts the label's whole
//! language, verified in game on 2026-10-09 (see docs/format-notes.md):
//!
//! - `<color=…>` is a STACK: nesting works, the innermost colour wins. Values
//!   are `0xAARRGGBB`, `#AARRGGBB` or a name; a 6-digit value is read with
//!   alpha 0 and draws nothing.
//! - `<b>`, `<i>`, `<u>`, `<uppercase>` are COUNTERS.
//! - `<fontsize=N>` and `<letterspace=N>` are stacks.
//! - `<font color= size=>` is the HTML-ish form of colour and size.
//! - `<hint=…>` hides its text on a tab. Alignment, links and localisation
//!   tags change nothing visible.
//! - `&lt; &gt; &amp; &nbsp;` are entities.
//!
//! [`parse`] splits a name into styled [`Piece`]s; [`format`] writes pieces
//! back. A name using anything [`format`] cannot write comes back with
//! `editable: false`, and callers must then edit it as raw markup rather than
//! rewrite it from its pieces — the pieces of such a name do not round-trip.

use serde::{Deserialize, Serialize};

/// One run of text sharing one style.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Piece {
    pub text: String,
    /// `AARRGGBB`, uppercase. None = the label's default colour.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "is_false")]
    pub bold: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub italic: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub underline: bool,
    /// Pixels. None = the tab's default (EVE_MEDIUM_FONTSIZE, 14 at Medium).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<i64>,
    /// Extra pixels after every letter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spacing: Option<i64>,
    #[serde(skip_serializing_if = "is_false")]
    pub uppercase: bool,
    /// Inside `<hint>`: present in the string, not drawn on the tab. Read-only:
    /// [`format`] cannot write a hint.
    #[serde(skip_serializing_if = "is_false")]
    pub hidden: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl Piece {
    fn same_style(&self, o: &Piece) -> bool {
        Piece { text: String::new(), ..self.clone() } == Piece { text: String::new(), ..o.clone() }
    }
}

/// A parsed name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TabName {
    pub pieces: Vec<Piece>,
    /// False when the name uses markup [`format`] cannot write (hint, font,
    /// uppercase, links, alignment, unknown or unbalanced tags). Edit such a
    /// name as raw markup.
    pub editable: bool,
    /// Why parts of the name will not show in game.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// EVE's `StringColorToHex` table, matched after Python's `capitalize()`.
const NAMED: [(&str, u32); 22] = [
    ("Black", 0xff000000), ("Green", 0xff008000), ("Silver", 0xffc0c0c0), ("Lime", 0xff00ff00),
    ("Gray", 0xff808080), ("Grey", 0xff808080), ("Olive", 0xff808000), ("White", 0xffffffff),
    ("Yellow", 0xffffff00), ("Maroon", 0xff800000), ("Navy", 0xff000080), ("Red", 0xffff0000),
    ("Blue", 0xff0000ff), ("Purple", 0xff800080), ("Teal", 0xff008080), ("Fuchsia", 0xffff00ff),
    ("Aqua", 0xff00ffff), ("Orange", 0xffff8000), ("Transparent", 0x00000000),
    ("Lightred", 0xffcc3333), ("Lightblue", 0xff7777ff), ("Lightgreen", 0xff80ff80),
];

/// A `<color=…>` value as `AARRGGBB`, the way `Label.ParseColorTag` reads it:
/// a name, else `#`→`0x` and Python's `long(v, 0)`.
fn color_value(v: &str) -> Option<String> {
    let mut cap = v.to_ascii_lowercase();
    if let Some(f) = cap.get_mut(0..1) {
        f.make_ascii_uppercase();
    }
    if let Some((_, n)) = NAMED.iter().find(|(name, _)| *name == cap) {
        return Some(format!("{n:08X}"));
    }
    let v = v.trim().replace('#', "0x");
    let n = match v.strip_prefix("0x").or_else(|| v.strip_prefix("0X")) {
        Some(hex) if !hex.is_empty() && hex.len() <= 8 => u32::from_str_radix(hex, 16).ok()?,
        Some(_) => return None,
        None => v.parse::<u32>().ok()?,
    };
    Some(format!("{n:08X}"))
}

fn attr<'a>(attrs: &'a str, key: &str) -> Option<&'a str> {
    attrs.split_whitespace().find_map(|kv| kv.strip_prefix(key)?.strip_prefix('='))
        .map(|v| v.trim_matches(|c| c == '"' || c == '\''))
}

fn warn(w: String, warnings: &mut Vec<String>) {
    if !warnings.contains(&w) {
        warnings.push(w);
    }
}

fn decode_entities(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&nbsp;", "\u{a0}").replace("&amp;", "&")
}

#[derive(Default)]
struct Style {
    color: Vec<Option<String>>,
    size: Vec<Option<i64>>,
    spacing: Vec<Option<i64>>,
    bold: u32,
    italic: u32,
    underline: u32,
    uppercase: u32,
    hidden: u32,
}

impl Style {
    fn piece(&self, text: String) -> Piece {
        Piece {
            text,
            color: self.color.last().cloned().flatten(),
            size: self.size.last().copied().flatten(),
            spacing: self.spacing.last().copied().flatten(),
            bold: self.bold > 0,
            italic: self.italic > 0,
            underline: self.underline > 0,
            uppercase: self.uppercase > 0,
            hidden: self.hidden > 0,
        }
    }
}

/// Split a stored name into the pieces the client draws. The client strips
/// the name first, so spaces OUTSIDE every tag never reach the screen; spaces
/// inside a tag do, and that is how a tab is widened in game.
pub fn parse(raw: &str) -> TabName {
    let s = raw.trim_matches(|c: char| c.is_ascii_whitespace());
    let mut st = Style::default();
    let mut pieces: Vec<Piece> = vec![];
    let mut editable = true;
    let mut warnings: Vec<String> = vec![];

    let mut rest = s;
    while !rest.is_empty() {
        let (text, tag) = match rest.find('<').and_then(|lt| rest[lt..].find('>').map(|gt| (lt, lt + gt))) {
            Some((lt, gt)) => {
                let t = (&rest[..lt], Some(&rest[lt + 1..gt]));
                rest = &rest[gt + 1..];
                t
            }
            None => {
                let t = (rest, None);
                rest = "";
                t
            }
        };
        if !text.is_empty() {
            let p = st.piece(decode_entities(text));
            match pieces.last_mut() {
                Some(last) if last.same_style(&p) => last.text.push_str(&p.text),
                _ => pieces.push(p),
            }
        }
        let Some(tag) = tag else { continue };
        // The tag's name runs to the first space or `=`; only `name=value`
        // carries a value (`<font color=…>` is a name plus attributes).
        let end = tag.find([' ', '=']).unwrap_or(tag.len());
        let name = &tag[..end];
        let value = tag[end..].strip_prefix('=');
        let unbalanced = |what: &str, warnings: &mut Vec<String>, editable: &mut bool| {
            *editable = false;
            if !warnings.iter().any(|w| w.contains(what)) {
                warnings.push(format!("a closing </{what}> has no matching opening tag"));
            }
        };
        match (name, value) {
            ("color", Some(v)) => match color_value(v) {
                Some(c) => {
                    if c.starts_with("00") {
                        warn(format!("colour {v} has zero alpha, so its text is invisible in game (write 0xAARRGGBB)"), &mut warnings);
                    }
                    st.color.push(Some(c));
                }
                None => {
                    editable = false;
                    warn(format!("EVE cannot read colour {v}"), &mut warnings);
                    st.color.push(st.color.last().cloned().flatten());
                }
            },
            ("/color", None) => {
                if st.color.pop().is_none() {
                    unbalanced("color", &mut warnings, &mut editable);
                }
            }
            ("fontsize", Some(v)) => match v.trim().parse::<i64>() {
                Ok(n) if n > 0 => st.size.push(Some(n)),
                _ => {
                    editable = false;
                    st.size.push(st.size.last().copied().flatten());
                }
            },
            ("/fontsize", None) => {
                if st.size.pop().is_none() {
                    unbalanced("fontsize", &mut warnings, &mut editable);
                }
            }
            ("letterspace", Some(v)) => match v.trim().parse::<i64>() {
                Ok(n) => st.spacing.push(Some(n)),
                Err(_) => {
                    editable = false;
                    st.spacing.push(st.spacing.last().copied().flatten());
                }
            },
            ("/letterspace", None) => {
                if st.spacing.pop().is_none() {
                    unbalanced("letterspace", &mut warnings, &mut editable);
                }
            }
            ("b" | "i" | "u" | "uppercase", None) => {
                if name == "uppercase" {
                    editable = false;
                }
                *match name { "b" => &mut st.bold, "i" => &mut st.italic, "u" => &mut st.underline, _ => &mut st.uppercase } += 1;
            }
            ("/b" | "/i" | "/u" | "/uppercase", None) => {
                let n = match name { "/b" => &mut st.bold, "/i" => &mut st.italic, "/u" => &mut st.underline, _ => &mut st.uppercase };
                if *n == 0 {
                    unbalanced(&name[1..], &mut warnings, &mut editable);
                }
                *n = n.saturating_sub(1);
            }
            ("font", _) => {
                editable = false;
                let attrs = &tag[4..];
                let color = attr(attrs, "color").and_then(color_value);
                st.color.push(color.or_else(|| st.color.last().cloned().flatten()));
                let size = attr(attrs, "size").and_then(|v| v.parse().ok());
                st.size.push(size.or_else(|| st.size.last().copied().flatten()));
            }
            ("/font", None) => {
                editable = false;
                st.color.pop();
                st.size.pop();
            }
            ("hint", _) => {
                editable = false;
                st.hidden += 1;
                warn("text inside <hint> is not drawn on the tab".into(), &mut warnings);
            }
            ("/hint", None) => {
                editable = false;
                st.hidden = st.hidden.saturating_sub(1);
            }
            ("url" | "/url" | "a" | "/a" | "localized" | "/localized" | "left" | "/left" | "right" | "/right"
                | "center" | "/center", _) => editable = false,
            _ => {
                editable = false;
                warn(format!("<{tag}> is not a tag EVE draws"), &mut warnings);
            }
        }
    }
    TabName { pieces, editable, warnings }
}

/// The text a reader sees, tags removed: for menus, toasts and headings.
pub fn plain(raw: &str) -> String {
    parse(raw).pieces.iter().filter(|p| !p.hidden).map(|p| p.text.as_str()).collect()
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Write pieces as markup. Each piece is wrapped on its own, in a fixed tag
/// order, with neighbours of the same style merged first — so `format` of a
/// `parse` is stable. Colours are always written with their alpha: a 6-digit
/// value given here is taken as opaque, never as alpha 0 the way EVE would
/// read it.
pub fn format(pieces: &[Piece]) -> Result<String, String> {
    let mut merged: Vec<Piece> = vec![];
    for p in pieces.iter().filter(|p| !p.text.is_empty()) {
        match merged.last_mut() {
            Some(last) if last.same_style(p) => last.text.push_str(&p.text),
            _ => merged.push(p.clone()),
        }
    }
    let mut out = String::new();
    for p in &merged {
        if p.hidden {
            return Err("a hidden (<hint>) piece cannot be written; edit the raw name instead".into());
        }
        let mut close: Vec<&str> = vec![];
        if let Some(c) = &p.color {
            let hex = c.trim_start_matches('#').trim_start_matches("0x").to_ascii_uppercase();
            let hex = match hex.len() {
                6 => format!("FF{hex}"),
                8 => hex,
                _ => return Err(format!("colour {c} is not AARRGGBB")),
            };
            if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(format!("colour {c} is not AARRGGBB"));
            }
            out += &format!("<color=0x{hex}>");
            close.push("</color>");
        }
        if let Some(n) = p.size {
            if !(1..=64).contains(&n) {
                return Err(format!("size {n} is outside 1..64"));
            }
            out += &format!("<fontsize={n}>");
            close.push("</fontsize>");
        }
        if let Some(n) = p.spacing {
            if !(-10..=50).contains(&n) {
                return Err(format!("spacing {n} is outside -10..50"));
            }
            out += &format!("<letterspace={n}>");
            close.push("</letterspace>");
        }
        for (on, open, shut) in [
            (p.bold, "<b>", "</b>"),
            (p.italic, "<i>", "</i>"),
            (p.underline, "<u>", "</u>"),
            (p.uppercase, "<uppercase>", "</uppercase>"),
        ] {
            if on {
                out += open;
                close.push(shut);
            }
        }
        out += &escape(&p.text);
        for c in close.iter().rev() {
            out += c;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(text: &str) -> Piece {
        Piece { text: text.into(), ..Default::default() }
    }
    fn col(text: &str, c: &str) -> Piece {
        Piece { text: text.into(), color: Some(c.into()), ..Default::default() }
    }

    // Shapes seen on real accounts, with placeholder text.
    #[test]
    fn a_coloured_marker_then_plain_text_is_two_pieces() {
        let n = parse("<color=0xFFA8C8E8>*</color>  main");
        assert_eq!(n.pieces, vec![col("*", "FFA8C8E8"), p("  main")]);
        assert!(n.editable && n.warnings.is_empty());
    }

    #[test]
    fn bold_inside_padded_colour_keeps_the_inner_padding() {
        let n = parse("<color=0xFFFF6F75>   <b>main</b>   </color>");
        let bold = Piece { bold: true, ..col("main", "FFFF6F75") };
        assert_eq!(n.pieces, vec![col("   ", "FFFF6F75"), bold, col("   ", "FFFF6F75")]);
    }

    #[test]
    fn spaces_outside_every_tag_are_stripped_as_the_client_does() {
        assert_eq!(parse("   pad   ").pieces, vec![p("pad")]);
        assert_eq!(parse("<b>  pad  </b>").pieces[0].text, "  pad  ");
    }

    #[test]
    fn nested_colours_are_a_stack() {
        let n = parse("<color=0xFFFF4040>out<color=0xFF40CFFF>in</color>out</color>");
        assert_eq!(n.pieces, vec![col("out", "FFFF4040"), col("in", "FF40CFFF"), col("out", "FFFF4040")]);
    }

    #[test]
    fn colour_forms_the_client_accepts() {
        assert_eq!(parse("<color=#FF40FF40>x</color>").pieces[0].color.as_deref(), Some("FF40FF40"));
        assert_eq!(parse("<color=orange>x</color>").pieces[0].color.as_deref(), Some("FFFF8000"));
        assert_eq!(parse("<color=LIGHTRED>x</color>").pieces[0].color.as_deref(), Some("FFCC3333"));
        assert_eq!(parse("<color=0xffffba4e>x</color>").pieces[0].color.as_deref(), Some("FFFFBA4E"));
    }

    #[test]
    fn a_six_digit_colour_is_alpha_zero_and_warned() {
        let n = parse("<color=0xFF4040>x</color>");
        assert_eq!(n.pieces[0].color.as_deref(), Some("00FF4040"));
        assert!(n.editable, "fixable by picking a colour");
        assert_eq!(n.warnings.len(), 1);
    }

    #[test]
    fn size_spacing_and_bius_are_authorable() {
        let n = parse("<fontsize=16><letterspace=3><b><i><u>x</u></i></b></letterspace></fontsize>");
        assert!(n.editable);
        assert_eq!(n.pieces, vec![Piece {
            text: "x".into(), size: Some(16), spacing: Some(3), bold: true, italic: true, underline: true,
            ..Default::default()
        }]);
    }

    #[test]
    fn display_only_tags_parse_but_are_not_editable() {
        let up = parse("<uppercase>up</uppercase>");
        assert!(up.pieces[0].uppercase && !up.editable);
        let font = parse("<font color=#FFFF40FF size=14>f</font>");
        assert_eq!((font.pieces[0].color.as_deref(), font.pieces[0].size), (Some("FFFF40FF"), Some(14)));
        assert!(!font.editable);
        let hint = parse("a<hint=tip>b</hint>");
        assert!(hint.pieces[1].hidden && !hint.editable && !hint.warnings.is_empty());
        assert_eq!(plain("a<hint=tip>b</hint>"), "a");
        assert!(!parse("<center>c</center>").editable);
        let bogus = parse("<bogus>x</bogus>");
        assert!(!bogus.editable && !bogus.warnings.is_empty());
    }

    #[test]
    fn unbalanced_closes_are_not_editable() {
        assert!(!parse("x</b>").editable);
        assert!(!parse("x</color>").editable);
    }

    #[test]
    fn entities_decode_and_re_escape() {
        let n = parse("&lt;esc&amp;&gt;");
        assert_eq!(n.pieces, vec![p("<esc&>")]);
        assert_eq!(format(&n.pieces).unwrap(), "&lt;esc&amp;&gt;");
    }

    #[test]
    fn a_lone_angle_bracket_is_text() {
        assert_eq!(parse("a < b").pieces, vec![p("a < b")]);
    }

    #[test]
    fn format_writes_alpha_and_a_fixed_tag_order() {
        let piece = Piece {
            text: "x".into(), color: Some("40ff40".into()), size: Some(12), bold: true, underline: true,
            ..Default::default()
        };
        assert_eq!(format(&[piece]).unwrap(), "<color=0xFF40FF40><fontsize=12><b><u>x</u></b></fontsize></color>");
        assert_eq!(format(&[p("a"), p("b"), p("")]).unwrap(), "ab");
    }

    #[test]
    fn format_refuses_what_it_cannot_write() {
        assert!(format(&[col("x", "FFF")]).is_err());
        assert!(format(&[Piece { hidden: true, ..p("x") }]).is_err());
        assert!(format(&[Piece { size: Some(0), ..p("x") }]).is_err());
    }

    #[test]
    fn format_of_parse_is_stable_and_keeps_the_pieces() {
        for raw in [
            "<color=0xFFA8C8E8>*</color>  main",
            "<color=0xFFFF6F75>   <b>main</b>   </color>",
            "<b> Exit! </b>",
            "<color=0xFFFF4040>out<color=0xFF40CFFF>in</color>out</color>",
            "<fontsize=8>s</fontsize> <letterspace=3>w</letterspace>",
            "plain",
        ] {
            let once = format(&parse(raw).pieces).unwrap();
            assert_eq!(parse(&once).pieces, parse(raw).pieces, "{raw}");
            assert_eq!(format(&parse(&once).pieces).unwrap(), once, "{raw}");
        }
    }
}
