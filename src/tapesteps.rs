// EH3D-PHOENIX-PNP - the tapes of the feeders, checked at every step of the job.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! `feeder-check` looks at the table of feeders as a table. A tape is used a step at a time, though: with each pick the reel has one part less, a strip moves one pitch along, and a reel that
//! was fine at the start may run dry, warn, or take the head out of its reach in the middle of the job. This walks the steps of a plan in order and, for every pick, says which part of the
//! reel it is, how many parts are left after it, where the pick place is, how far it is from the place of the pick before it on the same feeder (a strip must advance by its pitch), and
//! what the step comes to: `ok`, `low` (the first step that leaves fewer parts than the warning of the feeder), `last` (the reel is empty after it), `empty` (the reel had no part left), `pitch`
//! (the place moved by something other than the pitch), `reach` (the place is outside the area the machine reaches) or `nofeeder`. Plain arithmetic on the plan; nothing is run.

use crate::feeding::{FeederKind, FeederSpec};
use crate::parts::csv_field;
use std::collections::BTreeMap;

/// What a step comes to; the first of the list that holds is the one given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ok,
    /// The first step that leaves fewer parts than the warning of the feeder.
    Low,
    /// The step takes the last part of the reel.
    Last,
    /// The reel had no part left.
    Empty,
    /// The pick place is not where the pitch of the strip puts it.
    Pitch,
    /// The pick place is outside the area the machine reaches.
    Reach,
    /// The plan names a feeder that the table does not have.
    NoFeeder,
}

impl Status {
    pub fn name(self) -> &'static str {
        match self {
            Status::Ok => "ok",
            Status::Low => "low",
            Status::Last => "last",
            Status::Empty => "empty",
            Status::Pitch => "pitch",
            Status::Reach => "reach",
            Status::NoFeeder => "nofeeder",
        }
    }

    /// Whether the step is a fault of the job (a reel that runs dry, a place that is wrong or out of reach).
    pub fn is_fault(self) -> bool {
        matches!(
            self,
            Status::Empty | Status::Pitch | Status::Reach | Status::NoFeeder
        )
    }
}

/// The area the machine reaches and how far a strip may be from its pitch.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limits {
    pub x_min: f64,
    pub x_max: f64,
    pub y_min: f64,
    pub y_max: f64,
    pub pitch_tol_mm: f64,
}

/// One step of the job.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    /// The place of the step in the order of the plan, from 1.
    pub step: usize,
    pub reference: String,
    pub feeder: String,
    /// Which part of the reel this is, from 1.
    pub taken: u32,
    /// The parts left in the reel after the step (negative when the reel had run dry).
    pub remaining: i64,
    pub pick: (f64, f64),
    /// The distance from the pick place of the step before on the same feeder.
    pub advance_mm: Option<f64>,
    pub status: Status,
}

/// The steps of a plan, `(reference, feeder, pick place)` in the order of the plan, walked against the feeders. A feeder that is switched off still counts: the plan decides what it takes.
pub fn walk(
    placements: &[(&str, &str, (f64, f64))],
    feeders: &[FeederSpec],
    limits: &Limits,
) -> Vec<Step> {
    let by_id: BTreeMap<&str, &FeederSpec> =
        feeders.iter().map(|f| (f.base.id.as_str(), f)).collect();
    let mut taken: BTreeMap<&str, u32> = BTreeMap::new();
    let mut last_pick: BTreeMap<&str, (f64, f64)> = BTreeMap::new();
    let mut warned: BTreeMap<&str, bool> = BTreeMap::new();
    let mut out = Vec::with_capacity(placements.len());
    for (i, (reference, id, pick)) in placements.iter().enumerate() {
        let n = taken.entry(*id).or_insert(0);
        *n += 1;
        let advance = last_pick
            .insert(*id, *pick)
            .map(|p| (pick.0 - p.0).hypot(pick.1 - p.1));
        let (remaining, status) = match by_id.get(id) {
            None => (0, Status::NoFeeder),
            Some(f) => {
                let remaining = i64::from(f.base.count) - i64::from(*n);
                let outside = !(limits.x_min..=limits.x_max).contains(&pick.0)
                    || !(limits.y_min..=limits.y_max).contains(&pick.1);
                let pitch_wrong = match (f.kind, advance) {
                    (FeederKind::Strip { pitch, .. }, Some(a)) => {
                        (a - pitch.abs()).abs() > limits.pitch_tol_mm
                    }
                    _ => false,
                };
                let first_warning = f.warn_below > 0
                    && remaining < i64::from(f.warn_below)
                    && !warned.get(id).copied().unwrap_or(false);
                let status = if remaining < 0 {
                    Status::Empty
                } else if outside {
                    Status::Reach
                } else if pitch_wrong {
                    Status::Pitch
                } else if remaining == 0 {
                    Status::Last
                } else if first_warning {
                    Status::Low
                } else {
                    Status::Ok
                };
                if f.warn_below > 0 && remaining < i64::from(f.warn_below) {
                    warned.insert(*id, true);
                }
                (remaining, status)
            }
        };
        out.push(Step {
            step: i + 1,
            reference: (*reference).to_string(),
            feeder: (*id).to_string(),
            taken: *n,
            remaining,
            pick: *pick,
            advance_mm: advance,
            status,
        });
    }
    out
}

/// The steps that are a fault of the job.
pub fn faults(steps: &[Step]) -> usize {
    steps.iter().filter(|s| s.status.is_fault()).count()
}

/// The steps as a table: `step,reference,feeder,taken,remaining,pick_x,pick_y,advance_mm,status`.
pub fn csv(steps: &[Step]) -> String {
    let mut out =
        String::from("step,reference,feeder,taken,remaining,pick_x,pick_y,advance_mm,status\n");
    for s in steps {
        out.push_str(&format!(
            "{},{},{},{},{},{:.3},{:.3},{},{}\n",
            s.step,
            csv_field(&s.reference),
            csv_field(&s.feeder),
            s.taken,
            s.remaining,
            s.pick.0,
            s.pick.1,
            s.advance_mm.map_or(String::new(), |a| format!("{a:.3}")),
            s.status.name()
        ));
    }
    out
}

/// The report for a person: a line for each feeder (the picks it gets and the parts left at the end) and a line for each step that is not `ok`.
pub fn text(steps: &[Step]) -> String {
    let mut feeders: BTreeMap<&str, (u32, i64)> = BTreeMap::new();
    for s in steps {
        let e = feeders.entry(s.feeder.as_str()).or_insert((0, 0));
        e.0 = s.taken;
        e.1 = s.remaining;
    }
    let mut out = format!(
        "{} steps on {} feeders, {} faults\n",
        steps.len(),
        feeders.len(),
        faults(steps)
    );
    for (id, (picks, left)) in &feeders {
        out.push_str(&format!(
            "{id}: {picks} picks, {left} parts left after the last\n"
        ));
    }
    for s in steps.iter().filter(|s| s.status != Status::Ok) {
        out.push_str(&format!(
            "step {} ({}): {}, part {} of the reel, {} left, picked at {:.3},{:.3} - {}\n",
            s.step,
            s.reference,
            s.feeder,
            s.taken,
            s.remaining,
            s.pick.0,
            s.pick.1,
            s.status.name()
        ));
    }
    out
}
