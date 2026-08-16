use std::{
    collections::HashMap,
    fs,
    io::{self, BufReader},
    path::{Path, PathBuf},
    process::Command,
};

use eframe::egui::{ColorImage, Context, TextureHandle, TextureOptions};

use crate::media_library::MediaItem;

enum CachedMediaPreview {
    Ready(TextureHandle),
    Failed,
}

pub(crate) struct MediaPreviewCache {
    dir: Option<PathBuf>,
    textures: HashMap<String, CachedMediaPreview>,
}

impl MediaPreviewCache {
    pub(crate) fn new() -> Self {
        Self {
            dir: dirs::cache_dir().map(|dir| dir.join("symbolis").join("media-thumbs")),
            textures: HashMap::new(),
        }
    }

    pub(crate) fn texture(&mut self, ctx: &Context, item: &MediaItem) -> Option<&TextureHandle> {
        let key = preview_cache_key(item);
        if !self.textures.contains_key(&key) {
            let cached = self
                .render_or_load(ctx, item, &key)
                .map(CachedMediaPreview::Ready)
                .unwrap_or(CachedMediaPreview::Failed);
            self.textures.insert(key.clone(), cached);
        }

        match self.textures.get(&key) {
            Some(CachedMediaPreview::Ready(texture)) => Some(texture),
            _ => None,
        }
    }

    fn render_or_load(&self, ctx: &Context, item: &MediaItem, key: &str) -> Option<TextureHandle> {
        let dir = self.dir.as_ref()?;
        fs::create_dir_all(dir).ok()?;

        let path = dir.join(format!("{key}.png"));
        if !path.exists() && !render_media_thumbnail(&item.path, &path) {
            return None;
        }

        let image = load_png_color_image(&path).ok()?;
        Some(ctx.load_texture(
            format!("symbolis-media-{key}"),
            image,
            TextureOptions::LINEAR,
        ))
    }
}

fn render_media_thumbnail(input: &Path, output: &Path) -> bool {
    Command::new("ffmpeg")
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-y")
        .arg("-i")
        .arg(input)
        .arg("-frames:v")
        .arg("1")
        .arg("-vf")
        .arg("scale=320:240:force_original_aspect_ratio=decrease:flags=lanczos,format=rgba")
        .arg(output)
        .status()
        .is_ok_and(|status| status.success())
}

fn load_png_color_image(path: &Path) -> io::Result<ColorImage> {
    let file = fs::File::open(path)?;
    let mut decoder = png::Decoder::new(BufReader::new(file));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info()?;
    let buffer_size = reader
        .output_buffer_size()
        .ok_or_else(|| io::Error::other("PNG output buffer is too large"))?;
    let mut pixels = vec![0; buffer_size];
    let info = reader.next_frame(&mut pixels)?;
    pixels.truncate(info.buffer_size());

    let rgba = match info.color_type {
        png::ColorType::Rgba => pixels,
        png::ColorType::Rgb => {
            let mut rgba = Vec::with_capacity((info.width * info.height * 4) as usize);
            for rgb in pixels.chunks_exact(3) {
                rgba.extend_from_slice(rgb);
                rgba.push(255);
            }
            rgba
        }
        png::ColorType::Grayscale => {
            let mut rgba = Vec::with_capacity((info.width * info.height * 4) as usize);
            for value in pixels {
                rgba.extend_from_slice(&[value, value, value, 255]);
            }
            rgba
        }
        png::ColorType::GrayscaleAlpha => {
            let mut rgba = Vec::with_capacity((info.width * info.height * 4) as usize);
            for gray_alpha in pixels.chunks_exact(2) {
                rgba.extend_from_slice(&[
                    gray_alpha[0],
                    gray_alpha[0],
                    gray_alpha[0],
                    gray_alpha[1],
                ]);
            }
            rgba
        }
        png::ColorType::Indexed => return Err(io::Error::other("indexed PNG was not expanded")),
    };

    Ok(ColorImage::from_rgba_unmultiplied(
        [info.width as usize, info.height as usize],
        &rgba,
    ))
}

fn preview_cache_key(item: &MediaItem) -> String {
    format!("{}-{}-{}", item.id, item.modified_at, item.size_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_key_tracks_file_identity_and_version() {
        let item = MediaItem {
            id: "abc".to_owned(),
            title: "Clip".to_owned(),
            path: PathBuf::from("/tmp/clip.webm"),
            kind: crate::media_library::MediaKind::Gif,
            format: crate::media_library::MediaFormat::Webm,
            size_bytes: 42,
            modified_at: 7,
            search_text: String::new(),
        };

        assert_eq!(preview_cache_key(&item), "abc-7-42");
    }
}
