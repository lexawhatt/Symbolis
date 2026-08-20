use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    io::{self, BufReader},
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};

use eframe::egui::{ColorImage, Context, TextureHandle, TextureOptions};

use crate::{
    media_library::{MediaFormat, MediaItem, MediaKind},
    settings::{MEDIA_PREVIEW_FRAMERATE_MAX_FPS, MEDIA_PREVIEW_FRAMERATE_MIN_FPS},
};

const STATIC_TEXTURE_CACHE_LIMIT: usize = 256;
const STATIC_TEXTURE_CACHE_LIMIT_LOW_MEMORY: usize = 64;
const ANIMATED_TEXTURE_CACHE_LIMIT: usize = 2;
const ANIMATED_TEXTURE_CACHE_LIMIT_LOW_MEMORY: usize = 1;
const ANIMATED_PREVIEW_MAX_FRAMES: usize = 12;

enum CachedMediaPreview {
    Ready(TextureHandle),
    Failed,
}

enum CachedAnimatedMediaPreview {
    Ready(Vec<TextureHandle>),
    Failed,
}

pub(crate) struct MediaPreviewCache {
    dir: Option<PathBuf>,
    textures: HashMap<String, CachedMediaPreview>,
    animated_textures: HashMap<String, CachedAnimatedMediaPreview>,
    texture_order: VecDeque<String>,
    animated_texture_order: VecDeque<String>,
    in_flight: HashSet<String>,
    animated_in_flight: HashSet<String>,
    worker: Option<PreviewWorker>,
}

struct PreviewWorker {
    job_tx: Sender<PreviewJobRequest>,
    result_rx: Receiver<PreviewJobResult>,
}

enum PreviewJobRequest {
    Render {
        key: String,
        input: PathBuf,
        output: PathBuf,
        low_memory: bool,
    },
    RenderAnimation {
        key: String,
        input: PathBuf,
        output_dir: PathBuf,
        framerate_fps: u32,
    },
}

enum PreviewJobResult {
    Ready { key: String, path: PathBuf },
    AnimationReady { key: String, paths: Vec<PathBuf> },
    Failed { key: String },
    AnimationFailed { key: String },
}

impl MediaPreviewCache {
    pub(crate) fn new() -> Self {
        Self {
            dir: dirs::cache_dir().map(|dir| dir.join("symbolis").join("media-thumbs")),
            textures: HashMap::new(),
            animated_textures: HashMap::new(),
            texture_order: VecDeque::new(),
            animated_texture_order: VecDeque::new(),
            in_flight: HashSet::new(),
            animated_in_flight: HashSet::new(),
            worker: None,
        }
    }

    pub(crate) fn texture(
        &mut self,
        ctx: &Context,
        item: &MediaItem,
        low_memory: bool,
    ) -> Option<&TextureHandle> {
        self.drain_completed(ctx);

        let key = preview_cache_key(item, low_memory);
        if !self.textures.contains_key(&key) {
            if let Some(texture) = self.load_cached_texture(ctx, &key) {
                self.textures
                    .insert(key.clone(), CachedMediaPreview::Ready(texture));
                self.remember_texture_key(&key, low_memory);
            } else {
                self.queue_thumbnail_job(&key, item, low_memory);
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

    pub(crate) fn animated_texture(
        &mut self,
        ctx: &Context,
        item: &MediaItem,
        now_seconds: f64,
        low_memory: bool,
        framerate_fps: u32,
    ) -> Option<&TextureHandle> {
        if !media_can_animate(item) {
            return None;
        }

        self.drain_completed(ctx);

        let framerate_fps = normalized_animation_fps(framerate_fps);
        let key = animated_preview_cache_key(item, low_memory, framerate_fps);
        if !self.animated_textures.contains_key(&key) {
            if let Some(textures) = self.load_cached_animation(ctx, &key) {
                self.animated_textures
                    .insert(key.clone(), CachedAnimatedMediaPreview::Ready(textures));
                self.remember_animated_texture_key(&key, low_memory);
            } else {
                self.queue_animation_job(&key, item, framerate_fps);
            }
        }

        if self.animated_in_flight.contains(&key) {
            ctx.request_repaint_after(Duration::from_millis(120));
        }

        match self.animated_textures.get(&key) {
            Some(CachedAnimatedMediaPreview::Ready(frames)) if !frames.is_empty() => {
                ctx.request_repaint_after(Duration::from_millis(1000 / u64::from(framerate_fps)));
                let index = ((now_seconds * f64::from(framerate_fps)) as usize) % frames.len();
                frames.get(index)
            }
            _ => None,
        }
    }

    fn drain_completed(&mut self, ctx: &Context) {
        let Some(worker) = self.worker.as_ref() else {
            return;
        };
        let mut updated = false;
        let mut results = Vec::new();

        while let Ok(result) = worker.result_rx.try_recv() {
            results.push(result);
        }

        for result in results {
            match result {
                PreviewJobResult::Ready { key, path } => {
                    self.in_flight.remove(&key);
                    let cached = load_texture(ctx, &key, &path)
                        .map(CachedMediaPreview::Ready)
                        .unwrap_or(CachedMediaPreview::Failed);
                    self.textures.insert(key.clone(), cached);
                    self.remember_texture_key(&key, low_memory_from_preview_key(&key));
                }
                PreviewJobResult::AnimationReady { key, paths } => {
                    self.animated_in_flight.remove(&key);
                    let frames = paths
                        .iter()
                        .enumerate()
                        .filter_map(|(index, path)| {
                            load_texture(ctx, &format!("{key}-{index}"), path)
                        })
                        .collect::<Vec<_>>();
                    let cached = if frames.is_empty() {
                        CachedAnimatedMediaPreview::Failed
                    } else {
                        CachedAnimatedMediaPreview::Ready(frames)
                    };
                    self.animated_textures.insert(key.clone(), cached);
                    self.remember_animated_texture_key(&key, low_memory_from_preview_key(&key));
                }
                PreviewJobResult::Failed { key } => {
                    self.in_flight.remove(&key);
                    self.textures
                        .insert(key.clone(), CachedMediaPreview::Failed);
                    self.remember_texture_key(&key, low_memory_from_preview_key(&key));
                }
                PreviewJobResult::AnimationFailed { key } => {
                    self.animated_in_flight.remove(&key);
                    self.animated_textures
                        .insert(key.clone(), CachedAnimatedMediaPreview::Failed);
                    self.remember_animated_texture_key(&key, low_memory_from_preview_key(&key));
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

    fn load_cached_animation(&self, ctx: &Context, key: &str) -> Option<Vec<TextureHandle>> {
        let paths = cached_animation_frame_paths(&self.cached_animation_dir(key)?);
        if paths.is_empty() {
            return None;
        }

        let textures = paths
            .iter()
            .enumerate()
            .filter_map(|(index, path)| load_texture(ctx, &format!("{key}-{index}"), path))
            .collect::<Vec<_>>();
        if textures.is_empty() {
            None
        } else {
            Some(textures)
        }
    }

    fn queue_thumbnail_job(&mut self, key: &str, item: &MediaItem, low_memory: bool) {
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
            low_memory,
        };
        self.ensure_worker();
        let sent = self
            .worker
            .as_ref()
            .is_some_and(|worker| worker.job_tx.send(request).is_ok());
        if !sent {
            self.worker = None;
            self.in_flight.remove(key.as_str());
            self.textures
                .insert(key.clone(), CachedMediaPreview::Failed);
            self.remember_texture_key(&key, low_memory);
        }
    }

    fn queue_animation_job(&mut self, key: &str, item: &MediaItem, framerate_fps: u32) {
        if self.animated_in_flight.contains(key) {
            return;
        }

        let Some(output_dir) = self.cached_animation_dir(key) else {
            self.animated_textures
                .insert(key.to_owned(), CachedAnimatedMediaPreview::Failed);
            return;
        };

        let input = item.path.clone();
        let key = key.to_owned();
        self.animated_in_flight.insert(key.clone());
        let request = PreviewJobRequest::RenderAnimation {
            key: key.clone(),
            input,
            output_dir,
            framerate_fps,
        };
        self.ensure_worker();
        let sent = self
            .worker
            .as_ref()
            .is_some_and(|worker| worker.job_tx.send(request).is_ok());
        if !sent {
            self.worker = None;
            self.animated_in_flight.remove(key.as_str());
            self.animated_textures
                .insert(key.clone(), CachedAnimatedMediaPreview::Failed);
            self.remember_animated_texture_key(&key, low_memory_from_preview_key(&key));
        }
    }

    fn cached_png_path(&self, key: &str) -> Option<PathBuf> {
        let dir = self.dir.as_ref()?;
        fs::create_dir_all(dir).ok()?;
        Some(dir.join(format!("{key}.png")))
    }

    fn cached_animation_dir(&self, key: &str) -> Option<PathBuf> {
        let dir = self.dir.as_ref()?.join(format!("{key}-frames"));
        fs::create_dir_all(&dir).ok()?;
        Some(dir)
    }

    fn remember_texture_key(&mut self, key: &str, low_memory: bool) {
        remember_cache_key(
            &mut self.textures,
            &mut self.texture_order,
            key,
            static_texture_cache_limit(low_memory),
        );
    }

    fn remember_animated_texture_key(&mut self, key: &str, low_memory: bool) {
        remember_cache_key(
            &mut self.animated_textures,
            &mut self.animated_texture_order,
            key,
            animated_texture_cache_limit(low_memory),
        );
    }

    fn ensure_worker(&mut self) {
        if self.worker.is_none() {
            let (job_tx, result_rx) = spawn_preview_worker();
            self.worker = Some(PreviewWorker { job_tx, result_rx });
        }
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
        PreviewJobRequest::Render {
            key,
            input,
            output,
            low_memory,
        } => {
            if render_media_thumbnail(&input, &output, low_memory) {
                PreviewJobResult::Ready { key, path: output }
            } else {
                let _ = fs::remove_file(output);
                PreviewJobResult::Failed { key }
            }
        }
        PreviewJobRequest::RenderAnimation {
            key,
            input,
            output_dir,
            framerate_fps,
        } => {
            let paths = render_media_animation(&input, &output_dir, framerate_fps);
            if paths.is_empty() {
                let _ = fs::remove_dir_all(output_dir);
                PreviewJobResult::AnimationFailed { key }
            } else {
                PreviewJobResult::AnimationReady { key, paths }
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

fn render_media_thumbnail(input: &Path, output: &Path, low_memory: bool) -> bool {
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
        .arg(thumbnail_scale_filter(low_memory))
        .arg(output)
        .status()
        .is_ok_and(|status| status.success())
}

fn render_media_animation(input: &Path, output_dir: &Path, framerate_fps: u32) -> Vec<PathBuf> {
    let _ = fs::remove_dir_all(output_dir);
    if fs::create_dir_all(output_dir).is_err() {
        return Vec::new();
    }

    let output_pattern = output_dir.join("frame_%03d.png");
    let framerate_fps = normalized_animation_fps(framerate_fps);
    let status = Command::new("ffmpeg")
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-y")
        .arg("-i")
        .arg(input)
        .arg("-vf")
        .arg(format!(
            "fps={framerate_fps},scale=320:240:force_original_aspect_ratio=decrease:flags=lanczos,format=rgba"
        ))
        .arg("-frames:v")
        .arg(ANIMATED_PREVIEW_MAX_FRAMES.to_string())
        .arg(output_pattern)
        .status();

    if !status.is_ok_and(|status| status.success()) {
        return Vec::new();
    }

    cached_animation_frame_paths(output_dir)
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

fn preview_cache_key(item: &MediaItem, low_memory: bool) -> String {
    format!(
        "{}-{}-{}-{}",
        preview_profile_key(low_memory),
        item.id,
        item.modified_at,
        item.size_bytes
    )
}

fn animated_preview_cache_key(item: &MediaItem, low_memory: bool, framerate_fps: u32) -> String {
    format!(
        "{}-anim-fps{}",
        preview_cache_key(item, low_memory),
        normalized_animation_fps(framerate_fps)
    )
}

fn preview_profile_key(low_memory: bool) -> &'static str {
    if low_memory { "low" } else { "normal" }
}

fn low_memory_from_preview_key(key: &str) -> bool {
    key.starts_with("low-")
}

fn static_texture_cache_limit(low_memory: bool) -> usize {
    if low_memory {
        STATIC_TEXTURE_CACHE_LIMIT_LOW_MEMORY
    } else {
        STATIC_TEXTURE_CACHE_LIMIT
    }
}

fn animated_texture_cache_limit(low_memory: bool) -> usize {
    if low_memory {
        ANIMATED_TEXTURE_CACHE_LIMIT_LOW_MEMORY
    } else {
        ANIMATED_TEXTURE_CACHE_LIMIT
    }
}

fn normalized_animation_fps(framerate_fps: u32) -> u32 {
    framerate_fps.clamp(
        MEDIA_PREVIEW_FRAMERATE_MIN_FPS,
        MEDIA_PREVIEW_FRAMERATE_MAX_FPS,
    )
}

fn thumbnail_scale_filter(low_memory: bool) -> &'static str {
    if low_memory {
        "scale=256:192:force_original_aspect_ratio=decrease:flags=lanczos,format=rgba"
    } else {
        "scale=320:240:force_original_aspect_ratio=decrease:flags=lanczos,format=rgba"
    }
}

fn cached_animation_frame_paths(dir: &Path) -> Vec<PathBuf> {
    let mut paths = fs::read_dir(dir)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "png"))
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

fn media_can_animate(item: &MediaItem) -> bool {
    item.kind == MediaKind::Gif
        && matches!(
            item.format,
            MediaFormat::Gif | MediaFormat::Mp4 | MediaFormat::Webm
        )
}

fn remember_cache_key<T>(
    cache: &mut HashMap<String, T>,
    order: &mut VecDeque<String>,
    key: &str,
    limit: usize,
) {
    if !order.iter().any(|existing| existing == key) {
        order.push_back(key.to_owned());
    }

    while cache.len() > limit {
        let Some(oldest) = order.pop_front() else {
            break;
        };
        if oldest == key && cache.len() > 1 {
            order.push_back(oldest);
            continue;
        }
        cache.remove(&oldest);
    }
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

        assert_eq!(preview_cache_key(&item, false), "normal-abc-7-42");
        assert_eq!(preview_cache_key(&item, true), "low-abc-7-42");
        assert_eq!(
            animated_preview_cache_key(&item, false, 6),
            "normal-abc-7-42-anim-fps6"
        );
        assert_eq!(
            animated_preview_cache_key(&item, true, 12),
            "low-abc-7-42-anim-fps12"
        );
    }

    #[test]
    fn animated_preview_framerate_is_clamped_for_cache_key() {
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

        assert_eq!(
            animated_preview_cache_key(&item, false, 0),
            "normal-abc-7-42-anim-fps1"
        );
        assert_eq!(
            animated_preview_cache_key(&item, false, 120),
            "normal-abc-7-42-anim-fps24"
        );
    }
}
