use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::LazyLock;

use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeName {
    CatppuccinMocha,
    CatppuccinLatte,
    Dracula,
    TokyoNight,
    Gruvbox,
    Nord,
    OneDark,
    Ayu,
    SolarizedDark,
    SolarizedLight,
    Monokai,
    Kanagawa,
    Everforest,
    Nightfox,
    RosePine,
    MaterialOcean,
    Palenight,
    TomorrowNight,
}

const THEME_NAMES: [ThemeName; 18] = [
    ThemeName::CatppuccinMocha,
    ThemeName::CatppuccinLatte,
    ThemeName::Dracula,
    ThemeName::TokyoNight,
    ThemeName::Gruvbox,
    ThemeName::Nord,
    ThemeName::OneDark,
    ThemeName::Ayu,
    ThemeName::SolarizedDark,
    ThemeName::SolarizedLight,
    ThemeName::Monokai,
    ThemeName::Kanagawa,
    ThemeName::Everforest,
    ThemeName::Nightfox,
    ThemeName::RosePine,
    ThemeName::MaterialOcean,
    ThemeName::Palenight,
    ThemeName::TomorrowNight,
];

const DEFAULT_THEME_INDEX: usize = 0;

impl ThemeName {
    pub fn all() -> &'static [ThemeName] {
        &THEME_NAMES
    }

    pub fn display_name(self) -> &'static str {
        match self {
            ThemeName::CatppuccinMocha => "Catppuccin Mocha",
            ThemeName::CatppuccinLatte => "Catppuccin Latte",
            ThemeName::Dracula => "Dracula",
            ThemeName::TokyoNight => "TokyoNight",
            ThemeName::Gruvbox => "Gruvbox",
            ThemeName::Nord => "Nord",
            ThemeName::OneDark => "OneDark",
            ThemeName::Ayu => "Ayu",
            ThemeName::SolarizedDark => "Solarized Dark",
            ThemeName::SolarizedLight => "Solarized Light",
            ThemeName::Monokai => "Monokai",
            ThemeName::Kanagawa => "Kanagawa",
            ThemeName::Everforest => "Everforest",
            ThemeName::Nightfox => "Nightfox",
            ThemeName::RosePine => "Rose Pine",
            ThemeName::MaterialOcean => "Material Ocean",
            ThemeName::Palenight => "Palenight",
            ThemeName::TomorrowNight => "Tomorrow Night",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ThemeName::CatppuccinMocha => "catppuccin-mocha",
            ThemeName::CatppuccinLatte => "catppuccin-latte",
            ThemeName::Dracula => "dracula",
            ThemeName::TokyoNight => "tokyonight",
            ThemeName::Gruvbox => "gruvbox",
            ThemeName::Nord => "nord",
            ThemeName::OneDark => "onedark",
            ThemeName::Ayu => "ayu",
            ThemeName::SolarizedDark => "solarized-dark",
            ThemeName::SolarizedLight => "solarized-light",
            ThemeName::Monokai => "monokai",
            ThemeName::Kanagawa => "kanagawa",
            ThemeName::Everforest => "everforest",
            ThemeName::Nightfox => "nightfox",
            ThemeName::RosePine => "rose-pine",
            ThemeName::MaterialOcean => "material-ocean",
            ThemeName::Palenight => "palenight",
            ThemeName::TomorrowNight => "tomorrow-night",
        }
    }

    pub fn from_str(value: &str) -> Option<ThemeName> {
        let normalized = value.trim().to_ascii_lowercase().replace(['_', ' '], "-");
        match normalized.as_str() {
            "dark" | "catppuccin-mocha" | "mocha" => Some(ThemeName::CatppuccinMocha),
            "light" | "catppuccin-latte" | "latte" => Some(ThemeName::CatppuccinLatte),
            "dracula" => Some(ThemeName::Dracula),
            "tokyonight" | "tokyo-night" => Some(ThemeName::TokyoNight),
            "gruvbox" => Some(ThemeName::Gruvbox),
            "nord" => Some(ThemeName::Nord),
            "onedark" | "one-dark" => Some(ThemeName::OneDark),
            "ayu" => Some(ThemeName::Ayu),
            "solarized-dark" | "solarizeddark" => Some(ThemeName::SolarizedDark),
            "solarized-light" | "solarizedlight" => Some(ThemeName::SolarizedLight),
            "monokai" => Some(ThemeName::Monokai),
            "kanagawa" => Some(ThemeName::Kanagawa),
            "everforest" => Some(ThemeName::Everforest),
            "nightfox" => Some(ThemeName::Nightfox),
            "rose-pine" | "rosepine" => Some(ThemeName::RosePine),
            "material-ocean" | "materialocean" | "material" => Some(ThemeName::MaterialOcean),
            "palenight" | "pale-night" => Some(ThemeName::Palenight),
            "tomorrow-night" | "tomorrownight" => Some(ThemeName::TomorrowNight),
            _ => None,
        }
    }

    pub fn to_theme(self) -> Theme {
        match self {
            ThemeName::CatppuccinMocha => Theme::catppuccin_mocha(),
            ThemeName::CatppuccinLatte => Theme::catppuccin_latte(),
            ThemeName::Dracula => Theme::dracula(),
            ThemeName::TokyoNight => Theme::tokyo_night(),
            ThemeName::Gruvbox => Theme::gruvbox(),
            ThemeName::Nord => Theme::nord(),
            ThemeName::OneDark => Theme::one_dark(),
            ThemeName::Ayu => Theme::ayu(),
            ThemeName::SolarizedDark => Theme::solarized_dark(),
            ThemeName::SolarizedLight => Theme::solarized_light(),
            ThemeName::Monokai => Theme::monokai(),
            ThemeName::Kanagawa => Theme::kanagawa(),
            ThemeName::Everforest => Theme::everforest(),
            ThemeName::Nightfox => Theme::nightfox(),
            ThemeName::RosePine => Theme::rose_pine(),
            ThemeName::MaterialOcean => Theme::material_ocean(),
            ThemeName::Palenight => Theme::palenight(),
            ThemeName::TomorrowNight => Theme::tomorrow_night(),
        }
    }
}

/// Centralized theme token system for the entire UI.
///
/// All colors are defined here. No ad-hoc `Color::Rgb(...)` calls
/// should exist outside this module.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    // Text
    pub fg: Color,
    pub fg_muted: Color,
    pub fg_dim: Color,
    pub fg_inverse: Color,

    // Surfaces
    pub bg: Color,
    pub panel_bg: Color,
    pub panel_focus_bg: Color,
    pub hint_bg: Color,
    pub overlay_bg: Color,

    // Borders
    pub border_dim: Color,
    pub border_focus: Color,

    // Accent
    pub accent: Color,
    pub accent_soft: Color,

    // Selection
    pub selection_bg: Color,
    pub selection_fg: Color,

    // Semantic
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub info: Color,

    // Highlights
    pub hotkey: Color,
    pub link: Color,
    pub emphasis: Color,

    // Chrome
    pub chrome_bg: Color,
    pub chrome_fg: Color,
}

impl Theme {
    pub fn catppuccin_mocha() -> Self {
        Self {
            fg: Color::Rgb(205, 214, 244),
            fg_muted: Color::Rgb(166, 173, 200),
            fg_dim: Color::Rgb(108, 112, 134),
            fg_inverse: Color::Rgb(17, 17, 27),
            bg: Color::Rgb(24, 24, 37),
            panel_bg: Color::Rgb(30, 30, 46),
            panel_focus_bg: Color::Rgb(40, 37, 60),
            hint_bg: Color::Rgb(26, 27, 39),
            overlay_bg: Color::Rgb(36, 36, 58),
            border_dim: Color::Rgb(69, 71, 90),
            border_focus: Color::Rgb(203, 166, 247),
            accent: Color::Rgb(203, 166, 247),
            accent_soft: Color::Rgb(108, 92, 152),
            selection_bg: Color::Rgb(58, 52, 82),
            selection_fg: Color::Rgb(205, 214, 244),
            success: Color::Rgb(166, 227, 161),
            warning: Color::Rgb(249, 226, 175),
            error: Color::Rgb(243, 139, 168),
            info: Color::Rgb(137, 180, 250),
            hotkey: Color::Rgb(249, 226, 175),
            link: Color::Rgb(137, 180, 250),
            emphasis: Color::Rgb(203, 166, 247),
            chrome_bg: Color::Rgb(26, 27, 39),
            chrome_fg: Color::Rgb(186, 194, 222),
        }
    }

    pub fn catppuccin_latte() -> Self {
        Self {
            fg: Color::Rgb(76, 79, 105),
            fg_muted: Color::Rgb(108, 111, 133),
            fg_dim: Color::Rgb(156, 160, 176),
            fg_inverse: Color::Rgb(239, 241, 245),
            bg: Color::Rgb(230, 233, 239),
            panel_bg: Color::Rgb(239, 241, 245),
            panel_focus_bg: Color::Rgb(234, 228, 245),
            hint_bg: Color::Rgb(224, 228, 236),
            overlay_bg: Color::Rgb(220, 224, 232),
            border_dim: Color::Rgb(172, 176, 190),
            border_focus: Color::Rgb(136, 57, 239),
            accent: Color::Rgb(136, 57, 239),
            accent_soft: Color::Rgb(188, 159, 237),
            selection_bg: Color::Rgb(224, 214, 244),
            selection_fg: Color::Rgb(76, 79, 105),
            success: Color::Rgb(64, 160, 43),
            warning: Color::Rgb(223, 142, 29),
            error: Color::Rgb(210, 15, 57),
            info: Color::Rgb(30, 102, 245),
            hotkey: Color::Rgb(223, 142, 29),
            link: Color::Rgb(30, 102, 245),
            emphasis: Color::Rgb(136, 57, 239),
            chrome_bg: Color::Rgb(224, 228, 236),
            chrome_fg: Color::Rgb(76, 79, 105),
        }
    }

    pub fn dracula() -> Self {
        Self {
            fg: Color::Rgb(248, 248, 242),
            fg_muted: Color::Rgb(98, 114, 164),
            fg_dim: Color::Rgb(68, 71, 90),
            fg_inverse: Color::Rgb(40, 42, 54),
            bg: Color::Rgb(40, 42, 54),
            panel_bg: Color::Rgb(49, 52, 68),
            panel_focus_bg: Color::Rgb(68, 71, 90),
            hint_bg: Color::Rgb(36, 38, 50),
            overlay_bg: Color::Rgb(56, 59, 76),
            border_dim: Color::Rgb(68, 71, 90),
            border_focus: Color::Rgb(189, 147, 249),
            accent: Color::Rgb(189, 147, 249),
            accent_soft: Color::Rgb(131, 103, 174),
            selection_bg: Color::Rgb(68, 71, 90),
            selection_fg: Color::Rgb(248, 248, 242),
            success: Color::Rgb(80, 250, 123),
            warning: Color::Rgb(255, 184, 108),
            error: Color::Rgb(255, 85, 85),
            info: Color::Rgb(139, 233, 253),
            hotkey: Color::Rgb(241, 250, 140),
            link: Color::Rgb(139, 233, 253),
            emphasis: Color::Rgb(255, 121, 198),
            chrome_bg: Color::Rgb(36, 38, 50),
            chrome_fg: Color::Rgb(189, 147, 249),
        }
    }

    pub fn tokyo_night() -> Self {
        Self {
            fg: Color::Rgb(200, 211, 245),
            fg_muted: Color::Rgb(99, 111, 168),
            fg_dim: Color::Rgb(84, 95, 145),
            fg_inverse: Color::Rgb(34, 36, 54),
            bg: Color::Rgb(34, 36, 54),
            panel_bg: Color::Rgb(40, 44, 66),
            panel_focus_bg: Color::Rgb(55, 62, 92),
            hint_bg: Color::Rgb(30, 33, 50),
            overlay_bg: Color::Rgb(46, 50, 74),
            border_dim: Color::Rgb(65, 72, 104),
            border_focus: Color::Rgb(130, 170, 255),
            accent: Color::Rgb(130, 170, 255),
            accent_soft: Color::Rgb(86, 112, 179),
            selection_bg: Color::Rgb(61, 72, 110),
            selection_fg: Color::Rgb(200, 211, 245),
            success: Color::Rgb(195, 232, 141),
            warning: Color::Rgb(255, 199, 119),
            error: Color::Rgb(255, 117, 127),
            info: Color::Rgb(134, 225, 252),
            hotkey: Color::Rgb(255, 199, 119),
            link: Color::Rgb(130, 170, 255),
            emphasis: Color::Rgb(192, 153, 255),
            chrome_bg: Color::Rgb(30, 33, 50),
            chrome_fg: Color::Rgb(172, 184, 220),
        }
    }

    pub fn gruvbox() -> Self {
        Self {
            fg: Color::Rgb(235, 219, 178),
            fg_muted: Color::Rgb(168, 153, 132),
            fg_dim: Color::Rgb(146, 131, 116),
            fg_inverse: Color::Rgb(40, 40, 40),
            bg: Color::Rgb(40, 40, 40),
            panel_bg: Color::Rgb(50, 48, 47),
            panel_focus_bg: Color::Rgb(60, 56, 54),
            hint_bg: Color::Rgb(40, 40, 40),
            overlay_bg: Color::Rgb(58, 56, 54),
            border_dim: Color::Rgb(102, 92, 84),
            border_focus: Color::Rgb(131, 165, 152),
            accent: Color::Rgb(131, 165, 152),
            accent_soft: Color::Rgb(102, 127, 119),
            selection_bg: Color::Rgb(69, 64, 61),
            selection_fg: Color::Rgb(251, 241, 199),
            success: Color::Rgb(142, 192, 124),
            warning: Color::Rgb(250, 189, 47),
            error: Color::Rgb(251, 73, 52),
            info: Color::Rgb(131, 165, 152),
            hotkey: Color::Rgb(254, 128, 25),
            link: Color::Rgb(131, 165, 152),
            emphasis: Color::Rgb(211, 134, 155),
            chrome_bg: Color::Rgb(50, 48, 47),
            chrome_fg: Color::Rgb(213, 196, 161),
        }
    }

    pub fn nord() -> Self {
        Self {
            fg: Color::Rgb(216, 222, 233),
            fg_muted: Color::Rgb(180, 190, 212),
            fg_dim: Color::Rgb(129, 161, 193),
            fg_inverse: Color::Rgb(46, 52, 64),
            bg: Color::Rgb(46, 52, 64),
            panel_bg: Color::Rgb(59, 66, 82),
            panel_focus_bg: Color::Rgb(67, 76, 94),
            hint_bg: Color::Rgb(52, 60, 74),
            overlay_bg: Color::Rgb(67, 76, 94),
            border_dim: Color::Rgb(76, 86, 106),
            border_focus: Color::Rgb(136, 192, 208),
            accent: Color::Rgb(136, 192, 208),
            accent_soft: Color::Rgb(94, 140, 153),
            selection_bg: Color::Rgb(76, 86, 106),
            selection_fg: Color::Rgb(236, 239, 244),
            success: Color::Rgb(163, 190, 140),
            warning: Color::Rgb(235, 203, 139),
            error: Color::Rgb(191, 97, 106),
            info: Color::Rgb(129, 161, 193),
            hotkey: Color::Rgb(136, 192, 208),
            link: Color::Rgb(129, 161, 193),
            emphasis: Color::Rgb(180, 142, 173),
            chrome_bg: Color::Rgb(52, 60, 74),
            chrome_fg: Color::Rgb(216, 222, 233),
        }
    }

    pub fn one_dark() -> Self {
        Self {
            fg: Color::Rgb(171, 178, 191),
            fg_muted: Color::Rgb(127, 132, 142),
            fg_dim: Color::Rgb(92, 99, 112),
            fg_inverse: Color::Rgb(40, 44, 52),
            bg: Color::Rgb(40, 44, 52),
            panel_bg: Color::Rgb(44, 49, 60),
            panel_focus_bg: Color::Rgb(53, 58, 71),
            hint_bg: Color::Rgb(33, 37, 43),
            overlay_bg: Color::Rgb(56, 61, 74),
            border_dim: Color::Rgb(73, 80, 95),
            border_focus: Color::Rgb(97, 175, 239),
            accent: Color::Rgb(97, 175, 239),
            accent_soft: Color::Rgb(76, 136, 186),
            selection_bg: Color::Rgb(62, 68, 82),
            selection_fg: Color::Rgb(198, 205, 218),
            success: Color::Rgb(152, 195, 121),
            warning: Color::Rgb(229, 192, 123),
            error: Color::Rgb(224, 108, 117),
            info: Color::Rgb(86, 182, 194),
            hotkey: Color::Rgb(229, 192, 123),
            link: Color::Rgb(97, 175, 239),
            emphasis: Color::Rgb(198, 120, 221),
            chrome_bg: Color::Rgb(33, 37, 43),
            chrome_fg: Color::Rgb(171, 178, 191),
        }
    }

    pub fn ayu() -> Self {
        Self {
            fg: Color::Rgb(191, 189, 182),
            fg_muted: Color::Rgb(130, 141, 153),
            fg_dim: Color::Rgb(92, 103, 115),
            fg_inverse: Color::Rgb(11, 14, 20),
            bg: Color::Rgb(11, 14, 20),
            panel_bg: Color::Rgb(17, 21, 28),
            panel_focus_bg: Color::Rgb(28, 34, 45),
            hint_bg: Color::Rgb(14, 18, 24),
            overlay_bg: Color::Rgb(25, 31, 41),
            border_dim: Color::Rgb(51, 62, 75),
            border_focus: Color::Rgb(230, 180, 80),
            accent: Color::Rgb(230, 180, 80),
            accent_soft: Color::Rgb(161, 125, 56),
            selection_bg: Color::Rgb(36, 44, 58),
            selection_fg: Color::Rgb(228, 226, 220),
            success: Color::Rgb(170, 217, 76),
            warning: Color::Rgb(255, 180, 84),
            error: Color::Rgb(240, 113, 120),
            info: Color::Rgb(57, 186, 230),
            hotkey: Color::Rgb(255, 180, 84),
            link: Color::Rgb(57, 186, 230),
            emphasis: Color::Rgb(210, 166, 255),
            chrome_bg: Color::Rgb(14, 18, 24),
            chrome_fg: Color::Rgb(191, 189, 182),
        }
    }

    pub fn solarized_dark() -> Self {
        Self {
            fg: Color::Rgb(131, 148, 150),
            fg_muted: Color::Rgb(101, 123, 131),
            fg_dim: Color::Rgb(88, 110, 117),
            fg_inverse: Color::Rgb(253, 246, 227),
            bg: Color::Rgb(0, 43, 54),
            panel_bg: Color::Rgb(7, 54, 66),
            panel_focus_bg: Color::Rgb(13, 65, 78),
            hint_bg: Color::Rgb(4, 48, 60),
            overlay_bg: Color::Rgb(10, 60, 72),
            border_dim: Color::Rgb(88, 110, 117),
            border_focus: Color::Rgb(38, 139, 210),
            accent: Color::Rgb(38, 139, 210),
            accent_soft: Color::Rgb(26, 94, 135),
            selection_bg: Color::Rgb(18, 72, 88),
            selection_fg: Color::Rgb(238, 232, 213),
            success: Color::Rgb(133, 153, 0),
            warning: Color::Rgb(181, 137, 0),
            error: Color::Rgb(220, 50, 47),
            info: Color::Rgb(42, 161, 152),
            hotkey: Color::Rgb(181, 137, 0),
            link: Color::Rgb(38, 139, 210),
            emphasis: Color::Rgb(108, 113, 196),
            chrome_bg: Color::Rgb(7, 54, 66),
            chrome_fg: Color::Rgb(147, 161, 161),
        }
    }

    pub fn solarized_light() -> Self {
        Self {
            fg: Color::Rgb(101, 123, 131),
            fg_muted: Color::Rgb(131, 148, 150),
            fg_dim: Color::Rgb(147, 161, 161),
            fg_inverse: Color::Rgb(0, 43, 54),
            bg: Color::Rgb(253, 246, 227),
            panel_bg: Color::Rgb(238, 232, 213),
            panel_focus_bg: Color::Rgb(233, 226, 206),
            hint_bg: Color::Rgb(245, 240, 225),
            overlay_bg: Color::Rgb(238, 232, 213),
            border_dim: Color::Rgb(147, 161, 161),
            border_focus: Color::Rgb(38, 139, 210),
            accent: Color::Rgb(38, 139, 210),
            accent_soft: Color::Rgb(122, 171, 200),
            selection_bg: Color::Rgb(220, 214, 191),
            selection_fg: Color::Rgb(0, 43, 54),
            success: Color::Rgb(133, 153, 0),
            warning: Color::Rgb(181, 137, 0),
            error: Color::Rgb(220, 50, 47),
            info: Color::Rgb(42, 161, 152),
            hotkey: Color::Rgb(203, 75, 22),
            link: Color::Rgb(38, 139, 210),
            emphasis: Color::Rgb(108, 113, 196),
            chrome_bg: Color::Rgb(245, 240, 225),
            chrome_fg: Color::Rgb(101, 123, 131),
        }
    }

    pub fn monokai() -> Self {
        Self {
            fg: Color::Rgb(248, 248, 242),
            fg_muted: Color::Rgb(165, 159, 133),
            fg_dim: Color::Rgb(117, 113, 94),
            fg_inverse: Color::Rgb(39, 40, 34),
            bg: Color::Rgb(39, 40, 34),
            panel_bg: Color::Rgb(45, 46, 40),
            panel_focus_bg: Color::Rgb(58, 59, 51),
            hint_bg: Color::Rgb(35, 36, 30),
            overlay_bg: Color::Rgb(54, 55, 47),
            border_dim: Color::Rgb(90, 87, 74),
            border_focus: Color::Rgb(174, 129, 255),
            accent: Color::Rgb(249, 38, 114),
            accent_soft: Color::Rgb(160, 37, 87),
            selection_bg: Color::Rgb(73, 72, 62),
            selection_fg: Color::Rgb(248, 248, 242),
            success: Color::Rgb(166, 226, 46),
            warning: Color::Rgb(253, 151, 31),
            error: Color::Rgb(249, 38, 114),
            info: Color::Rgb(102, 217, 239),
            hotkey: Color::Rgb(230, 219, 116),
            link: Color::Rgb(102, 217, 239),
            emphasis: Color::Rgb(174, 129, 255),
            chrome_bg: Color::Rgb(35, 36, 30),
            chrome_fg: Color::Rgb(197, 200, 178),
        }
    }

    pub fn kanagawa() -> Self {
        Self {
            fg: Color::Rgb(220, 215, 186),
            fg_muted: Color::Rgb(166, 166, 156),
            fg_dim: Color::Rgb(114, 113, 105),
            fg_inverse: Color::Rgb(31, 31, 40),
            bg: Color::Rgb(31, 31, 40),
            panel_bg: Color::Rgb(42, 42, 55),
            panel_focus_bg: Color::Rgb(54, 54, 70),
            hint_bg: Color::Rgb(37, 37, 50),
            overlay_bg: Color::Rgb(49, 49, 66),
            border_dim: Color::Rgb(84, 84, 109),
            border_focus: Color::Rgb(126, 156, 216),
            accent: Color::Rgb(126, 156, 216),
            accent_soft: Color::Rgb(90, 116, 161),
            selection_bg: Color::Rgb(58, 58, 79),
            selection_fg: Color::Rgb(220, 215, 186),
            success: Color::Rgb(152, 187, 108),
            warning: Color::Rgb(230, 195, 132),
            error: Color::Rgb(195, 64, 67),
            info: Color::Rgb(127, 180, 202),
            hotkey: Color::Rgb(255, 160, 102),
            link: Color::Rgb(126, 156, 216),
            emphasis: Color::Rgb(149, 127, 184),
            chrome_bg: Color::Rgb(37, 37, 50),
            chrome_fg: Color::Rgb(200, 192, 147),
        }
    }

    pub fn everforest() -> Self {
        Self {
            fg: Color::Rgb(211, 198, 170),
            fg_muted: Color::Rgb(157, 169, 160),
            fg_dim: Color::Rgb(122, 132, 120),
            fg_inverse: Color::Rgb(30, 35, 38),
            bg: Color::Rgb(45, 53, 59),
            panel_bg: Color::Rgb(52, 63, 68),
            panel_focus_bg: Color::Rgb(63, 75, 80),
            hint_bg: Color::Rgb(43, 51, 57),
            overlay_bg: Color::Rgb(58, 69, 74),
            border_dim: Color::Rgb(86, 99, 95),
            border_focus: Color::Rgb(127, 187, 179),
            accent: Color::Rgb(127, 187, 179),
            accent_soft: Color::Rgb(91, 138, 133),
            selection_bg: Color::Rgb(74, 85, 91),
            selection_fg: Color::Rgb(211, 198, 170),
            success: Color::Rgb(167, 192, 128),
            warning: Color::Rgb(219, 188, 127),
            error: Color::Rgb(230, 126, 128),
            info: Color::Rgb(131, 192, 146),
            hotkey: Color::Rgb(230, 152, 117),
            link: Color::Rgb(127, 187, 179),
            emphasis: Color::Rgb(214, 153, 182),
            chrome_bg: Color::Rgb(43, 51, 57),
            chrome_fg: Color::Rgb(201, 191, 165),
        }
    }

    pub fn nightfox() -> Self {
        Self {
            fg: Color::Rgb(205, 206, 207),
            fg_muted: Color::Rgb(174, 175, 176),
            fg_dim: Color::Rgb(113, 131, 155),
            fg_inverse: Color::Rgb(19, 26, 36),
            bg: Color::Rgb(19, 26, 36),
            panel_bg: Color::Rgb(25, 35, 48),
            panel_focus_bg: Color::Rgb(36, 49, 66),
            hint_bg: Color::Rgb(17, 24, 33),
            overlay_bg: Color::Rgb(31, 43, 58),
            border_dim: Color::Rgb(66, 88, 111),
            border_focus: Color::Rgb(113, 156, 214),
            accent: Color::Rgb(113, 156, 214),
            accent_soft: Color::Rgb(79, 109, 150),
            selection_bg: Color::Rgb(43, 59, 81),
            selection_fg: Color::Rgb(205, 206, 207),
            success: Color::Rgb(129, 178, 154),
            warning: Color::Rgb(219, 192, 116),
            error: Color::Rgb(201, 79, 109),
            info: Color::Rgb(99, 205, 207),
            hotkey: Color::Rgb(244, 162, 97),
            link: Color::Rgb(113, 156, 214),
            emphasis: Color::Rgb(157, 121, 214),
            chrome_bg: Color::Rgb(17, 24, 33),
            chrome_fg: Color::Rgb(174, 175, 176),
        }
    }

    pub fn rose_pine() -> Self {
        Self {
            fg: Color::Rgb(224, 222, 244),
            fg_muted: Color::Rgb(144, 140, 170),
            fg_dim: Color::Rgb(110, 106, 134),
            fg_inverse: Color::Rgb(25, 23, 36),
            bg: Color::Rgb(25, 23, 36),
            panel_bg: Color::Rgb(31, 29, 46),
            panel_focus_bg: Color::Rgb(38, 35, 58),
            hint_bg: Color::Rgb(33, 32, 46),
            overlay_bg: Color::Rgb(38, 35, 58),
            border_dim: Color::Rgb(110, 106, 134),
            border_focus: Color::Rgb(196, 167, 231),
            accent: Color::Rgb(196, 167, 231),
            accent_soft: Color::Rgb(137, 117, 161),
            selection_bg: Color::Rgb(57, 53, 80),
            selection_fg: Color::Rgb(224, 222, 244),
            success: Color::Rgb(156, 207, 216),
            warning: Color::Rgb(246, 193, 119),
            error: Color::Rgb(235, 111, 146),
            info: Color::Rgb(49, 116, 143),
            hotkey: Color::Rgb(246, 193, 119),
            link: Color::Rgb(49, 116, 143),
            emphasis: Color::Rgb(235, 188, 186),
            chrome_bg: Color::Rgb(33, 32, 46),
            chrome_fg: Color::Rgb(144, 140, 170),
        }
    }

    pub fn material_ocean() -> Self {
        Self {
            fg: Color::Rgb(166, 172, 205),
            fg_muted: Color::Rgb(143, 147, 162),
            fg_dim: Color::Rgb(103, 107, 125),
            fg_inverse: Color::Rgb(15, 17, 26),
            bg: Color::Rgb(15, 17, 26),
            panel_bg: Color::Rgb(26, 28, 37),
            panel_focus_bg: Color::Rgb(35, 39, 51),
            hint_bg: Color::Rgb(20, 22, 31),
            overlay_bg: Color::Rgb(31, 35, 46),
            border_dim: Color::Rgb(66, 72, 89),
            border_focus: Color::Rgb(130, 170, 255),
            accent: Color::Rgb(130, 170, 255),
            accent_soft: Color::Rgb(91, 118, 179),
            selection_bg: Color::Rgb(49, 56, 74),
            selection_fg: Color::Rgb(198, 204, 230),
            success: Color::Rgb(195, 232, 141),
            warning: Color::Rgb(255, 203, 107),
            error: Color::Rgb(240, 113, 120),
            info: Color::Rgb(137, 221, 255),
            hotkey: Color::Rgb(255, 203, 107),
            link: Color::Rgb(137, 221, 255),
            emphasis: Color::Rgb(199, 146, 234),
            chrome_bg: Color::Rgb(20, 22, 31),
            chrome_fg: Color::Rgb(166, 172, 205),
        }
    }

    pub fn palenight() -> Self {
        Self {
            fg: Color::Rgb(166, 172, 205),
            fg_muted: Color::Rgb(127, 133, 163),
            fg_dim: Color::Rgb(103, 110, 149),
            fg_inverse: Color::Rgb(27, 30, 43),
            bg: Color::Rgb(41, 45, 62),
            panel_bg: Color::Rgb(47, 51, 77),
            panel_focus_bg: Color::Rgb(56, 62, 93),
            hint_bg: Color::Rgb(37, 41, 58),
            overlay_bg: Color::Rgb(52, 57, 85),
            border_dim: Color::Rgb(79, 85, 119),
            border_focus: Color::Rgb(199, 146, 234),
            accent: Color::Rgb(199, 146, 234),
            accent_soft: Color::Rgb(139, 102, 163),
            selection_bg: Color::Rgb(65, 72, 99),
            selection_fg: Color::Rgb(199, 204, 230),
            success: Color::Rgb(195, 232, 141),
            warning: Color::Rgb(255, 203, 107),
            error: Color::Rgb(240, 113, 120),
            info: Color::Rgb(130, 170, 255),
            hotkey: Color::Rgb(255, 203, 107),
            link: Color::Rgb(130, 170, 255),
            emphasis: Color::Rgb(247, 140, 108),
            chrome_bg: Color::Rgb(37, 41, 58),
            chrome_fg: Color::Rgb(166, 172, 205),
        }
    }

    pub fn tomorrow_night() -> Self {
        Self {
            fg: Color::Rgb(197, 200, 198),
            fg_muted: Color::Rgb(150, 152, 150),
            fg_dim: Color::Rgb(112, 120, 128),
            fg_inverse: Color::Rgb(29, 31, 33),
            bg: Color::Rgb(29, 31, 33),
            panel_bg: Color::Rgb(36, 38, 40),
            panel_focus_bg: Color::Rgb(47, 50, 53),
            hint_bg: Color::Rgb(26, 28, 30),
            overlay_bg: Color::Rgb(43, 45, 47),
            border_dim: Color::Rgb(95, 99, 100),
            border_focus: Color::Rgb(129, 162, 190),
            accent: Color::Rgb(129, 162, 190),
            accent_soft: Color::Rgb(92, 114, 136),
            selection_bg: Color::Rgb(55, 59, 65),
            selection_fg: Color::Rgb(197, 200, 198),
            success: Color::Rgb(181, 189, 104),
            warning: Color::Rgb(240, 198, 116),
            error: Color::Rgb(204, 102, 102),
            info: Color::Rgb(138, 190, 183),
            hotkey: Color::Rgb(240, 198, 116),
            link: Color::Rgb(129, 162, 190),
            emphasis: Color::Rgb(178, 148, 187),
            chrome_bg: Color::Rgb(26, 28, 30),
            chrome_fg: Color::Rgb(197, 200, 198),
        }
    }

    pub fn dark_256() -> Self {
        Self {
            fg: Color::Indexed(252),
            fg_muted: Color::Indexed(245),
            fg_dim: Color::Indexed(240),
            fg_inverse: Color::Indexed(16),
            bg: Color::Indexed(234),
            panel_bg: Color::Indexed(235),
            panel_focus_bg: Color::Indexed(238),
            hint_bg: Color::Indexed(236),
            overlay_bg: Color::Indexed(236),
            border_dim: Color::Indexed(238),
            border_focus: Color::Indexed(141),
            accent: Color::Indexed(141),
            accent_soft: Color::Indexed(97),
            selection_bg: Color::Indexed(60),
            selection_fg: Color::Indexed(252),
            success: Color::Indexed(114),
            warning: Color::Indexed(222),
            error: Color::Indexed(204),
            info: Color::Indexed(111),
            hotkey: Color::Indexed(222),
            link: Color::Indexed(111),
            emphasis: Color::Indexed(141),
            chrome_bg: Color::Indexed(236),
            chrome_fg: Color::Indexed(250),
        }
    }

    pub fn dark_16() -> Self {
        Self {
            fg: Color::White,
            fg_muted: Color::Gray,
            fg_dim: Color::DarkGray,
            fg_inverse: Color::Black,
            bg: Color::Reset,
            panel_bg: Color::Reset,
            panel_focus_bg: Color::DarkGray,
            hint_bg: Color::DarkGray,
            overlay_bg: Color::Reset,
            border_dim: Color::DarkGray,
            border_focus: Color::Magenta,
            accent: Color::Magenta,
            accent_soft: Color::Magenta,
            selection_bg: Color::DarkGray,
            selection_fg: Color::White,
            success: Color::Green,
            warning: Color::Yellow,
            error: Color::Red,
            info: Color::Blue,
            hotkey: Color::Yellow,
            link: Color::Blue,
            emphasis: Color::Magenta,
            chrome_bg: Color::DarkGray,
            chrome_fg: Color::White,
        }
    }

    // Text styles
    pub fn text(&self) -> Style {
        Style::default().fg(self.fg)
    }
    pub fn text_muted(&self) -> Style {
        Style::default().fg(self.fg_muted)
    }
    pub fn text_dim(&self) -> Style {
        Style::default().fg(self.fg_dim)
    }
    pub fn text_emphasis(&self) -> Style {
        Style::default()
            .fg(self.emphasis)
            .add_modifier(Modifier::BOLD)
    }

    // Panel/Block styles
    pub fn panel_border(&self) -> Style {
        Style::default().fg(self.border_dim)
    }
    pub fn panel_border_focused(&self) -> Style {
        Style::default()
            .fg(self.border_focus)
            .add_modifier(Modifier::BOLD)
    }
    pub fn panel_style(&self) -> Style {
        Style::default().bg(self.panel_bg)
    }
    pub fn panel_style_focused(&self) -> Style {
        Style::default().bg(self.panel_focus_bg)
    }

    // Selection/Highlight
    pub fn list_highlight(&self) -> Style {
        Style::default()
            .bg(self.selection_bg)
            .fg(self.selection_fg)
            .add_modifier(Modifier::BOLD)
    }
    pub fn table_highlight(&self) -> Style {
        Style::default()
            .bg(self.selection_bg)
            .fg(self.selection_fg)
            .add_modifier(Modifier::BOLD)
    }

    // Chrome
    pub fn chrome(&self) -> Style {
        Style::default().fg(self.chrome_fg).bg(self.chrome_bg)
    }
    pub fn chrome_accent(&self) -> Style {
        Style::default()
            .fg(self.fg_inverse)
            .bg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    // Modal
    pub fn modal_border(&self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }
    pub fn modal_bg(&self) -> Style {
        Style::default().bg(self.overlay_bg).fg(self.fg)
    }

    // Semantic
    pub fn status_success(&self) -> Style {
        Style::default().fg(self.success)
    }
    pub fn status_warning(&self) -> Style {
        Style::default().fg(self.warning)
    }
    pub fn status_error(&self) -> Style {
        Style::default().fg(self.error).add_modifier(Modifier::BOLD)
    }
    pub fn status_info(&self) -> Style {
        Style::default().fg(self.info)
    }

    pub fn text_link(&self) -> Style {
        Style::default().fg(self.link)
    }

    // Hints
    pub fn hint_key(&self) -> Style {
        Style::default()
            .fg(self.hotkey)
            .add_modifier(Modifier::BOLD)
    }
    pub fn hint_desc(&self) -> Style {
        Style::default().fg(self.chrome_fg)
    }
    pub fn hint_sep(&self) -> Style {
        Style::default().fg(self.fg_dim)
    }
    pub fn hint_bar(&self) -> Style {
        Style::default().bg(self.hint_bg).fg(self.fg_muted)
    }
}

static THEMES: LazyLock<Vec<Theme>> = LazyLock::new(|| {
    ThemeName::all()
        .iter()
        .map(|theme_name| theme_name.to_theme())
        .collect()
});
static FALLBACK_THEME_256: LazyLock<Theme> = LazyLock::new(Theme::dark_256);
static FALLBACK_THEME_16: LazyLock<Theme> = LazyLock::new(Theme::dark_16);
static ACTIVE_THEME_INDEX: AtomicUsize = AtomicUsize::new(DEFAULT_THEME_INDEX);

pub fn apply_env_theme_preference() {
    let Some(value) = std::env::var("ANSIBLE_TUI_THEME").ok() else {
        return;
    };
    if let Some(theme_name) = ThemeName::from_str(&value) {
        set_theme(theme_name);
    }
}

pub fn active_theme_name() -> ThemeName {
    let index = ACTIVE_THEME_INDEX.load(Ordering::Relaxed);
    ThemeName::all()
        .get(index)
        .copied()
        .unwrap_or(ThemeName::CatppuccinMocha)
}

pub fn set_theme(theme_name: ThemeName) {
    let index = ThemeName::all()
        .iter()
        .position(|candidate| *candidate == theme_name)
        .unwrap_or(DEFAULT_THEME_INDEX);
    ACTIVE_THEME_INDEX.store(index, Ordering::Relaxed);
}

/// Returns the active theme.
pub fn current() -> &'static Theme {
    if supports_truecolor() {
        let index = ACTIVE_THEME_INDEX.load(Ordering::Relaxed);
        return THEMES
            .get(index)
            .unwrap_or_else(|| &THEMES[DEFAULT_THEME_INDEX]);
    }

    if supports_256_color() {
        &FALLBACK_THEME_256
    } else {
        &FALLBACK_THEME_16
    }
}

fn supports_truecolor() -> bool {
    std::env::var("COLORTERM")
        .ok()
        .map(|value| {
            let lowered = value.to_ascii_lowercase();
            lowered.contains("truecolor") || lowered.contains("24bit")
        })
        .unwrap_or(false)
}

fn supports_256_color() -> bool {
    std::env::var("TERM")
        .ok()
        .map(|value| value.to_ascii_lowercase().contains("256"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::ThemeName;

    #[test]
    fn theme_name_from_str_supports_legacy_aliases() {
        assert_eq!(
            ThemeName::from_str("dark"),
            Some(ThemeName::CatppuccinMocha)
        );
        assert_eq!(
            ThemeName::from_str("light"),
            Some(ThemeName::CatppuccinLatte)
        );
        assert_eq!(
            ThemeName::from_str("tokyo-night"),
            Some(ThemeName::TokyoNight)
        );
        assert_eq!(ThemeName::from_str("one_dark"), Some(ThemeName::OneDark));
        assert_eq!(ThemeName::from_str("rose pine"), Some(ThemeName::RosePine));
        assert_eq!(
            ThemeName::from_str("material"),
            Some(ThemeName::MaterialOcean)
        );
        assert_eq!(
            ThemeName::from_str("tomorrow-night"),
            Some(ThemeName::TomorrowNight)
        );
    }

    #[test]
    fn theme_name_round_trips_with_as_str() {
        for theme_name in ThemeName::all() {
            assert_eq!(ThemeName::from_str(theme_name.as_str()), Some(*theme_name));
        }
    }
}
