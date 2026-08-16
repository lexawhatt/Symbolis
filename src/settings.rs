use std::{
    fs, io,
    path::{Path, PathBuf},
};

use eframe::egui::{
    self, Color32, Context, FontData, FontDefinitions, FontFamily, Rounding, Stroke,
};
use serde::{Deserialize, Serialize};

use crate::gif_provider::GifProvider;

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

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct UiSettings {
    #[serde(default)]
    pub(crate) interface_mode: InterfaceMode,
    #[serde(default)]
    pub(crate) gif_provider: GifProvider,
    #[serde(default)]
    pub(crate) theme: ThemeSelection,
    #[serde(default)]
    pub(crate) custom_themes: Vec<CustomTheme>,
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
            gif_provider: GifProvider::Local,
            theme: ThemeSelection::Preset(preset),
            custom_themes: Vec::new(),
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
        self.gif_provider = previous.gif_provider;
        self.custom_themes = previous.custom_themes;
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

    Some(settings)
}

pub(crate) fn save_settings(path: Option<&Path>, settings: &UiSettings) -> io::Result<()> {
    let Some(path) = path else {
        return Ok(());
    };

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let json = serde_json::to_string_pretty(settings)?;
    fs::write(path, json)
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
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, colors.text.color());
    style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, colors.text.color());
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, colors.text.color());
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

fn sanitize_theme_name(value: &str) -> String {
    value.trim().chars().take(48).collect()
}

pub(crate) fn configure_fonts(ctx: &Context) {
    let mut fonts = FontDefinitions::default();

    for (name, path) in [
        ("NotoSans", "/usr/share/fonts/noto/NotoSans-Regular.ttf"),
        (
            "NotoSansSymbols2",
            "/usr/share/fonts/noto/NotoSansSymbols2-Regular.ttf",
        ),
        (
            "NotoSansSymbols",
            "/usr/share/fonts/noto/NotoSansSymbols-Regular.ttf",
        ),
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
        (
            "NotoSansCJK",
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        ),
        ("DejaVuSans", "/usr/share/fonts/TTF/DejaVuSans.ttf"),
    ] {
        let Ok(bytes) = fs::read(path) else {
            continue;
        };

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

    ctx.set_fonts(fonts);
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
