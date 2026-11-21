//! The configuration of the machine and of the planner: a text file of `key = value` lines, the same format as the other programs of the suite use. Every key is optional and keeps the
//! default when it is missing; a key that is not known, written twice, not a valid value or outside its range is an error that names the line. The numbers that decide how the machine
//! moves, how often it tries again or what is safe live here and never in the code.

use crate::camera::CameraModel;
use crate::catalog::Defaults;
use crate::error::{Code, Lang, PnpError};
use crate::feeding::TrayFill;
use crate::fidcheck::{Policy as FiducialCheck, Reject as FiducialReject};
use crate::fiducial::FitMode;
use crate::import::{ImportUnit, Settings as ImportSettings};
use crate::motion::BacklashMode;
use crate::parts::{BottomRotation, PositionColumns, Profile};
use crate::server::Mode;
use crate::tips::{TipChoice, TipRule, MAX_RULE_TIPS, MAX_TIP_RULES};
use std::collections::HashSet;

/// The version of the schema of the file that this program writes and reads; a file with a higher number is refused.
pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_CONFIG_BYTES: usize = 262_144;
const MAX_LINE_CHARS: usize = 4096;

/// Every key that the file knows, in the order of the example `examples/machine.cfg`.
pub const KEYS: [&str; 266] = [
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
    "placement_order",
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
    "nozzle_runout_mm",
    "nozzle_runout_deg",
    "polarity_refs",
    "polarity_packages",
    "backlash_x_mm",
    "backlash_y_mm",
    "backlash_mode",
    "order_spare_percent",
    "order_spare_min",
    "order_boards",
    "panel_cols",
    "panel_rows",
    "panel_pitch_x_mm",
    "panel_pitch_y_mm",
    "panel_fiducial_mm",
    "panel_board_width_mm",
    "panel_board_height_mm",
    "part_size_tol_mm",
    "part_angle_tol_deg",
    "part_offset_max_mm",
    "fiducial_check",
    "fiducial_max_rotation_deg",
    "fiducial_scale_tol_percent",
    "fiducial_shear_tol_deg",
    "fiducial_max_shift_mm",
    "fiducial_level_turn_deg",
    "fiducial_reject",
    "feeder_fail_percent",
    "feeder_fail_min_picks",
    "feeder_pitch_step_mm",
    "feeder_min_gap_mm",
    "nozzle_life_warn",
    "camera_pattern_pitch_mm",
    "camera_pattern_max_residual_mm",
    "queue_confirm",
    "queue_stop_on_failure",
    "camera_correct_lens",
    "queue_relative_paths",
    "queue_gui_ask_run",
    "camera_combined",
    "camera_pattern_origin_x_mm",
    "camera_pattern_origin_y_mm",
    "camera_combined_touch",
    "align_rereads",
    "place_learn",
    "place_learn_apply",
    "place_learn_cap_mm",
    "place_learn_outlier",
    "place_learn_min",
    "place_learn_memory",
    "hooks",
    "hooks_timeout_s",
    "hooks_max_runs",
    "parts_diff_tol_mm",
    "parts_diff_rot_tol_deg",
    "board_stats_near_mm",
    "board_stats_limit",
    "feeder_slots_max_plans",
    "simulate_runs_count",
    "journal_steps",
    "mode",
    "server_address",
    "server_token",
    "server_machine",
    "server_timeout_s",
    "server_poll_ms",
    "server_attempts",
    "server_retries",
    "server_mdns",
    "nozzle_z_tol_mm",
    "tape_pitch_tol_mm",
    "shift_minutes",
    "reel_change_s",
    "panel_fid_min_sep_mm",
    "clearance_min_mm",
    "lot_max_weeks",
    "fab_dnp_mode",
    "fab_dnp_column",
    "fab_tht_words",
    "cpl_origin_x_mm",
    "cpl_origin_y_mm",
    "cpl_flip_y",
    "cpl_rotation_cw",
    "cpl_rotation_offset_deg",
    "cpl_bottom_rotation",
    "ipc_density",
    "ipc_search_limit",
    "ipc_search_distance",
    "dfm_verified",
    "dfm_pad_pitch_mm",
    "dfm_pad_gap_min_mm",
    "dfm_smt_gap_mm",
    "dfm_track_center_frac",
    "dfm_tooling_edge_mm",
    "dfm_smt_tooling_edge_mm",
    "dfm_tooling_hole_mm",
    "dfm_hole_tol_mm",
    "dfm_fid_diameter_mm",
    "dfm_fid_edge_mm",
    "dfm_fid_ring_mm",
    "dfm_rail_min_mm",
    "stencil_thickness_mm",
    "stencil_thickness_min_mm",
    "stencil_thickness_max_mm",
    "stencil_reduction_percent",
    "stencil_reduction_min_percent",
    "stencil_reduction_max_percent",
    "stencil_fine_reduction_percent",
    "stencil_fine_reduction_min_percent",
    "stencil_fine_reduction_max_percent",
    "stencil_fine_pitch_mm",
    "stencil_area_ratio_min",
    "stencil_thermal_percent",
    "stencil_thermal_min_percent",
    "stencil_thermal_max_percent",
    "stencil_thermal_side_mm",
    "stencil_window_max_mm",
    "stencil_web_mm",
    "tower_idle_gap_s",
    "tower_slow_factor",
    "busy_gaps_listed",
    "lml_max_step_mm",
    "lml_tolerance_mm",
    "lml_max_iterations",
    "lml_gain",
    "lml_offset_x_mm",
    "lml_offset_y_mm",
    "vacuum_margin_pct",
    "vacuum_min_gap_kpa",
    "vacuum_min_readings",
    "testboard_pitch_mm",
    "testboard_count",
    "slot_pitch_mm",
    "slot_gap_mm",
    "slot_bank_slots",
    "slot_reach_mm",
    "node_silence_limit",
    "node_timeout_s",
    "panel_clearance_mm",
    "spec_table",
    "spec_machine",
    "spec_work_area",
    "spec_smallest_part",
    "spec_max_speed_mm_s",
    "heightmap_max_warp_mm",
    "drag_hole_pitch_mm",
    "drag_max_picks_per_stroke",
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
    /// The list of keys that orders the places in place of `order` (empty: `order` decides), see `placeorder`.
    pub placement_order: Vec<crate::placeorder::Term>,
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
    /// How far the tip of the nozzle is from the axis it turns about; the G-code of a plan moves each place by the opposite of the runout at the angle of the part. 0 does nothing.
    pub nozzle_runout_mm: f64,
    /// The direction of the tip from the axis when the nozzle is at 0 degrees.
    pub nozzle_runout_deg: f64,
    /// A part whose reference matches one of these patterns (lower case, `*` and `?`) has a direction and is on the sheet of `polarity-sheet`.
    pub polarity_refs: Vec<String>,
    /// A part whose package matches one of these patterns has a direction too.
    pub polarity_packages: Vec<String>,
    /// The slack of the X and Y axes, in millimetres; 0 is an axis with none.
    pub backlash_x_mm: f64,
    pub backlash_y_mm: f64,
    /// What the simulated machine does about the slack.
    pub backlash_mode: BacklashMode,
    /// The per cent of parts added to an order for the parts lost in the feeders and in failed picks (`bom-order`).
    pub order_spare_percent: f64,
    /// The fewest parts added to each line of an order.
    pub order_spare_min: usize,
    /// The number of boards that the window asks for in the list of parts to buy.
    pub order_boards: usize,
    /// What the panel form of the window starts with: the boards along X and along Y, their pitch, the margin of the fiducials (0 puts none) and the real size of a board (0 is the pitch).
    pub panel_cols: usize,
    pub panel_rows: usize,
    pub panel_pitch_x_mm: f64,
    pub panel_pitch_y_mm: f64,
    pub panel_fiducial_mm: f64,
    pub panel_board_width_mm: f64,
    pub panel_board_height_mm: f64,
    /// How far either side of the part on the nozzle may be from the size of its package, in millimetres (`part-inspect`).
    pub part_size_tol_mm: f64,
    /// How far the turn of the part on the nozzle may be from the expected one, in degrees.
    pub part_angle_tol_deg: f64,
    /// How far from the axis of the nozzle the part may hang, in millimetres.
    pub part_offset_max_mm: f64,
    /// What the check of the fit of the fiducials does: nothing, print the findings or stop the job.
    pub fiducial_check: FiducialCheck,
    /// The limits of the check: the turn from the design, the scale along an axis (per cent), how far the axes are from square, how far the middle of the marks moved, and how
    /// much a board may be turned in the panel beyond the turn of the panel.
    pub fiducial_max_rotation_deg: f64,
    pub fiducial_scale_tol_percent: f64,
    pub fiducial_shear_tol_deg: f64,
    pub fiducial_max_shift_mm: f64,
    pub fiducial_level_turn_deg: f64,
    /// What is done with a fit whose worst mark is too far from it: refuse it, or leave out the one mark that alone explains it.
    pub fiducial_reject: FiducialReject,
    /// A feeder that fails more than this share of its picks (per cent) is marked in the report of the feeders, once it was tried `feeder_fail_min_picks` times.
    pub feeder_fail_percent: f64,
    pub feeder_fail_min_picks: u32,
    /// The pitch of a strip is a whole number of this many millimetres (0 does not check) and two feeders are at least this far apart (0 does not check): `feeder-check`.
    pub feeder_pitch_step_mm: f64,
    pub feeder_min_gap_mm: f64,
    /// A nozzle tip that has placed this many parts is told to the person to look at (0 does not warn): `nozzle-life`, `setup-sheet` and `simulate --nozzle-life`.
    pub nozzle_life_warn: u64,
    /// The distance between neighbouring points of the pattern of the camera calibration (millimetres) and how far a point may be from the model of the calibration (millimetres): `camera-pattern`.
    pub camera_pattern_pitch_mm: f64,
    pub camera_pattern_max_residual_mm: f64,
    /// A job of a queue runs only when the person confirmed its plan (`queue-confirm`); off for the simulated machine only.
    pub queue_confirm: bool,
    /// The queue stops at the first job that fails; off goes on to the next ones.
    pub queue_stop_on_failure: bool,
    /// The camera takes the bending of the lens (`camera_lens_k1`) out of what it sees; for a lens that `camera-pattern` calibrated (off: the bending goes into the readings).
    pub camera_correct_lens: bool,
    /// queue-add keeps the files of a job that are in the folder of the queue (or below it) as paths from that folder, so that the folder can be moved; off keeps absolute paths.
    pub queue_relative_paths: bool,
    /// The Run button of the Queue tool asks first (the second press starts the run); off starts the run at the first press. A run is on the simulated machine in either case.
    pub queue_gui_ask_run: bool,
    /// The combined calibration of the camera (`camera-combined`, the Combined camera tool) is allowed: a pattern fixed to the bed while the head moves. Off by default; the separate calibrations do not depend on it.
    pub camera_combined: bool,
    /// Where the point 0,0 of the pattern lies in the machine (millimetres) when the pattern is fixed to the bed: `camera-combined`.
    pub camera_pattern_origin_x_mm: f64,
    /// See `camera_pattern_origin_x_mm`.
    pub camera_pattern_origin_y_mm: f64,
    /// The combined calibration takes the origin of the sheet from a row `origin,X,Y` of the file of sightings (the place of the nozzle when it touched the point 0,0) instead of from `camera_pattern_origin_x_mm` and `camera_pattern_origin_y_mm`. Off by default.
    pub camera_combined_touch: bool,
    /// How many more times the camera looks at a part whose first look was further than `pick_tolerance_mm` before the part is thrown away (0 to 5; 0 throws it away at once). A look that is within the tolerance is taken as the first look.
    pub align_rereads: u32,
    /// Learning the offset of the placement of each package from readings (`place-learn`) is allowed. Off by default.
    pub place_learn: bool,
    /// A job may take the learned offsets out of the places (`simulate --place-offsets`). Off by default.
    pub place_learn_apply: bool,
    /// The largest learned offset, in millimetres either way (0.01 to 1).
    pub place_learn_cap_mm: f64,
    /// A reading farther from the median of its package than this many spreads is left out (1.5 to 10).
    pub place_learn_outlier: f64,
    /// The fewest readings of a package, once the strays are out, for its offset to change (1 to 1000).
    pub place_learn_min: u32,
    /// How many readings the offset that is already learned counts for when new readings come in (1 to 100000).
    pub place_learn_memory: u32,
    /// Programs may be run for the events of a job (`simulate --hooks`). Off by default.
    pub hooks: bool,
    /// The seconds a hook may run before it is stopped (1 to 60).
    pub hooks_timeout_s: f64,
    /// The most programs run for one job (1 to 100); the rest are left out and told.
    pub hooks_max_runs: u32,
    /// How near two places are to be the same part in `parts-diff` and in its panel (millimetres, 0 to 100), and how near two turns (degrees, 0 to 180).
    pub parts_diff_tol_mm: f64,
    pub parts_diff_rot_tol_deg: f64,
    /// The distance under which `board-stats` and its panel report two parts of one side as too near (millimetres, 0.01 to 50; 0 leaves the look off), and the most pairs it lists (1 to 1000).
    pub board_stats_near_mm: f64,
    pub board_stats_limit: u32,
    /// The most plans that `feeder-slots` and its panel make while they look for a better place for the feeders (1 to 1000000).
    pub feeder_slots_max_plans: u32,
    /// How many times `simulate-runs`, `simulate-hotspots` and their panels run the job (1 to 10000).
    pub simulate_runs_count: u32,
    /// How many steps the journal panel runs (0 to 1000000; 0 runs the job to its end).
    pub journal_steps: u32,
    /// `local` (the default) or `server`: whether the commands of the server may send jobs to the EH3D server.
    pub mode: Mode,
    /// Where the server is (`host:port`), the token of this program on it and the id of the machine there; empty until they are given.
    pub server_address: String,
    pub server_token: String,
    pub server_machine: String,
    /// The seconds a request to the server may take (0.5 to 120), the milliseconds between two readings of a job that is followed (50 to 60000) and how many times a request that got no
    /// answer is tried (1 to 10).
    pub server_timeout_s: f64,
    pub server_poll_ms: u32,
    pub server_attempts: u32,
    /// How many more times the link to the EH3D server tries a request that failed on the network (0 to 5), and whether it may look for the server on the local network (off by default).
    pub server_retries: u32,
    pub server_mdns: bool,
    /// The most that the height of a nozzle may differ from the mean of the nozzles before `nozzle-zcal` calls it out of line (0.001 to 1 mm).
    pub nozzle_z_tol_mm: f64,
    /// The most that the pitch of a tape feeder may differ from the pitch its package asks for before `tape-check` refuses it (0 to 2 mm).
    pub tape_pitch_tol_mm: f64,
    /// The length of a shift for `shift-oee` and `queue-eta` (30 to 1440 minutes) and the seconds a person takes to change one reel (0 to 3600).
    pub shift_minutes: u32,
    pub reel_change_s: f64,
    /// The least distance (mm) between the two marks of a board that are farthest apart for `panel-fids` to call the board aligned by its own marks (0 to 1000).
    pub panel_fid_min_sep_mm: f64,
    /// The least space (mm) between the bodies of two parts on the same side for `clearance` to call them apart (0 to 5).
    pub clearance_min_mm: f64,
    /// The age (weeks) over which `lot-age` calls the lot of a reel old (1 to 1040).
    pub lot_max_weeks: u32,
    /// What the files for an assembly house do with a part that is not to be mounted: `column` keeps it and marks it, `omit` leaves it out.
    pub fab_dnp_mode: crate::jlcpcb::DnpMode,
    /// The name of the column that marks a part that is not to be mounted in the files for an assembly house.
    pub fab_dnp_column: String,
    /// The words of a package name that tell a part that goes through the board.
    pub fab_tht_words: Vec<String>,
    /// The point of the file (mm) that becomes the origin when a position file is converted (`convert-cpl`).
    pub cpl_origin_x_mm: f64,
    /// The same along Y.
    pub cpl_origin_y_mm: f64,
    /// Whether `convert-cpl` turns the Y axis the other way.
    pub cpl_flip_y: bool,
    /// Whether the rotation of the file that `convert-cpl` reads is counted clockwise.
    pub cpl_rotation_cw: bool,
    /// A turn (degrees) that `convert-cpl` adds to every part, after the direction is set right.
    pub cpl_rotation_offset_deg: f64,
    /// What `convert-cpl` does last with the rotation of the parts of the bottom side: keep, mirror or half_turn.
    pub cpl_bottom_rotation: BottomRotation,
    /// The density of the land that the names of the convention of IPC-7351B carry: L least, N nominal, M most.
    pub ipc_density: crate::ipcname::Density,
    /// The most names that `footprint-find` lists.
    pub ipc_search_limit: u32,
    /// How many letters a name of the library may differ by to be listed as near by `footprint-find` (0 turns it off).
    pub ipc_search_distance: u32,
    /// Whether the `dfm_*` limits have been compared with the rules of the house that assembles the board; the report of `dfm-check` says so while it is off.
    pub dfm_verified: bool,
    /// dfm-check: pads of one part at or under this pitch (mm) are told as fine pitch.
    pub dfm_pad_pitch_mm: f64,
    /// dfm-check: the least gap (mm) between the pads of two parts.
    pub dfm_pad_gap_min_mm: f64,
    /// dfm-check: the least space (mm) between the bodies of two parts on the surface.
    pub dfm_smt_gap_mm: f64,
    /// dfm-check: a track that ends within this share of the half side of a pad from its middle is called out (0 to 1).
    pub dfm_track_center_frac: f64,
    /// dfm-check: the least distance (mm) from a tooling hole to the edge of the outline.
    pub dfm_tooling_edge_mm: f64,
    /// dfm-check: the least distance (mm) from a part on the surface to the edge that has the tooling holes.
    pub dfm_smt_tooling_edge_mm: f64,
    /// dfm-check: the diameter (mm) of a tooling hole.
    pub dfm_tooling_hole_mm: f64,
    /// dfm-check: how far (mm) the diameter of a tooling hole or a fiducial may be from its figure.
    pub dfm_hole_tol_mm: f64,
    /// dfm-check: the diameter (mm) of a fiducial.
    pub dfm_fid_diameter_mm: f64,
    /// dfm-check: the least distance (mm) from a fiducial to the edge of the outline.
    pub dfm_fid_edge_mm: f64,
    /// dfm-check: the radius (mm) around a fiducial that is kept free of pads and holes.
    pub dfm_fid_ring_mm: f64,
    /// dfm-check: the least width (mm) of a tab or rail of the panel.
    pub dfm_rail_min_mm: f64,
    /// stencil-apertures: the thickness (mm) of the stencil.
    pub stencil_thickness_mm: f64,
    /// stencil-apertures: the least thickness (mm) the stencil should have.
    pub stencil_thickness_min_mm: f64,
    /// stencil-apertures: the greatest thickness (mm) the stencil should have.
    pub stencil_thickness_max_mm: f64,
    /// stencil-apertures: how much each side of an opening is smaller than its pad (per cent).
    pub stencil_reduction_percent: f64,
    /// stencil-apertures: the least reduction (per cent) that is in range.
    pub stencil_reduction_min_percent: f64,
    /// stencil-apertures: the greatest reduction (per cent) that is in range.
    pub stencil_reduction_max_percent: f64,
    /// stencil-apertures: the reduction (per cent) for the pads of a fine pitch.
    pub stencil_fine_reduction_percent: f64,
    /// stencil-apertures: the least reduction (per cent) for a fine pitch that is in range.
    pub stencil_fine_reduction_min_percent: f64,
    /// stencil-apertures: the greatest reduction (per cent) for a fine pitch that is in range.
    pub stencil_fine_reduction_max_percent: f64,
    /// stencil-apertures: pads at or under this pitch (mm) are of a fine pitch.
    pub stencil_fine_pitch_mm: f64,
    /// stencil-apertures: the least area ratio of an opening (its area over the area of its walls).
    pub stencil_area_ratio_min: f64,
    /// stencil-apertures: the share (per cent) of a thermal pad that its windows cover.
    pub stencil_thermal_percent: f64,
    /// stencil-apertures: the least cover (per cent) of a thermal pad that is in range.
    pub stencil_thermal_min_percent: f64,
    /// stencil-apertures: the greatest cover (per cent) of a thermal pad that is in range.
    pub stencil_thermal_max_percent: f64,
    /// stencil-apertures: a pad whose two sides are at least this long (mm) is a thermal pad.
    pub stencil_thermal_side_mm: f64,
    /// stencil-apertures: the longest side (mm) of a window of a thermal pad.
    pub stencil_window_max_mm: f64,
    /// stencil-apertures: the least web (mm) between the windows of a thermal pad.
    pub stencil_web_mm: f64,
    /// sim-tower: a wait between two steps over this many seconds turns the tower yellow.
    pub tower_idle_gap_s: f64,
    /// sim-tower: a step that takes more than this many times the median step of its kind turns the tower yellow.
    pub tower_slow_factor: f64,
    /// sim-busy: how many of the longest waits are listed.
    pub busy_gaps_listed: u32,
    /// align-iterate: the longest step (mm) the head makes after a look; a look that gets worse halves it for the rest of the run.
    pub lml_max_step_mm: f64,
    /// align-iterate: the error (mm) under which the head is aligned.
    pub lml_tolerance_mm: f64,
    /// align-iterate: how many moves are made before it gives up.
    pub lml_max_iterations: u32,
    /// align-iterate: the share of what is seen that is moved at each step (0.05 to 1).
    pub lml_gain: f64,
    /// align-iterate: the constant error of the camera against the nozzle (mm), subtracted from what is seen.
    pub lml_offset_x_mm: f64,
    /// The same along Y.
    pub lml_offset_y_mm: f64,
    /// vacuum-calibrate: how far inside the gap between the readings with and without a part each threshold is put (per cent of the gap).
    pub vacuum_margin_pct: f64,
    /// vacuum-calibrate: a gap (kPa) under this is told as small.
    pub vacuum_min_gap_kpa: f64,
    /// vacuum-calibrate: fewer readings than this of a state are told as few.
    pub vacuum_min_readings: u32,
    /// test-board-gen: the distance (mm) between two parts of a made-up board.
    pub testboard_pitch_mm: f64,
    /// test-board-gen: how many parts a made-up board has.
    pub testboard_count: u32,
    /// slot-bank: how wide one slot of a bank of feeders is (millimetres), the gap a feeder needs beside its tape (millimetres), how many slots a bank has, and how far from the inner edge of
    /// the bank the head can pick (millimetres).
    pub slot_pitch_mm: f64,
    pub slot_gap_mm: f64,
    pub slot_bank_slots: u32,
    pub slot_reach_mm: f64,
    /// The nodes of the feeders and of the head: how many questions in a row may go unanswered before the link is told to be silent and the machine is held, and how long one question may take.
    pub node_silence_limit: u32,
    pub node_timeout_s: u64,
    /// panel-compose: the least distance between the outlines of two boards of a panel (millimetres).
    pub panel_clearance_mm: f64,
    /// The specification sheet of the machine (`machine-sheet`): the table, the machine, the work area and the smallest part tried, as text, and the greatest speed (mm/s). Empty, and 0,
    /// mean that nobody measured it; the sheet then says so and no figure is made up.
    pub spec_table: String,
    pub spec_machine: String,
    pub spec_work_area: String,
    pub spec_smallest_part: String,
    pub spec_max_speed_mm_s: f64,
    /// height-map: how much a board may bend (the highest measured point less the lowest, mm) before the check is not passed.
    pub heightmap_max_warp_mm: f64,
    /// drag-plan: the distance between the sprocket holes of a tape (mm); a drag feeder moves a tape by a whole number of them. The standard of the tapes gives 4.
    pub drag_hole_pitch_mm: f64,
    /// drag-plan: the most picks that one stroke of a drag feeder may serve (a pocket pitch that needs more is refused).
    pub drag_max_picks_per_stroke: u32,
}

/// The most retries of the link to the server.
pub const SERVER_RETRIES_MAX: u32 = 5;

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
            placement_order: Vec::new(),
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
            nozzle_runout_mm: 0.0,
            nozzle_runout_deg: 0.0,
            polarity_refs: ["d*", "led*", "u*", "ic*", "q*", "cp*", "j*"]
                .map(String::from)
                .to_vec(),
            polarity_packages: [
                "*led*", "*dip*", "*soic*", "*sop*", "*qfn*", "*qfp*", "*bga*", "*sot*", "*sod*",
                "*dfn*", "*tant*", "cp_*", "*elec*",
            ]
            .map(String::from)
            .to_vec(),
            backlash_x_mm: 0.0,
            backlash_y_mm: 0.0,
            backlash_mode: BacklashMode::Off,
            order_spare_percent: 0.0,
            order_spare_min: 0,
            order_boards: 1,
            panel_cols: 2,
            panel_rows: 2,
            panel_pitch_x_mm: 50.0,
            panel_pitch_y_mm: 50.0,
            panel_fiducial_mm: 0.0,
            panel_board_width_mm: 0.0,
            panel_board_height_mm: 0.0,
            part_size_tol_mm: 0.3,
            part_angle_tol_deg: 5.0,
            part_offset_max_mm: 2.0,
            fiducial_check: FiducialCheck::Warn,
            fiducial_max_rotation_deg: 5.0,
            fiducial_scale_tol_percent: 1.0,
            fiducial_shear_tol_deg: 0.5,
            fiducial_max_shift_mm: 5.0,
            fiducial_level_turn_deg: 2.0,
            fiducial_reject: FiducialReject::Refuse,
            feeder_fail_percent: 20.0,
            feeder_fail_min_picks: 10,
            feeder_pitch_step_mm: 2.0,
            feeder_min_gap_mm: 3.0,
            nozzle_life_warn: 500_000,
            camera_pattern_pitch_mm: 1.0,
            camera_pattern_max_residual_mm: 0.05,
            queue_confirm: true,
            queue_stop_on_failure: true,
            camera_correct_lens: false,
            queue_relative_paths: false,
            queue_gui_ask_run: true,
            camera_combined: false,
            camera_pattern_origin_x_mm: 0.0,
            camera_pattern_origin_y_mm: 0.0,
            camera_combined_touch: false,
            align_rereads: 0,
            place_learn: false,
            place_learn_apply: false,
            place_learn_cap_mm: 0.3,
            place_learn_outlier: 3.5,
            place_learn_min: 3,
            place_learn_memory: 100,
            hooks: false,
            hooks_timeout_s: 5.0,
            hooks_max_runs: 20,
            parts_diff_tol_mm: 0.01,
            parts_diff_rot_tol_deg: 0.5,
            board_stats_near_mm: 0.0,
            board_stats_limit: 20,
            feeder_slots_max_plans: 5000,
            simulate_runs_count: 20,
            journal_steps: 0,
            mode: Mode::Local,
            server_address: String::new(),
            server_token: String::new(),
            server_machine: String::new(),
            server_timeout_s: 5.0,
            server_poll_ms: 500,
            server_attempts: 3,
            server_retries: 2,
            server_mdns: false,
            nozzle_z_tol_mm: 0.02,
            tape_pitch_tol_mm: 0.05,
            shift_minutes: 480,
            reel_change_s: 60.0,
            panel_fid_min_sep_mm: 10.0,
            clearance_min_mm: 0.2,
            lot_max_weeks: 104,
            fab_dnp_mode: crate::jlcpcb::DnpMode::Column,
            fab_dnp_column: String::from("Customer Supply"),
            fab_tht_words: crate::jlcpcb::DEFAULT_THT_WORDS
                .iter()
                .map(|s| s.to_string())
                .collect(),
            cpl_origin_x_mm: 0.0,
            cpl_origin_y_mm: 0.0,
            cpl_flip_y: false,
            cpl_rotation_cw: false,
            cpl_rotation_offset_deg: 0.0,
            cpl_bottom_rotation: BottomRotation::Keep,
            ipc_density: crate::ipcname::Density::Nominal,
            ipc_search_limit: 20,
            ipc_search_distance: 2,
            dfm_verified: false,
            dfm_pad_pitch_mm: 1.27,
            dfm_pad_gap_min_mm: 0.2,
            dfm_smt_gap_mm: 1.0,
            dfm_track_center_frac: 0.25,
            dfm_tooling_edge_mm: 3.0,
            dfm_smt_tooling_edge_mm: 5.0,
            dfm_tooling_hole_mm: 2.0,
            dfm_hole_tol_mm: 0.1,
            dfm_fid_diameter_mm: 1.0,
            dfm_fid_edge_mm: 3.85,
            dfm_fid_ring_mm: 2.0,
            dfm_rail_min_mm: 4.0,
            stencil_thickness_mm: 0.12,
            stencil_thickness_min_mm: 0.10,
            stencil_thickness_max_mm: 0.15,
            stencil_reduction_percent: 6.0,
            stencil_reduction_min_percent: 5.0,
            stencil_reduction_max_percent: 7.0,
            stencil_fine_reduction_percent: 15.0,
            stencil_fine_reduction_min_percent: 10.0,
            stencil_fine_reduction_max_percent: 20.0,
            stencil_fine_pitch_mm: 0.5,
            stencil_area_ratio_min: 0.66,
            stencil_thermal_percent: 60.0,
            stencil_thermal_min_percent: 50.0,
            stencil_thermal_max_percent: 75.0,
            stencil_thermal_side_mm: 2.0,
            stencil_window_max_mm: 2.0,
            stencil_web_mm: 0.3,
            tower_idle_gap_s: 2.0,
            tower_slow_factor: 3.0,
            busy_gaps_listed: 5,
            lml_max_step_mm: 0.5,
            lml_tolerance_mm: 0.02,
            lml_max_iterations: 4,
            lml_gain: 0.8,
            lml_offset_x_mm: 0.0,
            lml_offset_y_mm: 0.0,
            vacuum_margin_pct: 25.0,
            vacuum_min_gap_kpa: 10.0,
            vacuum_min_readings: 5,
            testboard_pitch_mm: 2.0,
            testboard_count: 40,
            slot_pitch_mm: 4.0,
            slot_gap_mm: 4.0,
            slot_bank_slots: 60,
            slot_reach_mm: 240.0,
            node_silence_limit: 3,
            node_timeout_s: 2,
            panel_clearance_mm: 1.0,
            spec_table: String::new(),
            spec_machine: String::new(),
            spec_work_area: String::new(),
            spec_smallest_part: String::new(),
            spec_max_speed_mm_s: 0.0,
            heightmap_max_warp_mm: 1.0,
            drag_hole_pitch_mm: 4.0,
            drag_max_picks_per_stroke: 4,
        }
    }
}

/// A text of the server settings: no control character, at most `max` characters; empty is allowed (the setting is not given).
fn server_text(line: usize, key: &str, v: &str, max: usize) -> Result<String, PnpError> {
    if v.chars().count() > max || v.chars().any(char::is_control) {
        return Err(bad(
            line,
            key,
            v,
            &format!("a text of up to {max} characters with no control character"),
        ));
    }
    Ok(v.to_string())
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
        // server_attempts is the old name of server_retries (one try more than the retries): it still works, and the retries it asks for win only when server_retries is not written
        if seen.contains("server_attempts") && !seen.contains("server_retries") {
            cfg.server_retries = cfg
                .server_attempts
                .saturating_sub(1)
                .min(SERVER_RETRIES_MAX);
        }
        cfg.validate()?;
        Ok(cfg)
    }

    /// The notes about keys of a text of settings that still work but have a new name: one line each, to be shown to the person (the settings are read the same).
    pub fn deprecation_notes(text: &str) -> Vec<String> {
        let mut notes = Vec::new();
        let (mut attempts, mut retries) = (false, false);
        for raw in text.trim_start_matches('\u{feff}').lines() {
            let t = raw.trim();
            if t.starts_with('#') {
                continue;
            }
            match t
                .split_once('=')
                .map(|(k, _)| k.trim().to_ascii_lowercase())
            {
                Some(k) if k == "server_attempts" => attempts = true,
                Some(k) if k == "server_retries" => retries = true,
                _ => {}
            }
        }
        if attempts && !retries {
            notes.push("server_attempts is the old name of server_retries (the retries are one fewer than the attempts); write server_retries instead".to_string());
        }
        notes
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
            "placement_order" => {
                self.placement_order = crate::placeorder::parse(v)
                    .map_err(|why| bad(line, key, v, &format!("a list of keys: {why}")))?;
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
            "nozzle_runout_mm" => self.nozzle_runout_mm = n(0.0, 5.0)?,
            "nozzle_runout_deg" => self.nozzle_runout_deg = n(-180.0, 180.0)?,
            "order_spare_percent" => self.order_spare_percent = n(0.0, 100.0)?,
            "order_spare_min" => self.order_spare_min = int(line, key, v, 0, 1000)? as usize,
            "order_boards" => self.order_boards = int(line, key, v, 1, 100_000)? as usize,
            "panel_cols" => self.panel_cols = int(line, key, v, 1, 100)? as usize,
            "panel_rows" => self.panel_rows = int(line, key, v, 1, 100)? as usize,
            "panel_pitch_x_mm" => self.panel_pitch_x_mm = n(0.001, 10_000.0)?,
            "panel_pitch_y_mm" => self.panel_pitch_y_mm = n(0.001, 10_000.0)?,
            "panel_fiducial_mm" => self.panel_fiducial_mm = n(0.0, 1000.0)?,
            "panel_board_width_mm" => self.panel_board_width_mm = n(0.0, 10_000.0)?,
            "panel_board_height_mm" => self.panel_board_height_mm = n(0.0, 10_000.0)?,
            "part_size_tol_mm" => self.part_size_tol_mm = n(0.01, 5.0)?,
            "part_angle_tol_deg" => self.part_angle_tol_deg = n(0.1, 45.0)?,
            "part_offset_max_mm" => self.part_offset_max_mm = n(0.1, 20.0)?,
            "fiducial_check" => {
                self.fiducial_check = FiducialCheck::parse(&v.to_ascii_lowercase())
                    .ok_or_else(|| bad(line, key, v, "off, warn or refuse"))?
            }
            "fiducial_max_rotation_deg" => self.fiducial_max_rotation_deg = n(0.1, 180.0)?,
            "fiducial_scale_tol_percent" => self.fiducial_scale_tol_percent = n(0.01, 50.0)?,
            "fiducial_shear_tol_deg" => self.fiducial_shear_tol_deg = n(0.01, 45.0)?,
            "fiducial_max_shift_mm" => self.fiducial_max_shift_mm = n(0.1, 1000.0)?,
            "fiducial_level_turn_deg" => self.fiducial_level_turn_deg = n(0.1, 180.0)?,
            "fiducial_reject" => {
                self.fiducial_reject = FiducialReject::parse(&v.to_ascii_lowercase())
                    .ok_or_else(|| bad(line, key, v, "refuse or drop_worst"))?
            }
            "feeder_fail_percent" => self.feeder_fail_percent = n(0.0, 100.0)?,
            "feeder_fail_min_picks" => {
                self.feeder_fail_min_picks = int(line, key, v, 1, 100_000)? as u32
            }
            "feeder_pitch_step_mm" => self.feeder_pitch_step_mm = n(0.0, 100.0)?,
            "feeder_min_gap_mm" => self.feeder_min_gap_mm = n(0.0, 1000.0)?,
            "nozzle_life_warn" => {
                self.nozzle_life_warn = int(line, key, v, 0, 1_000_000_000)? as u64
            }
            "camera_pattern_pitch_mm" => self.camera_pattern_pitch_mm = n(0.01, 100.0)?,
            "camera_pattern_max_residual_mm" => {
                self.camera_pattern_max_residual_mm = n(0.0001, 5.0)?
            }
            "queue_confirm" => self.queue_confirm = flag(line, key, v)?,
            "queue_stop_on_failure" => self.queue_stop_on_failure = flag(line, key, v)?,
            "camera_correct_lens" => self.camera_correct_lens = flag(line, key, v)?,
            "queue_relative_paths" => self.queue_relative_paths = flag(line, key, v)?,
            "queue_gui_ask_run" => self.queue_gui_ask_run = flag(line, key, v)?,
            "camera_combined" => self.camera_combined = flag(line, key, v)?,
            "camera_pattern_origin_x_mm" => self.camera_pattern_origin_x_mm = n(-10000.0, 10000.0)?,
            "camera_pattern_origin_y_mm" => self.camera_pattern_origin_y_mm = n(-10000.0, 10000.0)?,
            "camera_combined_touch" => self.camera_combined_touch = flag(line, key, v)?,
            "align_rereads" => self.align_rereads = int(line, key, v, 0, 5)? as u32,
            "place_learn" => self.place_learn = flag(line, key, v)?,
            "place_learn_apply" => self.place_learn_apply = flag(line, key, v)?,
            "place_learn_cap_mm" => self.place_learn_cap_mm = n(0.01, 1.0)?,
            "place_learn_outlier" => self.place_learn_outlier = n(1.5, 10.0)?,
            "place_learn_min" => self.place_learn_min = int(line, key, v, 1, 1000)? as u32,
            "place_learn_memory" => self.place_learn_memory = int(line, key, v, 1, 100000)? as u32,
            "hooks" => self.hooks = flag(line, key, v)?,
            "hooks_timeout_s" => self.hooks_timeout_s = n(1.0, 60.0)?,
            "hooks_max_runs" => self.hooks_max_runs = int(line, key, v, 1, 100)? as u32,
            "parts_diff_tol_mm" => self.parts_diff_tol_mm = n(0.0, 100.0)?,
            "parts_diff_rot_tol_deg" => self.parts_diff_rot_tol_deg = n(0.0, 180.0)?,
            "board_stats_near_mm" => {
                let x = n(0.0, 50.0)?;
                if x != 0.0 {
                    n(0.01, 50.0)?;
                }
                self.board_stats_near_mm = x;
            }
            "board_stats_limit" => self.board_stats_limit = int(line, key, v, 1, 1000)? as u32,
            "feeder_slots_max_plans" => {
                self.feeder_slots_max_plans = int(line, key, v, 1, 1_000_000)? as u32
            }
            "simulate_runs_count" => {
                self.simulate_runs_count = int(line, key, v, 1, 10_000)? as u32
            }
            "journal_steps" => self.journal_steps = int(line, key, v, 0, 1_000_000)? as u32,
            "mode" => {
                self.mode = Mode::parse(v).ok_or_else(|| bad(line, key, v, "local or server"))?
            }
            "server_address" => self.server_address = server_text(line, key, v, 255)?,
            "server_token" => self.server_token = server_text(line, key, v, 1024)?,
            "server_machine" => {
                if !v.is_empty() && !crate::server::safe_segment(v) {
                    return Err(bad(
                        line,
                        key,
                        v,
                        "an id of up to 64 letters, digits, _ and -",
                    ));
                }
                self.server_machine = v.to_string();
            }
            "server_timeout_s" => self.server_timeout_s = n(0.5, 120.0)?,
            "server_poll_ms" => self.server_poll_ms = int(line, key, v, 50, 60_000)? as u32,
            "server_attempts" => self.server_attempts = int(line, key, v, 1, 10)? as u32,
            "server_retries" => {
                self.server_retries = int(line, key, v, 0, i64::from(SERVER_RETRIES_MAX))? as u32
            }
            "server_mdns" => self.server_mdns = flag(line, key, v)?,
            "nozzle_z_tol_mm" => self.nozzle_z_tol_mm = n(0.001, 1.0)?,
            "tape_pitch_tol_mm" => self.tape_pitch_tol_mm = n(0.0, 2.0)?,
            "shift_minutes" => self.shift_minutes = int(line, key, v, 30, 1440)? as u32,
            "reel_change_s" => self.reel_change_s = n(0.0, 3600.0)?,
            "panel_fid_min_sep_mm" => self.panel_fid_min_sep_mm = n(0.0, 1000.0)?,
            "clearance_min_mm" => self.clearance_min_mm = n(0.0, 5.0)?,
            "lot_max_weeks" => self.lot_max_weeks = int(line, key, v, 1, 1040)? as u32,
            "fab_dnp_mode" => {
                self.fab_dnp_mode = crate::jlcpcb::DnpMode::parse(v)
                    .ok_or_else(|| bad(line, key, v, "column or omit"))?
            }
            "fab_dnp_column" => self.fab_dnp_column = server_text(line, key, v, 64)?,
            "fab_tht_words" => self.fab_tht_words = names(line, key, v)?,
            "cpl_origin_x_mm" => self.cpl_origin_x_mm = n(-10000.0, 10000.0)?,
            "cpl_origin_y_mm" => self.cpl_origin_y_mm = n(-10000.0, 10000.0)?,
            "cpl_flip_y" => self.cpl_flip_y = flag(line, key, v)?,
            "cpl_rotation_cw" => self.cpl_rotation_cw = flag(line, key, v)?,
            "cpl_rotation_offset_deg" => self.cpl_rotation_offset_deg = n(-360.0, 360.0)?,
            "cpl_bottom_rotation" => {
                self.cpl_bottom_rotation = match v.to_ascii_lowercase().as_str() {
                    "keep" => BottomRotation::Keep,
                    "mirror" => BottomRotation::Mirror,
                    "half_turn" => BottomRotation::HalfTurn,
                    _ => return Err(bad(line, key, v, "keep, mirror or half_turn")),
                }
            }
            "ipc_density" => {
                self.ipc_density = crate::ipcname::Density::parse(v)
                    .ok_or_else(|| bad(line, key, v, "L, N or M"))?
            }
            "ipc_search_limit" => self.ipc_search_limit = int(line, key, v, 1, 1000)? as u32,
            "ipc_search_distance" => self.ipc_search_distance = int(line, key, v, 0, 8)? as u32,
            "dfm_verified" => self.dfm_verified = flag(line, key, v)?,
            "dfm_pad_pitch_mm" => self.dfm_pad_pitch_mm = n(0.05, 20.0)?,
            "dfm_pad_gap_min_mm" => self.dfm_pad_gap_min_mm = n(0.0, 5.0)?,
            "dfm_smt_gap_mm" => self.dfm_smt_gap_mm = n(0.0, 20.0)?,
            "dfm_track_center_frac" => self.dfm_track_center_frac = n(0.0, 1.0)?,
            "dfm_tooling_edge_mm" => self.dfm_tooling_edge_mm = n(0.0, 50.0)?,
            "dfm_smt_tooling_edge_mm" => self.dfm_smt_tooling_edge_mm = n(0.0, 50.0)?,
            "dfm_tooling_hole_mm" => self.dfm_tooling_hole_mm = n(0.1, 10.0)?,
            "dfm_hole_tol_mm" => self.dfm_hole_tol_mm = n(0.0, 2.0)?,
            "dfm_fid_diameter_mm" => self.dfm_fid_diameter_mm = n(0.1, 5.0)?,
            "dfm_fid_edge_mm" => self.dfm_fid_edge_mm = n(0.0, 50.0)?,
            "dfm_fid_ring_mm" => self.dfm_fid_ring_mm = n(0.0, 20.0)?,
            "dfm_rail_min_mm" => self.dfm_rail_min_mm = n(0.0, 50.0)?,
            "stencil_thickness_mm" => self.stencil_thickness_mm = n(0.03, 0.5)?,
            "stencil_thickness_min_mm" => self.stencil_thickness_min_mm = n(0.03, 0.5)?,
            "stencil_thickness_max_mm" => self.stencil_thickness_max_mm = n(0.03, 0.5)?,
            "stencil_reduction_percent" => self.stencil_reduction_percent = n(0.0, 50.0)?,
            "stencil_reduction_min_percent" => self.stencil_reduction_min_percent = n(0.0, 50.0)?,
            "stencil_reduction_max_percent" => self.stencil_reduction_max_percent = n(0.0, 50.0)?,
            "stencil_fine_reduction_percent" => self.stencil_fine_reduction_percent = n(0.0, 50.0)?,
            "stencil_fine_reduction_min_percent" => {
                self.stencil_fine_reduction_min_percent = n(0.0, 50.0)?
            }
            "stencil_fine_reduction_max_percent" => {
                self.stencil_fine_reduction_max_percent = n(0.0, 50.0)?
            }
            "stencil_fine_pitch_mm" => self.stencil_fine_pitch_mm = n(0.1, 5.0)?,
            "stencil_area_ratio_min" => self.stencil_area_ratio_min = n(0.1, 3.0)?,
            "stencil_thermal_percent" => self.stencil_thermal_percent = n(1.0, 100.0)?,
            "stencil_thermal_min_percent" => self.stencil_thermal_min_percent = n(1.0, 100.0)?,
            "stencil_thermal_max_percent" => self.stencil_thermal_max_percent = n(1.0, 100.0)?,
            "stencil_thermal_side_mm" => self.stencil_thermal_side_mm = n(0.5, 50.0)?,
            "stencil_window_max_mm" => self.stencil_window_max_mm = n(0.3, 20.0)?,
            "stencil_web_mm" => self.stencil_web_mm = n(0.05, 5.0)?,
            "tower_idle_gap_s" => self.tower_idle_gap_s = n(0.0, 3600.0)?,
            "tower_slow_factor" => self.tower_slow_factor = n(1.0, 100.0)?,
            "busy_gaps_listed" => self.busy_gaps_listed = int(line, key, v, 1, 1000)? as u32,
            "lml_max_step_mm" => self.lml_max_step_mm = n(0.01, 20.0)?,
            "lml_tolerance_mm" => self.lml_tolerance_mm = n(0.001, 5.0)?,
            "lml_max_iterations" => self.lml_max_iterations = int(line, key, v, 1, 20)? as u32,
            "lml_gain" => self.lml_gain = n(0.05, 1.0)?,
            "lml_offset_x_mm" => self.lml_offset_x_mm = n(-50.0, 50.0)?,
            "lml_offset_y_mm" => self.lml_offset_y_mm = n(-50.0, 50.0)?,
            "vacuum_margin_pct" => self.vacuum_margin_pct = n(0.0, 45.0)?,
            "vacuum_min_gap_kpa" => self.vacuum_min_gap_kpa = n(0.0, 100.0)?,
            "vacuum_min_readings" => self.vacuum_min_readings = int(line, key, v, 1, 1000)? as u32,
            "testboard_pitch_mm" => self.testboard_pitch_mm = n(0.2, 100.0)?,
            "testboard_count" => self.testboard_count = int(line, key, v, 1, 5000)? as u32,
            "slot_pitch_mm" => self.slot_pitch_mm = n(0.5, 50.0)?,
            "slot_gap_mm" => self.slot_gap_mm = n(0.0, 50.0)?,
            "slot_bank_slots" => self.slot_bank_slots = int(line, key, v, 1, 256)? as u32,
            "slot_reach_mm" => self.slot_reach_mm = n(1.0, 5000.0)?,
            "node_silence_limit" => self.node_silence_limit = int(line, key, v, 1, 100)? as u32,
            "node_timeout_s" => self.node_timeout_s = int(line, key, v, 1, 120)? as u64,
            "panel_clearance_mm" => self.panel_clearance_mm = n(0.0, 50.0)?,
            "spec_table" => self.spec_table = server_text(line, key, v, 80)?,
            "spec_machine" => self.spec_machine = server_text(line, key, v, 80)?,
            "spec_work_area" => self.spec_work_area = server_text(line, key, v, 80)?,
            "spec_smallest_part" => self.spec_smallest_part = server_text(line, key, v, 80)?,
            "spec_max_speed_mm_s" => self.spec_max_speed_mm_s = n(0.0, 100_000.0)?,
            "heightmap_max_warp_mm" => self.heightmap_max_warp_mm = n(0.05, 20.0)?,
            "drag_hole_pitch_mm" => self.drag_hole_pitch_mm = n(1.0, 16.0)?,
            "drag_max_picks_per_stroke" => {
                self.drag_max_picks_per_stroke = int(line, key, v, 1, 16)? as u32
            }
            "backlash_x_mm" => self.backlash_x_mm = n(0.0, 2.0)?,
            "backlash_y_mm" => self.backlash_y_mm = n(0.0, 2.0)?,
            "backlash_mode" => {
                self.backlash_mode =
                    BacklashMode::parse(v).ok_or_else(|| bad(line, key, v, "off or approach"))?
            }
            "polarity_refs" => self.polarity_refs = names(line, key, v)?,
            "polarity_packages" => self.polarity_packages = names(line, key, v)?,
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
