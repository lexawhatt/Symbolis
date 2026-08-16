use std::{collections::HashMap, fs, io::BufReader, path::PathBuf, process::Command};

use eframe::egui::{ColorImage, Context, TextureHandle, TextureOptions};

enum CachedEmoji {
    Ready(TextureHandle),
    Failed,
}

pub(crate) struct EmojiCache {
    dir: Option<PathBuf>,
    textures: HashMap<String, CachedEmoji>,
}

impl EmojiCache {
    pub(crate) fn new() -> Self {
        Self {
            dir: dirs::cache_dir().map(|dir| dir.join("symbolis").join("emoji")),
            textures: HashMap::new(),
        }
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
        if !path.exists() && !render_emoji_png(emoji, &path) {
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

fn render_emoji_png(emoji: &str, path: &PathBuf) -> bool {
    Command::new("pango-view")
        .arg("--no-display")
        .arg("--font=Noto Color Emoji 42")
        .arg("--background=transparent")
        .arg(format!("--output={}", path.display()))
        .arg(format!("--text={emoji}"))
        .status()
        .is_ok_and(|status| status.success())
}

fn cache_key(value: &str) -> String {
    value
        .chars()
        .map(|ch| format!("{:x}", ch as u32))
        .collect::<Vec<_>>()
        .join("-")
}
