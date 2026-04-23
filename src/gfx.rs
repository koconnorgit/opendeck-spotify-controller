use ab_glyph::{Font, FontArc, Glyph, PxScale, ScaleFont, point};
use anyhow::Result;
use base64::{Engine as _, engine::general_purpose};
use image::{Rgba, RgbaImage};
use std::io::Cursor;
use std::sync::LazyLock;

const ICON_SIZE: u32 = 144;
const LCD_WIDTH: u32 = 200;
const LCD_HEIGHT: u32 = 100;

static TITLE_FONT: LazyLock<Option<FontArc>> = LazyLock::new(|| {
    let paths: &[&str] = &[
        "/usr/share/fonts/noto/NotoSans-Bold.ttf",
        "/usr/share/fonts/truetype/noto/NotoSans-Bold.ttf",
        "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
        "/usr/share/fonts/dejavu/DejaVuSans-Bold.ttf",
    ];
    for path in paths {
        if let Ok(bytes) = std::fs::read(path) {
            if let Ok(font) = FontArc::try_from_vec(bytes) {
                return Some(font);
            }
        }
    }
    None
});

// ── Helpers ──────────────────────────────────────────────────────────────────

fn image_to_data_uri(img: &RgbaImage) -> Result<String> {
    let mut buf = Vec::new();
    img.write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)?;
    Ok(format!(
        "data:image/png;base64,{}",
        general_purpose::STANDARD.encode(&buf)
    ))
}

pub fn measure_text_width(font: &FontArc, text: &str, scale: PxScale) -> f32 {
    let scaled = font.as_scaled(scale);
    let mut width = 0.0f32;
    let mut prev: Option<ab_glyph::GlyphId> = None;
    for c in text.chars() {
        let gid = font.glyph_id(c);
        if let Some(p) = prev {
            width += scaled.kern(p, gid);
        }
        width += scaled.h_advance(gid);
        prev = Some(gid);
    }
    width
}

fn fit_text(font: &FontArc, text: &str, scale: PxScale, max_w: f32) -> String {
    if measure_text_width(font, text, scale) <= max_w {
        return text.to_string();
    }
    let mut s = text.to_string();
    while measure_text_width(font, &s, scale) > max_w && !s.is_empty() {
        s.pop();
    }
    s
}

fn draw_text_centered(
    img: &mut RgbaImage,
    text: &str,
    area_x: u32,
    area_y: u32,
    area_w: u32,
    size_px: f32,
    color: Rgba<u8>,
) {
    let Some(font) = TITLE_FONT.as_ref() else {
        return;
    };
    if text.is_empty() {
        return;
    }

    let scale = PxScale::from(size_px);
    let fitted = fit_text(font, text, scale, area_w as f32 - 4.0);
    if fitted.is_empty() {
        return;
    }

    let scaled = font.as_scaled(scale);
    let ascent = scaled.ascent();
    let width = measure_text_width(font, &fitted, scale);
    let x_start = area_x as f32 + (area_w as f32 - width) / 2.0;
    let y_baseline = area_y as f32 + ascent + 1.0;

    let mut x_cursor = x_start;
    let mut prev: Option<ab_glyph::GlyphId> = None;
    for c in fitted.chars() {
        let gid = font.glyph_id(c);
        if let Some(p) = prev {
            x_cursor += scaled.kern(p, gid);
        }
        let glyph: Glyph = gid.with_scale_and_position(scale, point(x_cursor, y_baseline));

        if let Some(outlined) = font.outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            outlined.draw(|gx, gy, coverage| {
                let px = bounds.min.x as i32 + gx as i32;
                let py = bounds.min.y as i32 + gy as i32;
                if px >= 0 && py >= 0 && (px as u32) < img.width() && (py as u32) < img.height() {
                    let bg = *img.get_pixel(px as u32, py as u32);
                    let a = coverage * (color[3] as f32 / 255.0);
                    let r = (color[0] as f32 * a + bg[0] as f32 * (1.0 - a)) as u8;
                    let g = (color[1] as f32 * a + bg[1] as f32 * (1.0 - a)) as u8;
                    let b = (color[2] as f32 * a + bg[2] as f32 * (1.0 - a)) as u8;
                    img.put_pixel(px as u32, py as u32, Rgba([r, g, b, bg[3]]));
                }
            });
        }
        x_cursor += scaled.h_advance(gid);
        prev = Some(gid);
    }
}

pub fn title_font() -> Option<&'static FontArc> {
    TITLE_FONT.as_ref()
}

/// Draw text scrolling horizontally within a clipped region, with seamless wrap-around.
/// `scroll_offset` is in pixels; the text repeats after `text_width + gap`.
fn draw_text_scrolling(
    img: &mut RgbaImage,
    text: &str,
    area_x: u32,
    area_y: u32,
    area_w: u32,
    size_px: f32,
    color: Rgba<u8>,
    scroll_offset: f32,
    text_width: f32,
    gap: f32,
) {
    let Some(font) = TITLE_FONT.as_ref() else {
        return;
    };
    if text.is_empty() {
        return;
    }

    let scale = PxScale::from(size_px);
    let scaled = font.as_scaled(scale);
    let ascent = scaled.ascent();
    let y_baseline = area_y as f32 + ascent + 1.0;
    let cycle = text_width + gap;

    for copy in 0..2 {
        let x_start = area_x as f32 + 2.0 - scroll_offset + copy as f32 * cycle;

        if x_start > area_x as f32 + area_w as f32 {
            continue;
        }
        if x_start + text_width < area_x as f32 {
            continue;
        }

        let mut x_cursor = x_start;
        let mut prev: Option<ab_glyph::GlyphId> = None;
        for c in text.chars() {
            let gid = font.glyph_id(c);
            if let Some(p) = prev {
                x_cursor += scaled.kern(p, gid);
            }
            let glyph: Glyph = gid.with_scale_and_position(scale, point(x_cursor, y_baseline));

            if let Some(outlined) = font.outline_glyph(glyph) {
                let bounds = outlined.px_bounds();
                outlined.draw(|gx, gy, coverage| {
                    let px = bounds.min.x as i32 + gx as i32;
                    let py = bounds.min.y as i32 + gy as i32;
                    if px >= area_x as i32
                        && (px as u32) < area_x + area_w
                        && py >= 0
                        && (py as u32) < img.height()
                    {
                        let bg = *img.get_pixel(px as u32, py as u32);
                        let a = coverage * (color[3] as f32 / 255.0);
                        let r = (color[0] as f32 * a + bg[0] as f32 * (1.0 - a)) as u8;
                        let g = (color[1] as f32 * a + bg[1] as f32 * (1.0 - a)) as u8;
                        let b = (color[2] as f32 * a + bg[2] as f32 * (1.0 - a)) as u8;
                        img.put_pixel(px as u32, py as u32, Rgba([r, g, b, bg[3]]));
                    }
                });
            }
            x_cursor += scaled.h_advance(gid);
            prev = Some(gid);
        }
    }
}

// ── Triangle rasterization ───────────────────────────────────────────────────

fn sign(px: f32, py: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    (px - x2) * (y1 - y2) - (x1 - x2) * (py - y2)
}

fn point_in_triangle(
    px: f32, py: f32,
    x1: f32, y1: f32,
    x2: f32, y2: f32,
    x3: f32, y3: f32,
) -> bool {
    let d1 = sign(px, py, x1, y1, x2, y2);
    let d2 = sign(px, py, x2, y2, x3, y3);
    let d3 = sign(px, py, x3, y3, x1, y1);
    let has_neg = (d1 < 0.0) || (d2 < 0.0) || (d3 < 0.0);
    let has_pos = (d1 > 0.0) || (d2 > 0.0) || (d3 > 0.0);
    !(has_neg && has_pos)
}

fn fill_triangle(img: &mut RgbaImage, color: Rgba<u8>, verts: [(f32, f32); 3]) {
    for py in 0..img.height() {
        for px in 0..img.width() {
            if point_in_triangle(
                px as f32, py as f32,
                verts[0].0, verts[0].1,
                verts[1].0, verts[1].1,
                verts[2].0, verts[2].1,
            ) {
                img.put_pixel(px, py, color);
            }
        }
    }
}

fn fill_rect(img: &mut RgbaImage, color: Rgba<u8>, x: u32, y: u32, w: u32, h: u32) {
    for py in y..y + h {
        for px in x..x + w {
            if px < img.width() && py < img.height() {
                img.put_pixel(px, py, color);
            }
        }
    }
}

// ── Rounded-bar drawing ──────────────────────────────────────────────────────

fn blend_colors(bg: Rgba<u8>, fg: Rgba<u8>, alpha: f32) -> Rgba<u8> {
    let alpha = alpha.clamp(0.0, 1.0);

    if bg[3] == 0 {
        return Rgba([fg[0], fg[1], fg[2], (fg[3] as f32 * alpha) as u8]);
    }

    let fg_alpha = (fg[3] as f32 / 255.0) * alpha;
    let bg_alpha = bg[3] as f32 / 255.0;
    let final_alpha = fg_alpha + bg_alpha * (1.0 - fg_alpha);

    if final_alpha == 0.0 {
        return Rgba([0, 0, 0, 0]);
    }

    let r = ((fg[0] as f32 * fg_alpha + bg[0] as f32 * bg_alpha * (1.0 - fg_alpha)) / final_alpha) as u8;
    let g = ((fg[1] as f32 * fg_alpha + bg[1] as f32 * bg_alpha * (1.0 - fg_alpha)) / final_alpha) as u8;
    let b = ((fg[2] as f32 * fg_alpha + bg[2] as f32 * bg_alpha * (1.0 - fg_alpha)) / final_alpha) as u8;
    let a = (final_alpha * 255.0) as u8;

    Rgba([r, g, b, a])
}

fn rounded_rect_distance(px: f32, py: f32, x: f32, y: f32, w: f32, h: f32, r: f32) -> f32 {
    let dx = (px - x - w / 2.0).abs() - (w / 2.0 - r);
    let dy = (py - y - h / 2.0).abs() - (h / 2.0 - r);

    let outside = (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt();
    let inside = dx.max(dy).min(0.0);

    outside + inside - r
}

/// Rounded-rect horizontal bar: track across the full width, leftmost
/// `fill_ratio` portion uses `fill_color`, rest uses `track_color`.
fn draw_horizontal_volume_bar(
    img: &mut RgbaImage,
    x: u32, y: u32, w: u32, h: u32, radius: u32,
    track_color: Rgba<u8>,
    fill_color: Rgba<u8>,
    fill_ratio: f32,
) {
    let x_f = x as f32;
    let y_f = y as f32;
    let w_f = w as f32;
    let h_f = h as f32;
    let r = radius as f32;
    let fill_x_max = x_f + w_f * fill_ratio.clamp(0.0, 1.0);

    let min_x = x.saturating_sub(1);
    let max_x = (x + w + 1).min(img.width());
    let min_y = y.saturating_sub(1);
    let max_y = (y + h + 1).min(img.height());

    for py in min_y..max_y {
        for px in min_x..max_x {
            let px_f = px as f32 + 0.5;
            let py_f = py as f32 + 0.5;
            let d = rounded_rect_distance(px_f, py_f, x_f, y_f, w_f, h_f, r);
            if d > 0.5 {
                continue;
            }
            let coverage = (0.5 - d).clamp(0.0, 1.0);
            let color = if px_f < fill_x_max { fill_color } else { track_color };
            let bg = *img.get_pixel(px, py);
            img.put_pixel(px, py, blend_colors(bg, color, coverage));
        }
    }
}

// ── Keypad button icons (144x144) ────────────────────────────────────────────

pub fn play_icon() -> Result<String> {
    let mut img = RgbaImage::from_pixel(ICON_SIZE, ICON_SIZE, Rgba([0, 0, 0, 255]));
    let white = Rgba([255, 255, 255, 255]);
    fill_triangle(&mut img, white, [(44.0, 30.0), (44.0, 114.0), (114.0, 72.0)]);
    image_to_data_uri(&img)
}

pub fn pause_icon() -> Result<String> {
    let mut img = RgbaImage::from_pixel(ICON_SIZE, ICON_SIZE, Rgba([0, 0, 0, 255]));
    let white = Rgba([255, 255, 255, 255]);
    fill_rect(&mut img, white, 38, 30, 20, 84);
    fill_rect(&mut img, white, 86, 30, 20, 84);
    image_to_data_uri(&img)
}

pub fn next_icon() -> Result<String> {
    let mut img = RgbaImage::from_pixel(ICON_SIZE, ICON_SIZE, Rgba([0, 0, 0, 255]));
    let white = Rgba([255, 255, 255, 255]);
    fill_triangle(&mut img, white, [(30.0, 30.0), (30.0, 114.0), (94.0, 72.0)]);
    fill_rect(&mut img, white, 100, 30, 12, 84);
    image_to_data_uri(&img)
}

pub fn prev_icon() -> Result<String> {
    let mut img = RgbaImage::from_pixel(ICON_SIZE, ICON_SIZE, Rgba([0, 0, 0, 255]));
    let white = Rgba([255, 255, 255, 255]);
    fill_rect(&mut img, white, 32, 30, 12, 84);
    fill_triangle(&mut img, white, [(114.0, 30.0), (114.0, 114.0), (50.0, 72.0)]);
    image_to_data_uri(&img)
}

// ── Inactive (Spotify not running) icons ─────────────────────────────────────

/// Dimmed play icon shown when Spotify is not running.
pub fn inactive_play_icon() -> Result<String> {
    let mut img = RgbaImage::from_pixel(ICON_SIZE, ICON_SIZE, Rgba([0, 0, 0, 255]));
    let dim = Rgba([60, 60, 60, 255]);
    fill_triangle(&mut img, dim, [(44.0, 30.0), (44.0, 114.0), (114.0, 72.0)]);
    image_to_data_uri(&img)
}

/// Dimmed next icon shown when Spotify is not running.
pub fn inactive_next_icon() -> Result<String> {
    let mut img = RgbaImage::from_pixel(ICON_SIZE, ICON_SIZE, Rgba([0, 0, 0, 255]));
    let dim = Rgba([60, 60, 60, 255]);
    fill_triangle(&mut img, dim, [(30.0, 30.0), (30.0, 114.0), (94.0, 72.0)]);
    fill_rect(&mut img, dim, 100, 30, 12, 84);
    image_to_data_uri(&img)
}

/// Dimmed prev icon shown when Spotify is not running.
pub fn inactive_prev_icon() -> Result<String> {
    let mut img = RgbaImage::from_pixel(ICON_SIZE, ICON_SIZE, Rgba([0, 0, 0, 255]));
    let dim = Rgba([60, 60, 60, 255]);
    fill_rect(&mut img, dim, 32, 30, 12, 84);
    fill_triangle(&mut img, dim, [(114.0, 30.0), (114.0, 114.0), (50.0, 72.0)]);
    image_to_data_uri(&img)
}

/// Dark encoder LCD shown when Spotify is not running.
pub fn inactive_encoder_lcd() -> Result<String> {
    let mut img = RgbaImage::from_pixel(LCD_WIDTH, LCD_HEIGHT, Rgba([18, 18, 18, 255]));
    draw_text_centered(&mut img, "Spotify", 0, 30, LCD_WIDTH, 22.0, Rgba([60, 60, 60, 255]));
    draw_text_centered(&mut img, "not running", 0, 58, LCD_WIDTH, 16.0, Rgba([50, 50, 50, 255]));
    image_to_data_uri(&img)
}

// ── Encoder LCD (200x100) ────────────────────────────────────────────────────
//
// Layout (top-left origin):
//   - Album art: 96x96, x=2..98, y=2..98
//   - Right column: x=102..198 (96px wide, 4px side padding → 88px content)
//       - Title:   y=2,  22px, scrolls if overflowing
//       - Artist:  y=28, 14px, scrolls if overflowing
//       - Vol bar: x=108..192 (84px), y=54, h=12, radius 6 (Spotify green)
//       - Percent: y=70, 20px, centered
//   - Whole image dimmed to ~0.45 when paused.

// Font sizes and content width used by both gfx and scroll modules.
pub const LCD_TITLE_SIZE: f32 = 22.0;
pub const LCD_ARTIST_SIZE: f32 = 14.0;
pub const LCD_SCROLL_GAP: f32 = 30.0;

const ART_SIZE: u32 = 96;
const ART_X_OFF: i32 = 2;
const ART_Y_OFF: i32 = 2;

const RIGHT_X: u32 = 102;
const RIGHT_W: u32 = 96;
const COL_PAD: u32 = 4;
const COL_X: u32 = RIGHT_X + COL_PAD;      // 106
const COL_W: u32 = RIGHT_W - COL_PAD * 2;  // 88

const TITLE_Y: u32 = 2;
const ARTIST_Y: u32 = 28;

const BAR_X: u32 = RIGHT_X + 6; // 108
const BAR_W: u32 = RIGHT_W - 12; // 84
const BAR_Y: u32 = 54;
const BAR_H: u32 = 12;
const BAR_RADIUS: u32 = 6;

const PCT_Y: u32 = 70;
const PCT_SIZE: f32 = 20.0;

/// Render the 200x100 encoder LCD: album art on the left, title/artist/volume
/// bar/percent in the right column.
///
/// `title_scroll` / `artist_scroll`: pixel offsets for scrolling text.
/// Pass `None` to center-fit (static), or `Some((offset, text_width))` to scroll.
pub fn render_encoder_lcd(
    title: &str,
    artist: &str,
    art_data: Option<&[u8]>,
    volume_percent: f32,
    is_playing: bool,
    title_scroll: Option<(f32, f32)>,
    artist_scroll: Option<(f32, f32)>,
) -> Result<String> {
    let mut img = RgbaImage::from_pixel(LCD_WIDTH, LCD_HEIGHT, Rgba([18, 18, 18, 255]));

    // --- Album art, left side ---
    if let Some(art_bytes) = art_data {
        if let Ok(art_img) = image::load_from_memory(art_bytes) {
            let resized = art_img.resize(ART_SIZE, ART_SIZE, image::imageops::FilterType::Lanczos3);
            let rgba = resized.to_rgba8();
            for (px, py, pixel) in rgba.enumerate_pixels() {
                let x = px as i32 + ART_X_OFF;
                let y = py as i32 + ART_Y_OFF;
                if x >= 0 && y >= 0 && (x as u32) < LCD_WIDTH && (y as u32) < LCD_HEIGHT {
                    let a = pixel[3] as f32 / 255.0;
                    let bg = img.get_pixel(x as u32, y as u32);
                    let r = (pixel[0] as f32 * a + bg[0] as f32 * (1.0 - a)) as u8;
                    let g = (pixel[1] as f32 * a + bg[1] as f32 * (1.0 - a)) as u8;
                    let b = (pixel[2] as f32 * a + bg[2] as f32 * (1.0 - a)) as u8;
                    img.put_pixel(x as u32, y as u32, Rgba([r, g, b, 255]));
                }
            }
        }
    }

    // --- Title ---
    let title_color = Rgba([230, 230, 230, 255]);
    if let Some((offset, text_width)) = title_scroll {
        draw_text_scrolling(
            &mut img, title, COL_X, TITLE_Y, COL_W, LCD_TITLE_SIZE,
            title_color, offset, text_width, LCD_SCROLL_GAP,
        );
    } else {
        draw_text_centered(&mut img, title, COL_X, TITLE_Y, COL_W, LCD_TITLE_SIZE, title_color);
    }

    // --- Artist ---
    let artist_color = Rgba([160, 160, 160, 255]);
    if let Some((offset, text_width)) = artist_scroll {
        draw_text_scrolling(
            &mut img, artist, COL_X, ARTIST_Y, COL_W, LCD_ARTIST_SIZE,
            artist_color, offset, text_width, LCD_SCROLL_GAP,
        );
    } else {
        draw_text_centered(&mut img, artist, COL_X, ARTIST_Y, COL_W, LCD_ARTIST_SIZE, artist_color);
    }

    // --- Horizontal volume bar (Spotify green) ---
    let track_color = Rgba([40, 40, 40, 255]);
    let fill_color = Rgba([30, 215, 96, 255]);
    let fill_ratio = (volume_percent / 100.0).clamp(0.0, 1.0);
    draw_horizontal_volume_bar(
        &mut img, BAR_X, BAR_Y, BAR_W, BAR_H, BAR_RADIUS,
        track_color, fill_color, fill_ratio,
    );

    // --- Percent readout ---
    let pct_str = format!("{}%", volume_percent.round() as i32);
    draw_text_centered(&mut img, &pct_str, COL_X, PCT_Y, COL_W, PCT_SIZE, title_color);

    // --- Dim overlay when paused ---
    if !is_playing {
        for pixel in img.pixels_mut() {
            pixel[0] = (pixel[0] as f32 * 0.45) as u8;
            pixel[1] = (pixel[1] as f32 * 0.45) as u8;
            pixel[2] = (pixel[2] as f32 * 0.45) as u8;
        }
    }

    image_to_data_uri(&img)
}

// ── Album-art tile icons (144x144) ───────────────────────────────────────────

/// Render the (tile_row, tile_col) tile of an NxN grid covering the album art.
/// The source art is scaled to N*ICON_SIZE square; this button gets a 144x144
/// slice of that, pushed out as a PNG data URI.
pub fn render_art_tile(art_bytes: &[u8], n: u8, tile_row: u8, tile_col: u8) -> Result<String> {
    let art = image::load_from_memory(art_bytes)?;
    let full_side = ICON_SIZE * n as u32;
    let scaled = art.resize_exact(full_side, full_side, image::imageops::FilterType::Lanczos3);
    let cropped = image::imageops::crop_imm(
        &scaled,
        tile_col as u32 * ICON_SIZE,
        tile_row as u32 * ICON_SIZE,
        ICON_SIZE,
        ICON_SIZE,
    )
    .to_image();
    image_to_data_uri(&cropped)
}

/// Tile shown when the action's instances don't form a valid NxN block.
pub fn misplaced_tile(n: u8) -> Result<String> {
    let mut img = RgbaImage::from_pixel(ICON_SIZE, ICON_SIZE, Rgba([32, 8, 8, 255]));
    let border = Rgba([200, 50, 50, 255]);
    let thick: u32 = 6;
    fill_rect(&mut img, border, 0, 0, ICON_SIZE, thick);
    fill_rect(&mut img, border, 0, ICON_SIZE - thick, ICON_SIZE, thick);
    fill_rect(&mut img, border, 0, 0, thick, ICON_SIZE);
    fill_rect(&mut img, border, ICON_SIZE - thick, 0, thick, ICON_SIZE);
    let label = format!("{}x{}", n, n);
    draw_text_centered(&mut img, &label, 0, 40, ICON_SIZE, 32.0, Rgba([255, 220, 220, 255]));
    draw_text_centered(&mut img, "arrange", 0, 78, ICON_SIZE, 16.0, Rgba([220, 180, 180, 255]));
    draw_text_centered(&mut img, "as block", 0, 98, ICON_SIZE, 16.0, Rgba([220, 180, 180, 255]));
    image_to_data_uri(&img)
}

/// Blank tile shown when Spotify isn't running (or no art cached yet).
pub fn inactive_tile() -> Result<String> {
    let img = RgbaImage::from_pixel(ICON_SIZE, ICON_SIZE, Rgba([18, 18, 18, 255]));
    image_to_data_uri(&img)
}
