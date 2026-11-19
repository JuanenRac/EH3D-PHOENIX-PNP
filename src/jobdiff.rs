// EH3D-PHOENIX-PNP - what differs between two jobs in files.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A table of feeders was edited, a setting was changed, a part was moved: the plan that comes out is not the one that was looked at before. `job-export` writes a plan, and the state of the job
//! it is in, as a file; this says what differs between two such files, part by part: the parts that are in one and not in the other, the feeder, the nozzle, the batch and the tip of a part,
//! where it is picked and where it is placed, how it is turned and what became of it in the job, and the order in which the parts are placed. The numbers of the plan (how many batches, tip
//! changes and tiers, the travel and the time) and the phase of the job are compared too. A place within a thousandth of a millimetre and a turn within a hundredth of a degree are the same.

use crate::exchange::{phase_name, Exchange, ExchangePart};
use crate::parts::csv_field;
use std::collections::HashMap;

/// Places closer than this are the same place, in millimetres.
pub const PLACE_TOL_MM: f64 = 0.001;
/// Turns closer than this, the short way round, are the same turn, in degrees.
pub const TURN_TOL_DEG: f64 = 0.01;
/// The travel and the seconds of a plan differ when they are further apart than this.
pub const TOTAL_TOL: f64 = 0.05;

/// One difference of a part, or of the plan as a whole (the reference is then empty).
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub reference: String,
    /// `added`, `removed`, `feeder`, `nozzle`, `batch`, `tip`, `pick`, `place`, `turn`, `status`, `reason`, `sequence` (the order of the parts that are in both), `order`, `batches`, `tip_changes`, `tiers`, `travel`, `seconds` or `phase`.
    pub kind: &'static str,
    pub first: String,
    pub second: String,
}

/// What differs between two jobs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct JobDiff {
    pub changes: Vec<Change>,
    /// The parts that are in both and are in the same order among themselves.
    pub same_sequence: bool,
}

fn turn_gap(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

fn xy(p: (f64, f64)) -> String {
    format!("{:.3},{:.3}", p.0, p.1)
}

fn far(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() > PLACE_TOL_MM || (a.1 - b.1).abs() > PLACE_TOL_MM
}

/// The differences of the part `a` (of the first job) and `b` (of the second).
fn part_changes(a: &ExchangePart, b: &ExchangePart, out: &mut Vec<Change>) {
    let mut push = |kind: &'static str, first: String, second: String| {
        out.push(Change {
            reference: a.reference.clone(),
            kind,
            first,
            second,
        });
    };
    if a.feeder != b.feeder {
        push("feeder", a.feeder.clone(), b.feeder.clone());
    }
    if a.nozzle != b.nozzle {
        push(
            "nozzle",
            (a.nozzle + 1).to_string(),
            (b.nozzle + 1).to_string(),
        );
    }
    if a.batch != b.batch {
        push(
            "batch",
            (a.batch + 1).to_string(),
            (b.batch + 1).to_string(),
        );
    }
    if a.tip != b.tip {
        push("tip", a.tip.clone(), b.tip.clone());
    }
    if far(a.pick, b.pick) {
        push("pick", xy(a.pick), xy(b.pick));
    }
    if far(a.place, b.place) {
        push("place", xy(a.place), xy(b.place));
    }
    if turn_gap(a.rotation, b.rotation) > TURN_TOL_DEG {
        push(
            "turn",
            format!("{:.2}", a.rotation),
            format!("{:.2}", b.rotation),
        );
    }
    if a.status != b.status {
        push(
            "status",
            a.status.name().to_string(),
            b.status.name().to_string(),
        );
    }
    if a.reason != b.reason {
        push("reason", a.reason.clone(), b.reason.clone());
    }
}

/// What differs between the job `first` and the job `second`: the plan as a whole first, then the parts in the order of the first job (and the parts only the second has, in its order).
pub fn diff(first: &Exchange, second: &Exchange) -> JobDiff {
    let mut changes = Vec::new();
    let mut whole = |kind: &'static str, a: String, b: String| {
        if a != b {
            changes.push(Change {
                reference: String::new(),
                kind,
                first: a,
                second: b,
            });
        }
    };
    whole("order", first.order.clone(), second.order.clone());
    whole(
        "batches",
        first.batches.to_string(),
        second.batches.to_string(),
    );
    whole(
        "tip_changes",
        first.tip_changes.to_string(),
        second.tip_changes.to_string(),
    );
    whole("tiers", first.tiers.to_string(), second.tiers.to_string());
    if (first.travel_mm - second.travel_mm).abs() > TOTAL_TOL {
        whole(
            "travel",
            format!("{:.1} mm", first.travel_mm),
            format!("{:.1} mm", second.travel_mm),
        );
    }
    if (first.seconds - second.seconds).abs() > TOTAL_TOL {
        whole(
            "seconds",
            format!("{:.1} s", first.seconds),
            format!("{:.1} s", second.seconds),
        );
    }
    whole(
        "phase",
        phase_name(first.phase).to_string(),
        phase_name(second.phase).to_string(),
    );
    let index_b: HashMap<&str, usize> = second
        .parts
        .iter()
        .enumerate()
        .map(|(i, p)| (p.reference.as_str(), i))
        .collect();
    let index_a: HashMap<&str, usize> = first
        .parts
        .iter()
        .enumerate()
        .map(|(i, p)| (p.reference.as_str(), i))
        .collect();
    for a in &first.parts {
        match index_b.get(a.reference.as_str()) {
            None => changes.push(Change {
                reference: a.reference.clone(),
                kind: "removed",
                first: a.feeder.clone(),
                second: String::new(),
            }),
            Some(&i) => part_changes(a, &second.parts[i], &mut changes),
        }
    }
    for b in &second.parts {
        if !index_a.contains_key(b.reference.as_str()) {
            changes.push(Change {
                reference: b.reference.clone(),
                kind: "added",
                first: String::new(),
                second: b.feeder.clone(),
            });
        }
    }
    // the parts that are in both, in the order of each job
    let common_a: Vec<&str> = first
        .parts
        .iter()
        .map(|p| p.reference.as_str())
        .filter(|r| index_b.contains_key(r))
        .collect();
    let common_b: Vec<&str> = second
        .parts
        .iter()
        .map(|p| p.reference.as_str())
        .filter(|r| index_a.contains_key(r))
        .collect();
    let same_sequence = common_a == common_b;
    if let Some(n) = common_a.iter().zip(&common_b).position(|(a, b)| a != b) {
        changes.push(Change {
            reference: common_a[n].to_string(),
            kind: "sequence",
            first: format!("step {} of {}", n + 1, common_a.len()),
            second: format!("{} comes there", common_b[n]),
        });
    }
    JobDiff {
        changes,
        same_sequence,
    }
}

impl JobDiff {
    pub fn is_same(&self) -> bool {
        self.changes.is_empty()
    }

    /// How many parts have a difference of their own (the whole plan and the sequence are not counted).
    pub fn parts_changed(&self) -> usize {
        let mut seen: Vec<&str> = self
            .changes
            .iter()
            .filter(|c| !c.reference.is_empty() && c.kind != "sequence")
            .map(|c| c.reference.as_str())
            .collect();
        seen.sort_unstable();
        seen.dedup();
        seen.len()
    }

    /// One line for each difference, then a line that sums it up.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for c in &self.changes {
            if c.reference.is_empty() {
                out.push_str(&format!("plan {}: {} -> {}\n", c.kind, c.first, c.second));
            } else if c.kind == "added" {
                out.push_str(&format!(
                    "{}: added, from the feeder {}\n",
                    c.reference, c.second
                ));
            } else if c.kind == "removed" {
                out.push_str(&format!(
                    "{}: removed, it was from the feeder {}\n",
                    c.reference, c.first
                ));
            } else if c.kind == "sequence" {
                out.push_str(&format!(
                    "the order of the parts that are in both differs: {} was {}, and there {}\n",
                    c.reference, c.first, c.second
                ));
            } else {
                out.push_str(&format!(
                    "{}: {} {} -> {}\n",
                    c.reference, c.kind, c.first, c.second
                ));
            }
        }
        if self.is_same() {
            out.push_str("the two jobs are the same\n");
        } else {
            out.push_str(&format!(
                "{} parts differ, {} differences in all{}\n",
                self.parts_changed(),
                self.changes.len(),
                if self.same_sequence {
                    ""
                } else {
                    "; the order of the parts is not the same"
                }
            ));
        }
        out
    }

    /// The differences as a table: `reference,change,first,second` (the reference is empty for the plan as a whole).
    pub fn csv(&self) -> String {
        let mut out = String::from("reference,change,first,second\n");
        for c in &self.changes {
            out.push_str(&format!(
                "{},{},{},{}\n",
                csv_field(&c.reference),
                c.kind,
                csv_field(&c.first),
                csv_field(&c.second)
            ));
        }
        out
    }
}
