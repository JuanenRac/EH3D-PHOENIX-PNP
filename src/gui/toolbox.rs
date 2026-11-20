// EH3D-PHOENIX-PNP - the Tools tab of the window, without the window.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! What the Tools tab does besides drawing: the slack of the axes and the runout of the nozzle, the calibration of the camera, the teaching of feeders, the sheet of the parts that have a
//! direction, the check and the order of a bill of materials and the panel of a grid of boards. Each tool reads the text that was typed or read from a file, calls the same library
//! functions as its command (`bom-check`, `bom-order`, `camera-calibrate`, `nozzle-calibrate`, `feeder-teach`, `polarity-sheet`, `panelize`) and gives text; what a tool fits can be
//! applied to the machine, and what it makes can be written to the output folder, never over a file that exists. What the forms start with comes from the configuration.

use super::queuetab::QueueTab;
use super::texts::Text;
use super::toolmore;
use super::workspace::{FileKind, Workspace};
use crate::bom::{compare, order, order_csv, Bom};
use crate::calibrate::{
    calibrate_camera, fit_runout, parse_camera_points, parse_runout_samples, taught_csv,
    teach_file, CameraCalibration, NozzleRunout, Taught,
};
use crate::combined::{
    self, calibrate_combined, parse_combined_touch, resolve_origin, CombinedCalibration,
};
use crate::compare;
use crate::config::MachineConfig;
use crate::error::{Code, Lang, PnpError};
use crate::feedlib;
use crate::fidcheck;
use crate::fiducial;
use crate::inspect;
use crate::nozzlelife::NozzleLife;
use crate::panel::{grid_panel, Grid};
use crate::parts::Side;
use crate::partsdiff;
use crate::pattern::{calibrate_pattern, parse_pattern, PatternCalibration};
use crate::polarity;
use crate::preview;
use crate::queue::Queue;
use crate::report;
use crate::setup::{self, PanelInfo};
use crate::stock::stock_plan;
use crate::toolvalues::ToolValues;
use std::path::Path;

/// The tools of the tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Backlash,
    Runout,
    Camera,
    Feeders,
    Polarity,
    Bom,
    Stock,
    Setup,
    Panel,
    NozzleLife,
    Inspect,
    FidCheck,
    FeederCheck,
    Compare,
    Preview,
    CameraPattern,
    CameraCombined,
    Queue,
    PartsDiff,
    ConfigDiff,
    BoardStats,
    QueueStock,
    FeederDeplete,
    Journal,
    FeederRefill,
    AsideSheet,
    JobDiff,
    FeederSlots,
    Runs,
    ReelRunway,
    NozzleCompare,
    Hotspots,
    QueueChangeover,
    ServerMachine,
}

impl Tool {
    pub const ALL: [Tool; 34] = [
        Tool::Backlash,
        Tool::Runout,
        Tool::Camera,
        Tool::Feeders,
        Tool::Polarity,
        Tool::Bom,
        Tool::Stock,
        Tool::Setup,
        Tool::Panel,
        Tool::NozzleLife,
        Tool::Inspect,
        Tool::FidCheck,
        Tool::FeederCheck,
        Tool::Compare,
        Tool::Preview,
        Tool::CameraPattern,
        Tool::CameraCombined,
        Tool::Queue,
        Tool::PartsDiff,
        Tool::ConfigDiff,
        Tool::BoardStats,
        Tool::QueueStock,
        Tool::FeederDeplete,
        Tool::Journal,
        Tool::FeederRefill,
        Tool::AsideSheet,
        Tool::JobDiff,
        Tool::FeederSlots,
        Tool::Runs,
        Tool::ReelRunway,
        Tool::NozzleCompare,
        Tool::Hotspots,
        Tool::QueueChangeover,
        Tool::ServerMachine,
    ];

    /// The name of the tool on the command line of the window (`--tool`) and in the log.
    pub fn key(self) -> &'static str {
        match self {
            Tool::Backlash => "backlash",
            Tool::Runout => "runout",
            Tool::Camera => "camera",
            Tool::Feeders => "feeders",
            Tool::Polarity => "polarity",
            Tool::Bom => "bom",
            Tool::Stock => "stock",
            Tool::Setup => "setup",
            Tool::Panel => "panel",
            Tool::NozzleLife => "life",
            Tool::Inspect => "inspect",
            Tool::FidCheck => "fiducials",
            Tool::FeederCheck => "feedercheck",
            Tool::Compare => "compare",
            Tool::Preview => "preview",
            Tool::CameraPattern => "pattern",
            Tool::CameraCombined => "combined",
            Tool::Queue => "queue",
            Tool::PartsDiff => "partsdiff",
            Tool::ConfigDiff => "configdiff",
            Tool::BoardStats => "boardstats",
            Tool::QueueStock => "queuestock",
            Tool::FeederDeplete => "deplete",
            Tool::Journal => "journal",
            Tool::FeederRefill => "refill",
            Tool::AsideSheet => "aside",
            Tool::JobDiff => "jobdiff",
            Tool::FeederSlots => "slots",
            Tool::Runs => "runs",
            Tool::ReelRunway => "runway",
            Tool::NozzleCompare => "nozzles",
            Tool::Hotspots => "hotspots",
            Tool::QueueChangeover => "changeover",
            Tool::ServerMachine => "server",
        }
    }

    pub fn from_key(text: &str) -> Option<Tool> {
        let t = text.trim().to_ascii_lowercase();
        Tool::ALL.into_iter().find(|tool| tool.key() == t)
    }

    /// The word that names the tool in the window.
    pub fn text(self) -> Text {
        match self {
            Tool::Backlash => Text::ToolBacklash,
            Tool::Runout => Text::ToolRunout,
            Tool::Camera => Text::ToolCamera,
            Tool::Feeders => Text::ToolFeeders,
            Tool::Polarity => Text::ToolPolarity,
            Tool::Bom => Text::ToolBom,
            Tool::Stock => Text::ToolStock,
            Tool::Setup => Text::ToolSetup,
            Tool::Panel => Text::ToolPanel,
            Tool::NozzleLife => Text::ToolNozzleLife,
            Tool::Inspect => Text::ToolInspect,
            Tool::FidCheck => Text::ToolFidCheck,
            Tool::FeederCheck => Text::ToolFeederCheck,
            Tool::Compare => Text::ToolCompare,
            Tool::Preview => Text::ToolPreview,
            Tool::CameraPattern => Text::ToolCameraPattern,
            Tool::CameraCombined => Text::ToolCameraCombined,
            Tool::Queue => Text::ToolQueue,
            Tool::PartsDiff => Text::ToolPartsDiff,
            Tool::ConfigDiff => Text::ToolConfigDiff,
            Tool::BoardStats => Text::ToolBoardStats,
            Tool::QueueStock => Text::ToolQueueStock,
            Tool::FeederDeplete => Text::ToolFeederDeplete,
            Tool::Journal => Text::ToolJournal,
            Tool::FeederRefill => Text::ToolFeederRefill,
            Tool::AsideSheet => Text::ToolAsideSheet,
            Tool::JobDiff => Text::ToolJobDiff,
            Tool::FeederSlots => Text::ToolFeederSlots,
            Tool::Runs => Text::ToolRuns,
            Tool::ReelRunway => Text::ToolReelRunway,
            Tool::NozzleCompare => Text::ToolNozzleCompare,
            Tool::Hotspots => Text::ToolHotspots,
            Tool::QueueChangeover => Text::ToolQueueChangeover,
            Tool::ServerMachine => Text::ToolServerMachine,
        }
    }

    /// Whether the tool reads a text (the others are settings).
    pub fn reads_text(self) -> bool {
        matches!(
            self,
            Tool::Runout
                | Tool::Camera
                | Tool::Feeders
                | Tool::Bom
                | Tool::Setup
                | Tool::NozzleLife
                | Tool::Inspect
                | Tool::FidCheck
                | Tool::CameraPattern
                | Tool::CameraCombined
                | Tool::Queue
                | Tool::PartsDiff
                | Tool::ConfigDiff
                | Tool::FeederDeplete
                | Tool::FeederRefill
                | Tool::AsideSheet
                | Tool::JobDiff
                | Tool::ServerMachine
        )
    }

    /// Whether the tool reads a second text, beside the first (the tools that compare two files, or a table and a state).
    pub fn reads_second(self) -> bool {
        matches!(
            self,
            Tool::PartsDiff
                | Tool::ConfigDiff
                | Tool::FeederDeplete
                | Tool::FeederRefill
                | Tool::JobDiff
        )
    }

    /// The name of the file that the result is written to, when the tool makes one.
    pub fn output_name(self) -> Option<&'static str> {
        match self {
            Tool::Feeders => Some("feeders_taught.csv"),
            Tool::Polarity => Some("polarity.csv"),
            Tool::Bom => Some("bom_order.csv"),
            Tool::Stock => Some("stock.csv"),
            Tool::Setup => Some("setup_sheet.txt"),
            Tool::Panel => Some("panel.json"),
            Tool::NozzleLife => Some("nozzle_life.csv"),
            Tool::Inspect => Some("part_inspect.csv"),
            Tool::FidCheck => Some("fiducial_residuals.csv"),
            Tool::FeederCheck => Some("feeder_check.txt"),
            Tool::Compare => Some("order_compare.txt"),
            Tool::Preview => Some("plan.svg"),
            Tool::CameraPattern => Some("camera_pattern.cfg"),
            Tool::CameraCombined => Some("camera_combined.cfg"),
            Tool::PartsDiff => Some("parts_diff.csv"),
            Tool::ConfigDiff => Some("config_diff.txt"),
            Tool::BoardStats => Some("board_stats.txt"),
            Tool::QueueStock => Some("queue_stock.csv"),
            Tool::FeederDeplete => Some("feeders_after_job.csv"),
            Tool::Journal => Some("journal.csv"),
            Tool::FeederRefill => Some("feeders_refilled.csv"),
            Tool::AsideSheet => Some("aside_sheet.csv"),
            Tool::JobDiff => Some("job_diff.csv"),
            Tool::FeederSlots => Some("feeders_slots.csv"),
            Tool::Runs => Some("runs.csv"),
            Tool::ReelRunway => Some("reel_runway.csv"),
            Tool::NozzleCompare => Some("nozzle_compare.csv"),
            Tool::Hotspots => Some("hotspots.csv"),
            Tool::QueueChangeover => Some("queue_changeover.csv"),
            Tool::ServerMachine => Some("server_feeders.csv"),
            _ => None,
        }
    }
}

/// The most bytes of a text that a tool reads from a file.
pub const MAX_TOOL_BYTES: u64 = 1_048_576;

/// The form of the panel.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelForm {
    pub cols: usize,
    pub rows: usize,
    pub pitch_x_mm: f64,
    pub pitch_y_mm: f64,
    pub rotation_deg: f64,
    pub fiducial_mm: f64,
    pub board_width_mm: f64,
    pub board_height_mm: f64,
    /// The boards marked as bad, separated by commas, spaces or semicolons.
    pub x_out: String,
    /// The file of the board as the panel names it (the file that was loaded when it is empty).
    pub board_file: String,
}

impl PanelForm {
    pub fn from_config(cfg: &MachineConfig) -> PanelForm {
        PanelForm {
            cols: cfg.panel_cols,
            rows: cfg.panel_rows,
            pitch_x_mm: cfg.panel_pitch_x_mm,
            pitch_y_mm: cfg.panel_pitch_y_mm,
            rotation_deg: 0.0,
            fiducial_mm: cfg.panel_fiducial_mm,
            board_width_mm: cfg.panel_board_width_mm,
            board_height_mm: cfg.panel_board_height_mm,
            x_out: String::new(),
            board_file: String::new(),
        }
    }
}

/// The state of the Tools tab.
#[derive(Debug, Clone)]
pub struct Toolbox {
    pub tool: Tool,
    /// The text of each tool that reads one.
    pub runout_text: String,
    pub camera_text: String,
    pub feeders_text: String,
    pub bom_text: String,
    /// The record of the nozzle tips (the tool of the tips reads it, and so does the setup sheet to say which tip is worn).
    pub life_text: String,
    /// The readings of the parts on the nozzle.
    pub inspect_text: String,
    /// The measured places of the fiducials (the ones that were loaded when it is empty).
    pub fiducials_text: String,
    /// The points of a pattern seen by the camera.
    pub pattern_text: String,
    /// The sightings of a pattern that is fixed to the bed while the head moves.
    pub combined_text: String,
    /// The text of a queue of jobs.
    pub queue_text: String,
    /// The Queue tool: the queue file as a list of jobs, the form of a new job and the buttons.
    pub queue: QueueTab,
    /// A path typed to read the text of the tool from.
    pub path_text: String,
    /// The first and the second table of the parts diff, the two configuration files, and the table of feeders and the saved state of the depletion (a blank first is what is loaded).
    pub parts_first: String,
    pub parts_second: String,
    pub config_first: String,
    pub config_second: String,
    pub deplete_feeders: String,
    pub deplete_state: String,
    /// A path typed to read the second text of the tool from.
    pub second_path_text: String,
    /// The tolerances of the parts diff, the distance and the number of pairs of the board statistics (0 is off) and the steps of the journal (0 is all).
    pub diff_tol_mm: f64,
    pub diff_rot_tol_deg: f64,
    pub near_mm: f64,
    pub near_limit: usize,
    pub journal_steps: usize,
    /// The table of feeders and the table of refills of the reels that were changed, the two files of `job-export` that are compared and the saved state for the sheet of the parts by hand.
    pub refill_feeders: String,
    pub refill_rows: String,
    pub jobdiff_first: String,
    pub jobdiff_second: String,
    pub aside_state: String,
    /// Whether the parts that the job did not get to are on the sheet of the parts by hand.
    pub aside_pending: bool,
    /// The most plans that the search for the places of the feeders makes, and the number of runs and the first seed of the runs.
    pub slots_plans: usize,
    pub runs_count: usize,
    pub runs_seed: u64,
    /// The boards that the reels are wanted to last (the reels that last fewer are marked) and the most nozzles that are compared.
    pub runway_boards: usize,
    pub nozzles_most: usize,
    /// The lines of the Server machine panel (what to do on the machine of the EH3D server, one thing for each line) and whether it talks to a server in memory.
    pub server_script: String,
    pub server_sim: bool,
    /// The saved state of the job that is under way in the Job tab, for the depletion when no state is given (the window fills it before a run).
    pub job_state: String,
    /// The side the sheet of polarity and the stock are limited to (both when none).
    pub polarity_side: Option<Side>,
    pub boards: usize,
    pub panel: PanelForm,
    /// What the last run found, in a few lines, and the text of the file that it makes; `made_by` is the tool that gave them.
    pub info: String,
    pub output: String,
    pub made_by: Option<Tool>,
    /// What went wrong, in the language of the window; empty when nothing did.
    pub error: String,
    /// What was done last (written, applied), in the language of the window; empty when nothing was.
    pub notice: String,
}

fn failed(e: PnpError, lang: Lang) -> String {
    e.localized(lang)
}

/// Whether two paths are the same file.
fn same_file(a: &str, b: &str) -> bool {
    match (Path::new(a).canonicalize(), Path::new(b).canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

impl Toolbox {
    pub fn new(cfg: &MachineConfig) -> Toolbox {
        Toolbox {
            tool: Tool::Backlash,
            runout_text: String::new(),
            camera_text: String::new(),
            feeders_text: String::new(),
            bom_text: String::new(),
            life_text: String::new(),
            inspect_text: String::new(),
            fiducials_text: String::new(),
            pattern_text: String::new(),
            combined_text: String::new(),
            queue_text: String::new(),
            queue: QueueTab::new(),
            path_text: String::new(),
            parts_first: String::new(),
            parts_second: String::new(),
            config_first: String::new(),
            config_second: String::new(),
            deplete_feeders: String::new(),
            deplete_state: String::new(),
            second_path_text: String::new(),
            diff_tol_mm: cfg.parts_diff_tol_mm,
            diff_rot_tol_deg: cfg.parts_diff_rot_tol_deg,
            near_mm: cfg.board_stats_near_mm,
            near_limit: cfg.board_stats_limit as usize,
            journal_steps: cfg.journal_steps as usize,
            refill_feeders: String::new(),
            refill_rows: String::new(),
            jobdiff_first: String::new(),
            jobdiff_second: String::new(),
            aside_state: String::new(),
            aside_pending: false,
            slots_plans: cfg.feeder_slots_max_plans as usize,
            runs_count: cfg.simulate_runs_count as usize,
            runs_seed: cfg.sim_seed,
            runway_boards: 1,
            nozzles_most: 6,
            server_script: String::new(),
            server_sim: false,
            job_state: String::new(),
            polarity_side: None,
            boards: cfg.order_boards,
            panel: PanelForm::from_config(cfg),
            info: String::new(),
            output: String::new(),
            made_by: None,
            error: String::new(),
            notice: String::new(),
        }
    }

    /// Takes the starting values of the forms from the configuration again (when a configuration was loaded).
    pub fn take_defaults(&mut self, cfg: &MachineConfig) {
        self.boards = cfg.order_boards;
        self.runs_seed = cfg.sim_seed;
        self.load_values(&ToolValues::from_config(cfg));
        let (file, x_out) = (
            std::mem::take(&mut self.panel.board_file),
            std::mem::take(&mut self.panel.x_out),
        );
        self.panel = PanelForm::from_config(cfg);
        self.panel.board_file = file;
        self.panel.x_out = x_out;
    }

    /// The values of the forms that are settings of the machine file (`toolvalues`), as they stand now.
    pub fn values(&self) -> ToolValues {
        let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        ToolValues {
            parts_diff_tol_mm: self.diff_tol_mm,
            parts_diff_rot_tol_deg: self.diff_rot_tol_deg,
            board_stats_near_mm: self.near_mm,
            board_stats_limit: count(self.near_limit),
            feeder_slots_max_plans: count(self.slots_plans),
            simulate_runs_count: count(self.runs_count),
            journal_steps: count(self.journal_steps),
        }
    }

    /// Puts the values of the machine file in the forms.
    pub fn load_values(&mut self, v: &ToolValues) {
        self.diff_tol_mm = v.parts_diff_tol_mm;
        self.diff_rot_tol_deg = v.parts_diff_rot_tol_deg;
        self.near_mm = v.board_stats_near_mm;
        self.near_limit = v.board_stats_limit as usize;
        self.slots_plans = v.feeder_slots_max_plans as usize;
        self.runs_count = v.simulate_runs_count as usize;
        self.journal_steps = v.journal_steps as usize;
    }

    /// The text of the tool that is open, for a box to edit.
    pub fn text_mut(&mut self) -> Option<&mut String> {
        match self.tool {
            Tool::Runout => Some(&mut self.runout_text),
            Tool::Camera => Some(&mut self.camera_text),
            Tool::Feeders => Some(&mut self.feeders_text),
            Tool::Bom => Some(&mut self.bom_text),
            Tool::Setup | Tool::NozzleLife => Some(&mut self.life_text),
            Tool::Inspect => Some(&mut self.inspect_text),
            Tool::FidCheck => Some(&mut self.fiducials_text),
            Tool::CameraPattern => Some(&mut self.pattern_text),
            Tool::CameraCombined => Some(&mut self.combined_text),
            Tool::Queue => Some(&mut self.queue_text),
            Tool::PartsDiff => Some(&mut self.parts_first),
            Tool::ConfigDiff => Some(&mut self.config_first),
            Tool::FeederDeplete => Some(&mut self.deplete_feeders),
            Tool::FeederRefill => Some(&mut self.refill_feeders),
            Tool::AsideSheet => Some(&mut self.aside_state),
            Tool::JobDiff => Some(&mut self.jobdiff_first),
            Tool::ServerMachine => Some(&mut self.server_script),
            _ => None,
        }
    }

    /// The second text of the tool that is open, for a box to edit.
    pub fn second_text_mut(&mut self) -> Option<&mut String> {
        match self.tool {
            Tool::PartsDiff => Some(&mut self.parts_second),
            Tool::ConfigDiff => Some(&mut self.config_second),
            Tool::FeederDeplete => Some(&mut self.deplete_state),
            Tool::FeederRefill => Some(&mut self.refill_rows),
            Tool::JobDiff => Some(&mut self.jobdiff_second),
            _ => None,
        }
    }

    /// Clears the result and the messages.
    pub fn clear_result(&mut self) {
        self.info.clear();
        self.output.clear();
        self.made_by = None;
        self.error.clear();
        self.notice.clear();
    }

    /// Reads the text of the tool that is open from the file at `path_text`. A file over `MAX_TOOL_BYTES` is refused, and so is anything that is not text.
    pub fn read_file(&mut self) -> bool {
        let path = self.path_text.trim().to_string();
        self.read_path(&path, false)
    }

    /// Reads the second text of the tool that is open from the file at `second_path_text`, as `read_file` does.
    pub fn read_second_file(&mut self) -> bool {
        let path = self.second_path_text.trim().to_string();
        self.read_path(&path, true)
    }

    fn read_path(&mut self, path: &str, second: bool) -> bool {
        self.error.clear();
        self.notice.clear();
        let path = path.to_string();
        if path.is_empty() {
            return false;
        }
        let read = (|| -> Result<String, String> {
            let meta = std::fs::metadata(&path).map_err(|e| format!("{path}: {e}"))?;
            if meta.len() > MAX_TOOL_BYTES {
                return Err(format!(
                    "{path}: {} bytes, over the {MAX_TOOL_BYTES} that a tool reads",
                    meta.len()
                ));
            }
            let bytes = std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
            match crate::parts::decode_text(bytes) {
                (_, true) => Err(format!("{path}: the file is not UTF-8 or UTF-16 text")),
                (text, false) => Ok(text),
            }
        })();
        match read {
            Ok(text) => match if second {
                self.second_text_mut()
            } else {
                self.text_mut()
            } {
                Some(slot) => {
                    *slot = text;
                    true
                }
                None => false,
            },
            Err(e) => {
                self.error = e;
                false
            }
        }
    }

    /// Runs the tool that is open on the files that were loaded and the text that was typed. The result is in `output` and a failure in `error`.
    pub fn run(&mut self, ws: &Workspace, lang: Lang) {
        self.clear_result();
        let tool = self.tool;
        // what each tool gives: a few lines that say what was found, and the text of the file that it makes
        let result: Result<(String, String), String> = match tool {
            Tool::Backlash => Ok((String::new(), backlash_text(&ws.cfg))),
            Tool::Runout => runout_text(&self.runout_text, lang).map(|(_, i, o)| (i, o)),
            Tool::Camera => camera_text(&self.camera_text, lang).map(|(_, i, o)| (i, o)),
            Tool::Feeders => teach_text(&self.feeders_text, lang),
            Tool::Polarity => Ok(polarity_text(ws, self.polarity_side)),
            Tool::Bom => bom_text(ws, &self.bom_text, self.boards, lang),
            Tool::Stock => stock_text(ws, self.polarity_side, self.boards, lang),
            Tool::Setup => {
                let life = if self.life_text.trim().is_empty() {
                    Ok(None)
                } else {
                    NozzleLife::parse(&self.life_text)
                        .map(Some)
                        .map_err(|e| failed(e, lang))
                };
                life.and_then(|life| {
                    setup_text_with(ws, self.polarity_side, self.boards, life.as_ref(), lang)
                })
            }
            Tool::Panel => panel_text(ws, &self.panel, lang).map(|o| (String::new(), o)),
            Tool::NozzleLife => life_text(&self.life_text, ws.cfg.nozzle_life_warn, lang),
            Tool::Inspect => inspect_text(ws, &self.inspect_text, lang),
            Tool::FidCheck => fiducials_text(ws, &self.fiducials_text, lang),
            Tool::FeederCheck => Ok(feeder_check_text(ws)),
            Tool::Compare => compare_text(ws, lang),
            Tool::Preview => preview_text(ws, lang),
            Tool::CameraPattern => {
                pattern_text(&self.pattern_text, &ws.cfg, lang).map(|(_, i, o)| (i, o))
            }
            Tool::CameraCombined => {
                combined_text(&self.combined_text, &ws.cfg, lang).map(|(_, i, o)| (i, o))
            }
            Tool::Queue => queue_text(&self.queue_text, lang),
            Tool::PartsDiff => toolmore::parts_diff_text(
                ws,
                &self.parts_first,
                &self.parts_second,
                &partsdiff::Settings {
                    tol_mm: self.diff_tol_mm,
                    rot_tol_deg: self.diff_rot_tol_deg,
                },
                lang,
            ),
            Tool::ConfigDiff => {
                toolmore::config_diff_text(&self.config_first, &self.config_second, lang)
            }
            Tool::BoardStats => toolmore::board_stats_text(ws, self.near_mm, self.near_limit, lang),
            Tool::QueueStock => toolmore::queue_stock_text(ws, &self.queue.file, lang),
            Tool::FeederDeplete => toolmore::feeder_deplete_text(
                ws,
                &self.deplete_feeders,
                &self.deplete_state,
                &self.job_state,
                lang,
            ),
            Tool::Journal => toolmore::journal_text(ws, self.journal_steps, lang),
            Tool::FeederRefill => {
                toolmore::feeder_refill_text(ws, &self.refill_feeders, &self.refill_rows, lang)
            }
            Tool::AsideSheet => toolmore::aside_sheet_text(
                ws,
                &self.aside_state,
                &self.job_state,
                self.aside_pending,
                lang,
            ),
            Tool::JobDiff => {
                toolmore::job_diff_text(ws, &self.jobdiff_first, &self.jobdiff_second, lang)
            }
            Tool::FeederSlots => toolmore::feeder_slots_text(ws, self.slots_plans, lang),
            Tool::Runs => toolmore::runs_text(ws, self.runs_count, self.runs_seed, lang),
            Tool::ReelRunway => toolmore::runway_text(ws, self.runway_boards, lang),
            Tool::NozzleCompare => toolmore::nozzle_compare_text(ws, self.nozzles_most, lang),
            Tool::Hotspots => toolmore::hotspots_text(ws, self.runs_count, self.runs_seed, lang),
            Tool::QueueChangeover => toolmore::changeover_text(ws, &self.queue.file, lang),
            Tool::ServerMachine => {
                toolmore::server_machine_text(ws, &self.server_script, self.server_sim, lang)
            }
        };
        match result {
            Ok((info, output)) => {
                self.info = info;
                self.output = output;
                self.made_by = Some(tool);
            }
            Err(e) => self.error = e,
        }
    }

    /// Fits what the open tool fits (the camera or the runout) and puts it in the configuration. Gives whether it did; a failure is in `error`.
    pub fn apply(&mut self, cfg: &mut MachineConfig, lang: Lang) -> bool {
        self.error.clear();
        self.notice.clear();
        match self.tool {
            Tool::Camera => match camera_text(&self.camera_text, lang) {
                Ok((found, _, _)) => {
                    apply_camera(cfg, &found);
                    true
                }
                Err(e) => {
                    self.error = e;
                    false
                }
            },
            Tool::CameraPattern => match pattern_text(&self.pattern_text, cfg, lang) {
                Ok((found, _, _)) => match found.warning(cfg.camera_pattern_max_residual_mm) {
                    None => {
                        apply_pattern(cfg, &found);
                        true
                    }
                    Some(w) => {
                        self.error = w;
                        false
                    }
                },
                Err(e) => {
                    self.error = e;
                    false
                }
            },
            Tool::CameraCombined => match combined_text(&self.combined_text, cfg, lang) {
                Ok((found, _, _)) => match found.warning(cfg.camera_pattern_max_residual_mm) {
                    None => {
                        apply_combined(cfg, &found);
                        true
                    }
                    Some(w) => {
                        self.error = w;
                        false
                    }
                },
                Err(e) => {
                    self.error = e;
                    false
                }
            },
            Tool::Runout => match runout_text(&self.runout_text, lang) {
                Ok((found, _, _)) => {
                    apply_runout(cfg, &found);
                    true
                }
                Err(e) => {
                    self.error = e;
                    false
                }
            },
            _ => false,
        }
    }

    /// Writes the result to `dir` under the name of the tool, never over a file that exists. Gives the path that was written; a failure is in `error`.
    pub fn save(&mut self, dir: &str, lang: Lang) -> Option<String> {
        self.error.clear();
        self.notice.clear();
        let (Some(tool), false) = (self.made_by, self.output.is_empty()) else {
            return None;
        };
        let name = tool.output_name()?;
        let dir = if dir.trim().is_empty() {
            "."
        } else {
            dir.trim()
        };
        if let Err(e) = std::fs::create_dir_all(dir) {
            self.error = format!("{dir}: {e}");
            return None;
        }
        let path = Path::new(dir).join(name).to_string_lossy().into_owned();
        // what was read is never written over, as the commands refuse an output that is also an input
        let inputs = [&self.path_text, &self.second_path_text, &self.queue.file];
        if inputs
            .iter()
            .any(|i| !i.trim().is_empty() && same_file(i.trim(), &path))
        {
            self.error = format!("{path}: the output file is also an input; write to a new file");
            return None;
        }
        match report::write_new(&path, &self.output) {
            Ok(()) => Some(path),
            Err(e) => {
                self.error = failed(e, lang);
                None
            }
        }
    }
}

/// The slack of the axes and the runout as the configuration has them, for the tool of the settings.
pub fn backlash_text(cfg: &MachineConfig) -> String {
    format!(
        "backlash_x_mm = {}\nbacklash_y_mm = {}\nbacklash_mode = {}\nnozzle_runout_mm = {}\nnozzle_runout_deg = {}\n",
        cfg.backlash_x_mm,
        cfg.backlash_y_mm,
        cfg.backlash_mode.name(),
        cfg.nozzle_runout_mm,
        cfg.nozzle_runout_deg
    )
}

/// The runout of the nozzle from the readings in `text`: the fit and the summary with the lines for the configuration. A runout above 5 mm is refused, as `nozzle-calibrate` does.
pub fn runout_text(text: &str, lang: Lang) -> Result<(NozzleRunout, String, String), String> {
    let samples = parse_runout_samples(text).map_err(|e| failed(e, lang))?;
    let found = fit_runout(&samples).map_err(|e| failed(e, lang))?;
    if found.radius_mm > 5.0 {
        return Err(failed(
            PnpError::new(
                Code::CalibrationBad,
                format!(
                    "the runout is {:.3} mm, above the 5 mm that nozzle_runout_mm accepts: check the readings",
                    found.radius_mm
                ),
            ),
            lang,
        ));
    }
    let (summary, lines) = (found.summary(), found.config_lines());
    Ok((found, summary, lines))
}

/// The camera from the sightings in `text`.
pub fn camera_text(text: &str, lang: Lang) -> Result<(CameraCalibration, String, String), String> {
    let points = parse_camera_points(text).map_err(|e| failed(e, lang))?;
    let found = calibrate_camera(&points).map_err(|e| failed(e, lang))?;
    let (summary, lines) = (found.summary(), found.config_lines());
    Ok((found, summary, lines))
}

/// Puts the size of a pixel, the offset, the turn and the mirror that the calibration found in the configuration; the lens is left as it was.
pub fn apply_camera(cfg: &mut MachineConfig, found: &CameraCalibration) {
    cfg.camera.units_per_px = found.model.units_per_px;
    cfg.camera.offset_x_mm = found.model.offset_x_mm;
    cfg.camera.offset_y_mm = found.model.offset_y_mm;
    cfg.camera.rotation_deg = found.model.rotation_deg;
    cfg.camera.flip_x = found.model.flip_x;
    cfg.camera.flip_y = found.model.flip_y;
}

/// The size of a pixel and the bending of the lens from the points of the pattern in `text`, with the pitch of the configuration: the fit, the summary (with the warning when a point is
/// over `camera_pattern_max_residual_mm`) and the lines for the configuration.
pub fn pattern_text(
    text: &str,
    cfg: &MachineConfig,
    lang: Lang,
) -> Result<(PatternCalibration, String, String), String> {
    let points = parse_pattern(text).map_err(|e| failed(e, lang))?;
    let found =
        calibrate_pattern(&points, cfg.camera_pattern_pitch_mm).map_err(|e| failed(e, lang))?;
    let mut summary = found.summary();
    if let Some(w) = found.warning(cfg.camera_pattern_max_residual_mm) {
        summary.push_str("\nwarning: ");
        summary.push_str(&w);
    }
    let lines = found.config_lines();
    Ok((found, summary, lines))
}

/// Puts the size of a pixel and the bending of the lens that the pattern gave in the configuration; the turn, the mirror and the offset are left as they were.
pub fn apply_pattern(cfg: &mut MachineConfig, found: &PatternCalibration) {
    cfg.camera.units_per_px = found.units_per_px;
    cfg.camera.lens_k1 = found.lens_k1.clamp(-0.01, 0.01);
}

/// The camera, the turn of the sheet and the bending of the lens from the sightings in `text` of a pattern fixed to the bed (the pitch and the origin are those of the configuration): the
/// fit, the summary (with the warning when a sighting is over `camera_pattern_max_residual_mm`) and the lines for the configuration. Refused while `camera_combined` is off. With
/// `camera_combined_touch` on the origin is the row `origin,X,Y` of the text.
pub fn combined_text(
    text: &str,
    cfg: &MachineConfig,
    lang: Lang,
) -> Result<(CombinedCalibration, String, String), String> {
    combined::allowed(cfg)?;
    let (rows, file_origin) = parse_combined_touch(text).map_err(|e| failed(e, lang))?;
    let origin = resolve_origin(cfg, cfg.camera_combined_touch, file_origin, None)
        .map_err(|e| failed(e, lang))?;
    let found = calibrate_combined(&rows, cfg.camera_pattern_pitch_mm, origin)
        .map_err(|e| failed(e, lang))?;
    let mut summary = found.summary();
    if let Some(w) = found.warning(cfg.camera_pattern_max_residual_mm) {
        summary.push_str("\nwarning: ");
        summary.push_str(&w);
    }
    let lines = found.config_lines();
    Ok((found, summary, lines))
}

/// Puts the camera and the bending of the lens that the combined calibration found in the configuration. `camera_correct_lens` is left as it was: whether the camera takes the bending out of
/// what it sees is the person's to turn on.
pub fn apply_combined(cfg: &mut MachineConfig, found: &CombinedCalibration) {
    apply_camera(cfg, &found.camera);
    cfg.camera.lens_k1 = found.lens_k1.clamp(-0.01, 0.01);
}

/// Puts the runout that the calibration found in the configuration.
pub fn apply_runout(cfg: &mut MachineConfig, found: &NozzleRunout) {
    cfg.nozzle_runout_mm = found.radius_mm.clamp(0.0, 5.0);
    cfg.nozzle_runout_deg = found.phase_deg.clamp(-180.0, 180.0);
}

/// The feeders taught from the measures in `text`: a line for each, and the columns of the file of feeders.
pub fn teach_text(text: &str, lang: Lang) -> Result<(String, String), String> {
    let rows = teach_file(text).map_err(|e| failed(e, lang))?;
    let info: Vec<String> = rows
        .iter()
        .map(|(id, t)| match t {
            Taught::Strip(s) => format!("{id}: {}", s.summary()),
            Taught::Tray(t) => format!("{id}: {}", t.summary()),
        })
        .collect();
    Ok((info.join("\n"), taught_csv(&rows)))
}

/// The sheet of the parts that have a direction, of the parts of the plan (those a fix left out are not on it): a line that counts them and the CSV.
pub fn polarity_text(ws: &Workspace, side: Option<Side>) -> (String, String) {
    let parts = ws.active_parts();
    let rows: Vec<_> = polarity::sheet(&parts, &ws.cfg.polarity_refs, &ws.cfg.polarity_packages)
        .into_iter()
        .filter(|p| side.is_none_or(|s| p.side == s))
        .collect();
    let total = parts
        .iter()
        .filter(|p| side.is_none_or(|s| p.side == s))
        .count();
    (polarity::summary(&rows, total), polarity::to_csv(&rows))
}

/// The check of the BOM in `text` against the parts of the board (the findings) and the list of parts to buy for `boards` boards with the spare of the configuration (the CSV).
pub fn bom_text(
    ws: &Workspace,
    text: &str,
    boards: usize,
    lang: Lang,
) -> Result<(String, String), String> {
    let bom = Bom::parse(text, ws.cfg.max_parts).map_err(|e| failed(e, lang))?;
    let info = if ws.parts.is_empty() && ws.dnp.is_empty() {
        "(no table of parts is loaded: the BOM is not compared with a board)".to_string()
    } else {
        compare(&bom, &ws.parts, &ws.dnp)
            .text()
            .trim_end()
            .to_string()
    };
    let skipped: Vec<String> = ws.dnp.iter().map(|p| p.reference.clone()).collect();
    let lines = order(
        &bom,
        boards,
        ws.cfg.order_spare_percent,
        ws.cfg.order_spare_min,
        &skipped,
    )
    .map_err(|e| failed(e, lang))?;
    Ok((info, order_csv(&lines)))
}

/// Whether the feeders that are loaded hold enough for `boards` boards of the parts of the plan: the findings and the table as a CSV.
pub fn stock_text(
    ws: &Workspace,
    side: Option<Side>,
    boards: usize,
    lang: Lang,
) -> Result<(String, String), String> {
    let plan =
        stock_plan(&ws.active_parts(), &ws.feeders, side, boards).map_err(|e| failed(e, lang))?;
    Ok((plan.text().trim_end().to_string(), plan.csv()))
}

/// The setup sheet for `boards` boards of the parts of the plan: the lines that say whether the run can be loaded and what is not set (shown on the tab), and the whole sheet in the
/// language of the window (what is written to the output folder, to be printed).
pub fn setup_text(
    ws: &Workspace,
    side: Option<Side>,
    boards: usize,
    lang: Lang,
) -> Result<(String, String), String> {
    setup_text_with(ws, side, boards, None, lang)
}

/// The setup sheet as `setup_text` makes it, with the use of the nozzle tips so far when a record of it is given: the sheet then says which tip is worn and which will be by the end of
/// the run.
pub fn setup_text_with(
    ws: &Workspace,
    side: Option<Side>,
    boards: usize,
    life: Option<&NozzleLife>,
    lang: Lang,
) -> Result<(String, String), String> {
    let parts = ws.active_parts();
    let panel = ws.panel.as_ref().map(|_| PanelInfo {
        fiducials_measured: !ws.measured.is_empty(),
    });
    let sheet = setup::build(&setup::Inputs {
        parts: &parts,
        feeders: &ws.feeders,
        catalog: &ws.catalog,
        cfg: &ws.cfg,
        side,
        boards,
        panel,
        life,
    })
    .map_err(|e| failed(e, lang))?;
    Ok((sheet.summary(lang), sheet.text(lang)))
}

/// The panel file for the form, naming the board by the file that was loaded unless the form names one.
pub fn panel_text(ws: &Workspace, form: &PanelForm, lang: Lang) -> Result<String, String> {
    let board_file = if form.board_file.trim().is_empty() {
        ws.names
            .get(&FileKind::Parts)
            .cloned()
            .unwrap_or_else(|| "board.pos".to_string())
    } else {
        form.board_file.trim().to_string()
    };
    grid_panel(&Grid {
        board_file,
        cols: form.cols,
        rows: form.rows,
        pitch_x_mm: form.pitch_x_mm,
        pitch_y_mm: form.pitch_y_mm,
        rotation_deg: form.rotation_deg,
        fiducial_margin_mm: form.fiducial_mm,
        board_width_mm: form.board_width_mm,
        board_height_mm: form.board_height_mm,
        x_out: form
            .x_out
            .split([',', ';', ' '])
            .filter(|w| !w.is_empty())
            .map(str::to_string)
            .collect(),
    })
    .map_err(|e| failed(e, lang))
}

/// The use of the nozzle tips in `text` (the file that `nozzle-life` keeps): a line for each tip with the ones at or over `warn_at` marked, and the record as a CSV.
pub fn life_text(text: &str, warn_at: u64, lang: Lang) -> Result<(String, String), String> {
    let life = NozzleLife::parse(text).map_err(|e| failed(e, lang))?;
    let mut info = life.text(warn_at).trim_end().to_string();
    for w in life.warning_lines(warn_at) {
        info.push('\n');
        info.push_str(&w);
    }
    Ok((info, life.to_csv()))
}

/// The readings in `text` against the sizes of the packages of the table of packages that was loaded: how many were accepted, refused and could not be judged, and the rows as a CSV.
pub fn inspect_text(ws: &Workspace, text: &str, lang: Lang) -> Result<(String, String), String> {
    let readings = inspect::parse_readings(text).map_err(|e| failed(e, lang))?;
    let table = inspect::SizeTable::from_catalog(&ws.catalog);
    let limits = inspect::Limits {
        size_tol_mm: ws.cfg.part_size_tol_mm,
        angle_tol_deg: ws.cfg.part_angle_tol_deg,
        offset_max_mm: ws.cfg.part_offset_max_mm,
    };
    let rows = inspect::judge(&readings, &table, &limits);
    let (ok, refused, unknown) = inspect::tally(&rows);
    Ok((
        format!("{ok} accepted, {refused} refused, {unknown} without a size to be judged against"),
        inspect::rows_csv(&rows),
    ))
}

/// What the measured marks (`text`, or the ones that were loaded when it is blank) say about the panel against the marks of the design: the report with the residual of each mark and
/// the warnings (a mark over `fiducial_max_residual_mm`, the mark that does not agree, a turn or a scale over its limit), and the residuals as a CSV.
pub fn fiducials_text(ws: &Workspace, text: &str, lang: Lang) -> Result<(String, String), String> {
    if ws.design_fiducials.is_empty() {
        return Err("(no marks of a design are loaded: load a panel or a table of parts that has fiducials)".to_string());
    }
    let measured = if text.trim().is_empty() {
        ws.measured.clone()
    } else {
        fiducial::parse_measured(text).map_err(|e| failed(e, lang))?
    };
    if measured.is_empty() {
        return Err("(no measured marks: type them as id,x,y or load them)".to_string());
    }
    let (a, ids) = fidcheck::analyze_marks(&ws.design_fiducials, &measured, &ws.cfg)
        .map_err(|e| failed(e, lang))?;
    let mut info = fidcheck::report(&a, &ids).trim_end().to_string();
    for w in fidcheck::warnings(&a, &ids, ws.cfg.fiducial_max_residual_mm) {
        info.push_str("\nwarning: ");
        info.push_str(&w);
    }
    Ok((
        info,
        fidcheck::residual_table(&a, &ids, ws.cfg.fiducial_max_residual_mm),
    ))
}

/// Whether the table of feeders that is loaded agrees with itself: the findings (errors first) and how many there are of each.
pub fn feeder_check_text(ws: &Workspace) -> (String, String) {
    if ws.feeders.is_empty() {
        return ("(no table of feeders is loaded)".to_string(), String::new());
    }
    let cfg = &ws.cfg;
    let rules = feedlib::Rules {
        pitch_step_mm: cfg.feeder_pitch_step_mm,
        min_gap_mm: cfg.feeder_min_gap_mm,
        x_min: cfg.limit_x_min_mm,
        x_max: cfg.limit_x_max_mm,
        y_min: cfg.limit_y_min_mm,
        y_max: cfg.limit_y_max_mm,
    };
    let found = feedlib::check(&ws.feeders, Some(&ws.catalog), &rules);
    let (errors, warnings) = feedlib::counts(&found);
    let body = feedlib::text(&found);
    let info = format!(
        "{} feeders: {errors} errors, {warnings} warnings",
        ws.feeders.len()
    );
    let output = if body.trim().is_empty() {
        "no finding\n".to_string()
    } else {
        body
    };
    (info, output)
}

/// The board planned with every order of the places and every way of changing the nozzle tips, side by side, and what the fastest would save: the advice (shown on the tab) and the
/// table (what is written). Nothing is changed in the configuration.
pub fn compare_text(ws: &Workspace, lang: Lang) -> Result<(String, String), String> {
    if !ws.can_plan() {
        return Err(
            "(load a table of parts and a table of feeders to compare the plans)".to_string(),
        );
    }
    let planned = ws.make_plan().map_err(|e| failed(e, lang))?;
    let options = ws.planning_options(&planned.parts);
    let found = compare::compare(
        &planned.parts,
        &ws.feeders,
        &planned.picks,
        &ws.catalog,
        &ws.cfg,
        &options,
    )
    .map_err(|e| failed(e, lang))?;
    Ok((found.advice().trim_end().to_string(), found.text()))
}

/// The picture of the plan as an SVG: a line that counts what is in it, and the file.
pub fn preview_text(ws: &Workspace, lang: Lang) -> Result<(String, String), String> {
    if !ws.can_plan() {
        return Err("(load a table of parts and a table of feeders to draw the plan)".to_string());
    }
    let planned = ws.make_plan().map_err(|e| failed(e, lang))?;
    let picture = preview::svg(&planned.plan, &ws.feeders);
    let info = format!(
        "{} placements, {} bytes of SVG",
        planned.plan.plan.placements.len(),
        picture.len()
    );
    Ok((info, picture))
}

/// The queue of jobs in `text` (the file that `queue-add` and `queue-run` keep): a line for each job with what became of it and how many jobs are in each state, and the file as it
/// would be written (with the checksum). A file that was cut or changed by hand is refused. The window only reads a queue; the commands change it.
pub fn queue_text(text: &str, lang: Lang) -> Result<(String, String), String> {
    let q = Queue::parse(text).map_err(|e| failed(e, lang))?;
    Ok((q.text().trim_end().to_string(), q.to_text()))
}
