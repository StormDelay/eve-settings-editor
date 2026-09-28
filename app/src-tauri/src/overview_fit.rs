//! How an overview tab's columns come out in game: what each header and each
//! value SHOWS at the current widths, and the narrowest width that shows it
//! whole. One implementation for both consumers — the width preview draws from
//! it (with `masks`), the `overview_column_fit` MCP tool reports it.
//!
//! Every rule is the client's own, DECODED from its code (docs/format-notes.md,
//! "Overview column rendering") and checked against live captures; the text is
//! measured by `client_env`'s FreeType, which matches the game pixel for pixel.
//! Units are EVE UI pixels (at UI scale 1.0, screen pixels).

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

use crate::client_env::{self, Shaped};
use crate::ops::{self, AppState, ErrDto, Slot};

/// COLUMNMARGIN: text starts 8 in, and a row loses 8 more on the right.
const MARGIN: i64 = 8;
/// A header label is `width - 12` wide (8 in, 4 short of the edge).
const HEADER_PAD: i64 = 12;
/// A sorted header's arrow margin: its text fades out by `width - 28`.
const SORTED_PAD: i64 = 28;
/// SCROLL_COLUMN_FADEWIDTH: the longest header fade.
const FADE: f64 = 20.0;
/// COLUMNMINSIZE / COLUMNMINDEFAULTSIZE / the label padding a defaulted column grows by.
const MIN_WIDTH: i64 = 24;
const DEFAULT_WIDTH: i64 = 80;
const LABEL_PAD: i64 = 24;
/// Row box inside the window: MEASURED, a 446-wide window has 426 px rows.
const WINDOW_INSET: f64 = 20.0;
/// A header narrower than this hides its label (SortHeaders.CreateColumns).
const HEADER_HIDE_BELOW: i64 = 32;

const RIGHT_ALIGNED: [&str; 6] = ["DISTANCE", "SIZE", "VELOCITY", "RADIALVELOCITY", "ANGULARVELOCITY", "TRANSVERSALVELOCITY"];

/// The width EVE draws a column at, and where that number comes from.
/// `header_pen` only matters when no width is stored: the column then widens
/// to fit its header (except the two speed columns that opt out).
pub fn effective_width(name: &str, stored: Option<i64>, header_pen: i64) -> (i64, &'static str) {
    if name == "ICON" {
        return (22, "fixed");
    }
    if let Some(w) = stored {
        return (w.max(MIN_WIDTH), "stored");
    }
    let base = match name {
        "NAME" | "TYPE" => 112,
        "VELOCITY" | "ANGULARVELOCITY" => 58,
        _ => DEFAULT_WIDTH,
    };
    let w = if matches!(name, "VELOCITY" | "ANGULARVELOCITY") { base } else { base.max(header_pen + LABEL_PAD) };
    (w.max(MIN_WIDTH), "default")
}

/// The header text EVE shows: the column label plus its unit, when it has one
/// (overviewColumns.GetColumnLabel, `addFormatUnit=True`; strings from the
/// client's en-us localization).
pub fn header_label(name: &str, label: &str) -> String {
    match name {
        "VELOCITY" | "RADIALVELOCITY" | "TRANSVERSALVELOCITY" => format!("{label} (m/s)"),
        "ANGULARVELOCITY" => format!("{label} (deg/s)"),
        _ => label.to_string(),
    }
}

/// `[EVE_SMALL_FONTSIZE, EVE_MEDIUM_FONTSIZE]` per `clientFontSize` option,
/// and the option's in-game name. Rows use the medium preset (small with the
/// overview's "Use small font"); headers always use small.
fn font_px(option: i64) -> ([u32; 2], &'static str) {
    match option {
        4 => ([10, 12], "Tiny"),
        1 => ([11, 13], "Small"),
        3 => ([13, 15], "Large"),
        5 => ([14, 16], "Huge"),
        _ => ([12, 14], "Medium"),
    }
}

/// How many leading characters survive a `limit`: EVE's measurer adds whole
/// glyphs while the pen stays within it. Returns (characters, pen).
fn fit(pens: &[i32], limit: f64) -> (usize, i32) {
    let n = pens.iter().take_while(|&&p| p as f64 <= limit).count();
    (n, if n == 0 { 0 } else { pens[n - 1] })
}

/// Sample rows, one data case each, in EVE's own formatting: grouped digits;
/// speeds carry no unit (the header does); a stationary object shows "-";
/// distances are m under 10 km, grouped km under 10,000,000 km, then AU.
pub fn default_rows() -> Vec<BTreeMap<String, String>> {
    let row = |pairs: &[(&str, &str)]| pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    vec![
        // a fast hostile at close range: the widest speeds
        row(&[("DISTANCE", "12,345 m"), ("NAME", "Sample Pilot Name"), ("TYPE", "Vindicator"), ("CORPORATION", "[CORP.]"),
              ("ALLIANCE", "[ALLY]"), ("FACTION", "Serpentis"), ("MILITIA", "Gallente Federation"), ("SIZE", "1,250 m"),
              ("VELOCITY", "12,345"), ("RADIALVELOCITY", "-1,234"), ("TRANSVERSALVELOCITY", "12,345"), ("ANGULARVELOCITY", "2.5"), ("TAG", "A1")]),
        // the plain hull, then its Navy Issue: does the cut hide "Navy"?
        row(&[("DISTANCE", "2,847 km"), ("NAME", "Short"), ("TYPE", "Megathron"), ("CORPORATION", "[WWWWW]"),
              ("ALLIANCE", "[ALLY.]"), ("SIZE", "470 m"), ("VELOCITY", "187"), ("RADIALVELOCITY", "12"),
              ("TRANSVERSALVELOCITY", "185"), ("ANGULARVELOCITY", "0.03"), ("TAG", "10")]),
        row(&[("DISTANCE", "31 km"), ("NAME", "Another Pilot"), ("TYPE", "Megathron Navy Issue"), ("CORPORATION", "[CORP.]"),
              ("SIZE", "470 m"), ("VELOCITY", "1,203"), ("RADIALVELOCITY", "-640"), ("TRANSVERSALVELOCITY", "1,010"),
              ("ANGULARVELOCITY", "1.9")]),
        // a structure across the system: the longest km figure
        row(&[("DISTANCE", "9,999,999 km"), ("NAME", "Jita IV - Moon 4 - Caldari Navy Assembly Plant"), ("TYPE", "Caldari Navy Assembly Plant"),
              ("CORPORATION", "[CN]"), ("SIZE", "50,000 m"), ("VELOCITY", "-"), ("RADIALVELOCITY", "-"),
              ("TRANSVERSALVELOCITY", "-"), ("ANGULARVELOCITY", "-")]),
        // a gate in AU
        row(&[("DISTANCE", "143.8 AU"), ("NAME", "Perimeter II - Moon 1"), ("TYPE", "Stargate (Caldari System)"),
              ("VELOCITY", "-"), ("RADIALVELOCITY", "-"), ("TRANSVERSALVELOCITY", "-"), ("ANGULARVELOCITY", "-")]),
    ]
}

#[derive(Debug, Deserialize, Default)]
pub struct FitReq {
    pub tab: i64,
    /// Values to lay out, one map per row; None = `default_rows()`.
    #[serde(default)]
    pub rows: Option<Vec<BTreeMap<String, String>>>,
    /// Hypothetical widths, by column name — nothing is written.
    #[serde(default)]
    pub widths: BTreeMap<String, i64>,
    /// The column carrying the sort arrow; default DISTANCE, EVE's default.
    #[serde(default)]
    pub sorted_by: Option<String>,
    /// Include each label's rendered pixels (the preview draws them).
    #[serde(default)]
    pub masks: bool,
}

#[derive(Debug, Serialize)]
pub struct Fit {
    /// The in-game font size setting ("Tiny" .. "Huge") and the UI scale.
    pub font_size: &'static str,
    pub ui_scale: f64,
    pub use_small_text: bool,
    pub sorted_by: String,
    /// The window's row width, when the character file places this tab's window.
    pub row_width: Option<f64>,
    pub total_width: i64,
    pub rows: Vec<BTreeMap<String, String>>,
    pub columns: Vec<FitColumn>,
}

#[derive(Debug, Serialize)]
pub struct FitColumn {
    pub name: String,
    pub x: i64,
    pub width: i64,
    /// "stored", "default" (no width in the file) or "fixed" (ICON).
    pub width_source: &'static str,
    pub right_aligned: bool,
    /// Part of the column lies past the window's right edge.
    pub past_window_edge: bool,
    pub header: FitLabel,
    /// One per row; an empty value lays out as an empty label.
    pub cells: Vec<FitLabel>,
    /// The narrowest width showing every value whole (never below 24).
    pub min_width: i64,
}

#[derive(Debug, Serialize)]
pub struct FitLabel {
    pub text: String,
    pub shown: String,
    pub whole: bool,
    /// The narrowest column width showing this text whole.
    pub min_width: i64,
    pub text_x: f64,
    pub clip_w: f64,
    pub fade_w: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mask: Option<Shaped>,
}

pub fn overview_fit(state: &AppState, req: FitReq) -> Result<Fit, ErrDto> {
    let oc = ops::overview_columns(state)?;
    let tab = oc.tabs.iter().find(|t| t.index == req.tab).ok_or_else(|| ErrDto::new("bad_tab", format!("no tab {}", req.tab)))?;
    let window = oc.windows.iter().find(|w| w.tab_indices.contains(&req.tab)).map(|w| w.index).unwrap_or(0);
    let use_small_text = oc.appearance.bools.iter().any(|(k, v)| k == "useSmallText" && *v);

    let settings = client_env::settings_dir(state)?;
    let yaml = std::fs::read_to_string(settings.join("core_public__.yaml")).unwrap_or_default();
    let (scale, option) = client_env::scale_and_font(&yaml);
    let ([small, medium], font_name) = font_px(option);
    let (row_px, head_px) = (((if use_small_text { small } else { medium }) as f64 * scale).round() as u32, (small as f64 * scale).round() as u32);

    let row_width = ops::window_layout(state, Slot::Char).ok().and_then(|l| {
        let id = if window == 0 { "overview".to_string() } else { format!("overview_{window}") };
        l.windows.into_iter().find(|w| w.id == id).and_then(|w| w.geom).map(|g| g.w as f64 / scale - WINDOW_INSET)
    });

    let (regular, expanded) = client_env::fonts(&settings).map_err(|e| ErrDto::new("fonts", format!("EVE's fonts couldn't be read, so nothing can be measured: {e}")))?;
    let lib = freetype::Library::init().map_err(|e| ErrDto::new("freetype", e.to_string()))?;
    let reg = lib.new_memory_face(regular, 0).map_err(|e| ErrDto::new("freetype", e.to_string()))?;
    let exp = lib.new_memory_face(expanded, 0).map_err(|e| ErrDto::new("freetype", e.to_string()))?;
    let mut m = Measure { reg: &reg, exp: &exp, row_px, head_px, scale, cache: HashMap::new() };

    let rows = req.rows.unwrap_or_else(default_rows);
    let sorted_by = req.sorted_by.unwrap_or_else(|| "DISTANCE".into());
    let cols: Vec<(String, String, Option<i64>)> = tab.columns.iter().filter(|c| c.visible)
        .map(|c| (c.name.clone(), c.label.clone(), req.widths.get(&c.name).copied().or(c.width)))
        .collect();
    let columns = fit_columns(&cols, &rows, &sorted_by, row_width, req.masks, &mut m)?;
    let total_width = columns.iter().map(|c| c.width).sum();
    Ok(Fit { font_size: font_name, ui_scale: scale, use_small_text, sorted_by, row_width, total_width, rows, columns })
}

/// EVE's text engine at the two sizes a tab uses, one layout per distinct string.
struct Measure<'a> {
    reg: &'a freetype::Face,
    exp: &'a freetype::Face,
    row_px: u32,
    head_px: u32,
    scale: f64,
    cache: HashMap<(bool, String), Shaped>,
}

impl Measure<'_> {
    fn shape(&mut self, header: bool, text: &str) -> Result<Shaped, ErrDto> {
        if let Some(s) = self.cache.get(&(header, text.to_string())) {
            return Ok(s.clone());
        }
        let s = if header { client_env::layout(self.exp, self.head_px, text) } else { client_env::layout(self.reg, self.row_px, text) }
            .map_err(|e| ErrDto::new("freetype", e))?;
        self.cache.insert((header, text.to_string()), s.clone());
        Ok(s)
    }
}

/// The columns `(name, label, width)` left to right, each header and value
/// laid out and cut the way the client does it.
fn fit_columns(
    cols: &[(String, String, Option<i64>)],
    rows: &[BTreeMap<String, String>],
    sorted_by: &str,
    row_width: Option<f64>,
    masks: bool,
    m: &mut Measure,
) -> Result<Vec<FitColumn>, ErrDto> {
    // Pens are device pixels; everything else is UI pixels.
    let scale = m.scale;
    let ui = |p: i32| p as f64 / scale;
    let ceil_ui = |p: i32| (p as f64 / scale).ceil() as i64;
    let mut x = 0i64;
    let mut columns = Vec::new();
    for (name, col_label, stored) in cols {
        let text = header_label(name, col_label);
        let hs = m.shape(true, &text)?;
        let full_pen = *hs.pens.last().unwrap_or(&0);
        let (w, source) = effective_width(name, *stored, ceil_ui(full_pen));
        let sorted = name == sorted_by;

        // Header: whole glyphs within width - 12; the sorted one fades out by width - 28.
        let header = if w < HEADER_HIDE_BELOW {
            label(text.clone(), 0, false, 0, MARGIN as f64, 0.0, 0.0, None)
        } else {
            let (mut n, pen) = fit(&hs.pens, (w - HEADER_PAD) as f64 * scale);
            let fade_end = (w - SORTED_PAD) as f64;
            let (clip, fade) = if sorted && ui(pen) > fade_end { (fade_end - 0.5, (ui(pen) - fade_end).min(FADE)) } else { (ui(pen), 0.0) };
            if fade > 0.0 {
                n = fit(&hs.pens, fade_end * scale).0; // what is visible, the last glyph fading
            }
            let need = ceil_ui(full_pen) + if sorted { SORTED_PAD } else { HEADER_PAD };
            let whole = n == hs.pens.len() && fade == 0.0;
            label(text, n, whole, need.max(HEADER_HIDE_BELOW), (x + MARGIN) as f64, clip, fade, masks.then_some(hs))
        };

        // Cells: whole glyphs within width - 16, right-aligned columns flush right.
        let right = RIGHT_ALIGNED.contains(&name.as_str());
        let inner = (w - 2 * MARGIN) as f64;
        let mut cells = Vec::new();
        for r in rows {
            let v = r.get(name).cloned().unwrap_or_default();
            if name == "ICON" || v.is_empty() {
                cells.push(label(v, 0, true, 0, (x + MARGIN) as f64, 0.0, 0.0, None));
                continue;
            }
            let s = m.shape(false, &v)?;
            let (n, pen) = fit(&s.pens, inner * scale);
            let shown_w = ui(pen);
            let text_x = (x + MARGIN) as f64 + if right { inner - shown_w } else { 0.0 };
            let need = ceil_ui(*s.pens.last().unwrap_or(&0)) + 2 * MARGIN;
            cells.push(label(v, n, n == s.pens.len(), need.max(MIN_WIDTH), text_x, shown_w, 0.0, masks.then_some(s)));
        }
        let min_width = if source == "fixed" { w } else { cells.iter().map(|c| c.min_width).max().unwrap_or(0).max(MIN_WIDTH) };
        columns.push(FitColumn {
            name: name.clone(),
            x,
            width: w,
            width_source: source,
            right_aligned: right,
            past_window_edge: row_width.is_some_and(|rw| (x + w) as f64 > rw),
            header,
            cells,
            min_width,
        });
        x += w;
    }
    Ok(columns)
}

#[allow(clippy::too_many_arguments)]
fn label(text: String, n: usize, whole: bool, min_width: i64, text_x: f64, clip_w: f64, fade_w: f64, mask: Option<Shaped>) -> FitLabel {
    let shown = text.chars().take(n).collect();
    FitLabel { text, shown, whole, min_width, text_x, clip_w, fade_w, mask }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths_follow_the_client_defaults() {
        assert_eq!(effective_width("VELOCITY", Some(45), 0), (45, "stored"));
        assert_eq!(effective_width("VELOCITY", Some(10), 0), (24, "stored"), "COLUMNMINSIZE floor");
        assert_eq!(effective_width("ICON", Some(90), 0), (22, "fixed"));
        assert_eq!(effective_width("VELOCITY", None, 200), (58, "default"), "speed columns never widen for the header");
        assert_eq!(effective_width("NAME", None, 30), (112, "default"));
        assert_eq!(effective_width("CORPORATION", None, 30), (80, "default"));
        assert_eq!(effective_width("CORPORATION", None, 70), (94, "default"), "header + 24");
    }

    #[test]
    fn headers_carry_their_unit() {
        assert_eq!(header_label("VELOCITY", "Velocity"), "Velocity (m/s)");
        assert_eq!(header_label("ANGULARVELOCITY", "Angular Velocity"), "Angular Velocity (deg/s)");
        assert_eq!(header_label("TYPE", "Type"), "Type");
    }

    #[test]
    fn fit_keeps_whole_glyphs_within_the_limit() {
        // "Caldari Tra" in a 71 px Type column: the 11th pen is 55 of 55; the
        // "d" would end at 61 (a live capture, 2026-09-28).
        let pens = [7, 13, 17, 23, 26, 30, 34, 37, 43, 47, 55, 61, 68];
        assert_eq!(fit(&pens, 55.0), (11, 55));
        assert_eq!(fit(&pens, 3.0), (0, 0));
        assert_eq!(fit(&pens, 1000.0), (13, 68));
    }

    /// Every cut in a live capture (2026-09-28, Tiny font, 100%), through the
    /// real layout with EVE's fonts. Needs an EVE install, so it passes
    /// vacuously elsewhere. Only non-identifying strings from the capture.
    #[test]
    fn reproduces_a_live_capture() {
        let Some((regular, expanded)) = client_env::installed_fonts() else {
            eprintln!("EVE's fonts not installed here - skipping");
            return;
        };
        let lib = freetype::Library::init().unwrap();
        let (reg, exp) = (lib.new_memory_face(regular, 0).unwrap(), lib.new_memory_face(expanded, 0).unwrap());
        let mut m = Measure { reg: &reg, exp: &exp, row_px: 12, head_px: 10, scale: 1.0, cache: HashMap::new() };
        let col = |n: &str, l: &str, w: i64| (n.to_string(), l.to_string(), Some(w));
        let cols = [
            col("DISTANCE", "Distance", 65), col("TYPE", "Type", 71), col("VELOCITY", "Velocity", 44),
            col("TRANSVERSALVELOCITY", "Transversal Velocity", 60), col("ANGULARVELOCITY", "Angular Velocity", 48),
        ];
        let row = |t: &str| BTreeMap::from([("TYPE".to_string(), t.to_string())]);
        let fit = fit_columns(&cols, &[row("Caldari Trade Post"), row("Sun M0 (Orange radiant)")], "DISTANCE", None, false, &mut m).unwrap();
        let [dist, ty, vel, tr, ang] = [0, 1, 2, 3, 4].map(|i| &fit[i]);
        assert_eq!(dist.header.shown, "Distanc", "sorted: fades out by width - 28");
        assert!((dist.header.fade_w - 6.0).abs() < 1e-9);
        assert_eq!(vel.header.shown, "Veloci");
        assert_eq!(tr.header.shown, "Transvers");
        assert_eq!(ang.header.shown, "Angula");
        assert_eq!(ty.cells[0].shown, "Caldari Tra");
        assert_eq!(ty.cells[1].shown, "Sun M0 (");
        assert!(!ty.cells[0].whole && ty.cells[0].min_width > 71, "the width that cuts is below min_width");
    }
}
