// EH3D-PHOENIX-PNP - which feeders give the trouble when the same job is run many times on the simulated machine.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! `simulate-runs` says how the job goes as a whole. When it does not go well, the next question is where: which feeder fails to pick, which one is switched off by its faults, which one
//! leaves parts for a person to place. This runs the plan again and again, each time from another seed, as `simulate-runs` does, and adds up for each feeder the picks, the picks that failed
//! (also those that a retry put right), the runs in which the feeder was switched off and the parts of the feeder that were put aside, with the worst feeder first. It is a measure of the
//! plan and of the settings of the simulated machine, not of a machine: it says nothing about hardware that was never tried.

use crate::config::MachineConfig;
use crate::error::{Code, PnpError};
use crate::feeding::FeederSpec;
use crate::job::{Job, Status};
use crate::parts::csv_field;
use crate::planner::AdvancedPlan;
use crate::runs::MAX_RUNS;
use std::collections::BTreeMap;

/// What the runs say of one feeder.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Spot {
    pub id: String,
    /// The parts picked up in all the runs, and the picks that failed (also the ones that a retry put right): each pick that failed and each part picked up is one try.
    pub picks: u64,
    pub failures: u64,
    /// The runs in which the feeder was switched off, and the parts of the feeder that the runs put aside.
    pub switched_off: u64,
    pub put_aside: u64,
}

impl Spot {
    /// The tries to pick: the parts picked up and the picks that failed.
    pub fn tries(&self) -> u64 {
        self.picks + self.failures
    }

    /// The share of the tries that failed, in percent (0 when there was no try).
    pub fn failure_percent(&self) -> f64 {
        if self.tries() == 0 {
            0.0
        } else {
            self.failures as f64 * 100.0 / self.tries() as f64
        }
    }

    /// Whether the feeder gave trouble in any run.
    pub fn troubled(&self) -> bool {
        self.failures + self.switched_off + self.put_aside > 0
    }
}

/// The feeders of a plan, added up over the runs.
#[derive(Debug, Clone, PartialEq)]
pub struct Hotspots {
    pub runs: usize,
    pub first_seed: u64,
    pub spots: Vec<Spot>,
}

/// Runs the plan `runs` times from the seeds `first_seed`, `first_seed + 1`, and so on, each on a new simulated machine with the feeders as they are and the settings `cfg`, and adds up what
/// each feeder did. The worst feeder (the most failed picks, then the most switched off, then the most parts put aside, then the id) comes first.
pub fn run_many(
    plan: &AdvancedPlan,
    feeders: &[FeederSpec],
    cfg: &MachineConfig,
    runs: usize,
    first_seed: u64,
) -> Result<Hotspots, PnpError> {
    if runs == 0 || runs > MAX_RUNS {
        return Err(PnpError::new(
            Code::PlanSetting,
            format!("the number of runs must be from 1 to {MAX_RUNS}"),
        ));
    }
    let mut by: BTreeMap<String, Spot> = BTreeMap::new();
    for f in feeders {
        by.insert(
            f.base.id.clone(),
            Spot {
                id: f.base.id.clone(),
                ..Spot::default()
            },
        );
    }
    for n in 0..runs {
        let mut c = cfg.clone();
        c.sim_seed = first_seed.wrapping_add(n as u64);
        let mut job = Job::new(plan.clone(), feeders.to_vec(), c)?;
        // an error that stops a run is a result of the run, not of this: what was picked up to it counts
        let _ = job.run(None);
        for s in &job.stats().feeders {
            let spot = by.entry(s.id.clone()).or_insert_with(|| Spot {
                id: s.id.clone(),
                ..Spot::default()
            });
            spot.picks += u64::from(s.picked);
            spot.failures += u64::from(s.pick_failures);
            if s.disabled {
                spot.switched_off += 1;
            }
        }
        for (i, p) in plan.plan.placements.iter().enumerate() {
            if job.status()[i] == Status::Deferred {
                if let Some(spot) = by.get_mut(&p.feeder) {
                    spot.put_aside += 1;
                }
            }
        }
    }
    let mut spots: Vec<Spot> = by.into_values().collect();
    spots.sort_by(|a, b| {
        b.failures
            .cmp(&a.failures)
            .then_with(|| b.switched_off.cmp(&a.switched_off))
            .then_with(|| b.put_aside.cmp(&a.put_aside))
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(Hotspots {
        runs,
        first_seed,
        spots,
    })
}

impl Hotspots {
    /// The feeders that gave trouble in some run.
    pub fn troubled(&self) -> Vec<&Spot> {
        self.spots.iter().filter(|s| s.troubled()).collect()
    }

    /// The feeders as a CSV: `feeder,picks,failures,failure_percent,switched_off_runs,put_aside` (`picks` is the parts picked up; the share is of picks and failures together).
    pub fn csv(&self) -> String {
        let mut out =
            String::from("feeder,picks,failures,failure_percent,switched_off_runs,put_aside\n");
        for s in &self.spots {
            out.push_str(&format!(
                "{},{},{},{:.2},{},{}\n",
                csv_field(&s.id),
                s.picks,
                s.failures,
                s.failure_percent(),
                s.switched_off,
                s.put_aside
            ));
        }
        out
    }

    /// What the runs say, in lines for a person: one line for each feeder that gave trouble, then the line that sums up.
    pub fn text(&self) -> String {
        let mut out = String::new();
        let bad = self.troubled();
        for s in &bad {
            out.push_str(&format!(
                "{}: {} of {} tries to pick failed ({:.2} %), switched off in {} of {} runs, {} parts put aside\n",
                s.id,
                s.failures,
                s.tries(),
                s.failure_percent(),
                s.switched_off,
                self.runs,
                s.put_aside
            ));
        }
        if bad.is_empty() {
            out.push_str(&format!(
                "no feeder gave trouble in {} runs from the seed {}\n",
                self.runs, self.first_seed
            ));
        } else {
            out.push_str(&format!(
                "{} of {} feeders gave trouble in {} runs from the seed {}; {} gave the most\n",
                bad.len(),
                self.spots.len(),
                self.runs,
                self.first_seed,
                bad[0].id
            ));
        }
        out
    }
}
