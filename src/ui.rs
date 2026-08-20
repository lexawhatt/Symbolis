use std::time::Duration;

use eframe::egui::{
    self, Align, Align2, Button, Color32, Context, FontId, Frame, Key, Layout, Rect, RichText,
    Rounding, ScrollArea, Sense, Stroke, TextEdit, TopBottomPanel, color_picker::Alpha,
    containers::scroll_area::ScrollBarVisibility,
};

use crate::{
    app::{
        AppView, ContentMode, MediaItemSource, MediaView, SymbolisApp, Tab, has_hovered_files,
        hovered_media_drop_count,
    },
    data::{Category, DataSource, EmojiGroup, Entry},
    dev_metrics::{DevMetricsSnapshot, GpuMetric},
    gif_provider::{GifProvider, ProviderStatus},
    media_drag::DragOutBackend,
    media_library::{MediaFormat, MediaItem, MediaKind},
    settings::{
        HotkeyAction, HotkeyBinding, InterfaceMode, Palette, Preset, Rgb, ThemeSelection,
        hotkey_key_label,
    },
};

const SIDEBAR_WIDTH: f32 = 56.0;
const TILE_GAP: f32 = 10.0;
const EMOJI_TILE_WIDTH: f32 = 82.0;
const SYMBOL_TILE_WIDTH: f32 = 86.0;
const KAOMOJI_TILE_WIDTH: f32 = 142.0;
const SIDEBAR_BUTTON_SIZE: f32 = 40.0;
const SIDEBAR_GROUP_SIZE: f32 = 32.0;
const SIDEBAR_SETTINGS_HEIGHT: f32 = 58.0;
const SIDEBAR_STACK_GAP: f32 = 5.0;
const SIDEBAR_STACK_MAX_VISIBLE_ITEMS: f32 = 2.35;
const STICKER_PACK_SIDEBAR_MIN_WIDTH: f32 = 620.0;
const COMPACT_TOPBAR_MAX_WIDTH: f32 = 560.0;

#[derive(Clone, Copy)]
struct Chrome {
    sidebar_width: f32,
    sidebar_button_size: f32,
    sidebar_group_size: f32,
    sidebar_settings_height: f32,
    topbar_height: f32,
    footer_height: f32,
    tile_gap: f32,
    tile_rounding: f32,
    sidebar_rounding: f32,
    content_top_space: f32,
    grid_side_padding: f32,
}

fn chrome(mode: InterfaceMode) -> Chrome {
    if mode.is_modern() {
        Chrome {
            sidebar_width: 64.0,
            sidebar_button_size: 42.0,
            sidebar_group_size: 34.0,
            sidebar_settings_height: 62.0,
            topbar_height: 86.0,
            footer_height: 30.0,
            tile_gap: 12.0,
            tile_rounding: 8.0,
            sidebar_rounding: 8.0,
            content_top_space: 18.0,
            grid_side_padding: 18.0,
        }
    } else {
        Chrome {
            sidebar_width: SIDEBAR_WIDTH,
            sidebar_button_size: SIDEBAR_BUTTON_SIZE,
            sidebar_group_size: SIDEBAR_GROUP_SIZE,
            sidebar_settings_height: SIDEBAR_SETTINGS_HEIGHT,
            topbar_height: 78.0,
            footer_height: 27.0,
            tile_gap: TILE_GAP,
            tile_rounding: 7.0,
            sidebar_rounding: 7.0,
            content_top_space: 16.0,
            grid_side_padding: 14.0,
        }
    }
}

const TEXT_CATEGORIES: &[Category] =
    &[Category::Kaomoji, Category::Punctuation, Category::Keyboard];
const LANGUAGE_CATEGORIES: &[Category] = &[
    Category::Greek,
    Category::Cyrillic,
    Category::Latin,
    Category::Ipa,
    Category::Hebrew,
    Category::Arabic,
    Category::Kana,
];
const MATH_CATEGORIES: &[Category] = &[
    Category::Math,
    Category::Currency,
    Category::Units,
    Category::Fractions,
    Category::SuperscriptsSubscripts,
];
const DRAWING_CATEGORIES: &[Category] = &[
    Category::Arrows,
    Category::BoxDrawing,
    Category::Blocks,
    Category::Shapes,
    Category::Enclosed,
];
const FUN_CATEGORIES: &[Category] = &[Category::Braille, Category::Games, Category::Music];

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum SidebarGroup {
    Emoji,
    Text,
    Languages,
    Math,
    Drawing,
    Fun,
}

impl SidebarGroup {
    const ALL: [SidebarGroup; 6] = [
        SidebarGroup::Emoji,
        SidebarGroup::Text,
        SidebarGroup::Languages,
        SidebarGroup::Math,
        SidebarGroup::Drawing,
        SidebarGroup::Fun,
    ];

    fn label(self) -> &'static str {
        match self {
            SidebarGroup::Emoji => "Emoji",
            SidebarGroup::Text => "Text",
            SidebarGroup::Languages => "Languages",
            SidebarGroup::Math => "Math",
            SidebarGroup::Drawing => "Drawing",
            SidebarGroup::Fun => "Fun",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            SidebarGroup::Emoji => "🙂",
            SidebarGroup::Text => ":-)",
            SidebarGroup::Languages => "あ",
            SidebarGroup::Math => "∑",
            SidebarGroup::Drawing => "◆",
            SidebarGroup::Fun => "♫",
        }
    }

    fn categories(self) -> &'static [Category] {
        match self {
            SidebarGroup::Emoji => &[],
            SidebarGroup::Text => TEXT_CATEGORIES,
            SidebarGroup::Languages => LANGUAGE_CATEGORIES,
            SidebarGroup::Math => MATH_CATEGORIES,
            SidebarGroup::Drawing => DRAWING_CATEGORIES,
            SidebarGroup::Fun => FUN_CATEGORIES,
        }
    }

    fn default_tab(self) -> Tab {
        match self {
            SidebarGroup::Emoji => Tab::Category(Category::Emoji),
            SidebarGroup::Text => Tab::Category(Category::Kaomoji),
            SidebarGroup::Languages => Tab::Category(Category::Greek),
            SidebarGroup::Math => Tab::Category(Category::Math),
            SidebarGroup::Drawing => Tab::Category(Category::Arrows),
            SidebarGroup::Fun => Tab::Category(Category::Braille),
        }
    }

    fn for_category(category: Category) -> Option<Self> {
        if TEXT_CATEGORIES.contains(&category) {
            Some(SidebarGroup::Text)
        } else if LANGUAGE_CATEGORIES.contains(&category) {
            Some(SidebarGroup::Languages)
        } else if MATH_CATEGORIES.contains(&category) {
            Some(SidebarGroup::Math)
        } else if DRAWING_CATEGORIES.contains(&category) {
            Some(SidebarGroup::Drawing)
        } else if FUN_CATEGORIES.contains(&category) {
            Some(SidebarGroup::Fun)
        } else {
            None
        }
    }
}

impl SymbolisApp {
    pub(crate) fn draw(&mut self, ctx: &Context) {
        let chrome = chrome(self.settings.interface_mode);
        let filtered_entries =
            if self.app_view == AppView::Main && self.content_mode == ContentMode::Symbols {
                Some(self.filtered_entry_indices())
            } else {
                None
            };
        let filtered_media =
            if self.app_view == AppView::Main && self.content_mode.media_kind().is_some() {
                Some(self.filtered_media_sources())
            } else {
                None
            };
        let count = if let Some(filtered) = &filtered_entries {
            filtered.len()
        } else if let Some(filtered) = &filtered_media {
            filtered.len()
        } else {
            0
        };

        self.draw_sidebar(ctx);
        if self.should_draw_sticker_pack_sidebar(ctx) {
            self.draw_sticker_pack_sidebar(ctx);
        }
        self.draw_topbar(ctx);
        self.draw_footer(ctx, count);

        egui::CentralPanel::default()
            .frame(Frame::none().fill(self.settings.palette.bg.color()))
            .show(ctx, |ui| {
                if self.app_view == AppView::Settings {
                    self.draw_settings(ui, ctx);
                    return;
                }

                match self.content_mode {
                    ContentMode::Symbols => {
                        let filtered = filtered_entries.as_deref().unwrap_or(&[]);
                        if filtered.is_empty() {
                            draw_empty_state(ui, self, "No matches");
                            return;
                        }

                        ui.add_space(chrome.content_top_space);
                        draw_symbol_grid(ui, self, filtered);
                    }
                    ContentMode::Stickers => {
                        let filtered = filtered_media.as_deref().unwrap_or(&[]);
                        if filtered.is_empty() {
                            let message = match self.media_view {
                                MediaView::Library
                                    if self.media_items.is_empty()
                                        && self.media_scan_in_progress() =>
                                {
                                    "Indexing sticker library..."
                                }
                                MediaView::Library if self.media_items.is_empty() => {
                                    "Import Telegram stickers or drop WebP/PNG/WebM here"
                                }
                                MediaView::Library => "No stickers match",
                                MediaView::Favorites => "No favorite stickers yet",
                                MediaView::RecentlyUsed => "No recently used stickers yet",
                            };
                            draw_empty_state(ui, self, message);
                            return;
                        }

                        ui.add_space(chrome.content_top_space);
                        draw_media_grid(ui, self, filtered);
                    }
                    ContentMode::Gifs => {
                        let filtered = filtered_media.as_deref().unwrap_or(&[]);
                        if filtered.is_empty() {
                            let message = match self.media_view {
                                MediaView::Library
                                    if self.media_items.is_empty()
                                        && self.media_scan_in_progress() =>
                                {
                                    "Indexing media library..."
                                }
                                MediaView::Library if self.media_items.is_empty() => {
                                    "Drop GIFs, MP4, or WebM here"
                                }
                                MediaView::Library => "No media matches",
                                MediaView::Favorites => "No favorites yet",
                                MediaView::RecentlyUsed => "No recently used GIFs yet",
                            };
                            draw_empty_state(ui, self, message);
                            return;
                        }

                        ui.add_space(chrome.content_top_space);
                        draw_media_grid(ui, self, filtered);
                    }
                }
            });
        self.draw_drop_overlay(ctx);
        self.draw_dev_panel(ctx);
        self.draw_sticker_pack_delete_confirm(ctx);
    }

    fn draw_sticker_pack_delete_confirm(&mut self, ctx: &Context) {
        let Some(pack) = self.pending_sticker_pack_delete.clone() else {
            return;
        };
        let palette = self.settings.palette;

        egui::Window::new("Delete sticker pack")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .default_width(330.0)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(format!("Delete {}?", pack.label))
                        .size(15.0)
                        .strong()
                        .color(palette.text.color()),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(format!(
                        "This removes {} sticker file{} from disk. The pack folder is removed only if it becomes empty.",
                        pack.count,
                        if pack.count == 1 { "" } else { "s" },
                    ))
                    .size(12.0)
                    .color(palette.muted.color()),
                );
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    let delete = Button::new(
                        RichText::new("Delete pack")
                            .size(12.0)
                            .color(Color32::WHITE),
                    )
                    .fill(palette.danger.color());
                    if ui.add(delete).clicked() {
                        self.delete_sticker_pack(&pack.id);
                    }
                    if ui
                        .button(RichText::new("Cancel").color(palette.text.color()))
                        .clicked()
                    {
                        self.cancel_delete_sticker_pack();
                    }
                });
            });
    }

    fn draw_sidebar(&mut self, ctx: &Context) {
        if self.content_mode.media_kind().is_some() {
            self.draw_gif_sidebar(ctx);
            return;
        }

        let chrome = chrome(self.settings.interface_mode);

        egui::SidePanel::left("sidebar")
            .resizable(false)
            .exact_width(chrome.sidebar_width)
            .frame(Frame::none().fill(self.settings.palette.panel_dark.color()))
            .show(ctx, |ui| {
                let nav_height = (ui.available_height() - chrome.sidebar_settings_height).max(0.0);

                ui.allocate_ui_with_layout(
                    egui::vec2(chrome.sidebar_width, nav_height),
                    Layout::top_down(Align::Center),
                    |ui| {
                        self.draw_sidebar_nav_scroll(ui);
                    },
                );

                ui.allocate_ui_with_layout(
                    egui::vec2(chrome.sidebar_width, chrome.sidebar_settings_height),
                    Layout::bottom_up(Align::Center),
                    |ui| {
                        ui.add_space(10.0);
                        self.sidebar_button(ui, Tab::Settings, true, false, 1.0);
                    },
                );
            });
    }

    fn draw_gif_sidebar(&mut self, ctx: &Context) {
        let chrome = chrome(self.settings.interface_mode);

        egui::SidePanel::left("gif_sidebar")
            .resizable(false)
            .exact_width(chrome.sidebar_width)
            .frame(Frame::none().fill(self.settings.palette.panel_dark.color()))
            .show(ctx, |ui| {
                let nav_height = (ui.available_height() - chrome.sidebar_settings_height).max(0.0);

                ui.allocate_ui_with_layout(
                    egui::vec2(chrome.sidebar_width, nav_height),
                    Layout::top_down(Align::Center),
                    |ui| {
                        ui.add_space(10.0);
                        let library_icon = if self.content_mode == ContentMode::Stickers {
                            "▦"
                        } else {
                            "GIF"
                        };
                        self.media_sidebar_button(ui, MediaView::Library, library_icon, true);
                        ui.add_space(8.0);
                        self.media_sidebar_button(ui, MediaView::Favorites, "★", true);
                        ui.add_space(8.0);
                        self.media_sidebar_button(ui, MediaView::RecentlyUsed, "↺", true);
                    },
                );

                ui.allocate_ui_with_layout(
                    egui::vec2(chrome.sidebar_width, chrome.sidebar_settings_height),
                    Layout::bottom_up(Align::Center),
                    |ui| {
                        ui.add_space(10.0);
                        self.sidebar_button(ui, Tab::Settings, true, false, 1.0);
                    },
                );
            });
    }

    fn draw_sticker_pack_sidebar(&mut self, ctx: &Context) {
        let modern = self.settings.interface_mode.is_modern();
        let width = if modern { 176.0 } else { 154.0 };

        egui::SidePanel::left("sticker_pack_sidebar")
            .resizable(false)
            .exact_width(width)
            .frame(Frame::none().fill(self.settings.palette.panel.color()))
            .show(ctx, |ui| {
                ui.add_space(if modern { 14.0 } else { 12.0 });
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new("Sticker packs")
                            .size(12.0)
                            .strong()
                            .color(self.settings.palette.muted.color()),
                    );
                });
                ui.add_space(8.0);

                let packs = self.sticker_packs();
                let total = packs.iter().map(|pack| pack.count).sum::<usize>();

                ScrollArea::vertical()
                    .id_salt("sticker_pack_sidebar")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let all_selected = self.media_view == MediaView::Library
                            && self.selected_sticker_pack_id().is_none();
                        if draw_sticker_pack_row(ui, self, "All stickers", total, all_selected)
                            .clicked()
                        {
                            self.select_sticker_pack(None);
                        }

                        ui.add_space(4.0);
                        for pack in packs {
                            let selected = self.media_view == MediaView::Library
                                && self.selected_sticker_pack_id() == Some(pack.id.as_str());
                            let response =
                                draw_sticker_pack_row(ui, self, &pack.label, pack.count, selected);
                            if response.clicked() {
                                self.select_sticker_pack(Some(pack.id.clone()));
                            }
                            response.context_menu(|ui| {
                                if ui
                                    .button(
                                        RichText::new("Delete sticker pack")
                                            .color(self.settings.palette.danger.color()),
                                    )
                                    .on_hover_text("Deletes every indexed sticker in this pack")
                                    .clicked()
                                {
                                    self.request_delete_sticker_pack(pack.clone());
                                    ui.close_menu();
                                }
                            });
                            ui.add_space(4.0);
                        }

                        if total == 0 {
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                ui.add_space(12.0);
                                ui.label(
                                    RichText::new("No packs yet")
                                        .size(12.0)
                                        .color(self.settings.palette.muted.color()),
                                );
                            });
                        }
                    });
            });
    }

    fn media_sidebar_button(
        &mut self,
        ui: &mut egui::Ui,
        view: MediaView,
        icon: &str,
        enabled: bool,
    ) {
        let response = self.sidebar_icon_button(
            ui,
            view.label(),
            icon,
            self.app_view == AppView::Main && self.media_view == view,
            enabled,
            false,
            false,
            1.0,
        );

        if response.clicked() && enabled {
            self.app_view = AppView::Main;
            self.media_view = view;
        }
    }

    fn draw_sidebar_nav_scroll(&mut self, ui: &mut egui::Ui) {
        ScrollArea::vertical()
            .id_salt("sidebar_nav")
            .auto_shrink([false, false])
            .scroll_bar_visibility(ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(10.0);
                    self.sidebar_button(ui, Tab::Recent, !self.recent.is_empty(), false, 1.0);
                    ui.add_space(8.0);

                    for group in SidebarGroup::ALL {
                        self.sidebar_group_header(ui, group);
                        self.draw_sidebar_group_stack(ui, group);
                        ui.add_space(8.0);
                    }
                });
            });
    }

    fn sidebar_group_header(&mut self, ui: &mut egui::Ui, group: SidebarGroup) {
        let selected = self.active_sidebar_group() == Some(group);
        let emoji_icon = group == SidebarGroup::Emoji;
        let response = self.sidebar_icon_button(
            ui,
            group.label(),
            group.icon(),
            selected,
            true,
            emoji_icon,
            false,
            1.0,
        );

        if response.clicked() {
            self.app_view = AppView::Main;
            self.selected_tab = group.default_tab();
        }
    }

    fn active_sidebar_group(&self) -> Option<SidebarGroup> {
        if self.app_view != AppView::Main {
            return None;
        }

        match self.selected_tab {
            Tab::Category(Category::Emoji) | Tab::EmojiGroup(_) => Some(SidebarGroup::Emoji),
            Tab::Category(category) => SidebarGroup::for_category(category),
            Tab::Recent | Tab::Settings => None,
        }
    }

    fn draw_sidebar_group_stack(&mut self, ui: &mut egui::Ui, group: SidebarGroup) {
        let chrome = chrome(self.settings.interface_mode);
        let open = self.active_sidebar_group() == Some(group);
        let raw_t = ui
            .ctx()
            .animate_bool(ui.id().with(("sidebar_group_stack_open", group)), open);
        if raw_t <= 0.001 {
            return;
        }

        let t = ease_out_cubic(raw_t);
        let item_step = chrome.sidebar_group_size + SIDEBAR_STACK_GAP;
        let item_count = if group == SidebarGroup::Emoji {
            EmojiGroup::ALL.len()
        } else {
            group.categories().len()
        };
        let full_height = SIDEBAR_STACK_GAP + item_count as f32 * item_step;
        let viewport_height =
            full_height.min(SIDEBAR_STACK_GAP + SIDEBAR_STACK_MAX_VISIBLE_ITEMS * item_step);
        let (slot, _) = ui.allocate_exact_size(
            egui::vec2(chrome.sidebar_width, viewport_height * t),
            Sense::hover(),
        );
        let outline_rect = slot.shrink2(egui::vec2(8.0, 1.0));
        let content_rect = Rect::from_min_size(
            egui::pos2(slot.left(), slot.top() - (1.0 - t) * 12.0),
            egui::vec2(slot.width(), viewport_height),
        );
        let mut child_ui = ui.new_child(
            egui::UiBuilder::new()
                .id_salt(("sidebar_group_stack", group))
                .max_rect(content_rect)
                .layout(Layout::top_down(Align::Center)),
        );
        child_ui.set_clip_rect(slot.intersect(ui.clip_rect()));

        let interactive = t > 0.96;
        ScrollArea::vertical()
            .id_salt(("sidebar_group_stack_items", group))
            .auto_shrink([false, false])
            .scroll_bar_visibility(ScrollBarVisibility::AlwaysHidden)
            .show(&mut child_ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(SIDEBAR_STACK_GAP);
                    if group == SidebarGroup::Emoji {
                        for emoji_group in EmojiGroup::ALL {
                            self.sidebar_button(
                                ui,
                                Tab::EmojiGroup(emoji_group),
                                interactive,
                                true,
                                t,
                            );
                            ui.add_space(SIDEBAR_STACK_GAP);
                        }
                    } else {
                        for category in group.categories() {
                            self.sidebar_button(
                                ui,
                                Tab::Category(*category),
                                interactive,
                                false,
                                t,
                            );
                            ui.add_space(SIDEBAR_STACK_GAP);
                        }
                    }
                });
            });

        ui.painter().rect(
            outline_rect,
            Rounding::same(chrome.sidebar_rounding + 2.0),
            Color32::TRANSPARENT,
            Stroke::new(
                1.0,
                fade_color(
                    blend_color(
                        self.settings.palette.accent.color(),
                        self.settings.palette.panel.color(),
                        0.35,
                    ),
                    0.48 * t,
                ),
            ),
        );
    }

    fn sidebar_button(
        &mut self,
        ui: &mut egui::Ui,
        tab: Tab,
        enabled: bool,
        emoji_icon: bool,
        opacity: f32,
    ) {
        let selected = if tab == Tab::Settings {
            self.app_view == AppView::Settings
        } else {
            self.app_view == AppView::Main && self.selected_tab == tab
        };
        let is_group = matches!(tab, Tab::EmojiGroup(_));
        let response = self.sidebar_icon_button(
            ui,
            tab.label(),
            tab.icon(),
            selected,
            enabled,
            emoji_icon,
            is_group,
            opacity,
        );

        if response.clicked() && enabled {
            if tab == Tab::Settings {
                self.app_view = AppView::Settings;
            } else {
                self.app_view = AppView::Main;
                self.selected_tab = tab;
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn sidebar_icon_button(
        &mut self,
        ui: &mut egui::Ui,
        label: &str,
        icon: &str,
        selected: bool,
        enabled: bool,
        emoji_icon: bool,
        is_group: bool,
        opacity: f32,
    ) -> egui::Response {
        let chrome = chrome(self.settings.interface_mode);
        let size = if is_group {
            chrome.sidebar_group_size
        } else {
            chrome.sidebar_button_size
        };
        let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), Sense::click());
        let response = response.on_hover_text(label);
        let hover_t = ui
            .ctx()
            .animate_bool(response.id, response.hovered() && enabled);
        let draw_rect = rect.expand(hover_t * 2.0);
        let palette = self.settings.palette;
        let fill = if selected {
            blend_color(
                palette.accent.color(),
                palette.tile_hover.color(),
                hover_t * 0.16,
            )
        } else if response.hovered() && enabled {
            blend_color(
                Color32::TRANSPARENT,
                palette.tile_hover.color(),
                0.55 + hover_t * 0.35,
            )
        } else {
            Color32::TRANSPARENT
        };
        let stroke = if selected {
            Stroke::new(
                if self.settings.interface_mode.is_modern() {
                    1.5
                } else {
                    1.0
                },
                fade_color(
                    blend_color(palette.accent.color(), palette.text.color(), 0.22),
                    opacity,
                ),
            )
        } else {
            Stroke::new(1.0, Color32::TRANSPARENT)
        };

        ui.painter().rect(
            draw_rect,
            Rounding::same(chrome.sidebar_rounding),
            fade_color(fill, opacity),
            stroke,
        );

        let icon_rect = draw_rect.shrink(if is_group { 6.0 } else { 7.0 });
        if emoji_icon && self.settings.color_emoji {
            if let Some(texture) = self.emoji_cache.texture(ui.ctx(), icon) {
                let image_rect = fit_centered(icon_rect, texture.size_vec2());
                ui.painter().image(
                    texture.id(),
                    image_rect,
                    Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    fade_color(Color32::WHITE, opacity),
                );
            } else {
                self.paint_sidebar_text_icon(ui, icon_rect, icon, is_group, opacity);
            }
        } else {
            self.paint_sidebar_text_icon(ui, icon_rect, icon, is_group, opacity);
        }

        response
    }

    fn paint_sidebar_text_icon(
        &self,
        ui: &egui::Ui,
        rect: Rect,
        icon: &str,
        is_group: bool,
        opacity: f32,
    ) {
        let size = if icon == ":-)" {
            13.0
        } else if is_group {
            16.0
        } else {
            19.0
        };
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            icon,
            FontId::proportional(size),
            fade_color(self.settings.palette.text.color(), opacity),
        );
    }

    fn draw_topbar(&mut self, ctx: &Context) {
        let chrome = chrome(self.settings.interface_mode);
        let modern = self.settings.interface_mode.is_modern();
        let compact = self.compact_topbar(ctx);
        let topbar_height = if compact {
            chrome.topbar_height + 34.0
        } else {
            chrome.topbar_height
        };

        TopBottomPanel::top("top_bar")
            .exact_height(topbar_height)
            .frame(Frame::none().fill(if modern {
                self.settings.palette.bg.color()
            } else {
                self.settings.palette.panel.color()
            }))
            .show(ctx, |ui| {
                if compact {
                    self.draw_compact_topbar(ui, ctx, modern);
                } else {
                    self.draw_full_topbar(ui, modern);
                }
                if modern {
                    let y = ui.max_rect().bottom() - 1.0;
                    ui.painter().line_segment(
                        [
                            egui::pos2(ui.max_rect().left(), y),
                            egui::pos2(ui.max_rect().right(), y),
                        ],
                        Stroke::new(1.0, self.settings.palette.panel.color()),
                    );
                }
                ui.add_space(10.0);
            });
    }

    fn draw_full_topbar(&mut self, ui: &mut egui::Ui, modern: bool) {
        ui.add_space(if modern { 16.0 } else { 13.0 });
        ui.horizontal(|ui| {
            ui.add_space(if modern { 18.0 } else { 14.0 });
            self.draw_mode_switch(ui);
            ui.add_space(8.0);
            ui.label(
                RichText::new(self.topbar_title())
                    .size(if modern { 22.0 } else { 20.0 })
                    .strong()
                    .color(self.settings.palette.text.color()),
            );

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(if modern { 16.0 } else { 12.0 });
                self.draw_topbar_clear_actions(ui, modern);

                if self.app_view == AppView::Main {
                    let width = ui
                        .available_width()
                        .min(if modern { 300.0 } else { 260.0 })
                        .max(150.0);
                    self.draw_topbar_search(ui, modern, width);
                }
            });
        });
    }

    fn draw_compact_topbar(&mut self, ui: &mut egui::Ui, ctx: &Context, modern: bool) {
        ui.add_space(if modern { 10.0 } else { 8.0 });
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            self.draw_mode_switch(ui);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(10.0);
                self.draw_topbar_clear_actions(ui, modern);
            });
        });

        if self.app_view != AppView::Main {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.label(
                    RichText::new(self.topbar_title())
                        .size(if modern { 18.0 } else { 16.0 })
                        .strong()
                        .color(self.settings.palette.text.color()),
                );
            });
            return;
        }

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            if self.content_mode == ContentMode::Stickers
                && !self.should_draw_sticker_pack_sidebar(ctx)
                && self.media_view == MediaView::Library
                && !self.sticker_packs().is_empty()
            {
                self.draw_compact_sticker_pack_picker(ui);
                ui.add_space(6.0);
            }
            let width = ui.available_width().clamp(96.0, 270.0);
            self.draw_topbar_search(ui, modern, width);
        });
    }

    fn draw_topbar_clear_actions(&mut self, ui: &mut egui::Ui, modern: bool) {
        if self.app_view == AppView::Main && self.content_mode.media_kind().is_some() {
            if self.selected_media_count() > 0 {
                if ui
                    .button(RichText::new("Clear").color(self.settings.palette.text.color()))
                    .on_hover_text("Clear media selection")
                    .clicked()
                {
                    self.clear_media_selection();
                }
                ui.add_space(if modern { 8.0 } else { 6.0 });
                if ui
                    .button(RichText::new("Delete").color(self.settings.palette.danger.color()))
                    .on_hover_text("Delete selected library media files")
                    .clicked()
                {
                    self.delete_selected_media_files();
                }
                ui.add_space(if modern { 8.0 } else { 6.0 });
                if ui
                    .button(RichText::new("Unfavorite").color(self.settings.palette.text.color()))
                    .on_hover_text("Remove selected media from favorites")
                    .clicked()
                {
                    self.remove_selected_media_from_favorites();
                }
                ui.add_space(if modern { 8.0 } else { 6.0 });
                if ui
                    .button(RichText::new("Favorite").color(self.settings.palette.text.color()))
                    .on_hover_text("Add selected media to favorites")
                    .clicked()
                {
                    self.add_selected_media_to_favorites();
                }
                ui.add_space(if modern { 12.0 } else { 10.0 });
            }
        }

        if self.app_view == AppView::Main
            && self.content_mode == ContentMode::Symbols
            && self.selected_tab == Tab::Recent
            && !self.recent.is_empty()
        {
            if ui
                .button(RichText::new("Clear").color(self.settings.palette.text.color()))
                .on_hover_text("Clear recent symbols")
                .clicked()
            {
                self.clear_recent();
            }
            ui.add_space(if modern { 12.0 } else { 10.0 });
        }

        if self.app_view == AppView::Main
            && self.content_mode.media_kind().is_some()
            && self.media_view == MediaView::RecentlyUsed
            && !self.recent_media.is_empty()
        {
            if ui
                .button(RichText::new("Clear").color(self.settings.palette.text.color()))
                .on_hover_text("Clear recent media")
                .clicked()
            {
                self.clear_recent_media();
            }
            ui.add_space(if modern { 12.0 } else { 10.0 });
        }
    }

    fn draw_topbar_search(&mut self, ui: &mut egui::Ui, modern: bool, width: f32) {
        let hint = match self.content_mode {
            ContentMode::Symbols => "Search symbols...",
            ContentMode::Stickers => "Search stickers...",
            ContentMode::Gifs => "Search local GIFs...",
        };
        let query = match self.content_mode {
            ContentMode::Symbols => &mut self.query,
            ContentMode::Stickers => &mut self.gif_query,
            ContentMode::Gifs => &mut self.gif_query,
        };
        let response = ui.add_sized(
            [width, if modern { 34.0 } else { 30.0 }],
            TextEdit::singleline(query).hint_text(hint),
        );
        response.request_focus();
    }

    fn draw_compact_sticker_pack_picker(&mut self, ui: &mut egui::Ui) {
        let packs = self.sticker_packs();
        let total = packs.iter().map(|pack| pack.count).sum::<usize>();
        let selected_pack = self
            .selected_sticker_pack_id()
            .and_then(|selected| packs.iter().find(|pack| pack.id == selected))
            .cloned();
        let selected_label = self
            .selected_sticker_pack_id()
            .and_then(|selected| {
                packs
                    .iter()
                    .find(|pack| pack.id == selected)
                    .map(|pack| pack.label.as_str())
            })
            .unwrap_or("All");

        egui::ComboBox::from_id_salt("compact_sticker_pack_picker")
            .selected_text(truncate_chars(selected_label, 14))
            .width(112.0)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(
                        self.selected_sticker_pack_id().is_none(),
                        format!("All ({total})"),
                    )
                    .clicked()
                {
                    self.select_sticker_pack(None);
                    ui.close_menu();
                }

                for pack in &packs {
                    let selected = self.selected_sticker_pack_id() == Some(pack.id.as_str());
                    if ui
                        .selectable_label(
                            selected,
                            format!("{} ({})", truncate_chars(&pack.label, 22), pack.count),
                        )
                        .clicked()
                    {
                        self.select_sticker_pack(Some(pack.id.clone()));
                        ui.close_menu();
                    }
                }
            });

        if let Some(pack) = selected_pack
            && ui
                .add_sized(
                    [28.0, 30.0],
                    Button::new(
                        RichText::new("x")
                            .size(13.0)
                            .strong()
                            .color(self.settings.palette.danger.color()),
                    ),
                )
                .on_hover_text(format!("Delete sticker pack {}", pack.label))
                .clicked()
        {
            self.request_delete_sticker_pack(pack);
        }
    }

    fn draw_mode_switch(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for mode in ContentMode::CHOICES {
                if !self.content_mode_enabled(mode) {
                    continue;
                }
                let selected = self.content_mode == mode;
                let button = Button::new(
                    RichText::new(mode.label())
                        .size(12.0)
                        .color(self.settings.palette.text.color()),
                )
                .fill(if selected {
                    self.settings.palette.accent.color()
                } else {
                    self.settings.palette.tile.color()
                });

                if ui.add(button).clicked() {
                    self.app_view = AppView::Main;
                    self.content_mode = mode;
                }
            }
        });
    }

    fn topbar_title(&self) -> &'static str {
        if self.app_view == AppView::Settings {
            return "Preferences";
        }

        match self.content_mode {
            ContentMode::Symbols => self.selected_tab.label(),
            ContentMode::Stickers => "Stickers",
            ContentMode::Gifs => self.media_view.label(),
        }
    }

    fn should_draw_sticker_pack_sidebar(&self, ctx: &Context) -> bool {
        self.app_view == AppView::Main
            && self.content_mode == ContentMode::Stickers
            && ctx.screen_rect().width() >= STICKER_PACK_SIDEBAR_MIN_WIDTH
    }

    fn compact_topbar(&self, ctx: &Context) -> bool {
        ctx.screen_rect().width() < COMPACT_TOPBAR_MAX_WIDTH
    }

    fn draw_drop_overlay(&self, ctx: &Context) {
        if !has_hovered_files(ctx) {
            return;
        }

        let supported = hovered_media_drop_count(ctx);
        let screen = ctx.screen_rect();
        let rect = screen.shrink2(egui::vec2(28.0, 28.0));
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("media_drop_overlay"),
        ));
        let palette = self.settings.palette;
        let accent = if supported == 0 {
            palette.danger.color()
        } else {
            palette.accent.color()
        };
        let fill = fade_color(
            blend_color(palette.bg.color(), palette.panel.color(), 0.5),
            0.92,
        );

        painter.rect(rect, Rounding::same(12.0), fill, Stroke::new(2.0, accent));
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            if supported == 0 {
                "Drop GIF, MP4, PNG, WebP, WebM, or a folder"
            } else if self.content_mode == ContentMode::Stickers {
                "Drop to add to sticker library"
            } else {
                "Drop to add to GIF library"
            },
            FontId::proportional(20.0),
            self.settings.palette.text.color(),
        );
    }

    fn draw_dev_panel(&mut self, ctx: &Context) {
        if !self.dev_panel_open {
            return;
        }

        ctx.request_repaint_after(Duration::from_secs(1));
        let snapshot = self.dev_metrics.snapshot().clone();
        let mut open = self.dev_panel_open;
        let palette = self.settings.palette;

        egui::Window::new("Dev stuff")
            .open(&mut open)
            .default_width(380.0)
            .default_pos(egui::pos2(84.0, 86.0))
            .resizable(true)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new("F7 toggles this panel")
                        .size(12.0)
                        .color(palette.muted.color()),
                );
                ui.add_space(8.0);
                draw_dev_metrics(ui, palette, &snapshot);
                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);
                draw_dev_console(ui, self, palette);
                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);
                ui.label(
                    RichText::new("Danger Zone")
                        .strong()
                        .color(palette.danger.color()),
                );
                ui.label(
                    RichText::new(
                        "Clears Symbolis config, cache, recent/favorites, token, indexes, and local stored media. External folders are not deleted.",
                    )
                    .size(12.0)
                    .color(palette.muted.color()),
                );
                ui.add_space(6.0);

                if self.clear_everything_confirm {
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .add(
                                Button::new(
                                    RichText::new("Really clear everything")
                                        .color(Color32::WHITE),
                                )
                                .fill(palette.danger.color()),
                            )
                            .clicked()
                        {
                            self.clear_everything();
                        }
                        if ui
                            .add(
                                Button::new(
                                    RichText::new("Cancel").color(palette.text.color()),
                                )
                                .fill(palette.tile.color()),
                            )
                            .clicked()
                        {
                            self.clear_everything_confirm = false;
                        }
                    });
                } else if ui
                    .add(
                        Button::new(RichText::new("Clear everything").color(Color32::WHITE))
                            .fill(palette.danger.color()),
                    )
                    .clicked()
                {
                    self.clear_everything_confirm = true;
                }
            });

        self.dev_panel_open = open;
        if !self.dev_panel_open {
            self.clear_everything_confirm = false;
        }
    }

    fn draw_footer(&self, ctx: &Context, count: usize) {
        let chrome = chrome(self.settings.interface_mode);

        TopBottomPanel::bottom("footer")
            .exact_height(chrome.footer_height)
            .frame(Frame::none().fill(self.settings.palette.panel_dark.color()))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new(format!("{count} items"))
                            .size(12.0)
                            .color(self.settings.palette.muted.color()),
                    );

                    if let Some(status) = &self.status {
                        ui.separator();
                        let color = if status_is_error(status) {
                            self.settings.palette.danger.color()
                        } else {
                            self.settings.palette.muted.color()
                        };
                        ui.label(RichText::new(status).size(12.0).color(color));
                        return;
                    }

                    ui.separator();
                    match self.content_mode {
                        ContentMode::Symbols => {
                            let source = match &self.data_source {
                                DataSource::Rofimoji(path) => path.display().to_string(),
                                DataSource::BuiltIn => "built-in fallback".to_owned(),
                                DataSource::Disabled => "symbols disabled".to_owned(),
                            };
                            ui.label(
                                RichText::new(source)
                                    .size(12.0)
                                    .color(self.settings.palette.muted.color()),
                            );

                            ui.separator();
                            ui.label(
                                RichText::new(self.delivery_status())
                                    .size(12.0)
                                    .color(self.settings.palette.muted.color()),
                            );

                            ui.separator();
                            let color = if self.emoji_cache.color_renderer_available() {
                                self.settings.palette.muted.color()
                            } else {
                                self.settings.palette.danger.color()
                            };
                            ui.label(
                                RichText::new(self.color_emoji_status())
                                    .size(12.0)
                                    .color(color),
                            );
                        }
                        ContentMode::Stickers | ContentMode::Gifs => {
                            let media_kind = self
                                .content_mode
                                .media_kind()
                                .expect("media content mode has a media kind");
                            let indexed_count = self
                                .media_items
                                .iter()
                                .filter(|item| item.kind == media_kind)
                                .count();
                            let noun = match self.content_mode {
                                ContentMode::Stickers => "stickers",
                                ContentMode::Gifs => "GIFs",
                                ContentMode::Symbols => "media files",
                            };
                            ui.label(
                                RichText::new(format!("local library: {indexed_count} {noun}",))
                                    .size(12.0)
                                    .color(self.settings.palette.muted.color()),
                            );

                            ui.separator();
                            ui.label(
                                RichText::new(self.delivery_status())
                                    .size(12.0)
                                    .color(self.settings.palette.muted.color()),
                            );
                        }
                    }

                    ui.separator();
                    ui.label(
                        RichText::new(self.gif_provider_status())
                            .size(12.0)
                            .color(self.settings.palette.muted.color()),
                    );
                });
            });
    }

    fn draw_settings(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        let mut changed = false;
        let mut manual_changed = false;
        let mut theme_changed = false;
        let mut features_changed = false;
        let mut hotkeys_changed = false;

        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(18.0);
                ui.horizontal(|ui| {
                    ui.add_space(16.0);
                    ui.vertical(|ui| {
                        ui.set_width((ui.available_width() - 20.0).max(280.0));

                        settings_panel(ui, "Appearance", self.settings.palette, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                for mode in InterfaceMode::CHOICES {
                                    let selected = self.settings.interface_mode == mode;
                                    let button = Button::new(
                                        RichText::new(mode.label())
                                            .color(self.settings.palette.text.color()),
                                    )
                                    .fill(if selected {
                                        self.settings.palette.accent.color()
                                    } else {
                                        self.settings.palette.tile.color()
                                    });

                                    if ui.add(button).on_hover_text(mode.description()).clicked() {
                                        self.settings.apply_interface_mode(mode);
                                        changed = true;
                                    }
                                }
                            });

                            ui.add_space(10.0);
                            ui.horizontal_wrapped(|ui| {
                                for preset in Preset::CHOICES {
                                    let selected =
                                        self.settings.theme == ThemeSelection::Preset(preset);
                                    let button = Button::new(
                                        RichText::new(preset.label())
                                            .color(self.settings.palette.text.color()),
                                    )
                                    .fill(if selected {
                                        self.settings.palette.accent.color()
                                    } else {
                                        self.settings.palette.tile.color()
                                    });

                                    if ui.add(button).clicked() {
                                        self.settings.apply_preset(preset);
                                        changed = true;
                                    }
                                }
                            });

                            if !self.settings.custom_themes.is_empty() {
                                ui.add_space(10.0);
                                ui.horizontal_wrapped(|ui| {
                                    let themes: Vec<String> = self
                                        .settings
                                        .custom_themes
                                        .iter()
                                        .map(|theme| theme.name.clone())
                                        .collect();
                                    for name in themes {
                                        let selected = self.settings.theme
                                            == ThemeSelection::Custom(name.clone());
                                        let button = Button::new(
                                            RichText::new(&name)
                                                .color(self.settings.palette.text.color()),
                                        )
                                        .fill(if selected {
                                            self.settings.palette.accent.color()
                                        } else {
                                            self.settings.palette.tile.color()
                                        });

                                        if ui.add(button).clicked() {
                                            self.settings.apply_custom_theme(&name);
                                            changed = true;
                                        }
                                    }
                                });
                            }

                            if let Some(name) = self
                                .settings
                                .selected_custom_theme_name()
                                .map(str::to_owned)
                            {
                                ui.add_space(10.0);
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new("Name")
                                            .size(12.0)
                                            .color(self.settings.palette.muted.color()),
                                    );
                                    let mut editable_name = name.clone();
                                    let response = ui.add_sized(
                                        [180.0, 28.0],
                                        TextEdit::singleline(&mut editable_name),
                                    );
                                    if response.changed() && editable_name != name {
                                        self.settings.rename_selected_custom_theme(editable_name);
                                        changed = true;
                                    }
                                    if ui
                                        .add(
                                            Button::new(
                                                RichText::new("Delete")
                                                    .color(self.settings.palette.danger.color()),
                                            )
                                            .fill(self.settings.palette.tile.color()),
                                        )
                                        .on_hover_text("Delete selected custom theme")
                                        .clicked()
                                    {
                                        self.settings.delete_selected_custom_theme();
                                        changed = true;
                                    }
                                });
                            }

                            ui.add_space(10.0);
                            if ui
                                .checkbox(&mut self.settings.color_emoji, "Color emoji")
                                .on_hover_text("Render emoji tiles through the local color cache")
                                .changed()
                            {
                                changed = true;
                                manual_changed = true;
                            }
                        });

                        ui.add_space(12.0);
                        settings_panel(ui, "Features", self.settings.palette, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                features_changed |= ui
                                    .checkbox(&mut self.settings.features.symbols, "Symbols")
                                    .on_hover_text(
                                        "Load emoji, kaomoji, symbols, and wide glyph fonts",
                                    )
                                    .changed();
                                features_changed |= ui
                                    .checkbox(&mut self.settings.features.stickers, "Stickers")
                                    .on_hover_text("Index and show local sticker media")
                                    .changed();
                                features_changed |= ui
                                    .checkbox(&mut self.settings.features.gifs, "GIFs")
                                    .on_hover_text("Index and show GIF/video media")
                                    .changed();
                            });
                            ui.add_space(8.0);
                            ui.horizontal_wrapped(|ui| {
                                features_changed |= ui
                                    .checkbox(
                                        &mut self.settings.features.media_watcher,
                                        "Watch media folders",
                                    )
                                    .on_hover_text(
                                        "Automatically reindex media when watched folders change",
                                    )
                                    .changed();
                                features_changed |= ui
                                    .checkbox(
                                        &mut self.settings.features.deduplicate_media,
                                        "Deduplicate media",
                                    )
                                    .on_hover_text(
                                        "Collapse identical files during media library scans",
                                    )
                                    .changed();
                            });
                        });

                        ui.add_space(12.0);
                        settings_panel(ui, "Global Hotkey", self.settings.palette, |ui| {
                            hotkeys_changed |= ui
                                .checkbox(
                                    &mut self.settings.hotkeys.enabled,
                                    "Enable built-in hotkey backend",
                                )
                                .on_hover_text("Optional X11-oriented backend; system launcher shortcut below is preferred on Wayland")
                                .changed();
                            ui.add_space(8.0);
                            for action in HotkeyAction::CHOICES {
                                hotkeys_changed |= draw_hotkey_row(self, ui, ctx, action);
                            }

                            ui.label(
                                RichText::new(self.global_hotkey_status())
                                    .size(12.0)
                                    .color(self.settings.palette.muted.color()),
                            );

                            ui.add_space(10.0);
                            ui.separator();
                            ui.add_space(8.0);
                            ui.label(
                                RichText::new("System launcher command")
                                    .size(12.0)
                                    .strong()
                                    .color(self.settings.palette.text.color()),
                            );
                            ui.label(
                                RichText::new(self.toggle_command_label())
                                    .size(12.0)
                                    .monospace()
                                    .color(self.settings.palette.muted.color()),
                            );
                            ui.horizontal_wrapped(|ui| {
                                if ui
                                    .add(
                                        Button::new(
                                            RichText::new("Copy command")
                                                .color(self.settings.palette.text.color()),
                                        )
                                        .fill(self.settings.palette.tile.color()),
                                    )
                                    .on_hover_text("Use this command in your desktop shortcut settings")
                                    .clicked()
                                {
                                    self.copy_toggle_command();
                                }

                                if ui
                                    .add(
                                        Button::new(
                                            RichText::new("Install launcher")
                                                .color(self.settings.palette.text.color()),
                                        )
                                        .fill(self.settings.palette.tile.color()),
                                    )
                                    .on_hover_text("Creates ~/.local/share/applications/symbolis-toggle.desktop")
                                    .clicked()
                                {
                                    self.install_toggle_desktop_launcher();
                                }

                                if ui
                                    .add(
                                        Button::new(
                                            RichText::new("Apply KDE shortcut")
                                                .color(self.settings.palette.text.color()),
                                        )
                                        .fill(self.settings.palette.tile.color()),
                                    )
                                    .on_hover_text(
                                        "Writes Plasma's kglobalshortcutsrc using the selected global hotkey",
                                    )
                                    .clicked()
                                {
                                    self.apply_kde_toggle_shortcut();
                                }
                            });
                            ui.label(
                                RichText::new(
                                    "Bind this command or launcher in your desktop's keyboard shortcuts. It starts Symbolis if needed, shows it if hidden/minimized, and hides it when pressed again while focused.",
                                )
                                .size(12.0)
                                .color(self.settings.palette.muted.color()),
                            );
                        });

                        ui.add_space(12.0);
                        settings_panel(ui, "Media Sources", self.settings.palette, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                for provider in GifProvider::CHOICES {
                                    let selected = self.settings.gif_provider == provider;
                                    let button = Button::new(
                                        RichText::new(provider.label())
                                            .color(self.settings.palette.text.color()),
                                    )
                                    .fill(if selected {
                                        self.settings.palette.accent.color()
                                    } else {
                                        self.settings.palette.tile.color()
                                    });

                                    if ui
                                        .add(button)
                                        .on_hover_text(provider.description())
                                        .clicked()
                                    {
                                        self.settings.gif_provider = provider;
                                        changed = true;
                                    }
                                }
                            });

                            ui.add_space(10.0);
                            let provider = self.settings.gif_provider;
                            let status = provider.status();
                            let status_color = match status {
                                ProviderStatus::Ready(_) => self.settings.palette.muted.color(),
                                ProviderStatus::MissingApiKey(_) => {
                                    self.settings.palette.danger.color()
                                }
                            };
                            ui.label(
                                RichText::new(format!("{}: {}", provider.label(), status.label()))
                                    .size(12.0)
                                    .color(status_color),
                            );
                            if let Some(attribution) = provider.attribution() {
                                ui.label(
                                    RichText::new(attribution)
                                        .size(12.0)
                                        .strong()
                                        .color(self.settings.palette.muted.color()),
                                );
                            }

                            ui.add_space(12.0);
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("Telegram bot token")
                                        .size(12.0)
                                        .color(self.settings.palette.text.color()),
                                );
                                if ui
                                    .add(
                                        Button::new(
                                            RichText::new("Guide")
                                                .color(self.settings.palette.text.color()),
                                        )
                                        .fill(self.settings.palette.tile.color()),
                                    )
                                    .clicked()
                                {
                                    self.toggle_telegram_bot_token_guide();
                                }
                            });
                            ui.horizontal(|ui| {
                                let response = ui.add_sized(
                                    [ui.available_width().min(420.0), 28.0],
                                    TextEdit::singleline(&mut self.telegram_bot_token_input)
                                        .password(true)
                                        .hint_text("BotFather HTTP API token"),
                                );

                                if ui
                                    .add(
                                        Button::new(
                                            RichText::new("Save")
                                                .color(self.settings.palette.text.color()),
                                        )
                                        .fill(self.settings.palette.tile.color()),
                                    )
                                    .clicked()
                                    || (response.lost_focus()
                                        && ui.input(|input| input.key_pressed(Key::Enter)))
                                {
                                    self.save_telegram_bot_token_setting();
                                }

                                if ui
                                    .add(
                                        Button::new(
                                            RichText::new("Clear")
                                                .color(self.settings.palette.danger.color()),
                                        )
                                        .fill(self.settings.palette.tile.color()),
                                    )
                                    .clicked()
                                {
                                    self.clear_telegram_bot_token_setting();
                                }
                            });
                            ui.label(
                                RichText::new(self.telegram_bot_token_status())
                                    .size(12.0)
                                    .color(self.settings.palette.muted.color()),
                            );
                            if self.telegram_bot_token_guide_visible {
                                ui.label(
                                    RichText::new(
                                        "Guide: open @BotFather in Telegram, send /newbot, copy the HTTP API token, paste it here, then Save. If a token leaks, revoke it in BotFather and save the new one.",
                                    )
                                    .size(12.0)
                                    .color(self.settings.palette.muted.color()),
                                );
                            }

                            ui.add_space(12.0);
                            ui.horizontal(|ui| {
                                let response = ui.add_sized(
                                    [ui.available_width().min(420.0), 28.0],
                                    TextEdit::singleline(&mut self.gif_import_path_input)
                                        .hint_text("/path/to/folder, file.mp4, or t.me/addstickers/..."),
                                );

                                if ui
                                    .add(
                                        Button::new(
                                            RichText::new("Add")
                                                .color(self.settings.palette.text.color()),
                                        )
                                        .fill(self.settings.palette.tile.color()),
                                    )
                                    .clicked()
                                    || (response.lost_focus()
                                        && ui.input(|input| input.key_pressed(Key::Enter)))
                                {
                                    let input = self.gif_import_path_input.trim();
                                    if !input.is_empty() {
                                        self.add_media_import_path(input.into());
                                        self.gif_import_path_input.clear();
                                    }
                                }

                                if ui
                                    .add(
                                        Button::new(
                                            RichText::new("Rescan")
                                                .color(self.settings.palette.text.color()),
                                        )
                                        .fill(self.settings.palette.tile.color()),
                                    )
                                    .clicked()
                                {
                                    self.reload_media_library();
                                }
                            });

                            ui.add_space(8.0);
                            ui.label(
                                RichText::new(format!(
                                    "Indexed {} local media files. Drop GIF/MP4/WebM files to store them locally, add folders, or paste a Telegram sticker set link.",
                                    self.media_items.len()
                                ))
                                .size(12.0)
                                .color(self.settings.palette.muted.color()),
                            );

                            if !self.settings.gif_import_paths.is_empty() {
                                ui.add_space(8.0);
                            }

                            let import_paths = self.settings.gif_import_paths.clone();
                            for path in import_paths {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(truncate_chars(
                                            &path.display().to_string(),
                                            72,
                                        ))
                                        .size(12.0)
                                        .color(self.settings.palette.muted.color()),
                                    );
                                    if ui
                                        .small_button(
                                            RichText::new("Remove")
                                                .color(self.settings.palette.danger.color()),
                                        )
                                        .clicked()
                                    {
                                        self.remove_media_import_path(&path);
                                    }
                                });
                            }
                        });

                        ui.add_space(12.0);
                        settings_panel(ui, "System", self.settings.palette, |ui| {
                            ui.label(
                                RichText::new(self.delivery_status())
                                    .size(12.0)
                                    .color(self.settings.palette.muted.color()),
                            );

                            let color = if self.emoji_cache.color_renderer_available() {
                                self.settings.palette.muted.color()
                            } else {
                                self.settings.palette.danger.color()
                            };
                            ui.label(
                                RichText::new(self.color_emoji_status())
                                    .size(12.0)
                                    .color(color),
                            );

                            if !self.startup_warnings.is_empty() {
                                ui.add_space(8.0);
                            }

                            for warning in &self.startup_warnings {
                                ui.label(
                                    RichText::new(format!(
                                        "{}: {}",
                                        warning.feature, warning.message
                                    ))
                                    .size(12.0)
                                    .color(self.settings.palette.danger.color()),
                                );
                                if let Some(hint) = &warning.hint {
                                    ui.label(
                                        RichText::new(hint)
                                            .size(12.0)
                                            .color(self.settings.palette.muted.color()),
                                    );
                                }
                            }

                            ui.add_space(10.0);
                            ui.horizontal_wrapped(|ui| {
                                if ui
                                    .add(
                                        Button::new(
                                            RichText::new("Hide window")
                                                .color(self.settings.palette.text.color()),
                                        )
                                        .fill(self.settings.palette.tile.color()),
                                    )
                                    .on_hover_text("Keep Symbolis running in the background")
                                    .clicked()
                                {
                                    self.hide_window(ctx);
                                }
                                if ui
                                    .add(
                                        Button::new(
                                            RichText::new("Quit Symbolis")
                                                .color(self.settings.palette.danger.color()),
                                        )
                                        .fill(self.settings.palette.tile.color()),
                                    )
                                    .on_hover_text("Exit the background process")
                                    .clicked()
                                {
                                    self.quit_app(ctx);
                                }
                            });
                        });

                        ui.add_space(12.0);
                        settings_panel(ui, "Palette", self.settings.palette, |ui| {
                            egui::Grid::new("color_settings")
                                .num_columns(4)
                                .spacing([18.0, 10.0])
                                .striped(false)
                                .show(ui, |ui| {
                                    theme_changed |=
                                        color_row(ui, "Background", &mut self.settings.palette.bg);
                                    theme_changed |=
                                        color_row(ui, "Surface", &mut self.settings.palette.panel);
                                    ui.end_row();
                                    theme_changed |= color_row(
                                        ui,
                                        "Sidebar",
                                        &mut self.settings.palette.panel_dark,
                                    );
                                    theme_changed |=
                                        color_row(ui, "Tile", &mut self.settings.palette.tile);
                                    ui.end_row();
                                    theme_changed |= color_row(
                                        ui,
                                        "Tile hover",
                                        &mut self.settings.palette.tile_hover,
                                    );
                                    theme_changed |=
                                        color_row(ui, "Accent", &mut self.settings.palette.accent);
                                    ui.end_row();
                                    theme_changed |=
                                        color_row(ui, "Text", &mut self.settings.palette.text);
                                    theme_changed |=
                                        color_row(ui, "Muted", &mut self.settings.palette.muted);
                                    ui.end_row();
                                });
                        });

                        ui.add_space(12.0);
                        settings_panel(ui, "Sizing", self.settings.palette, |ui| {
                            ui.columns(2, |columns| {
                                manual_changed |= columns[0]
                                    .add(
                                        egui::Slider::new(
                                            &mut self.settings.tile_height,
                                            58.0..=96.0,
                                        )
                                        .text("Tile height"),
                                    )
                                    .changed();
                                manual_changed |= columns[1]
                                    .add(
                                        egui::Slider::new(
                                            &mut self.settings.emoji_size,
                                            22.0..=44.0,
                                        )
                                        .text("Emoji"),
                                    )
                                    .changed();
                            });
                            ui.columns(2, |columns| {
                                manual_changed |= columns[0]
                                    .add(
                                        egui::Slider::new(
                                            &mut self.settings.symbol_size,
                                            22.0..=44.0,
                                        )
                                        .text("Symbols"),
                                    )
                                    .changed();
                                manual_changed |= columns[1]
                                    .add(
                                        egui::Slider::new(
                                            &mut self.settings.kaomoji_size,
                                            12.0..=24.0,
                                        )
                                        .text("Kaomoji"),
                                    )
                                    .changed();
                            });
                        });
                    });
                });
                ui.add_space(16.0);
            });

        changed |= manual_changed;
        changed |= theme_changed;
        changed |= features_changed;
        changed |= hotkeys_changed;

        if changed {
            if theme_changed {
                self.settings.ensure_editable_theme();
            }
            if features_changed {
                self.apply_feature_settings(ctx);
            }
            if hotkeys_changed {
                self.rebuild_global_hotkeys();
            }
            crate::settings::configure_style(ctx, &self.settings);
            self.save_settings();
        }
    }
}

fn draw_sticker_pack_row(
    ui: &mut egui::Ui,
    app: &SymbolisApp,
    label: &str,
    count: usize,
    selected: bool,
) -> egui::Response {
    let modern = app.settings.interface_mode.is_modern();
    let height = if modern { 34.0 } else { 30.0 };
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), height), Sense::click());
    let row_rect = rect.shrink2(egui::vec2(10.0, 1.0));
    let palette = app.settings.palette;
    let hover_t = ui.ctx().animate_bool(response.id, response.hovered());
    let fill = if selected {
        blend_color(palette.accent.color(), palette.tile.color(), 0.18)
    } else {
        blend_color(palette.panel.color(), palette.tile.color(), hover_t * 0.62)
    };
    let stroke = if selected {
        Stroke::new(1.0, palette.accent.color())
    } else {
        Stroke::new(1.0, Color32::TRANSPARENT)
    };

    ui.painter()
        .rect(row_rect, Rounding::same(6.0), fill, stroke);

    let count_text = count.to_string();
    let count_width = (count_text.len() as f32 * 7.0 + 12.0).max(22.0);
    let label_limit = ((row_rect.width() - count_width - 18.0) / 6.6)
        .floor()
        .max(4.0) as usize;

    ui.painter().text(
        egui::pos2(row_rect.left() + 9.0, row_rect.center().y),
        Align2::LEFT_CENTER,
        truncate_chars(label, label_limit),
        FontId::proportional(if modern { 12.5 } else { 12.0 }),
        palette.text.color(),
    );
    ui.painter().text(
        egui::pos2(row_rect.right() - 9.0, row_rect.center().y),
        Align2::RIGHT_CENTER,
        count_text,
        FontId::proportional(11.0),
        palette.muted.color(),
    );

    response.on_hover_text(format!("{label} ({count})"))
}

fn draw_hotkey_row(
    app: &mut SymbolisApp,
    ui: &mut egui::Ui,
    ctx: &Context,
    action: HotkeyAction,
) -> bool {
    let mut changed = false;
    let palette = app.settings.palette;
    let capturing = app.capture_hotkey_action == Some(action);

    if capturing && let Some(key) = capture_hotkey_key(ctx) {
        let binding = app
            .settings
            .hotkeys
            .binding_mut(action)
            .get_or_insert_with(|| HotkeyBinding::new(""));
        binding.key = key;
        app.capture_hotkey_action = None;
        changed = true;
    }

    egui::Grid::new(format!("global_hotkey_binding_grid_{}", action.id()))
        .num_columns(3)
        .spacing([16.0, 8.0])
        .show(ui, |ui| {
            ui.label(
                RichText::new(action.label())
                    .size(12.0)
                    .color(palette.text.color()),
            );

            ui.horizontal(|ui| {
                let binding = app
                    .settings
                    .hotkeys
                    .binding_mut(action)
                    .get_or_insert_with(|| HotkeyBinding::new(""));
                changed |= ui.checkbox(&mut binding.shift, "Shift").changed();
                changed |= ui.checkbox(&mut binding.control, "Ctrl").changed();
                changed |= ui.checkbox(&mut binding.alt, "Alt").changed();
                changed |= ui.checkbox(&mut binding.super_key, "Super").changed();

                let key_label = if binding.key.trim().is_empty() {
                    "None".to_owned()
                } else {
                    hotkey_key_label(&binding.key).to_owned()
                };
                ui.label(
                    RichText::new(format!("Key: {key_label}"))
                        .size(12.0)
                        .color(palette.muted.color()),
                );
            });

            ui.horizontal(|ui| {
                if ui
                    .add(
                        Button::new(
                            RichText::new(if capturing {
                                "Press key..."
                            } else {
                                "Set Global Hotkey"
                            })
                            .color(palette.text.color()),
                        )
                        .fill(if capturing {
                            palette.accent.color()
                        } else {
                            palette.tile.color()
                        }),
                    )
                    .on_hover_text("Press the main key after clicking; modifiers are selected here")
                    .clicked()
                {
                    app.capture_hotkey_action = Some(action);
                }

                if ui
                    .small_button(RichText::new("Clear").color(palette.danger.color()))
                    .clicked()
                {
                    *app.settings.hotkeys.binding_mut(action) = None;
                    if app.capture_hotkey_action == Some(action) {
                        app.capture_hotkey_action = None;
                    }
                    changed = true;
                }
            });
            ui.end_row();

            ui.label("");
            let label = app
                .settings
                .hotkeys
                .binding(action)
                .filter(|binding| !binding.key.trim().is_empty())
                .map(HotkeyBinding::label)
                .unwrap_or_else(|| "None".to_owned());
            ui.label(
                RichText::new(format!("Global hotkey: {label}"))
                    .size(12.0)
                    .color(palette.muted.color()),
            );
            ui.label("");
            ui.end_row();
        });

    changed
}

fn capture_hotkey_key(ctx: &Context) -> Option<String> {
    ctx.input(|input| {
        for key in egui::Key::ALL {
            if input.key_pressed(*key)
                && let Some(code) = hotkey_code_for_egui_key(*key)
            {
                return Some(code.to_owned());
            }
        }
        None
    })
}

fn hotkey_code_for_egui_key(key: egui::Key) -> Option<&'static str> {
    use egui::Key;
    match key {
        Key::ArrowDown => Some("ArrowDown"),
        Key::ArrowLeft => Some("ArrowLeft"),
        Key::ArrowRight => Some("ArrowRight"),
        Key::ArrowUp => Some("ArrowUp"),
        Key::Tab => Some("Tab"),
        Key::Backspace => Some("Backspace"),
        Key::Enter => Some("Enter"),
        Key::Space => Some("Space"),
        Key::Insert => Some("Insert"),
        Key::Delete => Some("Delete"),
        Key::Home => Some("Home"),
        Key::End => Some("End"),
        Key::PageUp => Some("PageUp"),
        Key::PageDown => Some("PageDown"),
        Key::Comma => Some("Comma"),
        Key::Backslash | Key::Pipe => Some("Backslash"),
        Key::Slash | Key::Questionmark => Some("Slash"),
        Key::OpenBracket => Some("BracketLeft"),
        Key::CloseBracket => Some("BracketRight"),
        Key::Backtick => Some("Backquote"),
        Key::Minus => Some("Minus"),
        Key::Period => Some("Period"),
        Key::Plus | Key::Equals => Some("Equal"),
        Key::Semicolon | Key::Colon => Some("Semicolon"),
        Key::Quote => Some("Quote"),
        Key::Num0 => Some("Digit0"),
        Key::Num1 => Some("Digit1"),
        Key::Num2 => Some("Digit2"),
        Key::Num3 => Some("Digit3"),
        Key::Num4 => Some("Digit4"),
        Key::Num5 => Some("Digit5"),
        Key::Num6 => Some("Digit6"),
        Key::Num7 => Some("Digit7"),
        Key::Num8 => Some("Digit8"),
        Key::Num9 => Some("Digit9"),
        Key::A => Some("KeyA"),
        Key::B => Some("KeyB"),
        Key::C => Some("KeyC"),
        Key::D => Some("KeyD"),
        Key::E => Some("KeyE"),
        Key::F => Some("KeyF"),
        Key::G => Some("KeyG"),
        Key::H => Some("KeyH"),
        Key::I => Some("KeyI"),
        Key::J => Some("KeyJ"),
        Key::K => Some("KeyK"),
        Key::L => Some("KeyL"),
        Key::M => Some("KeyM"),
        Key::N => Some("KeyN"),
        Key::O => Some("KeyO"),
        Key::P => Some("KeyP"),
        Key::Q => Some("KeyQ"),
        Key::R => Some("KeyR"),
        Key::S => Some("KeyS"),
        Key::T => Some("KeyT"),
        Key::U => Some("KeyU"),
        Key::V => Some("KeyV"),
        Key::W => Some("KeyW"),
        Key::X => Some("KeyX"),
        Key::Y => Some("KeyY"),
        Key::Z => Some("KeyZ"),
        Key::F1 => Some("F1"),
        Key::F2 => Some("F2"),
        Key::F3 => Some("F3"),
        Key::F4 => Some("F4"),
        Key::F5 => Some("F5"),
        Key::F6 => Some("F6"),
        Key::F7 => Some("F7"),
        Key::F8 => Some("F8"),
        Key::F9 => Some("F9"),
        Key::F10 => Some("F10"),
        Key::F11 => Some("F11"),
        Key::F12 => Some("F12"),
        Key::F13 => Some("F13"),
        Key::F14 => Some("F14"),
        Key::F15 => Some("F15"),
        Key::F16 => Some("F16"),
        Key::F17 => Some("F17"),
        Key::F18 => Some("F18"),
        Key::F19 => Some("F19"),
        Key::F20 => Some("F20"),
        Key::F21 => Some("F21"),
        Key::F22 => Some("F22"),
        Key::F23 => Some("F23"),
        Key::F24 => Some("F24"),
        Key::F25 => Some("F25"),
        Key::F26 => Some("F26"),
        Key::F27 => Some("F27"),
        Key::F28 => Some("F28"),
        Key::F29 => Some("F29"),
        Key::F30 => Some("F30"),
        Key::F31 => Some("F31"),
        Key::F32 => Some("F32"),
        Key::F33 => Some("F33"),
        Key::F34 => Some("F34"),
        Key::F35 => Some("F35"),
        Key::Escape | Key::Copy | Key::Cut | Key::Paste => None,
    }
}

fn draw_dev_metrics(ui: &mut egui::Ui, palette: Palette, snapshot: &DevMetricsSnapshot) {
    draw_percent_row(ui, palette, "System CPU", snapshot.system_cpu_percent);
    draw_percent_row(ui, palette, "Process CPU", snapshot.process_cpu_percent);

    ui.add_space(8.0);
    if let Some(memory) = snapshot.system_memory {
        draw_bytes_row(
            ui,
            palette,
            "RAM",
            memory.used_bytes(),
            memory.total_bytes,
            Some(memory.used_percent()),
        );
    } else {
        draw_unavailable_row(ui, palette, "RAM");
    }

    if let Some(memory) = snapshot.process_memory {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Process memory")
                    .size(12.0)
                    .color(palette.text.color()),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!(
                        "RSS {} / virtual {}",
                        format_bytes(memory.resident_bytes),
                        format_bytes(memory.virtual_bytes)
                    ))
                    .size(12.0)
                    .color(palette.muted.color()),
                );
            });
        });
    } else {
        draw_unavailable_row(ui, palette, "Process memory");
    }

    ui.add_space(8.0);
    if snapshot.gpu.is_empty() {
        ui.label(
            RichText::new("GPU: unavailable via sysfs")
                .size(12.0)
                .color(palette.muted.color()),
        );
    } else {
        for gpu in &snapshot.gpu {
            draw_gpu_row(ui, palette, gpu);
        }
    }

    ui.add_space(8.0);
    if let Some(storage) = snapshot.media_storage {
        let suffix = if storage.truncated { " +" } else { "" };
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Local media storage")
                    .size(12.0)
                    .color(palette.text.color()),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{}{suffix}", format_bytes(storage.bytes)))
                        .size(12.0)
                        .color(palette.muted.color()),
                );
            });
        });
    } else {
        draw_unavailable_row(ui, palette, "Local media storage");
    }
}

fn draw_dev_console(ui: &mut egui::Ui, app: &SymbolisApp, palette: Palette) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Console log")
                .strong()
                .color(palette.text.color()),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new(format!(
                    "jobs {} / scans {}",
                    app.active_media_job_count(),
                    app.active_media_scan_count()
                ))
                .size(12.0)
                .color(palette.muted.color()),
            );
        });
    });

    let log_count = app.dev_log_entries().len();
    if log_count == 0 {
        ui.label(
            RichText::new("No events yet")
                .size(12.0)
                .color(palette.muted.color()),
        );
        return;
    }

    ScrollArea::vertical()
        .id_salt("dev_console_log")
        .max_height(150.0)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for entry in app.dev_log_entries().rev().take(80) {
                ui.horizontal_top(|ui| {
                    ui.label(
                        RichText::new(format_log_time(entry.elapsed_ms))
                            .size(11.0)
                            .monospace()
                            .color(palette.muted.color()),
                    );
                    ui.label(
                        RichText::new(&entry.message)
                            .size(12.0)
                            .color(palette.text.color()),
                    );
                });
            }
        });
}

fn draw_gpu_row(ui: &mut egui::Ui, palette: Palette, gpu: &GpuMetric) {
    draw_percent_row(
        ui,
        palette,
        &gpu.label,
        gpu.usage_percent.map(|value| value as f32),
    );

    match (gpu.vram_used_bytes, gpu.vram_total_bytes) {
        (Some(used), Some(total)) => {
            draw_bytes_row(ui, palette, "VRAM", used, total, gpu.vram_used_percent());
        }
        (Some(used), None) => {
            ui.horizontal(|ui| {
                ui.label(RichText::new("VRAM").size(12.0).color(palette.text.color()));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format_bytes(used))
                            .size(12.0)
                            .color(palette.muted.color()),
                    );
                });
            });
        }
        _ => {}
    }
}

fn draw_percent_row(ui: &mut egui::Ui, palette: Palette, label: &str, value: Option<f32>) {
    ui.horizontal(|ui| {
        ui.set_height(24.0);
        ui.label(RichText::new(label).size(12.0).color(palette.text.color()));
        let text = value
            .map(|value| format!("{value:.1}%"))
            .unwrap_or_else(|| "sampling...".to_owned());
        let progress = value
            .map(|value| (value / 100.0).clamp(0.0, 1.0))
            .unwrap_or(0.0);
        ui.add_sized(
            [ui.available_width().max(120.0), 18.0],
            egui::ProgressBar::new(progress)
                .text(text)
                .fill(palette.accent.color()),
        );
    });
}

fn draw_bytes_row(
    ui: &mut egui::Ui,
    palette: Palette,
    label: &str,
    used: u64,
    total: u64,
    percent_value: Option<f32>,
) {
    ui.horizontal(|ui| {
        ui.set_height(24.0);
        ui.label(RichText::new(label).size(12.0).color(palette.text.color()));
        let text = if total == 0 {
            format_bytes(used)
        } else {
            format!("{} / {}", format_bytes(used), format_bytes(total))
        };
        ui.add_sized(
            [ui.available_width().max(120.0), 18.0],
            egui::ProgressBar::new(
                percent_value
                    .map(|value| (value / 100.0).clamp(0.0, 1.0))
                    .unwrap_or(0.0),
            )
            .text(text)
            .fill(palette.accent.color()),
        );
    });
}

fn draw_unavailable_row(ui: &mut egui::Ui, palette: Palette, label: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(12.0).color(palette.text.color()));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new("unavailable")
                    .size(12.0)
                    .color(palette.muted.color()),
            );
        });
    });
}

fn draw_empty_state(ui: &mut egui::Ui, app: &SymbolisApp, message: &str) {
    ui.centered_and_justified(|ui| {
        ui.add(
            egui::Label::new(
                RichText::new(message)
                    .size(16.0)
                    .color(app.settings.palette.muted.color()),
            )
            .wrap(),
        );
    });
}

fn draw_symbol_grid(ui: &mut egui::Ui, app: &mut SymbolisApp, filtered: &[usize]) {
    let chrome = chrome(app.settings.interface_mode);
    let modern = app.settings.interface_mode.is_modern();
    let available_width = ui.available_width().max(1.0) - chrome.grid_side_padding * 2.0;
    let preferred_width = match app.selected_tab {
        Tab::Category(Category::Kaomoji) => KAOMOJI_TILE_WIDTH,
        Tab::Category(Category::Emoji) | Tab::EmojiGroup(_) | Tab::Recent => EMOJI_TILE_WIDTH,
        Tab::Category(_) => SYMBOL_TILE_WIDTH,
        Tab::Settings => EMOJI_TILE_WIDTH,
    } + if modern { 6.0 } else { 0.0 };
    let columns = ((available_width + chrome.tile_gap) / (preferred_width + chrome.tile_gap))
        .floor()
        .max(1.0) as usize;
    let tile_width = ((available_width - chrome.tile_gap * (columns.saturating_sub(1) as f32))
        / columns as f32)
        .floor()
        .max(48.0);
    let rows = filtered.len().div_ceil(columns);
    let row_height = app.settings.tile_height + chrome.tile_gap;

    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show_rows(ui, row_height, rows, |ui, row_range| {
            for row_index in row_range {
                let start = row_index * columns;
                let end = (start + columns).min(filtered.len());
                ui.horizontal(|ui| {
                    ui.add_space(chrome.grid_side_padding);
                    ui.spacing_mut().item_spacing.x = chrome.tile_gap;

                    for index in &filtered[start..end] {
                        let Some(entry) = app.entry_at_active_index(*index) else {
                            continue;
                        };
                        if draw_symbol_tile(ui, app, &entry, tile_width).clicked() {
                            app.copy_entry(&entry);
                        }
                    }
                });
                ui.add_space(chrome.tile_gap);
            }
        });
}

fn draw_media_grid(ui: &mut egui::Ui, app: &mut SymbolisApp, filtered: &[MediaItemSource]) {
    let chrome = chrome(app.settings.interface_mode);
    let available_width = ui.available_width().max(1.0) - chrome.grid_side_padding * 2.0;
    let preferred_width = if app.settings.interface_mode.is_modern() {
        160.0
    } else {
        142.0
    };
    let columns = ((available_width + chrome.tile_gap) / (preferred_width + chrome.tile_gap))
        .floor()
        .max(1.0) as usize;
    let tile_width = ((available_width - chrome.tile_gap * (columns.saturating_sub(1) as f32))
        / columns as f32)
        .floor()
        .max(118.0);
    let rows = filtered.len().div_ceil(columns);
    let row_height = media_tile_height(app) + chrome.tile_gap;

    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show_rows(ui, row_height, rows, |ui, row_range| {
            for row_index in row_range {
                let start = row_index * columns;
                let end = (start + columns).min(filtered.len());
                ui.horizontal(|ui| {
                    ui.add_space(chrome.grid_side_padding);
                    ui.spacing_mut().item_spacing.x = chrome.tile_gap;

                    for source in &filtered[start..end] {
                        let Some(item) = app.media_item_from_source(*source) else {
                            continue;
                        };
                        let response = draw_media_tile(ui, app, &item, tile_width);
                        if response.clicked() {
                            let pointer_pos = response.interact_pointer_pos();
                            let bulk_selecting = ui.input(|input| input.modifiers.shift);
                            let select_clicked = pointer_pos
                                .is_some_and(|pos| media_select_rect(response.rect).contains(pos));
                            let favorite_clicked = pointer_pos.is_some_and(|pos| {
                                media_favorite_rect(response.rect).contains(pos)
                            });
                            if select_clicked || bulk_selecting {
                                app.toggle_media_selected(&item);
                            } else if favorite_clicked {
                                app.toggle_media_favorite(&item);
                            } else {
                                app.copy_media_file(&item);
                            }
                        }
                        response.context_menu(|ui| {
                            let favorite_label = if app.is_media_favorite(&item) {
                                "Remove favorite"
                            } else {
                                "Add favorite"
                            };
                            if ui.button(favorite_label).clicked() {
                                app.toggle_media_favorite(&item);
                                ui.close_menu();
                            }
                            if ui
                                .add_enabled(
                                    matches!(item.format, MediaFormat::Gif | MediaFormat::Mp4),
                                    Button::new("Save optimized WebM"),
                                )
                                .on_hover_text(
                                    "Store an optimized WebM copy in the Symbolis media directory",
                                )
                                .clicked()
                            {
                                app.save_optimized_media_copy(&item);
                                ui.close_menu();
                            }
                            if ui.button("Copy file").clicked() {
                                app.copy_media_file(&item);
                                ui.close_menu();
                            }
                            if ui.button("Copy path").clicked() {
                                app.copy_media_path(&item);
                                ui.close_menu();
                            }
                            if ui
                                .add_enabled(
                                    app.drag_out.can_drag_files(),
                                    Button::new("Open drag helper"),
                                )
                                .clicked()
                            {
                                app.drag_media_file(&item);
                                ui.close_menu();
                            }
                            if ui.button("Open location").clicked() {
                                app.open_media_location(&item);
                                ui.close_menu();
                            }
                            ui.separator();
                            let delete_label = if item.kind == MediaKind::Sticker {
                                "Delete sticker"
                            } else {
                                "Delete file"
                            };
                            let delete_hover = if item.kind == MediaKind::Sticker {
                                "Deletes this sticker file from disk"
                            } else {
                                "Deletes this media file from disk"
                            };
                            if ui
                                .button(
                                    RichText::new(delete_label)
                                        .color(app.settings.palette.danger.color()),
                                )
                                .on_hover_text(delete_hover)
                                .clicked()
                            {
                                app.delete_media_file(&item);
                                ui.close_menu();
                            }
                        });
                    }
                });
                ui.add_space(chrome.tile_gap);
            }
        });
}

fn media_tile_height(app: &SymbolisApp) -> f32 {
    if app.settings.interface_mode.is_modern() {
        118.0
    } else {
        104.0
    }
}

fn draw_media_tile(
    ui: &mut egui::Ui,
    app: &mut SymbolisApp,
    item: &MediaItem,
    width: f32,
) -> egui::Response {
    let height = media_tile_height(app);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), Sense::click());
    let palette = app.settings.palette;
    let chrome = chrome(app.settings.interface_mode);
    let modern = app.settings.interface_mode.is_modern();
    let selected = app.is_media_selected(item);
    let hover_t = ui.ctx().animate_bool(response.id, response.hovered());
    let draw_rect = rect.expand(hover_t * if modern { 2.0 } else { 3.0 });
    let base_fill = if selected {
        blend_color(palette.tile.color(), palette.accent.color(), 0.22)
    } else {
        palette.tile.color()
    };
    let fill = blend_color(base_fill, palette.tile_hover.color(), hover_t * 0.72);
    let stroke = if selected {
        Stroke::new(
            if response.hovered() { 2.0 } else { 1.5 },
            palette.accent.color(),
        )
    } else if response.hovered() {
        Stroke::new(1.0, palette.accent.color())
    } else if modern {
        Stroke::new(
            1.0,
            blend_color(palette.panel.color(), palette.tile.color(), 0.35),
        )
    } else {
        Stroke::new(1.0, palette.panel.color())
    };

    ui.painter().rect(
        draw_rect,
        Rounding::same(chrome.tile_rounding),
        fill,
        stroke,
    );
    if selected {
        ui.painter().rect_filled(
            draw_rect.shrink(4.0),
            Rounding::same((chrome.tile_rounding - 2.0).max(3.0)),
            fade_color(palette.accent.color(), 0.10),
        );
    }

    let select_rect = media_select_rect(rect);
    ui.painter().rect(
        select_rect,
        Rounding::same(4.0),
        if selected {
            palette.accent.color()
        } else {
            fade_color(palette.bg.color(), 0.72)
        },
        Stroke::new(
            1.0,
            if selected {
                palette.accent.color()
            } else {
                palette.muted.color()
            },
        ),
    );
    if selected {
        ui.painter().text(
            select_rect.center(),
            Align2::CENTER_CENTER,
            "✓",
            FontId::proportional(15.0),
            palette.bg.color(),
        );
    }

    let preview_rect = Rect::from_min_max(
        egui::pos2(draw_rect.left() + 10.0, draw_rect.top() + 10.0),
        egui::pos2(draw_rect.right() - 10.0, draw_rect.bottom() - 42.0),
    );
    ui.painter().rect(
        preview_rect,
        Rounding::same((chrome.tile_rounding - 2.0).max(3.0)),
        blend_color(palette.panel_dark.color(), palette.tile.color(), 0.28),
        Stroke::new(
            1.0,
            if selected {
                blend_color(palette.accent.color(), palette.tile.color(), 0.35)
            } else {
                blend_color(palette.panel.color(), palette.tile.color(), 0.45)
            },
        ),
    );
    if let Some(texture) = app.media_preview_cache.texture(ui.ctx(), item) {
        let image_rect = fit_centered(preview_rect.shrink(3.0), texture.size_vec2());
        let image_rect = scale_rect_centered(image_rect, 1.0 + hover_t * 0.08);
        ui.painter().with_clip_rect(preview_rect).image(
            texture.id(),
            image_rect,
            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    } else {
        ui.painter().text(
            preview_rect.center(),
            Align2::CENTER_CENTER,
            "PREVIEW",
            FontId::proportional(if modern { 16.0 } else { 14.0 }),
            palette.muted.color(),
        );
    }
    let favorite_rect = media_favorite_rect(rect);
    ui.painter().circle_filled(
        favorite_rect.center(),
        favorite_rect.width() * 0.48,
        fade_color(palette.bg.color(), 0.72),
    );
    ui.painter().text(
        favorite_rect.center(),
        Align2::CENTER_CENTER,
        if app.is_media_favorite(item) {
            "★"
        } else {
            "☆"
        },
        FontId::proportional(17.0),
        if app.is_media_favorite(item) {
            palette.accent.color()
        } else {
            palette.muted.color()
        },
    );

    let badge = format!("{} · {}", item.kind.label(), item.display_size());
    ui.painter().text(
        egui::pos2(draw_rect.left() + 10.0, draw_rect.bottom() - 31.0),
        Align2::LEFT_CENTER,
        truncate_chars(
            &item.title,
            ((width - 20.0) / 7.0).floor().max(8.0) as usize,
        ),
        FontId::proportional(if modern { 12.5 } else { 12.0 }),
        palette.text.color(),
    );
    ui.painter().text(
        egui::pos2(draw_rect.left() + 10.0, draw_rect.bottom() - 13.0),
        Align2::LEFT_CENTER,
        truncate_chars(&badge, ((width - 20.0) / 6.5).floor().max(8.0) as usize),
        FontId::proportional(11.0),
        palette.muted.color(),
    );

    let transfer_hint = if matches!(item.format, MediaFormat::Mp4 | MediaFormat::Webm) {
        "Click exports GIF for clipboard; right-click for drag."
    } else {
        "Click copies the file; right-click for drag."
    };
    response.on_hover_text(format!(
        "{}\n{}\n{}",
        item.title,
        item.path.display(),
        transfer_hint,
    ))
}

fn media_favorite_rect(rect: Rect) -> Rect {
    Rect::from_center_size(
        egui::pos2(rect.right() - 22.0, rect.top() + 22.0),
        egui::vec2(26.0, 26.0),
    )
}

fn media_select_rect(rect: Rect) -> Rect {
    Rect::from_center_size(
        egui::pos2(rect.left() + 22.0, rect.top() + 22.0),
        egui::vec2(24.0, 24.0),
    )
}

fn status_is_error(status: &str) -> bool {
    let status = status.to_lowercase();
    status.contains("error") || status.starts_with("unsupported") || status.starts_with("drop ")
}

fn draw_symbol_tile(
    ui: &mut egui::Ui,
    app: &mut SymbolisApp,
    entry: &Entry,
    width: f32,
) -> egui::Response {
    let height = app.settings.tile_height;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), Sense::click());
    let palette = app.settings.palette;
    let chrome = chrome(app.settings.interface_mode);
    let modern = app.settings.interface_mode.is_modern();
    let hover_t = ui.ctx().animate_bool(response.id, response.hovered());
    let draw_rect = rect.expand(hover_t * if modern { 2.0 } else { 3.0 });
    let fill = blend_color(palette.tile.color(), palette.tile_hover.color(), hover_t);
    let stroke = if response.hovered() {
        Stroke::new(1.0, palette.accent.color())
    } else if modern {
        Stroke::new(
            1.0,
            blend_color(palette.panel.color(), palette.tile.color(), 0.35),
        )
    } else {
        Stroke::new(1.0, palette.panel.color())
    };

    ui.painter().rect(
        draw_rect,
        Rounding::same(chrome.tile_rounding),
        fill,
        stroke,
    );

    let symbol = display_symbol(entry, width);
    let label = display_label(entry, width);
    let symbol_rect = symbol_rect(draw_rect, app.settings.interface_mode);

    if entry.category == Category::Emoji && app.settings.color_emoji {
        if let Some(texture) = app.emoji_cache.texture(ui.ctx(), &entry.ch) {
            let image_rect = fit_centered(symbol_rect, texture.size_vec2());
            ui.painter().image(
                texture.id(),
                image_rect,
                Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        } else {
            paint_symbol_text(
                ui,
                symbol_rect,
                &symbol,
                app.settings.emoji_size * (1.0 + hover_t * 0.08),
                palette.text.color(),
            );
        }
    } else {
        let symbol_size = match entry.category {
            Category::Emoji => app.settings.emoji_size,
            Category::Kaomoji => app.settings.kaomoji_size,
            _ => app.settings.symbol_size,
        };
        paint_symbol_text(
            ui,
            symbol_rect,
            &symbol,
            symbol_size * (1.0 + hover_t * 0.08),
            palette.text.color(),
        );
    }

    ui.painter().text(
        label_rect(draw_rect, app.settings.interface_mode).center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(if modern { 12.0 } else { 11.5 }),
        palette.muted.color(),
    );

    response.on_hover_text(entry.desc.clone())
}

fn paint_symbol_text(ui: &egui::Ui, rect: Rect, text: &str, size: f32, color: Color32) {
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(size),
        color,
    );
}

fn symbol_rect(rect: Rect, mode: InterfaceMode) -> Rect {
    if mode.is_modern() {
        Rect::from_min_max(
            egui::pos2(rect.left() + 8.0, rect.top() + 10.0),
            egui::pos2(rect.right() - 8.0, rect.bottom() - 29.0),
        )
    } else {
        Rect::from_min_max(
            egui::pos2(rect.left() + 6.0, rect.top() + 8.0),
            egui::pos2(rect.right() - 6.0, rect.bottom() - 27.0),
        )
    }
}

fn label_rect(rect: Rect, mode: InterfaceMode) -> Rect {
    if mode.is_modern() {
        Rect::from_min_max(
            egui::pos2(rect.left() + 8.0, rect.bottom() - 27.0),
            egui::pos2(rect.right() - 8.0, rect.bottom() - 8.0),
        )
    } else {
        Rect::from_min_max(
            egui::pos2(rect.left() + 6.0, rect.bottom() - 25.0),
            egui::pos2(rect.right() - 6.0, rect.bottom() - 7.0),
        )
    }
}

fn fit_centered(bounds: Rect, size: egui::Vec2) -> Rect {
    let max_size = bounds.size();
    let scale = (max_size.x / size.x).min(max_size.y / size.y).min(1.0);
    let size = size * scale;
    Rect::from_center_size(bounds.center(), size)
}

fn scale_rect_centered(rect: Rect, scale: f32) -> Rect {
    Rect::from_center_size(rect.center(), rect.size() * scale)
}

fn display_symbol(entry: &Entry, width: f32) -> String {
    let limit = if entry.category == Category::Kaomoji {
        ((width - 14.0) / 8.2).floor().max(4.0) as usize
    } else {
        8
    };

    truncate_chars(&entry.ch, limit)
}

fn display_label(entry: &Entry, width: f32) -> String {
    let desc = short_description(entry);
    let limit = ((width - 14.0) / 6.2).floor().max(4.0) as usize;
    truncate_chars(&desc, limit)
}

fn short_description(entry: &Entry) -> String {
    let desc = entry.desc.split(" (").next().unwrap_or(&entry.desc).trim();
    let desc = match entry.category {
        Category::Greek => strip_any_prefix(
            desc,
            &[
                "Greek Small Letter ",
                "Greek Capital Letter ",
                "Greek Letter ",
                "Greek Small ",
                "Greek Capital ",
            ],
        ),
        Category::Math => strip_any_prefix(
            desc,
            &[
                "Mathematical ",
                "Modifier Letter ",
                "Double-Struck ",
                "Greek ",
            ],
        ),
        _ => desc,
    };

    desc.to_owned()
}

fn strip_any_prefix<'a>(value: &'a str, prefixes: &[&str]) -> &'a str {
    for prefix in prefixes {
        if let Some(stripped) = value.strip_prefix(prefix) {
            return stripped;
        }
    }

    value
}

fn truncate_chars(value: &str, limit: usize) -> String {
    let mut chars = value.chars();
    let truncated: String = chars.by_ref().take(limit).collect();

    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

fn format_bytes(bytes: u64) -> String {
    const KIB: f32 = 1024.0;
    const MIB: f32 = KIB * 1024.0;
    const GIB: f32 = MIB * 1024.0;

    let bytes = bytes as f32;
    if bytes >= GIB {
        format!("{:.1} GiB", bytes / GIB)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes / MIB)
    } else if bytes >= KIB {
        format!("{:.0} KiB", bytes / KIB)
    } else {
        format!("{bytes:.0} B")
    }
}

fn format_log_time(elapsed_ms: u128) -> String {
    let total_seconds = elapsed_ms / 1000;
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    let tenths = (elapsed_ms % 1000) / 100;
    format!("{minutes:02}:{seconds:02}.{tenths}")
}

fn blend_color(from: Color32, to: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let blend = |a: u8, b: u8| -> u8 { (f32::from(a) + (f32::from(b) - f32::from(a)) * t) as u8 };

    Color32::from_rgba_unmultiplied(
        blend(from.r(), to.r()),
        blend(from.g(), to.g()),
        blend(from.b(), to.b()),
        blend(from.a(), to.a()),
    )
}

fn fade_color(color: Color32, opacity: f32) -> Color32 {
    let opacity = opacity.clamp(0.0, 1.0);
    Color32::from_rgba_unmultiplied(
        color.r(),
        color.g(),
        color.b(),
        (f32::from(color.a()) * opacity) as u8,
    )
}

fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

fn settings_panel(
    ui: &mut egui::Ui,
    label: &str,
    palette: crate::settings::Palette,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    Frame::none()
        .fill(blend_color(palette.panel.color(), palette.bg.color(), 0.18))
        .stroke(Stroke::new(
            1.0,
            blend_color(palette.tile.color(), palette.panel.color(), 0.35),
        ))
        .rounding(Rounding::same(8.0))
        .inner_margin(egui::Margin::symmetric(14.0, 12.0))
        .show(ui, |ui| {
            ui.label(
                RichText::new(label)
                    .size(15.0)
                    .strong()
                    .color(palette.text.color()),
            );
            ui.add_space(10.0);
            add_contents(ui);
        });
}

fn color_row(ui: &mut egui::Ui, label: &str, rgb: &mut Rgb) -> bool {
    ui.label(label);
    let mut color = rgb.color();
    let changed =
        egui::color_picker::color_edit_button_srgba(ui, &mut color, Alpha::Opaque).changed();
    if changed {
        rgb.set_color(color);
    }
    changed
}
