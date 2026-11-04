// EH3D-PHOENIX-PNP - the colours of the window.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The themes of the window: data, without any drawing. The default is the green of the phoenix on a very dark green, the colours of the icon of the program.

/// A colour as red, green and blue.
pub type Rgb = (u8, u8, u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeId {
    Phoenix,
    Light,
    Contrast,
}

/// The colours of one theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub background: Rgb,
    pub panel: Rgb,
    pub raised: Rgb,
    pub line: Rgb,
    pub text: Rgb,
    pub dim: Rgb,
    pub accent: Rgb,
    pub accent2: Rgb,
    pub ok: Rgb,
    pub warn: Rgb,
    pub danger: Rgb,
    pub board: Rgb,
    pub grid: Rgb,
    pub feeder: Rgb,
    pub path: Rgb,
    pub dark: bool,
}

impl ThemeId {
    pub const ALL: [ThemeId; 3] = [ThemeId::Phoenix, ThemeId::Light, ThemeId::Contrast];

    /// The word of the configuration file (`gui_theme = phoenix`).
    pub fn key(self) -> &'static str {
        match self {
            ThemeId::Phoenix => "phoenix",
            ThemeId::Light => "light",
            ThemeId::Contrast => "contrast",
        }
    }

    /// The theme a word names, without regard to case.
    pub fn from_key(text: &str) -> Option<ThemeId> {
        let t = text.trim().to_ascii_lowercase();
        ThemeId::ALL.into_iter().find(|id| id.key() == t)
    }

    pub fn palette(self) -> Palette {
        match self {
            ThemeId::Phoenix => Palette {
                background: (0x07, 0x14, 0x0f),
                panel: (0x0d, 0x1f, 0x18),
                raised: (0x14, 0x2d, 0x22),
                line: (0x1f, 0x4d, 0x39),
                text: (0xd8, 0xff, 0xde),
                dim: (0x7f, 0xae, 0x93),
                accent: (0x2f, 0xff, 0x94),
                accent2: (0x8f, 0xff, 0xa4),
                ok: (0x3f, 0xff, 0x88),
                warn: (0xff, 0xb0, 0x00),
                danger: (0xff, 0x54, 0x70),
                board: (0x10, 0x3a, 0x2a),
                grid: (0x18, 0x36, 0x2a),
                feeder: (0x4c, 0xc9, 0xf0),
                path: (0xff, 0xb0, 0x00),
                dark: true,
            },
            ThemeId::Light => Palette {
                background: (0xf3, 0xf7, 0xf4),
                panel: (0xff, 0xff, 0xff),
                raised: (0xe4, 0xee, 0xe8),
                line: (0xb5, 0xcc, 0xbf),
                text: (0x14, 0x2b, 0x20),
                dim: (0x5b, 0x74, 0x66),
                accent: (0x0b, 0x8c, 0x64),
                accent2: (0x0a, 0x68, 0x48),
                ok: (0x1b, 0x9e, 0x4b),
                warn: (0xb3, 0x6b, 0x00),
                danger: (0xc4, 0x1e, 0x3a),
                board: (0xd3, 0xe8, 0xdb),
                grid: (0xd6, 0xe2, 0xdb),
                feeder: (0x1a, 0x6f, 0xa8),
                path: (0xb3, 0x6b, 0x00),
                dark: false,
            },
            ThemeId::Contrast => Palette {
                background: (0x00, 0x00, 0x00),
                panel: (0x00, 0x00, 0x00),
                raised: (0x1a, 0x1a, 0x1a),
                line: (0xff, 0xff, 0xff),
                text: (0xff, 0xff, 0xff),
                dim: (0xc8, 0xc8, 0xc8),
                accent: (0x00, 0xff, 0x7f),
                accent2: (0xff, 0xff, 0x00),
                ok: (0x00, 0xff, 0x00),
                warn: (0xff, 0xd0, 0x00),
                danger: (0xff, 0x40, 0x40),
                board: (0x00, 0x30, 0x18),
                grid: (0x40, 0x40, 0x40),
                feeder: (0x00, 0xd0, 0xff),
                path: (0xff, 0xff, 0x00),
                dark: true,
            },
        }
    }

    /// The word of the theme in the window.
    pub fn text(self) -> super::texts::Text {
        match self {
            ThemeId::Phoenix => super::texts::Text::ThemePhoenix,
            ThemeId::Light => super::texts::Text::ThemeLight,
            ThemeId::Contrast => super::texts::Text::ThemeContrast,
        }
    }
}
