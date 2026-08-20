use std::{
    fs, io,
    path::{Path, PathBuf},
};

use eframe::egui::{
    self, Color32, Context, FontData, FontDefinitions, FontFamily, Rounding, Stroke,
};
use serde::{Deserialize, Serialize};

use crate::gif_provider::GifProvider;
use crate::persistence::write_json_atomic;

pub(crate) const MEDIA_PREVIEW_FRAMERATE_MIN_FPS: u32 = 1;
pub(crate) const MEDIA_PREVIEW_FRAMERATE_MAX_FPS: u32 = 24;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FontLoadReport {
    pub(crate) built_in_fonts: usize,
    pub(crate) system_fonts: Vec<String>,
    pub(crate) proportional_family_fonts: usize,
    pub(crate) monospace_family_fonts: usize,
}

impl FontLoadReport {
    pub(crate) fn label(&self) -> String {
        format!(
            "fonts: built-in={}, system=[{}], proportional={}, monospace={}",
            self.built_in_fonts,
            if self.system_fonts.is_empty() {
                "none".to_owned()
            } else {
                self.system_fonts.join(", ")
            },
            self.proportional_family_fonts,
            self.monospace_family_fonts
        )
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) enum Preset {
    ModernDark,
    DefaultGray,
    CatppuccinLatte,
    CatppuccinFrappe,
    CatppuccinMacchiato,
    CatppuccinMocha,
    NuarContrastBlack,
    NuarContrastWhite,
    #[serde(rename = "Custom")]
    LegacyCustom,
}

impl Preset {
    pub(crate) const CHOICES: [Preset; 8] = [
        Preset::ModernDark,
        Preset::DefaultGray,
        Preset::CatppuccinLatte,
        Preset::CatppuccinFrappe,
        Preset::CatppuccinMacchiato,
        Preset::CatppuccinMocha,
        Preset::NuarContrastBlack,
        Preset::NuarContrastWhite,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Preset::ModernDark => "Obsidian",
            Preset::DefaultGray => "Graphite",
            Preset::CatppuccinLatte => "Catppuccin Latte",
            Preset::CatppuccinFrappe => "Catppuccin Frappe",
            Preset::CatppuccinMacchiato => "Catppuccin Macchiato",
            Preset::CatppuccinMocha => "Catppuccin Mocha",
            Preset::NuarContrastBlack => "High Contrast Dark",
            Preset::NuarContrastWhite => "High Contrast Light",
            Preset::LegacyCustom => "Custom",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "value")]
pub(crate) enum ThemeSelection {
    Preset(Preset),
    Custom(String),
}

impl Default for ThemeSelection {
    fn default() -> Self {
        Self::Preset(Preset::ModernDark)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct CustomTheme {
    pub(crate) name: String,
    pub(crate) palette: Palette,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct FeatureSettings {
    pub(crate) symbols: bool,
    pub(crate) stickers: bool,
    pub(crate) gifs: bool,
    pub(crate) media_watcher: bool,
    pub(crate) deduplicate_media: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub(crate) enum HotkeyAction {
    Main,
    Symbols,
    Stickers,
    Gifs,
}

impl HotkeyAction {
    pub(crate) const CHOICES: [HotkeyAction; 1] = [HotkeyAction::Main];

    pub(crate) fn label(self) -> &'static str {
        match self {
            HotkeyAction::Main => "Toggle window",
            HotkeyAction::Symbols => "Symbols",
            HotkeyAction::Stickers => "Stickers",
            HotkeyAction::Gifs => "GIFs",
        }
    }

    pub(crate) fn id(self) -> &'static str {
        match self {
            HotkeyAction::Main => "main",
            HotkeyAction::Symbols => "symbols",
            HotkeyAction::Stickers => "stickers",
            HotkeyAction::Gifs => "gifs",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct HotkeyBinding {
    #[serde(default)]
    pub(crate) shift: bool,
    #[serde(default)]
    pub(crate) control: bool,
    #[serde(default)]
    pub(crate) alt: bool,
    #[serde(default)]
    pub(crate) super_key: bool,
    pub(crate) key: String,
}

impl HotkeyBinding {
    pub(crate) fn new(key: impl Into<String>) -> Self {
        Self {
            shift: false,
            control: false,
            alt: false,
            super_key: false,
            key: key.into(),
        }
    }

    pub(crate) fn canonical(&self) -> String {
        let mut parts = Vec::new();
        if self.shift {
            parts.push("shift");
        }
        if self.control {
            parts.push("control");
        }
        if self.alt {
            parts.push("alt");
        }
        if self.super_key {
            parts.push("super");
        }
        parts.push(self.key.as_str());
        parts.join("+")
    }

    pub(crate) fn label(&self) -> String {
        let mut parts = Vec::new();
        if self.shift {
            parts.push("Shift".to_owned());
        }
        if self.control {
            parts.push("Ctrl".to_owned());
        }
        if self.alt {
            parts.push("Alt".to_owned());
        }
        if self.super_key {
            parts.push("Super".to_owned());
        }
        parts.push(hotkey_key_label(&self.key).to_owned());
        parts.join("+")
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct HotkeySettings {
    pub(crate) enabled: bool,
    #[serde(default)]
    pub(crate) main: Option<HotkeyBinding>,
    #[serde(default)]
    pub(crate) symbols: Option<HotkeyBinding>,
    #[serde(default)]
    pub(crate) stickers: Option<HotkeyBinding>,
    #[serde(default)]
    pub(crate) gifs: Option<HotkeyBinding>,
}

impl Default for HotkeySettings {
    fn default() -> Self {
        let mut main = HotkeyBinding::new("Period");
        main.super_key = true;
        Self {
            enabled: false,
            main: Some(main),
            symbols: None,
            stickers: None,
            gifs: None,
        }
    }
}

impl HotkeySettings {
    pub(crate) fn binding(&self, action: HotkeyAction) -> Option<&HotkeyBinding> {
        match action {
            HotkeyAction::Main => self.main.as_ref(),
            HotkeyAction::Symbols => self.symbols.as_ref(),
            HotkeyAction::Stickers => self.stickers.as_ref(),
            HotkeyAction::Gifs => self.gifs.as_ref(),
        }
    }

    pub(crate) fn binding_mut(&mut self, action: HotkeyAction) -> &mut Option<HotkeyBinding> {
        match action {
            HotkeyAction::Main => &mut self.main,
            HotkeyAction::Symbols => &mut self.symbols,
            HotkeyAction::Stickers => &mut self.stickers,
            HotkeyAction::Gifs => &mut self.gifs,
        }
    }
}

impl Default for FeatureSettings {
    fn default() -> Self {
        Self {
            symbols: true,
            stickers: true,
            gifs: true,
            media_watcher: true,
            deduplicate_media: true,
        }
    }
}

impl FeatureSettings {
    pub(crate) fn enabled_content_count(&self) -> usize {
        usize::from(self.symbols) + usize::from(self.stickers) + usize::from(self.gifs)
    }

    pub(crate) fn media_enabled(&self) -> bool {
        self.stickers || self.gifs
    }

    pub(crate) fn ensure_any_content_enabled(&mut self) {
        if self.enabled_content_count() == 0 {
            self.gifs = true;
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) enum InterfaceMode {
    #[default]
    Modern,
    RawDev,
}

impl InterfaceMode {
    pub(crate) const CHOICES: [InterfaceMode; 2] = [InterfaceMode::Modern, InterfaceMode::RawDev];

    pub(crate) fn label(self) -> &'static str {
        match self {
            InterfaceMode::Modern => "Comfort",
            InterfaceMode::RawDev => "Compact",
        }
    }

    pub(crate) fn description(self) -> &'static str {
        match self {
            InterfaceMode::Modern => "Roomier layout with clearer grouping and softer surfaces",
            InterfaceMode::RawDev => "Denser layout for fast keyboard-and-mouse scanning",
        }
    }

    pub(crate) fn is_modern(self) -> bool {
        self == InterfaceMode::Modern
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct Rgb([u8; 3]);

impl Rgb {
    const fn new(r: u8, g: u8, b: u8) -> Self {
        Self([r, g, b])
    }

    pub(crate) fn color(self) -> Color32 {
        Color32::from_rgb(self.0[0], self.0[1], self.0[2])
    }

    pub(crate) fn set_color(&mut self, color: Color32) {
        self.0 = [color.r(), color.g(), color.b()];
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct Palette {
    pub(crate) bg: Rgb,
    pub(crate) panel: Rgb,
    pub(crate) panel_dark: Rgb,
    pub(crate) tile: Rgb,
    pub(crate) tile_hover: Rgb,
    pub(crate) accent: Rgb,
    pub(crate) text: Rgb,
    pub(crate) muted: Rgb,
    pub(crate) danger: Rgb,
}

impl Palette {
    fn sanitize_readability(&mut self, fallback: Palette) {
        let surfaces = [self.bg, self.panel, self.tile];
        self.text = readable_color(self.text, fallback.text, surfaces, 4.5);
        self.muted = readable_color(self.muted, fallback.muted, surfaces, 3.0);
        self.danger = readable_color(self.danger, fallback.danger, surfaces, 3.0);
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub(crate) struct MediaHoverPreviewSettings {
    #[serde(default = "default_media_hover_preview_enabled")]
    pub(crate) enabled: bool,
    #[serde(default = "default_media_hover_preview_play_animated")]
    pub(crate) play_animated: bool,
    #[serde(default = "default_media_hover_preview_framerate_fps")]
    pub(crate) framerate_fps: u32,
    #[serde(default = "default_media_hover_preview_scale")]
    pub(crate) scale: f32,
    #[serde(default = "default_media_hover_preview_speed")]
    pub(crate) speed: f32,
    #[serde(default = "default_media_hover_preview_delay_ms")]
    pub(crate) delay_ms: f32,
}

impl Default for MediaHoverPreviewSettings {
    fn default() -> Self {
        Self {
            enabled: default_media_hover_preview_enabled(),
            play_animated: default_media_hover_preview_play_animated(),
            framerate_fps: default_media_hover_preview_framerate_fps(),
            scale: default_media_hover_preview_scale(),
            speed: default_media_hover_preview_speed(),
            delay_ms: default_media_hover_preview_delay_ms(),
        }
    }
}

impl MediaHoverPreviewSettings {
    pub(crate) fn sanitize(&mut self) {
        self.delay_ms = self.delay_ms.clamp(0.0, 1500.0);
        self.scale = self.scale.clamp(1.15, 2.8);
        self.speed = self.speed.clamp(0.03, 0.35);
        self.framerate_fps = self.framerate_fps.clamp(
            MEDIA_PREVIEW_FRAMERATE_MIN_FPS,
            MEDIA_PREVIEW_FRAMERATE_MAX_FPS,
        );
    }

    pub(crate) fn normalized_framerate_fps(self) -> u32 {
        self.framerate_fps.clamp(
            MEDIA_PREVIEW_FRAMERATE_MIN_FPS,
            MEDIA_PREVIEW_FRAMERATE_MAX_FPS,
        )
    }
}

fn default_media_hover_preview_enabled() -> bool {
    true
}

fn default_media_hover_preview_play_animated() -> bool {
    true
}

fn default_media_hover_preview_framerate_fps() -> u32 {
    6
}

fn default_media_hover_preview_scale() -> f32 {
    1.75
}

fn default_media_hover_preview_speed() -> f32 {
    0.12
}

fn default_media_hover_preview_delay_ms() -> f32 {
    500.0
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct UiSettings {
    #[serde(default)]
    pub(crate) interface_mode: InterfaceMode,
    #[serde(default)]
    pub(crate) features: FeatureSettings,
    #[serde(default)]
    pub(crate) hotkeys: HotkeySettings,
    #[serde(default)]
    pub(crate) gif_provider: GifProvider,
    #[serde(default)]
    pub(crate) gif_import_paths: Vec<PathBuf>,
    #[serde(default)]
    pub(crate) theme: ThemeSelection,
    #[serde(default)]
    pub(crate) custom_themes: Vec<CustomTheme>,
    #[serde(default)]
    pub(crate) low_memory_mode: bool,
    #[serde(default)]
    pub(crate) media_hover_preview: MediaHoverPreviewSettings,
    pub(crate) preset: Preset,
    pub(crate) palette: Palette,
    pub(crate) color_emoji: bool,
    pub(crate) tile_height: f32,
    pub(crate) emoji_size: f32,
    pub(crate) symbol_size: f32,
    pub(crate) kaomoji_size: f32,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self::from_preset(Preset::ModernDark)
    }
}

impl UiSettings {
    pub(crate) fn from_preset(preset: Preset) -> Self {
        Self {
            interface_mode: InterfaceMode::Modern,
            features: FeatureSettings::default(),
            hotkeys: HotkeySettings::default(),
            gif_provider: GifProvider::Local,
            gif_import_paths: Vec::new(),
            theme: ThemeSelection::Preset(preset),
            custom_themes: Vec::new(),
            low_memory_mode: false,
            media_hover_preview: MediaHoverPreviewSettings::default(),
            preset,
            palette: palette_for(preset),
            color_emoji: true,
            tile_height: 78.0,
            emoji_size: 32.0,
            symbol_size: 32.0,
            kaomoji_size: 16.0,
        }
    }

    pub(crate) fn apply_preset(&mut self, preset: Preset) {
        let previous = self.clone();
        *self = UiSettings::from_preset(preset);
        self.interface_mode = previous.interface_mode;
        self.features = previous.features;
        self.hotkeys = previous.hotkeys;
        self.gif_provider = previous.gif_provider;
        self.gif_import_paths = previous.gif_import_paths;
        self.custom_themes = previous.custom_themes;
        self.low_memory_mode = previous.low_memory_mode;
        self.media_hover_preview = previous.media_hover_preview;
        self.theme = ThemeSelection::Preset(preset);
        self.color_emoji = previous.color_emoji;
        self.tile_height = previous.tile_height;
        self.emoji_size = previous.emoji_size;
        self.symbol_size = previous.symbol_size;
        self.kaomoji_size = previous.kaomoji_size;
    }

    pub(crate) fn apply_interface_mode(&mut self, mode: InterfaceMode) {
        if self.interface_mode == mode {
            return;
        }

        self.interface_mode = mode;

        match mode {
            InterfaceMode::Modern => {
                self.tile_height = self.tile_height.max(78.0);
                self.emoji_size = self.emoji_size.max(32.0);
                self.symbol_size = self.symbol_size.max(32.0);
            }
            InterfaceMode::RawDev => {
                self.tile_height = 74.0;
                self.emoji_size = 30.0;
                self.symbol_size = 31.0;
                self.kaomoji_size = 16.0;
            }
        }
    }

    pub(crate) fn apply_custom_theme(&mut self, name: &str) {
        let Some(theme) = self.custom_themes.iter().find(|theme| theme.name == name) else {
            return;
        };

        self.theme = ThemeSelection::Custom(theme.name.clone());
        self.palette = theme.palette;
    }

    pub(crate) fn ensure_editable_theme(&mut self) {
        match &self.theme {
            ThemeSelection::Preset(_) => {
                let name = self.next_custom_theme_name();
                self.custom_themes.push(CustomTheme {
                    name: name.clone(),
                    palette: self.palette,
                });
                self.theme = ThemeSelection::Custom(name);
            }
            ThemeSelection::Custom(_) => {
                self.sync_selected_custom_theme();
            }
        }
    }

    pub(crate) fn sync_selected_custom_theme(&mut self) {
        let ThemeSelection::Custom(name) = &self.theme else {
            return;
        };

        if let Some(theme) = self
            .custom_themes
            .iter_mut()
            .find(|theme| theme.name == *name)
        {
            theme.palette = self.palette;
        }
    }

    pub(crate) fn rename_selected_custom_theme(&mut self, new_name: String) {
        let new_name = sanitize_theme_name(&new_name);
        if new_name.is_empty() {
            return;
        }

        let ThemeSelection::Custom(current_name) = &self.theme else {
            return;
        };

        if self
            .custom_themes
            .iter()
            .any(|theme| theme.name == new_name && theme.name != *current_name)
        {
            return;
        }

        if let Some(theme) = self
            .custom_themes
            .iter_mut()
            .find(|theme| theme.name == *current_name)
        {
            theme.name = new_name.clone();
            self.theme = ThemeSelection::Custom(new_name);
        }
    }

    pub(crate) fn delete_selected_custom_theme(&mut self) -> bool {
        let ThemeSelection::Custom(current_name) = self.theme.clone() else {
            return false;
        };

        let Some(index) = self
            .custom_themes
            .iter()
            .position(|theme| theme.name == current_name)
        else {
            self.apply_preset(self.fallback_preset());
            return false;
        };

        self.custom_themes.remove(index);

        if let Some(theme) = self.custom_themes.get(index).or_else(|| {
            index
                .checked_sub(1)
                .and_then(|index| self.custom_themes.get(index))
        }) {
            self.theme = ThemeSelection::Custom(theme.name.clone());
            self.palette = theme.palette;
        } else {
            self.apply_preset(self.fallback_preset());
        }

        true
    }

    fn sanitize_palette_readability(&mut self) {
        let fallback = palette_for(self.fallback_preset());
        self.palette.sanitize_readability(fallback);
        for theme in &mut self.custom_themes {
            theme.palette.sanitize_readability(fallback);
        }
    }

    pub(crate) fn selected_custom_theme_name(&self) -> Option<&str> {
        match &self.theme {
            ThemeSelection::Custom(name) => Some(name.as_str()),
            ThemeSelection::Preset(_) => None,
        }
    }

    fn fallback_preset(&self) -> Preset {
        if self.preset == Preset::LegacyCustom {
            Preset::ModernDark
        } else {
            self.preset
        }
    }

    fn next_custom_theme_name(&self) -> String {
        for index in 1.. {
            let name = format!("Custom {index}");
            if self.custom_themes.iter().all(|theme| theme.name != name) {
                return name;
            }
        }

        unreachable!()
    }
}

pub(crate) fn settings_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("symbolis").join("settings.json"))
}

pub(crate) fn load_settings(path: &Path) -> Option<UiSettings> {
    let content = fs::read_to_string(path).ok()?;
    let mut settings: UiSettings = serde_json::from_str(&content).ok()?;
    settings.features.ensure_any_content_enabled();

    if !content.contains("\"interface_mode\"") && settings.preset == Preset::DefaultGray {
        let color_emoji = settings.color_emoji;
        settings = UiSettings::default();
        settings.color_emoji = color_emoji;
    }

    if settings.preset == Preset::LegacyCustom {
        let name = settings.next_custom_theme_name();
        settings.custom_themes.push(CustomTheme {
            name: name.clone(),
            palette: settings.palette,
        });
        settings.theme = ThemeSelection::Custom(name);
        settings.preset = Preset::ModernDark;
    } else if !content.contains("\"theme\"") {
        settings.theme = ThemeSelection::Preset(settings.preset);
    }
    settings.sanitize_palette_readability();
    settings.media_hover_preview.sanitize();

    Some(settings)
}

pub(crate) fn save_settings(path: Option<&Path>, settings: &UiSettings) -> io::Result<()> {
    write_json_atomic(path, settings)
}

pub(crate) fn configure_style(ctx: &Context, settings: &UiSettings) {
    let colors = settings.palette;
    let modern = settings.interface_mode.is_modern();
    let rounding = Rounding::same(if modern { 8.0 } else { 6.0 });
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(if modern { 10.0 } else { 8.0 }, 8.0);
    style.spacing.button_padding = egui::vec2(if modern { 12.0 } else { 10.0 }, 6.0);
    style.visuals.dark_mode = is_dark(colors.bg.color());
    style.visuals.window_fill = colors.bg.color();
    style.visuals.panel_fill = colors.bg.color();
    style.visuals.widgets.noninteractive.bg_fill = colors.panel.color();
    style.visuals.widgets.inactive.bg_fill = colors.tile.color();
    style.visuals.widgets.hovered.bg_fill = colors.tile_hover.color();
    style.visuals.widgets.active.bg_fill = colors.accent.color();
    style.visuals.selection.bg_fill = colors.accent.color();
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, colors.text.color());
    style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, colors.text.color());
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, colors.text.color());
    style.visuals.widgets.noninteractive.rounding = rounding;
    style.visuals.widgets.inactive.rounding = rounding;
    style.visuals.widgets.hovered.rounding = rounding;
    style.visuals.widgets.active.rounding = rounding;
    style.visuals.window_rounding = rounding;
    ctx.set_style(style);
}

fn is_dark(color: Color32) -> bool {
    let luminance = 0.2126 * f32::from(color.r())
        + 0.7152 * f32::from(color.g())
        + 0.0722 * f32::from(color.b());
    luminance < 128.0
}

fn color_luminance(color: Rgb) -> f32 {
    0.2126 * f32::from(color.0[0]) + 0.7152 * f32::from(color.0[1]) + 0.0722 * f32::from(color.0[2])
}

fn contrast_ratio(a: Rgb, b: Rgb) -> f32 {
    fn channel(value: u8) -> f32 {
        let value = f32::from(value) / 255.0;
        if value <= 0.03928 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    }

    fn relative_luminance(color: Rgb) -> f32 {
        0.2126 * channel(color.0[0]) + 0.7152 * channel(color.0[1]) + 0.0722 * channel(color.0[2])
    }

    let a = relative_luminance(a);
    let b = relative_luminance(b);
    let lighter = a.max(b);
    let darker = a.min(b);
    (lighter + 0.05) / (darker + 0.05)
}

fn readable_color(color: Rgb, fallback: Rgb, surfaces: [Rgb; 3], min_contrast: f32) -> Rgb {
    if color_is_readable(color, surfaces, min_contrast) {
        return color;
    }
    if color_is_readable(fallback, surfaces, min_contrast) {
        return fallback;
    }

    let average_luminance = surfaces
        .iter()
        .map(|color| color_luminance(*color))
        .sum::<f32>()
        / surfaces.len() as f32;
    let high_contrast = if average_luminance < 128.0 {
        Rgb::new(245, 248, 252)
    } else {
        Rgb::new(18, 20, 24)
    };
    if color_is_readable(high_contrast, surfaces, min_contrast) {
        high_contrast
    } else if average_luminance < 128.0 {
        Rgb::new(255, 255, 255)
    } else {
        Rgb::new(0, 0, 0)
    }
}

fn color_is_readable(color: Rgb, surfaces: [Rgb; 3], min_contrast: f32) -> bool {
    surfaces
        .into_iter()
        .all(|surface| contrast_ratio(color, surface) >= min_contrast)
}

fn sanitize_theme_name(value: &str) -> String {
    value.trim().chars().take(48).collect()
}

pub(crate) fn hotkey_key_label(key: &str) -> &str {
    match key {
        "Period" => ".",
        "Comma" => ",",
        "Slash" => "/",
        "Backslash" => "\\",
        "Minus" => "-",
        "Equal" => "=",
        "Semicolon" => ";",
        "Quote" => "'",
        "Backquote" => "`",
        "Space" => "Space",
        "Enter" => "Enter",
        "Tab" => "Tab",
        "Escape" => "Esc",
        "ArrowUp" => "Up",
        "ArrowDown" => "Down",
        "ArrowLeft" => "Left",
        "ArrowRight" => "Right",
        "BracketLeft" => "[",
        "BracketRight" => "]",
        value if value.starts_with("Key") && value.len() == 4 => &value[3..],
        value if value.starts_with("Digit") && value.len() == 6 => &value[5..],
        value => value,
    }
}

pub(crate) fn configure_fonts(ctx: &Context, settings: &UiSettings) -> FontLoadReport {
    let mut fonts = FontDefinitions::default();
    let built_in_fonts = fonts.font_data.len();

    let mut font_paths = Vec::new();
    if let Some(base_font) = first_existing_font(&[
        ("NotoSans", "/usr/share/fonts/noto/NotoSans-Regular.ttf"),
        ("DejaVuSans", "/usr/share/fonts/TTF/DejaVuSans.ttf"),
        (
            "DejaVuSansDebian",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        ),
        (
            "LiberationSans",
            "/usr/share/fonts/truetype/liberation2/LiberationSans-Regular.ttf",
        ),
    ]) {
        font_paths.push(base_font);
    }

    font_paths.extend([
        (
            "NotoSansSymbols2",
            "/usr/share/fonts/noto/NotoSansSymbols2-Regular.ttf",
        ),
        (
            "NotoSansSymbols",
            "/usr/share/fonts/noto/NotoSansSymbols-Regular.ttf",
        ),
    ]);

    if settings.features.symbols {
        font_paths.extend([
            (
                "NotoSansMath",
                "/usr/share/fonts/noto/NotoSansMath-Regular.ttf",
            ),
            (
                "NotoSansArabic",
                "/usr/share/fonts/noto/NotoSansArabic-Regular.ttf",
            ),
            (
                "NotoSansHebrew",
                "/usr/share/fonts/noto/NotoSansHebrew-Regular.ttf",
            ),
        ]);

        if !settings.low_memory_mode {
            font_paths.push((
                "NotoSansCJK",
                "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
            ));
            font_paths.push((
                "NotoSansCJKDebian",
                "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            ));
        }
    }

    let mut system_fonts = Vec::new();
    for (name, path) in font_paths {
        let Ok(bytes) = fs::read(path) else {
            continue;
        };

        system_fonts.push(name.to_owned());
        fonts
            .font_data
            .insert(name.to_owned(), FontData::from_owned(bytes));

        if let Some(family) = fonts.families.get_mut(&FontFamily::Proportional) {
            family.push(name.to_owned());
        }
        if let Some(family) = fonts.families.get_mut(&FontFamily::Monospace) {
            family.push(name.to_owned());
        }
    }

    let proportional_family_fonts = fonts
        .families
        .get(&FontFamily::Proportional)
        .map_or(0, Vec::len);
    let monospace_family_fonts = fonts
        .families
        .get(&FontFamily::Monospace)
        .map_or(0, Vec::len);
    let report = FontLoadReport {
        built_in_fonts,
        system_fonts,
        proportional_family_fonts,
        monospace_family_fonts,
    };
    ctx.set_fonts(fonts);
    report
}

fn first_existing_font(
    fonts: &[(&'static str, &'static str)],
) -> Option<(&'static str, &'static str)> {
    fonts
        .iter()
        .copied()
        .find(|(_, path)| Path::new(path).is_file())
}

fn palette_for(preset: Preset) -> Palette {
    match preset {
        Preset::ModernDark => Palette {
            bg: Rgb::new(13, 15, 20),
            panel: Rgb::new(22, 25, 33),
            panel_dark: Rgb::new(9, 11, 16),
            tile: Rgb::new(30, 35, 47),
            tile_hover: Rgb::new(41, 49, 65),
            accent: Rgb::new(87, 219, 181),
            text: Rgb::new(239, 244, 248),
            muted: Rgb::new(151, 162, 178),
            danger: Rgb::new(255, 111, 125),
        },
        Preset::DefaultGray | Preset::LegacyCustom => Palette {
            bg: Rgb::new(18, 20, 22),
            panel: Rgb::new(35, 38, 42),
            panel_dark: Rgb::new(24, 26, 29),
            tile: Rgb::new(42, 45, 49),
            tile_hover: Rgb::new(54, 58, 64),
            accent: Rgb::new(145, 93, 198),
            text: Rgb::new(236, 236, 238),
            muted: Rgb::new(156, 160, 166),
            danger: Rgb::new(236, 112, 112),
        },
        Preset::CatppuccinLatte => Palette {
            bg: Rgb::new(239, 241, 245),
            panel: Rgb::new(230, 233, 239),
            panel_dark: Rgb::new(220, 224, 232),
            tile: Rgb::new(204, 208, 218),
            tile_hover: Rgb::new(188, 192, 204),
            accent: Rgb::new(136, 57, 239),
            text: Rgb::new(76, 79, 105),
            muted: Rgb::new(108, 111, 133),
            danger: Rgb::new(210, 15, 57),
        },
        Preset::CatppuccinFrappe => Palette {
            bg: Rgb::new(48, 52, 70),
            panel: Rgb::new(65, 69, 89),
            panel_dark: Rgb::new(41, 44, 60),
            tile: Rgb::new(81, 87, 109),
            tile_hover: Rgb::new(98, 104, 128),
            accent: Rgb::new(202, 158, 230),
            text: Rgb::new(198, 208, 245),
            muted: Rgb::new(165, 173, 206),
            danger: Rgb::new(231, 130, 132),
        },
        Preset::CatppuccinMacchiato => Palette {
            bg: Rgb::new(36, 39, 58),
            panel: Rgb::new(54, 58, 79),
            panel_dark: Rgb::new(30, 32, 48),
            tile: Rgb::new(73, 77, 100),
            tile_hover: Rgb::new(91, 96, 120),
            accent: Rgb::new(198, 160, 246),
            text: Rgb::new(202, 211, 245),
            muted: Rgb::new(165, 173, 203),
            danger: Rgb::new(237, 135, 150),
        },
        Preset::CatppuccinMocha => Palette {
            bg: Rgb::new(30, 30, 46),
            panel: Rgb::new(49, 50, 68),
            panel_dark: Rgb::new(24, 24, 37),
            tile: Rgb::new(69, 71, 90),
            tile_hover: Rgb::new(88, 91, 112),
            accent: Rgb::new(203, 166, 247),
            text: Rgb::new(205, 214, 244),
            muted: Rgb::new(166, 173, 200),
            danger: Rgb::new(243, 139, 168),
        },
        Preset::NuarContrastBlack => Palette {
            bg: Rgb::new(0, 0, 0),
            panel: Rgb::new(14, 14, 14),
            panel_dark: Rgb::new(5, 5, 5),
            tile: Rgb::new(24, 24, 24),
            tile_hover: Rgb::new(42, 42, 42),
            accent: Rgb::new(255, 214, 0),
            text: Rgb::new(255, 255, 255),
            muted: Rgb::new(188, 188, 188),
            danger: Rgb::new(255, 88, 88),
        },
        Preset::NuarContrastWhite => Palette {
            bg: Rgb::new(255, 255, 255),
            panel: Rgb::new(244, 244, 244),
            panel_dark: Rgb::new(232, 232, 232),
            tile: Rgb::new(255, 255, 255),
            tile_hover: Rgb::new(224, 224, 224),
            accent: Rgb::new(0, 68, 255),
            text: Rgb::new(0, 0, 0),
            muted: Rgb::new(70, 70, 70),
            danger: Rgb::new(190, 0, 0),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interface_mode_does_not_change_selected_theme_or_palette() {
        let mut settings = UiSettings::from_preset(Preset::CatppuccinMocha);
        let palette = settings.palette;

        settings.apply_interface_mode(InterfaceMode::RawDev);

        assert_eq!(
            settings.theme,
            ThemeSelection::Preset(Preset::CatppuccinMocha)
        );
        assert_eq!(settings.preset, Preset::CatppuccinMocha);
        assert_eq!(settings.palette, palette);
    }

    #[test]
    fn first_palette_edit_creates_one_named_custom_theme() {
        let mut settings = UiSettings::from_preset(Preset::ModernDark);
        settings.palette.accent = Rgb::new(1, 2, 3);

        settings.ensure_editable_theme();

        assert_eq!(settings.custom_themes.len(), 1);
        assert_eq!(
            settings.theme,
            ThemeSelection::Custom("Custom 1".to_owned())
        );
        assert_eq!(settings.custom_themes[0].name, "Custom 1");
        assert_eq!(settings.custom_themes[0].palette, settings.palette);
    }

    #[test]
    fn editing_selected_custom_theme_updates_it_in_place() {
        let mut settings = UiSettings::from_preset(Preset::ModernDark);
        settings.palette.accent = Rgb::new(1, 2, 3);
        settings.ensure_editable_theme();

        settings.palette.accent = Rgb::new(4, 5, 6);
        settings.ensure_editable_theme();

        assert_eq!(settings.custom_themes.len(), 1);
        assert_eq!(settings.custom_themes[0].palette, settings.palette);
    }

    #[test]
    fn selected_custom_theme_can_be_renamed() {
        let mut settings = UiSettings::from_preset(Preset::ModernDark);
        settings.palette.accent = Rgb::new(1, 2, 3);
        settings.ensure_editable_theme();

        settings.rename_selected_custom_theme("My Theme".to_owned());

        assert_eq!(
            settings.theme,
            ThemeSelection::Custom("My Theme".to_owned())
        );
        assert_eq!(settings.custom_themes[0].name, "My Theme");
    }

    #[test]
    fn deleting_only_custom_theme_returns_to_fallback_preset() {
        let mut settings = UiSettings::from_preset(Preset::CatppuccinMocha);
        settings.palette.accent = Rgb::new(1, 2, 3);
        settings.ensure_editable_theme();

        assert!(settings.delete_selected_custom_theme());

        assert!(settings.custom_themes.is_empty());
        assert_eq!(
            settings.theme,
            ThemeSelection::Preset(Preset::CatppuccinMocha)
        );
        assert_eq!(settings.palette, palette_for(Preset::CatppuccinMocha));
    }

    #[test]
    fn deleting_custom_theme_selects_neighbor_when_available() {
        let mut settings = UiSettings::from_preset(Preset::ModernDark);
        settings.palette.accent = Rgb::new(1, 2, 3);
        settings.ensure_editable_theme();
        settings.rename_selected_custom_theme("First".to_owned());
        settings.apply_preset(Preset::DefaultGray);
        settings.palette.accent = Rgb::new(4, 5, 6);
        settings.ensure_editable_theme();
        settings.rename_selected_custom_theme("Second".to_owned());

        assert!(settings.delete_selected_custom_theme());

        assert_eq!(settings.custom_themes.len(), 1);
        assert_eq!(settings.theme, ThemeSelection::Custom("First".to_owned()));
        assert_eq!(settings.custom_themes[0].name, "First");
    }

    #[test]
    fn media_preview_framerate_defaults_for_existing_settings() {
        let mut value = serde_json::to_value(UiSettings::from_preset(Preset::ModernDark)).unwrap();
        let preview = value
            .get_mut("media_hover_preview")
            .and_then(serde_json::Value::as_object_mut)
            .unwrap();
        preview.remove("framerate_fps");

        let settings: UiSettings = serde_json::from_value(value).unwrap();

        assert_eq!(settings.media_hover_preview.framerate_fps, 6);
    }

    #[test]
    fn unreadable_legacy_palette_gets_readable_text_colors() {
        let mut settings = UiSettings::from_preset(Preset::ModernDark);
        settings.palette.text = settings.palette.bg;
        settings.palette.muted = settings.palette.panel;
        settings.palette.danger = settings.palette.tile;

        let path = std::env::temp_dir().join(format!(
            "symbolis-unreadable-theme-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&path, serde_json::to_string(&settings).unwrap()).unwrap();

        let loaded = load_settings(&path).unwrap();
        let _ = fs::remove_file(path);

        assert!(contrast_ratio(loaded.palette.text, loaded.palette.bg) >= 4.5);
        assert!(contrast_ratio(loaded.palette.text, loaded.palette.panel) >= 4.5);
        assert!(contrast_ratio(loaded.palette.text, loaded.palette.tile) >= 4.5);
        assert!(contrast_ratio(loaded.palette.muted, loaded.palette.bg) >= 3.0);
    }

    #[test]
    fn legacy_custom_preset_loads_as_named_theme() {
        let mut settings = UiSettings::from_preset(Preset::ModernDark);
        settings.preset = Preset::LegacyCustom;
        settings.palette.accent = Rgb::new(9, 8, 7);

        let mut value = serde_json::to_value(&settings).unwrap();
        let object = value.as_object_mut().unwrap();
        object.remove("theme");
        object.remove("custom_themes");

        let path = std::env::temp_dir().join(format!(
            "symbolis-legacy-theme-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&path, serde_json::to_string(&value).unwrap()).unwrap();

        let loaded = load_settings(&path).unwrap();
        let _ = fs::remove_file(path);

        assert_eq!(loaded.preset, Preset::ModernDark);
        assert_eq!(loaded.theme, ThemeSelection::Custom("Custom 1".to_owned()));
        assert_eq!(loaded.custom_themes.len(), 1);
        assert_eq!(loaded.custom_themes[0].palette.accent, Rgb::new(9, 8, 7));
    }
}
