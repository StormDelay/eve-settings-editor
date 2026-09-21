//! `layout_render`: a PNG of the window layout for an MCP image block, in
//! pure Rust — an RGB buffer, filled boxes with borders, labels from a 5×7
//! bitmap font, `png` for the encoding. Screen furniture — the Neocom bar,
//! ship HUD, fighter panel, badge and target list — paints beneath the
//! windows, from the same rules as the canvas (`layout.ts`'s `hudRects`).
//! Spec §2.1 of the all-surfaces design.

use std::collections::HashSet;

use serde::Serialize;
use settings_model::{Hud, SetTarget, WindowLayout};

use crate::mcp_filter::{self, HiddenCounts, Overrides, WindowFilter};

pub(crate) const GROUND: [u8; 3] = [24, 26, 30];
const GRID: [u8; 3] = [40, 43, 48];
const BORDER: [u8; 3] = [230, 232, 235];
const TEXT: [u8; 3] = [20, 20, 20];
const OUTLINE: [u8; 3] = [120, 124, 130];
/// Furniture: a dark veil under a grey outline, so it reads as "not a window".
pub(crate) const FURNITURE: [u8; 3] = [58, 62, 68];
const FURNITURE_TEXT: [u8; 3] = [200, 203, 208];
/// Eight muted fills, cycled per drawn window.
pub(crate) const FILLS: [[u8; 3]; 8] = [
    [120, 170, 210], [200, 150, 110], [140, 190, 140], [200, 170, 120],
    [170, 140, 200], [120, 190, 190], [210, 140, 160], [180, 180, 130],
];
const MIN_W: u32 = 320;
const MAX_W: u32 = 2048;

#[derive(Serialize)]
pub(crate) struct LegendRow {
    pub id: String, pub label: String, pub x: i64, pub y: i64, pub w: i64, pub h: i64,
    pub drawn: bool, pub stack: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct Furniture {
    pub kind: &'static str, pub label: &'static str, pub x: i64, pub y: i64, pub w: i64, pub h: i64,
}

#[derive(Serialize)]
pub(crate) struct Legend {
    pub width: u32, pub height: u32, pub reference_w: i64, pub reference_h: i64, pub scale: f64,
    pub windows: Vec<LegendRow>,
    pub furniture: Vec<Furniture>,
    pub hidden: HiddenCounts,
}

// --- furniture: `layout.ts`'s hudRects, ported. The constants are measured
// there (2026-07-28/31, three native 2560x1440 shots) and documented there;
// keep the two in step.

/// How far the ship HUD extends LEFT of its anchor, the capacitor's centre.
const SHIP_ANCHOR_LEFT: i64 = 148;
const SHIP_TOP_MARGIN: i64 = 12;
const SHIP_BOTTOM_MARGIN: i64 = 12;
const SHIPUI: (i64, i64) = (648, 176);
const FIGHTER: (i64, i64) = (467, 264);
const BADGE: (i64, i64) = (32, 32);
/// One target slot; a list is N of these in a row or a column.
const TARGET: (i64, i64) = (110, 181);
/// The neocom width a MINTED target x is a fraction of — the corpus-typical
/// one, used only when the stored value doesn't carry its own denominator.
const TARGET_MARGIN: i64 = 72;

/// A field's number: its value, else its default; None when not writable.
fn hud_num(hud: &Hud, name: &str) -> Option<f64> {
    let e = hud.entries.iter().find(|e| e.name == name)?;
    if matches!(e.set, SetTarget::Unavailable) { return None; }
    e.value.as_deref().unwrap_or(&e.default).parse().ok()
}

fn hud_flag(hud: &Hud, name: &str) -> bool {
    let Some(e) = hud.entries.iter().find(|e| e.name == name) else { return false; };
    !matches!(e.set, SetTarget::Unavailable) && e.value.as_deref().unwrap_or(&e.default) == "true"
}

/// Whether the file actually holds the field, rather than falling back.
fn stored(hud: &Hud, name: &str) -> bool {
    hud.entries.iter().any(|e| e.name == name && !matches!(e.set, SetTarget::Unavailable) && e.value.is_some())
}

/// The width the target list's stored x is a fraction OF: the screen right of
/// the neocom. EVE writes an exact `pixels / denominator`, so the denominator
/// is read back out of the value when it is UNIQUE in the plausible range; a
/// round fraction divides evenly into dozens and says nothing.
fn target_denominator(f: f64, reference_w: i64) -> i64 {
    let fallback = (reference_w - TARGET_MARGIN).max(1);
    if !f.is_finite() || f <= 0.0 || reference_w <= TARGET_MARGIN { return fallback; }
    let mut found = 0;
    let mut d = reference_w - 1;
    while d >= reference_w - 200 && d > 0 {
        let p = f * d as f64;
        if (p - p.round()).abs() < 1e-6 {
            if found != 0 { return fallback; }
            found = d;
        }
        d -= 1;
    }
    if found != 0 { found } else { fallback }
}

/// The screen furniture in data px, in the canvas's fixed order. An element
/// whose values aren't writable is omitted rather than drawn at a guess.
/// `targets` is how many locked targets to draw the list at — a view
/// preference, since no file records how many things a pilot locks.
pub(crate) fn furniture(hud: &Hud, wl: &WindowLayout, targets: u8) -> Vec<Furniture> {
    let (rw, rh) = (wl.reference_w, wl.reference_h);
    let mut out = Vec::new();
    match hud_num(hud, "neocom_width") {
        Some(n) if n > 0.0 => out.push(Furniture { kind: "neocom", label: "Neocom", x: 0, y: 0, w: n as i64, h: rh }),
        _ => {}
    }
    // The offset places the capacitor wheel's centre at `rw/2 + offset`; the
    // element extends SHIP_ANCHOR_LEFT left of that and the rest to the right.
    if let Some(offset) = hud_num(hud, "ship_offset") {
        let (w, h) = SHIPUI;
        let y = if hud_flag(hud, "ship_top") { SHIP_TOP_MARGIN } else { rh - SHIP_BOTTOM_MARGIN - h };
        out.push(Furniture { kind: "shipui", label: "Ship HUD", x: (rw as f64 / 2.0 + offset).round() as i64 - SHIP_ANCHOR_LEFT, y, w, h });
    }
    if let (Some(x), Some(y)) = (hud_num(hud, "fighter_x"), hud_num(hud, "fighter_y")) {
        if hud_flag(hud, "fighter_detached") && hud_flag(hud, "fighter_shown") {
            out.push(Furniture { kind: "fighter", label: "Fighter panel", x: x as i64, y: y as i64, w: FIGHTER.0, h: FIGHTER.1 });
        }
    }
    if let (Some(x), Some(y)) = (hud_num(hud, "badge_x"), hud_num(hud, "badge_y")) {
        out.push(Furniture { kind: "badge", label: "Badge", x: x as i64, y: y as i64, w: BADGE.0, h: BADGE.1 });
    }
    // Drawn only when the fractions are actually STORED: EVE's own starting
    // position was never captured, and 0 would put the slot in a corner the
    // list has never been.
    if let (Some(fx), Some(fy)) = (hud_num(hud, "target_x"), hud_num(hud, "target_y")) {
        if stored(hud, "target_x") && stored(hud, "target_y") {
            let d = target_denominator(fx, rw);
            let (ax, ay) = (((rw - d) as f64 + fx * d as f64).round() as i64, (fy * rh as f64).round() as i64);
            let n = targets.max(1) as i64;
            let (w, h) = if hud_flag(hud, "target_horizontal") { (TARGET.0 * n, TARGET.1) } else { (TARGET.0, TARGET.1 * n) };
            // The anchor is the list's OUTER corner; the slots run toward the
            // middle of the screen, on each axis independently.
            let x = if ax > rw / 2 { ax - w } else { ax };
            let y = if ay > rh / 2 { ay - h } else { ay };
            out.push(Furniture { kind: "target", label: "Target list", x, y, w, h });
        }
    }
    out
}

struct Canvas { w: u32, h: u32, px: Vec<u8> }

impl Canvas {
    fn new(w: u32, h: u32) -> Self { Canvas { w, h, px: vec![0; (w * h * 3) as usize] } }
    fn set(&mut self, x: i64, y: i64, c: [u8; 3]) {
        if x < 0 || y < 0 || x >= self.w as i64 || y >= self.h as i64 { return; }
        let i = ((y as u32 * self.w + x as u32) * 3) as usize;
        self.px[i..i + 3].copy_from_slice(&c);
    }
    fn fill(&mut self, x: i64, y: i64, w: i64, h: i64, c: [u8; 3]) {
        for yy in y..y + h { for xx in x..x + w { self.set(xx, yy, c); } }
    }
    fn rect(&mut self, x: i64, y: i64, w: i64, h: i64, c: [u8; 3]) {
        for xx in x..x + w { self.set(xx, y, c); self.set(xx, y + h - 1, c); }
        for yy in y..y + h { self.set(x, yy, c); self.set(x + w - 1, yy, c); }
    }
    /// Draw `text` in the bitmap font at (x, y), `s` pixels per font pixel,
    /// stopping at `max_w` pixels.
    fn text(&mut self, x: i64, y: i64, text: &str, s: i64, max_w: i64, c: [u8; 3]) {
        let mut cx = x;
        for ch in text.chars() {
            if cx + 5 * s > x + max_w { break; }
            let g = glyph(ch);
            for (row, bits) in g.iter().enumerate() {
                for col in 0..5 {
                    if bits & (0b10000 >> col) != 0 {
                        self.fill(cx + col * s, y + row as i64 * s, s, s, c);
                    }
                }
            }
            cx += 6 * s;
        }
    }
}

/// 5×7 glyphs, one byte per row, bit 4 = leftmost column. Lower case maps
/// to upper; anything outside the set is `.`.
pub(crate) fn glyph(c: char) -> [u8; 7] {
    match c.to_ascii_uppercase() {
        'A' => [0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'B' => [0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E],
        'C' => [0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E],
        'D' => [0x1E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1E],
        'E' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F],
        'F' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10],
        'G' => [0x0E, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0F],
        'H' => [0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'I' => [0x0E, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E],
        'J' => [0x07, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0C],
        'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F],
        'M' => [0x11, 0x1B, 0x15, 0x15, 0x11, 0x11, 0x11],
        'N' => [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
        'O' => [0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'P' => [0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10],
        'Q' => [0x0E, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0D],
        'R' => [0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11],
        'S' => [0x0F, 0x10, 0x10, 0x0E, 0x01, 0x01, 0x1E],
        'T' => [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'V' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x0A, 0x04],
        'W' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x15, 0x0A],
        'X' => [0x11, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x11],
        'Y' => [0x11, 0x11, 0x0A, 0x04, 0x04, 0x04, 0x04],
        'Z' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1F],
        '0' => [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
        '1' => [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
        '2' => [0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F],
        '3' => [0x1F, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0E],
        '4' => [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
        '5' => [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
        '6' => [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
        '7' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
        '9' => [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C],
        '-' => [0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00],
        '_' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1F],
        '/' => [0x01, 0x01, 0x02, 0x04, 0x08, 0x10, 0x10],
        ' ' => [0x00; 7],
        _ => [0x00, 0x00, 0x00, 0x00, 0x00, 0x0C, 0x0C],
    }
}

/// The picture and its legend. `width` is clamped to 320..=2048; the height
/// follows the file's reference aspect ratio (16:9 when the file has none).
/// `f`/`o` are `layout_get`'s own view filter — a hidden window is neither
/// drawn nor listed, and contributes to `hidden` instead; the legend is what
/// a client without image support reads, so it must agree with the picture.
/// `hud` adds the screen furniture, beneath the windows and outside the
/// filter, as the canvas draws it.
pub(crate) fn layout_png(wl: &WindowLayout, hud: Option<&Hud>, width: u32, f: &WindowFilter, o: &Overrides) -> (Vec<u8>, Legend) {
    let width = width.clamp(MIN_W, MAX_W);
    let (rw, rh) = if wl.reference_w > 0 && wl.reference_h > 0 { (wl.reference_w, wl.reference_h) } else { (1920, 1080) };
    let scale = width as f64 / rw as f64;
    // A pathological reference_h (or a near-zero reference_w blowing up scale)
    // must not overflow the `width * height * 3` buffer arithmetic below.
    let height = ((rh as f64 * scale).round() as u32).clamp(1, MAX_W * 2);
    let mut c = Canvas::new(width, height);
    c.fill(0, 0, width as i64, height as i64, GROUND);
    for i in 1..10 {
        let gx = (width as i64 * i) / 10;
        let gy = (height as i64 * i) / 10;
        c.fill(gx, 0, 1, height as i64, GRID);
        c.fill(0, gy, width as i64, 1, GRID);
    }
    let furniture = hud.map(|h| furniture(h, wl, o.targets)).unwrap_or_default();
    for fu in &furniture {
        let (x, y, bw, bh) = ((fu.x as f64 * scale) as i64, (fu.y as f64 * scale) as i64, ((fu.w as f64 * scale) as i64).max(2), ((fu.h as f64 * scale) as i64).max(2));
        c.fill(x, y, bw, bh, FURNITURE);
        c.rect(x, y, bw, bh, OUTLINE);
        if bh >= 11 { c.text(x + 3, y + 3, fu.label, 1, bw - 6, FURNITURE_TEXT); }
    }

    // A stack draws once, at its anchor — the container when it is open with
    // geometry, else the frontmost open member (`windows.rs`'s anchor rule) —
    // with its members' labels stacked. Every window in the stack, container
    // included, carries a `StackRef`, so that is the key.
    let stack_of = |w: &settings_model::WindowRect| {
        w.stack.as_ref().and_then(|r| wl.stacks.iter().find(|s| s.container_id == r.container_id))
            .map(|s| (s.container_id.clone(), s.anchor_id.clone(), s.members.clone()))
    };

    // Pass 1: which windows individually pass the filter, and why the rest
    // don't — computed once, up front, so a window is counted in `hidden`
    // exactly once regardless of what pass 2 below decides to draw.
    let mut hidden = HiddenCounts::default();
    let mut visible: HashSet<&str> = HashSet::new();
    for w in &wl.windows {
        match mcp_filter::hidden_by(w, f, o) {
            Some(reason) => hidden.add(reason),
            None => { visible.insert(w.id.as_str()); }
        }
    }
    // A stack still draws — at its anchor, regardless of the ANCHOR's own
    // visibility — as long as some window of it (container or a member)
    // individually passed the filter: mirrors the canvas's `stackUnits`
    // (layout.ts ~88-121), which never lets a filtered-out anchor hide a
    // stack whose other tabs still match. Without this, `match: "market"`
    // with `market` inside a container whose own id/label doesn't match
    // would drop the whole stack, `market` included.
    let stack_draws = |container_id: &str, members: &[String]| {
        visible.contains(container_id) || members.iter().any(|m| visible.contains(m.as_str()))
    };

    let mut rows = Vec::new();
    let mut fill_i = 0usize;
    for w in &wl.windows {
        let stack = stack_of(w);
        let is_anchor = stack.as_ref().is_some_and(|(_, anchor, _)| *anchor == w.id);
        let is_visible = visible.contains(w.id.as_str());

        // Whether this window gets a legend row at all, and — separately —
        // whether IT is the one whose geometry paints the rectangle. Only
        // the anchor (for a stack) or the window itself (when free) ever
        // paints; a non-anchor member gets a row (so it isn't silently
        // dropped when it individually matched) but never its own box, same
        // as before the stack fix.
        let (has_row, draws_rect) = match &stack {
            Some((cid, _, members)) if is_anchor => {
                let draws = stack_draws(cid, members);
                (draws, draws)
            }
            Some(_) => (is_visible, false),
            None => (is_visible, is_visible),
        };
        if !has_row { continue; }

        let Some(g) = &w.geom else {
            rows.push(LegendRow { id: w.id.clone(), label: w.label.clone(), x: 0, y: 0, w: 0, h: 0, drawn: false, stack: stack.as_ref().map(|(cid, _, _)| cid.clone()) });
            continue;
        };
        let open = w.open && w.renderable;
        let drawn = draws_rect && (open || f.include_closed);
        rows.push(LegendRow { id: w.id.clone(), label: w.label.clone(), x: g.x, y: g.y, w: g.w, h: g.h, drawn, stack: stack.as_ref().map(|(cid, _, _)| cid.clone()) });
        if !drawn { continue; }
        let (x, y, bw, bh) = ((g.x as f64 * scale) as i64, (g.y as f64 * scale) as i64, ((g.w as f64 * scale) as i64).max(2), ((g.h as f64 * scale) as i64).max(2));
        if open {
            let fill = FILLS[fill_i % FILLS.len()];
            fill_i += 1;
            c.fill(x, y, bw, bh, fill);
            c.rect(x, y, bw, bh, BORDER);
        } else {
            c.rect(x, y, bw, bh, OUTLINE);
        }
        // Labels: the window's, or its stack's VISIBLE members, top-down — a
        // member the filter hid does not get its name painted into the box.
        let labels: Vec<String> = match &stack {
            Some((_, _, members)) => members.iter()
                .filter(|m| visible.contains(m.as_str()))
                .map(|m| wl.windows.iter().find(|x| &x.id == m).map(|x| x.label.clone()).unwrap_or_else(|| m.clone()))
                .collect(),
            None => vec![w.label.clone()],
        };
        let s = if bh >= 40 && bw >= 60 { 2 } else { 1 };
        let mut ty = y + 3;
        for label in labels {
            if ty + 7 * s > y + bh - 2 { break; }
            c.text(x + 3, ty, &label, s, bw - 6, if open { TEXT } else { OUTLINE });
            ty += 8 * s;
        }
        if stack.is_some() {
            // A tab strip along the top edge marks a stack.
            c.fill(x, y, bw, 2, BORDER);
        }
    }

    let mut png = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png, width, height);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().expect("png header");
        writer.write_image_data(&c.px).expect("png data");
    }
    // Same reasoning as `layout_get`: the legend is what a client without
    // image support sees, so a window nothing drew is dead weight unless
    // asked for — `drawn` itself stays on every row that remains.
    if !f.include_closed {
        rows.retain(|r| r.drawn);
    }
    let legend = Legend { width, height, reference_w: rw, reference_h: rh, scale, windows: rows, furniture, hidden };
    (png, legend)
}

#[cfg(test)]
mod tests {
    use super::*;
    use settings_model::{window_layout as project_window_layout, WindowLayout};
    use blue_marshal::Value;

    fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }

    /// overview open at (100,200) 400×600 on 2560×1440; fitting closed; m1+m2
    /// stacked in C at (1000, 100).
    fn layout() -> WindowLayout {
        let ts = || Value::Long(vec![0u8; 8]);
        let geom = |x: i64, y: i64, w: i64, h: i64| Value::Tuple(vec![Value::Int(x), Value::Int(y), Value::Int(w), Value::Int(h), Value::Int(2560), Value::Int(1440)]);
        let doc = Value::Dict(vec![(b("windows"), Value::Dict(vec![
            (b("windowSizesAndPositions_1"), Value::Tuple(vec![ts(), Value::Dict(vec![
                (b("overview"), geom(100, 200, 400, 600)), (b("fitting"), geom(0, 0, 500, 500)),
                (b("m1"), geom(1000, 100, 300, 200)), (b("m2"), geom(1000, 100, 300, 200)), (b("C"), geom(1000, 100, 300, 200)),
            ])])),
            (b("openWindows"), Value::Tuple(vec![ts(), Value::Dict(vec![
                (b("overview"), Value::Bool(true)), (b("fitting"), Value::Bool(false)),
                (b("m1"), Value::Bool(true)), (b("m2"), Value::Bool(true)), (b("C"), Value::Bool(true)),
            ])])),
            (b("stacksWindows"), Value::Tuple(vec![ts(), Value::Dict(vec![(b("m1"), b("C")), (b("m2"), b("C"))])])),
            (b("preferredIdxInStack3"), Value::Tuple(vec![ts(), Value::Dict(vec![(b("C"), Value::Dict(vec![(b("m1"), Value::Int(0)), (b("m2"), Value::Int(1))]))])])),
        ]))]);
        project_window_layout(&doc, None)
    }

    fn decode(png: &[u8]) -> (u32, u32, Vec<u8>) {
        // png 0.18's `Decoder::new` needs `BufRead + Seek`; `&[u8]` alone
        // isn't `Seek`, so wrap it. (Deviation from the brief's snippet.)
        let decoder = png::Decoder::new(std::io::Cursor::new(png));
        let mut reader = decoder.read_info().unwrap();
        // 0.18: `output_buffer_size` returns `Option<usize>`, not `usize`.
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut buf).unwrap();
        assert_eq!(info.color_type, png::ColorType::Rgb);
        (info.width, info.height, buf[..info.buffer_size()].to_vec())
    }

    fn px(img: &(u32, u32, Vec<u8>), x: u32, y: u32) -> [u8; 3] {
        let i = ((y * img.0 + x) * 3) as usize;
        [img.2[i], img.2[i + 1], img.2[i + 2]]
    }

    /// The pre-filter default: only `include_closed` varies, exactly as
    /// these tests were written before `layout_png` took the canvas's filter.
    fn filt(include_closed: bool) -> WindowFilter {
        WindowFilter { include_closed, hide_clutter: false, env: crate::mcp_filter::Env::All, matches: None }
    }

    #[test]
    fn renders_at_the_requested_width_and_reference_aspect() {
        let (png, legend) = layout_png(&layout(), None, 640, &filt(false), &Overrides::default());
        let img = decode(&png);
        assert_eq!((img.0, img.1), (640, 360));
        assert_eq!((legend.width, legend.height), (640, 360));
        assert!((legend.scale - 0.25).abs() < 1e-9);
    }

    #[test]
    fn an_open_window_is_filled_a_closed_one_is_absent_unless_asked_and_a_stack_draws_once() {
        let wl = layout();
        let (png, legend) = layout_png(&wl, None, 640, &filt(false), &Overrides::default());
        let img = decode(&png);
        // overview's centre at scale 0.25: (100+200, 200+300) -> (75, 125).
        let inside = px(&img, 75, 125);
        assert!(FILLS.contains(&inside), "centre of overview is a fill colour, got {inside:?}");
        // fitting's box would be around (0..125, 0..125); pick a point no other window covers.
        assert_eq!(px(&img, 5, 100), GROUND, "closed window not drawn");
        let rows = &legend.windows;
        assert!(rows.iter().find(|r| r.id == "overview").unwrap().drawn);
        assert!(rows.iter().all(|r| r.drawn), "the legend lists only drawn windows by default");
        assert!(rows.iter().find(|r| r.id == "fitting").is_none(), "closed window omitted from the legend unless asked");
        assert!(rows.iter().find(|r| r.id == "m1").is_none(), "a non-anchor stack member is never drawn, so it's omitted too");
        assert!(rows.iter().find(|r| r.id == "C").unwrap().drawn, "C is open with geometry, so it is the anchor");

        let (png, legend) = layout_png(&wl, None, 640, &filt(true), &Overrides::default());
        let img = decode(&png);
        assert_ne!(px(&img, 0, 62), GROUND, "closed window drawn as an outline when asked");
        let rows = &legend.windows;
        assert!(rows.iter().find(|r| r.id == "fitting").unwrap().drawn, "include_closed draws it as an outline");
        let in_stack: Vec<&LegendRow> = rows.iter().filter(|r| r.stack.as_deref() == Some("C")).collect();
        assert_eq!(in_stack.len(), 3, "include_closed lists m1, m2 and the container C, all belonging to the stack");
        assert_eq!(in_stack.iter().filter(|r| r.drawn).count(), 1, "a stack draws once, at its anchor");
    }

    /// A filtered-out anchor must not hide a stack whose OTHER window still
    /// matches: `C` (the container/anchor) does not itself contain "m1", but
    /// `m1` does, so the stack must still draw at C's geometry and `m1` must
    /// still appear in the legend — the bug `layout_render {match: "market"}`
    /// hit when `market` sat in a container whose own id/label didn't match.
    /// `include_closed: true` here isolates that from the unrelated
    /// `!include_closed { rows.retain(|r| r.drawn) }` step (covered by the
    /// test above): a non-anchor member's row is always `drawn: false`, so
    /// without `include_closed` this test would be asserting the retain step
    /// instead of the stack fix.
    #[test]
    fn a_filtered_anchor_does_not_hide_a_stack_whose_matched_member_still_shows() {
        let wl = layout();
        let matching = WindowFilter { include_closed: true, hide_clutter: false, env: crate::mcp_filter::Env::All, matches: Some("m1".into()) };
        let (png, legend) = layout_png(&wl, None, 640, &matching, &Overrides::default());
        let img = decode(&png);
        // C's box at scale 0.25: (1000,100,300,200) -> (250,25,75,50); a point inside it.
        let inside = px(&img, 260, 40);
        assert!(FILLS.contains(&inside), "the stack still draws at C's geometry, got {inside:?}");
        assert!(legend.windows.iter().any(|r| r.id == "m1"), "the legend lists m1, even though C (the anchor) did not itself match");

        let none_match = WindowFilter { matches: Some("nothing".into()), ..matching };
        let (png, legend) = layout_png(&wl, None, 640, &none_match, &Overrides::default());
        let img = decode(&png);
        assert_eq!(px(&img, 260, 40), GROUND, "nothing in the stack matched, so it does not draw at all");
        assert_eq!(legend.hidden.matched, 5, "every one of the 5 windows in the fixture is counted once as unmatched");
    }

    #[test]
    fn the_font_covers_its_glyphs_and_dots_the_rest() {
        for c in "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_./".chars() {
            assert!(glyph(c).iter().any(|row| *row != 0), "{c} has a pattern");
        }
        assert_eq!(glyph('é'), glyph('.'));
        assert_eq!(glyph('a'), glyph('A'), "labels are uppercased");
    }

    #[test]
    fn width_is_clamped() {
        let (png, _) = layout_png(&layout(), None, 10, &filt(false), &Overrides::default());
        assert_eq!(decode(&png).0, 320);
        let (png, _) = layout_png(&layout(), None, 9999, &filt(false), &Overrides::default());
        assert_eq!(decode(&png).0, 2048);
    }

    // --- furniture: the canvas's hudRects, ported ---------------------------
    use settings_model::{Hud, HudEntry, HudKind, HudScope, SetTarget};

    /// 1354/2488 and 752/1440 on a 2560x1440 client with a 72px neocom —
    /// `layout.test.ts`'s photographed values.
    const TARGET_X: &str = "0.5442122186495176";
    const TARGET_Y: &str = "0.5222222222222223";

    fn entry(name: &str, value: Option<&str>, kind: HudKind, default: &str, set: SetTarget) -> HudEntry {
        HudEntry { name: name.into(), kind, value: value.map(Into::into), default: default.into(), scope: HudScope::Char, set }
    }

    fn set() -> SetTarget { SetTarget::Set { path: vec![] } }

    /// Every field stored, as `layout.test.ts`'s `fullHud`.
    fn full_hud() -> Hud {
        Hud { entries: vec![
            entry("ship_offset", Some("-100"), HudKind::Float, "0", set()),
            entry("fighter_x", Some("326"), HudKind::Int, "0", set()),
            entry("fighter_y", Some("54"), HudKind::Int, "0", set()),
            entry("badge_x", Some("1000"), HudKind::Int, "0", set()),
            entry("badge_y", Some("20"), HudKind::Int, "0", set()),
            entry("ship_top", Some("false"), HudKind::Bool, "false", set()),
            entry("fighter_detached", Some("true"), HudKind::Bool, "false", set()),
            entry("fighter_shown", Some("true"), HudKind::Bool, "false", set()),
            entry("neocom_width", Some("37"), HudKind::Int, "37", set()),
            entry("target_x", Some(TARGET_X), HudKind::Float, "0", set()),
            entry("target_y", Some(TARGET_Y), HudKind::Float, "0", set()),
            entry("target_horizontal", Some("false"), HudKind::Bool, "false", set()),
        ] }
    }

    fn with(mut hud: Hud, e: HudEntry) -> Hud {
        hud.entries.retain(|x| x.name != e.name);
        hud.entries.push(e);
        hud
    }

    fn find<'a>(rects: &'a [Furniture], kind: &str) -> &'a Furniture {
        rects.iter().find(|r| r.kind == kind).unwrap_or_else(|| panic!("{kind} drawn"))
    }

    #[test]
    fn furniture_places_the_five_elements_as_the_canvas_does() {
        let wl = layout();
        let rects = furniture(&full_hud(), &wl, 1);
        let kinds: Vec<&str> = rects.iter().map(|r| r.kind).collect();
        assert_eq!(kinds, ["neocom", "shipui", "fighter", "badge", "target"], "all five, in the canvas's order");
        let n = find(&rects, "neocom");
        assert_eq!((n.x, n.y, n.w, n.h), (0, 0, 37, 1440), "a full-height left bar");
        let s = find(&rects, "shipui");
        assert_eq!(s.x + 148, 1280 - 100, "the offset places the capacitor, 148px in from the left edge");
        assert_eq!(s.y, 1440 - 12 - 176, "bottom-aligned by default");
        assert_eq!((s.w, s.h), (648, 176));
        let f = find(&rects, "fighter");
        assert_eq!((f.x, f.y, f.w, f.h), (326, 54, 467, 264));
        let b = find(&rects, "badge");
        assert_eq!((b.x, b.y, b.w, b.h), (1000, 20, 32, 32));
        // Anchor (72 + 1354, 752): right of centre and below it, so the slot
        // hangs up and to the left, toward the middle of the screen.
        let t = find(&rects, "target");
        assert_eq!((t.x, t.y, t.w, t.h), (1426 - 110, 752 - 181, 110, 181));

        let four = furniture(&full_hud(), &wl, 4);
        let t4 = find(&four, "target");
        assert_eq!((t4.x, t4.y, t4.w, t4.h), (t.x, t.y - 181 * 3, 110, 181 * 4), "four slots stack down, away from the anchored corner");
        let across = furniture(&with(full_hud(), entry("target_horizontal", Some("true"), HudKind::Bool, "false", set())), &wl, 4);
        let ta = find(&across, "target");
        assert_eq!((ta.x, ta.y, ta.w, ta.h), (t.x - 110 * 3, t.y, 110 * 4, 181), "horizontal runs them leftward from a right-hand anchor");

        let top = furniture(&with(full_hud(), entry("ship_top", Some("true"), HudKind::Bool, "false", set())), &wl, 4);
        assert_eq!(find(&top, "shipui").y, 12, "ship_top clears the top edge by the measured 12px");
        // horizontal2.png: 531/2488, anchor at x=603 — left of centre, so the
        // slot hangs to the RIGHT of the anchor.
        let left = furniture(&with(full_hud(), entry("target_x", Some("0.21342443729903537"), HudKind::Float, "0", set())), &wl, 1);
        assert_eq!(find(&left, "target").x, 603);
    }

    #[test]
    fn furniture_omits_what_the_file_does_not_place() {
        let wl = layout();
        let has = |hud: Hud, kind: &str| furniture(&hud, &wl, 4).iter().any(|r| r.kind == kind);
        assert!(!has(with(full_hud(), entry("neocom_width", Some("0"), HudKind::Int, "37", set())), "neocom"), "a zero-width neocom");
        assert!(!has(with(full_hud(), entry("neocom_width", None, HudKind::Int, "37", SetTarget::Unavailable)), "neocom"), "no account file");
        assert!(!has(with(full_hud(), entry("fighter_shown", Some("false"), HudKind::Bool, "false", set())), "fighter"), "a hidden fighter UI");
        assert!(!has(with(full_hud(), entry("fighter_detached", Some("false"), HudKind::Bool, "false", set())), "fighter"), "an attached fighter UI");
        assert!(!has(with(full_hud(), entry("target_x", None, HudKind::Float, "0", set())), "target"), "an unstored target anchor — its default is a placeholder");
        assert!(has(with(full_hud(), entry("badge_x", None, HudKind::Int, "0", set())), "badge"), "the badge falls back to its default like the rest");
    }

    #[test]
    fn the_picture_paints_furniture_beneath_the_windows_and_the_legend_lists_it() {
        let wl = layout();
        let hud = full_hud();
        let (png, legend) = layout_png(&wl, Some(&hud), 640, &filt(false), &Overrides { targets: 4, ..Overrides::default() });
        let img = decode(&png);
        // The neocom at scale 0.25 is x 0..9, full height; (4, 300) is under nothing else.
        assert_eq!(px(&img, 4, 300), FURNITURE, "the neocom bar is painted in the furniture fill");
        // The fighter panel (81..198, 13..79) overlaps overview (25..125, 50..200):
        // a pixel inside both (below the label row) is the WINDOW's fill, not the furniture's.
        let inside_both = px(&img, 100, 75);
        assert!(FILLS.contains(&inside_both), "a window paints over furniture, got {inside_both:?}");
        assert_eq!(legend.furniture.len(), 5);
        let t = legend.furniture.iter().find(|r| r.kind == "target").unwrap();
        assert_eq!((t.w, t.h), (110, 181 * 4), "the legend's target list is at the preference's count");
        let (_, legend) = layout_png(&wl, None, 640, &filt(false), &Overrides::default());
        assert!(legend.furniture.is_empty(), "no HUD projection, no furniture");
    }
}
