use std::{
    collections::HashMap,
    env, fs,
    io::BufReader,
    path::{Path, PathBuf},
    process::Command,
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use eframe::egui::{ColorImage, Context, TextureHandle, TextureOptions};

enum CachedEmoji {
    Ready(TextureHandle),
    Failed,
}

pub(crate) struct EmojiCache {
    dir: Option<PathBuf>,
    color_renderer_available: bool,
    textures: HashMap<String, CachedEmoji>,
}

impl EmojiCache {
    pub(crate) fn new(color_renderer_available: bool) -> Self {
        Self {
            dir: dirs::cache_dir().map(|dir| dir.join("symbolis").join("emoji")),
            color_renderer_available,
            textures: HashMap::new(),
        }
    }

    pub(crate) fn color_renderer_available(&self) -> bool {
        self.color_renderer_available
    }

    pub(crate) fn texture(&mut self, ctx: &Context, emoji: &str) -> Option<&TextureHandle> {
        let key = cache_key(emoji);
        if !self.textures.contains_key(&key) {
            let cached = self
                .render_or_load(ctx, emoji, &key)
                .map(CachedEmoji::Ready)
                .unwrap_or(CachedEmoji::Failed);
            self.textures.insert(key.clone(), cached);
        }

        match self.textures.get(&key) {
            Some(CachedEmoji::Ready(texture)) => Some(texture),
            _ => None,
        }
    }

    fn render_or_load(&self, ctx: &Context, emoji: &str, key: &str) -> Option<TextureHandle> {
        let dir = self.dir.as_ref()?;
        fs::create_dir_all(dir).ok()?;

        let path = dir.join(format!("{key}.png"));
        if !path.exists() && (!self.color_renderer_available || !render_emoji_png(emoji, &path)) {
            return None;
        }

        let file = fs::File::open(path).ok()?;
        let decoder = png::Decoder::new(BufReader::new(file));
        let mut reader = decoder.read_info().ok()?;
        let mut pixels = vec![0; reader.output_buffer_size()?];
        let info = reader.next_frame(&mut pixels).ok()?;

        if info.color_type != png::ColorType::Rgba {
            return None;
        }

        let size = [info.width as usize, info.height as usize];
        pixels.truncate(info.buffer_size());
        let color_image = ColorImage::from_rgba_unmultiplied(size, &pixels);

        Some(ctx.load_texture(
            format!("symbolis-emoji-{key}"),
            color_image,
            TextureOptions::LINEAR,
        ))
    }
}

pub(crate) fn detect_color_emoji_renderer() -> bool {
    find_executable_in_path("pango-view").is_some()
}

fn render_emoji_png(emoji: &str, path: &Path) -> bool {
    Command::new("pango-view")
        .arg("--no-display")
        .arg("--font=Noto Color Emoji 42")
        .arg("--background=transparent")
        .arg(format!("--output={}", path.display()))
        .arg(format!("--text={emoji}"))
        .status()
        .is_ok_and(|status| status.success())
}

fn find_executable_in_path(name: &str) -> Option<PathBuf> {
    env::var_os("PATH")?
        .to_string_lossy()
        .split(':')
        .filter(|dir| !dir.is_empty())
        .map(|dir| Path::new(dir).join(name))
        .find(|path| is_executable_file(path))
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };

    if !metadata.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        metadata.permissions().mode() & 0o111 != 0
    }

    #[cfg(not(unix))]
    {
        true
    }
}

fn cache_key(value: &str) -> String {
    value
        .chars()
        .map(|ch| format!("{:x}", ch as u32))
        .collect::<Vec<_>>()
        .join("-")
}
