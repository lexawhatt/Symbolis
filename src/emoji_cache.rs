use std::{
    collections::{HashMap, HashSet, VecDeque},
    env, fs,
    io::BufReader,
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use eframe::egui::{ColorImage, Context, TextureHandle, TextureOptions};

const EMOJI_TEXTURE_CACHE_LIMIT: usize = 128;
const EMOJI_TEXTURE_CACHE_LIMIT_LOW_MEMORY: usize = 64;

enum CachedEmoji {
    Ready(TextureHandle),
    Failed,
}

pub(crate) struct EmojiCache {
    dir: Option<PathBuf>,
    color_renderer_available: bool,
    textures: HashMap<String, CachedEmoji>,
    texture_order: VecDeque<String>,
    in_flight: HashSet<String>,
    worker: Option<EmojiWorker>,
}

struct EmojiWorker {
    job_tx: Sender<EmojiJobRequest>,
    result_rx: Receiver<EmojiJobResult>,
}

enum EmojiJobRequest {
    Render {
        key: String,
        emoji: String,
        output: PathBuf,
        low_memory: bool,
    },
}

enum EmojiJobResult {
    Ready {
        key: String,
        path: PathBuf,
        low_memory: bool,
    },
    Failed {
        key: String,
        low_memory: bool,
    },
}

impl EmojiCache {
    pub(crate) fn new(color_renderer_available: bool) -> Self {
        Self {
            dir: dirs::cache_dir().map(|dir| dir.join("symbolis").join("emoji")),
            color_renderer_available,
            textures: HashMap::new(),
            texture_order: VecDeque::new(),
            in_flight: HashSet::new(),
            worker: None,
        }
    }

    pub(crate) fn color_renderer_available(&self) -> bool {
        self.color_renderer_available
    }

    pub(crate) fn texture(
        &mut self,
        ctx: &Context,
        emoji: &str,
        low_memory: bool,
    ) -> Option<&TextureHandle> {
        self.drain_completed(ctx);

        let key = cache_key(emoji);
        if !self.textures.contains_key(&key) {
            if let Some(texture) = self.load_cached_texture(ctx, &key) {
                self.textures
                    .insert(key.clone(), CachedEmoji::Ready(texture));
                self.remember_texture_key(&key, low_memory);
            } else {
                self.queue_render_job(&key, emoji, low_memory);
            }
        }

        if self.in_flight.contains(&key) {
            ctx.request_repaint_after(Duration::from_millis(80));
        }

        match self.textures.get(&key) {
            Some(CachedEmoji::Ready(texture)) => Some(texture),
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
                EmojiJobResult::Ready {
                    key,
                    path,
                    low_memory,
                } => {
                    self.in_flight.remove(&key);
                    let cached = load_texture(ctx, &key, &path)
                        .map(CachedEmoji::Ready)
                        .unwrap_or(CachedEmoji::Failed);
                    self.textures.insert(key.clone(), cached);
                    self.remember_texture_key(&key, low_memory);
                }
                EmojiJobResult::Failed { key, low_memory } => {
                    self.in_flight.remove(&key);
                    self.textures.insert(key.clone(), CachedEmoji::Failed);
                    self.remember_texture_key(&key, low_memory);
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

    fn queue_render_job(&mut self, key: &str, emoji: &str, low_memory: bool) {
        if self.in_flight.contains(key) {
            return;
        }

        if !self.color_renderer_available {
            self.textures.insert(key.to_owned(), CachedEmoji::Failed);
            self.remember_texture_key(key, low_memory);
            return;
        }

        let Some(output) = self.cached_png_path(key) else {
            self.textures.insert(key.to_owned(), CachedEmoji::Failed);
            self.remember_texture_key(key, low_memory);
            return;
        };

        let key = key.to_owned();
        self.in_flight.insert(key.clone());
        let request = EmojiJobRequest::Render {
            key: key.clone(),
            emoji: emoji.to_owned(),
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
            self.textures.insert(key.clone(), CachedEmoji::Failed);
            self.remember_texture_key(&key, low_memory);
        }
    }

    fn cached_png_path(&self, key: &str) -> Option<PathBuf> {
        let dir = self.dir.as_ref()?;
        fs::create_dir_all(dir).ok()?;
        Some(dir.join(format!("{key}.png")))
    }

    fn remember_texture_key(&mut self, key: &str, low_memory: bool) {
        if !self.texture_order.iter().any(|existing| existing == key) {
            self.texture_order.push_back(key.to_owned());
        }

        while self.textures.len() > emoji_texture_cache_limit(low_memory) {
            let Some(oldest) = self.texture_order.pop_front() else {
                break;
            };
            if oldest == key && self.textures.len() > 1 {
                self.texture_order.push_back(oldest);
                continue;
            }
            self.textures.remove(&oldest);
        }
    }

    fn ensure_worker(&mut self) {
        if self.worker.is_none() {
            let (job_tx, result_rx) = spawn_emoji_worker();
            self.worker = Some(EmojiWorker { job_tx, result_rx });
        }
    }
}

fn emoji_texture_cache_limit(low_memory: bool) -> usize {
    if low_memory {
        EMOJI_TEXTURE_CACHE_LIMIT_LOW_MEMORY
    } else {
        EMOJI_TEXTURE_CACHE_LIMIT
    }
}

fn spawn_emoji_worker() -> (Sender<EmojiJobRequest>, Receiver<EmojiJobResult>) {
    let (job_tx, job_rx) = mpsc::channel::<EmojiJobRequest>();
    let (result_tx, result_rx) = mpsc::channel::<EmojiJobResult>();

    thread::spawn(move || {
        while let Ok(job) = job_rx.recv() {
            if result_tx.send(run_emoji_job(job)).is_err() {
                break;
            }
        }
    });

    (job_tx, result_rx)
}

fn run_emoji_job(job: EmojiJobRequest) -> EmojiJobResult {
    match job {
        EmojiJobRequest::Render {
            key,
            emoji,
            output,
            low_memory,
        } => {
            if render_emoji_png(&emoji, &output) {
                EmojiJobResult::Ready {
                    key,
                    path: output,
                    low_memory,
                }
            } else {
                let _ = fs::remove_file(output);
                EmojiJobResult::Failed { key, low_memory }
            }
        }
    }
}

fn load_texture(ctx: &Context, key: &str, path: &Path) -> Option<TextureHandle> {
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
