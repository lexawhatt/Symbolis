use std::{io, path::PathBuf};

use eframe::egui::{Context, Key, ViewportCommand};

use crate::{
    data::{DataSource, EmojiGroup, Entry, StoredEntry, load_entries, load_recent, recent_path},
    emoji_cache::EmojiCache,
    media_clipboard::MediaClipboard,
    media_drag::{DragOutBackend, LinuxDragOutBackend},
    preflight::PreflightReport,
    settings::{
        UiSettings, configure_fonts, configure_style, load_settings, save_settings, settings_path,
    },
};

const RECENT_LIMIT: usize = 48;

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
    pub(crate) selected_tab: Tab,
    pub(crate) query: String,
    pub(crate) recent_path: Option<PathBuf>,
    pub(crate) settings_path: Option<PathBuf>,
    pub(crate) clipboard: MediaClipboard,
    pub(crate) drag_out: LinuxDragOutBackend,
    pub(crate) status: Option<String>,
    pub(crate) data_source: DataSource,
    pub(crate) settings: UiSettings,
    pub(crate) emoji_cache: EmojiCache,
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

        Self {
            entries,
            recent,
            selected_tab: Tab::Category(crate::data::Category::Emoji),
            query: String::new(),
            recent_path,
            settings_path,
            clipboard: MediaClipboard::new().expect("clipboard was verified by startup preflight"),
            drag_out: LinuxDragOutBackend::new(preflight.linux_session, preflight.drag_helper),
            status: None,
            data_source,
            settings,
            emoji_cache: EmojiCache::new(),
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

    pub(crate) fn clear_recent(&mut self) {
        self.recent.clear();
        if let Err(err) = self.save_recent() {
            self.status = Some(format!("Recent save error: {err}"));
        } else {
            self.status = None;
            self.selected_tab = Tab::Category(crate::data::Category::Emoji);
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
            "file clipboard ready".to_owned()
        };
        format!("{} delivery: {drag}", self.drag_out.session_label())
    }

    pub(crate) fn gif_provider_status(&self) -> String {
        format!(
            "{} GIFs: {}",
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

    fn handle_keyboard(&mut self, ctx: &Context) {
        if ctx.input(|input| input.key_pressed(Key::Escape)) {
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }

        if ctx.input(|input| input.key_pressed(Key::Enter)) {
            if let Some(entry) = self.filtered_entries().first().cloned() {
                self.copy_entry(&entry);
            }
        }
    }
}

impl eframe::App for SymbolisApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        configure_style(ctx, &self.settings);
        self.handle_keyboard(ctx);
        self.draw(ctx);
    }
}
