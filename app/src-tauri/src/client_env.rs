//! What the overview width preview needs from OUTSIDE the settings files: the
//! client's UI scale and font-size option, and EVE's own text rendering.
//!
//! Both come from paths the open account file already implies. Its folder holds
//! `core_public__.yaml` (machine-scope settings), and that folder's parent is
//! named after the shared cache the client runs from —
//! `g_eve_shared_cache_sharedcache_tq_tranquility` is `G:\EVE Shared Cache\SharedCache\tq`
//! with every non-alphanumeric turned into `_`. The fonts are read from the
//! player's own install, never shipped with the editor.
//!
//! Text goes through FreeType exactly as the client's `Tr2FontMeasurer` does
//! it (docs/format-notes.md, "Overview column rendering"): the auto-hinter
//! (`trinity.fontMan.loadFlag = 32`), whole-pixel advances, `kern`-table
//! kerning. Checked pixel-for-pixel against a native capture — see the tests.

use base64::Engine;
use freetype::face::{KerningMode, LoadFlag};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use yaml_rust2::{Yaml, YamlLoader};

use crate::ops::{AppState, ErrDto};

/// One laid-out string. `pens[i]` is the pen position after character i — what
/// the client compares against a column's text width. `mask` is the rendered
/// coverage, `width` x `height` bytes (base64), placed `left` px from the pen
/// origin with the baseline `ascent` px down.
#[derive(Debug, Serialize, PartialEq, Clone)]
pub struct Shaped {
    pub pens: Vec<i32>,
    pub left: i32,
    pub ascent: i32,
    pub width: u32,
    pub height: u32,
    pub mask: String,
}

pub(crate) fn settings_dir(state: &AppState) -> Result<PathBuf, ErrDto> {
    let user = state.user.lock().unwrap();
    let udoc = user.as_ref().ok_or_else(|| ErrDto::new("no_document", "no account file open"))?;
    Ok(udoc.path.parent().map(Path::to_path_buf).unwrap_or_default())
}

/// `Tr2GlyphString.Insert`, natively: pen += kerning (whole pixels) + the
/// glyph's hinted, whole-pixel advance.
// FreeType's FT_Pos is a C `long`: i32 on Windows, i64 elsewhere, so the casts
// are needed off Windows and redundant on it.
#[allow(clippy::unnecessary_cast)]
pub(crate) fn layout(face: &freetype::Face, px: u32, text: &str) -> Result<Shaped, String> {
    face.set_pixel_sizes(0, px).map_err(|e| e.to_string())?;
    let m = face.size_metrics().ok_or("no size metrics")?;
    let ascent = ((m.ascender + 63) >> 6) as i32;
    let height = (ascent - (m.descender >> 6) as i32).max(1);

    // Pass 1: pens and each glyph's coverage, positioned.
    let mut pens = Vec::new();
    let mut glyphs = Vec::new(); // (x, y from top, w, h, bytes)
    let (mut pen, mut prev) = (0i32, None);
    for ch in text.chars() {
        let gi = face.get_char_index(ch as usize).unwrap_or(0);
        if let Some(p) = prev {
            pen += (face.get_kerning(p, gi, KerningMode::KerningDefault).map(|k| k.x).unwrap_or(0) >> 6) as i32;
        }
        face.load_glyph(gi, LoadFlag::FORCE_AUTOHINT | LoadFlag::RENDER).map_err(|e| e.to_string())?;
        let g = face.glyph();
        let bm = g.bitmap();
        let (w, h, pitch) = (bm.width(), bm.rows(), bm.pitch());
        let mut bytes = Vec::with_capacity((w * h) as usize);
        for y in 0..h {
            let row = (y * pitch) as usize;
            bytes.extend_from_slice(&bm.buffer()[row..row + w as usize]);
        }
        glyphs.push((pen + g.bitmap_left(), ascent - g.bitmap_top(), w, h, bytes));
        pen += ((g.advance().x + 32) >> 6) as i32;
        pens.push(pen);
        prev = Some(gi);
    }

    // Pass 2: one mask spanning pen origin, pen end and every glyph's ink.
    let left = glyphs.iter().map(|g| g.0).min().unwrap_or(0).min(0);
    let right = glyphs.iter().map(|g| g.0 + g.2).max().unwrap_or(0).max(pen);
    let width = (right - left).max(0) as u32;
    let mut mask = vec![0u8; (width * height as u32) as usize];
    for (gx, gy, w, h, bytes) in &glyphs {
        for y in 0..*h {
            let my = gy + y;
            if my < 0 || my >= height {
                continue;
            }
            for x in 0..*w {
                let i = (my as u32 * width + (gx + x - left) as u32) as usize;
                mask[i] = mask[i].max(bytes[(y * w + x) as usize]);
            }
        }
    }
    let mask = base64::engine::general_purpose::STANDARD.encode(&mask);
    Ok(Shaped { pens, left, ascent, width, height: height as u32, mask })
}

/// `(ui_scale, font_size)` from `core_public__.yaml`. Every value there is a
/// `[FILETIME, value]` pair. `WindowMode` 0 is fullscreen, which is the one mode
/// that reads `UIScaleFullscreen` (the corpus has mode-0 files whose windowed
/// scale is null); windowed and fixed-window read `UIScaleWindowed`.
pub(crate) fn scale_and_font(text: &str) -> (f64, i64) {
    let doc = YamlLoader::load_from_str(text).ok().and_then(|d| d.into_iter().next()).unwrap_or(Yaml::Null);
    let val = |sec: &str, key: &str| doc[sec][key][1].clone();
    let num = |y: Yaml| y.as_f64().or_else(|| y.as_i64().map(|i| i as f64));
    let fullscreen = val("device", "WindowMode").as_i64() == Some(0);
    let (mine, other) = if fullscreen { ("UIScaleFullscreen", "UIScaleWindowed") } else { ("UIScaleWindowed", "UIScaleFullscreen") };
    let scale = num(val("device", mine)).or_else(|| num(val("device", other))).unwrap_or(1.0);
    // ponytail: ko/zh/ja clients default to LARGE, not MEDIUM; only matters when the key is absent.
    let font = val("ui", "clientFontSize").as_i64().unwrap_or(2);
    (scale, font)
}

/// EVE's two font files for this profile, read once per profile folder.
type FontPair = (Vec<u8>, Vec<u8>);
static FONTS: Mutex<Option<(PathBuf, FontPair)>> = Mutex::new(None);

pub(crate) fn fonts(settings_dir: &Path) -> Result<FontPair, String> {
    let mut cache = FONTS.lock().unwrap();
    if let Some((dir, pair)) = cache.as_ref() {
        if dir == settings_dir {
            return Ok(pair.clone());
        }
    }
    let pair = load_fonts(settings_dir)?;
    *cache = Some((settings_dir.to_path_buf(), pair.clone()));
    Ok(pair)
}

fn load_fonts(settings_dir: &Path) -> Result<FontPair, String> {
    let profile = settings_dir.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str()).unwrap_or("");
    let index_dir = find_shared_cache(profile).ok_or_else(|| format!("EVE's shared cache for profile '{profile}' wasn't found"))?;
    let index = fs::read_to_string(index_dir.join("resfileindex.txt")).map_err(|e| format!("resfileindex.txt: {e}"))?;
    let res_files = index_dir.parent().unwrap_or(&index_dir).join("ResFiles");
    let read = |res: &str| -> Result<Vec<u8>, String> {
        let rel = index
            .lines()
            .find_map(|l| l.split_once(',').filter(|(k, _)| k.eq_ignore_ascii_case(res)).map(|(_, rest)| rest))
            .and_then(|rest| rest.split(',').next())
            .ok_or_else(|| format!("{res} is not in the resfile index"))?;
        fs::read(res_files.join(rel)).map_err(|e| format!("{res}: {e}"))
    };
    Ok((read("res:/ui/fonts/evesansneue-regular.otf")?, read("res:/ui/fonts/evesansneue-expanded.otf")?))
}

/// The shared-cache server folder (the one holding `resfileindex.txt`) a profile
/// folder name encodes. The name is lossy — a space and a backslash both became
/// `_` — so this walks the disk matching one real folder name at a time.
fn find_shared_cache(profile: &str) -> Option<PathBuf> {
    let mut chars = profile.chars();
    let drive = chars.next().filter(char::is_ascii_alphabetic)?;
    let rest = chars.as_str().strip_prefix('_')?;
    walk(&PathBuf::from(format!("{}:\\", drive.to_ascii_uppercase())), rest)
}

fn walk(dir: &Path, rest: &str) -> Option<PathBuf> {
    if dir.join("resfileindex.txt").is_file() {
        return Some(dir.to_path_buf()); // what remains is the server name
    }
    for entry in fs::read_dir(dir).ok()?.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let name = sanitize(&entry.file_name().to_string_lossy());
        let Some(after) = rest.strip_prefix(name.as_str()) else { continue };
        if let Some(after) = after.strip_prefix('_') {
            if let Some(found) = walk(&entry.path(), after) {
                return Some(found);
            }
        }
    }
    None
}

fn sanitize(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_scale_by_window_mode_and_font_option() {
        let windowed = "device:\n  UIScaleFullscreen: [1, 1.25]\n  UIScaleWindowed: [1, 1.0]\n  WindowMode: [1, 2]\nui:\n  clientFontSize: [1, 4]\n";
        assert_eq!(scale_and_font(windowed), (1.0, 4));
        let fullscreen = "device:\n  UIScaleFullscreen: [1, 1.25]\n  UIScaleWindowed: [1, null]\n  WindowMode: [1, 0]\n";
        assert_eq!(scale_and_font(fullscreen), (1.25, 2), "fullscreen scale; absent font option is MEDIUM");
        assert_eq!(scale_and_font(""), (1.0, 2), "no file is EVE's defaults");
    }

    #[test]
    fn sanitize_matches_the_profile_folder_naming() {
        assert_eq!(sanitize("EVE Shared Cache"), "eve_shared_cache");
        assert_eq!(sanitize("SharedCache"), "sharedcache");
    }

    #[test]
    fn walk_follows_a_lossy_name_to_the_index() {
        let root = std::env::temp_dir().join(format!("client_env_walk_{}", std::process::id()));
        let tq = root.join("EVE Shared Cache").join("SharedCache").join("tq");
        fs::create_dir_all(&tq).unwrap();
        fs::create_dir_all(root.join("EVE")).unwrap(); // a decoy sharing the first word
        fs::write(tq.join("resfileindex.txt"), "").unwrap();
        assert_eq!(walk(&root, "eve_shared_cache_sharedcache_tq_tranquility"), Some(tq));
        assert_eq!(walk(&root, "nothing_here"), None);
        fs::remove_dir_all(&root).unwrap();
    }

    /// Ink runs of a mask, as `start-end` relative to the first inked column,
    /// gaps over 3 px splitting runs — the profile the capture was measured by.
    fn runs(s: &Shaped, threshold: u8) -> String {
        let mask = base64::engine::general_purpose::STANDARD.decode(&s.mask).unwrap();
        let w = s.width as usize;
        let on: Vec<bool> = (0..w).map(|x| (0..s.height as usize).any(|y| mask[y * w + x] > threshold)).collect();
        let mut out: Vec<(usize, usize)> = vec![];
        for (x, &v) in on.iter().enumerate() {
            if !v {
                continue;
            }
            match out.last_mut() {
                Some(r) if x - r.1 <= 3 => r.1 = x,
                _ => out.push((x, x)),
            }
        }
        let b = out.first().map(|r| r.0).unwrap_or(0);
        out.iter().map(|(a, z)| format!("{}-{}", a - b, z - b)).collect::<Vec<_>>().join(",")
    }

    /// Pixel parity with the game. The expected runs are MEASURED off
    /// `fighter.png` (native 2560x1440, Tiny font: rows 12 px Regular, headers
    /// 10 px Expanded) by brightness profile at threshold 80 over a ~17 row
    /// ground. EVE gamma-corrects text coverage (~2.2), so that threshold is
    /// raw coverage ~18/255 here.
    ///
    /// Needs EVE's fonts, which the repository can't carry: it runs where the
    /// client is installed at the path below and passes vacuously elsewhere.
    #[test]
    fn matches_the_game_pixel_for_pixel() {
        let settings = PathBuf::from(std::env::var("LOCALAPPDATA").unwrap_or_default())
            .join("CCP/EVE/g_eve_shared_cache_sharedcache_tq_tranquility/settings_Default");
        let Ok((regular, expanded)) = load_fonts(&settings) else {
            eprintln!("EVE's fonts not installed here — skipping the pixel-parity check");
            return;
        };
        let lib = freetype::Library::init().unwrap();
        let reg = lib.new_memory_face(regular, 0).unwrap();
        let exp = lib.new_memory_face(expanded, 0).unwrap();
        let cases: [(&freetype::Face, u32, &str, &str); 14] = [
            (&reg, 12, "0 m", "0-4,12-18"),
            (&reg, 12, "SVM-3K - Jaka", "0-37,44-49,56-76"),
            (&reg, 12, "Fortizar", "0-38"),
            (&reg, 12, "[GUNS-]", "0-39"),
            (&reg, 12, "290 km", "0-16,24-35"),
            (&reg, 12, "SVM-3K - BEA", "0-37,44-49,57-74"),
            (&reg, 12, "[-D0-]", "0-30"),
            (&reg, 12, "1,106 km", "0-25,33-44"),
            (&reg, 12, "8QT-H4", "0-35"),
            (&reg, 12, "Smuggler", "0-46"),
            (&exp, 10, "Name", "0-25"),
            (&exp, 10, "Type", "0-21"),
            (&exp, 10, "Corporation", "0-54"),
            (&exp, 10, "Alliance", "0-36"),
        ];
        for (face, px, text, game) in cases {
            assert_eq!(runs(&layout(face, px, text).unwrap(), 18), game, "{text:?} at {px}px");
        }
        // Pens are whole pixels and monotonic: "1,106 km" advances 47 px.
        assert_eq!(layout(&reg, 12, "1,106 km").unwrap().pens.last(), Some(&47));
    }
}
