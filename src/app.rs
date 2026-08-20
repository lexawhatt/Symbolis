use std::{
    collections::{BTreeMap, HashSet, VecDeque},
    fs, io,
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc::{Receiver, Sender},
    time::{Duration, Instant},
};

use eframe::egui::{Context, Key, ViewportCommand};

use crate::{
    data::{DataSource, EmojiGroup, Entry, StoredEntry, load_entries, load_recent, recent_path},
    dev_metrics::DevMetricsSampler,
    emoji_cache::EmojiCache,
    global_hotkeys::GlobalHotkeyRuntime,
    ipc::{IpcCommand, IpcServer},
    media_clipboard::MediaClipboard,
    media_drag::{DragOutBackend, DragPreview, LinuxDragOutBackend},
    media_library::{
        MediaFormat, MediaItem, MediaKind, default_media_paths, favorite_media_path,
        is_supported_media_path, load_favorite_media_ids, load_media_index, load_recent_media,
        media_index_path, media_root, normalize_import_path, recent_media_path,
        save_favorite_media_ids, save_recent_media,
    },
    media_preview::MediaPreviewCache,
    media_runtime::{
        MediaJobRequest, MediaJobResult, MediaScanOptions, MediaScanRequest, MediaScanResult,
        MediaWatchRequest, MediaWatchResult, media_job_request_label, media_job_result_is_terminal,
        media_job_result_label, media_path_label, spawn_media_scan_worker,
        spawn_media_watch_worker, spawn_media_worker,
    },
    preflight::{PreflightReport, StartupWarning},
    settings::{
        FeatureSettings, HotkeyAction, UiSettings, configure_fonts, configure_style, load_settings,
        save_settings, settings_path,
    },
    telegram_stickers::{
        TELEGRAM_BOT_TOKEN_ENV, clear_saved_telegram_bot_token, load_saved_telegram_bot_token,
        save_telegram_bot_token, sticker_set_name_from_input, telegram_bot_token,
        telegram_bot_token_from_env, telegram_secret_path,
    },
};

const RECENT_LIMIT: usize = 48;
const DEV_LOG_LIMIT: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MediaImportMode {
    Reference,
    StoreFiles,
}

#[derive(Default)]
struct MediaDeletionSummary {
    deleted: usize,
    missing: usize,
    errors: Vec<String>,
}

impl MediaDeletionSummary {
    fn delete_files(items: &[MediaItem]) -> Self {
        let mut summary = Self::default();

        for item in items {
            match fs::remove_file(&item.path) {
                Ok(()) => summary.deleted += 1,
                Err(err) if err.kind() == io::ErrorKind::NotFound => summary.missing += 1,
                Err(err) => summary.errors.push(format!("{}: {err}", item.title)),
            }
        }

        summary
    }

    fn status(&self, prefix: impl Into<String>) -> String {
        let mut status = format!(
            "{}: {} file{}",
            prefix.into(),
            self.deleted,
            plural_suffix(self.deleted)
        );
        if self.missing > 0 {
            status.push_str(&format!(
                "; removed {} missing file{}",
                self.missing,
                plural_suffix(self.missing)
            ));
        }
        if !self.errors.is_empty() {
            status.push_str(&format!(
                "; {} delete error{}",
                self.errors.len(),
                plural_suffix(self.errors.len())
            ));
        }
        if let Some(first_error) = self.errors.first() {
            status.push_str(&format!(": {first_error}"));
        }

        status
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ContentMode {
    Symbols,
    Stickers,
    Gifs,
}

impl ContentMode {
    pub(crate) const CHOICES: [ContentMode; 3] = [
        ContentMode::Symbols,
        ContentMode::Stickers,
        ContentMode::Gifs,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            ContentMode::Symbols => "Emoji",
            ContentMode::Stickers => "Stickers",
            ContentMode::Gifs => "GIFs",
        }
    }

    pub(crate) fn media_kind(self) -> Option<MediaKind> {
        match self {
            ContentMode::Symbols => None,
            ContentMode::Stickers => Some(MediaKind::Sticker),
            ContentMode::Gifs => Some(MediaKind::Gif),
        }
    }

    pub(crate) fn first_enabled(features: &FeatureSettings) -> Self {
        if features.symbols {
            Self::Symbols
        } else if features.stickers {
            Self::Stickers
        } else {
            Self::Gifs
        }
    }

    pub(crate) fn enabled(self, features: &FeatureSettings) -> bool {
        match self {
            Self::Symbols => features.symbols,
            Self::Stickers => features.stickers,
            Self::Gifs => features.gifs,
        }
    }

    fn for_media_kind(kind: MediaKind) -> Self {
        match kind {
            MediaKind::Gif => Self::Gifs,
            MediaKind::Sticker => Self::Stickers,
        }
    }

    fn for_media_path(path: &Path) -> Self {
        match path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("png" | "webp") => Self::Stickers,
            _ => Self::Gifs,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AppView {
    Main,
    Settings,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MediaView {
    Library,
    Favorites,
    RecentlyUsed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MediaItemSource {
    Library(usize),
    Recent(usize),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StickerPack {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DevLogEntry {
    pub(crate) elapsed_ms: u128,
    pub(crate) message: String,
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
    pub(crate) selected_media_ids: HashSet<String>,
    pub(crate) app_view: AppView,
    pub(crate) content_mode: ContentMode,
    pub(crate) selected_tab: Tab,
    pub(crate) media_view: MediaView,
    pub(crate) selected_sticker_pack_id: Option<String>,
    pub(crate) query: String,
    pub(crate) gif_query: String,
    pub(crate) gif_import_path_input: String,
    pub(crate) telegram_bot_token_input: String,
    pub(crate) telegram_bot_token_saved: bool,
    pub(crate) telegram_bot_token_guide_visible: bool,
    pub(crate) dev_panel_open: bool,
    pub(crate) clear_everything_confirm: bool,
    pub(crate) capture_hotkey_action: Option<HotkeyAction>,
    pub(crate) hidden_to_background: bool,
    pub(crate) allow_close: bool,
    pub(crate) dev_metrics: DevMetricsSampler,
    pub(crate) app_started_at: Instant,
    pub(crate) dev_log: VecDeque<DevLogEntry>,
    pub(crate) pending_sticker_pack_delete: Option<StickerPack>,
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
    pub(crate) global_hotkeys: GlobalHotkeyRuntime,
    pub(crate) ipc_server: Option<IpcServer>,
    media_job_tx: Sender<MediaJobRequest>,
    media_job_rx: Receiver<MediaJobResult>,
    active_media_jobs: usize,
    media_scan_tx: Sender<MediaScanRequest>,
    media_scan_rx: Receiver<MediaScanResult>,
    media_watch_tx: Sender<MediaWatchRequest>,
    media_watch_rx: Receiver<MediaWatchResult>,
    active_media_scans: usize,
    media_scan_generation: u64,
    media_scan_completion_status: Option<(u64, String)>,
}

impl SymbolisApp {
    pub(crate) fn new(
        cc: &eframe::CreationContext<'_>,
        preflight: PreflightReport,
        initial_command: Option<IpcCommand>,
    ) -> Result<Self, String> {
        let settings_path = settings_path();
        let mut settings = settings_path
            .as_deref()
            .and_then(load_settings)
            .unwrap_or_default();
        settings.features.ensure_any_content_enabled();
        configure_fonts(&cc.egui_ctx, &settings);
        configure_style(&cc.egui_ctx, &settings);

        let (entries, data_source) = load_entries(settings.features.symbols);
        let recent_path = recent_path();
        let recent = if settings.features.symbols {
            recent_path
                .as_deref()
                .and_then(load_recent)
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let recent_media_path = recent_media_path();
        let recent_media = load_recent_media(recent_media_path.as_deref());
        let favorite_media_path = favorite_media_path();
        let favorite_media_ids = load_favorite_media_ids(favorite_media_path.as_deref());
        let media_index_path = media_index_path();
        let media_items = load_media_index(media_index_path.as_deref());
        let telegram_bot_token_input = load_saved_telegram_bot_token().unwrap_or_default();
        let telegram_bot_token_saved = !telegram_bot_token_input.is_empty();
        let app_started_at = Instant::now();
        let global_hotkeys = GlobalHotkeyRuntime::new(&settings.hotkeys, cc.egui_ctx.clone());
        let ipc_server = IpcServer::start(cc.egui_ctx.clone()).ok();
        let (media_job_tx, media_job_rx) = spawn_media_worker();
        let (media_scan_tx, media_scan_rx) = spawn_media_scan_worker();
        let (media_watch_tx, media_watch_rx) = spawn_media_watch_worker();
        let clipboard = MediaClipboard::new()
            .map_err(|err| format!("Clipboard backend became unavailable: {err}"))?;

        let mut app = Self {
            entries,
            recent,
            media_items,
            recent_media,
            favorite_media_ids,
            selected_media_ids: HashSet::new(),
            app_view: AppView::Main,
            content_mode: ContentMode::first_enabled(&settings.features),
            selected_tab: Tab::Category(crate::data::Category::Emoji),
            media_view: MediaView::Library,
            selected_sticker_pack_id: None,
            query: String::new(),
            gif_query: String::new(),
            gif_import_path_input: String::new(),
            telegram_bot_token_input,
            telegram_bot_token_saved,
            telegram_bot_token_guide_visible: false,
            dev_panel_open: false,
            clear_everything_confirm: false,
            capture_hotkey_action: None,
            hidden_to_background: false,
            allow_close: false,
            dev_metrics: DevMetricsSampler::default(),
            app_started_at,
            dev_log: VecDeque::new(),
            pending_sticker_pack_delete: None,
            recent_path,
            recent_media_path,
            favorite_media_path,
            media_index_path,
            settings_path,
            clipboard,
            drag_out: LinuxDragOutBackend::new(preflight.linux_session, preflight.drag_helper),
            status: None,
            startup_warnings: preflight.warnings,
            data_source,
            settings,
            emoji_cache: EmojiCache::new(preflight.color_emoji_renderer),
            media_preview_cache: MediaPreviewCache::new(),
            global_hotkeys,
            ipc_server,
            media_job_tx,
            media_job_rx,
            active_media_jobs: 0,
            media_scan_tx,
            media_scan_rx,
            media_watch_tx,
            media_watch_rx,
            active_media_scans: 0,
            media_scan_generation: 0,
            media_scan_completion_status: None,
        };
        app.retain_enabled_media_state();
        app.reload_media_library();
        app.update_media_watcher();
        if let Some(command) = initial_command {
            app.apply_ipc_command(command, &cc.egui_ctx);
        }
        Ok(app)
    }

    pub(crate) fn filtered_entry_indices(&self) -> Vec<usize> {
        if self.app_view != AppView::Main {
            return Vec::new();
        }

        let query = self.query.trim().to_lowercase();

        self.active_entries()
            .iter()
            .enumerate()
            .filter(|entry| match self.selected_tab {
                Tab::Recent => true,
                Tab::Category(category) => entry.1.category == category,
                Tab::EmojiGroup(group) => entry.1.emoji_group == Some(group),
                Tab::Settings => false,
            })
            .filter(|(_, entry)| query.is_empty() || entry.search_text.contains(&query))
            .map(|(index, _)| index)
            .collect()
    }

    pub(crate) fn entry_at_active_index(&self, index: usize) -> Option<Entry> {
        self.active_entries().get(index).cloned()
    }

    pub(crate) fn filtered_media_sources(&self) -> Vec<MediaItemSource> {
        if self.app_view != AppView::Main {
            return Vec::new();
        }

        let Some(kind) = self.content_mode.media_kind() else {
            return Vec::new();
        };
        let query = self.gif_query.trim().to_lowercase();

        match self.media_view {
            MediaView::Library => self
                .media_items
                .iter()
                .enumerate()
                .filter(|(_, item)| item.kind == kind)
                .filter(|(_, item)| self.matches_selected_sticker_pack(item))
                .filter(|(_, item)| query.is_empty() || item.search_text.contains(&query))
                .map(|(index, _)| MediaItemSource::Library(index))
                .collect(),
            MediaView::Favorites => self
                .media_items
                .iter()
                .enumerate()
                .filter(|(_, item)| item.kind == kind)
                .filter(|(_, item)| self.is_media_favorite(item))
                .filter(|(_, item)| query.is_empty() || item.search_text.contains(&query))
                .map(|(index, _)| MediaItemSource::Library(index))
                .collect(),
            MediaView::RecentlyUsed => self
                .recent_media
                .iter()
                .enumerate()
                .filter(|(_, item)| item.kind == kind)
                .filter(|(_, item)| query.is_empty() || item.search_text.contains(&query))
                .map(|(index, _)| MediaItemSource::Recent(index))
                .collect(),
        }
    }

    pub(crate) fn media_item_from_source(&self, source: MediaItemSource) -> Option<MediaItem> {
        match source {
            MediaItemSource::Library(index) => self.media_items.get(index),
            MediaItemSource::Recent(index) => self.recent_media.get(index),
        }
        .cloned()
    }

    pub(crate) fn sticker_packs(&self) -> Vec<StickerPack> {
        let mut packs = BTreeMap::<String, StickerPack>::new();
        for item in &self.media_items {
            if item.kind != MediaKind::Sticker {
                continue;
            }
            let Some(id) = sticker_pack_id(item) else {
                continue;
            };
            let label = sticker_pack_label(&id);
            packs
                .entry(id.clone())
                .and_modify(|pack| pack.count += 1)
                .or_insert(StickerPack {
                    id,
                    label,
                    count: 1,
                });
        }

        packs.into_values().collect()
    }

    pub(crate) fn select_sticker_pack(&mut self, pack_id: Option<String>) {
        self.selected_sticker_pack_id = pack_id;
        self.media_view = MediaView::Library;
    }

    pub(crate) fn selected_sticker_pack_id(&self) -> Option<&str> {
        self.selected_sticker_pack_id.as_deref()
    }

    pub(crate) fn request_delete_sticker_pack(&mut self, pack: StickerPack) {
        self.pending_sticker_pack_delete = Some(pack);
    }

    pub(crate) fn cancel_delete_sticker_pack(&mut self) {
        self.pending_sticker_pack_delete = None;
    }

    fn matches_selected_sticker_pack(&self, item: &MediaItem) -> bool {
        if self.content_mode != ContentMode::Stickers || self.media_view != MediaView::Library {
            return true;
        }

        let Some(selected) = self.selected_sticker_pack_id() else {
            return true;
        };

        sticker_pack_id(item).as_deref() == Some(selected)
    }

    fn retain_existing_sticker_pack_selection(&mut self) {
        let Some(selected) = self.selected_sticker_pack_id.clone() else {
            return;
        };
        if !self.media_items.iter().any(|item| {
            item.kind == MediaKind::Sticker && sticker_pack_id(item) == Some(selected.clone())
        }) {
            self.selected_sticker_pack_id = None;
        }
    }

    fn ensure_content_mode_enabled(&mut self) {
        if !self.content_mode.enabled(&self.settings.features) {
            self.content_mode = ContentMode::first_enabled(&self.settings.features);
        }
    }

    fn retain_enabled_media_state(&mut self) {
        let features = self.settings.features.clone();
        self.media_items
            .retain(|item| media_item_enabled(item, &features));
        self.recent_media
            .retain(|item| item.path.exists() && media_item_enabled(item, &features));
        self.favorite_media_ids.retain(|id| {
            self.media_items
                .iter()
                .any(|item| item.id == *id && media_item_enabled(item, &features))
        });
        self.selected_media_ids.retain(|id| {
            self.media_items
                .iter()
                .any(|item| item.id == *id && media_item_enabled(item, &features))
        });
        self.retain_existing_sticker_pack_selection();
    }

    fn update_media_watcher(&mut self) {
        let request =
            if self.settings.features.media_watcher && self.settings.features.media_enabled() {
                MediaWatchRequest::Watch {
                    paths: media_scan_paths(&self.settings.gif_import_paths),
                }
            } else {
                MediaWatchRequest::Stop
            };

        if self.media_watch_tx.send(request).is_err() {
            self.log_dev("media watcher unavailable");
        }
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
        let file_was_missing = match fs::remove_file(&path) {
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
        if !self.save_media_metadata() {
            return;
        }

        self.content_mode = ContentMode::for_media_kind(item.kind);
        let status = if file_was_missing {
            format!("Removed missing media: {title}")
        } else {
            format!("Deleted media file: {title}")
        };
        self.queue_media_scan(format!("{status}; reindexing media library..."));
    }

    pub(crate) fn delete_sticker_pack(&mut self, pack_id: &str) {
        let pack_path = PathBuf::from(pack_id);
        let pack_label = sticker_pack_label(pack_id);
        let pack_items = self
            .media_items
            .iter()
            .filter(|item| {
                item.kind == MediaKind::Sticker && sticker_pack_id(item).as_deref() == Some(pack_id)
            })
            .cloned()
            .collect::<Vec<_>>();

        if pack_items.is_empty() {
            self.pending_sticker_pack_delete = None;
            self.status = Some(format!("Sticker pack not found: {pack_label}"));
            self.log_dev(format!(
                "delete sticker pack skipped; not found: {pack_label}"
            ));
            return;
        }

        let item_ids = pack_items
            .iter()
            .map(|item| item.id.clone())
            .collect::<HashSet<_>>();
        let deletion = MediaDeletionSummary::delete_files(&pack_items);

        self.favorite_media_ids.retain(|id| !item_ids.contains(id));
        self.recent_media
            .retain(|existing| !item_ids.contains(&existing.id));
        let settings_changed = self.settings.gif_import_paths.iter().any(|path| {
            path == &pack_path || path.parent().is_some_and(|parent| parent == pack_path)
        });
        self.settings.gif_import_paths.retain(|path| {
            !(path == &pack_path || path.parent().is_some_and(|parent| parent == pack_path))
        });

        let _ = std::fs::remove_dir(&pack_path);

        if settings_changed
            && let Err(err) = save_settings(self.settings_path.as_deref(), &self.settings)
        {
            self.status = Some(format!("Settings save error: {err}"));
            return;
        }
        if !self.save_media_metadata() {
            return;
        }

        if self.selected_sticker_pack_id() == Some(pack_id) {
            self.selected_sticker_pack_id = None;
        }
        self.pending_sticker_pack_delete = None;
        self.app_view = AppView::Main;
        self.content_mode = ContentMode::Stickers;
        self.media_view = MediaView::Library;

        let status = deletion.status(format!("Deleted sticker pack {pack_label}"));
        self.log_dev(format!("delete sticker pack: {status}"));
        self.queue_media_scan(format!("{status}; reindexing sticker library..."));
    }

    pub(crate) fn add_media_import_path(&mut self, path: PathBuf) {
        let input = path.to_string_lossy();
        if let Some(set_name) = sticker_set_name_from_input(&input) {
            self.import_telegram_sticker_set(set_name);
            return;
        }

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
        let mut imported_content_mode = None;

        for path in paths {
            if !is_supported_media_path(&path) {
                rejected += 1;
                continue;
            }

            imported_content_mode = Some(ContentMode::for_media_path(&path));

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
            self.update_media_watcher();
        }
        if accepted > 0 || queued > 0 {
            self.content_mode = imported_content_mode.unwrap_or(ContentMode::Gifs);
            self.media_view = MediaView::Library;
        }
        let noun = if mode == MediaImportMode::StoreFiles {
            "media item"
        } else {
            "media source"
        };
        let mut status = if accepted > 0 {
            format!(
                "Imported {accepted} {noun}{}; indexing media library...",
                plural_suffix(accepted)
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
        if accepted > 0 {
            self.queue_media_scan(status);
        } else {
            self.status = Some(status);
        }
    }

    fn import_telegram_sticker_set(&mut self, set_name: String) {
        let Some(token) = telegram_bot_token() else {
            self.status = Some(format!(
                "Telegram sticker import needs a bot token in Preferences or {TELEGRAM_BOT_TOKEN_ENV}"
            ));
            self.log_dev(format!(
                "telegram import blocked for {set_name}; missing bot token"
            ));
            return;
        };

        if self.queue_media_job(MediaJobRequest::ImportTelegramStickerSet {
            set_name: set_name.clone(),
            token,
        }) {
            self.app_view = AppView::Main;
            self.content_mode = ContentMode::Stickers;
            self.media_view = MediaView::Library;
            self.status = Some(format!("Importing Telegram sticker set: {set_name}"));
            self.log_dev(format!("telegram import started: {set_name}"));
        }
    }

    pub(crate) fn remove_media_import_path(&mut self, path: &std::path::Path) {
        self.settings
            .gif_import_paths
            .retain(|existing| existing != path);
        self.save_settings();
        self.update_media_watcher();
        self.queue_media_scan("Removed media source; indexing media library...");
    }

    pub(crate) fn reload_media_library(&mut self) {
        self.update_media_watcher();
        self.queue_media_scan("Indexing media library...");
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

    pub(crate) fn rebuild_global_hotkeys(&mut self) {
        self.global_hotkeys.rebuild(&self.settings.hotkeys);
    }

    pub(crate) fn global_hotkey_status(&self) -> &str {
        self.global_hotkeys.status()
    }

    pub(crate) fn toggle_command_label(&self) -> String {
        std::env::current_exe()
            .map(|path| format!("{} {}", shell_quote(&path), IpcCommand::Toggle.arg()))
            .unwrap_or_else(|_| format!("symbolis {}", IpcCommand::Toggle.arg()))
    }

    pub(crate) fn copy_toggle_command(&mut self) {
        let command = self.toggle_command_label();
        match self.clipboard.copy_text(command) {
            Ok(()) => {
                self.status = Some("Copied toggle command".to_owned());
            }
            Err(err) => {
                self.status = Some(format!("Clipboard error: {err}"));
            }
        }
    }

    pub(crate) fn install_toggle_desktop_launcher(&mut self) {
        match self.write_toggle_desktop_launcher() {
            Ok(desktop_path) => {
                self.status = Some(format!(
                    "Installed launcher {}; bind it in your desktop shortcuts",
                    desktop_path.display()
                ));
            }
            Err(err) => {
                self.status = Some(format!("Desktop launcher install error: {err}"));
            }
        }
    }

    pub(crate) fn apply_kde_toggle_shortcut(&mut self) {
        let Some(binding) = self.settings.hotkeys.binding(HotkeyAction::Main) else {
            self.status = Some("Set global hotkey first".to_owned());
            return;
        };
        let Some(sequence) = kde_key_sequence(binding) else {
            self.status = Some("Set global hotkey key first".to_owned());
            return;
        };
        let Some(kwriteconfig) = command_exists("kwriteconfig6")
            .then_some("kwriteconfig6")
            .or_else(|| command_exists("kwriteconfig5").then_some("kwriteconfig5"))
        else {
            self.status =
                Some("KDE shortcut install needs kwriteconfig6 or kwriteconfig5".to_owned());
            return;
        };

        if let Err(err) = self.write_toggle_desktop_launcher() {
            self.status = Some(format!("KDE shortcut launcher error: {err}"));
            return;
        }

        let launch_value = format!("{sequence},none,Symbolis Toggle");
        let commands = [
            Command::new(kwriteconfig)
                .arg("--file")
                .arg("kglobalshortcutsrc")
                .arg("--group")
                .arg("symbolis-toggle.desktop")
                .arg("--key")
                .arg("_k_friendly_name")
                .arg("Symbolis Toggle")
                .output(),
            Command::new(kwriteconfig)
                .arg("--file")
                .arg("kglobalshortcutsrc")
                .arg("--group")
                .arg("symbolis-toggle.desktop")
                .arg("--key")
                .arg("_launch")
                .arg(&launch_value)
                .output(),
        ];

        for result in commands {
            match result {
                Ok(output) if output.status.success() => {}
                Ok(output) => {
                    self.status = Some(format!(
                        "KDE shortcut install error: {}",
                        String::from_utf8_lossy(&output.stderr).trim()
                    ));
                    return;
                }
                Err(err) => {
                    self.status = Some(format!("KDE shortcut install error: {err}"));
                    return;
                }
            }
        }

        restart_kde_global_accel();
        self.status = Some(format!("Applied KDE shortcut: {sequence}"));
    }

    fn write_toggle_desktop_launcher(&self) -> Result<PathBuf, String> {
        let Some(data_dir) = dirs::data_dir() else {
            return Err("data dir unavailable".to_owned());
        };
        let desktop_dir = data_dir.join("applications");
        let desktop_path = desktop_dir.join("symbolis-toggle.desktop");
        let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("symbolis"));
        let content = format!(
            "[Desktop Entry]\nType=Application\nName=Symbolis Toggle\nComment=Toggle Symbolis picker window\nExec={} {}\nTerminal=false\nNoDisplay=true\nStartupNotify=false\nCategories=Utility;\nX-KDE-GlobalAccel-CommandShortcut=true\n",
            desktop_exec_quote(&exe),
            IpcCommand::Toggle.arg()
        );

        fs::create_dir_all(&desktop_dir)
            .and_then(|_| fs::write(&desktop_path, content))
            .map_err(|err| err.to_string())?;
        Ok(desktop_path)
    }

    pub(crate) fn apply_feature_settings(&mut self, ctx: &Context) {
        self.settings.features.ensure_any_content_enabled();
        configure_fonts(ctx, &self.settings);
        let (entries, data_source) = load_entries(self.settings.features.symbols);
        self.entries = entries;
        self.data_source = data_source;
        if !self.settings.features.symbols {
            self.recent.clear();
            self.selected_tab = Tab::Category(crate::data::Category::Emoji);
        }

        self.ensure_content_mode_enabled();
        self.retain_enabled_media_state();
        self.update_media_watcher();
        self.reload_media_library();
    }

    pub(crate) fn content_mode_enabled(&self, mode: ContentMode) -> bool {
        mode.enabled(&self.settings.features)
    }

    pub(crate) fn selected_media_count(&self) -> usize {
        self.selected_media_ids.len()
    }

    pub(crate) fn is_media_selected(&self, item: &MediaItem) -> bool {
        self.selected_media_ids.contains(&item.id)
    }

    pub(crate) fn toggle_media_selected(&mut self, item: &MediaItem) {
        if !self.selected_media_ids.insert(item.id.clone()) {
            self.selected_media_ids.remove(&item.id);
        }
    }

    pub(crate) fn clear_media_selection(&mut self) {
        self.selected_media_ids.clear();
        self.status = None;
    }

    pub(crate) fn add_selected_media_to_favorites(&mut self) {
        let selected_ids = self.selected_media_ids.clone();
        let mut added = 0;
        for item in self
            .media_items
            .iter()
            .filter(|item| selected_ids.contains(&item.id))
        {
            if !self.favorite_media_ids.contains(&item.id) {
                self.favorite_media_ids.insert(0, item.id.clone());
                added += 1;
            }
        }
        self.favorite_media_ids.truncate(512);
        if let Err(err) = self.save_favorite_media_ids() {
            self.status = Some(format!("Favorites save error: {err}"));
            return;
        }
        self.status = Some(format!(
            "Added {added} selected media item{} to favorites",
            plural_suffix(added)
        ));
    }

    pub(crate) fn remove_selected_media_from_favorites(&mut self) {
        let selected_ids = self.selected_media_ids.clone();
        let before = self.favorite_media_ids.len();
        self.favorite_media_ids
            .retain(|id| !selected_ids.contains(id));
        let removed = before.saturating_sub(self.favorite_media_ids.len());
        if let Err(err) = self.save_favorite_media_ids() {
            self.status = Some(format!("Favorites save error: {err}"));
            return;
        }
        self.status = Some(format!(
            "Removed {removed} selected favorite{}",
            plural_suffix(removed)
        ));
    }

    pub(crate) fn delete_selected_media_files(&mut self) {
        let selected_ids = self.selected_media_ids.clone();
        let items = self
            .media_items
            .iter()
            .filter(|item| selected_ids.contains(&item.id))
            .cloned()
            .collect::<Vec<_>>();
        if items.is_empty() {
            self.selected_media_ids.clear();
            self.status = Some("No selected library media to delete".to_owned());
            return;
        }

        let deletion = MediaDeletionSummary::delete_files(&items);

        self.selected_media_ids.clear();
        self.favorite_media_ids
            .retain(|id| !selected_ids.contains(id));
        self.recent_media
            .retain(|item| !selected_ids.contains(&item.id));
        if !self.save_media_metadata() {
            return;
        }

        let status = deletion.status("Deleted selected media");
        self.queue_media_scan(format!("{status}; reindexing media library..."));
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

    pub(crate) fn telegram_bot_token_status(&self) -> String {
        let env_configured = telegram_bot_token_from_env().is_some();
        match (env_configured, self.telegram_bot_token_saved) {
            (true, true) => format!(
                "Telegram token: env override active; local token saved at {}",
                telegram_secret_path()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "config dir".to_owned())
            ),
            (true, false) => {
                format!("Telegram token: configured via {TELEGRAM_BOT_TOKEN_ENV}")
            }
            (false, true) => format!(
                "Telegram token: saved locally at {}",
                telegram_secret_path()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "config dir".to_owned())
            ),
            (false, false) => "Telegram token: not configured".to_owned(),
        }
    }

    pub(crate) fn save_telegram_bot_token_setting(&mut self) {
        match save_telegram_bot_token(&self.telegram_bot_token_input) {
            Ok(()) => {
                self.telegram_bot_token_input = self.telegram_bot_token_input.trim().to_owned();
                self.telegram_bot_token_saved = true;
                self.status = Some("Saved Telegram bot token".to_owned());
            }
            Err(err) => {
                self.status = Some(format!("Telegram token save error: {err}"));
            }
        }
    }

    pub(crate) fn clear_telegram_bot_token_setting(&mut self) {
        match clear_saved_telegram_bot_token() {
            Ok(()) => {
                self.telegram_bot_token_input.clear();
                self.telegram_bot_token_saved = false;
                self.status = Some("Cleared local Telegram bot token".to_owned());
            }
            Err(err) => {
                self.status = Some(format!("Telegram token clear error: {err}"));
            }
        }
    }

    pub(crate) fn toggle_telegram_bot_token_guide(&mut self) {
        self.telegram_bot_token_guide_visible = !self.telegram_bot_token_guide_visible;
    }

    pub(crate) fn active_media_job_count(&self) -> usize {
        self.active_media_jobs
    }

    pub(crate) fn active_media_scan_count(&self) -> usize {
        self.active_media_scans
    }

    pub(crate) fn dev_log_entries(&self) -> std::collections::vec_deque::Iter<'_, DevLogEntry> {
        self.dev_log.iter()
    }

    pub(crate) fn clear_everything(&mut self) {
        let mut errors = Vec::new();

        remove_symbolis_tree(symbolis_data_root(), "data", &mut errors);
        remove_symbolis_tree(symbolis_config_root(), "config", &mut errors);
        remove_symbolis_tree(symbolis_cache_root(), "cache", &mut errors);
        if let Some(root) = media_root() {
            remove_path_if_exists(&root, "media", &mut errors);
        }

        self.recent.clear();
        self.media_items.clear();
        self.recent_media.clear();
        self.favorite_media_ids.clear();
        self.settings = UiSettings::default();
        self.update_media_watcher();
        self.app_view = AppView::Main;
        self.content_mode = ContentMode::Symbols;
        self.selected_tab = Tab::Category(crate::data::Category::Emoji);
        self.media_view = MediaView::Library;
        self.selected_sticker_pack_id = None;
        self.query.clear();
        self.gif_query.clear();
        self.gif_import_path_input.clear();
        self.telegram_bot_token_input.clear();
        self.telegram_bot_token_saved = false;
        self.telegram_bot_token_guide_visible = false;
        self.clear_everything_confirm = false;
        self.capture_hotkey_action = None;
        self.pending_sticker_pack_delete = None;
        self.dev_log.clear();
        self.emoji_cache = EmojiCache::new(self.emoji_cache.color_renderer_available());
        self.media_preview_cache = MediaPreviewCache::new();
        self.media_scan_generation = self.media_scan_generation.wrapping_add(1);
        self.media_scan_completion_status = None;

        self.status = if errors.is_empty() {
            Some("Cleared Symbolis data, config, cache, and local media".to_owned())
        } else {
            Some(format!(
                "Clear finished with {} error{}: {}",
                errors.len(),
                plural_suffix(errors.len()),
                errors.join("; ")
            ))
        };
        self.log_dev("cleared Symbolis data/config/cache/local media");
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

    fn save_media_metadata(&mut self) -> bool {
        if let Err(err) = self.save_favorite_media_ids() {
            self.status = Some(format!("Favorites save error: {err}"));
            return false;
        }
        if let Err(err) = self.save_recent_media() {
            self.status = Some(format!("Recent media save error: {err}"));
            return false;
        }

        true
    }

    fn log_dev(&mut self, message: impl Into<String>) {
        if self.dev_log.len() >= DEV_LOG_LIMIT {
            self.dev_log.pop_front();
        }
        self.dev_log.push_back(DevLogEntry {
            elapsed_ms: self.app_started_at.elapsed().as_millis(),
            message: message.into(),
        });
    }

    fn queue_media_job(&mut self, job: MediaJobRequest) -> bool {
        let label = media_job_request_label(&job);
        self.log_dev(format!("queue media job: {label}"));
        self.active_media_jobs += 1;
        if self.media_job_tx.send(job).is_ok() {
            return true;
        }

        self.active_media_jobs = self.active_media_jobs.saturating_sub(1);
        self.status = Some("Media worker is unavailable".to_owned());
        self.log_dev("media worker unavailable; job was not queued");
        false
    }

    fn queue_media_scan(&mut self, status: impl Into<String>) {
        self.queue_media_scan_with_completion(status, None);
    }

    fn queue_media_scan_with_completion(
        &mut self,
        status: impl Into<String>,
        completion_status: Option<String>,
    ) {
        self.media_scan_generation = self.media_scan_generation.wrapping_add(1);
        let generation = self.media_scan_generation;
        let request = MediaScanRequest::Scan {
            generation,
            paths: media_scan_paths(&self.settings.gif_import_paths),
            index_path: self.media_index_path.clone(),
            options: MediaScanOptions::from_features(&self.settings.features),
        };
        self.active_media_scans += 1;
        if self.media_scan_tx.send(request).is_ok() {
            self.media_scan_completion_status =
                completion_status.map(|status| (generation, status));
            self.status = Some(status.into());
            self.log_dev(format!("queue media scan #{generation}"));
            return;
        }

        self.active_media_scans = self.active_media_scans.saturating_sub(1);
        self.media_scan_completion_status = None;
        self.status = Some("Media scan worker is unavailable".to_owned());
        self.log_dev(format!("media scan worker unavailable for #{generation}"));
    }

    fn poll_media_jobs(&mut self) {
        let mut completed = 0;

        while let Ok(result) = self.media_job_rx.try_recv() {
            if media_job_result_is_terminal(&result) {
                completed += 1;
            }
            self.handle_media_job_result(result);
        }

        if completed > 0 {
            self.active_media_jobs = self.active_media_jobs.saturating_sub(completed);
        }
    }

    fn poll_media_scans(&mut self) {
        let mut completed = 0;

        while let Ok(result) = self.media_scan_rx.try_recv() {
            completed += 1;
            self.handle_media_scan_result(result);
        }

        if completed > 0 {
            self.active_media_scans = self.active_media_scans.saturating_sub(completed);
        }
    }

    fn poll_media_watcher(&mut self) {
        let mut changed = false;
        while self.media_watch_rx.try_recv().is_ok() {
            changed = true;
        }

        if changed {
            self.queue_media_scan("Media folder changed; indexing media library...");
        }
    }

    fn poll_global_hotkeys(&mut self, ctx: &Context) {
        for action in self.global_hotkeys.poll() {
            self.activate_global_hotkey(action, ctx);
        }
    }

    fn poll_ipc_commands(&mut self, ctx: &Context) {
        let mut commands = Vec::new();
        if let Some(server) = &self.ipc_server {
            while let Some(command) = server.try_recv() {
                commands.push(command);
            }
        }

        for command in commands {
            self.apply_ipc_command(command, ctx);
        }
    }

    fn apply_ipc_command(&mut self, command: IpcCommand, ctx: &Context) {
        match command {
            IpcCommand::Toggle => self.toggle_window(ctx),
            IpcCommand::ShowMain => {
                self.show_window(ctx);
                self.app_view = AppView::Main;
                self.ensure_content_mode_enabled();
            }
            IpcCommand::ShowSymbols => {
                self.activate_global_hotkey(HotkeyAction::Symbols, ctx);
            }
            IpcCommand::ShowStickers => {
                self.activate_global_hotkey(HotkeyAction::Stickers, ctx);
            }
            IpcCommand::ShowGifs => {
                self.activate_global_hotkey(HotkeyAction::Gifs, ctx);
            }
            IpcCommand::Quit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
        }
    }

    fn toggle_window(&mut self, ctx: &Context) {
        let minimized = ctx.input(|input| input.viewport().minimized.unwrap_or(false));
        let focused = ctx.input(|input| input.viewport().focused.unwrap_or(false));
        if self.hidden_to_background || minimized || !focused {
            self.show_window(ctx);
        } else {
            self.hide_window(ctx);
        }
    }

    fn show_window(&mut self, ctx: &Context) {
        self.hidden_to_background = false;
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
        ctx.request_repaint();
    }

    pub(crate) fn hide_window(&mut self, ctx: &Context) {
        self.hidden_to_background = true;
        ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        ctx.request_repaint();
    }

    pub(crate) fn quit_app(&mut self, ctx: &Context) {
        self.allow_close = true;
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }

    fn activate_global_hotkey(&mut self, action: HotkeyAction, ctx: &Context) {
        self.show_window(ctx);

        self.app_view = AppView::Main;
        match action {
            HotkeyAction::Main => {
                self.ensure_content_mode_enabled();
            }
            HotkeyAction::Symbols => {
                self.activate_content_mode(ContentMode::Symbols);
            }
            HotkeyAction::Stickers => {
                self.activate_content_mode(ContentMode::Stickers);
            }
            HotkeyAction::Gifs => {
                self.activate_content_mode(ContentMode::Gifs);
            }
        }

        self.status = Some(format!("Opened via {}", action.label()));
    }

    fn activate_content_mode(&mut self, mode: ContentMode) {
        self.content_mode = if mode.enabled(&self.settings.features) {
            mode
        } else {
            ContentMode::first_enabled(&self.settings.features)
        };

        match self.content_mode {
            ContentMode::Symbols => {
                self.selected_tab = Tab::Category(crate::data::Category::Emoji);
            }
            ContentMode::Stickers | ContentMode::Gifs => {
                self.media_view = MediaView::Library;
            }
        }
    }

    fn handle_close_request(&mut self, ctx: &Context) {
        if self.allow_close || !ctx.input(|input| input.viewport().close_requested()) {
            return;
        }

        ctx.send_viewport_cmd(ViewportCommand::CancelClose);
        self.hide_window(ctx);
    }

    fn handle_media_scan_result(&mut self, result: MediaScanResult) {
        let MediaScanResult::Complete {
            generation,
            items,
            index_save_error,
        } = result;

        if generation != self.media_scan_generation {
            self.log_dev(format!(
                "ignore stale media scan #{generation}; current is #{}",
                self.media_scan_generation
            ));
            return;
        }

        let item_count = items.len();
        self.media_items = items;
        self.retain_enabled_media_state();
        let completion_status = self
            .media_scan_completion_status
            .take()
            .filter(|(status_generation, _)| *status_generation == generation)
            .map(|(_, status)| status);
        self.recent_media.retain(|item| item.path.exists());
        if let Err(err) = self.save_recent_media() {
            self.status = Some(format!("Recent media save error: {err}"));
            return;
        }

        self.status = Some(if let Some(err) = index_save_error {
            self.log_dev(format!("media scan #{generation} index save error: {err}"));
            format!("Media index save error: {err}")
        } else if let Some(status) = completion_status {
            self.log_dev(format!(
                "media scan #{generation} complete; indexed {item_count} media files"
            ));
            format!("{status}; indexed {} media files", self.media_items.len())
        } else {
            self.log_dev(format!(
                "media scan #{generation} complete; indexed {item_count} media files"
            ));
            format!("Indexed {} media files", self.media_items.len())
        });
    }

    fn handle_media_job_result(&mut self, result: MediaJobResult) {
        self.log_dev(media_job_result_label(&result));
        match result {
            MediaJobResult::StoredImport { original, result } => match result {
                Ok(path) => {
                    self.content_mode = ContentMode::for_media_path(&path);
                    self.media_view = MediaView::Library;
                    self.queue_media_scan(format!(
                        "Stored optimized media: {}; indexing media library...",
                        media_path_label(&path)
                    ));
                }
                Err(err) => {
                    self.import_original_after_storage_error(&original, &err);
                }
            },
            MediaJobResult::OptimizedCopy { title, result } => match result {
                Ok(path) => {
                    self.content_mode = ContentMode::Gifs;
                    self.media_view = MediaView::Library;
                    self.queue_media_scan(format!(
                        "Saved WebM copy: {}; indexing media library...",
                        path.display()
                    ));
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
            MediaJobResult::TelegramStickerImport { set_name, result } => match result {
                Ok(summary) => {
                    self.app_view = AppView::Main;
                    self.content_mode = ContentMode::Stickers;
                    self.media_view = MediaView::Library;
                    let label = summary.status_label();
                    self.queue_media_scan_with_completion(
                        format!("{label}; indexing media library..."),
                        Some(label),
                    );
                }
                Err(err) => {
                    self.status = Some(format!(
                        "Telegram sticker import error for {set_name}: {err}"
                    ));
                }
            },
            MediaJobResult::TelegramStickerImportProgress { set_name, message } => {
                self.app_view = AppView::Main;
                self.content_mode = ContentMode::Stickers;
                self.media_view = MediaView::Library;
                self.status = Some(format!("Importing Telegram {set_name}: {message}"));
            }
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
        self.content_mode = ContentMode::for_media_path(&path);
        self.media_view = MediaView::Library;
        self.queue_media_scan(format!(
            "Imported original: {}; storage warning: {err}; indexing media library...",
            media_path_label(&path)
        ));
    }

    fn media_jobs_active(&self) -> bool {
        self.active_media_jobs > 0 || self.active_media_scans > 0
    }

    fn request_media_job_repaint(&self, ctx: &Context) {
        if self.media_jobs_active() {
            ctx.request_repaint_after(Duration::from_millis(120));
        }
    }

    pub(crate) fn media_scan_in_progress(&self) -> bool {
        self.active_media_scans > 0
    }

    fn handle_keyboard(&mut self, ctx: &Context) {
        if self.capture_hotkey_action.is_some() {
            if ctx.input(|input| input.key_pressed(Key::Escape)) {
                self.capture_hotkey_action = None;
                self.status = Some("Global hotkey capture cancelled".to_owned());
            }
            return;
        }

        if ctx.input(|input| input.key_pressed(Key::F7)) {
            self.dev_panel_open = !self.dev_panel_open;
            self.clear_everything_confirm = false;
        }

        if ctx.input(|input| input.key_pressed(Key::Escape)) {
            if self.app_view == AppView::Settings {
                self.app_view = AppView::Main;
            } else {
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
        }

        if self.app_view != AppView::Main {
            return;
        }

        if ctx.input(|input| input.key_pressed(Key::Enter)) {
            match self.content_mode {
                ContentMode::Symbols => {
                    if let Some(entry) = self
                        .filtered_entry_indices()
                        .first()
                        .and_then(|index| self.entry_at_active_index(*index))
                    {
                        self.copy_entry(&entry);
                    }
                }
                ContentMode::Stickers | ContentMode::Gifs => {
                    if let Some(item) = self
                        .filtered_media_sources()
                        .first()
                        .and_then(|source| self.media_item_from_source(*source))
                    {
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

fn plural_suffix(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

fn media_transfer_requires_export(item: &MediaItem) -> bool {
    matches!(item.format, MediaFormat::Mp4 | MediaFormat::Webm)
}

fn sticker_pack_id(item: &MediaItem) -> Option<String> {
    item.path
        .parent()
        .map(|path| path.to_string_lossy().into_owned())
}

fn sticker_pack_label(id: &str) -> String {
    Path::new(id)
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.replace(['_', '-'], " "))
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "Loose stickers".to_owned())
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
        self.handle_close_request(ctx);
        self.poll_media_jobs();
        self.poll_media_scans();
        self.poll_media_watcher();
        self.poll_ipc_commands(ctx);
        self.poll_global_hotkeys(ctx);
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

fn media_item_enabled(item: &MediaItem, features: &FeatureSettings) -> bool {
    ContentMode::for_media_kind(item.kind).enabled(features)
}

fn shell_quote(path: &Path) -> String {
    let value = path.to_string_lossy();
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '_' | '-' | ':'))
    {
        value.into_owned()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

fn desktop_exec_quote(path: &Path) -> String {
    let value = path.to_string_lossy();
    if value.contains(' ') || value.contains('"') || value.contains('\\') {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        value.into_owned()
    }
}

fn kde_key_sequence(binding: &crate::settings::HotkeyBinding) -> Option<String> {
    if binding.key.trim().is_empty() {
        return None;
    }

    let mut parts = Vec::new();
    if binding.control {
        parts.push("Ctrl".to_owned());
    }
    if binding.alt {
        parts.push("Alt".to_owned());
    }
    if binding.shift {
        parts.push("Shift".to_owned());
    }
    if binding.super_key {
        parts.push("Meta".to_owned());
    }
    parts.push(crate::settings::hotkey_key_label(&binding.key).to_owned());
    Some(parts.join("+"))
}

fn command_exists(command: &str) -> bool {
    Command::new(command).arg("--version").output().is_ok()
}

fn restart_kde_global_accel() {
    if command_exists("systemctl") {
        let _ = Command::new("systemctl")
            .arg("--user")
            .arg("restart")
            .arg("plasma-kglobalaccel.service")
            .output();
    } else if command_exists("kquitapp6") {
        let _ = Command::new("kquitapp6").arg("kglobalaccel").output();
    } else if command_exists("kquitapp5") {
        let _ = Command::new("kquitapp5").arg("kglobalaccel").output();
    }
}

fn symbolis_data_root() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join("symbolis"))
}

fn symbolis_config_root() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("symbolis"))
}

fn symbolis_cache_root() -> Option<PathBuf> {
    dirs::cache_dir().map(|dir| dir.join("symbolis"))
}

fn remove_symbolis_tree(path: Option<PathBuf>, label: &str, errors: &mut Vec<String>) {
    let Some(path) = path else {
        return;
    };
    if path.file_name().and_then(|name| name.to_str()) != Some("symbolis") {
        errors.push(format!("refused to remove unexpected {label} path"));
        return;
    }
    remove_path_if_exists(&path, label, errors);
}

fn remove_path_if_exists(path: &Path, label: &str, errors: &mut Vec<String>) {
    if !path.exists() {
        return;
    }

    let result = if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    };
    if let Err(err) = result {
        errors.push(format!("{label}: {err}"));
    }
}

fn is_default_media_scan_path(path: &Path) -> bool {
    default_media_paths()
        .iter()
        .any(|default_path| path == default_path || path.starts_with(default_path))
}
