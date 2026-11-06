// EH3D-PHOENIX-PNP - what the window works on.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The workspace: the files that were loaded (parts, feeders, configuration, tables of packages and parts, panel, slots), the options of the plan, and the plan that comes from them.
//! It holds no window and no clock, so everything here can be tested with text. A file can be given as text with its name, or as a path; the kind of a file is found from its name and
//! its first line.

use crate::catalog::{Catalog, PartProps};
use crate::config::{MachineConfig, OrderStrategy};
use crate::error::{Code, Lang, PnpError};
use crate::feeders::Feeder;
use crate::feeding::{self, FeederSpec, Picks};
use crate::fiducial::{self, Fit};
use crate::loadsheet::{self, LoadSheet, Slot};
use crate::panel::Panel;
use crate::parts::{detect_delimiter, parse_all, Part, Side};
use crate::planner::{plan_advanced, AdvancedPlan, Options};
use crate::preflight::{preflight, Check, Severity, Source};
use std::cell::RefCell;
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
    catalog: Catalog,
    packages_text: Option<String>,
    edited: bool,
}

/// The files, the settings and the edits of the window.
#[derive(Debug, Clone)]
pub struct Workspace {
    pub lang: Lang,
    pub cfg: MachineConfig,
    pub parts: Vec<Part>,
    /// The parts that the table of positions marks as not to be mounted: they are not in `parts` and not in the plan.
    pub dnp: Vec<Part>,
    pub feeders: Vec<FeederSpec>,
    pub catalog: Catalog,
    pub slots: Vec<Slot>,
    pub options: Options,
    /// The references that a fix left out of the plan.
    pub excluded: BTreeSet<String>,
    /// The names (not the folders) of the files that were loaded.
    pub names: BTreeMap<FileKind, String>,
    /// The panel that was loaded, for the tables of panels and boards.
    pub panel: Option<Panel>,
    /// The fiducials of the panel as designed, in the coordinates of the panel.
    pub design_fiducials: Vec<(String, f64, f64)>,
    /// The measured places of the fiducials, as they were given.
    pub measured: Vec<(String, f64, f64)>,
    /// The fit of the measured fiducials to the designed ones, when it was accepted.
    pub fit: Option<Fit>,
    /// Why the fit was refused, in the language of the window.
    pub fit_error: Option<String>,
    parts_text: Option<String>,
    panel_parts: Option<Vec<Part>>,
    packages_text: Option<String>,
    catalog_text: Option<String>,
    undo: Vec<Snapshot>,
    /// An edit is going on (a field is being dragged or typed in): the edits that follow are one step to take back.
    in_edit: bool,
    /// The tables of feeders or packages were changed in the window.
    pub edited: bool,
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
            dnp: Vec::new(),
            feeders: Vec::new(),
            catalog: Catalog::new(),
            slots: Vec::new(),
            options: Options::default(),
            excluded: BTreeSet::new(),
            names: BTreeMap::new(),
            panel: None,
            design_fiducials: Vec::new(),
            measured: Vec::new(),
            fit: None,
            fit_error: None,
            parts_text: None,
            panel_parts: None,
            packages_text: None,
            catalog_text: None,
            undo: Vec::new(),
            in_edit: false,
            edited: false,
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
                let parsed = parse_all(text, &self.cfg.profile)?;
                self.finish_parts(parsed.parts, None)?;
                self.dnp = parsed.dnp;
                self.parts_text = Some(text.to_string());
                self.panel_parts = None;
                self.forget_panel();
                self.names.remove(&FileKind::Panel);
            }
            FileKind::Feeders => {
                let feeders = feeding::from_csv_with(text, self.cfg.tray_fill)
                    .map_err(|e| self.localize(&e))?;
                self.feeders = feeders;
                self.undo.clear();
                self.edited = false;
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
                self.refit();
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
        let left_out: RefCell<Vec<Part>> = RefCell::new(Vec::new());
        let load = |file: &str| -> Result<Vec<Part>, String> {
            let (board, _) = read_file(&base.join(file), max)?;
            let parsed = parse_all(&board, &profile)?;
            left_out.borrow_mut().extend(parsed.dnp);
            Ok(parsed.parts)
        };
        let expanded = panel.expand(&load).map_err(|e| self.localize(&e))?;
        // the panel is kept as it was read, so that a new configuration turns its parts again
        let raw = expanded.parts.clone();
        self.finish_parts(expanded.parts, None)?;
        self.dnp = left_out.into_inner();
        self.panel_parts = Some(raw);
        self.panel = Some(panel);
        self.design_fiducials = expanded.fiducials;
        self.measured.clear();
        self.fit = None;
        self.fit_error = None;
        self.parts_text = None;
        self.names.remove(&FileKind::Parts);
        Ok(())
    }

    /// Checks the number of parts, applies the rotation corrections of the configuration and takes the parts.
    /// `fit` is the correction of the fiducials, for the parts of a panel.
    fn finish_parts(&mut self, mut parts: Vec<Part>, fit: Option<&Fit>) -> Result<(), String> {
        if parts.len() > self.cfg.max_parts {
            return Err(self.localize(&PnpError::new(
                Code::TooManyParts,
                format!("{} parts, the limit is {}", parts.len(), self.cfg.max_parts),
            )));
        }
        if let Some(f) = fit {
            fiducial::correct(&mut parts, &f.transform);
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
            let parsed = parse_all(&text, &self.cfg.profile)?;
            self.finish_parts(parsed.parts, None)?;
            self.dnp = parsed.dnp;
        } else if let Some(raw) = self.panel_parts.clone() {
            let fit = self.fit.clone();
            self.finish_parts(raw, fit.as_ref())?;
        }
        Ok(())
    }

    /// Drops everything that belongs to a panel.
    fn forget_panel(&mut self) {
        self.panel = None;
        self.design_fiducials.clear();
        self.measured.clear();
        self.fit = None;
        self.fit_error = None;
    }

    /// Fits the measured fiducials again with the current configuration.
    fn refit(&mut self) {
        if self.measured.is_empty() {
            self.fit = None;
            self.fit_error = None;
            return;
        }
        match fiducial::solve(
            &self.design_fiducials,
            &self.measured,
            self.cfg.fiducial_mode,
            self.cfg.fiducial_max_residual_mm,
        ) {
            Ok(f) => {
                self.fit = Some(f);
                self.fit_error = None;
            }
            Err(e) => {
                self.fit = None;
                self.fit_error = Some(self.localize(&e));
            }
        }
    }

    /// Sets the measured places of the fiducials of the panel (an empty list takes the correction away) and moves the parts of the panel with the fit. A fit that is refused leaves the
    /// parts where the panel puts them and gives the reason.
    pub fn set_measured(&mut self, marks: Vec<(String, f64, f64)>) -> Result<(), String> {
        if self.panel_parts.is_none() {
            return Err(self.localize(&PnpError::new(
                Code::FiducialBad,
                "there is no panel: the marks are the ones of a panel",
            )));
        }
        self.measured = marks;
        self.refit();
        self.reparse_parts()?;
        match &self.fit_error {
            Some(e) => Err(e.clone()),
            None => Ok(()),
        }
    }

    /// Whether the parts are moved by the fiducials.
    pub fn has_fit(&self) -> bool {
        self.fit.is_some()
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
        let mut checks = preflight(
            &parts,
            options.side,
            &picks,
            &self.catalog,
            &self.cfg,
            Some(&plan),
        );
        for d in self.dnp.iter().filter(|d| d.side == options.side) {
            checks.push(Check {
                severity: Severity::Info,
                source: Source::Part(d.reference.clone()),
                message: "marked as not to be mounted: it is left out of the plan".to_string(),
            });
        }
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
        while self.undo.len() >= self.cfg.undo_limit.max(1) {
            self.undo.remove(0);
        }
        self.undo.push(Snapshot {
            feeders: self.feeders.clone(),
            excluded: self.excluded.clone(),
            catalog: self.catalog.clone(),
            packages_text: self.packages_text.clone(),
            edited: self.edited,
        });
    }

    /// Starts an edit of the tables: the state before it is kept once, however many changes the edit makes before `end_edit`.
    pub fn begin_edit(&mut self) {
        if !self.in_edit {
            self.remember();
            self.in_edit = true;
        }
    }

    /// Ends the edit that `begin_edit` started: the next change is a new step to take back.
    pub fn end_edit(&mut self) {
        self.in_edit = false;
    }

    /// Replaces the feeder at `index` by an edited one. The id cannot be changed. Gives whether anything changed.
    pub fn set_feeder(&mut self, index: usize, mut spec: FeederSpec) -> bool {
        let Some(old) = self.feeders.get(index) else {
            return false;
        };
        spec.base.id = old.base.id.clone();
        feeding::normalize(&mut spec);
        if *old == spec {
            return false;
        }
        self.begin_edit();
        self.feeders[index] = spec;
        self.edited = true;
        true
    }

    /// Adds a feeder with a new id and nothing to hold yet. Gives the index of the feeder.
    pub fn add_feeder(&mut self) -> usize {
        let mut n = self.feeders.len() + 1;
        let id = loop {
            let id = format!("F{n}");
            if self.feeders.iter().all(|f| f.base.id != id) {
                break id;
            }
            n += 1;
        };
        self.begin_edit();
        self.feeders.push(FeederSpec::from_basic(Feeder {
            id,
            value: String::new(),
            package: String::new(),
            x: 0.0,
            y: 0.0,
            count: 0,
        }));
        self.edited = true;
        self.feeders.len() - 1
    }

    /// Takes the feeder at `index` away. Gives whether there was one.
    pub fn remove_feeder(&mut self, index: usize) -> bool {
        if index >= self.feeders.len() {
            return false;
        }
        self.begin_edit();
        self.feeders.remove(index);
        self.edited = true;
        true
    }

    /// Gives a package its properties (a row of the table of packages is made if it had none). Gives whether anything changed.
    pub fn set_package_props(&mut self, package: &str, props: PartProps) -> bool {
        if self.catalog.package(package) == Some(&props) {
            return false;
        }
        self.begin_edit();
        self.catalog.set_package(package, props);
        self.packages_text = Some(self.catalog.packages_csv());
        self.edited = true;
        true
    }

    /// Takes the row of a package away. Gives whether it had one.
    pub fn remove_package_row(&mut self, package: &str) -> bool {
        if self.catalog.package(package).is_none() {
            return false;
        }
        self.begin_edit();
        self.catalog.remove_package(package);
        self.packages_text = Some(self.catalog.packages_csv());
        self.edited = true;
        true
    }

    /// The tables as they are now, when they were changed in the window: the file names and their texts.
    pub fn edited_files(&self) -> Vec<(&'static str, String)> {
        if !self.edited {
            return Vec::new();
        }
        let mut out = vec![("feeders_edited.csv", feeding::to_csv(&self.feeders))];
        if self.packages_text.is_some() {
            out.push(("packages_edited.csv", self.catalog.packages_csv()));
        }
        out
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
                self.catalog = s.catalog;
                self.packages_text = s.packages_text;
                self.edited = s.edited;
                self.in_edit = false;
                true
            }
            None => false,
        }
    }
}
