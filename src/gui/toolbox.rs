// EH3D-PHOENIX-PNP - the Tools tab of the window, without the window.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! What the Tools tab does besides drawing: the slack of the axes and the runout of the nozzle, the calibration of the camera, the teaching of feeders, the sheet of the parts that have a
//! direction, the check and the order of a bill of materials and the panel of a grid of boards. Each tool reads the text that was typed or read from a file, calls the same library
//! functions as its command (`bom-check`, `bom-order`, `camera-calibrate`, `nozzle-calibrate`, `feeder-teach`, `polarity-sheet`, `panelize`) and gives text; what a tool fits can be
//! applied to the machine, and what it makes can be written to the output folder, never over a file that exists. What the forms start with comes from the configuration.

use super::texts::Text;
use super::workspace::{FileKind, Workspace};
use crate::bom::{compare, order, order_csv, Bom};
use crate::calibrate::{
    calibrate_camera, fit_runout, parse_camera_points, parse_runout_samples, taught_csv,
    teach_file, CameraCalibration, NozzleRunout, Taught,
};
use crate::config::MachineConfig;
use crate::error::{Code, Lang, PnpError};
use crate::panel::{grid_panel, Grid};
use crate::parts::Side;
use crate::polarity;
use crate::report;
use crate::setup::{self, PanelInfo};
use crate::stock::stock_plan;
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
}

impl Tool {
    pub const ALL: [Tool; 9] = [
        Tool::Backlash,
        Tool::Runout,
        Tool::Camera,
        Tool::Feeders,
        Tool::Polarity,
        Tool::Bom,
        Tool::Stock,
        Tool::Setup,
        Tool::Panel,
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
        }
    }

    /// Whether the tool reads a text (the others are settings).
    pub fn reads_text(self) -> bool {
        matches!(
            self,
            Tool::Runout | Tool::Camera | Tool::Feeders | Tool::Bom
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
    /// A path typed to read the text of the tool from.
    pub path_text: String,
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

impl Toolbox {
    pub fn new(cfg: &MachineConfig) -> Toolbox {
        Toolbox {
            tool: Tool::Backlash,
            runout_text: String::new(),
            camera_text: String::new(),
            feeders_text: String::new(),
            bom_text: String::new(),
            path_text: String::new(),
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
        let (file, x_out) = (
            std::mem::take(&mut self.panel.board_file),
            std::mem::take(&mut self.panel.x_out),
        );
        self.panel = PanelForm::from_config(cfg);
        self.panel.board_file = file;
        self.panel.x_out = x_out;
    }

    /// The text of the tool that is open, for a box to edit.
    pub fn text_mut(&mut self) -> Option<&mut String> {
        match self.tool {
            Tool::Runout => Some(&mut self.runout_text),
            Tool::Camera => Some(&mut self.camera_text),
            Tool::Feeders => Some(&mut self.feeders_text),
            Tool::Bom => Some(&mut self.bom_text),
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
        self.error.clear();
        self.notice.clear();
        let path = self.path_text.trim().to_string();
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
            std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))
        })();
        match read {
            Ok(text) => match self.text_mut() {
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
            Tool::Setup => setup_text(ws, self.polarity_side, self.boards, lang),
            Tool::Panel => panel_text(ws, &self.panel, lang).map(|o| (String::new(), o)),
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
        life: None,
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
