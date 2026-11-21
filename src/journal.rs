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
    /// What the nozzle is doing after the step: `idle` or `carrying` (see `NozzleState`).
    pub nozzle_state: &'static str,
}

/// What a nozzle is doing. A nozzle is `idle` (the vacuum is off and nothing is held), `picking` (the vacuum is on and the part is not held yet), `carrying` (a part is held) or `placing`
/// (the part is being let go). A step of the journal takes a nozzle through `picking` or `placing`, and the row says where it stands when the step is over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NozzleState {
    Idle,
    Picking,
    Carrying,
    Placing,
}

impl NozzleState {
    pub const ALL: [NozzleState; 4] = [
        NozzleState::Idle,
        NozzleState::Picking,
        NozzleState::Carrying,
        NozzleState::Placing,
    ];

    pub fn name(self) -> &'static str {
        match self {
            NozzleState::Idle => "idle",
            NozzleState::Picking => "picking",
            NozzleState::Carrying => "carrying",
            NozzleState::Placing => "placing",
        }
    }

    pub fn parse(text: &str) -> Option<NozzleState> {
        NozzleState::ALL.into_iter().find(|s| s.name() == text)
    }

    /// The state of the nozzle while the step goes on.
    pub fn during(is_pick: bool) -> NozzleState {
        if is_pick {
            NozzleState::Picking
        } else {
            NozzleState::Placing
        }
    }

    /// The state of the nozzle when the step is over: a pick that went well leaves it carrying a part and one that did not, empty; a place that went well leaves it empty and one that did
    /// not, still carrying the part.
    pub fn after(is_pick: bool, ok: bool) -> NozzleState {
        if is_pick == ok {
            NozzleState::Carrying
        } else {
            NozzleState::Idle
        }
    }
}

/// The header of the CSV of a journal.
pub const HEADER: &str =
    "step,at_s,took_s,action,reference,feeder,nozzle,result,status,nozzle_state";

/// A moment of a step that an observer is told of: just before the step is done, and just after.
#[derive(Debug, Clone, PartialEq)]
pub struct Seen {
    /// True after the step, false before it.
    pub after: bool,
    /// The number of the step, from 1.
    pub step: usize,
    pub is_pick: bool,
    /// The index of the placement in the plan.
    pub index: usize,
    /// How the step ended (always true before it).
    pub ok: bool,
    /// The label of the error that stopped the step (empty when it went well).
    pub error: String,
    pub reference: String,
    pub feeder: String,
    /// The nozzle, from 1.
    pub nozzle: usize,
}

/// Runs `job` as `Job::run(max_steps)` does and notes every step that is done. The journal is given back whatever happens: when an error stops the job, the last row says which one.
/// Past `MAX_ROWS` the job goes on and no more rows are kept (`truncated` is true).
pub fn run_recorded(
    job: &mut Job,
    max_steps: Option<usize>,
    first_step: usize,
) -> (Result<Phase, PnpError>, Vec<Row>, bool) {
    run_observed(job, max_steps, first_step, &mut |_, _| {})
}

/// As `run_recorded`, and tells `observer` of every step twice: before it is done and after, with the job as it stands at that moment. The observer only looks (it has the job by reference),
/// so a job run with an observer does the same steps as one run without.
pub fn run_observed(
    job: &mut Job,
    max_steps: Option<usize>,
    first_step: usize,
    observer: &mut dyn FnMut(&Job, &Seen),
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
        let mut seen = Seen {
            after: false,
            step: first_step + done + 1,
            is_pick,
            index,
            ok: true,
            error: String::new(),
            reference: placement.reference.clone(),
            feeder: placement.feeder.clone(),
            nozzle: placement.nozzle + 1,
        };
        observer(job, &seen);
        let began = job.machine().now_us();
        let outcome = job.step();
        let ended = job.machine().now_us();
        done += 1;
        seen.after = true;
        seen.ok = outcome.is_ok();
        seen.error = match &outcome {
            Ok(()) => String::new(),
            Err(e) => e.code.label(),
        };
        observer(job, &seen);
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
                nozzle_state: NozzleState::after(is_pick, outcome.is_ok()).name(),
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
            "{},{:.3},{:.3},{},{},{},{},{},{},{}\n",
            r.step,
            r.at_s,
            r.took_s,
            r.action,
            csv_field(&r.reference),
            csv_field(&r.feeder),
            r.nozzle,
            r.result,
            r.status,
            r.nozzle_state
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
