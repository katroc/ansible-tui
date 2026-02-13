use ratatui::style::{Color, Modifier, Style};

/// Centralized theme token system for the entire UI.
///
/// All colors are defined here. No ad-hoc `Color::Rgb(...)` calls
/// should exist outside this module.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    // ── Text ───────────────────────────────────
    pub fg: Color,
    pub fg_muted: Color,
    pub fg_dim: Color,
    pub fg_inverse: Color,

    // ── Surfaces ───────────────────────────────
    pub bg: Color,
    pub panel_bg: Color,
    pub panel_focus_bg: Color,
    pub hint_bg: Color,
    pub overlay_bg: Color,

    // ── Borders ────────────────────────────────
    pub border_dim: Color,
    pub border_focus: Color,

    // ── Accent ─────────────────────────────────
    pub accent: Color,
    pub accent_soft: Color,

    // ── Selection ──────────────────────────────
    pub selection_bg: Color,
    pub selection_fg: Color,

    // ── Semantic ───────────────────────────────
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub info: Color,

    // ── Highlights ─────────────────────────────
    pub hotkey: Color,
    pub link: Color,
    pub emphasis: Color,

    // ── Chrome ─────────────────────────────────
    pub chrome_bg: Color,
    pub chrome_fg: Color,
}

impl Theme {
    pub fn dark() -> Self {
        // Catppuccin Mocha, refined
        Self {
            fg: Color::Rgb(205, 214, 244),       // CTP Text
            fg_muted: Color::Rgb(166, 173, 200), // CTP Subtext0
            fg_dim: Color::Rgb(108, 112, 134),   // CTP Overlay0
            fg_inverse: Color::Rgb(17, 17, 27),  // CTP Crust

            bg: Color::Rgb(24, 24, 37),             // CTP Mantle
            panel_bg: Color::Rgb(30, 30, 46),       // CTP Base
            panel_focus_bg: Color::Rgb(40, 37, 60), // subtle mauve focus tint
            hint_bg: Color::Rgb(26, 27, 39),        // low-contrast chrome strip
            overlay_bg: Color::Rgb(36, 36, 58),     // between Base and Surface0

            border_dim: Color::Rgb(69, 71, 90), // CTP Surface1
            border_focus: Color::Rgb(203, 166, 247), // CTP Mauve

            accent: Color::Rgb(203, 166, 247),     // CTP Mauve
            accent_soft: Color::Rgb(108, 92, 152), // Mauve at ~40% brightness

            selection_bg: Color::Rgb(58, 52, 82), // mauve-tinted focus surface
            selection_fg: Color::Rgb(205, 214, 244), // CTP Text

            success: Color::Rgb(166, 227, 161), // CTP Green
            warning: Color::Rgb(249, 226, 175), // CTP Yellow
            error: Color::Rgb(243, 139, 168),   // CTP Red
            info: Color::Rgb(137, 180, 250),    // CTP Blue

            hotkey: Color::Rgb(249, 226, 175),   // CTP Yellow
            link: Color::Rgb(137, 180, 250),     // CTP Blue
            emphasis: Color::Rgb(203, 166, 247), // CTP Mauve

            chrome_bg: Color::Rgb(26, 27, 39), // low-contrast chrome strip
            chrome_fg: Color::Rgb(186, 194, 222), // CTP Subtext1
        }
    }

    pub fn light() -> Self {
        // Catppuccin Latte
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

    // ── Text styles ────────────────────────────
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

    // ── Panel/Block styles ─────────────────────
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

    // ── Selection/Highlight ────────────────────
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

    // ── Chrome ─────────────────────────────────
    pub fn chrome(&self) -> Style {
        Style::default().fg(self.chrome_fg).bg(self.chrome_bg)
    }
    pub fn chrome_accent(&self) -> Style {
        Style::default()
            .fg(self.fg_inverse)
            .bg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    // ── Modal ──────────────────────────────────
    pub fn modal_border(&self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }
    pub fn modal_bg(&self) -> Style {
        Style::default().bg(self.overlay_bg).fg(self.fg)
    }

    // ── Semantic ───────────────────────────────
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

    // ── Hints ──────────────────────────────────
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

/// Returns the active theme.
pub fn current() -> &'static Theme {
    static THEME: std::sync::LazyLock<Theme> = std::sync::LazyLock::new(resolve_theme);
    &THEME
}

fn resolve_theme() -> Theme {
    let pref = std::env::var("ANSIBLE_TUI_THEME")
        .unwrap_or_default()
        .to_ascii_lowercase();
    if pref == "light" {
        return Theme::light();
    }

    if supports_truecolor() {
        Theme::dark()
    } else if supports_256_color() {
        Theme::dark_256()
    } else {
        Theme::dark_16()
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
