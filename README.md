# Symbolis

Symbolis is a Linux desktop picker for symbols, emoji, kaomoji, local stickers, and local GIF-like media. It is built with Rust and egui, keeps the default workflow offline-first, and is designed around fast clipboard delivery plus optional file drag-out.

## Run

```bash
cargo run
```

Symbolis runs startup checks before opening the main UI. Missing core desktop capabilities stop startup with a visible error window when a GUI session is available, and the same message is printed to stderr.

## Runtime Requirements

Required:

- Linux desktop session with either `WAYLAND_DISPLAY` or `DISPLAY`.
- A working system clipboard backend available to `arboard`.

Optional:

- `pango-view` for cached color emoji rendering. Without it, Symbolis falls back to text-rendered emoji.
- `ffmpeg` for media thumbnails, animated previews, GIF/MP4/WebM conversion, optimized WebM saves, and GIF export for transfer.
- `dragon-drop` or compatible `mwh/dragon` for file drag-out. Clipboard file delivery still works without it.
- `curl` for Telegram sticker set imports.

On Arch-based systems:

```bash
sudo pacman -S pango noto-fonts noto-fonts-emoji ffmpeg curl xorg-xwayland
```

On Debian/Ubuntu-based systems:

```bash
sudo apt install libpango1.0-bin fonts-noto fonts-noto-color-emoji ffmpeg curl xwayland
```

Install `dragon-drop` or `mwh/dragon` separately if your distribution does not package it.

## Linux Backends

For incoming file drag-and-drop from file managers, Symbolis defaults to X11/XWayland when `DISPLAY` is available, because the current `winit` Linux file-drop path is most reliable through X11.

| Desktop | Recommended mode | Command | Notes |
| --- | --- | --- | --- |
| Wayland with XWayland | Auto default | `cargo run` | Best practical default for KDE/GNOME Wayland when `DISPLAY` exists. |
| Native Wayland | Force Wayland | `SYMBOLIS_WINDOW_BACKEND=wayland cargo run` | Core UI, clipboard, drag-out helper, and media actions work when compositor clipboard protocols are available. File drops may depend on compositor/toolkit support. |
| X11 | Auto or force X11 | `cargo run` or `SYMBOLIS_WINDOW_BACKEND=x11 cargo run` | Uses the X11 winit backend and X11 clipboard path. |

The System section in Preferences shows the active backend, startup warnings, and color emoji renderer status.

## Symbols

Symbolis includes emoji, kaomoji, Greek, Cyrillic, Latin extended, IPA, Hebrew, Arabic, Kana, math, punctuation, currency, arrows, box drawing, blocks, shapes, keyboard symbols, superscripts/subscripts, fractions, Braille, games, music, units, and enclosed symbols.

Text entries copy directly to the clipboard. Recently used symbols are stored as small JSON metadata.

## Local Media

The `GIFs` and `Stickers` modes are offline-first. Symbolis scans:

- `~/.local/share/symbolis/media/gifs`
- `~/.local/share/symbolis/media/stickers`
- `~/.local/share/symbolis/media/saved`
- `~/.local/share/symbolis/media/optimized`
- common `GIFs` / `Stickers` folders under Pictures
- paths added in Preferences -> Media Sources

Supported local files are `.gif`, `.mp4`, `.m4v`, `.png`, `.webp`, and `.webm`.

Drop a supported file onto the window to store it locally, or add a folder path in Preferences to reference an existing library without copying it. Folder imports are zero-copy by default: Symbolis stores paths and metadata, then reads the original files from their existing locations.

The media watcher can automatically reindex watched folders. Content-hash deduplication can collapse identical media during scans.

Clicking a GIF/sticker copies a file-list payload through the system clipboard. Right-clicking a tile exposes explicit rename, file copy, favorite, drag-out, delete, and open-location actions. Drag-out uses `dragon-drop`/`mwh/dragon` when available.

Favorites and Recently Used are metadata only. They do not duplicate media files.

## Media Preview

Media tiles use cached static thumbnails and optional animated hover previews. Preferences -> Media Preview controls:

- hover zoom on/off
- animated GIF playback on hover
- hover delay
- zoom scale
- preview framerate from 1 to 24 FPS without changing source animation speed
- hover animation speed

Changing preview framerate creates a separate animated preview cache profile, so old frames are not reused at the wrong FPS.

## Media Storage

Symbolis does not store user media inside the project repository. User data lives under the platform config/data/cache directories returned by the `dirs` crate.

Typical Linux paths:

- settings: `~/.config/symbolis/settings.json`
- recent symbols: `~/.local/share/symbolis/recent.json`
- recent media: `~/.local/share/symbolis/recent_media.json`
- favorite media: `~/.local/share/symbolis/favorite_media.json`
- media index: `~/.local/share/symbolis/media/index.json`
- local media: `~/.local/share/symbolis/media/`
- optimized WebM copies: `~/.local/share/symbolis/media/optimized/`
- generated transfer exports: `~/.local/share/symbolis/media/exports/`
- thumbnails/previews: the user cache directory under `symbolis/media-thumbs`

Dropped or saved GIF/MP4 files are stored as content-addressed WebM copies where possible. When a WebM item needs to be copied or dragged as a GIF, Symbolis exports a regeneratable GIF into the exports directory and transfers that file.

JSON persistence uses temp-file plus rename writes for settings, recent symbols, recent media, favorites, and the local media index.

## Telegram Stickers

Telegram sticker set links can be pasted into Preferences -> Media Sources, for example:

```text
https://t.me/addstickers/EdgyCatboy
```

Telegram import requires a free BotFather token saved in Preferences. `SYMBOLIS_TELEGRAM_BOT_TOKEN` is also supported as an environment override.

Symbolis uses Telegram Bot API metadata to download static `.webp` stickers and video `.webm` stickers into:

```text
~/.local/share/symbolis/media/stickers/telegram/
```

Animated `.tgs` stickers are skipped for now.

## Online Providers

Local media is the default provider and needs no API key.

Optional providers:

- GIPHY requires `SYMBOLIS_GIPHY_API_KEY` and visible `Powered by GIPHY` attribution wherever API results are used.
- Klipy requires `SYMBOLIS_KLIPY_API_KEY` and visible `Powered by KLIPY` attribution.

Tenor is not implemented.

## Global Hotkey And IPC

Preferences -> Global Hotkey provides:

- an optional built-in global-hotkey backend
- a generated launcher command for desktop shortcut managers
- launcher installation to `~/.local/share/applications/symbolis-toggle.desktop`
- KDE shortcut application through `kwriteconfig5`/`kwriteconfig6` when available

The desktop-shortcut command is usually the preferred Wayland path. It is distro-neutral: the command is the same on Arch, Debian, Ubuntu, Fedora, and other Linux desktops.

Desktop notes:

- KDE Plasma: use `Apply KDE shortcut` when `kwriteconfig5` or `kwriteconfig6` is available, or install the launcher and bind it in System Settings -> Shortcuts.
- GNOME/Ubuntu: install the launcher or copy the command, then bind it in Settings -> Keyboard -> View and Customize Shortcuts -> Custom Shortcuts.
- X11 sessions: the optional built-in backend can also work directly when enabled.
- Wayland sessions: prefer the desktop shortcut/launcher route because the compositor owns global shortcuts.

The default built-in binding is `Super+Period` for toggling the window when the built-in backend is enabled.

Symbolis also accepts local IPC commands through a Unix socket:

```bash
symbolis --toggle
symbolis --show-main
symbolis --show-symbols
symbolis --show-stickers
symbolis --show-gifs
symbolis --quit
```

The socket uses `$XDG_RUNTIME_DIR/symbolis.sock` when available. If `XDG_RUNTIME_DIR` is missing, Symbolis creates a private `0700` fallback directory under the system temp directory and rejects symlink or foreign-owned fallback paths.

## Drag-Out Helper

For drag-out, Symbolis checks:

- `SYMBOLIS_DRAG_HELPER=/path/to/dragon`
- `dragon-drop` in `PATH`
- compatible `dragon` in `PATH`

Without a helper, media can still be copied through the file-list clipboard path.
