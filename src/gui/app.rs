// EH3D-PHOENIX-PNP - the state of the window, without the window.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! Everything the window does that is not drawing: load files, make the plan, find the problems, start, pause, resume and step the simulated job, put a problem right, write the
//! outputs. The window calls these and shows the result, so all of it can be tested without a screen.

use super::browse::{self, Browser, Purpose};
use super::cmdpanel::CommandBox;
use super::jobtab::JobFilter;
use super::log::{Level, Log};
use super::nodeview::Bench;
use super::problems::{self, Fix, Problem};
use super::samples::SampleFile;
use super::serverui::ServerUi;
use super::session::Session;
use super::settings::UiSettings;
use super::tables::{CellEdit, FeederColumn, PartColumn, PlanColumn, Selection, SortState};
use super::texts::{tr, Text};
use super::theme::ThemeId;
use super::toolbox::{Tool, Toolbox};
use super::view::{Scene, Viewport};
use super::wizard::{PlanState, Wizard};
use super::workspace::{FileKind, Planned, Workspace};
use crate::catalog::PartProps;
use crate::error::{Code, Lang, PnpError};
use crate::export;
use crate::feeding::FeederSpec;
use crate::fiducial;
use crate::job::{Action, Phase};
use crate::preflight::Severity;
use crate::report::{self, ReportCheck, ReportInput};
use crate::toolvalues;
use eh3d_view::server_panel::Action as ServerAction;
use std::path::{Path, PathBuf};

/// The tabs of the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Job,
    Parts,
    Packages,
    Feeders,
    Panels,
    Boards,
    LoadSheet,
    Machine,
    Tools,
    Problems,
    Log,
}

impl Tab {
    /// The name of the tab on the command line.
    pub fn key(self) -> &'static str {
        match self {
            Tab::Job => "job",
            Tab::Parts => "parts",
            Tab::Packages => "packages",
            Tab::Feeders => "feeders",
            Tab::Panels => "panels",
            Tab::Boards => "boards",
            Tab::LoadSheet => "loadsheet",
            Tab::Machine => "machine",
            Tab::Tools => "tools",
            Tab::Problems => "problems",
            Tab::Log => "log",
        }
    }

    /// The tab that a name on the command line stands for, without regard to case.
    pub fn from_key(text: &str) -> Option<Tab> {
        let t = text.trim().to_ascii_lowercase();
        Tab::ALL.into_iter().find(|tab| tab.key() == t)
    }

    pub const ALL: [Tab; 11] = [
        Tab::Job,
        Tab::Parts,
        Tab::Packages,
        Tab::Feeders,
        Tab::Panels,
        Tab::Boards,
        Tab::LoadSheet,
        Tab::Machine,
        Tab::Tools,
        Tab::Problems,
        Tab::Log,
    ];

    pub fn text(self) -> Text {
        match self {
            Tab::Job => Text::TabJob,
            Tab::Parts => Text::TabParts,
            Tab::Packages => Text::TabPackages,
            Tab::Feeders => Text::TabFeeders,
            Tab::Panels => Text::TabPanels,
            Tab::Boards => Text::TabBoards,
            Tab::LoadSheet => Text::TabLoadSheet,
            Tab::Machine => Text::TabMachine,
            Tab::Tools => Text::TabTools,
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

/// What is typed for the measured place of a fiducial, before it is applied.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FidEdit {
    /// The fiducial was measured and is used in the fit.
    pub on: bool,
    pub x: f64,
    pub y: f64,
}

/// The measured places typed for the fiducials of the panel, in the order of the design: the ones already measured with their places, the others at the designed place.
pub fn fid_buffer(ws: &Workspace) -> Vec<FidEdit> {
    ws.design_fiducials
        .iter()
        .map(|(id, x, y)| match ws.measured.iter().find(|m| m.0 == *id) {
            Some(m) => FidEdit {
                on: true,
                x: m.1,
                y: m.2,
            },
            None => FidEdit {
                on: false,
                x: *x,
                y: *y,
            },
        })
        .collect()
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
    /// What the table of the Job tab is limited to.
    pub job_filter: JobFilter,
    /// The measured places typed for the fiducials of the panel.
    pub fid_edit: Vec<FidEdit>,
    /// The file picker, when it is open.
    pub browser: Option<Browser>,
    /// The name typed for a new package.
    pub new_package: String,
    /// The Tools tab: the forms, the text of each tool and the last result.
    pub tools: Toolbox,
    /// The server panel of the Machine tab: the settings of the server, the state of the link and the login.
    pub server: ServerUi,
    /// The panels of the driver and the assistants: the arguments typed and what each command printed.
    pub commands: CommandBox,
    /// The panels of the slots and of the nodes: the banks of feeders, a head node and a feeder node that are made up, and the holds on them.
    pub bench: Bench,
    /// The files loaded since the window last took them, for the list of recent files of the File menu.
    pub recent_opened: Vec<PathBuf>,
    /// The paths of the files that were loaded, to keep an output from landing on an input.
    paths: Vec<PathBuf>,
    /// The configuration file that was loaded last, where the values of the forms of the tools are kept.
    config_path: Option<PathBuf>,
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
            job_filter: JobFilter::default(),
            fid_edit: Vec::new(),
            browser: None,
            new_package: String::new(),
            tools: Toolbox::new(&crate::config::MachineConfig::default()),
            server: ServerUi::new(&crate::config::MachineConfig::default()),
            commands: CommandBox::default(),
            bench: Bench::new(&crate::config::MachineConfig::default()),
            recent_opened: Vec::new(),
            paths: Vec::new(),
            config_path: None,
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
                    if self.recent_opened.len() < 64 {
                        self.recent_opened.push(path.clone());
                    }
                    if kind == FileKind::Config {
                        self.config_path = Some(path.clone());
                        self.server.reload(&self.ws.cfg);
                    }
                    if kind == FileKind::Banks {
                        // the bench shows the table, keeps its revision and saves back to the file
                        let cfg = self.ws.cfg.clone();
                        self.bench.open_file(path, &cfg);
                    }
                    self.after_load(kind, &name);
                }
                Err(e) => self.log.error(e),
            }
        }
        if loaded > 0 {
            self.fid_edit = fid_buffer(&self.ws);
            self.changed();
        }
        loaded
    }

    /// Loads the files of a sample from their text, the same way.
    pub fn load_sample(&mut self, files: &[SampleFile]) -> usize {
        self.ws.clear();
        self.ws.lang = self.ui.lang;
        self.paths.clear();
        self.config_path = None;
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
        self.fid_edit = fid_buffer(&self.ws);
        self.changed();
        loaded
    }

    fn after_load(&mut self, kind: FileKind, name: &str) {
        self.log.info(format!(
            "{name}: {} {}",
            kind.title(self.ui.lang),
            tr(Text::Loaded, self.ui.lang)
        ));
        if kind == FileKind::Config {
            self.tools.take_defaults(&self.ws.cfg);
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
        self.replan_with(true);
    }

    /// The same without a line in the log: for the edits of a table, which plan again at every change.
    pub fn replan_quiet(&mut self) {
        self.replan_with(false);
    }

    fn replan_with(&mut self, say: bool) {
        self.session.reset();
        self.reported = None;
        self.finish_logged = false;
        match self.ws.make_plan() {
            Ok(p) => {
                self.problems = problems::detect(&p);
                self.scene = Some(Scene::build(&p, &self.ws));
                let pl = &p.plan.plan;
                if say {
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
                }
                self.planned = Some(p);
            }
            Err(e) => {
                if say {
                    self.log.error(e.localized(self.ui.lang));
                }
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

    /// What the person decides for the job that an error stopped: the job goes on at once. The decision is told in the log; an action that does not fit is told as an error.
    pub fn decide(&mut self, action: Action) -> bool {
        let lang = self.ui.lang;
        match self.session.decide(action) {
            Ok(()) => {
                self.reported = None;
                self.log.info(format!(
                    "{} - {}",
                    tr(Text::ErrorTitle, lang),
                    tr(super::jobtab::action_text(action), lang)
                ));
                self.drain_events();
                true
            }
            Err(e) => {
                self.log.error(e.localized(lang));
                false
            }
        }
    }

    /// The findings of the plan and the parts not to be mounted, as the report of a job needs them.
    pub fn report_input(&self) -> ReportInput {
        let side = self.ws.options.side;
        ReportInput {
            dnp: self
                .ws
                .dnp
                .iter()
                .filter(|d| d.side == side)
                .map(|d| d.reference.clone())
                .collect(),
            checks: self
                .problems
                .iter()
                .map(|p| ReportCheck {
                    severity: p.severity.name().to_string(),
                    source: p.about(Lang::En),
                    message: p.line(Lang::En),
                })
                .collect(),
            life: None,
            machine: Some(crate::machsheet::from_config(&self.ws.cfg)),
        }
    }

    /// The work report of the job in the language of the window, for the clipboard; none while there is no job.
    pub fn report_text(&self) -> Option<String> {
        let job = self.session.job()?;
        Some(report::build(job, &self.report_input()).text(self.ui.lang))
    }

    /// Makes up to `n` steps of the job, one after the other (the job is made if there is none), and stops early when it finishes or an error stops it. Gives how many were made.
    pub fn run_steps(&mut self, n: u32) -> u32 {
        let mut made = 0;
        for _ in 0..n {
            if matches!(self.session.phase(), Some(Phase::Finished | Phase::Stopped)) {
                break;
            }
            self.step();
            if !self.session.has_job() {
                break;
            }
            made += 1;
        }
        made
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
        for line in self.session.take_events(self.ui.lang) {
            self.log.push(Level::Info, line);
        }
    }

    fn note_finish(&mut self) {
        if self.session.phase() == Some(Phase::Finished) && !self.finish_logged {
            self.finish_logged = true;
            let (done, total) = self.session.progress();
            let seconds = self.session.machine_seconds();
            let per_hour = if seconds > 0.0 {
                done as f64 * 3600.0 / seconds
            } else {
                0.0
            };
            self.log.info(format!(
                "{} - {done}/{total}, {:.1} s {}, {:.0} {}",
                tr(Text::PhaseFinished, self.ui.lang),
                seconds,
                tr(Text::SumMachineTime, self.ui.lang),
                per_hour,
                tr(Text::PartsPerHour, self.ui.lang)
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

    /// Ends the edit of a table that is going on: the next change is a new step for the undo.
    pub fn end_edit(&mut self) {
        self.ws.end_edit();
    }

    /// Changes a feeder (everything but its id) and plans again. Gives whether anything changed.
    pub fn edit_feeder(&mut self, index: usize, spec: FeederSpec) -> bool {
        if self.ws.set_feeder(index, spec) {
            self.replan_quiet();
            true
        } else {
            false
        }
    }

    /// Adds a feeder and chooses it.
    pub fn add_feeder(&mut self) {
        let i = self.ws.add_feeder();
        let id = self.ws.feeders[i].base.id.clone();
        self.selection.select_feeder(&id);
        self.replan_quiet();
    }

    /// Deletes the feeder at `index`.
    pub fn delete_feeder(&mut self, index: usize) -> bool {
        if self.ws.remove_feeder(index) {
            self.selection.clear();
            self.replan_quiet();
            true
        } else {
            false
        }
    }

    /// Gives a package its properties and plans again.
    pub fn edit_package(&mut self, package: &str, props: PartProps) -> bool {
        if self.ws.set_package_props(package, props) {
            self.replan_quiet();
            true
        } else {
            false
        }
    }

    /// Adds a row for a package that the table does not have yet; it inherits everything until it is edited. The name must be a name that can be used.
    pub fn add_package(&mut self, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 64 || name.chars().any(char::is_control) {
            return false;
        }
        if self.ws.catalog.package(name).is_some() {
            return false;
        }
        self.edit_package(name, PartProps::default())
    }

    /// Takes the row of a package away.
    pub fn delete_package(&mut self, package: &str) -> bool {
        if self.ws.remove_package_row(package) {
            self.replan_quiet();
            true
        } else {
            false
        }
    }

    /// Uses the measured places typed for the fiducials: the parts of the panel are moved by the fit. A fit that is refused is told in the log and the parts stay where the panel
    /// puts them. Gives whether the fit was accepted.
    pub fn apply_fiducials(&mut self) -> bool {
        let marks: Vec<(String, f64, f64)> = self
            .ws
            .design_fiducials
            .iter()
            .zip(&self.fid_edit)
            .filter(|(_, e)| e.on)
            .map(|(d, e)| (d.0.clone(), e.x, e.y))
            .collect();
        if marks.is_empty() {
            return false;
        }
        self.set_marks(marks)
    }

    /// Takes the correction of the fiducials away.
    pub fn clear_fiducials(&mut self) {
        let _ = self.ws.set_measured(Vec::new());
        self.fid_edit = fid_buffer(&self.ws);
        self.changed();
    }

    fn set_marks(&mut self, marks: Vec<(String, f64, f64)>) -> bool {
        let result = self.ws.set_measured(marks);
        self.fid_edit = fid_buffer(&self.ws);
        self.changed();
        match result {
            Ok(()) => {
                if let Some(f) = &self.ws.fit {
                    self.log.info(format!(
                        "{}: {}, {} {:.3} mm, {} {:.3} mm, {} {:.3}",
                        tr(Text::FitSummary, self.ui.lang),
                        f.mode.title(self.ui.lang),
                        tr(Text::ResidualRms, self.ui.lang),
                        f.rms,
                        tr(Text::ResidualMax, self.ui.lang),
                        f.max,
                        tr(Text::FitTurn, self.ui.lang),
                        f.transform.rotation_deg()
                    ));
                }
                for w in self.ws.fit_warnings.clone() {
                    self.log.warn(w);
                }
                true
            }
            Err(e) => {
                self.log.error(e);
                for w in self.ws.fit_warnings.clone() {
                    self.log.warn(w);
                }
                false
            }
        }
    }

    /// Reads the measured places of the fiducials from a file and uses them.
    pub fn load_marks_path(&mut self, path: &Path) -> bool {
        let lang = self.ui.lang;
        let text = match super::workspace::read_file(path, self.ws.cfg.max_file_bytes) {
            Ok((text, _)) => text,
            Err(e) => {
                self.log.error(e);
                return false;
            }
        };
        match fiducial::parse_measured(&text) {
            Ok(marks) => self.set_marks(marks),
            Err(e) => {
                self.log.error(e.localized(lang));
                false
            }
        }
    }

    /// Opens the file picker for a purpose, in the folder it was last in.
    pub fn open_browser(&mut self, purpose: Purpose) {
        let last = (!self.ui.browse_dir.is_empty()).then(|| PathBuf::from(&self.ui.browse_dir));
        let dir = browse::start_dir(last.as_deref());
        self.browser = Some(Browser::new(purpose, dir));
    }

    /// Closes the file picker and remembers the folder it was in.
    pub fn close_browser(&mut self) {
        if let Some(b) = self.browser.take() {
            self.ui.browse_dir = b.dir.display().to_string();
        }
    }

    /// A file was chosen in the picker. Files are loaded and the picker stays open for the next one; the file of measured marks closes it when it was read.
    pub fn browser_pick(&mut self, path: PathBuf) {
        let Some(purpose) = self.browser.as_ref().map(|b| b.purpose) else {
            return;
        };
        match purpose {
            Purpose::Files => {
                self.load_paths(std::slice::from_ref(&path));
            }
            Purpose::Fiducials => {
                if self.load_marks_path(&path) {
                    self.close_browser();
                }
            }
            Purpose::Folder => {}
        }
    }

    /// The folder being looked at is the output folder.
    pub fn browser_use_folder(&mut self) {
        if let Some(b) = &self.browser {
            if b.purpose == Purpose::Folder {
                self.ui.out_dir = b.dir.display().to_string();
            }
        }
        self.close_browser();
    }

    /// Sets the nozzles and the speed of the machine and plans again.
    pub fn set_machine(&mut self, nozzles: usize, speed_mm_s: f64) {
        self.ws.set_nozzles(nozzles);
        self.ws.set_speed(speed_mm_s);
        self.changed();
    }

    /// Sets the slack of the axes of the simulated machine and the way it is dealt with, keeping them in the ranges of the settings, and plans again.
    pub fn set_backlash(&mut self, x_mm: f64, y_mm: f64, mode: crate::motion::BacklashMode) {
        let keep = |v: f64, old: f64| {
            if v.is_finite() {
                v.clamp(0.0, 2.0)
            } else {
                old
            }
        };
        self.ws.cfg.backlash_x_mm = keep(x_mm, self.ws.cfg.backlash_x_mm);
        self.ws.cfg.backlash_y_mm = keep(y_mm, self.ws.cfg.backlash_y_mm);
        self.ws.cfg.backlash_mode = mode;
        self.changed();
    }

    /// Sets the runout of the nozzle tip (0 to 5 mm, and a direction from -180 to 180 degrees) and plans again.
    pub fn set_runout(&mut self, mm: f64, deg: f64) {
        if mm.is_finite() {
            self.ws.cfg.nozzle_runout_mm = mm.clamp(0.0, 5.0);
        }
        if deg.is_finite() {
            self.ws.cfg.nozzle_runout_deg = deg.clamp(-180.0, 180.0);
        }
        self.changed();
    }

    /// Sets the spare of the list of parts to buy (per cent from 0 to 100, parts from 0 to 1000) and the number of boards (1 to 100000).
    pub fn set_spare(&mut self, percent: f64, least: usize, boards: usize) {
        if percent.is_finite() {
            self.ws.cfg.order_spare_percent = percent.clamp(0.0, 100.0);
        }
        self.ws.cfg.order_spare_min = least.min(1000);
        self.tools.boards = boards.clamp(1, 100_000);
    }

    /// Runs the open tool on what was loaded and typed.
    pub fn tools_run(&mut self) {
        let lang = self.ui.lang;
        // the depletion of the feeders takes the state of the job that is under way when no state is given
        if matches!(self.tools.tool, Tool::FeederDeplete | Tool::AsideSheet) {
            self.tools.job_state = self
                .session
                .job()
                .map(|j| j.save_state())
                .unwrap_or_default();
        }
        self.tools.run(&self.ws, lang);
        if !self.tools.error.is_empty() {
            self.log.error(self.tools.error.clone());
        }
    }

    /// Reads the text of the open tool from the file typed in its box.
    pub fn tools_read_file(&mut self) {
        if !self.tools.read_file() {
            if !self.tools.error.is_empty() {
                self.log.error(self.tools.error.clone());
            }
            return;
        }
        self.tools_run();
    }

    /// Reads the second text of the open tool from the file typed in its box.
    pub fn tools_read_second_file(&mut self) {
        if !self.tools.read_second_file() {
            if !self.tools.error.is_empty() {
                self.log.error(self.tools.error.clone());
            }
            return;
        }
        self.tools_run();
    }

    /// Puts what the open tool fits (the camera or the runout) in the configuration and plans again.
    pub fn tools_apply(&mut self) {
        let lang = self.ui.lang;
        if self.tools.apply(&mut self.ws.cfg, lang) {
            self.tools.notice = tr(Text::ToolApplied, lang).to_string();
            self.log.info(self.tools.notice.clone());
            self.changed();
        } else if !self.tools.error.is_empty() {
            self.log.error(self.tools.error.clone());
        }
    }

    /// Puts what the Queue tool said last in the log.
    fn queue_logged(&mut self) {
        let (notice, error) = (
            self.tools.queue.notice.clone(),
            self.tools.queue.error.clone(),
        );
        if !notice.is_empty() {
            self.log.info(notice);
        }
        if !error.is_empty() {
            self.log.error(error);
        }
    }

    /// Reads the queue file of the Queue tool again.
    pub fn queue_refresh(&mut self) {
        let lang = self.ui.lang;
        self.tools.queue.error.clear();
        self.tools.queue.refresh(lang);
        if let Ok(text) = std::fs::read_to_string(self.tools.queue.file.trim()) {
            self.tools.queue_text = text;
        }
        self.queue_logged();
    }

    /// Adds the job of the form of the Queue tool to the queue.
    pub fn queue_add(&mut self) {
        let (lang, relative) = (self.ui.lang, self.ws.cfg.queue_relative_paths);
        self.tools.queue.add(relative, lang);
        self.queue_logged();
    }

    /// Plans the jobs of the queue that are still to be made and says which could be run now.
    pub fn queue_check(&mut self) {
        let lang = self.ui.lang;
        self.tools.queue.check(lang);
        self.queue_logged();
    }

    /// Records that the plan of the selected job, as it is now, was looked at and may be run.
    pub fn queue_confirm(&mut self) {
        let lang = self.ui.lang;
        self.tools.queue.confirm(lang);
        self.queue_logged();
    }

    /// Skips, resets, revokes the confirmation of or takes out the selected job of the queue.
    pub fn queue_edit(&mut self, action: &str) {
        let lang = self.ui.lang;
        self.tools.queue.edit(action, lang);
        self.queue_logged();
    }

    /// The press of the Run button of the Queue tool (the first press asks, unless `queue_gui_ask_run` is off).
    pub fn queue_run(&mut self) {
        let (lang, ask) = (self.ui.lang, self.ws.cfg.queue_gui_ask_run);
        self.tools.queue.press_run(ask, lang);
        self.queue_logged();
    }

    /// Stops the run of the queue.
    pub fn queue_stop(&mut self) {
        let lang = self.ui.lang;
        self.tools.queue.stop(lang);
        self.queue_logged();
    }

    /// Looks whether the run of the queue ended (to be called every frame); gives whether it is still going on.
    pub fn queue_poll(&mut self) -> bool {
        if !self.tools.queue.running() {
            return false;
        }
        let lang = self.ui.lang;
        let running = self.tools.queue.poll(lang);
        if !running {
            self.queue_logged();
        }
        running
    }

    /// Writes the result of the open tool to the output folder.
    pub fn tools_save(&mut self) {
        let lang = self.ui.lang;
        let dir = self.ui.out_dir.clone();
        match self.tools.save(&dir, lang) {
            Some(path) => {
                self.tools.notice = format!("{} {path}", tr(Text::ToolSaved, lang));
                self.log.info(self.tools.notice.clone());
            }
            None => {
                if !self.tools.error.is_empty() {
                    self.log.error(self.tools.error.clone());
                }
            }
        }
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
        let edited = self.ws.edited_files();
        files.extend(edited.iter().map(|(n, t)| (*n, t.clone())));
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

    /// Keeps the values of the forms of the tools (the tolerances, the distance of the near parts, the plans, the runs and the steps) in the configuration file that was loaded last: only
    /// their lines change. The result, or what went wrong, is in the notice or the error of the Tools tab and in the log.
    pub fn keep_tool_values(&mut self) {
        let lang = self.ui.lang;
        self.tools.notice.clear();
        self.tools.error.clear();
        let Some(path) = self.config_path.clone() else {
            self.tools.error = tr(Text::NoConfigToKeep, lang).to_string();
            return;
        };
        let values = self.tools.values();
        match toolvalues::keep(&path, &values) {
            Ok(()) => {
                values.apply(&mut self.ws.cfg);
                let line = format!("{} {}", tr(Text::ValuesKept, lang), path.display());
                self.log.info(line.clone());
                self.tools.notice = line;
            }
            Err(e) => self.tools.error = e.localized(lang),
        }
    }

    /// Does what the server panel asked. When the settings were applied they are in the configuration, and they are kept in the file of settings that was loaded (never the token); with no
    /// file loaded they are only in the configuration of this session.
    pub fn server_action(&mut self, action: ServerAction) {
        let lang = self.ui.lang;
        let Some(applied) = self.server.handle(action, &mut self.ws.cfg) else {
            return;
        };
        let Some(path) = self.config_path.clone() else {
            self.log.info(tr(Text::NoConfigToKeep, lang));
            return;
        };
        match toolvalues::keep_lines(&path, &applied.lines) {
            Ok(()) => self
                .log
                .info(format!("{} {}", tr(Text::ValuesKept, lang), path.display())),
            Err(e) => self.log.error(e),
        }
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
