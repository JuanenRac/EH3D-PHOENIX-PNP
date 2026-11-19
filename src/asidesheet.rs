// EH3D-PHOENIX-PNP - the sheet of the parts that a person places by hand.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A job leaves parts behind: the ones that were put aside by the policy `defer` or by the person, and the ones the person left out. They are not on the board when the machine is done, and
//! somebody has to place them. This reads a job (a plan with the state it is in, as `simulate --state` keeps it) and writes the sheet of those parts, sorted by value and package so that the
//! same reel is picked up once: the reference, the value, the package, the side, the place and the turn as the table of parts gives them, why it was left behind and the feeder it was to come
//! from. With `include_pending` the parts that the job did not get to yet (it stopped) are on the sheet too. A part that the table marks as not to be mounted is never on it.

use crate::job::{Job, Status};
use crate::parts::{csv_field, Part, Side};
use std::collections::HashMap;

/// One part that is left for a person.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub reference: String,
    pub value: String,
    pub package: String,
    pub side: Side,
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
    /// `Deferred`, `Skipped` or, with `include_pending`, `Pending`.
    pub status: Status,
    pub reason: String,
    pub feeder: String,
}

/// The sheet: the parts to place by hand, in the order of the sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct Sheet {
    pub rows: Vec<Row>,
}

/// The parts of the job that a person has to place. The value, the package, the side, the place and the turn are those of `parts` (found by reference); a placement that the table does not
/// have is on the sheet with the place of the plan and no value.
pub fn build(job: &Job, parts: &[Part], include_pending: bool) -> Sheet {
    let mut by_ref: HashMap<&str, &Part> = HashMap::new();
    for p in parts {
        by_ref.entry(p.reference.as_str()).or_insert(p);
    }
    let mut rows = Vec::new();
    for (i, placement) in job.plan().plan.placements.iter().enumerate() {
        let status = job.status()[i];
        let wanted = match status {
            Status::Deferred | Status::Skipped => true,
            Status::Pending => include_pending,
            Status::Placed => false,
        };
        if !wanted {
            continue;
        }
        let row = match by_ref.get(placement.reference.as_str()) {
            Some(p) => Row {
                reference: p.reference.clone(),
                value: p.value.clone(),
                package: p.package.clone(),
                side: p.side,
                x: p.x,
                y: p.y,
                rotation: p.rotation,
                status,
                reason: job.reason(i).to_string(),
                feeder: placement.feeder.clone(),
            },
            None => Row {
                reference: placement.reference.clone(),
                value: String::new(),
                package: String::new(),
                side: Side::Top,
                x: placement.x,
                y: placement.y,
                rotation: placement.rotation,
                status,
                reason: job.reason(i).to_string(),
                feeder: placement.feeder.clone(),
            },
        };
        rows.push(row);
    }
    rows.sort_by(|a, b| {
        (
            a.value.to_ascii_lowercase(),
            a.package.to_ascii_lowercase(),
            &a.reference,
        )
            .cmp(&(
                b.value.to_ascii_lowercase(),
                b.package.to_ascii_lowercase(),
                &b.reference,
            ))
    });
    Sheet { rows }
}

fn word(status: Status) -> &'static str {
    match status {
        Status::Deferred => "put aside",
        Status::Skipped => "left out",
        Status::Pending => "not placed yet",
        Status::Placed => "placed",
    }
}

impl Sheet {
    /// How many parts are in each state: put aside, left out, not placed yet.
    pub fn counts(&self) -> [usize; 3] {
        let n = |s: Status| self.rows.iter().filter(|r| r.status == s).count();
        [n(Status::Deferred), n(Status::Skipped), n(Status::Pending)]
    }

    /// The sheet as a table: `reference,value,package,side,x,y,rotation,status,reason,feeder`.
    pub fn csv(&self) -> String {
        let mut out =
            String::from("reference,value,package,side,x,y,rotation,status,reason,feeder\n");
        for r in &self.rows {
            out.push_str(&format!(
                "{},{},{},{},{},{},{},{},{},{}\n",
                csv_field(&r.reference),
                csv_field(&r.value),
                csv_field(&r.package),
                if r.side == Side::Top { "top" } else { "bottom" },
                r.x,
                r.y,
                r.rotation,
                r.status.name(),
                csv_field(&r.reason),
                csv_field(&r.feeder)
            ));
        }
        out
    }

    /// A line for each part and a line that sums the sheet up.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for r in &self.rows {
            out.push_str(&format!(
                "{}: {} {} ({}) at {:.3},{:.3} turned {:.1} - {}",
                r.reference,
                r.value,
                r.package,
                if r.side == Side::Top { "top" } else { "bottom" },
                r.x,
                r.y,
                r.rotation,
                word(r.status)
            ));
            if !r.reason.is_empty() {
                out.push_str(&format!(": {}", r.reason));
            }
            out.push('\n');
        }
        let c = self.counts();
        if self.rows.is_empty() {
            out.push_str("no part is left for a person to place\n");
        } else {
            out.push_str(&format!(
                "{} parts for a person to place: {} put aside, {} left out, {} not placed yet\n",
                self.rows.len(),
                c[0],
                c[1],
                c[2]
            ));
        }
        out
    }
}
