use openaction::*;
use openaction::global_events::{GlobalEventHandler, DidReceiveGlobalSettingsEvent, set_global_event_handler};

use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

use crate::{gfx, scroll, spotify, tiles};

const VOLUME_STEP: f64 = 0.05; // 5% per tick

/// Whether Spotify is currently registered on D-Bus.
static SPOTIFY_RUNNING: AtomicBool = AtomicBool::new(false);

/// Cached Spotify state shared across all action handlers.
pub static STATE: LazyLock<Mutex<spotify::SpotifyState>> =
    LazyLock::new(|| Mutex::new(spotify::SpotifyState::default()));

/// Cached album art bytes (keyed by art URL to avoid re-downloading).
pub static ART_CACHE: LazyLock<Mutex<ArtCache>> =
    LazyLock::new(|| Mutex::new(ArtCache::default()));

#[derive(Default)]
pub struct ArtCache {
    /// URL of the art held in `data`.
    pub url: Option<String>,
    pub data: Option<Vec<u8>>,
    /// Art we want but do not have yet, either never fetched or last fetch
    /// failed. Retried from the polling loop until it lands.
    pub pending: Option<String>,
    /// Consecutive failures for `pending`, driving the retry backoff.
    pub failures: u32,
    /// Earliest time the next attempt for `pending` may run.
    pub retry_at: Option<Instant>,
}

/// Backoff between retries of a failed art download: fast enough that a blip
/// recovers within a second, slow enough that a permanently broken URL does not
/// hammer the CDN once a second for the length of a track.
fn art_retry_delay(failures: u32) -> Duration {
    let shift = failures.saturating_sub(1).min(5);
    (Duration::from_secs(1) * (1 << shift)).min(Duration::from_secs(30))
}

/// Outcome of a state poll.
struct Refresh {
    running: bool,
    /// Whether the album art the UI should draw changed — including being
    /// cleared — so callers know to repaint even if the track did not change.
    art_changed: bool,
}

pub fn is_active() -> bool {
    SPOTIFY_RUNNING.load(Ordering::Relaxed)
}

// ── Global handler ───────────────────────────────────────────────────────────

pub struct GlobalHandler;

#[async_trait]
impl GlobalEventHandler for GlobalHandler {
    async fn plugin_ready(&self) -> OpenActionResult<()> {
        Ok(())
    }

    async fn did_receive_global_settings(&self, _event: DidReceiveGlobalSettingsEvent) -> OpenActionResult<()> {
        Ok(())
    }
}

// ── Play/Pause action (Keypad) ───────────────────────────────────────────────

pub struct PlayPauseAction;

#[async_trait]
impl Action for PlayPauseAction {
    const UUID: ActionUuid = "com.opendeck.spotify-controller.play-pause";
    type Settings = serde_json::Value;

    async fn will_appear(&self, instance: &Instance, _: &Self::Settings) -> OpenActionResult<()> {
        if is_active() {
            update_play_pause_icon(instance).await;
        } else {
            set_inactive_icon(instance, gfx::inactive_play_icon()).await;
        }
        Ok(())
    }

    async fn key_down(&self, instance: &Instance, _: &Self::Settings) -> OpenActionResult<()> {
        if !is_active() {
            if let Err(e) = spotify::launch() {
                println!("spotify launch error: {e}");
            }
            // The monitoring loop will pick up Spotify once it registers on D-Bus
            // and flip the UI to active.
            return Ok(());
        }
        if let Err(e) = spotify::play_pause().await {
            println!("play_pause error: {e}");
        }
        // Give Spotify a moment to toggle, then refresh
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        refresh_state().await;
        update_play_pause_icon(instance).await;
        Ok(())
    }
}

// ── Next Track action (Keypad) ───────────────────────────────────────────────

pub struct NextTrackAction;

#[async_trait]
impl Action for NextTrackAction {
    const UUID: ActionUuid = "com.opendeck.spotify-controller.next-track";
    type Settings = serde_json::Value;

    async fn will_appear(&self, instance: &Instance, _: &Self::Settings) -> OpenActionResult<()> {
        if is_active() {
            if let Ok(icon) = gfx::next_icon() {
                let _ = instance.set_image(Some(icon), None).await;
            }
        } else {
            set_inactive_icon(instance, gfx::inactive_next_icon()).await;
        }
        Ok(())
    }

    async fn key_down(&self, _instance: &Instance, _: &Self::Settings) -> OpenActionResult<()> {
        if !is_active() { return Ok(()); }
        if let Err(e) = spotify::next_track().await {
            println!("next_track error: {e}");
        }
        // Allow track change to propagate, then refresh UI
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        refresh_state().await;
        update_all_ui().await;
        Ok(())
    }
}

// ── Previous Track action (Keypad) ───────────────────────────────────────────

pub struct PrevTrackAction;

#[async_trait]
impl Action for PrevTrackAction {
    const UUID: ActionUuid = "com.opendeck.spotify-controller.prev-track";
    type Settings = serde_json::Value;

    async fn will_appear(&self, instance: &Instance, _: &Self::Settings) -> OpenActionResult<()> {
        if is_active() {
            if let Ok(icon) = gfx::prev_icon() {
                let _ = instance.set_image(Some(icon), None).await;
            }
        } else {
            set_inactive_icon(instance, gfx::inactive_prev_icon()).await;
        }
        Ok(())
    }

    async fn key_down(&self, _instance: &Instance, _: &Self::Settings) -> OpenActionResult<()> {
        if !is_active() { return Ok(()); }
        if let Err(e) = spotify::previous_track().await {
            println!("previous_track error: {e}");
        }
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        refresh_state().await;
        update_all_ui().await;
        Ok(())
    }
}

// ── Spotify Dial action (Encoder) ────────────────────────────────────────────

pub struct SpotifyDialAction;

#[async_trait]
impl Action for SpotifyDialAction {
    const UUID: ActionUuid = "com.opendeck.spotify-controller.dial";
    type Settings = serde_json::Value;

    async fn will_appear(&self, instance: &Instance, _: &Self::Settings) -> OpenActionResult<()> {
        if is_active() {
            update_encoder_lcd(instance).await;
        } else {
            set_inactive_icon(instance, gfx::inactive_encoder_lcd()).await;
        }
        let _ = instance.set_title(Some(""), None).await;
        Ok(())
    }

    async fn dial_rotate(
        &self,
        instance: &Instance,
        _: &Self::Settings,
        ticks: i16,
        _pressed: bool,
    ) -> OpenActionResult<()> {
        if !is_active() { return Ok(()); }
        let current_vol = {
            STATE.lock().await.volume
        };
        let new_vol = (current_vol + VOLUME_STEP * ticks as f64).clamp(0.0, 1.0);

        if let Err(e) = spotify::set_volume(new_vol).await {
            println!("set_volume error: {e}");
        }

        // Update cached state immediately for responsive UI
        STATE.lock().await.volume = new_vol;
        update_encoder_lcd(instance).await;

        Ok(())
    }

    async fn dial_down(&self, instance: &Instance, _: &Self::Settings) -> OpenActionResult<()> {
        if !is_active() {
            if let Err(e) = spotify::launch() {
                println!("spotify launch error: {e}");
            }
            return Ok(());
        }
        if let Err(e) = spotify::play_pause().await {
            println!("dial play_pause error: {e}");
        }
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        refresh_state().await;
        update_encoder_lcd(instance).await;
        // Also update the keypad play/pause button if visible
        update_all_play_pause_buttons().await;
        Ok(())
    }
}

// ── Album Art Tile actions (Keypad, no-op on press) ─────────────────────────
//
// Each tile action expects N^2 instances arranged in a contiguous NxN block on
// a single device. Position within the block is derived from each instance's
// coordinates; misplaced instances show an error icon until arranged correctly.

macro_rules! art_tile_action {
    ($name:ident, $uuid_const:ident, $n:expr) => {
        pub struct $name;

        #[async_trait]
        impl Action for $name {
            const UUID: ActionUuid = tiles::$uuid_const;
            type Settings = serde_json::Value;

            async fn will_appear(&self, instance: &Instance, _: &Self::Settings) -> OpenActionResult<()> {
                tiles::repaint_device(Self::UUID, $n, &instance.device_id).await;
                Ok(())
            }

            async fn will_disappear(&self, instance: &Instance, _: &Self::Settings) -> OpenActionResult<()> {
                tiles::repaint_device(Self::UUID, $n, &instance.device_id).await;
                Ok(())
            }
        }
    };
}

art_tile_action!(ArtTile1x1Action, TILE_1X1_UUID, 1);
art_tile_action!(ArtTile2x2Action, TILE_2X2_UUID, 2);
art_tile_action!(ArtTile3x3Action, TILE_3X3_UUID, 3);
art_tile_action!(ArtTile4x4Action, TILE_4X4_UUID, 4);

// ── State management ─────────────────────────────────────────────────────────

/// Refresh cached state, including a retry of any outstanding art download.
async fn refresh_state() -> Refresh {
    let Some(new_state) = spotify::poll_state().await else {
        SPOTIFY_RUNNING.store(false, Ordering::Relaxed);
        scroll::clear().await;
        return Refresh { running: false, art_changed: false };
    };

    SPOTIFY_RUNNING.store(true, Ordering::Relaxed);

    let mut state = STATE.lock().await;

    // Capture old values before overwriting
    let art_url_changed = state.track.art_url != new_state.track.art_url;
    let state_title = state.track.title.clone();
    let state_artist = state.track.artist.clone();
    *state = new_state.clone();
    drop(state);

    // If track changed, re-sync scroll state
    let track_changed = art_url_changed
        || state_title != new_state.track.title
        || state_artist != new_state.track.artist;

    if track_changed {
        scroll::sync(&new_state.track.title, &new_state.track.artist).await;
    }

    Refresh {
        running: true,
        art_changed: sync_album_art(&new_state.track.art_url).await,
    }
}

/// Bring the art cache in line with `wanted`, retrying earlier failures.
///
/// A failed download leaves the cache empty rather than falling back to the
/// previous track's cover: a blank tile is an honest signal that art is
/// missing, where stale art would quietly misrepresent what is playing.
///
/// Returns true if what the UI should draw changed.
async fn sync_album_art(wanted: &Option<String>) -> bool {
    let mut changed = false;

    let attempt = {
        let mut cache = ART_CACHE.lock().await;

        if cache.url.as_ref() == wanted.as_ref() {
            // Already holding the right art (or both are None: nothing to show).
            cache.pending = None;
        } else {
            // Anything cached belongs to a different track — drop it now so it
            // is never drawn alongside the wrong song.
            changed |= cache.data.is_some();
            cache.url = None;
            cache.data = None;

            if cache.pending.as_ref() != wanted.as_ref() {
                // New target, so start its retry budget fresh.
                cache.pending = wanted.clone();
                cache.failures = 0;
                cache.retry_at = None;
            }
        }

        match cache.pending.clone() {
            Some(url) if cache.retry_at.is_none_or(|at| Instant::now() >= at) => Some(url),
            _ => None,
        }
    };

    let Some(url) = attempt else {
        return changed;
    };

    match spotify::fetch_album_art(&url).await {
        Ok(data) => {
            let mut cache = ART_CACHE.lock().await;
            // The track may have moved on while the request was in flight.
            if cache.pending.as_deref() == Some(url.as_str()) {
                cache.url = Some(url);
                cache.data = Some(data);
                cache.pending = None;
                cache.failures = 0;
                cache.retry_at = None;
                changed = true;
            }
        }
        Err(e) => {
            let mut cache = ART_CACHE.lock().await;
            if cache.pending.as_deref() == Some(url.as_str()) {
                cache.failures += 1;
                let delay = art_retry_delay(cache.failures);
                cache.retry_at = Some(Instant::now() + delay);
                // `{e:#}` walks the whole source chain; reqwest's plain Display
                // stops at "error sending request" and hides the actual cause.
                println!(
                    "Failed to fetch album art (attempt {}, retrying in {}s): {e:#}",
                    cache.failures,
                    delay.as_secs()
                );
            }
        }
    }

    changed
}

// ── UI updates ───────────────────────────────────────────────────────────────

async fn set_inactive_icon(instance: &Instance, icon: anyhow::Result<String>) {
    if let Ok(uri) = icon {
        let _ = instance.set_image(Some(uri), None).await;
    }
}

/// Set all visible actions to their inactive/dimmed state.
async fn show_all_inactive() {
    for inst in visible_instances(PlayPauseAction::UUID).await {
        set_inactive_icon(&inst, gfx::inactive_play_icon()).await;
    }
    for inst in visible_instances(NextTrackAction::UUID).await {
        set_inactive_icon(&inst, gfx::inactive_next_icon()).await;
    }
    for inst in visible_instances(PrevTrackAction::UUID).await {
        set_inactive_icon(&inst, gfx::inactive_prev_icon()).await;
    }
    for inst in visible_instances(SpotifyDialAction::UUID).await {
        set_inactive_icon(&inst, gfx::inactive_encoder_lcd()).await;
        let _ = inst.set_title(Some(""), None).await;
    }
    tiles::repaint_all().await;
}

async fn update_play_pause_icon(instance: &Instance) {
    let playing = STATE.lock().await.playing;
    let icon = if playing {
        gfx::pause_icon()
    } else {
        gfx::play_icon()
    };
    if let Ok(data_uri) = icon {
        let _ = instance.set_image(Some(data_uri), None).await;
    }
}

async fn update_all_play_pause_buttons() {
    for inst in visible_instances(PlayPauseAction::UUID).await {
        update_play_pause_icon(&inst).await;
    }
}

async fn update_encoder_lcd(instance: &Instance) {
    let state = STATE.lock().await;
    let art_cache = ART_CACHE.lock().await;
    let art_data = art_cache.data.as_deref();
    let (title_scroll, artist_scroll) = scroll::scroll_params().await;

    match gfx::render_encoder_lcd(
        &state.track.title,
        &state.track.artist,
        art_data,
        (state.volume * 100.0) as f32,
        state.playing,
        title_scroll,
        artist_scroll,
    ) {
        Ok(uri) => {
            let _ = instance.set_image(Some(uri), None).await;
        }
        Err(e) => println!("Failed to render encoder LCD: {e}"),
    }
    let _ = instance.set_title(Some(""), None).await;
}

async fn update_all_encoder_lcds() {
    // If text is actively scrolling, the scroll timer handles LCD redraws
    // to avoid flicker from competing renders.
    if scroll::is_scrolling().await {
        return;
    }
    for inst in visible_instances(SpotifyDialAction::UUID).await {
        update_encoder_lcd(&inst).await;
    }
}

async fn update_all_ui() {
    update_all_play_pause_buttons().await;
    update_all_encoder_lcds().await;
    tiles::repaint_all().await;
}

/// Background task that polls Spotify state every second and updates all
/// visible action instances when something changes.
async fn monitoring_loop() {
    let mut prev_playing = false;
    let mut prev_title = String::new();
    let mut prev_volume: f64 = -1.0;
    let mut prev_running = false;

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        let refresh = refresh_state().await;
        let running = refresh.running;

        // Handle running state transitions
        if running != prev_running {
            prev_running = running;
            if running {
                println!("Spotify detected — activating buttons");
                // Force a full UI refresh with current state
                let state = STATE.lock().await;
                prev_playing = state.playing;
                prev_title = state.track.title.clone();
                prev_volume = state.volume;
                drop(state);
                update_all_ui().await;
            } else {
                println!("Spotify gone — showing inactive state");
                show_all_inactive().await;
            }
            continue;
        }

        if !running {
            continue;
        }

        let state = STATE.lock().await;
        // `art_changed` covers a retry landing (or art being dropped) on a track
        // that is otherwise unchanged — without it the new cover never repaints.
        let changed = refresh.art_changed
            || state.playing != prev_playing
            || state.track.title != prev_title
            || (state.volume - prev_volume).abs() > 0.005;

        if changed {
            prev_playing = state.playing;
            prev_title = state.track.title.clone();
            prev_volume = state.volume;
            drop(state);
            update_all_ui().await;
        }
    }
}

// ── Plugin init ──────────────────────────────────────────────────────────────

pub async fn init() -> OpenActionResult<()> {
    println!("Spotify Controller: initializing...");

    // Do an initial state fetch
    refresh_state().await;

    // Start background monitoring and scroll animation
    tokio::spawn(monitoring_loop());
    scroll::start_scroll_timer();

    // Register handlers and actions
    set_global_event_handler(&GlobalHandler);
    register_action(PlayPauseAction).await;
    register_action(NextTrackAction).await;
    register_action(PrevTrackAction).await;
    register_action(SpotifyDialAction).await;
    register_action(ArtTile1x1Action).await;
    register_action(ArtTile2x2Action).await;
    register_action(ArtTile3x3Action).await;
    register_action(ArtTile4x4Action).await;

    run(std::env::args().collect()).await
}
