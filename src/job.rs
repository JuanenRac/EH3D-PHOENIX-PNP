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

use crate::config::{ErrorPolicy, MachineConfig};
use crate::error::{Code, PnpError};
use crate::feeding::{choose, FeederSpec};
use crate::model::{FeederDevice, Machine, Nozzle};
use crate::planner::AdvancedPlan;
use crate::rotation::glob_match;
use crate::sim::{SimFeeder, SimMachine, SimOp};
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Pending,
    Placed,
    Deferred,
}

impl Status {
    fn name(self) -> &'static str {
        match self {
            Status::Pending => "pending",
            Status::Placed => "placed",
            Status::Deferred => "deferred",
        }
    }

    fn parse(text: &str) -> Option<Status> {
        match text {
            "pending" => Some(Status::Pending),
            "placed" => Some(Status::Placed),
            "deferred" => Some(Status::Deferred),
            _ => None,
        }
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

/// The statistics of the job.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Stats {
    pub placed: usize,
    pub deferred: usize,
    pub feed_failures: u64,
    pub pick_attempts: u64,
    pub pick_failures: u64,
    pub place_failures: u64,
    pub tip_changes: usize,
    pub feeders: Vec<FeederStats>,
    /// By package, in the order of the first feeder that holds it.
    pub kinds: Vec<KindStats>,
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
        let done = match self.cursor {
            Cursor::Pick { batch, k } => self.do_pick(batch, k),
            Cursor::Place { batch, k } => self.do_place(batch, k),
            Cursor::Finished => Ok(()),
        };
        match done {
            Ok(()) => {
                self.cursor = self.next_cursor(self.cursor);
                Ok(())
            }
            Err(e) => {
                self.error = Some(e.clone());
                Err(e)
            }
        }
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

    fn record(&mut self, f: usize, fault: bool) {
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
            self.devices[f].set_enabled(false);
            self.stats.feeders[f].disabled = true;
            let id = self.devices[f].id().to_string();
            self.machine
                .note(format!("feeder {id} switched off after {faults} faults"));
        }
    }

    /// What the job does with a part that could not be picked or placed.
    fn give_up(&mut self, idx: usize, nozzle: usize, e: PnpError) -> Result<(), PnpError> {
        match self.cfg.error_policy {
            ErrorPolicy::Alert => Err(e),
            ErrorPolicy::Defer => {
                self.machine.discard(nozzle);
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
                    self.machine.move_to(changer)?;
                }
                self.machine.change_tip(&det.tip);
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
            return self.give_up(idx, nozzle, e);
        };
        let at = self.feeders[f].pick_point(self.taken[f]);
        self.machine.move_to(at)?;
        let mut fed = false;
        for _ in 0..=self.cfg.feed_retries {
            self.machine.wait(self.cfg.feed_s);
            match self.devices[f].feed() {
                Ok(()) => {
                    fed = true;
                    break;
                }
                Err(e) if e.code == Code::SimFault => self.stats.feed_failures += 1,
                Err(e) => return Err(e),
            }
        }
        if !fed {
            self.record(f, true);
            let e = PnpError::new(
                Code::FeedFailed,
                format!("feeder '{}' for {}", self.feeders[f].base.id, pl.reference),
            );
            return self.give_up(idx, nozzle, e);
        }
        let mut picked = false;
        let kind = self.kind_of(f);
        let chance = combined_chance(self.part_chances(f).0, self.feeder_chance(f));
        self.machine.set_extra_fail(SimOp::Pick, chance);
        for _ in 0..=det.pick_retries {
            self.stats.pick_attempts += 1;
            match self.machine.pick(nozzle, &pl.reference) {
                Ok(()) => {
                    picked = true;
                    break;
                }
                Err(e) if e.code == Code::SimFault => {
                    self.stats.pick_failures += 1;
                    self.stats.kinds[kind].pick_failures += 1;
                    self.stats.feeders[f].pick_failures += 1;
                }
                Err(e) => return Err(e),
            }
        }
        self.machine.set_extra_fail(SimOp::Pick, 0.0);
        self.record(f, !picked);
        if !picked {
            let e = PnpError::new(
                Code::PickFailed,
                format!("{} from feeder '{}'", pl.reference, self.feeders[f].base.id),
            );
            return self.give_up(idx, nozzle, e);
        }
        self.devices[f].take();
        self.taken[f] += 1;
        self.stats.feeders[f].picked += 1;
        self.stats.kinds[kind].picked += 1;
        Ok(())
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
        self.machine.move_to((pl.x, pl.y))?;
        let source = self.index[&pl.feeder];
        let kind = self.kind_of(source);
        let place_chance = self.part_chances(source).1;
        self.machine.set_extra_fail(SimOp::Place, place_chance);
        for _ in 0..=self.cfg.place_retries {
            match self.machine.place(nozzle) {
                Ok(_) => {
                    self.machine.set_extra_fail(SimOp::Place, 0.0);
                    self.status[idx] = Status::Placed;
                    self.stats.placed += 1;
                    self.stats.kinds[kind].placed += 1;
                    return Ok(());
                }
                Err(e) if e.code == Code::SimFault => {
                    self.stats.place_failures += 1;
                    self.stats.kinds[kind].place_failures += 1;
                }
                Err(e) => return Err(e),
            }
        }
        self.machine.set_extra_fail(SimOp::Place, 0.0);
        // with `alert` the part stays on the nozzle, so that the step can be tried again after the cause is put right
        let e = PnpError::new(Code::PlaceFailed, pl.reference.clone());
        self.give_up(idx, nozzle, e)
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
        serde_json::to_string_pretty(&json!({
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
        }))
        .unwrap_or_default()
            + "\n"
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
            "feed_failures": s.feed_failures,
            "pick_attempts": s.pick_attempts,
            "pick_failures": s.pick_failures,
            "place_failures": s.place_failures,
            "tip_changes": s.tip_changes,
            "feeders": feeders,
            "kinds": kinds
        });
        serde_json::to_string_pretty(&json!({
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
        }))
        .unwrap_or_default()
            + "\n"
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
            .ok_or_else(|| state_bad("a status is not pending, placed or deferred"))?;
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
        Ok(())
    }
}
