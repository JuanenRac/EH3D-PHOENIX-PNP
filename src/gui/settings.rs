// EH3D-PHOENIX-PNP - what the window remembers between runs.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The settings of the window that belong to the person and not to the machine: the language, the theme, the layers of the view, the folder for the files that are written, the folder
//! the file picker was last in and the widths of the columns of the tables. They are kept in a small `key = value` file in the folder of the settings of the user. What tunes the machine or the simulation lives in `machine.cfg`, not here.

use super::theme::ThemeId;
use crate::error::Lang;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// The layers of the view that can be switched on and off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layers {
    pub parts: bool,
    pub feeders: bool,
    pub path: bool,
    pub polarity: bool,
}

impl Default for Layers {
    fn default() -> Self {
        Layers {
            parts: true,
            feeders: true,
            path: true,
            polarity: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiSettings {
    pub lang: Lang,
    pub theme: ThemeId,
    pub layers: Layers,
    /// Where the plan, the report and the load sheet are written.
    pub out_dir: String,
    /// Whether the person has seen the guide.
    pub guide_seen: bool,
    /// The folder the file picker was last in.
    pub browse_dir: String,
    /// The widths of the columns the person has changed, in pixels, by `table_column` (`parts_2`).
    pub col_widths: BTreeMap<String, u16>,
}

/// The narrowest and the widest a column can be made.
pub const MIN_COLUMN: u16 = 30;
pub const MAX_COLUMN: u16 = 600;
/// The most column widths that are kept.
pub const MAX_WIDTHS: usize = 200;

impl Default for UiSettings {
    fn default() -> Self {
        UiSettings {
            lang: Lang::En,
            theme: ThemeId::Phoenix,
            layers: Layers::default(),
            out_dir: String::new(),
            guide_seen: false,
            browse_dir: String::new(),
            col_widths: BTreeMap::new(),
        }
    }
}

/// Whether `text` names a column as `table_number`: a table name of letters and a column number.
fn column_key(text: &str) -> bool {
    match text.rsplit_once('_') {
        Some((table, number)) => {
            !table.is_empty()
                && table.len() <= 16
                && table.chars().all(|c| c.is_ascii_lowercase())
                && !number.is_empty()
                && number.len() <= 2
                && number.chars().all(|c| c.is_ascii_digit())
        }
        None => false,
    }
}

fn flag(text: &str) -> Option<bool> {
    match text.trim().to_ascii_lowercase().as_str() {
        "yes" | "true" | "on" | "1" => Some(true),
        "no" | "false" | "off" | "0" => Some(false),
        _ => None,
    }
}

fn word(on: bool) -> &'static str {
    if on {
        "yes"
    } else {
        "no"
    }
}

impl UiSettings {
    /// Reads the settings from text. A line that is not understood is skipped: a file from another version must never keep the window from opening.
    pub fn parse(text: &str) -> UiSettings {
        let mut s = UiSettings::default();
        for line in text.lines().take(200) {
            let t = line.trim();
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            let Some((k, v)) = t.split_once('=') else {
                continue;
            };
            let (k, v) = (k.trim().to_ascii_lowercase(), v.trim());
            match k.as_str() {
                "language" => {
                    if let Some(l) = Lang::from_code(v) {
                        s.lang = l;
                    }
                }
                "theme" => {
                    if let Some(id) = ThemeId::from_key(v) {
                        s.theme = id;
                    }
                }
                "layer_parts" => s.layers.parts = flag(v).unwrap_or(s.layers.parts),
                "layer_feeders" => s.layers.feeders = flag(v).unwrap_or(s.layers.feeders),
                "layer_path" => s.layers.path = flag(v).unwrap_or(s.layers.path),
                "layer_polarity" => s.layers.polarity = flag(v).unwrap_or(s.layers.polarity),
                "out_dir" => {
                    if v.chars().count() <= 1024 && !v.chars().any(char::is_control) {
                        s.out_dir = v.to_string();
                    }
                }
                "guide_seen" => s.guide_seen = flag(v).unwrap_or(s.guide_seen),
                "browse_dir" => {
                    if v.chars().count() <= 1024 && !v.chars().any(char::is_control) {
                        s.browse_dir = v.to_string();
                    }
                }
                other => {
                    if let Some(key) = other.strip_prefix("col_").filter(|k| column_key(k)) {
                        if let Ok(w) = v.parse::<u16>() {
                            if s.col_widths.len() < MAX_WIDTHS || s.col_widths.contains_key(key) {
                                s.col_widths
                                    .insert(key.to_string(), w.clamp(MIN_COLUMN, MAX_COLUMN));
                            }
                        }
                    }
                }
            }
        }
        s
    }

    /// The widths of the columns of a table: the ones the person chose, and the default for those that were never changed.
    pub fn column_widths(&self, table: &str, defaults: &[f32]) -> Vec<f32> {
        defaults
            .iter()
            .enumerate()
            .map(|(i, d)| {
                self.col_widths
                    .get(&format!("{table}_{i}"))
                    .map_or(*d, |w| f32::from(*w))
            })
            .collect()
    }

    /// Remembers the width of a column, kept between `MIN_COLUMN` and `MAX_COLUMN`.
    pub fn set_column_width(&mut self, table: &str, index: usize, width: f32) {
        if !width.is_finite() {
            return;
        }
        let w = (width.round() as i64).clamp(i64::from(MIN_COLUMN), i64::from(MAX_COLUMN)) as u16;
        let key = format!("{table}_{index}");
        if column_key(&key)
            && (self.col_widths.len() < MAX_WIDTHS || self.col_widths.contains_key(&key))
        {
            self.col_widths.insert(key, w);
        }
    }

    pub fn to_text(&self) -> String {
        let mut widths = String::new();
        for (k, w) in &self.col_widths {
            widths.push_str(&format!("col_{k} = {w}\n"));
        }
        format!(
            "# EH3D-PHOENIX-PNP window settings\nlanguage = {}\ntheme = {}\nlayer_parts = {}\nlayer_feeders = {}\nlayer_path = {}\nlayer_polarity = {}\nout_dir = {}\nguide_seen = {}\nbrowse_dir = {}\n{widths}",
            self.lang.code(),
            self.theme.key(),
            word(self.layers.parts),
            word(self.layers.feeders),
            word(self.layers.path),
            word(self.layers.polarity),
            self.out_dir,
            word(self.guide_seen),
            self.browse_dir,
        )
    }
}

/// The file of the settings, from the variables of the environment: `%APPDATA%` on Windows, else `$XDG_CONFIG_HOME` or `~/.config`. None when none of them is set.
pub fn settings_path(
    appdata: Option<&str>,
    xdg: Option<&str>,
    home: Option<&str>,
) -> Option<PathBuf> {
    let base = if let Some(a) = appdata.filter(|a| !a.is_empty()) {
        PathBuf::from(a).join("EH3D-PHOENIX-PNP")
    } else if let Some(x) = xdg.filter(|x| !x.is_empty()) {
        PathBuf::from(x).join("eh3d-phoenix-pnp")
    } else {
        let h = home.filter(|h| !h.is_empty())?;
        PathBuf::from(h).join(".config").join("eh3d-phoenix-pnp")
    };
    Some(base.join("window.cfg"))
}

fn env_path() -> Option<PathBuf> {
    settings_path(
        std::env::var("APPDATA").ok().as_deref(),
        std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
        std::env::var("HOME").ok().as_deref(),
    )
}

/// Writes a message beside the settings, in `last_error.txt`: the program of the window has no console to print an error on. Gives whether it could be written.
pub fn report_error(message: &str) -> bool {
    let Some(path) = env_path().and_then(|p| p.parent().map(|d| d.join("last_error.txt"))) else {
        return false;
    };
    if let Some(dir) = path.parent() {
        if std::fs::create_dir_all(dir).is_err() {
            return false;
        }
    }
    std::fs::write(path, format!("{message}\n")).is_ok()
}

/// The settings from the file of the user, or the defaults when there is none or it cannot be read.
pub fn load() -> UiSettings {
    env_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|t| UiSettings::parse(&t))
        .unwrap_or_default()
}

/// Writes the settings; a failure is not an error of the program (the window works without them).
pub fn save(s: &UiSettings) -> bool {
    let Some(path) = env_path() else {
        return false;
    };
    if let Some(dir) = path.parent() {
        if std::fs::create_dir_all(dir).is_err() {
            return false;
        }
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, s.to_text())
        .and_then(|()| std::fs::rename(&tmp, &path))
        .is_ok()
}
