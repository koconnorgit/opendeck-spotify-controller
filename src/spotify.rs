use std::collections::HashMap;
use tokio::sync::OnceCell;
use zbus::zvariant::OwnedValue;

static DBUS_CONNECTION: OnceCell<zbus::Connection> = OnceCell::const_new();

async fn connection() -> zbus::Result<&'static zbus::Connection> {
    DBUS_CONNECTION
        .get_or_try_init(|| async { zbus::Connection::session().await })
        .await
}

#[zbus::proxy(
    interface = "org.mpris.MediaPlayer2.Player",
    default_service = "org.mpris.MediaPlayer2.spotify",
    default_path = "/org/mpris/MediaPlayer2"
)]
trait Player {
    fn play_pause(&self) -> zbus::Result<()>;
    fn next(&self) -> zbus::Result<()>;
    fn previous(&self) -> zbus::Result<()>;

    #[zbus(property)]
    fn playback_status(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn metadata(&self) -> zbus::Result<HashMap<String, OwnedValue>>;

    #[zbus(property)]
    fn volume(&self) -> zbus::Result<f64>;

    #[zbus(property)]
    fn set_volume(&self, volume: f64) -> zbus::Result<()>;
}

#[derive(Clone, Debug, Default)]
pub struct TrackInfo {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub art_url: Option<String>,
}

#[derive(Clone, Debug)]
pub struct SpotifyState {
    pub playing: bool,
    pub track: TrackInfo,
    pub volume: f64,
}

impl Default for SpotifyState {
    fn default() -> Self {
        Self {
            playing: false,
            track: TrackInfo::default(),
            volume: 0.5,
        }
    }
}

async fn get_proxy() -> anyhow::Result<PlayerProxy<'static>> {
    let conn = connection().await?;
    let proxy = PlayerProxy::new(conn).await?;
    Ok(proxy)
}

pub async fn play_pause() -> anyhow::Result<()> {
    get_proxy().await?.play_pause().await?;
    Ok(())
}

pub async fn next_track() -> anyhow::Result<()> {
    get_proxy().await?.next().await?;
    Ok(())
}

pub async fn previous_track() -> anyhow::Result<()> {
    get_proxy().await?.previous().await?;
    Ok(())
}

pub async fn set_volume(volume: f64) -> anyhow::Result<()> {
    let vol = volume.clamp(0.0, 1.0);
    get_proxy().await?.set_volume(vol).await?;
    Ok(())
}

pub async fn get_volume() -> anyhow::Result<f64> {
    let vol = get_proxy().await?.volume().await?;
    Ok(vol)
}

/// Check if Spotify is registered on the D-Bus session bus.
pub async fn is_running() -> bool {
    let Ok(conn) = connection().await else {
        return false;
    };
    let Ok(dbus) = zbus::fdo::DBusProxy::new(conn).await else {
        return false;
    };
    dbus.name_has_owner("org.mpris.MediaPlayer2.spotify".try_into().unwrap())
        .await
        .unwrap_or(false)
}

/// Poll current Spotify state. Returns None if Spotify is not running.
pub async fn poll_state() -> Option<SpotifyState> {
    if !is_running().await {
        return None;
    }

    let proxy = get_proxy().await.ok()?;

    let playing = proxy
        .playback_status()
        .await
        .ok()
        .map(|s| s == "Playing")
        .unwrap_or(false);

    let volume = proxy.volume().await.unwrap_or(0.5);

    let track = match proxy.metadata().await {
        Ok(meta) => parse_metadata(&meta),
        Err(_) => TrackInfo::default(),
    };

    Some(SpotifyState {
        playing,
        track,
        volume,
    })
}

fn parse_metadata(meta: &HashMap<String, OwnedValue>) -> TrackInfo {
    let title = extract_string(meta, "xesam:title").unwrap_or_default();
    let artist = extract_string_array(meta, "xesam:artist")
        .map(|a| a.join(", "))
        .unwrap_or_default();
    let album = extract_string(meta, "xesam:album").unwrap_or_default();
    let art_url = extract_string(meta, "mpris:artUrl").map(|url| fix_art_url(&url));

    TrackInfo {
        title,
        artist,
        album,
        art_url,
    }
}

fn extract_string(meta: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    let val = meta.get(key)?;
    // OwnedValue -> &str via try_into or downcast
    String::try_from(val.clone()).ok()
}

fn extract_string_array(meta: &HashMap<String, OwnedValue>, key: &str) -> Option<Vec<String>> {
    let val = meta.get(key)?;
    <Vec<String>>::try_from(val.clone()).ok()
}

/// Spotify sends art URLs like https://open.spotify.com/image/<id>
/// which 404. The correct CDN URL is https://i.scdn.co/image/<id>.
fn fix_art_url(url: &str) -> String {
    if url.contains("open.spotify.com/image/") {
        let id = url.rsplit('/').next().unwrap_or("");
        format!("https://i.scdn.co/image/{}", id)
    } else {
        url.to_string()
    }
}

/// Download album art from a URL. Returns PNG/JPEG bytes.
pub async fn fetch_album_art(url: &str) -> anyhow::Result<Vec<u8>> {
    let bytes = reqwest::get(url).await?.bytes().await?;
    Ok(bytes.to_vec())
}

/// Launch the Spotify desktop client, detached from this process so it
/// outlives the plugin. Tries the native `spotify` binary first, then
/// falls back to the Flatpak package.
pub fn launch() -> anyhow::Result<()> {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    let mut last_err: Option<std::io::Error> = None;
    for cmd in [
        ("spotify", &[] as &[&str]),
        ("flatpak", &["run", "com.spotify.Client"]),
    ] {
        match Command::new(cmd.0)
            .args(cmd.1)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
        {
            Ok(_) => return Ok(()),
            Err(e) => last_err = Some(e),
        }
    }
    Err(anyhow::anyhow!(
        "failed to launch Spotify: {}",
        last_err.map(|e| e.to_string()).unwrap_or_default()
    ))
}
