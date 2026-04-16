use std::sync::LazyLock;
use tokio::sync::Mutex;

use ab_glyph::PxScale;
use openaction::Action;

use crate::gfx;
use crate::plugin::{ART_CACHE, STATE, SpotifyDialAction};

const SCROLL_SPEED_PX: f32 = 1.5;
const TICK_INTERVAL_MS: u64 = 50;

/// Maximum text width (pixels) before scrolling kicks in.
/// Right-column content width (88) minus a small margin.
const MAX_TITLE_WIDTH: f32 = 84.0;
const MAX_ARTIST_WIDTH: f32 = 84.0;

struct ScrollState {
    // title
    title: String,
    title_width: f32,
    title_offset: f32,
    title_scrolls: bool,
    // artist
    artist: String,
    artist_width: f32,
    artist_offset: f32,
    artist_scrolls: bool,
}

static SCROLL: LazyLock<Mutex<Option<ScrollState>>> = LazyLock::new(|| Mutex::new(None));

/// Recalculate scroll state after a track change.
pub async fn sync(title: &str, artist: &str) {
    let font = gfx::title_font();

    let title_width = font
        .map(|f| gfx::measure_text_width(f, title, PxScale::from(gfx::LCD_TITLE_SIZE)))
        .unwrap_or(0.0);
    let artist_width = font
        .map(|f| gfx::measure_text_width(f, artist, PxScale::from(gfx::LCD_ARTIST_SIZE)))
        .unwrap_or(0.0);

    *SCROLL.lock().await = Some(ScrollState {
        title: title.to_string(),
        title_width,
        title_offset: 0.0,
        title_scrolls: title_width > MAX_TITLE_WIDTH,
        artist: artist.to_string(),
        artist_width,
        artist_offset: 0.0,
        artist_scrolls: artist_width > MAX_ARTIST_WIDTH,
    });
}

/// Clear scroll state (e.g. when Spotify exits).
pub async fn clear() {
    *SCROLL.lock().await = None;
}

/// Whether any field is actively scrolling (used to skip redundant
/// static redraws from the monitoring loop).
pub async fn is_scrolling() -> bool {
    SCROLL
        .lock()
        .await
        .as_ref()
        .is_some_and(|s| s.title_scrolls || s.artist_scrolls)
}

/// Start the scroll animation timer. Call once during plugin init.
pub fn start_scroll_timer() {
    tokio::spawn(async {
        let mut interval =
            tokio::time::interval(tokio::time::Duration::from_millis(TICK_INTERVAL_MS));
        loop {
            interval.tick().await;
            scroll_tick().await;
        }
    });
}

/// Build the scroll params that `render_encoder_lcd` expects.
/// Returns `(title_scroll, artist_scroll)`.
pub async fn scroll_params() -> (Option<(f32, f32)>, Option<(f32, f32)>) {
    let guard = SCROLL.lock().await;
    let Some(s) = guard.as_ref() else {
        return (None, None);
    };
    let title = if s.title_scrolls {
        Some((s.title_offset, s.title_width))
    } else {
        None
    };
    let artist = if s.artist_scrolls {
        Some((s.artist_offset, s.artist_width))
    } else {
        None
    };
    (title, artist)
}

async fn scroll_tick() {
    // Phase 1: advance offsets
    let needs_redraw = {
        let mut guard = SCROLL.lock().await;
        let Some(s) = guard.as_mut() else {
            return;
        };

        let any_scrolling = s.title_scrolls || s.artist_scrolls;
        if !any_scrolling {
            return;
        }

        if s.title_scrolls {
            let cycle = s.title_width + gfx::LCD_SCROLL_GAP;
            s.title_offset += SCROLL_SPEED_PX;
            if s.title_offset >= cycle {
                s.title_offset -= cycle;
            }
        }
        if s.artist_scrolls {
            let cycle = s.artist_width + gfx::LCD_SCROLL_GAP;
            s.artist_offset += SCROLL_SPEED_PX;
            if s.artist_offset >= cycle {
                s.artist_offset -= cycle;
            }
        }
        true
    };

    if !needs_redraw {
        return;
    }

    // Phase 2: re-render the LCD for all visible dial instances
    let state = STATE.lock().await;
    let art_cache = ART_CACHE.lock().await;
    let (title_scroll, artist_scroll) = {
        let guard = SCROLL.lock().await;
        let Some(s) = guard.as_ref() else { return };
        let ts = if s.title_scrolls {
            Some((s.title_offset, s.title_width))
        } else {
            None
        };
        let ar = if s.artist_scrolls {
            Some((s.artist_offset, s.artist_width))
        } else {
            None
        };
        (ts, ar)
    };

    let art_data = art_cache.data.as_deref();
    let Ok(uri) = gfx::render_encoder_lcd(
        &state.track.title,
        &state.track.artist,
        art_data,
        (state.volume * 100.0) as f32,
        state.playing,
        title_scroll,
        artist_scroll,
    ) else {
        return;
    };

    // Drop locks before sending to instances
    drop(art_cache);
    drop(state);

    for inst in openaction::visible_instances(SpotifyDialAction::UUID).await {
        let _ = inst.set_image(Some(uri.clone()), None).await;
    }
}
