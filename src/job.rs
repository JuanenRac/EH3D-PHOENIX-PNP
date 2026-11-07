//! A job: a plan carried out, step by step, on the simulated machine. The job is a list of small steps (pick the part of a nozzle, place the part of a nozzle); each step does one thing and
//! says what comes next, and almost nothing is kept besides the cursor and the state of the parts, so a job can be paused, taken one step at a time, saved, and resumed later in
//! a new process, and it goes on from the same place with the same failures.
//!
//! What a job does when something fails:
//! * a feed or a pick is tried again as many times as the configuration says; a place too. The attempts are the retries plus the first one.
//! * when the attempts are used up, the policy decides: `alert` stops the job (the error says what and which, and `resume` tries the step again) and `defer` puts the part aside, goes
//!   on, and lists the parts put aside at the end.
//! * each feeder remembers how its last picks went; when too many failed it is switched off and the parts it held are taken from another feeder of the same kind, if there is one.
//! * a nozzle never picks while it holds a part and never places while it holds none.
//! * with `fault_action = alert` a feeder that reaches its limit of faults stops the job (`PHOENIX-PNP-047`) instead of being switched off, and the person decides.
//! * with `vacuum = on` the vacuum of the nozzle is read after every pick (no part: the pick failed) and after every place (the part still there: the place failed).

use crate::config::{ErrorPolicy, FaultAction, MachineConfig};
use crate::error::{Code, PnpError};
use crate::feeding::{choose, FeederSpec};
use crate::model::{Actuator, Camera, FeederDevice, Machine, Nozzle, Offset};
use crate::planner::AdvancedPlan;
use crate::rotation::glob_match;
use crate::sim::{SimActuator, SimCamera, SimFeeder, SimMachine, SimOp, SimVacuum};
use crate::vision::VisionConfig;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Pending,
    Placed,
    /// Put aside: the policy `defer` did it, or the person did it from the error dialog.
    Deferred,
    /// Left out by the person from the error dialog.
    Skipped,
}

impl Status {
    fn name(self) -> &'static str {
        match self {
            Status::Pending => "pending",
            Status::Placed => "placed",
            Status::Deferred => "deferred",
            Status::Skipped => "skipped",
        }
    }

    fn parse(text: &str) -> Option<Status> {
        match text {
            "pending" => Some(Status::Pending),
            "placed" => Some(Status::Placed),
            "deferred" => Some(Status::Deferred),
            "skipped" => Some(Status::Skipped),
            _ => None,
        }
    }
}

/// What the person can do with a job that an error stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Try the step that failed again.
    Retry,
    /// Leave the part out of the job.
    Skip,
    /// Put the part aside, to be placed by hand later.
    Defer,
    /// Switch the feeder of the failed step off, and try the step again with another one.
    SwitchOffFeeder,
}

impl Action {
    pub const ALL: [Action; 4] = [
        Action::Retry,
        Action::Skip,
        Action::Defer,
        Action::SwitchOffFeeder,
    ];

    /// The name used in the reports.
    pub fn name(self) -> &'static str {
        match self {
            Action::Retry => "retry",
            Action::Skip => "skip",
            Action::Defer => "defer",
            Action::SwitchOffFeeder => "switch_off_feeder",
        }
    }

    fn slot(self) -> usize {
        match self {
            Action::Retry => 0,
            Action::Skip => 1,
            Action::Defer => 2,
            Action::SwitchOffFeeder => 3,
        }
    }
}

/// The step that an error stopped.
#[derive(Debug, Clone, PartialEq)]
pub struct Failure {
    /// The index of the placement in the plan.
    pub index: usize,
    pub reference: String,
    /// The feeder the part comes from (the one that was in use, when the stop was in a pick).
    pub feeder: String,
    pub nozzle: usize,
    /// True when the stop was in the pick, false when it was in the place.
    pub at_pick: bool,
    pub error: PnpError,
}

/// What the job keeps for its report besides the statistics: where the time went, how the picks went at the first try, what the camera read and which errors came.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Trace {
    /// Microseconds of the machine clock spent in the feed and the pick of the parts (without the moves, the looks of the camera and the change of tip).
    pub pick_us: u64,
    /// ... in the looks of the camera after the picks.
    pub align_us: u64,
    /// ... in the places (without the moves).
    pub place_us: u64,
    /// ... in the moves of the head.
    pub travel_us: u64,
    /// ... in the changes of the nozzle tip.
    pub tip_us: u64,
    /// Parts picked at the first attempt.
    pub first_try: u32,
    /// Parts picked after one or more attempts that failed.
    pub retried: u32,
    /// Parts that were never picked: the feed or every attempt failed, or no feeder was left.
    pub lost: u32,
    /// What the camera read first on each corrected part, in the order of the picks.
    pub readings: Vec<Offset>,
    /// How many times each error was raised, by the number of its code.
    pub codes: BTreeMap<u16, u32>,
    /// How many times the person did each of the actions of [`Action::ALL`].
    pub actions: [u32; 4],
}

impl Trace {
    /// The microseconds that the phases add up to.
    pub fn total_us(&self) -> u64 {
        self.pick_us + self.align_us + self.place_us + self.travel_us + self.tip_us
    }

    fn count(&mut self, code: Code) {
        *self.codes.entry(code.number()).or_insert(0) += 1;
    }
}

/// Where the job is: the next step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cursor {
    Pick { batch: usize, k: usize },
    Place { batch: usize, k: usize },
    Finished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// It can run.
    Ready,
    /// It was paused: `run` does nothing, `step` goes one step.
    Paused,
    /// An error stopped it: `resume` clears the error and the step is tried again.
    Stopped,
    Finished,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct FeederStats {
    pub id: String,
    pub picked: u32,
    pub faults: u32,
    pub disabled: bool,
    /// Every pick from this feeder that failed, also those that a retry put right.
    pub pick_failures: u32,
}

/// What happened with the parts of one package.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct KindStats {
    pub package: String,
    pub picked: u32,
    pub pick_failures: u32,
    pub placed: u32,
    pub place_failures: u32,
}

/// The chance that one action fails when it is done on a part in a package and, for a pick, from a feeder: the failure of the package or of the feeder, whichever comes first.
pub fn combined_chance(a: f64, b: f64) -> f64 {
    1.0 - (1.0 - a) * (1.0 - b)
}

/// The header of `Job::stats_csv`: it never changes, so a sheet that reads the report keeps working.
pub const STATS_CSV_HEADER: &str =
    "section,name,value,picked,pick_failures,placed,place_failures,faults,disabled";

/// What the camera did with the parts of the job (all zero when the align step is off).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AlignStats {
    /// Parts that the camera found within the tolerance and whose place was corrected.
    pub corrected: u32,
    /// Parts thrown away because the camera found them further from the nozzle than `pick_tolerance_mm`.
    pub rejected: u32,
    /// Looks of the camera that gave no reading (the part was thrown away too).
    pub camera_failures: u32,
    /// The sum of the first readings of the corrected parts, in millimetres: how far each was from the nozzle.
    pub sum_mm: f64,
    /// The largest of those readings.
    pub max_mm: f64,
    /// The largest turn that was corrected, in degrees (without its sign).
    pub max_deg: f64,
}

impl AlignStats {
    /// The mean distance from the nozzle of the corrected parts, in millimetres; 0 when none was corrected.
    pub fn mean_mm(&self) -> f64 {
        if self.corrected > 0 {
            self.sum_mm / f64::from(self.corrected)
        } else {
            0.0
        }
    }
}

/// What a saved job keeps of the align step: the statistics, the generator and the looks of the camera, and the correction found for the part on each nozzle.
type AlignState = (AlignStats, (u64, u64), Vec<Option<Offset>>);

/// What the vacuum checks found (all zero when `vacuum` is off).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VacuumStats {
    /// Checks after a pick, and those that found no part on the nozzle.
    pub pick_checks: u32,
    pub pick_misses: u32,
    /// Checks after a place, and those that found the part still on the nozzle.
    pub place_checks: u32,
    pub place_stuck: u32,
    /// The lowest level (kilopascals) that said a part was held, and the highest that said it was gone.
    pub lowest_held_kpa: Option<f64>,
    pub highest_released_kpa: Option<f64>,
}

/// What a saved job keeps of the vacuum: the statistics, the generator and the counts of the checks, and the pump and the last level of each nozzle.
type VacuumState = (VacuumStats, (u64, u64, u64), Vec<bool>, Vec<f64>);

/// The statistics of the job.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Stats {
    pub placed: usize,
    pub deferred: usize,
    /// Parts the person left out from the error dialog.
    pub skipped: usize,
    pub feed_failures: u64,
    pub pick_attempts: u64,
    pub pick_failures: u64,
    pub place_failures: u64,
    pub tip_changes: usize,
    pub feeders: Vec<FeederStats>,
    /// By package, in the order of the first feeder that holds it.
    pub kinds: Vec<KindStats>,
    pub align: AlignStats,
    pub vacuum: VacuumStats,
}

#[derive(Debug, Clone)]
pub struct Job {
    plan: AdvancedPlan,
    feeders: Vec<FeederSpec>,
    cfg: MachineConfig,
    machine: SimMachine,
    devices: Vec<SimFeeder>,
    index: HashMap<String, usize>,
    batches: Vec<Vec<usize>>,
    hash: String,
    cursor: Cursor,
    paused: bool,
    error: Option<PnpError>,
    status: Vec<Status>,
    reasons: Vec<String>,
    taken: Vec<u32>,
    windows: Vec<Vec<bool>>,
    mounted: bool,
    stats: Stats,
    camera: SimCamera,
    vacuum: SimVacuum,
    /// The on/off devices of the machine, in the order of the configuration.
    actuators: Vec<SimActuator>,
    /// What the camera found on the part that each nozzle holds, to be corrected in the place.
    corrections: Vec<Option<Offset>>,
    trace: Trace,
    /// The feeder that served the last pick, to be switched off when that pick stops the job.
    last_feeder: Option<usize>,
    /// The part whose pick an error stopped, so that the pick tried again is not counted as one at the first attempt.
    stopped_pick: Option<usize>,
}

fn fnv1a(text: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn state_bad(message: impl Into<String>) -> PnpError {
    PnpError::new(Code::JobStateBad, message)
}

impl Job {
    /// A job that carries out `plan` with the feeders (the same list the plan was made with) on a simulated machine configured by `cfg`.
    pub fn new(
        plan: AdvancedPlan,
        feeders: Vec<FeederSpec>,
        cfg: MachineConfig,
    ) -> Result<Job, PnpError> {
        let machine = SimMachine::from_config(&cfg)?;
        let mut index = HashMap::new();
        for (n, f) in feeders.iter().enumerate() {
            index.insert(f.base.id.clone(), n);
        }
        let mut batches: Vec<Vec<usize>> = Vec::new();
        let mut text = String::new();
        for (i, p) in plan.plan.placements.iter().enumerate() {
            if !index.contains_key(&p.feeder) {
                return Err(PnpError::new(
                    Code::PlanSetting,
                    format!(
                        "{} comes from the feeder '{}', which is not in the list",
                        p.reference, p.feeder
                    ),
                ));
            }
            if p.nozzle >= cfg.nozzles {
                return Err(PnpError::new(
                    Code::PlanSetting,
                    format!(
                        "{} uses the nozzle {}, the machine has {}",
                        p.reference,
                        p.nozzle + 1,
                        cfg.nozzles
                    ),
                ));
            }
            if batches.len() <= p.batch {
                batches.resize(p.batch + 1, Vec::new());
            }
            batches[p.batch].push(i);
            text.push_str(&format!(
                "{}|{}|{:.6}|{:.6}|{:.6}|{}|{};",
                p.reference, p.feeder, p.x, p.y, p.rotation, p.nozzle, p.batch
            ));
        }
        for f in &feeders {
            text.push_str(&format!("{}:{};", f.base.id, f.base.count));
        }
        // a saved job of the align step is for the job with the step: a job without it keeps the number it always had
        if cfg.align {
            text.push_str("align;");
        }
        // the same for the vacuum: a job that checks it has failures that one without it has not
        if cfg.vacuum {
            text.push_str("vacuum;");
        }
        let batches: Vec<Vec<usize>> = batches.into_iter().filter(|b| !b.is_empty()).collect();
        let devices: Vec<SimFeeder> = feeders
            .iter()
            .enumerate()
            .map(|(n, f)| {
                SimFeeder::new(
                    &f.base.id,
                    f.base.count,
                    cfg.sim_seed,
                    n,
                    cfg.sim_feed_fail_rate,
                )
            })
            .collect();
        let count = plan.plan.placements.len();
        let mut kinds: Vec<KindStats> = Vec::new();
        for f in &feeders {
            let key = f.base.package.trim();
            if !kinds.iter().any(|k| k.package.eq_ignore_ascii_case(key)) {
                kinds.push(KindStats {
                    package: key.to_string(),
                    ..KindStats::default()
                });
            }
        }
        let stats = Stats {
            kinds,
            feeders: feeders
                .iter()
                .map(|f| FeederStats {
                    id: f.base.id.clone(),
                    ..FeederStats::default()
                })
                .collect(),
            ..Stats::default()
        };
        let cursor = if batches.is_empty() {
            Cursor::Finished
        } else {
            Cursor::Pick { batch: 0, k: 0 }
        };
        let camera = SimCamera::from_config(&cfg);
        let vacuum = SimVacuum::from_config(&cfg);
        let actuators: Vec<SimActuator> = cfg
            .actuators
            .iter()
            .map(|a| SimActuator::new(&a.name, a.delay_s))
            .collect();
        let corrections = vec![None; cfg.nozzles];
        Ok(Job {
            plan,
            windows: vec![Vec::new(); feeders.len()],
            taken: vec![0; feeders.len()],
            feeders,
            cfg,
            machine,
            devices,
            index,
            batches,
            hash: format!("{:016x}", fnv1a(&text)),
            cursor,
            paused: false,
            error: None,
            status: vec![Status::Pending; count],
            reasons: vec![String::new(); count],
            mounted: false,
            stats,
            camera,
            vacuum,
            actuators,
            corrections,
            trace: Trace::default(),
            last_feeder: None,
            stopped_pick: None,
        })
    }

    /// The index in the statistics of the package of the feeder `f`.
    fn kind_of(&self, f: usize) -> usize {
        let package = self.feeders[f].base.package.trim();
        self.stats
            .kinds
            .iter()
            .position(|k| k.package.eq_ignore_ascii_case(package))
            .unwrap_or(0)
    }

    /// The chances of failure that the configuration gives to the package of a feeder: `(pick, place)`.
    fn part_chances(&self, f: usize) -> (f64, f64) {
        let package = self.feeders[f].base.package.trim();
        self.cfg
            .sim_part_fail
            .iter()
            .find(|(pattern, _, _)| glob_match(pattern, package))
            .map_or((0.0, 0.0), |(_, pick, place)| (*pick, *place))
    }

    /// The chance that a pick from the feeder `f` fails because of the feeder.
    fn feeder_chance(&self, f: usize) -> f64 {
        let id = &self.feeders[f].base.id;
        self.cfg
            .sim_feeder_fail
            .iter()
            .find(|(feeder, _)| feeder == id)
            .map_or(0.0, |(_, pick)| *pick)
    }

    /// How many parts the simulated job places in an hour of machine time, from what it has placed so far. 0 before the machine has spent any time.
    pub fn parts_per_hour(&self) -> f64 {
        let seconds = self.machine.now_us() as f64 / 1e6;
        if seconds > 0.0 {
            self.stats.placed as f64 * 3600.0 / seconds
        } else {
            0.0
        }
    }

    /// The simulated machine, for tests that make it fail at a chosen place.
    pub fn machine_mut(&mut self) -> &mut SimMachine {
        &mut self.machine
    }

    pub fn machine(&self) -> &SimMachine {
        &self.machine
    }

    /// The simulated camera, for tests that make a look fail at a chosen place.
    pub fn camera_mut(&mut self) -> &mut SimCamera {
        &mut self.camera
    }

    /// The simulated vacuum sensor, for tests that make a check fail at a chosen place.
    pub fn vacuum_mut(&mut self) -> &mut SimVacuum {
        &mut self.vacuum
    }

    /// The last vacuum level read on a nozzle, in kilopascals; `None` when `vacuum` is off.
    pub fn vacuum_level(&self, nozzle: usize) -> Option<f64> {
        self.cfg.vacuum.then(|| self.vacuum.level(nozzle))
    }

    /// The on/off devices of the machine, in the order of the configuration.
    pub fn actuators(&self) -> &[SimActuator] {
        &self.actuators
    }

    /// Switches a device of the machine. The seconds it takes to answer go on the clock of the machine; a name the machine does not have is refused.
    pub fn set_actuator(&mut self, name: &str, on: bool) -> Result<(), PnpError> {
        let Some(n) = self.actuators.iter().position(|a| a.name() == name) else {
            return Err(PnpError::new(
                Code::ActuatorUnknown,
                format!("'{name}' is not one of the actuators of the machine"),
            ));
        };
        self.machine.wait(self.actuators[n].delay_s());
        self.actuators[n].set(on)?;
        self.machine.note(format!(
            "actuator {name} switched {}",
            if on { "on" } else { "off" }
        ));
        Ok(())
    }

    /// The settings of the vision pipeline for the simulated camera. They count only when the configuration says `vision = pipeline`; otherwise the camera reports its offset directly.
    pub fn set_vision(&mut self, settings: VisionConfig) {
        self.camera.set_vision(settings);
    }

    /// The simulated feeders, in the order of the list.
    pub fn devices_mut(&mut self) -> &mut [SimFeeder] {
        &mut self.devices
    }

    pub fn hash(&self) -> &str {
        &self.hash
    }

    pub fn cursor(&self) -> Cursor {
        self.cursor
    }

    pub fn error(&self) -> Option<&PnpError> {
        self.error.as_ref()
    }

    pub fn status(&self) -> &[Status] {
        &self.status
    }

    pub fn stats(&self) -> &Stats {
        &self.stats
    }

    /// Where the time went, how the picks went at the first try, the readings of the camera and the errors raised.
    pub fn trace(&self) -> &Trace {
        &self.trace
    }

    /// The settings the job runs with.
    pub fn config(&self) -> &MachineConfig {
        &self.cfg
    }

    /// The reason a part was put aside or left out (empty for the others).
    pub fn reason(&self, index: usize) -> &str {
        self.reasons.get(index).map_or("", String::as_str)
    }

    /// The part on each nozzle that the camera has already looked at, with what it found: the head is on its way to place it.
    pub fn correction(&self, nozzle: usize) -> Option<Offset> {
        self.corrections.get(nozzle).copied().flatten()
    }

    /// The step that an error stopped, when one did.
    pub fn failure(&self) -> Option<Failure> {
        let error = self.error.clone()?;
        let (batch, k, at_pick) = match self.cursor {
            Cursor::Pick { batch, k } => (batch, k, true),
            Cursor::Place { batch, k } => (batch, k, false),
            Cursor::Finished => return None,
        };
        let index = *self.batches.get(batch)?.get(k)?;
        let pl = self.plan.plan.placements.get(index)?;
        let planned = *self.index.get(&pl.feeder)?;
        let used = if at_pick {
            self.last_feeder.unwrap_or(planned)
        } else {
            planned
        };
        Some(Failure {
            index,
            reference: pl.reference.clone(),
            feeder: self.feeders[used].base.id.clone(),
            nozzle: pl.nozzle,
            at_pick,
            error,
        })
    }

    /// What the person decides for a job that an error stopped. `Retry` clears the error and the step is tried again; `Skip` and `Defer` take the part out of the job (left out, or
    /// put aside to be placed by hand) and the job goes on with the next step; `SwitchOffFeeder` switches the feeder of the failed step off and tries the step again, so that another
    /// feeder of the same kind takes over. A job that is not stopped by an error refuses all of them.
    pub fn act(&mut self, action: Action) -> Result<(), PnpError> {
        let Some(failure) = self.failure() else {
            return Err(PnpError::new(
                Code::JobActionBad,
                "the job is not stopped by an error",
            ));
        };
        // the person has looked at the feeder: the faults that stopped the job are not counted again
        if failure.error.code == Code::FeederFaultWindow && action != Action::SwitchOffFeeder {
            if let Some(&f) = self.index.get(&failure.feeder) {
                self.windows[f].clear();
            }
        }
        match action {
            Action::Retry => self.resume(),
            Action::Skip | Action::Defer => {
                let (idx, nozzle) = (failure.index, failure.nozzle);
                if self.machine.holding(nozzle) == Some(failure.reference.as_str()) {
                    self.machine.discard(nozzle);
                }
                self.vacuum.release(nozzle);
                self.corrections[nozzle] = None;
                if failure.at_pick {
                    self.trace.lost += 1;
                }
                if self.status[idx] == Status::Pending {
                    if action == Action::Skip {
                        self.status[idx] = Status::Skipped;
                        self.stats.skipped += 1;
                    } else {
                        self.status[idx] = Status::Deferred;
                        self.stats.deferred += 1;
                    }
                    self.reasons[idx] = format!("by the person, after: {}", failure.error);
                }
                self.machine.note(format!(
                    "{} {} by the person",
                    failure.reference,
                    if action == Action::Skip {
                        "left out"
                    } else {
                        "put aside"
                    }
                ));
                self.cursor = self.next_cursor(self.cursor);
                self.paused = false;
                self.error = None;
            }
            Action::SwitchOffFeeder => {
                let f =
                    self.index.get(&failure.feeder).copied().ok_or_else(|| {
                        PnpError::new(Code::JobActionBad, "the feeder is not known")
                    })?;
                if !self.devices[f].is_enabled() {
                    return Err(PnpError::new(
                        Code::JobActionBad,
                        format!("the feeder '{}' is already switched off", failure.feeder),
                    ));
                }
                self.devices[f].set_enabled(false);
                self.stats.feeders[f].disabled = true;
                self.machine.note(format!(
                    "feeder {} switched off by the person",
                    failure.feeder
                ));
                self.resume();
            }
        }
        self.trace.actions[action.slot()] += 1;
        Ok(())
    }

    pub fn plan(&self) -> &AdvancedPlan {
        &self.plan
    }

    /// The parts put aside, with the reason.
    pub fn deferred(&self) -> Vec<(String, String)> {
        self.plan
            .plan
            .placements
            .iter()
            .enumerate()
            .filter(|(i, _)| self.status[*i] == Status::Deferred)
            .map(|(i, p)| (p.reference.clone(), self.reasons[i].clone()))
            .collect()
    }

    pub fn phase(&self) -> Phase {
        if self.cursor == Cursor::Finished {
            Phase::Finished
        } else if self.error.is_some() {
            Phase::Stopped
        } else if self.paused {
            Phase::Paused
        } else {
            Phase::Ready
        }
    }

    pub fn pause(&mut self) {
        self.paused = true;
    }

    /// Takes the pause away and clears the error of a job that was stopped, so that the step that failed is tried again.
    pub fn resume(&mut self) {
        self.paused = false;
        self.error = None;
    }

    fn next_cursor(&self, c: Cursor) -> Cursor {
        match c {
            Cursor::Pick { batch, k } => {
                if k + 1 < self.batches[batch].len() {
                    Cursor::Pick { batch, k: k + 1 }
                } else {
                    Cursor::Place { batch, k: 0 }
                }
            }
            Cursor::Place { batch, k } => {
                if k + 1 < self.batches[batch].len() {
                    Cursor::Place { batch, k: k + 1 }
                } else if batch + 1 < self.batches.len() {
                    Cursor::Pick {
                        batch: batch + 1,
                        k: 0,
                    }
                } else {
                    Cursor::Finished
                }
            }
            Cursor::Finished => Cursor::Finished,
        }
    }

    /// Does one step, also when the job is paused. A job that is finished or stopped by an error does not go on.
    pub fn step(&mut self) -> Result<(), PnpError> {
        if self.cursor == Cursor::Finished || self.error.is_some() {
            return Err(PnpError::new(
                Code::JobNotRunnable,
                "the job is finished or stopped by an error",
            ));
        }
        let started = self.machine.now_us();
        let inside = self.trace.travel_us + self.trace.align_us + self.trace.tip_us;
        let at_pick = matches!(self.cursor, Cursor::Pick { .. });
        let picking = match self.cursor {
            Cursor::Pick { batch, k } => Some(self.batches[batch][k]),
            _ => None,
        };
        let done = match self.cursor {
            Cursor::Pick { batch, k } => self.do_pick(batch, k),
            Cursor::Place { batch, k } => self.do_place(batch, k),
            Cursor::Finished => Ok(()),
        };
        // the time of the step that the moves, the looks and the change of tip did not take is the time of the pick or of the place itself
        let spent = self.machine.now_us().saturating_sub(started);
        let counted =
            (self.trace.travel_us + self.trace.align_us + self.trace.tip_us).saturating_sub(inside);
        if at_pick {
            self.trace.pick_us += spent.saturating_sub(counted);
        } else {
            self.trace.place_us += spent.saturating_sub(counted);
        }
        match done {
            Ok(()) => {
                self.cursor = self.next_cursor(self.cursor);
                Ok(())
            }
            Err(e) => {
                self.trace.count(e.code);
                self.stopped_pick = picking;
                self.error = Some(e.clone());
                Err(e)
            }
        }
    }

    /// Moves the head, counting the time of the move.
    fn travel(&mut self, to: (f64, f64)) -> Result<(), PnpError> {
        let started = self.machine.now_us();
        let moved = self.machine.move_to(to);
        self.trace.travel_us += self.machine.now_us().saturating_sub(started);
        moved
    }

    /// Runs steps until the job finishes, is paused or is stopped by an error (returned as an error), or until `max_steps` steps have been done, when the job is paused.
    pub fn run(&mut self, max_steps: Option<usize>) -> Result<Phase, PnpError> {
        let mut done = 0usize;
        while self.phase() == Phase::Ready {
            if max_steps.is_some_and(|m| done >= m) {
                self.paused = true;
                break;
            }
            self.step()?;
            done += 1;
        }
        Ok(self.phase())
    }

    /// Notes how a pick from the feeder `f` went. When the faults of its window reach the limit the feeder is switched off, or, with `fault_action = alert`, the number of faults is given back
    /// so that the job stops and the person decides.
    fn record(&mut self, f: usize, fault: bool) -> Option<u32> {
        let cap = self.cfg.fault_window as usize;
        let w = &mut self.windows[f];
        w.push(fault);
        while w.len() > cap {
            w.remove(0);
        }
        let faults = w.iter().filter(|x| **x).count() as u32;
        if fault {
            self.stats.feeders[f].faults += 1;
        }
        if self.cfg.fault_limit > 0
            && faults >= self.cfg.fault_limit
            && self.devices[f].is_enabled()
        {
            if self.cfg.fault_action == FaultAction::Alert {
                return Some(faults);
            }
            self.devices[f].set_enabled(false);
            self.stats.feeders[f].disabled = true;
            let id = self.devices[f].id().to_string();
            self.machine
                .note(format!("feeder {id} switched off after {faults} faults"));
        }
        None
    }

    /// The error that stops the job when the faults of a feeder reach the limit and the person is to decide.
    fn window_error(&mut self, f: usize, faults: u32) -> PnpError {
        let id = self.devices[f].id().to_string();
        self.machine.note(format!(
            "feeder {id} has {faults} faults among its last {} picks: the job waits for the person",
            self.cfg.fault_window
        ));
        PnpError::new(
            Code::FeederFaultWindow,
            format!(
                "feeder '{id}': {faults} faults among its last {} picks",
                self.cfg.fault_window
            ),
        )
    }

    /// The vacuum is read after a pick: the pump has sucked for `vacuum_suck_s` seconds and the level must say that the part is held. A level below the threshold means the nozzle holds
    /// nothing: the part is let go of and the pick is a failure with `PHOENIX-PNP-044`.
    fn vacuum_after_pick(&mut self, nozzle: usize, reference: &str) -> Result<(), PnpError> {
        if !self.cfg.vacuum {
            return Ok(());
        }
        self.vacuum.suck(nozzle);
        self.machine.wait(self.vacuum.suck_s());
        let level = self.vacuum.read_after_pick(nozzle);
        let v = &mut self.stats.vacuum;
        v.pick_checks += 1;
        if self.vacuum.holds(level) {
            v.lowest_held_kpa = Some(v.lowest_held_kpa.map_or(level, |l| l.min(level)));
            self.machine.note(format!(
                "vacuum of nozzle {}: {level:.1} kPa, {reference} is held",
                nozzle + 1
            ));
            return Ok(());
        }
        v.pick_misses += 1;
        self.machine.note(format!(
            "vacuum of nozzle {}: {level:.1} kPa, no part on the nozzle",
            nozzle + 1
        ));
        self.machine.discard(nozzle);
        self.vacuum.release(nozzle);
        Err(PnpError::new(
            Code::VacuumNoPart,
            format!(
                "{reference} on nozzle {}: {level:.1} kPa, at least {} kPa was expected",
                nozzle + 1,
                self.cfg.vacuum_pick_min_kpa
            ),
        ))
    }

    /// The vacuum is read after a place: the nozzle has blown for `vacuum_blow_s` seconds and the level must say that the part is gone. A level above the threshold means the part is
    /// still stuck to the nozzle: the nozzle holds it again and the place is a failure with `PHOENIX-PNP-045`.
    fn vacuum_after_place(&mut self, nozzle: usize, reference: &str) -> Result<(), PnpError> {
        if !self.cfg.vacuum {
            return Ok(());
        }
        self.machine.wait(self.vacuum.blow_s());
        let level = self.vacuum.read_after_place(nozzle);
        let v = &mut self.stats.vacuum;
        v.place_checks += 1;
        if self.vacuum.gone(level) {
            v.highest_released_kpa = Some(v.highest_released_kpa.map_or(level, |l| l.max(level)));
            self.vacuum.release(nozzle);
            self.machine.note(format!(
                "vacuum of nozzle {}: {level:.1} kPa, {reference} let go",
                nozzle + 1
            ));
            return Ok(());
        }
        v.place_stuck += 1;
        self.machine.note(format!(
            "vacuum of nozzle {}: {level:.1} kPa, {reference} is still on the nozzle",
            nozzle + 1
        ));
        self.machine.regrip(nozzle, reference);
        Err(PnpError::new(
            Code::VacuumStuck,
            format!(
                "{reference} on nozzle {}: {level:.1} kPa, at most {} kPa was expected",
                nozzle + 1,
                self.cfg.vacuum_release_max_kpa
            ),
        ))
    }

    /// What the job does with a part that could not be picked or placed.
    fn give_up(&mut self, idx: usize, nozzle: usize, e: PnpError) -> Result<(), PnpError> {
        match self.cfg.error_policy {
            ErrorPolicy::Alert => Err(e),
            ErrorPolicy::Defer => {
                self.trace.count(e.code);
                self.machine.discard(nozzle);
                self.vacuum.release(nozzle);
                self.status[idx] = Status::Deferred;
                self.reasons[idx] = e.to_string();
                self.stats.deferred += 1;
                Ok(())
            }
        }
    }

    /// Whether a feeder is switched on, has not been switched off by its faults and still has parts.
    fn usable(&self, j: usize) -> bool {
        self.feeders[j].enabled && self.devices[j].is_enabled() && self.devices[j].remaining() > 0
    }

    fn do_pick(&mut self, batch: usize, k: usize) -> Result<(), PnpError> {
        let idx = self.batches[batch][k];
        if self.status[idx] != Status::Pending {
            return Ok(());
        }
        let earlier = self.stopped_pick == Some(idx);
        self.stopped_pick = None;
        let pl = self.plan.plan.placements[idx].clone();
        let det = self.plan.details[idx].clone();
        let nozzle = pl.nozzle;
        if let Some(held) = self.machine.holding(nozzle) {
            return Err(PnpError::new(
                Code::NozzleBusy,
                format!(
                    "nozzle {} already holds {held}: it cannot pick {}",
                    nozzle + 1,
                    pl.reference
                ),
            ));
        }
        if k == 0 {
            if !self.mounted {
                self.machine.mount_tip(&det.tip);
                self.mounted = true;
            } else if self.machine.tip() != det.tip {
                if let Some(changer) = self.cfg.tip_changer {
                    self.travel(changer)?;
                }
                let started = self.machine.now_us();
                self.machine.change_tip(&det.tip);
                self.trace.tip_us += self.machine.now_us().saturating_sub(started);
                self.stats.tip_changes += 1;
            }
        }
        let planned = self.index[&pl.feeder];
        let f = if self.usable(planned) {
            Some(planned)
        } else {
            let left: Vec<u32> = self
                .devices
                .iter()
                .map(|d| if d.is_enabled() { d.remaining() } else { 0 })
                .collect();
            choose(
                &self.feeders,
                &left,
                &self.taken,
                &self.feeders[planned].base.key(),
                (pl.x, pl.y),
            )
        };
        let Some(f) = f else {
            let e = PnpError::new(
                Code::FeederUnavailable,
                format!(
                    "{}: no feeder is available for it (the planned one is '{}')",
                    pl.reference, pl.feeder
                ),
            );
            if self.cfg.error_policy == ErrorPolicy::Defer {
                self.trace.lost += 1;
            }
            return self.give_up(idx, nozzle, e);
        };
        self.last_feeder = Some(f);
        let at = self.feeders[f].pick_point(self.taken[f]);
        self.travel(at)?;
        if !self.feed_part(f)? {
            if let Some(faults) = self.record(f, true) {
                return Err(self.window_error(f, faults));
            }
            let e = PnpError::new(
                Code::FeedFailed,
                format!("feeder '{}' for {}", self.feeders[f].base.id, pl.reference),
            );
            if self.cfg.error_policy == ErrorPolicy::Defer {
                self.trace.lost += 1;
            }
            return self.give_up(idx, nozzle, e);
        }
        let mut picked = false;
        // the attempt that picked the part (0 is the first)
        let mut won_at = 0u32;
        // why the last attempt was lost, when the camera or the vacuum was the cause
        let mut align_error: Option<PnpError> = None;
        self.corrections[nozzle] = None;
        let kind = self.kind_of(f);
        let chance = combined_chance(self.part_chances(f).0, self.feeder_chance(f));
        self.machine.set_extra_fail(SimOp::Pick, chance);
        for attempt in 0..=det.pick_retries {
            self.stats.pick_attempts += 1;
            let mut result = self.machine.pick(nozzle, &pl.reference);
            if result.is_ok() {
                if let Err(e) = self.vacuum_after_pick(nozzle, &pl.reference) {
                    result = Err(e);
                }
            }
            match result {
                Ok(()) if !self.cfg.align => {
                    picked = true;
                    won_at = attempt;
                    break;
                }
                Ok(()) => match self.align_part(nozzle) {
                    Ok(correction) => {
                        self.corrections[nozzle] = Some(correction);
                        picked = true;
                        won_at = attempt;
                        break;
                    }
                    Err(e) => {
                        // the part is thrown away, so it is gone from the feeder: the next one is brought for the next attempt
                        self.machine.discard(nozzle);
                        self.devices[f].take();
                        self.taken[f] += 1;
                        align_error = Some(e);
                        if attempt < det.pick_retries {
                            if !self.usable(f) {
                                break;
                            }
                            let at = self.feeders[f].pick_point(self.taken[f]);
                            self.travel(at)?;
                            if !self.feed_part(f)? {
                                break;
                            }
                        }
                    }
                },
                Err(e) if e.code == Code::SimFault || e.code == Code::VacuumNoPart => {
                    // the vacuum found no part: that is why the last attempt was lost
                    align_error = (e.code == Code::VacuumNoPart).then(|| e.clone());
                    self.trace.count(e.code);
                    self.stats.pick_failures += 1;
                    self.stats.kinds[kind].pick_failures += 1;
                    self.stats.feeders[f].pick_failures += 1;
                }
                Err(e) => return Err(e),
            }
        }
        self.machine.set_extra_fail(SimOp::Pick, 0.0);
        if let Some(faults) = self.record(f, !picked) {
            return Err(self.window_error(f, faults));
        }
        if picked {
            if won_at == 0 && !earlier {
                self.trace.first_try += 1;
            } else {
                self.trace.retried += 1;
            }
        } else if self.cfg.error_policy == ErrorPolicy::Defer {
            self.trace.lost += 1;
        }
        if !picked {
            let e = align_error.unwrap_or_else(|| {
                PnpError::new(
                    Code::PickFailed,
                    format!("{} from feeder '{}'", pl.reference, self.feeders[f].base.id),
                )
            });
            return self.give_up(idx, nozzle, e);
        }
        self.devices[f].take();
        self.taken[f] += 1;
        self.stats.feeders[f].picked += 1;
        self.stats.kinds[kind].picked += 1;
        Ok(())
    }

    /// Brings the next part of the feeder `f` to its picking place, trying again as many times as the configuration says. False when every attempt failed.
    fn feed_part(&mut self, f: usize) -> Result<bool, PnpError> {
        for _ in 0..=self.cfg.feed_retries {
            self.machine.wait(self.cfg.feed_s);
            match self.devices[f].feed() {
                Ok(()) => return Ok(true),
                Err(e) if e.code == Code::SimFault => {
                    self.trace.count(e.code);
                    self.stats.feed_failures += 1;
                }
                Err(e) => return Err(e),
            }
        }
        Ok(false)
    }

    /// The align step: the part that the nozzle has just picked is looked at by the camera `align_passes` times, waiting `align_settle_s` before each look, and what the camera sees
    /// is corrected each time. The first reading decides whether the part is good enough (it is not when it is further from the nozzle than `pick_tolerance_mm`). Gives the sum of the
    /// corrections, which the place applies.
    fn align_part(&mut self, nozzle: usize) -> Result<Offset, PnpError> {
        let started = self.machine.now_us();
        let looked = self.align_looks(nozzle);
        self.trace.align_us += self.machine.now_us().saturating_sub(started);
        if let Err(e) = &looked {
            self.trace.count(e.code);
        }
        looked
    }

    fn align_looks(&mut self, nozzle: usize) -> Result<Offset, PnpError> {
        self.camera.clear();
        let mut total = Offset::default();
        let mut first = Offset::default();
        let mut first_mm: f64 = 0.0;
        for pass in 0..self.cfg.align_passes {
            self.machine.wait(self.cfg.align_settle_s);
            let seen = match self.camera.measure(nozzle) {
                Ok(o) => o,
                Err(e) if e.code == Code::SimFault => {
                    self.stats.align.camera_failures += 1;
                    self.machine.note(format!(
                        "the camera gave no reading of the part on nozzle {}",
                        nozzle + 1
                    ));
                    return Err(PnpError::new(Code::AlignFailed, e.detail));
                }
                // the pipeline of the vision found no part in the picture: a look without a reading, with the code that says why
                Err(e)
                    if matches!(
                        e.code,
                        Code::VisionNotFound
                            | Code::VisionAreaOutOfRange
                            | Code::VisionSeveralSpots
                    ) =>
                {
                    self.stats.align.camera_failures += 1;
                    self.machine.note(format!(
                        "the vision found no part on nozzle {}: {}",
                        nozzle + 1,
                        e.code.label()
                    ));
                    return Err(e);
                }
                Err(e) => return Err(e),
            };
            let away_mm: f64 = seen.dx_mm.hypot(seen.dy_mm);
            if pass == 0 {
                first = seen;
                first_mm = away_mm;
                if away_mm > self.cfg.pick_tolerance_mm {
                    self.stats.align.rejected += 1;
                    self.machine.note(format!(
                        "the part on nozzle {} is {:.3} mm from the nozzle: thrown away",
                        nozzle + 1,
                        away_mm
                    ));
                    return Err(PnpError::new(
                        Code::AlignRejected,
                        format!(
                            "{:.3} mm from the nozzle, the tolerance is {} mm",
                            away_mm, self.cfg.pick_tolerance_mm
                        ),
                    ));
                }
            }
            total.dx_mm += seen.dx_mm;
            total.dy_mm += seen.dy_mm;
            total.rotation_deg += seen.rotation_deg;
            self.camera.apply(seen);
            self.machine.note(format!(
                "look {} at nozzle {}: X {:.3} Y {:.3} mm, turned {:.3} degrees",
                pass + 1,
                nozzle + 1,
                seen.dx_mm,
                seen.dy_mm,
                seen.rotation_deg
            ));
        }
        let a = &mut self.stats.align;
        a.corrected += 1;
        a.sum_mm += first_mm;
        a.max_mm = a.max_mm.max(first_mm);
        a.max_deg = a.max_deg.max(total.rotation_deg.abs());
        self.trace.readings.push(first);
        Ok(total)
    }

    fn do_place(&mut self, batch: usize, k: usize) -> Result<(), PnpError> {
        let idx = self.batches[batch][k];
        if self.status[idx] != Status::Pending {
            return Ok(());
        }
        let pl = self.plan.plan.placements[idx].clone();
        let nozzle = pl.nozzle;
        match self.machine.holding(nozzle) {
            None => {
                return Err(PnpError::new(
                    Code::NozzleEmpty,
                    format!(
                        "nozzle {} holds nothing: {} cannot be placed",
                        nozzle + 1,
                        pl.reference
                    ),
                ))
            }
            Some(h) if h != pl.reference => {
                return Err(PnpError::new(
                    Code::NozzleBusy,
                    format!("nozzle {} holds {h}, not {}", nozzle + 1, pl.reference),
                ))
            }
            Some(_) => {}
        }
        // the camera found the part off the nozzle by this much: the head goes to the place less that, and the nozzle turns less that
        let correction = self.corrections[nozzle];
        let target = match correction {
            Some(c) => (pl.x - c.dx_mm, pl.y - c.dy_mm),
            None => (pl.x, pl.y),
        };
        self.travel(target)?;
        if let Some(c) = correction {
            self.machine.note(format!(
                "nozzle {} places {} turned {:.3} degrees",
                nozzle + 1,
                pl.reference,
                pl.rotation - c.rotation_deg
            ));
        }
        let source = self.index[&pl.feeder];
        let kind = self.kind_of(source);
        let place_chance = self.part_chances(source).1;
        self.machine.set_extra_fail(SimOp::Place, place_chance);
        // why the last attempt was lost, when the vacuum was the cause
        let mut vacuum_error: Option<PnpError> = None;
        for _ in 0..=self.cfg.place_retries {
            let mut result = self.machine.place(nozzle);
            if result.is_ok() {
                if let Err(e) = self.vacuum_after_place(nozzle, &pl.reference) {
                    result = Err(e);
                }
            }
            match result {
                Ok(_) => {
                    self.machine.set_extra_fail(SimOp::Place, 0.0);
                    self.status[idx] = Status::Placed;
                    self.corrections[nozzle] = None;
                    self.stats.placed += 1;
                    self.stats.kinds[kind].placed += 1;
                    return Ok(());
                }
                Err(e) if e.code == Code::SimFault || e.code == Code::VacuumStuck => {
                    vacuum_error = (e.code == Code::VacuumStuck).then(|| e.clone());
                    self.trace.count(e.code);
                    self.stats.place_failures += 1;
                    self.stats.kinds[kind].place_failures += 1;
                }
                Err(e) => return Err(e),
            }
        }
        self.machine.set_extra_fail(SimOp::Place, 0.0);
        // with `alert` the part stays on the nozzle, so that the step can be tried again after the cause is put right
        let e =
            vacuum_error.unwrap_or_else(|| PnpError::new(Code::PlaceFailed, pl.reference.clone()));
        let given_up = self.give_up(idx, nozzle, e);
        if given_up.is_ok() {
            self.corrections[nozzle] = None;
        }
        given_up
    }

    /// The statistics as JSON.
    pub fn stats_json(&self) -> String {
        let s = &self.stats;
        let seconds = self.machine.now_us() as f64 / 1e6;
        let feeders: Vec<Value> = s
            .feeders
            .iter()
            .map(|f| json!({"id": f.id, "picked": f.picked, "faults": f.faults, "disabled": f.disabled, "pick_failures": f.pick_failures}))
            .collect();
        let kinds: Vec<Value> = s
            .kinds
            .iter()
            .map(|k| json!({"package": k.package, "picked": k.picked, "pick_failures": k.pick_failures, "placed": k.placed, "place_failures": k.place_failures}))
            .collect();
        let deferred: Vec<Value> = self
            .deferred()
            .into_iter()
            .map(|(r, why)| json!({"reference": r, "reason": why}))
            .collect();
        let mut doc = json!({
            "schema_version": 1,
            "placed": s.placed,
            "deferred": s.deferred,
            "of": self.status.len(),
            "seconds": seconds,
            "seconds_per_part": if s.placed > 0 { seconds / s.placed as f64 } else { 0.0 },
            "parts_per_hour": self.parts_per_hour(),
            "feed_failures": s.feed_failures,
            "pick_attempts": s.pick_attempts,
            "pick_failures": s.pick_failures,
            "place_failures": s.place_failures,
            "tip_changes": s.tip_changes,
            "feeders": feeders,
            "kinds": kinds,
            "deferred_parts": deferred
        });
        // only a job where the person left a part out reports it: the report of any other job is the one it always was
        if s.skipped > 0 {
            doc["skipped"] = json!(s.skipped);
        }
        // only a job with the align step reports it: the report of a job without it is the one it always was
        if self.cfg.align {
            doc["align"] = json!({
                "passes": self.cfg.align_passes,
                "pick_tolerance_mm": self.cfg.pick_tolerance_mm,
                "corrected": s.align.corrected,
                "rejected": s.align.rejected,
                "camera_failures": s.align.camera_failures,
                "mean_mm": s.align.mean_mm(),
                "max_mm": s.align.max_mm,
                "max_deg": s.align.max_deg
            });
        }
        // only a job with the vacuum checks reports them, and only one with actuators reports those: the report of any other job is the one it always was
        if self.cfg.vacuum {
            let v = &s.vacuum;
            doc["vacuum"] = json!({
                "suck_s": self.cfg.vacuum_suck_s,
                "blow_s": self.cfg.vacuum_blow_s,
                "pick_min_kpa": self.cfg.vacuum_pick_min_kpa,
                "release_max_kpa": self.cfg.vacuum_release_max_kpa,
                "pick_checks": v.pick_checks,
                "pick_misses": v.pick_misses,
                "place_checks": v.place_checks,
                "place_stuck": v.place_stuck,
                "lowest_held_kpa": v.lowest_held_kpa,
                "highest_released_kpa": v.highest_released_kpa,
                "nozzle_kpa": self.vacuum.levels()
            });
        }
        if !self.actuators.is_empty() {
            doc["actuators"] = Value::Array(
                self.actuators
                    .iter()
                    .map(|a| json!({"name": a.name(), "delay_s": a.delay_s(), "on": a.is_on(), "switches": a.switches()}))
                    .collect(),
            );
        }
        serde_json::to_string_pretty(&doc).unwrap_or_default() + "\n"
    }

    /// The report of the job as a CSV with the header `STATS_CSV_HEADER`: the `total` rows (name and value), a row for each package and each feeder, and a row for each part put aside
    /// with the reason in `value`.
    pub fn stats_csv(&self) -> String {
        let s = &self.stats;
        let seconds = self.machine.now_us() as f64 / 1e6;
        let mut out = format!("{STATS_CSV_HEADER}\n");
        let totals: [(&str, String); 10] = [
            ("placed", s.placed.to_string()),
            ("deferred", s.deferred.to_string()),
            ("of", self.status.len().to_string()),
            ("seconds", format!("{seconds:.3}")),
            ("parts_per_hour", format!("{:.1}", self.parts_per_hour())),
            ("feed_failures", s.feed_failures.to_string()),
            ("pick_attempts", s.pick_attempts.to_string()),
            ("pick_failures", s.pick_failures.to_string()),
            ("place_failures", s.place_failures.to_string()),
            ("tip_changes", s.tip_changes.to_string()),
        ];
        for (name, value) in totals {
            out.push_str(&format!("total,{name},{value},,,,,,\n"));
        }
        for k in &s.kinds {
            out.push_str(&format!(
                "package,{},,{},{},{},{},,\n",
                crate::parts::csv_field(&k.package),
                k.picked,
                k.pick_failures,
                k.placed,
                k.place_failures
            ));
        }
        for f in &s.feeders {
            out.push_str(&format!(
                "feeder,{},,{},{},,,{},{}\n",
                crate::parts::csv_field(&f.id),
                f.picked,
                f.pick_failures,
                f.faults,
                if f.disabled { "yes" } else { "no" }
            ));
        }
        for (reference, why) in self.deferred() {
            out.push_str(&format!(
                "deferred,{},{},,,,,,\n",
                crate::parts::csv_field(&reference),
                crate::parts::csv_field(&why)
            ));
        }
        // sections of their own, after the rows that were always there, and only for a job with the vacuum checks or with actuators
        if self.cfg.vacuum {
            let v = &s.vacuum;
            let level = |x: Option<f64>| x.map_or(String::new(), |l| format!("{l:.1}"));
            let rows: [(&str, String); 6] = [
                ("pick_checks", v.pick_checks.to_string()),
                ("pick_misses", v.pick_misses.to_string()),
                ("place_checks", v.place_checks.to_string()),
                ("place_stuck", v.place_stuck.to_string()),
                ("lowest_held_kpa", level(v.lowest_held_kpa)),
                ("highest_released_kpa", level(v.highest_released_kpa)),
            ];
            for (name, value) in rows {
                out.push_str(&format!("vacuum,{name},{value},,,,,,\n"));
            }
        }
        for a in &self.actuators {
            out.push_str(&format!(
                "actuator,{},{},,,,,{},\n",
                crate::parts::csv_field(a.name()),
                if a.is_on() { "on" } else { "off" },
                a.switches()
            ));
        }
        // a section of its own, after the rows that were always there, and only for a job with the align step
        if self.cfg.align {
            let a = &s.align;
            let rows: [(&str, String); 8] = [
                ("passes", self.cfg.align_passes.to_string()),
                (
                    "pick_tolerance_mm",
                    format!("{:.3}", self.cfg.pick_tolerance_mm),
                ),
                ("corrected", a.corrected.to_string()),
                ("rejected", a.rejected.to_string()),
                ("camera_failures", a.camera_failures.to_string()),
                ("mean_mm", format!("{:.3}", a.mean_mm())),
                ("max_mm", format!("{:.3}", a.max_mm)),
                ("max_deg", format!("{:.3}", a.max_deg)),
            ];
            for (name, value) in rows {
                out.push_str(&format!("align,{name},{value},,,,,,\n"));
            }
        }
        out
    }

    /// The state of the job as JSON, to resume it later: where it is, what each part is, what is left in each feeder, and the generators of the simulator.
    pub fn save_state(&self) -> String {
        let (rng, counts, clock) = self.machine.save();
        let holding: Vec<Value> = (0..self.machine.nozzle_count())
            .map(|n| match self.machine.holding(n) {
                Some(h) => json!(h),
                None => Value::Null,
            })
            .collect();
        let (kind, batch, k) = match self.cursor {
            Cursor::Pick { batch, k } => ("pick", batch, k),
            Cursor::Place { batch, k } => ("place", batch, k),
            Cursor::Finished => ("finished", 0, 0),
        };
        let devices: Vec<Value> = self
            .devices
            .iter()
            .enumerate()
            .map(|(n, d)| {
                let (r, a) = d.save();
                json!({
                    "remaining": d.remaining(), "enabled": d.is_enabled(),
                    "rng": r.to_string(), "attempts": a.to_string(),
                    "taken": self.taken[n], "window": self.windows[n]
                })
            })
            .collect();
        let s = &self.stats;
        let feeders: Vec<Value> = s
            .feeders
            .iter()
            .map(|f| json!({"picked": f.picked, "faults": f.faults, "disabled": f.disabled, "pick_failures": f.pick_failures}))
            .collect();
        let kinds: Vec<Value> = s
            .kinds
            .iter()
            .map(|k| json!({"picked": k.picked, "pick_failures": k.pick_failures, "placed": k.placed, "place_failures": k.place_failures}))
            .collect();
        let error = self
            .error
            .as_ref()
            .map(|e| json!({"code": e.code.number(), "detail": e.detail}));
        let status: Vec<&str> = self.status.iter().map(|s| s.name()).collect();
        let counts: Vec<String> = counts.iter().map(|c| c.to_string()).collect();
        let pos = self.machine.position();
        let cursor = json!({"kind": kind, "batch": batch, "k": k});
        let machine = json!({
            "rng": rng.to_string(),
            "counts": counts,
            "clock_us": clock.to_string(),
            "x": pos.0,
            "y": pos.1,
            "holding": holding,
            "tip": self.machine.tip()
        });
        let stats = json!({
            "placed": s.placed,
            "deferred": s.deferred,
            "skipped": s.skipped,
            "feed_failures": s.feed_failures,
            "pick_attempts": s.pick_attempts,
            "pick_failures": s.pick_failures,
            "place_failures": s.place_failures,
            "tip_changes": s.tip_changes,
            "feeders": feeders,
            "kinds": kinds
        });
        let mut doc = json!({
            "schema_version": 1,
            "hash": self.hash,
            "cursor": cursor,
            "paused": self.paused,
            "error": error,
            "status": status,
            "reasons": self.reasons,
            "mounted": self.mounted,
            "devices": devices,
            "machine": machine,
            "stats": stats
        });
        // the numbers are kept as the bits of the float, in a text, so that they come back exactly
        let bits_of = |x: f64| x.to_bits().to_string();
        let t = &self.trace;
        let readings: Vec<Value> = t
            .readings
            .iter()
            .map(|o| json!([bits_of(o.dx_mm), bits_of(o.dy_mm), bits_of(o.rotation_deg)]))
            .collect();
        let codes: Vec<Value> = t.codes.iter().map(|(c, n)| json!([c, n])).collect();
        doc["trace"] = json!({
            "pick_us": t.pick_us.to_string(),
            "look_us": t.align_us.to_string(),
            "place_us": t.place_us.to_string(),
            "travel_us": t.travel_us.to_string(),
            "tip_us": t.tip_us.to_string(),
            "first_try": t.first_try,
            "retried": t.retried,
            "lost": t.lost,
            "actions": t.actions,
            "codes": codes,
            "readings": readings
        });
        if self.cfg.vacuum {
            let (rng, picks, places) = self.vacuum.save();
            let v = &s.vacuum;
            let opt = |x: Option<f64>| x.map_or(Value::Null, |l| json!(bits_of(l)));
            doc["vacuum"] = json!({
                "rng": rng.to_string(),
                "picks": picks.to_string(),
                "places": places.to_string(),
                "pick_checks": v.pick_checks,
                "pick_misses": v.pick_misses,
                "place_checks": v.place_checks,
                "place_stuck": v.place_stuck,
                "lowest_held_kpa": opt(v.lowest_held_kpa),
                "highest_released_kpa": opt(v.highest_released_kpa),
                "on": self.vacuum.on_states(),
                "levels": self.vacuum.levels().iter().map(|l| bits_of(*l)).collect::<Vec<_>>()
            });
        }
        if !self.actuators.is_empty() {
            doc["actuators"] = Value::Array(
                self.actuators
                    .iter()
                    .map(|a| json!({"on": a.is_on(), "switches": a.switches()}))
                    .collect(),
            );
        }
        if self.cfg.align {
            let (rng, looks) = self.camera.save();
            let bits = |x: f64| x.to_bits().to_string();
            let corrections: Vec<Value> = self
                .corrections
                .iter()
                .map(|c| match c {
                    Some(o) => json!([bits(o.dx_mm), bits(o.dy_mm), bits(o.rotation_deg)]),
                    None => Value::Null,
                })
                .collect();
            doc["align"] = json!({
                "rng": rng.to_string(),
                "looks": looks.to_string(),
                "corrected": s.align.corrected,
                "rejected": s.align.rejected,
                "camera_failures": s.align.camera_failures,
                "sum_mm": bits(s.align.sum_mm),
                "max_mm": bits(s.align.max_mm),
                "max_deg": bits(s.align.max_deg),
                "corrections": corrections
            });
        }
        serde_json::to_string_pretty(&doc).unwrap_or_default() + "\n"
    }

    /// Puts a saved state back into a job made from the same plan and feeders. A state that belongs to another job, or that does not fit this one, is refused and the job is left as it was.
    pub fn restore_state(&mut self, text: &str) -> Result<(), PnpError> {
        let v: Value =
            serde_json::from_str(text).map_err(|e| state_bad(format!("not valid JSON: {e}")))?;
        if v.get("schema_version").and_then(Value::as_u64) != Some(1) {
            return Err(state_bad("schema_version is not 1"));
        }
        let hash = v
            .get("hash")
            .and_then(Value::as_str)
            .ok_or_else(|| state_bad("no hash"))?;
        if hash != self.hash {
            return Err(PnpError::new(
                Code::JobStateMismatch,
                format!("the state is for the job {hash}, this one is {}", self.hash),
            ));
        }
        let int = |v: &Value, key: &str| -> Result<usize, PnpError> {
            v.get(key)
                .and_then(Value::as_u64)
                .map(|n| n as usize)
                .ok_or_else(|| state_bad(format!("'{key}' is missing or is not a number")))
        };
        let text_u64 = |v: &Value, key: &str| -> Result<u64, PnpError> {
            v.get(key)
                .and_then(Value::as_str)
                .and_then(|s| s.parse::<u64>().ok())
                .ok_or_else(|| {
                    state_bad(format!("'{key}' is missing or is not a number in a text"))
                })
        };
        let list = |key: &str| {
            v.get(key)
                .and_then(Value::as_array)
                .ok_or_else(|| state_bad(format!("'{key}' is missing or is not a list")))
        };
        let c = v.get("cursor").ok_or_else(|| state_bad("no cursor"))?;
        let (batch, k) = (int(c, "batch")?, int(c, "k")?);
        let cursor = match c.get("kind").and_then(Value::as_str) {
            Some("pick") => Cursor::Pick { batch, k },
            Some("place") => Cursor::Place { batch, k },
            Some("finished") => Cursor::Finished,
            _ => {
                return Err(state_bad(
                    "the kind of the cursor is not pick, place or finished",
                ))
            }
        };
        if let Cursor::Pick { batch, k } | Cursor::Place { batch, k } = cursor {
            match self.batches.get(batch) {
                Some(b) if k < b.len() => {}
                _ => return Err(state_bad("the cursor points outside the plan")),
            }
        }
        let status: Vec<Status> = list("status")?
            .iter()
            .map(|s| s.as_str().and_then(Status::parse))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| state_bad("a status is not pending, placed, deferred or skipped"))?;
        let reasons: Vec<String> = list("reasons")?
            .iter()
            .map(|s| s.as_str().map(str::to_string))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| state_bad("a reason is not a text"))?;
        if status.len() != self.status.len() || reasons.len() != self.status.len() {
            return Err(state_bad("the number of parts does not fit the plan"));
        }
        let devices = list("devices")?;
        if devices.len() != self.devices.len() {
            return Err(state_bad("the number of feeders does not fit"));
        }
        let m = v.get("machine").ok_or_else(|| state_bad("no machine"))?;
        let counts: Vec<u64> = m
            .get("counts")
            .and_then(Value::as_array)
            .ok_or_else(|| state_bad("no counts"))?
            .iter()
            .map(|c| c.as_str().and_then(|s| s.parse::<u64>().ok()))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| state_bad("a count is not a number in a text"))?;
        if counts.len() != 3 {
            return Err(state_bad("the counts of the machine are not three"));
        }
        let holding: Vec<Option<String>> = m
            .get("holding")
            .and_then(Value::as_array)
            .ok_or_else(|| state_bad("no holding"))?
            .iter()
            .map(|h| h.as_str().map(str::to_string))
            .collect();
        let pos = (
            m.get("x")
                .and_then(Value::as_f64)
                .ok_or_else(|| state_bad("no x"))?,
            m.get("y")
                .and_then(Value::as_f64)
                .ok_or_else(|| state_bad("no y"))?,
        );
        let tip = m
            .get("tip")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        // everything has been read: now the job is changed
        let mut taken = Vec::new();
        let mut windows = Vec::new();
        let mut saved_devices = Vec::new();
        for d in devices {
            let remaining = int(d, "remaining")? as u32;
            let enabled = d
                .get("enabled")
                .and_then(Value::as_bool)
                .ok_or_else(|| state_bad("no enabled"))?;
            let window: Vec<bool> = d
                .get("window")
                .and_then(Value::as_array)
                .ok_or_else(|| state_bad("no window"))?
                .iter()
                .map(Value::as_bool)
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| state_bad("a window entry is not true or false"))?;
            saved_devices.push((
                (text_u64(d, "rng")?, text_u64(d, "attempts")?),
                remaining,
                enabled,
            ));
            taken.push(int(d, "taken")? as u32);
            windows.push(window);
        }
        let s = v.get("stats").ok_or_else(|| state_bad("no stats"))?;
        let mut stats = self.stats.clone();
        stats.placed = int(s, "placed")?;
        stats.deferred = int(s, "deferred")?;
        stats.feed_failures = int(s, "feed_failures")? as u64;
        stats.pick_attempts = int(s, "pick_attempts")? as u64;
        stats.pick_failures = int(s, "pick_failures")? as u64;
        stats.place_failures = int(s, "place_failures")? as u64;
        stats.tip_changes = int(s, "tip_changes")?;
        let sf = s
            .get("feeders")
            .and_then(Value::as_array)
            .ok_or_else(|| state_bad("no feeder statistics"))?;
        if sf.len() != stats.feeders.len() {
            return Err(state_bad("the statistics of the feeders do not fit"));
        }
        for (f, saved) in stats.feeders.iter_mut().zip(sf) {
            f.picked = int(saved, "picked")? as u32;
            f.faults = int(saved, "faults")? as u32;
            f.disabled = saved
                .get("disabled")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            f.pick_failures = saved
                .get("pick_failures")
                .and_then(Value::as_u64)
                .unwrap_or(0) as u32;
        }
        stats.skipped = s.get("skipped").and_then(Value::as_u64).unwrap_or(0) as usize;
        // a state saved before the report was kept has no trace: it starts again from zero
        let mut trace = Trace::default();
        if let Some(t) = v.get("trace") {
            let count = |key: &str| -> Result<u32, PnpError> { Ok(int(t, key)? as u32) };
            trace.pick_us = text_u64(t, "pick_us")?;
            trace.align_us = text_u64(t, "look_us")?;
            trace.place_us = text_u64(t, "place_us")?;
            trace.travel_us = text_u64(t, "travel_us")?;
            trace.tip_us = text_u64(t, "tip_us")?;
            trace.first_try = count("first_try")?;
            trace.retried = count("retried")?;
            trace.lost = count("lost")?;
            let actions = t
                .get("actions")
                .and_then(Value::as_array)
                .filter(|a| a.len() == trace.actions.len())
                .ok_or_else(|| state_bad("the actions of the person are not four numbers"))?;
            for (slot, a) in trace.actions.iter_mut().zip(actions) {
                *slot = a
                    .as_u64()
                    .and_then(|n| u32::try_from(n).ok())
                    .ok_or_else(|| state_bad("an action of the person is not a number"))?;
            }
            let codes = t
                .get("codes")
                .and_then(Value::as_array)
                .ok_or_else(|| state_bad("no codes in the trace"))?;
            for c in codes {
                let pair = c
                    .as_array()
                    .filter(|p| p.len() == 2)
                    .ok_or_else(|| state_bad("a code of the trace is not a pair"))?;
                let number = pair[0].as_u64().and_then(|n| u16::try_from(n).ok());
                let times = pair[1].as_u64().and_then(|n| u32::try_from(n).ok());
                match (number, times) {
                    (Some(number), Some(times)) => {
                        trace.codes.insert(number, times);
                    }
                    _ => return Err(state_bad("a code of the trace is not two numbers")),
                }
            }
            let readings = t
                .get("readings")
                .and_then(Value::as_array)
                .ok_or_else(|| state_bad("no readings in the trace"))?;
            for r in readings {
                let three = r
                    .as_array()
                    .filter(|p| p.len() == 3)
                    .ok_or_else(|| state_bad("a reading is not three numbers"))?;
                let mut n = [0.0f64; 3];
                for (slot, item) in n.iter_mut().zip(three) {
                    *slot = item
                        .as_str()
                        .and_then(|t| t.parse::<u64>().ok())
                        .map(f64::from_bits)
                        .filter(|x| x.is_finite())
                        .ok_or_else(|| state_bad("a reading is not a number"))?;
                }
                trace.readings.push(Offset {
                    dx_mm: n[0],
                    dy_mm: n[1],
                    rotation_deg: n[2],
                });
            }
        }
        // a state saved before the statistics by package are kept has none: they start again from zero
        if let Some(sk) = s.get("kinds").and_then(Value::as_array) {
            if sk.len() == stats.kinds.len() {
                for (k, saved) in stats.kinds.iter_mut().zip(sk) {
                    k.picked = int(saved, "picked")? as u32;
                    k.pick_failures = int(saved, "pick_failures")? as u32;
                    k.placed = int(saved, "placed")? as u32;
                    k.place_failures = int(saved, "place_failures")? as u32;
                }
            }
        }
        // a job without the align step ignores the section; one with it, and a state saved before it existed, starts it again from zero
        let mut align_state: Option<AlignState> = None;
        if self.cfg.align {
            if let Some(a) = v.get("align") {
                let bits = |key: &str| -> Result<f64, PnpError> {
                    text_u64(a, key)
                        .map(f64::from_bits)
                        .ok()
                        .filter(|x| x.is_finite())
                        .ok_or_else(|| {
                            state_bad(format!("'{key}' of the align step is not a number"))
                        })
                };
                let counted = |key: &str| -> Result<u32, PnpError> { Ok(int(a, key)? as u32) };
                let listed = a
                    .get("corrections")
                    .and_then(Value::as_array)
                    .ok_or_else(|| state_bad("no corrections"))?;
                if listed.len() != self.corrections.len() {
                    return Err(state_bad("the corrections do not fit the nozzles"));
                }
                let mut corrections: Vec<Option<Offset>> = Vec::new();
                for c in listed {
                    corrections.push(match c {
                        Value::Null => None,
                        Value::Array(o) if o.len() == 3 => {
                            let mut n = [0.0f64; 3];
                            for (slot, item) in n.iter_mut().zip(o) {
                                *slot = item
                                    .as_str()
                                    .and_then(|t| t.parse::<u64>().ok())
                                    .map(f64::from_bits)
                                    .filter(|x| x.is_finite())
                                    .ok_or_else(|| state_bad("a correction is not a number"))?;
                            }
                            Some(Offset {
                                dx_mm: n[0],
                                dy_mm: n[1],
                                rotation_deg: n[2],
                            })
                        }
                        _ => return Err(state_bad("a correction is not null or three numbers")),
                    });
                }
                let counters = AlignStats {
                    corrected: counted("corrected")?,
                    rejected: counted("rejected")?,
                    camera_failures: counted("camera_failures")?,
                    sum_mm: bits("sum_mm")?,
                    max_mm: bits("max_mm")?,
                    max_deg: bits("max_deg")?,
                };
                align_state = Some((
                    counters,
                    (text_u64(a, "rng")?, text_u64(a, "looks")?),
                    corrections,
                ));
            }
        }
        // a job without the vacuum checks ignores the section; one with them, and a state saved before they existed, starts them again from zero
        let mut vacuum_state: Option<VacuumState> = None;
        if self.cfg.vacuum {
            if let Some(a) = v.get("vacuum") {
                let bits = |x: &Value| -> Result<f64, PnpError> {
                    x.as_str()
                        .and_then(|t| t.parse::<u64>().ok())
                        .map(f64::from_bits)
                        .filter(|n| n.is_finite())
                        .ok_or_else(|| state_bad("a level of the vacuum is not a number"))
                };
                let optional = |key: &str| -> Result<Option<f64>, PnpError> {
                    match a.get(key) {
                        None | Some(Value::Null) => Ok(None),
                        Some(x) => bits(x).map(Some),
                    }
                };
                let counted = |key: &str| -> Result<u32, PnpError> { Ok(int(a, key)? as u32) };
                let on: Vec<bool> = a
                    .get("on")
                    .and_then(Value::as_array)
                    .ok_or_else(|| state_bad("no pumps in the vacuum"))?
                    .iter()
                    .map(Value::as_bool)
                    .collect::<Option<Vec<_>>>()
                    .ok_or_else(|| state_bad("a pump is not true or false"))?;
                let levels: Vec<f64> = a
                    .get("levels")
                    .and_then(Value::as_array)
                    .ok_or_else(|| state_bad("no levels in the vacuum"))?
                    .iter()
                    .map(&bits)
                    .collect::<Result<Vec<_>, _>>()?;
                if on.len() != self.cfg.nozzles || levels.len() != self.cfg.nozzles {
                    return Err(state_bad("the vacuum does not fit the nozzles"));
                }
                let counters = VacuumStats {
                    pick_checks: counted("pick_checks")?,
                    pick_misses: counted("pick_misses")?,
                    place_checks: counted("place_checks")?,
                    place_stuck: counted("place_stuck")?,
                    lowest_held_kpa: optional("lowest_held_kpa")?,
                    highest_released_kpa: optional("highest_released_kpa")?,
                };
                vacuum_state = Some((
                    counters,
                    (
                        text_u64(a, "rng")?,
                        text_u64(a, "picks")?,
                        text_u64(a, "places")?,
                    ),
                    on,
                    levels,
                ));
            }
        }
        let mut saved_actuators: Option<Vec<(bool, u32)>> = None;
        if !self.actuators.is_empty() {
            if let Some(list) = v.get("actuators").and_then(Value::as_array) {
                if list.len() != self.actuators.len() {
                    return Err(state_bad("the actuators do not fit the machine"));
                }
                let mut states = Vec::new();
                for item in list {
                    let on = item
                        .get("on")
                        .and_then(Value::as_bool)
                        .ok_or_else(|| state_bad("an actuator is not on or off"))?;
                    states.push((on, int(item, "switches")? as u32));
                }
                saved_actuators = Some(states);
            }
        }
        let error = match v.get("error") {
            Some(Value::Object(e)) => {
                let number = e.get("code").and_then(Value::as_u64).unwrap_or(0);
                let code = Code::ALL
                    .iter()
                    .copied()
                    .find(|c| u64::from(c.number()) == number)
                    .ok_or_else(|| state_bad("the code of the error is not known"))?;
                let detail = e.get("detail").and_then(Value::as_str).unwrap_or("");
                Some(PnpError::new(code, detail))
            }
            _ => None,
        };
        let saved_machine = (
            text_u64(m, "rng")?,
            [counts[0], counts[1], counts[2]],
            text_u64(m, "clock_us")?,
        );
        self.machine.restore(saved_machine, pos, holding, tip);
        match align_state {
            Some((counters, camera, corrections)) => {
                stats.align = counters;
                self.camera.restore(camera);
                self.corrections = corrections;
            }
            None => {
                stats.align = AlignStats::default();
                self.corrections = vec![None; self.corrections.len()];
            }
        }
        match vacuum_state {
            Some((counters, saved, on, levels)) => {
                stats.vacuum = counters;
                self.vacuum.restore(saved, on, levels);
            }
            None => stats.vacuum = VacuumStats::default(),
        }
        if let Some(states) = saved_actuators {
            for (a, (on, switches)) in self.actuators.iter_mut().zip(states) {
                a.restore(on, switches);
            }
        }
        for (d, (saved, remaining, enabled)) in self.devices.iter_mut().zip(saved_devices) {
            d.restore(saved, remaining, enabled);
        }
        self.cursor = cursor;
        self.paused = v.get("paused").and_then(Value::as_bool).unwrap_or(false);
        self.error = error;
        self.status = status;
        self.reasons = reasons;
        self.mounted = v.get("mounted").and_then(Value::as_bool).unwrap_or(false);
        self.taken = taken;
        self.windows = windows;
        self.stats = stats;
        self.trace = trace;
        self.last_feeder = None;
        Ok(())
    }
}
