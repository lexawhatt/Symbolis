# Symbolis

Symbolis is a Linux desktop symbol picker built with Rust and egui. It focuses on fast access to emoji, kaomoji, punctuation, math symbols, language alphabets, box drawing, blocks, shapes, music symbols, and related sets.

The app targets Wayland and X11 first. Text symbols copy to the clipboard; media drag-out support is prepared through an external drag helper.

## Run

```bash
cargo run
```

Symbolis performs startup checks before opening the main UI. Missing core desktop capabilities stop the app with a visible startup error when a GUI session can be opened, and the same message is printed to stderr.

## Required Runtime Capabilities

- Linux desktop session with either `WAYLAND_DISPLAY` or `DISPLAY`.
- A working system clipboard backend available to `arboard`.

## Optional Runtime Capabilities

- `pango-view` for cached color emoji rendering. Without it, Symbolis still runs and falls back to text-rendered emoji.
- `dragon-drop` or compatible `mwh/dragon` for file drag-out. Without it, Symbolis still runs and keeps clipboard delivery available.

For drag-out, Symbolis checks:

- `SYMBOLIS_DRAG_HELPER=/path/to/dragon`
- `dragon-drop` in `PATH`
- compatible `dragon` in `PATH`

## Common Packages

On Arch-based systems, the practical package set usually includes:

```bash
sudo pacman -S pango noto-fonts noto-fonts-emoji
```

On Debian/Ubuntu-based systems, the practical package set usually includes:

```bash
sudo apt install libpango1.0-bin fonts-noto fonts-noto-color-emoji
```

Install `dragon-drop` or `mwh/dragon` separately if your distribution does not package it.

## Data

Symbolis uses local symbol data when available and falls back to a built-in dataset. Recent entries and UI settings are stored under the user's config/data directories through the `dirs` crate.

## GIF And Sticker Foundation

The repository currently contains provider selection and request URL builders for local media, GIPHY, and Tenor. Actual network fetching, preview caching, and drag/copy delivery for GIFs and stickers are intentionally separate follow-up work.
