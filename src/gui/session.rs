// EH3D-PHOENIX-PNP - the simulated job that the window runs.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The session: a job on the simulated machine that the window starts, pauses, resumes and steps. The window gives it the seconds that went by; it turns them into steps at the speed
//! of the configuration (`gui_steps_per_s`). Nothing here waits or draws, so a test can run a whole job in a loop.

use crate::config::MachineConfig;
use crate::error::PnpError;
use crate::feeding::FeederSpec;
use crate::job::{Cursor, Job, Phase, Status};
use crate::model::Machine;
use crate::planner::AdvancedPlan;
use std::collections::BTreeMap;

/// The most seconds one call of `tick` counts, so that a window that was stuck does not make a flood of steps.
const MAX_TICK_S: f64 = 0.5;

#[derive(Debug, Default)]
pub struct Session {
    job: Option<Job>,
    running: bool,
    steps_per_s: f64,
    credit: f64,
    seen_events: usize,
    last_error: Option<PnpError>,
}

/// What the head does next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Next {
    /// Picks the placement with this index in the plan.
    Pick(usize),
    /// Places it.
    Place(usize),
    Done,
}

impl Session {
    pub fn new(steps_per_s: f64) -> Session {
        Session {
            steps_per_s: Session::clean_rate(steps_per_s),
            ..Session::default()
        }
    }

    fn clean_rate(rate: f64) -> f64 {
        if rate.is_finite() {
            rate.clamp(0.1, 1000.0)
        } else {
            4.0
        }
    }

    pub fn set_rate(&mut self, steps_per_s: f64) {
        self.steps_per_s = Session::clean_rate(steps_per_s);
    }

    pub fn rate(&self) -> f64 {
        self.steps_per_s
    }

    /// Starts a new job on a simulated machine; the old one is gone. The job is ready but not running: `play` runs it.
    pub fn start(
        &mut self,
        plan: AdvancedPlan,
        feeders: Vec<FeederSpec>,
        cfg: MachineConfig,
    ) -> Result<(), PnpError> {
        let rate = cfg.gui_steps_per_s;
        let job = Job::new(plan, feeders, cfg)?;
        self.job = Some(job);
        self.running = false;
        self.credit = 0.0;
        self.seen_events = 0;
        self.last_error = None;
        self.steps_per_s = Session::clean_rate(rate);
        Ok(())
    }

    /// Drops the job.
    pub fn reset(&mut self) {
        self.job = None;
        self.running = false;
        self.credit = 0.0;
        self.seen_events = 0;
        self.last_error = None;
    }

    pub fn job(&self) -> Option<&Job> {
        self.job.as_ref()
    }

    pub fn has_job(&self) -> bool {
        self.job.is_some()
    }

    /// Whether the job is being run by the clock of the window.
    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn phase(&self) -> Option<Phase> {
        self.job.as_ref().map(Job::phase)
    }

    /// The error that stopped the job, if one did.
    pub fn last_error(&self) -> Option<&PnpError> {
        self.last_error.as_ref()
    }

    /// Runs the job: takes the pause away and, when an error had stopped it, tries the step that failed again.
    pub fn play(&mut self) {
        if let Some(job) = self.job.as_mut() {
            if job.phase() == Phase::Finished {
                return;
            }
            job.resume();
            self.last_error = None;
            self.running = true;
        }
    }

    /// Pauses the job at the end of the step it is on.
    pub fn pause(&mut self) {
        self.running = false;
        if let Some(job) = self.job.as_mut() {
            job.pause();
        }
    }

    /// One step, whether the job is paused or not. After an error the step that failed is tried again. Gives the error when the step fails.
    pub fn step_once(&mut self) -> Option<PnpError> {
        let job = self.job.as_mut()?;
        self.running = false;
        match job.phase() {
            Phase::Finished => return None,
            Phase::Stopped => {
                job.resume();
                job.pause();
            }
            Phase::Ready => job.pause(),
            Phase::Paused => {}
        }
        match job.step() {
            Ok(()) => None,
            Err(e) => {
                self.last_error = Some(e.clone());
                Some(e)
            }
        }
    }

    /// Counts `dt_s` seconds of the window; makes the steps they are worth while the job runs. Gives how many steps were made. A step that fails stops the job.
    pub fn tick(&mut self, dt_s: f64) -> usize {
        if !self.running {
            return 0;
        }
        let dt = if dt_s.is_finite() {
            dt_s.clamp(0.0, MAX_TICK_S)
        } else {
            0.0
        };
        self.credit += dt * self.steps_per_s;
        let mut made = 0;
        while self.credit >= 1.0 {
            self.credit -= 1.0;
            let Some(job) = self.job.as_mut() else {
                self.running = false;
                break;
            };
            if job.phase() != Phase::Ready {
                self.running = false;
                break;
            }
            match job.step() {
                Ok(()) => made += 1,
                Err(e) => {
                    self.last_error = Some(e);
                    self.running = false;
                    break;
                }
            }
        }
        if self.phase() == Some(Phase::Finished) {
            self.running = false;
        }
        made
    }

    /// How many placements are done (placed or put aside) and how many there are.
    pub fn progress(&self) -> (usize, usize) {
        match &self.job {
            Some(job) => {
                let done = job
                    .status()
                    .iter()
                    .filter(|s| **s != Status::Pending)
                    .count();
                (done, job.status().len())
            }
            None => (0, 0),
        }
    }

    /// The seconds of the clock of the simulated machine.
    pub fn machine_seconds(&self) -> f64 {
        self.job
            .as_ref()
            .map_or(0.0, |j| j.machine().now_us() as f64 / 1e6)
    }

    /// Where the head is.
    pub fn head(&self) -> Option<(f64, f64)> {
        self.job.as_ref().map(|j| j.machine().position())
    }

    /// The events of the machine that came since the last call, each as `12.3 s text`.
    pub fn take_events(&mut self) -> Vec<String> {
        let Some(job) = self.job.as_ref() else {
            return Vec::new();
        };
        let events = &job.machine().events;
        let from = self.seen_events.min(events.len());
        let out: Vec<String> = events[from..]
            .iter()
            .map(|e| format!("{:.2} s {}", e.t_us as f64 / 1e6, e.text))
            .collect();
        self.seen_events = events.len();
        out
    }

    /// What the head does next, as an index into the placements of the plan.
    pub fn next(&self) -> Next {
        let Some(job) = &self.job else {
            return Next::Done;
        };
        // the job numbers its batches in the order of the batches of the plan, and a batch holds its placements in the order of the plan
        let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (i, p) in job.plan().plan.placements.iter().enumerate() {
            groups.entry(p.batch).or_default().push(i);
        }
        let find =
            |batch: usize, k: usize| groups.values().nth(batch).and_then(|g| g.get(k)).copied();
        match job.cursor() {
            Cursor::Pick { batch, k } => find(batch, k).map_or(Next::Done, Next::Pick),
            Cursor::Place { batch, k } => find(batch, k).map_or(Next::Done, Next::Place),
            Cursor::Finished => Next::Done,
        }
    }
}
