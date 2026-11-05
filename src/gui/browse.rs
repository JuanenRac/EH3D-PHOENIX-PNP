// EH3D-PHOENIX-PNP - the file picker of the window, without the window.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The list of files the picker of the window shows: the folder being looked at, its sub-folders first and then the files of the kinds the program reads, and what a click, a typed
//! path and the "up" button do. It only reads folders, so it is tested without a screen.

use std::path::{Path, PathBuf};

/// The most entries of a folder that the list holds; a folder with more is cut, and the list says so.
pub const MAX_ENTRIES: usize = 5000;

/// The extensions of the files the program reads.
pub const FILE_EXTENSIONS: [&str; 6] = ["pos", "csv", "txt", "cfg", "json", "tsv"];

/// A folder or a file of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

/// What the picker is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    /// Load files of any kind the program reads; the picker stays open for the next one.
    Files,
    /// The file with the measured places of the fiducials.
    Fiducials,
    /// Choose the folder where the outputs are written.
    Folder,
}

/// Whether a file name ends in one of the extensions, without regard to case; with no extensions every name matches. A name that is only an extension (`.csv`) has none.
pub fn matches_extension(name: &str, extensions: &[&str]) -> bool {
    if extensions.is_empty() {
        return true;
    }
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => {
            extensions.iter().any(|e| e.eq_ignore_ascii_case(ext))
        }
        _ => false,
    }
}

/// The contents of a folder: the sub-folders first, then the files that match, each group in alphabetical order without regard to case. A name that starts with a dot is left out.
/// The second value says that the folder had more than `MAX_ENTRIES` entries and was cut.
pub fn list(
    dir: &Path,
    extensions: &[&str],
    files_too: bool,
) -> Result<(Vec<Entry>, bool), String> {
    let read = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut folders = Vec::new();
    let mut files = Vec::new();
    let mut cut = false;
    for item in read.flatten() {
        if folders.len() + files.len() >= MAX_ENTRIES {
            cut = true;
            break;
        }
        let name = item.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let Ok(meta) = item.metadata() else {
            continue;
        };
        if meta.is_dir() {
            folders.push(Entry {
                name,
                is_dir: true,
                size: 0,
            });
        } else if files_too && meta.is_file() && matches_extension(&name, extensions) {
            files.push(Entry {
                name,
                is_dir: false,
                size: meta.len(),
            });
        }
    }
    folders.sort_by_key(|e| e.name.to_lowercase());
    files.sort_by_key(|e| e.name.to_lowercase());
    folders.extend(files);
    Ok((folders, cut))
}

/// A size for the list: bytes, KiB or MiB.
pub fn size_text(bytes: u64) -> String {
    if bytes >= 1_048_576 {
        format!("{:.1} MiB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

/// The folder where the picker starts: the one given if it exists, else the current one, else the one of the person.
pub fn start_dir(preferred: Option<&Path>) -> PathBuf {
    if let Some(p) = preferred.filter(|p| p.is_dir()) {
        return p.to_path_buf();
    }
    std::env::current_dir()
        .ok()
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// The state of the picker.
#[derive(Debug, Clone)]
pub struct Browser {
    pub purpose: Purpose,
    pub dir: PathBuf,
    pub entries: Vec<Entry>,
    /// The folder had more entries than the list holds.
    pub cut: bool,
    pub error: Option<String>,
    /// The text of the box where a path can be typed.
    pub path_text: String,
    /// Every file is shown, whatever its extension.
    pub show_all: bool,
}

impl Browser {
    pub fn new(purpose: Purpose, dir: PathBuf) -> Browser {
        let mut b = Browser {
            purpose,
            dir: PathBuf::new(),
            entries: Vec::new(),
            cut: false,
            error: None,
            path_text: String::new(),
            show_all: false,
        };
        b.go(dir);
        b
    }

    /// Reads the folder again (after the filter changed).
    pub fn refresh(&mut self) {
        let extensions: &[&str] = if self.show_all { &[] } else { &FILE_EXTENSIONS };
        match list(&self.dir, extensions, self.purpose != Purpose::Folder) {
            Ok((entries, cut)) => {
                self.entries = entries;
                self.cut = cut;
                self.error = None;
            }
            Err(e) => {
                self.entries.clear();
                self.cut = false;
                self.error = Some(e);
            }
        }
    }

    /// Goes to a folder. When it cannot be read the list is empty and the error is kept for the window to show.
    pub fn go(&mut self, dir: PathBuf) {
        self.dir = dir;
        self.path_text = self.dir.display().to_string();
        self.refresh();
    }

    /// Goes to the folder above, if there is one.
    pub fn up(&mut self) {
        if let Some(parent) = self.dir.parent() {
            if !parent.as_os_str().is_empty() {
                let parent = parent.to_path_buf();
                self.go(parent);
            }
        }
    }

    /// What a click on a name does: a folder is entered (and nothing is returned); a file is returned with its whole path.
    pub fn enter(&mut self, name: &str) -> Option<PathBuf> {
        let path = self.dir.join(name);
        if path.is_dir() {
            self.go(path);
            None
        } else {
            Some(path)
        }
    }

    /// What the path typed in the box means: a folder is entered, a file is returned, anything else is an error for the window to show.
    pub fn go_typed(&mut self) -> Option<PathBuf> {
        let path = PathBuf::from(self.path_text.trim().trim_matches('"'));
        if path.is_dir() {
            self.go(path);
            None
        } else if path.is_file() {
            Some(path)
        } else {
            self.error = Some(format!("{}: not found", path.display()));
            None
        }
    }
}
