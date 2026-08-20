use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};

use crate::{
    media_library::{
        MediaItem, MediaKind, export_media_for_transfer, save_media_as_webm, save_media_index,
        scan_media_library, store_media_file_for_library,
    },
    settings::FeatureSettings,
    telegram_stickers::{TelegramStickerImportSummary, import_telegram_sticker_set_with_progress},
};

pub(crate) enum MediaJobRequest {
    StoredImport { original: PathBuf },
    OptimizedCopy { item: MediaItem, title: String },
    ExportForCopy { item: MediaItem },
    ExportForDrag { item: MediaItem },
    ImportTelegramStickerSet { set_name: String, token: String },
}

pub(crate) enum MediaJobResult {
    StoredImport {
        original: PathBuf,
        result: Result<PathBuf, String>,
    },
    OptimizedCopy {
        title: String,
        result: Result<PathBuf, String>,
    },
    ExportForCopy {
        item: MediaItem,
        result: Result<PathBuf, String>,
    },
    ExportForDrag {
        item: MediaItem,
        result: Result<PathBuf, String>,
    },
    TelegramStickerImport {
        set_name: String,
        result: Result<TelegramStickerImportSummary, String>,
    },
    TelegramStickerImportProgress {
        set_name: String,
        message: String,
    },
}

pub(crate) enum MediaScanRequest {
    Scan {
        generation: u64,
        paths: Vec<PathBuf>,
        index_path: Option<PathBuf>,
        options: MediaScanOptions,
    },
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct MediaScanOptions {
    include_gifs: bool,
    include_stickers: bool,
    deduplicate: bool,
}

impl MediaScanOptions {
    pub(crate) fn from_features(features: &FeatureSettings) -> Self {
        Self {
            include_gifs: features.gifs,
            include_stickers: features.stickers,
            deduplicate: features.deduplicate_media,
        }
    }

    fn includes_kind(self, kind: MediaKind) -> bool {
        match kind {
            MediaKind::Gif => self.include_gifs,
            MediaKind::Sticker => self.include_stickers,
        }
    }
}

pub(crate) enum MediaScanResult {
    Complete {
        generation: u64,
        items: Vec<MediaItem>,
        index_save_error: Option<String>,
    },
}

pub(crate) enum MediaWatchRequest {
    Watch { paths: Vec<PathBuf> },
    Stop,
}

pub(crate) enum MediaWatchResult {
    Changed,
}

pub(crate) fn spawn_media_worker() -> (Sender<MediaJobRequest>, Receiver<MediaJobResult>) {
    let (job_tx, job_rx) = mpsc::channel::<MediaJobRequest>();
    let (result_tx, result_rx) = mpsc::channel::<MediaJobResult>();

    thread::spawn(move || {
        while let Ok(job) = job_rx.recv() {
            match job {
                MediaJobRequest::ImportTelegramStickerSet { set_name, token } => {
                    let progress_tx = result_tx.clone();
                    let progress_set_name = set_name.clone();
                    let result = run_telegram_sticker_import(set_name, token, |message| {
                        let _ = progress_tx.send(MediaJobResult::TelegramStickerImportProgress {
                            set_name: progress_set_name.clone(),
                            message,
                        });
                    });
                    if result_tx.send(result).is_err() {
                        break;
                    }
                }
                job => {
                    if result_tx.send(run_media_job(job)).is_err() {
                        break;
                    }
                }
            }
        }
    });

    (job_tx, result_rx)
}

fn run_telegram_sticker_import(
    set_name: String,
    token: String,
    progress: impl FnMut(String),
) -> MediaJobResult {
    let result = import_telegram_sticker_set_with_progress(&set_name, &token, progress)
        .map_err(|err| err.to_string());
    MediaJobResult::TelegramStickerImport { set_name, result }
}

pub(crate) fn spawn_media_scan_worker() -> (Sender<MediaScanRequest>, Receiver<MediaScanResult>) {
    let (scan_tx, scan_rx) = mpsc::channel::<MediaScanRequest>();
    let (result_tx, result_rx) = mpsc::channel::<MediaScanResult>();

    thread::spawn(move || {
        while let Ok(request) = scan_rx.recv() {
            if result_tx.send(run_media_scan(request)).is_err() {
                break;
            }
        }
    });

    (scan_tx, result_rx)
}

pub(crate) fn spawn_media_watch_worker() -> (Sender<MediaWatchRequest>, Receiver<MediaWatchResult>)
{
    let (request_tx, request_rx) = mpsc::channel::<MediaWatchRequest>();
    let (result_tx, result_rx) = mpsc::channel::<MediaWatchResult>();

    thread::spawn(move || run_media_watch_worker(request_rx, result_tx));

    (request_tx, result_rx)
}

fn run_media_watch_worker(
    request_rx: Receiver<MediaWatchRequest>,
    result_tx: Sender<MediaWatchResult>,
) {
    use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

    let (event_tx, event_rx) = mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher: Option<RecommendedWatcher> = None;

    loop {
        match request_rx.recv_timeout(Duration::from_millis(250)) {
            Ok(MediaWatchRequest::Watch { paths }) => {
                watcher = RecommendedWatcher::new(
                    {
                        let event_tx = event_tx.clone();
                        move |result| {
                            let _ = event_tx.send(result);
                        }
                    },
                    Config::default(),
                )
                .ok();

                if let Some(watcher) = watcher.as_mut() {
                    for target in media_watch_targets(&paths) {
                        let mode = if target.is_dir() {
                            RecursiveMode::Recursive
                        } else {
                            RecursiveMode::NonRecursive
                        };
                        let _ = watcher.watch(&target, mode);
                    }
                }
            }
            Ok(MediaWatchRequest::Stop) => {
                watcher = None;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        let _watcher_is_active = watcher.is_some();

        let mut changed = false;
        while let Ok(event) = event_rx.try_recv() {
            let Ok(event) = event else {
                continue;
            };
            if matches!(
                event.kind,
                EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) | EventKind::Any
            ) {
                changed = true;
            }
        }

        if changed {
            thread::sleep(Duration::from_millis(500));
            while event_rx.try_recv().is_ok() {}
            if result_tx.send(MediaWatchResult::Changed).is_err() {
                break;
            }
        }
    }
}

fn run_media_job(job: MediaJobRequest) -> MediaJobResult {
    match job {
        MediaJobRequest::StoredImport { original } => {
            let result = store_media_file_for_library(&original).map_err(|err| err.to_string());
            MediaJobResult::StoredImport { original, result }
        }
        MediaJobRequest::OptimizedCopy { item, title } => {
            let result = save_media_as_webm(&item).map_err(|err| err.to_string());
            MediaJobResult::OptimizedCopy { title, result }
        }
        MediaJobRequest::ExportForCopy { item } => {
            let result = export_media_for_transfer(&item).map_err(|err| err.to_string());
            MediaJobResult::ExportForCopy { item, result }
        }
        MediaJobRequest::ExportForDrag { item } => {
            let result = export_media_for_transfer(&item).map_err(|err| err.to_string());
            MediaJobResult::ExportForDrag { item, result }
        }
        MediaJobRequest::ImportTelegramStickerSet { set_name, token } => {
            run_telegram_sticker_import(set_name, token, |_| {})
        }
    }
}

fn run_media_scan(request: MediaScanRequest) -> MediaScanResult {
    match request {
        MediaScanRequest::Scan {
            generation,
            paths,
            index_path,
            options,
        } => {
            let mut items = scan_media_library(&paths, options.deduplicate);
            items.retain(|item| options.includes_kind(item.kind));
            let index_save_error = save_media_index(index_path.as_deref(), &items)
                .err()
                .map(|err| err.to_string());
            MediaScanResult::Complete {
                generation,
                items,
                index_save_error,
            }
        }
    }
}

pub(crate) fn media_job_request_label(job: &MediaJobRequest) -> String {
    match job {
        MediaJobRequest::StoredImport { original } => {
            format!("store import {}", media_path_label(original))
        }
        MediaJobRequest::OptimizedCopy { title, .. } => {
            format!("save optimized WebM for {title}")
        }
        MediaJobRequest::ExportForCopy { item } => {
            format!("export for clipboard {}", item.title)
        }
        MediaJobRequest::ExportForDrag { item } => {
            format!("export for drag {}", item.title)
        }
        MediaJobRequest::ImportTelegramStickerSet { set_name, .. } => {
            format!("import Telegram stickers {set_name}")
        }
    }
}

pub(crate) fn media_job_result_label(result: &MediaJobResult) -> String {
    match result {
        MediaJobResult::StoredImport { original, result } => match result {
            Ok(path) => format!(
                "job complete: stored {} -> {}",
                media_path_label(original),
                media_path_label(path)
            ),
            Err(err) => format!("job failed: store {}: {err}", media_path_label(original)),
        },
        MediaJobResult::OptimizedCopy { title, result } => match result {
            Ok(path) => format!(
                "job complete: optimized {title} -> {}",
                media_path_label(path)
            ),
            Err(err) => format!("job failed: optimize {title}: {err}"),
        },
        MediaJobResult::ExportForCopy { item, result } => match result {
            Ok(path) => format!(
                "job complete: exported {} for clipboard -> {}",
                item.title,
                media_path_label(path)
            ),
            Err(err) => format!("job failed: export {} for clipboard: {err}", item.title),
        },
        MediaJobResult::ExportForDrag { item, result } => match result {
            Ok(path) => format!(
                "job complete: exported {} for drag -> {}",
                item.title,
                media_path_label(path)
            ),
            Err(err) => format!("job failed: export {} for drag: {err}", item.title),
        },
        MediaJobResult::TelegramStickerImport { set_name, result } => match result {
            Ok(summary) => format!(
                "job complete: Telegram {set_name}; imported {}, skipped animated {}, unsupported {}, failed {}",
                summary.imported,
                summary.skipped_animated,
                summary.skipped_unsupported,
                summary.failed
            ),
            Err(err) => format!("job failed: Telegram {set_name}: {err}"),
        },
        MediaJobResult::TelegramStickerImportProgress { set_name, message } => {
            format!("job progress: Telegram {set_name}: {message}")
        }
    }
}

pub(crate) fn media_job_result_is_terminal(result: &MediaJobResult) -> bool {
    !matches!(result, MediaJobResult::TelegramStickerImportProgress { .. })
}

pub(crate) fn media_path_label(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| path.display().to_string())
}

fn media_watch_targets(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut targets = Vec::new();

    for path in paths {
        let target = if path.is_file() {
            path.parent().map(Path::to_path_buf)
        } else if path.exists() {
            Some(path.clone())
        } else {
            path.parent()
                .filter(|parent| parent.exists())
                .map(Path::to_path_buf)
        };

        let Some(target) = target else {
            continue;
        };
        let target = fs::canonicalize(&target).unwrap_or(target);
        if seen.insert(target.clone()) {
            targets.push(target);
        }
    }

    targets
}
