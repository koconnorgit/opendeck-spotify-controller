# OpenDeck Spotify Controller

An [OpenDeck](https://github.com/nekename/OpenDeck) plugin that gives you full control over your locally-installed Spotify client from your Stream Deck.

## Features

- **Play / Pause** button — toggles playback, icon reflects current state
- **Next Track** / **Previous Track** buttons
- **Encoder dial** (Stream Deck + / + XL) — rotate to adjust volume, press to play/pause
- **LCD display** — shows album art, track title, and artist with a volume bar
- **Scrolling text** — long titles and artist names scroll smoothly on the LCD
- **Auto-detection** — buttons dim automatically when Spotify isn't running and light up when it launches

## Requirements

- Linux (x86_64 or aarch64)
- [OpenDeck](https://github.com/nekename/OpenDeck) installed and running
- Spotify desktop client (uses MPRIS2 D-Bus interface)
- A system font: Noto Sans Bold or DejaVu Sans Bold

## Building from source

You'll need a [Rust toolchain](https://rustup.rs/) (1.85+).

```bash
git clone https://github.com/koconnorgit/opendeck-spotify-controller.git
cd opendeck-spotify-controller
cargo build --release
```

The binary is produced at `target/release/oa-spotify-controller`.

## Installation

1. **Build the plugin** (see above).

2. **Copy the plugin into OpenDeck's plugin directory:**

```bash
PLUGIN_DIR=~/.config/opendeck/plugins/com.opendeck.spotify-controller.sdPlugin
mkdir -p "$PLUGIN_DIR"
cp target/release/oa-spotify-controller "$PLUGIN_DIR/oa-spotify-controller-x86_64-unknown-linux-gnu"
cp manifest.json "$PLUGIN_DIR/"
```

3. **Restart OpenDeck.** The "Spotify Controller" category will appear in the action list.

## Usage

Open the OpenDeck interface and drag these actions onto your Stream Deck layout:

| Action | Controller | What it does |
|--------|-----------|--------------|
| **Play / Pause** | Button | Toggles Spotify playback. Shows ▶ when paused, ⏸ when playing. |
| **Next Track** | Button | Skips to the next track. |
| **Previous Track** | Button | Goes back to the previous track. |
| **Spotify Dial** | Encoder (SD+ / SD+ XL) | Rotate to adjust volume. Press to play/pause. The LCD shows album art, track title, artist, and a volume bar. |

When Spotify is not running, all buttons show dimmed icons and inputs are ignored. They activate automatically within ~1 second of Spotify launching.

## How it works

- **Playback control** — communicates with Spotify via the [MPRIS2](https://specifications.freedesktop.org/mpris/latest/) D-Bus interface using [zbus](https://crates.io/crates/zbus)
- **Volume** — adjusts Spotify's volume through the MPRIS2 `Volume` property
- **Album art** — reads the art URL from MPRIS2 metadata and fetches it from Spotify's CDN
- **Plugin framework** — built on [OpenAction](https://crates.io/crates/openaction), the OpenDeck plugin SDK

## License

MIT
