// EH3D-PHOENIX-PNP - one board shared between several machines.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A board with many parts can be placed by two machines or more, each with its own feeders, one after the other on a line. A reel is on one machine, so the parts of a feeder go together:
//! this gives each machine whole feeders so that every machine has about the same work. The feeders with the most parts are given out first, each to the machine that has the least so
//! far (the lowest number when two have the same), so the same board and the same tables always give the same split. The work of a part is the time of a pick and of a placement of the
//! settings (`pick_s` and `place_s`); the moves of the head are left out, so the times are for comparing the machines with each other and not for planning. Nothing is planned or moved.

use crate::feeding::{FeederSpec, Picks};
use crate::parts::{csv_field, Part};

/// The most machines.
pub const MAX_MACHINES: usize = 16;

/// What one machine is given.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Share {
    pub feeders: Vec<String>,
    /// The parts of the machine in the order of their references, each with the feeder it is picked from: `(reference, feeder)`.
    pub parts: Vec<(String, String)>,
    pub seconds: f64,
}

impl Share {
    /// The references of the parts of the machine.
    pub fn references(&self) -> Vec<&str> {
        self.parts.iter().map(|p| p.0.as_str()).collect()
    }
}

/// The split.
#[derive(Debug, Clone, PartialEq)]
pub struct Split {
    pub machines: Vec<Share>,
    /// The references of the parts that no feeder holds: they are given to no machine.
    pub without_feeder: Vec<String>,
}

/// Gives the feeders to `machines` machines. `part_s` is the seconds of a pick and a placement.
pub fn split(
    parts: &[Part],
    feeders: &[FeederSpec],
    picks: &Picks,
    machines: usize,
    part_s: f64,
) -> Split {
    let machines = machines.clamp(1, MAX_MACHINES);
    let mut groups: Vec<(String, Vec<String>)> = feeders
        .iter()
        .map(|f| (f.base.id.clone(), Vec::new()))
        .collect();
    for p in &picks.picks {
        groups[p.feeder].1.push(parts[p.part].reference.clone());
    }
    groups.retain(|(_, refs)| !refs.is_empty());
    for (_, refs) in &mut groups {
        refs.sort();
    }
    groups.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));
    let mut shares = vec![Share::default(); machines];
    for (feeder, refs) in groups {
        let at = shares
            .iter()
            .enumerate()
            .min_by(|a, b| a.1.seconds.total_cmp(&b.1.seconds).then(a.0.cmp(&b.0)))
            .map_or(0, |m| m.0);
        let share = &mut shares[at];
        share.seconds += refs.len() as f64 * part_s;
        share
            .parts
            .extend(refs.into_iter().map(|r| (r, feeder.clone())));
        share.feeders.push(feeder);
    }
    for s in &mut shares {
        s.parts.sort();
    }
    let mut without_feeder = picks.without_feeder.clone();
    without_feeder.sort();
    Split {
        machines: shares,
        without_feeder,
    }
}

impl Split {
    /// The seconds of the machine with the most work and of the one with the least.
    pub fn extremes(&self) -> (f64, f64) {
        let most = self.machines.iter().map(|m| m.seconds).fold(0.0, f64::max);
        let least = self
            .machines
            .iter()
            .map(|m| m.seconds)
            .fold(f64::INFINITY, f64::min);
        (most, if least.is_finite() { least } else { 0.0 })
    }

    /// A line for each machine, and how much the work differs.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for (n, m) in self.machines.iter().enumerate() {
            if m.parts.is_empty() {
                out.push_str(&format!(
                    "machine {}: nothing (there is no feeder left to give)\n",
                    n + 1
                ));
            } else {
                out.push_str(&format!(
                    "machine {}: {} parts from {} feeders ({}), about {:.1} s\n",
                    n + 1,
                    m.parts.len(),
                    m.feeders.len(),
                    m.feeders.join(", "),
                    m.seconds
                ));
            }
        }
        if !self.without_feeder.is_empty() {
            out.push_str(&format!(
                "no feeder holds: {}\n",
                self.without_feeder.join(" ")
            ));
        }
        let (most, least) = self.extremes();
        out.push_str(&format!(
            "the busiest machine has {most:.1} s and the least busy {least:.1} s: {:.1} s apart\n",
            most - least
        ));
        out
    }

    /// A row for each part: `machine,feeder,reference` (the machines are numbered from 1).
    pub fn csv(&self) -> String {
        let mut out = String::from("machine,feeder,reference\n");
        for (n, m) in self.machines.iter().enumerate() {
            for (r, f) in &m.parts {
                out.push_str(&format!("{},{},{}\n", n + 1, csv_field(f), csv_field(r)));
            }
        }
        out
    }
}
