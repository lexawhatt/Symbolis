# Symbolis

Symbolis is a Linux desktop picker for symbols, emoji, kaomoji, local stickers, and local GIF-like media. It is written in Rust with egui/eframe and is designed around fast clipboard delivery, local-first media indexing, and optional file drag-out.

The application builds to one executable binary:

```bash
cargo build --release
./target/release/symbolis
```

The binary is standalone as an application artifact and includes egui's default UI font. It still runs on top of normal Linux desktop services and shared system libraries. Some media features also call optional command-line tools such as `ffmpeg`, `pango-view`, `curl`, and `dragon-drop`.

## Quick Start

Run from source:

```bash
cargo run
```

Build an optimized binary:

```bash
cargo build --release
```

Run the built binary:

```bash
./target/release/symbolis
```

On startup, Symbolis checks the desktop session and optional helper tools. Missing required desktop capabilities stop startup with an error window when possible and also print the error to stderr. Missing optional tools are shown as warnings in Preferences -> System.

## Runtime Requirements

Required:

- Linux desktop session with either `WAYLAND_DISPLAY` or `DISPLAY`.
- A working clipboard backend available to `arboard`.
- A working OpenGL/EGL/GLX desktop stack for the egui window.

Optional:

- `pango-view` for cached color emoji images. Without it, emoji use egui text rendering.
- system text fonts such as Noto, DejaVu, or Liberation for broader symbol/language coverage. The core UI has a built-in fallback font.
- `fonts-noto-color-emoji` or another color emoji font for proper color emoji output.
- `ffmpeg` for thumbnails, animated hover previews, GIF/MP4/WebM conversion, optimized WebM saves, and GIF export for transfer.
- `curl` for Telegram sticker set imports.
- `dragon-drop` or compatible `mwh/dragon` for file drag-out. Clipboard file delivery still works without it.
- `kwriteconfig6` or `kwriteconfig5` for the KDE shortcut convenience button.

Arch-based systems:

```bash
sudo pacman -S pango noto-fonts noto-fonts-emoji ffmpeg curl xorg-xwayland
```

Debian/Ubuntu-based systems:

```bash
sudo apt update
sudo apt install pango1.0-tools fonts-noto fonts-noto-color-emoji ffmpeg curl xwayland
```

`pango-view` is provided by `pango1.0-tools` on Debian/Ubuntu. If your distribution does not package `dragon-drop`/`mwh/dragon`, install a compatible drag helper separately or use clipboard delivery instead.

## Linux Backends

Symbolis supports X11 and Wayland through winit/eframe.

For incoming file drag-and-drop from file managers, the automatic mode prefers X11/XWayland when `DISPLAY` is available. This is intentional: the current winit Linux file-drop path is more reliable through X11.

| Environment | Command | Notes |
| --- | --- | --- |
| Wayland with XWayland | `symbolis` or `cargo run` | Default practical path for KDE/GNOME Wayland when `DISPLAY` exists. |
| Native Wayland | `SYMBOLIS_WINDOW_BACKEND=wayland symbolis` | Core UI, clipboard, drag-out helper, and media actions work when compositor clipboard protocols are available. File drops may vary by compositor/toolkit. |
| X11 | `SYMBOLIS_WINDOW_BACKEND=x11 symbolis` | Uses the X11 winit backend and X11 clipboard path. |

The active backend, startup warnings, and detected helper tools are shown in Preferences -> System.

## Symbols

Symbolis includes built-in symbols and can also read rofimoji data when present.

Included categories cover emoji, kaomoji, Greek, Cyrillic, Latin extended, IPA, Hebrew, Arabic, Kana, math, punctuation, currency, arrows, box drawing, blocks, shapes, keyboard symbols, superscripts/subscripts, fractions, Braille, games, music, units, and enclosed symbols.

Text entries copy directly to the clipboard. Recently used symbols are stored as small JSON metadata.

Symbol data lookup order:

- `ROFIMOJI_DATA_DIR`
- Python site/dist packages under `/usr/lib` and `/usr/local/lib`
- `/usr/share/rofimoji/data`
- `/usr/local/share/rofimoji/data`
- Symbolis built-in fallback data

## Local Media

The GIFs and Stickers modes are local-first. The default provider is the local library and does not need an API key.

Symbolis scans:

- `~/.local/share/symbolis/media/gifs`
- `~/.local/share/symbolis/media/stickers`
- `~/.local/share/symbolis/media/saved`
- `~/.local/share/symbolis/media/optimized`
- `~/Pictures/GIFs`
- `~/Pictures/Stickers`
- paths added in Preferences -> Media Sources

Supported local files:

- `.gif`
- `.mp4`
- `.m4v`
- `.png`
- `.webp`
- `.webm`

Drop a supported file onto the window to store it locally, or add a folder path in Preferences to reference an existing library without copying it. Folder imports are zero-copy by default: Symbolis stores paths and metadata, then reads the original files from their existing locations.

When `ffmpeg` is available, dropped GIF/MP4 files are stored as content-addressed WebM copies where possible. WebM files are copied into optimized storage. PNG/WebP files are copied into saved storage.

The media watcher can automatically reindex watched folders. Content-hash deduplication can collapse identical media during scans.

Clicking a GIF or sticker copies a file-list payload through the system clipboard. Right-clicking a media tile provides actions for rename, favorite, optimized WebM save, file copy, path copy, drag-out, open location, and delete.

Favorites and Recently Used are metadata only. They do not duplicate media files.

## Media Preview

Media tiles use cached static thumbnails and optional animated hover previews.

Preferences -> Media Preview controls:

- hover zoom on/off
- animated playback on hover
- hover delay
- preview scale
- preview framerate from 1 to 24 FPS
- hover animation speed

Changing preview framerate creates a separate preview cache profile, so frames are not reused at the wrong FPS.

## Telegram Stickers

Telegram sticker set links can be pasted into Preferences -> Media Sources:

```text
https://t.me/addstickers/EdgyCatboy
tg://addstickers?set=EdgyCatboy
```

Telegram import requires a BotFather token. Save it in Preferences, or provide it with:

```bash
SYMBOLIS_TELEGRAM_BOT_TOKEN=123456:token symbolis
```

Symbolis uses the Telegram Bot API through `curl` and downloads supported stickers into:

```text
~/.local/share/symbolis/media/stickers/telegram/
```

Supported Telegram imports:

- static `.webp` stickers
- video `.webm` stickers

Animated `.tgs` stickers are skipped for now.

Saved Telegram tokens are stored in:

```text
~/.config/symbolis/telegram_secret.json
```

On Unix systems Symbolis sets this file to `0600`.

## Online GIF Providers

Local media is the default provider and needs no API key.

Optional online providers:

- GIPHY requires `SYMBOLIS_GIPHY_API_KEY` and displays `Powered by GIPHY`.
- Klipy requires `SYMBOLIS_KLIPY_API_KEY` and displays `Powered by KLIPY`.

The old `Tenor` settings value is treated as `Klipy` for compatibility. Tenor search itself is not implemented.

## Global Hotkey And IPC

Preferences -> Global Hotkey provides:

- optional built-in global-hotkey backend
- editable hotkey bindings
- a generated launcher command for desktop shortcut managers
- launcher installation to `~/.local/share/applications/symbolis-toggle.desktop`
- KDE shortcut application through `kwriteconfig6` or `kwriteconfig5` when available

On Wayland, the recommended path is usually the desktop shortcut/launcher command, because the compositor owns global shortcuts. The built-in backend can be useful on X11 and on desktop setups where global-hotkey registration works.

The default built-in toggle binding is `Super+Period`.

Symbolis also accepts local IPC commands through a Unix socket:

```bash
symbolis --toggle
symbolis --show-main
symbolis --show-symbols
symbolis --show-stickers
symbolis --show-gifs
symbolis --quit
```

When a Symbolis instance is already running, launching `symbolis` without arguments asks the existing instance to show the main window and exits. Commands such as `symbolis --toggle` are delivered to the running instance through IPC.

The IPC socket uses:

```text
$XDG_RUNTIME_DIR/symbolis.sock
```

If `XDG_RUNTIME_DIR` is missing, Symbolis creates a private `0700` fallback directory under the system temp directory and rejects symlink or foreign-owned fallback paths.

## Drag-Out Helper

For drag-out, Symbolis checks in this order:

- `SYMBOLIS_DRAG_HELPER=/path/to/dragon`
- `dragon-drop` in `PATH`
- compatible `dragon` in `PATH`

Without a helper, media can still be copied through the file-list clipboard path.

## User Data

Symbolis does not store user media inside the project repository. User data lives under the platform config/data/cache directories returned by the `dirs` crate.

Typical Linux paths:

- settings: `~/.config/symbolis/settings.json`
- Telegram token: `~/.config/symbolis/telegram_secret.json`
- recent symbols: `~/.local/share/symbolis/recent.json`
- recent media: `~/.local/share/symbolis/recent_media.json`
- favorite media IDs: `~/.local/share/symbolis/favorite_media.json`
- media index: `~/.local/share/symbolis/media/index.json`
- local media: `~/.local/share/symbolis/media/`
- optimized media: `~/.local/share/symbolis/media/optimized/`
- generated transfer exports: `~/.local/share/symbolis/media/exports/`
- thumbnails/previews: cache directory under `symbolis/media-thumbs`

JSON persistence uses temp-file plus rename writes for settings, recent symbols, recent media, favorites, and the local media index.

## Troubleshooting

Check helper detection in Preferences -> System first. The same startup warnings are also useful when running from a terminal.

If color emoji are missing on Debian/Ubuntu:

```bash
sudo apt install pango1.0-tools fonts-noto-color-emoji
which pango-view
```

If media thumbnails, optimized saves, or WebM/GIF transfer are unavailable:

```bash
sudo apt install ffmpeg
ffmpeg -version
```

If Telegram import is unavailable:

```bash
sudo apt install curl
curl --version
```

If file drops do not work on native Wayland, try the X11/XWayland backend:

```bash
SYMBOLIS_WINDOW_BACKEND=x11 symbolis
```

If drag-out does not work, install `dragon-drop` or set:

```bash
SYMBOLIS_DRAG_HELPER=/path/to/dragon symbolis
```
