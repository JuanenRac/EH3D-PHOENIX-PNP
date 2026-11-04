// EH3D-PHOENIX-PNP - the log of the window.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The log of the window: lines with a level, a limit on how many are kept and the text of a diagnostic that can be copied. The diagnostic names the version, the system, the counts and
//! the names of the files that were loaded (not their folders), and the last lines of the log; it holds no path and no setting that could be a secret.

use super::workspace::{FileKind, Workspace};

/// The most lines kept; the oldest go first.
pub const MAX_LINES: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Info,
    Warn,
    Error,
}

impl Level {
    pub fn name(self) -> &'static str {
        match self {
            Level::Info => "info",
            Level::Warn => "warn",
            Level::Error => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub level: Level,
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub struct Log {
    entries: Vec<Entry>,
}

impl Log {
    pub fn new() -> Log {
        Log::default()
    }

    pub fn push(&mut self, level: Level, text: impl Into<String>) {
        if self.entries.len() >= MAX_LINES {
            self.entries.remove(0);
        }
        self.entries.push(Entry {
            level,
            text: text.into(),
        });
    }

    pub fn info(&mut self, text: impl Into<String>) {
        self.push(Level::Info, text);
    }

    pub fn warn(&mut self, text: impl Into<String>) {
        self.push(Level::Warn, text);
    }

    pub fn error(&mut self, text: impl Into<String>) {
        self.push(Level::Error, text);
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// The whole log as text, one line per entry: `[level] text`.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for e in &self.entries {
            out.push_str(&format!("[{}] {}\n", e.level.name(), e.text));
        }
        out
    }
}

/// The text of a diagnostic to send or to copy.
pub fn diagnostics(ws: &Workspace, log: &Log) -> String {
    let mut out = String::new();
    out.push_str(&format!("EH3D-PHOENIX-PNP {}\n", env!("CARGO_PKG_VERSION")));
    out.push_str(&format!(
        "system: {} {}\n",
        std::env::consts::OS,
        std::env::consts::ARCH
    ));
    out.push_str(&format!("language: {}\n", ws.lang.code()));
    out.push_str(&format!(
        "parts: {}, feeders: {}, slots: {}\n",
        ws.parts.len(),
        ws.feeders.len(),
        ws.slots.len()
    ));
    out.push_str(&format!(
        "nozzles: {}, order: {}, side: {}\n",
        ws.cfg.nozzles,
        ws.cfg.order.name(),
        if ws.options.side == crate::parts::Side::Top {
            "top"
        } else {
            "bottom"
        }
    ));
    for kind in FileKind::ALL {
        if let Some(name) = ws.names.get(&kind) {
            out.push_str(&format!("file {}: {name}\n", kind.key()));
        }
    }
    out.push_str("log:\n");
    let from = log.entries.len().saturating_sub(100);
    for e in &log.entries[from..] {
        out.push_str(&format!("[{}] {}\n", e.level.name(), e.text));
    }
    out
}
