// EH3D-PHOENIX-PNP - what the window works on.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The workspace: the files that were loaded (parts, feeders, configuration, tables of packages and parts, panel, slots), the options of the plan, and the plan that comes from them.
//! It holds no window and no clock, so everything here can be tested with text. A file can be given as text with its name, or as a path; the kind of a file is found from its name and
//! its first line.

use crate::catalog::Catalog;
use crate::config::{MachineConfig, OrderStrategy};
use crate::error::{Code, Lang, PnpError};
use crate::feeding::{self, FeederSpec, Picks};
use crate::loadsheet::{self, LoadSheet, Slot};
use crate::panel::Panel;
use crate::parts::{detect_delimiter, parse_with, Part, Side};
use crate::planner::{plan_advanced, AdvancedPlan, Options};
use crate::preflight::{preflight, Check};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// The kinds of file the window knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FileKind {
    Parts,
    Feeders,
    Config,
    Packages,
    Catalog,
    Panel,
    Slots,
}

impl FileKind {
    pub const ALL: [FileKind; 7] = [
        FileKind::Parts,
        FileKind::Feeders,
        FileKind::Config,
        FileKind::Packages,
        FileKind::Catalog,
        FileKind::Panel,
        FileKind::Slots,
    ];

    pub fn key(self) -> &'static str {
        match self {
            FileKind::Parts => "parts",
            FileKind::Feeders => "feeders",
            FileKind::Config => "config",
            FileKind::Packages => "packages",
            FileKind::Catalog => "catalog",
            FileKind::Panel => "panel",
            FileKind::Slots => "slots",
        }
    }
}

/// The columns a table of parts or packages may have; a CSV whose header holds only these is a table, not a position file.
const TABLE_COLUMNS: [&str; 11] = [
    "value",
    "package",
    "height_mm",
    "through_board_mm",
    "pick_retries",
    "bin",
    "nozzle_tip",
    "rank",
    "origin_dx_mm",
    "origin_dy_mm",
    "tape_mm",
];

/// What kind of file this is, from its name and its first line; `None` when it is empty.
pub fn classify(name: &str, text: &str) -> Option<FileKind> {
    let lower = name.to_ascii_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or("");
    match ext {
        "pos" => return Some(FileKind::Parts),
        "cfg" => return Some(FileKind::Config),
        "json" => return Some(FileKind::Panel),
        _ => {}
    }
    let text = text.trim_start_matches('\u{feff}');
    let first = text.lines().find(|l| !l.trim().is_empty())?.trim();
    if first.starts_with("###") {
        return Some(FileKind::Parts);
    }
    if first.starts_with('{') {
        return Some(FileKind::Panel);
    }
    if first.starts_with('#') || (first.contains('=') && !first.contains(',')) {
        return Some(FileKind::Config);
    }
    let delim = detect_delimiter(first);
    let cols: Vec<String> = first
        .split(delim)
        .map(|c| c.trim().trim_matches('"').to_ascii_lowercase())
        .collect();
    let has = |n: &str| cols.iter().any(|c| c == n);
    if has("max_width_mm") {
        Some(FileKind::Slots)
    } else if has("id") && has("count") {
        Some(FileKind::Feeders)
    } else if cols.iter().all(|c| TABLE_COLUMNS.contains(&c.as_str())) {
        if has("value") {
            Some(FileKind::Catalog)
        } else if has("package") {
            Some(FileKind::Packages)
        } else {
            None
        }
    } else {
        Some(FileKind::Parts)
    }
}

/// The name of a file without its folder.
pub fn file_name(path: &Path) -> String {
    path.file_name()
        .map_or_else(|| "file".to_string(), |n| n.to_string_lossy().into_owned())
}

/// A text file no larger than `max` bytes. Text that is not UTF-8 is read as Latin-1, and the second value says so.
pub fn read_file(path: &Path, max: u64) -> Result<(String, bool), String> {
    let name = file_name(path);
    let meta = std::fs::metadata(path).map_err(|e| format!("{name}: {e}"))?;
    if meta.len() > max {
        return Err(PnpError::new(
            Code::FileTooBig,
            format!("{name} has {} bytes, the limit is {max}", meta.len()),
        )
        .to_string());
    }
    let bytes = std::fs::read(path).map_err(|e| format!("{name}: {e}"))?;
    match String::from_utf8(bytes) {
        Ok(text) => Ok((text, false)),
        Err(e) => Ok((
            e.into_bytes().iter().map(|&b| char::from(b)).collect(),
            true,
        )),
    }
}

/// The plan with what it was made from and the checks of it.
#[derive(Debug, Clone)]
pub struct Planned {
    pub plan: AdvancedPlan,
    pub picks: Picks,
    /// The kinds of part that no table describes, with how many parts.
    pub undefined: Vec<(String, usize)>,
    pub checks: Vec<Check>,
    /// The parts the plan was made for (those that were not left out).
    pub parts: Vec<Part>,
}

#[derive(Debug, Clone)]
struct Snapshot {
    feeders: Vec<FeederSpec>,
    excluded: BTreeSet<String>,
}

/// The files, the settings and the edits of the window.
#[derive(Debug, Clone)]
pub struct Workspace {
    pub lang: Lang,
    pub cfg: MachineConfig,
    pub parts: Vec<Part>,
    pub feeders: Vec<FeederSpec>,
    pub catalog: Catalog,
    pub slots: Vec<Slot>,
    pub options: Options,
    /// The references that a fix left out of the plan.
    pub excluded: BTreeSet<String>,
    /// The names (not the folders) of the files that were loaded.
    pub names: BTreeMap<FileKind, String>,
    parts_text: Option<String>,
    panel_parts: Option<Vec<Part>>,
    packages_text: Option<String>,
    catalog_text: Option<String>,
    undo: Vec<Snapshot>,
}

impl Default for Workspace {
    fn default() -> Self {
        Workspace::new(Lang::En)
    }
}

impl Workspace {
    pub fn new(lang: Lang) -> Workspace {
        Workspace {
            lang,
            cfg: MachineConfig::default(),
            parts: Vec::new(),
            feeders: Vec::new(),
            catalog: Catalog::new(),
            slots: Vec::new(),
            options: Options::default(),
            excluded: BTreeSet::new(),
            names: BTreeMap::new(),
            parts_text: None,
            panel_parts: None,
            packages_text: None,
            catalog_text: None,
            undo: Vec::new(),
        }
    }

    fn localize(&self, e: &PnpError) -> String {
        e.localized(self.lang)
    }

    /// Starts again with nothing loaded; the language stays.
    pub fn clear(&mut self) {
        *self = Workspace::new(self.lang);
    }

    /// Takes a file given as text. The kind is found from the name and the text; the name is kept (without any folder) for the list of files and the diagnostics.
    pub fn load_text(&mut self, name: &str, text: &str) -> Result<FileKind, String> {
        let kind = classify(name, text).ok_or_else(|| format!("{name}: empty"))?;
        self.load_kind(kind, name, text)?;
        Ok(kind)
    }

    /// Takes a file given as text, as a file of the kind stated.
    pub fn load_kind(&mut self, kind: FileKind, name: &str, text: &str) -> Result<(), String> {
        let max = self.cfg.max_file_bytes;
        if text.len() as u64 > max {
            return Err(self.localize(&PnpError::new(
                Code::FileTooBig,
                format!("{name} has {} bytes, the limit is {max}", text.len()),
            )));
        }
        match kind {
            FileKind::Parts => {
                let parts = parse_with(text, &self.cfg.profile)?;
                self.finish_parts(parts)?;
                self.parts_text = Some(text.to_string());
                self.panel_parts = None;
                self.names.remove(&FileKind::Panel);
            }
            FileKind::Feeders => {
                let feeders = feeding::from_csv(text).map_err(|e| self.localize(&e))?;
                self.feeders = feeders;
                self.undo.clear();
            }
            FileKind::Config => {
                let cfg = MachineConfig::parse(text).map_err(|e| self.localize(&e))?;
                let sets_language = text.lines().any(|l| {
                    l.trim()
                        .to_ascii_lowercase()
                        .strip_prefix("language")
                        .is_some_and(|rest| rest.trim_start().starts_with('='))
                });
                if sets_language {
                    self.lang = cfg.language;
                }
                self.cfg = cfg;
                self.reparse_parts()?;
            }
            FileKind::Packages => {
                let mut table = Catalog::new();
                table.add_packages(text).map_err(|e| self.localize(&e))?;
                self.packages_text = Some(text.to_string());
                self.rebuild_catalog()?;
            }
            FileKind::Catalog => {
                let mut table = Catalog::new();
                table.add_parts(text).map_err(|e| self.localize(&e))?;
                self.catalog_text = Some(text.to_string());
                self.rebuild_catalog()?;
            }
            FileKind::Panel => {
                return Err(format!(
                    "{name}: a panel names board files and is loaded from a path"
                ));
            }
            FileKind::Slots => {
                self.slots = loadsheet::slots_from_csv(text).map_err(|e| self.localize(&e))?;
            }
        }
        self.names.insert(kind, name.to_string());
        Ok(())
    }

    /// Takes a file by its path. A panel brings its boards, which are read from the folder of the panel.
    pub fn load_path(&mut self, path: &Path) -> Result<FileKind, String> {
        let name = file_name(path);
        let (text, _latin1) = read_file(path, self.cfg.max_file_bytes)?;
        let kind = classify(&name, &text).ok_or_else(|| format!("{name}: empty"))?;
        if kind == FileKind::Panel {
            self.load_panel(path, &text)?;
            self.names.insert(FileKind::Panel, name);
        } else {
            self.load_kind(kind, &name, &text)?;
        }
        Ok(kind)
    }

    fn load_panel(&mut self, path: &Path, text: &str) -> Result<(), String> {
        let panel = Panel::parse(text).map_err(|e| self.localize(&e))?;
        let base = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let max = self.cfg.max_file_bytes;
        let profile = self.cfg.profile.clone();
        let load = |file: &str| -> Result<Vec<Part>, String> {
            let (board, _) = read_file(&base.join(file), max)?;
            parse_with(&board, &profile)
        };
        let expanded = panel.expand(&load).map_err(|e| self.localize(&e))?;
        // the panel is kept as it was read, so that a new configuration turns its parts again
        let raw = expanded.parts.clone();
        self.finish_parts(expanded.parts)?;
        self.panel_parts = Some(raw);
        self.parts_text = None;
        self.names.remove(&FileKind::Parts);
        Ok(())
    }

    /// Checks the number of parts, applies the rotation corrections of the configuration and takes the parts.
    fn finish_parts(&mut self, mut parts: Vec<Part>) -> Result<(), String> {
        if parts.len() > self.cfg.max_parts {
            return Err(self.localize(&PnpError::new(
                Code::TooManyParts,
                format!("{} parts, the limit is {}", parts.len(), self.cfg.max_parts),
            )));
        }
        let table = self.cfg.rotation_table().map_err(|e| self.localize(&e))?;
        table.apply(&mut parts);
        self.parts = parts;
        self.excluded.clear();
        self.undo.clear();
        Ok(())
    }

    /// The parts are read again when the configuration changes, because it says how to read the columns and how to turn the parts.
    fn reparse_parts(&mut self) -> Result<(), String> {
        if let Some(text) = self.parts_text.clone() {
            let parts = parse_with(&text, &self.cfg.profile)?;
            self.finish_parts(parts)?;
        } else if let Some(raw) = self.panel_parts.clone() {
            self.finish_parts(raw)?;
        }
        Ok(())
    }

    fn rebuild_catalog(&mut self) -> Result<(), String> {
        let mut table = Catalog::new();
        if let Some(t) = &self.packages_text {
            table.add_packages(t).map_err(|e| self.localize(&e))?;
        }
        if let Some(t) = &self.catalog_text {
            table.add_parts(t).map_err(|e| self.localize(&e))?;
        }
        self.catalog = table;
        Ok(())
    }

    /// How many nozzles the head has, from 1 to 6.
    pub fn set_nozzles(&mut self, n: usize) {
        self.cfg.nozzles = n.clamp(1, 6);
    }

    /// The top speed of both axes, in mm/s, from 1 to 5000.
    pub fn set_speed(&mut self, mm_s: f64) {
        let s = if mm_s.is_finite() {
            mm_s.clamp(1.0, 5000.0)
        } else {
            self.cfg.x_speed_mm_s
        };
        self.cfg.x_speed_mm_s = s;
        self.cfg.y_speed_mm_s = s;
    }

    pub fn set_order(&mut self, order: OrderStrategy) {
        self.cfg.order = order;
    }

    pub fn set_side(&mut self, side: Side) {
        self.options.side = side;
    }

    /// The parts that go in the plan: all but those a fix left out.
    pub fn active_parts(&self) -> Vec<Part> {
        self.parts
            .iter()
            .filter(|p| !self.excluded.contains(&p.reference))
            .cloned()
            .collect()
    }

    /// Whether there is enough to make a plan: parts and feeders.
    pub fn can_plan(&self) -> bool {
        !self.parts.is_empty() && !self.feeders.is_empty()
    }

    /// The width of the board, from its parts, for the bottom side that is mirrored about its middle, when the person has not given one.
    pub fn board_width(&self) -> Option<f64> {
        let (mut lo, mut hi) = (f64::MAX, f64::MIN);
        for p in &self.parts {
            lo = lo.min(p.x);
            hi = hi.max(p.x);
        }
        (lo <= hi).then_some(hi - lo)
    }

    /// The plan, the picks and the checks. The bottom side needs a width; the one of the parts is used when none was set.
    pub fn make_plan(&self) -> Result<Planned, PnpError> {
        let parts = self.active_parts();
        let mut options = self.options;
        if options.side == Side::Bottom && options.board_width_mm.is_none() {
            let (mut lo, mut hi) = (f64::MAX, f64::MIN);
            for p in &parts {
                lo = lo.min(p.x);
                hi = hi.max(p.x);
            }
            if lo <= hi {
                options.origin_x_mm = lo;
                options.board_width_mm = Some(hi - lo);
            }
        }
        let picks = feeding::assign(&parts, &self.feeders, Some(options.side));
        let side_parts: Vec<Part> = parts
            .iter()
            .filter(|p| p.side == options.side)
            .cloned()
            .collect();
        let undefined = self.catalog.undefined(&side_parts);
        let plan = plan_advanced(
            &parts,
            &self.feeders,
            &picks,
            &self.catalog,
            &self.cfg,
            &options,
        )?;
        let checks = preflight(
            &parts,
            options.side,
            &picks,
            &self.catalog,
            &self.cfg,
            Some(&plan),
        );
        Ok(Planned {
            plan,
            picks,
            undefined,
            checks,
            parts,
        })
    }

    /// The load sheet for the slots that were loaded, if there are any.
    pub fn load_sheet(&self) -> Option<LoadSheet> {
        if self.slots.is_empty() || self.parts.is_empty() {
            return None;
        }
        let parts = self.active_parts();
        let (needs, middle) =
            loadsheet::needs(&parts, Some(self.options.side), &self.catalog, &self.cfg);
        Some(loadsheet::assign_slots(&needs, &self.slots, middle))
    }

    /// Keeps the state before an edit, so that `undo` can come back to it.
    pub fn remember(&mut self) {
        if self.undo.len() >= 100 {
            self.undo.remove(0);
        }
        self.undo.push(Snapshot {
            feeders: self.feeders.clone(),
            excluded: self.excluded.clone(),
        });
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Takes back the last edit of the feeders or of the parts left out; false when there is nothing to take back.
    pub fn undo(&mut self) -> bool {
        match self.undo.pop() {
            Some(s) => {
                self.feeders = s.feeders;
                self.excluded = s.excluded;
                true
            }
            None => false,
        }
    }
}
