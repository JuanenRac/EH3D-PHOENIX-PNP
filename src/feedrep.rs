// EH3D-PHOENIX-PNP - how each feeder did in a job.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The picks of a job feeder by feeder: how many the plan asked of it and for how many seconds the plan keeps it busy, how many picks worked, how many failed (also those that a retry put
//! right), how often that is, the faults it had and whether it was switched off, and how many of the parts it gave were placed and for how many seconds the steps that picked from it
//! kept the machine busy. A feeder whose failures are above a share of its picks, once it has been tried enough times to say, is
//! marked so that the person looks at that feeder (a worn tape, a cover film that does not peel, a bad pick place) instead of reading the whole report. Plain arithmetic and text.

use crate::job::{FeederStats, Job};
use crate::planner::PickTime;
use std::collections::HashMap;

/// The numbers of one feeder.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub id: String,
    /// How many picks the plan asks of the feeder and for how many seconds it keeps the feeder busy.
    pub planned_picks: u32,
    pub planned_busy_s: f64,
    pub picked: u32,
    pub pick_failures: u32,
    pub faults: u32,
    pub disabled: bool,
    /// The parts picked from the feeder that were placed, and the seconds that the steps picking from it took (the way to the feeder, the feed, the pick and the look).
    pub placed: u32,
    pub busy_s: f64,
    /// The failed picks among all that were tried (the ones that worked and the ones that failed), in per cent; 0 when none was tried.
    pub fail_percent: f64,
    /// The feeder failed more than the limit and was tried at least the least number of times.
    pub flagged: bool,
}

/// The rows from the numbers of a job and the plan. A pick of the plan for a feeder that the job does not know is not counted.
pub fn rows_from(
    stats: &[FeederStats],
    picks: &[PickTime],
    limit_percent: f64,
    min_picks: u32,
) -> Vec<Row> {
    let mut planned: HashMap<&str, (u32, f64)> = HashMap::new();
    for p in picks {
        let e = planned.entry(p.feeder.as_str()).or_insert((0, 0.0));
        e.0 += 1;
        e.1 += p.busy_s;
    }
    stats
        .iter()
        .map(|f| {
            let tried = f.picked + f.pick_failures;
            let fail_percent = if tried > 0 {
                100.0 * f64::from(f.pick_failures) / f64::from(tried)
            } else {
                0.0
            };
            let (planned_picks, planned_busy_s) =
                planned.get(f.id.as_str()).copied().unwrap_or((0, 0.0));
            Row {
                id: f.id.clone(),
                planned_picks,
                planned_busy_s,
                picked: f.picked,
                pick_failures: f.pick_failures,
                faults: f.faults,
                disabled: f.disabled,
                placed: f.placed,
                busy_s: f.busy_s(),
                fail_percent,
                flagged: tried >= min_picks && fail_percent > limit_percent,
            }
        })
        .collect()
}

/// The rows of a job.
pub fn build(job: &Job, limit_percent: f64, min_picks: u32) -> Vec<Row> {
    rows_from(
        &job.stats().feeders,
        &job.plan().pick_times,
        limit_percent,
        min_picks,
    )
}

/// The header of [`csv`]: a column is only ever added at the end, so a sheet that reads the columns by name keeps working.
pub const CSV_HEADER: &str = "feeder,planned_picks,planned_busy_s,picked,pick_failures,fail_percent,faults,disabled,flagged,placed,busy_s";

/// The rows as a CSV, in the order of the feeders.
pub fn csv(rows: &[Row]) -> String {
    let mut out = format!("{CSV_HEADER}\n");
    for r in rows {
        out.push_str(&format!(
            "{},{},{:.3},{},{},{:.1},{},{},{},{},{:.3}\n",
            crate::parts::csv_field(&r.id),
            r.planned_picks,
            r.planned_busy_s,
            r.picked,
            r.pick_failures,
            r.fail_percent,
            r.faults,
            if r.disabled { "yes" } else { "no" },
            if r.flagged { "yes" } else { "no" },
            r.placed,
            r.busy_s
        ));
    }
    out
}

/// The rows as text, the feeders that fail most first; a flagged feeder says so.
pub fn text(rows: &[Row]) -> String {
    let mut sorted: Vec<&Row> = rows.iter().collect();
    sorted.sort_by(|a, b| {
        b.fail_percent
            .total_cmp(&a.fail_percent)
            .then(a.id.cmp(&b.id))
    });
    let mut out = String::new();
    for r in sorted {
        out.push_str(&format!(
            "feeder {}: {} picked, {} failed ({:.1} per cent), {} faults, plan {} picks in {:.1} s, {} placed, busy {:.1} s{}{}\n",
            r.id,
            r.picked,
            r.pick_failures,
            r.fail_percent,
            r.faults,
            r.planned_picks,
            r.planned_busy_s,
            r.placed,
            r.busy_s,
            if r.disabled { ", switched off" } else { "" },
            if r.flagged { " <- look at this feeder" } else { "" }
        ));
    }
    out
}
