// EH3D-PHOENIX-PNP - the state of the window, without the window.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! Everything the window does that is not drawing: load files, make the plan, find the problems, start, pause, resume and step the simulated job, put a problem right, write the
//! outputs. The window calls these and shows the result, so all of it can be tested without a screen.

use super::log::{Level, Log};
use super::problems::{self, Fix, Problem};
use super::samples::SampleFile;
use super::session::Session;
use super::settings::UiSettings;
use super::tables::{CellEdit, FeederColumn, PartColumn, PlanColumn, Selection, SortState};
use super::texts::{tr, Text};
use super::theme::ThemeId;
use super::view::{Scene, Viewport};
use super::wizard::{PlanState, Wizard};
use super::workspace::{FileKind, Planned, Workspace};
use crate::error::{Code, Lang, PnpError};
use crate::export;
use crate::job::Phase;
use crate::preflight::Severity;
use std::path::{Path, PathBuf};

/// The tabs of the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Job,
    Parts,
    Packages,
    Feeders,
    LoadSheet,
    Machine,
    Problems,
    Log,
}

impl Tab {
    pub const ALL: [Tab; 8] = [
        Tab::Job,
        Tab::Parts,
        Tab::Packages,
        Tab::Feeders,
        Tab::LoadSheet,
        Tab::Machine,
        Tab::Problems,
        Tab::Log,
    ];

    pub fn text(self) -> Text {
        match self {
            Tab::Job => Text::TabJob,
            Tab::Parts => Text::TabParts,
            Tab::Packages => Text::TabPackages,
            Tab::Feeders => Text::TabFeeders,
            Tab::LoadSheet => Text::TabLoadSheet,
            Tab::Machine => Text::TabMachine,
            Tab::Problems => Text::TabProblems,
            Tab::Log => Text::TabLog,
        }
    }
}

/// The numbers of the plan for the line above the view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Summary {
    pub parts: usize,
    pub batches: usize,
    pub tip_changes: usize,
    pub travel_mm: f64,
    pub seconds: f64,
}

/// The state of the window.
#[derive(Debug)]
pub struct AppState {
    pub ws: Workspace,
    pub session: Session,
    pub log: Log,
    pub ui: UiSettings,
    pub planned: Option<Planned>,
    pub problems: Vec<Problem>,
    pub scene: Option<Scene>,
    pub selection: Selection,
    pub edit: CellEdit,
    pub wizard: Wizard,
    pub view: Viewport,
    pub tab: Tab,
    /// The text of the box where a path is typed.
    pub path_text: String,
    pub parts_sort: SortState<PartColumn>,
    pub plan_sort: SortState<PlanColumn>,
    pub feeder_sort: SortState<FeederColumn>,
    /// The paths of the files that were loaded, to keep an output from landing on an input.
    paths: Vec<PathBuf>,
    reported: Option<PnpError>,
    finish_logged: bool,
}

impl AppState {
    pub fn new(ui: UiSettings) -> AppState {
        let lang = ui.lang;
        let wizard = Wizard {
            open: !ui.guide_seen,
            ..Wizard::default()
        };
        AppState {
            ws: Workspace::new(lang),
            session: Session::new(4.0),
            log: Log::new(),
            ui,
            planned: None,
            problems: Vec::new(),
            scene: None,
            selection: Selection::default(),
            edit: CellEdit::default(),
            wizard,
            view: Viewport::default(),
            tab: Tab::Job,
            path_text: String::new(),
            parts_sort: SortState::new(PartColumn::Reference),
            plan_sort: SortState::new(PlanColumn::Step),
            feeder_sort: SortState::new(FeederColumn::Id),
            paths: Vec::new(),
            reported: None,
            finish_logged: false,
        }
    }

    pub fn lang(&self) -> Lang {
        self.ui.lang
    }

    pub fn t(&self, text: Text) -> &'static str {
        tr(text, self.ui.lang)
    }

    pub fn set_lang(&mut self, lang: Lang) {
        self.ui.lang = lang;
        self.ws.lang = lang;
    }

    pub fn set_theme(&mut self, theme: ThemeId) {
        self.ui.theme = theme;
    }

    /// Loads files by their paths, one after the other; each one that fails is told in the log and the others still load. Gives how many were loaded. The plan is made again when
    /// something was loaded and there is enough to plan.
    pub fn load_paths(&mut self, paths: &[PathBuf]) -> usize {
        let mut loaded = 0;
        // the configuration and the tables first: they say how the parts are read
        let mut order: Vec<&PathBuf> = paths.iter().collect();
        order.sort_by_key(|p| {
            let name = super::workspace::file_name(p).to_ascii_lowercase();
            if name.ends_with(".cfg") {
                0
            } else if name.contains("package") || name.contains("catalog") {
                1
            } else {
                2
            }
        });
        for path in order {
            let name = super::workspace::file_name(path);
            match self.ws.load_path(path) {
                Ok(kind) => {
                    loaded += 1;
                    self.paths.push(path.clone());
                    self.after_load(kind, &name);
                }
                Err(e) => self.log.error(e),
            }
        }
        if loaded > 0 {
            self.changed();
        }
        loaded
    }

    /// Loads the files of a sample from their text, the same way.
    pub fn load_sample(&mut self, files: &[SampleFile]) -> usize {
        self.ws.clear();
        self.ws.lang = self.ui.lang;
        self.paths.clear();
        let mut loaded = 0;
        for (name, text) in files {
            match self.ws.load_text(name, text) {
                Ok(kind) => {
                    loaded += 1;
                    self.after_load(kind, name);
                }
                Err(e) => self.log.error(e),
            }
        }
        self.changed();
        loaded
    }

    fn after_load(&mut self, kind: FileKind, name: &str) {
        self.log.info(format!(
            "{name}: {} {}",
            kind.key(),
            tr(Text::Loaded, self.ui.lang)
        ));
        if kind == FileKind::Config {
            // the configuration may choose the language and the colours
            self.ui.lang = self.ws.lang;
            if let Some(theme) = ThemeId::from_key(&self.ws.cfg.gui_theme) {
                self.ui.theme = theme;
            }
        }
    }

    /// Something that the plan depends on changed: the job is dropped and the plan is made again when it can be.
    pub fn changed(&mut self) {
        self.session.reset();
        self.reported = None;
        self.finish_logged = false;
        if self.ws.can_plan() {
            self.replan();
        } else {
            self.planned = None;
            self.scene = None;
            self.problems.clear();
        }
    }

    /// Makes the plan, the problems and the drawing again.
    pub fn replan(&mut self) {
        self.session.reset();
        self.reported = None;
        self.finish_logged = false;
        match self.ws.make_plan() {
            Ok(p) => {
                self.problems = problems::detect(&p);
                self.scene = Some(Scene::build(&p, &self.ws));
                let pl = &p.plan.plan;
                self.log.info(format!(
                    "{} {}, {} {}, {:.0} mm {}, {:.0} s",
                    pl.placements.len(),
                    tr(Text::SumParts, self.ui.lang),
                    pl.batches,
                    tr(Text::SumBatches, self.ui.lang),
                    pl.travel_mm,
                    tr(Text::SumTravel, self.ui.lang),
                    pl.seconds
                ));
                self.planned = Some(p);
            }
            Err(e) => {
                self.log.error(e.localized(self.ui.lang));
                self.problems = vec![problems::from_error(&e)];
                self.planned = None;
                self.scene = None;
            }
        }
        if let Some(sel) = self.selection.reference.clone() {
            self.selection.select_part(&sel, self.planned.as_ref());
        }
    }

    /// The numbers of the plan.
    pub fn summary(&self) -> Option<Summary> {
        let p = self.planned.as_ref()?;
        Some(Summary {
            parts: p.plan.plan.placements.len(),
            batches: p.plan.plan.batches,
            tip_changes: p.plan.tip_changes,
            travel_mm: p.plan.plan.travel_mm,
            seconds: p.plan.plan.seconds,
        })
    }

    pub fn has_errors(&self) -> bool {
        self.problems.iter().any(|p| p.severity == Severity::Error)
    }

    pub fn plan_state(&self) -> PlanState {
        if self.planned.is_none() {
            PlanState::NoPlan
        } else if self.has_errors() {
            PlanState::WithErrors
        } else {
            PlanState::Clean
        }
    }

    /// Whether a job can be started: there is a plan and it has no error.
    pub fn can_start(&self) -> bool {
        self.planned.is_some() && !self.has_errors()
    }

    /// Makes the job from the plan if there is none. Gives false (and says why in the log) when it cannot be made.
    fn ensure_job(&mut self) -> bool {
        if self.session.has_job() {
            return true;
        }
        if self.planned.is_none() && self.ws.can_plan() {
            self.replan();
        }
        let Some(p) = &self.planned else {
            return false;
        };
        if self.has_errors() {
            let e = PnpError::new(Code::PreflightFailed, "see the Problems tab");
            self.log.warn(e.localized(self.ui.lang));
            return false;
        }
        let plan = p.plan.clone();
        match self
            .session
            .start(plan, self.ws.feeders.clone(), self.ws.cfg.clone())
        {
            Ok(()) => {
                self.reported = None;
                self.finish_logged = false;
                true
            }
            Err(e) => {
                self.log.error(e.localized(self.ui.lang));
                false
            }
        }
    }

    /// Starts the job, or runs it again after a pause or an error.
    pub fn start(&mut self) {
        if self.ensure_job() {
            self.reported = None;
            self.session.play();
        }
    }

    pub fn pause(&mut self) {
        self.session.pause();
    }

    /// One step, starting a paused job if there is none.
    pub fn step(&mut self) {
        if !self.ensure_job() {
            return;
        }
        if let Some(e) = self.session.step_once() {
            self.report(&e);
        }
        self.drain_events();
        self.note_finish();
    }

    /// Drops the job; the plan stays.
    pub fn reset_job(&mut self) {
        self.session.reset();
        self.reported = None;
        self.finish_logged = false;
    }

    /// Counts the seconds of the window: runs the job at its speed and takes the events of the machine to the log.
    pub fn tick(&mut self, dt_s: f64) {
        let made = self.session.tick(dt_s);
        if made > 0 {
            self.drain_events();
        }
        if let Some(e) = self.session.last_error().cloned() {
            self.report(&e);
        }
        self.note_finish();
    }

    fn report(&mut self, e: &PnpError) {
        if self.reported.as_ref() != Some(e) {
            self.log.error(e.localized(self.ui.lang));
            self.reported = Some(e.clone());
        }
    }

    fn drain_events(&mut self) {
        for line in self.session.take_events() {
            self.log.push(Level::Info, line);
        }
    }

    fn note_finish(&mut self) {
        if self.session.phase() == Some(Phase::Finished) && !self.finish_logged {
            self.finish_logged = true;
            let (done, total) = self.session.progress();
            self.log.info(format!(
                "{} - {done}/{total}, {:.1} s {}",
                tr(Text::PhaseFinished, self.ui.lang),
                self.session.machine_seconds(),
                tr(Text::SumMachineTime, self.ui.lang)
            ));
        }
    }

    /// Puts the problem at `index` right with its fix, and plans again to see whether it went away. Gives whether anything changed.
    pub fn apply_fix(&mut self, index: usize) -> bool {
        let Some(fix) = self.problems.get(index).and_then(|p| p.fix.clone()) else {
            return false;
        };
        self.apply(&fix)
    }

    pub fn apply(&mut self, fix: &Fix) -> bool {
        if problems::apply(&mut self.ws, fix) {
            self.replan();
            true
        } else {
            false
        }
    }

    /// Takes back the last fix or edit of the feeders.
    pub fn undo(&mut self) -> bool {
        if self.ws.undo() {
            self.replan();
            true
        } else {
            false
        }
    }

    /// Sets the count of a feeder (an edit of its table), keeping the old state for the undo.
    pub fn set_feeder_count(&mut self, id: &str, count: u32) -> bool {
        let Some(at) = self.ws.feeders.iter().position(|f| f.base.id == id) else {
            return false;
        };
        if self.ws.feeders[at].base.count == count {
            return false;
        }
        self.ws.remember();
        self.ws.feeders[at].base.count = count;
        self.replan();
        true
    }

    /// Sets the nozzles and the speed of the machine and plans again.
    pub fn set_machine(&mut self, nozzles: usize, speed_mm_s: f64) {
        self.ws.set_nozzles(nozzles);
        self.ws.set_speed(speed_mm_s);
        self.changed();
    }

    pub fn set_side(&mut self, side: crate::parts::Side) {
        self.ws.set_side(side);
        self.changed();
    }

    pub fn set_order(&mut self, order: crate::config::OrderStrategy) {
        self.ws.set_order(order);
        self.changed();
    }

    /// Writes the plan, the report and the load sheet (when there are slots) to the output folder. Gives the folder, or what went wrong. An output never goes over a file that was loaded.
    pub fn save_outputs(&mut self) -> Result<String, String> {
        let Some(p) = &self.planned else {
            return Err(tr(Text::NoPlan, self.ui.lang).to_string());
        };
        let dir = if self.ui.out_dir.trim().is_empty() {
            PathBuf::from(".")
        } else {
            PathBuf::from(self.ui.out_dir.trim())
        };
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let mut files: Vec<(&str, String)> = vec![
            ("plan.json", p.plan.to_json()),
            (
                "report.txt",
                export::report(&p.plan, &p.picks, &p.undefined),
            ),
        ];
        if let Some(sheet) = self.ws.load_sheet() {
            files.push(("load_sheet.csv", sheet.to_csv()));
            files.push(("load_sheet_feeders.csv", sheet.to_feeders_csv()));
        }
        for (name, _) in &files {
            let target = dir.join(name);
            if self.is_input(&target) {
                return Err(format!(
                    "{name}: the output file is also an input; choose another folder"
                ));
            }
        }
        for (name, text) in &files {
            let target = dir.join(name);
            let tmp = dir.join(format!("{name}.tmp"));
            std::fs::write(&tmp, text)
                .and_then(|()| std::fs::rename(&tmp, &target))
                .map_err(|e| {
                    let _ = std::fs::remove_file(&tmp);
                    format!("{name}: {e}")
                })?;
        }
        self.log.info(format!(
            "{}: {}",
            tr(Text::Saved, self.ui.lang),
            dir.display()
        ));
        Ok(dir.display().to_string())
    }

    fn is_input(&self, target: &Path) -> bool {
        let Ok(t) = target.canonicalize() else {
            return false;
        };
        self.paths
            .iter()
            .any(|p| p.canonicalize().is_ok_and(|c| c == t))
    }
}
