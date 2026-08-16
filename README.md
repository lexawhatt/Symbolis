# Symbolis

Symbolis is a Linux desktop symbol and local GIF picker built with Rust and egui. It focuses on fast access to emoji, kaomoji, punctuation, math symbols, language alphabets, box drawing, blocks, shapes, music symbols, and local reaction media.

The app targets Linux desktop sessions first. Text symbols copy to the clipboard; media drag-out support is prepared through an external drag helper.

## Run

```bash
cargo run
```

Symbolis performs startup checks before opening the main UI. Missing core desktop capabilities stop the app with a visible startup error when a GUI session can be opened, and the same message is printed to stderr.

## Required Runtime Capabilities

- Linux desktop session with either `WAYLAND_DISPLAY` or `DISPLAY`.
- A working system clipboard backend available to `arboard`.

For incoming file drag-and-drop from file managers, Symbolis defaults to X11/XWayland when `DISPLAY` is available, because the current `winit` Linux file-drop implementation is available through its X11 backend. Set `SYMBOLIS_WINDOW_BACKEND=wayland` to force native Wayland, or `SYMBOLIS_WINDOW_BACKEND=x11` to force X11/XWayland.

## Optional Runtime Capabilities

- `pango-view` for cached color emoji rendering. Without it, Symbolis still runs and falls back to text-rendered emoji.
- `dragon-drop` or compatible `mwh/dragon` for file drag-out. Without it, Symbolis still runs and keeps clipboard delivery available.
- `ffmpeg` for GIF/MP4/WebM conversion. Without it, Symbolis can still reference local files, but cannot save optimized WebM copies or export WebM/MP4 back to GIF for transfer.

For drag-out, Symbolis checks:

- `SYMBOLIS_DRAG_HELPER=/path/to/dragon`
- `dragon-drop` in `PATH`
- compatible `dragon` in `PATH`

## Common Packages

On Arch-based systems, the practical package set usually includes:

```bash
sudo pacman -S pango noto-fonts noto-fonts-emoji ffmpeg
```

On Debian/Ubuntu-based systems, the practical package set usually includes:

```bash
sudo apt install libpango1.0-bin fonts-noto fonts-noto-color-emoji ffmpeg
```

Install `dragon-drop` or `mwh/dragon` separately if your distribution does not package it.

On Wayland desktops, make sure XWayland is installed if you want reliable file drops from file managers into Symbolis. Arch usually packages it as `xorg-xwayland`; Debian/Ubuntu usually package it as `xwayland`.

## Data

Symbolis uses local symbol data when available and falls back to a built-in dataset. Recent entries, recent media, UI settings, and the local media index are stored under the user's config/data directories through the `dirs` crate.

## Local GIFs

The `GIFs` mode is offline-first and does not require paid provider APIs. Symbolis scans:

- `~/.local/share/symbolis/media/gifs`
- `~/.local/share/symbolis/media/stickers`
- `~/.local/share/symbolis/media/saved`
- `~/.local/share/symbolis/media/optimized`
- common `GIFs` / `Stickers` folders under Pictures
- paths added in Preferences

Supported local files are `.gif`, `.mp4`, `.m4v`, `.png`, `.webp`, and `.webm`. Drop a supported file onto the app window to store it locally, or add a folder path in Preferences -> Media Sources to reference an existing library without copying it.

Clicking a GIF/sticker copies a file-list payload through the system clipboard. Right-clicking a tile exposes explicit file copy, favorite, drag-out, and open-location actions. Drag-out uses `dragon-drop`/`mwh/dragon` when available; without it, file-list clipboard delivery still works.

Favorites and Recently Used are metadata only. They do not duplicate the GIF files.

## Media Storage Model

Symbolis does not store user GIFs inside the project/repository. The project tree stays source code only. Media storage lives under the user's data directory.

Folder imports are zero-copy by default: the app stores paths and metadata in small JSON files, then reads the original files from their existing locations. This avoids doubling disk use for large libraries.

Individual dropped/saved GIF and MP4 files are stored as WebM under `~/.local/share/symbolis/media/optimized/` using content-addressed names, so identical files deduplicate. When that WebM item is copied or dragged out, Symbolis exports a GIF into `~/.local/share/symbolis/media/exports/` and transfers that GIF. The export cache is regeneratable and is not scanned back into the library.

The intended long-term storage model is:

- referenced local files stay where the user keeps them
- files explicitly saved from online providers or dropped as GIF/MP4 go into `~/.local/share/symbolis/media/`, preferably as WebM for GIF-like animations
- saved provider/imported copies use content-addressed names to deduplicate identical files
- thumbnails/previews are separate cache files and can be regenerated
- referenced originals are not recompressed by default; optimized saved copies are WebM, with GIF generated only for compatibility on transfer

GIPHY and Klipy remain optional configured providers because they require API keys, provider attribution, and provider-specific limits. The default local library path is free for the user and has no provider-side request limits.

Provider notes:

- Tenor API is not implemented because Google retired developer API integrations on June 30, 2026.
- GIPHY requires `SYMBOLIS_GIPHY_API_KEY` and visible `Powered by GIPHY` attribution wherever API results are used.
- Klipy requires `SYMBOLIS_KLIPY_API_KEY` and visible `Powered by KLIPY` attribution. Klipy's migration guide presents it as a Tenor-compatible endpoint replacement.
