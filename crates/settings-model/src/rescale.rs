//! Rescale a whole window layout, and the HUD positions stored in pixels, to
//! another desktop size. See docs/superpowers/specs/2026-11-21-layout-rescale-design.md.
//!
//! Every edge is computed on its own and a size is right edge minus left edge,
//! so windows that touched still touch. An edge the client treats as docked —
//! left/top at 0 or 16, right/bottom flush or 16 short of the desktop
//! (`Window.GetRegisteredPositionAndSizeByClass`) — keeps its inset unscaled,
//! because the client's test is exact and a 1px miss would turn a docked
//! window into a floating one.

use std::collections::HashMap;

use serde::Serialize;

use crate::hud::Hud;
use crate::mutate::Mutation;
use crate::window_sizes::SizeLimits;
use crate::windows::{Geom, WindowLayout};

/// The desktop the client lays windows out on: the resolution divided by the
/// UI scale, truncated (`UIRoot.UpdateSize`).
pub fn desktop_size(res_w: i64, res_h: i64, ui_scale: f64) -> (i64, i64) {
    let d = |r: i64| (r as f64 / ui_scale) as i64;
    (d(res_w), d(res_h))
}

#[derive(Debug, Serialize)]
pub struct Rescale {
    #[serde(skip)]
    pub mutations: Vec<Mutation>,
    /// `(field name, text)` for `set_hud_value`.
    #[serde(skip)]
    pub hud: Vec<(String, String)>,
    pub windows_changed: usize,
    /// Windows raised to their minimum or kept at a fixed size: where the
    /// layout may now overlap.
    pub held: Vec<String>,
}

/// One axis. `pos`/`len` are the saved values on a `src`-wide desktop.
fn axis(pos: i64, len: i64, src: i64, dst: i64, min: i64, fixed: Option<i64>) -> (i64, i64, bool) {
    let scale = |v: i64| (v as f64 * dst as f64 / src as f64).round() as i64;
    let near_docked = pos == 0 || pos == 16;
    let far = pos + len;
    let far_docked = far == src || far == src - 16;
    let lo = if near_docked { pos } else { scale(pos) };
    let hi = if far_docked { dst - (src - far) } else { scale(far) };
    let (mut size, mut held) = (hi - lo, false);
    if let Some(f) = fixed {
        (size, held) = (f, true);
    } else if size < min {
        (size, held) = (min, true);
    }
    let lo = if far_docked && !near_docked { hi - size } else { lo };
    // The minimum beats the desktop, as in `InitializeSize`; only the position
    // is clamped.
    (lo.clamp(0, (dst - size).max(0)), size, held)
}

pub fn rescale_layout(
    layout: &WindowLayout,
    hud: &Hud,
    target_w: i64,
    target_h: i64,
    limits: &dyn Fn(&str) -> Option<SizeLimits>,
) -> Rescale {
    // A stack's minimum is the largest of its members' (`stack.py`).
    let mut stack_limits: HashMap<&str, SizeLimits> = HashMap::new();
    for s in &layout.stacks {
        let ids = std::iter::once(&s.container_id).chain(&s.members);
        let mut acc: Option<SizeLimits> = None;
        for l in ids.filter_map(|id| limits(id)) {
            acc = Some(match acc {
                None => l,
                Some(a) => SizeLimits {
                    min_w: a.min_w.max(l.min_w),
                    min_h: a.min_h.max(l.min_h),
                    fixed_w: a.fixed_w.or(l.fixed_w),
                    fixed_h: a.fixed_h.or(l.fixed_h),
                },
            });
        }
        if let Some(a) = acc {
            stack_limits.insert(s.container_id.as_str(), a);
        }
    }

    let mut out = Rescale { mutations: vec![], hud: vec![], windows_changed: 0, held: vec![] };
    for w in &layout.windows {
        let Some(g) = &w.geom else { continue };
        if g.screen_w <= 0 || g.screen_h <= 0 {
            continue;
        }
        let lim = match &w.stack {
            Some(s) => stack_limits.get(s.container_id.as_str()).copied(),
            None => limits(&w.id),
        };
        let (min_w, min_h, fixed_w, fixed_h) = lim.map_or((0, 0, None, None), |l| (l.min_w, l.min_h, l.fixed_w, l.fixed_h));
        let (x, nw, held_x) = axis(g.x, g.w, g.screen_w, target_w, min_w, fixed_w);
        let (y, nh, held_y) = axis(g.y, g.h, g.screen_h, target_h, min_h, fixed_h);
        let changed = push_changes(&mut out.mutations, g, [x, y, nw, nh, target_w, target_h]);
        if changed {
            out.windows_changed += 1;
            if held_x || held_y {
                out.held.push(w.id.clone());
            }
        }
    }
    out.held.sort();
    out.held.dedup();

    if layout.reference_w > 0 && layout.reference_h > 0 {
        let sx = target_w as f64 / layout.reference_w as f64;
        let sy = target_h as f64 / layout.reference_h as f64;
        for e in &hud.entries {
            let Some(v) = e.value.as_deref().and_then(|v| v.parse::<f64>().ok()) else { continue };
            let text = match e.name.as_str() {
                "ship_offset" => format!("{}", v * sx),
                // The client clamps a stored fighter point: one 32px button
                // across, the whole 266px panel down (`fighterPoint`).
                "fighter_x" => ((v * sx).round() as i64).min(target_w - 32).to_string(),
                "fighter_y" => ((v * sy).round() as i64).min(target_h - 266).to_string(),
                "badge_x" => ((v * sx).round() as i64).to_string(),
                "badge_y" => ((v * sy).round() as i64).to_string(),
                _ => continue,
            };
            if text.parse::<f64>().ok() != Some(v) {
                out.hud.push((e.name.clone(), text));
            }
        }
    }
    out
}

/// A `set_scalar` per element that differs; true when any did.
fn push_changes(ms: &mut Vec<Mutation>, g: &Geom, next: [i64; 6]) -> bool {
    let cur = [g.x, g.y, g.w, g.h, g.screen_w, g.screen_h];
    let paths = [&g.x_path, &g.y_path, &g.w_path, &g.h_path, &g.screen_w_path, &g.screen_h_path];
    let mut any = false;
    for i in 0..6 {
        if cur[i] != next[i] {
            ms.push(Mutation::SetScalar { path: paths[i].clone(), text: next[i].to_string() });
            any = true;
        }
    }
    any
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hud::{HudEntry, HudKind, HudScope};
    use crate::path::Step;
    use crate::windows::{Geom, SetTarget, Stack, StackRef, StackRole, WindowRect};

    fn rect(i: usize, id: &str, x: i64, y: i64, w: i64, h: i64, sw: i64, sh: i64) -> WindowRect {
        let p = |k| vec![Step::DictValue(i), Step::Tuple(k)];
        WindowRect {
            id: id.into(),
            label: id.into(),
            name: None,
            open: true,
            renderable: true,
            resolution_matches: true,
            geom: Some(Geom {
                x, y, w, h, screen_w: sw, screen_h: sh,
                x_path: p(0), y_path: p(1), w_path: p(2), h_path: p(3), screen_w_path: p(4), screen_h_path: p(5),
            }),
            flags: vec![],
            stack: None,
        }
    }

    fn layout(windows: Vec<WindowRect>) -> WindowLayout {
        WindowLayout { reference_w: 2560, reference_h: 1440, windows, stacks: vec![] }
    }

    fn no_hud() -> Hud {
        Hud { entries: vec![] }
    }

    fn none(_: &str) -> Option<SizeLimits> {
        None
    }

    /// The window's rect after the mutations: its saved values, overwritten by
    /// every mutation aimed at one of its paths.
    fn after(r: &Rescale, l: &WindowLayout, id: &str) -> (i64, i64, i64, i64, i64, i64) {
        let g = l.windows.iter().find(|w| w.id == id).unwrap().geom.as_ref().unwrap();
        let mut v = [g.x, g.y, g.w, g.h, g.screen_w, g.screen_h];
        let paths = [&g.x_path, &g.y_path, &g.w_path, &g.h_path, &g.screen_w_path, &g.screen_h_path];
        for m in &r.mutations {
            let Mutation::SetScalar { path, text } = m else { panic!("only set_scalar expected") };
            if let Some(k) = paths.iter().position(|p| *p == path) {
                v[k] = text.parse().unwrap();
            }
        }
        (v[0], v[1], v[2], v[3], v[4], v[5])
    }

    fn xw(r: &Rescale, l: &WindowLayout, id: &str) -> (i64, i64) {
        let a = after(r, l, id);
        (a.0, a.2)
    }

    fn market(_: &str) -> Option<SizeLimits> {
        Some(SizeLimits { min_w: 650, min_h: 435, fixed_w: None, fixed_h: None })
    }

    #[test]
    fn desktop_size_truncates_like_the_client() {
        assert_eq!(desktop_size(2560, 1440, 1.25), (2048, 1152));
        assert_eq!(desktop_size(1920, 1080, 1.25), (1536, 864));
        assert_eq!(desktop_size(1366, 768, 1.1), (1241, 698));
    }

    #[test]
    fn left_docked_keeps_its_inset() {
        let l = layout(vec![rect(0, "a", 16, 100, 400, 300, 2560, 1440)]);
        let r = rescale_layout(&l, &no_hud(), 1920, 1080, &none);
        assert_eq!(xw(&r, &l, "a"), (16, 296));
    }

    #[test]
    fn right_docked_flush_and_inset_stay_docked() {
        let l = layout(vec![rect(0, "flush", 2160, 100, 400, 300, 2560, 1440), rect(1, "inset", 2144, 500, 400, 300, 2560, 1440)]);
        let r = rescale_layout(&l, &no_hud(), 1920, 1080, &none);
        assert_eq!(xw(&r, &l, "flush"), (1620, 300));
        assert_eq!(xw(&r, &l, "inset"), (1608, 296));
    }

    #[test]
    fn floating_neighbour_of_an_inset_dock_still_touches() {
        let l = layout(vec![rect(0, "dock", 2144, 100, 400, 300, 2560, 1440), rect(1, "n", 1744, 100, 400, 300, 2560, 1440)]);
        let r = rescale_layout(&l, &no_hud(), 1920, 1080, &none);
        let (nx, nw) = xw(&r, &l, "n");
        assert_eq!((nx, nx + nw), (1308, 1608));
        assert_eq!(xw(&r, &l, "dock").0, 1608, "the dock starts where the neighbour ends");
    }

    #[test]
    fn full_width_window_spans_the_target() {
        let l = layout(vec![rect(0, "a", 0, 100, 2560, 300, 2560, 1440)]);
        let r = rescale_layout(&l, &no_hud(), 1920, 1080, &none);
        assert_eq!(xw(&r, &l, "a"), (0, 1920));
    }

    #[test]
    fn market_floored_right_docked_grows_leftwards() {
        let l = layout(vec![rect(0, "market", 2160, 100, 400, 600, 2560, 1440)]);
        let r = rescale_layout(&l, &no_hud(), 1920, 1080, &market);
        assert_eq!(xw(&r, &l, "market"), (1270, 650));
        assert_eq!(r.held, vec!["market".to_string()]);
    }

    #[test]
    fn market_floored_floating_keeps_left_edge() {
        let l = layout(vec![rect(0, "market", 1000, 100, 400, 600, 2560, 1440)]);
        let r = rescale_layout(&l, &no_hud(), 1920, 1080, &market);
        assert_eq!(xw(&r, &l, "market"), (750, 650));
    }

    #[test]
    fn floored_window_is_clamped_on_screen() {
        let l = layout(vec![rect(0, "market", 2000, 100, 400, 600, 2560, 1440)]);
        let r = rescale_layout(&l, &no_hud(), 1920, 1080, &market);
        assert_eq!(xw(&r, &l, "market"), (1270, 650));
    }

    #[test]
    fn minimum_beats_a_smaller_desktop() {
        let tall = |_: &str| Some(SizeLimits { min_w: 0, min_h: 832, fixed_w: None, fixed_h: None });
        let l = layout(vec![rect(0, "corporation", 100, 0, 800, 1000, 2560, 1440)]);
        let r = rescale_layout(&l, &no_hud(), 1024, 768, &tall);
        let a = after(&r, &l, "corporation");
        assert_eq!((a.1, a.3), (0, 832));
    }

    #[test]
    fn fixed_height_is_kept() {
        let fixed = |_: &str| Some(SizeLimits { min_w: 256, min_h: 128, fixed_w: None, fixed_h: Some(760) });
        let l = layout(vec![rect(0, "fittingWnd", 400, 300, 1000, 760, 2560, 1440)]);
        let r = rescale_layout(&l, &no_hud(), 1920, 1080, &fixed);
        assert_eq!(after(&r, &l, "fittingWnd").3, 760);
        assert_eq!(r.held, vec!["fittingWnd".to_string()]);
    }

    #[test]
    fn stack_members_share_the_largest_minimum() {
        let mut ws = vec![
            rect(0, "C", 0, 0, 100, 80, 2560, 1440),
            rect(1, "journal", 0, 0, 100, 80, 2560, 1440),
            rect(2, "notepad", 0, 0, 100, 80, 2560, 1440),
        ];
        ws[0].stack = Some(StackRef { container_id: "C".into(), role: StackRole::Container });
        for w in &mut ws[1..] {
            w.stack = Some(StackRef { container_id: "C".into(), role: StackRole::Member });
        }
        let mut l = layout(ws);
        l.stacks = vec![Stack {
            container_id: "C".into(),
            container_label: "C".into(),
            anchor_id: "C".into(),
            members: vec!["journal".into(), "notepad".into()],
        }];
        let journal = |id: &str| (id == "journal").then_some(SizeLimits { min_w: 465, min_h: 300, fixed_w: None, fixed_h: None });
        let r = rescale_layout(&l, &no_hud(), 1920, 1080, &journal);
        for id in ["C", "journal", "notepad"] {
            assert_eq!(after(&r, &l, id), (0, 0, 465, 300, 1920, 1080), "{id}");
        }
    }

    #[test]
    fn stale_resolution_window_converts_from_its_own_size() {
        let l = layout(vec![rect(0, "stale", 10, 10, 300, 300, 1920, 1080), rect(1, "big", 100, 200, 400, 600, 2560, 1440)]);
        let r = rescale_layout(&l, &no_hud(), 1920, 1080, &none);
        assert_eq!(after(&r, &l, "stale"), (10, 10, 300, 300, 1920, 1080));
        assert_eq!(r.windows_changed, 1, "only the window saved at another size changed");
    }

    #[test]
    fn already_at_target_is_a_no_op() {
        let l = layout(vec![rect(0, "a", 10, 10, 300, 300, 1920, 1080), rect(1, "b", 16, 0, 300, 300, 1920, 1080)]);
        let r = rescale_layout(&l, &no_hud(), 1920, 1080, &none);
        assert!(r.mutations.is_empty());
        assert_eq!(r.windows_changed, 0);
    }

    #[test]
    fn zero_resolution_window_is_skipped() {
        let l = layout(vec![rect(0, "zero", 10, 10, 300, 300, 0, 0)]);
        let r = rescale_layout(&l, &no_hud(), 1920, 1080, &none);
        assert!(r.mutations.is_empty());
    }

    #[test]
    fn every_rewritten_tuple_is_restamped() {
        let l = layout(vec![rect(0, "a", 100, 200, 400, 600, 2560, 1440)]);
        let r = rescale_layout(&l, &no_hud(), 1920, 1080, &none);
        assert_eq!(after(&r, &l, "a"), (75, 150, 300, 450, 1920, 1080));
    }

    fn entry(name: &str, kind: HudKind, value: Option<&str>) -> HudEntry {
        HudEntry { name: name.into(), kind, value: value.map(Into::into), default: "0".into(), scope: HudScope::Char, set: SetTarget::Unavailable }
    }

    #[test]
    fn ship_offset_scales_and_stays_float() {
        let hud = Hud {
            entries: vec![
                entry("ship_offset", HudKind::Float, Some("-120.5")),
                entry("fighter_x", HudKind::Int, Some("1000")),
                entry("fighter_y", HudKind::Int, Some("700")),
                entry("badge_x", HudKind::Int, Some("16")),
                entry("badge_y", HudKind::Int, Some("16")),
                entry("target_x", HudKind::Float, Some("0.5")),
                entry("neocom_width", HudKind::Int, Some("48")),
            ],
        };
        let r = rescale_layout(&layout(vec![]), &hud, 1920, 1080, &none);
        let got = |n: &str| r.hud.iter().find(|(k, _)| k == n).map(|(_, v)| v.as_str());
        assert_eq!(got("ship_offset"), Some("-90.375"));
        assert_eq!((got("fighter_x"), got("fighter_y")), (Some("750"), Some("525")));
        assert_eq!((got("badge_x"), got("badge_y")), (Some("12"), Some("12")));
        assert_eq!(got("target_x"), None, "fractions are already resolution-free");
        assert_eq!(got("neocom_width"), None);
    }

    #[test]
    fn absent_hud_values_stay_absent() {
        let hud = Hud { entries: vec![entry("ship_offset", HudKind::Float, None), entry("fighter_x", HudKind::Int, None)] };
        let r = rescale_layout(&layout(vec![]), &hud, 1920, 1080, &none);
        assert!(r.hud.is_empty());
    }
}
