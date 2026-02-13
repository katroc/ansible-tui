use ratatui::style::Color;

// Catppuccin Mocha palette
pub const MAUVE: Color = Color::Rgb(203, 166, 247);
pub const RED: Color = Color::Rgb(243, 139, 168);
pub const YELLOW: Color = Color::Rgb(249, 226, 175);
pub const GREEN: Color = Color::Rgb(166, 227, 161);
#[allow(dead_code)]
pub const BLUE: Color = Color::Rgb(137, 180, 250);
#[allow(dead_code)]
pub const PEACH: Color = Color::Rgb(250, 179, 135);
pub const OVERLAY0: Color = Color::Rgb(108, 112, 134);

pub const TEXT: Color = Color::Rgb(205, 214, 244);
pub const SUBTEXT1: Color = Color::Rgb(186, 194, 222);
pub const SUBTEXT0: Color = Color::Rgb(166, 173, 200);
pub const SURFACE1: Color = Color::Rgb(69, 71, 90);
pub const SURFACE0: Color = Color::Rgb(49, 50, 68);
pub const BASE: Color = Color::Rgb(30, 30, 46);
pub const MANTLE: Color = Color::Rgb(24, 24, 37);
pub const CRUST: Color = Color::Rgb(17, 17, 27);

// Semantic aliases used for consistent UX emphasis.
pub const FOCUS_BORDER: Color = MAUVE;
pub const HINT_BG: Color = SURFACE0;
pub const HINT_TEXT: Color = TEXT;
pub const HINT_KEY: Color = YELLOW;
