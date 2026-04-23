# OpenDeck Spotify Controller

An [OpenDeck](https://github.com/nekename/OpenDeck) plugin that gives you full control over your locally-installed Spotify client from your Stream Deck.

## Features

- **Play / Pause** button — toggles playback, icon reflects current state
- **Next Track** / **Previous Track** buttons
- **Encoder dial** (Stream Deck + / + XL) — rotate to adjust volume, press to play/pause
- **LCD display** — shows album art, track title, and artist alongside a volume bar and percent readout
- **Scrolling text** — long titles and artist names scroll smoothly on the LCD
- **Multi-button album art** — tile the currently-playing album art across 1×1, 2×2, 3×3, or 4×4 blocks of buttons
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
| **Spotify Dial** | Encoder (SD+ / SD+ XL) | Rotate to adjust volume. Press to play/pause. The 200×100 LCD shows album art on the left with track title, artist, a volume bar, and percent readout on the right. |
| **Album Art 1×1** | Button | Displays the full album art on a single button. Press does nothing. |
| **Album Art 2×2** | Button | Displays the album art spread across 4 buttons. Drop 4 copies into a contiguous 2×2 block. |
| **Album Art 3×3** | Button | Displays the album art spread across 9 buttons. Drop 9 copies into a contiguous 3×3 block. |
| **Album Art 4×4** | Button | Displays the album art spread across 16 buttons. Drop 16 copies into a contiguous 4×4 block. |

When Spotify is not running, all buttons show dimmed icons and inputs are ignored. They activate automatically within ~1 second of Spotify launching.

### Multi-button album art

The Album Art N×N actions tile the currently-playing album art across a block of buttons. Because an OpenDeck plugin can only paint buttons it's been placed on, you need to drop **N² copies** of the action — one per tile. The plugin looks at the coordinates of all visible copies on the same device and auto-assigns each copy its slice of the art:

- Drop all N² copies into a **contiguous N×N block** and the art tiles correctly across them.
- Any other arrangement (missing copies, non-square bounding box, gaps, duplicates) shows a red **"N×N / arrange as block"** placeholder on every copy until you fix the layout.

Position within the block is derived from each copy's row/column on the device — no per-button configuration needed. The tiles refresh whenever the playing track changes.

## How it works

- **Playback control** — communicates with Spotify via the [MPRIS2](https://specifications.freedesktop.org/mpris/latest/) D-Bus interface using [zbus](https://crates.io/crates/zbus)
- **Volume** — adjusts Spotify's volume through the MPRIS2 `Volume` property
- **Album art** — reads the art URL from MPRIS2 metadata and fetches it from Spotify's CDN
- **Plugin framework** — built on [OpenAction](https://crates.io/crates/openaction), the OpenDeck plugin SDK

## License

MIT
