//! The configuration of the machine and of the planner: a text file of `key = value` lines, the same format as the other programs of the suite use. Every key is optional and keeps the
//! default when it is missing; a key that is not known, written twice, not a valid value or outside its range is an error that names the line. The numbers that decide how the machine
//! moves, how often it tries again or what is safe live here and never in the code.

use crate::camera::CameraModel;
use crate::catalog::Defaults;
use crate::error::{Code, Lang, PnpError};
use crate::feeding::TrayFill;
use crate::fiducial::FitMode;
use crate::import::{ImportUnit, Settings as ImportSettings};
use crate::parts::{BottomRotation, PositionColumns, Profile};
use crate::tips::{TipChoice, TipRule, MAX_RULE_TIPS, MAX_TIP_RULES};
use std::collections::HashSet;

/// The version of the schema of the file that this program writes and reads; a file with a higher number is refused.
pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_CONFIG_BYTES: usize = 262_144;
const MAX_LINE_CHARS: usize = 4096;

/// Every key that the file knows, in the order of the example `examples/machine.cfg`.
pub const KEYS: [&str; 118] = [
    "schema_version",
    "language",
    "max_file_bytes",
    "max_parts",
    "nozzles",
    "speed_mm_s",
    "x_speed_mm_s",
    "y_speed_mm_s",
    "x_accel_mm_s2",
    "y_accel_mm_s2",
    "x_jerk_mm_s3",
    "y_jerk_mm_s3",
    "pick_s",
    "place_s",
    "feed_s",
    "tip_change_s",
    "rotation_deg_s",
    "nozzle_angle_min_deg",
    "nozzle_angle_max_deg",
    "pre_rotate",
    "safe_z_mm",
    "board_top_z_mm",
    "z_min_mm",
    "z_max_mm",
    "limit_x_min_mm",
    "limit_x_max_mm",
    "limit_y_min_mm",
    "limit_y_max_mm",
    "feed_retries",
    "pick_retries",
    "place_retries",
    "max_placement_attempts",
    "error_policy",
    "fault_window",
    "fault_limit",
    "fault_action",
    "order",
    "tip_strategy",
    "two_opt",
    "two_opt_max_points",
    "two_opt_passes",
    "rank_step",
    "component_origin",
    "default_height_mm",
    "default_through_board_mm",
    "default_bin",
    "default_tip",
    "default_rank",
    "default_tape_mm",
    "tip_changer_x_mm",
    "tip_changer_y_mm",
    "rotation",
    "header_reference",
    "header_value",
    "header_package",
    "header_x",
    "header_y",
    "header_rotation",
    "header_side",
    "header_dnp",
    "header_fitted",
    "dnp_yes_words",
    "dnp_no_words",
    "position",
    "bottom_rotation",
    "sim_seed",
    "sim_feed_fail_rate",
    "sim_pick_fail_rate",
    "sim_place_fail_rate",
    "gui_steps_per_s",
    "gui_theme",
    "gui_part_size_mm",
    "fiducial_mode",
    "fiducial_max_residual_mm",
    "fiducial_words",
    "fiducial_refs",
    "tray_fill",
    "undo_limit",
    "tip_opt_max_evals",
    "tip_opt_time_limit_s",
    "sim_part_fail",
    "sim_feeder_fail",
    "align",
    "pick_tolerance_mm",
    "align_passes",
    "align_settle_s",
    "sim_camera_fail_rate",
    "sim_align_dx_mm",
    "sim_align_dy_mm",
    "sim_align_deg",
    "sim_align_noise_mm",
    "vision",
    "vacuum",
    "vacuum_suck_s",
    "vacuum_blow_s",
    "vacuum_free_kpa",
    "vacuum_held_kpa",
    "vacuum_pick_min_kpa",
    "vacuum_release_max_kpa",
    "sim_vacuum_noise_kpa",
    "sim_vacuum_miss_rate",
    "sim_vacuum_stuck_rate",
    "actuator",
    "camera_units_per_px",
    "camera_offset_x_mm",
    "camera_offset_y_mm",
    "camera_rotation_deg",
    "camera_flip_x",
    "camera_flip_y",
    "camera_lens_scale",
    "camera_lens_k1",
    "head",
    "tip_compat",
    "tip_compat_mode",
    "recipe_max_bytes",
    "recipe_max_rules",
    "import_unit",
    "import_bottom_mirror_width_mm",
];

/// The order in which the places are visited.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OrderStrategy {
    /// Always to the nearest place that is left.
    #[default]
    Nearest,
    /// Kind by kind (value and package), each kind nearest-first.
    Part,
    /// From the lowest parts to the highest, and kind by kind inside a height.
    HeightPart,
    /// Feeder by feeder, each feeder nearest-first.
    Pick,
    /// Nozzle tip by nozzle tip, each nearest-first.
    Tips,
    /// By reference, with no attempt to shorten the way.
    None,
}

impl OrderStrategy {
    pub fn parse(text: &str) -> Option<OrderStrategy> {
        match text {
            "nearest" => Some(OrderStrategy::Nearest),
            "part" => Some(OrderStrategy::Part),
            "height_part" => Some(OrderStrategy::HeightPart),
            "pick" => Some(OrderStrategy::Pick),
            "tips" => Some(OrderStrategy::Tips),
            "none" => Some(OrderStrategy::None),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            OrderStrategy::Nearest => "nearest",
            OrderStrategy::Part => "part",
            OrderStrategy::HeightPart => "height_part",
            OrderStrategy::Pick => "pick",
            OrderStrategy::Tips => "tips",
            OrderStrategy::None => "none",
        }
    }
}

/// What the planner does about the nozzle tips of the parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TipStrategy {
    /// Parts that need the same tip are brought together, so the tip is changed as few times as possible.
    #[default]
    Minimize,
    /// The tips are changed whenever the planned order asks for another one.
    AsPlanned,
    /// The parts are brought together by tip, and the order of the tips is searched for the shortest way and time, counting the trip to the tip changer.
    Optimize,
}

/// What the job does when a part cannot be picked or placed after all the retries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ErrorPolicy {
    /// The job stops at once.
    #[default]
    Alert,
    /// The part is put aside and the job goes on; the parts put aside are listed at the end.
    Defer,
}

/// What the job does when a feeder has `fault_limit` faults among its last `fault_window` picks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FaultAction {
    /// The feeder is switched off and the parts it held are taken from another feeder of the same kind, if there is one.
    #[default]
    Disable,
    /// The job stops with `PHOENIX-PNP-047` and the person decides: try the step again, leave the part out, put it aside or switch the feeder off.
    Alert,
}

impl FaultAction {
    pub fn name(self) -> &'static str {
        match self {
            FaultAction::Disable => "disable",
            FaultAction::Alert => "alert",
        }
    }
}

/// An on/off device of the simulated machine (a valve, a light, a pump) and the seconds it takes to answer.
#[derive(Debug, Clone, PartialEq)]
pub struct ActuatorSpec {
    pub name: String,
    pub delay_s: f64,
}

/// The most actuators a machine can declare.
pub const MAX_ACTUATORS: usize = 32;

/// The most heads a machine can declare.
pub const MAX_HEADS: usize = 4;

/// One head of the machine: its nozzles, where it is set with respect to the frame of the machine and its camera. A field of the camera that the line leaves out is the one of the
/// camera of the configuration (`camera_*`).
#[derive(Debug, Clone, PartialEq)]
pub struct HeadSpec {
    pub name: String,
    /// How many nozzles the head carries, from 1 to 6.
    pub nozzles: usize,
    /// Where the head is with respect to the carriage: to put its first nozzle over a place the carriage goes to that place less this offset (millimetres).
    pub offset_x_mm: f64,
    pub offset_y_mm: f64,
    /// Where the axis of the nozzle is in the picture of this head's camera.
    pub camera_x_mm: Option<f64>,
    pub camera_y_mm: Option<f64>,
    pub camera_rotation_deg: Option<f64>,
    pub camera_units_per_px: Option<f64>,
    pub camera_lens_scale: Option<f64>,
}

impl HeadSpec {
    /// The head of a machine that declares none: the nozzles of `nozzles`, no offset and the camera of the configuration.
    pub fn implicit(nozzles: usize) -> HeadSpec {
        HeadSpec {
            name: "head".to_string(),
            nozzles,
            offset_x_mm: 0.0,
            offset_y_mm: 0.0,
            camera_x_mm: None,
            camera_y_mm: None,
            camera_rotation_deg: None,
            camera_units_per_px: None,
            camera_lens_scale: None,
        }
    }

    /// The camera of the head: `base` with what the head says changed.
    pub fn camera(&self, base: &CameraModel) -> CameraModel {
        CameraModel {
            units_per_px: self.camera_units_per_px.unwrap_or(base.units_per_px),
            offset_x_mm: self.camera_x_mm.unwrap_or(base.offset_x_mm),
            offset_y_mm: self.camera_y_mm.unwrap_or(base.offset_y_mm),
            rotation_deg: self.camera_rotation_deg.unwrap_or(base.rotation_deg),
            lens_scale: self.camera_lens_scale.unwrap_or(base.lens_scale),
            ..*base
        }
    }
}

/// Where the simulated camera takes what it reports about a part from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VisionMode {
    /// The camera reports the offset it was given, with the error of `sim_align_noise_mm`.
    #[default]
    Off,
    /// The camera draws a picture of the part (an image made in memory) and the vision pipeline measures it.
    Pipeline,
}

impl VisionMode {
    pub fn name(self) -> &'static str {
        match self {
            VisionMode::Off => "off",
            VisionMode::Pipeline => "pipeline",
        }
    }
}

/// Where the point of a part is, for a footprint of the CAD file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ComponentOrigin {
    /// The origin of the footprint, as the position file gives it.
    #[default]
    CadOrigin,
    /// The centre of the pads: the correction of the table of parts is applied.
    PadsCenter,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MachineConfig {
    pub schema_version: u32,
    pub language: Lang,
    pub max_file_bytes: u64,
    pub max_parts: usize,
    pub nozzles: usize,
    pub x_speed_mm_s: f64,
    pub y_speed_mm_s: f64,
    pub x_accel_mm_s2: f64,
    pub y_accel_mm_s2: f64,
    pub x_jerk_mm_s3: f64,
    pub y_jerk_mm_s3: f64,
    pub pick_s: f64,
    pub place_s: f64,
    pub feed_s: f64,
    pub tip_change_s: f64,
    pub rotation_deg_s: f64,
    pub nozzle_angle_min_deg: f64,
    pub nozzle_angle_max_deg: f64,
    pub pre_rotate: bool,
    pub safe_z_mm: f64,
    pub board_top_z_mm: f64,
    pub z_min_mm: f64,
    pub z_max_mm: f64,
    pub limit_x_min_mm: f64,
    pub limit_x_max_mm: f64,
    pub limit_y_min_mm: f64,
    pub limit_y_max_mm: f64,
    pub feed_retries: u32,
    pub pick_retries: u32,
    pub place_retries: u32,
    pub max_placement_attempts: u32,
    pub error_policy: ErrorPolicy,
    pub fault_window: u32,
    pub fault_limit: u32,
    /// What happens when the limit is reached.
    pub fault_action: FaultAction,
    pub order: OrderStrategy,
    pub tip_strategy: TipStrategy,
    pub two_opt: bool,
    pub two_opt_max_points: usize,
    pub two_opt_passes: usize,
    pub rank_step: i32,
    pub component_origin: ComponentOrigin,
    pub default_height_mm: f64,
    pub default_through_board_mm: f64,
    pub default_bin: u8,
    pub default_tip: String,
    pub default_rank: i32,
    pub default_tape_mm: f64,
    pub tip_changer: Option<(f64, f64)>,
    pub rotation_rules: Vec<(String, f64)>,
    pub profile: Profile,
    pub sim_seed: u64,
    pub sim_feed_fail_rate: f64,
    pub sim_pick_fail_rate: f64,
    pub sim_place_fail_rate: f64,
    /// The window: how many steps of the simulated job it makes in a second.
    pub gui_steps_per_s: f64,
    /// The window: the colours, one of `phoenix`, `light` and `contrast`.
    pub gui_theme: String,
    /// The window: the side of the square drawn for a part whose package is not known, in millimetres.
    pub gui_part_size_mm: f64,
    /// The fit of the fiducials of a panel: a similarity, an affine fit, or the one that fits the number of marks.
    pub fiducial_mode: FitMode,
    /// A fiducial that is further than this from where the fit puts it makes the fit refused, in millimetres.
    pub fiducial_max_residual_mm: f64,
    /// A part whose value or package holds one of these words (lower case) is a mark of its board and is not placed.
    pub fiducial_words: Vec<String>,
    /// A part whose reference is one of these prefixes (lower case) followed by a number (`FID1`) is a mark of its board and is not placed.
    pub fiducial_refs: Vec<String>,
    /// The order of the cells of a tray whose row says nothing about it.
    pub tray_fill: TrayFill,
    /// The window: how many edits of the tables can be taken back.
    pub undo_limit: usize,
    /// The search for the best order of the nozzle tips stops after this many orders have been tried.
    pub tip_opt_max_evals: usize,
    /// ... or after this many seconds of the clock, whichever comes first (only the number of orders tried gives the same plan on every computer).
    pub tip_opt_time_limit_s: f64,
    /// Chances of failure by package: `(pattern, pick, place)`; the first rule that matches the package of a part is the one used. They add to the general chances.
    pub sim_part_fail: Vec<(String, f64, f64)>,
    /// Chances that a pick from a feeder fails: `(feeder id, pick)`. They add to the general chances.
    pub sim_feeder_fail: Vec<(String, f64)>,
    /// Look at the part with the camera between the pick and the place and correct the place by what it sees (off by default: a job then does what it always did).
    pub align: bool,
    /// A part whose position the camera finds further than this from the nozzle (millimetres) is thrown away and picked again, as the error policy says.
    pub pick_tolerance_mm: f64,
    /// How many times the camera looks at the part and the correction is made again, from 1 to 5.
    pub align_passes: u32,
    /// The seconds to wait after the pick, and again before each further look, for the part to settle on the nozzle.
    pub align_settle_s: f64,
    /// The simulated camera: the chance (0 to 1) that a look gives no reading.
    pub sim_camera_fail_rate: f64,
    /// The simulated camera: how far (millimetres) and how much turned (degrees) it finds every part from the nozzle, before any correction.
    pub sim_align_dx_mm: f64,
    pub sim_align_dy_mm: f64,
    pub sim_align_deg: f64,
    /// The simulated camera: a random error of every reading, from minus to plus this many millimetres on each axis (taken from the seed).
    pub sim_align_noise_mm: f64,
    /// Where the simulated camera takes its readings from: `off` (the offset it was given) or `pipeline` (the measure of a picture of the part, see `vision`).
    pub vision: VisionMode,
    /// Check the vacuum after every pick (a part must show on the nozzle) and after every place (the part must be gone); off by default: a job then does what it always did.
    pub vacuum: bool,
    /// The seconds the pump sucks before the vacuum is read after a pick, and the seconds of blowing before it is read after a place.
    pub vacuum_suck_s: f64,
    pub vacuum_blow_s: f64,
    /// The simulated vacuum, in kilopascals below the air: with the nozzle open and with a part held.
    pub vacuum_free_kpa: f64,
    pub vacuum_held_kpa: f64,
    /// A level at or above this after a pick says that the part is held; a level at or below `vacuum_release_max_kpa` after a place says that the part is gone.
    pub vacuum_pick_min_kpa: f64,
    pub vacuum_release_max_kpa: f64,
    /// A random error of each reading, from minus to plus this many kilopascals (taken from the seed).
    pub sim_vacuum_noise_kpa: f64,
    /// The chance (0 to 1) that a pick leaves no part on the nozzle, and that a place leaves the part stuck to it.
    pub sim_vacuum_miss_rate: f64,
    pub sim_vacuum_stuck_rate: f64,
    /// The on/off devices of the machine, in the order of the file.
    pub actuators: Vec<ActuatorSpec>,
    /// How the camera is set on the machine and what its lens does (the identity by default).
    pub camera: CameraModel,
    /// The heads of the machine, in the order of the file; none means one head with `nozzles` nozzles.
    pub heads: Vec<HeadSpec>,
    /// The nozzle tips that each kind of package accepts, in the order of the file (the first rule that matches a package is the one used).
    pub tip_rules: Vec<TipRule>,
    /// What the planner prefers when a package accepts several tips.
    pub tip_choice: TipChoice,
    /// The most bytes a file of a recipe may have.
    pub recipe_max_bytes: u64,
    /// The most rules (of package, feeder and tip) a recipe may have.
    pub recipe_max_rules: usize,
    /// The unit of the numbers of a position file that gives none (the import of a centroid, EAGLE or KiCad file).
    pub import_unit: ImportUnit,
    /// When above 0, the X of the parts of the bottom side of an imported file becomes this width minus X; 0 leaves them as the file has them.
    pub import_bottom_mirror_width_mm: f64,
}

/// The most rules of failure by package or by feeder.
pub const MAX_FAIL_RULES: usize = 1000;

impl Default for MachineConfig {
    fn default() -> Self {
        MachineConfig {
            schema_version: SCHEMA_VERSION,
            language: Lang::En,
            max_file_bytes: 16_777_216,
            max_parts: crate::parts::MAX_PARTS,
            nozzles: 1,
            x_speed_mm_s: 150.0,
            y_speed_mm_s: 150.0,
            x_accel_mm_s2: 2000.0,
            y_accel_mm_s2: 2000.0,
            x_jerk_mm_s3: 0.0,
            y_jerk_mm_s3: 0.0,
            pick_s: 0.4,
            place_s: 0.4,
            feed_s: 0.2,
            tip_change_s: 3.0,
            rotation_deg_s: 720.0,
            nozzle_angle_min_deg: -360.0,
            nozzle_angle_max_deg: 360.0,
            pre_rotate: true,
            safe_z_mm: 5.0,
            board_top_z_mm: 0.0,
            z_min_mm: -10.0,
            z_max_mm: 50.0,
            limit_x_min_mm: -2000.0,
            limit_x_max_mm: 2000.0,
            limit_y_min_mm: -2000.0,
            limit_y_max_mm: 2000.0,
            feed_retries: 1,
            pick_retries: 2,
            place_retries: 1,
            max_placement_attempts: 3,
            error_policy: ErrorPolicy::Alert,
            fault_window: 6,
            fault_limit: 3,
            fault_action: FaultAction::Disable,
            order: OrderStrategy::Nearest,
            tip_strategy: TipStrategy::Minimize,
            two_opt: true,
            two_opt_max_points: 2000,
            two_opt_passes: 10,
            rank_step: 10,
            component_origin: ComponentOrigin::CadOrigin,
            default_height_mm: 1.0,
            default_through_board_mm: 0.0,
            default_bin: 0,
            default_tip: String::new(),
            default_rank: 0,
            default_tape_mm: 8.0,
            tip_changer: None,
            rotation_rules: Vec::new(),
            profile: Profile::default(),
            sim_seed: 1,
            sim_feed_fail_rate: 0.0,
            sim_pick_fail_rate: 0.0,
            sim_place_fail_rate: 0.0,
            gui_steps_per_s: 4.0,
            gui_theme: "phoenix".to_string(),
            gui_part_size_mm: 2.0,
            fiducial_mode: FitMode::Auto,
            fiducial_max_residual_mm: 0.5,
            fiducial_words: vec!["fiducial".to_string()],
            fiducial_refs: vec!["fid".to_string(), "fd".to_string()],
            tray_fill: TrayFill::Rows,
            undo_limit: 100,
            tip_opt_max_evals: 20_000,
            tip_opt_time_limit_s: 1.0,
            sim_part_fail: Vec::new(),
            sim_feeder_fail: Vec::new(),
            align: false,
            pick_tolerance_mm: 0.5,
            align_passes: 1,
            align_settle_s: 0.1,
            sim_camera_fail_rate: 0.0,
            sim_align_dx_mm: 0.0,
            sim_align_dy_mm: 0.0,
            sim_align_deg: 0.0,
            sim_align_noise_mm: 0.0,
            vision: VisionMode::Off,
            vacuum: false,
            vacuum_suck_s: 0.08,
            vacuum_blow_s: 0.05,
            vacuum_free_kpa: 8.0,
            vacuum_held_kpa: 62.0,
            vacuum_pick_min_kpa: 40.0,
            vacuum_release_max_kpa: 20.0,
            sim_vacuum_noise_kpa: 0.0,
            sim_vacuum_miss_rate: 0.0,
            sim_vacuum_stuck_rate: 0.0,
            actuators: Vec::new(),
            camera: CameraModel::default(),
            heads: Vec::new(),
            tip_rules: Vec::new(),
            tip_choice: TipChoice::FewestChanges,
            recipe_max_bytes: 65_536,
            recipe_max_rules: 64,
            import_unit: ImportUnit::Mm,
            import_bottom_mirror_width_mm: 0.0,
        }
    }
}

pub(crate) fn bad(line: usize, key: &str, v: &str, what: &str) -> PnpError {
    PnpError::new(
        Code::ConfigBadValue,
        format!("line {line}: {key} '{v}' is not {what}"),
    )
}

pub(crate) fn out_of_range(line: usize, key: &str, v: &str, lo: f64, hi: f64) -> PnpError {
    PnpError::new(
        Code::ConfigOutOfRange,
        format!("line {line}: {key} {v} is not from {lo} to {hi}"),
    )
}

pub(crate) fn num(line: usize, key: &str, v: &str, lo: f64, hi: f64) -> Result<f64, PnpError> {
    let x: f64 = v
        .parse()
        .ok()
        .filter(|x: &f64| x.is_finite())
        .ok_or_else(|| bad(line, key, v, "a number"))?;
    if x < lo || x > hi {
        return Err(out_of_range(line, key, v, lo, hi));
    }
    Ok(x)
}

pub(crate) fn int(line: usize, key: &str, v: &str, lo: i64, hi: i64) -> Result<i64, PnpError> {
    let x: i64 = v.parse().map_err(|_| bad(line, key, v, "a whole number"))?;
    if x < lo || x > hi {
        return Err(out_of_range(line, key, v, lo as f64, hi as f64));
    }
    Ok(x)
}

fn flag(line: usize, key: &str, v: &str) -> Result<bool, PnpError> {
    match v.to_ascii_lowercase().as_str() {
        "yes" | "true" | "on" | "1" => Ok(true),
        "no" | "false" | "off" | "0" => Ok(false),
        _ => Err(bad(line, key, v, "yes or no")),
    }
}

fn names(line: usize, key: &str, v: &str) -> Result<Vec<String>, PnpError> {
    let items: Vec<String> = v
        .split(',')
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    if items.len() > 50 || items.iter().any(|s| s.chars().count() > 64) {
        return Err(bad(
            line,
            key,
            v,
            "a list of at most 50 names of at most 64 characters",
        ));
    }
    Ok(items)
}

/// A head: `NAME : NOZZLES : OFFSET_X : OFFSET_Y : CAMERA_X : CAMERA_Y : CAMERA_ROT : CAMERA_UNITS : CAMERA_LENS`; from the third field on they may be left out or empty.
fn parse_head(line: usize, key: &str, v: &str) -> Result<HeadSpec, PnpError> {
    let fields: Vec<&str> = v.split(':').map(str::trim).collect();
    let name = fields.first().copied().unwrap_or("");
    let named = !name.is_empty()
        && name.chars().count() <= 32
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if fields.len() < 2 || fields.len() > 9 || !named {
        return Err(bad(
            line,
            key,
            v,
            "NAME : NOZZLES [: OFFSET_X : OFFSET_Y : CAMERA_X : CAMERA_Y : CAMERA_ROT : CAMERA_UNITS : CAMERA_LENS]",
        ));
    }
    let optional = |i: usize, lo: f64, hi: f64| -> Result<Option<f64>, PnpError> {
        match fields.get(i) {
            Some(f) if !f.is_empty() => num(line, key, f, lo, hi).map(Some),
            _ => Ok(None),
        }
    };
    let units = optional(7, 0.0, 10.0)?;
    if units.is_some_and(|u| u > 0.0 && u < 0.001) {
        return Err(out_of_range(line, key, fields[7], 0.001, 10.0));
    }
    Ok(HeadSpec {
        name: name.to_string(),
        nozzles: int(line, key, fields[1], 1, 6)? as usize,
        offset_x_mm: optional(2, -2000.0, 2000.0)?.unwrap_or(0.0),
        offset_y_mm: optional(3, -2000.0, 2000.0)?.unwrap_or(0.0),
        camera_x_mm: optional(4, -500.0, 500.0)?,
        camera_y_mm: optional(5, -500.0, 500.0)?,
        camera_rotation_deg: optional(6, -180.0, 180.0)?,
        camera_units_per_px: units,
        camera_lens_scale: optional(8, 0.5, 2.0)?,
    })
}

impl MachineConfig {
    /// What the import of a position file takes from the configuration: the unit, the mirror of the bottom side and the rule for its angle.
    pub fn import_settings(&self) -> ImportSettings {
        ImportSettings {
            unit: self.import_unit,
            mirror_width_mm: self.import_bottom_mirror_width_mm,
            bottom_rotation: self.profile.bottom_rotation,
        }
    }

    /// The heads of the machine: the ones the file declares, or one head with `nozzles` nozzles.
    pub fn head_list(&self) -> Vec<HeadSpec> {
        if self.heads.is_empty() {
            vec![HeadSpec::implicit(self.nozzles)]
        } else {
            self.heads.clone()
        }
    }

    /// Reads a configuration. A missing key keeps its default; the result is validated as a whole (see `validate`).
    pub fn parse(text: &str) -> Result<MachineConfig, PnpError> {
        if text.len() > MAX_CONFIG_BYTES {
            return Err(PnpError::new(
                Code::FileTooBig,
                format!("the configuration is larger than {MAX_CONFIG_BYTES} bytes"),
            ));
        }
        let mut cfg = MachineConfig::default();
        let mut seen: HashSet<String> = HashSet::new();
        for (i, raw) in text.trim_start_matches('\u{feff}').lines().enumerate() {
            let line = i + 1;
            let t = raw.trim();
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            if t.chars().count() > MAX_LINE_CHARS {
                return Err(PnpError::new(
                    Code::ConfigSyntax,
                    format!("line {line}: longer than {MAX_LINE_CHARS} characters"),
                ));
            }
            let Some((k, v)) = t.split_once('=') else {
                return Err(PnpError::new(
                    Code::ConfigSyntax,
                    format!("line {line}: '{t}' is not key = value"),
                ));
            };
            let (key, value) = (k.trim().to_ascii_lowercase(), v.trim());
            let repeats = [
                "rotation",
                "sim_part_fail",
                "sim_feeder_fail",
                "actuator",
                "head",
                "tip_compat",
            ];
            if !repeats.contains(&key.as_str()) && !seen.insert(key.clone()) {
                return Err(PnpError::new(
                    Code::ConfigDuplicateKey,
                    format!("line {line}: '{key}' is written twice"),
                ));
            }
            cfg.set(&key, value, line)?;
        }
        cfg.validate()?;
        Ok(cfg)
    }

    fn set(&mut self, key: &str, v: &str, line: usize) -> Result<(), PnpError> {
        let n = |lo: f64, hi: f64| num(line, key, v, lo, hi);
        match key {
            "schema_version" => {
                let s = int(line, key, v, 1, 1_000_000)? as u32;
                if s > SCHEMA_VERSION {
                    return Err(PnpError::new(
                        Code::ConfigSchemaNewer,
                        format!("line {line}: schema_version {s}, this program reads up to {SCHEMA_VERSION}"),
                    ));
                }
                self.schema_version = s;
            }
            "language" => {
                self.language = Lang::from_code(v)
                    .ok_or_else(|| bad(line, key, v, "one of en es de fr it ja zh pt"))?;
            }
            "max_file_bytes" => {
                self.max_file_bytes = int(line, key, v, 1024, 1_073_741_824)? as u64
            }
            "max_parts" => {
                self.max_parts = int(line, key, v, 1, crate::parts::MAX_PARTS as i64)? as usize
            }
            "nozzles" => self.nozzles = int(line, key, v, 1, 6)? as usize,
            "speed_mm_s" => {
                let s = n(1.0, 5000.0)?;
                self.x_speed_mm_s = s;
                self.y_speed_mm_s = s;
            }
            "x_speed_mm_s" => self.x_speed_mm_s = n(1.0, 5000.0)?,
            "y_speed_mm_s" => self.y_speed_mm_s = n(1.0, 5000.0)?,
            "x_accel_mm_s2" => self.x_accel_mm_s2 = n(1.0, 100_000.0)?,
            "y_accel_mm_s2" => self.y_accel_mm_s2 = n(1.0, 100_000.0)?,
            "x_jerk_mm_s3" => self.x_jerk_mm_s3 = n(0.0, 10_000_000.0)?,
            "y_jerk_mm_s3" => self.y_jerk_mm_s3 = n(0.0, 10_000_000.0)?,
            "pick_s" => self.pick_s = n(0.0, 60.0)?,
            "place_s" => self.place_s = n(0.0, 60.0)?,
            "feed_s" => self.feed_s = n(0.0, 60.0)?,
            "tip_change_s" => self.tip_change_s = n(0.0, 600.0)?,
            "rotation_deg_s" => self.rotation_deg_s = n(1.0, 100_000.0)?,
            "nozzle_angle_min_deg" => self.nozzle_angle_min_deg = n(-720.0, 0.0)?,
            "nozzle_angle_max_deg" => self.nozzle_angle_max_deg = n(0.0, 720.0)?,
            "pre_rotate" => self.pre_rotate = flag(line, key, v)?,
            "safe_z_mm" => self.safe_z_mm = n(-1000.0, 1000.0)?,
            "board_top_z_mm" => self.board_top_z_mm = n(-1000.0, 1000.0)?,
            "z_min_mm" => self.z_min_mm = n(-1000.0, 1000.0)?,
            "z_max_mm" => self.z_max_mm = n(-1000.0, 1000.0)?,
            "limit_x_min_mm" => self.limit_x_min_mm = n(-100_000.0, 100_000.0)?,
            "limit_x_max_mm" => self.limit_x_max_mm = n(-100_000.0, 100_000.0)?,
            "limit_y_min_mm" => self.limit_y_min_mm = n(-100_000.0, 100_000.0)?,
            "limit_y_max_mm" => self.limit_y_max_mm = n(-100_000.0, 100_000.0)?,
            "feed_retries" => self.feed_retries = int(line, key, v, 0, 10)? as u32,
            "pick_retries" => self.pick_retries = int(line, key, v, 0, 10)? as u32,
            "place_retries" => self.place_retries = int(line, key, v, 0, 10)? as u32,
            "max_placement_attempts" => {
                self.max_placement_attempts = int(line, key, v, 1, 20)? as u32
            }
            "error_policy" => {
                self.error_policy = match v.to_ascii_lowercase().as_str() {
                    "alert" => ErrorPolicy::Alert,
                    "defer" => ErrorPolicy::Defer,
                    _ => return Err(bad(line, key, v, "alert or defer")),
                }
            }
            "fault_window" => self.fault_window = int(line, key, v, 1, 100)? as u32,
            "fault_limit" => self.fault_limit = int(line, key, v, 0, 100)? as u32,
            "fault_action" => {
                self.fault_action = match v.to_ascii_lowercase().as_str() {
                    "disable" => FaultAction::Disable,
                    "alert" => FaultAction::Alert,
                    _ => return Err(bad(line, key, v, "disable or alert")),
                }
            }
            "order" => {
                self.order = OrderStrategy::parse(&v.to_ascii_lowercase()).ok_or_else(|| {
                    bad(
                        line,
                        key,
                        v,
                        "one of nearest part height_part pick tips none",
                    )
                })?
            }
            "tip_strategy" => {
                self.tip_strategy = match v.to_ascii_lowercase().as_str() {
                    "minimize" => TipStrategy::Minimize,
                    "as_planned" => TipStrategy::AsPlanned,
                    "optimize" => TipStrategy::Optimize,
                    _ => return Err(bad(line, key, v, "minimize, as_planned or optimize")),
                }
            }
            "two_opt" => self.two_opt = flag(line, key, v)?,
            "two_opt_max_points" => {
                self.two_opt_max_points = int(line, key, v, 3, 20_000)? as usize
            }
            "two_opt_passes" => self.two_opt_passes = int(line, key, v, 1, 100)? as usize,
            "rank_step" => self.rank_step = int(line, key, v, 1, 1000)? as i32,
            "component_origin" => {
                self.component_origin = match v.to_ascii_lowercase().as_str() {
                    "cad_origin" => ComponentOrigin::CadOrigin,
                    "pads_center" => ComponentOrigin::PadsCenter,
                    _ => return Err(bad(line, key, v, "cad_origin or pads_center")),
                }
            }
            "default_height_mm" => self.default_height_mm = n(0.0, 100.0)?,
            "default_through_board_mm" => self.default_through_board_mm = n(0.0, 20.0)?,
            "default_bin" => self.default_bin = int(line, key, v, 0, 4)? as u8,
            "default_tip" => {
                if v.chars().count() > 64 || v.chars().any(char::is_control) {
                    return Err(bad(line, key, v, "a name of at most 64 characters"));
                }
                self.default_tip = v.to_string();
            }
            "default_rank" => self.default_rank = int(line, key, v, -1000, 1000)? as i32,
            "default_tape_mm" => self.default_tape_mm = n(1.0, 100.0)?,
            "tip_changer_x_mm" => {
                let x = n(-100_000.0, 100_000.0)?;
                self.tip_changer = Some((x, self.tip_changer.map_or(f64::NAN, |c| c.1)));
            }
            "tip_changer_y_mm" => {
                let y = n(-100_000.0, 100_000.0)?;
                self.tip_changer = Some((self.tip_changer.map_or(f64::NAN, |c| c.0), y));
            }
            "rotation" => {
                let Some((pattern, deg)) = v.rsplit_once(':') else {
                    return Err(PnpError::new(
                        Code::RotationRule,
                        format!("line {line}: '{v}' is not PATTERN : DEGREES"),
                    ));
                };
                let deg: f64 = deg.trim().parse().map_err(|_| {
                    PnpError::new(
                        Code::RotationRule,
                        format!("line {line}: '{}' is not an angle", deg.trim()),
                    )
                })?;
                self.rotation_rules.push((pattern.trim().to_string(), deg));
                if self.rotation_rules.len() > crate::rotation::RotationTable::MAX_RULES {
                    return Err(PnpError::new(
                        Code::RotationRule,
                        format!("line {line}: too many rules"),
                    ));
                }
            }
            "header_reference" => self.profile.extra_reference = names(line, key, v)?,
            "header_value" => self.profile.extra_value = names(line, key, v)?,
            "header_package" => self.profile.extra_package = names(line, key, v)?,
            "header_x" => self.profile.extra_x = names(line, key, v)?,
            "header_y" => self.profile.extra_y = names(line, key, v)?,
            "header_rotation" => self.profile.extra_rotation = names(line, key, v)?,
            "header_side" => self.profile.extra_side = names(line, key, v)?,
            "header_dnp" => self.profile.extra_dnp = names(line, key, v)?,
            "header_fitted" => self.profile.extra_fitted = names(line, key, v)?,
            "dnp_yes_words" => self.profile.extra_yes = names(line, key, v)?,
            "dnp_no_words" => self.profile.extra_no = names(line, key, v)?,
            "position" => {
                self.profile.position = match v.to_ascii_lowercase().as_str() {
                    "any" => PositionColumns::Any,
                    "center" => PositionColumns::Center,
                    "origin" => PositionColumns::Origin,
                    _ => return Err(bad(line, key, v, "any, center or origin")),
                }
            }
            "bottom_rotation" => {
                self.profile.bottom_rotation = match v.to_ascii_lowercase().as_str() {
                    "keep" => BottomRotation::Keep,
                    "mirror" => BottomRotation::Mirror,
                    "half_turn" => BottomRotation::HalfTurn,
                    _ => return Err(bad(line, key, v, "keep, mirror or half_turn")),
                }
            }
            "sim_seed" => self.sim_seed = int(line, key, v, 0, i64::MAX)? as u64,
            "sim_feed_fail_rate" => self.sim_feed_fail_rate = n(0.0, 1.0)?,
            "sim_pick_fail_rate" => self.sim_pick_fail_rate = n(0.0, 1.0)?,
            "sim_place_fail_rate" => self.sim_place_fail_rate = n(0.0, 1.0)?,
            "gui_steps_per_s" => self.gui_steps_per_s = n(0.1, 1000.0)?,
            "gui_theme" => {
                let name = v.to_ascii_lowercase();
                if !["phoenix", "light", "contrast"].contains(&name.as_str()) {
                    return Err(bad(line, key, v, "phoenix, light or contrast"));
                }
                self.gui_theme = name;
            }
            "gui_part_size_mm" => self.gui_part_size_mm = n(0.2, 50.0)?,
            "fiducial_mode" => {
                self.fiducial_mode = FitMode::parse(&v.to_ascii_lowercase())
                    .ok_or_else(|| bad(line, key, v, "auto, similarity or affine"))?
            }
            "fiducial_max_residual_mm" => self.fiducial_max_residual_mm = n(0.001, 100.0)?,
            "fiducial_words" => self.fiducial_words = names(line, key, v)?,
            "fiducial_refs" => self.fiducial_refs = names(line, key, v)?,
            "undo_limit" => self.undo_limit = int(line, key, v, 1, 1000)? as usize,
            "tip_opt_max_evals" => {
                self.tip_opt_max_evals = int(line, key, v, 1, 1_000_000)? as usize
            }
            "tip_opt_time_limit_s" => self.tip_opt_time_limit_s = n(0.01, 60.0)?,
            "sim_part_fail" => {
                let fields: Vec<&str> = v.split(':').map(str::trim).collect();
                if fields.len() != 3 || fields[0].is_empty() || fields[0].chars().count() > 128 {
                    return Err(bad(line, key, v, "PATTERN : PICK : PLACE"));
                }
                let pick = num(line, key, fields[1], 0.0, 1.0)?;
                let place = num(line, key, fields[2], 0.0, 1.0)?;
                if self.sim_part_fail.len() >= MAX_FAIL_RULES {
                    return Err(bad(line, key, v, "one of at most 1000 rules"));
                }
                self.sim_part_fail
                    .push((fields[0].to_string(), pick, place));
            }
            "sim_feeder_fail" => {
                let fields: Vec<&str> = v.split(':').map(str::trim).collect();
                if fields.len() != 2 || fields[0].is_empty() || fields[0].chars().count() > 64 {
                    return Err(bad(line, key, v, "FEEDER : PICK"));
                }
                let pick = num(line, key, fields[1], 0.0, 1.0)?;
                if self.sim_feeder_fail.len() >= MAX_FAIL_RULES {
                    return Err(bad(line, key, v, "one of at most 1000 rules"));
                }
                self.sim_feeder_fail.push((fields[0].to_string(), pick));
            }
            "align" => self.align = flag(line, key, v)?,
            "pick_tolerance_mm" => self.pick_tolerance_mm = n(0.001, 50.0)?,
            "align_passes" => self.align_passes = int(line, key, v, 1, 5)? as u32,
            "align_settle_s" => self.align_settle_s = n(0.0, 10.0)?,
            "sim_camera_fail_rate" => self.sim_camera_fail_rate = n(0.0, 1.0)?,
            "sim_align_dx_mm" => self.sim_align_dx_mm = n(-50.0, 50.0)?,
            "sim_align_dy_mm" => self.sim_align_dy_mm = n(-50.0, 50.0)?,
            "sim_align_deg" => self.sim_align_deg = n(-180.0, 180.0)?,
            "sim_align_noise_mm" => self.sim_align_noise_mm = n(0.0, 10.0)?,
            "vision" => {
                self.vision = match v.to_ascii_lowercase().as_str() {
                    "off" => VisionMode::Off,
                    "pipeline" => VisionMode::Pipeline,
                    _ => return Err(bad(line, key, v, "off or pipeline")),
                }
            }
            "vacuum" => self.vacuum = flag(line, key, v)?,
            "vacuum_suck_s" => self.vacuum_suck_s = n(0.0, 10.0)?,
            "vacuum_blow_s" => self.vacuum_blow_s = n(0.0, 10.0)?,
            "vacuum_free_kpa" => self.vacuum_free_kpa = n(0.0, 101.0)?,
            "vacuum_held_kpa" => self.vacuum_held_kpa = n(0.0, 101.0)?,
            "vacuum_pick_min_kpa" => self.vacuum_pick_min_kpa = n(0.0, 101.0)?,
            "vacuum_release_max_kpa" => self.vacuum_release_max_kpa = n(0.0, 101.0)?,
            "sim_vacuum_noise_kpa" => self.sim_vacuum_noise_kpa = n(0.0, 50.0)?,
            "sim_vacuum_miss_rate" => self.sim_vacuum_miss_rate = n(0.0, 1.0)?,
            "sim_vacuum_stuck_rate" => self.sim_vacuum_stuck_rate = n(0.0, 1.0)?,
            "actuator" => {
                let fields: Vec<&str> = v.split(':').map(str::trim).collect();
                let name = fields.first().copied().unwrap_or("");
                let named = !name.is_empty()
                    && name.chars().count() <= 32
                    && name
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
                if fields.len() != 2 || !named {
                    return Err(bad(
                        line,
                        key,
                        v,
                        "NAME : DELAY_S (a name of letters, digits, _ and -)",
                    ));
                }
                let delay_s = num(line, key, fields[1], 0.0, 10.0)?;
                if self.actuators.iter().any(|a| a.name == name) {
                    return Err(bad(line, key, v, "a name that no other actuator has"));
                }
                if self.actuators.len() >= MAX_ACTUATORS {
                    return Err(bad(line, key, v, "one of at most 32 actuators"));
                }
                self.actuators.push(ActuatorSpec {
                    name: name.to_string(),
                    delay_s,
                });
            }
            "camera_units_per_px" => {
                let x = n(0.0, 10.0)?;
                if x > 0.0 && x < 0.001 {
                    return Err(out_of_range(line, key, v, 0.001, 10.0));
                }
                self.camera.units_per_px = x;
            }
            "camera_offset_x_mm" => self.camera.offset_x_mm = n(-500.0, 500.0)?,
            "camera_offset_y_mm" => self.camera.offset_y_mm = n(-500.0, 500.0)?,
            "camera_rotation_deg" => self.camera.rotation_deg = n(-180.0, 180.0)?,
            "camera_flip_x" => self.camera.flip_x = flag(line, key, v)?,
            "camera_flip_y" => self.camera.flip_y = flag(line, key, v)?,
            "camera_lens_scale" => self.camera.lens_scale = n(0.5, 2.0)?,
            "camera_lens_k1" => self.camera.lens_k1 = n(-0.01, 0.01)?,
            "head" => {
                let head = parse_head(line, key, v)?;
                if self.heads.len() >= MAX_HEADS {
                    return Err(bad(line, key, v, "one of at most 4 heads"));
                }
                if self.heads.iter().any(|h| h.name == head.name) {
                    return Err(bad(line, key, v, "a name that no other head has"));
                }
                self.heads.push(head);
            }
            "tip_compat" => {
                let Some((pattern, tips)) = v.rsplit_once(':') else {
                    return Err(bad(line, key, v, "PATTERN : TIP, TIP, ..."));
                };
                let pattern = pattern.trim();
                let tips: Vec<String> = tips.split(',').map(|t| t.trim().to_string()).collect();
                let tips_ok = !tips.is_empty()
                    && tips.len() <= MAX_RULE_TIPS
                    && tips.iter().enumerate().all(|(i, t)| {
                        !t.is_empty()
                            && t.chars().count() <= 64
                            && !t.chars().any(char::is_control)
                            && !tips[..i].contains(t)
                    });
                if pattern.is_empty() || pattern.chars().count() > 128 || !tips_ok {
                    return Err(bad(
                        line,
                        key,
                        v,
                        "PATTERN : TIP, TIP, ... (one to eight different tips of at most 64 characters)",
                    ));
                }
                if self.tip_rules.len() >= MAX_TIP_RULES {
                    return Err(bad(line, key, v, "one of at most 64 rules"));
                }
                if self.tip_rules.iter().any(|r| r.pattern == pattern) {
                    return Err(bad(line, key, v, "a pattern that no other rule has"));
                }
                self.tip_rules.push(TipRule {
                    pattern: pattern.to_string(),
                    tips,
                });
            }
            "recipe_max_bytes" => self.recipe_max_bytes = int(line, key, v, 256, 1_048_576)? as u64,
            "recipe_max_rules" => self.recipe_max_rules = int(line, key, v, 1, 1000)? as usize,
            "import_unit" => {
                self.import_unit =
                    ImportUnit::parse(v).ok_or_else(|| bad(line, key, v, "mm, mil or inch"))?
            }
            "import_bottom_mirror_width_mm" => {
                self.import_bottom_mirror_width_mm = n(0.0, 10_000.0)?
            }
            "tip_compat_mode" => {
                self.tip_choice = TipChoice::parse(&v.to_ascii_lowercase())
                    .ok_or_else(|| bad(line, key, v, "fewest_changes or preferred"))?
            }
            "tray_fill" => {
                self.tray_fill = TrayFill::parse(&v.to_ascii_lowercase())
                    .ok_or_else(|| bad(line, key, v, "rows, columns or snake"))?
            }
            other => {
                return Err(PnpError::new(
                    Code::ConfigUnknownKey,
                    format!("line {line}: '{other}' is not a key of the configuration"),
                ))
            }
        }
        Ok(())
    }

    /// The checks that need several values together: the safety limits.
    pub fn validate(&self) -> Result<(), PnpError> {
        let unsafe_setting = |m: String| Err(PnpError::new(Code::UnsafeSetting, m));
        if self.z_min_mm >= self.z_max_mm {
            return unsafe_setting(format!(
                "z_min_mm {} must be below z_max_mm {}",
                self.z_min_mm, self.z_max_mm
            ));
        }
        if self.safe_z_mm < self.z_min_mm || self.safe_z_mm > self.z_max_mm {
            return unsafe_setting(format!(
                "safe_z_mm {} is outside the travel of Z, {} to {}",
                self.safe_z_mm, self.z_min_mm, self.z_max_mm
            ));
        }
        if self.safe_z_mm < self.board_top_z_mm {
            return unsafe_setting(format!(
                "safe_z_mm {} is below the top of the board, board_top_z_mm {}: the head would drag the parts",
                self.safe_z_mm, self.board_top_z_mm
            ));
        }
        if self.limit_x_min_mm >= self.limit_x_max_mm || self.limit_y_min_mm >= self.limit_y_max_mm
        {
            return unsafe_setting("the limits of X or Y are not a range".to_string());
        }
        if self.nozzle_angle_min_deg > self.nozzle_angle_max_deg {
            return unsafe_setting(
                "nozzle_angle_min_deg is above nozzle_angle_max_deg".to_string(),
            );
        }
        if self.fault_limit > self.fault_window {
            return unsafe_setting(format!(
                "fault_limit {} is above fault_window {}",
                self.fault_limit, self.fault_window
            ));
        }
        if self.place_retries + 1 > self.max_placement_attempts {
            return unsafe_setting(format!(
                "place_retries {} allows more attempts than max_placement_attempts {}",
                self.place_retries, self.max_placement_attempts
            ));
        }
        if !(self.vacuum_free_kpa <= self.vacuum_release_max_kpa
            && self.vacuum_release_max_kpa < self.vacuum_pick_min_kpa
            && self.vacuum_pick_min_kpa <= self.vacuum_held_kpa)
        {
            return unsafe_setting(format!(
                "the levels of the vacuum must satisfy vacuum_free_kpa {} <= vacuum_release_max_kpa {} < vacuum_pick_min_kpa {} <= vacuum_held_kpa {}",
                self.vacuum_free_kpa,
                self.vacuum_release_max_kpa,
                self.vacuum_pick_min_kpa,
                self.vacuum_held_kpa
            ));
        }
        if let Some((x, y)) = self.tip_changer {
            if x.is_nan() || y.is_nan() {
                return Err(PnpError::new(
                    Code::ConfigBadValue,
                    "tip_changer_x_mm and tip_changer_y_mm go together",
                ));
            }
        }
        if self.max_parts > crate::parts::MAX_PARTS {
            return unsafe_setting("max_parts is above the limit of the program".to_string());
        }
        crate::rotation::RotationTable::new(&self.rotation_rules)?;
        Ok(())
    }

    /// The defaults for the properties of a part (the last link of the chain of inheritance).
    pub fn part_defaults(&self) -> Defaults {
        Defaults {
            height_mm: self.default_height_mm,
            through_board_mm: self.default_through_board_mm,
            pick_retries: self.pick_retries,
            bin: self.default_bin,
            nozzle_tip: self.default_tip.clone(),
            rank: self.default_rank,
            origin_dx_mm: 0.0,
            origin_dy_mm: 0.0,
            tape_mm: self.default_tape_mm,
        }
    }

    /// The table of rotation corrections of the configuration.
    pub fn rotation_table(&self) -> Result<crate::rotation::RotationTable, PnpError> {
        crate::rotation::RotationTable::new(&self.rotation_rules)
    }
}
