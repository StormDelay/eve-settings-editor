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
              ("ALLIANCE", "[-D0-]"), ("SIZE", "470 m"), ("VELOCITY", "187"), ("RADIALVELOCITY", "12"),
              ("TRANSVERSALVELOCITY", "185"), ("ANGULARVELOCITY", "0.03"), ("TAG", "10")]),
        row(&[("DISTANCE", "31 km"), ("NAME", "Another Pilot"), ("TYPE", "Megathron Navy Issue"), ("CORPORATION", "[CORP.]"),
              ("SIZE", "470 m"), ("VELOCITY", "1,203"), ("RADIALVELOCITY", "-640"), ("TRANSVERSALVELOCITY", "1,010"),
              ("ANGULARVELOCITY", "1.9")]),
        // a structure across the system: the longest km figure
        row(&[("DISTANCE", "9,999,999 km"), ("NAME", "Jita IV - Moon 4 - Caldari Navy Assembly Plant"), ("TYPE", "Caldari Navy Assembly Plant"),
              ("CORPORATION", "[CN]"), ("SIZE", "50,000 m"), ("VELOCITY", "-"), ("RADIALVELOCITY", "-"),
              ("TRANSVERSALVELOCITY", "-"), ("ANGULARVELOCITY", "-")]),
        // a gate in AU
        row(&[("DISTANCE", "143.8 AU"), ("NAME", "Itamo VIII - Moon 12"), ("TYPE", "Stargate (Caldari System)"),
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

    // FreeType, one layout per distinct string.
    let (regular, expanded) = client_env::fonts(&settings).map_err(|e| ErrDto::new("fonts", format!("EVE's fonts couldn't be read, so nothing can be measured: {e}")))?;
    let lib = freetype::Library::init().map_err(|e| ErrDto::new("freetype", e.to_string()))?;
    let reg = lib.new_memory_face(regular, 0).map_err(|e| ErrDto::new("freetype", e.to_string()))?;
    let exp = lib.new_memory_face(expanded, 0).map_err(|e| ErrDto::new("freetype", e.to_string()))?;
    let mut cache: HashMap<(bool, String), Shaped> = HashMap::new();
    let mut shape = |header: bool, text: &str| -> Result<Shaped, ErrDto> {
        if let Some(s) = cache.get(&(header, text.to_string())) {
            return Ok(s.clone());
        }
        let s = if header { client_env::layout(&exp, head_px, text) } else { client_env::layout(&reg, row_px, text) }
            .map_err(|e| ErrDto::new("freetype", e))?;
        cache.insert((header, text.to_string()), s.clone());
        Ok(s)
    };
    // Pens are device pixels; everything else is UI pixels.
    let ui = |p: i32| p as f64 / scale;
    let ceil_ui = |p: i32| (p as f64 / scale).ceil() as i64;

    let rows = req.rows.unwrap_or_else(default_rows);
    let sorted_by = req.sorted_by.unwrap_or_else(|| "DISTANCE".into());
    let mut x = 0i64;
    let mut columns = Vec::new();
    for c in tab.columns.iter().filter(|c| c.visible) {
        let text = header_label(&c.name, &c.label);
        let hs = shape(true, &text)?;
        let full_pen = *hs.pens.last().unwrap_or(&0);
        let stored = req.widths.get(&c.name).copied().or(c.width);
        let (w, source) = effective_width(&c.name, stored, ceil_ui(full_pen));
        let sorted = c.name == sorted_by;

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
            label(text, n, whole, need.max(HEADER_HIDE_BELOW), (x + MARGIN) as f64, clip, fade, req.masks.then_some(hs))
        };

        // Cells: whole glyphs within width - 16, right-aligned columns flush right.
        let right = RIGHT_ALIGNED.contains(&c.name.as_str());
        let inner = (w - 2 * MARGIN) as f64;
        let mut cells = Vec::new();
        for r in &rows {
            let v = r.get(&c.name).cloned().unwrap_or_default();
            if c.name == "ICON" || v.is_empty() {
                cells.push(label(v, 0, true, 0, (x + MARGIN) as f64, 0.0, 0.0, None));
                continue;
            }
            let s = shape(false, &v)?;
            let (n, pen) = fit(&s.pens, inner * scale);
            let shown_w = ui(pen);
            let text_x = (x + MARGIN) as f64 + if right { inner - shown_w } else { 0.0 };
            let need = ceil_ui(*s.pens.last().unwrap_or(&0)) + 2 * MARGIN;
            cells.push(label(v, n, n == s.pens.len(), need.max(MIN_WIDTH), text_x, shown_w, 0.0, req.masks.then_some(s)));
        }
        let min_width = if source == "fixed" { w } else { cells.iter().map(|c| c.min_width).max().unwrap_or(0).max(MIN_WIDTH) };
        columns.push(FitColumn {
            name: c.name.clone(),
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
    Ok(Fit { font_size: font_name, ui_scale: scale, use_small_text, sorted_by, row_width, total_width: x, rows, columns })
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
        // "d" would end at 61 (Holy Storm's overview, 2026-09-28).
        let pens = [7, 13, 17, 23, 26, 30, 34, 37, 43, 47, 55, 61, 68];
        assert_eq!(fit(&pens, 55.0), (11, 55));
        assert_eq!(fit(&pens, 3.0), (0, 0));
        assert_eq!(fit(&pens, 1000.0), (13, 68));
    }

    /// End to end against a live capture: Holy Storm's overview (2026-09-28),
    /// at the widths it was captured with. Needs that character's files and
    /// EVE's fonts, so it passes vacuously on any other machine.
    #[test]
    fn reproduces_the_holy_storm_capture() {
        let dir = std::path::PathBuf::from(std::env::var("LOCALAPPDATA").unwrap_or_default())
            .join("CCP/EVE/g_eve_shared_cache_sharedcache_tq_tranquility/settings_Default");
        let (user, char) = (dir.join("core_user_13375506.dat"), dir.join("core_char_96821229.dat"));
        let state = AppState::new();
        if !user.is_file() || !char.is_file()
            || ops::open_file(&state, Slot::User, &user.to_string_lossy()).is_err()
            || ops::open_file(&state, Slot::Char, &char.to_string_lossy()).is_err()
        {
            eprintln!("Holy Storm's files aren't here — skipping");
            return;
        }
        let widths = [("DISTANCE", 65), ("NAME", 97), ("TYPE", 71), ("VELOCITY", 44), ("TRANSVERSALVELOCITY", 60), ("ANGULARVELOCITY", 48)];
        let rows = [
            [("NAME", "Itamo VI - Moon 1"), ("TYPE", "Caldari Trade Post")],
            [("NAME", "Itamo - PUBLIC"), ("TYPE", "Sun M0 (Orange radiant)")],
        ];
        let Ok(fit) = overview_fit(&state, FitReq {
            tab: 1,
            rows: Some(rows.iter().map(|r| r.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()).collect()),
            widths: widths.iter().map(|(k, w)| (k.to_string(), *w)).collect(),
            sorted_by: None,
            masks: false,
        }) else {
            eprintln!("EVE's fonts or tab 1 aren't here — skipping");
            return;
        };
        let col = |n: &str| fit.columns.iter().find(|c| c.name == n);
        let (Some(dist), Some(name), Some(ty), Some(vel), Some(tr), Some(ang)) =
            (col("DISTANCE"), col("NAME"), col("TYPE"), col("VELOCITY"), col("TRANSVERSALVELOCITY"), col("ANGULARVELOCITY"))
        else {
            eprintln!("tab 1 no longer shows the captured columns — skipping");
            return;
        };
        assert_eq!(fit.font_size, "Tiny");
        assert_eq!(dist.header.shown, "Distanc", "sorted: fades out by width - 28");
        assert!(dist.header.fade_w > 5.9 && dist.header.fade_w < 6.1);
        assert_eq!(vel.header.shown, "Veloci");
        assert_eq!(tr.header.shown, "Transvers");
        assert_eq!(ang.header.shown, "Angula");
        assert_eq!(name.cells[0].shown, "Itamo VI - Moo");
        assert_eq!(name.cells[1].shown, "Itamo - PUBLIC");
        assert_eq!(ty.cells[0].shown, "Caldari Tra");
        assert_eq!(ty.cells[1].shown, "Sun M0 (");
        // min_width is the pen + 16: one more pixel than the width that cuts.
        assert_eq!(name.cells[1].min_width, 96, "\"Itamo - PUBLIC\" ends at 80");
    }
}
