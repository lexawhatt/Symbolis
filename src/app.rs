use std::{
    io,
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};

use eframe::egui::{Context, Key, ViewportCommand};

use crate::{
    data::{DataSource, EmojiGroup, Entry, StoredEntry, load_entries, load_recent, recent_path},
    emoji_cache::EmojiCache,
    media_clipboard::MediaClipboard,
    media_drag::{DragOutBackend, DragPreview, LinuxDragOutBackend},
    media_library::{
        MediaFormat, MediaItem, default_media_paths, export_media_for_transfer,
        favorite_media_path, is_supported_media_path, load_favorite_media_ids, load_recent_media,
        media_index_path, normalize_import_path, recent_media_path, save_favorite_media_ids,
        save_media_as_webm, save_media_index, save_recent_media, scan_media_library,
        store_media_file_for_library,
    },
    media_preview::MediaPreviewCache,
    preflight::{PreflightReport, StartupWarning},
    settings::{
        UiSettings, configure_fonts, configure_style, load_settings, save_settings, settings_path,
    },
};

const RECENT_LIMIT: usize = 48;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MediaImportMode {
    Reference,
    StoreFiles,
}

enum MediaJobRequest {
    StoredImport { original: PathBuf },
    OptimizedCopy { item: MediaItem, title: String },
    ExportForCopy { item: MediaItem },
    ExportForDrag { item: MediaItem },
}

enum MediaJobResult {
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ContentMode {
    Symbols,
    Gifs,
}

impl ContentMode {
    pub(crate) const CHOICES: [ContentMode; 2] = [ContentMode::Symbols, ContentMode::Gifs];

    pub(crate) fn label(self) -> &'static str {
        match self {
            ContentMode::Symbols => "Symbols",
            ContentMode::Gifs => "GIFs",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MediaView {
    Library,
    Favorites,
    RecentlyUsed,
}

impl MediaView {
    pub(crate) fn label(self) -> &'static str {
        match self {
            MediaView::Library => "Local GIFs",
            MediaView::Favorites => "Favorites",
            MediaView::RecentlyUsed => "Recently Used",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Tab {
    Recent,
    Category(crate::data::Category),
    EmojiGroup(EmojiGroup),
    Settings,
}

impl Tab {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Tab::Recent => "Recent",
            Tab::Category(category) => category.label(),
            Tab::EmojiGroup(group) => group.label(),
            Tab::Settings => "Preferences",
        }
    }

    pub(crate) fn icon(self) -> &'static str {
        match self {
            Tab::Recent => "↺",
            Tab::Category(category) => category.icon(),
            Tab::EmojiGroup(group) => group.icon(),
            Tab::Settings => "⚙",
        }
    }
}

pub(crate) struct SymbolisApp {
    pub(crate) entries: Vec<Entry>,
    pub(crate) recent: Vec<Entry>,
    pub(crate) media_items: Vec<MediaItem>,
    pub(crate) recent_media: Vec<MediaItem>,
    pub(crate) favorite_media_ids: Vec<String>,
    pub(crate) content_mode: ContentMode,
    pub(crate) selected_tab: Tab,
    pub(crate) media_view: MediaView,
    pub(crate) query: String,
    pub(crate) gif_query: String,
    pub(crate) gif_import_path_input: String,
    pub(crate) recent_path: Option<PathBuf>,
    pub(crate) recent_media_path: Option<PathBuf>,
    pub(crate) favorite_media_path: Option<PathBuf>,
    pub(crate) media_index_path: Option<PathBuf>,
    pub(crate) settings_path: Option<PathBuf>,
    pub(crate) clipboard: MediaClipboard,
    pub(crate) drag_out: LinuxDragOutBackend,
    pub(crate) status: Option<String>,
    pub(crate) startup_warnings: Vec<StartupWarning>,
    pub(crate) data_source: DataSource,
    pub(crate) settings: UiSettings,
    pub(crate) emoji_cache: EmojiCache,
    pub(crate) media_preview_cache: MediaPreviewCache,
    media_job_tx: Sender<MediaJobRequest>,
    media_job_rx: Receiver<MediaJobResult>,
    active_media_jobs: usize,
}

impl SymbolisApp {
    pub(crate) fn new(cc: &eframe::CreationContext<'_>, preflight: PreflightReport) -> Self {
        configure_fonts(&cc.egui_ctx);

        let settings_path = settings_path();
        let settings = settings_path
            .as_deref()
            .and_then(load_settings)
            .unwrap_or_default();
        configure_style(&cc.egui_ctx, &settings);

        let (entries, data_source) = load_entries();
        let recent_path = recent_path();
        let recent = recent_path
            .as_deref()
            .and_then(load_recent)
            .unwrap_or_default();
        let recent_media_path = recent_media_path();
        let recent_media = load_recent_media(recent_media_path.as_deref());
        let favorite_media_path = favorite_media_path();
        let favorite_media_ids = load_favorite_media_ids(favorite_media_path.as_deref());
        let media_index_path = media_index_path();
        let media_items = scan_media_library(&media_scan_paths(&settings.gif_import_paths));
        if let Err(err) = save_media_index(media_index_path.as_deref(), &media_items) {
            eprintln!("failed to save media index: {err}");
        }
        let (media_job_tx, media_job_rx) = spawn_media_worker();

        Self {
            entries,
            recent,
            media_items,
            recent_media,
            favorite_media_ids,
            content_mode: ContentMode::Symbols,
            selected_tab: Tab::Category(crate::data::Category::Emoji),
            media_view: MediaView::Library,
            query: String::new(),
            gif_query: String::new(),
            gif_import_path_input: String::new(),
            recent_path,
            recent_media_path,
            favorite_media_path,
            media_index_path,
            settings_path,
            clipboard: MediaClipboard::new().expect("clipboard was verified by startup preflight"),
            drag_out: LinuxDragOutBackend::new(preflight.linux_session, preflight.drag_helper),
            status: None,
            startup_warnings: preflight.warnings,
            data_source,
            settings,
            emoji_cache: EmojiCache::new(preflight.color_emoji_renderer),
            media_preview_cache: MediaPreviewCache::new(),
            media_job_tx,
            media_job_rx,
            active_media_jobs: 0,
        }
    }

    pub(crate) fn filtered_entries(&self) -> Vec<Entry> {
        let query = self.query.trim().to_lowercase();

        self.active_entries()
            .iter()
            .filter(|entry| match self.selected_tab {
                Tab::Recent => true,
                Tab::Category(category) => entry.category == category,
                Tab::EmojiGroup(group) => entry.emoji_group == Some(group),
                Tab::Settings => false,
            })
            .filter(|entry| query.is_empty() || entry.search_text.contains(&query))
            .cloned()
            .collect()
    }

    pub(crate) fn filtered_media_items(&self) -> Vec<MediaItem> {
        let query = self.gif_query.trim().to_lowercase();
        let items: Vec<MediaItem> = match self.media_view {
            MediaView::Library => self.media_items.clone(),
            MediaView::Favorites => self
                .media_items
                .iter()
                .filter(|item| self.is_media_favorite(item))
                .cloned()
                .collect(),
            MediaView::RecentlyUsed => self.recent_media.clone(),
        };

        items
            .into_iter()
            .filter(|item| query.is_empty() || item.search_text.contains(&query))
            .collect()
    }

    pub(crate) fn copy_entry(&mut self, entry: &Entry) {
        if let Err(err) = self.clipboard.copy_text(entry.ch.clone()) {
            self.status = Some(format!("Clipboard error: {err}"));
            return;
        }

        self.remember_recent(entry);
        if let Err(err) = self.save_recent() {
            self.status = Some(format!("Recent save error: {err}"));
            return;
        }

        let label = if entry.desc.trim().is_empty() {
            "Copied".to_owned()
        } else {
            format!("Copied {}", entry.desc)
        };
        self.status = Some(label);
    }

    pub(crate) fn copy_media_file(&mut self, item: &MediaItem) {
        if media_transfer_requires_export(item) {
            let item = item.clone();
            let title = item.title.clone();
            if self.queue_media_job(MediaJobRequest::ExportForCopy { item }) {
                self.status = Some(format!("Exporting GIF for clipboard: {title}"));
            }
            return;
        }

        self.copy_media_transfer_path(item, item.path.clone());
    }

    fn copy_media_transfer_path(&mut self, item: &MediaItem, path: PathBuf) {
        if let Err(err) = self.clipboard.copy_file_list(std::slice::from_ref(&path)) {
            self.status = Some(format!("Media clipboard error: {err}"));
            return;
        }

        self.remember_recent_media(item);
        if let Err(err) = self.save_recent_media() {
            self.status = Some(format!("Recent media save error: {err}"));
            return;
        }

        self.status = Some(if media_transfer_requires_export(item) {
            "Exported GIF and copied file".to_owned()
        } else {
            format!("Copied {} file", item.format.label())
        });
    }

    pub(crate) fn drag_media_file(&mut self, item: &MediaItem) {
        if media_transfer_requires_export(item) {
            let item = item.clone();
            let title = item.title.clone();
            if self.queue_media_job(MediaJobRequest::ExportForDrag { item }) {
                self.status = Some(format!("Exporting GIF for drag: {title}"));
            }
            return;
        }

        self.drag_media_transfer_path(item, item.path.clone());
    }

    fn drag_media_transfer_path(&mut self, item: &MediaItem, path: PathBuf) {
        let preview = DragPreview {
            label: item.title.clone(),
            mime: transfer_mime_for_path(&path, item).to_owned(),
        };

        if let Err(err) = self
            .drag_out
            .begin_file_drag(std::slice::from_ref(&path), preview)
        {
            self.status = Some(format!("Drag error: {err}"));
            return;
        }

        self.remember_recent_media(item);
        if let Err(err) = self.save_recent_media() {
            self.status = Some(format!("Recent media save error: {err}"));
            return;
        }
        self.status = Some(format!("Drag helper opened: {}", item.title));
    }

    pub(crate) fn save_optimized_media_copy(&mut self, item: &MediaItem) {
        let item = item.clone();
        let title = item.title.clone();
        let status_title = title.clone();
        if self.queue_media_job(MediaJobRequest::OptimizedCopy { item, title }) {
            self.status = Some(format!("Optimizing WebM copy: {status_title}"));
        }
    }

    pub(crate) fn copy_media_path(&mut self, item: &MediaItem) {
        if let Err(err) = self.clipboard.copy_text(item.path.display().to_string()) {
            self.status = Some(format!("Clipboard error: {err}"));
            return;
        }

        self.status = Some("Copied media path".to_owned());
    }

    pub(crate) fn is_media_favorite(&self, item: &MediaItem) -> bool {
        self.favorite_media_ids.contains(&item.id)
    }

    pub(crate) fn toggle_media_favorite(&mut self, item: &MediaItem) {
        if self.is_media_favorite(item) {
            self.favorite_media_ids.retain(|id| id != &item.id);
            self.status = Some(format!("Removed favorite: {}", item.title));
        } else {
            self.favorite_media_ids.insert(0, item.id.clone());
            self.status = Some(format!("Added favorite: {}", item.title));
        }
        self.favorite_media_ids.truncate(512);

        if let Err(err) = self.save_favorite_media_ids() {
            self.status = Some(format!("Favorites save error: {err}"));
        }
    }

    pub(crate) fn open_media_location(&mut self, item: &MediaItem) {
        let target = item.path.parent().unwrap_or(&item.path);
        match Command::new("xdg-open").arg(target).spawn() {
            Ok(_) => {
                self.status = Some(format!("Opened {}", target.display()));
            }
            Err(err) => {
                self.status = Some(format!("Open location error: {err}"));
            }
        }
    }

    pub(crate) fn delete_media_file(&mut self, item: &MediaItem) {
        let path = item.path.clone();
        let title = item.title.clone();
        let file_was_missing = match std::fs::remove_file(&path) {
            Ok(()) => false,
            Err(err) if err.kind() == io::ErrorKind::NotFound => true,
            Err(err) => {
                self.status = Some(format!("Delete media error: {err}"));
                return;
            }
        };

        let settings_changed = self
            .settings
            .gif_import_paths
            .iter()
            .any(|path| path == &item.path);
        self.settings
            .gif_import_paths
            .retain(|existing| existing != &item.path);

        self.favorite_media_ids.retain(|id| id != &item.id);
        self.recent_media.retain(|existing| existing.id != item.id);

        if settings_changed
            && let Err(err) = save_settings(self.settings_path.as_deref(), &self.settings)
        {
            self.status = Some(format!("Settings save error: {err}"));
            return;
        }
        if let Err(err) = self.save_favorite_media_ids() {
            self.status = Some(format!("Favorites save error: {err}"));
            return;
        }
        if let Err(err) = self.save_recent_media() {
            self.status = Some(format!("Recent media save error: {err}"));
            return;
        }

        self.reload_media_library();
        self.content_mode = ContentMode::Gifs;
        self.status = Some(if file_was_missing {
            format!("Removed missing media: {title}")
        } else {
            format!("Deleted media file: {title}")
        });
    }

    pub(crate) fn add_media_import_path(&mut self, path: PathBuf) {
        self.add_media_import_paths(vec![path]);
    }

    pub(crate) fn add_media_import_paths(&mut self, paths: Vec<PathBuf>) {
        self.add_media_import_paths_with_mode(paths, MediaImportMode::Reference);
    }

    pub(crate) fn add_media_drop_paths(&mut self, paths: Vec<PathBuf>) {
        self.add_media_import_paths_with_mode(paths, MediaImportMode::StoreFiles);
    }

    fn add_media_import_paths_with_mode(&mut self, paths: Vec<PathBuf>, mode: MediaImportMode) {
        let mut accepted = 0;
        let mut added = 0;
        let mut rejected = 0;
        let mut queued = 0;

        for path in paths {
            if !is_supported_media_path(&path) {
                rejected += 1;
                continue;
            }

            if mode == MediaImportMode::StoreFiles && path.is_file() {
                if self.queue_media_job(MediaJobRequest::StoredImport {
                    original: path.clone(),
                }) {
                    queued += 1;
                } else {
                    rejected += 1;
                }
                continue;
            }

            let import_path = normalize_import_path(&path);

            let Some(path) = import_path else {
                rejected += 1;
                continue;
            };

            accepted += 1;
            if !is_default_media_scan_path(&path) && !self.settings.gif_import_paths.contains(&path)
            {
                self.settings.gif_import_paths.push(path);
                added += 1;
            }
        }

        if accepted == 0 && queued == 0 {
            self.status = Some(if rejected == 0 {
                "No media paths dropped".to_owned()
            } else {
                "Drop a folder or .gif/.mp4/.png/.webp/.webm file".to_owned()
            });
            return;
        }

        if added > 0 {
            self.save_settings();
        }
        if accepted > 0 {
            self.reload_media_library();
        }
        if accepted > 0 || queued > 0 {
            self.content_mode = ContentMode::Gifs;
            self.media_view = MediaView::Library;
        }
        let noun = if mode == MediaImportMode::StoreFiles {
            "media item"
        } else {
            "media source"
        };
        let mut status = if accepted > 0 {
            format!(
                "Imported {accepted} {noun}{}; indexed {} files",
                plural_suffix(accepted),
                self.media_items.len()
            )
        } else {
            String::new()
        };
        if queued > 0 {
            let queued_status = format!("queued {queued} media import{}", plural_suffix(queued));
            if status.is_empty() {
                status = queued_status;
            } else {
                status.push_str(&format!("; {queued_status}"));
            }
        };
        if rejected > 0 {
            status.push_str(&format!(
                "; skipped {rejected} unsupported file{}",
                plural_suffix(rejected)
            ));
        }
        self.status = Some(status);
    }

    pub(crate) fn remove_media_import_path(&mut self, path: &std::path::Path) {
        self.settings
            .gif_import_paths
            .retain(|existing| existing != path);
        self.save_settings();
        self.reload_media_library();
    }

    pub(crate) fn reload_media_library(&mut self) {
        self.media_items = scan_media_library(&media_scan_paths(&self.settings.gif_import_paths));
        self.recent_media.retain(|item| item.path.exists());
        if let Err(err) = self.save_recent_media() {
            self.status = Some(format!("Recent media save error: {err}"));
            return;
        }
        match save_media_index(self.media_index_path.as_deref(), &self.media_items) {
            Ok(()) => {
                self.status = Some(format!("Indexed {} media files", self.media_items.len()));
            }
            Err(err) => {
                self.status = Some(format!("Media index save error: {err}"));
            }
        }
    }

    pub(crate) fn clear_recent(&mut self) {
        self.recent.clear();
        if let Err(err) = self.save_recent() {
            self.status = Some(format!("Recent save error: {err}"));
        } else {
            self.status = None;
            self.selected_tab = Tab::Category(crate::data::Category::Emoji);
        }
    }

    pub(crate) fn clear_recent_media(&mut self) {
        self.recent_media.clear();
        match self.save_recent_media() {
            Ok(()) => {
                self.status = Some("Cleared recently used media".to_owned());
            }
            Err(err) => {
                self.status = Some(format!("Recent media save error: {err}"));
            }
        }
    }

    pub(crate) fn save_settings(&mut self) {
        if let Err(err) = save_settings(self.settings_path.as_deref(), &self.settings) {
            self.status = Some(format!("Settings save error: {err}"));
        }
    }

    pub(crate) fn delivery_status(&self) -> String {
        let drag = if self.drag_out.can_drag_files() {
            format!("drag ready via {}", self.drag_out.helper_label())
        } else {
            "drag disabled; clipboard ready".to_owned()
        };
        format!("{} delivery: {drag}", self.drag_out.session_label())
    }

    pub(crate) fn color_emoji_status(&self) -> &'static str {
        if self.emoji_cache.color_renderer_available() {
            "Color emoji: pango-view ready"
        } else {
            "Color emoji: fallback text renderer"
        }
    }

    pub(crate) fn gif_provider_status(&self) -> String {
        format!(
            "{} GIF source: {}",
            self.settings.gif_provider.label(),
            self.settings.gif_provider.status().label()
        )
    }

    fn active_entries(&self) -> &[Entry] {
        match self.selected_tab {
            Tab::Recent => &self.recent,
            Tab::Category(_) => &self.entries,
            Tab::EmojiGroup(_) => &self.entries,
            Tab::Settings => &[],
        }
    }

    fn remember_recent(&mut self, entry: &Entry) {
        self.recent.retain(|existing| existing.ch != entry.ch);
        self.recent.insert(0, entry.clone());
        self.recent.truncate(RECENT_LIMIT);
    }

    fn remember_recent_media(&mut self, item: &MediaItem) {
        self.recent_media.retain(|existing| existing.id != item.id);
        self.recent_media.insert(0, item.clone());
        self.recent_media.truncate(RECENT_LIMIT);
    }

    fn save_recent(&self) -> io::Result<()> {
        let Some(path) = &self.recent_path else {
            return Ok(());
        };

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let recent: Vec<StoredEntry> = self.recent.iter().map(StoredEntry::from).collect();
        let json = serde_json::to_string_pretty(&recent)?;
        std::fs::write(path, json)
    }

    fn save_recent_media(&self) -> io::Result<()> {
        save_recent_media(self.recent_media_path.as_deref(), &self.recent_media)
    }

    fn save_favorite_media_ids(&self) -> io::Result<()> {
        save_favorite_media_ids(
            self.favorite_media_path.as_deref(),
            &self.favorite_media_ids,
        )
    }

    fn queue_media_job(&mut self, job: MediaJobRequest) -> bool {
        self.active_media_jobs += 1;
        if self.media_job_tx.send(job).is_ok() {
            return true;
        }

        self.active_media_jobs = self.active_media_jobs.saturating_sub(1);
        self.status = Some("Media worker is unavailable".to_owned());
        false
    }

    fn poll_media_jobs(&mut self) {
        let mut completed = 0;

        while let Ok(result) = self.media_job_rx.try_recv() {
            completed += 1;
            self.handle_media_job_result(result);
        }

        if completed > 0 {
            self.active_media_jobs = self.active_media_jobs.saturating_sub(completed);
        }
    }

    fn handle_media_job_result(&mut self, result: MediaJobResult) {
        match result {
            MediaJobResult::StoredImport { original, result } => match result {
                Ok(path) => {
                    self.reload_media_library();
                    self.content_mode = ContentMode::Gifs;
                    self.media_view = MediaView::Library;
                    self.status = Some(format!(
                        "Stored optimized media: {}; indexed {} files",
                        media_path_label(&path),
                        self.media_items.len()
                    ));
                }
                Err(err) => {
                    self.import_original_after_storage_error(&original, &err);
                }
            },
            MediaJobResult::OptimizedCopy { title, result } => match result {
                Ok(path) => {
                    self.reload_media_library();
                    self.content_mode = ContentMode::Gifs;
                    self.media_view = MediaView::Library;
                    self.status = Some(format!("Saved WebM copy: {}", path.display()));
                }
                Err(err) => {
                    self.status = Some(format!("WebM save error for {title}: {err}"));
                }
            },
            MediaJobResult::ExportForCopy { item, result } => match result {
                Ok(path) => self.copy_media_transfer_path(&item, path),
                Err(err) => {
                    self.status = Some(format!("Media export error: {err}"));
                }
            },
            MediaJobResult::ExportForDrag { item, result } => match result {
                Ok(path) => self.drag_media_transfer_path(&item, path),
                Err(err) => {
                    self.status = Some(format!("Media export error: {err}"));
                }
            },
        }
    }

    fn import_original_after_storage_error(&mut self, original: &Path, err: &str) {
        let Some(path) = normalize_import_path(original) else {
            self.status = Some(format!("Media storage error: {err}"));
            return;
        };

        if !is_default_media_scan_path(&path) && !self.settings.gif_import_paths.contains(&path) {
            self.settings.gif_import_paths.push(path.clone());
            self.save_settings();
        }
        self.reload_media_library();
        self.content_mode = ContentMode::Gifs;
        self.media_view = MediaView::Library;
        self.status = Some(format!(
            "Imported original: {}; storage warning: {err}",
            media_path_label(&path)
        ));
    }

    fn media_jobs_active(&self) -> bool {
        self.active_media_jobs > 0
    }

    fn request_media_job_repaint(&self, ctx: &Context) {
        if self.media_jobs_active() {
            ctx.request_repaint_after(Duration::from_millis(120));
        }
    }

    fn handle_keyboard(&mut self, ctx: &Context) {
        if ctx.input(|input| input.key_pressed(Key::Escape)) {
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }

        if ctx.input(|input| input.key_pressed(Key::Enter)) {
            match self.content_mode {
                ContentMode::Symbols => {
                    if let Some(entry) = self.filtered_entries().first().cloned() {
                        self.copy_entry(&entry);
                    }
                }
                ContentMode::Gifs => {
                    if let Some(item) = self.filtered_media_items().first().cloned() {
                        self.copy_media_file(&item);
                    }
                }
            }
        }
    }

    fn handle_dropped_media(&mut self, ctx: &Context) {
        let dropped_files = ctx.input(|input| input.raw.dropped_files.clone());
        if dropped_files.is_empty() {
            return;
        }

        let dropped_paths: Vec<PathBuf> = dropped_files
            .iter()
            .filter_map(|file| file.path.clone())
            .collect();
        if dropped_paths
            .iter()
            .all(|path| !is_supported_media_path(path))
        {
            self.status = Some(format!(
                "Unsupported drop: {} file{}",
                dropped_files.len(),
                plural_suffix(dropped_files.len())
            ));
            return;
        }

        self.add_media_drop_paths(dropped_paths);
    }
}

pub(crate) fn hovered_media_drop_count(ctx: &Context) -> usize {
    ctx.input(|input| {
        input
            .raw
            .hovered_files
            .iter()
            .filter(|file| file.path.as_deref().is_none_or(is_supported_media_path))
            .count()
    })
}

pub(crate) fn has_hovered_files(ctx: &Context) -> bool {
    ctx.input(|input| !input.raw.hovered_files.is_empty())
}

fn spawn_media_worker() -> (Sender<MediaJobRequest>, Receiver<MediaJobResult>) {
    let (job_tx, job_rx) = mpsc::channel::<MediaJobRequest>();
    let (result_tx, result_rx) = mpsc::channel::<MediaJobResult>();

    thread::spawn(move || {
        while let Ok(job) = job_rx.recv() {
            if result_tx.send(run_media_job(job)).is_err() {
                break;
            }
        }
    });

    (job_tx, result_rx)
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
    }
}

fn plural_suffix(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

fn media_transfer_requires_export(item: &MediaItem) -> bool {
    matches!(item.format, MediaFormat::Mp4 | MediaFormat::Webm)
}

fn media_path_label(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| path.display().to_string())
}

fn transfer_mime_for_path<'a>(path: &std::path::Path, item: &'a MediaItem) -> &'a str {
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("gif"))
    {
        "image/gif"
    } else {
        item.format.mime()
    }
}

impl eframe::App for SymbolisApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        configure_style(ctx, &self.settings);
        self.poll_media_jobs();
        self.request_media_job_repaint(ctx);
        self.handle_dropped_media(ctx);
        self.handle_keyboard(ctx);
        self.draw(ctx);
    }
}

fn media_scan_paths(import_paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut paths = default_media_paths();
    paths.extend(import_paths.iter().cloned());
    paths
}

fn is_default_media_scan_path(path: &Path) -> bool {
    default_media_paths()
        .iter()
        .any(|default_path| path == default_path || path.starts_with(default_path))
}
