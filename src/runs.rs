// EH3D-PHOENIX-PNP - the same job run many times on the simulated machine.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The simulated machine fails by chance (`sim_pick_fail_rate`, `sim_place_fail_rate`, `sim_feed_fail_rate`, a feeder that is bad), and one run says how that one throw of the dice went. This runs
//! the plan again and again, each time from another seed (the first one and the next ones, one more for each run), and says how the job goes on the whole: how many runs ended with every
//! part placed, how many were stopped by an error (and by which), how many parts were put aside, and the shortest, the middle and the longest time. The same plan, the same settings and
//! the same first seed give the same table. It is a measure of the plan and of the settings on a simulated machine, not of a machine: it says nothing about hardware that was never tried.

use crate::config::MachineConfig;
use crate::error::{Code, PnpError};
use crate::exchange::phase_name;
use crate::feeding::FeederSpec;
use crate::job::{Job, Phase, Status};
use crate::model::Machine;
use crate::planner::AdvancedPlan;
use std::collections::BTreeMap;

/// The most runs of one call.
pub const MAX_RUNS: usize = 10_000;

/// One run.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub seed: u64,
    pub phase: Phase,
    pub placed: usize,
    pub deferred: usize,
    pub skipped: usize,
    pub pending: usize,
    /// The seconds of the clock of the machine when the run ended.
    pub seconds: f64,
    /// The code of the error that stopped the run, when one did.
    pub error: Option<String>,
}

/// The runs of one plan.
#[derive(Debug, Clone, PartialEq)]
pub struct Runs {
    pub rows: Vec<Run>,
    /// How many parts the plan places.
    pub parts: usize,
}

/// Runs the plan `runs` times from the seeds `first_seed`, `first_seed + 1`, and so on, each on a new simulated machine with the feeders as they are and the settings `cfg`.
pub fn run_many(
    plan: &AdvancedPlan,
    feeders: &[FeederSpec],
    cfg: &MachineConfig,
    runs: usize,
    first_seed: u64,
) -> Result<Runs, PnpError> {
    if runs == 0 || runs > MAX_RUNS {
        return Err(PnpError::new(
            Code::PlanSetting,
            format!("the number of runs must be from 1 to {MAX_RUNS}"),
        ));
    }
    let mut rows = Vec::with_capacity(runs);
    for n in 0..runs {
        let seed = first_seed.wrapping_add(n as u64);
        let mut cfg = cfg.clone();
        cfg.sim_seed = seed;
        let mut job = Job::new(plan.clone(), feeders.to_vec(), cfg)?;
        let outcome = job.run(None);
        let count = |s: Status| job.status().iter().filter(|x| **x == s).count();
        rows.push(Run {
            seed,
            phase: job.phase(),
            placed: count(Status::Placed),
            deferred: count(Status::Deferred),
            skipped: count(Status::Skipped),
            pending: count(Status::Pending),
            seconds: job.machine().now_us() as f64 / 1e6,
            error: outcome.err().map(|e| e.code.label()),
        });
    }
    Ok(Runs {
        rows,
        parts: plan.plan.placements.len(),
    })
}

impl Runs {
    /// The runs that placed every part.
    pub fn complete(&self) -> usize {
        self.rows.iter().filter(|r| r.placed == self.parts).count()
    }

    /// The runs that an error stopped.
    pub fn stopped(&self) -> usize {
        self.rows.iter().filter(|r| r.error.is_some()).count()
    }

    /// The errors that stopped runs, with how many runs each stopped, the most frequent first (then by code).
    pub fn errors(&self) -> Vec<(String, usize)> {
        let mut by: BTreeMap<&str, usize> = BTreeMap::new();
        for e in self.rows.iter().filter_map(|r| r.error.as_deref()) {
            *by.entry(e).or_insert(0) += 1;
        }
        let mut v: Vec<(String, usize)> = by.into_iter().map(|(k, n)| (k.to_string(), n)).collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        v
    }

    /// The shortest, the middle and the longest time of the runs that ended with every part placed or put aside (none when no run did).
    pub fn seconds(&self) -> Option<(f64, f64, f64)> {
        let mut s: Vec<f64> = self
            .rows
            .iter()
            .filter(|r| r.pending == 0)
            .map(|r| r.seconds)
            .collect();
        if s.is_empty() {
            return None;
        }
        s.sort_by(f64::total_cmp);
        let mid = if s.len() % 2 == 1 {
            s[s.len() / 2]
        } else {
            (s[s.len() / 2 - 1] + s[s.len() / 2]) / 2.0
        };
        Some((s[0], mid, s[s.len() - 1]))
    }

    /// The least, the mean and the most parts placed in a run.
    pub fn placed(&self) -> (usize, f64, usize) {
        let all: Vec<usize> = self.rows.iter().map(|r| r.placed).collect();
        let sum: usize = all.iter().sum();
        (
            all.iter().copied().min().unwrap_or(0),
            sum as f64 / all.len().max(1) as f64,
            all.iter().copied().max().unwrap_or(0),
        )
    }

    /// The mean number of parts put aside in a run.
    pub fn mean_deferred(&self) -> f64 {
        self.rows.iter().map(|r| r.deferred).sum::<usize>() as f64 / self.rows.len().max(1) as f64
    }

    /// The runs as a table: `run,seed,phase,placed,deferred,skipped,pending,seconds,error`.
    pub fn csv(&self) -> String {
        let mut out =
            String::from("run,seed,phase,placed,deferred,skipped,pending,seconds,error\n");
        for (i, r) in self.rows.iter().enumerate() {
            out.push_str(&format!(
                "{},{},{},{},{},{},{},{:.3},{}\n",
                i + 1,
                r.seed,
                phase_name(r.phase),
                r.placed,
                r.deferred,
                r.skipped,
                r.pending,
                r.seconds,
                crate::parts::csv_field(r.error.as_deref().unwrap_or(""))
            ));
        }
        out
    }

    /// What the runs say, in lines for a person.
    pub fn text(&self) -> String {
        let n = self.rows.len();
        let first = self.rows.first().map_or(0, |r| r.seed);
        let mut out = format!(
            "{n} runs from the seed {first}: {} placed every part, {} were stopped by an error\n",
            self.complete(),
            self.stopped()
        );
        let (least, mean, most) = self.placed();
        out.push_str(&format!(
            "parts placed in a run: {least} at the least, {mean:.1} on average, {most} at the most, of {}\n",
            self.parts
        ));
        out.push_str(&format!(
            "parts put aside in a run: {:.2} on average\n",
            self.mean_deferred()
        ));
        match self.seconds() {
            Some((lo, mid, hi)) => out.push_str(&format!(
                "time of a run that went to the end: {lo:.1} s at the least, {mid:.1} s the middle, {hi:.1} s at the most\n"
            )),
            None => out.push_str("no run went to the end\n"),
        }
        for (code, times) in self.errors() {
            out.push_str(&format!("error {code}: {times} runs\n"));
        }
        out
    }
}
