use eframe::egui::{
    self, Align, Align2, Button, Color32, Context, FontId, Frame, Key, Layout, Rect, RichText,
    Rounding, ScrollArea, Sense, Stroke, TextEdit, TopBottomPanel, color_picker::Alpha,
    containers::scroll_area::ScrollBarVisibility,
};

use crate::{
    app::{ContentMode, MediaView, SymbolisApp, Tab, has_hovered_files, hovered_media_drop_count},
    data::{Category, DataSource, EmojiGroup, Entry},
    gif_provider::{GifProvider, ProviderStatus},
    media_drag::DragOutBackend,
    media_library::{MediaFormat, MediaItem},
    settings::{InterfaceMode, Preset, Rgb, ThemeSelection},
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
        let count = if self.selected_tab == Tab::Settings {
            0
        } else {
            match self.content_mode {
                ContentMode::Symbols => self.filtered_entries().len(),
                ContentMode::Gifs => self.filtered_media_items().len(),
            }
        };

        self.draw_sidebar(ctx);
        self.draw_topbar(ctx);
        self.draw_footer(ctx, count);

        egui::CentralPanel::default()
            .frame(Frame::none().fill(self.settings.palette.bg.color()))
            .show(ctx, |ui| {
                if self.selected_tab == Tab::Settings {
                    self.draw_settings(ui, ctx);
                    return;
                }

                match self.content_mode {
                    ContentMode::Symbols => {
                        let filtered = self.filtered_entries();
                        if filtered.is_empty() {
                            draw_empty_state(ui, self, "No matches");
                            return;
                        }

                        ui.add_space(chrome.content_top_space);
                        draw_symbol_grid(ui, self, &filtered);
                    }
                    ContentMode::Gifs => {
                        let filtered = self.filtered_media_items();
                        if filtered.is_empty() {
                            let message = match self.media_view {
                                MediaView::Library if self.media_items.is_empty() => {
                                    "Drop GIFs or WebM here"
                                }
                                MediaView::Library => "No media matches",
                                MediaView::Favorites => "No favorites yet",
                                MediaView::RecentlyUsed => "No recently used GIFs yet",
                            };
                            draw_empty_state(ui, self, message);
                            return;
                        }

                        ui.add_space(chrome.content_top_space);
                        draw_media_grid(ui, self, &filtered);
                    }
                }
            });
        self.draw_drop_overlay(ctx);
    }

    fn draw_sidebar(&mut self, ctx: &Context) {
        if self.content_mode == ContentMode::Gifs && self.selected_tab != Tab::Settings {
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
                        self.media_sidebar_button(ui, MediaView::Library, "GIF", true);
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
            self.media_view == view,
            enabled,
            false,
            false,
            1.0,
        );

        if response.clicked() && enabled {
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
            self.selected_tab = group.default_tab();
        }
    }

    fn active_sidebar_group(&self) -> Option<SidebarGroup> {
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
        let selected = self.selected_tab == tab;
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
            self.selected_tab = tab;
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

        TopBottomPanel::top("top_bar")
            .exact_height(chrome.topbar_height)
            .frame(Frame::none().fill(if modern {
                self.settings.palette.bg.color()
            } else {
                self.settings.palette.panel.color()
            }))
            .show(ctx, |ui| {
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

                        if self.content_mode == ContentMode::Symbols
                            && self.selected_tab == Tab::Recent
                            && !self.recent.is_empty()
                        {
                            if ui
                                .button(
                                    RichText::new("Clear")
                                        .color(self.settings.palette.text.color()),
                                )
                                .on_hover_text("Clear recent symbols")
                                .clicked()
                            {
                                self.clear_recent();
                            }
                            ui.add_space(if modern { 12.0 } else { 10.0 });
                        }

                        if self.content_mode == ContentMode::Gifs
                            && self.media_view == MediaView::RecentlyUsed
                            && !self.recent_media.is_empty()
                        {
                            if ui
                                .button(
                                    RichText::new("Clear")
                                        .color(self.settings.palette.text.color()),
                                )
                                .on_hover_text("Clear recent GIFs")
                                .clicked()
                            {
                                self.clear_recent_media();
                            }
                            ui.add_space(if modern { 12.0 } else { 10.0 });
                        }

                        if self.selected_tab != Tab::Settings {
                            let width = ui
                                .available_width()
                                .min(if modern { 300.0 } else { 260.0 })
                                .max(150.0);
                            let hint = match self.content_mode {
                                ContentMode::Symbols => "Search symbols...",
                                ContentMode::Gifs => "Search local GIFs...",
                            };
                            let query = match self.content_mode {
                                ContentMode::Symbols => &mut self.query,
                                ContentMode::Gifs => &mut self.gif_query,
                            };
                            let response = ui.add_sized(
                                [width, if modern { 36.0 } else { 32.0 }],
                                TextEdit::singleline(query).hint_text(hint),
                            );
                            response.request_focus();
                        }
                    });
                });
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

    fn draw_mode_switch(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for mode in ContentMode::CHOICES {
                let selected = self.content_mode == mode && self.selected_tab != Tab::Settings;
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
                    self.content_mode = mode;
                    if self.selected_tab == Tab::Settings {
                        self.selected_tab = Tab::Category(Category::Emoji);
                    }
                }
            }
        });
    }

    fn topbar_title(&self) -> &'static str {
        if self.selected_tab == Tab::Settings {
            return self.selected_tab.label();
        }

        match self.content_mode {
            ContentMode::Symbols => self.selected_tab.label(),
            ContentMode::Gifs => self.media_view.label(),
        }
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
                "Drop GIF, PNG, WebP, WebM, or a folder"
            } else {
                "Drop to add to GIF library"
            },
            FontId::proportional(20.0),
            self.settings.palette.text.color(),
        );
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
                        ContentMode::Gifs => {
                            ui.label(
                                RichText::new(format!(
                                    "local library: {} files",
                                    self.media_items.len()
                                ))
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
                                let response = ui.add_sized(
                                    [ui.available_width().min(420.0), 28.0],
                                    TextEdit::singleline(&mut self.gif_import_path_input)
                                        .hint_text("/path/to/folder or /path/to/file.webm"),
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
                                    "Indexed {} local media files. Drop GIF/WebM files to store them locally, or add folders as referenced libraries.",
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

        if changed {
            if theme_changed {
                self.settings.ensure_editable_theme();
            }
            crate::settings::configure_style(ctx, &self.settings);
            self.save_settings();
        }
    }
}

fn draw_empty_state(ui: &mut egui::Ui, app: &SymbolisApp, message: &str) {
    ui.centered_and_justified(|ui| {
        ui.label(
            RichText::new(message)
                .size(16.0)
                .color(app.settings.palette.muted.color()),
        );
    });
}

fn draw_symbol_grid(ui: &mut egui::Ui, app: &mut SymbolisApp, filtered: &[Entry]) {
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

                    for entry in &filtered[start..end] {
                        if draw_symbol_tile(ui, app, entry, tile_width).clicked() {
                            app.copy_entry(entry);
                        }
                    }
                });
                ui.add_space(chrome.tile_gap);
            }
        });
}

fn draw_media_grid(ui: &mut egui::Ui, app: &mut SymbolisApp, filtered: &[MediaItem]) {
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

                    for item in &filtered[start..end] {
                        let response = draw_media_tile(ui, app, item, tile_width);
                        if response.clicked() {
                            let favorite_clicked =
                                response.interact_pointer_pos().is_some_and(|pos| {
                                    media_favorite_rect(response.rect).contains(pos)
                                });
                            if favorite_clicked {
                                app.toggle_media_favorite(item);
                            } else {
                                app.copy_media_file(item);
                            }
                        }
                        response.context_menu(|ui| {
                            let favorite_label = if app.is_media_favorite(item) {
                                "Remove favorite"
                            } else {
                                "Add favorite"
                            };
                            if ui.button(favorite_label).clicked() {
                                app.toggle_media_favorite(item);
                                ui.close_menu();
                            }
                            if ui
                                .add_enabled(
                                    item.format == MediaFormat::Gif,
                                    Button::new("Save optimized WebM"),
                                )
                                .on_hover_text(
                                    "Store an optimized WebM copy in the Symbolis media directory",
                                )
                                .clicked()
                            {
                                app.save_optimized_media_copy(item);
                                ui.close_menu();
                            }
                            if ui.button("Copy file").clicked() {
                                app.copy_media_file(item);
                                ui.close_menu();
                            }
                            if ui.button("Copy path").clicked() {
                                app.copy_media_path(item);
                                ui.close_menu();
                            }
                            if ui
                                .add_enabled(
                                    app.drag_out.can_drag_files(),
                                    Button::new("Open drag helper"),
                                )
                                .clicked()
                            {
                                app.drag_media_file(item);
                                ui.close_menu();
                            }
                            if ui.button("Open location").clicked() {
                                app.open_media_location(item);
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
            blend_color(palette.panel.color(), palette.tile.color(), 0.45),
        ),
    );
    if let Some(texture) = app.media_preview_cache.texture(ui.ctx(), item) {
        let image_rect = fit_centered(preview_rect.shrink(3.0), texture.size_vec2());
        ui.painter().image(
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
    paint_media_format_badge(ui, preview_rect, item.format.label(), palette.bg.color());

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

    let transfer_hint = if item.format == MediaFormat::Webm {
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

fn paint_media_format_badge(ui: &egui::Ui, rect: Rect, label: &str, bg: Color32) {
    let badge_rect = Rect::from_min_size(
        egui::pos2(rect.left() + 7.0, rect.top() + 7.0),
        egui::vec2(42.0, 18.0),
    );
    ui.painter().rect(
        badge_rect,
        Rounding::same(4.0),
        fade_color(bg, 0.74),
        Stroke::new(1.0, fade_color(Color32::WHITE, 0.08)),
    );
    ui.painter().text(
        badge_rect.center(),
        Align2::CENTER_CENTER,
        label.to_ascii_uppercase(),
        FontId::proportional(10.5),
        Color32::WHITE,
    );
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
