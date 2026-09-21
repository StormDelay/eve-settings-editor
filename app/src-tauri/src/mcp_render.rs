//! `layout_render`: a PNG of the window layout for an MCP image block, in
//! pure Rust — an RGB buffer, filled boxes with borders, labels from a 5×7
//! bitmap font, `png` for the encoding. Windows and stacks only; no HUD
//! furniture, no neocom. Spec §2.1 of the all-surfaces design.

use serde::Serialize;
use settings_model::WindowLayout;

use crate::mcp_filter::{self, HiddenCounts, Overrides, WindowFilter};

pub(crate) const GROUND: [u8; 3] = [24, 26, 30];
const GRID: [u8; 3] = [40, 43, 48];
const BORDER: [u8; 3] = [230, 232, 235];
const TEXT: [u8; 3] = [20, 20, 20];
const OUTLINE: [u8; 3] = [120, 124, 130];
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
pub(crate) struct Legend {
    pub width: u32, pub height: u32, pub reference_w: i64, pub reference_h: i64, pub scale: f64,
    pub windows: Vec<LegendRow>,
    pub hidden: HiddenCounts,
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
pub(crate) fn layout_png(wl: &WindowLayout, width: u32, f: &WindowFilter, o: &Overrides) -> (Vec<u8>, Legend) {
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

    // A stack draws once, at its anchor — the container when it is open with
    // geometry, else the frontmost open member (`windows.rs`'s anchor rule) —
    // with its members' labels stacked. Every window in the stack, container
    // included, carries a `StackRef`, so that is the key.
    let stack_of = |w: &settings_model::WindowRect| {
        w.stack.as_ref().and_then(|r| wl.stacks.iter().find(|s| s.container_id == r.container_id))
            .map(|s| (s.container_id.clone(), s.anchor_id.clone(), s.members.clone()))
    };
    let mut rows = Vec::new();
    let mut hidden = HiddenCounts::default();
    let mut fill_i = 0usize;
    for w in &wl.windows {
        if let Some(reason) = mcp_filter::hidden_by(w, f, o) {
            hidden.add(reason);
            continue;
        }
        let Some(g) = &w.geom else {
            rows.push(LegendRow { id: w.id.clone(), label: w.label.clone(), x: 0, y: 0, w: 0, h: 0, drawn: false, stack: None });
            continue;
        };
        let stack = stack_of(w);
        let is_anchor_or_free = stack.as_ref().is_none_or(|(_, anchor, _)| anchor == &w.id);
        let open = w.open && w.renderable;
        let drawn = is_anchor_or_free && (open || f.include_closed);
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
        // Labels: the window's, or every member's for a stack, top-down.
        let labels: Vec<String> = match &stack {
            Some((_, _, members)) => members.iter().map(|m| wl.windows.iter().find(|x| &x.id == m).map(|x| x.label.clone()).unwrap_or_else(|| m.clone())).collect(),
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
    let legend = Legend { width, height, reference_w: rw, reference_h: rh, scale, windows: rows, hidden };
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
        let (png, legend) = layout_png(&layout(), 640, &filt(false), &Overrides::default());
        let img = decode(&png);
        assert_eq!((img.0, img.1), (640, 360));
        assert_eq!((legend.width, legend.height), (640, 360));
        assert!((legend.scale - 0.25).abs() < 1e-9);
    }

    #[test]
    fn an_open_window_is_filled_a_closed_one_is_absent_unless_asked_and_a_stack_draws_once() {
        let wl = layout();
        let (png, legend) = layout_png(&wl, 640, &filt(false), &Overrides::default());
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

        let (png, legend) = layout_png(&wl, 640, &filt(true), &Overrides::default());
        let img = decode(&png);
        assert_ne!(px(&img, 0, 62), GROUND, "closed window drawn as an outline when asked");
        let rows = &legend.windows;
        assert!(rows.iter().find(|r| r.id == "fitting").unwrap().drawn, "include_closed draws it as an outline");
        let in_stack: Vec<&LegendRow> = rows.iter().filter(|r| r.stack.as_deref() == Some("C")).collect();
        assert_eq!(in_stack.len(), 3, "include_closed lists m1, m2 and the container C, all belonging to the stack");
        assert_eq!(in_stack.iter().filter(|r| r.drawn).count(), 1, "a stack draws once, at its anchor");
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
        let (png, _) = layout_png(&layout(), 10, &filt(false), &Overrides::default());
        assert_eq!(decode(&png).0, 320);
        let (png, _) = layout_png(&layout(), 9999, &filt(false), &Overrides::default());
        assert_eq!(decode(&png).0, 2048);
    }
}
