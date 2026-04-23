use openaction::*;
use openaction::global_events::{GlobalEventHandler, DidReceiveGlobalSettingsEvent, set_global_event_handler};

use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};
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
    pub url: Option<String>,
    pub data: Option<Vec<u8>>,
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

/// Refresh cached state. Returns true if Spotify is running.
async fn refresh_state() -> bool {
    if let Some(new_state) = spotify::poll_state().await {
        SPOTIFY_RUNNING.store(true, Ordering::Relaxed);

        let mut state = STATE.lock().await;

        // Capture old values before overwriting
        let art_url_changed = state.track.art_url != new_state.track.art_url;
        let state_title = state.track.title.clone();
        let state_artist = state.track.artist.clone();
        *state = new_state.clone();
        drop(state);

        // If track changed, re-sync scroll state and fetch new art
        let track_changed = art_url_changed
            || state_title != new_state.track.title
            || state_artist != new_state.track.artist;

        if track_changed {
            scroll::sync(&new_state.track.title, &new_state.track.artist).await;
        }

        if art_url_changed {
            if let Some(ref url) = new_state.track.art_url {
                match spotify::fetch_album_art(url).await {
                    Ok(data) => {
                        let mut cache = ART_CACHE.lock().await;
                        cache.url = Some(url.clone());
                        cache.data = Some(data);
                    }
                    Err(e) => {
                        println!("Failed to fetch album art: {e}");
                        let mut cache = ART_CACHE.lock().await;
                        cache.url = None;
                        cache.data = None;
                    }
                }
            } else {
                let mut cache = ART_CACHE.lock().await;
                cache.url = None;
                cache.data = None;
            }
        }
        true
    } else {
        SPOTIFY_RUNNING.store(false, Ordering::Relaxed);
        scroll::clear().await;
        false
    }
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

        let running = refresh_state().await;

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
        let changed = state.playing != prev_playing
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
