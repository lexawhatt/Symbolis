use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{self, BufReader},
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
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
    in_flight: HashSet<String>,
    job_tx: Sender<PreviewJobRequest>,
    result_rx: Receiver<PreviewJobResult>,
}

enum PreviewJobRequest {
    Render {
        key: String,
        input: PathBuf,
        output: PathBuf,
    },
}

enum PreviewJobResult {
    Ready { key: String, path: PathBuf },
    Failed { key: String },
}

impl MediaPreviewCache {
    pub(crate) fn new() -> Self {
        let (job_tx, result_rx) = spawn_preview_worker();
        Self {
            dir: dirs::cache_dir().map(|dir| dir.join("symbolis").join("media-thumbs")),
            textures: HashMap::new(),
            in_flight: HashSet::new(),
            job_tx,
            result_rx,
        }
    }

    pub(crate) fn texture(&mut self, ctx: &Context, item: &MediaItem) -> Option<&TextureHandle> {
        self.drain_completed(ctx);

        let key = preview_cache_key(item);
        if !self.textures.contains_key(&key) {
            if let Some(texture) = self.load_cached_texture(ctx, &key) {
                self.textures
                    .insert(key.clone(), CachedMediaPreview::Ready(texture));
            } else {
                self.queue_thumbnail_job(&key, item);
            }
        }

        if self.in_flight.contains(&key) {
            ctx.request_repaint_after(Duration::from_millis(120));
        }

        match self.textures.get(&key) {
            Some(CachedMediaPreview::Ready(texture)) => Some(texture),
            _ => None,
        }
    }

    fn drain_completed(&mut self, ctx: &Context) {
        let mut updated = false;

        while let Ok(result) = self.result_rx.try_recv() {
            match result {
                PreviewJobResult::Ready { key, path } => {
                    self.in_flight.remove(&key);
                    let cached = load_texture(ctx, &key, &path)
                        .map(CachedMediaPreview::Ready)
                        .unwrap_or(CachedMediaPreview::Failed);
                    self.textures.insert(key, cached);
                }
                PreviewJobResult::Failed { key } => {
                    self.in_flight.remove(&key);
                    self.textures.insert(key, CachedMediaPreview::Failed);
                }
            }
            updated = true;
        }

        if updated {
            ctx.request_repaint();
        }
    }

    fn load_cached_texture(&self, ctx: &Context, key: &str) -> Option<TextureHandle> {
        let path = self.cached_png_path(key)?;
        if !path.exists() {
            return None;
        }
        load_texture(ctx, key, &path)
    }

    fn queue_thumbnail_job(&mut self, key: &str, item: &MediaItem) {
        if self.in_flight.contains(key) {
            return;
        }

        let Some(output) = self.cached_png_path(key) else {
            self.textures
                .insert(key.to_owned(), CachedMediaPreview::Failed);
            return;
        };

        let input = item.path.clone();
        let key = key.to_owned();
        self.in_flight.insert(key.clone());
        let request = PreviewJobRequest::Render {
            key: key.clone(),
            input,
            output,
        };
        if self.job_tx.send(request).is_err() {
            self.in_flight.remove(key.as_str());
            self.textures.insert(key, CachedMediaPreview::Failed);
        }
    }

    fn cached_png_path(&self, key: &str) -> Option<PathBuf> {
        let dir = self.dir.as_ref()?;
        fs::create_dir_all(dir).ok()?;
        Some(dir.join(format!("{key}.png")))
    }
}

fn spawn_preview_worker() -> (Sender<PreviewJobRequest>, Receiver<PreviewJobResult>) {
    let (job_tx, job_rx) = mpsc::channel::<PreviewJobRequest>();
    let (result_tx, result_rx) = mpsc::channel::<PreviewJobResult>();

    thread::spawn(move || {
        while let Ok(job) = job_rx.recv() {
            if result_tx.send(run_preview_job(job)).is_err() {
                break;
            }
        }
    });

    (job_tx, result_rx)
}

fn run_preview_job(job: PreviewJobRequest) -> PreviewJobResult {
    match job {
        PreviewJobRequest::Render { key, input, output } => {
            if render_media_thumbnail(&input, &output) {
                PreviewJobResult::Ready { key, path: output }
            } else {
                let _ = fs::remove_file(output);
                PreviewJobResult::Failed { key }
            }
        }
    }
}

fn load_texture(ctx: &Context, key: &str, path: &Path) -> Option<TextureHandle> {
    let image = load_png_color_image(path).ok()?;
    Some(ctx.load_texture(
        format!("symbolis-media-{key}"),
        image,
        TextureOptions::LINEAR,
    ))
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
