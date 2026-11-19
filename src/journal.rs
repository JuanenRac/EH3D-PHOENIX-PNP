// EH3D-PHOENIX-PNP - the journal of a simulated job: one row for every step.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The report says how a job went in all; the journal says what happened, step by step: which part was picked or placed, from which feeder, with which nozzle, at what second of the
//! clock of the machine, how long the step took, and how it ended (`ok`, or the code of the error that stopped the job). It is for the person who asks why the job stopped where it did, and for
//! the program that wants to draw the timeline. A job that is run with the journal does the same steps as one that is not: the journal only looks.

use crate::error::PnpError;
use crate::job::{Job, Phase, Status};
use crate::model::Machine;
use crate::parts::csv_field;

/// The most rows of a journal (a job of fifty thousand parts has two steps for each part and more for the tips).
pub const MAX_ROWS: usize = 400_000;

/// One step of the job.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// The number of the step, from 1, counting the steps that were done before when the job was resumed.
    pub step: usize,
    /// The second of the clock of the machine when the step began.
    pub at_s: f64,
    /// How long the step took, in seconds.
    pub took_s: f64,
    /// `pick` or `place`.
    pub action: &'static str,
    pub reference: String,
    /// The feeder the plan takes the part from (a pick that went to another feeder still shows the planned one).
    pub feeder: String,
    /// The nozzle, from 1.
    pub nozzle: usize,
    /// `ok`, or the label of the error that stopped the job at this step.
    pub result: String,
    /// The status of the part after the step: `pending`, `placed`, `deferred` or `skipped`.
    pub status: &'static str,
}

/// The header of the CSV of a journal.
pub const HEADER: &str = "step,at_s,took_s,action,reference,feeder,nozzle,result,status";

/// Runs `job` as `Job::run(max_steps)` does and notes every step that is done. The journal is given back whatever happens: when an error stops the job, the last row says which one.
/// Past `MAX_ROWS` the job goes on and no more rows are kept (`truncated` is true).
pub fn run_recorded(
    job: &mut Job,
    max_steps: Option<usize>,
    first_step: usize,
) -> (Result<Phase, PnpError>, Vec<Row>, bool) {
    let mut rows: Vec<Row> = Vec::new();
    let mut truncated = false;
    let mut done = 0usize;
    while job.phase() == Phase::Ready {
        if max_steps.is_some_and(|m| done >= m) {
            job.pause();
            break;
        }
        let Some((is_pick, index)) = job.next_step() else {
            break;
        };
        let placement = job.plan().plan.placements[index].clone();
        let began = job.machine().now_us();
        let outcome = job.step();
        let ended = job.machine().now_us();
        done += 1;
        if rows.len() >= MAX_ROWS {
            truncated = true;
        } else {
            rows.push(Row {
                step: first_step + done,
                at_s: began as f64 / 1e6,
                took_s: ended.saturating_sub(began) as f64 / 1e6,
                action: if is_pick { "pick" } else { "place" },
                reference: placement.reference,
                feeder: placement.feeder,
                nozzle: placement.nozzle + 1,
                result: match &outcome {
                    Ok(()) => "ok".to_string(),
                    Err(e) => e.code.label(),
                },
                status: status_name(job.status()[index]),
            });
        }
        if let Err(e) = outcome {
            return (Err(e), rows, truncated);
        }
    }
    (Ok(job.phase()), rows, truncated)
}

fn status_name(s: Status) -> &'static str {
    s.name()
}

/// The journal as a CSV, with its header; the seconds have three decimals.
pub fn csv(rows: &[Row]) -> String {
    let mut out = String::from(HEADER);
    out.push('\n');
    for r in rows {
        out.push_str(&format!(
            "{},{:.3},{:.3},{},{},{},{},{},{}\n",
            r.step,
            r.at_s,
            r.took_s,
            r.action,
            csv_field(&r.reference),
            csv_field(&r.feeder),
            r.nozzle,
            r.result,
            r.status
        ));
    }
    out
}

/// A line that says what the journal holds: the steps, the picks and the places, and how many did not end well.
pub fn summary(rows: &[Row]) -> String {
    let picks = rows.iter().filter(|r| r.action == "pick").count();
    let bad = rows.iter().filter(|r| r.result != "ok").count();
    format!(
        "{} steps ({} picks, {} places), {} that did not end well",
        rows.len(),
        picks,
        rows.len() - picks,
        bad
    )
}
